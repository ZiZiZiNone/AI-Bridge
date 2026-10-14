//! 宿主装配（薄入口的下半部）：core 服务 + 系统能力实现的组装点。
//!
//! 分层（docs/架构与契约.md）：本文件只做**组装与存储实现**，不含业务规则 ——
//! 规则一律在 core（ab-*）；core 缺 API 处一律留注释并在 desktop/README.md「core 缺口」
//! 报告，不在本层补第二份规则。
//!
//! 依赖注入：所有 core 服务构造时注入实现（`CngEntropy` / `DpapiVaultStore` /
//! `HostSessionSource` / `FileAuditSink`），无全局单例、无服务定位器。
//! 不依赖 `tauri`：MCP stdio 入口（`commands::mcp::run_stdio_entry`）与单测共用本装配。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ab_audit::{AuditError, AuditEvent, AuditSink, InMemoryAuditSink};
use ab_broker::BrokerService;
use ab_state::{
    HealthPolicy, InMemoryStateStore, SessionSource, StateError, StateService, SystemClock,
};
use ab_sync::{DevicePlatform, PairedDevice, SyncIdentity, SyncService};
use ab_vault::{RecoveryKey, VaultSession};
use ab_converter::{
    ConverterHost, SandboxError, SandboxRequest, SandboxRunner, SignatureVerifier, VerifyError,
};
use ab_contract::{ConverterManifest, StandardAccess};
use ab_crypto::{ed25519_verify, sha256};
use serde::{Deserialize, Serialize};

use crate::native::dpapi::{self, CngEntropy, DpapiVaultStore};

/// 审计「谁」：桌面宿主发起的动作统一身份（C3.1 谁/何时/…）。
pub const HOST_WHO: &str = "desktop";

/// 托管目录布局（A6.5 单一备份目录；可改的设置面属 W10，未接线）。
#[derive(Debug, Clone)]
pub struct HostPaths {
    pub root: PathBuf,
}

impl HostPaths {
    /// `%APPDATA%\AI-Bridge`（不可用时退到 `std::env::temp_dir()`，不 panic）。
    pub fn default_root() -> PathBuf {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("AI-Bridge")
    }

    /// 活动库文件（DPAPI 外壳，S6 明文零落盘）。
    pub fn vault_file(&self) -> PathBuf {
        self.root.join("vault.aibvault")
    }

    /// 恢复密钥（S3）的 DPAPI 封装文件（只展示一次的展示形态由初始化向导输出）。
    pub fn recovery_key_file(&self) -> PathBuf {
        self.root.join("recovery.key")
    }

    /// 托管备份目录（A6.5）。
    pub fn backup_dir(&self) -> PathBuf {
        self.root.join("backup")
    }

    /// 追加式审计日志（JSONL）。
    pub fn audit_log(&self) -> PathBuf {
        self.root.join("audit.jsonl")
    }

    /// 配对身份秘密（DPAPI 封装）。
    pub fn sync_identity_file(&self) -> PathBuf {
        self.root.join("sync-identity.key")
    }

    /// 已确认设备清单（公开信息：公钥 + 指纹，不含秘密）。
    pub fn devices_file(&self) -> PathBuf {
        self.root.join("devices.json")
    }

    /// 转换器托管目录（显式安装的目标地）。
    pub fn converters_dir(&self) -> PathBuf {
        self.root.join("converters")
    }
}

/// 追加式审计出口：JSONL 落盘 + 内存索引。
///
/// 查询语义不重写（「同一规则不写第二份」）：`query` / `count` 全部委托 core 的
/// `InMemoryAuditSink`（其筛选规则是唯一事实源）；本实现只补「追加落盘」这一宿主职责，
/// 且先落盘后入索引 —— 落盘失败即 `Err`，调用方敏感操作按 fail-closed 中止。
pub struct FileAuditSink {
    path: PathBuf,
    inner: InMemoryAuditSink,
}

impl FileAuditSink {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            inner: InMemoryAuditSink::new(),
        }
    }

    /// 启动回灌：把已有日志按追加顺序装回内存索引（损坏行整体跳过，不猜内容）。
    pub fn load(&self) -> Result<(), AuditError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(AuditError::Unavailable),
        };
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            if let Ok(event) = serde_json::from_str::<AuditEvent>(line) {
                self.inner.record_or_fail(event)?;
            }
        }
        Ok(())
    }
}

