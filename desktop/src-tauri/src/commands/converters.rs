//! W8 转换器面板的命令接线（B4.1：列表 + 显式安装 + 信任链验证）。
//!
//! 信任链（docs/安全模型.md §6）：结构校验 → 签名覆盖 manifest 规范化字节 + module_sha256
//! → 摘要与执行字节一致 → 占位签名拒载 —— 全部经 `ab-converter::ConverterHost::load`
//! 与注入的 [`crate::host::HostSignatureVerifier`]；本层不写验签规则。
//!
//! 边界（docs/功能点全景.md §5）：**无第三方野生转换器安装入口**，安装 = 显式命令 +
//! 官方签名通过；UI 无安装按钮时面板状态照旧（空态）。

use ab_audit::{AuditAction, AuditResult};
use ab_contract::{AccessLevel, ConverterManifest};
use serde::{Deserialize, Serialize};

use super::{record, CmdResult};
use crate::host::HostState;

/// 转换器列表行（W8：验签状态 + 接入等级 + 可读路径声明）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConverterView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub level: AccessLevel,
    pub readable_paths: Vec<String>,
    pub signer: String,
    /// 信任链验证结果（结构 + 验签 + 摘要一致性）。
    pub verified: bool,
    /// 拒载理由（`VerifyError` 闭合枚举文本，不外泄验签细节）。
    pub verify_error: Option<String>,
}

fn view_of(manifest: &ConverterManifest, verified: bool, error: Option<String>) -> ConverterView {
    ConverterView {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        level: manifest.level,
        readable_paths: manifest.readable_paths.clone(),
        signer: manifest.signature.signer.clone(),
        verified,
        verify_error: error,
    }
}

/// 列出托管目录中的转换器（逐个走信任链验证；失败行如实标注）。
pub fn list(services: &HostState) -> CmdResult<Vec<ConverterView>> {
    let dir = services.paths.converters_dir();
    let mut views = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(views); // 未安装任何转换器 = 空态
    };
    let mut dirs: Vec<_> = entries.filter_map(|entry| entry.ok()).collect();
    dirs.sort_by_key(|entry| entry.file_name());

    for entry in dirs {
        let path = entry.path();
        let manifest_path = path.join("manifest.json");
        let manifest_json = match std::fs::read_to_string(&manifest_path) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let module = match manifest_entry(&manifest_json) {
            Ok(entry_name) => match std::fs::read(path.join(entry_name)) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        match services.converter_host.load(&manifest_json, &module) {
            Ok(verified) => views.push(view_of(verified.manifest(), true, None)),
            Err(error) => {
                if let Ok(manifest) = serde_json::from_str::<ConverterManifest>(&manifest_json) {
                    views.push(view_of(&manifest, false, Some(error.to_string())));
                }
            }
        }
    }
    Ok(views)
}

fn manifest_entry(manifest_json: &str) -> CmdResult<String> {
    let manifest: ConverterManifest =
        serde_json::from_str(manifest_json).map_err(|error| error.to_string())?;
    Ok(manifest.entry)
}

