//! UDL 对象实现：每个方法只做「转发 + 形状搬运」，零业务规则。
//!
//! 漂移处一律显式失败或回填中性值（不编造数据），逐条记录在 `core/ffi/README.md`。
//! 审计「who」在 UDL 无注入面：统一记 [`FFI_ACTOR`]（来源标记，非身份主张）。

use std::sync::{Arc, Mutex};

use ab_audit as audit;
use ab_audit::AuditSink as _;
use ab_broker as broker;
use ab_contract as contract;
use ab_crypto as crypto;
use ab_state as state;
use ab_sync as sync;
use ab_vault as vault;

use crate::adapters::{BlobStoreAdapter, ClockAdapter, EntropyAdapter, SourceAdapter, TransportAdapter};
use crate::traits::{Clock, Entropy, SessionSource, Transport, VaultStore};
use crate::types::*;

/// 审计 `who`：UDL 未提供调用方身份注入，统一记来源标记（见 README 待确认清单）。
pub(crate) const FFI_ACTOR: &str = "ffi";

/// UDL 漂移面（core 无对应公开 API / 入参不足）统一落 [`CoreError::INTERNAL`]。
const DRIFT: CoreError = CoreError::INTERNAL;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/* ============================== Vault ============================== */

/// ab-vault 镜像：会话句柄自持（`Mutex<Option<VaultSession>>`，`lock()` 即释放）。
/// 变更只改内存（与 core `Vault::add/update/trash/restore` 同语义），`save()` 才落存储。
pub struct Vault {
    store: Arc<dyn VaultStore>,
    clock: Arc<dyn Clock>,
    entropy: Arc<dyn Entropy>,
    session: Mutex<Option<vault::VaultSession>>,
}

