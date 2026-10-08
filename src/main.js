import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';

// ===== 字体本地化 =====
// 原来走 Google Fonts CDN，桌面端离线不可用，改为 @fontsource 打包进产物。
// font-family 名与 CSS 令牌一致：Orbitron / Rajdhani / JetBrains Mono
import '@fontsource/orbitron/400.css';
import '@fontsource/orbitron/700.css';
import '@fontsource/rajdhani/400.css';
import '@fontsource/rajdhani/500.css';
import '@fontsource/rajdhani/600.css';
import '@fontsource/rajdhani/700.css';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';

// ===== CodeMirror 5 =====
// 保留 CM5（而非升级 CM6）：现有 SQL 补全引擎重度依赖 CM5 的 show-hint / getTokenAt API
import 'codemirror/lib/codemirror.css';
import 'codemirror/theme/monokai.css';
import 'codemirror/addon/hint/show-hint.css';

// ===== 全局主题（深色科技感 / Neon HUD）=====
import './assets/styles/theme.css';

const app = createApp(App);
app.use(createPinia());
app.mount('#app');
