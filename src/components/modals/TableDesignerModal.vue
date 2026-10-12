<script setup>
import { ref, computed, watch } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import DesignerFields from '@/components/database/designer/DesignerFields.vue';
import DesignerIndexes from '@/components/database/designer/DesignerIndexes.vue';
import DesignerForeignKeys from '@/components/database/designer/DesignerForeignKeys.vue';
import DesignerTableOptions from '@/components/database/designer/DesignerTableOptions.vue';
import { useTableStore } from '@/stores/table';
import { useConnectionStore } from '@/stores/connection';
import { useUiStore } from '@/stores/ui';
import { buildAlterScript, toDraftColumns, toDraftIndexes, toDraftForeignKeys } from '@/utils/ddl';

const table = useTableStore();
const conn = useConnectionStore();
const ui = useUiStore();

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
}

// 结构表单改动后自动重算 SQL；用户手工改过 SQL 就不再覆盖
watch([columns, indexes, foreignKeys, options], () => {
  if (!manual.value) regenerate();
}, { deep: true });

watch(() => table.designerModal.visible, (v) => { if (v) load(); });

function onSqlInput() {
  manual.value = true;
}

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
      <textarea
        class="dz-sql"
        rows="9"
        autocapitalize="off" autocorrect="off" spellcheck="false"
        v-model="sql"
        placeholder="未检测到结构变更"
        @input="onSqlInput"
      ></textarea>

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
  padding: 8px 18px;
  border: 1px solid transparent;
  border-bottom: none;
  border-radius: 6px 6px 0 0;
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
}
.dz-tab.active {
  color: var(--cyan);
  border-color: var(--line-strong);
  background: rgba(var(--cyan-rgb), .06);
  box-shadow: inset 0 0 18px -10px var(--cyan);
}
.dz-panel {
  min-height: 180px;
  margin-bottom: 16px;
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
.dz-sql {
  width: 100%;
  box-sizing: border-box;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--bg-1);
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12.5px;
  line-height: 1.6;
  outline: none;
  resize: vertical;
}
.dz-sql:focus {
  border-color: var(--cyan);
  box-shadow: 0 0 0 1px rgba(var(--cyan-rgb), .25), 0 0 14px -3px rgba(var(--cyan-rgb), .6);
}
.dz-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  justify-content: flex-end;
  margin-top: 14px;
}
.dz-actions .dz-hint {
  margin-right: auto;
  color: var(--text-dim);
  font-size: 12px;
}
</style>
