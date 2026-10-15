//! W3 库面板 / W11 初始化向导 的命令接线（A1 条目 · A1.5 回收站 · S1 解锁 · S3 恢复密钥）。
//!
//! 纯函数只调 core：条目本体在 `ab_vault`（KDBX4 标准字段 + 自定义字段 account_ref/kind/
//! totp_seed），审计经 `ab-audit`（fail-closed）。本层不含业务判断。
//!
//! core 缺口（不在本层补第二份规则，见 README）：A5 口令生成器、A1.2 检索/排序、
//! A1.3 密码体检在 `ab-vault` 无公开 API。

use ab_audit::{AuditAction, AuditResult};
use ab_contract::CredentialKind;
use ab_vault::{Entry, RecoveryKey, VaultSession};
use serde::{Deserialize, Serialize};

use super::{record, CmdResult};
use crate::host::HostState;

/// 条目列表行（W3 列表用）：**不含秘密字段**（S6：秘密不经过非必要通道）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryView {
    pub uuid: String,
    pub title: String,
    pub account_ref: String,
    pub kind: CredentialKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub has_secret: bool,
    pub has_totp: bool,
}

/// 条目明细（编辑表单 / 填充用）：含秘密字段，`Debug` 手工脱敏（F7，沿用 core 口径）。
#[derive(Clone, Serialize, Deserialize)]
pub struct EntryDetail {
    pub uuid: String,
    pub title: String,
    pub account_ref: String,
    pub kind: CredentialKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl std::fmt::Debug for EntryDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntryDetail")
            .field("uuid", &self.uuid)
            .field("title", &self.title)
            .field("account_ref", &self.account_ref)
            .field("kind", &self.kind)
            .field("username", &self.username)
            .field("secret", &self.secret.as_deref().map(|_| "[REDACTED]"))
            .field("totp_seed", &self.totp_seed.as_deref().map(|_| "[REDACTED]"))
            .field("url", &self.url)
            .field("note", &self.note.as_deref().map(|_| "[REDACTED]"))
            .finish()
    }
}

/// 新建 / 编辑条目的输入（全字段；含自定义字段 account_ref / kind / totp_seed）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInput {
    pub title: String,
    pub account_ref: String,
    pub kind: CredentialKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_seed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// W11 初始化结果：恢复密钥**只此一次**返回展示形态（S3），UI 必须当场提示离线保存。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitView {
    pub recovery_display: String,
}

/* ---------------- 装配与视图映射（机械转换，无业务判断） ---------------- */

fn to_view(entry: &Entry) -> EntryView {
    EntryView {
        uuid: uuid_hex(&entry.uuid),
        title: entry.title.clone(),
        account_ref: entry.account_ref.clone(),
        kind: entry.kind,
        username: entry.username.clone(),
        url: entry.url.clone(),
        has_secret: entry.secret.is_some(),
        has_totp: entry.totp_seed.is_some(),
    }
}

fn to_detail(entry: &Entry) -> EntryDetail {
    EntryDetail {
        uuid: uuid_hex(&entry.uuid),
        title: entry.title.clone(),
        account_ref: entry.account_ref.clone(),
        kind: entry.kind,
        username: entry.username.clone(),
        secret: entry.secret.clone(),
        totp_seed: entry.totp_seed.clone(),
        url: entry.url.clone(),
        note: entry.note.clone(),
    }
}

fn apply_input(entry: &mut Entry, input: &EntryInput) {
    entry.title = input.title.clone();
    entry.account_ref = input.account_ref.clone();
    entry.kind = input.kind;
    entry.username = input.username.clone();
    entry.secret = input.secret.clone();
    entry.totp_seed = input.totp_seed.clone();
    entry.url = input.url.clone();
    entry.note = input.note.clone();
}

pub fn uuid_hex(uuid: &[u8; 16]) -> String {
    uuid.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn uuid_from_hex(text: &str) -> CmdResult<[u8; 16]> {
    if text.len() != 32 || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("条目标识非法".into());
    }
    let mut uuid = [0u8; 16];
    for (index, slot) in uuid.iter_mut().enumerate() {
        *slot = u8::from_str_radix(text.get(2 * index..2 * index + 2).unwrap_or_default(), 16)
            .map_err(|_| "条目标识非法".to_string())?;
    }
    Ok(uuid)
}

