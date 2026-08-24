#![cfg(windows)]

use std::{
    fs::OpenOptions,
    io,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

use quantix_lib::{
    ai::vault::{AiConnectionVault, VaultError, VaultLoadState, VaultSnapshot},
    ai::windows_dpapi::protect_for_current_user,
    ensure_quantix_setup, QuantixHost, SetupPlatform, SetupState, StoragePermissions,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
};
use zeroize::Zeroizing;

use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

const VAULT_HELPER_MODE_ENV: &str = "QUANTIX_TEST_VAULT_HELPER_MODE";
const VAULT_HELPER_HOME_ENV: &str = "QUANTIX_TEST_VAULT_HELPER_HOME";
const VAULT_HELPER_WORKER_ENV: &str = "QUANTIX_TEST_VAULT_HELPER_WORKER";

struct HelperChild {
    child: Option<Child>,
    reaped: Arc<std::sync::atomic::AtomicBool>,
}

impl HelperChild {
    fn new(child: Child) -> Self {
        Self {
            child: Some(child),
            reaped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    fn reap_observer(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.reaped)
    }

    fn wait_until(&mut self, deadline: Instant) -> io::Result<std::process::ExitStatus> {
        loop {
            let status = self
                .child
                .as_mut()
                .ok_or_else(|| io::Error::other("vault helper was already reaped"))?
                .try_wait()?;
            if let Some(status) = status {
                self.child.take();
                self.reaped
                    .store(true, std::sync::atomic::Ordering::Release);
                return Ok(status);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "vault helper completion timed out",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for HelperChild {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if matches!(child.try_wait(), Ok(Some(_))) {
            self.reaped
                .store(true, std::sync::atomic::Ordering::Release);
            return;
        }
        let _ = child.kill();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if matches!(child.try_wait(), Ok(Some(_))) {
                self.reaped
                    .store(true, std::sync::atomic::Ordering::Release);
                return;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                if child.wait().is_ok() {
                    self.reaped
                        .store(true, std::sync::atomic::Ordering::Release);
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

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
fn separate_processes_commit_contiguous_lossless_mutations() {
    let home = initialized_private_home("vault-cross-process-contention");
    let release = home.path.join("vault-helper-release");
    let mut children = [
        spawn_vault_helper(&home.path, 0),
        spawn_vault_helper(&home.path, 1),
    ];
    wait_for_helper_files(&[
        home.path.join("vault-helper-ready-0"),
        home.path.join("vault-helper-ready-1"),
    ]);
    std::fs::write(&release, b"release").unwrap();

    let completion_deadline = Instant::now() + Duration::from_secs(10);
    for child in &mut children {
        assert!(child.wait_until(completion_deadline).unwrap().success());
    }
    let mut revisions = [
        read_helper_revision(&home.path.join("vault-helper-result-0")),
        read_helper_revision(&home.path.join("vault-helper-result-1")),
    ];
    revisions.sort_unstable();
    assert_eq!(revisions, [1, 2]);

    let snapshot = AiConnectionVault::new(&home.path)
        .unwrap()
        .load()
        .unwrap()
        .ready()
        .unwrap()
        .clone();
    assert_eq!(
        snapshot.connection_ids,
        vec![
            "000000000000000000000000000000a0".to_owned(),
            "000000000000000000000000000000a1".to_owned(),
        ]
    );
    assert_eq!(snapshot.mutation_revision, 2);
}

#[test]
fn helper_child_guard_bounds_wait_and_terminates_an_unreleased_helper() {
    let home = initialized_private_home("vault-helper-guard");
    let mut child = spawn_vault_helper(&home.path, 0);
    wait_for_helper_files(&[home.path.join("vault-helper-ready-0")]);

    let error = child
        .wait_until(Instant::now() + Duration::from_millis(20))
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    let reaped = child.reap_observer();
    drop(child);
    assert!(reaped.load(std::sync::atomic::Ordering::Acquire));
}

#[test]
fn vault_helper_process_entrypoint() {
    if let Some(exit_code) = run_vault_helper_if_requested() {
        std::process::exit(exit_code);
    }
}

fn spawn_vault_helper(application_home: &Path, worker: u8) -> HelperChild {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "vault_helper_process_entrypoint",
            "--test-threads=1",
        ])
        .env(VAULT_HELPER_MODE_ENV, "1")
        .env(VAULT_HELPER_HOME_ENV, application_home)
        .env(VAULT_HELPER_WORKER_ENV, worker.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    HelperChild::new(child)
}

fn run_vault_helper_if_requested() -> Option<i32> {
    std::env::var_os(VAULT_HELPER_MODE_ENV)?;
    Some(if run_vault_helper().is_ok() { 0 } else { 70 })
}

fn run_vault_helper() -> Result<(), ()> {
    let application_home = std::env::var_os(VAULT_HELPER_HOME_ENV)
        .map(PathBuf::from)
        .ok_or(())?;
    let worker: u8 = std::env::var(VAULT_HELPER_WORKER_ENV)
        .map_err(|_| ())?
        .parse()
        .map_err(|_| ())?;
    if worker > 1 {
        return Err(());
    }

    let ready = application_home.join(format!("vault-helper-ready-{worker}"));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(ready)
        .and_then(|file| file.sync_all())
        .map_err(|_| ())?;
    wait_for_helper_files(&[application_home.join("vault-helper-release")]);

    let vault = AiConnectionVault::new(&application_home).map_err(|_| ())?;
    let connection_id = format!("{:032x}", 0xa0u64 + u64::from(worker));
    let secret = Zeroizing::new(format!("cross-process-helper-{worker}"));
    let revision = vault
        .fixture_insert_current(&connection_id, &secret)
        .map_err(|_| ())?
        .mutation_revision;
    let result = application_home.join(format!("vault-helper-result-{worker}"));
    let mut result_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(result)
        .map_err(|_| ())?;
    use std::io::Write as _;
    write!(result_file, "{revision}").map_err(|_| ())?;
    result_file.sync_all().map_err(|_| ())?;
    Ok(())
}

fn wait_for_helper_files(paths: &[PathBuf]) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !paths.iter().all(|path| {
        std::fs::symlink_metadata(path)
            .map(|metadata| metadata.is_file())
            .unwrap_or(false)
    }) {
        assert!(Instant::now() < deadline, "vault helper barrier timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn read_helper_revision(path: &Path) -> u64 {
    std::fs::read_to_string(path).unwrap().parse().unwrap()
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
fn cleartext_writer_remains_an_exact_four_mibibyte_serialization_backstop() {
    AiConnectionVault::fixture_verify_cleartext_writer_backstop().unwrap();
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
fn target_and_persistent_lock_reject_named_alternate_streams() {
    let target_home = initialized_private_home("vault-target-ads");
    let target_vault = AiConnectionVault::new(&target_home.path).unwrap();
    let target_secret = Zeroizing::new("target-ads-sentinel".to_owned());
    target_vault
        .fixture_insert(0, "00000000000000000000000000000080", &target_secret)
        .unwrap();
    write_named_stream(
        &target_home.path.join("ai-connections.vault"),
        "named",
        b"named-stream-marker",
    );
    assert!(matches!(target_vault.load(), Err(VaultError::Unavailable)));

    let lock_home = initialized_private_home("vault-lock-ads");
    let lock_vault = AiConnectionVault::new(&lock_home.path).unwrap();
    assert!(matches!(
        lock_vault.load().unwrap(),
        VaultLoadState::Missing
    ));
    write_named_stream(
        &lock_home.path.join("ai-connections.vault.lock"),
        "named",
        b"named-stream-marker",
    );
    assert!(matches!(lock_vault.load(), Err(VaultError::Unavailable)));
}

#[test]
fn owned_stage_rejects_a_named_alternate_stream_before_publication() {
    let home = initialized_private_home("vault-stage-ads");
    let vault = AiConnectionVault::new(&home.path).unwrap();
    vault.fixture_add_staged_ads_before_publish_once();
    let secret = Zeroizing::new("stage-ads-sentinel".to_owned());

    assert!(matches!(
        vault.fixture_insert(0, "00000000000000000000000000000081", &secret),
        Err(VaultError::Unavailable)
    ));
    assert!(matches!(vault.load().unwrap(), VaultLoadState::Missing));
    assert_no_vault_staging_files(&home.path);
}

#[test]
fn owned_stage_cleanup_never_deletes_a_swapped_path_object() {
    let home = initialized_private_home("vault-stage-swap");
    let vault = AiConnectionVault::new(&home.path).unwrap();
    vault.fixture_swap_staged_path_before_failure_once();
    let secret = Zeroizing::new("stage-swap-sentinel".to_owned());

    assert!(matches!(
        vault.fixture_insert(0, "00000000000000000000000000000082", &secret),
        Err(VaultError::Unavailable)
    ));
    assert!(matches!(vault.load().unwrap(), VaultLoadState::Missing));

    let replacement_paths = vault_staging_paths(&home.path);
    assert_eq!(replacement_paths.len(), 1);
    assert_eq!(
        std::fs::read(&replacement_paths[0]).unwrap(),
        b"replacement-path-object"
    );
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
        Err(VaultError::RevisionOverflow)
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
fn mutation_callbacks_cannot_override_payload_invariants() {
    type VaultMutation = fn(&AiConnectionVault) -> Result<VaultSnapshot, VaultError>;

    let cases: [(&str, VaultMutation); 4] = [
        (
            "vault-mutation-revision",
            AiConnectionVault::fixture_override_revision_current,
        ),
        (
            "vault-mutation-key-mismatch",
            AiConnectionVault::fixture_insert_key_mismatch_current,
        ),
        (
            "vault-mutation-invalid-record",
            AiConnectionVault::fixture_insert_invalid_record_current,
        ),
        (
            "vault-mutation-schema",
            AiConnectionVault::fixture_override_schema_current,
        ),
    ];
    for (name, invalid_mutation) in cases {
        let home = initialized_private_home(name);
        let vault = AiConnectionVault::new(&home.path).unwrap();
        let secret = Zeroizing::new("mutation-boundary-sentinel".to_owned());
        vault
            .fixture_insert(0, "00000000000000000000000000000090", &secret)
            .unwrap();
        let path = home.path.join("ai-connections.vault");
        let before = std::fs::read(&path).unwrap();

        assert!(matches!(invalid_mutation(&vault), Err(VaultError::Invalid)));
        assert_eq!(std::fs::read(path).unwrap(), before);
        assert_eq!(vault.load().unwrap().ready().unwrap().mutation_revision, 1);
    }
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
    assert_eq!(vault_staging_paths(application_home).len(), 0);
}

fn vault_staging_paths(application_home: &Path) -> Vec<std::path::PathBuf> {
    std::fs::read_dir(application_home)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(".ai-connections.vault.") && name.ends_with(".tmp")
        })
        .map(|entry| entry.path())
        .collect()
}

fn write_named_stream(path: &Path, stream_name: &str, bytes: &[u8]) {
    let stream_path = std::path::PathBuf::from(format!("{}:{stream_name}", path.display()));
    std::fs::write(stream_path, bytes).unwrap();
}
