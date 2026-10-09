/**
 * UI 整体态（frontend/rules/ui-states.md 五态）：
 * idle → loading → success/error/empty，不得跳态。
 * 整体态与操作反馈（errorMessage）是两个维度，分开存放。
 */
export type UIState = 'idle' | 'loading' | 'success' | 'error' | 'empty';
