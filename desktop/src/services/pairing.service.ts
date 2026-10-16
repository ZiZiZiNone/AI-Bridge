/**
 * W9 配对命令（commands/pairing.rs ← ab-sync）。X25519 握手、指纹唯一算法、
 * 「确认后才认设备」在 core；本层只搬运。QR 图案渲染由宿主负责（前端无 QR 库，
 * 本层返回 offer_json 文本）。
 */

import type { OfferView, PairedDevice } from '../types/commands';
import type { Result } from '../types/result';
import { invokeCommand } from './tauriClient';

export function createPairingOffer(deviceName: string): Promise<Result<OfferView>> {
  // 顶层参数名经 Tauri 的 camelCase → snake_case 转换（deviceName → device_name）
  return invokeCommand<OfferView>('pairing_offer', { deviceName });
}

export function reviewPairingOffer(offerJson: string): Promise<Result<PairedDevice>> {
  return invokeCommand<PairedDevice>('pairing_review', { offerJson });
}

export function confirmPairing(device: PairedDevice): Promise<Result<null>> {
  return invokeCommand<null>('pairing_confirm', { device });
}