impl AuditSink for FileAuditSink {
    fn record_or_fail(&self, event: AuditEvent) -> Result<(), AuditError> {
        event.validate()?;
        let line = serde_json::to_string(&event).map_err(|_| AuditError::Unavailable)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| AuditError::Unavailable)?;
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|_| AuditError::Unavailable)?;
        writeln!(file, "{line}").map_err(|_| AuditError::Unavailable)?;
        self.inner.record_or_fail(event)
    }

    fn query(&self, filter: &ab_audit::AuditFilter) -> Result<Vec<AuditEvent>, AuditError> {
        self.inner.query(filter)
    }

    fn count(&self) -> Result<usize, AuditError> {
        self.inner.count()
    }
}

/// 端会话读取（ab-state `SessionSource` 的宿主实现）：bridge / 扩展捕获的会话写入口。
///
/// B2 数据流：扩展 `cookie_io` → bridge（协议翻译）→ [`HostSessionSource::ingest`] →
/// ab-state。bridge 处理函数的接线待 extensions/ 的协议词汇定稿（并行任务），见 README。
/// `Clone` 为句柄语义（与 core `InMemoryStateStore` 同口径）：克隆共享同一份会话缓存。
#[derive(Clone, Default)]
pub struct HostSessionSource {
    sessions: Arc<Mutex<Vec<(ab_state::Endpoint, Vec<ab_contract::SessionRecord>)>>>,
}

impl HostSessionSource {
    pub fn new() -> Self {
        Self::default()
    }

    /// bridge 入口：整体替换某个端的会话集合（cookie 属性零丢弃由扩展侧保证）。
    /// 当前消费方：接线单测；bridge 处理函数接线待 extensions 协议定稿。
    #[allow(dead_code)]
    pub fn ingest(&self, endpoint: ab_state::Endpoint, records: Vec<ab_contract::SessionRecord>) {
        let mut sessions = self.sessions.lock().expect("会话缓存互斥锁中毒");
        sessions.retain(|(existing, _)| existing != &endpoint);
        sessions.push((endpoint, records));
    }
}

impl SessionSource for HostSessionSource {
    fn read(&self, endpoint: &ab_state::Endpoint) -> Result<Vec<ab_contract::SessionRecord>, StateError> {
        let sessions = self.sessions.lock().map_err(|_| StateError::Store("会话缓存互斥锁中毒".into()))?;
        Ok(sessions
            .iter()
            .find(|(existing, _)| existing == endpoint)
            .map(|(_, records)| records.clone())
            .unwrap_or_default())
    }
}

/// 官方发布公钥（转换器信任链的信任根）。
///
/// 待确认：官方 ed25519 发布公钥的字节形态在仓库内无事实源
/// （tools/README.md「未验证项」：外部签名工具与密钥格式待定）。
/// 因此信任根由宿主显式配置；未配置时**拒绝一切安装**（无后门开关）。
pub struct OfficialVerifier {
    trusted_signer: Option<String>,
    public_key: Option<[u8; 32]>,
}

impl OfficialVerifier {
    /// 信任根注入；`None` 即拒载一切（fail-closed）。
    pub fn new(trusted_signer: Option<String>, public_key: Option<[u8; 32]>) -> Self {
        Self {
            trusted_signer,
            public_key,
        }
    }

    /// 从宿主环境取信任根（待确认：发布密钥的正式供给方式未定稿，见 tools/README.md；
    /// 本口径为 `AIBRIDGE_OFFICIAL_SIGNER` + `AIBRIDGE_OFFICIAL_PUBKEY_HEX`（64 hex））。
    /// 未配置即 `None` —— 安装一律拒绝，无后门开关。
    pub fn from_env() -> Self {
        let signer = std::env::var("AIBRIDGE_OFFICIAL_SIGNER").ok();
        let public_key = std::env::var("AIBRIDGE_OFFICIAL_PUBKEY_HEX")
            .ok()
            .and_then(|hex| decode_hex_32(&hex));
        Self::new(signer, public_key)
    }
}

