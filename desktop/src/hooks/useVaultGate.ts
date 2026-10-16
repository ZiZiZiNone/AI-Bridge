/**
 * W3 库就绪门（S1 解锁 / 建库 / 锁定）。
 * 单一关注点：维护「库是否就绪」与门控操作；口令校验、恢复密钥生成在 core。
 */

import { ref } from 'vue';
import { attemptMessage } from '../logic/uiState.logic';
import * as vaultService from '../services/vault.service';
import type { UIState } from '../types/ui';
import { useRequestGuard } from './useRequestGuard';

export function useVaultGate() {
  const guard = useRequestGuard();

  const isUnlocked = ref(false);
  const state = ref<UIState>('idle');
  const errorMessage = ref('');
  /** 建库成功后的恢复密钥展示形态（S3 只此一次；确认知悉后清空）。 */
  const recoveryDisplay = ref('');
  /** 门操作进行中（解锁 / 建库 / 锁定），用于禁用提交防重复。 */
  const isBusy = ref(false);

  async function check(): Promise<void> {
    const { isStale } = guard.start();
    state.value = 'loading';
    const result = await vaultService.isVaultUnlocked();
    if (isStale()) return;
    if (!result.success) {
      errorMessage.value = attemptMessage(result.error.message, guard.attempts.value);
      state.value = 'error';
      return;
    }
    isUnlocked.value = result.data;
    state.value = 'idle';
    errorMessage.value = '';
  }

  /** 解锁；成功返回 true（调用方据此加载条目）。 */
  async function unlock(password: string): Promise<boolean> {
    isBusy.value = true;
    const result = await vaultService.unlockVault(password);
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = `解锁失败：${result.error.message}`;
      return false;
    }
    isUnlocked.value = true;
    errorMessage.value = '';
    return true;
  }

  /** 建库；成功返回 true 并持有恢复密钥展示形态（S3 当场展示）。 */
  async function create(name: string, password: string): Promise<boolean> {
    isBusy.value = true;
    const result = await vaultService.createVault(name, password);
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = `建库失败：${result.error.message}`;
      return false;
    }
    isUnlocked.value = true;
    recoveryDisplay.value = result.data.recovery_display;
    errorMessage.value = '';
    return true;
  }

  /** 锁定（明文随 VaultSession Drop 清零，规则在 core）。 */
  async function lock(): Promise<boolean> {
    isBusy.value = true;
    const result = await vaultService.lockVault();
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = result.error.message;
      return false;
    }
    isUnlocked.value = false;
    recoveryDisplay.value = '';
    errorMessage.value = '';
    return true;
  }

  /** 恢复密钥已离线保存（S3 知悉确认），清掉展示形态。 */
  function acknowledgeRecovery(): void {
    recoveryDisplay.value = '';
  }

  return {
    isUnlocked,
    state,
    errorMessage,
    recoveryDisplay,
    isBusy,
    check,
    unlock,
    create,
    lock,
    acknowledgeRecovery,
  };
}
