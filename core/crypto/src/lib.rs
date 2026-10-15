//! `ab-crypto` —— 密码学原语封装；全仓唯一允许引入密码学依赖的 crate。
//!
//! 硬规则：`forbid(unsafe_code)`；非测试代码禁 `unwrap/expect/panic!`；
//! MAC/签名/摘要比较走常数时间路径；秘密材料一律 `Zeroizing`；
//! KDF 参数强制下限，降级一律拒绝；AES-GCM nonce 96-bit 随机（注入熵源）。
#![forbid(unsafe_code)]

use aes_gcm::aead::{Aead, Payload as AeadPayload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use ed25519_dalek::{Signer, Signature, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use subtle::ConstantTimeEq;
pub use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;
type HmacSha1 = Hmac<Sha1>;
type HmacSha512 = Hmac<Sha512>;

/// `Display` 只输出固定文案：密钥/明文/密文材料不进错误信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    EntropyFailed,
    InvalidKeyLength,
    InvalidNonceLength,
    InvalidSaltLength,
    InvalidOutputLength,
    AuthenticationFailed,
    InvalidSignature,
    InvalidPublicKey,
    ParamsBelowBaseline,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CryptoError::EntropyFailed => "随机源不可用",
            CryptoError::InvalidKeyLength => "密钥长度非法",
            CryptoError::InvalidNonceLength => "nonce 长度非法",
            CryptoError::InvalidSaltLength => "盐长度非法",
            CryptoError::InvalidOutputLength => "输出长度非法",
            CryptoError::AuthenticationFailed => "认证失败：密文被篡改或密钥/附加数据不符",
            CryptoError::InvalidSignature => "签名验证失败",
            CryptoError::InvalidPublicKey => "公钥格式非法",
            CryptoError::ParamsBelowBaseline => "KDF 参数低于安全基线，拒绝使用",
        };
        f.write_str(s)
    }
}

impl std::error::Error for CryptoError {}

/// 熵源注入点：core 不自取系统随机。
pub trait Entropy: Send + Sync {
    fn fill(&self, out: &mut [u8]) -> Result<(), CryptoError>;
}

pub struct OsEntropy;

impl Entropy for OsEntropy {
    fn fill(&self, out: &mut [u8]) -> Result<(), CryptoError> {
        getrandom::getrandom(out).map_err(|_| CryptoError::EntropyFailed)
    }
}

pub fn random_bytes(entropy: &dyn Entropy, out: &mut [u8]) -> Result<(), CryptoError> {
    entropy.fill(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        KdfParams { m_kib: 65536, t: 3, p: 4 }
    }
}

impl KdfParams {
    pub const MIN_M_KIB: u32 = 65536;
    pub const MAX_M_KIB: u32 = 1024 * 1024;
    pub const MIN_T: u32 = 3;
    pub const MAX_T: u32 = 32;
    pub const MAX_P: u32 = 4;

    /// 文件头里的 KDF 参数是攻击者可控输入，越出安全边界直接拒绝，不「尽力解锁」。
    pub fn validated(self) -> Result<Self, CryptoError> {
        if self.m_kib < Self::MIN_M_KIB
            || self.m_kib > Self::MAX_M_KIB
            || self.t < Self::MIN_T
            || self.t > Self::MAX_T
            || self.p == 0
            || self.p > Self::MAX_P
        {
            return Err(CryptoError::ParamsBelowBaseline);
        }
        Ok(self)
    }
}

/// Argon2id 派生 32 字节密钥。口令不做规范化（原始字节），编码由调用方统一。
pub fn derive(
    password: &[u8],
    salt: &[u8],
    params: &KdfParams,
) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    let p = params.validated()?;
    if salt.len() < 16 {
        return Err(CryptoError::InvalidSaltLength);
    }
    let argon_params = Params::new(p.m_kib, p.t, p.p, Some(32))
        .map_err(|_| CryptoError::ParamsBelowBaseline)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password, salt, out.as_mut())
        .map_err(|_| CryptoError::InvalidOutputLength)?;
    Ok(out)
}

/// 域分离标签：全仓唯一来源，不同用途永不派生出相同密钥。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainLabel {
    VaultUnlock,
    ItemSeal,
    Mac,
    SyncPair,
    RecoveryPack,
    RecoveryKey,
    BrokerToken,
}

impl DomainLabel {
    fn as_bytes(self) -> &'static [u8] {
        match self {
            DomainLabel::VaultUnlock => b"aibridge/v1/vault-unlock",
            DomainLabel::ItemSeal => b"aibridge/v1/item-seal",
            DomainLabel::Mac => b"aibridge/v1/mac",
            DomainLabel::SyncPair => b"aibridge/v1/sync-pair",
            DomainLabel::RecoveryPack => b"aibridge/v1/recovery-pack",
            DomainLabel::RecoveryKey => b"aibridge/v1/recovery-key",
            DomainLabel::BrokerToken => b"aibridge/v1/broker-token",
        }
    }
}

