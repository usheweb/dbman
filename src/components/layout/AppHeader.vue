<script setup>
import { useConnectionStore } from '@/stores/connection';
import { useUiStore } from '@/stores/ui';

const conn = useConnectionStore();
const ui = useUiStore();
</script>

<template>
  <div class="header">
    <div class="brand">
      <img class="brand-logo" src="@/assets/logo.svg" alt="">
      <h1>DBMan</h1>
    </div>
    <div class="header-right">
      <button class="btn btn-secondary btn-sm" @click="ui.openModal('aiConfigModal')">AI 设置</button>
      <div id="connectionStatus">
        <template v-if="conn.isConnected">
          <span style="color:#25f4b0;font-family:var(--font-mono);font-size:13px;text-shadow:0 0 12px rgba(37,244,176,.6);">✓ 已连接: {{ conn.currentConnData.name }}</span>
          <button class="disconnect-btn" @click="conn.disconnectFromUi()">断开连接</button>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
}

.brand-logo {
  width: 28px;
  height: 28px;
  display: block;
  filter: drop-shadow(0 0 10px rgba(0, 229, 255, .35));
}

.header-right {
  display: flex;
  align-items: center;
  gap: 14px;
}

.btn-sm {
  padding: 4px 12px;
  font-size: 12px;
}
</style>
