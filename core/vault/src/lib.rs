//! 保险库：KDBX4 格式级兼容的条目存储（灾备方向：文件可被 KeePassXC 直接打开）。
//! 存取走 `VaultStore` 注入，核心不碰文件系统；保存复用 transformed，不重跑 Argon2。

mod generator;
mod kdbx;
mod totp;
mod xml;

pub use generator::{
    generate_passphrase, generate_secret, GeneratedSecret, GeneratorSpec, PassphraseSpec,
};
pub use totp::{totp_code, TotpAlgorithm, TotpParams};

use std::fmt;

use ab_contract::CredentialKind;
use ab_crypto::{derive_domain, random_bytes, DomainLabel, Entropy, KdfParams, Zeroizing};

pub use kdbx::KdbxError;

const REDACTED: &str = "[REDACTED]";
const ROOT_GROUP: &str = "Root";
const RECYCLE_BIN_GROUP: &str = "Recycle Bin";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultError {
    Store,
    Kdbx(KdbxError),
    WeakKdf,
    Malformed,
    NotFound,
    NoSuchEntry,
    DuplicateEntry,
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultError::Store => write!(f, "存储读写失败"),
            VaultError::Kdbx(e) => write!(f, "KDBX 容器错误: {e:?}"),
            VaultError::WeakKdf => write!(f, "KDF 参数低于基线"),
            VaultError::Malformed => write!(f, "保险库数据损坏或格式不合法"),
            VaultError::NotFound => write!(f, "保险库不存在"),
            VaultError::NoSuchEntry => write!(f, "条目不存在"),
            VaultError::DuplicateEntry => write!(f, "条目 UUID 重复"),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<KdbxError> for VaultError {
    fn from(e: KdbxError) -> Self {
        VaultError::Kdbx(e)
    }
}

/// 持久化注入：宿主层决定存哪（文件 / 测试内存），核心只认字节。
pub trait VaultStore {
    fn read(&mut self) -> Result<Option<Vec<u8>>, VaultError>;
    fn write(&mut self, data: &[u8]) -> Result<(), VaultError>;
}

#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    pub uuid: [u8; 16],
    pub title: String,
    pub account_ref: String,
    pub kind: CredentialKind,
    pub username: Option<String>,
    pub secret: Option<String>,
    pub totp_seed: Option<String>,
    pub url: Option<String>,
    pub note: Option<String>,
    /// A1.1 标签（空即省略，不落空字段）。
    pub tags: Vec<String>,
    /// A1.2 收藏。
    pub favorite: bool,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("uuid", &self.uuid)
            .field("title", &self.title)
            .field("account_ref", &self.account_ref)
            .field("kind", &self.kind)
            .field("username", &self.username)
            .field("secret", &self.secret.as_deref().map(|_| REDACTED))
            .field("totp_seed", &self.totp_seed.as_deref().map(|_| REDACTED))
            .field("url", &self.url)
            .field("note", &self.note.as_deref().map(|_| REDACTED))
            .field("tags", &self.tags)
            .field("favorite", &self.favorite)
            .finish()
    }
}

