use std::{io, path::Path, sync::Arc};

use quantix_lib::{
    ensure_quantix_setup, DeviceProtection, QuantixHost, SetupIssue, SetupPlatform, SetupState,
    StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES,
};

#[derive(Clone)]
struct FakeSetupPlatform {
    available_space: u64,
    writable: bool,
    permissions: StoragePermissions,
    device_protection: DeviceProtection,
}

struct NoStorageProbePlatform;

impl SetupPlatform for NoStorageProbePlatform {
    fn available_space(&self, _path: &Path) -> io::Result<u64> {
        panic!("unsupported catalogues must be inspected before free-space probes")
    }

    fn is_writable(&self, _path: &Path) -> io::Result<bool> {
        panic!("unsupported catalogues must be inspected before write probes")
    }

    fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
        panic!("unsupported catalogues must be inspected before permission probes")
    }

    fn device_protection(&self, _path: &Path) -> DeviceProtection {
        panic!("unsupported catalogues must be inspected before protection probes")
    }
}

impl Default for FakeSetupPlatform {
    fn default() -> Self {
        Self {
            available_space: MINIMUM_SETUP_FREE_SPACE_BYTES,
            writable: true,
            permissions: StoragePermissions::Restrictive,
            device_protection: DeviceProtection::Protected,
        }
    }
}

impl SetupPlatform for FakeSetupPlatform {
    fn available_space(&self, _path: &Path) -> io::Result<u64> {
        Ok(self.available_space)
    }

    fn is_writable(&self, _path: &Path) -> io::Result<bool> {
        Ok(self.writable)
    }

    fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
        Ok(self.permissions)
    }

    fn device_protection(&self, _path: &Path) -> DeviceProtection {
        self.device_protection
    }
}

fn host(application_home: &Path, platform: FakeSetupPlatform) -> QuantixHost {
    QuantixHost::with_setup_platform(application_home, Arc::new(platform))
}

#[test]
fn clean_setup_creates_the_exact_application_home_once() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let host = host(&application_home, FakeSetupPlatform::default());

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::Ready);
    assert!(outcome.setup_performed);
    assert!(outcome.issues.is_empty());
    for directory in [
        "archives", "backups", "exports", "logs", "models", "runtimes", "staging", "tenders",
        "trash",
    ] {
        assert!(application_home.join(directory).is_dir(), "{directory}");
    }
    assert!(application_home.join("installation.sqlite").is_file());
    assert!(!application_home.join(".setup-in-progress").exists());
}

#[test]
fn repeated_setup_reuses_the_existing_installation_without_duplication() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let host = host(&application_home, FakeSetupPlatform::default());

    let first = ensure_quantix_setup(&host);
    let second = ensure_quantix_setup(&host);

    assert!(first.setup_performed);
    assert_eq!(second.state, SetupState::Ready);
    assert!(!second.setup_performed);
    assert!(second.issues.is_empty());
}

#[test]
fn interrupted_setup_resumes_and_discards_only_its_owned_staging_database() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    std::fs::create_dir(&application_home).expect("partial application home");
    std::fs::write(application_home.join(".setup-in-progress"), b"1\n").expect("setup marker");
    std::fs::create_dir(application_home.join("tenders")).expect("partial directory");
    std::fs::write(
        application_home.join("installation.sqlite.staging"),
        b"interrupted",
    )
    .expect("partial setup database");
    for companion in [
        "installation.sqlite.staging-journal",
        "installation.sqlite.staging-shm",
        "installation.sqlite.staging-wal",
    ] {
        std::fs::write(application_home.join(companion), b"interrupted")
            .expect("partial SQLite companion");
    }
    let host = host(&application_home, FakeSetupPlatform::default());

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::Ready);
    assert!(outcome.setup_performed);
    assert!(application_home.join("installation.sqlite").is_file());
    assert!(!application_home
        .join("installation.sqlite.staging")
        .exists());
    for companion in [
        "installation.sqlite.staging-journal",
        "installation.sqlite.staging-shm",
        "installation.sqlite.staging-wal",
    ] {
        assert!(!application_home.join(companion).exists(), "{companion}");
    }
    assert!(!application_home.join(".setup-in-progress").exists());
}

