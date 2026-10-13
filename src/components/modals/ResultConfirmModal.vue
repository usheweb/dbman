<script setup>
// 结果表通用确认弹窗：保存修改 / 删除记录（含 SQL 预览与预计影响行数）+ 复制为表格数据（含是否带表头勾选）
import { ref, computed, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { highlightSqlSyntax } from '@/utils/sql';

const ws = useWorkspaceStore();
const okBtn = ref(null);
const checked = ref(false);

// SQL 预览按关键字着色，配色对齐 SQL 编辑器
const sqlHtml = computed(() => highlightSqlSyntax(String(ws.resultConfirm.sql || '').trim()));

watch(() => ws.resultConfirm.visible, async (visible) => {
  if (!visible) return;
  checked.value = !!ws.resultConfirm.checkboxValue;
  await nextTick();
  if (okBtn.value) okBtn.value.focus();
});

function close() {
  ws.resolveResultConfirm(false);
}
</script>

<template>
  <ModalShell :visible="ws.resultConfirm.visible" width="640px" @close="close">
    <template #title>{{ ws.resultConfirm.title }}</template>
    <div class="result-confirm-body">
      <div class="rc-desc">{{ ws.resultConfirm.desc }}</div>
      <div v-if="ws.resultConfirm.estimate" class="rc-estimate">
        <span class="rc-estimate-dot"></span>{{ ws.resultConfirm.estimate }}
      </div>
      <pre v-if="ws.resultConfirm.sql" class="confirm-sql" v-html="sqlHtml"></pre>
      <label v-if="ws.resultConfirm.checkboxLabel" class="rc-check">
        <input
          type="checkbox"
          v-model="checked"
          @keyup.enter.prevent="ws.resolveResultConfirm(true, checked)"
        >
        <span>{{ ws.resultConfirm.checkboxLabel }}</span>
      </label>
    </div>
    <div class="confirm-actions">
      <button class="btn btn-secondary" @click="close">取消</button>
      <button
        ref="okBtn"
        class="btn"
        :class="{ 'btn-danger': ws.resultConfirm.danger }"
        @click="ws.resolveResultConfirm(true, checked)"
      >{{ ws.resultConfirm.confirmText }}</button>
    </div>
  </ModalShell>
</template>