impl Entry {
    pub fn new(
        title: &str,
        account_ref: &str,
        kind: CredentialKind,
        entropy: &dyn Entropy,
    ) -> Result<Self, VaultError> {
        let mut uuid = [0u8; 16];
        random_bytes(entropy, &mut uuid).map_err(|_| VaultError::Malformed)?;
        Ok(Self {
            uuid,
            title: title.to_string(),
            account_ref: account_ref.to_string(),
            kind,
            username: None,
            secret: None,
            totp_seed: None,
            url: None,
            note: None,
            tags: Vec::new(),
            favorite: false,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Group {
    pub uuid: [u8; 16],
    pub name: String,
    pub entries: Vec<Entry>,
    pub groups: Vec<Group>,
}

/// A4.2 身份档案：一组默认账号（account_ref）的集合，整组切换。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityProfile {
    pub name: String,
    pub account_refs: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Vault {
    pub name: String,
    pub root: Group,
    pub recycle_bin: [u8; 16],
    /// A4.2 身份档案集合（随库持久化）。
    pub profiles: Vec<IdentityProfile>,
    /// 当前生效档案名（None = 未启用档案）。
    pub active_profile: Option<String>,
}

impl Vault {
    /// 新增/更新同名档案。
    pub fn set_profile(&mut self, profile: IdentityProfile) {
        match self.profiles.iter_mut().find(|p| p.name == profile.name) {
            Some(existing) => *existing = profile,
            None => self.profiles.push(profile),
        }
    }

    /// 删除档案；若为生效档案则一并清除生效标记。返回是否删到。
    pub fn remove_profile(&mut self, name: &str) -> bool {
        let before = self.profiles.len();
        self.profiles.retain(|p| p.name != name);
        if self.active_profile.as_deref() == Some(name) {
            self.active_profile = None;
        }
        self.profiles.len() != before
    }

    /// 整组切换：激活指定档案（须已存在）；None = 取消启用。
    pub fn set_active_profile(&mut self, name: Option<&str>) -> Result<(), VaultError> {
        match name {
            None => {
                self.active_profile = None;
                Ok(())
            }
            Some(name) => {
                if self.profiles.iter().any(|p| p.name == name) {
                    self.active_profile = Some(name.to_string());
                    Ok(())
                } else {
                    Err(VaultError::NotFound)
                }
            }
        }
    }
}

/// A1.3 密码体检（纯本地计算）——已存口令的强度估值（bit）。
/// 公式：长度 × log2(实测字符类并集)；类容量：小写 26 / 大写 26 / 数字 10 /
/// ASCII 标点 32 / 其它（非 ASCII）64（估值，仅用于强度条分档）。
pub fn estimate_strength_bits(secret: &str) -> f64 {
    let mut classes = 0u32;
    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digit = false;
    let mut has_punct = false;
    let mut has_other = false;
    for ch in secret.chars() {
        if ch.is_ascii_lowercase() {
            has_lower = true;
        } else if ch.is_ascii_uppercase() {
            has_upper = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        } else if ch.is_ascii_graphic() {
            has_punct = true;
        } else {
            has_other = true;
        }
    }
    if has_lower {
        classes += 26;
    }
    if has_upper {
        classes += 26;
    }
    if has_digit {
        classes += 10;
    }
    if has_punct {
        classes += 32;
    }
    if has_other {
        classes += 64;
    }
    if classes == 0 {
        return 0.0;
    }
    secret.chars().count() as f64 * (classes as f64).log2()
}

/// A1.3 重复使用检测的分组结果（只含 uuid，零秘密外泄）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReuseGroup {
    pub member_uuids: Vec<[u8; 16]>,
}

/// A1.3 重复使用检测：同一非空秘密出现在多条条目 → 每组返回其 uuid 列表。
/// 纯本地计算；结果不含任何秘密内容。
pub fn find_reused_secrets<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Vec<ReuseGroup> {
    let mut by_secret: Vec<(&str, Vec<[u8; 16]>)> = Vec::new();
    for entry in entries {
        let Some(secret) = entry.secret.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        match by_secret.iter_mut().find(|(s, _)| *s == secret) {
            Some((_, uuids)) => uuids.push(entry.uuid),
            None => by_secret.push((secret, vec![entry.uuid])),
        }
    }
    by_secret
        .into_iter()
        .filter(|(_, uuids)| uuids.len() >= 2)
        .map(|(_, member_uuids)| ReuseGroup { member_uuids })
        .collect()
}

fn find_in<'a>(group: &'a Group, uuid: &[u8; 16]) -> Option<&'a Entry> {
    group
        .entries
        .iter()
        .find(|e| e.uuid == *uuid)
        .or_else(|| group.groups.iter().find_map(|g| find_in(g, uuid)))
}

fn find_in_mut<'a>(group: &'a mut Group, uuid: &[u8; 16]) -> Option<&'a mut Entry> {
    if let Some(pos) = group.entries.iter().position(|e| e.uuid == *uuid) {
        return group.entries.get_mut(pos);
    }
    group.groups.iter_mut().find_map(|g| find_in_mut(g, uuid))
}

