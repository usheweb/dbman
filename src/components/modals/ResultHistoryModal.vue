<script setup>
// 查询结果历史：弹窗面板里逐条展示完整信息（完整 SQL + 时间 / 行数 / 耗时 / 库）。
// 原先是结果区工具条上的下拉框，SQL 只能显示一行摘要、长语句看不全，故改为弹窗。
import { ref, computed, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';
import { useWorkspaceStore } from '@/stores/workspace';
import { highlightSqlSyntax } from '@/utils/sql';
import { formatDateTime } from '@/utils/format';

const ui = useUiStore();
const ws = useWorkspaceStore();

const keyword = ref('');
const searchInput = ref(null);

// 按 SQL 关键字过滤；historyList 已是最新在前
const items = computed(() => {
  const q = keyword.value.trim().toLowerCase();
  if (!q) return ws.historyList;
  return ws.historyList.filter(e => String(e.sql || '').toLowerCase().includes(q));
});

watch(() => ui.modals.resultHistoryModal, async (v) => {
  if (!v) return;
  keyword.value = '';
  await nextTick();
  if (searchInput.value) searchInput.value.focus();
});

// 查看该条历史对应的结果集
function viewResult(e) {
  ws.selectHistory(e.id, false);
  ui.closeModal('resultHistoryModal');
  ui.showToast('已切换到该条历史结果', 'success');
}

// 查看结果并把该条 SQL 载回编辑器
function loadSql(e) {
  ws.selectHistory(e.id, true);
  ui.closeModal('resultHistoryModal');
  ui.showToast('已把该条 SQL 载入编辑器', 'success');
}

// 退出历史回看，回到最新一次执行的结果
function backToLatest() {
  ws.selectHistory(null, false);
  ui.closeModal('resultHistoryModal');
  ui.showToast('已回到最新结果', 'success');
}
</script>

<template>
  <ModalShell :visible="ui.modals.resultHistoryModal" width="900px" @close="ui.closeModal('resultHistoryModal')">
    <template #title>查询结果历史（{{ ws.historyList.length }} 条）</template>
    <div class="hist-tools">
      <input
        ref="searchInput"
        v-model="keyword"
        type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
        class="find-input"
        placeholder="按 SQL 关键字筛选..."
        v-tip="'输入关键字过滤历史记录'"
      >
      <span class="hist-count">{{ items.length }} / {{ ws.historyList.length }} 条</span>
      <button
        class="btn btn-secondary"
        :disabled="!ws.historyView"
        v-tip="'退出历史回看，回到最新一次执行的结果'"
        @click="backToLatest"
      >回到最新结果</button>
    </div>

    <div class="hist-list">
      <div
        v-for="e in items"
        :key="e.id"
        class="hist-item"
        :class="{ active: e.id === ws.historyView }"
        v-tip="'点击查看这一条的历史结果'"
        @click="viewResult(e)"
      >
        <div class="hist-meta">
          <span class="hist-time">{{ formatDateTime(new Date(e.time)) }}</span>
          <span>{{ e.data ? e.data.length : 0 }} 行</span>
          <span v-if="e.elapsed != null">{{ e.elapsed }}ms</span>
          <span v-if="e.db">库: {{ e.db }}</span>
          <span v-if="e.id === ws.historyView" class="hist-flag">当前查看</span>
          <button
            class="btn btn-secondary hist-load"
            v-tip="'把这条 SQL 载入编辑器并查看结果'"
            @click.stop="loadSql(e)"
          >载入 SQL</button>
        </div>
        <pre class="hist-sql" v-html="highlightSqlSyntax(e.sql, keyword)"></pre>
      </div>
      <div class="hist-empty" v-if="!items.length">
        {{ ws.historyList.length ? '没有匹配的历史记录' : '暂无查询结果历史' }}
      </div>
    </div>
  </ModalShell>
</template>
