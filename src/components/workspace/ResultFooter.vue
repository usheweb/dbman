<script setup>
// 结果区底部状态栏：分页 + 每页显示 + 执行状态 + 数据导出
import { computed } from 'vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import { escapeHtml } from '@/utils/escape';

const ws = useWorkspaceStore();
const ui = useUiStore();

const statusHtml = computed(() => {
  const p = ws.pageInfo;
  // 还没执行过查询：不谎报「执行成功」
  if (p.idle) return '';
  if (p.error) {
    return `<span class="err">✕</span> 执行失败：${escapeHtml(p.error)}`;
  }
  if (p.write) {
    return `<span class="ok">✓</span> 执行成功，影响 <b>${p.rows}</b> 行，执行耗时 <b>${p.elapsed}ms</b>`;
  }
  return `<span class="ok">✓</span> 执行成功，当前返回 <b>${p.rows}</b> 行，执行耗时 <b>${p.elapsed}ms</b>`
    + (p.hasMore ? '；<span class="hint">更多数据请使用</span>' : '')
    + (p.paged ? '' : '；<span class="hint">SQL 自带 LIMIT 或非查询语句，未启用分页</span>');
});

const pageSizeOptions = [100, 300, 1000, 3000];

function onChangePageSize(e) {
  ws.changePageSize(e.target.value);
}
</script>

<template>
  <div class="result-footer">
    <div class="page-group">
      <button
        class="page-btn"
        title="上一页"
        :disabled="!ws.pageInfo.paged || ws.pageNo <= 1"
        @click="ws.gotoPage(ws.pageNo - 1)"
      >‹</button>
      <span class="page-no">{{ ws.pageNo }}</span>
      <button
        class="page-btn"
        title="下一页"
        :disabled="!ws.pageInfo.paged || !ws.pageInfo.hasMore"
        @click="ws.gotoPage(ws.pageNo + 1)"
      >›</button>
    </div>
    <label class="page-size-label">每页显示
      <select
        class="page-size"
        :value="String(ws.pageSize)"
        :disabled="!ws.pageInfo.paged"
        :title="ws.pageInfo.paged ? '每页显示条数' : 'SQL 自带 LIMIT 或不是查询语句，未启用分页'"
        @change="onChangePageSize"
      >
        <option v-for="opt in pageSizeOptions" :key="opt" :value="String(opt)">{{ opt }}</option>
      </select>
    </label>
    <span class="footer-status" v-html="statusHtml"></span>
    <button
      class="export-link"
      title="导出结果为 CSV（可选表头与范围）"
      :disabled="!ws.lastSql || ws.pageInfo.idle || ws.pageInfo.write"
      @click="ui.openModal('exportModal')"
    >【数据导出】</button>
  </div>
</template>
