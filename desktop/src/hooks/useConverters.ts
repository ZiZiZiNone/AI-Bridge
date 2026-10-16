/**
 * W8 转换器清单加载（B4.1）。单一关注点：验签后的清单五态；
 * 信任链验证在 core，UI 只呈现结果。启用/停用开关 core 无命令，不造假。
 */

import { ref, shallowRef } from 'vue';
import { attemptMessage, resolveListState } from '../logic/uiState.logic';
import * as convertersService from '../services/converters.service';
import type { ConverterView } from '../types/commands';
import type { UIState } from '../types/ui';
import { useRequestGuard } from './useRequestGuard';

export function useConverters() {
  const guard = useRequestGuard();

  const converters = shallowRef<ConverterView[]>([]);
  const state = ref<UIState>('idle');
  const errorMessage = ref('');

  async function reload(): Promise<void> {
    const { isStale } = guard.start();
    const hasData = state.value === 'success';
    state.value = 'loading';
    const result = await convertersService.listConverters();
    if (isStale()) return;
    if (!result.success) {
      errorMessage.value = attemptMessage(result.error.message, guard.attempts.value);
      if (!hasData) state.value = 'error';
      return;
    }
    converters.value = result.data;
    state.value = resolveListState(result.data);
    errorMessage.value = '';
  }

  return { converters, state, errorMessage, reload };
}
