<script setup>
// 单元格大编辑弹窗：右键「编辑内容…」打开，适合 JSON / 超长文本；确定后写回暂存
import { ref, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useWorkspaceStore } from '@/stores/workspace';

const ws = useWorkspaceStore();
const taEl = ref(null);

watch(() => ws.cellEdit.visible, async (visible) => {
  if (!visible) return;
  await nextTick();
  if (taEl.value) { taEl.value.focus(); taEl.value.select(); }
});
</script>

<template>
  <ModalShell :visible="ws.cellEdit.visible" width="640px" @close="ws.closeCellEdit()">
    <template #title>编辑单元格</template>
    <div class="export-body">
      <div class="cell-edit-head">
        <div class="cell-edit-col">{{ ws.cellEdit.table ? ws.cellEdit.table + '.' : '' }}{{ ws.cellEdit.col }}</div>
        <div class="cell-edit-original" :title="ws.cellEdit.original">原值：{{ ws.cellEdit.original || '（空）' }}</div>
      </div>
    </div>
    <textarea
      ref="taEl"
      class="large-textarea"
      rows="8"
      autocapitalize="off" autocorrect="off" spellcheck="false"
      v-model="ws.cellEdit.text"
      @keydown.esc.stop.prevent="ws.closeCellEdit()"
      @keydown.enter.meta.stop.prevent="ws.commitCellEdit()"
      @keydown.enter.ctrl.stop.prevent="ws.commitCellEdit()"
    ></textarea>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="ws.closeCellEdit()">取消</button>
      <button class="btn" @click="ws.commitCellEdit()">确定</button>
    </div>
  </ModalShell>
</template>
