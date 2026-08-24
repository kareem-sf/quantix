#![cfg(windows)]

use std::{
    fs::OpenOptions,
    io,
    os::windows::fs::OpenOptionsExt,
    path::Path,
    sync::{Arc, Barrier},
};

use quantix_lib::{
    ai::vault::{AiConnectionVault, VaultError, VaultLoadState},
    ai::windows_dpapi::protect_for_current_user,
    ensure_quantix_setup, QuantixHost, SetupPlatform, SetupState, StoragePermissions,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use zeroize::Zeroizing;

use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

struct ReadySetupPlatform;

impl SetupPlatform for ReadySetupPlatform {
    fn available_space(&self, _path: &Path) -> io::Result<u64> {
        Ok(MINIMUM_SETUP_FREE_SPACE_BYTES)
    }

    fn is_writable(&self, _path: &Path) -> io::Result<bool> {
        Ok(true)
    }

    fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
        Ok(StoragePermissions::Restrictive)
    }
}

struct TestApplicationHome {
    _root: tempfile::TempDir,
    path: std::path::PathBuf,
}

fn initialized_private_home(name: &str) -> TestApplicationHome {
    let root = tempfile::Builder::new()
        .prefix(name)
        .tempdir()
        .expect("temporary application-home parent");
    let path = root.path().join(".quantix");
    let host = QuantixHost::with_setup_platform(&path, Arc::new(ReadySetupPlatform));
    let outcome = ensure_quantix_setup(&host);
    assert_eq!(outcome.state, SetupState::Ready);
    TestApplicationHome { _root: root, path }
}

#[derive(Clone, Copy)]
enum VaultFault {
    CorruptCiphertext,
    TruncatedCiphertext,
    WrongVersion,
    ReparsePoint,
    HardLink,
}

fn home_with_fault(fault: VaultFault) -> TestApplicationHome {
    let name = match fault {
        VaultFault::CorruptCiphertext => "vault-corrupt",
        VaultFault::TruncatedCiphertext => "vault-truncated",
        VaultFault::WrongVersion => "vault-version",
        VaultFault::ReparsePoint => "vault-reparse",
        VaultFault::HardLink => "vault-hardlink",
    };
    let home = initialized_private_home(name);
    let path = home.path.join("ai-connections.vault");
    match fault {
        VaultFault::CorruptCiphertext => std::fs::write(path, [0xa5; 64]).unwrap(),
        VaultFault::TruncatedCiphertext => std::fs::write(path, [0x5a; 8]).unwrap(),
        VaultFault::WrongVersion => {
            let clear = serde_json::to_vec(&serde_json::json!({
                "schema_version": 2,
                "mutation_revision": 0,
                "connections": {},
                "future_version_field": { "future_shape": true },
            }))
            .unwrap();
            let ciphertext = protect_for_current_user(Zeroizing::new(clear)).unwrap();
            std::fs::write(path, ciphertext).unwrap();
        }
        VaultFault::ReparsePoint => {
            let target = home._root.path().join("reparse-target");
            std::fs::create_dir(&target).unwrap();
            junction::create(target, path).unwrap();
        }
        VaultFault::HardLink => {
            let vault = AiConnectionVault::new(&home.path).unwrap();
            let secret = Zeroizing::new("hard-link-sentinel".to_owned());
            vault
                .fixture_insert(0, "00000000000000000000000000000003", &secret)
                .unwrap();
            std::fs::hard_link(&path, home.path.join("vault-hardlink-alias")).unwrap();
        }
    }
    home
}

#[test]
fn encrypted_vault_round_trips_and_rejects_stale_cas() {
    let home = initialized_private_home("vault-cas");
    let vault = AiConnectionVault::new(&home.path).unwrap();
    assert!(matches!(vault.load().unwrap(), VaultLoadState::Missing));

    let secret_a = Zeroizing::new("secret-A".to_owned());
    let first = vault
        .fixture_insert(0, "00000000000000000000000000000001", &secret_a)
        .unwrap();
    let bytes = std::fs::read(home.path.join("ai-connections.vault")).unwrap();
    assert!(!bytes.windows(8).any(|part| part == b"secret-A"));

    let secret_b = Zeroizing::new("secret-B".to_owned());
    assert!(matches!(
        vault.fixture_insert(0, "00000000000000000000000000000002", &secret_b),
        Err(VaultError::RevisionConflict)
    ));
    assert_eq!(
        vault.load().unwrap().ready().unwrap().mutation_revision,
        first.mutation_revision
    );
}

