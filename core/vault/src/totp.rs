//! B7 TOTP 验证码（RFC 6238）——「点按出 6 位当前验证码」的唯一业务实现。
//!
//! 边界（docs/功能点全景.md B7.1/B7.2）：
//!   - 仅 TOTP；默认 30s / 6 位 / SHA-1。参数随条目可改：`totp_seed` 允许是
//!     `otpauth://totp/...?secret=...&period=..&digits=..&algorithm=..` 形式，
//!     查询参数缺省即默认值；纯 base32 种子用全部默认。
//!   - 时间从外部传入（`at_secs`，UTC epoch 秒），不取系统时间——时钟属宿主注入面。
//!   - 种子解码（base32 / RFC 4648）是编码不是密码学，本文件实现；
//!     HMAC 原语一律走 `ab_crypto`（SHA-1 / SHA-256 / SHA-512）。

use ab_crypto::{hmac_sha1, hmac_sha512, hmac_sign, Zeroizing};

use crate::VaultError;

/// TOTP 算法（B7.2 参数「可改」面；默认 SHA-1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

/// TOTP 参数（B7.2 默认：30s / 6 位 / SHA-1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TotpParams {
    pub period_secs: u64,
    pub digits: u8,
    pub algorithm: TotpAlgorithm,
}

impl Default for TotpParams {
    fn default() -> Self {
        Self {
            period_secs: 30,
            digits: 6,
            algorithm: TotpAlgorithm::Sha1,
        }
    }
}

/// 当前时间片的 TOTP 验证码（十进制，左补零到 `digits` 位）。
pub fn totp_code(seed: &str, at_secs: u64) -> Result<String, VaultError> {
    let (secret, params) = parse_seed(seed)?;
    let counter = at_secs / params.period_secs;
    let message = counter.to_be_bytes();

    let digest: Vec<u8> = match params.algorithm {
        TotpAlgorithm::Sha1 => hmac_sha1(&secret, &message)
            .map_err(|_| VaultError::Malformed)?
            .to_vec(),
        TotpAlgorithm::Sha256 => hmac_sign(&secret, &message)
            .map_err(|_| VaultError::Malformed)?
            .to_vec(),
        TotpAlgorithm::Sha512 => hmac_sha512(&secret, &message)
            .map_err(|_| VaultError::Malformed)?
            .to_vec(),
    };

    // 动态截断（RFC 4226 §5.3）。
    let last = *digest.last().ok_or(VaultError::Malformed)?;
    let offset = (last & 0x0f) as usize;
    let byte = |i: usize| -> Result<u32, VaultError> {
        Ok(*digest.get(i).ok_or(VaultError::Malformed)? as u32)
    };
    let binary = ((byte(offset)? & 0x7f) << 24)
        | (byte(offset + 1)? << 16)
        | (byte(offset + 2)? << 8)
        | byte(offset + 3)?;

    let modulo = 10u32.pow(params.digits as u32);
    let code = binary % modulo;
    Ok(format!("{code:0width$}", width = params.digits as usize))
}

/// 解析种子：`otpauth://` URI（参数随条目）或纯 base32（全默认）。
/// 秘密材料 `Zeroizing` 承载。
fn parse_seed(seed: &str) -> Result<(Zeroizing<Vec<u8>>, TotpParams), VaultError> {
    let trimmed = seed.trim();
    if trimmed.is_empty() {
        return Err(VaultError::Malformed);
    }

    if trimmed.len() >= 10 && trimmed[..10].eq_ignore_ascii_case("otpauth://") {
        let query = trimmed.splitn(2, '?').nth(1).ok_or(VaultError::Malformed)?;
        let mut secret_b32: Option<String> = None;
        let mut params = TotpParams::default();
        for pair in query.split('&') {
            let mut kv = pair.splitn(2, '=');
            let key = kv.next().unwrap_or_default();
            let value = kv.next().unwrap_or_default();
            match key {
                "secret" => secret_b32 = Some(value.to_string()),
                "period" => {
                    let period: u64 = value.parse().map_err(|_| VaultError::Malformed)?;
                    if period == 0 {
                        return Err(VaultError::Malformed);
                    }
                    params.period_secs = period;
                }
                "digits" => {
                    let digits: u8 = value.parse().map_err(|_| VaultError::Malformed)?;
                    if !(6..=8).contains(&digits) {
                        return Err(VaultError::Malformed);
                    }
                    params.digits = digits;
                }
                "algorithm" => {
                    params.algorithm = if value.eq_ignore_ascii_case("SHA1") {
                        TotpAlgorithm::Sha1
                    } else if value.eq_ignore_ascii_case("SHA256") {
                        TotpAlgorithm::Sha256
                    } else if value.eq_ignore_ascii_case("SHA512") {
                        TotpAlgorithm::Sha512
                    } else {
                        return Err(VaultError::Malformed);
                    }
                }
                _ => {}
            }
        }
        let secret = base32_decode(secret_b32.as_deref().ok_or(VaultError::Malformed)?)?;
        return Ok((secret, params));
    }

    Ok((base32_decode(trimmed)?, TotpParams::default()))
}