fn find_group_mut<'a>(group: &'a mut Group, uuid: &[u8; 16]) -> Option<&'a mut Group> {
    if group.uuid == *uuid {
        return Some(group);
    }
    group.groups.iter_mut().find_map(|g| find_group_mut(g, uuid))
}

fn find_group<'a>(group: &'a Group, uuid: &[u8; 16]) -> Option<&'a Group> {
    if group.uuid == *uuid {
        return Some(group);
    }
    group.groups.iter().find_map(|g| find_group(g, uuid))
}

fn take_entry(group: &mut Group, uuid: &[u8; 16]) -> Option<Entry> {
    if let Some(pos) = group.entries.iter().position(|e| e.uuid == *uuid) {
        return Some(group.entries.remove(pos));
    }
    group.groups.iter_mut().find_map(|g| take_entry(g, uuid))
}

fn collect_entries<'a>(group: &'a Group, bin: &[u8; 16], out: &mut Vec<&'a Entry>) {
    out.extend(group.entries.iter());
    for g in &group.groups {
        if g.uuid != *bin {
            collect_entries(g, bin, out);
        }
    }
}

impl Vault {
    pub fn new(name: &str, entropy: &dyn Entropy) -> Result<Self, VaultError> {
        let mut root_uuid = [0u8; 16];
        let mut bin_uuid = [0u8; 16];
        random_bytes(entropy, &mut root_uuid).map_err(|_| VaultError::Malformed)?;
        random_bytes(entropy, &mut bin_uuid).map_err(|_| VaultError::Malformed)?;
        Ok(Self {
            name: name.to_string(),
            root: Group {
                uuid: root_uuid,
                name: ROOT_GROUP.to_string(),
                entries: Vec::new(),
                groups: vec![Group {
                    uuid: bin_uuid,
                    name: RECYCLE_BIN_GROUP.to_string(),
                    entries: Vec::new(),
                    groups: Vec::new(),
                }],
            },
            recycle_bin: bin_uuid,
            profiles: Vec::new(),
            active_profile: None,
        })
    }

    pub fn find(&self, uuid: &[u8; 16]) -> Option<&Entry> {
        find_in(&self.root, uuid)
    }

    /// 回收站之外的全部条目。
    pub fn entries(&self) -> Vec<&Entry> {
        let mut out = Vec::new();
        collect_entries(&self.root, &self.recycle_bin, &mut out);
        out
    }

    pub fn trash_entries(&self) -> &[Entry] {
        find_group(&self.root, &self.recycle_bin)
            .map(|bin| bin.entries.as_slice())
            .unwrap_or_default()
    }

    pub fn add(&mut self, entry: Entry) -> Result<(), VaultError> {
        if self.find(&entry.uuid).is_some() {
            return Err(VaultError::DuplicateEntry);
        }
        self.root.entries.push(entry);
        Ok(())
    }

    pub fn update(&mut self, entry: Entry) -> Result<(), VaultError> {
        match find_in_mut(&mut self.root, &entry.uuid) {
            Some(slot) => {
                *slot = entry;
                Ok(())
            }
            None => Err(VaultError::NoSuchEntry),
        }
    }

    pub fn trash(&mut self, uuid: [u8; 16]) -> Result<(), VaultError> {
        let entry = take_entry(&mut self.root, &uuid).ok_or(VaultError::NoSuchEntry)?;
        let bin = find_group_mut(&mut self.root, &self.recycle_bin).ok_or(VaultError::Malformed)?;
        bin.entries.push(entry);
        Ok(())
    }

    pub fn restore(&mut self, uuid: [u8; 16]) -> Result<(), VaultError> {
        let bin = find_group_mut(&mut self.root, &self.recycle_bin).ok_or(VaultError::Malformed)?;
        let entry = take_entry(bin, &uuid).ok_or(VaultError::NoSuchEntry)?;
        self.root.entries.push(entry);
        Ok(())
    }

