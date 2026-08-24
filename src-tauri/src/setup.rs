use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde::Serialize;
use ts_rs::TS;

use crate::QuantixHost;

pub const MINIMUM_SETUP_FREE_SPACE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(crate) const INSTALLATION_SCHEMA_VERSION: i64 = 25;
pub(crate) const CHATGPT_AUTH_STORE: &str = "auth.json";
const SETUP_MARKER: &str = ".setup-in-progress";
const INSTALLATION_DATABASE: &str = "installation.sqlite";
const INSTALLATION_DATABASE_COMPANIONS: [&str; 3] = [
    "installation.sqlite-journal",
    "installation.sqlite-shm",
    "installation.sqlite-wal",
];
const STAGED_INSTALLATION_DATABASE: &str = "installation.sqlite.staging";
const STAGED_INSTALLATION_COMPANIONS: [&str; 3] = [
    "installation.sqlite.staging-journal",
    "installation.sqlite.staging-shm",
    "installation.sqlite.staging-wal",
];
const INSTALLATION_TABLE_SQL: &str = "CREATE TABLE installation (
           singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
           schema_version INTEGER NOT NULL CHECK (schema_version = 25)
         )";
pub(crate) const APPLICATION_SETTINGS_TABLE_SQL: &str = "CREATE TABLE application_settings (
           singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
           settings_json TEXT NOT NULL CHECK (json_valid(settings_json)),
           updated_at TEXT NOT NULL
         )";
pub(crate) const PROVIDER_CONNECTIONS_TABLE_SQL: &str = "CREATE TABLE provider_connections (
           connection_id TEXT PRIMARY KEY CHECK (length(CAST(connection_id AS BLOB)) BETWEEN 1 AND 100),
           provider_kind TEXT NOT NULL CHECK (provider_kind IN ('codex')),
           connection_json TEXT NOT NULL CHECK (json_valid(connection_json)),
           updated_at TEXT NOT NULL
         )";
const MANAGER_WORKSPACE_SELECTION_TABLE_SQL: &str = "CREATE TABLE manager_workspace_selection (
           singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
           selected_tender_id TEXT CHECK (selected_tender_id IS NULL OR length(selected_tender_id) = 32),
           selection_sequence INTEGER NOT NULL CHECK (selection_sequence >= 0),
           selected_at TEXT,
           pending_tender_id TEXT CHECK (pending_tender_id IS NULL OR length(pending_tender_id) = 32),
           pending_at TEXT,
           CHECK ((selected_tender_id IS NULL) = (selected_at IS NULL)),
           CHECK ((pending_tender_id IS NULL) = (pending_at IS NULL))
         )";
pub(crate) const TENDER_BACKUPS_TABLE_SQL: &str = "CREATE TABLE tender_backups (
           backup_id TEXT PRIMARY KEY CHECK (length(backup_id) = 32),
           tender_id TEXT NOT NULL CHECK (length(tender_id) = 32),
           state TEXT NOT NULL CHECK (state IN ('creating', 'ready', 'failed')),
           source_json TEXT,
           content_object_count INTEGER NOT NULL CHECK (content_object_count >= 0),
           manifest_sha256 TEXT CHECK (manifest_sha256 IS NULL OR length(manifest_sha256) = 64),
           archive_size_bytes INTEGER CHECK (archive_size_bytes IS NULL OR archive_size_bytes > 0),
           diagnostic_code TEXT,
           created_at TEXT NOT NULL,
           CHECK (
             (state = 'creating' AND source_json IS NULL AND content_object_count = 0
              AND manifest_sha256 IS NULL AND archive_size_bytes IS NULL AND diagnostic_code IS NULL)
             OR
             (state = 'ready' AND source_json IS NOT NULL AND manifest_sha256 IS NOT NULL
              AND archive_size_bytes IS NOT NULL AND diagnostic_code IS NULL)
             OR
             (state = 'failed' AND source_json IS NULL AND manifest_sha256 IS NULL
              AND archive_size_bytes IS NULL AND diagnostic_code IS NOT NULL)
           )
         )";
pub(crate) const TENDER_RECOVERIES_TABLE_SQL: &str = "CREATE TABLE tender_recoveries (
           recovery_id TEXT PRIMARY KEY CHECK (length(recovery_id) = 32),
           tender_id TEXT NOT NULL CHECK (length(tender_id) = 32),
           backup_id TEXT NOT NULL,
           state TEXT NOT NULL CHECK (state IN ('preparing', 'awaiting_approval', 'applying', 'applied', 'rejected', 'failed')),
           backup_source_json TEXT,
           current_source_json TEXT,
           diagnostic_code TEXT,
           created_at TEXT NOT NULL,
           FOREIGN KEY (backup_id) REFERENCES tender_backups(backup_id),
           CHECK (
             (state = 'preparing' AND backup_source_json IS NOT NULL
              AND diagnostic_code IS NULL)
             OR
             (state = 'awaiting_approval' AND backup_source_json IS NOT NULL
              AND diagnostic_code IS NULL)
             OR
             (state IN ('applying', 'applied', 'rejected') AND backup_source_json IS NOT NULL
              AND diagnostic_code IS NULL)
             OR
             (state = 'failed' AND diagnostic_code IS NOT NULL)
           )
         )";
pub(crate) const TENDER_RECOVERY_DECISIONS_TABLE_SQL: &str =
    "CREATE TABLE tender_recovery_decisions (
           recovery_id TEXT PRIMARY KEY CHECK (length(recovery_id) = 32),
           decision TEXT NOT NULL CHECK (decision IN ('approve_replacement', 'reject')),
           rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 500),
           decided_by TEXT NOT NULL CHECK (length(CAST(decided_by AS BLOB)) BETWEEN 1 AND 200),
           manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
           current_audit_chain_head TEXT CHECK (
             current_audit_chain_head IS NULL OR length(current_audit_chain_head) = 64
           ),
           decided_at TEXT NOT NULL,
           FOREIGN KEY (recovery_id) REFERENCES tender_recoveries(recovery_id)
         )";