/// 就绪态判定（托盘三态驱动源之一；S2 的自动锁策略待 core，见 README）。
pub fn is_unlocked(services: &HostState) -> bool {
    services.vault.lock().map(|session| session.is_some()).unwrap_or(false)
}

/// 在就绪态上执行一段库操作；未解锁一律报错（不猜、不自动解锁）。
fn with_vault<T>(
    services: &HostState,
    operation: impl FnOnce(&mut VaultSession) -> CmdResult<T>,
) -> CmdResult<T> {
    let mut guard = services
        .vault
        .lock()
        .map_err(|_| "库状态互斥锁中毒".to_string())?;
    match guard.as_mut() {
        Some(session) => operation(session),
        None => Err("库未解锁".into()),
    }
}

/// 保存活动库（DPAPI 外壳）+ 审计馈给（fail-closed）。
fn save_and_audit(services: &HostState, session: &VaultSession) -> CmdResult<()> {
    session
        .save(&mut services.vault_store(), &services.entropy)
        .map_err(|error| error.to_string())?;
    record(
        services,
        AuditAction::VaultWrite,
        AuditResult::Ok,
        None,
        None,
        &[("target", "vault")],
    )
}

/* ---------------- 纯函数（命令 → core 的全部路径） ---------------- */

/// W11 建库（A1 + S1 + S3）：创建库 → 活动库落盘 → 生成恢复密钥并 DPAPI 封装保存。
pub fn create(services: &HostState, name: &str, password: &str) -> CmdResult<InitView> {
    if services.vault_store().load().map_err(|e| e.to_string())?.is_some() {
        return Err("库已存在，改用解锁".into());
    }
    let session = VaultSession::create(
        name,
        password.as_bytes(),
        &ab_crypto::KdfParams::default(),
        &services.entropy,
    )
    .map_err(|error| error.to_string())?;

    let recovery = RecoveryKey::generate(&services.entropy).map_err(|error| error.to_string())?;
    session
        .save(&mut services.vault_store(), &services.entropy)
        .map_err(|error| error.to_string())?;
    services.store_recovery(&recovery)?;

    // fail-closed：写账失败则不进入就绪态。
    record(
        services,
        AuditAction::VaultWrite,
        AuditResult::Ok,
        None,
        None,
        &[("target", "vault-create")],
    )?;
    record(
        services,
        AuditAction::Unlock,
        AuditResult::Ok,
        None,
        None,
        &[("mode", "create")],
    )?;
    *services.vault.lock().map_err(|_| "库状态互斥锁中毒".to_string())? = Some(session);
    Ok(InitView {
        recovery_display: recovery.display(),
    })
}

/// S1 解锁：core 校验口令 → 审计（fail-closed）→ 进入就绪态。
pub fn unlock(services: &HostState, password: &str) -> CmdResult<()> {
    let session = match VaultSession::unlock(&mut services.vault_store(), password.as_bytes()) {
        Ok(session) => session,
        Err(error) => {
            let message = error.to_string();
            // 拒绝路径同样入账（写账失败以审计错误为准，fail-closed 口径同 ab-broker）。
            record(
                services,
                AuditAction::Unlock,
                AuditResult::Denied,
                None,
                None,
                &[("reason", "bad_credentials")],
            )?;
            return Err(message);
        }
    };
    record(
        services,
        AuditAction::Unlock,
        AuditResult::Ok,
        None,
        None,
        &[("mode", "password")],
    )?;
    *services.vault.lock().map_err(|_| "库状态互斥锁中毒".to_string())? = Some(session);
    Ok(())
}

