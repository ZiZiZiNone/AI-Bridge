//! `ab-sync` —— 配对与传输：QR 指纹配对（E3）、局域网 E2E 直传（E1/E2）、A8 离线传送包。
//!
//! 核心承诺（`docs/安全模型.md` §7、`docs/流程/桌面端流程.md` W-F4）：
//! - 配对：X25519 公钥入码（QR）+ 双方指纹比对确认；指纹不一致必须拒绝（人工比对在 UI，
//!   core 负责唯一一份指纹计算与「确认后才认设备」的门）。
//! - 直传：配对密钥（`aibridge/v1/sync-pair` 域派生）AES-256-GCM 密封 SessionEnvelope，
//!   **cookie 完整属性零丢弃**（A2 保真度）；传输信道必须注入（[`Transport`]），本 crate
//!   不含任何网络；中继与云被明确排除。
//! - A8 离线包：对端 X25519 公钥加密（`aibridge/v1/recovery-pack` 域），时间戳作为
//!   AEAD 附加认证数据（防篡改/防回拨，即「签名时间戳」的落地形态），导入时**过期拒解**
//!   且有效期在导出时设定（A8.3）；包可经任意介质搬运，内容零泄露（A8.2）。
//!
//! 明确不做：网络 IO（传输注入）、审计写入（宿主层）、设备持久化（宿主层保存
//! [`PairedDevice`] 列表与 [`SyncIdentity`] 秘密，构造时回灌）。
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

use ab_contract::SessionEnvelope;
use ab_crypto::{
    derive_domain, sha256, x25519_dh, x25519_public, CryptoError, DomainLabel, Entropy, Sealed,
    Zeroizing,
};
use serde::{Deserialize, Serialize};

const TRANSFER_AAD: &str = "aibridge/transfer/v1";
const OFFLINE_AAD: &str = "aibridge/offline/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    Crypto(CryptoError),
    EntropyFailed,
    /// 配对载荷 / 信封结构非法。
    InvalidOffer,
    Malformed,
    /// 设备不在已确认列表（含未确认的配对候选）。
    UnknownDevice,
    AlreadyPaired,
    /// A8.3：包已过期，拒解。
    Expired,
    /// 有效期必须为正。
    InvalidValidity,
    Transport,
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SyncError::Crypto(e) => return write!(f, "密码学操作失败: {e}"),
            SyncError::EntropyFailed => "随机源不可用",
            SyncError::InvalidOffer => "配对载荷非法",
            SyncError::Malformed => "数据损坏或格式非法",
            SyncError::UnknownDevice => "设备不存在（先配对并确认指纹）",
            SyncError::AlreadyPaired => "设备已配对（先解绑）",
            SyncError::Expired => "离线传送包已过期，拒绝解密",
            SyncError::InvalidValidity => "有效期必须为正",
            SyncError::Transport => "传输信道失败",
        };
        f.write_str(s)
    }
}

impl std::error::Error for SyncError {}

impl From<CryptoError> for SyncError {
    fn from(e: CryptoError) -> Self {
        SyncError::Crypto(e)
    }
}

/// 配对设备平台；边界（M7）：仅 Windows / iOS / Android。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DevicePlatform {
    Windows,
    Ios,
    Android,
}

/// QR 内嵌载荷：X25519 公钥 + 设备信息（宿主负责序列化成 QR 图案）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingOffer {
    pub device_id: String,
    pub device_name: String,
    pub platform: DevicePlatform,
    pub public_key: [u8; 32],
}

/// 已配对设备（可序列化，宿主持久化）；指纹用于「两端显示、人工比对」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    pub device_name: String,
    pub platform: DevicePlatform,
    pub public_key: [u8; 32],
    pub fingerprint: String,
    pub paired_at_secs: i64,
}

/// 本机配对身份：X25519 静态密钥（QR 里只有公钥；秘密由宿主持久化）。
pub struct SyncIdentity {
    device_id: String,
    secret: Zeroizing<[u8; 32]>,
    public_key: [u8; 32],
}

