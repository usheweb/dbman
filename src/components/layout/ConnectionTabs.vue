<script setup>
// 顶部连接 Tab 条：每条已打开的连接一个按钮，右上角 × 关闭（断开连接）
import { useConnectionStore } from '@/stores/connection';
import ItemIcon from '@/components/common/ItemIcon.vue';

const conn = useConnectionStore();
</script>

<template>
  <!-- 始终渲染容器：顶栏用它占住中间那一列，保证右侧按钮靠右 -->
  <div class="conn-tabs">
    <button
      v-for="t in conn.tabs"
      :key="t.id"
      class="conn-tab"
      :class="{ active: t.active }"
      v-tip="t.name"
      @click="conn.activateTab(t.id)"
    >
      <ItemIcon class="conn-tab-icon" :name="t.type" />
      <span class="conn-tab-name">{{ t.name }}</span>
      <span class="conn-tab-close" v-tip="'关闭连接'" @click.stop="conn.closeTab(t.id)">
        <svg viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true">
          <path d="M2.2 2.2 7.8 7.8M7.8 2.2 2.2 7.8" />
        </svg>
      </span>
    </button>
  </div>
</template>