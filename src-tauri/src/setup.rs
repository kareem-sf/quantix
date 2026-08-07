use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, TransactionBehavior};
use serde::Serialize;
use ts_rs::TS;

use crate::QuantixHost;

pub const MINIMUM_SETUP_FREE_SPACE_BYTES: u64 = 1024 * 1024 * 1024;

const INSTALLATION_SCHEMA_VERSION: i64 = 1;
const SETUP_MARKER: &str = ".setup-in-progress";
const INSTALLATION_DATABASE: &str = "installation.sqlite";
const STAGED_INSTALLATION_DATABASE: &str = "installation.sqlite.staging";
const APPLICATION_DIRECTORIES: [&str; 9] = [
    "archives", "backups", "exports", "logs", "models", "runtimes", "staging", "tenders", "trash",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoragePermissions {
    Restrictive,
    Unsafe,
    Unverified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceProtection {
    Protected,
    Unprotected,
    Unverified,
}

pub trait SetupPlatform: Send + Sync {
    fn available_space(&self, path: &Path) -> io::Result<u64>;
    fn is_writable(&self, path: &Path) -> io::Result<bool>;
    fn storage_permissions(&self, path: &Path) -> io::Result<StoragePermissions>;
    fn device_protection(&self, path: &Path) -> DeviceProtection;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum SetupState {
    Ready,
    Warning,
    AuthenticationRequired,
    MissingCapability,
    UnsupportedVersion,
    RepairRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum SetupIssue {
    ApplicationHomeUnavailable,
    DeviceProtectionDisabled,
    DeviceProtectionUnverified,
    InstallationCatalogueCorrupt,
    InsufficientFreeSpace,
    StorageNotWritable,
    StoragePermissionsUnverified,
    UnrecognizedApplicationHome,
    UnsafeStoragePermissions,
    UnsupportedInstallationVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "../../src/bindings/")]
pub struct SetupOutcome {
    pub state: SetupState,
    pub setup_performed: bool,
    pub issues: Vec<SetupIssue>,
}

impl SetupOutcome {
    fn blocked(state: SetupState, issue: SetupIssue) -> Self {
        Self {
            state,
            setup_performed: false,
            issues: vec![issue],
        }
    }

    fn ready(setup_performed: bool, issues: Vec<SetupIssue>) -> Self {
        Self {
            state: if issues.is_empty() {
                SetupState::Ready
            } else {
                SetupState::Warning
            },
            setup_performed,
            issues,
        }
    }
}

pub struct SystemSetupPlatform;

impl SetupPlatform for SystemSetupPlatform {
    fn available_space(&self, path: &Path) -> io::Result<u64> {
        fs4::available_space(path)
    }

    fn is_writable(&self, path: &Path) -> io::Result<bool> {
        match tempfile::Builder::new()
            .prefix(".quantix-write-probe-")
            .tempfile_in(path)
        {
            Ok(file) => file.close().map(|_| true),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn storage_permissions(&self, path: &Path) -> io::Result<StoragePermissions> {
        system_storage_permissions(path)
    }

    fn device_protection(&self, path: &Path) -> DeviceProtection {
        system_device_protection(path)
    }
}

pub fn ensure_quantix_setup(host: &QuantixHost) -> SetupOutcome {
    host.ensure_setup()
}

pub(crate) fn ensure_application_home(
    application_home: &Path,
    platform: &dyn SetupPlatform,
) -> SetupOutcome {
    if !application_home.is_absolute() {
        return SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::ApplicationHomeUnavailable,
        );
    }

    let existed = application_home.exists();
    if existed && !application_home.is_dir() {
        return SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::ApplicationHomeUnavailable,
        );
    }

    let inspection = if existed {
        match inspect_existing_home(application_home) {
            Ok(inspection) => inspection,
            Err(issue) => return issue,
        }
    } else {
        ExistingHome::Empty
    };

    let probe_path = match nearest_existing_directory(application_home) {
        Some(path) => path,
        None => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::ApplicationHomeUnavailable,
            )
        }
    };

    match platform.available_space(&probe_path) {
        Ok(bytes) if bytes < MINIMUM_SETUP_FREE_SPACE_BYTES => {
            return SetupOutcome::blocked(
                SetupState::MissingCapability,
                SetupIssue::InsufficientFreeSpace,
            )
        }
        Ok(_) => {}
        Err(_) => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::ApplicationHomeUnavailable,
            )
        }
    }

    match platform.is_writable(&probe_path) {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::StorageNotWritable,
            )
        }
    }

    if matches!(inspection, ExistingHome::Installed) {
        if let Some(outcome) = validate_existing_installation(application_home) {
            return outcome;
        }

        if application_home.join(SETUP_MARKER).exists()
            && fs::remove_file(application_home.join(SETUP_MARKER)).is_err()
        {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::InstallationCatalogueCorrupt,
            );
        }

        return finish_with_storage_diagnostics(application_home, platform, false);
    }

    if !existed
        && (fs::create_dir(application_home).is_err()
            || secure_created_directory(application_home).is_err())
    {
        return SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::ApplicationHomeUnavailable,
        );
    }

    match platform.is_writable(application_home) {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::StorageNotWritable,
            )
        }
    }

    match platform.storage_permissions(application_home) {
        Ok(StoragePermissions::Unsafe) => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::UnsafeStoragePermissions,
            )
        }
        Ok(StoragePermissions::Restrictive | StoragePermissions::Unverified) => {}
        Err(_) => {}
    }

    if begin_or_resume_setup(application_home).is_err() {
        return SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::InstallationCatalogueCorrupt,
        );
    }
    if publish_installation_catalogue(application_home).is_err() {
        return SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::InstallationCatalogueCorrupt,
        );
    }

    finish_with_storage_diagnostics(application_home, platform, true)
}

