/**
 * W4 会话矩阵加载（A2.1/A2.3）。单一关注点：矩阵数据五态；
 * 筛选/排序与健康分级在 core（ab-state），本层不判定。
 */

import { ref, shallowRef } from 'vue';
import { attemptMessage, resolveListState } from '../logic/uiState.logic';
import * as stateService from '../services/state.service';
import type { MatrixRow } from '../types/commands';
import type { UIState } from '../types/ui';
import { useRequestGuard } from './useRequestGuard';

export function useSessionMatrix() {
  const guard = useRequestGuard();

  const rows = shallowRef<MatrixRow[]>([]);
  const state = ref<UIState>('idle');
  const errorMessage = ref('');

  async function reload(): Promise<void> {
    const { isStale } = guard.start();
    const hasData = state.value === 'success';
    state.value = 'loading';
    // 空筛选 = 全量（MatrixFilters 带 serde default），排序用 core 默认（到期升序）
    const result = await stateService.querySessionMatrix({});
    if (isStale()) return;
    if (!result.success) {
      errorMessage.value = attemptMessage(result.error.message, guard.attempts.value);
      if (!hasData) state.value = 'error';
      return;
    }
    rows.value = result.data;
    state.value = resolveListState(result.data);
    errorMessage.value = '';
  }

  return { rows, state, errorMessage, reload };
}
