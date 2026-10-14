<script setup>
// 结果区：标题 / 全局显示模式开关 / 全屏 / 历史结果面板 / 查找条（同一行）+ 结果表 + 底部状态栏
import { onMounted, onUnmounted } from 'vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { useUiStore } from '@/stores/ui';
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
          v-if="ws.hasPendingEdits"
          class="icon-btn"
          :title="'放弃当前 ' + ws.pendingEditCount + ' 个未保存的单元格修改'"
          @click="ws.discardPendingEdits()"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M6 6l12 12M18 6L6 18" />
          </svg>
          <span class="btn-label">放弃修改</span>
        </button>
        <button
          v-if="ws.hasPendingEdits"
          class="icon-btn save-edit-btn"
          :title="'把结果表中暂存的 ' + ws.pendingEditCount + ' 个单元格修改生成 UPDATE 并执行'"
          @click="ws.savePendingEdits()"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M19.5 21h-15A1.5 1.5 0 0 1 3 19.5v-15A1.5 1.5 0 0 1 4.5 3H16l5 5v11.5A1.5 1.5 0 0 1 19.5 21z" />
            <path d="M17 21v-7H7v7" />
            <path d="M7 3v5h7" />
          </svg>
          <span class="btn-label">保存修改 ({{ ws.pendingEditCount }})</span>
        </button>
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
        <button
          class="icon-btn"
          :class="{ active: !!ws.historyView }"
          :title="'查看历史查询结果（最多保留 ' + ui.viewOpts.resultHistoryMax + ' 条）'"
          @click="ui.openModal('resultHistoryModal')"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3.5 12a8.5 8.5 0 1 0 2.5-6" />
            <path d="M3.5 4.5V9H8" />
            <path d="M12 7.5V12l3 1.8" />
          </svg>
          <span class="btn-label">历史结果 ({{ ws.historyList.length }})</span>
        </button>
      </div>
      <ResultFindBar />
    </div>
    <ResultTable />
    <ResultFooter />
  </div>
</template>