impl Vault {
    fn lock_session(&self) -> Result<std::sync::MutexGuard<'_, Option<vault::VaultSession>>, CoreError> {
        self.session.lock().map_err(|_| CoreError::INTERNAL)
    }

    /// 已解锁会话（未解锁 = 拒绝）。
    fn session<'a>(
        guard: &'a mut Option<vault::VaultSession>,
    ) -> Result<&'a mut vault::VaultSession, CoreError> {
        guard.as_mut().ok_or(CoreError::DENIED)
    }

    fn find_by_account_ref<'a>(
        entries: &[&'a vault::Entry],
        account_ref: &str,
    ) -> Option<&'a vault::Entry> {
        entries.iter().copied().find(|e| e.account_ref == account_ref)
    }

    pub fn unlock(&self, master_password: String) -> Result<(), CoreError> {
        let mut adapter = BlobStoreAdapter(self.store.clone());
        // 首次解锁（存储空）= 初始化新库（S3 初始化流程的建库语义；恢复密钥随
        // Vault::recovery_copy 单独生成，不在此触发）。已存数据走正常 unlock。
        let opened = match vault::VaultSession::unlock(&mut adapter, master_password.as_bytes()) {
            Ok(session) => session,
            Err(vault::VaultError::NotFound) => {
                let kdf = crypto::KdfParams::default().validated().map_err(CoreError::from)?;
                let created = vault::VaultSession::create(
                    "AI-Bridge",
                    master_password.as_bytes(),
                    &kdf,
                    &EntropyAdapter(self.entropy.clone()),
                )
                .map_err(CoreError::from)?;
                created.save(&mut adapter, &EntropyAdapter(self.entropy.clone()))?;
                created
            }
            Err(e) => return Err(CoreError::from(e)),
        };
        *self.lock_session()? = Some(opened);
        Ok(())
    }

    pub fn lock(&self) {
        if let Ok(mut guard) = self.lock_session() {
            *guard = None;
        }
    }

    pub fn is_locked(&self) -> bool {
        self.lock_session().map(|g| g.is_none()).unwrap_or(true)
    }

    pub fn list_entries(&self, query: Option<String>) -> Result<Vec<EntrySummary>, CoreError> {
        // core `Vault::entries()` 无查询语义（UDL 漂移）：只镜像全量列表。
        if query.is_some() {
            return Err(DRIFT);
        }
        let mut guard = self.lock_session()?;
        let session = Self::session(&mut guard)?;
        Ok(session.vault.entries().into_iter().map(EntrySummary::from).collect())
    }

    pub fn get_entry(&self, account_ref: String) -> Result<Entry, CoreError> {
        let mut guard = self.lock_session()?;
        let session = Self::session(&mut guard)?;
        let entries = session.vault.entries();
        Self::find_by_account_ref(&entries, &account_ref)
            .map(Entry::from)
            .ok_or(CoreError::NOT_FOUND)
    }

    pub fn put_entry(&self, entry: Entry) -> Result<(), CoreError> {
        let mut guard = self.lock_session()?;
        let session = Self::session(&mut guard)?;
        let existing_uuid = {
            let entries = session.vault.entries();
            Self::find_by_account_ref(&entries, &entry.account_ref).map(|e| e.uuid)
        };
        match existing_uuid {
            Some(uuid) => {
                // UDL 无 uuid 字段：按 account_ref 找回既有 uuid，字段级覆盖（无合并策略）。
                let mut core_entry: vault::Entry = entry.into();
                core_entry.uuid = uuid;
                session.vault.update(core_entry).map_err(CoreError::from)
            }
            None => {
                let mut core_entry = vault::Entry::new(
                    &entry.title,
                    &entry.account_ref,
                    entry.kind.into(),
                    &EntropyAdapter(self.entropy.clone()),
                )
                .map_err(CoreError::from)?;
                core_entry.username = entry.username.clone();
                core_entry.secret = entry.secret.clone();
                core_entry.totp_seed = entry.totp_seed.clone();
                core_entry.url = entry.url.clone();
                core_entry.note = entry.note.clone();
                session.vault.add(core_entry).map_err(CoreError::from)
            }
        }
    }

    pub fn delete_entry(&self, account_ref: String) -> Result<(), CoreError> {
        let mut guard = self.lock_session()?;
        let session = Self::session(&mut guard)?;
        let uuid = {
            let entries = session.vault.entries();
            Self::find_by_account_ref(&entries, &account_ref)
                .map(|e| e.uuid)
                .ok_or(CoreError::NOT_FOUND)?
        };
        // UDL `delete_entry` ↔ core `Vault::trash`（UDL 另有 trash()/restore() 面）。
        session.vault.trash(uuid).map_err(CoreError::from)
    }

    pub fn trash(&self) -> Vec<EntrySummary> {
        self.lock_session()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .map(|s| s.vault.trash_entries().iter().map(EntrySummary::from).collect())
            })
            .unwrap_or_default()
    }

    pub fn restore(&self, account_ref: String) -> Result<(), CoreError> {
        let mut guard = self.lock_session()?;
        let session = Self::session(&mut guard)?;
        let uuid = session
            .vault
            .trash_entries()
            .iter()
            .find(|e| e.account_ref == account_ref)
            .map(|e| e.uuid)
            .ok_or(CoreError::NOT_FOUND)?;
        session.vault.restore(uuid).map_err(CoreError::from)
    }

    pub fn generate(&self, spec: GeneratorSpec) -> Result<GeneratedSecret, CoreError> {
        // core `ab_vault::generate_secret`（B6）：只转发，字符集/长度语义在 core。
        let generated = vault::generate_secret(
            &vault::GeneratorSpec::from(spec),
            &EntropyAdapter(self.entropy.clone()),
        )
        .map_err(CoreError::from)?;
        Ok(GeneratedSecret::from(generated))
    }

    pub fn generate_passphrase(&self, spec: PassphraseSpec) -> Result<String, CoreError> {
        // core `ab_vault::generate_passphrase`（A5.2）：词表/词数/分隔符全由调用方配置。
        let phrase = vault::generate_passphrase(
            &vault::PassphraseSpec::from(spec),
            &EntropyAdapter(self.entropy.clone()),
        )
        .map_err(CoreError::from)?;
        Ok(phrase.to_string())
    }

    pub fn totp(&self, account_ref: String) -> Result<String, CoreError> {
        // core `ab_vault::totp_code`（B7）：seed 取条目 totp_seed，时间取注入 Clock。
        let seed = {
            let mut guard = self.lock_session()?;
            let session = Self::session(&mut guard)?;
            let entries = session.vault.entries();
            Self::find_by_account_ref(&entries, &account_ref)
                .and_then(|e| e.totp_seed.clone())
                .ok_or(CoreError::NOT_FOUND)?
        };
        let now = self.clock.now_unix_secs().max(0) as u64;
        vault::totp_code(&seed, now).map_err(CoreError::from)
    }

    pub fn save(&self) -> Result<(), CoreError> {
        let mut guard = self.lock_session()?;
        let session = Self::session(&mut guard)?;
        session
            .save(&mut BlobStoreAdapter(self.store.clone()), &EntropyAdapter(self.entropy.clone()))
            .map_err(CoreError::from)
    }

    pub fn recovery_copy(&self, recovery: Arc<RecoveryKey>) -> Result<Vec<u8>, CoreError> {
        let mut guard = self.lock_session()?;
        let session = Self::session(&mut guard)?;
        session
            .recovery_copy(&recovery.inner, &EntropyAdapter(self.entropy.clone()))
            .map_err(CoreError::from)
    }

    pub fn from_backup(&self, master_password: String, data: Vec<u8>) -> Result<(), CoreError> {
        let opened =
            vault::VaultSession::from_backup(master_password.as_bytes(), &data).map_err(CoreError::from)?;
        *self.lock_session()? = Some(opened);
        Ok(())
    }

    pub fn open_copy(&self, recovery: Arc<RecoveryKey>, data: Vec<u8>) -> Result<(), CoreError> {
        let opened = recovery.inner.open_copy(&data).map_err(CoreError::from)?;
        *self.lock_session()? = Some(opened);
        Ok(())
    }
}