const TENDER_RECOVERY_DECISIONS_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER tender_recovery_decisions_no_update
         BEFORE UPDATE ON tender_recovery_decisions
         BEGIN
           SELECT RAISE(ABORT, 'Tender Recovery decisions are immutable');
         END";
const TENDER_RECOVERY_DECISIONS_NO_DELETE_SQL: &str =
    "CREATE TRIGGER tender_recovery_decisions_no_delete
         BEFORE DELETE ON tender_recovery_decisions
         WHEN NOT EXISTS (
           SELECT 1 FROM tender_recoveries AS recovery
           JOIN tender_trash AS trash ON trash.tender_id = recovery.tender_id
           WHERE recovery.recovery_id = OLD.recovery_id AND trash.state = 'purging'
         )
         BEGIN
           SELECT RAISE(ABORT, 'Tender Recovery decisions are immutable');
         END";
const TENDER_CATALOGUE_TABLE_SQL: &str = "CREATE TABLE tender_catalogue (
           tender_id TEXT PRIMARY KEY CHECK (length(tender_id) = 32),
           name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 200),
           revision INTEGER NOT NULL CHECK (revision > 0),
           audit_event_count INTEGER NOT NULL CHECK (audit_event_count > 0),
           audit_chain_head TEXT NOT NULL CHECK (length(audit_chain_head) = 64)
         )";
const PORTABLE_TENDER_ARCHIVES_TABLE_SQL: &str = "CREATE TABLE portable_tender_archives (
           archive_id TEXT PRIMARY KEY CHECK (length(archive_id) = 32),
           tender_id TEXT NOT NULL CHECK (length(tender_id) = 32),
           backup_id TEXT NOT NULL CHECK (length(backup_id) = 32),
           relative_path TEXT NOT NULL UNIQUE CHECK (length(CAST(relative_path AS BLOB)) BETWEEN 1 AND 1000),
           manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
           archive_size_bytes INTEGER NOT NULL CHECK (archive_size_bytes > 0),
           archive_json TEXT NOT NULL CHECK (json_valid(archive_json)),
           created_at TEXT NOT NULL
         )";
const TENDER_TRASH_TABLE_SQL: &str = "CREATE TABLE tender_trash (
           deletion_id TEXT PRIMARY KEY CHECK (length(deletion_id) = 32),
           tender_id TEXT NOT NULL CHECK (length(tender_id) = 32),
           state TEXT NOT NULL CHECK (state IN ('moving', 'trashed', 'restoring', 'purging', 'restored', 'purged', 'failed')),
           relative_path TEXT NOT NULL UNIQUE CHECK (length(CAST(relative_path AS BLOB)) BETWEEN 1 AND 1000),
           approval_json TEXT NOT NULL CHECK (json_valid(approval_json)),
           approval_manifest_sha256 TEXT NOT NULL CHECK (length(approval_manifest_sha256) = 64),
           diagnostic_code TEXT,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL
         )";
const TENDER_TRASH_DECISIONS_TABLE_SQL: &str = "CREATE TABLE tender_trash_decisions (
           decision_id TEXT PRIMARY KEY CHECK (length(decision_id) = 32),
           deletion_id TEXT NOT NULL,
           action TEXT NOT NULL CHECK (action IN ('restore', 'purge')),
           rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
           decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
           acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_engineer'),
           decision_json TEXT NOT NULL CHECK (json_valid(decision_json)),
           manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
           created_at TEXT NOT NULL,
           FOREIGN KEY (deletion_id) REFERENCES tender_trash(deletion_id)
         )";
const TENDER_TRASH_DECISIONS_NO_UPDATE_SQL: &str = "CREATE TRIGGER tender_trash_decisions_no_update
         BEFORE UPDATE ON tender_trash_decisions
         BEGIN SELECT RAISE(ABORT, 'Tender Trash decisions are immutable'); END";
const TENDER_TRASH_DECISIONS_NO_DELETE_SQL: &str = "CREATE TRIGGER tender_trash_decisions_no_delete
         BEFORE DELETE ON tender_trash_decisions
         WHEN NOT EXISTS (
           SELECT 1 FROM tender_trash
           WHERE deletion_id = OLD.deletion_id AND state = 'purging'
         )
         BEGIN SELECT RAISE(ABORT, 'Tender Trash decisions are immutable'); END";
const DELETION_RECEIPTS_TABLE_SQL: &str = "CREATE TABLE deletion_receipts (
           receipt_id TEXT PRIMARY KEY CHECK (length(receipt_id) = 32),
           tender_id TEXT NOT NULL UNIQUE CHECK (length(tender_id) = 32),
           deletion_id TEXT NOT NULL UNIQUE CHECK (length(deletion_id) = 32),
           receipt_json TEXT NOT NULL CHECK (json_valid(receipt_json)),
           manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
           created_at TEXT NOT NULL
         )";
const PROVIDER_CLEANUP_JOBS_TABLE_SQL: &str = "CREATE TABLE provider_cleanup_jobs (
           cleanup_id TEXT PRIMARY KEY CHECK (length(cleanup_id) = 32),
           deletion_id TEXT NOT NULL CHECK (length(deletion_id) = 32),
           target_ordinal INTEGER NOT NULL CHECK (target_ordinal >= 0),
           provider_kind TEXT NOT NULL CHECK (provider_kind IN ('codex')),
           thread_ref TEXT CHECK (thread_ref IS NULL OR length(CAST(thread_ref AS BLOB)) BETWEEN 1 AND 1000),
           target_manifest_sha256 TEXT NOT NULL CHECK (length(target_manifest_sha256) = 64),
           status TEXT NOT NULL CHECK (status IN ('pending', 'completed')),
           attempt_count INTEGER NOT NULL CHECK (attempt_count >= 0),
           diagnostic_code TEXT CHECK (diagnostic_code IS NULL OR length(CAST(diagnostic_code AS BLOB)) BETWEEN 1 AND 100),
           last_attempt_at TEXT,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL,
           UNIQUE (deletion_id, target_ordinal),
           CHECK (
             (status = 'pending' AND thread_ref IS NOT NULL)
             OR (status = 'completed' AND thread_ref IS NULL AND diagnostic_code IS NULL)
           )
         )";
