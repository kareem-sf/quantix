use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use garde::Validate;
use rusqlite::{
    backup::StepResult, params, Connection, OpenFlags, Transaction, TransactionBehavior,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use super::*;
use crate::application_settings::AiProviderKind;

const BACKUP_FORMAT_VERSION: u32 = 1;
const MAX_BACKUP_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_BACKUP_DATABASE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_BACKUP_CONTENT_OBJECTS: usize = 100_000;
const MAX_BACKUP_EXPANDED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_BACKUP_ARCHIVE_BYTES: u64 = MAX_BACKUP_EXPANDED_BYTES + 64 * 1024 * 1024;
const MIN_BACKUP_FREE_SPACE_RESERVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BACKUP_OPERATION_DURATION: Duration = Duration::from_secs(30 * 60);

#[derive(Clone, Copy)]
struct StorageOperationBudget {
    deadline: Instant,
}

impl StorageOperationBudget {
    fn for_tender(tender_id: &TenderId) -> Self {
        #[cfg(feature = "runtime-fixture")]
        if std::env::var("QUANTIX_STORAGE_OPERATION_TIMEOUT")
            .ok()
            .and_then(|fixture| {
                fixture.split_once(':').map(|(fixture_tender_id, state)| {
                    (fixture_tender_id.to_owned(), state.to_owned())
                })
            })
            .is_some_and(|(fixture_tender_id, state)| {
                fixture_tender_id == tender_id.as_str() && state == "expired"
            })
        {
            return Self {
                deadline: Instant::now(),
            };
        }
        #[cfg(not(feature = "runtime-fixture"))]
        let _ = tender_id;
        Self {
            deadline: Instant::now()
                .checked_add(MAX_BACKUP_OPERATION_DURATION)
                .expect("the fixed backup operation duration fits Instant"),
        }
    }

    fn check(self) -> Result<(), TenderCommandError> {
        if Instant::now() >= self.deadline {
            Err(TenderCommandError::new(TenderErrorCode::OperationTimedOut))
        } else {
            Ok(())
        }
    }
}

fn lock_with_budget<'a>(
    lock: &'a std::sync::Mutex<()>,
    budget: StorageOperationBudget,
) -> Result<std::sync::MutexGuard<'a, ()>, TenderCommandError> {
    loop {
        match lock.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::WouldBlock) => {
                budget.check()?;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateTenderBackupCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub tender_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderBackupState {
    Creating,
    Ready,
    Failed,
}

impl TenderBackupState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Creating => "creating",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "creating" => Ok(Self::Creating),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderBackupRecord {
    pub backup_id: String,
    pub tender_id: String,
    pub state: TenderBackupState,
    pub source: Option<TenderSummary>,
    pub content_object_count: u64,
    pub manifest_sha256: Option<String>,
    pub archive_size_bytes: Option<u64>,
    pub diagnostic_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PrepareTenderRecoveryCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub backup_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecoveryDecision {
    ApproveReplacement,
    Reject,
}

impl TenderRecoveryDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::ApproveReplacement => "approve_replacement",
            Self::Reject => "reject",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "approve_replacement" => Ok(Self::ApproveReplacement),
            "reject" => Ok(Self::Reject),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecoveryDecisionRecord {
    pub decision: TenderRecoveryDecision,
    pub rationale: String,
    pub decided_by: String,
    pub manifest_sha256: String,
    pub current_audit_chain_head: Option<String>,
    pub decided_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ResolveTenderRecoveryCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub recovery_id: String,
    #[garde(skip)]
    pub decision: TenderRecoveryDecision,
    #[garde(length(bytes, min = 1, max = 500))]
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecoveryState {
    Preparing,
    AwaitingApproval,
    Applying,
    Applied,
    Rejected,
    Failed,
}

impl TenderRecoveryState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::AwaitingApproval => "awaiting_approval",
            Self::Applying => "applying",
            Self::Applied => "applied",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "preparing" => Ok(Self::Preparing),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "applying" => Ok(Self::Applying),
            "applied" => Ok(Self::Applied),
            "rejected" => Ok(Self::Rejected),
            "failed" => Ok(Self::Failed),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecoveryRecord {
    pub recovery_id: String,
    pub tender_id: String,
    pub backup_id: String,
    pub state: TenderRecoveryState,
    pub backup_source: Option<TenderSummary>,
    pub current_source: Option<TenderSummary>,
    pub diagnostic_code: Option<String>,
    pub decision_record: Option<TenderRecoveryDecisionRecord>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreatePortableTenderArchiveCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ImportPortableTenderArchiveCommand {
    #[garde(length(bytes, min = 1, max = 32767))]
    pub source_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct TenderRetentionDecisionCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct TrashedTenderDecisionCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub deletion_id: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PurgeTrashedTenderCommand {
    #[garde(length(bytes, min = 32, max = 32), ascii)]
    pub deletion_id: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
    #[garde(length(bytes, min = 1, max = 200))]
    pub confirmation_tender_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRetentionState {
    Active,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PortableTenderArchiveRecord {
    pub archive_id: String,
    pub tender_id: String,
    pub backup_id: String,
    pub relative_path: String,
    pub source: TenderSummary,
    pub database_sha256: String,
    pub content_object_count: u64,
    pub audit_event_count: u64,
    pub audit_chain_head: String,
    pub tender_schema_version: i64,
    pub quantix_version: String,
    pub manifest_sha256: String,
    pub archive_size_bytes: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRetentionDecisionRecord {
    pub decision_id: String,
    pub tender_id: String,
    pub state: TenderRetentionState,
    pub rationale: String,
    pub decided_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub decided_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TrashedTenderState {
    Moving,
    Trashed,
    Restoring,
    Purging,
    Restored,
    Purged,
    Failed,
}

impl TrashedTenderState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Moving => "moving",
            Self::Trashed => "trashed",
            Self::Restoring => "restoring",
            Self::Purging => "purging",
            Self::Restored => "restored",
            Self::Purged => "purged",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "moving" => Ok(Self::Moving),
            "trashed" => Ok(Self::Trashed),
            "restoring" => Ok(Self::Restoring),
            "purging" => Ok(Self::Purging),
            "restored" => Ok(Self::Restored),
            "purged" => Ok(Self::Purged),
            "failed" => Ok(Self::Failed),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrashedTenderRecord {
    pub deletion_id: String,
    pub tender_id: String,
    pub tender_name: String,
    pub state: TrashedTenderState,
    pub relative_path: String,
    pub rationale: String,
    pub decided_by: String,
    pub acting_role: String,
    pub approval_manifest_sha256: String,
    pub diagnostic_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErasedTenderCopyClass {
    TenderStore,
    TenderBackup,
    PortableTenderArchive,
    DeliveryExport,
    AgentRunWorkspace,
    StagingItem,
    QuarantineItem,
    TenderLog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderCleanupStatus {
    NotRequired,
    Pending,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeletionReceipt {
    pub receipt_id: String,
    pub deletion_id: String,
    pub tender_id: String,
    pub audit_event_count: u64,
    pub audit_chain_head: String,
    pub local_deletion_completed: bool,
    pub erased_copy_classes: Vec<ErasedTenderCopyClass>,
    pub provider_cleanup_status: ProviderCleanupStatus,
    pub provider_thread_count: u32,
    pub confirmed_provider_thread_deletions: u32,
    pub external_copy_exclusions: Vec<String>,
    pub purged_by: String,
    pub acting_role: String,
    pub purged_at: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DeletionReceiptRecord {
    receipt_id: String,
    deletion_id: String,
    tender_id: String,
    audit_event_count: u64,
    audit_chain_head: String,
    local_deletion_completed: bool,
    erased_copy_classes: Vec<ErasedTenderCopyClass>,
    provider_thread_count: u32,
    provider_cleanup_manifest_sha256: String,
    external_copy_exclusions: Vec<String>,
    purged_by: String,
    acting_role: String,
    purged_at: String,
    manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProviderCleanupTarget {
    provider: AiProviderKind,
    thread_ref: String,
}

struct PermanentDeletionPlan {
    erased_copy_classes: Vec<ErasedTenderCopyClass>,
    backup_ids: Vec<String>,
    portable_archive_paths: Vec<String>,
    recovery_ids: Vec<String>,
    provider_cleanup_targets: Vec<ProviderCleanupTarget>,
}

struct PendingProviderCleanup {
    cleanup_id: String,
    provider: AiProviderKind,
    thread_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TrashedTenderLifecycleDecision {
    decision_id: String,
    deletion_id: String,
    tender_id: String,
    action: String,
    rationale: String,
    decided_by: String,
    acting_role: String,
    created_at: String,
    audit_event_count: Option<u64>,
    audit_chain_head: Option<String>,
    erased_copy_classes: Vec<ErasedTenderCopyClass>,
    backup_ids: Vec<String>,
    portable_archive_paths: Vec<String>,
    recovery_ids: Vec<String>,
    provider_cleanup_targets: Vec<ProviderCleanupTarget>,
    manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TenderBackupManifest {
    format_version: u32,
    tender_id: String,
    tender_schema_version: i64,
    quantix_version: String,
    source: TenderSummary,
    database: BackupFileManifest,
    content: Vec<BackupContentManifest>,
    creation: BackupCreationRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BackupFileManifest {
    archive_path: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BackupContentManifest {
    archive_path: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BackupCreationRecord {
    backup_id: String,
    created_at: String,
    created_by: String,
}

struct BackupContentSource {
    manifest: BackupContentManifest,
    integrity: cacache::Integrity,
}

impl TenderStore {
    pub(super) fn retention_boundary_is_safe(&self) -> Result<bool, TenderCommandError> {
        let lifecycle_phase = TenderLifecyclePhase::parse(
            &self
                .connection
                .query_row(
                    "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sql_error)?,
        )?;
        let terminal = match lifecycle_phase {
            TenderLifecyclePhase::Declined => true,
            TenderLifecyclePhase::FinalReview => self
                .connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1
                       FROM submission_package_head AS head
                       JOIN submission_package_versions AS package
                         ON package.package_id = head.package_id
                        AND package.version = head.current_version
                       JOIN submission_release_approvals AS approval
                         ON approval.package_id = head.package_id
                        AND approval.package_version = head.current_version
                        AND approval.package_manifest_sha256 = package.manifest_sha256
                       WHERE head.singleton = 1
                     )",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?,
            _ => false,
        };
        if !terminal {
            return Ok(false);
        }
        let protected_work = self
            .connection
            .query_row(
                "SELECT
                   EXISTS(SELECT 1 FROM agent_runs WHERE status IN ('running', 'indeterminate'))
                   OR EXISTS(
                     SELECT 1 FROM production_tasks
                     WHERE status NOT IN ('ready_for_integration', 'cancelled')
                   )
                   OR EXISTS(
                     SELECT 1 FROM manager_intake_runs
                     WHERE stage IN (
                       'waiting_for_provider', 'package_registered', 'reading_documents',
                       'extracting_tender_facts', 'reviewing_tender_facts',
                       'preparing_first_decision'
                     )
                   )",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
        Ok(!protected_work)
    }

    fn create_verified_backup(
        &self,
        application_home: &Path,
        tender_id: &TenderId,
        backup_id: &str,
        created_at: &str,
        available_space: u64,
        budget: StorageOperationBudget,
    ) -> Result<(TenderBackupRecord, PathBuf), TenderCommandError> {
        budget.check()?;
        let integrity =
            TenderStore::inspect_integrity_with_check(&self.root, tender_id, || budget.check())?;
        if integrity.state != TenderIntegrityState::Ready {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }

        let source = self.summary()?;
        let staging_parent = application_home.join("staging");
        let staging = staging_parent.join(format!("backup-{backup_id}"));
        fs::create_dir(&staging).map_err(store_unavailable)?;

        let result = (|| {
            let snapshot_path = staging.join("tender.sqlite");
            create_sqlite_snapshot(&self.connection, &snapshot_path, budget)?;
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&snapshot_path)
                .and_then(|file| file.sync_all())
                .map_err(store_unavailable)?;

            let (database_sha256, database_size_bytes) = sha256_file(&snapshot_path, budget)?;
            if database_size_bytes == 0 || database_size_bytes > MAX_BACKUP_DATABASE_BYTES {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let database = BackupFileManifest {
                archive_path: "tender.sqlite".into(),
                sha256: database_sha256,
                size_bytes: database_size_bytes,
            };
            let content = self.backup_content(budget)?;
            let content_size_bytes = content.iter().try_fold(0_u64, |total, entry| {
                total
                    .checked_add(entry.manifest.size_bytes)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
            })?;
            let expanded_size_bytes = database
                .size_bytes
                .checked_add(content_size_bytes)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if expanded_size_bytes > MAX_BACKUP_EXPANDED_BYTES {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let required_space = database
                .size_bytes
                .checked_add(expanded_size_bytes)
                .and_then(|bytes| bytes.checked_add(MIN_BACKUP_FREE_SPACE_RESERVE_BYTES))
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if available_space < required_space {
                return Err(TenderCommandError::new(TenderErrorCode::InsufficientSpace));
            }
            let manifest = TenderBackupManifest {
                format_version: BACKUP_FORMAT_VERSION,
                tender_id: tender_id.as_str().to_owned(),
                tender_schema_version: TENDER_SCHEMA_VERSION,
                quantix_version: env!("CARGO_PKG_VERSION").into(),
                source: source.clone(),
                database,
                content: content.iter().map(|entry| entry.manifest.clone()).collect(),
                creation: BackupCreationRecord {
                    backup_id: backup_id.to_owned(),
                    created_at: created_at.to_owned(),
                    created_by: "engineer_user".into(),
                },
            };
            let manifest_json = serde_json_canonicalizer::to_string(&manifest)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if manifest_json.len() as u64 > MAX_BACKUP_MANIFEST_BYTES {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
            let candidate = staging.join("backup.qtbackup");
            write_backup_archive(
                &candidate,
                manifest_json.as_bytes(),
                &snapshot_path,
                &self.root.join("content"),
                &content,
                budget,
            )?;
            verify_backup_archive(&candidate, &manifest, budget)?;
            storage_publication_failpoint("backup_after_verify");
            let archive_size_bytes = fs::metadata(&candidate).map_err(store_unavailable)?.len();
            Ok((
                TenderBackupRecord {
                    backup_id: backup_id.to_owned(),
                    tender_id: tender_id.as_str().to_owned(),
                    state: TenderBackupState::Ready,
                    source: Some(source),
                    content_object_count: manifest.content.len() as u64,
                    manifest_sha256: Some(manifest_sha256),
                    archive_size_bytes: Some(archive_size_bytes),
                    diagnostic_code: None,
                    created_at: created_at.to_owned(),
                },
                candidate,
            ))
        })();

        if result.is_err() {
            let _ = remove_verified_directory(&staging_parent, &staging);
        }
        result
    }

    fn backup_content(
        &self,
        budget: StorageOperationBudget,
    ) -> Result<Vec<BackupContentSource>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare("SELECT sha256, integrity, size_bytes FROM content_objects ORDER BY sha256")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(sql_error)?;
        let mut content = Vec::new();
        for row in rows {
            budget.check()?;
            if content.len() >= MAX_BACKUP_CONTENT_OBJECTS {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (sha256, integrity, size_bytes) = row.map_err(sql_error)?;
            let size_bytes = u64::try_from(size_bytes)
                .ok()
                .filter(|size| *size > 0 && *size <= MAX_CONTENT_BYTES as u64)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let integrity = integrity
                .parse::<cacache::Integrity>()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let mut reader =
                cacache::SyncReader::open_hash(self.root.join("content"), integrity.clone())
                    .map_err(content_store_error)?;
            let (actual_sha256, actual_size_bytes) =
                sha256_reader(&mut reader, size_bytes, budget)?;
            if actual_size_bytes != size_bytes || actual_sha256 != sha256 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            content.push(BackupContentSource {
                manifest: BackupContentManifest {
                    archive_path: format!("content/{sha256}"),
                    sha256,
                    size_bytes,
                },
                integrity,
            });
        }
        Ok(content)
    }
}

impl QuantixHost {
    pub fn create_tender_backup(
        &self,
        command: CreateTenderBackupCommand,
    ) -> Result<TenderBackupRecord, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = StorageOperationBudget::for_tender(&tender_id);
        let _operation_guard = lock_with_budget(self.recovery_operation_lock(), budget)?;
        if self.update_installation_is_active() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let backup_id = self.installation_identifier()?;
        let created_at = self.installation_timestamp()?;
        let backups_root = self.application_home().join("backups");
        let archive = backups_root.join(format!("{backup_id}.qtbackup"));
        let staging_parent = self.application_home().join("staging");
        let staging = staging_parent.join(format!("backup-{backup_id}"));
        self.insert_backup_record(&TenderBackupRecord {
            backup_id: backup_id.clone(),
            tender_id: tender_id.as_str().to_owned(),
            state: TenderBackupState::Creating,
            source: None,
            content_object_count: 0,
            manifest_sha256: None,
            archive_size_bytes: None,
            diagnostic_code: None,
            created_at: created_at.clone(),
        })?;
        let result = (|| {
            budget.check()?;
            let report = self.inspect_tender_integrity_with_check(&tender_id, || budget.check())?;
            if report.state != TenderIntegrityState::Ready {
                return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
            }
            let available_space = backup_available_space(self, &backups_root, &tender_id)?;
            let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
            let (record, candidate) = lock_mutex_with_check(&store, &mut || budget.check())?
                .create_verified_backup(
                    self.application_home(),
                    &tender_id,
                    &backup_id,
                    &created_at,
                    available_space,
                    budget,
                )?;
            if archive.exists() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            fs::rename(&candidate, &archive).map_err(store_unavailable)?;
            storage_publication_failpoint("backup_after_publish");
            remove_verified_directory(&staging_parent, &staging)?;
            if let Err(error) = self.update_backup_record(&record, TenderBackupState::Creating) {
                let _ = fs::remove_file(&archive);
                return Err(error);
            }
            Ok(record)
        })();

        match result {
            Ok(record) => Ok(record),
            Err(error) => {
                let _ = fs::remove_file(&archive);
                let _ = remove_verified_directory(&staging_parent, &staging);
                self.update_backup_record(
                    &TenderBackupRecord {
                        backup_id,
                        tender_id: tender_id.as_str().to_owned(),
                        state: TenderBackupState::Failed,
                        source: None,
                        content_object_count: 0,
                        manifest_sha256: None,
                        archive_size_bytes: None,
                        diagnostic_code: Some(tender_error_code(error.code).into()),
                        created_at,
                    },
                    TenderBackupState::Creating,
                )?;
                Err(error)
            }
        }
    }

    pub fn inspect_tender_backups(
        &self,
        tender_id: &str,
    ) -> Result<Vec<TenderBackupRecord>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        self.inspect_tender_backups_for_update(&tender_id)
    }

    pub(crate) fn inspect_tender_backups_for_update(
        &self,
        tender_id: &TenderId,
    ) -> Result<Vec<TenderBackupRecord>, TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let mut statement = connection
            .prepare(
                "SELECT backup_id, state, source_json, content_object_count,
                        manifest_sha256, archive_size_bytes, diagnostic_code, created_at
                 FROM tender_backups WHERE tender_id = ?1 ORDER BY created_at, backup_id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([tender_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(sql_error)?;
        let mut records = Vec::new();
        for row in rows {
            let (
                backup_id,
                state,
                source_json,
                content_object_count,
                manifest_sha256,
                archive_size_bytes,
                diagnostic_code,
                created_at,
            ) = row.map_err(sql_error)?;
            records.push(TenderBackupRecord {
                backup_id,
                tender_id: tender_id.as_str().to_owned(),
                state: TenderBackupState::parse(&state)?,
                source: source_json
                    .map(|value| serde_json::from_str(&value))
                    .transpose()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                content_object_count: content_object_count
                    .try_into()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                manifest_sha256,
                archive_size_bytes: archive_size_bytes
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                diagnostic_code,
                created_at,
            });
        }
        Ok(records)
    }

    pub(crate) fn has_exact_verified_backup_for_update(
        &self,
        tender_id: &TenderId,
        expected_source: &TenderSummary,
    ) -> Result<bool, TenderCommandError> {
        let records = self.inspect_tender_backups_for_update(tender_id)?;
        let budget = StorageOperationBudget::for_tender(tender_id);
        for record in records.into_iter().filter(|record| {
            record.state == TenderBackupState::Ready
                && record.source.as_ref() == Some(expected_source)
        }) {
            budget.check()?;
            let archive = self
                .application_home()
                .join("backups")
                .join(format!("{}.qtbackup", record.backup_id));
            if bind_verified_backup_archive(&archive, tender_id, &record, budget).is_ok() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn prepare_tender_recovery(
        &self,
        command: PrepareTenderRecoveryCommand,
    ) -> Result<TenderRecoveryRecord, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.backup_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let budget = StorageOperationBudget::for_tender(&tender_id);
        let _operation_guard = lock_with_budget(self.recovery_operation_lock(), budget)?;
        if self.update_installation_is_active() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let backup = self
            .inspect_tender_backups(tender_id.as_str())?
            .into_iter()
            .find(|record| record.backup_id == command.backup_id)
            .filter(|record| record.state == TenderBackupState::Ready)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let recovery_id = self.installation_identifier()?;
        let created_at = self.installation_timestamp()?;
        let current_source = inspect_current_summary(self.application_home(), &tender_id);
        if inspect_current_identity(self.application_home(), &tender_id).as_deref()
            != Some(tender_id.as_str())
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        self.insert_recovery_record(&TenderRecoveryRecord {
            recovery_id: recovery_id.clone(),
            tender_id: tender_id.as_str().to_owned(),
            backup_id: backup.backup_id.clone(),
            state: TenderRecoveryState::Preparing,
            backup_source: backup.source.clone(),
            current_source: current_source.clone(),
            diagnostic_code: None,
            decision_record: None,
            created_at: created_at.clone(),
        })?;
        let staging_parent = self.application_home().join("staging");
        let staging = staging_parent.join(format!("recovery-{recovery_id}"));
        let available_space = recovery_available_space(self, &staging_parent, &tender_id)?;
        let archive = self
            .application_home()
            .join("backups")
            .join(format!("{}.qtbackup", backup.backup_id));
        let prepared = (|| {
            fs::create_dir(&staging).map_err(store_unavailable)?;
            extract_verified_recovery_candidate(
                &archive,
                &staging,
                &tender_id,
                &backup,
                available_space,
                budget,
            )
        })();
        let backup_source = match prepared {
            Ok(source) => {
                storage_publication_failpoint("recovery_after_verify");
                source
            }
            Err(error) => {
                let _ = remove_verified_directory(&staging_parent, &staging);
                self.update_recovery_record(
                    &TenderRecoveryRecord {
                        recovery_id,
                        tender_id: tender_id.as_str().to_owned(),
                        backup_id: backup.backup_id,
                        state: TenderRecoveryState::Failed,
                        backup_source: backup.source,
                        current_source,
                        diagnostic_code: Some(tender_error_code(error.code).into()),
                        decision_record: None,
                        created_at,
                    },
                    TenderRecoveryState::Preparing,
                )?;
                return Err(error);
            }
        };
        let record = TenderRecoveryRecord {
            recovery_id,
            tender_id: tender_id.as_str().to_owned(),
            backup_id: backup.backup_id,
            state: TenderRecoveryState::AwaitingApproval,
            backup_source: Some(backup_source),
            current_source,
            diagnostic_code: None,
            decision_record: None,
            created_at,
        };
        if let Err(error) = self.update_recovery_record(&record, TenderRecoveryState::Preparing) {
            let _ = remove_verified_directory(&staging_parent, &staging);
            let mut failed = record;
            failed.state = TenderRecoveryState::Failed;
            failed.diagnostic_code = Some(tender_error_code(error.code).into());
            self.update_recovery_record(&failed, TenderRecoveryState::Preparing)?;
            return Err(error);
        }
        Ok(record)
    }

    pub fn inspect_tender_recoveries(
        &self,
        tender_id: &str,
    ) -> Result<Vec<TenderRecoveryRecord>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let mut statement = connection
            .prepare(
                "SELECT r.recovery_id, r.backup_id, r.state, r.backup_source_json,
                        r.current_source_json, r.diagnostic_code, r.created_at,
                        d.decision, d.rationale, d.decided_by, d.manifest_sha256,
                        d.current_audit_chain_head, d.decided_at
                 FROM tender_recoveries r
                 LEFT JOIN tender_recovery_decisions d ON d.recovery_id = r.recovery_id
                 WHERE r.tender_id = ?1 ORDER BY r.created_at, r.recovery_id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([tender_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                ))
            })
            .map_err(sql_error)?;
        let mut records = Vec::new();
        for row in rows {
            let (
                recovery_id,
                backup_id,
                state,
                backup_source_json,
                current_source_json,
                diagnostic_code,
                created_at,
                decision,
                rationale,
                decided_by,
                manifest_sha256,
                current_audit_chain_head,
                decided_at,
            ) = row.map_err(sql_error)?;
            let decision_record = decode_recovery_decision(
                decision,
                rationale,
                decided_by,
                manifest_sha256,
                current_audit_chain_head,
                decided_at,
            )?;
            records.push(TenderRecoveryRecord {
                recovery_id,
                tender_id: tender_id.as_str().to_owned(),
                backup_id,
                state: TenderRecoveryState::parse(&state)?,
                backup_source: decode_summary(backup_source_json)?,
                current_source: decode_summary(current_source_json)?,
                diagnostic_code,
                decision_record,
                created_at,
            });
        }
        Ok(records)
    }

    pub fn resolve_tender_recovery(
        &self,
        command: ResolveTenderRecoveryCommand,
    ) -> Result<TenderRecoveryRecord, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.recovery_id) || command.rationale.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let budget = StorageOperationBudget::for_tender(&tender_id);
        let _operation_guard = lock_with_budget(self.recovery_operation_lock(), budget)?;
        if self.update_installation_is_active() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let record = self
            .inspect_tender_recoveries(tender_id.as_str())?
            .into_iter()
            .find(|record| record.recovery_id == command.recovery_id)
            .filter(|record| record.state == TenderRecoveryState::AwaitingApproval)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let backup = self
            .inspect_tender_backups(tender_id.as_str())?
            .into_iter()
            .find(|backup| {
                backup.backup_id == record.backup_id && backup.state == TenderBackupState::Ready
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let rationale = command.rationale.trim();
        match command.decision {
            TenderRecoveryDecision::Reject => {
                budget.check()?;
                let staging = self
                    .application_home()
                    .join("staging")
                    .join(format!("recovery-{}", record.recovery_id));
                let rejected = self.decide_recovery_record(
                    record,
                    TenderRecoveryState::Rejected,
                    TenderRecoveryDecision::Reject,
                    rationale,
                    &backup,
                )?;
                budget.check()?;
                remove_verified_directory(&self.application_home().join("staging"), &staging)?;
                Ok(rejected)
            }
            TenderRecoveryDecision::ApproveReplacement => {
                self.apply_recovery_candidate(&tender_id, record, backup, rationale, budget)
            }
        }
    }

    fn insert_backup_record(&self, record: &TenderBackupRecord) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let source_json = record
            .source
            .as_ref()
            .map(serde_json_canonicalizer::to_string)
            .transpose()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        transaction
            .execute(
                "INSERT INTO tender_backups (
                   backup_id, tender_id, state, source_json, content_object_count,
                   manifest_sha256, archive_size_bytes, diagnostic_code, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    record.backup_id,
                    record.tender_id,
                    record.state.as_str(),
                    source_json,
                    i64::try_from(record.content_object_count)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    record.manifest_sha256,
                    record
                        .archive_size_bytes
                        .map(i64::try_from)
                        .transpose()
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    record.diagnostic_code,
                    record.created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)
    }

    fn update_backup_record(
        &self,
        record: &TenderBackupRecord,
        expected_state: TenderBackupState,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let source_json = encode_summary(record.source.as_ref())?;
        let changed = transaction
            .execute(
                "UPDATE tender_backups
                 SET state = ?2, source_json = ?3, content_object_count = ?4,
                     manifest_sha256 = ?5, archive_size_bytes = ?6, diagnostic_code = ?7
                 WHERE backup_id = ?1 AND state = ?8",
                params![
                    record.backup_id,
                    record.state.as_str(),
                    source_json,
                    i64::try_from(record.content_object_count)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    record.manifest_sha256,
                    record
                        .archive_size_bytes
                        .map(i64::try_from)
                        .transpose()
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    record.diagnostic_code,
                    expected_state.as_str(),
                ],
            )
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        transaction.commit().map_err(sql_error)
    }

    fn insert_recovery_record(
        &self,
        record: &TenderRecoveryRecord,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let backup_source_json = encode_summary(record.backup_source.as_ref())?;
        let current_source_json = encode_summary(record.current_source.as_ref())?;
        transaction
            .execute(
                "INSERT INTO tender_recoveries (
                   recovery_id, tender_id, backup_id, state, backup_source_json,
                   current_source_json, diagnostic_code, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    record.recovery_id,
                    record.tender_id,
                    record.backup_id,
                    record.state.as_str(),
                    backup_source_json,
                    current_source_json,
                    record.diagnostic_code,
                    record.created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)
    }

    fn update_recovery_record(
        &self,
        record: &TenderRecoveryRecord,
        expected_state: TenderRecoveryState,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let changed = transaction
            .execute(
                "UPDATE tender_recoveries
                 SET state = ?2, backup_source_json = ?3, current_source_json = ?4,
                     diagnostic_code = ?5
                 WHERE recovery_id = ?1 AND state = ?6",
                params![
                    record.recovery_id,
                    record.state.as_str(),
                    encode_summary(record.backup_source.as_ref())?,
                    encode_summary(record.current_source.as_ref())?,
                    record.diagnostic_code,
                    expected_state.as_str(),
                ],
            )
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        transaction.commit().map_err(sql_error)
    }

    fn decide_recovery_record(
        &self,
        mut record: TenderRecoveryRecord,
        state: TenderRecoveryState,
        decision: TenderRecoveryDecision,
        rationale: &str,
        backup: &TenderBackupRecord,
    ) -> Result<TenderRecoveryRecord, TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let decided_at: String = transaction
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        let manifest_sha256 = backup
            .manifest_sha256
            .as_deref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let current_audit_chain_head = record
            .current_source
            .as_ref()
            .map(|source| source.audit_chain_head.as_str());
        transaction
            .execute(
                "INSERT INTO tender_recovery_decisions (
                   recovery_id, decision, rationale, decided_by, manifest_sha256,
                   current_audit_chain_head, decided_at
                 ) VALUES (?1, ?2, ?3, 'engineer_user', ?4, ?5, ?6)",
                params![
                    record.recovery_id,
                    decision.as_str(),
                    rationale,
                    manifest_sha256,
                    current_audit_chain_head,
                    decided_at,
                ],
            )
            .map_err(sql_error)?;
        if transaction
            .execute(
                "UPDATE tender_recoveries
                 SET state = ?2
                 WHERE recovery_id = ?1 AND state = 'awaiting_approval'",
                params![record.recovery_id, state.as_str()],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        transaction.commit().map_err(sql_error)?;
        record.state = state;
        record.decision_record = Some(TenderRecoveryDecisionRecord {
            decision,
            rationale: rationale.to_owned(),
            decided_by: "engineer_user".into(),
            manifest_sha256: manifest_sha256.into(),
            current_audit_chain_head: current_audit_chain_head.map(str::to_owned),
            decided_at,
        });
        Ok(record)
    }

    fn apply_recovery_candidate(
        &self,
        tender_id: &TenderId,
        record: TenderRecoveryRecord,
        backup: TenderBackupRecord,
        rationale: &str,
        budget: StorageOperationBudget,
    ) -> Result<TenderRecoveryRecord, TenderCommandError> {
        let staging_parent = self.application_home().join("staging");
        let staging = staging_parent.join(format!("recovery-{}", record.recovery_id));
        let archive = self
            .application_home()
            .join("backups")
            .join(format!("{}.qtbackup", backup.backup_id));
        let candidate_error = bind_verified_backup_archive(&archive, tender_id, &backup, budget)
            .and_then(|manifest| {
                verify_recovery_candidate_directory(&staging, tender_id, &manifest, budget)
            })
            .err();
        if let Some(error) = candidate_error {
            remove_verified_directory(&staging_parent, &staging)?;
            let mut failed = record;
            failed.state = TenderRecoveryState::Failed;
            failed.diagnostic_code = Some(tender_error_code(error.code).into());
            self.update_recovery_record(&failed, TenderRecoveryState::AwaitingApproval)?;
            return Err(error);
        }

        let was_recovery_required = {
            let mut recovery_required = self
                .recovery_required_tenders()
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            let was_recovery_required = recovery_required.contains(tender_id);
            recovery_required.insert(tender_id.clone());
            let mut stores = self
                .open_tender_stores()
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            if stores
                .get(tender_id)
                .is_some_and(|store| Arc::strong_count(store) != 1)
            {
                if !was_recovery_required {
                    recovery_required.remove(tender_id);
                }
                return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
            }
            stores.remove(tender_id);
            was_recovery_required
        };

        let current = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let retained = self
            .application_home()
            .join("trash")
            .join(format!("recovery-replaced-{}", record.recovery_id));
        let current_source = inspect_current_summary(self.application_home(), tender_id);
        let current_identity = inspect_current_identity(self.application_home(), tender_id);
        if current_source != record.current_source
            || current_identity.as_deref() != Some(tender_id.as_str())
        {
            remove_verified_directory(&staging_parent, &staging)?;
            let mut failed = record;
            failed.state = TenderRecoveryState::Failed;
            failed.diagnostic_code = Some("current_changed".into());
            self.update_recovery_record(&failed, TenderRecoveryState::AwaitingApproval)?;
            if !was_recovery_required {
                self.recovery_required_tenders()
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .remove(tender_id);
            }
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if let Err(error) = validate_recovery_swap_directory(&current)
            .and_then(|()| validate_recovery_swap_directory(&staging))
        {
            if !was_recovery_required {
                self.recovery_required_tenders()
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .remove(tender_id);
            }
            return Err(error);
        }
        if retained.exists() {
            if !was_recovery_required {
                self.recovery_required_tenders()
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .remove(tender_id);
            }
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let applying = match self.decide_recovery_record(
            record,
            TenderRecoveryState::Applying,
            TenderRecoveryDecision::ApproveReplacement,
            rationale,
            &backup,
        ) {
            Ok(applying) => applying,
            Err(error) => {
                if !was_recovery_required {
                    self.recovery_required_tenders()
                        .lock()
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                        .remove(tender_id);
                }
                return Err(error);
            }
        };
        let applied = (|| {
            fs::rename(&current, &retained).map_err(store_unavailable)?;
            storage_publication_failpoint("recovery_after_current_retained");
            if recovery_io_failure(tender_id, "publish_and_restore") {
                return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
            }
            if let Err(error) = fs::rename(&staging, &current) {
                fs::rename(&retained, &current).map_err(store_unavailable)?;
                return Err(store_unavailable(error));
            }
            storage_publication_failpoint("recovery_after_publish");
            if recovery_io_failure(tender_id, "damage_after_publish") {
                fs::OpenOptions::new()
                    .append(true)
                    .open(current.join("tender.sqlite"))
                    .map_err(store_unavailable)?
                    .write_all(b"post-publication-corruption")
                    .map_err(store_unavailable)?;
            }
            let manifest = bind_verified_backup_archive(&archive, tender_id, &backup, budget)?;
            verify_recovery_candidate_directory(&current, tender_id, &manifest, budget)?;
            let mut completed = applying.clone();
            completed.state = TenderRecoveryState::Applied;
            self.update_recovery_record(&completed, TenderRecoveryState::Applying)?;
            Ok(completed)
        })();
        let mut recovery_required = self
            .recovery_required_tenders()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if applied.is_ok() {
            recovery_required.remove(tender_id);
        }
        if let Err(error) = &applied {
            let current_exists = current.try_exists().map_err(store_unavailable)?;
            let staging_exists = staging.try_exists().map_err(store_unavailable)?;
            let retained_exists = retained.try_exists().map_err(store_unavailable)?;
            let restored = if current_exists && staging_exists && !retained_exists {
                remove_verified_directory(&staging_parent, &staging)?;
                true
            } else if current_exists && !staging_exists && retained_exists {
                restore_retained_tender(&current, &retained, &staging_parent, &staging)?;
                true
            } else {
                false
            };
            if restored {
                let mut failed = applying;
                failed.state = TenderRecoveryState::Failed;
                failed.diagnostic_code = Some(tender_error_code(error.code).into());
                self.update_recovery_record(&failed, TenderRecoveryState::Applying)?;
                if !was_recovery_required {
                    recovery_required.remove(tender_id);
                }
            }
        }
        applied
    }

    fn installation_identifier(&self) -> Result<String, TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?
            .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
            .map_err(sql_error)
    }

    fn installation_timestamp(&self) -> Result<String, TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)
    }
}

impl QuantixHost {
    pub fn create_portable_tender_archive(
        &self,
        command: CreatePortableTenderArchiveCommand,
    ) -> Result<PortableTenderArchiveRecord, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let backup = self.create_tender_backup(CreateTenderBackupCommand {
            tender_id: tender_id.as_str().into(),
        })?;
        if backup.state != TenderBackupState::Ready {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let budget = StorageOperationBudget::for_tender(&tender_id);
        let source = self
            .application_home()
            .join("backups")
            .join(format!("{}.qtbackup", backup.backup_id));
        let manifest = bind_verified_backup_archive(&source, &tender_id, &backup, budget)?;
        let archive_id = self.installation_identifier()?;
        let relative_path = format!("{archive_id}.qtarchive");
        let destination = self
            .application_home()
            .join("archives")
            .join(&relative_path);
        copy_file_verified(&source, &destination, budget)?;
        bind_verified_backup_archive(&destination, &tender_id, &backup, budget)?;
        let archive_size_bytes = fs::metadata(&destination).map_err(store_unavailable)?.len();
        let manifest_json = serde_json_canonicalizer::to_string(&manifest)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let record = PortableTenderArchiveRecord {
            archive_id,
            tender_id: tender_id.as_str().into(),
            backup_id: backup.backup_id,
            relative_path,
            source: manifest.source.clone(),
            database_sha256: manifest.database.sha256.clone(),
            content_object_count: manifest.content.len() as u64,
            audit_event_count: manifest.source.audit_event_count,
            audit_chain_head: manifest.source.audit_chain_head.clone(),
            tender_schema_version: manifest.tender_schema_version,
            quantix_version: manifest.quantix_version.clone(),
            manifest_sha256: sha256_hex(manifest_json.as_bytes()),
            archive_size_bytes,
            created_at: manifest.creation.created_at,
        };
        if let Err(error) = self.insert_portable_archive(&record) {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        Ok(record)
    }

    pub fn inspect_portable_tender_archives(
        &self,
        tender_id: &str,
    ) -> Result<Vec<PortableTenderArchiveRecord>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let mut statement = connection
            .prepare(
                "SELECT archive_json FROM portable_tender_archives
                 WHERE tender_id = ?1 ORDER BY created_at, archive_id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([tender_id.as_str()], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut records = Vec::new();
        for row in rows {
            records.push(parse_canonical_record(&row.map_err(sql_error)?)?);
        }
        Ok(records)
    }

    pub fn import_portable_tender_archive(
        &self,
        command: ImportPortableTenderArchiveCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let source = Path::new(&command.source_path);
        if !source.is_absolute()
            || fs::symlink_metadata(source)
                .map(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
                .unwrap_or(true)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let provisional_budget = StorageOperationBudget {
            deadline: Instant::now()
                .checked_add(MAX_BACKUP_OPERATION_DURATION)
                .expect("fixed archive duration fits Instant"),
        };
        let manifest = read_backup_manifest(source, provisional_budget)?;
        let tender_id = TenderId::parse(&manifest.tender_id)?;
        let budget = StorageOperationBudget::for_tender(&tender_id);
        self.ensure_tender_identity_available(&tender_id, None)?;
        let manifest_json = serde_json_canonicalizer::to_string(&manifest)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let record = TenderBackupRecord {
            backup_id: manifest.creation.backup_id.clone(),
            tender_id: tender_id.as_str().into(),
            state: TenderBackupState::Ready,
            source: Some(manifest.source.clone()),
            content_object_count: manifest.content.len() as u64,
            manifest_sha256: Some(sha256_hex(manifest_json.as_bytes())),
            archive_size_bytes: Some(fs::metadata(source).map_err(store_unavailable)?.len()),
            diagnostic_code: None,
            created_at: manifest.creation.created_at.clone(),
        };
        bind_verified_backup_archive(source, &tender_id, &record, budget)?;
        let staging = self
            .application_home()
            .join("staging")
            .join(format!("archive-import-{}", tender_id.as_str()));
        let final_root = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        if staging.exists() {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
        fs::create_dir(&staging).map_err(store_unavailable)?;
        let available = fs4::available_space(
            staging
                .parent()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        )
        .map_err(store_unavailable)?;
        let imported = extract_verified_recovery_candidate(
            source, &staging, &tender_id, &record, available, budget,
        );
        let summary = match imported {
            Ok(summary) => summary,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
        };
        if let Err(error) = fs::rename(&staging, &final_root) {
            let _ = fs::remove_dir_all(&staging);
            return Err(store_unavailable(error));
        }
        let verified =
            TenderStore::inspect_integrity_with_check(&final_root, &tender_id, || budget.check())?;
        if verified.state != TenderIntegrityState::Ready {
            let _ = fs::rename(&final_root, &staging);
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(summary)
    }

    pub fn archive_tender(
        &self,
        command: TenderRetentionDecisionCommand,
    ) -> Result<TenderRetentionDecisionRecord, TenderCommandError> {
        self.set_tender_retention(command, TenderRetentionState::Archived)
    }

    pub fn restore_archived_tender(
        &self,
        command: TenderRetentionDecisionCommand,
    ) -> Result<TenderRetentionDecisionRecord, TenderCommandError> {
        self.set_tender_retention(command, TenderRetentionState::Active)
    }

    pub fn inspect_tender_retention(
        &self,
        tender_id: &str,
    ) -> Result<TenderRetentionState, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let archived = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .archived;
        Ok(if archived {
            TenderRetentionState::Archived
        } else {
            TenderRetentionState::Active
        })
    }

    pub fn trash_tender(
        &self,
        command: TenderRetentionDecisionCommand,
    ) -> Result<TrashedTenderRecord, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.rationale.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let tender_id = TenderId::parse(&command.tender_id)?;
        let integrity = self.inspect_tender_integrity(tender_id.as_str())?;
        if integrity.state != TenderIntegrityState::Ready {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }
        let store = self.tender_store(&tender_id)?;
        let tender_name = {
            let store = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            if !store.retention_boundary_is_safe()? {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            store.summary()?.name
        };
        let deletion_id = self.installation_identifier()?;
        let created_at = self.installation_timestamp()?;
        let relative_path = format!("{}-{}", tender_id.as_str(), deletion_id);
        let mut record = TrashedTenderRecord {
            deletion_id,
            tender_id: tender_id.as_str().into(),
            tender_name,
            state: TrashedTenderState::Moving,
            relative_path,
            rationale: command.rationale.trim().into(),
            decided_by: "engineer_user".into(),
            acting_role: "tendering_engineer".into(),
            approval_manifest_sha256: String::new(),
            diagnostic_code: None,
            created_at: created_at.clone(),
            updated_at: created_at,
        };
        record.approval_manifest_sha256 = manifest_sha256_record(&record)?;
        self.insert_trash_record(&record)?;
        {
            let mut store = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            if !store.retention_boundary_is_safe()? {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let transaction = store
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let summary = store_summary_from_connection(&transaction)?;
            let decision_time = sqlite_timestamp(&transaction)?;
            append_audit_event_with_sequence(
                &transaction,
                tender_id.as_str(),
                "tender_deletion_approved",
                summary.revision,
                serde_json::json!({
                    "approval_manifest_sha256": record.approval_manifest_sha256,
                    "deletion_id": record.deletion_id,
                    "decided_by": record.decided_by,
                }),
                &decision_time,
            )?;
            transaction.commit().map_err(sql_error)?;
        }
        drop(store);
        self.close_tender(tender_id.as_str())?;
        let source = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let destination = self
            .application_home()
            .join("trash")
            .join(&record.relative_path);
        match fs::rename(&source, &destination) {
            Ok(()) => {
                record.state = TrashedTenderState::Trashed;
                if let Ok(updated_at) = self.installation_timestamp() {
                    record.updated_at = updated_at;
                }
                let _ = self.update_trash_record(&record, TrashedTenderState::Moving);
                Ok(record)
            }
            Err(error) => {
                record.state = TrashedTenderState::Failed;
                record.diagnostic_code = Some("move_failed".into());
                record.updated_at = self.installation_timestamp()?;
                self.update_trash_record(&record, TrashedTenderState::Moving)?;
                Err(store_unavailable(error))
            }
        }
    }

    pub fn inspect_trashed_tenders(&self) -> Result<Vec<TrashedTenderRecord>, TenderCommandError> {
        require_setup(self)?;
        self.reconcile_trash_records()?;
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let mut statement = connection
            .prepare("SELECT approval_json, state, diagnostic_code, updated_at FROM tender_trash ORDER BY created_at, deletion_id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sql_error)?;
        let mut records = Vec::new();
        for row in rows {
            let (json, state, diagnostic_code, updated_at) = row.map_err(sql_error)?;
            let mut record: TrashedTenderRecord = parse_canonical_record(&json)?;
            record.state = TrashedTenderState::parse(&state)?;
            record.diagnostic_code = diagnostic_code;
            record.updated_at = updated_at;
            records.push(record);
        }
        Ok(records)
    }

    pub fn restore_trashed_tender(
        &self,
        command: TrashedTenderDecisionCommand,
    ) -> Result<TrashedTenderRecord, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.rationale.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut record = self
            .inspect_trashed_tenders()?
            .into_iter()
            .find(|record| {
                record.deletion_id == command.deletion_id
                    && record.state == TrashedTenderState::Trashed
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&record.tender_id)?;
        self.ensure_tender_identity_available(&tender_id, Some(&record.deletion_id))?;
        let source = self
            .application_home()
            .join("trash")
            .join(&record.relative_path);
        let destination = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let integrity = TenderStore::inspect_integrity_with_check(&source, &tender_id, || Ok(()))?;
        if integrity.state != TenderIntegrityState::Ready {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        record = self.begin_restore_from_trash(record, command.rationale.trim())?;
        if let Err(error) = fs::rename(&source, &destination) {
            return Err(store_unavailable(error));
        }
        record.state = TrashedTenderState::Restored;
        if let Ok(updated_at) = self.installation_timestamp() {
            record.updated_at = updated_at;
        }
        let _ = self.update_trash_record(&record, TrashedTenderState::Restoring);
        Ok(record)
    }

    pub fn purge_trashed_tender(
        &self,
        command: PurgeTrashedTenderCommand,
    ) -> Result<DeletionReceipt, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.rationale.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut record = self
            .inspect_trashed_tenders()?
            .into_iter()
            .find(|record| {
                record.deletion_id == command.deletion_id
                    && record.state == TrashedTenderState::Trashed
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.confirmation_tender_name != record.tender_name {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let tender_id = TenderId::parse(&record.tender_id)?;
        let source = self
            .application_home()
            .join("trash")
            .join(&record.relative_path);
        let integrity = TenderStore::inspect_integrity_with_check(&source, &tender_id, || Ok(()))?;
        if integrity.state != TenderIntegrityState::Ready {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let summary = inspect_summary_at(&source, &tender_id)?;
        let purge_root = self
            .application_home()
            .join("staging")
            .join(format!("purge-{}", record.deletion_id));
        if purge_root.exists() {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
        let plan = self.permanent_deletion_plan(&tender_id, &source)?;
        let (next_record, decision) =
            self.begin_purge_from_trash(record, command.rationale.trim(), &summary, plan)?;
        record = next_record;
        storage_publication_failpoint("purge_after_decision");
        fs::rename(&source, &purge_root).map_err(store_unavailable)?;
        remove_permanent_deletion_files(self.application_home(), &record, &decision)?;
        storage_publication_failpoint("purge_after_local_delete");
        self.complete_purge_from_trash(record)
    }

    pub fn inspect_deletion_receipts(&self) -> Result<Vec<DeletionReceipt>, TenderCommandError> {
        require_setup(self)?;
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let mut statement = connection
            .prepare("SELECT receipt_json FROM deletion_receipts ORDER BY created_at, receipt_id")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut receipts = Vec::new();
        for row in rows {
            let record: DeletionReceiptRecord = parse_canonical_record(&row.map_err(sql_error)?)?;
            if !record.local_deletion_completed
                || manifest_sha256_record(&record)? != record.manifest_sha256
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            receipts.push(deletion_receipt_view(&connection, &record)?);
        }
        Ok(receipts)
    }

    pub(crate) async fn retry_pending_provider_cleanup(&self) -> Result<(), TenderCommandError> {
        let _execution = self.provider_cleanup_execution().lock().await;
        let pending = self.pending_provider_cleanup_jobs()?;
        if pending.is_empty() {
            return Ok(());
        }
        if self.runtime_is_verified()
            && pending
                .iter()
                .any(|job| job.provider == AiProviderKind::Codex)
            && self.agent_provider().lock().await.is_none()
        {
            let _ = self
                .inspect_codex_subscription(tokio_util::sync::CancellationToken::new())
                .await;
        }
        for job in pending {
            let deleted = match job.provider {
                AiProviderKind::Anthropic | AiProviderKind::Gemini => true,
                AiProviderKind::Codex => {
                    let provider = self.agent_provider().lock().await.as_ref().cloned();
                    match provider {
                        Some(provider) => {
                            provider.delete_thread(job.thread_ref.clone()).await.is_ok()
                        }
                        None => false,
                    }
                }
            };
            self.record_provider_cleanup_attempt(&job, deleted)?;
        }
        Ok(())
    }

    fn pending_provider_cleanup_jobs(
        &self,
    ) -> Result<Vec<PendingProviderCleanup>, TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let mut statement = connection
            .prepare(
                "SELECT cleanup_id, provider_kind, thread_ref, target_manifest_sha256
                 FROM provider_cleanup_jobs WHERE status = 'pending'
                 ORDER BY deletion_id, target_ordinal",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sql_error)?;
        let mut jobs = Vec::new();
        for row in rows {
            let (cleanup_id, provider, thread_ref, target_manifest_sha256) =
                row.map_err(sql_error)?;
            let provider = match provider.as_str() {
                "codex" => AiProviderKind::Codex,
                "anthropic" => AiProviderKind::Anthropic,
                "gemini" => AiProviderKind::Gemini,
                _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
            };
            if manifest_sha256_record(&ProviderCleanupTarget {
                provider,
                thread_ref: thread_ref.clone(),
            })? != target_manifest_sha256
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            jobs.push(PendingProviderCleanup {
                cleanup_id,
                provider,
                thread_ref,
            });
        }
        Ok(jobs)
    }

    fn record_provider_cleanup_attempt(
        &self,
        job: &PendingProviderCleanup,
        deleted: bool,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let changed = if deleted {
            connection.execute(
                "UPDATE provider_cleanup_jobs
                 SET status = 'completed', thread_ref = NULL,
                     attempt_count = attempt_count + 1, diagnostic_code = NULL,
                     last_attempt_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE cleanup_id = ?1 AND status = 'pending' AND thread_ref = ?2",
                params![job.cleanup_id, job.thread_ref],
            )
        } else {
            connection.execute(
                "UPDATE provider_cleanup_jobs
                 SET attempt_count = attempt_count + 1,
                     diagnostic_code = 'provider_unavailable',
                     last_attempt_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE cleanup_id = ?1 AND status = 'pending' AND thread_ref = ?2",
                params![job.cleanup_id, job.thread_ref],
            )
        }
        .map_err(sql_error)?;
        if changed != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(())
    }

    fn set_tender_retention(
        &self,
        command: TenderRetentionDecisionCommand,
        state: TenderRetentionState,
    ) -> Result<TenderRetentionDecisionRecord, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.rationale.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let tender_id = TenderId::parse(&command.tender_id)?;
        let integrity = self.inspect_tender_integrity(tender_id.as_str())?;
        if integrity.state != TenderIntegrityState::Ready {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }
        let store = self.tender_store(&tender_id)?;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if state == TenderRetentionState::Archived && !store.retention_boundary_is_safe()? {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if (state == TenderRetentionState::Archived && store.archived)
            || (state == TenderRetentionState::Active && !store.archived)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let transaction = store
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let summary = store_summary_from_connection(&transaction)?;
        let decided_at = sqlite_timestamp(&transaction)?;
        let decision_id = random_identifier(&transaction)?;
        let mut decision = TenderRetentionDecisionRecord {
            decision_id,
            tender_id: tender_id.as_str().into(),
            state,
            rationale: command.rationale.trim().into(),
            decided_by: "engineer_user".into(),
            acting_role: "tendering_engineer".into(),
            manifest_sha256: String::new(),
            decided_at: decided_at.clone(),
        };
        decision.manifest_sha256 = manifest_sha256_record(&decision)?;
        let state_value = match state {
            TenderRetentionState::Active => "active",
            TenderRetentionState::Archived => "archived",
        };
        let changed = transaction
            .execute(
                "UPDATE tender_retention
                 SET state = ?1, decision_id = ?2, decision_manifest_sha256 = ?3, updated_at = ?4
                 WHERE singleton = 1 AND state != ?1",
                params![
                    state_value,
                    decision.decision_id,
                    decision.manifest_sha256,
                    decision.decided_at,
                ],
            )
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            match state {
                TenderRetentionState::Active => "tender_restored_from_archive",
                TenderRetentionState::Archived => "tender_archived_read_only",
            },
            summary.revision,
            serde_json::json!({
                "decision_id": decision.decision_id,
                "manifest_sha256": decision.manifest_sha256,
                "state": state_value,
            }),
            &decided_at,
        )?;
        transaction
            .execute(
                "INSERT INTO tender_retention_decisions (
                   decision_id, state, rationale, decided_by, acting_role,
                   decision_json, manifest_sha256, audit_sequence, decided_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    decision.decision_id,
                    state_value,
                    decision.rationale,
                    decision.decided_by,
                    decision.acting_role,
                    canonical_json_record(&decision)?,
                    decision.manifest_sha256,
                    audit_sequence,
                    decision.decided_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        store.archived = state == TenderRetentionState::Archived;
        Ok(decision)
    }

    fn insert_portable_archive(
        &self,
        record: &PortableTenderArchiveRecord,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?
            .execute(
                "INSERT INTO portable_tender_archives (
                   archive_id, tender_id, backup_id, relative_path, manifest_sha256,
                   archive_size_bytes, archive_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    record.archive_id,
                    record.tender_id,
                    record.backup_id,
                    record.relative_path,
                    record.manifest_sha256,
                    i64::try_from(record.archive_size_bytes)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    canonical_json_record(record)?,
                    record.created_at,
                ],
            )
            .map(|_| ())
            .map_err(sql_error)
    }

    fn begin_restore_from_trash(
        &self,
        mut record: TrashedTenderRecord,
        rationale: &str,
    ) -> Result<TrashedTenderRecord, TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at: String = transaction
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        let mut decision = TrashedTenderLifecycleDecision {
            decision_id: random_identifier(&transaction)?,
            deletion_id: record.deletion_id.clone(),
            tender_id: record.tender_id.clone(),
            action: "restore".into(),
            rationale: rationale.into(),
            decided_by: "engineer_user".into(),
            acting_role: "tendering_engineer".into(),
            created_at: created_at.clone(),
            audit_event_count: None,
            audit_chain_head: None,
            erased_copy_classes: Vec::new(),
            backup_ids: Vec::new(),
            portable_archive_paths: Vec::new(),
            recovery_ids: Vec::new(),
            provider_cleanup_targets: Vec::new(),
            manifest_sha256: String::new(),
        };
        decision.manifest_sha256 = manifest_sha256_record(&decision)?;
        insert_trash_lifecycle_decision(&transaction, &decision)?;
        let changed = transaction
            .execute(
                "UPDATE tender_trash SET state = 'restoring', updated_at = ?2
                 WHERE deletion_id = ?1 AND state = 'trashed'",
                params![record.deletion_id, created_at],
            )
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        transaction.commit().map_err(sql_error)?;
        record.state = TrashedTenderState::Restoring;
        record.updated_at = created_at;
        Ok(record)
    }

    fn permanent_deletion_plan(
        &self,
        tender_id: &TenderId,
        trashed_root: &Path,
    ) -> Result<PermanentDeletionPlan, TenderCommandError> {
        let store_database = Connection::open_with_flags(
            trashed_root.join("tender.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let mut provider_statement = store_database
            .prepare(
                "SELECT DISTINCT json_extract(bindings.binding_json, '$.provider'),
                        runs.provider_thread_ref
                 FROM agent_runs AS runs
                 JOIN agent_run_provider_bindings AS bindings ON bindings.run_id = runs.run_id
                 WHERE runs.provider_thread_ref IS NOT NULL
                 ORDER BY 1, 2",
            )
            .map_err(sql_error)?;
        let provider_rows = provider_statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut provider_cleanup_targets = Vec::new();
        for row in provider_rows {
            let (provider, thread_ref) = row.map_err(sql_error)?;
            let provider = match provider.as_str() {
                "codex" => AiProviderKind::Codex,
                "anthropic" => AiProviderKind::Anthropic,
                "gemini" => AiProviderKind::Gemini,
                _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
            };
            if thread_ref.is_empty() || thread_ref.len() > 1000 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            provider_cleanup_targets.push(ProviderCleanupTarget {
                provider,
                thread_ref,
            });
        }

        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let installation = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let backup_ids = load_string_column(
            &installation,
            "SELECT backup_id FROM tender_backups WHERE tender_id = ?1 ORDER BY backup_id",
            tender_id.as_str(),
        )?;
        let portable_archive_paths = load_string_column(
            &installation,
            "SELECT relative_path FROM portable_tender_archives
             WHERE tender_id = ?1 ORDER BY relative_path",
            tender_id.as_str(),
        )?;
        let recovery_ids = load_string_column(
            &installation,
            "SELECT recovery_id FROM tender_recoveries WHERE tender_id = ?1 ORDER BY recovery_id",
            tender_id.as_str(),
        )?;
        Ok(PermanentDeletionPlan {
            erased_copy_classes: vec![
                ErasedTenderCopyClass::TenderStore,
                ErasedTenderCopyClass::TenderBackup,
                ErasedTenderCopyClass::PortableTenderArchive,
                ErasedTenderCopyClass::DeliveryExport,
                ErasedTenderCopyClass::AgentRunWorkspace,
                ErasedTenderCopyClass::StagingItem,
                ErasedTenderCopyClass::QuarantineItem,
                ErasedTenderCopyClass::TenderLog,
            ],
            backup_ids,
            portable_archive_paths,
            recovery_ids,
            provider_cleanup_targets,
        })
    }

    fn begin_purge_from_trash(
        &self,
        mut record: TrashedTenderRecord,
        rationale: &str,
        summary: &TenderSummary,
        plan: PermanentDeletionPlan,
    ) -> Result<(TrashedTenderRecord, TrashedTenderLifecycleDecision), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at: String = transaction
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        let mut decision = TrashedTenderLifecycleDecision {
            decision_id: random_identifier(&transaction)?,
            deletion_id: record.deletion_id.clone(),
            tender_id: record.tender_id.clone(),
            action: "purge".into(),
            rationale: rationale.into(),
            decided_by: "engineer_user".into(),
            acting_role: "tendering_engineer".into(),
            created_at: created_at.clone(),
            audit_event_count: Some(summary.audit_event_count),
            audit_chain_head: Some(summary.audit_chain_head.clone()),
            erased_copy_classes: plan.erased_copy_classes,
            backup_ids: plan.backup_ids,
            portable_archive_paths: plan.portable_archive_paths,
            recovery_ids: plan.recovery_ids,
            provider_cleanup_targets: plan.provider_cleanup_targets,
            manifest_sha256: String::new(),
        };
        decision.manifest_sha256 = manifest_sha256_record(&decision)?;
        insert_trash_lifecycle_decision(&transaction, &decision)?;
        let changed = transaction
            .execute(
                "UPDATE tender_trash SET state = 'purging', updated_at = ?2
                 WHERE deletion_id = ?1 AND state = 'trashed'",
                params![record.deletion_id, created_at],
            )
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        transaction.commit().map_err(sql_error)?;
        record.state = TrashedTenderState::Purging;
        record.updated_at = created_at;
        Ok((record, decision))
    }

    fn load_purge_decision(
        &self,
        deletion_id: &str,
    ) -> Result<TrashedTenderLifecycleDecision, TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open_with_flags(
            self.application_home().join("installation.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(sql_error)?;
        let decision_json: String = connection
            .query_row(
                "SELECT decision_json FROM tender_trash_decisions
                 WHERE deletion_id = ?1 AND action = 'purge'
                 ORDER BY created_at DESC, decision_id DESC LIMIT 1",
                [deletion_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let decision = parse_canonical_record::<TrashedTenderLifecycleDecision>(&decision_json)?;
        if decision.deletion_id != deletion_id
            || decision.action != "purge"
            || manifest_sha256_record(&decision)? != decision.manifest_sha256
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(decision)
    }

    fn complete_purge_from_trash(
        &self,
        record: TrashedTenderRecord,
    ) -> Result<DeletionReceipt, TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let decision_json: String = transaction
            .query_row(
                "SELECT decision_json FROM tender_trash_decisions
                 WHERE deletion_id = ?1 AND action = 'purge'
                 ORDER BY created_at DESC, decision_id DESC LIMIT 1",
                params![record.deletion_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let decision = parse_canonical_record::<TrashedTenderLifecycleDecision>(&decision_json)?;
        if decision.deletion_id != record.deletion_id
            || decision.tender_id != record.tender_id
            || decision.action != "purge"
            || manifest_sha256_record(&decision)? != decision.manifest_sha256
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let purged_at: String = transaction
            .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        let provider_thread_count = u32::try_from(decision.provider_cleanup_targets.len())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let provider_target_manifests = decision
            .provider_cleanup_targets
            .iter()
            .map(manifest_sha256_record)
            .collect::<Result<Vec<_>, _>>()?;
        let provider_cleanup_manifest_sha256 = manifest_sha256_record(&provider_target_manifests)?;
        let mut receipt = DeletionReceiptRecord {
            receipt_id: random_identifier(&transaction)?,
            deletion_id: record.deletion_id.clone(),
            tender_id: record.tender_id.clone(),
            audit_event_count: decision
                .audit_event_count
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            audit_chain_head: decision
                .audit_chain_head
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            local_deletion_completed: true,
            erased_copy_classes: decision.erased_copy_classes.clone(),
            provider_thread_count,
            provider_cleanup_manifest_sha256,
            external_copy_exclusions: vec![
                "original_source_packages".into(),
                "user_copied_exports".into(),
                "third_party_backups".into(),
                "recipient_copies".into(),
                "operating_system_backups".into(),
                "application_provider_credentials".into(),
            ],
            purged_by: decision.decided_by.clone(),
            acting_role: decision.acting_role.clone(),
            purged_at: purged_at.clone(),
            manifest_sha256: String::new(),
        };
        receipt.manifest_sha256 = manifest_sha256_record(&receipt)?;
        transaction
            .execute(
                "INSERT INTO deletion_receipts (
                   receipt_id, tender_id, deletion_id, receipt_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    receipt.receipt_id,
                    receipt.tender_id,
                    receipt.deletion_id,
                    canonical_json_record(&receipt)?,
                    receipt.manifest_sha256,
                    receipt.purged_at,
                ],
            )
            .map_err(sql_error)?;
        for (target_ordinal, target) in decision.provider_cleanup_targets.iter().enumerate() {
            let (provider_kind, status, thread_ref) = match target.provider {
                AiProviderKind::Codex => ("codex", "pending", Some(target.thread_ref.as_str())),
                AiProviderKind::Anthropic => ("anthropic", "completed", None),
                AiProviderKind::Gemini => ("gemini", "completed", None),
            };
            transaction
                .execute(
                    "INSERT INTO provider_cleanup_jobs (
                       cleanup_id, deletion_id, target_ordinal, provider_kind, thread_ref,
                       target_manifest_sha256, status,
                       attempt_count, diagnostic_code, last_attempt_at, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, NULL, NULL, ?8, ?8)",
                    params![
                        random_identifier(&transaction)?,
                        record.deletion_id,
                        i64::try_from(target_ordinal).map_err(|_| TenderCommandError::new(
                            TenderErrorCode::IntegrityFailed
                        ))?,
                        provider_kind,
                        thread_ref,
                        provider_target_manifests[target_ordinal],
                        status,
                        purged_at,
                    ],
                )
                .map_err(sql_error)?;
        }
        transaction
            .execute(
                "DELETE FROM tender_recovery_decisions
                 WHERE recovery_id IN (
                   SELECT recovery_id FROM tender_recoveries WHERE tender_id = ?1
                 )",
                [record.tender_id.as_str()],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "DELETE FROM tender_recoveries WHERE tender_id = ?1",
                [record.tender_id.as_str()],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "DELETE FROM portable_tender_archives WHERE tender_id = ?1",
                [record.tender_id.as_str()],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "DELETE FROM tender_backups WHERE tender_id = ?1",
                [record.tender_id.as_str()],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "DELETE FROM tender_catalogue WHERE tender_id = ?1",
                [record.tender_id.as_str()],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "UPDATE manager_workspace_selection
                 SET selected_tender_id = NULL, selected_at = NULL,
                     selection_sequence = selection_sequence + 1
                 WHERE singleton = 1 AND selected_tender_id = ?1",
                [record.tender_id.as_str()],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "UPDATE manager_workspace_selection
                 SET pending_tender_id = NULL, pending_at = NULL
                 WHERE singleton = 1 AND pending_tender_id = ?1",
                [record.tender_id.as_str()],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "DELETE FROM tender_trash_decisions WHERE deletion_id = ?1",
                [record.deletion_id.as_str()],
            )
            .map_err(sql_error)?;
        let changed = transaction
            .execute(
                "DELETE FROM tender_trash WHERE deletion_id = ?1 AND state = 'purging'",
                [record.deletion_id.as_str()],
            )
            .map_err(sql_error)?;
        if changed != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let result = deletion_receipt_view(&transaction, &receipt)?;
        transaction.commit().map_err(sql_error)?;
        Ok(result)
    }

    fn ensure_tender_identity_available(
        &self,
        tender_id: &TenderId,
        excluded_deletion_id: Option<&str>,
    ) -> Result<(), TenderCommandError> {
        if self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str())
            .exists()
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let collision: bool = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM tender_trash
                   WHERE tender_id = ?1 AND deletion_id != COALESCE(?2, '')
                   UNION ALL
                   SELECT 1 FROM deletion_receipts WHERE tender_id = ?1
                 )",
                params![tender_id.as_str(), excluded_deletion_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if collision {
            Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
        } else {
            Ok(())
        }
    }

    fn insert_trash_record(&self, record: &TrashedTenderRecord) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?
            .execute(
                "INSERT INTO tender_trash (
                   deletion_id, tender_id, state, relative_path, approval_json,
                   approval_manifest_sha256, diagnostic_code, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    record.deletion_id,
                    record.tender_id,
                    record.state.as_str(),
                    record.relative_path,
                    canonical_json_record(record)?,
                    record.approval_manifest_sha256,
                    record.diagnostic_code,
                    record.created_at,
                    record.updated_at,
                ],
            )
            .map(|_| ())
            .map_err(sql_error)
    }

    fn update_trash_record(
        &self,
        record: &TrashedTenderRecord,
        expected: TrashedTenderState,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let changed = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?
            .execute(
                "UPDATE tender_trash SET state = ?2, diagnostic_code = ?3, updated_at = ?4
                 WHERE deletion_id = ?1 AND state = ?5",
                params![
                    record.deletion_id,
                    record.state.as_str(),
                    record.diagnostic_code,
                    record.updated_at,
                    expected.as_str(),
                ],
            )
            .map_err(sql_error)?;
        if changed == 1 {
            Ok(())
        } else {
            Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
        }
    }

    fn reconcile_trash_records(&self) -> Result<(), TenderCommandError> {
        let records = {
            let _guard = self
                .catalogue_lock()
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            let connection = Connection::open_with_flags(
                self.application_home().join("installation.sqlite"),
                OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(sql_error)?;
            let mut statement = connection
                .prepare(
                    "SELECT approval_json, state FROM tender_trash
                     WHERE state IN ('moving', 'restoring', 'purging')",
                )
                .map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(sql_error)?;
            let mut records = Vec::new();
            for row in rows {
                let (json, state) = row.map_err(sql_error)?;
                let mut record = parse_canonical_record::<TrashedTenderRecord>(&json)?;
                record.state = TrashedTenderState::parse(&state)?;
                records.push(record);
            }
            records
        };
        for mut record in records {
            let live = self
                .application_home()
                .join("tenders")
                .join(&record.tender_id);
            let trashed = self
                .application_home()
                .join("trash")
                .join(&record.relative_path);
            let purging = self
                .application_home()
                .join("staging")
                .join(format!("purge-{}", record.deletion_id));
            record.updated_at = self.installation_timestamp()?;
            let expected = record.state;
            match expected {
                TrashedTenderState::Moving => match (live.exists(), trashed.exists()) {
                    (false, true) => record.state = TrashedTenderState::Trashed,
                    (true, false) => {
                        record.state = TrashedTenderState::Failed;
                        record.diagnostic_code = Some("move_not_published".into());
                    }
                    _ => return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired)),
                },
                TrashedTenderState::Restoring => match (live.exists(), trashed.exists()) {
                    (false, true) => {
                        fs::rename(&trashed, &live).map_err(store_unavailable)?;
                        record.state = TrashedTenderState::Restored;
                    }
                    (true, false) => record.state = TrashedTenderState::Restored,
                    _ => return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired)),
                },
                TrashedTenderState::Purging => {
                    if live.exists() || (trashed.exists() && purging.exists()) {
                        return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
                    }
                    if trashed.exists() {
                        fs::rename(&trashed, &purging).map_err(store_unavailable)?;
                    }
                    let decision = self.load_purge_decision(&record.deletion_id)?;
                    remove_permanent_deletion_files(self.application_home(), &record, &decision)?;
                    self.complete_purge_from_trash(record)?;
                    continue;
                }
                _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
            }
            self.update_trash_record(&record, expected)?;
        }
        Ok(())
    }
}

pub(crate) fn reconcile_interrupted_backup_operations(
    application_home: &Path,
) -> Result<(), TenderCommandError> {
    let connection =
        Connection::open(application_home.join("installation.sqlite")).map_err(sql_error)?;
    let interrupted_backups = {
        let mut statement = connection
            .prepare("SELECT backup_id FROM tender_backups WHERE state = 'creating'")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows
    };
    for backup_id in interrupted_backups {
        if !valid_identifier(&backup_id) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        remove_operation_staging(
            &application_home.join("staging"),
            &application_home
                .join("staging")
                .join(format!("backup-{backup_id}")),
        )?;
        remove_operation_file(
            &application_home.join("backups"),
            &application_home
                .join("backups")
                .join(format!("{backup_id}.qtbackup")),
        )?;
        if connection
            .execute(
                "UPDATE tender_backups SET state = 'failed', diagnostic_code = 'interrupted'
                 WHERE backup_id = ?1 AND state = 'creating'",
                [&backup_id],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }

    let interrupted_recoveries = {
        let mut statement = connection
            .prepare(
                "SELECT recovery_id, tender_id, backup_id, state, backup_source_json
                 FROM tender_recoveries WHERE state IN ('preparing', 'applying')",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows
    };
    for (recovery_id, tender_id, backup_id, state, backup_source_json) in interrupted_recoveries {
        if !valid_identifier(&recovery_id) || !valid_identifier(&tender_id) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let staging_parent = application_home.join("staging");
        let staging = staging_parent.join(format!("recovery-{recovery_id}"));
        if state == "preparing" {
            remove_operation_staging(&staging_parent, &staging)?;
            mark_interrupted_recovery_failed(&connection, &recovery_id, "preparing")?;
            continue;
        }

        let current = application_home.join("tenders").join(&tender_id);
        let retained = application_home
            .join("trash")
            .join(format!("recovery-replaced-{recovery_id}"));
        let current_exists = current.try_exists().map_err(store_unavailable)?;
        let staging_exists = staging.try_exists().map_err(store_unavailable)?;
        let retained_exists = retained.try_exists().map_err(store_unavailable)?;
        match (current_exists, staging_exists, retained_exists) {
            (true, true, false) => {
                remove_operation_staging(&staging_parent, &staging)?;
                mark_interrupted_recovery_failed(&connection, &recovery_id, "applying")?;
            }
            (true, false, false) => {
                validate_recovery_swap_directory(&current)?;
                mark_interrupted_recovery_failed(&connection, &recovery_id, "applying")?;
            }
            (false, true, true) => {
                validate_recovery_swap_directory(&retained)?;
                fs::rename(&retained, &current).map_err(store_unavailable)?;
                remove_operation_staging(&staging_parent, &staging)?;
                mark_interrupted_recovery_failed(&connection, &recovery_id, "applying")?;
            }
            (true, false, true) => {
                let tender_id = TenderId::parse(&tender_id)?;
                let budget = StorageOperationBudget::for_tender(&tender_id);
                let completion = (|| {
                    let expected_source = decode_summary(backup_source_json)?
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                    let backup = read_backup_record(&connection, &tender_id, &backup_id)?;
                    let archive = application_home
                        .join("backups")
                        .join(format!("{backup_id}.qtbackup"));
                    let manifest =
                        bind_verified_backup_archive(&archive, &tender_id, &backup, budget)?;
                    if verify_recovery_candidate_directory(&current, &tender_id, &manifest, budget)?
                        != expected_source
                    {
                        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                    }
                    Ok(())
                })();
                match completion {
                    Ok(()) => {
                        if connection
                            .execute(
                                "UPDATE tender_recoveries SET state = 'applied'
                             WHERE recovery_id = ?1 AND state = 'applying'",
                                [&recovery_id],
                            )
                            .map_err(sql_error)?
                            != 1
                        {
                            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                        }
                    }
                    Err(error) => {
                        restore_retained_tender(&current, &retained, &staging_parent, &staging)?;
                        mark_recovery_failed(
                            &connection,
                            &recovery_id,
                            "applying",
                            tender_error_code(error.code),
                        )?;
                    }
                }
            }
            _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
    let rejected_recoveries = {
        let mut statement = connection
            .prepare("SELECT recovery_id FROM tender_recoveries WHERE state = 'rejected'")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows
    };
    for recovery_id in rejected_recoveries {
        if !valid_identifier(&recovery_id) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let staging_parent = application_home.join("staging");
        remove_operation_staging(
            &staging_parent,
            &staging_parent.join(format!("recovery-{recovery_id}")),
        )?;
    }
    Ok(())
}

fn copy_file_verified(
    source: &Path,
    destination: &Path,
    budget: StorageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut input = File::open(source).map_err(store_unavailable)?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)
        .map_err(store_unavailable)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut source_hasher = sha2::Sha256::new();
    loop {
        budget.check()?;
        let read = input.read(&mut buffer).map_err(store_unavailable)?;
        if read == 0 {
            break;
        }
        source_hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(store_unavailable)?;
    }
    output.sync_all().map_err(store_unavailable)?;
    let source_hash: String = source_hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let (destination_hash, _) = sha256_file(destination, budget)?;
    if source_hash != destination_hash {
        let _ = fs::remove_file(destination);
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn inspect_summary_at(
    root: &Path,
    tender_id: &TenderId,
) -> Result<TenderSummary, TenderCommandError> {
    let connection = Connection::open_with_flags(
        root.join("tender.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(sql_error)?;
    let summary = store_summary_from_connection(&connection)?;
    if summary.tender_id != tender_id.as_str() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(summary)
}

fn store_summary_from_connection(
    connection: &Connection,
) -> Result<TenderSummary, TenderCommandError> {
    let (tender_id, revision, lifecycle_phase, name): (String, u32, String, String) = connection
        .query_row(
            "SELECT tender.tender_id, tender.current_revision, tender.lifecycle_phase,
                    tender_revisions.name
             FROM tender
             JOIN tender_revisions ON tender_revisions.revision = tender.current_revision
             WHERE tender.singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(sql_error)?;
    let (audit_event_count, audit_chain_head): (i64, Option<String>) = connection
        .query_row(
            "SELECT COUNT(*), MAX(current_hash) FILTER (
               WHERE sequence = (SELECT MAX(sequence) FROM audit_events)
             ) FROM audit_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    Ok(TenderSummary {
        tender_id,
        name,
        revision,
        lifecycle_phase: TenderLifecyclePhase::parse(&lifecycle_phase)?,
        audit_event_count: audit_event_count
            .try_into()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        audit_chain_head: audit_chain_head.unwrap_or_else(|| ZERO_AUDIT_HASH.into()),
    })
}

fn canonical_json_record<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn insert_trash_lifecycle_decision(
    transaction: &Transaction<'_>,
    decision: &TrashedTenderLifecycleDecision,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO tender_trash_decisions (
               decision_id, deletion_id, action, rationale, decided_by, acting_role,
               decision_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                decision.decision_id,
                decision.deletion_id,
                decision.action,
                decision.rationale,
                decision.decided_by,
                decision.acting_role,
                canonical_json_record(decision)?,
                decision.manifest_sha256,
                decision.created_at,
            ],
        )
        .map(|_| ())
        .map_err(sql_error)
}

fn parse_canonical_record<T: for<'de> Deserialize<'de> + Serialize>(
    value: &str,
) -> Result<T, TenderCommandError> {
    let record: T = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json_record(&record)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(record)
}

fn manifest_sha256_record<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    let mut value = serde_json::to_value(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if let Some(object) = value.as_object_mut() {
        let self_hash_field = ["manifest_sha256", "approval_manifest_sha256"]
            .into_iter()
            .find(|field| object.contains_key(*field));
        if let Some(field) = self_hash_field {
            object.insert(field.into(), serde_json::Value::String(String::new()));
        }
    }
    let bytes = serde_json_canonicalizer::to_vec(&value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(sha256_hex(&bytes))
}

fn read_backup_record(
    connection: &Connection,
    tender_id: &TenderId,
    backup_id: &str,
) -> Result<TenderBackupRecord, TenderCommandError> {
    let row = connection
        .query_row(
            "SELECT state, source_json, content_object_count, manifest_sha256,
                    archive_size_bytes, diagnostic_code, created_at
             FROM tender_backups WHERE tender_id = ?1 AND backup_id = ?2",
            params![tender_id.as_str(), backup_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let record = TenderBackupRecord {
        backup_id: backup_id.to_owned(),
        tender_id: tender_id.as_str().to_owned(),
        state: TenderBackupState::parse(&row.0)?,
        source: decode_summary(row.1)?,
        content_object_count: row
            .2
            .try_into()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        manifest_sha256: row.3,
        archive_size_bytes: row
            .4
            .map(u64::try_from)
            .transpose()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        diagnostic_code: row.5,
        created_at: row.6,
    };
    if record.state != TenderBackupState::Ready {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(record)
}

fn mark_interrupted_recovery_failed(
    connection: &Connection,
    recovery_id: &str,
    expected_state: &str,
) -> Result<(), TenderCommandError> {
    mark_recovery_failed(connection, recovery_id, expected_state, "interrupted")
}

fn mark_recovery_failed(
    connection: &Connection,
    recovery_id: &str,
    expected_state: &str,
    diagnostic_code: &str,
) -> Result<(), TenderCommandError> {
    if connection
        .execute(
            "UPDATE tender_recoveries SET state = 'failed', diagnostic_code = ?3
             WHERE recovery_id = ?1 AND state = ?2",
            params![recovery_id, expected_state, diagnostic_code],
        )
        .map_err(sql_error)?
        != 1
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn remove_operation_staging(parent: &Path, target: &Path) -> Result<(), TenderCommandError> {
    match fs::symlink_metadata(target) {
        Ok(_) => remove_verified_directory(parent, target),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(store_unavailable(error)),
    }
}

fn remove_permanent_deletion_files(
    application_home: &Path,
    record: &TrashedTenderRecord,
    decision: &TrashedTenderLifecycleDecision,
) -> Result<(), TenderCommandError> {
    if decision.action != "purge"
        || decision.deletion_id != record.deletion_id
        || decision.tender_id != record.tender_id
        || manifest_sha256_record(decision)? != decision.manifest_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let backups = application_home.join("backups");
    for backup_id in &decision.backup_ids {
        if !valid_identifier(backup_id) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        remove_operation_file(&backups, &backups.join(format!("{backup_id}.qtbackup")))?;
    }
    let archives = application_home.join("archives");
    for relative_path in &decision.portable_archive_paths {
        let relative = Path::new(relative_path);
        if relative
            .parent()
            .is_some_and(|parent| parent != Path::new(""))
            || relative.file_name().and_then(|value| value.to_str()) != Some(relative_path)
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        remove_operation_file(&archives, &archives.join(relative))?;
    }
    remove_operation_staging(
        &application_home.join("exports"),
        &application_home.join("exports").join(&record.tender_id),
    )?;

    let mut identifiers = vec![record.tender_id.clone(), record.deletion_id.clone()];
    identifiers.extend(decision.backup_ids.iter().cloned());
    identifiers.extend(decision.recovery_ids.iter().cloned());
    identifiers.extend(decision.portable_archive_paths.iter().cloned());
    remove_matching_managed_children(&application_home.join("staging"), &identifiers)?;
    remove_matching_managed_children(&application_home.join("logs"), &identifiers)?;
    Ok(())
}

fn remove_matching_managed_children(
    parent: &Path,
    identifiers: &[String],
) -> Result<(), TenderCommandError> {
    let entries = fs::read_dir(parent).map_err(store_unavailable)?;
    for entry in entries {
        let entry = entry.map_err(store_unavailable)?;
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .to_owned();
        if !identifiers
            .iter()
            .any(|identifier| !identifier.is_empty() && name.contains(identifier))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
        if metadata.is_dir() && !metadata_is_unsafe_storage_link(&metadata) {
            remove_operation_staging(parent, &entry.path())?;
        } else if metadata.is_file() && !metadata_is_unsafe_storage_link(&metadata) {
            remove_operation_file(parent, &entry.path())?;
        } else {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    Ok(())
}

fn load_string_column(
    connection: &Connection,
    sql: &str,
    parameter: &str,
) -> Result<Vec<String>, TenderCommandError> {
    let mut statement = connection.prepare(sql).map_err(sql_error)?;
    let rows = statement
        .query_map([parameter], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    rows.map(|row| row.map_err(sql_error)).collect()
}

fn deletion_receipt_view(
    connection: &Connection,
    record: &DeletionReceiptRecord,
) -> Result<DeletionReceipt, TenderCommandError> {
    let target_manifests = load_string_column(
        connection,
        "SELECT target_manifest_sha256 FROM provider_cleanup_jobs
         WHERE deletion_id = ?1 ORDER BY target_ordinal",
        &record.deletion_id,
    )?;
    if manifest_sha256_record(&target_manifests)? != record.provider_cleanup_manifest_sha256 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let (provider_threads, confirmed): (i64, i64) = connection
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(status = 'completed'), 0)
             FROM provider_cleanup_jobs WHERE deletion_id = ?1",
            [record.deletion_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    let provider_threads = u32::try_from(provider_threads)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let confirmed = u32::try_from(confirmed)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if provider_threads != record.provider_thread_count || confirmed > provider_threads {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let provider_cleanup_status = if provider_threads == 0 {
        ProviderCleanupStatus::NotRequired
    } else if confirmed == provider_threads {
        ProviderCleanupStatus::Completed
    } else {
        ProviderCleanupStatus::Pending
    };
    Ok(DeletionReceipt {
        receipt_id: record.receipt_id.clone(),
        deletion_id: record.deletion_id.clone(),
        tender_id: record.tender_id.clone(),
        audit_event_count: record.audit_event_count,
        audit_chain_head: record.audit_chain_head.clone(),
        local_deletion_completed: record.local_deletion_completed,
        erased_copy_classes: record.erased_copy_classes.clone(),
        provider_cleanup_status,
        provider_thread_count: provider_threads,
        confirmed_provider_thread_deletions: confirmed,
        external_copy_exclusions: record.external_copy_exclusions.clone(),
        purged_by: record.purged_by.clone(),
        acting_role: record.acting_role.clone(),
        purged_at: record.purged_at.clone(),
        manifest_sha256: record.manifest_sha256.clone(),
    })
}

fn remove_operation_file(parent: &Path, target: &Path) -> Result<(), TenderCommandError> {
    if target.parent() != Some(parent) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(store_unavailable(error)),
    };
    if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_file() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let canonical_parent = fs::canonicalize(parent).map_err(store_unavailable)?;
    let canonical_target = fs::canonicalize(target).map_err(store_unavailable)?;
    if canonical_target.parent() != Some(canonical_parent.as_path()) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    fs::remove_file(canonical_target).map_err(store_unavailable)
}

fn backup_available_space(
    host: &QuantixHost,
    backups_root: &Path,
    tender_id: &TenderId,
) -> Result<u64, TenderCommandError> {
    #[cfg(feature = "runtime-fixture")]
    if let Ok(fixture) = std::env::var("QUANTIX_BACKUP_AVAILABLE_SPACE_BYTES") {
        if let Some((fixture_tender_id, available_space)) = fixture.split_once(':') {
            if fixture_tender_id == tender_id.as_str() {
                return available_space
                    .parse()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
    }
    #[cfg(not(feature = "runtime-fixture"))]
    let _ = tender_id;
    host.setup_platform()
        .available_space(backups_root)
        .map_err(store_unavailable)
}

fn recovery_available_space(
    host: &QuantixHost,
    staging_root: &Path,
    tender_id: &TenderId,
) -> Result<u64, TenderCommandError> {
    #[cfg(feature = "runtime-fixture")]
    if let Ok(fixture) = std::env::var("QUANTIX_RECOVERY_AVAILABLE_SPACE_BYTES") {
        if let Some((fixture_tender_id, available_space)) = fixture.split_once(':') {
            if fixture_tender_id == tender_id.as_str() {
                return available_space
                    .parse()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
    }
    #[cfg(not(feature = "runtime-fixture"))]
    let _ = tender_id;
    host.setup_platform()
        .available_space(staging_root)
        .map_err(store_unavailable)
}

fn recovery_io_failure(tender_id: &TenderId, name: &str) -> bool {
    #[cfg(feature = "runtime-fixture")]
    if let Ok(fixture) = std::env::var("QUANTIX_RECOVERY_IO_FAILURE") {
        return fixture
            .split_once(':')
            .is_some_and(|(fixture_tender_id, fixture_name)| {
                fixture_tender_id == tender_id.as_str() && fixture_name == name
            });
    }
    #[cfg(not(feature = "runtime-fixture"))]
    let _ = (tender_id, name);
    false
}

fn tender_error_code(code: TenderErrorCode) -> &'static str {
    match code {
        TenderErrorCode::InsufficientSpace => "insufficient_space",
        TenderErrorCode::IntegrityFailed => "integrity_failed",
        TenderErrorCode::InvalidCommand => "invalid_command",
        TenderErrorCode::NotFound => "not_found",
        TenderErrorCode::OauthAlreadyRunning => "oauth_already_running",
        TenderErrorCode::OauthPortBlocked => "oauth_port_blocked",
        TenderErrorCode::OperationTimedOut => "operation_timed_out",
        TenderErrorCode::RecoveryRequired => "recovery_required",
        TenderErrorCode::RuntimeRequired => "runtime_required",
        TenderErrorCode::SetupRequired => "setup_required",
        TenderErrorCode::StoreUnavailable => "store_unavailable",
    }
}

fn extract_verified_recovery_candidate(
    archive_path: &Path,
    staging: &Path,
    tender_id: &TenderId,
    record: &TenderBackupRecord,
    available_space: u64,
    budget: StorageOperationBudget,
) -> Result<TenderSummary, TenderCommandError> {
    budget.check()?;
    let manifest = bind_verified_backup_archive(archive_path, tender_id, record, budget)?;
    let expanded_size_bytes = backup_expanded_size(&manifest, budget)?;
    let required_space = expanded_size_bytes
        .checked_add(MIN_BACKUP_FREE_SPACE_RESERVE_BYTES)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if available_space < required_space {
        return Err(TenderCommandError::new(TenderErrorCode::InsufficientSpace));
    }

    for directory in ["content", "runs", "staging"] {
        fs::create_dir(staging.join(directory)).map_err(store_unavailable)?;
    }
    let file = File::open(archive_path).map_err(store_unavailable)?;
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    let database_sha256 = extract_archive_file(
        &mut archive,
        &manifest.database,
        &staging.join("tender.sqlite"),
        budget,
    )?;
    if database_sha256 != manifest.database.sha256 {
        return Err(backup_integrity_failed("recovery_database_identity"));
    }
    for content in &manifest.content {
        budget.check()?;
        let bytes = read_archive_bytes(
            &mut archive,
            &content.archive_path,
            content.size_bytes,
            budget,
        )?;
        if sha256_hex(&bytes) != content.sha256 {
            return Err(backup_integrity_failed("recovery_content_identity"));
        }
        cacache::write_hash_sync(staging.join("content"), &bytes).map_err(content_store_error)?;
    }
    verify_recovery_candidate_directory(staging, tender_id, &manifest, budget)
}

fn bind_verified_backup_archive(
    archive_path: &Path,
    tender_id: &TenderId,
    record: &TenderBackupRecord,
    budget: StorageOperationBudget,
) -> Result<TenderBackupManifest, TenderCommandError> {
    budget.check()?;
    let manifest = read_backup_manifest(archive_path, budget)?;
    let manifest_json = serde_json_canonicalizer::to_string(&manifest)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if manifest.format_version != BACKUP_FORMAT_VERSION
        || manifest.tender_id != tender_id.as_str()
        || manifest.tender_schema_version != TENDER_SCHEMA_VERSION
        || manifest.quantix_version != env!("CARGO_PKG_VERSION")
        || manifest.creation.backup_id != record.backup_id
        || manifest.creation.created_at != record.created_at
        || manifest.creation.created_by != "engineer_user"
        || record.tender_id != tender_id.as_str()
        || record.source.as_ref() != Some(&manifest.source)
        || record.content_object_count != manifest.content.len() as u64
        || record.manifest_sha256.as_deref() != Some(sha256_hex(manifest_json.as_bytes()).as_str())
        || record.archive_size_bytes
            != Some(fs::metadata(archive_path).map_err(store_unavailable)?.len())
    {
        return Err(backup_integrity_failed("recovery_manifest_binding"));
    }
    validate_backup_manifest(&manifest, budget)?;
    verify_backup_archive(archive_path, &manifest, budget)?;
    Ok(manifest)
}

fn validate_backup_manifest(
    manifest: &TenderBackupManifest,
    budget: StorageOperationBudget,
) -> Result<(), TenderCommandError> {
    budget.check()?;
    if manifest.database.archive_path != "tender.sqlite"
        || manifest.database.size_bytes == 0
        || manifest.database.size_bytes > MAX_BACKUP_DATABASE_BYTES
        || !valid_sha256(&manifest.database.sha256)
        || manifest.content.len() > MAX_BACKUP_CONTENT_OBJECTS
    {
        return Err(backup_integrity_failed("manifest_limits"));
    }
    for pair in manifest.content.windows(2) {
        budget.check()?;
        if pair[0].sha256 >= pair[1].sha256 {
            return Err(backup_integrity_failed("content_manifest_order"));
        }
    }
    for content in &manifest.content {
        budget.check()?;
        if content.archive_path != format!("content/{}", content.sha256)
            || content.sha256.len() != 64
            || !valid_sha256(&content.sha256)
            || content.size_bytes == 0
            || content.size_bytes > MAX_CONTENT_BYTES as u64
        {
            return Err(backup_integrity_failed("content_manifest_identity"));
        }
    }
    backup_expanded_size(manifest, budget)?;
    Ok(())
}

fn verify_recovery_candidate_directory(
    staging: &Path,
    tender_id: &TenderId,
    manifest: &TenderBackupManifest,
    budget: StorageOperationBudget,
) -> Result<TenderSummary, TenderCommandError> {
    budget.check()?;
    let (database_sha256, database_size_bytes) =
        sha256_file(&staging.join("tender.sqlite"), budget)?;
    if database_sha256 != manifest.database.sha256
        || database_size_bytes != manifest.database.size_bytes
    {
        return Err(backup_integrity_failed("candidate_database_identity"));
    }
    let integrity =
        TenderStore::inspect_integrity_with_check(staging, tender_id, || budget.check())?;
    if integrity.state != TenderIntegrityState::Ready {
        return Err(backup_integrity_failed("recovery_candidate_integrity"));
    }
    let connection = Connection::open_with_flags(
        staging.join("tender.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(sql_error)?;
    let store = TenderStore {
        root: staging.to_path_buf(),
        connection,
        recovery_required: false,
        archived: false,
    };
    let source = store.summary()?;
    if source != manifest.source {
        return Err(backup_integrity_failed("recovery_source_summary"));
    }
    let candidate_content = store.backup_content(budget)?;
    if candidate_content
        .iter()
        .map(|content| content.manifest.clone())
        .collect::<Vec<_>>()
        != manifest.content
    {
        return Err(backup_integrity_failed("candidate_content_identity"));
    }
    Ok(source)
}

fn backup_expanded_size(
    manifest: &TenderBackupManifest,
    budget: StorageOperationBudget,
) -> Result<u64, TenderCommandError> {
    let mut size_bytes = manifest.database.size_bytes;
    for content in &manifest.content {
        budget.check()?;
        size_bytes = size_bytes
            .checked_add(content.size_bytes)
            .ok_or_else(|| backup_integrity_failed("expanded_size_overflow"))?;
    }
    if size_bytes > MAX_BACKUP_EXPANDED_BYTES {
        return Err(backup_integrity_failed("expanded_size_limit"));
    }
    Ok(size_bytes)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn read_backup_manifest(
    path: &Path,
    budget: StorageOperationBudget,
) -> Result<TenderBackupManifest, TenderCommandError> {
    budget.check()?;
    let file = File::open(path).map_err(store_unavailable)?;
    if file.metadata().map_err(store_unavailable)?.len() > MAX_BACKUP_ARCHIVE_BYTES {
        return Err(backup_integrity_failed("archive_size"));
    }
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    if archive.len() > MAX_BACKUP_CONTENT_OBJECTS + 2 {
        return Err(backup_integrity_failed("archive_entry_limit"));
    }
    let mut manifest_indices = Vec::new();
    for index in 0..archive.len() {
        budget.check()?;
        if archive
            .by_index_raw(index)
            .is_ok_and(|entry| entry.name() == "manifest.json")
        {
            manifest_indices.push(index);
        }
    }
    if manifest_indices.len() != 1 {
        return Err(backup_integrity_failed("manifest_entry_count"));
    }
    let mut entry = archive.by_index(manifest_indices[0]).map_err(zip_error)?;
    if entry.size() > MAX_BACKUP_MANIFEST_BYTES
        || entry.compression() != CompressionMethod::Stored
        || entry.compressed_size() != entry.size()
        || entry.enclosed_name().is_none()
    {
        return Err(backup_integrity_failed("manifest_entry_safety"));
    }
    let mut manifest_json = String::new();
    entry
        .read_to_string(&mut manifest_json)
        .map_err(store_unavailable)?;
    let manifest: TenderBackupManifest = serde_json::from_str(&manifest_json)
        .map_err(|_| backup_integrity_failed("manifest_json"))?;
    if serde_json_canonicalizer::to_string(&manifest)
        .map_err(|_| backup_integrity_failed("manifest_canonicalization"))?
        != manifest_json
    {
        return Err(backup_integrity_failed("manifest_not_canonical"));
    }
    validate_backup_manifest(&manifest, budget)?;
    Ok(manifest)
}

fn read_archive_bytes(
    archive: &mut ZipArchive<File>,
    path: &str,
    expected_size: u64,
    budget: StorageOperationBudget,
) -> Result<Vec<u8>, TenderCommandError> {
    if expected_size == 0 || expected_size > MAX_CONTENT_BYTES as u64 {
        return Err(backup_integrity_failed("archive_content_limit"));
    }
    let mut entry = archive.by_name(path).map_err(zip_error)?;
    if entry.size() != expected_size
        || entry.compression() != CompressionMethod::Stored
        || entry.compressed_size() != entry.size()
    {
        return Err(backup_integrity_failed("archive_entry_size"));
    }
    let capacity = usize::try_from(expected_size)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut bytes = Vec::with_capacity(capacity);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        budget.check()?;
        let read = entry.read(&mut buffer).map_err(store_unavailable)?;
        if read == 0 {
            break;
        }
        if bytes.len().saturating_add(read) > capacity {
            return Err(backup_integrity_failed("archive_entry_length"));
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    if bytes.len() != capacity {
        return Err(backup_integrity_failed("archive_entry_length"));
    }
    Ok(bytes)
}

fn extract_archive_file(
    archive: &mut ZipArchive<File>,
    expected: &BackupFileManifest,
    output: &Path,
    budget: StorageOperationBudget,
) -> Result<String, TenderCommandError> {
    let mut entry = archive.by_name(&expected.archive_path).map_err(zip_error)?;
    if entry.size() != expected.size_bytes
        || entry.compression() != CompressionMethod::Stored
        || entry.compressed_size() != entry.size()
        || expected.size_bytes == 0
        || expected.size_bytes > MAX_BACKUP_DATABASE_BYTES
    {
        return Err(backup_integrity_failed("archive_database_limit"));
    }
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(store_unavailable)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size_bytes = 0_u64;
    loop {
        budget.check()?;
        let read = entry.read(&mut buffer).map_err(store_unavailable)?;
        if read == 0 {
            break;
        }
        size_bytes = size_bytes
            .checked_add(read as u64)
            .ok_or_else(|| backup_integrity_failed("database_size_overflow"))?;
        if size_bytes > expected.size_bytes {
            return Err(backup_integrity_failed("database_size_limit"));
        }
        digest.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(store_unavailable)?;
    }
    if size_bytes != expected.size_bytes {
        return Err(backup_integrity_failed("database_size_mismatch"));
    }
    output.sync_all().map_err(store_unavailable)?;
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn create_sqlite_snapshot(
    source: &Connection,
    destination_path: &Path,
    budget: StorageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut destination = Connection::open(destination_path).map_err(sql_error)?;
    let backup = rusqlite::backup::Backup::new(source, &mut destination).map_err(sql_error)?;
    loop {
        budget.check()?;
        match backup.step(100).map_err(sql_error)? {
            StepResult::Done => break,
            StepResult::More => {}
            StepResult::Busy | StepResult::Locked => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable)),
        }
    }
    drop(backup);
    destination.close().map_err(|(_, error)| sql_error(error))
}

fn inspect_current_summary(application_home: &Path, tender_id: &TenderId) -> Option<TenderSummary> {
    let root = application_home.join("tenders").join(tender_id.as_str());
    let connection = Connection::open_with_flags(
        root.join("tender.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    TenderStore {
        root,
        connection,
        recovery_required: false,
        archived: false,
    }
    .summary()
    .ok()
}

fn inspect_current_identity(application_home: &Path, tender_id: &TenderId) -> Option<String> {
    Connection::open_with_flags(
        application_home
            .join("tenders")
            .join(tender_id.as_str())
            .join("tender.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?
    .query_row(
        "SELECT tender_id FROM tender WHERE singleton = 1",
        [],
        |row| row.get(0),
    )
    .ok()
}

fn restore_retained_tender(
    current: &Path,
    retained: &Path,
    staging_parent: &Path,
    staging: &Path,
) -> Result<(), TenderCommandError> {
    validate_recovery_swap_directory(current)?;
    validate_recovery_swap_directory(retained)?;
    match fs::symlink_metadata(staging) {
        Ok(_) => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(store_unavailable(error)),
    }
    fs::rename(current, staging).map_err(store_unavailable)?;
    fs::rename(retained, current).map_err(store_unavailable)?;
    remove_verified_directory(staging_parent, staging)
}

fn validate_recovery_swap_directory(path: &Path) -> Result<(), TenderCommandError> {
    let metadata = fs::symlink_metadata(path).map_err(store_unavailable)?;
    if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn encode_summary(summary: Option<&TenderSummary>) -> Result<Option<String>, TenderCommandError> {
    summary
        .map(serde_json_canonicalizer::to_string)
        .transpose()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn decode_summary(value: Option<String>) -> Result<Option<TenderSummary>, TenderCommandError> {
    value
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn decode_recovery_decision(
    decision: Option<String>,
    rationale: Option<String>,
    decided_by: Option<String>,
    manifest_sha256: Option<String>,
    current_audit_chain_head: Option<String>,
    decided_at: Option<String>,
) -> Result<Option<TenderRecoveryDecisionRecord>, TenderCommandError> {
    let Some(decision) = decision else {
        if rationale.is_some()
            || decided_by.is_some()
            || manifest_sha256.is_some()
            || current_audit_chain_head.is_some()
            || decided_at.is_some()
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        return Ok(None);
    };
    Ok(Some(TenderRecoveryDecisionRecord {
        decision: TenderRecoveryDecision::parse(&decision)?,
        rationale: rationale
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        decided_by: decided_by
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        manifest_sha256: manifest_sha256
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        current_audit_chain_head,
        decided_at: decided_at
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
    }))
}

fn write_backup_archive(
    path: &Path,
    manifest: &[u8],
    database: &Path,
    content_root: &Path,
    content: &[BackupContentSource],
    budget: StorageOperationBudget,
) -> Result<(), TenderCommandError> {
    budget.check()?;
    let file = File::create(path).map_err(store_unavailable)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(true);
    archive
        .start_file("manifest.json", options)
        .map_err(zip_error)?;
    archive.write_all(manifest).map_err(store_unavailable)?;
    archive
        .start_file("tender.sqlite", options)
        .map_err(zip_error)?;
    let database_size = fs::metadata(database).map_err(store_unavailable)?.len();
    copy_exact(
        &mut File::open(database).map_err(store_unavailable)?,
        &mut archive,
        database_size,
        budget,
    )?;
    for entry in content {
        budget.check()?;
        archive
            .start_file(&entry.manifest.archive_path, options)
            .map_err(zip_error)?;
        let mut reader = cacache::SyncReader::open_hash(content_root, entry.integrity.clone())
            .map_err(content_store_error)?;
        let sha256 =
            copy_and_sha256_exact(&mut reader, &mut archive, entry.manifest.size_bytes, budget)?;
        if sha256 != entry.manifest.sha256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    let file = archive.finish().map_err(zip_error)?;
    file.sync_all().map_err(store_unavailable)
}

fn verify_backup_archive(
    path: &Path,
    expected: &TenderBackupManifest,
    budget: StorageOperationBudget,
) -> Result<(), TenderCommandError> {
    budget.check()?;
    let file = File::open(path).map_err(store_unavailable)?;
    if file.metadata().map_err(store_unavailable)?.len() > MAX_BACKUP_ARCHIVE_BYTES {
        return Err(backup_integrity_failed("archive_size"));
    }
    let mut archive = ZipArchive::new(file).map_err(zip_error)?;
    if archive.len() != expected.content.len() + 2 {
        return Err(backup_integrity_failed("entry_count"));
    }
    let expected_paths = std::iter::once("manifest.json".to_owned())
        .chain(std::iter::once(expected.database.archive_path.clone()))
        .chain(
            expected
                .content
                .iter()
                .map(|entry| entry.archive_path.clone()),
        )
        .collect::<HashSet<_>>();
    let mut actual_paths = HashSet::new();
    for index in 0..archive.len() {
        budget.check()?;
        let entry = archive.by_index(index).map_err(zip_error)?;
        let name = entry.name().to_owned();
        if entry.is_dir()
            || entry.compression() != CompressionMethod::Stored
            || entry.compressed_size() != entry.size()
            || entry.enclosed_name().is_none()
            || !expected_paths.contains(&name)
            || !actual_paths.insert(name)
        {
            return Err(backup_integrity_failed("entry_path"));
        }
    }
    if actual_paths != expected_paths {
        return Err(backup_integrity_failed("entry_set"));
    }

    let mut manifest_entry = archive.by_name("manifest.json").map_err(zip_error)?;
    if manifest_entry.size() > MAX_BACKUP_MANIFEST_BYTES {
        return Err(backup_integrity_failed("manifest_size"));
    }
    let mut manifest_json = String::new();
    manifest_entry
        .read_to_string(&mut manifest_json)
        .map_err(store_unavailable)?;
    drop(manifest_entry);
    let parsed: TenderBackupManifest = serde_json::from_str(&manifest_json)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if parsed != *expected
        || serde_json_canonicalizer::to_string(&parsed)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            != manifest_json
    {
        return Err(backup_integrity_failed("manifest_identity"));
    }

    verify_archive_entry(&mut archive, &expected.database, budget)?;
    for content in &expected.content {
        budget.check()?;
        verify_archive_content(&mut archive, content, budget)?;
    }
    Ok(())
}

fn verify_archive_entry(
    archive: &mut ZipArchive<File>,
    expected: &BackupFileManifest,
    budget: StorageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut entry = archive.by_name(&expected.archive_path).map_err(zip_error)?;
    let (sha256, size_bytes) = sha256_reader(&mut entry, expected.size_bytes, budget)?;
    if size_bytes != expected.size_bytes || sha256 != expected.sha256 {
        return Err(backup_integrity_failed("database_identity"));
    }
    Ok(())
}

fn verify_archive_content(
    archive: &mut ZipArchive<File>,
    expected: &BackupContentManifest,
    budget: StorageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut entry = archive.by_name(&expected.archive_path).map_err(zip_error)?;
    let (sha256, size_bytes) = sha256_reader(&mut entry, expected.size_bytes, budget)?;
    if size_bytes != expected.size_bytes || sha256 != expected.sha256 {
        return Err(backup_integrity_failed("content_identity"));
    }
    Ok(())
}

fn sha256_file(
    path: &Path,
    budget: StorageOperationBudget,
) -> Result<(String, u64), TenderCommandError> {
    let size_bytes = fs::metadata(path).map_err(store_unavailable)?.len();
    let (sha256, read_bytes) = sha256_reader(
        &mut File::open(path).map_err(store_unavailable)?,
        size_bytes,
        budget,
    )?;
    if read_bytes != size_bytes {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((sha256, size_bytes))
}

fn sha256_reader(
    reader: &mut impl Read,
    expected_size: u64,
    budget: StorageOperationBudget,
) -> Result<(String, u64), TenderCommandError> {
    let limit = expected_size
        .checked_add(1)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut reader = reader.take(limit);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size_bytes = 0_u64;
    loop {
        budget.check()?;
        let read = reader.read(&mut buffer).map_err(store_unavailable)?;
        if read == 0 {
            break;
        }
        size_bytes = size_bytes
            .checked_add(read as u64)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if size_bytes > expected_size {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        digest.update(&buffer[..read]);
    }
    Ok((
        digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        size_bytes,
    ))
}

fn copy_exact(
    reader: &mut impl Read,
    writer: &mut impl Write,
    expected_size: u64,
    budget: StorageOperationBudget,
) -> Result<(), TenderCommandError> {
    let limit = expected_size
        .checked_add(1)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut reader = reader.take(limit);
    let mut buffer = [0_u8; 64 * 1024];
    let mut copied = 0_u64;
    loop {
        budget.check()?;
        let read = reader.read(&mut buffer).map_err(store_unavailable)?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(read as u64)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if copied > expected_size {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        writer
            .write_all(&buffer[..read])
            .map_err(store_unavailable)?;
    }
    if copied != expected_size {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn copy_and_sha256_exact(
    reader: &mut impl Read,
    writer: &mut impl Write,
    expected_size: u64,
    budget: StorageOperationBudget,
) -> Result<String, TenderCommandError> {
    let limit = expected_size
        .checked_add(1)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut reader = reader.take(limit);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut size_bytes = 0_u64;
    loop {
        budget.check()?;
        let read = reader.read(&mut buffer).map_err(store_unavailable)?;
        if read == 0 {
            break;
        }
        size_bytes = size_bytes
            .checked_add(read as u64)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if size_bytes > expected_size {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        digest.update(&buffer[..read]);
        writer
            .write_all(&buffer[..read])
            .map_err(store_unavailable)?;
    }
    if size_bytes != expected_size {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn zip_error(error: zip::result::ZipError) -> TenderCommandError {
    #[cfg(feature = "runtime-fixture")]
    eprintln!("Tender Backup fixture ZIP failure: {error}");
    #[cfg(not(feature = "runtime-fixture"))]
    let _ = error;
    TenderCommandError::new(TenderErrorCode::IntegrityFailed)
}

fn backup_integrity_failed(stage: &str) -> TenderCommandError {
    #[cfg(feature = "runtime-fixture")]
    eprintln!("Tender Backup fixture integrity failure: {stage}");
    #[cfg(not(feature = "runtime-fixture"))]
    let _ = stage;
    TenderCommandError::new(TenderErrorCode::IntegrityFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_lock_queue_respects_the_operation_deadline() {
        let lock = std::sync::Mutex::new(());
        let _held = lock.lock().expect("hold recovery lock");
        let error = lock_with_budget(
            &lock,
            StorageOperationBudget {
                deadline: Instant::now(),
            },
        )
        .expect_err("expired queue wait must fail");
        assert_eq!(error.code, TenderErrorCode::OperationTimedOut);
    }

    #[test]
    fn deletion_receipt_keeps_provider_cleanup_pending_content_free_and_idempotent() {
        let connection = Connection::open_in_memory().expect("open cleanup fixture catalogue");
        connection
            .execute_batch(
                "CREATE TABLE provider_cleanup_jobs (
                   deletion_id TEXT NOT NULL,
                   target_ordinal INTEGER NOT NULL,
                   target_manifest_sha256 TEXT NOT NULL,
                   status TEXT NOT NULL
                 );",
            )
            .expect("create cleanup fixture ledger");
        let target = ProviderCleanupTarget {
            provider: AiProviderKind::Codex,
            thread_ref: "opaque-provider-thread".into(),
        };
        let target_manifest = manifest_sha256_record(&target).expect("hash cleanup target");
        let cleanup_manifest =
            manifest_sha256_record(&vec![target_manifest.clone()]).expect("hash cleanup ledger");
        connection
            .execute(
                "INSERT INTO provider_cleanup_jobs (
                   deletion_id, target_ordinal, target_manifest_sha256, status
                 ) VALUES (?1, 0, ?2, 'pending')",
                params!["1".repeat(32), target_manifest],
            )
            .expect("record pending provider cleanup");
        let mut receipt = DeletionReceiptRecord {
            receipt_id: "2".repeat(32),
            deletion_id: "1".repeat(32),
            tender_id: "3".repeat(32),
            audit_event_count: 1,
            audit_chain_head: "4".repeat(64),
            local_deletion_completed: true,
            erased_copy_classes: vec![ErasedTenderCopyClass::TenderStore],
            provider_thread_count: 1,
            provider_cleanup_manifest_sha256: cleanup_manifest,
            external_copy_exclusions: vec!["application_provider_credentials".into()],
            purged_by: "engineer_user".into(),
            acting_role: "tendering_engineer".into(),
            purged_at: "2026-08-15T00:00:00.000Z".into(),
            manifest_sha256: String::new(),
        };
        receipt.manifest_sha256 = manifest_sha256_record(&receipt).expect("hash receipt");
        let serialized = canonical_json_record(&receipt).expect("serialize receipt");
        assert!(!serialized.contains("opaque-provider-thread"));

        let pending =
            deletion_receipt_view(&connection, &receipt).expect("inspect pending cleanup");
        assert_eq!(
            pending.provider_cleanup_status,
            ProviderCleanupStatus::Pending
        );
        assert_eq!(pending.confirmed_provider_thread_deletions, 0);
        connection
            .execute(
                "UPDATE provider_cleanup_jobs SET status = 'completed' WHERE deletion_id = ?1",
                [&receipt.deletion_id],
            )
            .expect("confirm provider cleanup");
        let completed =
            deletion_receipt_view(&connection, &receipt).expect("inspect completed cleanup");
        assert_eq!(
            completed.provider_cleanup_status,
            ProviderCleanupStatus::Completed
        );
        assert_eq!(completed.confirmed_provider_thread_deletions, 1);
        assert_eq!(
            deletion_receipt_view(&connection, &receipt)
                .expect("repeat completed cleanup inspection"),
            completed
        );
    }
}
