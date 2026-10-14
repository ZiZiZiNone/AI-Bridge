//! KeePass XML：标准条目字段 + 自定义字段；保护值走 KDBX ChaCha20 连续流。
//! 标准字段名固定，KeePassXC 等生态工具可直接读取（灾备逃生）。

use ab_contract::CredentialKind;
use ab_crypto::{sha512, ChaCha20Stream};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use quick_xml::escape;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use quick_xml::XmlVersion;

use crate::{Entry, Group, IdentityProfile, Vault, VaultError};

const K_TITLE: &str = "Title";
const K_USERNAME: &str = "UserName";
const K_PASSWORD: &str = "Password";
const K_URL: &str = "URL";
const K_NOTES: &str = "Notes";
const K_ACCOUNT_REF: &str = "account_ref";
const K_KIND: &str = "kind";
const K_TOTP: &str = "totp_seed";
const K_TAGS: &str = "tags";
const K_FAVORITE: &str = "favorite";

fn wio<T>(r: std::io::Result<T>) -> Result<T, VaultError> {
    r.map_err(|_| VaultError::Malformed)
}

/// KDBX 约定：key/nonce 取 sha512(stream_key) 的前 32 / 次 12 字节，全文档一条流。
pub fn protected_stream(stream_key: &[u8; 64]) -> Result<ChaCha20Stream, VaultError> {
    let h = sha512(stream_key);
    let key: [u8; 32] = h[..32].try_into().map_err(|_| VaultError::Malformed)?;
    let nonce: [u8; 12] = h[32..44].try_into().map_err(|_| VaultError::Malformed)?;
    Ok(ChaCha20Stream::new(&key, &nonce))
}

fn kind_str(kind: CredentialKind) -> &'static str {
    match kind {
        CredentialKind::Password => "password",
        CredentialKind::Passkey => "passkey",
        CredentialKind::Totp => "totp",
    }
}

fn kind_from(s: &str) -> CredentialKind {
    match s {
        "passkey" => CredentialKind::Passkey,
        "totp" => CredentialKind::Totp,
        _ => CredentialKind::Password,
    }
}

fn elem_text(w: &mut Writer<Vec<u8>>, name: &str, text: &str) -> Result<(), VaultError> {
    wio(w.write_event(Event::Start(BytesStart::new(name))))?;
    wio(w.write_event(Event::Text(BytesText::new(text))))?;
    wio(w.write_event(Event::End(BytesEnd::new(name))))?;
    Ok(())
}

fn string_field(
    w: &mut Writer<Vec<u8>>,
    key: &str,
    value: Option<&str>,
    protect: bool,
    stream: &mut ChaCha20Stream,
) -> Result<(), VaultError> {
    let value = match value {
        Some(v) => v,
        None => return Ok(()),
    };
    wio(w.write_event(Event::Start(BytesStart::new("String"))))?;
    elem_text(w, "Key", key)?;
    if protect {
        let mut encoded = value.as_bytes().to_vec();
        stream.apply(&mut encoded);
        let mut vs = BytesStart::new("Value");
        vs.push_attribute(("Protected", "True"));
        wio(w.write_event(Event::Start(vs)))?;
        wio(w.write_event(Event::Text(BytesText::new(&BASE64.encode(&encoded)))))?;
        wio(w.write_event(Event::End(BytesEnd::new("Value"))))?;
    } else {
        elem_text(w, "Value", value)?;
    }
    wio(w.write_event(Event::End(BytesEnd::new("String"))))?;
    Ok(())
}

fn entry_elem(
    w: &mut Writer<Vec<u8>>,
    entry: &Entry,
    stream: &mut ChaCha20Stream,
) -> Result<(), VaultError> {
    wio(w.write_event(Event::Start(BytesStart::new("Entry"))))?;
    elem_text(w, "UUID", &BASE64.encode(entry.uuid))?;
    string_field(w, K_TITLE, Some(&entry.title), false, stream)?;
    string_field(w, K_USERNAME, entry.username.as_deref(), false, stream)?;
    string_field(w, K_PASSWORD, entry.secret.as_deref(), true, stream)?;
    string_field(w, K_URL, entry.url.as_deref(), false, stream)?;
    string_field(w, K_NOTES, entry.note.as_deref(), false, stream)?;
    string_field(w, K_ACCOUNT_REF, Some(&entry.account_ref), false, stream)?;
    string_field(
        w,
        K_KIND,
        Some(kind_str(entry.kind)),
        false,
        stream,
    )?;
    string_field(w, K_TOTP, entry.totp_seed.as_deref(), true, stream)?;
    // A1.1 标签 / A1.2 收藏：空即省略（与 skip_serializing_if 口径一致）。
    if !entry.tags.is_empty() {
        let joined = entry.tags.join(";");
        string_field(w, K_TAGS, Some(&joined), false, stream)?;
    }
    if entry.favorite {
        string_field(w, K_FAVORITE, Some("true"), false, stream)?;
    }
    wio(w.write_event(Event::End(BytesEnd::new("Entry"))))?;
    Ok(())
}

