import { createApp } from 'vue';
import type { Component } from 'vue';
import { resolveWindowView } from './ui/window-registry';
import './styles/app.css';

/**
 * 桌面 UI 唯一入口（薄入口，架构与契约 §0）：
 * 只做「窗口标签 → 视图组件 → 挂载」，不含业务判断、数据转换与状态计算。
 *
 * 标签来源：Tauri 窗口创建时的 URL（`index.html?window=<label>`），
 * 标签清单的权威定义在 `src-tauri/src/ui/window.rs` 的 `WINDOW_SPECS`。
 * 业务数据一律经 tauri `invoke` 从 core 取，UI 不直接读系统或存储。
 */

const label = new URLSearchParams(window.location.search).get('window') ?? '';
const view: Component | undefined = resolveWindowView(label);

const mountPoint = document.getElementById('app');
if (mountPoint === null) {
  throw new Error('挂载点 #app 缺失（index.html 被改动？）');
}

if (view === undefined) {
  mountPoint.textContent = `未知窗口标签：${label === '' ? '(空)' : label}`;
} else {
  createApp(view).mount(mountPoint);
}
