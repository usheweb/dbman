<script setup>
import { ref, computed, watch, nextTick } from 'vue';
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';
import { useWorkspaceStore } from '@/stores/workspace';
import { useSqlStore } from '@/stores/sql';
import { highlightSqlSyntax } from '@/utils/sql';
import { formatDateTime } from '@/utils/format';

const ui = useUiStore();
const ws = useWorkspaceStore();
const sqlStore = useSqlStore();

const keyword = ref('');
const date = ref('');
const searchInput = ref(null);

const items = computed(() => sqlStore.filterLogs(keyword.value, date.value));
const dates = computed(() => sqlStore.logDates());

watch(() => ui.modals.sqlLogModal, async (v) => {
  if (v) {
    keyword.value = '';
    date.value = '';
    await nextTick();
    if (searchInput.value) searchInput.value.focus();
  }
});

function useLogSql(idx) {
  const e = sqlStore.sqlLog[idx];
  if (!e) return;
  ws.setSql(e.sql);
  ui.closeModal('sqlLogModal');
  ui.showToast('已载入 SQL 到编辑器', 'success');
}

function deleteLog(idx) {
  sqlStore.removeLog(idx);
  ui.showToast('已删除该条日志', 'success');
}

function favFromLog(idx) {
  const e = sqlStore.sqlLog[idx];
  if (!e) return;
  const r = sqlStore.toggleFavBySql(e.sql);
  if (r) {
    ui.showToast(r);
  } else {
    sqlStore.openFavDraft({ sql: e.sql, db: e.db, conn: e.conn });
    ui.openModal('favEditModal');
  }
}

function clearLog() {
  if (!confirm('确定清空全部 SQL 日志吗？')) return;
  sqlStore.clearLog();
  ui.showToast('SQL 日志已清空', 'success');
}
</script>

<template>
  <ModalShell :visible="ui.modals.sqlLogModal" width="900px" @close="ui.closeModal('sqlLogModal')">
    <template #title>SQL 日志</template>
    <div class="log-tools">
      <input type="text" autocapitalize="off" autocorrect="off" spellcheck="false" class="find-input" ref="searchInput" v-model="keyword" placeholder="模糊匹配 SQL 语句..." title="支持包含匹配与模糊匹配">
      <select class="log-date" v-model="date" title="按日期筛选">
        <option value="">全部日期</option>
        <option v-for="d in dates" :key="d.date" :value="d.date">{{ d.date }}（{{ d.count }}）</option>
      </select>
      <span class="log-count">{{ items.length }} / {{ sqlStore.sqlLog.length }} 条</span>
      <button class="btn btn-secondary" @click="clearLog">清空日志</button>
    </div>
    <div class="log-list">
      <div class="log-item" v-for="x in items" :key="x.idx" @click="useLogSql(x.idx)" title="点击把这条 SQL 载入编辑器">
        <div class="log-meta">
          <span class="log-type" :class="sqlStore.sqlLogClass(x.e.type)">{{ x.e.type }}</span>
          <span>{{ formatDateTime(new Date(x.e.t)) }}</span>
          <span v-if="x.e.db">库: {{ x.e.db }}</span>
          <span v-if="x.e.conn">连接: {{ x.e.conn }}</span>
          <span :class="x.e.ok ? 'log-ok' : 'log-err'">{{ x.e.ok ? '✓ ' + x.e.rows + ' 行' : '✕ 失败' }} · {{ x.e.elapsed }}ms</span>
          <span v-if="x.e.err" class="log-err">{{ x.e.err }}</span>
          <button class="log-fav" :class="{ active: sqlStore.isFaved(x.e.sql) }" :title="sqlStore.isFaved(x.e.sql) ? '取消收藏' : '收藏这条 SQL'" @click.stop="favFromLog(x.idx)">&starf;</button>
          <button class="log-del" title="删除这条日志" @click.stop="deleteLog(x.idx)">&times;</button>
        </div>
        <div class="log-sql" v-html="highlightSqlSyntax(x.e.sql, keyword)"></div>
      </div>
      <div class="log-empty" v-if="!items.length">{{ sqlStore.sqlLog.length ? '没有匹配的 SQL' : '暂无 SQL 日志' }}</div>
    </div>
  </ModalShell>
</template>
