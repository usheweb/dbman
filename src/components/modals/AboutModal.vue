<script setup>
import ModalShell from '@/components/common/ModalShell.vue';
import { useUiStore } from '@/stores/ui';

const ui = useUiStore();
// 编译期常量（vite.config.js 的 define 注入）
const version = __APP_VERSION__;

// 官网 / 联系邮箱 / 源码仓库。WebView 里点外链会顶掉当前页面（未装 opener 插件），
// 所以统一给「复制」按钮，用户拿到地址后自己去浏览器打开 —— 与「复制表名」同一套剪贴板用法
const CONTACTS = [
  { label: '官网', value: 'http://dbman.tangsanlang.com/' },
  { label: '联系邮箱', value: 'app@usheweb.com' },
  { label: '源码仓库', value: 'https://github.com/usheweb/dbman' }
];

async function copy(text, label) {
  try {
    await navigator.clipboard.writeText(text);
    ui.showToast(`已复制${label}`, 'success');
  } catch (e) {
    ui.showToast('复制失败', 'error');
  }
}
</script>

<template>
  <ModalShell :visible="ui.modals.aboutModal" width="560px" @close="ui.closeModal('aboutModal')">
    <template #title>关于</template>
    <div class="about-body">
      <div class="about-head">
        <span class="about-name">唐三浪DBMan</span>
        <span class="about-ver">v{{ version }}</span>
      </div>
      <p class="about-desc">
        基于 Tauri 2 + Rust + Vue 3 的现代桌面数据库管理工具，把 MySQL / MariaDB、PostgreSQL、
        SQLite、SQL Server、Oracle、MongoDB、Redis 放进同一个工作台。安装包小、内存占用低，
        无需额外安装运行时。
      </p>
      <div class="about-list">
        <div v-for="c in CONTACTS" :key="c.label" class="about-row">
          <span class="about-label">{{ c.label }}</span>
          <span class="about-value">{{ c.value }}</span>
          <button class="btn-line about-copy" @click="copy(c.value, c.label)">复制</button>
        </div>
      </div>
      <p class="about-foot">本项目基于 MIT License 开源</p>
    </div>
  </ModalShell>
</template>

<style scoped>
.about-body {
  font-family: var(--font-cjk);
  font-size: 13px;
  line-height: 1.7;
  letter-spacing: .5px;
  color: var(--text-dim);
}

.about-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.about-name {
  font-family: var(--font-display);
  font-size: 17px;
  font-weight: 700;
  color: var(--cyan);
  text-shadow: 0 0 14px rgba(var(--cyan-rgb), .35);
}

.about-ver {
  font-family: var(--font-mono);
  font-size: 12px;
  font-weight: 600;
  color: var(--text-dim);
}

.about-desc {
  margin: 10px 0 14px;
}

.about-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.about-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border: 1px solid var(--line);
  border-radius: 6px;
  background: rgba(var(--ink-4-rgb), .45);
}

.about-label {
  flex: 0 0 auto;
  min-width: 60px;
  color: var(--text);
}

.about-value {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--cyan);
  user-select: text;
}

.about-copy {
  flex: 0 0 auto;
  height: 24px;
  padding: 0 8px;
}

.about-foot {
  margin: 14px 0 0;
  font-size: 12px;
  opacity: .8;
}
</style>
