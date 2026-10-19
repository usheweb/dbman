import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import { fileURLToPath, URL } from 'node:url';
import { readFileSync } from 'node:fs';

// 版本号从 package.json 读入编译期常量：顶栏要显示当前版本，
// 打包后再去读文件没有意义（资源已嵌入二进制），运行时的 Tauri getVersion 又要额外授权
const pkg = JSON.parse(readFileSync(fileURLToPath(new URL('./package.json', import.meta.url)), 'utf8'));

// Tauri 期望固定端口且开发时忽略 src-tauri 目录，避免 Rust 编译触发前端热重载
export default defineConfig({
  plugins: [vue()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version)
  },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    }
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ['**/src-tauri/**']
    }
  },
  // WebView 内核按各平台系统版本走，这里对齐 Tauri 官方建议的构建目标
  build: {
    target: 'es2021',
    minify: 'esbuild',
    sourcemap: false,
    chunkSizeWarningLimit: 1500
  }
});
