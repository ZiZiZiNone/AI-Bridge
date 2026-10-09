//! 单元测试：四个必需场景（验签失败拒载 / 等级裁剪正确 / 路径越界拒绝 / 合法加载全链）
//! 全部用 mock 实现覆盖——本 crate 不引密码学库，也不引 WASM 运行时。

use std::cell::Cell;
use std::rc::Rc;

use ab_contract::{AccessLevel, ConverterManifest, StandardAccess};

use crate::{
    ConverterError, ConverterHost, PathDenied, PathGuard, SandboxError, SandboxRequest,
    SandboxRunner, SignatureVerifier, VerifyError,
};

const MANIFEST_OK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/manifest_ok.json"
));
const MANIFEST_INVALID: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/manifest_invalid.json"
));
const ACCESS_FULL: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/standard_access_full.json"
));

/// 清单声明路径的子路径（白名单内）。
const IN_SCOPE_PATH: &str = "C:\\Users\\me\\AppData\\Roaming\\QQ\\nt_data\\cookies.db";
/// 白名单外的路径（系统凭据库）。
const OUT_OF_SCOPE_PATH: &str = "C:\\Windows\\System32\\config\\SAM";
/// 清单声明的可读根。
const DECLARED_ROOT: &str = "C:\\Users\\me\\AppData\\Roaming\\QQ";

/* ---------------- mock 实现 ---------------- */

/// 验签 mock：按预设结论返回，并记录调用次数。
struct MockVerifier {
    verdict: Result<(), String>,
    calls: Rc<Cell<usize>>,
}

impl MockVerifier {
    fn accepting() -> Self {
        Self {
            verdict: Ok(()),
            calls: Rc::new(Cell::new(0)),
        }
    }

    fn rejecting(reason: &str) -> Self {
        Self {
            verdict: Err(reason.to_string()),
            calls: Rc::new(Cell::new(0)),
        }
    }

    fn accepting_counted() -> (Self, Rc<Cell<usize>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                verdict: Ok(()),
                calls: Rc::clone(&calls),
            },
            calls,
        )
    }
}

impl SignatureVerifier for MockVerifier {
    fn verify(&self, _manifest: &ConverterManifest) -> Result<(), VerifyError> {
        self.calls.set(self.calls.get() + 1);
        self.verdict.clone().map_err(VerifyError::new)
    }
}

/// 沙盒 mock：可声明要读的路径（经守卫判定）、可指定输出、可指定失败。
struct MockRunner {
    requested_path: Option<String>,
    output: Option<StandardAccess>,
    failure: Option<String>,
}

impl MockRunner {
    fn new() -> Self {
        Self {
            requested_path: None,
            output: None,
            failure: None,
        }
    }

    fn requesting(path: &str) -> Self {
        Self {
            requested_path: Some(path.to_string()),
            ..Self::new()
        }
    }

    fn returning(output: StandardAccess) -> Self {
        Self {
            output: Some(output),
            ..Self::new()
        }
    }

    fn failing(reason: &str) -> Self {
        Self {
            failure: Some(reason.to_string()),
            ..Self::new()
        }
    }
}

impl SandboxRunner for MockRunner {
    fn run(&self, request: &SandboxRequest<'_>) -> Result<StandardAccess, SandboxError> {
        if let Some(path) = &self.requested_path {
            // 与真实运行器同一路径：先经守卫取得路径，越界即失败退出。
            request.guard.resolve(path)?;
        }
        if let Some(reason) = &self.failure {
            return Err(SandboxError::failed(reason.clone()));
        }
        if let Some(output) = &self.output {
            return Ok(output.clone());
        }
        Ok(full_access_fixture())
    }
}

fn full_access_fixture() -> StandardAccess {
    serde_json::from_str(ACCESS_FULL)
        .expect("夹具 standard_access_full.json 必须是合法 StandardAccess")
}

fn accepting_host(runner: MockRunner) -> ConverterHost<MockVerifier, MockRunner> {
    ConverterHost::new(MockVerifier::accepting(), runner)
}

fn manifest_json_with_level(level: &str) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(MANIFEST_OK).expect("夹具 manifest_ok.json 必须是合法 JSON");
    value["level"] = serde_json::Value::String(level.to_string());
    value.to_string()
}