pub fn derive_domain(
    ikm: &[u8],
    salt: Option<&[u8]>,
    label: DomainLabel,
) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    let hk = Hkdf::<Sha256>::new(salt, ikm);
    let mut out = Zeroizing::new([0u8; 32]);
    hk.expand(label.as_bytes(), out.as_mut())
        .map_err(|_| CryptoError::InvalidOutputLength)?;
    Ok(out)
}

/// 96-bit 随机 nonce + 密文（认证标签内含）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

fn aead_key(key: &[u8; 32]) -> Aes256Gcm {
    <Aes256Gcm as aes_gcm::aead::KeyInit>::new(Key::<Aes256Gcm>::from_slice(key))
}

pub fn seal(
    entropy: &dyn Entropy,
    key: &[u8; 32],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Sealed, CryptoError> {
    let mut nonce_bytes = [0u8; 12];
    entropy.fill(&mut nonce_bytes)?;
    let cipher = aead_key(key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            AeadPayload { msg: plaintext, aad },
        )
        .map_err(|_| CryptoError::AuthenticationFailed)?;
    Ok(Sealed { nonce: nonce_bytes, ciphertext })
}

/// 认证失败（篡改/错钥/错 AAD）一律 [`CryptoError::AuthenticationFailed`]。
pub fn open(
    key: &[u8; 32],
    aad: &[u8],
    sealed: &Sealed,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let cipher = aead_key(key);
    let nonce = Nonce::from_slice(&sealed.nonce);
    let plaintext = cipher
        .decrypt(
            nonce,
            AeadPayload { msg: &sealed.ciphertext, aad },
        )
        .map_err(|_| CryptoError::AuthenticationFailed)?;
    Ok(Zeroizing::new(plaintext))
}

pub fn hmac_sign(key: &[u8], message: &[u8]) -> Result<[u8; 32], CryptoError> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength)?;
    mac.update(message);
    let tag = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&tag);
    Ok(out)
}

/// 常数时间验证；失败统一 [`CryptoError::InvalidSignature`]。
pub fn hmac_verify(key: &[u8], message: &[u8], tag: &[u8; 32]) -> Result<(), CryptoError> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength)?;
    mac.update(message);
    mac.verify_slice(tag).map_err(|_| CryptoError::InvalidSignature)
}

/// 常数时间比较；禁止用 `==` 比较秘密。
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

pub fn ed25519_keypair(entropy: &dyn Entropy) -> Result<(Zeroizing<[u8; 32]>, [u8; 32]), CryptoError> {
    let mut sk = Zeroizing::new([0u8; 32]);
    entropy.fill(sk.as_mut())?;
    let signing = SigningKey::from_bytes(&sk);
    Ok((sk, signing.verifying_key().to_bytes()))
}

pub fn ed25519_sign(secret_key: &[u8; 32], message: &[u8]) -> [u8; 64] {
    let signing = SigningKey::from_bytes(secret_key);
    signing.sign(message).to_bytes()
}

/// `verify_strict`；失败统一 [`CryptoError::InvalidSignature`]。
pub fn ed25519_verify(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<(), CryptoError> {
    let vk = VerifyingKey::from_bytes(public_key).map_err(|_| CryptoError::InvalidPublicKey)?;
    let sig = Signature::from_bytes(signature);
    vk.verify_strict(message, &sig)
        .map_err(|_| CryptoError::InvalidSignature)
}

pub fn x25519_keypair(entropy: &dyn Entropy) -> Result<(Zeroizing<[u8; 32]>, [u8; 32]), CryptoError> {
    let mut sk = Zeroizing::new([0u8; 32]);
    entropy.fill(sk.as_mut())?;
    let secret = x25519_dalek::StaticSecret::from(*sk);
    let public = x25519_dalek::PublicKey::from(&secret);
    Ok((sk, public.to_bytes()))
}

/// 由既有私钥导出公钥（持久化恢复身份等场景）。
pub fn x25519_public(secret_key: &[u8; 32]) -> [u8; 32] {
    let secret = x25519_dalek::StaticSecret::from(*secret_key);
    x25519_dalek::PublicKey::from(&secret).to_bytes()
}

/// 共享秘密 `Zeroizing` 承载。
pub fn x25519_dh(
    secret_key: &[u8; 32],
    peer_public: &[u8; 32],
) -> Result<Zeroizing<[u8; 32]>, CryptoError> {
    let secret = x25519_dalek::StaticSecret::from(*secret_key);
    let public = x25519_dalek::PublicKey::from(*peer_public);
    let shared = secret.diffie_hellman(&public);
    let bytes = shared.to_bytes();
    if ct_eq(&bytes, &[0u8; 32]) {
        return Err(CryptoError::InvalidPublicKey);
    }
    Ok(Zeroizing::new(bytes))
}

/// 公开数据用；不用于秘密比较。
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// AES-256-CBC（PKCS#7）：KDBX4 外层加密。
pub fn aes256_cbc_encrypt(key: &[u8; 32], iv: &[u8; 16], plaintext: &[u8]) -> Vec<u8> {
    use aes::Aes256;
    use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    cbc::Encryptor::<Aes256>::new(key.into(), iv.into())
        .encrypt_padded_vec_mut::<Pkcs7>(plaintext)
}

/// 解密失败（篡改/错钥/错 IV）一律 [`CryptoError::AuthenticationFailed`]。
pub fn aes256_cbc_decrypt(
    key: &[u8; 32],
    iv: &[u8; 16],
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    use aes::Aes256;
    use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    let plain = cbc::Decryptor::<Aes256>::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| CryptoError::AuthenticationFailed)?;
    Ok(Zeroizing::new(plain))
}

