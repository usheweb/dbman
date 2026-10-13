<script setup>
import { ref, computed, watch, nextTick } from 'vue';
import CodeMirror from 'codemirror';
import 'codemirror/mode/sql/sql.js';
import ModalShell from '@/components/common/ModalShell.vue';
import DesignerFields from '@/components/database/designer/DesignerFields.vue';
import DesignerIndexes from '@/components/database/designer/DesignerIndexes.vue';
import DesignerForeignKeys from '@/components/database/designer/DesignerForeignKeys.vue';
import DesignerTableOptions from '@/components/database/designer/DesignerTableOptions.vue';
import { useTableStore } from '@/stores/table';
import { useConnectionStore } from '@/stores/connection';
import { useUiStore } from '@/stores/ui';
import { useThemeStore } from '@/stores/theme';
import { buildAlterScript, toDraftColumns, toDraftIndexes, toDraftForeignKeys } from '@/utils/ddl';

const table = useTableStore();
const conn = useConnectionStore();
const ui = useUiStore();
const theme = useThemeStore();

const TABS = [
  { key: 'fields', label: '字段' },
  { key: 'indexes', label: '索引' },
  { key: 'foreignKeys', label: '外键' },
  { key: 'options', label: '表选项' }
];

const tab = ref('fields');
const loading = ref(false);
const applying = ref(false);
const manual = ref(false);
const original = ref(null);
const columns = ref([]);
const indexes = ref([]);
const foreignKeys = ref([]);
const options = ref({ engine: null, charset: null, collation: null, comment: null, autoIncrement: null });
const sql = ref('');

// ALTER 语句用 CodeMirror（与 SQL 编辑器同一套），既能关键词着色又能手工编辑
const taRef = ref(null);
let cm = null;

function ensureCm() {
  if (cm || !taRef.value) return;
  cm = CodeMirror.fromTextArea(taRef.value, {
    mode: 'text/x-mysql',
    theme: theme.editorTheme,
    lineNumbers: true,
    indentWithTabs: false,
    indentUnit: 2,
    tabSize: 2,
    lineWrapping: true
  });
  cm.setValue(sql.value);
  // 用户在编辑器里改动即视为手工修改，暂停自动生成
  cm.on('change', () => {
    const v = cm.getValue();
    if (v === sql.value) return;
    sql.value = v;
    manual.value = true;
  });
}

function destroyCm() {
  if (!cm) return;
  cm.toTextArea();
  cm = null;
}

const tableName = computed(() => table.designerModal.table);
const refTables = computed(() => conn.allTables.map(t => t.tableName));
const hasChanges = computed(() => !!sql.value.trim());

async function load() {
  loading.value = true;
  manual.value = false;
  tab.value = 'fields';
  try {
    const s = await table.loadStructure(tableName.value);
    original.value = s;
    columns.value = toDraftColumns(s.columns);
    indexes.value = toDraftIndexes(s.indexes);
    foreignKeys.value = toDraftForeignKeys(s.foreignKeys);
    options.value = { ...s.options };
    regenerate();
  } catch (e) {
    ui.showToast(String(e.message || e), 'error');
    table.closeDesigner();
  } finally {
    loading.value = false;
    // 结构表单渲染出来后再建 CodeMirror，并刷新一次尺寸
    await nextTick();
    if (table.designerModal.visible) {
      ensureCm();
      cm?.refresh();
    }
  }
}

function regenerate() {
  if (!original.value) return;
  sql.value = buildAlterScript(
    original.value,
    { columns: columns.value, indexes: indexes.value, foreignKeys: foreignKeys.value, options: options.value },
    tableName.value
  );
  manual.value = false;
  // 同步到编辑器；此时 sql.value 已等于新值，change 回调不会把它当成手工修改
  if (cm) cm.setValue(sql.value);
}

// 结构表单改动后自动重算 SQL；用户手工改过 SQL 就不再覆盖
watch([columns, indexes, foreignKeys, options], () => {
  if (!manual.value) regenerate();
}, { deep: true });

// 主题切换时同步 CodeMirror 皮肤
watch(() => theme.editorTheme, (name) => { if (cm) cm.setOption('theme', name); });

watch(() => table.designerModal.visible, (v) => {
  if (v) load();
  else destroyCm();
});

async function apply() {
  const script = sql.value.trim().replace(/;+$/, '') + ';';
  if (!script) {
    ui.showToast('未检测到结构变更', 'error');
    return;
  }
  if (!confirm(`即将对表「${tableName.value}」执行以下 DDL，修改期间可能锁表，且多条语句中途失败会留下半改状态。\n\n确定继续吗？`)) return;
  applying.value = true;
  try {
    await table.execDdl(script, tableName.value, 'alter');
    ui.showToast('表结构已更新', 'success');
    table.closeDesigner();
  } catch (e) {
    ui.showToast(String(e.message || e), 'error');
  } finally {
    applying.value = false;
  }
}

