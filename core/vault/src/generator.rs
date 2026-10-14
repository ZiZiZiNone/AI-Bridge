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

/// A5.2 口令词模式：「可切换、可配词表」——词表、词数、分隔符全部由调用方配置，
/// core 不内置词表、不发明默认值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseSpec {
    pub words: Vec<String>,
    pub word_count: usize,
    pub separator: String,
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
    let charset = spec.charset()?;

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

impl GeneratorSpec {
    /// 选中字符集的并集。
    fn charset(&self) -> Result<String, VaultError> {
        let mut charset = String::new();
        if self.lowercase {
            charset.push_str(LOWER);
        }
        if self.uppercase {
            charset.push_str(UPPER);
        }
        if self.digits {
            charset.push_str(DIGITS);
        }
        if self.symbols {
            charset.push_str(SYMBOLS);
        }
        if charset.is_empty() {
            return Err(VaultError::Malformed);
        }
        Ok(charset)
    }

    /// A5.1「实时强度条」的数值基础：本生成过程的香农熵（bit）= 长度 × log2(字符集)。
    pub fn entropy_bits(&self) -> Result<f64, VaultError> {
        if self.length == 0 {
            return Ok(0.0);
        }
        Ok(self.length as f64 * (self.charset()?.len() as f64).log2())
    }
}

impl PassphraseSpec {
    /// 口令词模式的香农熵（bit）= 词数 × log2(词表)。
    pub fn entropy_bits(&self) -> Result<f64, VaultError> {
        if self.words.is_empty() {
            return Err(VaultError::Malformed);
        }
        Ok(self.word_count as f64 * (self.words.len() as f64).log2())
    }
}

/// A5.2 口令词生成：词表上均匀取样（拒绝采样无模偏差），按分隔符连接。
/// 与字符模式同纪律：无状态、不保存生成历史（B6.2）。
pub fn generate_passphrase(
    spec: &PassphraseSpec,
    entropy: &dyn Entropy,
) -> Result<Zeroizing<String>, VaultError> {
    if spec.words.is_empty() || spec.words.iter().any(|w| w.is_empty()) {
        return Err(VaultError::Malformed);
    }
    if spec.word_count == 0 || spec.word_count > 64 {
        return Err(VaultError::Malformed);
    }
    let n = spec.words.len();
    if n > 65536 {
        return Err(VaultError::Malformed);
    }
    // u16 取样 + 阈值拒绝（去模偏差），单条路径覆盖任意词表 ≤ 65536。
    let limit = 65536 - (65536 % n);
    let mut picked: Vec<&str> = Vec::with_capacity(spec.word_count);
    let mut buf = [0u8; 64];
    while picked.len() < spec.word_count {
        random_bytes(entropy, &mut buf).map_err(|_| VaultError::Malformed)?;
        for chunk in buf.chunks_exact(2) {
            let value = u16::from_be_bytes([chunk[0], chunk[1]]) as usize;
            if value < limit {
                picked.push(spec.words[value % n].as_str());
                if picked.len() == spec.word_count {
                    break;
                }
            }
        }
    }
    Ok(Zeroizing::new(picked.join(&spec.separator)))
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

    #[test]
    fn passphrase_uses_only_configured_wordlist() {
        let entropy = OsEntropy;
        let spec = PassphraseSpec {
            words: (0..2048).map(|i| format!("word{i}")).collect(),
            word_count: 6,
            separator: "-".to_string(),
        };
        let phrase = generate_passphrase(&spec, &entropy).unwrap();
        let parts: Vec<&str> = phrase.split('-').collect();
        assert_eq!(parts.len(), 6);
        assert!(parts.iter().all(|w| spec.words.iter().any(|x| x == w)));
    }

    #[test]
    fn passphrase_rejects_bad_specs() {
        let entropy = OsEntropy;
        let base = |w: usize, c: usize| PassphraseSpec {
            words: (0..w).map(|i| format!("w{i}")).collect(),
            word_count: c,
            separator: " ".to_string(),
        };
        assert_eq!(
            generate_passphrase(&base(0, 5), &entropy).unwrap_err(),
            VaultError::Malformed
        ); // 空词表
        assert_eq!(
            generate_passphrase(&base(4, 0), &entropy).unwrap_err(),
            VaultError::Malformed
        ); // 词数为 0
        let mut bad = base(4, 5);
        bad.words[1] = String::new();
        assert_eq!(
            generate_passphrase(&bad, &entropy).unwrap_err(),
            VaultError::Malformed
        ); // 空词
    }

    #[test]
    fn entropy_bits_matches_information_theory() {
        // A5.1 强度条数值：20 位 × log2(26) ≈ 94.04 bit。
        let spec = GeneratorSpec {
            length: 20,
            lowercase: true,
            uppercase: false,
            digits: false,
            symbols: false,
        };
        assert!((spec.entropy_bits().unwrap() - 20.0 * 26f64.log2()).abs() < 1e-9);
        // 口令词：6 词 × log2(2048) = 66 bit。
        let phrase = PassphraseSpec {
            words: (0..2048).map(|i| format!("w{i}")).collect(),
            word_count: 6,
            separator: "-".to_string(),
        };
        assert!((phrase.entropy_bits().unwrap() - 66.0).abs() < 1e-9);
    }
}
