//! KDBX 4.0 容器：头 / VariantDictionary / HMAC 块 / 内层头。
//! 字节布局与 KeePass 系工具一致（生态工具可打开，灾备逃生）。

use std::io::{Read, Write};

use ab_crypto::{
    aes256_cbc_decrypt, aes256_cbc_encrypt, derive, hmac_sign, hmac_verify, sha256, sha512,
    Entropy, KdfParams, Zeroizing,
};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

pub const SIGNATURE_1: u32 = 0x9AA2_D903;
pub const SIGNATURE_2: u32 = 0xB54B_FB67;
const FILE_VERSION_4: u32 = 0x0004_0000;

const CIPHER_AES256: [u8; 16] = [
    0x31, 0xc1, 0xf2, 0xe6, 0xbf, 0x71, 0x43, 0x50, 0xbe, 0x58, 0x05, 0x21, 0x6a, 0xfc, 0x5a, 0xff,
];
const KDF_ARGON2ID: [u8; 16] = [
    0x9e, 0x29, 0x8b, 0x19, 0x56, 0xdb, 0x47, 0x73, 0xb2, 0x3d, 0xfc, 0x3e, 0xc6, 0xf0, 0xa1, 0xe6,
];
const ARGON2_VERSION: u32 = 0x13;

const HF_END: u8 = 0;
const HF_CIPHER_ID: u8 = 2;
const HF_COMPRESSION: u8 = 3;
const HF_MASTER_SEED: u8 = 4;
const HF_ENCRYPTION_IV: u8 = 7;
const HF_KDF_PARAMS: u8 = 11;

const IH_END: u8 = 0;
const IH_STREAM_ID: u8 = 1;
const IH_STREAM_KEY: u8 = 2;
const INNER_STREAM_CHACHA20: u32 = 3;

const BLOCK: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KdbxError {
    Malformed,
    BadCredentials,
    UnsupportedKdf,
}

/// 解包结果：内层数据 + 复用派生所需的材料（同会话内保存不再重跑 Argon2）。
pub struct Unpacked {
    pub stream_key: Zeroizing<[u8; 64]>,
    pub body: Vec<u8>,
    pub salt: Zeroizing<[u8; 32]>,
    pub transformed: Zeroizing<[u8; 32]>,
    pub kdf: KdfParams,
}

fn composite_key(password: &[u8]) -> Zeroizing<[u8; 32]> {
    let pw_hash = sha256(password);
    Zeroizing::new(sha256(&pw_hash))
}

fn hmac_base(master_seed: &[u8; 32], transformed: &[u8; 32]) -> Zeroizing<[u8; 64]> {
    let mut buf = Vec::with_capacity(65);
    buf.extend_from_slice(master_seed);
    buf.extend_from_slice(transformed);
    buf.push(0x01);
    Zeroizing::new(sha512(&buf))
}

fn block_key(index: u64, base: &[u8; 64]) -> [u8; 64] {
    let mut buf = [0u8; 72];
    buf[..8].copy_from_slice(&index.to_le_bytes());
    buf[8..].copy_from_slice(base);
    sha512(&buf)
}

fn push_field(out: &mut Vec<u8>, id: u8, data: &[u8]) {
    out.push(id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
}

fn kdf_params_dict(kdf: &KdfParams, salt: &[u8; 32]) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&0x0100u16.to_le_bytes());
    entry(&mut d, 0x42, b"$UUID", &KDF_ARGON2ID);
    entry(&mut d, 0x42, b"S", salt);
    entry(&mut d, 0x04, b"P", &kdf.p.to_le_bytes());
    entry(&mut d, 0x05, b"M", &(u64::from(kdf.m_kib) * 1024).to_le_bytes());
    entry(&mut d, 0x05, b"I", &u64::from(kdf.t).to_le_bytes());
    entry(&mut d, 0x04, b"V", &ARGON2_VERSION.to_le_bytes());
    d.push(0x00);
    d
}

fn entry(out: &mut Vec<u8>, kind: u8, key: &[u8], value: &[u8]) {
    out.push(kind);
    out.extend_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value);
}

