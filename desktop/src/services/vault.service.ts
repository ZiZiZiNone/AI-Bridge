/**
 * W3 库管理命令（commands/library.rs）。业务规则（KDBX 容器、Argon2id、审计
 * fail-closed）在 core（ab-vault），本层只描述命令与类型。
 */

import type { EntryInput, EntryView, InitView } from '../types/commands';
import type { Result } from '../types/result';
import { invokeCommand } from './tauriClient';

export function isVaultUnlocked(): Promise<Result<boolean>> {
  return invokeCommand<boolean>('vault_is_unlocked');
}

export function createVault(name: string, password: string): Promise<Result<InitView>> {
  return invokeCommand<InitView>('vault_create', { name, password });
}

export function unlockVault(password: string): Promise<Result<null>> {
  return invokeCommand<null>('vault_unlock', { password });
}

export function lockVault(): Promise<Result<null>> {
  return invokeCommand<null>('vault_lock');
}

export function listVaultEntries(): Promise<Result<EntryView[]>> {
  return invokeCommand<EntryView[]>('vault_list_entries');
}

export function addVaultEntry(input: EntryInput): Promise<Result<EntryView>> {
  // 参数名 input 嵌套一层；嵌套字段按 serde 字段名（snake_case）原样传递
  return invokeCommand<EntryView>('vault_add_entry', { input });
}