impl SyncIdentity {
    pub fn generate(device_id: &str, entropy: &dyn Entropy) -> Result<Self, SyncError> {
        check_device_id(device_id)?;
        let mut secret = Zeroizing::new([0u8; 32]);
        entropy.fill(secret.as_mut()).map_err(|_| SyncError::EntropyFailed)?;
        Self::from_secret(device_id, secret)
    }

    /// 宿主从持久化恢复身份。
    pub fn from_secret(device_id: &str, secret: Zeroizing<[u8; 32]>) -> Result<Self, SyncError> {
        check_device_id(device_id)?;
        let public_key = x25519_public(&*secret);
        Ok(Self {
            device_id: device_id.to_string(),
            secret,
            public_key,
        })
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.public_key
    }

    /// 供宿主持久化；调用方负责安全存储。
    pub fn secret_key(&self) -> Zeroizing<[u8; 32]> {
        self.secret.clone()
    }

    /// QR 载荷（公钥 + 设备信息）。
    pub fn offer(&self, device_name: &str, platform: DevicePlatform) -> PairingOffer {
        PairingOffer {
            device_id: self.device_id.clone(),
            device_name: device_name.to_string(),
            platform,
            public_key: self.public_key,
        }
    }
}

/// 直传信道（宿主实现）：只允许局域网直连 / 面对面通道，禁止中继与云。
pub trait Transport: Send + Sync {
    fn send(&self, device_id: &str, bytes: &[u8]) -> Result<(), SyncError>;
}

/// 直传载荷（E1/E2 信封密封体）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedTransfer {
    pub from_device_id: String,
    pub sealed: Sealed,
}

/// A8 离线传送包（导出即文件字节）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePack {
    pub format: String,
    /// 一次性 X25519 公钥，收件方与自己的私钥做 DH。
    pub ephemeral_public: [u8; 32],
    pub issued_at_secs: i64,
    pub expires_at_secs: i64,
    pub sealed: Sealed,
}

/// 配对与传输服务：身份 + 已确认设备列表自持（进程内存；持久化由宿主回灌）。
pub struct SyncService {
    identity: SyncIdentity,
    devices: Mutex<BTreeMap<String, PairedDevice>>,
}

impl SyncService {
    /// `known` 是宿主恢复的已确认设备列表（上次会话持久化）。
    pub fn new(identity: SyncIdentity, known: Vec<PairedDevice>) -> Self {
        let devices = known
            .into_iter()
            .map(|device| (device.device_id.clone(), device))
            .collect();
        Self {
            identity,
            devices: Mutex::new(devices),
        }
    }

    pub fn identity(&self) -> &SyncIdentity {
        &self.identity
    }

    /// 本机指纹展示值（与对端各自计算、人工比对）。
    pub fn fingerprint(&self, peer_public: &[u8; 32]) -> String {
        fingerprint(&self.identity.public_key, peer_public)
    }

    /// 配对第一步（E3）：计算指纹与设备记录，**不入列表**；UI 比对一致后调 [`confirm_pairing`]。
    pub fn pair(&self, offer: &PairingOffer, now_unix_secs: i64) -> Result<PairedDevice, SyncError> {
        if offer.device_id.trim().is_empty()
            || offer.device_name.trim().is_empty()
            || offer.device_id == self.identity.device_id
        {
            return Err(SyncError::InvalidOffer);
        }
        Ok(PairedDevice {
            device_id: offer.device_id.clone(),
            device_name: offer.device_name.clone(),
            platform: offer.platform,
            public_key: offer.public_key,
            fingerprint: self.fingerprint(&offer.public_key),
            paired_at_secs: now_unix_secs,
        })
    }

    /// 配对第二步：用户确认指纹一致后入设备列表（「确认 → 设备入列表」）。
    pub fn confirm_pairing(&self, device: &PairedDevice) -> Result<(), SyncError> {
        let mut devices = self.devices.lock().map_err(|_| SyncError::Transport)?;
        if devices.contains_key(&device.device_id) {
            return Err(SyncError::AlreadyPaired);
        }
        devices.insert(device.device_id.clone(), device.clone());
        Ok(())
    }