fn read_u32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn read_u64(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn parse_kdf_params(dict: &[u8]) -> Result<(KdfParams, Zeroizing<[u8; 32]>), KdbxError> {
    let version = read_u32(dict, 0).ok_or(KdbxError::Malformed)?; // u16 版本 + 保留
    if (version & 0xffff) != 0x0100 {
        return Err(KdbxError::Malformed);
    }
    let mut pos = 2usize;
    let mut uuid = [0u8; 16];
    let mut salt = Zeroizing::new([0u8; 32]);
    let mut m_kib = 0u32;
    let mut t = 0u32;
    let mut p = 0u32;
    loop {
        let kind = *dict.get(pos).ok_or(KdbxError::Malformed)?;
        pos += 1;
        if kind == 0x00 {
            break;
        }
        let key_len = read_u32(dict, pos).ok_or(KdbxError::Malformed)? as usize;
        pos += 4;
        let key = dict.get(pos..pos + key_len).ok_or(KdbxError::Malformed)?;
        pos += key_len;
        let val_len = read_u32(dict, pos).ok_or(KdbxError::Malformed)? as usize;
        pos += 4;
        let val = dict.get(pos..pos + val_len).ok_or(KdbxError::Malformed)?;
        pos += val_len;
        match key {
            b"$UUID" => {
                if val.len() != 16 {
                    return Err(KdbxError::Malformed);
                }
                uuid.copy_from_slice(val);
            }
            b"S" => {
                if val.len() != 32 {
                    return Err(KdbxError::Malformed);
                }
                salt.copy_from_slice(val);
            }
            b"M" => {
                let bytes = read_u64(val, 0).ok_or(KdbxError::Malformed)?;
                m_kib = u32::try_from(bytes / 1024).map_err(|_| KdbxError::Malformed)?;
            }
            b"I" => t = read_u64(val, 0).ok_or(KdbxError::Malformed)? as u32,
            b"P" => p = read_u32(val, 0).ok_or(KdbxError::Malformed)?,
            b"V" => {
                if read_u32(val, 0) != Some(ARGON2_VERSION) {
                    return Err(KdbxError::UnsupportedKdf);
                }
            }
            _ => {}
        }
    }
    if uuid != KDF_ARGON2ID {
        return Err(KdbxError::UnsupportedKdf);
    }
    // 头里的 KDF 参数是攻击者可控输入，低于基线拒绝（不「尽力解锁」）。
    let kdf = KdfParams { m_kib, t, p }
        .validated()
        .map_err(|_| KdbxError::UnsupportedKdf)?;
    Ok((kdf, salt))
}

fn gzip(data: &[u8]) -> Result<Vec<u8>, KdbxError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(|_| KdbxError::Malformed)?;
    encoder.finish().map_err(|_| KdbxError::Malformed)
}

fn gunzip(data: &[u8]) -> Result<Vec<u8>, KdbxError> {
    let mut out = Vec::new();
    GzDecoder::new(data)
        .read_to_end(&mut out)
        .map_err(|_| KdbxError::Malformed)?;
    Ok(out)
}

/// 口令 → transformed key；保存会话复用它，不再重跑 Argon2。
pub fn transformed_key(
    password: &[u8],
    salt: &[u8; 32],
    kdf: &KdfParams,
) -> Result<Zeroizing<[u8; 32]>, KdbxError> {
    let kdf = kdf.validated().map_err(|_| KdbxError::UnsupportedKdf)?;
    derive(composite_key(password).as_ref(), salt, &kdf).map_err(|_| KdbxError::Malformed)
}