enum ExistingHome {
    Empty,
    Interrupted,
    Installed,
}

fn inspect_existing_home(application_home: &Path) -> Result<ExistingHome, SetupOutcome> {
    let entries = fs::read_dir(application_home).map_err(|_| {
        SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::ApplicationHomeUnavailable,
        )
    })?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| {
            SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::ApplicationHomeUnavailable,
            )
        })?;
        names.push(entry.file_name());
    }

    if names.iter().any(|name| !is_known_application_entry(name)) {
        return Err(SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::UnrecognizedApplicationHome,
        ));
    }

    if names.iter().any(|name| name == INSTALLATION_DATABASE) {
        return Ok(ExistingHome::Installed);
    }

    if names.is_empty() {
        return Ok(ExistingHome::Empty);
    }

    if names.iter().any(|name| name == SETUP_MARKER) {
        Ok(ExistingHome::Interrupted)
    } else {
        Err(SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::UnrecognizedApplicationHome,
        ))
    }
}

fn is_known_application_entry(name: &OsStr) -> bool {
    APPLICATION_DIRECTORIES
        .iter()
        .any(|directory| name == OsStr::new(directory))
        || [
            SETUP_MARKER,
            INSTALLATION_DATABASE,
            STAGED_INSTALLATION_DATABASE,
            "installation.sqlite-shm",
            "installation.sqlite-wal",
        ]
        .iter()
        .any(|known| name == OsStr::new(known))
}

fn nearest_existing_directory(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|candidate| candidate.is_dir())
        .map(Path::to_path_buf)
}

fn begin_or_resume_setup(application_home: &Path) -> io::Result<()> {
    let marker = application_home.join(SETUP_MARKER);
    if !marker.exists() {
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&marker)?;
    }

    for directory in APPLICATION_DIRECTORIES {
        let path = application_home.join(directory);
        if !path.exists() {
            fs::create_dir(&path)?;
            secure_created_directory(&path)?;
        } else if !path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Quantix application directory is not a directory",
            ));
        }
    }

    Ok(())
}

