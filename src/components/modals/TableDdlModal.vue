<script setup>
import { computed } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useTableStore } from '@/stores/table';
import { useUiStore } from '@/stores/ui';
import { highlightSqlSyntax } from '@/utils/sql';

const table = useTableStore();
const ui = useUiStore();

// 建表语句按关键字着色，配色对齐 SQL 编辑器
const ddlHtml = computed(() => (
  table.ddlModal.loading ? '加载中…' : highlightSqlSyntax(table.ddlModal.ddl)
));

async function copy() {
  try {
    await navigator.clipboard.writeText(table.ddlModal.ddl || '');
    ui.showToast('已复制建表语句', 'success');
  } catch (e) {
    ui.showToast('复制失败', 'error');
  }
}
</script>

<template>
  <ModalShell :visible="table.ddlModal.visible" width="820px" @close="table.closeDdl()">
    <template #title>建表语句 · {{ table.ddlModal.table }}</template>
    <div class="log-tools">
      <button class="btn btn-secondary" @click="copy">复制</button>
    </div>
    <pre class="ddl-pre" v-html="ddlHtml"></pre>
  </ModalShell>
</template>

<style scoped>
.ddl-pre {
  margin: 8px 0 0;
  max-height: 62vh;
  overflow: auto;
  padding: 12px 14px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--bg-1);
  color: var(--text);
  font-family: var(--font-mono);
  font-size: 12.5px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
