<script setup>
// 多语句执行的逐条结果反馈列表（phpMyAdmin 风格）：
// 一次执行多条语句后，按条列出「序号 / 成功·失败·未执行 / 语句摘要 / 影响行数 / 耗时 / 错误」，
// 让用户一眼看清哪条执行成功、哪条失败、失败后哪些没跑
import { computed } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useWorkspaceStore } from '@/stores/workspace';

const ws = useWorkspaceStore();
const ui = useUiStore();

const list = computed(() => ws.stmtResults || []);

const summary = computed(() => {
  const l = list.value;
  return {
    total: l.length,
    ok: l.filter(r => r.status === 'success').length,
    fail: l.filter(r => r.status === 'error').length,
    skip: l.filter(r => r.status === 'skipped').length
  };
});

const STATUS_LABEL = { success: '成功', error: '失败', skipped: '未执行' };

// 语句摘要：合并空白后截断，全文走悬浮提示（项目统一 v-tip，禁止原生 title）
function brief(sql) {
  const s = String(sql || '').replace(/\s+/g, ' ').trim();
  return s.length > 70 ? s.slice(0, 70) + '…' : s;
}
</script>

<template>
  <div class="stmt-list">
    <div class="stmt-head">
      <span class="stmt-sum">共 {{ summary.total }} 条语句</span>
      <span class="ok">成功 {{ summary.ok }}</span>
      <span v-if="summary.fail" class="err">失败 {{ summary.fail }}</span>
      <span v-if="summary.skip" class="dim">未执行 {{ summary.skip }}</span>
    </div>
    <ul class="stmt-rows">
      <li v-for="(r, i) in list" :key="i" class="stmt-row" :class="r.status">
        <span class="stmt-no">{{ i + 1 }}</span>
        <span class="stmt-badge" :class="r.status">{{ STATUS_LABEL[r.status] || r.status }}</span>
        <div class="stmt-main">
          <div class="stmt-sql" v-tip="r.sql" @dblclick="ui.openContent(r.sql)">{{ brief(r.sql) }}</div>
          <div
            v-if="r.error"
            class="stmt-error"
            v-tip="r.error"
            @dblclick="ui.openContent(r.error)"
          >{{ r.error }}</div>
        </div>
        <span class="stmt-meta">
          <template v-if="r.status === 'success'">
            影响 <b>{{ r.affected }}</b> 行<template v-if="r.rows"> · 返回 <b>{{ r.rows }}</b> 行</template>
          </template>
          <template v-else>—</template>
        </span>
        <span class="stmt-ms">{{ r.status === 'skipped' ? '' : r.elapsed_ms + 'ms' }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.stmt-list {
  padding: 10px 12px 16px;
  font-family: var(--font-cjk);
}
.stmt-head {
  display: flex;
  align-items: center;
  gap: 14px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--line);
  font-size: 12px;
  letter-spacing: 1px;
  color: var(--text-dim);
}
.stmt-sum {
  color: var(--text);
  font-weight: 600;
}
.stmt-head .ok { color: var(--green); }
.stmt-head .err { color: var(--magenta); }
.stmt-head .dim { color: var(--text-dim); }

.stmt-rows {
  list-style: none;
  margin: 0;
  padding: 0;
}
.stmt-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 8px 6px;
  border-bottom: 1px dashed var(--line);
}
.stmt-row.error {
  background: rgba(var(--magenta-rgb), 0.07);
}
.stmt-no {
  flex: none;
  width: 22px;
  padding-top: 1px;
  text-align: right;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-dim);
}
.stmt-badge {
  flex: none;
  min-width: 48px;
  padding: 1px 6px;
  border: 1px solid transparent;
  border-radius: 4px;
  text-align: center;
  font-size: 11px;
  letter-spacing: 1px;
}
.stmt-badge.success {
  color: var(--green);
  border-color: rgba(var(--green-rgb), 0.5);
}
.stmt-badge.error {
  color: var(--magenta);
  border-color: rgba(var(--magenta-rgb), 0.55);
}
.stmt-badge.skipped {
  color: var(--text-dim);
  border-color: var(--line);
}
.stmt-main {
  flex: 1;
  min-width: 0;
}
.stmt-sql {
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.stmt-error {
  margin-top: 3px;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--magenta);
  overflow-wrap: break-word;
}
.stmt-meta {
  flex: none;
  width: 156px;
  padding-top: 1px;
  text-align: right;
  font-size: 12px;
  color: var(--text-dim);
}
.stmt-meta b {
  color: var(--cyan);
}
.stmt-ms {
  flex: none;
  width: 58px;
  padding-top: 1px;
  text-align: right;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text-dim);
}
</style>
