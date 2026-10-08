<script setup>
import { ref, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useWorkspaceStore } from '@/stores/workspace';

const ws = useWorkspaceStore();
const okBtn = ref(null);

watch(() => ws.execConfirm.visible, async (v) => {
  if (v) {
    await nextTick();
    if (okBtn.value) okBtn.value.focus();
  }
});
</script>

<template>
  <ModalShell :visible="ws.execConfirm.visible" width="560px" @close="ws.resolveExecConfirm(false)">
    <template #title>危险操作确认</template>
    <div class="confirm-body">
      <span class="confirm-warn">{{ ws.execConfirm.kinds.join(' / ') }}</span>
      <div class="confirm-tip">该语句会改动数据库的数据、结构或运行状态，部分操作不可撤销，请确认无误后再执行：</div>
      <pre class="confirm-sql">{{ ws.execConfirm.sql.trim() }}</pre>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="ws.resolveExecConfirm(false)">取消</button>
      <button class="btn btn-danger" id="confirmOkBtn" ref="okBtn" @click="ws.resolveExecConfirm(true)">确认执行</button>
    </div>
  </ModalShell>
</template>