const PROVIDER_CLEANUP_JOBS_IDENTITY_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER provider_cleanup_jobs_identity_no_update
         BEFORE UPDATE OF cleanup_id, deletion_id, target_ordinal, provider_kind,
                          target_manifest_sha256 ON provider_cleanup_jobs
         BEGIN SELECT RAISE(ABORT, 'Provider cleanup identities are immutable'); END";
const PROVIDER_CLEANUP_JOBS_NO_DELETE_SQL: &str = "CREATE TRIGGER provider_cleanup_jobs_no_delete
         BEFORE DELETE ON provider_cleanup_jobs
         BEGIN SELECT RAISE(ABORT, 'Provider cleanup records are immutable'); END";
const PORTABLE_TENDER_ARCHIVES_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER portable_tender_archives_no_update
         BEFORE UPDATE ON portable_tender_archives
         BEGIN SELECT RAISE(ABORT, 'Portable Tender Archive records are immutable'); END";
const PORTABLE_TENDER_ARCHIVES_NO_DELETE_SQL: &str =
    "CREATE TRIGGER portable_tender_archives_no_delete
         BEFORE DELETE ON portable_tender_archives
         WHEN NOT EXISTS (
           SELECT 1 FROM tender_trash
           WHERE tender_id = OLD.tender_id AND state = 'purging'
         )
         BEGIN SELECT RAISE(ABORT, 'Portable Tender Archive records are immutable'); END";
const DELETION_RECEIPTS_NO_UPDATE_SQL: &str = "CREATE TRIGGER deletion_receipts_no_update
         BEFORE UPDATE ON deletion_receipts
         BEGIN SELECT RAISE(ABORT, 'Deletion Receipts are immutable'); END";
const DELETION_RECEIPTS_NO_DELETE_SQL: &str = "CREATE TRIGGER deletion_receipts_no_delete
         BEFORE DELETE ON deletion_receipts
         BEGIN SELECT RAISE(ABORT, 'Deletion Receipts are immutable'); END";
const PRODUCT_ACCEPTANCE_RUNS_TABLE_SQL: &str = "CREATE TABLE product_acceptance_runs (
           run_id TEXT PRIMARY KEY CHECK (length(run_id) = 32),
           suite TEXT NOT NULL CHECK (suite IN ('deterministic', 'live_provider', 'native_package')),
           source_revision TEXT NOT NULL CHECK (length(CAST(source_revision AS BLOB)) BETWEEN 1 AND 200),
           outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
           run_json TEXT NOT NULL CHECK (json_valid(run_json)),
           manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
           created_at TEXT NOT NULL
         )";
const PRODUCT_ACCEPTANCE_RECORDS_TABLE_SQL: &str = "CREATE TABLE product_acceptance_records (
           record_id TEXT PRIMARY KEY CHECK (length(record_id) = 32),
           source_revision TEXT NOT NULL CHECK (length(CAST(source_revision AS BLOB)) BETWEEN 1 AND 200),
           record_json TEXT NOT NULL CHECK (json_valid(record_json)),
           manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
           created_at TEXT NOT NULL
         )";
const PRODUCT_ACCEPTANCE_RUNS_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER product_acceptance_runs_no_update
         BEFORE UPDATE ON product_acceptance_runs
         BEGIN SELECT RAISE(ABORT, 'Product Acceptance Runs are immutable'); END";
const PRODUCT_ACCEPTANCE_RUNS_NO_DELETE_SQL: &str =
    "CREATE TRIGGER product_acceptance_runs_no_delete
         BEFORE DELETE ON product_acceptance_runs
         BEGIN SELECT RAISE(ABORT, 'Product Acceptance Runs are immutable'); END";
const PRODUCT_ACCEPTANCE_RECORDS_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER product_acceptance_records_no_update
         BEFORE UPDATE ON product_acceptance_records
         BEGIN SELECT RAISE(ABORT, 'Product Acceptance Records are immutable'); END";
const PRODUCT_ACCEPTANCE_RECORDS_NO_DELETE_SQL: &str =
    "CREATE TRIGGER product_acceptance_records_no_delete
         BEFORE DELETE ON product_acceptance_records
         BEGIN SELECT RAISE(ABORT, 'Product Acceptance Records are immutable'); END";
const LIVE_QUALIFICATION_RUNS_TABLE_SQL: &str = "CREATE TABLE live_qualification_runs (
           run_id TEXT PRIMARY KEY CHECK (length(run_id) = 32),
           release_candidate_sha256 TEXT NOT NULL CHECK (length(release_candidate_sha256) = 64),
           sequence_number INTEGER NOT NULL CHECK (sequence_number BETWEEN 1 AND 5),
           outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
           run_json TEXT NOT NULL CHECK (json_valid(run_json)),
           manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
           created_at TEXT NOT NULL
         )";
const PRIVATE_QUALIFICATION_RECORDS_TABLE_SQL: &str = "CREATE TABLE private_qualification_records (
           record_id TEXT PRIMARY KEY CHECK (length(record_id) = 32),
           release_candidate_sha256 TEXT NOT NULL UNIQUE CHECK (length(release_candidate_sha256) = 64),
           record_json TEXT NOT NULL CHECK (json_valid(record_json)),
           manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
           created_at TEXT NOT NULL
         )";
const LIVE_QUALIFICATION_RUNS_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER live_qualification_runs_no_update
         BEFORE UPDATE ON live_qualification_runs
         BEGIN SELECT RAISE(ABORT, 'Live Qualification Runs are immutable'); END";
const LIVE_QUALIFICATION_RUNS_NO_DELETE_SQL: &str =
    "CREATE TRIGGER live_qualification_runs_no_delete
         BEFORE DELETE ON live_qualification_runs
         BEGIN SELECT RAISE(ABORT, 'Live Qualification Runs are immutable'); END";
const PRIVATE_QUALIFICATION_RECORDS_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER private_qualification_records_no_update
         BEFORE UPDATE ON private_qualification_records
         BEGIN SELECT RAISE(ABORT, 'Private Qualification Records are immutable'); END";