    pub fn purge_trash(&mut self) {
        if let Some(bin) = find_group_mut(&mut self.root, &self.recycle_bin) {
            bin.entries.clear();
        }
    }
}

#[derive(Debug)]
pub struct VaultSession {
    pub vault: Vault,
    salt: Zeroizing<[u8; 32]>,
    transformed: Zeroizing<[u8; 32]>,
    kdf: KdfParams,
}

/// 恢复密钥（S3「唯一后门」）：初始化一次性生成、只展示一次、离线保存。
/// 它是 A6.4「恢复密钥加密副本」的口令来源——副本不需要主口令即可恢复（A6.9）。
/// 秘密 `Zeroizing` 持有；展示一次是 UI 纪律，core 不拦重复调用。
pub struct RecoveryKey(Zeroizing<[u8; 32]>);

impl RecoveryKey {
    pub fn generate(entropy: &dyn Entropy) -> Result<Self, VaultError> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        random_bytes(entropy, bytes.as_mut()).map_err(|_| VaultError::Malformed)?;
        Ok(Self(bytes))
    }

    /// 离线保存的展示形态：64 位小写 hex，按 8 位分组。
    pub fn display(&self) -> String {
        let hex: String = self.0.iter().map(|b| format!("{b:02x}")).collect();
        hex.as_bytes()
            .chunks(8)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// 从展示形态恢复（忽略空白）；长度或字符非法整体拒绝。
    pub fn from_display(text: &str) -> Result<Self, VaultError> {
        let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if cleaned.len() != 64 {
            return Err(VaultError::Malformed);
        }
        let mut bytes = Zeroizing::new([0u8; 32]);
        for (i, slot) in bytes.iter_mut().enumerate() {
            *slot = u8::from_str_radix(
                cleaned.get(2 * i..2 * i + 2).ok_or(VaultError::Malformed)?,
                16,
            )
            .map_err(|_| VaultError::Malformed)?;
        }
        Ok(Self(bytes))
    }

    /// A6.9 恢复路径：用恢复密钥打开 A6.4 加密副本（不需要主口令）。
    pub fn open_copy(&self, data: &[u8]) -> Result<VaultSession, VaultError> {
        let material = self.password_material()?;
        VaultSession::from_backup(&*material, data)
    }

    /// KDBX 口令材料：域分离派生（`aibridge/v1/recovery-key`），与主口令路径永不混用。
    fn password_material(&self) -> Result<Zeroizing<[u8; 32]>, VaultError> {
        derive_domain(&*self.0, None, DomainLabel::RecoveryKey).map_err(|_| VaultError::Malformed)
    }
}

// 手工 Debug：恢复密钥脱敏（F7 内存卫生）。
impl fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryKey [REDACTED]")
    }
}

impl VaultSession {
    pub fn create(
        name: &str,
        password: &[u8],
        kdf: &KdfParams,
        entropy: &dyn Entropy,
    ) -> Result<Self, VaultError> {
        let kdf = kdf.validated().map_err(|_| VaultError::WeakKdf)?;
        let mut salt = Zeroizing::new([0u8; 32]);
        random_bytes(entropy, salt.as_mut()).map_err(|_| VaultError::Malformed)?;
        let transformed = kdbx::transformed_key(password, &salt, &kdf)?;
        Ok(Self {
            vault: Vault::new(name, entropy)?,
            salt,
            transformed,
            kdf,
        })
    }

    pub fn unlock(store: &mut dyn VaultStore, password: &[u8]) -> Result<Self, VaultError> {
        let data = store.read()?.ok_or(VaultError::NotFound)?;
        Self::from_backup(password, &data)
    }