    /// 解绑（E3）：从设备列表移除。
    pub fn unbind(&self, device_id: &str) -> Result<PairedDevice, SyncError> {
        let mut devices = self.devices.lock().map_err(|_| SyncError::Transport)?;
        devices.remove(device_id).ok_or(SyncError::UnknownDevice)
    }

    /// 已确认设备列表。
    pub fn devices(&self) -> Result<Vec<PairedDevice>, SyncError> {
        let devices = self.devices.lock().map_err(|_| SyncError::Transport)?;
        Ok(devices.values().cloned().collect())
    }

    /// E1/E2 直传：密封信封（cookie 完整属性）并经注入信道送往已确认设备。
    pub fn transfer(
        &self,
        to_device_id: &str,
        envelope: &SessionEnvelope,
        transport: &dyn Transport,
        entropy: &dyn Entropy,
    ) -> Result<(), SyncError> {
        envelope.validate().map_err(|_| SyncError::Malformed)?;
        let peer = self.confirmed_device(to_device_id)?;
        let bytes = serde_json::to_vec(envelope).map_err(|_| SyncError::Malformed)?;
        let key = pairing_key(&self.identity.secret, &peer.public_key)?;
        let aad = transfer_aad(&self.identity.device_id, to_device_id);
        let sealed = seal_with(entropy, &key, aad.as_bytes(), &bytes)?;
        let wire = SealedTransfer {
            from_device_id: self.identity.device_id.clone(),
            sealed,
        };
        let payload = serde_json::to_vec(&wire).map_err(|_| SyncError::Malformed)?;
        transport.send(to_device_id, &payload)
    }

    /// 直传收端：按发送方设备开信并校验信封。
    pub fn receive(
        &self,
        from_device_id: &str,
        bytes: &[u8],
    ) -> Result<SessionEnvelope, SyncError> {
        let wire: SealedTransfer =
            serde_json::from_slice(bytes).map_err(|_| SyncError::Malformed)?;
        if wire.from_device_id != from_device_id {
            return Err(SyncError::Malformed);
        }
        let peer = self.confirmed_device(from_device_id)?;
        let key = pairing_key(&self.identity.secret, &peer.public_key)?;
        let aad = transfer_aad(from_device_id, &self.identity.device_id);
        let plain = open_with(&key, aad.as_bytes(), &wire.sealed)?;
        let envelope: SessionEnvelope =
            serde_json::from_slice(&plain).map_err(|_| SyncError::Malformed)?;
        envelope.validate().map_err(|_| SyncError::Malformed)?;
        Ok(envelope)
    }

    /// A8 导出（A8.1）：对端公钥加密 + AEAD 绑定时间戳；`validity_secs` 由导出方设定（A8.3）。
    pub fn pack(
        &self,
        to_device_id: &str,
        envelope: &SessionEnvelope,
        validity_secs: u64,
        now_unix_secs: i64,
        entropy: &dyn Entropy,
    ) -> Result<OfflinePack, SyncError> {
        envelope.validate().map_err(|_| SyncError::Malformed)?;
        if validity_secs == 0 {
            return Err(SyncError::InvalidValidity);
        }
        let peer = self.confirmed_device(to_device_id)?;
        let expires_at_secs = now_unix_secs
            .checked_add(validity_secs as i64)
            .ok_or(SyncError::InvalidValidity)?;
        self.pack_with_public(&peer.public_key, envelope, now_unix_secs, expires_at_secs, entropy)
    }

    /// A8 导入（A8.3）：先校验时间戳，过期拒解；解密后再校验信封。
    pub fn unpack(
        &self,
        pack: &OfflinePack,
        now_unix_secs: i64,
    ) -> Result<SessionEnvelope, SyncError> {
        if pack.format != OFFLINE_FORMAT {
            return Err(SyncError::Malformed);
        }
        if now_unix_secs >= pack.expires_at_secs {
            return Err(SyncError::Expired);
        }
        let key = ephemeral_key(&self.identity.secret, &pack.ephemeral_public)?;
        let aad = offline_aad(pack.issued_at_secs, pack.expires_at_secs);
        let plain = open_with(&key, aad.as_bytes(), &pack.sealed)?;
        let envelope: SessionEnvelope =
            serde_json::from_slice(&plain).map_err(|_| SyncError::Malformed)?;
        envelope.validate().map_err(|_| SyncError::Malformed)?;
        Ok(envelope)
    }

