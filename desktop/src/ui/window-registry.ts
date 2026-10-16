import type { Component } from 'vue';
import CandidateBar from './candidate/CandidateBar.vue';
import AuditPage from './pages/AuditPage.vue';
import BackupPage from './pages/BackupPage.vue';
import ConvertersPage from './pages/ConvertersPage.vue';
import LibraryPage from './pages/LibraryPage.vue';
import PairingPage from './pages/PairingPage.vue';
import SessionsPage from './pages/SessionsPage.vue';
import StatusPage from './pages/StatusPage.vue';
import SettingsPanel from './panels/SettingsPanel.vue';
import NotificationToast from './tray/NotificationToast.vue';
import TrayMenu from './tray/TrayMenu.vue';
import InitWizard from './wizard/InitWizard.vue';
import PairingWizard from './wizard/PairingWizard.vue';
import RecoveryDrillWizard from './wizard/RecoveryDrillWizard.vue';
import RestoreWizard from './wizard/RestoreWizard.vue';

/**
 * 窗口标签 → 视图组件（UI 侧唯一登记表，无业务逻辑）。
 *
 * 标签集合必须与 `src-tauri/src/ui/window.rs` 的 `WINDOW_SPECS` 一致（13 个 Tauri 窗口），
 * 由 `npm run check:structure` 校验；窗口几何 / 形态只在 Rust 侧声明，本表不重复声明。
 * 视图编号与 `docs/功能点全景.md` §3.1 的 W1–W15 对应。
 *
 * 已接线的面板走 Page 容器（持有 hook 与状态，数据 props 下传、事件上接）；
 * 未接线界面（W10 设置、向导、候选条）仍为骨架视图。
 */

/** Tauri 窗口（W2–W14）：候选条 1 个 + 面板 8 个 + 向导 4 个。 */
export const WINDOW_VIEWS: Readonly<Record<string, Component>> = {
  candidate: CandidateBar, // W2 候选条
  'panel-library': LibraryPage, // W3 库管理
  'panel-status': StatusPage, // W4 状态清单
  'panel-sessions': SessionsPage, // W5 临时会话
  'panel-audit': AuditPage, // W6 审计
  'panel-backup': BackupPage, // W7 备份
  'panel-converters': ConvertersPage, // W8 转换器
  'panel-pairing': PairingPage, // W9 配对
  'panel-settings': SettingsPanel, // W10 设置
  'wizard-init': InitWizard, // W11 初始化向导
  'wizard-pairing': PairingWizard, // W12 配对向导
  'wizard-restore': RestoreWizard, // W13 恢复向导
  'wizard-drill': RecoveryDrillWizard, // W14 恢复演练向导
};

/**
 * 非窗口界面（不在 WINDOW_SPECS 内）：
 * W1 托盘由 Rust 原生构建（本视图是同一份菜单清单的 UI 呈现），W15 由系统通知承载。
 * 仅在 `npm run dev` 下用 `?window=preview-*` 直接预览。
 */
export const PREVIEW_VIEWS: Readonly<Record<string, Component>> = {
  'preview-tray-menu': TrayMenu, // W1 托盘菜单
  'preview-notification-toast': NotificationToast, // W15 通知 toast
};

/** 按窗口标签取视图；未知标签返回 undefined（由 main.ts 呈现提示，不抛错）。 */
export function resolveWindowView(label: string): Component | undefined {
  return (WINDOW_VIEWS as Record<string, Component>)[label] ?? (PREVIEW_VIEWS as Record<string, Component>)[label];
}
