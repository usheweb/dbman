<script setup>
import { ref } from 'vue';
import { useUiStore } from '@/stores/ui';
import { useThemeStore } from '@/stores/theme';
import { useConnectionStore } from '@/stores/connection';
import { useAiStore } from '@/stores/ai';
import { confirmDialog } from '@/api';
import ContextMenu from '@/components/common/ContextMenu.vue';
import ConnectionTabs from '@/components/layout/ConnectionTabs.vue';

const ui = useUiStore();
const theme = useThemeStore();
const connStore = useConnectionStore();
const ai = useAiStore();

// 「设置」菜单：AI 设置 / 视图设置 / 清除已保存的密码
const settingsVisible = ref(false);
const settingsBtnEl = ref(null);
const settingsX = ref(0);
const settingsY = ref(0);

const settingsItems = [
  { label: 'AI 设置', value: 'ai' },
  { label: '视图设置', value: 'view' },
  { separator: true },
  { label: '清除已保存的密码', value: 'purge-secrets', danger: true }
];

function openSettingsMenu() {
  if (settingsVisible.value) {
    settingsVisible.value = false;
    return;
  }
  const rect = settingsBtnEl.value && settingsBtnEl.value.getBoundingClientRect();
  if (rect) {
    settingsX.value = rect.left;
    settingsY.value = rect.bottom + 6;
  }
  settingsVisible.value = true;
}

async function onSettingsSelect(item) {
  settingsVisible.value = false;
  if (!item) return;
  if (item.value === 'ai') ui.openModal('aiConfigModal');
  else if (item.value === 'view') ui.openModal('viewSettingsModal');
  else if (item.value === 'purge-secrets') await purgeSecrets();
}

// 清空系统钥匙串里 DBMan 保存的全部秘密（数据库密码、SSH 密码/私钥、AI apiKey）。
// 连接配置本身保留，下次连接时重新输入密码即可
async function purgeSecrets() {
  const ok = await confirmDialog(
    '将清除钥匙串中 DBMan 保存的全部密码：数据库密码、SSH 密码/私钥、AI apiKey。\n\n连接配置本身会保留，下次连接时需要重新输入密码。是否继续？',
    { title: '清除已保存的密码', kind: 'warning' }
  );
  if (!ok) return;
  const connOk = await connStore.purgeSecrets();
  const aiOk = await ai.purgeSecret();
  if (connOk && aiOk) ui.showToast('已清除全部已保存的密码', 'success');
  else ui.showToast('部分凭据清除失败，请在系统钥匙串中手动删除', 'error');
}
</script>

<template>
  <div class="header">
    <div class="brand">
      <img class="brand-logo" src="@/assets/logo.svg" alt="">
      <h1>唐三浪DBMan</h1>
    </div>
    <ConnectionTabs />
    <div class="header-right">
      <ContextMenu
        v-if="settingsVisible"
        :x="settingsX"
        :y="settingsY"
        :items="settingsItems"
        @select="onSettingsSelect"
        @close="settingsVisible = false"
      />
      <button
        class="icon-btn theme-toggle"
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
      <button
        ref="settingsBtnEl"
        class="icon-btn"
        :class="{ open: settingsVisible }"
        title="设置"
        @mousedown.stop
        @click.stop="openSettingsMenu"
      >
        <!-- 齿轮图标，与右侧主题切换按钮同一套图标按钮样式 -->
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9c.14.35.4.64.73.83.3.18.65.27 1 .27H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
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

/* 图标按钮样式见 theme.css 的 .icon-btn */

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
