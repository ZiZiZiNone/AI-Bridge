//! ab-ffi 集成测试：宿主注入面（trait 假实现）→ new_core → 各服务走通。
//!
//! 验证「FFI 层只转发」：与 core 直测同一套断言口径（tests/e2e-pipeline 同款语义），
//! 不重复 core 的业务断言，只锁 FFI 适配层的形状正确性。

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use ab_ffi::{
    new_core, AuditAction, AuditFilter, AuditResult, Clock, CoreConfig, CredentialKind, Entropy,
    GeneratedSecret, GeneratorSpec, PassphraseSpec, SessionSource, Transport, VaultStore,
};

/// ScopeSpec 最小合法值（core 校验要求域名白名单非空）。
fn scope_spec() -> ab_ffi::ScopeSpec {
    ab_ffi::ScopeSpec {
        domain_allowlist: vec!["example.test".into()],
        read_only: true,
        single_use: false,
    }
}

/* ------------------------------ 假实现 ------------------------------ */

/// 内存 VaultStore（list_keys 语义：blob 键空间，主库固定键）。
#[derive(Default)]
struct FakeVaultStore {
    blobs: Mutex<HashMap<String, Vec<u8>>>,
}

impl VaultStore for FakeVaultStore {
    fn list_keys(&self) -> Result<Vec<String>, ab_ffi::CoreError> {
        Ok(self.blobs.lock().unwrap().keys().cloned().collect())
    }
    fn read(&self, key: String) -> Result<Vec<u8>, ab_ffi::CoreError> {
        self.blobs
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .ok_or(ab_ffi::CoreError::NOT_FOUND)
    }
    fn remove(&self, key: String) -> Result<(), ab_ffi::CoreError> {
        self.blobs
            .lock()
            .unwrap()
            .remove(&key)
            .map(|_| ())
            .ok_or(ab_ffi::CoreError::NOT_FOUND)
    }
    fn write(&self, key: String, value: Vec<u8>) -> Result<(), ab_ffi::CoreError> {
        self.blobs.lock().unwrap().insert(key, value);
        Ok(())
    }
}

/// 固定步进时钟。
struct FakeClock(AtomicI64);
impl Clock for FakeClock {
    fn now_unix_secs(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// 计数熵（非零、每次调用不同：Entry::new 的 uuid 需互异）。
struct FakeEntropy(AtomicI64);
impl FakeEntropy {
    fn new() -> Self {
        Self(AtomicI64::new(0))
    }
}
impl Entropy for FakeEntropy {
    fn fill(&self, length: u32) -> Vec<u8> {
        let salt = self.0.fetch_add(1, Ordering::SeqCst) as u8;
        (0..length as usize)
            .map(|i| ((i + salt as usize) % 251 + 1) as u8)
            .collect()
    }
}

/// 空会话源（StateBus 只测无会话路径）。
struct NoSessions;
impl SessionSource for NoSessions {
    fn available_sources(&self) -> Vec<String> {
        Vec::new()
    }
    fn capture(&self, _source_id: String, _domain: String) -> Vec<ab_ffi::Cookie> {
        Vec::new()
    }
    fn inject(&self, _source_id: String, _cookies: Vec<ab_ffi::Cookie>) {}
}

/// 环回传输（send 落队列，receive 取出）。
#[derive(Default)]
struct LoopTransport {
    inbox: Mutex<Vec<Vec<u8>>>,
    peers: Mutex<Vec<ab_ffi::PeerInfo>>,
}
impl Transport for LoopTransport {
    fn peers(&self) -> Vec<ab_ffi::PeerInfo> {
        self.peers.lock().unwrap().clone()
    }
    fn receive(&self, _timeout_millis: u32) -> Option<Vec<u8>> {
        self.inbox.lock().unwrap().pop()
    }
    fn send(&self, _peer_id: String, payload: Vec<u8>) -> Result<(), ab_ffi::CoreError> {
        self.inbox.lock().unwrap().push(payload);
        Ok(())
    }
}

fn fixture_core() -> (Arc<ab_ffi::AibridgeCore>, Arc<FakeVaultStore>, Arc<FakeClock>) {
    let store = Arc::new(FakeVaultStore::default());
    let clock = Arc::new(FakeClock(AtomicI64::new(1_700_000_000)));
    let core = new_core(
        CoreConfig {
            data_dir: String::new(),
            display_name: "test".into(),
        },
        store.clone(),
        Arc::new(NoSessions),
        Arc::new(LoopTransport::default()),
        clock.clone(),
        Arc::new(FakeEntropy::new()),
    );
    (core, store, clock)
}

/* ------------------------------ 测试 ------------------------------ */

#[test]
fn vault_full_chain_unlock_put_generate_totp_save_restore() {
    let (core, store, clock) = fixture_core();
    let vault = core.vault();

    // 初始化 + 解锁。
    vault.unlock("master-pw".into()).unwrap();
    assert!(!vault.is_locked());

    // A1.1 条目：全字段。
    let entry = ab_ffi::Entry {
        account_ref: "acc-1".into(),
        title: "示例站".into(),
        kind: CredentialKind::TOTP,
        username: Some("demo@example.test".into()),
        secret: None,
        totp_seed: Some("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ".into()),
        url: Some("https://portal.example.test".into()),
        note: Some("备注".into()),
    };
    vault.put_entry(entry.clone()).unwrap();

    let listed = vault.list_entries(None).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].account_ref, "acc-1");