/// base32 解码（RFC 4648，无填充要求；大小写与空白不敏感）。
fn base32_decode(text: &str) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let mut bits: u32 = 0;
    let mut nbits: u32 = 0;
    let mut out = Vec::with_capacity(text.len() * 5 / 8 + 1);
    for ch in text.chars() {
        if ch == '=' {
            break;
        }
        if ch.is_whitespace() {
            continue;
        }
        let value = match ch.to_ascii_uppercase() {
            'A'..='Z' => ch as u32 - 'A' as u32,
            '2'..='7' => ch as u32 - '2' as u32 + 26,
            _ => return Err(VaultError::Malformed),
        };
        bits = (bits << 5) | value;
        nbits += 5;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    if out.is_empty() {
        return Err(VaultError::Malformed);
    }
    Ok(Zeroizing::new(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 6238 附录 B 测试向量（ASCII 种子 "12345678901234567890" 及其加长版）。
    // base32 编码值由 base32 编码向量（RFC 4648）锁定的同一套字母表得出。
    const SEED_SHA1: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn rfc6238_vectors_sha1() {
        let uri = format!("otpauth://totp/test?secret={SEED_SHA1}&digits=8&period=30&algorithm=SHA1");
        for (at, want) in [
            (59u64, "94287082"),
            (1_111_111_109, "07081804"),
            (1_111_111_111, "14050471"),
            (1_234_567_890, "89005924"),
            (2_000_000_000, "69279037"),
            (20_000_000_000, "65353130"),
        ] {
            assert_eq!(totp_code(&uri, at).unwrap(), want, "T={at}");
        }
    }

    #[test]
    fn rfc6238_vectors_sha256_and_sha512() {
        // 种子分别长 32 / 64 字节 ASCII（"1234…7890" 循环加长）。
        let seed_256 = base32_encode(b"12345678901234567890123456789012");
        let seed_512 = base32_encode(
            b"1234567890123456789012345678901234567890123456789012345678901234",
        );
        let uri_256 =
            format!("otpauth://totp/test?secret={seed_256}&digits=8&period=30&algorithm=SHA256");
        let uri_512 =
            format!("otpauth://totp/test?secret={seed_512}&digits=8&period=30&algorithm=SHA512");
        assert_eq!(totp_code(&uri_256, 59).unwrap(), "46119246");
        assert_eq!(totp_code(&uri_256, 1_111_111_109).unwrap(), "68084774");
        assert_eq!(totp_code(&uri_256, 1_111_111_111).unwrap(), "67062674");
        assert_eq!(totp_code(&uri_512, 59).unwrap(), "90693936");
        assert_eq!(totp_code(&uri_512, 1_111_111_109).unwrap(), "25091201");
        assert_eq!(totp_code(&uri_512, 1_111_111_111).unwrap(), "99943326");
    }

    #[test]
    fn bare_base32_seed_uses_b72_defaults() {
        // 默认 30s / 6 位 / SHA-1：T=59 的 8 位向量 94287082 → 后 6 位 287082。
        assert_eq!(totp_code(SEED_SHA1, 59).unwrap(), "287082");
        // 同一时间片（counter=1 覆盖 T∈[30,60)）内不变，跨片翻转。
        assert_eq!(totp_code(SEED_SHA1, 30).unwrap(), "287082");
        assert_ne!(totp_code(SEED_SHA1, 60).unwrap(), "287082");
    }

    #[test]
    fn rejects_bad_seeds_and_params() {
        assert_eq!(totp_code("", 0).unwrap_err(), VaultError::Malformed);
        assert_eq!(totp_code("1", 0).unwrap_err(), VaultError::Malformed); // 非法 base32 字符
        assert_eq!(
            totp_code("otpauth://totp/test?digits=6", 0).unwrap_err(),
            VaultError::Malformed
        ); // 缺 secret
        assert_eq!(
            totp_code("otpauth://totp/test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&period=0", 0)
                .unwrap_err(),
            VaultError::Malformed
        );
        assert_eq!(
            totp_code("otpauth://totp/test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=5", 0)
                .unwrap_err(),
            VaultError::Malformed
        );
        assert_eq!(
            totp_code("otpauth://totp/test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&algorithm=MD5", 0)
                .unwrap_err(),
            VaultError::Malformed
        );
    }

    #[test]
    fn base32_matches_rfc4648_vectors() {
        for (input, want) in [
            (b"".as_slice(), b"".as_slice()),
            (b"f", b"MY======".as_slice()),
            (b"fo", b"MZXQ====".as_slice()),
            (b"foo", b"MZXW6===".as_slice()),
            (b"foob", b"MZXW6YQ=".as_slice()),
            (b"fooba", b"MZXW6YTB".as_slice()),
            (b"foobar", b"MZXW6YTBOI======".as_slice()),
        ] {
            assert_eq!(base32_encode(input).as_bytes(), want);
            if !input.is_empty() {
                assert_eq!(&*base32_decode(want_str(want).as_str()).unwrap(), input);
            }
        }
    }

    fn want_str(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// 测试辅助编码器（生产只解码）；与上面 RFC 向量互相锁定。
    fn base32_encode(data: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let mut out = String::new();
        let mut bits = 0u32;
        let mut nbits = 0u32;
        for &b in data {
            bits = (bits << 8) | b as u32;
            nbits += 8;
            while nbits >= 5 {
                nbits -= 5;
                out.push(A[((bits >> nbits) & 31) as usize] as char);
            }
        }
        if nbits > 0 {
            out.push(A[((bits << (5 - nbits)) & 31) as usize] as char);
        }
        while out.len() % 8 != 0 {
            out.push('=');
        }
        out
    }
}