#[test]
fn unsafe_storage_permissions_block_setup() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    std::fs::create_dir(&application_home).expect("application home");
    let platform = FakeSetupPlatform {
        permissions: StoragePermissions::Unsafe,
        ..FakeSetupPlatform::default()
    };
    let host = host(&application_home, platform);

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::RepairRequired);
    assert_eq!(outcome.issues, vec![SetupIssue::UnsafeStoragePermissions]);
    assert!(!application_home.join("installation.sqlite").exists());
}

#[test]
fn unsafe_existing_installation_is_not_modified() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let ready_host = host(&application_home, FakeSetupPlatform::default());
    assert!(ensure_quantix_setup(&ready_host).setup_performed);
    let catalogue_path = application_home.join("installation.sqlite");
    let catalogue_before = std::fs::read(&catalogue_path).expect("installation catalogue");
    let unsafe_host = host(
        &application_home,
        FakeSetupPlatform {
            permissions: StoragePermissions::Unsafe,
            ..FakeSetupPlatform::default()
        },
    );

    let outcome = ensure_quantix_setup(&unsafe_host);

    assert_eq!(outcome.state, SetupState::RepairRequired);
    assert_eq!(outcome.issues, vec![SetupIssue::UnsafeStoragePermissions]);
    assert_eq!(
        std::fs::read(catalogue_path).expect("installation catalogue"),
        catalogue_before
    );
}

#[test]
fn insufficient_space_blocks_setup_before_creating_the_application_home() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let platform = FakeSetupPlatform {
        available_space: MINIMUM_SETUP_FREE_SPACE_BYTES - 1,
        ..FakeSetupPlatform::default()
    };
    let host = host(&application_home, platform);

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::MissingCapability);
    assert_eq!(outcome.issues, vec![SetupIssue::InsufficientFreeSpace]);
    assert!(!application_home.exists());
}

#[test]
fn unrecognized_existing_application_home_requires_repair_without_overwriting_data() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    std::fs::create_dir(&application_home).expect("legacy application home");
    let legacy_data = application_home.join("events.db");
    std::fs::write(&legacy_data, b"preserve me").expect("legacy data");
    let host = host(&application_home, FakeSetupPlatform::default());

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::RepairRequired);
    assert_eq!(
        outcome.issues,
        vec![SetupIssue::UnrecognizedApplicationHome]
    );
    assert_eq!(
        std::fs::read(legacy_data).expect("legacy data"),
        b"preserve me"
    );
    assert!(!application_home.join("installation.sqlite").exists());
}

#[cfg(any(unix, windows))]
#[test]
fn linked_application_home_is_rejected_without_touching_its_target() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let linked_target = parent.path().join("linked-target");
    let application_home = parent.path().join(".quantix");
    std::fs::create_dir(&linked_target).expect("linked target");
    let canary = linked_target.join("preserve-me");
    std::fs::write(&canary, b"preserve me").expect("linked target canary");
    create_directory_link(&linked_target, &application_home);
    let host = host(&application_home, FakeSetupPlatform::default());

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::RepairRequired);
    assert_eq!(outcome.issues, vec![SetupIssue::UnsafeStorageLocation]);
    assert_eq!(
        std::fs::read(canary).expect("linked target canary"),
        b"preserve me"
    );
    assert!(!linked_target.join("installation.sqlite").exists());
    remove_directory_link(&application_home);
}

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("application home symlink");
}

#[cfg(unix)]
fn remove_directory_link(link: &Path) {
    std::fs::remove_file(link).expect("remove application home symlink");
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) {
    junction::create(target, link).expect("application home junction");
}

