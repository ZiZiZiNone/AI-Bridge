/**
 * W9 配对（E3）。单一关注点：本机配对载荷生成与「对端配对码 → 候选 → 确认」编排；
 * X25519 握手、指纹唯一算法、「确认后才认设备」在 core（ab-sync）。
 */

import { ref, shallowRef } from 'vue';
import { attemptMessage } from '../logic/uiState.logic';
import * as pairingService from '../services/pairing.service';
import type { OfferView } from '../types/commands';
import type { UIState } from '../types/ui';
import { useRequestGuard } from './useRequestGuard';

export function usePairing() {
  const guard = useRequestGuard();

  const offer = shallowRef<OfferView | null>(null);
  const state = ref<UIState>('idle');
  const errorMessage = ref('');
  /** 动作反馈（确认 / 拒绝的结果文案），与整体态分开存放。 */
  const actionMessage = ref('');
  /** 对端指纹（review 产生候选时更新，供比对屏展示）。 */
  const peerFingerprint = ref('');
  const isBusy = ref(false);

  async function reload(): Promise<void> {
    const { isStale } = guard.start();
    state.value = 'loading';
    const result = await pairingService.createPairingOffer('本机');
    if (isStale()) return;
    if (!result.success) {
      errorMessage.value = attemptMessage(result.error.message, guard.attempts.value);
      state.value = 'error';
      return;
    }
    offer.value = result.data;
    state.value = 'success';
    errorMessage.value = '';
  }

  /**
   * 确认配对：对端配对码 → 候选设备（含指纹）→ 确认认设备。
   * 指纹比对是用户职责（两端屏幕核对），本层不做判断。
   */
  async function confirm(offerJson: string): Promise<void> {
    isBusy.value = true;
    const reviewed = await pairingService.reviewPairingOffer(offerJson);
    if (!reviewed.success) {
      isBusy.value = false;
      errorMessage.value = `确认配对失败：${reviewed.error.message}`;
      return;
    }
    peerFingerprint.value = reviewed.data.fingerprint;
    const confirmed = await pairingService.confirmPairing(reviewed.data);
    isBusy.value = false;
    if (!confirmed.success) {
      errorMessage.value = `确认配对失败：${confirmed.error.message}`;
      return;
    }
    actionMessage.value = `已配对：${reviewed.data.device_name}（指纹 ${reviewed.data.fingerprint}）`;
    errorMessage.value = '';
  }

  /** 拒绝：仅清除待确认意图，不产生任何 core 写入。 */
  function reject(): void {
    peerFingerprint.value = '';
    actionMessage.value = '已拒绝本次配对，未写入任何设备';
  }

  return { offer, state, errorMessage, actionMessage, peerFingerprint, isBusy, reload, confirm, reject };
}