    let got = vault.get_entry("acc-1".into()).unwrap();
    assert_eq!(got.totp_seed.as_deref(), Some("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"));

    // B7.2 TOTP：RFC 6238 T=59 向量（8 位 URI 参数）。
    let uri_seed = "otpauth://totp/test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8&period=30&algorithm=SHA1";
    let entry_uri = ab_ffi::Entry {
        account_ref: "acc-2".into(),
        title: "URI 种子".into(),
        kind: CredentialKind::TOTP,
        username: None,
        secret: None,
        totp_seed: Some(uri_seed.into()),
        url: None,
        note: None,
    };
    vault.put_entry(entry_uri).unwrap();
    // 期望值 = core 直算（同种子、同注入时钟），FFI 只验证转发无形状损失。
    let expected = ab_vault::totp_code(uri_seed, 1_700_000_000).unwrap();
    assert_eq!(vault.totp("acc-2".into()).unwrap(), expected);

    // B6 生成器 + A5.1 强度值。
    let spec = GeneratorSpec {
        length: 20,
        lowercase: true,
        uppercase: true,
        digits: true,
        symbols: false,
    };
    let GeneratedSecret { value, charset } = vault.generate(spec).unwrap();
    assert_eq!(value.chars().count(), 20);
    assert!(value.chars().all(|c| charset.contains(c)));
    let bits = ab_ffi::generator_entropy_bits(GeneratorSpec {
        length: 20,
        lowercase: true,
        uppercase: false,
        digits: false,
        symbols: false,
    })
    .unwrap();
    assert!((bits - 20.0 * 26f64.log2()).abs() < 1e-9);

    // A5.2 口令词模式。
    let phrase = vault
        .generate_passphrase(PassphraseSpec {
            words: vec!["alpha".into(), "beta".into(), "gamma".into(), "delta".into()],
            word_count: 3,
            separator: "-".into(),
        })
        .unwrap();
    assert_eq!(phrase.split('-').count(), 3);

    // A6.1 保存 → A6.9 恢复：锁后经 from_backup 重开，条目仍在。
    vault.save().unwrap();
    let bytes = store.read(ab_ffi::VAULT_BLOB_KEY.into()).unwrap();
    vault.lock();
    assert!(vault.is_locked());
    vault.from_backup("master-pw".into(), bytes).unwrap();
    let restored = vault.get_entry("acc-1".into()).unwrap();
    assert_eq!(restored.title, "示例站");

    // 回收站（A1.5）：软删 + 恢复。
    vault.delete_entry("acc-1".into()).unwrap();
    assert!(vault.get_entry("acc-1".into()).is_err());
    assert!(vault.trash().iter().any(|s| s.account_ref == "acc-1"));
    vault.restore("acc-1".into()).unwrap();
    assert!(vault.get_entry("acc-1".into()).is_ok());