const PRIVATE_QUALIFICATION_RECORDS_NO_DELETE_SQL: &str =
    "CREATE TRIGGER private_qualification_records_no_delete
         BEFORE DELETE ON private_qualification_records
         BEGIN SELECT RAISE(ABORT, 'Private Qualification Records are immutable'); END";
const PUBLIC_RELEASE_GATE_RECORDS_TABLE_SQL: &str = "CREATE TABLE public_release_gate_records (
           gate_id TEXT PRIMARY KEY CHECK (length(gate_id) = 32),
           release_candidate_manifest_sha256 TEXT NOT NULL CHECK (length(release_candidate_manifest_sha256) = 64),
           outcome TEXT NOT NULL CHECK (outcome IN ('blocked', 'authorized')),
           record_json TEXT NOT NULL CHECK (json_valid(record_json)),
           manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
           created_at TEXT NOT NULL
         )";
const NATIVE_PLATFORM_QUALIFICATION_RECORDS_TABLE_SQL: &str =
    "CREATE TABLE native_platform_qualification_records (
           record_id TEXT PRIMARY KEY CHECK (length(record_id) = 32),
           platform TEXT NOT NULL CHECK (platform IN ('windows_11_x64', 'macos_14_apple_silicon', 'ubuntu_24_04_x64')),
           outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
           record_json TEXT NOT NULL CHECK (json_valid(record_json)),
           manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
           created_at TEXT NOT NULL
         )";
const NATIVE_PLATFORM_QUALIFICATION_RECORDS_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER native_platform_qualification_records_no_update
         BEFORE UPDATE ON native_platform_qualification_records
         BEGIN SELECT RAISE(ABORT, 'Native platform Qualification records are immutable'); END";
const NATIVE_PLATFORM_QUALIFICATION_RECORDS_NO_DELETE_SQL: &str =
    "CREATE TRIGGER native_platform_qualification_records_no_delete
         BEFORE DELETE ON native_platform_qualification_records
         BEGIN SELECT RAISE(ABORT, 'Native platform Qualification records are immutable'); END";
const PUBLIC_RELEASE_GATE_RECORDS_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER public_release_gate_records_no_update
         BEFORE UPDATE ON public_release_gate_records
         BEGIN SELECT RAISE(ABORT, 'Public Release Gate records are immutable'); END";
const PUBLIC_RELEASE_GATE_RECORDS_NO_DELETE_SQL: &str =
    "CREATE TRIGGER public_release_gate_records_no_delete
         BEFORE DELETE ON public_release_gate_records
         BEGIN SELECT RAISE(ABORT, 'Public Release Gate records are immutable'); END";
pub(crate) const RUNTIME_PREPARATION_TABLE_SQL: &str = "CREATE TABLE runtime_preparation (
           singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
           status TEXT NOT NULL CHECK (status IN ('not_started', 'preparing', 'ready', 'failed')),
           uv_version TEXT,
           ocr_version TEXT,
           updated_at TEXT NOT NULL
         )";
pub(crate) const UPDATE_OPERATIONS_TABLE_SQL: &str = "CREATE TABLE update_operations (
           update_id TEXT PRIMARY KEY CHECK (length(update_id) = 64),
           state TEXT NOT NULL CHECK (state IN (
             'awaiting_approval', 'approved', 'denied', 'installing',
             'restart_validation_required', 'ready', 'rejected', 'repair_required',
             'rolled_back'
           )),
           offer_json TEXT NOT NULL,
           diagnostic_code TEXT,
           created_at TEXT NOT NULL,
           updated_at TEXT NOT NULL,
           CHECK (
             (state IN ('awaiting_approval', 'approved', 'denied', 'installing',
                        'restart_validation_required', 'ready') AND diagnostic_code IS NULL)
             OR
             (state IN ('rejected', 'repair_required', 'rolled_back') AND diagnostic_code IS NOT NULL)
           )
         )";
const UPDATE_OPERATIONS_OFFER_NO_UPDATE_SQL: &str =
    "CREATE TRIGGER update_operations_offer_no_update
         BEFORE UPDATE OF update_id, offer_json, created_at ON update_operations
         BEGIN
           SELECT RAISE(ABORT, 'Update offer evidence is immutable');
         END";
const UPDATE_OPERATIONS_NO_DELETE_SQL: &str = "CREATE TRIGGER update_operations_no_delete
         BEFORE DELETE ON update_operations
         BEGIN
           SELECT RAISE(ABORT, 'Update operation history is immutable');
         END";
pub(crate) const UPDATE_DECISIONS_TABLE_SQL: &str = "CREATE TABLE update_decisions (
           sequence INTEGER PRIMARY KEY,
           update_id TEXT NOT NULL UNIQUE CHECK (length(update_id) = 64),
           offer_sha256 TEXT NOT NULL CHECK (length(offer_sha256) = 64),
           decision TEXT NOT NULL CHECK (decision IN ('approve', 'deny')),
           rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
           decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
           acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
           decided_at TEXT NOT NULL,
           preceding_hash TEXT NOT NULL CHECK (length(preceding_hash) = 64),
           current_hash TEXT NOT NULL UNIQUE CHECK (length(current_hash) = 64),
           FOREIGN KEY (update_id) REFERENCES update_operations(update_id),
           CHECK (offer_sha256 = update_id)
         )";
const UPDATE_DECISIONS_NO_UPDATE_SQL: &str = "CREATE TRIGGER update_decisions_no_update
         BEFORE UPDATE ON update_decisions
         BEGIN
           SELECT RAISE(ABORT, 'Update decisions are immutable');
         END";
const UPDATE_DECISIONS_NO_DELETE_SQL: &str = "CREATE TRIGGER update_decisions_no_delete
         BEFORE DELETE ON update_decisions
         BEGIN
           SELECT RAISE(ABORT, 'Update decisions are immutable');
         END";
