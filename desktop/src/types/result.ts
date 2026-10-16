/**
 * 命令调用结果口径（frontend/rules/error-handling.md 标准错误对象 + Result 信封）。
 *
 * Service 层整体不向上抛异常，一律返回 Result；错误归一为 StandardError，
 * UI 只消费结构化结果。错误文案由 core（F7 脱敏口径）或传输层给出，UI 不改写。
 */

/** 标准错误对象（frontend/rules/error-handling.md「标准错误对象结构」）。 */
export interface StandardError {
  code: string;
  /** 用户可读的错误信息（无技术术语、无堆栈）。 */
  message: string;
  type: 'network' | 'business' | 'client';
  retryable: boolean;
}

/** 统一结果信封：success 为 true 时必有 data，为 false 时必有 error。 */
export type Result<T> =
  | { success: true; data: T }
  | { success: false; error: StandardError };

export function ok<T>(data: T): Result<T> {
  return { success: true, data };
}

export function err(error: StandardError): Result<never> {
  return { success: false, error };
}
