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
      v-tip="'回车或点放大镜执行查找'"
      @keydown="onKey"
    />
    <span class="find-count" v-tip="'匹配的单元格数'">{{ ws.findState.keyword ? String(ws.matchCount) : '' }}</span>
    <label class="find-filter" v-tip="'勾选后只显示包含查找内容的行'">
      <input type="checkbox" :checked="ws.findState.only" @change="onToggle">过滤
    </label>
    <button class="icon-btn find-btn" v-tip="'查找 / 下一处'" @click="onFind">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <circle cx="10.5" cy="10.5" r="6.5" />
        <path d="M15.5 15.5L20 20" />
      </svg>
      <span class="btn-label">查找</span>
    </button>
    <button class="icon-btn find-btn" v-tip="'清除查找'" @click="onClear">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M6 6l12 12M18 6L6 18" />
      </svg>
      <span class="btn-label">清除</span>
    </button>
  </div>
</template>