fn publish_installation_catalogue(application_home: &Path) -> rusqlite::Result<()> {
    let staged = application_home.join(STAGED_INSTALLATION_DATABASE);
    let published = application_home.join(INSTALLATION_DATABASE);

    if staged.exists() {
        fs::remove_file(&staged)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    }
    if published.exists() {
        return Err(rusqlite::Error::InvalidPath(published));
    }

    let mut connection = Connection::open(&staged)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE installation (
           singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
           schema_version INTEGER NOT NULL CHECK (schema_version = 1)
         );
         INSERT INTO installation (singleton, schema_version) VALUES (1, 1);
         PRAGMA user_version = 1;",
    )?;
    transaction.commit()?;
    drop(connection);

    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&staged)
        .and_then(|file| file.sync_all())
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    fs::rename(&staged, &published)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;

    match catalogue_status(&published)? {
        CatalogueStatus::Ready => {}
        CatalogueStatus::Unsupported | CatalogueStatus::Corrupt => {
            return Err(rusqlite::Error::InvalidQuery)
        }
    }

    fs::remove_file(application_home.join(SETUP_MARKER))
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    Ok(())
}

fn validate_existing_installation(application_home: &Path) -> Option<SetupOutcome> {
    if APPLICATION_DIRECTORIES
        .iter()
        .any(|directory| !application_home.join(directory).is_dir())
    {
        return Some(SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::InstallationCatalogueCorrupt,
        ));
    }

    match catalogue_status(&application_home.join(INSTALLATION_DATABASE)) {
        Ok(CatalogueStatus::Ready) => None,
        Ok(CatalogueStatus::Unsupported) => Some(SetupOutcome::blocked(
            SetupState::UnsupportedVersion,
            SetupIssue::UnsupportedInstallationVersion,
        )),
        Ok(CatalogueStatus::Corrupt) | Err(_) => Some(SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::InstallationCatalogueCorrupt,
        )),
    }
}

enum CatalogueStatus {
    Ready,
    Unsupported,
    Corrupt,
}

fn catalogue_status(path: &Path) -> rusqlite::Result<CatalogueStatus> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;

    let quick_check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Ok(CatalogueStatus::Corrupt);
    }

    let schema_version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if schema_version != INSTALLATION_SCHEMA_VERSION {
        return Ok(CatalogueStatus::Unsupported);
    }

    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_schema
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         ORDER BY name",
    )?;
    let table_names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    if table_names != ["installation"] {
        return Ok(CatalogueStatus::Corrupt);
    }

    let row: (i64, i64) = connection.query_row(
        "SELECT singleton, schema_version FROM installation",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if row != (1, INSTALLATION_SCHEMA_VERSION) {
        return Ok(CatalogueStatus::Corrupt);
    }
    let row_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM installation", [], |row| row.get(0))?;
    if row_count != 1 {
        return Ok(CatalogueStatus::Corrupt);
    }

    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;",
    )?;
    Ok(CatalogueStatus::Ready)
}

fn finish_with_storage_diagnostics(
    application_home: &Path,
    platform: &dyn SetupPlatform,
    setup_performed: bool,
) -> SetupOutcome {
    let mut issues = Vec::new();

    match platform.storage_permissions(application_home) {
        Ok(StoragePermissions::Restrictive) => {}
        Ok(StoragePermissions::Unsafe) => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::UnsafeStoragePermissions,
            )
        }
        Ok(StoragePermissions::Unverified) | Err(_) => {
            issues.push(SetupIssue::StoragePermissionsUnverified)
        }
    }

    match platform.device_protection(application_home) {
        DeviceProtection::Protected => {}
        DeviceProtection::Unprotected => issues.push(SetupIssue::DeviceProtectionDisabled),
        DeviceProtection::Unverified => issues.push(SetupIssue::DeviceProtectionUnverified),
    }

    SetupOutcome::ready(setup_performed, issues)
}