/// core `ab_vault::RecoveryKey` 镜像（不透明句柄；`Debug` 整体脱敏）。
pub struct RecoveryKey {
    inner: vault::RecoveryKey,
}

impl RecoveryKey {
    // 构造器返回裸 `Self`：`#[uniffi::constructor]` 脚手架负责 Arc 包装。
    pub fn generate(entropy: Arc<dyn Entropy>) -> Result<RecoveryKey, CoreError> {
        let key = vault::RecoveryKey::generate(&EntropyAdapter(entropy)).map_err(CoreError::from)?;
        Ok(RecoveryKey { inner: key })
    }

    pub fn from_display(text: String) -> Result<RecoveryKey, CoreError> {
        let key = vault::RecoveryKey::from_display(&text).map_err(CoreError::from)?;
        Ok(RecoveryKey { inner: key })
    }

    pub fn display(&self) -> String {
        self.inner.display()
    }
}

impl std::fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryKey [REDACTED]")
    }
}

/* ============================== StateBus ============================== */

/// ab-state 镜像：`InMemoryStateStore`（UDL 无状态持久化注入面，见 README）。
pub struct StateBus {
    service: state::StateService<state::InMemoryStateStore, ClockAdapter, SourceAdapter>,
}

impl StateBus {
    pub fn matrix(&self) -> Vec<MatrixRow> {
        // UDL 无过滤参数：全量 + 默认排序。UDL 无 Throws：失败回空表（见 README）。
        let filters = state::MatrixFilters {
            account_ref: None,
            endpoint_id: None,
            endpoint_kind: None,
            source_type: None,
            source_id: None,
            expiry: state::ExpiryFilter::Any,
            sort: state::MatrixSort::default(),
        };
        self.service
            .matrix(&filters)
            .map(|rows| rows.iter().map(MatrixRow::from).collect())
            .unwrap_or_default()
    }

    pub fn capture(&self, _source_id: String, _domain: String) -> Result<SessionEnvelope, CoreError> {
        // 漂移：core `StateService::capture(&Endpoint) -> CaptureReport` 与 UDL
        // `capture(source_id, domain) -> SessionEnvelope` 形状不同，且 SessionSource 读取
        // 面缺 account_ref 语义（见 README），不自造信封。
        Err(DRIFT)
    }

    pub fn snapshot(&self) -> Vec<SnapshotSummary> {
        self.service
            .list_snapshots()
            .map(|metas| metas.iter().map(SnapshotSummary::from).collect())
            .unwrap_or_default()
    }

    pub fn rollback(&self, snapshot_id: String) -> Result<(), CoreError> {
        self.service
            .rollback(&snapshot_id)
            .map(|_| ())
            .map_err(CoreError::from)
    }

    pub fn health(&self) -> HealthReport {
        // UDL `HealthReport` 三字段在 core `ab_state::HealthReport` 无数据源（见 README）：
        // 未回填（fail-safe `false` / 空），不编造读数。
        HealthReport::default()
    }
}

/* ============================== Broker ============================== */

/// ab-broker 镜像：`who` 记 [`FFI_ACTOR`]；`issue` 的真实会话入参 UDL 未提供（漂移）。
pub struct Broker {
    service: Result<broker::BrokerService, CoreError>,
    clock: Arc<dyn Clock>,
}