/// 同会话重打包：复用 transformed（不重跑 Argon2），换新 master seed / IV / 流密钥。
pub fn repack(
    kdf: &KdfParams,
    salt: &[u8; 32],
    transformed: &[u8; 32],
    stream_key: &[u8; 64],
    body: &[u8],
    entropy: &dyn Entropy,
) -> Result<Vec<u8>, KdbxError> {
    let kdf = kdf.validated().map_err(|_| KdbxError::UnsupportedKdf)?;
    let mut master_seed = [0u8; 32];
    let mut iv = [0u8; 16];
    entropy
        .fill(&mut master_seed)
        .and_then(|_| entropy.fill(&mut iv))
        .map_err(|_| KdbxError::Malformed)?;

    let mut key_input = Vec::with_capacity(64);
    key_input.extend_from_slice(&master_seed);
    key_input.extend_from_slice(transformed);
    let final_key = Zeroizing::new(sha256(&key_input));
    let hbase = hmac_base(&master_seed, transformed);

    let mut header = Vec::new();
    header.extend_from_slice(&SIGNATURE_1.to_le_bytes());
    header.extend_from_slice(&SIGNATURE_2.to_le_bytes());
    header.extend_from_slice(&FILE_VERSION_4.to_le_bytes());
    push_field(&mut header, HF_CIPHER_ID, &CIPHER_AES256);
    push_field(&mut header, HF_COMPRESSION, &1u32.to_le_bytes());
    push_field(&mut header, HF_MASTER_SEED, &master_seed);
    push_field(&mut header, HF_ENCRYPTION_IV, &iv);
    push_field(&mut header, HF_KDF_PARAMS, &kdf_params_dict(&kdf, salt));
    push_field(&mut header, HF_END, b"\r\n\r\n");

    let mut inner = Vec::with_capacity(body.len() + 80);
    push_field(&mut inner, IH_STREAM_ID, &INNER_STREAM_CHACHA20.to_le_bytes());
    push_field(&mut inner, IH_STREAM_KEY, stream_key);
    push_field(&mut inner, IH_END, b"");
    inner.extend_from_slice(body);

    let ciphertext = aes256_cbc_encrypt(&final_key, &iv, &gzip(&inner)?);

    let mut out = header;
    let header_hash = sha256(&out);
    out.extend_from_slice(&header_hash);
    let mut index = u64::MAX;
    let hk = block_key(index, &hbase);
    let head_hmac = hmac_sign(&hk, &out).map_err(|_| KdbxError::Malformed)?;
    out.extend_from_slice(&head_hmac);

    index = 0;
    for chunk in ciphertext.chunks(BLOCK).chain(std::iter::once(&[][..])) {
        let hk = block_key(index, &hbase);
        let mut msg = Vec::with_capacity(chunk.len() + 12);
        msg.extend_from_slice(&index.to_le_bytes());
        msg.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        msg.extend_from_slice(chunk);
        let tag = hmac_sign(&hk, &msg).map_err(|_| KdbxError::Malformed)?;
        out.extend_from_slice(&tag);
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(chunk);
        index += 1;
    }
    Ok(out)
}

