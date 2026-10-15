//! W7 备份面板 / W13 恢复向导 / W14 演练向导 的命令接线（A6.1–A6.5、A6.7–A6.9）。
//!
//! 规则归属：备份**内容**的加密、恢复密钥副本（A6.4）、免主口令恢复（A6.9）全部在 core
//! （`ab_vault::VaultSession::save` / `recovery_copy` / `from_backup` / `RecoveryKey::open_copy`）；
//! 本层只做编排（文件布局 + 校验编排 + 审计馈给），不含加密或保留规则。
//!
//! core 缺口（见 README，不在本层补第二份规则）：
//! - A6.6 滚动保留（月快照 12 份 / 变更快照 10 份）：core 无保留策略 API，未实现；
//! - A6.7 免口令校验：主备份解密校验需要主口令或等价 API（如 `VaultSession::verify_self`），
//!   core 未提供 —— 无口令时只校验恢复密钥副本并如实回报。
//!
//! 校验口径（A6.7，实现选择，待产品确认）：每次备份自动「解密恢复密钥副本 + 逐条目集比对」
//! 作为完整解密校验；主口令可用时再校验主备份。`last_verified_at` 记录最近一次
//! 解密校验通过时刻（备份面板展示）。

use std::path::PathBuf;

use ab_audit::{AuditAction, AuditResult};
use ab_vault::{RecoveryKey, VaultError, VaultSession, VaultStore};
use serde::{Deserialize, Serialize};

use super::{now_unix_secs, now_rfc3339, record, CmdResult};
use crate::host::HostState;

/// 备份报告（W7 展示；只回文件名，不回完整路径）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupReport {
    pub main_file: String,
    pub recovery_file: String,
    pub entry_count: usize,
    /// 主备份解密校验：`None` = 未校验（无主口令），`Some(false)` 不会出现（失败即报错）。
    pub main_verified: Option<bool>,
    /// 恢复密钥副本解密校验（每次备份必做）。
    pub recovery_verified: bool,
    /// A6.7 最后验证时间（RFC 3339）。
    pub last_verified_at: Option<String>,
}

/// 备份状态（W7：最后验证时间 + 文件清单）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatusView {
    pub last_backup_at: Option<String>,
    pub last_verified_at: Option<String>,
    pub files: Vec<String>,
}

/// A6.8 恢复演练报告（沙盒打开备份并比对，不替换活动库）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrillReport {
    pub checked_files: Vec<String>,
    pub main_opened: Option<bool>,
    pub recovery_opened: bool,
    pub entry_count: usize,
    /// 与当前库条目集（UUID 集合）一致。
    pub entries_match: bool,
}

/// 明文文件存储（备份本身是 KDBX4 加密容器，A6.4「永不落明文」由 core 的容器加密保证）。
struct FileStore {
    path: PathBuf,
}

impl VaultStore for FileStore {
    fn read(&mut self) -> Result<Option<Vec<u8>>, VaultError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(VaultError::Store),
        }
    }

    fn write(&mut self, data: &[u8]) -> Result<(), VaultError> {
        std::fs::create_dir_all(self.path.parent().unwrap_or(&self.path))
            .map_err(|_| VaultError::Store)?;
        std::fs::write(&self.path, data).map_err(|_| VaultError::Store)
    }
}

fn backup_dir(services: &HostState) -> PathBuf {
    services.paths.backup_dir()
}

/// 最新备份文件对（按文件名中的纪元秒取最大；无备份返回 None）。
fn latest_pair(services: &HostState) -> Option<(PathBuf, PathBuf)> {
    let dir = backup_dir(services);
    let mut stamps: Vec<(i64, String)> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let stamp = name
                .strip_prefix("vault-backup-")?
                .strip_suffix(".kdbx")?
                .parse::<i64>()
                .ok()?;
            Some((stamp, name))
        })
        .collect();
    stamps.sort();
    let (_, name) = stamps.pop()?;
    let recovery_name = name.replace(".kdbx", ".recovery.kdbx");
    Some((dir.join(&name), dir.join(recovery_name)))
}