impl SignatureVerifier for OfficialVerifier {
    /// 实现义务（core `SignatureVerifier` 文档）：
    /// ① 签名覆盖清单全部字段（含 `module_sha256`）且非占位；
    /// ② `sha256(module)` 与 `manifest.module_sha256` 相符。
    fn verify(&self, manifest: &ConverterManifest, module: &[u8]) -> Result<(), VerifyError> {
        let (Some(signer), Some(public_key)) = (&self.trusted_signer, &self.public_key) else {
            return Err(VerifyError::SignerNotAllowed);
        };
        if &manifest.signature.signer != signer {
            return Err(VerifyError::SignerNotAllowed);
        }
        if manifest.signature.algorithm != "ed25519" {
            return Err(VerifyError::UnsupportedAlgorithm);
        }

        // ② 模块字节一致性：验签的就是执行的（防「换模块保清单」）。
        let digest = hex_lower(&sha256(module));
        if digest != manifest.module_sha256 {
            return Err(VerifyError::BadSignature);
        }

        // ① 覆盖范围 =「manifest 规范化字节 + module_sha256」：
        //    字节构造与 tools/sign-converter.py 的 canonical_manifest_bytes() 逐字对齐
        //    （该脚本是待签名字节构造的唯一事实源；对齐不对称即互不认，见 README 待确认）。
        let signature = decode_hex_64(&manifest.signature.value).ok_or(VerifyError::Malformed)?;
        let signed = canonical_signed_bytes(manifest, &digest);
        ed25519_verify(public_key, &signed, &signature).map_err(|_| VerifyError::BadSignature)
    }
}

/// 规范化待签名字节（对齐 tools/sign-converter.py）：
/// manifest 去掉 `signature`、`module_sha256` 置为本次计算值 → 键排序 + 紧凑 JSON
/// （serde_json::Map 为 BTreeMap，键序与 Python `sort_keys=True` 对 ASCII 键一致）
/// → 拼接 module_sha256 的 ASCII 字节。
fn canonical_signed_bytes(manifest: &ConverterManifest, module_sha256: &str) -> Vec<u8> {
    let mut value = match serde_json::to_value(manifest) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    value.remove("signature");
    value.insert("module_sha256".into(), module_sha256.into());
    let mut signed = serde_json::to_string(&serde_json::Value::Object(value))
        .unwrap_or_default()
        .into_bytes();
    signed.extend_from_slice(module_sha256.as_bytes());
    signed
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex_64(text: &str) -> Option<[u8; 64]> {
    if text.len() != 128 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 64];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(2 * index..2 * index + 2)?, 16).ok()?;
    }
    Some(out)
}

fn decode_hex_32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(2 * index..2 * index + 2)?, 16).ok()?;
    }
    Some(out)
}

/// WASM 沙盒执行 —— **未实现**（待确认：进程级沙盒形态属 ab-converter 实现期首验项，
/// 见 docs/架构与契约.md §8）。构造 `ConverterHost` 必须给出本注入实现；
/// `run_extract` 一律以明确错误失败，不静默放行。
pub struct PendingSandbox;

impl SandboxRunner for PendingSandbox {
    fn run(&self, _request: &SandboxRequest<'_>) -> Result<StandardAccess, SandboxError> {
        Err(SandboxError::failed("WASM 沙盒执行待实现"))
    }
}

/// 局域网直传信道 —— **未实现**（待确认：设备地址发现与信道形态在 docs/ 中未定稿）。
/// E1/E2 直传命令注入本实现后一律明确失败；单测注入记录型假实现验证 core 路径。
pub struct PendingTransport;

impl ab_sync::Transport for PendingTransport {
    fn send(&self, _device_id: &str, _bytes: &[u8]) -> Result<(), ab_sync::SyncError> {
        Err(ab_sync::SyncError::Transport)
    }
}

/// 备份元数据（A6.7 最后验证时间；A6.6 滚动保留策略需 core 支持，未实现，见 README）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_backup_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_at: Option<String>,
}

/// 桌面宿主的全部注入服务（构造即组装；无全局单例）。
pub struct HostServices {
    pub paths: HostPaths,
    pub entropy: CngEntropy,
    /// 审计出口（trait 对象：生产为 [`FileAuditSink`]，单测注入假实现验证 fail-closed）。
    pub audit: Arc<dyn AuditSink>,
    /// 活动库会话（锁定 = None；S2 就绪态由宿主时序承载，自动锁策略待 core，见 README）。
    pub vault: Mutex<Option<VaultSession>>,
    /// 恢复密钥（S3）：DPAPI 文件回灌；展示一次是 UI 纪律。
    pub recovery: Mutex<Option<RecoveryKey>>,
    pub broker: Arc<BrokerService>,
    pub state: Arc<StateService<InMemoryStateStore, SystemClock, HostSessionSource>>,
    /// 会话读取注入面（Clone 为句柄语义，与 `state` 共享同一份缓存）。
    pub source: HostSessionSource,
    pub sync: SyncService,
    pub converter_host: ConverterHost<OfficialVerifier, PendingSandbox>,
    /// MCP HTTP（127.0.0.1）启动状态；每进程一个监听。
    pub mcp_http: Mutex<Option<u16>>,
}