pub(crate) const UPDATE_RECOVERY_POINTS_TABLE_SQL: &str = "CREATE TABLE update_recovery_points (
           update_id TEXT PRIMARY KEY CHECK (length(update_id) = 64),
           application_version TEXT NOT NULL CHECK (length(CAST(application_version AS BLOB)) BETWEEN 1 AND 64),
           artifact_kind TEXT NOT NULL CHECK (artifact_kind IN ('windows_bundle', 'mac_os_bundle', 'linux_app_image')),
           destination_root TEXT NOT NULL CHECK (length(CAST(destination_root AS BLOB)) BETWEEN 1 AND 32767),
           manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
           manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
           FOREIGN KEY (update_id) REFERENCES update_operations(update_id)
         )";
const UPDATE_RECOVERY_POINTS_NO_UPDATE_SQL: &str = "CREATE TRIGGER update_recovery_points_no_update
         BEFORE UPDATE ON update_recovery_points
         BEGIN
           SELECT RAISE(ABORT, 'Update recovery points are immutable');
         END";
const UPDATE_RECOVERY_POINTS_NO_DELETE_SQL: &str = "CREATE TRIGGER update_recovery_points_no_delete
         BEFORE DELETE ON update_recovery_points
         BEGIN
           SELECT RAISE(ABORT, 'Update recovery points are immutable');
         END";
const APPLICATION_DIRECTORIES: [&str; 10] = [
    "archives",
    "backups",
    "exports",
    "logs",
    "models",
    "runtimes",
    "staging",
    "tenders",
    "trash",
    "update-recovery",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoragePermissions {
    Restrictive,
    Unsafe,
    Unverified,
}

pub trait SetupPlatform: Send + Sync {
    fn available_space(&self, path: &Path) -> io::Result<u64>;
    fn is_writable(&self, path: &Path) -> io::Result<bool>;
    fn storage_permissions(&self, path: &Path) -> io::Result<StoragePermissions>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SetupState {
    Ready,
    Warning,
    MissingCapability,
    UnsupportedVersion,
    RepairRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SetupIssue {
    ApplicationHomeUnavailable,
    InstallationCatalogueCorrupt,
    StorageNotWritable,
    StoragePermissionsUnverified,
    UnrecognizedApplicationHome,
    UnsafeStorageLocation,
    UnsafeStoragePermissions,
    UnsupportedInstallationVersion,
    UpdateInstallationActive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct SetupOutcome {
    pub state: SetupState,
    pub setup_performed: bool,
    pub issues: Vec<SetupIssue>,
}

impl SetupOutcome {
    pub(crate) fn blocked(state: SetupState, issue: SetupIssue) -> Self {
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

    let application_home_metadata = match fs::symlink_metadata(application_home) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(_) => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::ApplicationHomeUnavailable,
            )
        }
    };
    let existed = application_home_metadata.is_some();
    if let Some(metadata) = application_home_metadata.as_ref() {
        if metadata_is_unsafe_storage_link(metadata) {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::UnsafeStorageLocation,
            );
        }
        if !metadata.is_dir() {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::ApplicationHomeUnavailable,
            );
        }
    }

    let inspection = if existed {
        match inspect_existing_home(application_home) {
            Ok(inspection) => inspection,
            Err(issue) => return issue,
        }
    } else {
        ExistingHome::Empty
    };

    if matches!(inspection, ExistingHome::Installed) {
        if let Some(outcome) = validate_existing_installation(application_home) {
            return outcome;
        }
    }

    let probe_path = match nearest_existing_directory(application_home)
        .and_then(|path| fs::canonicalize(path).ok())
    {
        Some(path) => path,
        None => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::ApplicationHomeUnavailable,
            )
        }
    };

    let mut storage_permissions = if existed {
        match checked_storage_permissions(&probe_path, platform) {
            Ok(permissions) => Some(permissions),
            Err(outcome) => return outcome,
        }
    } else {
        None
    };

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
        if application_home.join(SETUP_MARKER).exists()
            && fs::remove_file(application_home.join(SETUP_MARKER)).is_err()
        {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::InstallationCatalogueCorrupt,
            );
        }

        return finish_with_storage_diagnostics(
            storage_permissions.expect("existing homes have permission diagnostics"),
            false,
        );
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

    let application_storage = match fs::canonicalize(application_home) {
        Ok(path) => path,
        Err(_) => {
            return SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::ApplicationHomeUnavailable,
            )
        }
    };

    if !existed {
        match platform.is_writable(&application_storage) {
            Ok(true) => {}
            Ok(false) | Err(_) => {
                return SetupOutcome::blocked(
                    SetupState::RepairRequired,
                    SetupIssue::StorageNotWritable,
                )
            }
        }
        storage_permissions = match checked_storage_permissions(&application_storage, platform) {
            Ok(permissions) => Some(permissions),
            Err(outcome) => return outcome,
        };
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

    finish_with_storage_diagnostics(
        storage_permissions.expect("application homes have permission diagnostics"),
        true,
    )
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
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| {
            SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::ApplicationHomeUnavailable,
            )
        })?;
        if metadata_is_unsafe_storage_link(&metadata) {
            return Err(SetupOutcome::blocked(
                SetupState::RepairRequired,
                SetupIssue::UnsafeStorageLocation,
            ));
        }
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
        ]
        .iter()
        .any(|known| name == OsStr::new(known))
        || INSTALLATION_DATABASE_COMPANIONS
            .iter()
            .any(|known| name == OsStr::new(known))
        || STAGED_INSTALLATION_COMPANIONS
            .iter()
            .any(|known| name == OsStr::new(known))
        || is_chatgpt_auth_store_entry(name)
}

fn is_chatgpt_auth_store_entry(name: &OsStr) -> bool {
    if name == OsStr::new(CHATGPT_AUTH_STORE) {
        return true;
    }
    let atomic_prefix = format!(".{CHATGPT_AUTH_STORE}.");
    let Some(sequence) = name
        .to_str()
        .and_then(|name| name.strip_prefix(&atomic_prefix))
        .and_then(|name| name.strip_suffix(".tmp"))
    else {
        return false;
    };
    let mut parts = sequence.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(process_id), Some(sequence), None)
            if !process_id.is_empty()
                && process_id.bytes().all(|byte| byte.is_ascii_digit())
                && !sequence.is_empty()
                && sequence.bytes().all(|byte| byte.is_ascii_digit())
    )
}