    /// 从备份字节解锁（口令路径与恢复路径共用的核心入口）。
    pub fn from_backup(password: &[u8], data: &[u8]) -> Result<Self, VaultError> {
        let unpacked = kdbx::unpack(password, data)?;
        let mut stream = xml::protected_stream(&unpacked.stream_key)?;
        let body = std::str::from_utf8(&unpacked.body).map_err(|_| VaultError::Malformed)?;
        let vault = xml::parse_document(body, &mut stream)?;
        Ok(Self {
            vault,
            salt: unpacked.salt,
            transformed: unpacked.transformed,
            kdf: unpacked.kdf,
        })
    }

    /// 重打包复用 transformed（不再重跑 Argon2），每次保存换新随机材料。
    pub fn save(&self, store: &mut dyn VaultStore, entropy: &dyn Entropy) -> Result<(), VaultError> {
        let mut stream_key = Zeroizing::new([0u8; 64]);
        random_bytes(entropy, stream_key.as_mut()).map_err(|_| VaultError::Malformed)?;
        let mut stream = xml::protected_stream(&stream_key)?;
        let body = xml::write_document(&self.vault, &mut stream)?;
        let bytes =
            kdbx::repack(&self.kdf, &self.salt, &self.transformed, &stream_key, &body, entropy)?;
        store.write(&bytes)
    }

