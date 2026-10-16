/**
 * W6 审计流加载（C3.1）。单一关注点：审计记录的五态与筛选查询；
 * 筛选语义在 core，记录只读呈现（UI 不修改历史）。
 */

import { ref, shallowRef } from 'vue';
import { attemptMessage, resolveListState } from '../logic/uiState.logic';
import * as auditService from '../services/audit.service';
import type { AuditEvent, AuditFilter } from '../types/commands';
import type { UIState } from '../types/ui';
import { useRequestGuard } from './useRequestGuard';

export function useAuditTrail() {
  const guard = useRequestGuard();

  const events = shallowRef<AuditEvent[]>([]);
  const state = ref<UIState>('idle');
  const errorMessage = ref('');
  /** 最近一次筛选（重试沿用同一条件，不缓存参数副本以外的状态）。 */
  let lastFilter: AuditFilter = {};

  async function query(filter: AuditFilter): Promise<void> {
    lastFilter = { ...filter };
    const { isStale } = guard.start();
    const hasData = state.value === 'success';
    state.value = 'loading';
    const result = await auditService.queryAuditTrail(filter);
    if (isStale()) return;
    if (!result.success) {
      errorMessage.value = attemptMessage(result.error.message, guard.attempts.value);
      if (!hasData) state.value = 'error';
      return;
    }
    events.value = result.data;
    state.value = resolveListState(result.data);
    errorMessage.value = '';
  }

  /** 按最近一次筛选重载（async-operations.md：重试复用同一加载入口）。 */
  async function reload(): Promise<void> {
    await query(lastFilter);
  }

  return { events, state, errorMessage, query, reload };
}
