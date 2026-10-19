<script setup>
// 结果区底部状态栏：分页 + 每页显示 + 执行状态 + 数据导出
import { computed, ref, watch, onUnmounted } from 'vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import { escapeHtml } from '@/utils/escape';

const ws = useWorkspaceStore();
const ui = useUiStore();

// 编译期常量（vite.config.js 的 define 注入），在状态栏右下角展示当前版本
const version = __APP_VERSION__;

// 执行中计时：慢查询 / 被元数据锁挡住时，秒数在走才能说明「还在跑」而不是「卡死了」，
// 否则用户只能盯着一个空白结果面板 + 转圈的图标猜
const busyMs = ref(0);
let busyTimer = null;
let busyStarted = 0;
watch(() => ws.busy, v => {
  clearInterval(busyTimer);
  busyTimer = null;
  busyMs.value = 0;
  if (!v) return;
  busyStarted = Date.now();
  busyTimer = setInterval(() => { busyMs.value = Date.now() - busyStarted; }, 100);
});
onUnmounted(() => clearInterval(busyTimer));

const statusHtml = computed(() => {
  const p = ws.pageInfo;
  // 执行中：先给运行态，替代「执行成功/失败」的既有文案
  if (ws.busy) {
    return `<span class="hint">正在执行… 已用 <b>${(busyMs.value / 1000).toFixed(1)}</b> 秒</span>`;
  }
  // 多语句执行：结果区是逐条反馈列表，底部只给汇总
  if (ws.stmtResults) {
    const l = ws.stmtResults;
    const ok = l.filter(r => r.status === 'success').length;
    const fail = l.filter(r => r.status === 'error').length;
    const skip = l.filter(r => r.status === 'skipped').length;
    return `<span class="ok">共 ${l.length} 条：成功 ${ok}</span>`
      + (fail ? ` · <span class="err">失败 ${fail}</span>` : '')
      + (skip ? ` · <span class="hint">未执行 ${skip}</span>` : '');
  }
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

// 查看历史结果时禁用翻页：翻页是复用最近一次 SQL 重新查库，历史条目已定型
const pagingDisabled = computed(() => !!ws.historyView || !ws.pageInfo.paged);
const pagingTitle = computed(() => ws.historyView
  ? '正在查看历史结果，翻页请先切回「最新结果」'
  : (ws.pageInfo.paged ? '输入页码后回车跳转' : 'SQL 自带 LIMIT 或不是查询语句，未启用分页'));

function onChangePageSize(e) {
  ws.changePageSize(e.target.value);
}

// ===== 页码输入框：可直接输入页码跳转 =====
const pageInput = ref(String(ws.pageNo));
watch(() => ws.pageNo, v => { pageInput.value = String(v); });

function onPageInput(e) {
  pageInput.value = e.target.value.replace(/\D/g, '');
}

async function commitPage() {
  const before = ws.pageNo;
  const n = parseInt(pageInput.value, 10);
  if (!ws.pageInfo.paged || !n || n < 1 || n === before) {
    pageInput.value = String(before);
    return;
  }
  // 总页数未知（分页靠多取一行判断），向前跳只允许到已确认还有数据的页
  if (n > before && !ws.pageInfo.hasMore) {
    pageInput.value = String(before);
    ui.showToast('已经是最后一页了', 'error');
    return;
  }
  await ws.gotoPage(n);
  const p = ws.pageInfo;
  // 跳到超出数据的页码会返回 0 行，回退到原来的页并提示
  if (n > before && ws.pageNo === n && !p.error && !p.write && p.rows === 0) {
    await ws.gotoPage(before);
    ui.showToast(`第 ${n} 页没有数据`, 'error');
  }
  pageInput.value = String(ws.pageNo);
}
</script>

<template>
  <div class="result-footer">
    <div class="page-group">
      <button
        class="page-btn"
        v-tip="'上一页'"
        :disabled="pagingDisabled || ws.pageNo <= 1"
        @click="ws.gotoPage(ws.pageNo - 1)"
      >‹</button>
      <input
        class="page-input"
        type="text"
        inputmode="numeric"
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
        :disabled="pagingDisabled"
        v-tip="pagingTitle"
        :value="pageInput"
        @input="onPageInput"
        @keyup.enter="commitPage"
        @blur="commitPage"
      />
      <button
        class="page-btn"
        v-tip="'下一页'"
        :disabled="pagingDisabled || !ws.pageInfo.hasMore"
        @click="ws.gotoPage(ws.pageNo + 1)"
      >›</button>
    </div>
    <label class="page-size-label">每页显示
      <select
        class="page-size"
        :value="String(ws.pageSize)"
        :disabled="pagingDisabled"
        v-tip="pagingTitle"
        @change="onChangePageSize"
      >
        <option v-for="opt in pageSizeOptions" :key="opt" :value="String(opt)">{{ opt }}</option>
      </select>
    </label>
    <span class="footer-status" v-html="statusHtml"></span>
    <span v-if="ws.hasPendingEdits" class="footer-pending" v-tip="'翻页会丢弃这些未保存的修改'">
      有 {{ ws.pendingEditCount }} 个单元格修改待保存
    </span>
    <button
      class="export-link"
      v-tip="'导出结果为 CSV（可选表头与范围）'"
      :disabled="!ws.lastSql || ws.pageInfo.idle || ws.pageInfo.write || !!ws.stmtResults"
      @click="ui.openModal('exportModal')"
    >【数据导出】</button>
    <!-- 当前版本：贴右下角，点击打开「关于」看产品说明 / 联系方式 / 官网 -->
    <button
      class="footer-version"
      v-tip="'查看版本与产品说明'"
      @click="ui.openModal('aboutModal')"
    >v{{ version }}</button>
  </div>
</template>
