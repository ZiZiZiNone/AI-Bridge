import vue from '@vitejs/plugin-vue';
import { defineConfig } from 'vite';

// Tauri 2 约定：dev 端口固定 1420，与 src-tauri/tauri.conf.json 的 devUrl 一致；
// 产物目录 dist/ 与 frontendDist 一致（宿主只加载本目录产物，不在此做任何业务处理）。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    // 只面向 Tauri 2 自带的 WebView2（Chromium 内核），无需为旧浏览器降级。
    target: 'chrome110',
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
  },
});
