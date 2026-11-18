//! Tests for the encrypted history file, with a fake Keychain.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use alfred_passwords::history::{Entry, History};
use alfred_passwords::vault::{EncryptedFile, HistoryVault, NoVault, Unreadable};
use common::FakeKeyStore;

const SECRET: &str = "bevfzw-yuzdmg-edx4Fe";

fn history() -> History {
    let mut history = History::default();
    history.push(Entry {
        id: "a1b2".to_owned(),
        password: SECRET.to_owned(),
        created_at: 1_800_000_000,
        style: "Apple format".to_owned(),
        saved_for: Some("me (github.com)".to_owned()),
    });

    history
}

#[test]
fn path_is_in_the_workflow_data_folder() {
    assert_eq!(
        EncryptedFile::path(Some(Path::new("/data"))),
        Some(Path::new("/data/history.bin").to_path_buf())
    );
    assert_eq!(EncryptedFile::path(Some(Path::new(""))), None);
    assert_eq!(EncryptedFile::path(None), None);
}

#[test]
fn save_then_load_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let keys = FakeKeyStore::new(7);
    let vault = EncryptedFile::new(dir.path().join("nested/history.bin"), &keys);

    vault.save(&history()).unwrap();

    assert_eq!(vault.load().unwrap(), history());
}

#[test]
fn the_file_is_encrypted_and_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.bin");
    let keys = FakeKeyStore::new(7);

    EncryptedFile::new(path.clone(), &keys)
        .save(&history())
        .unwrap();

    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"APH1"));
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains(SECRET));
    assert!(!text.contains("github.com"));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!path.with_extension("bin.tmp").exists());
}

#[test]
fn each_save_uses_a_fresh_nonce() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.bin");
    let keys = FakeKeyStore::new(7);
    let vault = EncryptedFile::new(path.clone(), &keys);

    vault.save(&history()).unwrap();
    let first = fs::read(&path).unwrap();
    vault.save(&history()).unwrap();
    let second = fs::read(&path).unwrap();

    assert_ne!(first, second);
}

#[test]
fn a_missing_file_is_an_empty_history_without_asking_for_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let keys = FakeKeyStore::new(7);
    let vault = EncryptedFile::new(dir.path().join("history.bin"), &keys);

    assert_eq!(vault.load().unwrap(), History::default());
    assert_eq!(keys.reads.get(), 0);
}

#[test]
fn another_key_cannot_read_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.bin");
    let writer = FakeKeyStore::new(7);
    let reader = FakeKeyStore::new(8);

    EncryptedFile::new(path.clone(), &writer)
        .save(&history())
        .unwrap();
    let err = EncryptedFile::new(path, &reader).load().unwrap_err();

    assert!(err.downcast_ref::<Unreadable>().is_some());
}

#[test]
fn a_tampered_or_foreign_file_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.bin");
    let keys = FakeKeyStore::new(7);
    let vault = EncryptedFile::new(path.clone(), &keys);

    vault.save(&history()).unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&path, &bytes).unwrap();
    assert!(vault
        .load()
        .unwrap_err()
        .downcast_ref::<Unreadable>()
        .is_some());

    fs::write(&path, b"{\"entries\":[]}").unwrap();
    assert!(vault
        .load()
        .unwrap_err()
        .downcast_ref::<Unreadable>()
        .is_some());
}

#[test]
fn a_denied_keychain_is_an_error_not_an_unreadable_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.bin");
    let keys = FakeKeyStore::new(7);
    EncryptedFile::new(path.clone(), &keys)
        .save(&history())
        .unwrap();

    let denied = FakeKeyStore::denied();
    let err = EncryptedFile::new(path, &denied).load().unwrap_err();

    assert!(err.downcast_ref::<Unreadable>().is_none());
    assert_eq!(err.to_string(), "the user denied Keychain access");
}

#[test]
fn no_vault_keeps_nothing() {
    assert_eq!(NoVault.load().unwrap(), History::default());
    assert!(NoVault.save(&history()).is_err());
}