#[test]
fn vault_never_treats_invalid_storage_as_empty() {
    for fault in [
        VaultFault::CorruptCiphertext,
        VaultFault::TruncatedCiphertext,
    ] {
        let home = home_with_fault(fault);
        assert!(matches!(
            AiConnectionVault::new(&home.path).unwrap().load(),
            Ok(VaultLoadState::Corrupt)
        ));
    }

    let home = home_with_fault(VaultFault::WrongVersion);
    assert!(matches!(
        AiConnectionVault::new(&home.path).unwrap().load(),
        Ok(VaultLoadState::Unsupported)
    ));

    for fault in [VaultFault::ReparsePoint, VaultFault::HardLink] {
        let home = home_with_fault(fault);
        assert!(matches!(
            AiConnectionVault::new(&home.path).unwrap().load(),
            Err(VaultError::Unavailable)
        ));
    }
}

#[test]
fn concurrent_current_mutations_are_contiguous_and_lossless() {
    const MUTATIONS_PER_THREAD: u64 = 8;

    let home = initialized_private_home("vault-contention");
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for worker in 0..2u64 {
        let path = home.path.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            let vault = AiConnectionVault::new(&path).unwrap();
            let secret = Zeroizing::new(format!("contention-sentinel-{worker}"));
            barrier.wait();
            (0..MUTATIONS_PER_THREAD)
                .map(|offset| {
                    let connection_id = format!("{:032x}", 0x100 + worker * 0x10 + offset);
                    vault
                        .fixture_insert_current(&connection_id, &secret)
                        .unwrap()
                        .mutation_revision
                })
                .collect::<Vec<_>>()
        }));
    }
    barrier.wait();

    let mut revisions: Vec<_> = workers
        .into_iter()
        .flat_map(|worker| worker.join().unwrap())
        .collect();
    revisions.sort_unstable();
    assert_eq!(
        revisions,
        (1..=2 * MUTATIONS_PER_THREAD).collect::<Vec<_>>()
    );

    let snapshot = AiConnectionVault::new(&home.path)
        .unwrap()
        .load()
        .unwrap()
        .ready()
        .unwrap()
        .clone();
    let mut expected_ids: Vec<_> = (0..2u64)
        .flat_map(|worker| {
            (0..MUTATIONS_PER_THREAD)
                .map(move |offset| format!("{:032x}", 0x100 + worker * 0x10 + offset))
        })
        .collect();
    expected_ids.sort();
    assert_eq!(snapshot.connection_ids, expected_ids);
    assert_eq!(snapshot.mutation_revision, 2 * MUTATIONS_PER_THREAD);
}

#[test]
fn ambiguous_publication_error_reconciles_the_committed_revision() {
    let home = initialized_private_home("vault-ambiguous-publish");
    let vault = AiConnectionVault::new(&home.path).unwrap();
    let first_secret = Zeroizing::new("first-publication-sentinel".to_owned());
    vault
        .fixture_insert(0, "00000000000000000000000000000020", &first_secret)
        .unwrap();

    vault.fixture_fail_after_publish_once();
    let second_secret = Zeroizing::new("second-publication-sentinel".to_owned());
    let snapshot = vault
        .fixture_insert_current("00000000000000000000000000000021", &second_secret)
        .unwrap();

    assert_eq!(snapshot.mutation_revision, 2);
    assert_eq!(snapshot.connection_ids.len(), 2);
    assert_no_vault_staging_files(&home.path);
}