impl Broker {
    fn service(&self) -> Result<&broker::BrokerService, CoreError> {
        self.service.as_ref().map_err(|e| *e)
    }

    fn now(&self) -> i64 {
        self.clock.now_unix_secs()
    }

    pub fn issue(&self, _scope: ScopeSpec, _ttl_seconds: u32) -> Result<IssuedSession, CoreError> {
        // 漂移：core `BrokerService::issue` 需要 `who` + 真实 `SessionEnvelope`（代持对象），
        // UDL 签名两者皆缺；IssuedSession.account_ref 亦无来源。不自造会话（见 README）。
        Err(DRIFT)
    }

    pub fn renew(&self, session_id: String, ttl_seconds: u32) -> Result<IssuedSession, CoreError> {
        let status = self
            .service()?
            .renew(FFI_ACTOR, &session_id, Some(ttl_seconds as u64), self.now())
            .map_err(CoreError::from)?;
        Ok(IssuedSession::from(&status))
    }

    pub fn release(&self, session_id: String) -> Result<(), CoreError> {
        self.service()?
            .release(FFI_ACTOR, &session_id, self.now())
            .map_err(CoreError::from)
    }

    pub fn revoke(&self, session_id: String) -> Result<(), CoreError> {
        self.service()?
            .revoke(FFI_ACTOR, &session_id, self.now())
            .map_err(CoreError::from)
    }

    pub fn revoke_all(&self) -> Vec<IssuedSession> {
        // UDL 无 Throws：失败回空表（见 README）。core 返回计数、UDL 返回会话行：
        // 先取活动行再整体吊销（纯形状搬运）。
        (|| -> Result<Vec<IssuedSession>, CoreError> {
            let service = self.service()?;
            let now = self.now();
            let actives: Vec<IssuedSession> = service
                .list(now)
                .map_err(CoreError::from)?
                .iter()
                .filter(|s| s.state == broker::GrantState::Active)
                .map(IssuedSession::from)
                .collect();
            service.revoke_all(FFI_ACTOR, now).map_err(CoreError::from)?;
            Ok(actives)
        })()
        .unwrap_or_default()
    }

    pub fn list_sessions(&self) -> Vec<IssuedSession> {
        // UDL 无 Throws：失败回空表（见 README）。
        self.service()
            .and_then(|s| s.list(self.now()).map_err(CoreError::from))
            .map(|rows| rows.iter().map(IssuedSession::from).collect())
            .unwrap_or_default()
    }

    pub fn sweep(&self) -> Vec<IssuedSession> {
        // UDL 无 Throws：失败回空表（见 README）。core 返回被释放 id、UDL 返回会话行：
        // 按 id 回连总览（纯形状搬运）。
        (|| -> Result<Vec<IssuedSession>, CoreError> {
            let service = self.service()?;
            let now = self.now();
            let released = service.sweep(FFI_ACTOR, now).map_err(CoreError::from)?;
            let rows = service.list(now).map_err(CoreError::from)?;
            Ok(rows
                .iter()
                .filter(|s| released.iter().any(|id| id == &s.session_id))
                .map(IssuedSession::from)
                .collect())
        })()
        .unwrap_or_default()
    }
}

/* ============================== SyncEngine ============================== */

/// ab-sync 镜像。本机 `SyncIdentity` 在 UDL 无注入面：以注入 Entropy 生成一次性
/// device_id + 密钥（shim，见 README 待确认清单），否则 unpack/transfer 无法运行。
pub struct SyncEngine {
    service: Result<sync::SyncService, CoreError>,
    transport: Arc<dyn Transport>,
    entropy: Arc<dyn Entropy>,
    clock: Arc<dyn Clock>,
}

impl SyncEngine {
    fn service(&self) -> Result<&sync::SyncService, CoreError> {
        self.service.as_ref().map_err(|e| *e)
    }

    pub fn pair(&self) -> Result<PairingTicket, CoreError> {
        // 漂移：core `SyncService::pair(&PairingOffer, now)` 需远端 offer；UDL 无入参，
        // 亦无 ticket 表可回填（见 README）。
        Err(DRIFT)
    }

    pub fn confirm(&self, _ticket_id: String, _remote_fingerprint: String) -> Result<PairingTicket, CoreError> {
        // 漂移：core `confirm_pairing(&PairedDevice)` 需全量设备记录；UDL 无对应载荷（见 README）。
        Err(DRIFT)
    }