/// S7 锁定：清就绪态（明文随 `VaultSession` Drop 清零）+ 审计。
pub fn lock(services: &HostState) -> CmdResult<()> {
    let mut guard = services
        .vault
        .lock()
        .map_err(|_| "库状态互斥锁中毒".to_string())?;
    if guard.is_none() {
        return Ok(());
    }
    *guard = None;
    drop(guard);
    record(
        services,
        AuditAction::Lock,
        AuditResult::Ok,
        None,
        None,
        &[],
    )
}

/// A1.1 条目列表（无秘密字段）。
pub fn list_entries(services: &HostState) -> CmdResult<Vec<EntryView>> {
    with_vault(services, |session| {
        Ok(session.vault.entries().iter().map(|entry| to_view(entry)).collect())
    })
}

/// A1.5 回收站列表。
pub fn list_trash(services: &HostState) -> CmdResult<Vec<EntryView>> {
    with_vault(services, |session| {
        Ok(session.vault.trash_entries().iter().map(to_view).collect())
    })
}

/// A1.1 条目明细（含秘密，编辑 / 填充用）。
pub fn get_entry(services: &HostState, uuid: &str) -> CmdResult<EntryDetail> {
    let uuid = uuid_from_hex(uuid)?;
    with_vault(services, |session| {
        session
            .vault
            .find(&uuid)
            .map(to_detail)
            .ok_or_else(|| "条目不存在".to_string())
    })
}

/// A1.1 新建条目（全字段含自定义字段）。
pub fn add_entry(services: &HostState, input: EntryInput) -> CmdResult<EntryView> {
    with_vault(services, |session| {
        let mut entry = Entry::new(
            &input.title,
            &input.account_ref,
            input.kind,
            &services.entropy,
        )
        .map_err(|error| error.to_string())?;
        apply_input(&mut entry, &input);
        let view = to_view(&entry);
        session.vault.add(entry).map_err(|error| error.to_string())?;
        save_and_audit(services, session)?;
        Ok(view)
    })
}

/// A1.1 编辑条目。
pub fn update_entry(
    services: &HostState,
    uuid: &str,
    input: EntryInput,
) -> CmdResult<EntryView> {
    let uuid = uuid_from_hex(uuid)?;
    with_vault(services, |session| {
        let mut entry = session
            .vault
            .find(&uuid)
            .cloned()
            .ok_or_else(|| "条目不存在".to_string())?;
        apply_input(&mut entry, &input);
        let view = to_view(&entry);
        session.vault.update(entry).map_err(|error| error.to_string())?;
        save_and_audit(services, session)?;
        Ok(view)
    })
}

/// A1.5 软删除入回收站。
pub fn trash_entry(services: &HostState, uuid: &str) -> CmdResult<()> {
    let uuid = uuid_from_hex(uuid)?;
    with_vault(services, |session| {
        session.vault.trash(uuid).map_err(|error| error.to_string())?;
        save_and_audit(services, session)
    })
}

/// A1.5 从回收站恢复。
pub fn restore_entry(services: &HostState, uuid: &str) -> CmdResult<()> {
    let uuid = uuid_from_hex(uuid)?;
    with_vault(services, |session| {
        session.vault.restore(uuid).map_err(|error| error.to_string())?;
        save_and_audit(services, session)
    })
}

/// A1.5 彻底删除（清空回收站）。
pub fn purge_trash(services: &HostState) -> CmdResult<()> {
    with_vault(services, |session| {
        session.vault.purge_trash();
        save_and_audit(services, session)
    })
}

