// 主题：亮色 / 暗色切换
// 实现：JS 切 <html> 的 data-theme 属性（light / dark），CSS 变量据此换色板。
// 持久化用 localStorage（key = dbman_theme）：必须同步读写，页面加载前就能拿到值，
// 因此这里不接 store.rs 那套异步落盘，避免启动时先渲染暗色再跳亮色。
import { defineStore } from 'pinia';
import { computed, ref } from 'vue';

export const THEME_KEY = 'dbman_theme';
const THEMES = ['light', 'dark'];

// 同步读取已保存的主题，非法值/读取失败一律回退亮色（默认亮色）
function readStored() {
  try {
    const t = localStorage.getItem(THEME_KEY);
    return THEMES.includes(t) ? t : 'light';
  } catch (e) {
    return 'light';
  }
}

export const useThemeStore = defineStore('theme', () => {
  const theme = ref(readStored());

  // CodeMirror 5 主题名：亮色用 eclipse，暗色用 monokai
  const editorTheme = computed(() => (theme.value === 'dark' ? 'monokai' : 'eclipse'));

  // 把当前主题写到 <html data-theme>，CSS 变量随之切换
  function apply() {
    document.documentElement.setAttribute('data-theme', theme.value);
  }

  function setTheme(next) {
    theme.value = THEMES.includes(next) ? next : 'light';
    apply();
    try {
      localStorage.setItem(THEME_KEY, theme.value);
    } catch (e) {
      // 写入失败（隐私模式等）不影响本次会话的主题
    }
  }

  function toggle() {
    setTheme(theme.value === 'dark' ? 'light' : 'dark');
  }

  // 启动兜底：index.html 的内联脚本已提前设置过属性，这里再同步一次 store 状态
  function init() {
    theme.value = readStored();
    apply();
  }

  return { theme, editorTheme, apply, setTheme, toggle, init };
});
