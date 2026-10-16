/**
 * W3 条目列表加载与新增（A1.1）。
 * 单一关注点：条目集合的五态与行级新增；字段校验在 core，UI 不判断。
 */

import { ref, shallowRef } from 'vue';
import { attemptMessage, resolveListState } from '../logic/uiState.logic';
import * as vaultService from '../services/vault.service';
import type { EntryInput, EntryView } from '../types/commands';
import type { UIState } from '../types/ui';
import { useRequestGuard } from './useRequestGuard';

export function useVaultEntries() {
  const guard = useRequestGuard();

  const entries = shallowRef<EntryView[]>([]);
  const state = ref<UIState>('idle');
  const errorMessage = ref('');
  /** 新增进行中，用于禁用表单提交防重复。 */
  const isBusy = ref(false);

  async function reload(): Promise<void> {
    const { isStale } = guard.start();
    const hasData = state.value === 'success';
    state.value = 'loading';
    const result = await vaultService.listVaultEntries();
    if (isStale()) return;
    if (!result.success) {
      // 已有数据时失败保留内容，错误只经 errorMessage 呈现（ui-states.md）
      errorMessage.value = attemptMessage(result.error.message, guard.attempts.value);
      if (!hasData) state.value = 'error';
      return;
    }
    entries.value = result.data;
    state.value = resolveListState(result.data);
    errorMessage.value = '';
  }

  /** 新增条目；成功后刷新列表并返回 true（表单据此关闭、保留用户输入与否）。 */
  async function add(input: EntryInput): Promise<boolean> {
    isBusy.value = true;
    const result = await vaultService.addVaultEntry(input);
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = `新增失败：${result.error.message}`;
      return false;
    }
    await reload();
    return true;
  }

  function reset(): void {
    guard.invalidateAll();
    entries.value = [];
    state.value = 'idle';
    errorMessage.value = '';
  }

  return { entries, state, errorMessage, isBusy, reload, add, reset };
}