/// 供恢复路径替换就绪态（A6.9 / A8.3 恢复后进入就绪态）。
pub fn adopt_session(services: &HostState, session: VaultSession) -> CmdResult<()> {
    session
        .save(&mut services.vault_store(), &services.entropy)
        .map_err(|error| error.to_string())?;
    record(
        services,
        AuditAction::Restore,
        AuditResult::Ok,
        None,
        None,
        &[("target", "vault")],
    )?;
    *services.vault.lock().map_err(|_| "库状态互斥锁中毒".to_string())? = Some(session);
    Ok(())
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn vault_create(
    state: tauri::State<'_, HostState>,
    name: String,
    password: String,
) -> CmdResult<InitView> {
    create(&state, &name, &password)
}

#[tauri::command]
pub fn vault_unlock(state: tauri::State<'_, HostState>, password: String) -> CmdResult<()> {
    unlock(&state, &password)
}

#[tauri::command]
pub fn vault_lock(state: tauri::State<'_, HostState>) -> CmdResult<()> {
    lock(&state)
}

#[tauri::command]
pub fn vault_is_unlocked(state: tauri::State<'_, HostState>) -> bool {
    is_unlocked(&state)
}

#[tauri::command]
pub fn vault_list_entries(state: tauri::State<'_, HostState>) -> CmdResult<Vec<EntryView>> {
    list_entries(&state)
}

#[tauri::command]
pub fn vault_list_trash(state: tauri::State<'_, HostState>) -> CmdResult<Vec<EntryView>> {
    list_trash(&state)
}

#[tauri::command]
pub fn vault_get_entry(state: tauri::State<'_, HostState>, uuid: String) -> CmdResult<EntryDetail> {
    get_entry(&state, &uuid)
}

#[tauri::command]
pub fn vault_add_entry(
    state: tauri::State<'_, HostState>,
    input: EntryInput,
) -> CmdResult<EntryView> {
    add_entry(&state, input)
}

#[tauri::command]
pub fn vault_update_entry(
    state: tauri::State<'_, HostState>,
    uuid: String,
    input: EntryInput,
) -> CmdResult<EntryView> {
    update_entry(&state, &uuid, input)
}

#[tauri::command]
pub fn vault_trash_entry(state: tauri::State<'_, HostState>, uuid: String) -> CmdResult<()> {
    trash_entry(&state, &uuid)
}

#[tauri::command]
pub fn vault_restore_entry(state: tauri::State<'_, HostState>, uuid: String) -> CmdResult<()> {
    restore_entry(&state, &uuid)
}

#[tauri::command]
pub fn vault_purge_trash(state: tauri::State<'_, HostState>) -> CmdResult<()> {
    purge_trash(&state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{HostLayout, HostState, HostSignatureVerifier};
    use ab_audit::{AuditFilter, AuditSink, InMemoryAuditSink};
    use std::sync::Arc;

    /// 假审计出口（core `AuditSink` 注入 trait 的假实现）：可注入写账失败，
    /// 用于断言命令 → core 的 fail-closed 口径。
    struct FakeAudit {
        inner: InMemoryAuditSink,
        fail: bool,
    }
    impl AuditSink for FakeAudit {
        fn record_or_fail(&self, event: ab_audit::AuditEvent) -> Result<(), ab_audit::AuditError> {
            if self.fail {
                return Err(ab_audit::AuditError::Unavailable);
            }
            self.inner.record_or_fail(event)
        }
        fn query(
            &self,
            filter: &AuditFilter,
        ) -> Result<Vec<ab_audit::AuditEvent>, ab_audit::AuditError> {
            self.inner.query(filter)
        }
        fn count(&self) -> Result<usize, ab_audit::AuditError> {
            self.inner.count()
        }
    }

    fn test_root() -> std::path::PathBuf {
        crate::host::test_root("aibridge-cmd-test")
    }

    fn test_services_at(root: &std::path::Path, audit: Arc<dyn AuditSink>) -> HostState {
        HostState::build_with(
            "pc-test",
            HostLayout {
                root: root.to_path_buf(),
            },
            HostSignatureVerifier::new(None, None),
            audit,
        )
    }

    fn test_services() -> HostState {
        let root = test_root();
        let _ = std::fs::remove_dir_all(&root);
        test_services_at(&root, Arc::new(InMemoryAuditSink::new()))
    }

    fn input(title: &str) -> EntryInput {
        EntryInput {
            title: title.into(),
            account_ref: "acc-1".into(),
            kind: CredentialKind::Password,
            username: Some("alice".into()),
            secret: Some("p@ss".into()),
            totp_seed: Some("JBSWY3DPEHPK3PXP".into()),
            url: Some("https://github.com/login".into()),
            note: Some("备注".into()),
        }
    }

    #[test]
    fn create_unlock_lock_lifecycle_records_audit() {
        let services = test_services();
        assert!(!is_unlocked(&services));

        let init = create(&services, "个人", "主口令").expect("建库失败");
        assert_eq!(init.recovery_display.len(), 64 + 7, "S3 展示形态：64 hex + 7 空格");
        assert!(is_unlocked(&services), "建库后应进入就绪态");

        lock(&services).expect("锁定失败");
        assert!(!is_unlocked(&services));
        assert_eq!(
            unlock(&services, "错误口令").unwrap_err(),
            "KDBX 容器错误: BadCredentials"
        );
        assert!(!is_unlocked(&services), "失败的解锁不得进入就绪态");
        unlock(&services, "主口令").expect("解锁失败");
        assert!(is_unlocked(&services));

        let events = services
            .audit
            .query(&AuditFilter::default())
            .expect("查询失败");
        let actions: Vec<ab_audit::AuditAction> =
            events.iter().map(|event| event.action).collect();
        assert!(actions.contains(&AuditAction::Unlock));
        assert!(actions.contains(&AuditAction::Lock));
        assert!(actions.contains(&AuditAction::VaultWrite));
        assert!(events
            .iter()
            .any(|event| event.result == AuditResult::Denied));
    }

    #[test]
    fn entry_crud_roundtrip_and_secret_stays_out_of_views() {
        let services = test_services();
        create(&services, "个人", "主口令").expect("建库失败");

        let view = add_entry(&services, input("GitHub <main>")).expect("新建失败");
        assert!(view.has_secret && view.has_totp);
        assert_eq!(view.username.as_deref(), Some("alice"));

        let list = list_entries(&services).expect("列表失败");
        assert_eq!(list.len(), 1);
        let debug = format!("{list:?}");
        assert!(!debug.contains("p@ss"), "列表视图不得携带秘密");

        let detail = get_entry(&services, &view.uuid).expect("明细失败");
        assert_eq!(detail.secret.as_deref(), Some("p@ss"));
        assert_eq!(format!("{detail:?}").find("p@ss"), None, "Debug 必须脱敏（F7）");

        update_entry(&services, &view.uuid, input("GitHub <renamed>")).expect("编辑失败");
        assert_eq!(
            get_entry(&services, &view.uuid).expect("明细失败").title,
            "GitHub <renamed>"
        );

        trash_entry(&services, &view.uuid).expect("软删除失败");
        assert!(list_entries(&services).expect("列表失败").is_empty());
        assert_eq!(list_trash(&services).expect("回收站失败").len(), 1);

        restore_entry(&services, &view.uuid).expect("恢复失败");
        assert_eq!(list_entries(&services).expect("列表失败").len(), 1);

        trash_entry(&services, &view.uuid).expect("软删除失败");
        purge_trash(&services).expect("清空失败");
        assert!(list_trash(&services).expect("回收站失败").is_empty());
    }

    #[test]
    fn mutations_rejected_while_locked() {
        let services = test_services();
        assert_eq!(list_entries(&services).unwrap_err(), "库未解锁");
        assert_eq!(add_entry(&services, input("x")).unwrap_err(), "库未解锁");
    }

    #[test]
    fn unlock_is_fail_closed_when_audit_sink_fails() {
        // 先用好审计出口建库（盘上有库），再换「写账必败」的假审计出口：
        // 解锁必须被拒绝且不进入就绪态（命令 → core 的 fail-closed 断言）。
        let root = test_root();
        let _ = std::fs::remove_dir_all(&root);
        let good = test_services_at(&root, Arc::new(InMemoryAuditSink::new()));
        create(&good, "个人", "主口令").expect("建库失败");
        lock(&good).expect("锁定失败");

        let failing = test_services_at(
            &root,
            Arc::new(FakeAudit {
                inner: InMemoryAuditSink::new(),
                fail: true,
            }),
        );
        let error = unlock(&failing, "主口令").unwrap_err();
        assert_eq!(error, "审计出口不可用", "审计写入失败必须拒绝解锁");
        assert!(!is_unlocked(&failing), "fail-closed：不得进入就绪态");
    }
}
