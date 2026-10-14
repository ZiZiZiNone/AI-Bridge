//! `recovery-drill-tests` —— 恢复演练（tests/recovery-drill）。
//!
//! 回答三件事：
//! 1. **A6.8 恢复演练**：备份字节在**全新 store** 里沙盒恢复后，条目数与逐条完整性
//!    （uuid + 每个字段）必须与演练前一致；
//! 2. **安全模型 §3**：「解密失败/校验失败一律整体拒绝，**不输出半解密内容**」——
//!    损坏备份（翻转字节）与错误口令都必须整体拒绝，错误输出不得带出任何秘密字段；
//! 3. **A6.9 / A8.3 导入支线**：`ab-sync` 的 `OfflinePack` 过期拒解 + 篡改拒解。
//!
//! 结构验收（`docs/架构与契约.md` §5）：全部逻辑直接搬进单测，
//! 只注入 [`ab_vault::VaultStore`] 与 [`ab_sync::Transport`] / [`ab_crypto::Entropy`]，
//! 不改 core 内部代码、不起任何宿主。
//!
//! 数据纪律：全部 fixture 假数据（`example.test` 域名、`fixture-` 前缀敏感值），
//! 无真实凭据。
#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    use ab_contract::{
        Cookie, CredentialKind, Payload, SameSite, SCHEMA_VERSION, SessionEnvelope, SessionKind,
        Source, SourceType, Target, TargetKind,
    };
    use ab_crypto::{Entropy, KdfParams, OsEntropy};
    use ab_sync::{DevicePlatform, SyncError, SyncIdentity, SyncService};
    use ab_vault::{Entry, KdbxError, VaultError, VaultSession, VaultStore};

    /// 演练用 KDF 参数：与 `core/vault` 自测一致（不低于 `ab-crypto` 安全基线）。
    const FAST_KDF: KdfParams = KdfParams { m_kib: 65536, t: 3, p: 1 };

    const PASSWORD: &[u8] = b"fixture-master-password";
    const WRONG_PASSWORD: &[u8] = b"fixture-wrong-password";

    // ============ 注入面：内存存储（宿主 = 文件，单测 = 内存，核心只认字节） ============

    #[derive(Default)]
    struct MemoryStore(Option<Vec<u8>>);

    impl MemoryStore {
        fn from_bytes(bytes: &[u8]) -> Self {
            MemoryStore(Some(bytes.to_vec()))
        }

        fn bytes(&self) -> Option<&[u8]> {
            self.0.as_deref()
        }
    }

    impl VaultStore for MemoryStore {
        fn read(&mut self) -> Result<Option<Vec<u8>>, VaultError> {
            Ok(self.0.clone())
        }

        fn write(&mut self, data: &[u8]) -> Result<(), VaultError> {
            self.0 = Some(data.to_vec());
            Ok(())
        }
    }

    // ============ 演练样本：多条目（passkey / TOTP / 备注 / 自定义字段） ============

    /// 自定义字段口径（`docs/安全模型.md` §3）：KDBX4 标准字段
    /// （Title/UserName/Password/URL/Notes）之外，自定义字段为 account_ref / kind / totp_seed。
    /// 覆盖：passkey 条目、TOTP 条目（totp_seed 自定义字段）、备注条目（Notes）、
    /// 账号密码条目（Password + 全字段）。
    fn drill_entries(entropy: &dyn Entropy) -> Vec<Entry> {
        let mut passkey = Entry::new(
            "Passkey <示例站>",
            "acc-passkey-1",
            CredentialKind::Passkey,
            entropy,
        )
        .unwrap();
        passkey.username = Some("demo.user@example.test".into());
        passkey.url = Some("https://portal.example.test/login".into());
        passkey.note = Some("fixture-note-passkey".into());

        let mut totp = Entry::new("TOTP <示例站>", "acc-totp-1", CredentialKind::Totp, entropy).unwrap();
        totp.username = Some("demo.user@example.test".into());
        totp.secret = Some("fixture-password-2".into());
        totp.totp_seed = Some("fixture-totp-seed-1".into());
        totp.url = Some("https://portal.example.test".into());

        let mut note = Entry::new("备注条目 <note>", "acc-note-1", CredentialKind::Password, entropy).unwrap();
        note.note = Some("fixture-note-长备注：仅供恢复演练比对，无真实内容".into());

        let mut password = Entry::new(
            "账号密码 <示例站>",
            "acc-password-1",
            CredentialKind::Password,
            entropy,
        )
        .unwrap();
        password.username = Some("demo.user@example.test".into());
        password.secret = Some("fixture-password-1".into());
        password.url = Some("https://console.example.test".into());

        vec![passkey, totp, note, password]
    }

    /// 备份字节 = `VaultSession::save` 的产物（A6.1/A6.4：备份即加密容器字节）。
    fn backup_bytes() -> (Vec<Entry>, Vec<u8>) {
        let entropy = OsEntropy;
        let mut session =
            VaultSession::create("恢复演练库", PASSWORD, &FAST_KDF, &entropy).unwrap();
        let entries = drill_entries(&entropy);
        for entry in entries.clone() {
            session.vault.add(entry).unwrap();
        }
        let mut store = MemoryStore::default();
        session.save(&mut store, &entropy).unwrap();
        (entries, store.bytes().expect("save 后应有备份字节").to_vec())
    }

    /// 逐条逐字段比对（A6.8「完整性」= uuid + 每个字段逐一相等）。
    fn assert_entry_eq(label: &str, restored: Option<&Entry>, want: &Entry) {
        let got = restored.unwrap_or_else(|| panic!("{label}: 条目丢失，uuid = {:?}", want.uuid));
        assert_eq!(got.uuid, want.uuid, "{label}: uuid 不一致");
        assert_eq!(got.title, want.title, "{label}: title 不一致");
        assert_eq!(got.account_ref, want.account_ref, "{label}: account_ref 不一致");
        assert_eq!(got.kind, want.kind, "{label}: kind 不一致");
        assert_eq!(got.username, want.username, "{label}: username 不一致");
        assert_eq!(got.secret, want.secret, "{label}: secret 不一致");
        assert_eq!(got.totp_seed, want.totp_seed, "{label}: totp_seed 不一致");
        assert_eq!(got.url, want.url, "{label}: url 不一致");
        assert_eq!(got.note, want.note, "{label}: note 不一致");
    }

    fn assert_no_secret_leak(text: &str) {
        for secret in [
            "fixture-password-1",
            "fixture-password-2",
            "fixture-totp-seed-1",
            "fixture-note-passkey",
            "fixture-note-长备注",
            "demo.user@example.test",
        ] {
            assert!(!text.contains(secret), "输出泄漏秘密字段：{secret}\n{text}");
        }
    }

    // ============ 1. A6.8 沙盒恢复：条目数 + 逐条完整性 ============

    /// 备份字节 → 全新 store → unlock：条目数一致、每条 uuid 与全部字段逐一相等；
    /// 演练不改动原始备份（沙盒语义）。
    #[test]
    fn sandbox_restore_reproduces_entry_count_and_full_fidelity() {
        let (entries, backup) = backup_bytes();
        assert_eq!(entries.len(), 4, "演练样本应覆盖 passkey/TOTP/备注/账号密码 四类");

        // 沙盒：全新 store，只喂备份字节；原始备份不动。
        let mut sandbox = MemoryStore::from_bytes(&backup);
        let restored = VaultSession::unlock(&mut sandbox, PASSWORD).unwrap();

        assert_eq!(
            restored.vault.entries().len(),
            entries.len(),
            "A6.8：恢复后条目数必须一致"
        );
        assert!(restored.vault.trash_entries().is_empty(), "回收站应为空");
        assert_eq!(restored.vault.name, "恢复演练库", "库名随备份恢复");

        for want in &entries {
            let got = restored.vault.find(&want.uuid);
            assert_entry_eq("恢复条目", got, want);
        }

        // 沙盒恢复不回头写原始备份：字节不变。
        assert_eq!(sandbox.bytes(), Some(backup.as_slice()), "沙盒不得回写备份");

        // 演练后的 store 仍然是一份可用的独立备份（再来一轮解锁比对）。
        let reopened = VaultSession::unlock(&mut sandbox, PASSWORD).unwrap();
        assert_eq!(reopened.vault.entries().len(), entries.len());
        for want in &entries {
            assert_entry_eq("二次恢复", reopened.vault.find(&want.uuid), want);
        }
        assert_eq!(restored.vault.entries().len(), 4);
    }

    // ============ 2. 安全模型 §3：损坏备份整体拒绝、不输出半解密 ============

    /// 翻转任意位置字节后 unlock 必须整体失败：不允许返回任何「部分解密」的库，
    /// 错误输出不得带出条目秘密。
    #[test]
    fn corrupted_backup_rejected_whole_without_partial_output() {
        let (entries, backup) = backup_bytes();
        let len = backup.len();
        assert!(len > 32, "备份字节过短：{len}");

        for offset in [0usize, 1, 16, len / 4, len / 2, len * 3 / 4, len - 1] {
            let mut corrupted = backup.clone();
            corrupted[offset] ^= 0x5a;
            assert_ne!(corrupted, backup, "翻转应真的改动字节");

            let mut store = MemoryStore::from_bytes(&corrupted);
            let result = VaultSession::unlock(&mut store, PASSWORD);
            match result {
                Ok(_) => panic!("损坏备份（偏移 {offset}）必须整体拒绝，不得解出任何内容"),
                Err(err) => {
                    // 不输出半解密：错误输出里没有任何条目字段。
                    assert_no_secret_leak(&format!("{err:?}"));
                    assert_no_secret_leak(&err.to_string());
                }
            }
        }

        // 对照：完好备份仍可恢复出同样条目（证明拒绝来自损坏而非样本问题）。
        let mut store = MemoryStore::from_bytes(&backup);
        let restored = VaultSession::unlock(&mut store, PASSWORD).unwrap();
        assert_eq!(restored.vault.entries().len(), entries.len());
    }

    // ============ 3. 错误口令整体拒绝 ============

    #[test]
    fn wrong_password_rejected_without_leaking() {
        let (_, backup) = backup_bytes();
        let mut store = MemoryStore::from_bytes(&backup);

        for wrong in [WRONG_PASSWORD, b"", b"fixture-master-password-wrong"] {
            let err = VaultSession::unlock(&mut store, wrong).unwrap_err();
            assert!(
                matches!(err, VaultError::Kdbx(KdbxError::BadCredentials)),
                "错误口令应报 BadCredentials，实际 {err:?}"
            );
            assert_no_secret_leak(&format!("{err:?}"));
        }

        // 错口令全部拒绝后，正确口令仍可开（口令判定不污染存储）。
        let restored = VaultSession::unlock(&mut store, PASSWORD).unwrap();
        assert_eq!(restored.vault.entries().len(), 4);
    }

    // ============ 4. A6.9 / A8.3：离线传送包导入支线（ab-sync） ============

    fn fixture_envelope() -> SessionEnvelope {
        SessionEnvelope {
            schema_version: SCHEMA_VERSION.to_string(),
            account_ref: "acc-1".into(),
            target: Target {
                kind: TargetKind::Web,
                domain: Some("example.test".into()),
                app_id: None,
            },
            kind: SessionKind::Cookie,
            payload: {
                let mut payload = Payload::default();
                payload.cookies.push(Cookie {
                    name: "sid".into(),
                    value: "fixture-cookie-sid-1".into(),
                    domain: Some(".example.test".into()),
                    path: Some("/app".into()),
                    secure: Some(true),
                    http_only: Some(true),
                    same_site: Some(SameSite::Lax),
                    expires_at: Some("2026-12-01T00:00:00Z".into()),
                });
                payload
            },
            issued_at: "2026-10-09T00:00:00Z".into(),
            expires_at: Some("2026-12-01T00:00:00Z".into()),
            source: Source {
                source_type: SourceType::Sync,
                id: "fixture-device-a".into(),
            },
            scope: None,
            client_env: None,
        }
    }

    fn paired_services() -> (SyncService, SyncService) {
        let a = SyncService::new(
            SyncIdentity::generate("fixture-device-a", &OsEntropy).unwrap(),
            vec![],
        );
        let b = SyncService::new(
            SyncIdentity::generate("fixture-device-b", &OsEntropy).unwrap(),
            vec![],
        );
        let dev_b = a
            .pair(&b.identity().offer("Fixture Phone", DevicePlatform::Ios), 1_000)
            .unwrap();
        let dev_a = b
            .pair(&a.identity().offer("Fixture PC", DevicePlatform::Windows), 1_000)
            .unwrap();
        assert_eq!(dev_a.fingerprint, dev_b.fingerprint, "E3：双方指纹必须一致");
        a.confirm_pairing(&dev_b).unwrap();
        b.confirm_pairing(&dev_a).unwrap();
        (a, b)
    }

    /// A8.1/A8.3：有效期内可解且逐字段一致；过期拒解；篡改（密文 / nonce / 时间戳）拒解；
    /// 非收件设备拒解——一律整体拒绝，无半解密输出。
    #[test]
    fn offline_pack_import_rejects_expired_and_tampered() {
        let (a, b) = paired_services();
        let env = fixture_envelope();
        let pack = a
            .pack("fixture-device-b", &env, 3600, 1_000, &OsEntropy)
            .unwrap();
        assert_eq!(pack.expires_at_secs, 4_600, "有效期由导出方设定（A8.3）");

        // 基线：有效期内导入成功，信封逐字段一致。
        let got = b.unpack(&pack, 1_500).unwrap();
        assert_eq!(got, env, "导入产物必须与导出信封一致");

        // 过期拒解（A8.3）：到期时刻与到期后一律 Expired。
        for now in [4_600, 4_601, 9_999] {
            let err = b.unpack(&pack, now).unwrap_err();
            assert_eq!(err, SyncError::Expired, "过期包（now={now}）必须拒解");
        }

        // 篡改拒解：密文 / nonce / 时间戳（AEAD 绑定时间戳 = 签名时间戳语义）。
        let mut bad_ct = pack.clone();
        bad_ct.sealed.ciphertext[0] ^= 0x01;
        assert!(matches!(b.unpack(&bad_ct, 1_500), Err(SyncError::Crypto(_))), "篡改密文必须拒解");

        let mut bad_nonce = pack.clone();
        bad_nonce.sealed.nonce[0] ^= 0x01;
        assert!(matches!(b.unpack(&bad_nonce, 1_500), Err(SyncError::Crypto(_))), "篡改 nonce 必须拒解");

        let mut extended = pack.clone();
        extended.expires_at_secs = 99_999;
        assert!(
            matches!(b.unpack(&extended, 1_500), Err(SyncError::Crypto(_))),
            "回拨/延长有效期必须被时间戳绑定拦下"
        );

        let mut rewound = pack.clone();
        rewound.issued_at_secs = 0;
        assert!(matches!(b.unpack(&rewound, 1_500), Err(SyncError::Crypto(_))), "篡改签发时间必须拒解");

        // 非收件设备的私钥解不开（A8.1 对端公钥加密）。
        let outsider = SyncService::new(
            SyncIdentity::generate("fixture-device-c", &OsEntropy).unwrap(),
            vec![],
        );
        assert!(
            matches!(outsider.unpack(&pack, 1_500), Err(SyncError::Crypto(_))),
            "非收件设备必须拒解"
        );

        // 拒绝路径全部不产出信封（无半解密）：错误输出不含任何秘密字段。
        for err in [
            b.unpack(&pack, 9_999).unwrap_err(),
            b.unpack(&bad_ct, 1_500).unwrap_err(),
            b.unpack(&extended, 1_500).unwrap_err(),
        ] {
            assert_no_secret_leak(&format!("{err:?}"));
            assert_no_secret_leak(&err.to_string());
        }
    }
}