#[test]
fn prepublication_failure_preserves_the_previous_ciphertext() {
    let home = initialized_private_home("vault-prepublish-failure");
    let vault = AiConnectionVault::new(&home.path).unwrap();
    let first_secret = Zeroizing::new("preserved-publication-sentinel".to_owned());
    vault
        .fixture_insert(0, "00000000000000000000000000000030", &first_secret)
        .unwrap();
    let before = std::fs::read(home.path.join("ai-connections.vault")).unwrap();

    vault.fixture_fail_before_publish_once();
    let rejected_secret = Zeroizing::new("rejected-publication-sentinel".to_owned());
    assert!(matches!(
        vault.fixture_insert_current("00000000000000000000000000000031", &rejected_secret),
        Err(VaultError::Unavailable)
    ));

    assert_eq!(
        std::fs::read(home.path.join("ai-connections.vault")).unwrap(),
        before
    );
    assert_eq!(vault.load().unwrap().ready().unwrap().mutation_revision, 1);
    assert_no_vault_staging_files(&home.path);
}

#[test]
fn vault_enforces_cleartext_and_ciphertext_byte_bounds() {
    const MAX_CLEAR_BYTES: usize = 4 * 1024 * 1024;
    const MAX_CIPHERTEXT_BYTES: usize = 8 * 1024 * 1024;

    let clear_home = initialized_private_home("vault-clear-bound");
    let clear = Zeroizing::new(vec![b' '; MAX_CLEAR_BYTES + 1]);
    let ciphertext = protect_for_current_user(clear).unwrap();
    assert!(ciphertext.len() < MAX_CIPHERTEXT_BYTES);
    std::fs::write(clear_home.path.join("ai-connections.vault"), ciphertext).unwrap();
    assert!(matches!(
        AiConnectionVault::new(&clear_home.path).unwrap().load(),
        Ok(VaultLoadState::Corrupt)
    ));

    let cipher_home = initialized_private_home("vault-cipher-bound");
    let oversized = vec![0xa5; MAX_CIPHERTEXT_BYTES + 1];
    std::fs::write(cipher_home.path.join("ai-connections.vault"), oversized).unwrap();
    assert!(matches!(
        AiConnectionVault::new(&cipher_home.path).unwrap().load(),
        Ok(VaultLoadState::Corrupt)
    ));
}

#[test]
fn vault_rejects_unexpected_paths_and_lock_objects() {
    let directory_home = initialized_private_home("vault-directory");
    std::fs::create_dir(directory_home.path.join("ai-connections.vault")).unwrap();
    assert!(matches!(
        AiConnectionVault::new(&directory_home.path).unwrap().load(),
        Err(VaultError::Unavailable)
    ));

    let ads_home = initialized_private_home("vault-ads-home");
    let ads_path = std::path::PathBuf::from(format!("{}:alternate", ads_home.path.display()));
    assert!(matches!(
        AiConnectionVault::new(&ads_path),
        Err(VaultError::Unavailable)
    ));

    let reparse_home = initialized_private_home("vault-reparse-home");
    let linked_home = reparse_home._root.path().join("linked-home");
    junction::create(&reparse_home.path, &linked_home).unwrap();
    assert!(matches!(
        AiConnectionVault::new(&linked_home),
        Err(VaultError::Unavailable)
    ));

    let lock_reparse_home = initialized_private_home("vault-lock-reparse");
    let lock_target = lock_reparse_home._root.path().join("lock-target");
    std::fs::create_dir(&lock_target).unwrap();
    junction::create(
        lock_target,
        lock_reparse_home.path.join("ai-connections.vault.lock"),
    )
    .unwrap();
    assert!(matches!(
        AiConnectionVault::new(&lock_reparse_home.path)
            .unwrap()
            .load(),
        Err(VaultError::Unavailable)
    ));

    let lock_link_home = initialized_private_home("vault-lock-hardlink");
    let vault = AiConnectionVault::new(&lock_link_home.path).unwrap();
    assert!(matches!(vault.load().unwrap(), VaultLoadState::Missing));
    let lock_path = lock_link_home.path.join("ai-connections.vault.lock");
    std::fs::hard_link(&lock_path, lock_link_home.path.join("lock-hardlink-alias")).unwrap();
    assert!(matches!(vault.load(), Err(VaultError::Unavailable)));
}