impl HostServices {
    /// 组装：托管目录 + core 服务 + 系统能力实现。转换器信任根取 [`OfficialVerifier::from_env`]，
    /// 审计出口为 [`FileAuditSink`]（JSONL 追加落盘）。
    pub fn build(paths: HostPaths) -> Self {
        let audit = FileAuditSink::new(paths.audit_log());
        let _ = audit.load();
        Self::build_with(
            &device_id(),
            paths,
            OfficialVerifier::from_env(),
            Arc::new(audit),
        )
    }

    /// 组装（设备 id / 信任根 / 审计出口显式注入；单测用假审计出口验证 fail-closed）。
    pub fn build_with(
        device_id: &str,
        paths: HostPaths,
        verifier: OfficialVerifier,
        audit: Arc<dyn AuditSink>,
    ) -> Self {
        // broker 句柄域密钥：每进程随机主键（ab-broker 只用它派生查表键，不落表；
        // 临时会话随进程存续，重启即清）。与解锁后 MK 的绑定待 core 提供生命周期 API。
        let entropy = CngEntropy;
        let mut master = [0u8; 32];
        let _ = entropy.fill(&mut master);
        let broker = Arc::new(
            BrokerService::new(audit.clone(), &master).expect("broker 构造不应失败"),
        );

        let source = HostSessionSource::new();
        let state = Arc::new(StateService::new(
            InMemoryStateStore::new(),
            SystemClock,
            source.clone(),
            HealthPolicy::default(),
        ));

        let sync = Self::restore_sync(device_id, &paths);
        let converter_host = ConverterHost::new(verifier, PendingSandbox);

        let services = Self {
            paths,
            entropy,
            audit,
            vault: Mutex::new(None),
            recovery: Mutex::new(None),
            broker,
            state,
            source,
            sync,
            converter_host,
            mcp_http: Mutex::new(None),
        };
        services.load_recovery();
        services
    }

    /// 活动库的存储实现（DPAPI 外壳；句柄无状态，按需构造）。
    pub fn vault_store(&self) -> DpapiVaultStore {
        DpapiVaultStore::new(self.paths.vault_file())
    }

    /// 恢复密钥文件回灌（S3）：展示形态经 DPAPI 封装保存，恢复时还原。
    fn load_recovery(&self) {
        let sealed = match std::fs::read(self.paths.recovery_key_file()) {
            Ok(bytes) => bytes,
            Err(_) => return,
        };
        if let Ok(text) = dpapi::unprotect(&sealed) {
            if let Ok(display) = String::from_utf8(text) {
                if let Ok(key) = RecoveryKey::from_display(&display) {
                    *self.recovery.lock().expect("恢复密钥互斥锁中毒") = Some(key);
                }
            }
        }
    }

    /// 恢复密钥落盘（DPAPI 封装；调用方负责「只展示一次」的 UI 纪律）。
    pub fn store_recovery(&self, recovery: &RecoveryKey) -> Result<(), String> {
        let sealed = dpapi::protect(recovery.display().as_bytes()).map_err(|e| e.to_string())?;
        std::fs::write(self.paths.recovery_key_file(), sealed).map_err(|e| e.to_string())?;
        *self.recovery.lock().expect("恢复密钥互斥锁中毒") =
            Some(RecoveryKey::from_display(&recovery.display()).map_err(|e| e.to_string())?);
        Ok(())
    }

    /// 配对身份与设备清单回灌（宿主持久化，ab-sync 只认内存）。
    fn restore_sync(device_id: &str, paths: &HostPaths) -> SyncService {
        let identity = match dpapi::unprotect(&std::fs::read(paths.sync_identity_file()).unwrap_or_default())
        {
            Ok(secret) if secret.len() == 32 => {
                let mut key = [0u8; 32];
                key.copy_from_slice(&secret);
                SyncIdentity::from_secret(device_id, ab_crypto::Zeroizing::new(key))
                    .unwrap_or_else(|_| Self::fresh_identity(device_id, paths))
            }
            _ => Self::fresh_identity(device_id, paths),
        };
        let devices: Vec<PairedDevice> = std::fs::read_to_string(paths.devices_file())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        SyncService::new(identity, devices)
    }

