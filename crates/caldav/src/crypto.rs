//! Credential encryption at rest: AES-256-GCM with a key derived from the
//! user's passphrase via PBKDF2-HMAC-SHA256. Pure Rust, so it compiles to WASM
//! without JS glue and is unit-testable natively. The caller supplies the
//! randomness (browser: WebCrypto `getRandomValues`).
//!
//! Blob format: `salt(16) || nonce(12) || ciphertext+tag`.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::Sha256;

pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12;
pub const PBKDF2_ROUNDS: u32 = 210_000;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("malformed encrypted blob")]
    Malformed,
    #[error("wrong passphrase or corrupted data")]
    WrongPassphrase,
}

pub fn derive_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, PBKDF2_ROUNDS, &mut key);
    key
}

pub fn encrypt(
    passphrase: &str,
    plaintext: &[u8],
    fill_random: &mut dyn FnMut(&mut [u8]),
) -> Vec<u8> {
    let mut salt = [0u8; SALT_LEN];
    fill_random(&mut salt);
    let mut nonce = [0u8; NONCE_LEN];
    fill_random(&mut nonce);
    let key = derive_key(passphrase, &salt);
    let cipher = Aes256Gcm::new_from_slice(&key).expect("32-byte key is always valid");
    let nonce = Nonce::try_from(nonce.as_slice()).expect("12-byte nonce");
    let ct = cipher
        .encrypt(&nonce, plaintext)
        .expect("in-memory encryption cannot fail");
    let mut blob = Vec::with_capacity(SALT_LEN + NONCE_LEN + ct.len());
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ct);
    blob
}

pub fn decrypt(passphrase: &str, blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < SALT_LEN + NONCE_LEN + 16 {
        return Err(CryptoError::Malformed);
    }
    let (salt, rest) = blob.split_at(SALT_LEN);
    let (nonce, ct) = rest.split_at(NONCE_LEN);
    let key = derive_key(passphrase, salt);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| CryptoError::Malformed)?;
    let nonce = Nonce::try_from(nonce).expect("12-byte nonce");
    cipher
        .decrypt(&nonce, ct)
        .map_err(|_| CryptoError::WrongPassphrase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_random(buf: &mut [u8]) {
        // deterministic per-call but varying across calls, so salts differ
        use std::cell::Cell;
        thread_local! {
            static COUNTER: Cell<u8> = const { Cell::new(0) };
        }
        for b in buf.iter_mut() {
            *b = COUNTER.with(|c| {
                let v = c.get();
                c.set(v.wrapping_add(1));
                v
            }) ^ 0x5A;
        }
    }

    #[test]
    fn roundtrip() {
        let blob = encrypt("hunter2", b"alice:secret-password", &mut test_random);
        let back = decrypt("hunter2", &blob).unwrap();
        assert_eq!(back, b"alice:secret-password");
    }

    #[test]
    fn wrong_passphrase_fails() {
        let blob = encrypt("hunter2", b"data", &mut test_random);
        assert_eq!(decrypt("hunter3", &blob), Err(CryptoError::WrongPassphrase));
    }

    #[test]
    fn malformed_blob_fails() {
        assert_eq!(decrypt("x", &[0u8; 5]), Err(CryptoError::Malformed));
    }

    #[test]
    fn blobs_are_salted() {
        let a = encrypt("p", b"same", &mut test_random);
        let b = encrypt("p", b"same", &mut test_random);
        assert_ne!(a, b, "different salts/nonces must give different blobs");
    }
}