    /// A6.4 恢复密钥加密副本：同一库内容、独立盐与流密钥，口令 = 恢复密钥域派生材料。
    /// 与主备份并列存放（主备份仍用主口令）；两端都永不落明文。
    pub fn recovery_copy(
        &self,
        recovery: &RecoveryKey,
        entropy: &dyn Entropy,
    ) -> Result<Vec<u8>, VaultError> {
        let mut stream_key = Zeroizing::new([0u8; 64]);
        random_bytes(entropy, stream_key.as_mut()).map_err(|_| VaultError::Malformed)?;
        let mut stream = xml::protected_stream(&stream_key)?;
        let body = xml::write_document(&self.vault, &mut stream)?;
        let mut salt = Zeroizing::new([0u8; 32]);
        random_bytes(entropy, salt.as_mut()).map_err(|_| VaultError::Malformed)?;
        let material = recovery.password_material()?;
        let transformed = kdbx::transformed_key(&*material, &salt, &self.kdf)?;
        Ok(kdbx::repack(
            &self.kdf,
            &salt,
            &transformed,
            &stream_key,
            &body,
            entropy,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_crypto::OsEntropy;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;

    const FAST_KDF: KdfParams = KdfParams {
        m_kib: 65536,
        t: 3,
        p: 1,
    };

    struct MemoryStore(Option<Vec<u8>>);

    impl VaultStore for MemoryStore {
        fn read(&mut self) -> Result<Option<Vec<u8>>, VaultError> {
            Ok(self.0.clone())
        }

        fn write(&mut self, data: &[u8]) -> Result<(), VaultError> {
            self.0 = Some(data.to_vec());
            Ok(())
        }
    }

    fn sample_entry(entropy: &dyn Entropy) -> Entry {
        let mut entry = Entry::new("GitHub <main>", "acc-1", CredentialKind::Password, entropy).unwrap();
        entry.username = Some("tom & jerry".into());
        entry.secret = Some("s3cr3t <&>".into());
        entry.totp_seed = Some("JBSWY3DPEHPK3PXP".into());
        entry.url = Some("https://example.com/?a=1&b=2".into());
        entry.note = Some("备注 \"引号\"".into());
        entry
    }

    #[test]
    fn roundtrip_preserves_entries() {
        let entropy = OsEntropy;
        let mut session = VaultSession::create("个人", "pw-密码".as_bytes(), &FAST_KDF, &entropy).unwrap();
        let entry = sample_entry(&entropy);
        let uuid = entry.uuid;
        session.vault.add(entry.clone()).unwrap();

        let mut store = MemoryStore(None);
        session.save(&mut store, &entropy).unwrap();

        let opened = VaultSession::unlock(&mut store, "pw-密码".as_bytes()).unwrap();
        assert_eq!(opened.vault.name, "个人");
        assert_eq!(opened.vault.find(&uuid), Some(&entry));
        assert_eq!(opened.vault.entries().len(), 1);
    }

    #[test]
    fn wrong_password_rejected() {
        let entropy = OsEntropy;
        let session = VaultSession::create("v", b"right", &FAST_KDF, &entropy).unwrap();
        let mut store = MemoryStore(None);
        session.save(&mut store, &entropy).unwrap();
        let err = VaultSession::unlock(&mut store, b"wrong").unwrap_err();
        assert!(matches!(err, VaultError::Kdbx(KdbxError::BadCredentials)));
    }

    #[test]
    fn tampered_file_rejected() {
        let entropy = OsEntropy;
        let session = VaultSession::create("v", b"pw", &FAST_KDF, &entropy).unwrap();
        let mut store = MemoryStore(None);
        session.save(&mut store, &entropy).unwrap();
        let mut bad = store.0.clone().unwrap();
        let mid = bad.len() / 2;
        bad[mid] ^= 0x01;
        let mut store = MemoryStore(Some(bad));
        assert!(VaultSession::unlock(&mut store, b"pw").is_err());
    }

    #[test]
    fn trash_restore_and_purge() {
        let entropy = OsEntropy;
        let mut session = VaultSession::create("v", b"pw", &FAST_KDF, &entropy).unwrap();
        let entry = sample_entry(&entropy);
        let uuid = entry.uuid;
        session.vault.add(entry).unwrap();
        session.vault.trash(uuid).unwrap();
        assert!(session.vault.entries().is_empty());
        assert_eq!(session.vault.trash_entries().len(), 1);

        let mut store = MemoryStore(None);
        session.save(&mut store, &entropy).unwrap();
        let mut opened = VaultSession::unlock(&mut store, b"pw").unwrap();
        assert_eq!(opened.vault.trash_entries().len(), 1);

        opened.vault.restore(uuid).unwrap();
        assert_eq!(opened.vault.entries().len(), 1);
        opened.vault.trash(uuid).unwrap();
        opened.vault.purge_trash();
        assert!(opened.vault.trash_entries().is_empty());
    }

    #[test]
    fn debug_redacts_secrets() {
        let entropy = OsEntropy;
        let mut entry = sample_entry(&entropy);
        entry.secret = Some("top-secret".into());
        entry.totp_seed = Some("seed-secret".into());
        entry.note = Some("note-secret".into());
        let out = format!("{entry:?}");
        assert!(!out.contains("top-secret"));
        assert!(!out.contains("seed-secret"));
        assert!(!out.contains("note-secret"));
    }

    #[test]
    fn document_uses_standard_field_names() {
        let entropy = OsEntropy;
        let mut vault = Vault::new("v", &entropy).unwrap();
        vault.add(sample_entry(&entropy)).unwrap();
        let mut stream = xml::protected_stream(&[5u8; 64]).unwrap();
        let text = String::from_utf8(xml::write_document(&vault, &mut stream).unwrap()).unwrap();
        for marker in [
            "<Key>Title</Key>",
            "<Key>UserName</Key>",
            "<Key>Password</Key>",
            "<Key>URL</Key>",
            "<Key>Notes</Key>",
            "<Key>account_ref</Key>",
            "<Key>kind</Key>",
            "<Key>totp_seed</Key>",
            "<Value Protected=\"True\">",
            "<RecycleBinEnabled>True</RecycleBinEnabled>",
        ] {
            assert!(text.contains(marker), "missing {marker}");
        }
    }

    #[test]
    fn foreign_shaped_document_parses() {
        // 生态工具写 Times/History 等元素：未知元素忽略，保护值按连续流解。
        let entropy = OsEntropy;
        let bin_uuid = BASE64.encode([2u8; 16]);
        let root_uuid = BASE64.encode([1u8; 16]);
        let entry_uuid = BASE64.encode([3u8; 16]);
        let mut stream = xml::protected_stream(&[7u8; 64]).unwrap();
        let mut secret = b"db-pass".to_vec();
        stream.apply(&mut secret);
        let secret = BASE64.encode(&secret);
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<KeePassFile>
<Meta><Generator>KeePass</Generator><DatabaseName>ext</DatabaseName>
<RecycleBinEnabled>True</RecycleBinEnabled><RecycleBinUUID>{bin_uuid}</RecycleBinUUID></Meta>
<Root><Group><UUID>{root_uuid}</UUID><Name>Root</Name>
<Times><CreationTime>2024-01-01T00:00:00Z</CreationTime></Times>
<Group><UUID>{bin_uuid}</UUID><Name>Recycle Bin</Name></Group>
<Entry><UUID>{entry_uuid}</UUID><IconID>0</IconID>
<String><Key>Title</Key><Value>ext-entry</Value></String>
<String><Key>Password</Key><Value Protected="True">{secret}</Value></String>
<String><Key>UserName</Key><Value>alice &amp; bob</Value></String>
</Entry>
</Group></Root>
</KeePassFile>"#
        );
        let mut stream = xml::protected_stream(&[7u8; 64]).unwrap();
        let vault = xml::parse_document(&xml, &mut stream).unwrap();
        assert_eq!(vault.name, "ext");
        let entry = vault.find(&[3u8; 16]).unwrap();
        assert_eq!(entry.title, "ext-entry");
        assert_eq!(entry.secret.as_deref(), Some("db-pass"));
        assert_eq!(entry.username.as_deref(), Some("alice & bob"));
        let _ = &entropy;
    }

    #[test]
    fn recovery_key_display_roundtrips() {
        let entropy = OsEntropy;
        let key = RecoveryKey::generate(&entropy).unwrap();
        let shown = key.display();
        assert_eq!(shown.len(), 64 + 7, "64 位 hex + 7 个分组空格");
        assert_eq!(RecoveryKey::from_display(&shown).unwrap().display(), shown);
        assert_eq!(RecoveryKey::from_display("zz").unwrap_err(), VaultError::Malformed);
        assert_eq!(
            RecoveryKey::from_display("").unwrap_err(),
            VaultError::Malformed
        );
    }

    #[test]
    fn recovery_copy_opens_without_master_password() {
        let entropy = OsEntropy;
        let mut session = VaultSession::create("v", b"master-pw", &FAST_KDF, &entropy).unwrap();
        let entry = sample_entry(&entropy);
        let uuid = entry.uuid;
        session.vault.add(entry.clone()).unwrap();

        let recovery = RecoveryKey::generate(&entropy).unwrap();
        let copy = session.recovery_copy(&recovery, &entropy).unwrap();

        // A6.9 恢复路径：不需要主口令，逐字段一致。
        let restored = recovery.open_copy(&copy).unwrap();
        assert_eq!(restored.vault.find(&uuid), Some(&entry));
        assert_eq!(restored.vault.name, "v");

        // 两路互不混用：主口令开不了副本，恢复密钥开不了主备份。
        assert!(matches!(
            VaultSession::from_backup(b"master-pw", &copy),
            Err(VaultError::Kdbx(KdbxError::BadCredentials))
        ));
        let mut store = MemoryStore(None);
        session.save(&mut store, &entropy).unwrap();
        let main = store.0.clone().unwrap();
        assert_ne!(copy, main, "副本必须独立成盐/流密钥，不是主备份的拷贝");
        let other = RecoveryKey::generate(&entropy).unwrap();
        assert!(other.open_copy(&main).is_err());
        assert!(recovery.open_copy(&main).is_err());
    }

    #[test]
    fn corrupted_recovery_copy_rejected_whole() {
        let entropy = OsEntropy;
        let mut session = VaultSession::create("v", b"pw", &FAST_KDF, &entropy).unwrap();
        session.vault.add(sample_entry(&entropy)).unwrap();
        let recovery = RecoveryKey::generate(&entropy).unwrap();
        let copy = session.recovery_copy(&recovery, &entropy).unwrap();

        for offset in [0usize, 1, copy.len() / 2, copy.len() - 1] {
            let mut bad = copy.clone();
            bad[offset] ^= 0x01;
            assert!(recovery.open_copy(&bad).is_err(), "偏移 {offset} 篡改必须整体拒绝");
        }
    }

    #[test]
    fn recovery_key_debug_redacts() {
        let entropy = OsEntropy;
        let key = RecoveryKey::generate(&entropy).unwrap();
        let dbg = format!("{key:?}");
        assert!(dbg.contains(REDACTED));
        assert!(!dbg.contains(&key.display().replace(' ', "")));
    }

    #[test]
    fn tags_and_favorite_roundtrip() {
        let entropy = OsEntropy;
        let mut session = VaultSession::create("v", b"pw", &FAST_KDF, &entropy).unwrap();
        let mut entry = sample_entry(&entropy);
        entry.tags = vec!["work".into(), "finance".into()];
        entry.favorite = true;
        let uuid = entry.uuid;
        session.vault.add(entry.clone()).unwrap();

        let mut store = MemoryStore(None);
        session.save(&mut store, &entropy).unwrap();
        let restored = VaultSession::unlock(&mut store, b"pw").unwrap();
        assert_eq!(restored.vault.find(&uuid), Some(&entry));

        // 空即省略：无标签、未收藏的条目不落 tags / favorite 字段。
        let plain = sample_entry(&entropy);
        let mut s2 = VaultSession::create("v2", b"pw", &FAST_KDF, &entropy).unwrap();
        s2.vault.add(plain).unwrap();
        let bytes = xml::write_document(&s2.vault, &mut xml::protected_stream(&[0u8; 64]).unwrap())
            .unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("<Key>tags</Key>"));
        assert!(!text.contains("<Key>favorite</Key>"));
    }

    #[test]
    fn reused_secrets_detected_without_leaking() {
        let entropy = OsEntropy;
        let mut a = sample_entry(&entropy);
        a.secret = Some("same-secret".into());
        let mut b = sample_entry(&entropy);
        b.secret = Some("same-secret".into());
        let mut c = sample_entry(&entropy);
        c.secret = Some("unique-secret".into());
        let mut d = sample_entry(&entropy);
        d.secret = None;

        let groups = find_reused_secrets([&a, &b, &c, &d]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].member_uuids, vec![a.uuid, b.uuid]);
        let dbg = format!("{groups:?}");
        assert!(!dbg.contains("same-secret"), "结果不得携带秘密");
    }

