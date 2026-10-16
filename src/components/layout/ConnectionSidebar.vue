<script setup>
import { useUiStore } from '@/stores/ui';
import { useConnectionStore } from '@/stores/connection';
import ConnectionItem from '@/components/connection/ConnectionItem.vue';

const ui = useUiStore();
const connStore = useConnectionStore();
</script>

<template>
  <div class="sidebar" :class="{ collapsed: ui.panelState.sidebar }">
    <div class="sidebar-header">
      <span>连接</span>
      <div class="sidebar-header-actions">
        <button id="addConnBtn" class="btn" @click="ui.openConnectionModal(null)">+ 新建</button>
        <button
          class="panel-toggle"
          v-tip="(ui.panelState.sidebar ? '展开' : '收起') + ui.panelTitle('sidebar') + '列'"
          @click="ui.togglePanel('sidebar')"
        >
          <span class="toggle-glyph">{{ ui.panelState.sidebar ? '›' : '‹' }}</span>
        </button>
      </div>
    </div>
    <div class="connection-list">
      <ConnectionItem v-for="conn in connStore.connections" :key="conn.id" :conn="conn" />
    </div>
  </div>
</template>
