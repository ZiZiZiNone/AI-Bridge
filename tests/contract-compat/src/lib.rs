//! `contract-compat-tests` —— 契约向后兼容锁（tests/contract-compat）。
//!
//! 回答唯一问题：**旧数据在新代码下是否仍可用**。对应 `docs/架构与契约.md` §6：
//! 「版本化：`schema_version` 必填；新增字段一律可选；删除/改语义升主版本。
//! `tests/contract-compat` 锁定向后兼容」。
//!
//! - fixture 全部为假数据（example.com / fake-*），不含真实凭据；
//! - `contracts/*.schema.json` 经 `include_str!` 编译期嵌入，与 Rust 结构序列化结果对拍，
//!   锁死「schema 与代码不漂移」；
//! - 只读 `core/contract` 与 `contracts/`，不修改二者。
#![forbid(unsafe_code)]

#[cfg(test)]
mod tests {
    use ab_contract::*;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};
    use std::collections::{BTreeMap, BTreeSet};

    // ---- 唯一事实源：contracts/*.schema.json（编译期嵌入）----
    const SCHEMA_STANDARD: &str = include_str!("../../../contracts/standard-access.schema.json");
    const SCHEMA_MANIFEST: &str = include_str!("../../../contracts/converter-manifest.schema.json");
    const SCHEMA_ENVELOPE: &str = include_str!("../../../contracts/session-envelope.schema.json");

    // ============ fixture：v1.0 旧数据（全部假数据） ============

    /// 最小 StandardAccess：只含 required 四字段，其余可选字段全省。
    const SA_MIN: &str = r#"{
      "schema_version": "1.0",
      "app_id": "qq",
      "level": "c",
      "generated_at": "2026-10-09T00:00:00Z"
    }"#;

    /// 全字段 StandardAccess：三个 session 覆盖 cookie / token / credential 三种载荷。
    const SA_FULL: &str = r#"{
      "schema_version": "1.0",
      "app_id": "github",
      "level": "b",
      "generated_at": "2026-10-09T00:00:00Z",
      "credentials": [
        {
          "account_ref": "acc-1",
          "kind": "totp",
          "username": "demo-user",
          "secret": "fake-secret-not-real",
          "totp_seed": "FAKESEEDJBSWY3DP",
          "url": "https://example.com/login"
        }
      ],
      "sessions": [
        {
          "account_ref": "acc-1",
          "target": { "kind": "web", "domain": "example.com" },
          "kind": "cookie",
          "payload": {
            "cookies": [
              {
                "name": "sid",
                "value": "fake-cookie-value",
                "domain": "example.com",
                "path": "/",
                "secure": true,
                "http_only": true,
                "same_site": "lax",
                "expires_at": "2026-12-01T00:00:00Z"
              }
            ],
            "local_storage": { "ls_key": "fake-ls-value" }
          },
          "expires_at": "2026-12-01T00:00:00Z",
          "source": { "type": "extension", "id": "chromium" },
          "client_env": {
            "user_agent": "Mozilla/5.0 (Fake) CompatTest",
            "platform": "windows",
            "device_id": "fake-device-1"
          }
        },
        {
          "account_ref": "acc-1",
          "target": { "kind": "app", "app_id": "qq" },
          "kind": "token",
          "payload": { "token": "fake-token-value" },
          "source": { "type": "converter", "id": "qq" }
        },
        {
          "account_ref": "acc-2",
          "target": { "kind": "container" },
          "kind": "credential",
          "payload": { "credential": { "username": "demo-user", "secret": "fake-secret-not-real" } },
          "source": { "type": "manual", "id": "hand-entry" }
        }
      ],
      "meta": { "converter_version": "fake-1.0.0", "note": "contract-compat fixture" },
      "client_env": {
        "user_agent": "Mozilla/5.0 (Fake) CompatTest",
        "platform": "windows",
        "device_id": "fake-device-1"
      }
    }"#;

    /// 最小 ConverterManifest：只含 required 字段（min_app_version 全省）。
    const MANIFEST_MIN: &str = r#"{
      "id": "qq",
      "name": "QQ 假转换器",
      "version": "1.2.0",
      "level": "c",
      "readable_paths": ["C:\\Users\\demo\\AppData\\Roaming\\Tencent\\QQ"],
      "entry": "qq.wasm",
      "module_sha256": "3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788",
      "signature": {
        "algorithm": "ed25519",
        "signer": "official-fixture",
        "value": "6f1d0b2a4c8e7395a0b1c2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718899aabbccddeeff00112233445566778899aabbccddeeff0011223344556677"
      }
    }"#;

    /// 全字段 ConverterManifest。
    const MANIFEST_FULL: &str = r#"{
      "id": "qq",
      "name": "QQ 假转换器",
      "version": "1.2.0",
      "level": "c",
      "readable_paths": ["C:\\Users\\demo\\AppData\\Roaming\\Tencent\\QQ"],
      "entry": "qq.wasm",
      "module_sha256": "3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788",
      "signature": {
        "algorithm": "ed25519",
        "signer": "official-fixture",
        "value": "6f1d0b2a4c8e7395a0b1c2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718899aabbccddeeff00112233445566778899aabbccddeeff0011223344556677"
      },
      "min_app_version": "2.0.0"
    }"#;

    /// 最小 SessionEnvelope（cookie 载荷）：Cookie 只剩 name/value 两个必填。
    const ENV_MIN_COOKIE: &str = r#"{
      "schema_version": "1.0",
      "account_ref": "acc-1",
      "target": { "kind": "web", "domain": "example.com" },
      "kind": "cookie",
      "payload": { "cookies": [ { "name": "sid", "value": "fake-cookie-value" } ] },
      "issued_at": "2026-10-09T00:00:00Z",
      "source": { "type": "sync", "id": "device-a" }
    }"#;

    /// 最小 SessionEnvelope（token 载荷）。
    const ENV_MIN_TOKEN: &str = r#"{
      "schema_version": "1.0",
      "account_ref": "acc-1",
      "target": { "kind": "app", "app_id": "qq" },
      "kind": "token",
      "payload": { "token": "fake-token-value" },
      "issued_at": "2026-10-09T00:00:00Z",
      "source": { "type": "converter", "id": "qq" }
    }"#;

    /// 最小 SessionEnvelope（credential 载荷）。
    const ENV_MIN_CRED: &str = r#"{
      "schema_version": "1.0",
      "account_ref": "acc-2",
      "target": { "kind": "container" },
      "kind": "credential",
      "payload": { "credential": { "username": "demo-user", "secret": "fake-secret-not-real" } },
      "issued_at": "2026-10-09T00:00:00Z",
      "source": { "type": "manual", "id": "hand-entry" }
    }"#;

    /// 全字段 SessionEnvelope：含 scope / client_env / expires_at，可当临时会话用。
    const ENV_FULL: &str = r#"{
      "schema_version": "1.0",
      "account_ref": "acc-1",
      "target": { "kind": "web", "domain": "example.com", "app_id": "qq" },
      "kind": "cookie",
      "payload": {
        "cookies": [
          {
            "name": "sid",
            "value": "fake-cookie-value",
            "domain": "example.com",
            "path": "/",
            "secure": true,
            "http_only": true,
            "same_site": "lax",
            "expires_at": "2026-12-01T00:00:00Z"
          }
        ],
        "token": "fake-token-value",
        "credential": { "username": "demo-user", "secret": "fake-secret-not-real" },
        "local_storage": { "ls_key": "fake-ls-value" }
      },
      "issued_at": "2026-10-09T00:00:00Z",
      "expires_at": "2026-11-01T00:00:00Z",
      "source": { "type": "extension", "id": "chromium" },
      "scope": { "domain_allowlist": ["example.com"], "read_only": true, "single_use": true },
      "client_env": {
        "user_agent": "Mozilla/5.0 (Fake) CompatTest",
        "platform": "windows",
        "device_id": "fake-device-1"
      }
    }"#;

    // ============ 工具 ============

    fn de<T: DeserializeOwned>(json: &str) -> T {
        serde_json::from_str(json).expect("fixture 应能被 serde 反序列化（旧数据兼容）")
    }

    fn schema(text: &str) -> Value {
        serde_json::from_str(text).expect("contracts/*.schema.json 应是合法 JSON")
    }

    /// 解引用 schema 内部 `$ref`（`#/$defs/...`）。
    fn resolve<'a>(root: &'a Value, node: &'a Value) -> &'a Value {
        match node.get("$ref") {
            Some(Value::String(r)) => {
                let path = r.strip_prefix("#/").expect("仅支持本文档内的 $ref");
                let mut cur = root;
                for seg in path.split('/') {
                    cur = cur.get(seg).unwrap_or_else(|| panic!("$ref {r} 无法解析"));
                }
                resolve(root, cur)
            }
            _ => node,
        }
    }

    fn keys(v: &Value) -> BTreeSet<String> {
        v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
    }

    fn required_keys(node: &Value) -> BTreeSet<String> {
        node.get("required")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    }

    #[derive(Clone, Copy)]
    enum KeyMode {
        /// 全字段序列化：对象 key 集 == schema properties 集。
        Properties,
        /// 最小序列化：对象 key 集 == schema required 集（可选/空集合全被跳过）。
        Required,
    }

    /// 递归对拍「Rust 结构序列化结果」与「schema 节点」的 key 集合（含嵌套）。
    /// 自由对象（meta / local_storage 这类无 properties 的 map）不比对。
    fn assert_keys_match(value: &Value, node: &Value, root: &Value, path: &str, mode: KeyMode) {
        let node = resolve(root, node);
        match value {
            Value::Object(map) => {
                let props = node.get("properties");
                let expected = match mode {
                    KeyMode::Properties => props.map(keys),
                    KeyMode::Required => props.map(|_| required_keys(node)),
                };
                if let Some(expected) = expected {
                    let label = match mode {
                        KeyMode::Properties => "schema properties",
                        KeyMode::Required => "schema required",
                    };
                    assert_eq!(keys(value), expected, "字段集合与 {label} 不一致：{path}");
                }
                for (k, v) in map {
                    if let Some(sub) = props.and_then(|p| p.get(k)) {
                        assert_keys_match(v, sub, root, &format!("{path}.{k}"), mode);
                    }
                }
            }
            Value::Array(items) => {
                if let Some(sub) = node.get("items") {
                    for (i, item) in items.iter().enumerate() {
                        assert_keys_match(item, sub, root, &format!("{path}[{i}]"), mode);
                    }
                }
            }
            _ => {}
        }
    }

    // ---- 全字段 Rust 值：每个 Option 都 Some、每个 Vec/Map 都非空 ----

    fn full_cookie() -> Cookie {
        Cookie {
            name: "sid".into(),
            value: "fake-cookie-value".into(),
            domain: Some("example.com".into()),
            path: Some("/".into()),
            secure: Some(true),
            http_only: Some(true),
            same_site: Some(SameSite::Lax),
            expires_at: Some("2026-12-01T00:00:00Z".into()),
        }
    }

    fn full_target() -> Target {
        Target {
            kind: TargetKind::Web,
            domain: Some("example.com".into()),
            app_id: Some("qq".into()),
        }
    }

    fn full_source() -> Source {
        Source {
            source_type: SourceType::Extension,
            id: "chromium".into(),
        }
    }

    fn full_client_env() -> ClientEnv {
        ClientEnv {
            user_agent: "Mozilla/5.0 (Fake) CompatTest".into(),
            platform: "windows".into(),
            device_id: Some("fake-device-1".into()),
        }
    }

    fn full_payload() -> Payload {
        Payload {
            cookies: vec![full_cookie()],
            token: Some("fake-token-value".into()),
            credential: Some(PlainCredential {
                username: "demo-user".into(),
                secret: "fake-secret-not-real".into(),
            }),
            local_storage: BTreeMap::from([("ls_key".to_string(), "fake-ls-value".to_string())]),
        }
    }

    fn full_credential_record() -> CredentialRecord {
        CredentialRecord {
            account_ref: "acc-1".into(),
            kind: CredentialKind::Totp,
            username: Some("demo-user".into()),
            secret: Some("fake-secret-not-real".into()),
            totp_seed: Some("FAKESEEDJBSWY3DP".into()),
            url: Some("https://example.com/login".into()),
        }
    }

    fn full_session_record() -> SessionRecord {
        SessionRecord {
            account_ref: "acc-1".into(),
            target: full_target(),
            kind: SessionKind::Cookie,
            payload: full_payload(),
            expires_at: Some("2026-12-01T00:00:00Z".into()),
            source: full_source(),
            client_env: Some(full_client_env()),
        }
    }

    fn full_standard_access() -> StandardAccess {
        StandardAccess {
            schema_version: SCHEMA_VERSION.into(),
            app_id: "github".into(),
            level: AccessLevel::B,
            generated_at: "2026-10-09T00:00:00Z".into(),
            credentials: vec![full_credential_record()],
            sessions: vec![full_session_record()],
            meta: BTreeMap::from([(
                "converter_version".to_string(),
                json!("fake-1.0.0"),
            )]),
            client_env: Some(full_client_env()),
        }
    }

    fn full_envelope() -> SessionEnvelope {
        SessionEnvelope {
            schema_version: SCHEMA_VERSION.into(),
            account_ref: "acc-1".into(),
            target: full_target(),
            kind: SessionKind::Cookie,
            payload: full_payload(),
            issued_at: "2026-10-09T00:00:00Z".into(),
            expires_at: Some("2026-11-01T00:00:00Z".into()),
            source: full_source(),
            scope: Some(ScopeSpec {
                domain_allowlist: vec!["example.com".into()],
                read_only: true,
                single_use: true,
            }),
            client_env: Some(full_client_env()),
        }
    }

    fn full_manifest() -> ConverterManifest {
        ConverterManifest {
            id: "qq".into(),
            name: "QQ 假转换器".into(),
            version: "1.2.0".into(),
            level: AccessLevel::C,
            readable_paths: vec!["C:\\Users\\demo\\AppData\\Roaming\\Tencent\\QQ".into()],
            entry: "qq.wasm".into(),
            module_sha256: "3f2a9c1d5b7e4088a1c2d3e4f5061728394a5b6c7d8e9f001122334455667788".into(),
            signature: Signature {
                algorithm: "ed25519".into(),
                signer: "official-fixture".into(),
                value: "6f1d0b2a4c8e7395a0b1c2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f60718899aabbccddeeff00112233445566778899aabbccddeeff0011223344556677"
                    .into(),
            },
            min_app_version: Some("2.0.0".into()),
        }
    }

    // ---- 最小 Rust 值：Option 全 None、Vec/Map 全空 ----

    fn min_standard_access() -> StandardAccess {
        StandardAccess {
            schema_version: SCHEMA_VERSION.into(),
            app_id: "qq".into(),
            level: AccessLevel::C,
            generated_at: "2026-10-09T00:00:00Z".into(),
            credentials: vec![],
            sessions: vec![],
            meta: BTreeMap::new(),
            client_env: None,
        }
    }

    fn min_envelope() -> SessionEnvelope {
        SessionEnvelope {
            schema_version: SCHEMA_VERSION.into(),
            account_ref: "acc-1".into(),
            target: Target {
                kind: TargetKind::Web,
                domain: None,
                app_id: None,
            },
            kind: SessionKind::Cookie,
            payload: Payload::default(),
            issued_at: "2026-10-09T00:00:00Z".into(),
            expires_at: None,
            source: full_source(),
            scope: None,
            client_env: None,
        }
    }

    fn min_manifest() -> ConverterManifest {
        ConverterManifest {
            min_app_version: None,
            signature: Signature {
                algorithm: "ed25519".into(),
                signer: "official-fixture".into(),
                value: full_manifest().signature.value,
            },
            ..full_manifest()
        }
    }

    // ============ 1. 旧数据兼容：v1.0 最小/全字段载荷反序列化 + validate 通过 ============

    #[test]
    fn v1_standard_access_minimal_and_full_still_work() {
        let min: StandardAccess = de(SA_MIN);
        min.validate().expect("v1 最小 StandardAccess 应通过 validate");
        assert!(min.credentials.is_empty() && min.sessions.is_empty() && min.meta.is_empty());

        let full: StandardAccess = de(SA_FULL);
        full.validate().expect("v1 全字段 StandardAccess 应通过 validate");
        // 三种载荷 kind（cookie / token / credential）的旧 session 全部可用
        assert_eq!(full.sessions.len(), 3);
    }

    #[test]
    fn v1_converter_manifest_minimal_and_full_still_work() {
        let min: ConverterManifest = de(MANIFEST_MIN);
        min.validate().expect("v1 最小 ConverterManifest 应通过 validate");
        assert!(min.min_app_version.is_none());

        let full: ConverterManifest = de(MANIFEST_FULL);
        full.validate().expect("v1 全字段 ConverterManifest 应通过 validate");
        assert_eq!(full.min_app_version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn v1_session_envelope_three_payload_kinds_still_work() {
        for (kind, json) in [
            ("cookie", ENV_MIN_COOKIE),
            ("token", ENV_MIN_TOKEN),
            ("credential", ENV_MIN_CRED),
        ] {
            let env: SessionEnvelope = de(json);
            env.validate()
                .unwrap_or_else(|e| panic!("{kind} 载荷的最小信封 validate 失败：{e}"));
        }

        let full: SessionEnvelope = de(ENV_FULL);
        full.validate().expect("v1 全字段 SessionEnvelope 应通过 validate");
        full.validate_temporary()
            .expect("全字段信封含 expires_at + scope，临时会话旧数据也应可用");
    }

    // ============ 2. 新增可选字段（未知字段）不破坏兼容 ============

    /// 锁定 serde 默认「忽略未知字段」：任何人给结构加 `deny_unknown_fields`，本测试即红。
    /// 对应 §6「新增字段一律可选」：新版本新增的字段对旧读者必须是可忽略的未知字段。
    #[test]
    fn unknown_fields_are_tolerated_not_denied() {
        let mut sa: Value = schema(SA_FULL);
        sa["future_field"] = json!(123);
        sa["sessions"][0]["target"]["future_field"] = json!(123);
        sa["credentials"][0]["future_field"] = json!("new");
        let parsed: StandardAccess =
            serde_json::from_value(sa).expect("StandardAccess 应容忍顶层与嵌套未知字段");
        parsed.validate().expect("未知字段不影响 validate");

        let mut m: Value = schema(MANIFEST_FULL);
        m["future_field"] = json!(123);
        m["signature"]["future_field"] = json!(true);
        let parsed: ConverterManifest =
            serde_json::from_value(m).expect("ConverterManifest 应容忍顶层与嵌套未知字段");
        parsed.validate().expect("未知字段不影响 validate");

        let mut e: Value = schema(ENV_FULL);
        e["future_field"] = json!(123);
        e["payload"]["cookies"][0]["future_field"] = json!(123);
        e["scope"]["future_field"] = json!(123);
        let parsed: SessionEnvelope =
            serde_json::from_value(e).expect("SessionEnvelope 应容忍顶层与嵌套未知字段");
        parsed.validate().expect("未知字段不影响 validate");
    }

    // ============ 3. 省略可选字段仍可用 ============

    #[test]
    fn omitted_optional_fields_still_work() {
        // Cookie 只剩 name/value（domain / path / secure / http_only / same_site / expires_at 全省）
        let env: SessionEnvelope = de(ENV_MIN_COOKIE);
        env.validate().expect("省略 Cookie 可选属性应通过 validate");
        let c = &env.payload.cookies[0];
        assert_eq!(
            (
                c.domain.as_deref(),
                c.path.as_deref(),
                c.secure,
                c.http_only,
                c.same_site,
                c.expires_at.as_deref(),
            ),
            (None, None, None, None, None, None),
        );
        assert!(env.expires_at.is_none() && env.scope.is_none() && env.client_env.is_none());

        let m: ConverterManifest = de(MANIFEST_MIN);
        m.validate().expect("省略 min_app_version 应通过 validate");
        assert!(m.min_app_version.is_none());

        // 任意子集省略：从全字段信封删掉一半可选字段仍可用
        let mut v: Value = schema(ENV_FULL);
        v.as_object_mut().unwrap().remove("expires_at");
        v.as_object_mut().unwrap().remove("scope");
        v["payload"]["cookies"][0].as_object_mut().unwrap().remove("path");
        v["payload"]["cookies"][0].as_object_mut().unwrap().remove("secure");
        v["payload"]["cookies"][0].as_object_mut().unwrap().remove("same_site");
        let stripped: SessionEnvelope =
            serde_json::from_value(v).expect("可选字段任意省略应可反序列化");
        stripped.validate().expect("可选字段任意省略应通过 validate");
        assert!(stripped.expires_at.is_none() && stripped.scope.is_none());
    }

    // ============ 4. schema_version 非 "1.0" 必须被 validate 拒绝 ============

    /// 注：converter-manifest 无 schema_version 字段（用 version 标转换器自身版本），
    /// 与 check-compat.mjs「converter-manifest 用 version（转换器自身版本）而非
    /// schema_version，锁定为『无声明』」一致，故本项只测 StandardAccess / SessionEnvelope。
    #[test]
    fn unsupported_schema_version_rejected() {
        for bad in ["0.9", "1", "2.0", "1.0.0", ""] {
            let mut sa: Value = schema(SA_FULL);
            sa["schema_version"] = json!(bad);
            let parsed: StandardAccess =
                serde_json::from_value(sa).expect("serde 不校验版本，由 validate 把关");
            assert!(
                matches!(parsed.validate(), Err(ContractError::UnsupportedSchemaVersion(v)) if v == bad),
                "StandardAccess schema_version={bad:?} 应报 UnsupportedSchemaVersion"
            );

            let mut e: Value = schema(ENV_FULL);
            e["schema_version"] = json!(bad);
            let parsed: SessionEnvelope = serde_json::from_value(e).unwrap();
            assert!(
                matches!(parsed.validate(), Err(ContractError::UnsupportedSchemaVersion(v)) if v == bad),
                "SessionEnvelope schema_version={bad:?} 应报 UnsupportedSchemaVersion"
            );
        }
    }

    // ============ 5. schema 与代码不漂移：序列化 key 集 == schema properties 集（含嵌套） ============

    #[test]
    fn schema_matches_code_standard_access() {
        let root = schema(SCHEMA_STANDARD);
        let value = serde_json::to_value(full_standard_access()).unwrap();
        assert_keys_match(&value, &root, &root, "StandardAccess", KeyMode::Properties);
    }

    #[test]
    fn schema_matches_code_converter_manifest() {
        let root = schema(SCHEMA_MANIFEST);
        let value = serde_json::to_value(full_manifest()).unwrap();
        assert_keys_match(&value, &root, &root, "ConverterManifest", KeyMode::Properties);
    }

    #[test]
    fn schema_matches_code_session_envelope() {
        let root = schema(SCHEMA_ENVELOPE);
        let value = serde_json::to_value(full_envelope()).unwrap();
        assert_keys_match(&value, &root, &root, "SessionEnvelope", KeyMode::Properties);
    }

    // ============ 6. 空 Option / 空 Vec / 空 Map 被跳过（与 schema required 对应） ============

    #[test]
    fn empty_option_vec_map_skipped_in_serialization() {
        let root = schema(SCHEMA_STANDARD);
        let sa = serde_json::to_value(min_standard_access()).unwrap();
        for skipped in ["credentials", "sessions", "meta", "client_env"] {
            assert!(sa.get(skipped).is_none(), "空集合/None 应被跳过：StandardAccess.{skipped}");
        }
        assert_keys_match(&sa, &root, &root, "StandardAccess(min)", KeyMode::Required);

        let root = schema(SCHEMA_ENVELOPE);
        let env = serde_json::to_value(min_envelope()).unwrap();
        for skipped in ["expires_at", "scope", "client_env"] {
            assert!(env.get(skipped).is_none(), "空 Option 应被跳过：SessionEnvelope.{skipped}");
        }
        assert_eq!(
            serde_json::to_value(Payload::default()).unwrap(),
            json!({}),
            "空 Payload 应序列化为 {{}}"
        );
        assert_eq!(
            serde_json::to_value(Cookie {
                name: "sid".into(),
                value: "fake-cookie-value".into(),
                domain: None,
                path: None,
                secure: None,
                http_only: None,
                same_site: None,
                expires_at: None,
            })
            .unwrap(),
            json!({ "name": "sid", "value": "fake-cookie-value" }),
            "Cookie 空 Option 应被跳过"
        );
        assert_keys_match(&env, &root, &root, "SessionEnvelope(min)", KeyMode::Required);

        let root = schema(SCHEMA_MANIFEST);
        let m = serde_json::to_value(min_manifest()).unwrap();
        assert!(m.get("min_app_version").is_none(), "空 Option 应被跳过：ConverterManifest.min_app_version");
        assert_keys_match(&m, &root, &root, "ConverterManifest(min)", KeyMode::Required);
    }
}
