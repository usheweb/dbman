<script setup>
import { ref, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useTableStore } from '@/stores/table';

const table = useTableStore();
const inputEl = ref(null);

watch(() => table.prompt.visible, async (v) => {
  if (v) {
    await nextTick();
    if (inputEl.value) {
      inputEl.value.focus();
      inputEl.value.select();
    }
  }
});

function ok() {
  const v = String(table.prompt.value || '').trim();
  if (!v) return;
  table.resolvePrompt(v);
}
</script>

<template>
  <ModalShell :visible="table.prompt.visible" width="460px" @close="table.resolvePrompt(null)">
    <template #title>{{ table.prompt.title }}</template>
    <div class="export-body">
      <div class="export-row">
        <div class="export-label">{{ table.prompt.label }}</div>
        <input
          ref="inputEl"
          type="text"
          autocapitalize="off" autocorrect="off" spellcheck="false"
          class="filter-input"
          style="width: 100%;"
          v-model="table.prompt.value"
          :placeholder="table.prompt.placeholder"
          @keyup.enter="ok"
        >
      </div>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="table.resolvePrompt(null)">取消</button>
      <button class="btn" @click="ok">{{ table.prompt.confirmText }}</button>
    </div>
  </ModalShell>
</template>
