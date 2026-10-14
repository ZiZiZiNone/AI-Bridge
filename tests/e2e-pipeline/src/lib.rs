//! `e2e-pipeline-tests` —— core 层跨端管线（tests/e2e-pipeline）。
//!
//! 覆盖 `docs/架构与契约.md` §5 四条关键数据流中**可 core 化**的部分：
//! - **流程 1（填充数据源）**：`ab-vault` 查询 + `ab-state` 匹配 → 结构化候选；
//! - **流程 3（AI 临时会话）**：`ab-state` 持有真会话 → `ab-mcp` `acquire_session`
//!   → `ab-broker` 代持 → list / renew / release 全链；
//! - **流程 4（跨设备扩散）**：两个 `ab-sync` 实例配对（指纹一致）→ `transfer`
//!   （注入假 [`ab_sync::Transport`]）→ 收端 `open` 得到属性完整的 `SessionEnvelope`。
//!
//! 结构验收口径（§5：「把 core 搬进单测，能否不改内部代码直接工作？」）：
//! 三条流全部直接构造 core 公开 API，只注入测试侧 `VaultStore` / `Clock` /
//! `SessionSource` / `Transport` / `Entropy` / `AuditSink`，不改 core 内部代码、
//! 不需要任何宿主（无 Tauri / HTTP / 浏览器）。
//!
//! 流程 2（跨浏览器桥）的扩展 `cookie_io` 与 bridge 协议在宿主层，不在 core 可测范围。
//!
//! 数据纪律：全部 fixture 假数据（`example.test` 域名、`fixture-` 前缀敏感值）。
#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use ab_audit::{AuditAction, AuditFilter, AuditResult, AuditSink, InMemoryAuditSink};
    use ab_broker::{BrokerService, UseAction, UseRequest};
    use ab_contract::time::format_rfc3339_secs;
    use ab_contract::{
        ClientEnv, Cookie, CredentialKind, Payload, PlainCredential, SameSite, SCHEMA_VERSION,
        SessionEnvelope, SessionKind, SessionRecord, Source, SourceType, Target, TargetKind,
    };
    use ab_crypto::{KdfParams, OsEntropy};
    use ab_mcp::{AcquireParams, McpScope, McpService, McpTarget, SessionIdParams};
    use ab_state::{
        Clock, Endpoint, EndpointKind, HealthPolicy, InMemoryStateStore, SessionEntry, SessionSource,
        StateError, StateService, StateStore,
    };
    use ab_sync::{DevicePlatform, SyncError, SyncIdentity, SyncService, Transport};
    use ab_vault::{VaultSession, VaultStore};

    const MASTER: [u8; 32] = [7u8; 32];
    const NOW: i64 = 1_791_504_000; // 2026-10-09T00:00:00Z，固定时刻
    const FAST_KDF: KdfParams = KdfParams { m_kib: 65536, t: 3, p: 1 };

    /* ------------------------- 注入面（单测 = 宿主的替身） ------------------------- */

    struct MemoryStore(Option<Vec<u8>>);
    impl VaultStore for MemoryStore {
        fn read(&mut self) -> Result<Option<Vec<u8>>, ab_vault::VaultError> {
            Ok(self.0.clone())
        }
        fn write(&mut self, data: &[u8]) -> Result<(), ab_vault::VaultError> {
            self.0 = Some(data.to_vec());
            Ok(())
        }
    }

    struct FixedClock(i64);
    impl Clock for FixedClock {
        fn now_unix_secs(&self) -> i64 {
            self.0
        }
    }

    struct NoSource;
    impl SessionSource for NoSource {
        fn read(&self, _endpoint: &Endpoint) -> Result<Vec<SessionRecord>, StateError> {
            Ok(Vec::new())
        }
    }

    /// 流程 4 的假传输信道：只存字节，不碰网络（`Transport` 由宿主注入的验收点）。
    #[derive(Default)]
    struct CaptureTransport(Mutex<Vec<Vec<u8>>>);
    impl CaptureTransport {
        fn take(&self) -> Vec<u8> {
            self.0.lock().unwrap().pop().expect("应有传输字节")
        }
    }
    impl Transport for CaptureTransport {
        fn send(&self, _device_id: &str, bytes: &[u8]) -> Result<(), SyncError> {
            self.0.lock().unwrap().push(bytes.to_vec());
            Ok(())
        }
    }

    fn state_service(
        entries: Vec<SessionEntry>,
        at: i64,
    ) -> StateService<InMemoryStateStore, FixedClock, NoSource> {
        let store = InMemoryStateStore::new();
        store.replace_sessions(entries).unwrap();
        StateService::new(store, FixedClock(at), NoSource, HealthPolicy::default())
    }

    fn cookie_payload() -> Payload {
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
        payload.local_storage.insert("ls_key".into(), "fixture-ls-value-1".into());
        payload
    }

    fn session_record(account_ref: &str, domain: &str, kind: SessionKind, payload: Payload) -> SessionRecord {
        SessionRecord {
            account_ref: account_ref.into(),
            target: Target {
                kind: TargetKind::Web,
                domain: Some(domain.into()),
                app_id: None,
            },
            kind,
            payload,
            expires_at: Some("2026-12-01T00:00:00Z".into()),
            source: Source {
                source_type: SourceType::Extension,
                id: "chromium".into(),
            },
            client_env: Some(ClientEnv {
                user_agent: "Mozilla/5.0 (Fake) E2ETest".into(),
                platform: "windows".into(),
                device_id: Some("fixture-device-a".into()),
            }),
        }
    }

    /* ================= 流程 1：填充数据源（vault 查询 + state 匹配 → 结构化候选） ================= */

    /// §5 流程 1（B5/B6/B7 的可 core 化部分）：宿主只把「目标域名」传进来，
    /// core 侧 `ab-vault` 查询 + `ab-state` 匹配直接产出结构化候选，零宿主对象。
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Candidate {
        account_ref: String,
        title: String,
        kind: CredentialKind,
        username: Option<String>,
        has_password: bool,
        has_totp: bool,
        /// 该账号在目标域名下的会话 kind（候选条的「可注入」依据）。
        session_kinds: Vec<String>,
    }

    #[test]
    fn flow1_vault_query_plus_state_match_yields_structured_candidates_without_host() {
        // —— ab-vault：解锁后的库（假数据：example.test + fixture- 前缀）——
        let entropy = OsEntropy;
        let mut session =
            VaultSession::create("填充源库", b"fixture-master-password", &FAST_KDF, &entropy).unwrap();
        for (title, account_ref, kind, username, secret, totp_seed, url) in [
            (
                "账号密码 <示例站>",
                "acc-1",
                CredentialKind::Password,
                Some("demo.user@example.test"),
                Some("fixture-password-1"),
                None,
                Some("https://console.example.test"),
            ),
            (
                "TOTP <示例站>",
                "acc-1",
                CredentialKind::Totp,
                Some("demo.user@example.test"),
                None,
                Some("fixture-totp-seed-1"),
                Some("https://console.example.test"),
            ),
            (
                "Passkey <示例站>",
                "acc-2",
                CredentialKind::Passkey,
                Some("other.user@example.test"),
                None,
                None,
                Some("https://console.example.test"),
            ),
            (
                "无关账号 <别的站>",
                "acc-3",
                CredentialKind::Password,
                Some("demo.user@other.example.test"),
                Some("fixture-password-3"),
                None,
                Some("https://console.other.example.test"),
            ),
        ] {
            let mut entry = ab_vault::Entry::new(title, account_ref, kind, &entropy).unwrap();
            entry.username = username.map(str::to_string);
            entry.secret = secret.map(str::to_string);
            entry.totp_seed = totp_seed.map(str::to_string);
            entry.url = url.map(str::to_string);
            session.vault.add(entry).unwrap();
        }
        // 存取经 VaultStore 注入（宿主 = 文件 / 卶测 = 内存）：保存并重新解锁后查询。
        let mut store = MemoryStore(None);
        session.save(&mut store, &entropy).unwrap();
        let vault = VaultSession::unlock(&mut store, b"fixture-master-password").unwrap();

        // —— ab-state：目标域名下的真会话（含多账号）——
        let mut token_payload = Payload::default();
        token_payload.token = Some("fixture-token-access-1".into());
        let mut cred_payload = Payload::default();
        cred_payload.credential = Some(PlainCredential {
            username: "other.user@example.test".into(),
            secret: "fixture-password-2".into(),
        });
        let state = state_service(
            vec![
                SessionEntry::new(
                    Endpoint::new(EndpointKind::Browser, "chromium"),
                    session_record("acc-1", "example.test", SessionKind::Cookie, cookie_payload()),
                ),
                SessionEntry::new(
                    Endpoint::new(EndpointKind::Desktop, "fixture-desktop-app"),
                    session_record("acc-1", "example.test", SessionKind::Token, token_payload),
                ),
                SessionEntry::new(
                    Endpoint::new(EndpointKind::Browser, "chromium"),
                    session_record("acc-2", "example.test", SessionKind::Credential, cred_payload),
                ),
            ],
            NOW,
        );

        // —— 组合：vault 查询 + state 匹配 → 结构化候选（薄入口只转发，业务判定在 core）——
        let target = Target {
            kind: TargetKind::Web,
            domain: Some("example.test".into()),
            app_id: None,
        };
        let sessions = state.sessions_for_target(&target).unwrap();
        let mut kinds_by_account: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in &sessions {
            kinds_by_account
                .entry(entry.record.account_ref.clone())
                .or_default()
                .push(format!("{:?}", entry.record.kind).to_lowercase());
        }

        let mut candidates: Vec<Candidate> = vault
            .vault
            .entries()
            .into_iter()
            .filter_map(|entry| {
                let session_kinds = kinds_by_account.get(&entry.account_ref)?.clone();
                Some(Candidate {
                    account_ref: entry.account_ref.clone(),
                    title: entry.title.clone(),
                    kind: entry.kind,
                    username: entry.username.clone(),
                    has_password: entry.secret.is_some(),
                    has_totp: entry.totp_seed.is_some(),
                    session_kinds,
                })
            })
            .collect();
        candidates.sort_by(|a, b| (&a.account_ref, &a.title).cmp(&(&b.account_ref, &b.title)));

        let mut expected = vec![
            Candidate {
                account_ref: "acc-1".into(),
                title: "TOTP <示例站>".into(),
                kind: CredentialKind::Totp,
                username: Some("demo.user@example.test".into()),
                has_password: false,
                has_totp: true,
                session_kinds: vec!["cookie".into(), "token".into()],
            },
            Candidate {
                account_ref: "acc-1".into(),
                title: "账号密码 <示例站>".into(),
                kind: CredentialKind::Password,
                username: Some("demo.user@example.test".into()),
                has_password: true,
                has_totp: false,
                session_kinds: vec!["cookie".into(), "token".into()],
            },
            Candidate {
                account_ref: "acc-2".into(),
                title: "Passkey <示例站>".into(),
                kind: CredentialKind::Passkey,
                username: Some("other.user@example.test".into()),
                has_password: false,
                has_totp: false,
                session_kinds: vec!["credential".into()],
            },
        ];
        expected.sort_by(|a, b| (&a.account_ref, &a.title).cmp(&(&b.account_ref, &b.title)));

        assert_eq!(
            candidates, expected,
            "流程 1：vault 查询 + state 匹配必须产出与输入逐字段一致的结构化候选"
        );

        // 匹配边界：目标域名无会话 → 无候选；无关账号 acc-3 不得混入。
        let other = Target {
            kind: TargetKind::Web,
            domain: Some("nothing.example.test".into()),
            app_id: None,
        };
        assert!(state.sessions_for_target(&other).unwrap().is_empty());
        assert!(candidates.iter().all(|c| c.account_ref != "acc-3"));
    }

    /* ================= 流程 3：AI 临时会话（state → ab-mcp → ab-broker 代持） ================= */

    /// §5 流程 3（C1–C5）：state 持有真会话 → `McpService::acquire_session` →
    /// broker 代持真会话、只发不透明句柄 → `list_sessions` / `renew_session` /
    /// `release_session` 全链；AI 侧任何输出都拿不到真实凭据（C2.1）。
    #[test]
    fn flow3_ai_temp_session_acquire_list_renew_release_chain() {
        let audit = Arc::new(InMemoryAuditSink::new());
        let broker = Arc::new(BrokerService::new(audit.clone(), &MASTER).unwrap());
        let state = state_service(
            vec![SessionEntry::new(
                Endpoint::new(EndpointKind::Browser, "chromium"),
                session_record("acc-1", "example.test", SessionKind::Cookie, cookie_payload()),
            )],
            NOW,
        );
        let service = McpService::new(
            broker.clone(),
            Arc::new(state),
            Arc::new(FixedClock(NOW)),
            Arc::new(OsEntropy),
            "fixture-ai-client",
        );

        // —— acquire_session：签发临时会话，返回一次性不透明句柄 ——
        let grant = service
            .acquire_session(&AcquireParams {
                target: McpTarget {
                    kind: "web".into(),
                    domain: Some("example.test".into()),
                    app_id: None,
                },
                scope: McpScope {
                    domain_allowlist: vec!["example.test".into()],
                    read_only: true,
                    single_use: false,
                },
                ttl_secs: None,
                account_ref: None,
            })
            .unwrap();
        assert_eq!(grant.account_ref, "acc-1");
        assert_eq!(grant.handle.len(), 64, "不透明句柄 = 32 随机字节 hex");
        assert!(grant.handle.chars().all(|c| c.is_ascii_hexdigit()));
        let grant_json = serde_json::to_string(&grant).unwrap();
        assert!(
            !grant_json.contains("fixture-cookie-sid-1") && !grant_json.contains("payload"),
            "AI 拿到的签发结果不得携带真会话载荷：{grant_json}"
        );

        // —— broker 代持：真会话只在代持通道（UsePermit）里出现 ——
        let permit = broker
            .authorize(
                "fixture-ai-client",
                UseRequest {
                    handle: &grant.handle,
                    domain: Some("example.test"),
                    action: UseAction::Read,
                },
                NOW + 10,
            )
            .unwrap();
        assert_eq!(permit.session_id, grant.session_id);
        assert_eq!(
            permit.envelope.payload.cookies[0].value, "fixture-cookie-sid-1",
            "真会话由 broker 代持（代持通道可见，AI 侧不可见）"
        );

        // —— list_sessions：剩余 TTL / scope / 状态，不含句柄 ——
        let list = service.list_sessions().unwrap();
        assert_eq!(list.sessions.len(), 1);
        let row = &list.sessions[0];
        assert_eq!(row.session_id, grant.session_id);
        assert_eq!(row.remaining_ttl_secs, 3600, "TTL 缺省 1h（C2.2）");
        assert_eq!(row.state, "active");
        assert!(row.scope.read_only && row.scope.domain_allowlist == ["example.test"]);
        assert!(!serde_json::to_string(row).unwrap().contains(&grant.handle));

        // —— renew_session：上限内续期 ——
        let renewed = service
            .renew_session(&SessionIdParams {
                session_id: grant.session_id.clone(),
                ttl_secs: Some(600),
            })
            .unwrap();
        assert_eq!(renewed.expires_at, format_rfc3339_secs(NOW + 600));
        assert_eq!(renewed.remaining_ttl_secs, 600);

        // —— release_session：显式释放，只失效我方凭证 ——
        service.release_session(&grant.session_id).unwrap();
        assert_eq!(service.list_sessions().unwrap().sessions[0].state, "released");
        assert_eq!(
            broker
                .authorize(
                    "fixture-ai-client",
                    UseRequest {
                        handle: &grant.handle,
                        domain: Some("example.test"),
                        action: UseAction::Read,
                    },
                    NOW + 20,
                )
                .unwrap_err(),
            ab_broker::BrokerError::NotActive,
            "释放后句柄立即作废"
        );

        // —— 审计流（C3.1）：Issue/Use/Renew/Release 全链入账，且不含秘密与句柄 ——
        let events = audit.query(&AuditFilter::default()).unwrap();
        let ok_actions: Vec<AuditAction> = events
            .iter()
            .filter(|e| e.result == AuditResult::Ok)
            .map(|e| e.action)
            .collect();
        assert_eq!(
            ok_actions,
            vec![
                AuditAction::Issue,
                AuditAction::Use,
                AuditAction::Renew,
                AuditAction::Release
            ]
        );
        let audit_json = serde_json::to_string(&events).unwrap();
        assert!(!audit_json.contains("fixture-cookie-sid-1"), "审计不得含秘密");
        assert!(!audit_json.contains(&grant.handle), "审计不得含句柄");
    }

    /* ================= 流程 4：跨设备扩散（ab-sync 配对 → transfer → open） ================= */

    /// §5 流程 4（E1/E2/E3）：两个 `ab-sync` 实例配对、双方指纹一致后，
    /// `transfer` 经注入的假 `Transport` 送出，收端 `receive`（open）得到
    /// **属性完整**的 `SessionEnvelope`（cookie 完整属性零丢弃，A2 保真度）。
    #[test]
    fn flow4_cross_device_transfer_opens_attribute_complete_envelope() {
        let a = SyncService::new(
            SyncIdentity::generate("fixture-device-a", &OsEntropy).unwrap(),
            vec![],
        );
        let b = SyncService::new(
            SyncIdentity::generate("fixture-device-b", &OsEntropy).unwrap(),
            vec![],
        );

        // —— E3 配对：双方独立计算的指纹必须一致 ——
        let dev_b = a
            .pair(&b.identity().offer("Fixture Phone", DevicePlatform::Ios), NOW)
            .unwrap();
        let dev_a = b
            .pair(&a.identity().offer("Fixture PC", DevicePlatform::Windows), NOW)
            .unwrap();
        assert_eq!(dev_a.fingerprint, dev_b.fingerprint, "E3：双方指纹必须一致");
        a.confirm_pairing(&dev_b).unwrap();
        b.confirm_pairing(&dev_a).unwrap();

        // —— 扩散步：属性完整的 SessionEnvelope（cookie 全属性 + local_storage + client_env）——
        let mut payload = cookie_payload();
        payload.token = Some("fixture-token-access-1".into());
        let env = SessionEnvelope {
            schema_version: SCHEMA_VERSION.to_string(),
            account_ref: "acc-1".into(),
            target: Target {
                kind: TargetKind::Web,
                domain: Some("example.test".into()),
                app_id: None,
            },
            kind: SessionKind::Cookie,
            payload,
            issued_at: "2026-10-09T00:00:00Z".into(),
            expires_at: Some("2026-12-01T00:00:00Z".into()),
            source: Source {
                source_type: SourceType::Extension,
                id: "chromium".into(),
            },
            scope: None,
            client_env: Some(ClientEnv {
                user_agent: "Mozilla/5.0 (Fake) E2ETest".into(),
                platform: "windows".into(),
                device_id: Some("fixture-device-a".into()),
            }),
        };

        let transport = CaptureTransport::default();
        a.transfer("fixture-device-b", &env, &transport, &OsEntropy).unwrap();

        // —— 收端 open：信封逐字段一致，cookie 属性一项不丢 ——
        let got = b.receive("fixture-device-a", &transport.take()).unwrap();
        assert_eq!(got, env, "收端 open 的信封必须与发送端逐字段一致");

        let want_cookie = &env.payload.cookies[0];
        let got_cookie = &got.payload.cookies[0];
        assert_eq!(got.account_ref, env.account_ref);
        assert_eq!(got.target, env.target);
        assert_eq!(got.kind, env.kind);
        assert_eq!(got.issued_at, env.issued_at);
        assert_eq!(got.expires_at, env.expires_at);
        assert_eq!(got.source, env.source);
        assert_eq!(got.client_env, env.client_env);
        assert_eq!(got.payload.local_storage, env.payload.local_storage);
        assert_eq!(got.payload.token, env.payload.token);
        assert_eq!(got_cookie.name, want_cookie.name, "cookie name 属性完整");
        assert_eq!(got_cookie.value, want_cookie.value, "cookie value 属性完整");
        assert_eq!(got_cookie.domain, want_cookie.domain, "cookie domain 属性完整");
        assert_eq!(got_cookie.path, want_cookie.path, "cookie path 属性完整");
        assert_eq!(got_cookie.secure, want_cookie.secure, "cookie secure 属性完整");
        assert_eq!(got_cookie.http_only, want_cookie.http_only, "cookie http_only 属性完整");
        assert_eq!(got_cookie.same_site, want_cookie.same_site, "cookie same_site 属性完整");
        assert_eq!(got_cookie.expires_at, want_cookie.expires_at, "cookie expires_at 属性完整");
    }
}