#[test]
fn orphan_staging_is_never_promoted_and_the_lock_remains_persistent() {
    let home = initialized_private_home("vault-orphan-stage");
    let orphan_clear = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "mutation_revision": 77,
        "connections": {},
    }))
    .unwrap();
    let orphan_ciphertext = protect_for_current_user(Zeroizing::new(orphan_clear)).unwrap();
    let orphan = home
        .path
        .join(".ai-connections.vault.00000000000000000000000000000000.tmp");
    std::fs::write(&orphan, orphan_ciphertext).unwrap();

    let vault = AiConnectionVault::new(&home.path).unwrap();
    assert!(matches!(vault.load().unwrap(), VaultLoadState::Missing));
    let lock = home.path.join("ai-connections.vault.lock");
    assert!(lock.is_file());

    let secret = Zeroizing::new("orphan-stage-sentinel".to_owned());
    let snapshot = vault
        .fixture_insert(0, "00000000000000000000000000000040", &secret)
        .unwrap();
    assert_eq!(snapshot.mutation_revision, 1);
    assert!(orphan.is_file());
    assert!(lock.is_file());
}

#[test]
fn revision_overflow_and_malformed_payloads_fail_closed() {
    let overflow_home = initialized_private_home("vault-revision-overflow");
    let clear = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "mutation_revision": u64::MAX,
        "connections": {},
    }))
    .unwrap();
    let before = protect_for_current_user(Zeroizing::new(clear)).unwrap();
    std::fs::write(overflow_home.path.join("ai-connections.vault"), &before).unwrap();
    let overflow_vault = AiConnectionVault::new(&overflow_home.path).unwrap();
    let secret = Zeroizing::new("overflow-sentinel".to_owned());
    assert!(matches!(
        overflow_vault.fixture_insert_current("00000000000000000000000000000050", &secret),
        Err(VaultError::Invalid)
    ));
    assert_eq!(
        std::fs::read(overflow_home.path.join("ai-connections.vault")).unwrap(),
        before
    );

    let malformed_home = initialized_private_home("vault-unknown-field");
    let malformed = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "mutation_revision": 0,
        "connections": {},
        "unexpected": true,
    }))
    .unwrap();
    let malformed = protect_for_current_user(Zeroizing::new(malformed)).unwrap();
    std::fs::write(malformed_home.path.join("ai-connections.vault"), malformed).unwrap();
    assert!(matches!(
        AiConnectionVault::new(&malformed_home.path).unwrap().load(),
        Ok(VaultLoadState::Corrupt)
    ));
}

#[test]
fn replace_sharing_failure_preserves_the_previous_revision() {
    let home = initialized_private_home("vault-replace-sharing");
    let vault = AiConnectionVault::new(&home.path).unwrap();
    let first_secret = Zeroizing::new("sharing-first-sentinel".to_owned());
    vault
        .fixture_insert(0, "00000000000000000000000000000060", &first_secret)
        .unwrap();
    let target = home.path.join("ai-connections.vault");
    let before = std::fs::read(&target).unwrap();
    let held = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(&target)
        .unwrap();

    let second_secret = Zeroizing::new("sharing-second-sentinel".to_owned());
    assert!(matches!(
        vault.fixture_insert_current("00000000000000000000000000000061", &second_secret),
        Err(VaultError::Unavailable)
    ));
    drop(held);

    assert_eq!(std::fs::read(target).unwrap(), before);
    assert_eq!(vault.load().unwrap().ready().unwrap().mutation_revision, 1);
    assert_no_vault_staging_files(&home.path);
}

#[test]
fn public_snapshots_and_errors_are_redacted() {
    let home = initialized_private_home("vault-redaction");
    let vault = AiConnectionVault::new(&home.path).unwrap();
    let sentinel = Zeroizing::new("never-project-this-sentinel".to_owned());
    vault
        .fixture_insert(0, "00000000000000000000000000000070", &sentinel)
        .unwrap();
    let error = vault
        .fixture_insert(0, "00000000000000000000000000000071", &sentinel)
        .unwrap_err();

    for rendered in [
        format!("{error:?}"),
        error.to_string(),
        format!("{:?}", vault.load().unwrap()),
    ] {
        assert!(!rendered.contains(sentinel.as_str()));
        assert!(!rendered.contains(&home.path.to_string_lossy().to_string()));
    }
}

fn assert_no_vault_staging_files(application_home: &Path) {
    let staging_count = std::fs::read_dir(application_home)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(".ai-connections.vault.") && name.ends_with(".tmp")
        })
        .count();
    assert_eq!(staging_count, 0);
}
