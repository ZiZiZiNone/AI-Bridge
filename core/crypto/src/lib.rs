//! `ab-crypto` —— 密码学原语封装；全仓唯一允许引入密码学依赖的 crate。
#![forbid(unsafe_code)]

use aes_gcm::aead::{Aead, Payload as AeadPayload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

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
            CryptoError::ParamsBelowBaseline => "KDF 参数超出安全边界，拒绝使用",
        };
        f.write_str(s)
    }
}

impl std::error::Error for CryptoError {}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainLabel {
    VaultUnlock,
    ItemSeal,
    Mac,
    SyncPair,
    RecoveryPack,
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
        .encrypt(nonce, AeadPayload { msg: plaintext, aad })
        .map_err(|_| CryptoError::AuthenticationFailed)?;
    Ok(Sealed {
        nonce: nonce_bytes,
        ciphertext,
    })
}

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
            AeadPayload {
                msg: &sealed.ciphertext,
                aad,
            },
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

pub fn hmac_verify(key: &[u8], message: &[u8], tag: &[u8; 32]) -> Result<(), CryptoError> {
    let mut mac = HmacSha256::new_from_slice(key).map_err(|_| CryptoError::InvalidKeyLength)?;
    mac.update(message);
    mac.verify_slice(tag).map_err(|_| CryptoError::InvalidSignature)
}

pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

pub fn ed25519_keypair(
    entropy: &dyn Entropy,
) -> Result<(Zeroizing<[u8; 32]>, [u8; 32]), CryptoError> {
    let mut sk = Zeroizing::new([0u8; 32]);
    entropy.fill(sk.as_mut())?;
    let signing = SigningKey::from_bytes(&sk);
    Ok((sk, signing.verifying_key().to_bytes()))
}

pub fn ed25519_sign(secret_key: &[u8; 32], message: &[u8]) -> [u8; 64] {
    let signing = SigningKey::from_bytes(secret_key);
    signing.sign(message).to_bytes()
}

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

pub fn x25519_keypair(
    entropy: &dyn Entropy,
) -> Result<(Zeroizing<[u8; 32]>, [u8; 32]), CryptoError> {
    let mut sk = Zeroizing::new([0u8; 32]);
    entropy.fill(sk.as_mut())?;
    let secret = x25519_dalek::StaticSecret::from(*sk);
    let public = x25519_dalek::PublicKey::from(&secret);
    Ok((sk, public.to_bytes()))
}

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

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
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
    fn kdf_params_below_or_above_baseline_rejected() {
        let weak = KdfParams { m_kib: 8, t: 1, p: 1 };
        assert_eq!(weak.validated(), Err(CryptoError::ParamsBelowBaseline));
        let weak_t = KdfParams { m_kib: 65536, t: 2, p: 4 };
        assert_eq!(weak_t.validated(), Err(CryptoError::ParamsBelowBaseline));
        let weak_p = KdfParams { m_kib: 65536, t: 3, p: 9 };
        assert_eq!(weak_p.validated(), Err(CryptoError::ParamsBelowBaseline));
        let excessive_m = KdfParams { m_kib: 4 * 1024 * 1024, t: 3, p: 4 };
        assert_eq!(excessive_m.validated(), Err(CryptoError::ParamsBelowBaseline));
        let excessive_t = KdfParams { m_kib: 65536, t: 1000, p: 4 };
        assert_eq!(excessive_t.validated(), Err(CryptoError::ParamsBelowBaseline));
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
    fn x25519_agreement_and_rejects_low_order_point() {
        let e1 = TestEntropy(11);
        let e2 = TestEntropy(22);
        let (sk1, pk1) = x25519_keypair(&e1).expect("密钥生成失败");
        let (sk2, pk2) = x25519_keypair(&e2).expect("密钥生成失败");
        let s1 = x25519_dh(&sk1, &pk2).expect("协商失败");
        let s2 = x25519_dh(&sk2, &pk1).expect("协商失败");
        assert_eq!(s1.as_ref(), s2.as_ref());
        assert_eq!(x25519_dh(&sk1, &[0u8; 32]), Err(CryptoError::InvalidPublicKey));
    }

    #[test]
    fn garbage_inputs_error_not_panic() {
        let _ = hmac_verify(b"", b"", &[0u8; 32]);
        let mut odd_pk = [0u8; 32];
        odd_pk[31] = 0xff;
        let _ = ed25519_verify(&odd_pk, b"", &[0u8; 64]);
    }
}