/// 条目 UUID 集合比对（A6.8「比对条目数/完整性」的机械化口径）。
fn same_entry_set(a: &VaultSession, b: &VaultSession) -> bool {
    let mut left: Vec<[u8; 16]> = a.vault.entries().iter().map(|entry| entry.uuid).collect();
    let mut right: Vec<[u8; 16]> = b.vault.entries().iter().map(|entry| entry.uuid).collect();
    left.sort_unstable();
    right.sort_unstable();
    left == right
}

fn recovery_key(services: &HostState) -> CmdResult<RecoveryKey> {
    let guard = services
        .recovery
        .lock()
        .map_err(|_| "恢复密钥互斥锁中毒".to_string())?;
    let display = guard
        .as_ref()
        .map(|key| key.display())
        .ok_or_else(|| "恢复密钥不可用（S3：初始化时生成并离线保存）".to_string())?;
    drop(guard);
    // core 未提供 RecoveryKey 的 Clone / 字节序列化口（见 README 缺口），
    // 由展示形态重建同一把（展示形态本身就是 core 的公开往返口）。
    RecoveryKey::from_display(&display).map_err(|error| error.to_string())
}

/* ---------------- 纯函数 ---------------- */

/// A6.1/A6.2/A6.4 备份：主备份（主口令）+ 恢复密钥加密副本 + 自动解密校验（A6.7）。
pub fn run(services: &HostState, password: Option<&str>) -> CmdResult<BackupReport> {
    let session_guard = services
        .vault
        .lock()
        .map_err(|_| "库状态互斥锁中毒".to_string())?;
    let session = session_guard.as_ref().ok_or("库未解锁")?;

    let stamp = now_unix_secs();
    let dir = backup_dir(services);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let main_path = dir.join(format!("vault-backup-{stamp}.kdbx"));
    let recovery_path = dir.join(format!("vault-backup-{stamp}.recovery.kdbx"));

    // 主备份：core 打包（复用 transformed，不重跑 Argon2）。
    session
        .save(&mut FileStore { path: main_path.clone() }, &services.entropy)
        .map_err(|error| error.to_string())?;
    let main_bytes = std::fs::read(&main_path).map_err(|error| error.to_string())?;

    // A6.4 恢复密钥加密副本：独立盐与流密钥，口令 = 恢复密钥域派生材料。
    let recovery = recovery_key(services)?;
    let copy_bytes = session
        .recovery_copy(&recovery, &services.entropy)
        .map_err(|error| error.to_string())?;
    std::fs::write(&recovery_path, &copy_bytes).map_err(|error| error.to_string())?;

    // A6.7 自动解密校验（见文件头「校验口径」）。
    let copy_restored = recovery
        .open_copy(&copy_bytes)
        .map_err(|error| error.to_string())?;
    let entry_count = session.vault.entries().len();
    if !same_entry_set(session, &copy_restored) || copy_restored.vault.entries().len() != entry_count
    {
        return Err("恢复密钥副本校验失败：条目集不一致".into());
    }
    let main_verified = match password {
        Some(password) => {
            let restored = VaultSession::from_backup(password.as_bytes(), &main_bytes)
                .map_err(|error| error.to_string())?;
            if !same_entry_set(session, &restored) {
                return Err("主备份校验失败：条目集不一致".into());
            }
            Some(true)
        }
        None => None,
    };

    let verified_at = now_rfc3339();
    let mut meta = services.backup_meta();
    meta.last_backup_at = Some(verified_at.clone());
    meta.last_verified_at = Some(verified_at.clone());
    services.write_backup_meta(&meta)?;

    // fail-closed：写账失败即报错（备份已落盘，但不谎报成功）。
    record(
        services,
        AuditAction::Export,
        AuditResult::Ok,
        None,
        None,
        &[("target", "backup"), ("entries", &entry_count.to_string())],
    )?;

    Ok(BackupReport {
        main_file: main_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        recovery_file: recovery_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
        entry_count,
        main_verified,
        recovery_verified: true,
        last_verified_at: meta.last_verified_at,
    })
}

/// W7 状态：最后验证时间 + 备份文件清单。
pub fn status(services: &HostState) -> CmdResult<BackupStatusView> {
    let meta = services.backup_meta();
    let mut files: Vec<String> = std::fs::read_dir(backup_dir(services))
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("vault-backup-"))
        .collect();
    files.sort();
    Ok(BackupStatusView {
        last_backup_at: meta.last_backup_at,
        last_verified_at: meta.last_verified_at,
        files,
    })
}

