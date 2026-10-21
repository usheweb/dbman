<script setup>
// 删除数据库：从当前连接的库列表里选一个库执行 DROP DATABASE。
// 删除不可恢复，执行前会再过一遍二次确认
import { computed, nextTick, ref, watch } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import IconSelect from '@/components/common/IconSelect.vue';
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';

const ui = useUiStore();
const connStore = useConnectionStore();

const dbs = ref([]);
const db = ref('');
const busy = ref(false);
const errorMsg = ref('');

const conn = computed(() => ui.dropDbTarget || {});
const dbType = computed(() => String(conn.value.type || '').toLowerCase());
const isMysql = computed(() => dbType.value === 'mysql' || dbType.value === 'mariadb');
const isPg = computed(() => dbType.value === 'postgres' || dbType.value === 'postgresql');
const isMssql = computed(() => dbType.value === 'sqlserver' || dbType.value === 'mssql');

const dbOptions = computed(() => dbs.value.map(v => ({ value: v, label: v })));
// 从某个库上右键进来的：目标已经确定，直接展示库名，不再让用户选一遍
const presetDb = computed(() => ui.dropDbPreset || '');

// 按类型套用各自的标识符引用方式，避免 pg / sqlserver 拿到反引号语法
function buildSql(nm) {
  if (isPg.value) return 'DROP DATABASE "' + nm + '"';
  if (isMssql.value) return 'DROP DATABASE [' + nm + ']';
  return 'DROP DATABASE `' + nm + '`';
}

const sqlPreview = computed(() => buildSql(db.value || '数据库名'));

// 删库弹窗打开时快照一份库列表：删完刷新的是连接下的列表，这里的候选不用跟着变
watch(() => ui.modals.dropDbModal, async (v) => {
  if (!v) return;
  dbs.value = [...(connStore.currentDatabaseList || [])];
  db.value = ui.dropDbPreset || '';
  busy.value = false;
  errorMsg.value = '';
  await nextTick();
});

function isMysqlType(type) {
  const t = String(type || '').toLowerCase();
  return t === 'mysql' || t === 'mariadb';
}

async function submit() {
  if (busy.value) return;
  errorMsg.value = '';
  const target = ui.dropDbTarget;
  if (!target) return;
  if (!db.value) {
    errorMsg.value = '请选择要删除的数据库';
    return;
  }
  // MySQL 的系统库一旦删除，服务直接不可用，直接拦下
  if (isMysqlType(target.type)
    && ['mysql', 'information_schema', 'performance_schema', 'sys'].includes(db.value)) {
    errorMsg.value = '系统数据库不能删除';
    return;
  }
  const ok = await ui.askConfirm({
    title: '删除数据库',
    message: `确定要删除数据库「${db.value}」吗？库内所有表与数据都会被永久删除，且无法恢复。`,
    confirmText: '删除',
    danger: true
  });
  if (!ok) return;
  busy.value = true;
  try {
    await connStore.dropDatabaseFor(target.id, buildSql(db.value), db.value);
    ui.showToast(`数据库「${db.value}」已删除`);
    ui.closeModal('dropDbModal');
  } catch (e) {
    errorMsg.value = String(e && e.message ? e.message : e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ModalShell :visible="ui.modals.dropDbModal" width="500px" @close="ui.closeModal('dropDbModal')">
    <template #title>删除数据库</template>
    <div class="export-body">
      <div class="export-row">
        <div class="export-label">连接</div>
        <div class="export-opts">
          <span class="export-hint">{{ conn.name || conn.host }} · {{ dbType.toUpperCase() }}</span>
        </div>
      </div>
      <div class="export-row">
        <div class="export-label">数据库</div>
        <div class="export-opts">
          <span v-if="presetDb" class="drop-db-name">{{ presetDb }}</span>
          <IconSelect
            v-else
            v-model="db"
            :options="dbOptions"
            searchable
            placeholder="请选择要删除的数据库"
            class="drop-db-select"
          />
        </div>
        <div v-if="!dbs.length" class="export-hint">当前连接下没有可删除的数据库。</div>
      </div>
      <div class="form-group">
        <label>语句预览</label>
        <pre class="drop-db-sql">{{ sqlPreview }}</pre>
      </div>
      <div class="drop-db-warn">删除后库内的所有表与数据都会被永久清除，且无法恢复。</div>
      <div v-if="errorMsg" class="drop-db-error">{{ errorMsg }}</div>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="ui.closeModal('dropDbModal')">取消</button>
      <button class="btn btn-danger" :disabled="busy" @click="submit">{{ busy ? '删除中…' : '删除' }}</button>
    </div>
  </ModalShell>
</template>

<style scoped>
.drop-db-select {
  flex: 1;
  min-width: 0;
}

.drop-db-name {
  flex: 1;
  min-width: 0;
  padding: 7px 12px;
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.drop-db-sql {
  margin: 0;
  padding: 10px 12px;
  background: rgba(var(--void-rgb), .7);
  border: 1px solid var(--line);
  border-radius: 6px;
  color: var(--cyan);
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
}

.drop-db-warn {
  margin-top: 10px;
  color: var(--magenta);
  font-size: 12px;
  line-height: 1.5;
}

.drop-db-error {
  margin-top: 8px;
  color: var(--magenta);
  font-size: 12px;
  line-height: 1.5;
}
</style>
