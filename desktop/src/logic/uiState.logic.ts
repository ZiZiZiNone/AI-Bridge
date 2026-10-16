/**
 * UI 状态判定（框架无关，frontend/rules/ui-states.md 五态口径）。
 *
 * 规则归属：success / empty 由数据形状判定，属业务规则 → 本层一份；
 * Hook 只调用结果赋值，不重复写条件表达式。
 */

import type { UIState } from '../types/ui';

/** 列表加载后的整体态：有数据 success，无数据 empty。 */
export function resolveListState<T>(list: T[]): UIState {
  return list.length > 0 ? 'success' : 'empty';
}

/**
 * 重试计数文案（frontend/rules/async-operations.md「手动重试：显示重试次数」）。
 * 首次失败不加后缀；重试后仍失败标注第几次，让用户（与验收断言）能区分两次尝试。
 */
export function attemptMessage(message: string, attempts: number): string {
  return attempts > 1 ? `${message}（第 ${attempts} 次尝试）` : message;
}