    pub fn transfer(&self, peer_id: String, envelopes: Vec<SessionEnvelope>) -> Result<TransferReport, CoreError> {
        // 逐信封转发 core `transfer`，按结果计 sent/failed（无其它判定）。
        let service = self.service()?;
        let transport = TransportAdapter(self.transport.clone());
        let entropy = EntropyAdapter(self.entropy.clone());
        let mut report = TransferReport::default();
        for envelope in envelopes {
            let core_envelope = contract::SessionEnvelope::from(envelope);
            match service.transfer(&peer_id, &core_envelope, &transport, &entropy) {
                Ok(()) => report.sent += 1,
                Err(_) => report.failed += 1,
            }
        }
        Ok(report)
    }

    pub fn pack(&self, _envelopes: Vec<SessionEnvelope>) -> Result<Vec<u8>, CoreError> {
        // 漂移：core `pack(to_device_id, envelope, validity_secs, now, entropy)` 的收件方与
        // 有效期 UDL 未提供；A8 包为单信封，UDL 是信封序列（见 README）。
        Err(DRIFT)
    }

    pub fn unpack(&self, package: Vec<u8>) -> Result<Vec<SessionEnvelope>, CoreError> {
        // A8 包字节 = `OfflinePack` 的 serde 形态（core「导出即文件字节」）。
        let pack: sync::OfflinePack =
            serde_json::from_slice(&package).map_err(|_| CoreError::CONVERSION)?;
        let envelope = self
            .service()?
            .unpack(&pack, self.clock.now_unix_secs())
            .map_err(CoreError::from)?;
        Ok(vec![SessionEnvelope::from(envelope)])
    }
}

/* ============================== ConverterHost ============================== */

/// ab-converter 镜像。core `ConverterHost` 需注入 `SignatureVerifier` / `SandboxRunner`
/// 且 `load` 需模块字节，UDL 均未提供（见 README），三方法显式失败，不自造验签/沙盒。
pub struct ConverterHost;

impl ConverterHost {
    pub fn load(&self, _manifest: ConverterManifest) -> Result<(), CoreError> {
        Err(DRIFT)
    }

    pub fn verify(&self, _manifest: ConverterManifest) -> Result<ManifestVerification, CoreError> {
        Err(DRIFT)
    }

    pub fn run_extract(&self, _converter_id: String, _level: String) -> Result<StandardAccess, CoreError> {
        Err(DRIFT)
    }
}

/* ============================== AuditLog ============================== */

/// ab-audit 镜像：与 Broker 共用同一 `InMemoryAuditSink` 实例。
/// 持久化 sink 在 UDL 无注入面（见 README）。
pub struct AuditLog {
    sink: Arc<audit::InMemoryAuditSink>,
}

impl AuditLog {
    pub fn record_or_fail(&self, event: AuditEvent) -> Result<(), CoreError> {
        // 审计写入唯一入口（fail-closed）：失败即报错，调用方不得继续敏感操作。
        self.sink
            .record_or_fail(audit::AuditEvent::from(event))
            .map_err(CoreError::from)
    }

    pub fn query(&self, filter: AuditFilter) -> Vec<AuditEvent> {
        // UDL 无 Throws：失败回空表（见 README）。
        self.sink
            .query(&audit::AuditFilter::from(filter))
            .map(|events| events.into_iter().map(AuditEvent::from).collect())
            .unwrap_or_default()
    }
}

/* ============================== McpServer ============================== */

/// ab-mcp 镜像（仅 desktop 启动；移动端保留镜像面）。UDL 无构造入口，
/// `serve()` 无从装配 `McpService`（见 README）。
pub struct McpServer;

impl McpServer {
    pub fn serve(&self) -> Result<(), CoreError> {
        // 漂移：core `serve_stdio(McpService)` 为 async 且需 broker/state/who 装配，
        // UDL 未提供构造入口（见 README）。
        Err(DRIFT)
    }

    pub fn tools(&self) -> Vec<McpTool> {
        // core `McpService::tool_router()` 为私有、无其它公开工具面（core 缺口，见 README）：
        // 不硬抄工具名，回空表待 core 导出后机械接线。
        Vec::new()
    }
}

/* ============================== A5.1 强度值（core spec 方法的自由函数镜像） ============================== */

