//! B6 密码生成器——「默认 20 位；charset/长度/符号规则可改」的唯一业务实现。
//!
//! 边界（docs/功能点全景.md B6.1/B6.2）：
//!   - 无状态：不保存、不回传任何生成历史（B6.2 明文纪律）；调用方拿到即用。
//!   - 熵经注入的 `Entropy`（与 core 全部随机面同一纪律），拒绝采样去模偏差。
//!   - 秘密 `Zeroizing` 承载，Debug 脱敏（F7 内存卫生）。

use std::fmt;

use ab_crypto::{random_bytes, Entropy, Zeroizing};

use crate::VaultError;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
/// 符号集 = ASCII 可打印标点全集（docs 未规定确切符号集，README 记为待确认）。
const SYMBOLS: &str = "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

/// 生成参数（B6.1：默认 20 位，四类字符集全开；均可改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorSpec {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Default for GeneratorSpec {
    fn default() -> Self {
        Self {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
        }
    }
}

/// 生成结果（`value` 秘密脱敏；`charset` 为本次实际使用的字符集，供 UI 展示）。
pub struct GeneratedSecret {
    pub value: Zeroizing<String>,
    pub charset: String,
}

impl fmt::Debug for GeneratedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratedSecret")
            .field("value", &"[REDACTED]")
            .field("charset", &self.charset)
            .finish()
    }
}

/// 生成口令：在选中字符集上均匀取样（拒绝采样无模偏差）。
pub fn generate_secret(
    spec: &GeneratorSpec,
    entropy: &dyn Entropy,
) -> Result<GeneratedSecret, VaultError> {
    if spec.length == 0 || spec.length > 1024 {
        return Err(VaultError::Malformed);
    }
    let mut charset = String::new();
    if spec.lowercase {
        charset.push_str(LOWER);
    }
    if spec.uppercase {
        charset.push_str(UPPER);
    }
    if spec.digits {
        charset.push_str(DIGITS);
    }
    if spec.symbols {
        charset.push_str(SYMBOLS);
    }
    if charset.is_empty() {
        return Err(VaultError::Malformed);
    }

    let chars: Vec<char> = charset.chars().collect();
    // 拒绝采样阈值：小于该值的字节才可映射，去掉模偏差（字符集 ≤ 94 < 256）。
    let limit = 256 - (256 % chars.len());
    let mut value = String::with_capacity(spec.length);
    let mut buf = [0u8; 64];
    while value.len() < spec.length {
        random_bytes(entropy, &mut buf).map_err(|_| VaultError::Malformed)?;
        for &byte in &buf {
            if (byte as usize) < limit {
                value.push(chars[byte as usize % chars.len()]);
                if value.len() == spec.length {
                    break;
                }
            }
        }
    }

    Ok(GeneratedSecret {
        value: Zeroizing::new(value),
        charset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_crypto::OsEntropy;

    #[test]
    fn default_spec_is_b61() {
        let spec = GeneratorSpec::default();
        assert_eq!(spec.length, 20);
        assert!(spec.lowercase && spec.uppercase && spec.digits && spec.symbols);
    }

    #[test]
    fn generates_requested_length_within_charset() {
        let entropy = OsEntropy;
        for spec in [
            GeneratorSpec::default(),
            GeneratorSpec {
                length: 8,
                symbols: false,
                ..Default::default()
            },
            GeneratorSpec {
                length: 64,
                lowercase: false,
                uppercase: false,
                digits: false,
                symbols: true,
            },
        ] {
            let secret = generate_secret(&spec, &entropy).unwrap();
            assert_eq!(secret.value.chars().count(), spec.length);
            assert!(secret.value.chars().all(|c| secret.charset.contains(c)));
        }
    }

    #[test]
    fn rejects_bad_specs() {
        let entropy = OsEntropy;
        assert_eq!(
            generate_secret(
                &GeneratorSpec {
                    length: 0,
                    ..Default::default()
                },
                &entropy
            )
            .unwrap_err(),
            VaultError::Malformed
        );
        assert_eq!(
            generate_secret(
                &GeneratorSpec {
                    lowercase: false,
                    uppercase: false,
                    digits: false,
                    symbols: false,
                    ..Default::default()
                },
                &entropy
            )
            .unwrap_err(),
            VaultError::Malformed
        );
    }

    #[test]
    fn debug_redacts_value() {
        let entropy = OsEntropy;
        let secret = generate_secret(&GeneratorSpec::default(), &entropy).unwrap();
        let dbg = format!("{secret:?}");
        assert!(dbg.contains("[REDACTED]"));
        assert!(!dbg.contains(secret.value.as_str()));
    }

    #[test]
    fn no_history_state() {
        // B6.2：生成器无状态——本模块不含任何存储/历史结构，此测试锁住该纪律。
        let entropy = OsEntropy;
        let a = generate_secret(&GeneratorSpec::default(), &entropy).unwrap();
        let b = generate_secret(&GeneratorSpec::default(), &entropy).unwrap();
        assert_ne!(a.value.as_str(), b.value.as_str(), "两次生成互不相同");
    }
}
