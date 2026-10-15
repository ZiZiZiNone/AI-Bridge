//! W9 配对面板的命令接线（E3 QR 配对 · E1/E2 扩散 · A8 离线传送包）。
//!
//! 规则归属：X25519 握手、指纹（SAS）唯一算法、「确认后才认设备」、过期拒解（A8.3）
//! 全部在 `ab-sync`；本层只搬运 + 落盘设备清单（宿主持久化职责）+ 审计馈给（fail-closed）。
//!
//! 待确认 / core 缺口（见 README）：
//! - 局域网直传信道（`Transport`）：设备地址发现与信道形态 docs/ 未定稿，宿主注入
//!   [`crate::host::UnconfiguredTransport`]（明确失败桩），单测注入记录型假实现验证 core 路径；
//! - `SessionEntry → SessionEnvelope` 归一化：ab-mcp 内部私有（`to_envelope` / `pick_session`），
//!   core 无公开 API —— 本层 [`envelope_of`] 是唯一一处机械映射（同规则不写第二份），
//!   规则定稿后应下沉 core。

use ab_audit::{AuditAction, AuditResult};
use ab_contract::{
    SessionEnvelope, SessionKind, SessionRecord, Source, SourceType, Target, SCHEMA_VERSION,
};
use ab_state::SessionSource;
use ab_sync::{OfflinePack, PairedDevice, PairingOffer, Transport};
use serde::{Deserialize, Serialize};

use super::{now_rfc3339, now_unix_secs, record, CmdResult};
use crate::host::{device_id, device_platform, HostState, UnconfiguredTransport};

/// 配对载荷（QR 内容 + 展示字段）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferView {
    /// QR 内嵌 JSON（X25519 公钥 + 设备信息；宿主负责渲染成图案）。
    pub offer_json: String,
    pub device_id: String,
    pub device_name: String,
    pub public_key_hex: String,
}

/// 会话选取（扩散 / 离线包的输入；cookie 原文不经过 UI 层）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPick {
    pub account_ref: String,
    pub target: Target,
}

/// A8.3 导入报告（过期拒解在 ab-sync 强制）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportView {
    pub account_ref: String,
    pub target: Target,
    pub kind: SessionKind,
    pub expires_at: Option<String>,
}

/// 生成配对载荷（E3：X25519 公钥入码）。
pub fn offer(services: &HostState, device_name: &str) -> CmdResult<OfferView> {
    let offer = services
        .sync
        .identity()
        .offer(device_name, device_platform());
    let offer_json = serde_json::to_string(&offer).map_err(|error| error.to_string())?;
    Ok(OfferView {
        public_key_hex: hex_lower(&offer.public_key),
        device_id: offer.device_id.clone(),
        device_name: offer.device_name.clone(),
        offer_json,
    })
}

/// 扫码后候选设备（含指纹，供双方比对；**未确认不入列表**）。
pub fn review(services: &HostState, offer_json: &str) -> CmdResult<PairedDevice> {
    let offer: PairingOffer =
        serde_json::from_str(offer_json).map_err(|error| error.to_string())?;
    services
        .sync
        .pair(&offer, now_unix_secs())
        .map_err(|error| error.to_string())
}

/// 指纹确认（E3 指纹确认屏：一致才确认）→ 认设备 + 落盘 + 审计（fail-closed）。
pub fn confirm(services: &HostState, device: PairedDevice) -> CmdResult<()> {
    services
        .sync
        .confirm_pairing(&device)
        .map_err(|error| error.to_string())?;
    services.persist_devices()?;
    record(
        services,
        AuditAction::Pair,
        AuditResult::Ok,
        None,
        None,
        &[("device", &device.device_id)],
    )
}

/// 设备列表（E3）。
pub fn devices(services: &HostState) -> CmdResult<Vec<PairedDevice>> {
    services.sync.devices().map_err(|error| error.to_string())
}

/// 解绑（E3）。
pub fn unbind(services: &HostState, device_id: &str) -> CmdResult<PairedDevice> {
    let device = services
        .sync
        .unbind(device_id)
        .map_err(|error| error.to_string())?;
    services.persist_devices()?;
    record(
        services,
        AuditAction::Pair,
        AuditResult::Ok,
        None,
        None,
        &[("device", device_id), ("op", "unbind")],
    )?;
    Ok(device)
}

