//! Encrypted storage of the generated-password history.
//!
//! The history is a JSON document encrypted with ChaCha20-Poly1305 in
//! `history.bin`, under Alfred's workflow data folder. The 256-bit key is
//! generated on first use and kept in the login Keychain, never on disk or in
//! the workflow configuration. A copy of the file alone (backup, sync) is
//! useless without the Keychain.
//!
//! File layout: `APH1` magic, 12-byte random nonce, ciphertext. The magic is
//! also the authenticated associated data, so a file from another format or
//! version fails to decrypt instead of being misread.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

use crate::history::History;

/// Where the encryption key comes from.
pub trait KeyStore {
    /// The key, created and stored on first use.
    fn key(&self) -> Result<[u8; 32]>;
}

/// Load and save the history.
pub trait HistoryVault {
    /// The stored history; empty when there is none yet.
    fn load(&self) -> Result<History>;

    fn save(&self, history: &History) -> Result<()>;
}

/// The history file exists but cannot be decrypted with the current key
/// (the Keychain item was deleted, or the file is corrupt). Its content is
/// lost either way, so callers may start a new history over it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable;

impl fmt::Display for Unreadable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the password history cannot be decrypted")
    }
}

impl std::error::Error for Unreadable {}

/// The key in the user's login Keychain, as a generic password.
pub struct KeychainKeyStore;

impl KeychainKeyStore {
    const SERVICE: &'static str = "io.github.pdegeeter.alfred-passwords";
    const ACCOUNT: &'static str = "history-key";

    /// `errSecItemNotFound`.
    const ITEM_NOT_FOUND: i32 = -25300;
}

impl KeyStore for KeychainKeyStore {
    fn key(&self) -> Result<[u8; 32]> {
        use security_framework::passwords::{get_generic_password, set_generic_password};

        match get_generic_password(KeychainKeyStore::SERVICE, KeychainKeyStore::ACCOUNT) {
            Ok(bytes) => bytes
                .try_into()
                .map_err(|_| anyhow!("the history key in the Keychain is malformed")),
            Err(err) if err.code() == KeychainKeyStore::ITEM_NOT_FOUND => {
                let mut key = [0u8; 32];
                getrandom::fill(&mut key)
                    .map_err(|err| anyhow!("the system random source failed: {err}"))?;

                set_generic_password(KeychainKeyStore::SERVICE, KeychainKeyStore::ACCOUNT, &key)
                    .map_err(|err| {
                        anyhow!("could not store the history key in the Keychain: {err}")
                    })?;

                Ok(key)
            }
            Err(err) => Err(anyhow!(
                "could not read the history key from the Keychain: {err}"
            )),
        }
    }
}

/// The history in an encrypted file.
pub struct EncryptedFile<'a> {
    path: PathBuf,
    keys: &'a dyn KeyStore,
}

impl<'a> EncryptedFile<'a> {
    const FILE_NAME: &'static str = "history.bin";
    const MAGIC: &'static [u8] = b"APH1";
    const NONCE_LEN: usize = 12;

    pub fn new(path: PathBuf, keys: &'a dyn KeyStore) -> Self {
        Self { path, keys }
    }

    /// Where the history lives, given Alfred's `alfred_workflow_data` folder.
    pub fn path(data_dir: Option<&Path>) -> Option<PathBuf> {
        data_dir
            .filter(|dir| !dir.as_os_str().is_empty())
            .map(|dir| dir.join(EncryptedFile::FILE_NAME))
    }

    fn cipher(&self) -> Result<ChaCha20Poly1305> {
        let key = self.keys.key()?;

        Ok(ChaCha20Poly1305::new(Key::from_slice(&key)))
    }
}

impl HistoryVault for EncryptedFile<'_> {
    fn load(&self) -> Result<History> {
        // No file yet: no need to touch the Keychain.
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(History::default());
            }
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", self.path.display()));
            }
        };

        let header = EncryptedFile::MAGIC.len() + EncryptedFile::NONCE_LEN;

        if bytes.len() < header || !bytes.starts_with(EncryptedFile::MAGIC) {
            return Err(anyhow!(Unreadable));
        }

        let nonce = Nonce::from_slice(&bytes[EncryptedFile::MAGIC.len()..header]);
        let plaintext = self
            .cipher()?
            .decrypt(
                nonce,
                Payload {
                    msg: &bytes[header..],
                    aad: EncryptedFile::MAGIC,
                },
            )
            .map_err(|_| anyhow!(Unreadable))?;

        serde_json::from_slice(&plaintext).map_err(|_| anyhow!(Unreadable))
    }

    fn save(&self, history: &History) -> Result<()> {
        let cipher = self.cipher()?;
        let plaintext = serde_json::to_vec(history).context("failed to encode the history")?;

        let mut nonce = [0u8; EncryptedFile::NONCE_LEN];
        getrandom::fill(&mut nonce)
            .map_err(|err| anyhow!("the system random source failed: {err}"))?;

        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: EncryptedFile::MAGIC,
                },
            )
            .map_err(|_| anyhow!("failed to encrypt the history"))?;

        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }

        let temporary = self.path.with_extension("bin.tmp");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temporary)
            .with_context(|| format!("failed to write {}", temporary.display()))?;

        file.write_all(EncryptedFile::MAGIC)?;
        file.write_all(&nonce)?;
        file.write_all(&ciphertext)?;
        file.sync_all()?;

        fs::rename(&temporary, &self.path)
            .with_context(|| format!("failed to replace {}", self.path.display()))?;

        Ok(())
    }
}

/// Used when Alfred gives no data folder: nothing is kept.
pub struct NoVault;

impl HistoryVault for NoVault {
    fn load(&self) -> Result<History> {
        Ok(History::default())
    }

    fn save(&self, _history: &History) -> Result<()> {
        bail!("no workflow data folder to keep the history in")
    }
}
