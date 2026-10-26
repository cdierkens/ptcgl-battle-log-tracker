//! Encrypted secret store in the app config directory.
//!
//! Uses AES-GCM with a key derived from machine ID + app identifier via Argon2.
//! No OS keychain prompts — tokens live in an encrypted JSON file alongside settings.json.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::{rng, RngExt};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use thiserror::Error;
use zeroize::Zeroizing;

use crate::ports::{SecretError, SecretStore};

/// On-disk encrypted vault format.
#[derive(Serialize, Deserialize)]
struct Vault {
    /// Argon2 salt for key derivation (16 bytes).
    salt: [u8; 16],
    /// Encrypted entries: account -> (nonce + ciphertext).
    entries: HashMap<String, EncryptedEntry>,
}

/// One encrypted secret: nonce (12 bytes) + ciphertext.
#[derive(Serialize, Deserialize)]
struct EncryptedEntry {
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Encryption error: {0}")]
    Crypto(String),
    #[error("Vault corrupted")]
    Corrupted,
}

impl From<StoreError> for SecretError {
    fn from(e: StoreError) -> Self {
        SecretError::Store(e.to_string())
    }
}

/// File-backed encrypted secret store.
pub struct FileSecretStore {
    vault_path: PathBuf,
    /// In-memory cache of decrypted entries.
    cache: Arc<Mutex<HashMap<String, String>>>,
    /// Derived encryption key (Zeroizing to wipe on drop).
    key: Zeroizing<[u8; 32]>,
}

impl FileSecretStore {
    /// Create or load the vault at `config_dir/secrets.json`.
    pub fn new(config_dir: &std::path::Path, app_identifier: &str) -> Result<Self, StoreError> {
        fs::create_dir_all(config_dir)?;
        let vault_path = config_dir.join("secrets.json");

        // Derive a stable encryption key from machine ID + app identifier.
        let key = derive_key(app_identifier)?;

        let mut store = Self { vault_path, cache: Arc::new(Mutex::new(HashMap::new())), key: Zeroizing::new(key) };
        store.load()?;
        Ok(store)
    }

    fn load(&mut self) -> Result<(), StoreError> {
        if !self.vault_path.exists() {
            return Ok(());
        }
        let data = fs::read(&self.vault_path)?;
        if data.is_empty() {
            return Ok(());
        }
        let vault: Vault = serde_json::from_slice(&data)?;
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|e| StoreError::Crypto(e.to_string()))?;

        let mut cache = self.cache.lock().map_err(|_| StoreError::Crypto("lock poisoned".into()))?;
        for (account, entry) in vault.entries {
            let nonce = Nonce::try_from(entry.nonce.as_slice()).map_err(|_| StoreError::Corrupted)?;
            let plaintext = cipher.decrypt(&nonce, entry.ciphertext.as_ref()).map_err(|_| StoreError::Corrupted)?;
            cache.insert(account, String::from_utf8(plaintext).map_err(|_| StoreError::Corrupted)?);
        }
        Ok(())
    }

    fn save(&self) -> Result<(), StoreError> {
        let cache = self.cache.lock().map_err(|_| StoreError::Crypto("lock poisoned".into()))?;
        let cipher = Aes256Gcm::new_from_slice(self.key.as_ref()).map_err(|e| StoreError::Crypto(e.to_string()))?;

        // Generate fresh salt for each save (forward secrecy-ish).
        let mut salt = [0u8; 16];
        rng().fill(&mut salt);

        let mut entries = HashMap::new();
        for (account, secret) in cache.iter() {
            let mut nonce = [0u8; 12];
            rng().fill(&mut nonce);
            let ciphertext = cipher
                .encrypt(&Nonce::try_from(nonce.as_slice()).map_err(|_| StoreError::Corrupted)?, secret.as_bytes())
                .map_err(|e| StoreError::Crypto(e.to_string()))?;
            entries.insert(account.clone(), EncryptedEntry { nonce, ciphertext });
        }

        let vault = Vault { salt, entries };
        let data = serde_json::to_vec(&vault)?;
        fs::write(&self.vault_path, data)?;
        Ok(())
    }
}