/// E1/E2 扩散直传（真会话 → 信封 → 配对密钥密封 → 注入信道）。
/// 信道 = [`UnconfiguredTransport`]（待确认：局域网信道未定稿，见文件头）；
/// core 密封路径由 `transfer_with` 的单测以假信道验证。
pub fn transfer(services: &HostState, to_device_id: &str, pick: &SessionPick) -> CmdResult<()> {
    transfer_with(services, &UnconfiguredTransport, to_device_id, pick)?;
    record(
        services,
        AuditAction::Sync,
        AuditResult::Ok,
        Some(&pick.account_ref),
        pick.target.domain.as_deref(),
        &[("device", to_device_id)],
    )
}

/// 直传（信道注入）：core `ab-sync` 的唯一入口，`transfer` 与单测共用同一路径。
pub fn transfer_with<T: Transport>(
    services: &HostState,
    transport: &T,
    to_device_id: &str,
    pick: &SessionPick,
) -> CmdResult<()> {
    let envelope = envelope_of(services, pick)?;
    services
        .sync
        .transfer(to_device_id, &envelope, transport, &services.entropy)
        .map_err(|error| error.to_string())
}

/// A8.1 导出离线传送包（对端公钥加密 + 签名时间戳）。
pub fn export_pack(
    services: &HostState,
    to_device_id: &str,
    pick: &SessionPick,
    validity_secs: u64,
) -> CmdResult<Vec<u8>> {
    let envelope = envelope_of(services, pick)?;
    let pack: OfflinePack = services
        .sync
        .pack(
            to_device_id,
            &envelope,
            validity_secs,
            now_unix_secs(),
            &services.entropy,
        )
        .map_err(|error| error.to_string())?;
    record(
        services,
        AuditAction::Export,
        AuditResult::Ok,
        Some(&envelope.account_ref),
        envelope.target.domain.as_deref(),
        &[("target", "offline-pack"), ("device", to_device_id)],
    )?;
    serde_json::to_vec(&pack).map_err(|error| error.to_string())
}

/// A8.3 导入离线传送包（过期拒解在 ab-sync 强制）→ 归入本机会话清单。
pub fn import_pack(services: &HostState, pack_bytes: &[u8]) -> CmdResult<ImportView> {
    let pack: OfflinePack =
        serde_json::from_slice(pack_bytes).map_err(|error| error.to_string())?;
    let envelope: SessionEnvelope = services
        .sync
        .unpack(&pack, now_unix_secs())
        .map_err(|error| error.to_string())?;

    // 归入本机（桌面）端的会话清单：后续注入由 W-F3 流程承接（bridge 注入待接线）。
    let endpoint = ab_state::Endpoint::new(ab_state::EndpointKind::Desktop, device_id());
    let mut records = services
        .source
        .read(&endpoint)
        .map_err(|error| error.to_string())?;
    records.push(session_record_of(&envelope));
    services.source.ingest(endpoint.clone(), records);
    services
        .state
        .capture(&endpoint)
        .map_err(|error| error.to_string())?;

    record(
        services,
        AuditAction::Sync,
        AuditResult::Ok,
        Some(&envelope.account_ref),
        envelope.target.domain.as_deref(),
        &[("target", "offline-pack-import")],
    )?;
    Ok(ImportView {
        account_ref: envelope.account_ref.clone(),
        target: envelope.target.clone(),
        kind: envelope.kind,
        expires_at: envelope.expires_at.clone(),
    })
}

/// 信封归一（core 缺口的机械映射，见文件头；唯一实现处）。
fn envelope_of(services: &HostState, pick: &SessionPick) -> CmdResult<SessionEnvelope> {
    let entries = services
        .state
        .sessions_for_target(&pick.target)
        .map_err(|error| error.to_string())?;
    let matched: Vec<&ab_state::SessionEntry> = entries
        .iter()
        .filter(|entry| entry.record.account_ref == pick.account_ref)
        .collect();
    let entry = match matched.as_slice() {
        [entry] => *entry,
        [] => return Err("目标没有可用会话".into()),
        _ => return Err("同站多账号并存，请指定 account_ref".into()),
    };
    Ok(SessionEnvelope {
        schema_version: SCHEMA_VERSION.to_string(),
        account_ref: entry.record.account_ref.clone(),
        target: entry.record.target.clone(),
        kind: entry.record.kind,
        payload: entry.record.payload.clone(),
        issued_at: now_rfc3339(),
        expires_at: entry.record.expires_at.clone(),
        source: entry.record.source.clone(),
        client_env: entry.record.client_env.clone(),
        scope: None,
    })
}

