use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("encryption failed")]
    EncryptionFailed,
    #[error("decryption failed")]
    DecryptionFailed,
    #[error("invalid data format")]
    InvalidFormat,
}

const NONCE_SIZE: usize = 12; // 96 bits for AES-256-GCM

/// Derive a 256-bit encryption key from machine-specific data.
/// Uses a combination of factors to create a stable key unique to this machine.
fn derive_key() -> [u8; 32] {
    let mut hasher = Sha256::new();

    // Use multiple sources for key derivation to make it machine-specific
    // but stable across restarts

    // 1. Application identifier (constant)
    hasher.update(b"vibe-kanban-settings-encryption-v1");

    // 2. User's home directory (stable per user)
    if let Some(home) = dirs::home_dir() {
        hasher.update(home.to_string_lossy().as_bytes());
    }

    // 3. Data directory (stable per installation)
    if let Some(data_dir) = dirs::data_local_dir() {
        hasher.update(data_dir.to_string_lossy().as_bytes());
    }

    // 4. Config directory (stable per installation)
    if let Some(config_dir) = dirs::config_dir() {
        hasher.update(config_dir.to_string_lossy().as_bytes());
    }

    hasher.finalize().into()
}

/// Encrypt a token using AES-256-GCM with a machine-derived key.
/// Returns a URL-safe base64-encoded string containing nonce + ciphertext.
pub fn encrypt_token(token: &str) -> Result<String, EncryptionError> {
    let key_bytes = derive_key();
    let key = Key::<Aes256Gcm>::from(key_bytes);
    let cipher = Aes256Gcm::new(&key);

    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, token.as_bytes())
        .map_err(|_| EncryptionError::EncryptionFailed)?;

    // Combine nonce + ciphertext
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(URL_SAFE_NO_PAD.encode(combined))
}

/// Decrypt a token using AES-256-GCM with a machine-derived key.
/// Expects a URL-safe base64-encoded string containing nonce + ciphertext.
pub fn decrypt_token(encrypted: &str) -> Result<String, EncryptionError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(encrypted)
        .map_err(|_| EncryptionError::InvalidFormat)?;

    if decoded.len() < NONCE_SIZE {
        return Err(EncryptionError::InvalidFormat);
    }

    let key_bytes = derive_key();
    let key = Key::<Aes256Gcm>::from(key_bytes);
    let cipher = Aes256Gcm::new(&key);

    let nonce_bytes: [u8; NONCE_SIZE] = decoded[..NONCE_SIZE]
        .try_into()
        .map_err(|_| EncryptionError::InvalidFormat)?;
    let nonce = Nonce::from(nonce_bytes);
    let ciphertext = &decoded[NONCE_SIZE..];

    let plaintext = cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|_| EncryptionError::DecryptionFailed)?;

    String::from_utf8(plaintext).map_err(|_| EncryptionError::DecryptionFailed)
}
