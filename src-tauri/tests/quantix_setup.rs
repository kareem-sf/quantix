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
    let host = host(&application_home, FakeSetupPlatform::default());

    let outcome = ensure_quantix_setup(&host);

    assert_eq!(outcome.state, SetupState::Ready);
    assert!(outcome.setup_performed);
    assert!(application_home.join("installation.sqlite").is_file());
    assert!(!application_home
        .join("installation.sqlite.staging")
        .exists());
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
        .execute_batch("PRAGMA user_version = 2;")
        .expect("newer schema marker");
    drop(catalogue);

    let outcome = ensure_quantix_setup(&host);

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