pub fn sha512(data: &[u8]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn hmac_sha1(key: &[u8], message: &[u8]) -> Result<[u8; 20], CryptoError> {
    let mut mac = HmacSha1::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength)?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().into())
}

pub fn hmac_sha512(key: &[u8], message: &[u8]) -> Result<[u8; 64], CryptoError> {
    let mut mac = HmacSha512::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength)?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().into())
}

/// 一次性 ChaCha20（计数器 0 起）。
pub fn chacha20_xor(key: &[u8; 32], nonce: &[u8; 12], data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    let mut cipher = ChaCha20::new(key.into(), nonce.into());
    cipher.apply_keystream(&mut out);
    out
}

/// 连续密钥流（KDBX 内部保护值流）：多次调用共享同一条流。
pub struct ChaCha20Stream {
    cipher: ChaCha20,
}

impl ChaCha20Stream {
    /// key/nonce 来自 `sha512(stream_key)` 切片（KDBX 约定）。
    pub fn new(key: &[u8; 32], nonce: &[u8; 12]) -> Self {
        Self {
            cipher: ChaCha20::new(key.into(), nonce.into()),
        }
    }

    pub fn apply(&mut self, data: &mut [u8]) {
        self.cipher.apply_keystream(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEntropy(u8);
    impl Entropy for TestEntropy {
        fn fill(&self, out: &mut [u8]) -> Result<(), CryptoError> {
            for (i, b) in out.iter_mut().enumerate() {
                *b = self.0.wrapping_add(i as u8).wrapping_mul(31);
            }
            Ok(())
        }
    }

    fn test_salt() -> [u8; 16] {
        [7u8; 16]
    }

    #[test]
    fn kdf_params_below_baseline_rejected() {
        let weak = KdfParams { m_kib: 8, t: 1, p: 1 };
        assert_eq!(weak.validated(), Err(CryptoError::ParamsBelowBaseline));
        let weak_t = KdfParams { m_kib: 65536, t: 2, p: 4 };
        assert_eq!(weak_t.validated(), Err(CryptoError::ParamsBelowBaseline));
        let weak_p = KdfParams { m_kib: 65536, t: 3, p: 9 };
        assert_eq!(weak_p.validated(), Err(CryptoError::ParamsBelowBaseline));
        assert!(KdfParams::default().validated().is_ok());
    }

    #[test]
    fn derive_rejects_short_salt_and_is_deterministic() {
        let params = KdfParams::default();
        assert_eq!(derive(b"pw", &[0u8; 8], &params), Err(CryptoError::InvalidSaltLength));
        let a = derive(b"pw", &test_salt(), &params).expect("派生失败");
        let b = derive(b"pw", &test_salt(), &params).expect("派生失败");
        assert_eq!(a.as_ref(), b.as_ref());
    }

    #[test]
    fn domain_labels_never_collide() {
        let ikm = [9u8; 32];
        let mut keys = Vec::new();
        for label in [
            DomainLabel::VaultUnlock,
            DomainLabel::ItemSeal,
            DomainLabel::Mac,
            DomainLabel::SyncPair,
            DomainLabel::RecoveryPack,
            DomainLabel::RecoveryKey,
            DomainLabel::BrokerToken,
        ] {
            keys.push(derive_domain(&ikm, None, label).expect("派生失败"));
        }
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(keys[i].as_ref(), keys[j].as_ref(), "域标签 {i} 与 {j} 冲突");
            }
        }
    }

    #[test]
    fn seal_open_roundtrip() {
        let entropy = TestEntropy(1);
        let key = [3u8; 32];
        let msg = "秘密明文";
        let sealed = seal(&entropy, &key, b"aad", msg.as_bytes()).expect("封印失败");
        let plain = open(&key, b"aad", &sealed).expect("拆封失败");
        assert_eq!(plain.as_slice(), msg.as_bytes());
    }

