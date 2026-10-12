<script setup>
import { useConnectionStore } from '@/stores/connection';
import { useUiStore } from '@/stores/ui';
import { useThemeStore } from '@/stores/theme';

const conn = useConnectionStore();
const ui = useUiStore();
const theme = useThemeStore();
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
          <span class="conn-ok">✓ 已连接: {{ conn.currentConnData.name }}</span>
          <button class="disconnect-btn" @click="conn.disconnectFromUi()">断开连接</button>
        </template>
      </div>
      <button
        class="theme-toggle"
        :title="theme.theme === 'dark' ? '切换到亮色主题' : '切换到暗色主题'"
        @click="theme.toggle()"
      >
        <!-- 暗色显示太阳（点击切亮色），亮色显示月亮（点击切暗色） -->
        <svg v-if="theme.theme === 'dark'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M19.1 4.9l-1.4 1.4M6.3 17.7l-1.4 1.4" />
        </svg>
        <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
        </svg>
      </button>
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
  filter: drop-shadow(0 0 10px rgba(var(--cyan-rgb), .35));
}

.conn-ok {
  color: var(--green);
  font-family: var(--font-mono);
  font-size: 13px;
  text-shadow: 0 0 12px rgba(var(--green-rgb), .6);
}

/* 主题切换：亮色显示月亮、暗色显示太阳，图标随主题变化 */
.theme-toggle {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  padding: 0;
  border-radius: 6px;
  border: 1px solid var(--line);
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  transition: all .2s ease;
}

.theme-toggle:hover {
  color: var(--cyan);
  border-color: var(--line-strong);
  box-shadow: var(--glow-cyan);
}

.theme-toggle svg {
  width: 16px;
  height: 16px;
  display: block;
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