/// 信封 → 状态层会话记录（字段搬运，零加工）。
fn session_record_of(envelope: &SessionEnvelope) -> SessionRecord {
    SessionRecord {
        account_ref: envelope.account_ref.clone(),
        target: envelope.target.clone(),
        kind: envelope.kind,
        payload: envelope.payload.clone(),
        expires_at: envelope.expires_at.clone(),
        source: envelope.source.clone(),
        client_env: envelope.client_env.clone(),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn pairing_offer(
    state: tauri::State<'_, HostState>,
    device_name: String,
) -> CmdResult<OfferView> {
    offer(&state, &device_name)
}

#[tauri::command]
pub fn pairing_review(
    state: tauri::State<'_, HostState>,
    offer_json: String,
) -> CmdResult<PairedDevice> {
    review(&state, &offer_json)
}

#[tauri::command]
pub fn pairing_confirm(
    state: tauri::State<'_, HostState>,
    device: PairedDevice,
) -> CmdResult<()> {
    confirm(&state, device)
}

#[tauri::command]
pub fn pairing_devices(state: tauri::State<'_, HostState>) -> CmdResult<Vec<PairedDevice>> {
    devices(&state)
}

#[tauri::command]
pub fn pairing_unbind(
    state: tauri::State<'_, HostState>,
    device_id: String,
) -> CmdResult<PairedDevice> {
    unbind(&state, &device_id)
}

#[tauri::command]
pub fn sync_transfer(
    state: tauri::State<'_, HostState>,
    to_device_id: String,
    pick: SessionPick,
) -> CmdResult<()> {
    transfer(&state, &to_device_id, &pick)
}

#[tauri::command]
pub fn sync_export_pack(
    state: tauri::State<'_, HostState>,
    to_device_id: String,
    pick: SessionPick,
    validity_secs: u64,
) -> CmdResult<Vec<u8>> {
    export_pack(&state, &to_device_id, &pick, validity_secs)
}

#[tauri::command]
pub fn sync_import_pack(
    state: tauri::State<'_, HostState>,
    pack_bytes: Vec<u8>,
) -> CmdResult<ImportView> {
    import_pack(&state, &pack_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::state::{self, EndpointDto};
    use crate::host::{HostLayout, HostState, HostSignatureVerifier};
    use ab_audit::InMemoryAuditSink;
    use ab_contract::{Cookie, Payload, SameSite, TargetKind};
    use ab_state::{Endpoint, EndpointKind, MatrixFilters};
    use ab_sync::SyncError;
    use std::sync::{Arc, Mutex};

    fn test_services_named(device_id: &str) -> HostState {
        let root = crate::host::test_root(&format!("aibridge-pair-test-{device_id}"));
        let _ = std::fs::remove_dir_all(&root);
        HostState::build_with(
            device_id,
            HostLayout { root },
            HostSignatureVerifier::new(None, None),
            Arc::new(InMemoryAuditSink::new()),
        )
    }

    /// 记录型假 Transport（core 注入 trait 假实现）。
    struct RecordingTransport {
        sent: Mutex<Vec<(String, Vec<u8>)>>,
    }
    impl Transport for RecordingTransport {
        fn send(&self, device_id: &str, bytes: &[u8]) -> Result<(), SyncError> {
            self.sent
                .lock()
                .unwrap()
                .push((device_id.to_string(), bytes.to_vec()));
            Ok(())
        }
    }

    fn seed(services: &HostState) {
        let endpoint = EndpointDto {
            kind: EndpointKind::Browser,
            id: "chromium".into(),
        };
        services.source.ingest(
            Endpoint::from(&endpoint),
            vec![SessionRecord {
                account_ref: "acc-1".into(),
                target: Target {
                    kind: TargetKind::Web,
                    domain: Some("github.com".into()),
                    app_id: None,
                },
                kind: SessionKind::Cookie,
                payload: Payload {
                    cookies: vec![Cookie {
                        name: "sid".into(),
                        value: "cookie-value-secret".into(),
                        domain: Some("github.com".into()),
                        path: Some("/".into()),
                        secure: Some(true),
                        http_only: Some(true),
                        same_site: Some(SameSite::Lax),
                        expires_at: Some("2030-01-01T00:00:00Z".into()),
                    }],
                    ..Payload::default()
                },
                expires_at: None,
                source: Source {
                    source_type: SourceType::Extension,
                    id: "chromium".into(),
                },
                client_env: None,
            }],
        );
        state::capture(&services, &endpoint).expect("捕获失败");
    }

    fn pick() -> SessionPick {
        SessionPick {
            account_ref: "acc-1".into(),
            target: Target {
                kind: TargetKind::Web,
                domain: Some("github.com".into()),
                app_id: None,
            },
        }
    }

    /// 互配对的两个宿主（本机 ⇄ 对端），返回 (本机, 对端, 对端设备, 本机设备)。
    fn mutual_pairing() -> (HostState, HostState, PairedDevice, PairedDevice) {
        let local = test_services_named("pc-local");
        let peer = test_services_named("pc-peer");
        let peer_device =
            review(&local, &offer(&peer, "对端手机").expect("载荷失败").offer_json)
                .expect("候选失败");
        confirm(&local, peer_device.clone()).expect("确认失败");
        let local_device =
            review(&peer, &offer(&local, "本机").expect("载荷失败").offer_json)
                .expect("候选失败");
        confirm(&peer, local_device.clone()).expect("确认失败");
        (local, peer, peer_device, local_device)
    }

    #[test]
    fn pairing_flow_needs_confirmation_before_device_is_known() {
        let local = test_services_named("pc-local");
        let peer = test_services_named("pc-peer");

        let view = offer(&peer, "对端手机").expect("载荷失败");
        assert_eq!(view.public_key_hex.len(), 64);
        let offered: PairingOffer = serde_json::from_str(&view.offer_json).expect("载荷非法");

        // review 只产生候选（含指纹），未确认不入列表。
        let device = review(&local, &view.offer_json).expect("候选失败");
        assert!(!device.fingerprint.is_empty());
        assert!(devices(&local).expect("列表失败").is_empty());

        confirm(&local, device.clone()).expect("确认失败");
        let listed = devices(&local).expect("列表失败");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].fingerprint, device.fingerprint);
        assert_eq!(listed[0].public_key, offered.public_key);

        // 解绑。
        unbind(&local, &device.device_id).expect("解绑失败");
        assert!(devices(&local).expect("列表失败").is_empty());
    }

    #[test]
    fn transfer_seals_through_injected_transport() {
        let (local, peer, peer_device, _local_device) = mutual_pairing();
        seed(&local);

        // E1/E2 直传：注入记录型 Transport（core 注入 trait 假实现），验证 core 密封路径。
        let transport = RecordingTransport {
            sent: Mutex::new(Vec::new()),
        };
        transfer_with(&local, &transport, &peer_device.device_id, &pick()).expect("直传失败");
        let sent = transport.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, peer_device.device_id);

        // 对端 receive 开信：信封零丢弃（A2 保真度）。
        let envelope = peer
            .sync
            .receive(&local.sync.identity().device_id().to_string(), &sent[0].1)
            .expect("收端开信失败");
        assert_eq!(envelope.account_ref, "acc-1");
        assert_eq!(envelope.payload.cookies[0].value, "cookie-value-secret");
    }

    #[test]
    fn offline_pack_roundtrip_keeps_cookie_attributes() {
        let (local, peer, peer_device, _local_device) = mutual_pairing();
        seed(&local);

        // A8：本机导出（对端公钥加密）→ 对端解包：信封零丢弃（A2 保真度）。
        let pack_bytes =
            export_pack(&local, &peer_device.device_id, &pick(), 3600).expect("导出失败");
        let pack: OfflinePack = serde_json::from_slice(&pack_bytes).expect("包非法");
        let received = peer
            .sync
            .unpack(&pack, now_unix_secs())
            .expect("解包失败");
        assert_eq!(received.account_ref, "acc-1");
        assert_eq!(received.payload.cookies[0].value, "cookie-value-secret");
        assert_eq!(received.payload.cookies[0].same_site, Some(SameSite::Lax));
        assert_eq!(received.payload.cookies[0].http_only, Some(true));

        // A8.3 过期拒解：有效期已过的包必须拒解。
        assert!(matches!(
            peer.sync.unpack(&pack, now_unix_secs() + 7200),
            Err(SyncError::Expired)
        ));
    }

    #[test]
    fn import_pack_lands_in_local_session_matrix() {
        let (local, peer, _peer_device, local_device) = mutual_pairing();
        seed(&peer);

        // 对端把会话打包给本机 → 本机导入（A8.3 校验在 ab-sync）。
        let pack_bytes =
            export_pack(&peer, &local_device.device_id, &pick(), 3600).expect("导出失败");
        let view = import_pack(&local, &pack_bytes).expect("导入失败");
        assert_eq!(view.account_ref, "acc-1");

        let rows = local
            .state
            .matrix(&MatrixFilters::default())
            .expect("矩阵失败");
        assert!(rows.iter().any(|row| row.account_ref == "acc-1"));
    }
}
