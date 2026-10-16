<script setup>
// 查询 Tab 条：每个连接一组查询 Tab，各自独立的 SQL 编辑器内容、结果集与 AI 栏
import { computed } from 'vue';
import { useWorkspaceStore } from '@/stores/workspace';
import { useConnectionStore } from '@/stores/connection';

const ws = useWorkspaceStore();
const conn = useConnectionStore();

// 提示文案带上连接与库，避免多个连接里同名「查询 1」分不清
const ctx = computed(() => {
  const c = conn.currentConnData;
  return [c && c.name, conn.currentDatabase].filter(Boolean).join(' · ');
});
</script>

<template>
  <div class="query-tabs-bar">
    <button
      v-for="t in ws.activeTabs"
      :key="t.id"
      class="conn-tab query-tab"
      :class="{ active: t.id === ws.activeTabId }"
      v-tip="ctx ? t.name + '（' + ctx + '）' : t.name"
      @click="ws.activateQueryTab(t.id)"
    >
      <svg class="conn-tab-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" aria-hidden="true">
        <path d="M9.4 1.8H4.6a1.4 1.4 0 0 0-1.4 1.4v9.6a1.4 1.4 0 0 0 1.4 1.4h6.8a1.4 1.4 0 0 0 1.4-1.4V5.2L9.4 1.8Z" />
        <path d="M9.4 1.8v3.4h3.4" />
      </svg>
      <span class="conn-tab-name">{{ t.name }}</span>
      <span
        v-if="ws.activeTabs.length > 1"
        class="conn-tab-close"
        v-tip="'关闭查询'"
        @click.stop="ws.closeQueryTab(t.id)"
      >
        <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
          <path d="M2.2 2.2 7.8 7.8M7.8 2.2 2.2 7.8" />
        </svg>
      </span>
    </button>
    <button class="query-tab-add" v-tip="'新建查询'" @click="ws.newQueryTab()">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true">
        <path d="M8 3.5v9M3.5 8h9" />
      </svg>
    </button>
  </div>
</template>
