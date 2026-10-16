/**
 * W5 临时会话总览与签发/吊销（C1–C5）。
 * 单一关注点：会话总览五态 + 两个面板动作；scope / TTL / 消歧在 core，
 * 本层不接触也不落任何明文（S6）。
 */

import { ref, shallowRef } from 'vue';
import { attemptMessage, resolveListState } from '../logic/uiState.logic';
import * as sessionsService from '../services/sessions.service';
import type { IssueParams, SessionSummary } from '../types/commands';
import type { UIState } from '../types/ui';
import { useRequestGuard } from './useRequestGuard';

export function useTemporarySessions() {
  const guard = useRequestGuard();

  const sessions = shallowRef<SessionSummary[]>([]);
  const state = ref<UIState>('idle');
  const errorMessage = ref('');
  /** 动作反馈（签发 / 吊销的结果文案），与整体态分开存放。 */
  const actionMessage = ref('');
  const isBusy = ref(false);

  async function reload(): Promise<void> {
    const { isStale } = guard.start();
    const hasData = state.value === 'success';
    state.value = 'loading';
    const result = await sessionsService.listTemporarySessions();
    if (isStale()) return;
    if (!result.success) {
      errorMessage.value = attemptMessage(result.error.message, guard.attempts.value);
      if (!hasData) state.value = 'error';
      return;
    }
    sessions.value = result.data.sessions;
    state.value = resolveListState(result.data.sessions);
    errorMessage.value = '';
  }

  /** 签发临时会话；成功返回 true（表单据此关闭）。 */
  async function issue(params: IssueParams): Promise<boolean> {
    isBusy.value = true;
    const result = await sessionsService.issueTemporarySession(params);
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = `签发失败：${result.error.message}`;
      return false;
    }
    actionMessage.value = '会话已签发（真会话由 core 代持，AI 端只持有临时凭证）';
    errorMessage.value = '';
    await reload();
    return true;
  }

  async function revokeAll(): Promise<void> {
    isBusy.value = true;
    const result = await sessionsService.revokeAllTemporarySessions();
    isBusy.value = false;
    if (!result.success) {
      errorMessage.value = `吊销失败：${result.error.message}`;
      return;
    }
    actionMessage.value = `已吊销 ${result.data} 个临时会话`;
    errorMessage.value = '';
    await reload();
  }

  return { sessions, state, errorMessage, actionMessage, isBusy, reload, issue, revokeAll };
}