    #[test]
    fn strength_estimator_matches_formula() {
        // 8 位纯小写 = 8 × log2(26)；4 位四类 = 4 × log2(94)。
        assert!((estimate_strength_bits("aaaaaaaa") - 8.0 * 26f64.log2()).abs() < 1e-9);
        assert!((estimate_strength_bits("aA0!") - 4.0 * 94f64.log2()).abs() < 1e-9);
        assert_eq!(estimate_strength_bits(""), 0.0);
    }

    #[test]
    fn identity_profiles_crud_and_roundtrip() {
        let entropy = OsEntropy;
        let mut session = VaultSession::create("v", b"pw", &FAST_KDF, &entropy).unwrap();
        session.vault.set_profile(IdentityProfile {
            name: "工作".into(),
            account_refs: vec!["acc-work-1".into(), "acc-work-2".into()],
        });
        session.vault.set_profile(IdentityProfile {
            name: "个人".into(),
            account_refs: vec!["acc-home-1".into()],
        });
        session.vault.set_active_profile(Some("工作")).unwrap();
        assert_eq!(
            session.vault.set_active_profile(Some("不存在")).unwrap_err(),
            VaultError::NotFound
        );

        let mut store = MemoryStore(None);
        session.save(&mut store, &entropy).unwrap();
        let restored = VaultSession::unlock(&mut store, b"pw").unwrap();
        assert_eq!(restored.vault.profiles, session.vault.profiles);
        assert_eq!(restored.vault.active_profile.as_deref(), Some("工作"));

        // 删除生效档案同时清除生效标记。
        let mut v = restored.vault;
        assert!(v.remove_profile("工作"));
        assert!(!v.remove_profile("工作"));
        assert_eq!(v.active_profile, None);
    }
}