#[cfg(unix)]
fn secure_created_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn secure_created_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn system_storage_permissions(path: &Path) -> io::Result<StoragePermissions> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)?.permissions().mode();
    Ok(if mode & 0o077 == 0 {
        StoragePermissions::Restrictive
    } else {
        StoragePermissions::Unsafe
    })
}

#[cfg(windows)]
fn system_storage_permissions(path: &Path) -> io::Result<StoragePermissions> {
    use windows_permissions::{
        constants::{AceType, SeObjectType, SecurityInformation},
        utilities::current_process_sid,
        wrappers::{ConvertStringSidToSid, EqualSid, GetNamedSecurityInfo},
    };

    let descriptor = GetNamedSecurityInfo(
        path.as_os_str(),
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::Owner,
    )?;
    let current_user = current_process_sid()?;
    if descriptor
        .owner()
        .is_none_or(|owner| !EqualSid(owner, &current_user))
    {
        return Ok(StoragePermissions::Unsafe);
    }

    let mut allowed_sids = vec![current_user];
    for sid in ["S-1-5-18", "S-1-5-32-544", "S-1-3-0", "S-1-3-4"] {
        allowed_sids.push(ConvertStringSidToSid(sid)?);
    }

    let dacl = match descriptor.dacl() {
        Some(dacl) => dacl,
        None => return Ok(StoragePermissions::Unsafe),
    };
    for index in 0..dacl.len() {
        let Some(ace) = dacl.get_ace(index) else {
            return Ok(StoragePermissions::Unverified);
        };
        let is_allow = matches!(
            ace.ace_type(),
            AceType::ACCESS_ALLOWED_ACE_TYPE
                | AceType::ACCESS_ALLOWED_CALLBACK_ACE_TYPE
                | AceType::ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE
                | AceType::ACCESS_ALLOWED_OBJECT_ACE_TYPE
        );
        if !is_allow {
            continue;
        }
        let Some(sid) = ace.sid() else {
            return Ok(StoragePermissions::Unverified);
        };
        if !allowed_sids.iter().any(|allowed| EqualSid(sid, allowed)) {
            return Ok(StoragePermissions::Unsafe);
        }
    }

    Ok(StoragePermissions::Restrictive)
}

#[cfg(not(any(unix, windows)))]
fn system_storage_permissions(_path: &Path) -> io::Result<StoragePermissions> {
    Ok(StoragePermissions::Unverified)
}

#[cfg(windows)]
fn system_device_protection(path: &Path) -> DeviceProtection {
    use serde::Deserialize;
    use std::path::{Component, Prefix};
    use wmi::{AuthLevel, WMIConnection};

    #[derive(Deserialize)]
    #[serde(rename = "Win32_EncryptableVolume", rename_all = "PascalCase")]
    struct EncryptableVolume {
        drive_letter: Option<String>,
        protection_status: Option<u32>,
    }

    let drive = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                format!("{}:", char::from(letter).to_ascii_uppercase())
            }
            _ => return DeviceProtection::Unverified,
        },
        _ => return DeviceProtection::Unverified,
    };

    let connection = match WMIConnection::with_namespace_path(
        "ROOT\\CIMV2\\Security\\MicrosoftVolumeEncryption",
    ) {
        Ok(connection) => connection,
        Err(_) => return DeviceProtection::Unverified,
    };
    if connection.set_proxy_blanket(AuthLevel::PktPrivacy).is_err() {
        return DeviceProtection::Unverified;
    }
    let volumes: Vec<EncryptableVolume> = match connection.query() {
        Ok(volumes) => volumes,
        Err(_) => return DeviceProtection::Unverified,
    };

    match volumes
        .into_iter()
        .find(|volume| volume.drive_letter.as_deref() == Some(drive.as_str()))
        .and_then(|volume| volume.protection_status)
    {
        Some(1) => DeviceProtection::Protected,
        Some(0) => DeviceProtection::Unprotected,
        _ => DeviceProtection::Unverified,
    }
}

#[cfg(not(windows))]
fn system_device_protection(_path: &Path) -> DeviceProtection {
    DeviceProtection::Unverified
}