    let _ = clock; // 时钟仅由 broker/sync 消费，vault 不读。
}

#[test]
fn broker_issue_is_udl_drift_renew_release_chain_works_on_listed_rows() {
    // UDL 漂移面（issue/pair/pack 等显式 DRIFT→INTERNAL）不在本测试范围——只锁可用链路：
    // renew/release 需要 issue 产生的 id，而 issue 是漂移面 → 验证错误形状。
    let (core, _store, _clock) = fixture_core();
    let broker = core.broker();

    let err = broker.issue(scope_spec(), 3600).unwrap_err();
    assert_eq!(err, ab_ffi::CoreError::INTERNAL, "issue = UDL 漂移面");
    let err = broker
        .renew("nope".into(), 600)
        .unwrap_err();
    assert!(matches!(err, ab_ffi::CoreError::NOT_FOUND));
    broker.list_sessions(); // 无 Throws：空表不 panic。
    broker.revoke_all();
    broker.sweep();
}

#[test]
fn audit_roundtrip_via_ffi_shapes() {
    let (core, _store, _clock) = fixture_core();
    let audit = core.audit();

    let event = ab_ffi::AuditEvent {
        action: AuditAction::ISSUE,
        result: AuditResult::OK,
        who: "test-actor".into(),
        account_ref: Some("acc-1".into()),
        domain: Some("example.test".into()),
        when: "2026-10-12T08:00:00Z".into(),
        detail: HashMap::new(),
    };
    audit.record_or_fail(event).unwrap();

    let rows = audit.query(AuditFilter::default());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].who, "test-actor");
    let text = format!("{rows:?}");
    assert!(!text.contains("master-pw"), "审计零秘密");
}

#[test]
fn broker_and_audit_share_one_sink() {
    // 审查报告 P1-6：broker 与 AuditLog 必须共用同一 sink，否则
    // audit.query 永远查不到 broker 侧的签发/吊销审计。
    let (core, _store, _clock) = fixture_core();
    let before = core.audit().query(AuditFilter::default()).len();

    // renew 不存在的会话 → broker 审计一条 Denied（fail-closed 链路）。
    assert!(core.broker().renew("nope".into(), 600).is_err());

    let after = core.audit().query(AuditFilter::default());
    assert_eq!(
        after.len(),
        before + 1,
        "broker 的审计事件必须出现在 audit.query 结果里"
    );
    assert_eq!(after.last().unwrap().action, AuditAction::RENEW);
}

#[test]
fn sync_unpack_rejects_garbage_and_state_is_payload_free() {
    let (core, _store, _clock) = fixture_core();
    // 垃圾字节：CONVERSION 错误，不 panic。
    assert!(core.sync().unpack(vec![0u8; 8]).is_err());
    // 状态矩阵：无会话 → 空表；快照列表空；回滚不存在 → 报错。
    assert!(core.state().matrix().is_empty());
    assert!(core.state().snapshot().is_empty());
    assert!(core.state().rollback("nope".into()).is_err());
    // UDL 漂移面显式失败（INTERNAL）。
    assert_eq!(core.sync().pair().unwrap_err(), ab_ffi::CoreError::INTERNAL);
    // 转换器三面（core 需注入验证器/沙盒/模块字节，UDL 未提供）→ INTERNAL。
    let manifest = ab_ffi::ConverterManifest {
        id: "qq".into(),
        name: "QQ".into(),
        version: "0.1.0".into(),
        level: ab_ffi::AccessLevel::C,
        readable_paths: vec!["{APPDATA}\\Tencent\\QQ\\session-store.json".into()],
        entry: "converter.wasm".into(),
        module_sha256: "0".repeat(64),
        signature: ab_ffi::Signature {
            algorithm: "ed25519".into(),
            signer: "test".into(),
            value: "0".repeat(128),
        },
        min_app_version: Some("0.1.0".into()),
    };
    assert_eq!(
        core.converter_host().verify(manifest).unwrap_err(),
        ab_ffi::CoreError::INTERNAL
    );
    assert!(core.converter_host().run_extract("qq".into(), "c".into()).is_err());
}