    fn pack_with_public(
        &self,
        recipient_public: &[u8; 32],
        envelope: &SessionEnvelope,
        issued_at_secs: i64,
        expires_at_secs: i64,
        entropy: &dyn Entropy,
    ) -> Result<OfflinePack, SyncError> {
        let mut ephemeral_secret = Zeroizing::new([0u8; 32]);
        entropy
            .fill(ephemeral_secret.as_mut())
            .map_err(|_| SyncError::EntropyFailed)?;
        let ephemeral_public = x25519_public(&*ephemeral_secret);
        let key = ephemeral_key(&ephemeral_secret, recipient_public)?;
        let aad = offline_aad(issued_at_secs, expires_at_secs);
        let bytes = serde_json::to_vec(envelope).map_err(|_| SyncError::Malformed)?;
        let sealed = seal_with(entropy, &key, aad.as_bytes(), &bytes)?;
        Ok(OfflinePack {
            format: OFFLINE_FORMAT.to_string(),
            ephemeral_public,
            issued_at_secs,
            expires_at_secs,
            sealed,
        })
    }

    fn confirmed_device(&self, device_id: &str) -> Result<PairedDevice, SyncError> {
        let devices = self.devices.lock().map_err(|_| SyncError::Transport)?;
        devices.get(device_id).cloned().ok_or(SyncError::UnknownDevice)
    }
}

const OFFLINE_FORMAT: &str = "aibridge-offline/1";

/// 短认证串（SAS）：两把公钥排序后哈希，两端独立计算必须一致；MITM 换钥即失配。
pub fn fingerprint(local_public: &[u8; 32], peer_public: &[u8; 32]) -> String {
    let (a, b) = if local_public <= peer_public {
        (local_public, peer_public)
    } else {
        (peer_public, local_public)
    };
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(a);
    buf[32..].copy_from_slice(b);
    let digest = sha256(&buf);
    let hex: String = digest[..12].iter().map(|b| format!("{b:02x}")).collect();
    hex.as_bytes()
        .chunks(4)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("-")
}

