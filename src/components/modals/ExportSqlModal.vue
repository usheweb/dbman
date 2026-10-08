<script setup>
import { ref, watch } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useTableStore } from '@/stores/table';

const tableStore = useTableStore();

const withDrop = ref(true);
const withCreate = ref(true);
const batchSize = ref(1000);
const busy = ref(false);

watch(() => tableStore.exportSqlModal.visible, (v) => {
  if (v) {
    withDrop.value = true;
    withCreate.value = true;
    batchSize.value = 1000;
    busy.value = false;
  }
});

async function go() {
  const name = tableStore.exportSqlModal.table;
  const mode = tableStore.exportSqlModal.mode;
  busy.value = true;
  try {
    if (mode === 'structure') {
      await tableStore.exportStructureSql(name, { withDrop: withDrop.value });
    } else {
      await tableStore.exportDataSql(name, {
        withDrop: withDrop.value,
        withCreate: withCreate.value,
        batchSize: parseInt(batchSize.value, 10) || 1000
      });
    }
  } finally {
    busy.value = false;
    tableStore.closeExportSql();
  }
}
</script>

<template>
  <ModalShell :visible="tableStore.exportSqlModal.visible" width="560px" @close="tableStore.closeExportSql()">
    <template #title>{{ tableStore.exportSqlModal.mode === 'structure' ? '导出表结构 SQL' : '导出数据 SQL' }}</template>
    <div class="export-body">
      <div class="export-row">
        <div class="export-label">目标表</div>
        <div class="export-opts"><span class="export-hint">{{ tableStore.exportSqlModal.table }}</span></div>
      </div>
      <div class="export-row">
        <div class="export-label">文件内容</div>
        <div class="export-opts">
          <label class="export-opt"><input type="checkbox" v-model="withDrop">附 DROP TABLE IF EXISTS</label>
          <label v-if="tableStore.exportSqlModal.mode === 'data'" class="export-opt">
            <input type="checkbox" v-model="withCreate">附建表语句 CREATE TABLE
          </label>
        </div>
      </div>
      <div v-if="tableStore.exportSqlModal.mode === 'data'" class="export-row">
        <div class="export-label">批量行数</div>
        <div class="export-opts">
          <input type="number" class="filter-input" style="width: 120px;" v-model="batchSize" min="1">
        </div>
        <div class="export-hint">每条 INSERT 合并的行数；过大可能超过 max_allowed_packet。</div>
      </div>
      <div v-if="tableStore.exportSqlModal.mode === 'data'" class="export-hint">
        导出会重新执行 SELECT 全表扫描，数据量大时较慢，导出期间请勿关闭窗口。
      </div>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="tableStore.closeExportSql()">取消</button>
      <button class="btn" :disabled="busy" @click="go">{{ busy ? '导出中…' : '导出' }}</button>
    </div>
  </ModalShell>
</template>
