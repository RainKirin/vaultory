use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::password_hash::{rand_core::RngCore, SaltString};
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use zeroize::Zeroize;

const ARGON2_M_COST: u32 = 19_456;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;

fn argon2_instance() -> Argon2<'static> {
    let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, None)
        .expect("Argon2 参数无效");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub struct CryptoManager {
    cipher: Aes256Gcm,
    key: [u8; 32],
}

impl CryptoManager {
    pub fn derive_key(master_password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
        let argon2 = argon2_instance();
        let mut key = [0u8; 32];
        argon2
            .hash_password_into(master_password.as_bytes(), salt, &mut key)
            .map_err(|e| format!("密钥派生失败: {}", e))?;
        Ok(key)
    }

    pub fn hash_master_password_with_salt(password: &str, salt: &[u8]) -> Result<String, String> {
        let salt_string =
            SaltString::encode_b64(salt).map_err(|e| format!("盐编码失败: {}", e))?;
        let argon2 = argon2_instance();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt_string)
            .map_err(|e| format!("密码哈希失败: {}", e))?;
        Ok(hash.to_string())
    }

    pub fn verify_master_password(password: &str, hash: &str) -> Result<bool, String> {
        let parsed = PasswordHash::new(hash).map_err(|e| format!("密码哈希无效: {}", e))?;
        let argon2 = argon2_instance();
        match argon2.verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(e) => Err(format!("密码验证失败: {}", e)),
        }
    }

    pub fn new(key: &[u8; 32]) -> Self {
        let cipher = Aes256Gcm::new_from_slice(key).expect("Invalid key length");
        Self { cipher, key: *key }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String, String> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| format!("加密失败: {}", e))?;

        let mut combined = nonce_bytes.to_vec();
        combined.extend_from_slice(&ciphertext);
        Ok(BASE64.encode(&combined))
    }

    pub fn decrypt(&self, encrypted: &str) -> Result<String, String> {
        let data = BASE64
            .decode(encrypted)
            .map_err(|e| format!("Base64 解码失败: {}", e))?;

        if data.len() < 12 {
            return Err("加密数据无效".to_string());
        }

        let nonce = Nonce::from_slice(&data[..12]);
        let plaintext = self
            .cipher
            .decrypt(nonce, &data[12..])
            .map_err(|e| format!("解密失败: {}", e))?;

        String::from_utf8(plaintext).map_err(|e| format!("UTF-8 编码无效: {}", e))
    }
}

impl Drop for CryptoManager {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

pub fn generate_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    salt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_master_password_should_reject_wrong_password() {
        let salt = [1u8; 32];
        let hash = CryptoManager::hash_master_password_with_salt("correct-password", &salt)
            .expect("hash should succeed");
        assert!(CryptoManager::verify_master_password("correct-password", &hash).unwrap());
        assert!(!CryptoManager::verify_master_password("wrong-password", &hash).unwrap());
    }

    #[test]
    fn verify_master_password_should_return_err_for_invalid_hash_format() {
        let result = CryptoManager::verify_master_password("any", "not-a-valid-phc-string");
        assert!(result.is_err());
    }

    #[test]
    fn argon2_params_should_match_recommended_defaults() {
        assert_eq!(ARGON2_M_COST, 19_456);
        assert_eq!(ARGON2_T_COST, 2);
        assert_eq!(ARGON2_P_COST, 1);
        let _ = argon2_instance();
    }
}
