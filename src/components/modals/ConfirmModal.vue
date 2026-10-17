<script setup>
// 通用确认弹窗：替代在本项目 WKWebView 里完全不生效的原生 window.confirm
// （wry 的 WKUIDelegate 未实现 runJavaScriptConfirmPanel，confirm() 会被当成「取消」
// 静默返回 false），全项目的二次确认统一走这里
import { ref, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';

const ui = useUiStore();
const okBtn = ref(null);

watch(() => ui.confirmModal.visible, async (v) => {
  if (!v) return;
  await nextTick();
  if (okBtn.value) okBtn.value.focus();
});
</script>

<template>
  <ModalShell :visible="ui.confirmModal.visible" width="480px" @close="ui.resolveConfirm(false)">
    <template #title>{{ ui.confirmModal.title }}</template>
    <div class="confirm-body">
      <div class="confirm-tip">{{ ui.confirmModal.message }}</div>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="ui.resolveConfirm(false)">{{ ui.confirmModal.cancelText }}</button>
      <button
        ref="okBtn"
        class="btn"
        :class="ui.confirmModal.danger ? 'btn-danger' : 'btn-secondary'"
        @click="ui.resolveConfirm(true)"
      >{{ ui.confirmModal.confirmText }}</button>
    </div>
  </ModalShell>
</template>

<style scoped>
.confirm-body {
  padding: 4px 0 2px;
}
.confirm-tip {
  color: var(--text);
  font-family: var(--font-cjk);
  line-height: 1.7;
  white-space: pre-line;
  overflow-wrap: break-word;
}
.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-top: 16px;
}
</style>
