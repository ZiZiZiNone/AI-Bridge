//! 保险库：KDBX4 格式级兼容的条目存储（灾备方向：文件可被 KeePassXC 直接打开）。
//! 存取走 `VaultStore` 注入，核心不碰文件系统；保存复用 transformed，不重跑 Argon2。

mod kdbx;
mod xml;

use std::fmt;

use ab_contract::CredentialKind;
use ab_crypto::{random_bytes, Entropy, KdfParams, Zeroizing};

pub use kdbx::KdbxError;

const REDACTED: &str = "[REDACTED]";
const ROOT_GROUP: &str = "Root";
const RECYCLE_BIN_GROUP: &str = "Recycle Bin";

#[derive(Debug)]
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

#[derive(Clone, Debug)]
pub struct Vault {
    pub name: String,
    pub root: Group,
    pub recycle_bin: [u8; 16],
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
        let unpacked = kdbx::unpack(password, &data)?;
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
}