fn nearest_existing_directory(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|candidate| candidate.is_dir())
        .map(Path::to_path_buf)
}

pub(crate) fn validate_application_home_path(application_home: &Path) -> io::Result<PathBuf> {
    if !application_home.is_absolute() || path_contains_alternate_stream(application_home) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid application home",
        ));
    }

    let metadata = fs::symlink_metadata(application_home)?;
    if !metadata.is_dir() || metadata_is_unsafe_storage_link(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid application home",
        ));
    }

    let canonical = fs::canonicalize(application_home)?;
    if !paths_are_exactly_equivalent(application_home, &canonical) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid application home",
        ));
    }
    Ok(canonical)
}

fn path_contains_alternate_stream(path: &Path) -> bool {
    path.components().any(|component| match component {
        std::path::Component::Normal(value) => value.to_string_lossy().contains(':'),
        _ => false,
    })
}

#[cfg(windows)]
fn paths_are_exactly_equivalent(left: &Path, right: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    fn comparable(path: &Path) -> Vec<u16> {
        let encoded: Vec<_> = path.as_os_str().encode_wide().collect();
        const VERBATIM_PREFIX: [u16; 4] = [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
        const UNC_PREFIX: [u16; 4] = [b'U' as u16, b'N' as u16, b'C' as u16, b'\\' as u16];
        if encoded.starts_with(&VERBATIM_PREFIX) {
            let remainder = &encoded[VERBATIM_PREFIX.len()..];
            if remainder.starts_with(&UNC_PREFIX) {
                let mut normalized = vec![b'\\' as u16, b'\\' as u16];
                normalized.extend_from_slice(&remainder[UNC_PREFIX.len()..]);
                return normalized;
            }
            return remainder.to_vec();
        }
        encoded
    }

    comparable(left) == comparable(right)
}

#[cfg(not(windows))]
fn paths_are_exactly_equivalent(left: &Path, right: &Path) -> bool {
    left == right
}

fn metadata_is_unsafe_storage_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }

    false
}

