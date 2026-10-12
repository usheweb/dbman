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
      :title="t.name"
      @click="conn.activateTab(t.id)"
    >
      <ItemIcon :name="t.type" />
      <span class="conn-tab-name">{{ t.name }}</span>
      <span class="conn-tab-close" title="关闭连接" @click.stop="conn.closeTab(t.id)">×</span>
    </button>
  </div>
</template>