/// core `GeneratorSpec::entropy_bits`（UDL dictionary 无方法面，镜像为自由函数）。
pub fn generator_entropy_bits(spec: GeneratorSpec) -> Result<f64, CoreError> {
    vault::GeneratorSpec::from(spec)
        .entropy_bits()
        .map_err(CoreError::from)
}

/// core `PassphraseSpec::entropy_bits`（同上）。
pub fn passphrase_entropy_bits(spec: PassphraseSpec) -> Result<f64, CoreError> {
    vault::PassphraseSpec::from(spec)
        .entropy_bits()
        .map_err(CoreError::from)
}

/* ============================== AibridgeCore 与组合根 ============================== */

/// 聚合入口：各子模块对象由 core 持有，宿主只经本对象取得句柄。
pub struct AibridgeCore {
    vault: Arc<Vault>,
    state: Arc<StateBus>,
    broker: Arc<Broker>,
    sync: Arc<SyncEngine>,
    converter_host: Arc<ConverterHost>,
    audit: Arc<AuditLog>,
}

impl AibridgeCore {
    pub fn vault(&self) -> Arc<Vault> {
        self.vault.clone()
    }

    pub fn state(&self) -> Arc<StateBus> {
        self.state.clone()
    }

    pub fn broker(&self) -> Arc<Broker> {
        self.broker.clone()
    }

    pub fn sync(&self) -> Arc<SyncEngine> {
        self.sync.clone()
    }

    pub fn converter_host(&self) -> Arc<ConverterHost> {
        self.converter_host.clone()
    }

    pub fn audit(&self) -> Arc<AuditLog> {
        self.audit.clone()
    }
}

/// 组合根：core 对象由宿主显式构造并注入全部 trait（无单例、无服务定位器）。
pub fn new_core(
    config: CoreConfig,
    vault_store: Arc<dyn VaultStore>,
    session_source: Arc<dyn SessionSource>,
    transport: Arc<dyn Transport>,
    clock: Arc<dyn Clock>,
    entropy: Arc<dyn Entropy>,
) -> Arc<AibridgeCore> {
    // `CoreConfig`：core 无配置面消费点（UDL 漂移，见 README），仅镜像签名。
    let _ = &config;

    // 审计单例纪律：broker 与 AuditLog 共用**同一** sink 实例（审查报告 P1-6：
    // 曾各建一个 InMemoryAuditSink，audit.query 永远查不到 broker 的签发/吊销事件）。
    let sink: Arc<audit::InMemoryAuditSink> = Arc::new(audit::InMemoryAuditSink::new());

    // broker 句柄查表键：UDL 无主密钥注入面，以注入 Entropy 生成一次性主密钥（shim）。
    let broker_service = (|| -> Result<broker::BrokerService, CoreError> {
        let adapter = EntropyAdapter(entropy.clone());
        let mut master_key = [0u8; 32];
        crypto::Entropy::fill(&adapter, &mut master_key).map_err(CoreError::from)?;
        broker::BrokerService::new(sink.clone(), &master_key).map_err(CoreError::from)
    })();

    // 本机同步身份：UDL 无 device_id/身份注入面，以注入 Entropy 生成一次性身份（shim）。
    let sync_service = (|| -> Result<sync::SyncService, CoreError> {
        let adapter = EntropyAdapter(entropy.clone());
        let mut id_bytes = [0u8; 8];
        crypto::Entropy::fill(&adapter, &mut id_bytes).map_err(CoreError::from)?;
        let device_id = format!("ffi-{}", hex(&id_bytes));
        let identity = sync::SyncIdentity::generate(&device_id, &adapter).map_err(CoreError::from)?;
        Ok(sync::SyncService::new(identity, Vec::new()))
    })();

    Arc::new(AibridgeCore {
        vault: Arc::new(Vault {
            store: vault_store.clone(),
            clock: clock.clone(),
            entropy: entropy.clone(),
            session: Mutex::new(None),
        }),
        state: Arc::new(StateBus {
            service: state::StateService::new(
                state::InMemoryStateStore::new(),
                ClockAdapter(clock.clone()),
                SourceAdapter(session_source),
                state::HealthPolicy::default(),
            ),
        }),
        broker: Arc::new(Broker {
            service: broker_service,
            clock: clock.clone(),
        }),
        sync: Arc::new(SyncEngine {
            service: sync_service,
            transport,
            entropy,
            clock,
        }),
        converter_host: Arc::new(ConverterHost),
        audit: Arc::new(AuditLog { sink: sink.clone() }),
    })
}
