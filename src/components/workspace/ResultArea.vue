<script setup>
// 结果区：标题 / 全局显示模式开关 / 全屏 / 历史结果下拉 / 查找条（同一行）+ 结果表 + 底部状态栏
import { computed, onMounted, onUnmounted } from 'vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
import IconSelect from '@/components/common/IconSelect.vue';
import ResultFindBar from './ResultFindBar.vue';
import ResultTable from './ResultTable.vue';
import ResultFooter from './ResultFooter.vue';

const ws = useWorkspaceStore();
const ui = useUiStore();

// 全屏显示结果表：Esc 退出，按钮同样可退出
function toggleFullscreen() {
  ui.toggleResultFullscreen();
}

function onKeydown(e) {
  if (e.key === 'Escape' && ui.resultFullscreen) ui.toggleResultFullscreen();
}

// 历史结果下拉：时间为「MM-DD HH:mm:ss」，附行数与 SQL 摘要
function histTime(ts) {
  const d = new Date(ts);
  const p = n => String(n).padStart(2, '0');
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function histLabel(e) {
  const brief = String(e.sql || '').replace(/\s+/g, ' ').trim();
  const sql = brief.length > 60 ? brief.slice(0, 60) + '…' : brief;
  return `${histTime(e.time)} · ${(e.data ? e.data.length : 0)} 行 · ${sql}`;
}

// 历史结果下拉的选项：首项回到「最新结果」，其余为各条历史（时间 · 行数 · SQL 摘要）
const historyOptions = computed(() => [
  { value: '', label: `最新结果（${ws.historyList.length} 条历史）` },
  ...ws.historyList.map(e => ({ value: e.id, label: histLabel(e) }))
]);

// 切换历史结果：先问是否把对应 SQL 载入编辑器，再切换结果
function onHistoryChange(id) {
  if (!id) { ws.selectHistory(null, false); return; }
  const loadSql = window.confirm('是否把这条历史记录对应的 SQL 载入编辑器？');
  ws.selectHistory(id, loadSql);
}

onMounted(() => window.addEventListener('keydown', onKeydown));
onUnmounted(() => window.removeEventListener('keydown', onKeydown));
</script>

<template>
  <div class="result-area">
    <div class="result-header">
      <span>查询结果</span>
      <span>{{ ws.lastResult ? ws.lastResult.data.length + ' 行' : '' }}</span>
      <div class="result-tools">
        <button
          class="icon-btn"
          :class="{ active: ws.globalDisplay.timestamp }"
          title="把时间戳列显示为可读时间"
          @click="ws.toggleGlobalMode('timestamp')"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="8.5" />
            <path d="M12 7.5V12l3 1.8" />
          </svg>
          <span class="btn-label">时间戳转时间</span>
        </button>
        <button
          class="icon-btn"
          :class="{ active: ws.globalDisplay.enum }"
          title="按字段注释里的枚举映射显示为文字"
          @click="ws.toggleGlobalMode('enum')"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 4h7l9 9-7 7-9-9z" />
            <circle cx="8.5" cy="8.5" r="1.3" />
          </svg>
          <span class="btn-label">枚举转文字</span>
        </button>
        <button
          class="icon-btn"
          :class="{ active: ui.resultFullscreen }"
          :title="ui.resultFullscreen ? '退出全屏（Esc）' : '全屏显示结果表（Esc 退出）'"
          @click="toggleFullscreen"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M9 4H4v5" />
            <path d="M15 4h5v5" />
            <path d="M15 20h5v-5" />
            <path d="M9 20H4v-5" />
          </svg>
          <span class="btn-label">{{ ui.resultFullscreen ? '退出全屏' : '全屏' }}</span>
        </button>
        <IconSelect
          class="history-picker"
          :title="'查看历史查询结果（最多保留 ' + ui.viewOpts.resultHistoryMax + ' 条）'"
          :model-value="ws.historyView || ''"
          :options="historyOptions"
          searchable
          @update:model-value="onHistoryChange"
        />
      </div>
      <ResultFindBar />
    </div>
    <ResultTable />
    <ResultFooter />
  </div>
</template>