    fn fresh_identity(device_id: &str, paths: &HostPaths) -> SyncIdentity {
        let identity = SyncIdentity::generate(device_id, &CngEntropy)
            .expect("配对身份生成不应失败");
        let secret = identity.secret_key();
        if let Ok(sealed) = dpapi::protect(secret.as_slice()) {
            let _ = std::fs::create_dir_all(&paths.root);
            let _ = std::fs::write(paths.sync_identity_file(), sealed);
        }
        identity
    }

    /// 设备清单落盘（公开信息：公钥 + 指纹）。
    pub fn persist_devices(&self) -> Result<(), String> {
        let devices = self.sync.devices().map_err(|e| e.to_string())?;
        std::fs::write(
            self.paths.devices_file(),
            serde_json::to_string_pretty(&devices).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }

    /// 备份元数据读写（A6.7 最后验证时间）。
    pub fn backup_meta(&self) -> BackupMeta {
        std::fs::read_to_string(self.paths.backup_dir().join("backup-meta.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn write_backup_meta(&self, meta: &BackupMeta) -> Result<(), String> {
        let dir = self.paths.backup_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("backup-meta.json"),
            serde_json::to_string_pretty(meta).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}

/// 本机设备 id（E3）：`COMPUTERNAME` 小写；取不到退 `pc`（不猜测硬件标识）。
pub fn device_id() -> String {
    std::env::var("COMPUTERNAME")
        .map(|name| name.to_lowercase())
        .unwrap_or_else(|_| "pc".to_string())
}

/// 配对平台（桌面宿主固定 Windows，M7 边界：仅 Windows / iOS / Android）。
pub fn device_platform() -> DevicePlatform {
    DevicePlatform::Windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_verifier_rejects_when_trust_root_unconfigured() {
        let verifier = OfficialVerifier::new(None, None);
        let manifest = sample_manifest("00".repeat(64), "ab".repeat(64));
        assert!(matches!(
            verifier.verify(&manifest, b"module"),
            Err(VerifyError::SignerNotAllowed)
        ));
    }

    #[test]
    fn official_verifier_accepts_self_signed_sample_and_rejects_tamper() {
        let module = b"wasm-module-bytes".to_vec();
        let digest = hex_lower(&sha256(&module));
        let mut manifest = sample_manifest(digest.clone(), String::new());
        let (secret, public) = ab_crypto::ed25519_keypair(&CngEntropy).expect("密钥生成失败");
        manifest.signature.signer = "aibridge-official-2026".into();
        let signed = canonical_signed_bytes(&manifest, &digest);
        manifest.signature.value = hex_lower(&ab_crypto::ed25519_sign(&secret, &signed));

        let verifier = OfficialVerifier::new(
            Some("aibridge-official-2026".into()),
            Some(public),
        );
        assert!(verifier.verify(&manifest, &module).is_ok(), "自签样本应通过验签");

        // 换模块保清单 → 拒载。
        assert!(matches!(
            verifier.verify(&manifest, b"other-module"),
            Err(VerifyError::BadSignature)
        ));

        // 换信任根公钥 → 拒载。
        let (_, other_public) = ab_crypto::ed25519_keypair(&CngEntropy).expect("密钥生成失败");
        let wrong_key = OfficialVerifier::new(Some("aibridge-official-2026".into()), Some(other_public));
        assert!(matches!(
            wrong_key.verify(&manifest, &module),
            Err(VerifyError::BadSignature)
        ));

        // 签名者不受信 → 拒载。
        let wrong_signer = OfficialVerifier::new(Some("someone-else".into()), Some(public));
        assert!(matches!(
            wrong_signer.verify(&manifest, &module),
            Err(VerifyError::SignerNotAllowed)
        ));
    }

    fn sample_manifest(module_sha256: String, signature_value: String) -> ConverterManifest {
        ConverterManifest {
            id: "sample-app".into(),
            name: "Sample".into(),
            version: "0.1.0".into(),
            level: ab_contract::AccessLevel::B,
            readable_paths: vec!["{APPDATA}\\sample-app".into()],
            entry: "converter.wasm".into(),
            module_sha256,
            signature: ab_contract::Signature {
                algorithm: "ed25519".into(),
                signer: "aibridge-official-2026".into(),
                value: signature_value,
            },
            min_app_version: None,
        }
    }
}