#[cfg(windows)]
fn remove_directory_link(link: &Path) {
    junction::delete(link).expect("remove application home junction");
}

#[test]
fn unavailable_device_protection_is_an_attributable_warning() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let platform = FakeSetupPlatform {
        device_protection: DeviceProtection::Unverified,
        ..FakeSetupPlatform::default()
    };
    let host = host(&application_home, platform);

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::Warning);
    assert_eq!(outcome.issues, vec![SetupIssue::DeviceProtectionUnverified]);
}

#[test]
fn newer_installation_catalogue_requires_a_supported_quantix_version() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let host = host(&application_home, FakeSetupPlatform::default());
    assert!(ensure_quantix_setup(&host).setup_performed);

    let catalogue = rusqlite::Connection::open(application_home.join("installation.sqlite"))
        .expect("installation catalogue");
    catalogue
        .execute_batch("PRAGMA user_version = 3;")
        .expect("newer schema marker");
    drop(catalogue);
    let inspection_host =
        QuantixHost::with_setup_platform(&application_home, Arc::new(NoStorageProbePlatform));

    let outcome = ensure_quantix_setup(&inspection_host);

    assert_eq!(outcome.state, SetupState::UnsupportedVersion);
    assert_eq!(
        outcome.issues,
        vec![SetupIssue::UnsupportedInstallationVersion]
    );
}

#[test]
fn corrupt_installation_catalogue_requires_repair() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let host = host(&application_home, FakeSetupPlatform::default());
    assert!(ensure_quantix_setup(&host).setup_performed);
    std::fs::write(
        application_home.join("installation.sqlite"),
        b"not a SQLite catalogue",
    )
    .expect("corrupt installation catalogue");

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::RepairRequired);
    assert_eq!(
        outcome.issues,
        vec![SetupIssue::InstallationCatalogueCorrupt]
    );
}

#[test]
fn empty_sqlite_catalogue_requires_repair_instead_of_an_update() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let host = host(&application_home, FakeSetupPlatform::default());
    assert!(ensure_quantix_setup(&host).setup_performed);
    std::fs::remove_file(application_home.join("installation.sqlite"))
        .expect("remove installation catalogue");
    drop(
        rusqlite::Connection::open(application_home.join("installation.sqlite"))
            .expect("empty SQLite catalogue"),
    );

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::RepairRequired);
    assert_eq!(
        outcome.issues,
        vec![SetupIssue::InstallationCatalogueCorrupt]
    );
}

#[test]
fn altered_installation_schema_requires_repair() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let host = host(&application_home, FakeSetupPlatform::default());
    assert!(ensure_quantix_setup(&host).setup_performed);
    let catalogue = rusqlite::Connection::open(application_home.join("installation.sqlite"))
        .expect("installation catalogue");
    catalogue
        .execute_batch("ALTER TABLE installation ADD COLUMN unrecognized TEXT;")
        .expect("altered installation schema");
    drop(catalogue);

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::RepairRequired);
    assert_eq!(
        outcome.issues,
        vec![SetupIssue::InstallationCatalogueCorrupt]
    );
}

#[test]
fn setup_diagnostics_serialize_only_attributable_states_and_issues() {
    let parent = tempfile::tempdir().expect("temporary user home");
    let application_home = parent.path().join(".quantix");
    let platform = FakeSetupPlatform {
        permissions: StoragePermissions::Unverified,
        device_protection: DeviceProtection::Unverified,
        ..FakeSetupPlatform::default()
    };
    let host = host(&application_home, platform);

    let serialized =
        serde_json::to_string(&ensure_quantix_setup(&host)).expect("serialized setup diagnostics");

    assert_eq!(
        serialized,
        r#"{"state":"warning","setup_performed":true,"issues":["storage_permissions_unverified","device_protection_unverified"]}"#
    );
    for forbidden in ["credential", "token", "reasoning", "content", "\\", "/"] {
        assert!(!serialized.contains(forbidden), "{forbidden}");
    }
}
