<script setup>
// 结果区：标题 / 全局显示模式开关 / 查找条（同一行）+ 结果表 + 底部状态栏
import { useWorkspaceStore } from '@/stores/workspace';
import ResultFindBar from './ResultFindBar.vue';
import ResultTable from './ResultTable.vue';
import ResultFooter from './ResultFooter.vue';

const ws = useWorkspaceStore();
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
      </div>
      <ResultFindBar />
    </div>
    <ResultTable />
    <ResultFooter />
  </div>
</template>