fn checked_storage_permissions(
    path: &Path,
    platform: &dyn SetupPlatform,
) -> Result<StoragePermissions, SetupOutcome> {
    let mut permissions = platform.storage_permissions(path);
    if matches!(permissions, Ok(StoragePermissions::Unsafe))
        && secure_created_directory(path).is_ok()
    {
        permissions = platform.storage_permissions(path);
    }
    match permissions {
        Ok(StoragePermissions::Unsafe) => Err(SetupOutcome::blocked(
            SetupState::RepairRequired,
            SetupIssue::UnsafeStoragePermissions,
        )),
        Ok(permissions) => Ok(permissions),
        Err(_) => Ok(StoragePermissions::Unverified),
    }
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

    for setup_file in
        std::iter::once(STAGED_INSTALLATION_DATABASE).chain(STAGED_INSTALLATION_COMPANIONS)
    {
        let setup_path = application_home.join(setup_file);
        if setup_path.exists() {
            fs::remove_file(&setup_path)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
    }
    if published.exists() {
        return Err(rusqlite::Error::InvalidPath(published));
    }

    let mut connection = Connection::open(&staged)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    let journal_mode: String =
        connection.query_row("PRAGMA journal_mode = MEMORY", [], |row| row.get(0))?;
    if journal_mode != "memory" {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(INSTALLATION_TABLE_SQL, [])?;
    transaction.execute(APPLICATION_SETTINGS_TABLE_SQL, [])?;
    transaction.execute(PROVIDER_CONNECTIONS_TABLE_SQL, [])?;
    transaction.execute(MANAGER_WORKSPACE_SELECTION_TABLE_SQL, [])?;
    transaction.execute(RUNTIME_PREPARATION_TABLE_SQL, [])?;
    transaction.execute(TENDER_BACKUPS_TABLE_SQL, [])?;
    transaction.execute(TENDER_CATALOGUE_TABLE_SQL, [])?;
    transaction.execute(PORTABLE_TENDER_ARCHIVES_TABLE_SQL, [])?;
    transaction.execute(TENDER_TRASH_TABLE_SQL, [])?;
    transaction.execute(TENDER_TRASH_DECISIONS_TABLE_SQL, [])?;
    transaction.execute(DELETION_RECEIPTS_TABLE_SQL, [])?;
    transaction.execute(PROVIDER_CLEANUP_JOBS_TABLE_SQL, [])?;
    transaction.execute(PRODUCT_ACCEPTANCE_RUNS_TABLE_SQL, [])?;
    transaction.execute(PRODUCT_ACCEPTANCE_RECORDS_TABLE_SQL, [])?;
    transaction.execute(LIVE_QUALIFICATION_RUNS_TABLE_SQL, [])?;
    transaction.execute(PRIVATE_QUALIFICATION_RECORDS_TABLE_SQL, [])?;
    transaction.execute(PUBLIC_RELEASE_GATE_RECORDS_TABLE_SQL, [])?;
    transaction.execute(NATIVE_PLATFORM_QUALIFICATION_RECORDS_TABLE_SQL, [])?;
    transaction.execute(TENDER_RECOVERIES_TABLE_SQL, [])?;
    transaction.execute(TENDER_RECOVERY_DECISIONS_TABLE_SQL, [])?;
    transaction.execute(UPDATE_OPERATIONS_TABLE_SQL, [])?;
    transaction.execute(UPDATE_DECISIONS_TABLE_SQL, [])?;
    transaction.execute(UPDATE_RECOVERY_POINTS_TABLE_SQL, [])?;
    transaction.execute(TENDER_RECOVERY_DECISIONS_NO_UPDATE_SQL, [])?;
    transaction.execute(TENDER_RECOVERY_DECISIONS_NO_DELETE_SQL, [])?;
    transaction.execute(UPDATE_DECISIONS_NO_UPDATE_SQL, [])?;
    transaction.execute(UPDATE_DECISIONS_NO_DELETE_SQL, [])?;
    transaction.execute(UPDATE_OPERATIONS_OFFER_NO_UPDATE_SQL, [])?;
    transaction.execute(UPDATE_OPERATIONS_NO_DELETE_SQL, [])?;
    transaction.execute(UPDATE_RECOVERY_POINTS_NO_UPDATE_SQL, [])?;
    transaction.execute(UPDATE_RECOVERY_POINTS_NO_DELETE_SQL, [])?;
    transaction.execute(PORTABLE_TENDER_ARCHIVES_NO_UPDATE_SQL, [])?;
    transaction.execute(PORTABLE_TENDER_ARCHIVES_NO_DELETE_SQL, [])?;
    transaction.execute(DELETION_RECEIPTS_NO_UPDATE_SQL, [])?;
    transaction.execute(DELETION_RECEIPTS_NO_DELETE_SQL, [])?;
    transaction.execute(PROVIDER_CLEANUP_JOBS_IDENTITY_NO_UPDATE_SQL, [])?;
    transaction.execute(PROVIDER_CLEANUP_JOBS_NO_DELETE_SQL, [])?;
    transaction.execute(TENDER_TRASH_DECISIONS_NO_UPDATE_SQL, [])?;
    transaction.execute(TENDER_TRASH_DECISIONS_NO_DELETE_SQL, [])?;
    transaction.execute(PRODUCT_ACCEPTANCE_RUNS_NO_UPDATE_SQL, [])?;
    transaction.execute(PRODUCT_ACCEPTANCE_RUNS_NO_DELETE_SQL, [])?;
    transaction.execute(PRODUCT_ACCEPTANCE_RECORDS_NO_UPDATE_SQL, [])?;
    transaction.execute(PRODUCT_ACCEPTANCE_RECORDS_NO_DELETE_SQL, [])?;
    transaction.execute(LIVE_QUALIFICATION_RUNS_NO_UPDATE_SQL, [])?;
    transaction.execute(LIVE_QUALIFICATION_RUNS_NO_DELETE_SQL, [])?;
    transaction.execute(PRIVATE_QUALIFICATION_RECORDS_NO_UPDATE_SQL, [])?;
    transaction.execute(PRIVATE_QUALIFICATION_RECORDS_NO_DELETE_SQL, [])?;
    transaction.execute(PUBLIC_RELEASE_GATE_RECORDS_NO_UPDATE_SQL, [])?;
    transaction.execute(PUBLIC_RELEASE_GATE_RECORDS_NO_DELETE_SQL, [])?;
    transaction.execute(NATIVE_PLATFORM_QUALIFICATION_RECORDS_NO_UPDATE_SQL, [])?;
    transaction.execute(NATIVE_PLATFORM_QUALIFICATION_RECORDS_NO_DELETE_SQL, [])?;
    transaction.execute(
        "INSERT INTO installation (singleton, schema_version) VALUES (1, ?1)",
        [INSTALLATION_SCHEMA_VERSION],
    )?;
    transaction.execute(
        "INSERT INTO application_settings (singleton, settings_json, updated_at)
         VALUES (1, '{\"general_preferences\":{\"appearance\":\"system\",\"reduced_motion\":false,\"larger_text\":false,\"notify_when_attention_needed\":false},\"ai_execution_selection\":null,\"ai_execution_approval\":null}', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        [],
    )?;
    transaction.execute(
        "INSERT INTO runtime_preparation (
           singleton, status, uv_version, ocr_version, updated_at
         ) VALUES (1, 'not_started', NULL, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        [],
    )?;
    transaction.execute(
        "INSERT INTO manager_workspace_selection (
           singleton, selected_tender_id, selection_sequence, selected_at,
           pending_tender_id, pending_at
         ) VALUES (1, NULL, 0, NULL, NULL, NULL)",
        [],
    )?;
    transaction.pragma_update(None, "user_version", INSTALLATION_SCHEMA_VERSION)?;
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

fn schema_object(
    object_type: &str,
    table_name: &str,
    sql: &str,
) -> (String, String, String, Option<String>) {
    let object_name = sql
        .split_ascii_whitespace()
        .nth(2)
        .expect("owned Quantix schema declarations have an object name");
    (
        object_type.to_owned(),
        object_name.to_owned(),
        table_name.to_owned(),
        Some(sql.to_owned()),
    )
}

fn catalogue_status(path: &Path) -> rusqlite::Result<CatalogueStatus> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(Duration::from_secs(5))?;

    let quick_check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Ok(CatalogueStatus::Corrupt);
    }

    let schema_version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if schema_version > INSTALLATION_SCHEMA_VERSION {
        return Ok(CatalogueStatus::Unsupported);
    }
    if schema_version < INSTALLATION_SCHEMA_VERSION {
        return Ok(CatalogueStatus::Corrupt);
    }

    let mut statement = connection.prepare(
        "SELECT type, name, tbl_name, sql FROM sqlite_schema
         WHERE name NOT LIKE 'sqlite_%'
         ORDER BY type, name",
    )?;
    let schema_objects = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let mut expected_schema_objects = vec![
        schema_object("table", "installation", INSTALLATION_TABLE_SQL),
        schema_object(
            "table",
            "application_settings",
            APPLICATION_SETTINGS_TABLE_SQL,
        ),
        schema_object(
            "table",
            "provider_connections",
            PROVIDER_CONNECTIONS_TABLE_SQL,
        ),
        schema_object(
            "table",
            "manager_workspace_selection",
            MANAGER_WORKSPACE_SELECTION_TABLE_SQL,
        ),
        schema_object(
            "table",
            "runtime_preparation",
            RUNTIME_PREPARATION_TABLE_SQL,
        ),
        schema_object("table", "tender_backups", TENDER_BACKUPS_TABLE_SQL),
        schema_object("table", "tender_catalogue", TENDER_CATALOGUE_TABLE_SQL),
        schema_object(
            "table",
            "portable_tender_archives",
            PORTABLE_TENDER_ARCHIVES_TABLE_SQL,
        ),
        schema_object("table", "tender_trash", TENDER_TRASH_TABLE_SQL),
        schema_object(
            "table",
            "tender_trash_decisions",
            TENDER_TRASH_DECISIONS_TABLE_SQL,
        ),
        schema_object("table", "deletion_receipts", DELETION_RECEIPTS_TABLE_SQL),
        schema_object(
            "table",
            "provider_cleanup_jobs",
            PROVIDER_CLEANUP_JOBS_TABLE_SQL,
        ),
        schema_object(
            "table",
            "product_acceptance_runs",
            PRODUCT_ACCEPTANCE_RUNS_TABLE_SQL,
        ),
        schema_object(
            "table",
            "product_acceptance_records",
            PRODUCT_ACCEPTANCE_RECORDS_TABLE_SQL,
        ),
        schema_object(
            "table",
            "live_qualification_runs",
            LIVE_QUALIFICATION_RUNS_TABLE_SQL,
        ),
        schema_object(
            "table",
            "private_qualification_records",
            PRIVATE_QUALIFICATION_RECORDS_TABLE_SQL,
        ),
        schema_object(
            "table",
            "public_release_gate_records",
            PUBLIC_RELEASE_GATE_RECORDS_TABLE_SQL,
        ),
        schema_object(
            "table",
            "native_platform_qualification_records",
            NATIVE_PLATFORM_QUALIFICATION_RECORDS_TABLE_SQL,
        ),
        schema_object("table", "tender_recoveries", TENDER_RECOVERIES_TABLE_SQL),
        schema_object(
            "table",
            "tender_recovery_decisions",
            TENDER_RECOVERY_DECISIONS_TABLE_SQL,
        ),
        schema_object("table", "update_decisions", UPDATE_DECISIONS_TABLE_SQL),
        schema_object("table", "update_operations", UPDATE_OPERATIONS_TABLE_SQL),
        schema_object(
            "table",
            "update_recovery_points",
            UPDATE_RECOVERY_POINTS_TABLE_SQL,
        ),
        schema_object(
            "trigger",
            "tender_recovery_decisions",
            TENDER_RECOVERY_DECISIONS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "tender_recovery_decisions",
            TENDER_RECOVERY_DECISIONS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "update_decisions",
            UPDATE_DECISIONS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "update_decisions",
            UPDATE_DECISIONS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "update_operations",
            UPDATE_OPERATIONS_OFFER_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "update_operations",
            UPDATE_OPERATIONS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "update_recovery_points",
            UPDATE_RECOVERY_POINTS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "update_recovery_points",
            UPDATE_RECOVERY_POINTS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "portable_tender_archives",
            PORTABLE_TENDER_ARCHIVES_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "portable_tender_archives",
            PORTABLE_TENDER_ARCHIVES_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "deletion_receipts",
            DELETION_RECEIPTS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "deletion_receipts",
            DELETION_RECEIPTS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "provider_cleanup_jobs",
            PROVIDER_CLEANUP_JOBS_IDENTITY_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "provider_cleanup_jobs",
            PROVIDER_CLEANUP_JOBS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "tender_trash_decisions",
            TENDER_TRASH_DECISIONS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "tender_trash_decisions",
            TENDER_TRASH_DECISIONS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "product_acceptance_runs",
            PRODUCT_ACCEPTANCE_RUNS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "product_acceptance_runs",
            PRODUCT_ACCEPTANCE_RUNS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "product_acceptance_records",
            PRODUCT_ACCEPTANCE_RECORDS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "product_acceptance_records",
            PRODUCT_ACCEPTANCE_RECORDS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "live_qualification_runs",
            LIVE_QUALIFICATION_RUNS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "live_qualification_runs",
            LIVE_QUALIFICATION_RUNS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "private_qualification_records",
            PRIVATE_QUALIFICATION_RECORDS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "private_qualification_records",
            PRIVATE_QUALIFICATION_RECORDS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "public_release_gate_records",
            PUBLIC_RELEASE_GATE_RECORDS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "public_release_gate_records",
            PUBLIC_RELEASE_GATE_RECORDS_NO_DELETE_SQL,
        ),
        schema_object(
            "trigger",
            "native_platform_qualification_records",
            NATIVE_PLATFORM_QUALIFICATION_RECORDS_NO_UPDATE_SQL,
        ),
        schema_object(
            "trigger",
            "native_platform_qualification_records",
            NATIVE_PLATFORM_QUALIFICATION_RECORDS_NO_DELETE_SQL,
        ),
    ];
    expected_schema_objects.sort();
    if schema_objects != expected_schema_objects {
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
    let application_settings_row_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM application_settings WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if application_settings_row_count != 1 {
        return Ok(CatalogueStatus::Corrupt);
    }
    let runtime_row_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM runtime_preparation WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if runtime_row_count != 1 {
        return Ok(CatalogueStatus::Corrupt);
    }
    let workspace_selection_row_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM manager_workspace_selection WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    if workspace_selection_row_count != 1 {
        return Ok(CatalogueStatus::Corrupt);
    }

    Ok(CatalogueStatus::Ready)
}

fn finish_with_storage_diagnostics(
    storage_permissions: StoragePermissions,
    setup_performed: bool,
) -> SetupOutcome {
    let mut issues = Vec::new();

    if matches!(storage_permissions, StoragePermissions::Unverified) {
        issues.push(SetupIssue::StoragePermissionsUnverified);
    }

    SetupOutcome::ready(setup_performed, issues)
}

#[cfg(unix)]
fn secure_created_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(windows)]
fn secure_created_directory(path: &Path) -> io::Result<()> {
    use windows_permissions::{
        constants::{SeObjectType, SecurityInformation},
        wrappers::SetNamedSecurityInfo,
        LocalBox, SecurityDescriptor,
    };

    let descriptor: LocalBox<SecurityDescriptor> =
        "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)".parse()?;
    let dacl = descriptor
        .dacl()
        .ok_or_else(|| io::Error::other("private directory DACL is missing"))?;
    SetNamedSecurityInfo(
        path.as_os_str(),
        SeObjectType::SE_FILE_OBJECT,
        SecurityInformation::Dacl | SecurityInformation::ProtectedDacl,
        None,
        None,
        Some(dacl),
        None,
    )
}

#[cfg(not(any(unix, windows)))]
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
