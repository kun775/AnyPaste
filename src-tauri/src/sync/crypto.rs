//! 端到端加密：口令经 Argon2id 派生 64 字节，前 32 字节作 AES-256-GCM 密钥，
//! 后 32 字节作对象名的哈希密钥（避免以内容哈希命名而泄露“是否包含某段已知内容”）。

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, bail};
use argon2::Argon2;

pub const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

pub struct Cipher {
    aead: Aes256Gcm,
    id_key: [u8; 32],
}

impl Cipher {
    pub fn derive(passphrase: &str, salt: &[u8]) -> anyhow::Result<Self> {
        let mut key = [0u8; 64];
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|e| anyhow!("派生密钥失败: {e}"))?;
        let mut id_key = [0u8; 32];
        id_key.copy_from_slice(&key[32..]);
        Ok(Self { aead: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key[..32])), id_key })
    }

    /// 输出格式：12 字节随机 nonce + 密文（含认证标签）。
    pub fn encrypt(&self, plain: &[u8]) -> anyhow::Result<Vec<u8>> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = self.aead.encrypt(&nonce, plain).map_err(|_| anyhow!("加密失败"))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub fn decrypt(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        if data.len() < NONCE_LEN {
            bail!("密文长度无效");
        }
        let (nonce, ct) = data.split_at(NONCE_LEN);
        self.aead
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|_| anyhow!("解密失败：同步口令错误或数据已损坏"))
    }

    /// 由内容哈希得到云端对象名。
    pub fn object_id(&self, content_hash: &str) -> String {
        blake3::keyed_hash(&self.id_key, content_hash.as_bytes()).to_hex().to_string()
    }
}

pub fn random_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_wrong_key() {
        let salt = random_salt();
        let a = Cipher::derive("correct horse", &salt).unwrap();
        let ct = a.encrypt(b"hello").unwrap();
        assert_ne!(&ct[NONCE_LEN..], b"hello");
        assert_eq!(a.decrypt(&ct).unwrap(), b"hello");
        // 同一明文两次加密结果不同（随机 nonce）
        assert_ne!(a.encrypt(b"hello").unwrap(), ct);

        let b = Cipher::derive("wrong", &salt).unwrap();
        assert!(b.decrypt(&ct).is_err());
        assert!(a.decrypt(&ct[..5]).is_err());
    }

    #[test]
    fn object_id_is_keyed_and_stable() {
        let salt = random_salt();
        let a = Cipher::derive("p", &salt).unwrap();
        assert_eq!(a.object_id("h"), Cipher::derive("p", &salt).unwrap().object_id("h"));
        assert_ne!(a.object_id("h"), Cipher::derive("p", &random_salt()).unwrap().object_id("h"));
        assert_ne!(a.object_id("h"), "h");
    }
}