/// A6.8 恢复演练：沙盒打开最新备份（不替换活动库）并比对条目集。
pub fn drill(services: &HostState, password: Option<&str>) -> CmdResult<DrillReport> {
    let session_guard = services
        .vault
        .lock()
        .map_err(|_| "库状态互斥锁中毒".to_string())?;
    let session = session_guard.as_ref().ok_or("库未解锁")?;
    let (main_path, recovery_path) = latest_pair(services).ok_or("无可用备份")?;

    let mut checked_files = Vec::new();
    let mut opened: Vec<VaultSession> = Vec::new();
    let mut main_opened = None;

    if let Some(password) = password {
        let bytes = std::fs::read(&main_path).map_err(|error| error.to_string())?;
        match VaultSession::from_backup(password.as_bytes(), &bytes) {
            Ok(restored) => {
                opened.push(restored);
                main_opened = Some(true);
            }
            Err(_) => main_opened = Some(false),
        }
        checked_files.push(
            main_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
    }

    let copy_bytes = std::fs::read(&recovery_path).map_err(|error| error.to_string())?;
    let recovery = recovery_key(services)?;
    let copy_restored = recovery
        .open_copy(&copy_bytes)
        .map_err(|error| error.to_string())?;
    checked_files.push(
        recovery_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default(),
    );

    let entries_match = same_entry_set(session, &copy_restored)
        && opened.iter().all(|restored| same_entry_set(session, restored));
    let entry_count = session.vault.entries().len();

    record(
        services,
        AuditAction::Restore,
        AuditResult::Ok,
        None,
        None,
        &[("target", "backup-drill")],
    )?;

    Ok(DrillReport {
        checked_files,
        main_opened,
        recovery_opened: true,
        entry_count,
        entries_match,
    })
}

/// A6.9 恢复：从最新主备份恢复（主口令路径）。
pub fn restore_with_password(services: &HostState, password: &str) -> CmdResult<()> {
    let (main_path, _) = latest_pair(services).ok_or("无可用备份")?;
    let bytes = std::fs::read(&main_path).map_err(|error| error.to_string())?;
    let session = VaultSession::from_backup(password.as_bytes(), &bytes)
        .map_err(|error| error.to_string())?;
    super::library::adopt_session(services, session)
}

/// A6.9 恢复：从恢复密钥副本恢复（S3 唯一后门，免主口令）。
pub fn restore_with_recovery(services: &HostState, recovery_display: &str) -> CmdResult<()> {
    let (_, recovery_path) = latest_pair(services).ok_or("无可用备份")?;
    let bytes = std::fs::read(&recovery_path).map_err(|error| error.to_string())?;
    let recovery = RecoveryKey::from_display(recovery_display).map_err(|error| error.to_string())?;
    let session = recovery.open_copy(&bytes).map_err(|error| error.to_string())?;
    super::library::adopt_session(services, session)
}

/* ---------------- Tauri 命令包装（只转发） ---------------- */

#[tauri::command]
pub fn backup_run(
    state: tauri::State<'_, HostState>,
    password: Option<String>,
) -> CmdResult<BackupReport> {
    run(&state, password.as_deref())
}

#[tauri::command]
pub fn backup_status(state: tauri::State<'_, HostState>) -> CmdResult<BackupStatusView> {
    status(&state)
}

#[tauri::command]
pub fn backup_drill(
    state: tauri::State<'_, HostState>,
    password: Option<String>,
) -> CmdResult<DrillReport> {
    drill(&state, password.as_deref())
}

#[tauri::command]
pub fn backup_restore_with_password(
    state: tauri::State<'_, HostState>,
    password: String,
) -> CmdResult<()> {
    restore_with_password(&state, &password)
}

#[tauri::command]
pub fn backup_restore_with_recovery(
    state: tauri::State<'_, HostState>,
    recovery_display: String,
) -> CmdResult<()> {
    restore_with_recovery(&state, &recovery_display)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::library;
    use crate::host::{HostLayout, HostState, HostSignatureVerifier};
    use ab_audit::InMemoryAuditSink;
    use ab_contract::CredentialKind;
    use std::sync::Arc;

    fn test_services() -> HostState {
        let root = crate::host::test_root("aibridge-backup-test");
        let _ = std::fs::remove_dir_all(&root);
        HostState::build_with(
            "pc-test",
            HostLayout { root },
            HostSignatureVerifier::new(None, None),
            Arc::new(InMemoryAuditSink::new()),
        )
    }

    fn input(title: &str) -> library::EntryInput {
        library::EntryInput {
            title: title.into(),
            account_ref: "acc-1".into(),
            kind: CredentialKind::Password,
            username: None,
            secret: Some("secret".into()),
            totp_seed: None,
            url: None,
            note: None,
        }
    }

    #[test]
    fn backup_run_writes_main_and_recovery_copy_and_verifies() {
        let services = test_services();
        library::create(&services, "个人", "主口令").expect("建库失败");
        library::add_entry(&services, input("A")).expect("新建失败");
        library::add_entry(&services, input("B")).expect("新建失败");

        let report = run(&services, Some("主口令")).expect("备份失败");
        assert_eq!(report.entry_count, 2);
        assert_eq!(report.main_verified, Some(true));
        assert!(report.recovery_verified);
        assert!(report.last_verified_at.is_some());

        // 两份文件都在托管目录，且互不相同（独立盐/流密钥）。
        let dir = services.paths.backup_dir();
        let main = std::fs::read(dir.join(&report.main_file)).expect("主备份读取失败");
        let copy = std::fs::read(dir.join(&report.recovery_file)).expect("副本读取失败");
        assert_ne!(main, copy, "副本必须独立成盐，不是主备份的拷贝");

        // A6.8 演练：条目集一致。
        let drill_report = drill(&services, Some("主口令")).expect("演练失败");
        assert!(drill_report.recovery_opened);
        assert_eq!(drill_report.main_opened, Some(true));
        assert!(drill_report.entries_match);
        assert_eq!(drill_report.entry_count, 2);

        // A6.7 状态面：最后验证时间可读。
        let status = status(&services).expect("状态失败");
        assert!(status.last_verified_at.is_some());
        assert!(status.files.contains(&report.main_file));
    }

    #[test]
    fn backup_without_password_verifies_recovery_copy_only() {
        let services = test_services();
        library::create(&services, "个人", "主口令").expect("建库失败");

        let report = run(&services, None).expect("备份失败");
        assert_eq!(report.main_verified, None, "无口令不得谎报主备份已校验");
        assert!(report.recovery_verified);
    }

    #[test]
    fn restore_paths_roundtrip_through_recovery_copy() {
        let services = test_services();
        library::create(&services, "个人", "主口令").expect("建库失败");
        let view = library::add_entry(&services, input("A")).expect("新建失败");
        let report = run(&services, Some("主口令")).expect("备份失败");

        // 破坏活动库（软删 + 彻底删）后从恢复密钥副本恢复（免主口令）。
        library::trash_entry(&services, &view.uuid).expect("软删除失败");
        library::purge_trash(&services).expect("清空失败");
        assert!(library::list_entries(&services).expect("列表失败").is_empty());

        let recovery_display = services
            .recovery
            .lock()
            .unwrap()
            .as_ref()
            .map(|key| key.display())
            .expect("恢复密钥应回灌");
        restore_with_recovery(&services, &recovery_display).expect("恢复失败");
        assert_eq!(library::list_entries(&services).expect("列表失败").len(), 1);

        // 主口令路径同样可恢复。
        library::purge_trash(&services).expect("清空失败");
        library::trash_entry(&services, &view.uuid).expect("软删除失败");
        restore_with_password(&services, "主口令").expect("恢复失败");
        assert_eq!(library::list_entries(&services).expect("列表失败").len(), 1);
        assert_eq!(report.entry_count, 1);
    }

    #[test]
    fn drill_detects_mismatch_and_missing_backups() {
        let services = test_services();
        library::create(&services, "个人", "主口令").expect("建库失败");
        assert_eq!(drill(&services, None).unwrap_err(), "无可用备份");

        run(&services, None).expect("备份失败");
        // 备份后新增条目 → 演练比对必须报不一致。
        library::add_entry(&services, input("late")).expect("新建失败");
        let report = drill(&services, None).expect("演练失败");
        assert!(!report.entries_match, "条目集不一致必须如实报告");
    }
}