function close() {
  if (hasChanges.value && !confirm('关闭将丢弃当前的修改，确定关闭吗？')) return;
  table.closeDesigner();
}
</script>

<template>
  <ModalShell :visible="table.designerModal.visible" width="1040px" @close="close">
    <template #title>修改表结构 · {{ tableName }}</template>

    <div v-if="loading" class="dz-loading">正在读取表结构…</div>

    <template v-else>
      <div class="dz-tabs">
        <button
          v-for="t in TABS"
          :key="t.key"
          class="dz-tab"
          :class="{ active: tab === t.key }"
          @click="tab = t.key"
        >{{ t.label }}</button>
      </div>

      <div class="dz-panel">
        <DesignerFields v-show="tab === 'fields'" :columns="columns" />
        <DesignerIndexes v-show="tab === 'indexes'" :indexes="indexes" :columns="columns" />
        <DesignerForeignKeys
          v-show="tab === 'foreignKeys'"
          :foreign-keys="foreignKeys"
          :columns="columns"
          :tables="refTables"
        />
        <DesignerTableOptions v-show="tab === 'options'" :options="options" />
      </div>

      <div class="dz-sql-head">
        <span>生成的 ALTER 语句（可直接手工编辑）</span>
        <span v-if="manual" class="dz-manual">已手工修改，自动生成已暂停</span>
        <button class="btn btn-secondary dz-regen" @click="regenerate">重新生成</button>
      </div>
      <div class="dz-sql-wrap">
        <textarea
          ref="taRef"
          autocapitalize="off" autocorrect="off" spellcheck="false"
          placeholder="未检测到结构变更"
        ></textarea>
      </div>

      <div class="dz-actions">
        <span class="dz-hint">{{ hasChanges ? '执行前请确认 SQL 正确；DDL 无法回滚。' : '未检测到结构变更。' }}</span>
        <button class="btn btn-secondary" @click="close">取消</button>
        <button class="btn" :disabled="applying || !hasChanges" @click="apply">{{ applying ? '执行中…' : '应用变更' }}</button>
      </div>
    </template>
  </ModalShell>
</template>

<style scoped>
.dz-loading {
  padding: 40px;
  text-align: center;
  color: var(--text-dim);
  font-size: 13px;
}
.dz-tabs {
  display: flex;
  gap: 6px;
  margin-bottom: 14px;
  border-bottom: 1px solid var(--line);
}
.dz-tab {
  position: relative;
  padding: 9px 20px;
  border: 1px solid transparent;
  border-bottom: none;
  border-radius: 8px 8px 0 0;
  background: none;
  color: var(--text-dim);
  font-family: var(--font-cjk);
  font-size: 13px;
  letter-spacing: 1px;
  cursor: pointer;
  transition: all .2s;
}
.dz-tab:hover {
  color: var(--text);
  background: rgba(var(--cyan-rgb), .05);
}
.dz-tab.active {
  color: var(--cyan);
  border-color: var(--line-strong);
  background: rgba(var(--cyan-rgb), .08);
  box-shadow: inset 0 0 18px -10px var(--cyan);
}
.dz-tab.active::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: -1px;
  height: 2px;
  background: var(--cyan);
  box-shadow: 0 0 10px rgba(var(--cyan-rgb), .8);
}
/* 面板外框：让字段 / 索引 / 外键 / 表选项四块内容有统一的承载容器 */
.dz-panel {
  min-height: 180px;
  margin-bottom: 16px;
  padding: 14px 16px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--panel-solid);
}
.dz-sql-head {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 8px;
  color: var(--text-dim);
  font-size: 12px;
  letter-spacing: 1px;
}
.dz-manual {
  color: var(--cyan-soft);
}
.dz-regen {
  margin-left: auto;
  padding: 5px 12px;
  font-size: 12px;
}
/* ALTER 语句编辑器：外观对齐原来的 textarea，内部是 CodeMirror（关键词着色 + 行号） */
.dz-sql-wrap :deep(.CodeMirror) {
  height: 190px !important;
  border: 1px solid var(--line);
  border-radius: 10px;
  color: var(--editor-fg);
  font-size: 12.5px;
  letter-spacing: 0;
  line-height: 1.6;
  transition: border-color .18s, box-shadow .18s;
}
.dz-sql-wrap :deep(.CodeMirror:hover) {
  border-color: var(--line-strong);
}
.dz-sql-wrap :deep(.CodeMirror-focused) {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25), 0 0 14px -3px rgba(var(--cyan-rgb), .6);
}
.dz-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  justify-content: flex-end;
  margin-top: 14px;
  padding-top: 14px;
  border-top: 1px solid var(--line);
}
.dz-actions .dz-hint {
  margin-right: auto;
  color: var(--text-dim);
  font-size: 12px;
}
</style>
