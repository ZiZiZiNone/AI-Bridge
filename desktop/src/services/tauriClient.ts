/**
 * Tauri 命令统一出口（frontend/rules/api.md + async-operations.md）：
 * 整个前端唯一调用 `invoke` 的地方，try-catch 与错误归一只此一份。
 *
 * - 后端命令签名见 `src-tauri/src/commands/*.rs` 的 `#[tauri::command]` 包装，
 *   错误是 `CmdResult<T> = Result<T, String>` 的 Err → Tauri 以 promise 拒绝透出字符串；
 * - 传输层失败（未运行在 Tauri 窗口内等）→ TypeError，归一为 network 类错误；
 * - 不做自动重试：非幂等写操作（建库 / 新增条目 / 签发会话等）禁止自动重试
 *   （async-operations.md 幂等性前置判断），失败交用户手动触发；
 * - invoke 走本机 IPC，无 AbortSignal 通道，故不做超时与取消（不造假 guard）。
 */

import { invoke } from '@tauri-apps/api/core';
import type { Result, StandardError } from '../types/result';
import { err, ok } from '../types/result';

/** IPC 传输层错误（未运行在 Tauri 宿主内 / 命令名不存在）。 */
const IPC_UNAVAILABLE: StandardError = {
  code: 'IPC_UNAVAILABLE',
  message: '无法连接桌面宿主：请确认本页面运行在 AI-Bridge 窗口内',
  type: 'network',
  retryable: true,
};

/** 命令业务错误（core 侧 Display 已按 F7 脱敏，直接透出）。 */
function toBusinessError(message: string): StandardError {
  return {
    code: 'COMMAND_ERROR',
    message,
    type: 'business',
    retryable: false,
  };
}

/**
 * 调用一个 tauri command。参数为顶层命令入参（Tauri 会做 camelCase → snake_case
 * 参数名转换；嵌套对象字段按 serde 字段名原样传递，见各 service）。
 */
export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<Result<T>> {
  try {
    const data = (await invoke(command, args)) as T;
    return ok(data);
  } catch (error) {
    // 业务错误：后端透出的用户可读字符串
    if (typeof error === 'string' && error !== '') {
      return err(toBusinessError(error));
    }
    if (error instanceof Error && error.message !== '') {
      // Tauri v2 对非字符串错误会包一层 Error；文案仍以后端为准
      return err(toBusinessError(error.message));
    }
    // 传输层失败（如裸浏览器里 __TAURI_INTERNALS__ 缺失）
    return err(IPC_UNAVAILABLE);
  }
}