/* ---------------- 场景四：合法加载全链 ---------------- */

#[test]
fn legal_load_and_extract_full_chain() {
    let host = accepting_host(MockRunner::requesting(IN_SCOPE_PATH));

    let converter = host.load(MANIFEST_OK).expect("合法清单应加载成功");
    assert_eq!(converter.manifest().id, "qq");
    assert_eq!(converter.manifest().level, AccessLevel::C);

    let access = host
        .run_extract(&converter, AccessLevel::C)
        .expect("白名单内路径应执行成功");
    assert_eq!(access.level, AccessLevel::C);
    assert_eq!(access.app_id, "qq");
    assert_eq!(access.credentials.len(), 1);
    assert_eq!(access.credentials[0].account_ref, "acc-qq-1");
    assert_eq!(access.sessions.len(), 1);
    assert_eq!(access.meta.len(), 2);
}

#[test]
fn verification_runs_once_per_load() {
    let (verifier, calls) = MockVerifier::accepting_counted();
    let host = ConverterHost::new(verifier, MockRunner::new());

    host.load(MANIFEST_OK).expect("合法清单应加载成功");
    assert_eq!(calls.get(), 1);
    host.load(MANIFEST_OK).expect("合法清单应加载成功");
    assert_eq!(calls.get(), 2);
}

/* ---------------- 场景一：验签失败拒载 ---------------- */

#[test]
fn failed_signature_rejects_load() {
    let host = ConverterHost::new(
        MockVerifier::rejecting("签名与官方公钥不匹配"),
        MockRunner::new(),
    );
    assert_eq!(
        host.load(MANIFEST_OK),
        Err(ConverterError::SignatureRejected(
            "签名与官方公钥不匹配".to_string()
        ))
    );
}

#[test]
fn invalid_manifest_rejected_before_verification() {
    let (verifier, calls) = MockVerifier::accepting_counted();
    let host = ConverterHost::new(verifier, MockRunner::new());

    assert!(matches!(
        host.load(MANIFEST_INVALID),
        Err(ConverterError::Contract(_))
    ));
    assert_eq!(calls.get(), 0, "结构校验失败时不得消耗验签能力");

    assert!(matches!(
        host.load("{ not json"),
        Err(ConverterError::InvalidManifest(_))
    ));
    assert_eq!(calls.get(), 0);
}

/* ---------------- 场景二：等级裁剪正确 ---------------- */

#[test]
fn level_a_keeps_only_meta_and_launch_info() {
    let host = accepting_host(MockRunner::new());
    let converter = host.load(MANIFEST_OK).expect("合法清单应加载成功");

    let access = host
        .run_extract(&converter, AccessLevel::A)
        .expect("a 级执行应成功");
    assert_eq!(access.level, AccessLevel::A);
    assert!(access.credentials.is_empty(), "a 级不得外流凭据");
    assert!(access.sessions.is_empty(), "a 级不得外流会话");
    assert_eq!(access.app_id, "qq");
    assert_eq!(access.generated_at, "2026-10-09T10:00:00Z");
    assert_eq!(access.meta.len(), 2, "a 级保留 meta 自述信息");
}

#[test]
fn level_b_keeps_only_credentials() {
    let host = accepting_host(MockRunner::new());
    let converter = host.load(MANIFEST_OK).expect("合法清单应加载成功");

    let access = host
        .run_extract(&converter, AccessLevel::B)
        .expect("b 级执行应成功");
    assert_eq!(access.level, AccessLevel::B);
    assert_eq!(access.credentials.len(), 1, "b 级保留凭据");
    assert!(access.sessions.is_empty(), "b 级不得外流会话");
    assert_eq!(access.meta.len(), 2);
}

#[test]
fn level_c_keeps_sessions() {
    let host = accepting_host(MockRunner::new());
    let converter = host.load(MANIFEST_OK).expect("合法清单应加载成功");

    let access = host
        .run_extract(&converter, AccessLevel::C)
        .expect("c 级执行应成功");
    assert_eq!(access.level, AccessLevel::C);
    assert_eq!(access.credentials.len(), 1);
    assert_eq!(access.sessions.len(), 1, "c 级含会话");
}