fn pairing_key(
    local_secret: &Zeroizing<[u8; 32]>,
    peer_public: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, SyncError> {
    let shared = x25519_dh(&*local_secret, peer_public)?;
    Ok(derive_domain(shared.as_ref(), None, DomainLabel::SyncPair)?)
}

fn ephemeral_key(
    local_secret: &Zeroizing<[u8; 32]>,
    peer_public: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, SyncError> {
    let shared = x25519_dh(&*local_secret, peer_public)?;
    Ok(derive_domain(
        shared.as_ref(),
        None,
        DomainLabel::RecoveryPack,
    )?)
}

fn transfer_aad(from_device_id: &str, to_device_id: &str) -> String {
    format!("{TRANSFER_AAD}|{from_device_id}|{to_device_id}")
}

fn offline_aad(issued_at_secs: i64, expires_at_secs: i64) -> String {
    format!("{OFFLINE_AAD}|{issued_at_secs}|{expires_at_secs}")
}

fn seal_with(
    entropy: &dyn Entropy,
    key: &Zeroizing<[u8; 32]>,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Sealed, SyncError> {
    Ok(ab_crypto::seal(entropy, &*key, aad, plaintext)?)
}

fn open_with(
    key: &Zeroizing<[u8; 32]>,
    aad: &[u8],
    sealed: &Sealed,
) -> Result<Zeroizing<Vec<u8>>, SyncError> {
    Ok(ab_crypto::open(&*key, aad, sealed)?)
}

fn check_device_id(device_id: &str) -> Result<(), SyncError> {
    if device_id.trim().is_empty() {
        Err(SyncError::InvalidOffer)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_contract::{Cookie, SameSite, SessionKind, Source, SourceType, Target, TargetKind};

    struct FixedEntropy(u8);
    impl Entropy for FixedEntropy {
        fn fill(&self, out: &mut [u8]) -> Result<(), CryptoError> {
            for (i, b) in out.iter_mut().enumerate() {
                *b = self.0.wrapping_add((i as u8).wrapping_mul(17));
            }
            Ok(())
        }
    }

    #[derive(Default)]
    struct CaptureTransport(Mutex<Vec<Vec<u8>>>);
    impl CaptureTransport {
        fn take(&self) -> Vec<u8> {
            self.0.lock().unwrap().pop().unwrap()
        }
    }
    impl Transport for CaptureTransport {
        fn send(&self, _device_id: &str, bytes: &[u8]) -> Result<(), SyncError> {
            self.0.lock().unwrap().push(bytes.to_vec());
            Ok(())
        }
    }

    fn envelope() -> SessionEnvelope {
        SessionEnvelope {
            schema_version: ab_contract::SCHEMA_VERSION.to_string(),
            account_ref: "acc-1".into(),
            target: Target {
                kind: TargetKind::Web,
                domain: Some("github.com".into()),
                app_id: None,
            },
            kind: SessionKind::Cookie,
            payload: {
                let mut payload = ab_contract::Payload::default();
                payload.cookies.push(Cookie {
                    name: "sid".into(),
                    value: "secret-cookie".into(),
                    domain: Some(".github.com".into()),
                    path: Some("/app".into()),
                    secure: Some(true),
                    http_only: Some(true),
                    same_site: Some(SameSite::None),
                    expires_at: Some("2026-12-01T00:00:00Z".into()),
                });
                payload
            },
            issued_at: "2026-10-09T00:00:00Z".into(),
            expires_at: Some("2026-12-01T00:00:00Z".into()),
            source: Source {
                source_type: SourceType::Extension,
                id: "chromium".into(),
            },
            scope: None,
            client_env: None,
        }
    }

    fn service(id: &str, seed: u8) -> SyncService {
        SyncService::new(
            SyncIdentity::generate(id, &FixedEntropy(seed)).unwrap(),
            vec![],
        )
    }

    fn paired() -> (SyncService, SyncService) {
        let a = service("pc-1", 1);
        let b = service("phone-1", 2);
        let dev_b = a.pair(&b.identity().offer("Phone", DevicePlatform::Ios), 100).unwrap();
        let dev_a = b.pair(&a.identity().offer("PC", DevicePlatform::Windows), 100).unwrap();
        assert_eq!(dev_a.fingerprint, dev_b.fingerprint, "双方指纹必须一致");
        a.confirm_pairing(&dev_b).unwrap();
        b.confirm_pairing(&dev_a).unwrap();
        (a, b)
    }

    #[test]
    fn fingerprint_is_order_independent_and_mitm_breaks_it() {
        let a = service("pc-1", 1);
        let b = service("phone-1", 2);
        let c = service("mallory", 3);
        assert_eq!(
            a.fingerprint(&b.identity().public_key()),
            b.fingerprint(&a.identity().public_key())
        );
        assert_ne!(
            a.fingerprint(&b.identity().public_key()),
            c.fingerprint(&b.identity().public_key()),
            "中间人换钥必须产生不同指纹"
        );
    }

    #[test]
    fn pairing_requires_confirmation_before_transfer() {
        let a = service("pc-1", 1);
        let b = service("phone-1", 2);
        let dev_b = a.pair(&b.identity().offer("Phone", DevicePlatform::Ios), 100).unwrap();
        let transport = CaptureTransport::default();

        let err = a
            .transfer("phone-1", &envelope(), &transport, &FixedEntropy(9))
            .unwrap_err();
        assert_eq!(err, SyncError::UnknownDevice, "未确认不得传输");

        a.confirm_pairing(&dev_b).unwrap();
        a.transfer("phone-1", &envelope(), &transport, &FixedEntropy(9))
            .unwrap();
        assert_eq!(
            a.confirm_pairing(&dev_b).unwrap_err(),
            SyncError::AlreadyPaired
        );

        a.unbind("phone-1").unwrap();
        let err = a
            .transfer("phone-1", &envelope(), &transport, &FixedEntropy(9))
            .unwrap_err();
        assert_eq!(err, SyncError::UnknownDevice, "解绑后不得传输");
    }

    #[test]
    fn self_pairing_rejected() {
        let a = service("pc-1", 1);
        let err = a
            .pair(&a.identity().offer("PC", DevicePlatform::Windows), 100)
            .unwrap_err();
        assert_eq!(err, SyncError::InvalidOffer);
    }

    #[test]
    fn transfer_roundtrip_preserves_full_cookie_attributes() {
        let (a, b) = paired();
        let transport = CaptureTransport::default();
        let env = envelope();
        a.transfer("phone-1", &env, &transport, &FixedEntropy(9)).unwrap();
        let got = b.receive("pc-1", &transport.take()).unwrap();
        assert_eq!(got, env, "cookie 完整属性零丢弃（A2 保真度）");
    }

    #[test]
    fn transfer_rejects_forged_or_tampered_wire() {
        let (a, b) = paired();
        let transport = CaptureTransport::default();
        a.transfer("phone-1", &envelope(), &transport, &FixedEntropy(9)).unwrap();
        let wire = transport.take();

        assert!(b.receive("evil", &wire).is_err(), "伪造发送方必须拒绝");

        let mut tampered: SealedTransfer =
            serde_json::from_slice(&wire).unwrap();
        tampered.sealed.ciphertext[0] ^= 0x01;
        let bytes = serde_json::to_vec(&tampered).unwrap();
        assert!(b.receive("pc-1", &bytes).is_err(), "篡改密文必须拒绝");
    }

    #[test]
    fn offline_pack_roundtrip() {
        let (a, b) = paired();
        let env = envelope();
        let pack = a.pack("phone-1", &env, 3600, 1_000, &FixedEntropy(9)).unwrap();
        assert_eq!(pack.expires_at_secs, 4_600);
        let got = b.unpack(&pack, 1_500).unwrap();
        assert_eq!(got, env);
    }

    #[test]
    fn offline_pack_rejects_expired() {
        let (a, b) = paired();
        let pack = a
            .pack("phone-1", &envelope(), 3600, 1_000, &FixedEntropy(9))
            .unwrap();
        assert_eq!(b.unpack(&pack, 4_600).unwrap_err(), SyncError::Expired);
    }

    #[test]
    fn offline_pack_timestamps_are_tamper_proof() {
        let (a, b) = paired();
        let mut pack = a
            .pack("phone-1", &envelope(), 3600, 1_000, &FixedEntropy(9))
            .unwrap();
        // 延长有效期必须被 AEAD 绑定拦下（「签名时间戳」语义）。
        pack.expires_at_secs = 99_999;
        assert!(matches!(b.unpack(&pack, 1_500), Err(SyncError::Crypto(_))));
    }

    #[test]
    fn offline_pack_rejects_wrong_recipient() {
        let (a, _) = paired();
        let c = service("laptop-1", 3);
        let pack = a
            .pack("phone-1", &envelope(), 3600, 1_000, &FixedEntropy(9))
            .unwrap();
        assert!(
            matches!(c.unpack(&pack, 1_500), Err(SyncError::Crypto(_))),
            "非收件设备私钥必须解不开"
        );
    }

    #[test]
    fn pack_rejects_zero_validity() {
        let (a, _) = paired();
        let err = a
            .pack("phone-1", &envelope(), 0, 1_000, &FixedEntropy(9))
            .unwrap_err();
        assert_eq!(err, SyncError::InvalidValidity);
    }

    #[test]
    fn identity_roundtrips_through_host_persistence() {
        let a = service("pc-1", 1);
        let restored = SyncIdentity::from_secret("pc-1", a.identity().secret_key()).unwrap();
        assert_eq!(restored.public_key(), a.identity().public_key());
    }

    #[test]
    fn offer_carries_public_key_and_platform() {
        let a = service("pc-1", 1);
        let offer = a.identity().offer("PC", DevicePlatform::Windows);
        assert_eq!(offer.public_key, a.identity().public_key());
        let json = serde_json::to_string(&offer).unwrap();
        assert!(json.contains("windows"), "平台序列化为小写");
    }
}