/// 显式安装：信任链通过才落托管目录（拒绝即整体不落盘）；动作进审计（fail-closed）。
pub fn install(
    services: &HostState,
    manifest_json: &str,
    module: &[u8],
) -> CmdResult<ConverterView> {
    match services.converter_host.load(manifest_json, module) {
        Ok(verified) => {
            let manifest = verified.manifest();
            let dir = services.paths.converters_dir().join(&manifest.id);
            std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
            std::fs::write(dir.join("manifest.json"), manifest_json)
                .map_err(|error| error.to_string())?;
            std::fs::write(dir.join(&manifest.entry), module)
                .map_err(|error| error.to_string())?;
            record(
                services,
                AuditAction::ConverterLoad,
                AuditResult::Ok,
                None,
                None,
                &[("converter", &manifest.id)],
            )?;
            Ok(view_of(manifest, true, None))
        }
        Err(error) => {
            // 拒载同样入账（fail-closed：写账失败以审计错误为准）。
            record(
                services,
                AuditAction::ConverterReject,
                AuditResult::Failed,
                None,
                None,
                &[("reason", "trust_chain")],
            )?;
            Err(error.to_string())
        }
    }
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn converter_list(state: tauri::State<'_, HostState>) -> CmdResult<Vec<ConverterView>> {
    list(&state)
}

#[tauri::command]
pub fn converter_install(
    state: tauri::State<'_, HostState>,
    manifest_json: String,
    module: Vec<u8>,
) -> CmdResult<ConverterView> {
    install(&state, &manifest_json, &module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostLayout, HostState, HostSignatureVerifier};
    use ab_audit::{AuditAction, AuditFilter, InMemoryAuditSink};
    use ab_contract::Signature;
    use ab_crypto::{ed25519_keypair, ed25519_sign, sha256};
    use std::sync::Arc;

    fn test_root() -> std::path::PathBuf {
        crate::host::test_root("aibridge-conv-test")
    }

    fn hex_lower(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn services_with(root: &std::path::Path, signer: Option<String>, key: Option<[u8; 32]>) -> HostState {
        HostState::build_with(
            "pc-test",
            HostLayout {
                root: root.to_path_buf(),
            },
            HostSignatureVerifier::new(signer, key),
            Arc::new(InMemoryAuditSink::new()),
        )
    }

    #[test]
    fn install_requires_trust_chain_and_records_audit() {
        let root = test_root();
        let _ = std::fs::remove_dir_all(&root);

        // 一次性密钥对：签名与验证用同一信任根（真验签路径）。
        let (secret, public) = ed25519_keypair(&ab_crypto::OsEntropy).expect("密钥生成失败");
        let module = b"wasm-module-bytes".to_vec();
        let digest = hex_lower(&sha256(&module));
        let mut manifest = ab_contract::ConverterManifest {
            id: "sample-app".into(),
            name: "Sample".into(),
            version: "0.1.0".into(),
            level: AccessLevel::B,
            readable_paths: vec!["{APPDATA}\\sample-app".into()],
            entry: "converter.wasm".into(),
            module_sha256: digest.clone(),
            signature: Signature {
                algorithm: "ed25519".into(),
                signer: "aibridge-official-2026".into(),
                value: String::new(),
            },
            min_app_version: None,
        };
        let mut value = serde_json::to_value(&manifest).unwrap();
        if let serde_json::Value::Object(ref mut map) = value {
            map.remove("signature");
            map.insert("module_sha256".into(), digest.clone().into());
        }
        let mut signed = serde_json::to_string(&value).unwrap().into_bytes();
        signed.extend_from_slice(digest.as_bytes());
        manifest.signature.value = hex_lower(&ed25519_sign(&secret, &signed));
        let manifest_json = serde_json::to_string(&manifest).unwrap();

        let services = services_with(
            &root,
            Some("aibridge-official-2026".into()),
            Some(public),
        );

        // 换模块保清单 → 拒载 + ConverterReject 入账。
        let error = install(&services, &manifest_json, b"other-module").unwrap_err();
        assert!(error.contains("签名与清单内容不匹配"), "实际：{error}");
        let events = services.audit.query(&AuditFilter::default()).expect("查询失败");
        assert!(events
            .iter()
            .any(|event| event.action == AuditAction::ConverterReject));

        // 正确模块 → 安装 + ConverterLoad 入账 + 列表可见。
        let view = install(&services, &manifest_json, &module).expect("安装失败");
        assert!(view.verified);
        assert_eq!(view.level, AccessLevel::B);
        let events = services.audit.query(&AuditFilter::default()).expect("查询失败");
        assert!(events
            .iter()
            .any(|event| event.action == AuditAction::ConverterLoad));

        let listed = list(&services).expect("列表失败");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].verified);
        assert_eq!(listed[0].readable_paths, vec!["{APPDATA}\\sample-app"]);

        // 未配置信任根的宿主一律拒载（无后门开关）。
        let unconfigured = services_with(&root.join("other"), None, None);
        let error = install(&unconfigured, &manifest_json, &module).unwrap_err();
        assert!(error.contains("签名者不受信"), "实际：{error}");
    }

    #[test]
    fn placeholder_signature_is_rejected_by_contract() {
        let root = test_root();
        let _ = std::fs::remove_dir_all(&root);
        let services = services_with(
            &root,
            Some("aibridge-official-2026".into()),
            Some([7u8; 32]),
        );
        // 全零签名是模板占位：契约层结构校验即拒载（安全模型 §6）。
        let manifest_json = serde_json::json!({
            "id": "sample-app",
            "name": "Sample",
            "version": "0.1.0",
            "level": "b",
            "readable_paths": ["{APPDATA}\\\\sample-app"],
            "entry": "converter.wasm",
            "module_sha256": "1".repeat(64),
            "signature": {
                "algorithm": "ed25519",
                "signer": "aibridge-official-2026",
                "value": "0".repeat(128)
            }
        })
        .to_string();
        let error = install(&services, &manifest_json, b"m").unwrap_err();
        assert!(error.contains("占位"), "实际：{error}");
    }
}
