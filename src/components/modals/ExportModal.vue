<script setup>
import { ref, computed, watch } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';
import { useWorkspaceStore } from '@/stores/workspace';

const ui = useUiStore();
const ws = useWorkspaceStore();

const exportHeader = ref(ui.exportOpts.header);
const exportScope = ref(ui.exportOpts.scope);

const pageRows = computed(() => (ws.lastResult ? ws.lastResult.data.length : 0));

const scopeHint = computed(() => (exportScope.value === 'all'
  ? '全部会重新执行当前 SQL，由服务端边查边写流式生成文件，不会把全部数据读进内存；但数据量很大（如上千万行）时仍然会比较慢，请谨慎使用。'
  : '仅导出当前页已显示的内容，速度快。'));

// 每次打开弹窗时用已保存的选项回填
watch(() => ui.modals.exportModal, (v) => {
  if (v) {
    exportHeader.value = ui.exportOpts.header;
    exportScope.value = ui.exportOpts.scope;
  }
});

function doExport() {
  ui.saveExportOpts({ header: exportHeader.value, scope: exportScope.value });
  ui.closeModal('exportModal');
  if (exportScope.value === 'all') ws.exportAllRows();
  else ws.exportCurrentPage();
}
</script>

<template>
  <ModalShell :visible="ui.modals.exportModal" width="540px" @close="ui.closeModal('exportModal')">
    <template #title>导出 CSV</template>
    <div class="export-body">
      <div class="export-row">
        <div class="export-label">表头标题</div>
        <div class="export-opts">
          <label class="export-opt"><input type="radio" name="exportHeader" value="field" v-model="exportHeader">字段名</label>
          <label class="export-opt"><input type="radio" name="exportHeader" value="comment" v-model="exportHeader">注释</label>
          <label class="export-opt"><input type="radio" name="exportHeader" value="both" v-model="exportHeader">字段名 + 注释</label>
        </div>
        <div class="export-hint">「注释」会自动去掉其中的枚举说明部分，如「用户状态(1.正常,2.冻结)」只保留「用户状态」。</div>
      </div>
      <div class="export-row">
        <div class="export-label">导出范围</div>
        <div class="export-opts">
          <label class="export-opt"><input type="radio" name="exportScope" value="page" v-model="exportScope">当前显示（本页 {{ pageRows }} 行）</label>
          <label class="export-opt"><input type="radio" name="exportScope" value="all" v-model="exportScope">全部</label>
        </div>
        <div class="export-hint" id="exportScopeHint">{{ scopeHint }}</div>
      </div>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="ui.closeModal('exportModal')">取消</button>
      <button class="btn" id="exportGoBtn" @click="doExport">导出</button>
    </div>
  </ModalShell>
</template>