/// 解包：校验头 HMAC 与块 HMAC 后解密，失败一律 `BadCredentials`/`Malformed`。
pub fn unpack(password: &[u8], data: &[u8]) -> Result<Unpacked, KdbxError> {
    if read_u32(data, 0) != Some(SIGNATURE_1) || read_u32(data, 4) != Some(SIGNATURE_2) {
        return Err(KdbxError::Malformed);
    }
    let version = read_u32(data, 8).ok_or(KdbxError::Malformed)?;
    if version & 0xffff_0000 != FILE_VERSION_4 {
        return Err(KdbxError::Malformed);
    }

    let mut pos = 12usize;
    let mut master_seed = [0u8; 32];
    let mut iv = [0u8; 16];
    let mut kdf = None;
    let mut salt = None;
    let mut compression = 0u32;
    loop {
        let id = *data.get(pos).ok_or(KdbxError::Malformed)?;
        pos += 1;
        let len = read_u32(data, pos).ok_or(KdbxError::Malformed)? as usize;
        pos += 4;
        let val = data.get(pos..pos + len).ok_or(KdbxError::Malformed)?;
        pos += len;
        match id {
            HF_END => break,
            HF_MASTER_SEED if len == 32 => master_seed.copy_from_slice(val),
            HF_ENCRYPTION_IV if len == 16 => iv.copy_from_slice(val),
            HF_COMPRESSION if len == 4 => {
                compression = read_u32(val, 0).ok_or(KdbxError::Malformed)?;
            }
            HF_KDF_PARAMS => {
                let (k, s) = parse_kdf_params(val)?;
                kdf = Some(k);
                salt = Some(s);
            }
            _ => {}
        }
    }
    let header_end = pos;
    let kdf = kdf.ok_or(KdbxError::Malformed)?;
    let salt = salt.ok_or(KdbxError::Malformed)?;

    let header_hash = data.get(pos..pos + 32).ok_or(KdbxError::Malformed)?;
    pos += 32;
    // KDBX4 头部 HMAC 覆盖 头字段 + 头哈希（与 repack 签名范围一致）。
    let signed_end = pos;
    let head_hmac: [u8; 32] = data
        .get(pos..pos + 32)
        .ok_or(KdbxError::Malformed)?
        .try_into()
        .map_err(|_| KdbxError::Malformed)?;
    pos += 32;

    let composite = composite_key(password);
    let transformed = derive(composite.as_ref(), &*salt, &kdf).map_err(|_| KdbxError::Malformed)?;
    let mut key_input = Vec::with_capacity(64);
    key_input.extend_from_slice(&master_seed);
    key_input.extend_from_slice(&*transformed);
    let final_key = Zeroizing::new(sha256(&key_input));
    let hbase = hmac_base(&master_seed, &transformed);

    if header_hash != sha256(&data[..header_end]).as_slice() {
        return Err(KdbxError::Malformed);
    }
    let hk = block_key(u64::MAX, &hbase);
    hmac_verify(&hk, &data[..signed_end], &head_hmac).map_err(|_| KdbxError::BadCredentials)?;

    let mut ciphertext = Vec::new();
    let mut index = 0u64;
    loop {
        let tag: [u8; 32] = data
            .get(pos..pos + 32)
            .ok_or(KdbxError::Malformed)?
            .try_into()
            .map_err(|_| KdbxError::Malformed)?;
        pos += 32;
        let len = read_u32(data, pos).ok_or(KdbxError::Malformed)? as usize;
        pos += 4;
        let block = data.get(pos..pos + len).ok_or(KdbxError::Malformed)?;
        pos += len;
        let hk = block_key(index, &hbase);
        let mut msg = Vec::with_capacity(len + 12);
        msg.extend_from_slice(&index.to_le_bytes());
        msg.extend_from_slice(&(len as u32).to_le_bytes());
        msg.extend_from_slice(block);
        hmac_verify(&hk, &msg, &tag).map_err(|_| KdbxError::BadCredentials)?;
        if len == 0 {
            break;
        }
        ciphertext.extend_from_slice(block);
        index += 1;
    }

    let inner = aes256_cbc_decrypt(&final_key, &iv, &ciphertext).map_err(|_| KdbxError::BadCredentials)?;
    let inner = match compression {
        0 => inner,
        1 => Zeroizing::new(gunzip(&inner)?),
        _ => return Err(KdbxError::Malformed),
    };

    let mut pos = 0usize;
    let mut stream_key = Zeroizing::new([0u8; 64]);
    loop {
        let id = *inner.get(pos).ok_or(KdbxError::Malformed)?;
        pos += 1;
        let len = read_u32(&inner, pos).ok_or(KdbxError::Malformed)? as usize;
        pos += 4;
        let val = inner.get(pos..pos + len).ok_or(KdbxError::Malformed)?;
        pos += len;
        match id {
            IH_END => break,
            IH_STREAM_ID if len == 4 => {
                if read_u32(val, 0) != Some(INNER_STREAM_CHACHA20) {
                    return Err(KdbxError::UnsupportedKdf);
                }
            }
            IH_STREAM_KEY if len == 64 => stream_key.copy_from_slice(val),
            _ => {}
        }
    }

    Ok(Unpacked {
        stream_key,
        body: inner[pos..].to_vec(),
        salt,
        transformed,
        kdf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_crypto::OsEntropy;

    #[test]
    fn pack_unpack_roundtrip_and_bad_password() {
        let entropy = OsEntropy;
        let kdf = KdfParams {
            m_kib: 65536,
            t: 3,
            p: 1,
        };
        let salt = [9u8; 32];
        let stream_key = [3u8; 64];
        let body = b"<KeePassFile/>".to_vec();
        let transformed = transformed_key(b"pw", &salt, &kdf).unwrap();
        let packed = repack(&kdf, &salt, &transformed, &stream_key, &body, &entropy).unwrap();
        let unpacked = unpack(b"pw", &packed).unwrap();
        assert_eq!(unpacked.body, body);
        assert_eq!(*unpacked.stream_key, stream_key);
        assert_eq!(*unpacked.salt, salt);
        assert!(matches!(
            unpack(b"no", &packed),
            Err(KdbxError::BadCredentials)
        ));
    }

    #[test]
    fn weak_kdf_in_header_rejected() {
        let kdf = KdfParams {
            m_kib: 1024,
            t: 1,
            p: 1,
        };
        assert!(matches!(
            transformed_key(b"pw", &[9u8; 32], &kdf),
            Err(KdbxError::UnsupportedKdf)
        ));
    }
}
