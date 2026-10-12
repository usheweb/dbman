<script setup>
// 结果区：标题 / 全局显示模式开关 / 全屏 / 查找条（同一行）+ 结果表 + 底部状态栏
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
          class="mode-toggle"
          :class="{ active: ws.globalDisplay.timestamp }"
          @click="ws.toggleGlobalMode('timestamp')"
        >时间戳转时间</button>
        <button
          class="mode-toggle"
          :class="{ active: ws.globalDisplay.enum }"
          @click="ws.toggleGlobalMode('enum')"
        >枚举转文字</button>
        <button
          class="mode-toggle"
          :class="{ active: ui.resultFullscreen }"
          :title="ui.resultFullscreen ? '退出全屏（Esc）' : '全屏显示结果表（Esc 退出）'"
          @click="toggleFullscreen"
        >{{ ui.resultFullscreen ? '退出全屏' : '全屏' }}</button>
      </div>
      <ResultFindBar />
    </div>
    <ResultTable />
    <ResultFooter />
  </div>
</template>
