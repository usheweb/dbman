<script setup>
// 结果集查找 / 过滤工具条
import { ref } from 'vue';
import { useWorkspaceStore } from '@/stores/workspace';

const ws = useWorkspaceStore();
const keyword = ref('');

function onKey(e) {
  if (e.key === 'Enter') {
    e.preventDefault();
    ws.applyFind(keyword.value);
  } else if (e.key === 'Escape') {
    ws.clearFind();
  }
}

function onFind() {
  ws.applyFind(keyword.value);
}

function onClear() {
  keyword.value = '';
  ws.clearFind();
}

function onToggle(e) {
  const ok = ws.toggleFindOnly(e.target.checked, keyword.value);
  if (!ok) e.target.checked = false;
}

function clear() {
  keyword.value = '';
  ws.clearFind();
}

defineExpose({ clear });
</script>

<template>
  <div class="result-find">
    <input
      v-model="keyword"
      type="text" autocapitalize="off" autocorrect="off" spellcheck="false"
      class="find-input"
      placeholder="在结果中查找..."
      title="回车或点放大镜执行查找"
      @keydown="onKey"
    />
    <span class="find-count" title="匹配的单元格数">{{ ws.findState.keyword ? String(ws.matchCount) : '' }}</span>
    <label class="find-filter" title="勾选后只显示包含查找内容的行">
      <input type="checkbox" :checked="ws.findState.only" @change="onToggle">过滤
    </label>
    <button class="find-btn" title="查找 / 下一处" @click="onFind">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" aria-hidden="true"><circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2L14 14"/></svg>
    </button>
    <button class="find-btn" title="清除查找" @click="onClear">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8"/></svg>
    </button>
  </div>
</template>