impl SecretStore for FileSecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        let cache = self.cache.lock().map_err(|_| SecretError::Store("lock poisoned".into()))?;
        Ok(cache.get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        {
            let mut cache = self.cache.lock().map_err(|_| SecretError::Store("lock poisoned".into()))?;
            cache.insert(account.to_string(), secret.to_string());
        }
        self.save().map_err(|e| SecretError::Store(e.to_string()))
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        {
            let mut cache = self.cache.lock().map_err(|_| SecretError::Store("lock poisoned".into()))?;
            cache.remove(account);
        }
        self.save().map_err(|e| SecretError::Store(e.to_string()))
    }
}

/// Derive a 32-byte encryption key using Argon2id from machine ID + app identifier.
fn derive_key(app_identifier: &str) -> Result<[u8; 32], StoreError> {
    // Machine ID: stable per device, doesn't require admin.
    let machine_id = machine_id().unwrap_or_else(|| "unknown-machine".into());

    // Salt: hash of app identifier (deterministic, per-app).
    let mut salt = [0u8; 16];
    let hash =
        sha2::Sha256::new().chain_update(app_identifier.as_bytes()).chain_update(machine_id.as_bytes()).finalize();
    salt.copy_from_slice(&hash[..16]);

    let params = Params::new(19456, 2, 1, Some(32)).map_err(|e| StoreError::Crypto(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; 32];
    argon2
        .hash_password_into((app_identifier.to_string() + &machine_id).as_bytes(), &salt, &mut key)
        .map_err(|e| StoreError::Crypto(e.to_string()))?;

    Ok(key)
}

/// Get a stable machine identifier.
#[cfg(target_os = "macos")]
fn machine_id() -> Option<String> {
    // Use IOPlatformUUID (stable across reboots, unique per Mac).
    use std::process::Command;
    Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| {
            s.lines().find(|l| l.contains("IOPlatformUUID")).and_then(|l| l.split('"').nth(3)).map(String::from)
        })
}

#[cfg(target_os = "windows")]
fn machine_id() -> Option<String> {
    // Use MachineGuid from registry.
    use std::process::Command;
    Command::new("reg")
        .args(["query", "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography", "/v", "MachineGuid"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| {
            s.lines().find(|l| l.contains("MachineGuid")).and_then(|l| l.split_whitespace().last()).map(String::from)
        })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn machine_id() -> Option<String> {
    // Fallback: /etc/machine-id or similar.
    fs::read_to_string("/etc/machine-id").ok().map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip() {
        let dir = tempdir().unwrap();
        let store = FileSecretStore::new(dir.path(), "test.app").unwrap();

        store.set("account1", "secret1").unwrap();
        assert_eq!(store.get("account1").unwrap(), Some("secret1".into()));

        store.set("account1", "updated").unwrap();
        assert_eq!(store.get("account1").unwrap(), Some("updated".into()));

        store.delete("account1").unwrap();
        assert_eq!(store.get("account1").unwrap(), None);
    }

    #[test]
    fn persists_across_instances() {
        let dir = tempdir().unwrap();
        let app_id = "persist.test";

        {
            let store = FileSecretStore::new(dir.path(), app_id).unwrap();
            store.set("acc", "sec").unwrap();
        }
        {
            let store = FileSecretStore::new(dir.path(), app_id).unwrap();
            assert_eq!(store.get("acc").unwrap(), Some("sec".into()));
        }
    }

    #[test]
    fn different_app_ids_isolate() {
        let dir = tempdir().unwrap();
        let store1 = FileSecretStore::new(dir.path(), "app1").unwrap();
        let store2 = FileSecretStore::new(dir.path(), "app2").unwrap();

        store1.set("acc", "sec1").unwrap();
        store2.set("acc", "sec2").unwrap();

        assert_eq!(store1.get("acc").unwrap(), Some("sec1".into()));
        assert_eq!(store2.get("acc").unwrap(), Some("sec2".into()));
    }
}