    #[test]
    fn tampered_ciphertext_wrong_key_and_wrong_aad_all_fail() {
        let entropy = TestEntropy(2);
        let key = [3u8; 32];
        let sealed = seal(&entropy, &key, b"aad", b"data").expect("封印失败");

        let mut bad = sealed.clone();
        bad.ciphertext[0] ^= 0x01;
        assert_eq!(open(&key, b"aad", &bad), Err(CryptoError::AuthenticationFailed));

        let wrong_key = [4u8; 32];
        assert_eq!(open(&wrong_key, b"aad", &sealed), Err(CryptoError::AuthenticationFailed));

        assert_eq!(open(&key, b"bad", &sealed), Err(CryptoError::AuthenticationFailed));
    }

    #[test]
    fn nonce_is_random_per_call() {
        let entropy = OsEntropy;
        let key = [5u8; 32];
        let s1 = seal(&entropy, &key, b"", b"x").expect("封印失败");
        let s2 = seal(&entropy, &key, b"", b"x").expect("封印失败");
        assert_ne!(s1.nonce, s2.nonce, "同钥同明文必须产生不同 nonce");
    }

    #[test]
    fn hmac_sign_verify_and_tamper() {
        let tag = hmac_sign(b"key", b"msg").expect("签名失败");
        assert!(hmac_verify(b"key", b"msg", &tag).is_ok());
        let mut bad = tag;
        bad[0] ^= 0x01;
        assert_eq!(hmac_verify(b"key", b"msg", &bad), Err(CryptoError::InvalidSignature));
        assert_eq!(hmac_verify(b"key2", b"msg", &tag), Err(CryptoError::InvalidSignature));
    }

    #[test]
    fn ed25519_sign_verify_and_tamper() {
        let entropy = TestEntropy(3);
        let (sk, pk) = ed25519_keypair(&entropy).expect("密钥生成失败");
        let sig = ed25519_sign(&sk, b"payload");
        assert!(ed25519_verify(&pk, b"payload", &sig).is_ok());
        let mut bad_sig = sig;
        bad_sig[0] ^= 0x01;
        assert_eq!(ed25519_verify(&pk, b"payload", &bad_sig), Err(CryptoError::InvalidSignature));
        assert_eq!(ed25519_verify(&pk, b"other", &sig), Err(CryptoError::InvalidSignature));
    }

    #[test]
    fn x25519_agreement() {
        let e1 = TestEntropy(11);
        let e2 = TestEntropy(22);
        let (sk1, pk1) = x25519_keypair(&e1).expect("密钥生成失败");
        let (sk2, pk2) = x25519_keypair(&e2).expect("密钥生成失败");
        let s1 = x25519_dh(&sk1, &pk2).expect("协商失败");
        let s2 = x25519_dh(&sk2, &pk1).expect("协商失败");
        assert_eq!(s1.as_ref(), s2.as_ref());
    }

    #[test]
    fn x25519_public_derives_from_secret() {
        let e = TestEntropy(11);
        let (sk, pk) = x25519_keypair(&e).expect("密钥生成失败");
        assert_eq!(x25519_public(&sk), pk, "同私钥必须导出同一公钥");
    }

    #[test]
    fn garbage_inputs_error_not_panic() {
        let _ = hmac_verify(b"", b"", &[0u8; 32]);
        let mut odd_pk = [0u8; 32];
        odd_pk[31] = 0xff;
        let _ = ed25519_verify(&odd_pk, b"", &[0u8; 64]);
    }

    #[test]
    fn aes_cbc_roundtrip_and_tamper() {
        let key = [7u8; 32];
        let iv = [9u8; 16];
        let pt = b"KDBX payload";
        let ct = aes256_cbc_encrypt(&key, &iv, pt);
        assert_eq!(aes256_cbc_decrypt(&key, &iv, &ct).unwrap().as_slice(), pt);

        let mut bad = ct.clone();
        bad[0] ^= 0x01;
        assert_eq!(
            aes256_cbc_decrypt(&key, &iv, &bad),
            Err(CryptoError::AuthenticationFailed)
        );
    }

    #[test]
    fn chacha20_stream_is_sequential() {
        let key = [1u8; 32];
        let nonce = [2u8; 12];
        let mut full = b"0123456789abcdef".to_vec();
        let mut split = full.clone();
        let oneshot = chacha20_xor(&key, &nonce, &full);
        let mut stream = ChaCha20Stream::new(&key, &nonce);
        stream.apply(&mut split[..7]);
        stream.apply(&mut split[7..]);
        full = oneshot.clone();
        assert_eq!(full, split, "连续流与一次性结果必须一致");
    }
}