fn group_elem(
    w: &mut Writer<Vec<u8>>,
    group: &Group,
    stream: &mut ChaCha20Stream,
) -> Result<(), VaultError> {
    wio(w.write_event(Event::Start(BytesStart::new("Group"))))?;
    elem_text(w, "UUID", &BASE64.encode(group.uuid))?;
    elem_text(w, "Name", &group.name)?;
    for child in &group.groups {
        group_elem(w, child, stream)?;
    }
    for entry in &group.entries {
        entry_elem(w, entry, stream)?;
    }
    wio(w.write_event(Event::End(BytesEnd::new("Group"))))?;
    Ok(())
}

pub fn write_document(vault: &Vault, stream: &mut ChaCha20Stream) -> Result<Vec<u8>, VaultError> {
    let mut w = Writer::new(Vec::new());
    wio(w.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("utf-8"),
        None,
    ))))?;
    wio(w.write_event(Event::Start(BytesStart::new("KeePassFile"))))?;
    wio(w.write_event(Event::Start(BytesStart::new("Meta"))))?;
    elem_text(&mut w, "Generator", "AI-Bridge")?;
    elem_text(&mut w, "DatabaseName", &vault.name)?;
    elem_text(&mut w, "RecycleBinEnabled", "True")?;
    elem_text(&mut w, "RecycleBinUUID", &BASE64.encode(vault.recycle_bin))?;
    // A4.2 身份档案：Meta/CustomData（KeePass 标准位），值为 JSON。
    if !vault.profiles.is_empty() || vault.active_profile.is_some() {
        wio(w.write_event(Event::Start(BytesStart::new("CustomData"))))?;
        custom_item(&mut w, "profiles", &profiles_json(&vault.profiles))?;
        if let Some(active) = &vault.active_profile {
            custom_item(&mut w, "active_profile", active)?;
        }
        wio(w.write_event(Event::End(BytesEnd::new("CustomData"))))?;
    }
    wio(w.write_event(Event::End(BytesEnd::new("Meta"))))?;
    wio(w.write_event(Event::Start(BytesStart::new("Root"))))?;
    group_elem(&mut w, &vault.root, stream)?;
    wio(w.write_event(Event::End(BytesEnd::new("Root"))))?;
    wio(w.write_event(Event::End(BytesEnd::new("KeePassFile"))))?;
    Ok(w.into_inner())
}

// 读取侧用轻量元素树：生态工具会写入 Times/History 等未知元素，原样忽略。
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Element>,
}

impl Element {
    fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

fn make_element(e: BytesStart<'_>) -> Result<Element, VaultError> {
    let name = e.name().as_ref().to_string();
    let mut attrs = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|_| VaultError::Malformed)?;
        let key = a.key.as_ref().to_string();
        let val = a
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|_| VaultError::Malformed)?
            .into_owned();
        attrs.push((key, val));
    }
    Ok(Element {
        name,
        attrs,
        text: String::new(),
        children: Vec::new(),
    })
}

fn attach(
    el: Element,
    stack: &mut Vec<Element>,
    root: &mut Option<Element>,
) -> Result<(), VaultError> {
    match stack.last_mut() {
        Some(parent) => parent.children.push(el),
        None => {
            if root.is_some() {
                return Err(VaultError::Malformed);
            }
            *root = Some(el);
        }
    }
    Ok(())
}

fn parse_tree(xml: &str) -> Result<Element, VaultError> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    loop {
        match reader.read_event().map_err(|_| VaultError::Malformed)? {
            Event::Start(e) => stack.push(make_element(e)?),
            Event::Empty(e) => attach(make_element(e)?, &mut stack, &mut root)?,
            Event::Text(t) => {
                let raw = t.into_inner();
                let text = escape::unescape(&raw).map_err(|_| VaultError::Malformed)?;
                match stack.last_mut() {
                    Some(parent) => parent.text.push_str(&text),
                    None if text.trim().is_empty() => {}
                    None => return Err(VaultError::Malformed),
                }
            }
            // quick-xml 0.42 把实体引用拆成独立事件，必须还原回文本流。
            Event::GeneralRef(r) => {
                let entity = format!("&{};", r.into_inner());
                let text = escape::unescape(&entity).map_err(|_| VaultError::Malformed)?;
                match stack.last_mut() {
                    Some(parent) => parent.text.push_str(&text),
                    None => return Err(VaultError::Malformed),
                }
            }
            Event::End(_) => {
                let el = stack.pop().ok_or(VaultError::Malformed)?;
                attach(el, &mut stack, &mut root)?;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(VaultError::Malformed);
    }
    root.ok_or(VaultError::Malformed)
}

fn uuid_from_text(raw: &str) -> Result<[u8; 16], VaultError> {
    let bytes = BASE64
        .decode(raw.trim())
        .map_err(|_| VaultError::Malformed)?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::Malformed)
}