#[test]
fn manifest_capability_narrows_requested_level() {
    let host = accepting_host(MockRunner::new());
    let a_manifest = manifest_json_with_level("a");
    let converter = host.load(&a_manifest).expect("合法清单应加载成功");
    assert_eq!(converter.manifest().level, AccessLevel::A);

    let access = host
        .run_extract(&converter, AccessLevel::C)
        .expect("降级执行应成功");
    assert_eq!(access.level, AccessLevel::A, "清单只有 a 级能力时不得放行 c 级输出");
    assert!(access.credentials.is_empty());
    assert!(access.sessions.is_empty());
}

/* ---------------- 场景三：路径越界拒绝 ---------------- */

#[test]
fn path_outside_scope_rejected() {
    let guard = PathGuard::new(&[DECLARED_ROOT.to_string()]);

    assert!(guard.resolve(IN_SCOPE_PATH).is_ok(), "白名单子路径应放行");
    assert!(guard.resolve(DECLARED_ROOT).is_ok(), "白名单根自身应放行");

    assert_eq!(
        guard.resolve(OUT_OF_SCOPE_PATH),
        Err(PathDenied::new(OUT_OF_SCOPE_PATH))
    );
    assert!(
        guard
            .resolve("C:\\Users\\me\\AppData\\Roaming\\QQBackup\\x.db")
            .is_err(),
        "同前缀的兄弟目录不是子路径"
    );
    assert!(
        guard
            .resolve("C:\\Users\\me\\AppData\\Roaming\\QQ\\..\\..\\..\\Windows\\System32\\config\\SAM")
            .is_err(),
        "`..` 路径穿越一律拒绝"
    );
    assert!(
        guard.resolve("Users/me/AppData/Roaming/QQ/x.db").is_err(),
        "相对路径不得冒充绝对白名单"
    );
    assert!(guard.resolve("").is_err(), "空路径拒绝");

    assert!(
        PathGuard::new(&[]).resolve(IN_SCOPE_PATH).is_err(),
        "无声明即可读一切是禁止的"
    );
    assert!(
        PathGuard::new(&["C:\\Users\\me\\..\\..\\Windows".to_string()])
            .resolve("C:\\Users\\me\\..\\..\\Windows\\System32")
            .is_err(),
        "声明里带 `..` 的根必须被丢弃（fail-closed）"
    );
}

#[test]
fn sandbox_path_violation_aborts_extract() {
    let host = accepting_host(MockRunner::requesting(OUT_OF_SCOPE_PATH));
    let converter = host.load(MANIFEST_OK).expect("合法清单应加载成功");

    assert_eq!(
        host.run_extract(&converter, AccessLevel::C),
        Err(ConverterError::PathOutsideScope(
            OUT_OF_SCOPE_PATH.to_string()
        ))
    );
}

/* ---------------- 附加：执行失败与畸形输出 ---------------- */

#[test]
fn sandbox_failure_is_reported() {
    let host = accepting_host(MockRunner::failing("wasm 陷阱：越界内存访问"));
    let converter = host.load(MANIFEST_OK).expect("合法清单应加载成功");

    assert_eq!(
        host.run_extract(&converter, AccessLevel::C),
        Err(ConverterError::SandboxFailed(
            "wasm 陷阱：越界内存访问".to_string()
        ))
    );
}

#[test]
fn malformed_sandbox_output_rejected_by_contract() {
    let broken: StandardAccess = serde_json::from_str(
        r#"{
            "schema_version": "1.0",
            "app_id": "qq",
            "level": "c",
            "generated_at": "2026-10-09T10:00:00Z",
            "sessions": [
                {
                    "account_ref": "acc-qq-1",
                    "target": { "kind": "web", "domain": "example.invalid" },
                    "kind": "cookie",
                    "payload": {},
                    "source": { "type": "converter", "id": "qq" }
                }
            ]
        }"#,
    )
    .expect("反序列化本身应成功：问题在 kind 与 payload 的一致性");

    let host = accepting_host(MockRunner::returning(broken));
    let converter = host.load(MANIFEST_OK).expect("合法清单应加载成功");

    assert!(matches!(
        host.run_extract(&converter, AccessLevel::C),
        Err(ConverterError::Contract(_))
    ));
}