fn uuid_value(el: &Element) -> Result<[u8; 16], VaultError> {
    uuid_from_text(el.child("UUID").map(|c| c.text.as_str()).unwrap_or_default())
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn parse_entry(el: &Element, stream: &mut ChaCha20Stream) -> Result<Entry, VaultError> {
    let mut entry = Entry {
        uuid: uuid_value(el)?,
        title: String::new(),
        account_ref: String::new(),
        kind: CredentialKind::Password,
        username: None,
        secret: None,
        totp_seed: None,
        url: None,
        note: None,
        tags: Vec::new(),
        favorite: false,
    };
    for s in el.children.iter().filter(|c| c.name == "String") {
        let key = s.child("Key").map(|c| c.text.as_str()).unwrap_or_default();
        let value = s.child("Value").ok_or(VaultError::Malformed)?;
        let raw = if value
            .attr("Protected")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
        {
            let mut bytes = BASE64
                .decode(value.text.trim())
                .map_err(|_| VaultError::Malformed)?;
            stream.apply(&mut bytes);
            String::from_utf8(bytes).map_err(|_| VaultError::Malformed)?
        } else {
            value.text.clone()
        };
        match key {
            K_TITLE => entry.title = raw,
            K_USERNAME => entry.username = non_empty(raw),
            K_PASSWORD => entry.secret = non_empty(raw),
            K_URL => entry.url = non_empty(raw),
            K_NOTES => entry.note = non_empty(raw),
            K_ACCOUNT_REF => entry.account_ref = raw,
            K_KIND => entry.kind = kind_from(&raw),
            K_TOTP => entry.totp_seed = non_empty(raw),
            K_TAGS => {
                entry.tags = raw
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            }
            K_FAVORITE => entry.favorite = raw.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    Ok(entry)
}

fn parse_group(el: &Element, stream: &mut ChaCha20Stream) -> Result<Group, VaultError> {
    let mut group = Group {
        uuid: uuid_value(el)?,
        name: el
            .child("Name")
            .map(|c| c.text.clone())
            .unwrap_or_default(),
        entries: Vec::new(),
        groups: Vec::new(),
    };
    for c in &el.children {
        match c.name.as_str() {
            "Entry" => group.entries.push(parse_entry(c, stream)?),
            "Group" => group.groups.push(parse_group(c, stream)?),
            _ => {}
        }
    }
    Ok(group)
}

pub fn parse_document(xml: &str, stream: &mut ChaCha20Stream) -> Result<Vault, VaultError> {
    let doc = parse_tree(xml)?;
    if doc.name != "KeePassFile" {
        return Err(VaultError::Malformed);
    }
    let meta = doc.child("Meta").ok_or(VaultError::Malformed)?;
    let name = meta
        .child("DatabaseName")
        .map(|c| c.text.clone())
        .unwrap_or_default();
    let recycle_bin = meta
        .child("RecycleBinUUID")
        .map(|c| uuid_from_text(&c.text))
        .transpose()?
        .ok_or(VaultError::Malformed)?;
    let root = doc.child("Root").ok_or(VaultError::Malformed)?;
    let group = root
        .children
        .iter()
        .find(|c| c.name == "Group")
        .ok_or(VaultError::Malformed)?;
    // A4.2 身份档案（Meta/CustomData；缺省即空）。
    let mut profiles = Vec::new();
    let mut active_profile = None;
    if let Some(custom) = meta.child("CustomData") {
        for item in custom.children.iter().filter(|c| c.name == "Item") {
            let key = item.child("Key").map(|c| c.text.as_str()).unwrap_or_default();
            let value = item.child("Value").map(|c| c.text.clone()).unwrap_or_default();
            match key {
                "profiles" => profiles = profiles_from_json(&value)?,
                "active_profile" => active_profile = Some(value).filter(|s| !s.is_empty()),
                _ => {}
            }
        }
    }
    Ok(Vault {
        name,
        root: parse_group(group, stream)?,
        recycle_bin,
        profiles,
        active_profile,
    })
}

fn custom_item(w: &mut Writer<Vec<u8>>, key: &str, value: &str) -> Result<(), VaultError> {
    wio(w.write_event(Event::Start(BytesStart::new("Item"))))?;
    elem_text(w, "Key", key)?;
    elem_text(w, "Value", value)?;
    wio(w.write_event(Event::End(BytesEnd::new("Item"))))?;
    Ok(())
}

fn profiles_json(profiles: &[IdentityProfile]) -> String {
    let items: Vec<serde_json::Value> = profiles
        .iter()
        .map(|p| serde_json::json!({ "name": p.name, "account_refs": p.account_refs }))
        .collect();
    serde_json::Value::Array(items).to_string()
}

fn profiles_from_json(text: &str) -> Result<Vec<IdentityProfile>, VaultError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|_| VaultError::Malformed)?;
    let items = value.as_array().ok_or(VaultError::Malformed)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or(VaultError::Malformed)?;
        let refs = item
            .get("account_refs")
            .and_then(|v| v.as_array())
            .ok_or(VaultError::Malformed)?;
        let mut account_refs = Vec::with_capacity(refs.len());
        for r in refs {
            account_refs.push(r.as_str().ok_or(VaultError::Malformed)?.to_string());
        }
        out.push(IdentityProfile {
            name: name.to_string(),
            account_refs,
        });
    }
    Ok(out)
}
