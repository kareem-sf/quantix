use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::{
    append_audit_event_with_sequence,
    package_validation::{load_final_review_for_transaction, FinalReviewInspection},
    random_identifier, require_setup, sha256_hex, sql_error, sqlite_timestamp,
    submission_packages::{
        load_exact_submission_package_item_bytes_for_transaction,
        load_submission_package_for_review_transaction, ExactSubmissionPackage,
    },
    tender_records::TenderRecordFieldCandidate,
    BidPackageOperationBudget, QuantixHost, TenderCommandError, TenderErrorCode, TenderId,
    TenderStore,
};

const MAX_FINAL_LIST_ITEMS: usize = 64;
const MAX_FINAL_TEXT_BYTES: usize = 4_000;
const MAX_RELEASE_EXPORTS: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApproveSubmissionReleaseCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1, max = 32))]
    pub package_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub package_manifest_sha256: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub readiness_report_id: String,
    #[garde(range(min = 1))]
    pub readiness_report_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub readiness_report_manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
    #[garde(length(max = 64), inner(length(bytes, min = 1, max = 4000)))]
    pub conditions: Vec<String>,
    #[garde(length(max = 64), inner(length(bytes, min = 1, max = 4000)))]
    pub exceptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ExportReleaseCopyCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub approval_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub approval_manifest_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionReleaseState {
    NotApproved,
    ReadyForSubmission,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionReleaseApproval {
    pub approval_id: String,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub canonical_manifest_root: String,
    pub readiness_report_id: String,
    pub readiness_report_version: u32,
    pub readiness_report_manifest_sha256: String,
    pub engineer_identity: String,
    pub acting_role: String,
    pub rationale: String,
    pub conditions: Vec<String>,
    pub exceptions: Vec<String>,
    pub approved_at: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReleaseCopyItem {
    pub package_path: String,
    pub content_sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReleaseCopyExport {
    pub export_id: String,
    pub approval_id: String,
    pub relative_path: String,
    pub items: Vec<ReleaseCopyItem>,
    pub verified: bool,
    pub created_at: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionReleaseInspection {
    pub final_review: FinalReviewInspection,
    pub approval: Option<SubmissionReleaseApproval>,
    pub state: SubmissionReleaseState,
    pub exports: Vec<ReleaseCopyExport>,
    pub submission_claimed: bool,
}

impl QuantixHost {
    pub fn approve_submission_release(
        &self,
        command: ApproveSubmissionReleaseCommand,
    ) -> Result<SubmissionReleaseInspection, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        validate_final_text(&command.rationale, &command.conditions, &command.exceptions)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = super::lock_mutex_with_check(&store, &mut || budget.check())?
            .approve_submission_release(&tender_id, &command, budget);
        result
    }

    pub fn inspect_submission_release(
        &self,
        tender_id: &str,
    ) -> Result<Option<SubmissionReleaseInspection>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = super::lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_submission_release(budget);
        result
    }

    pub fn export_release_copy(
        &self,
        command: ExportReleaseCopyCommand,
    ) -> Result<SubmissionReleaseInspection, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = super::lock_mutex_with_check(&store, &mut || budget.check())?
            .export_release_copy(self.application_home(), &tender_id, &command, budget);
        result
    }
}

impl TenderStore {
    fn approve_submission_release(
        &mut self,
        tender_id: &TenderId,
        command: &ApproveSubmissionReleaseCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<SubmissionReleaseInspection, TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let package = load_submission_package_for_review_transaction(
            &transaction,
            &command.package_id,
            command.package_version,
            &command.package_manifest_sha256,
            true,
            budget,
        )?;
        verify_exact_package_bytes(&self.root.join("content"), &package, budget)?;
        let final_review = load_final_review_for_transaction(&transaction, &package, None, budget)?;
        if !final_review.current
            || !final_review.ready
            || final_review.report.report_id != command.readiness_report_id
            || final_review.report.version != command.readiness_report_version
            || final_review.report.manifest_sha256 != command.readiness_report_manifest_sha256
            || !final_review.report.current
            || !final_review.report.ready
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        ensure_deadline_current(&transaction, &final_review)?;
        let existing: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM submission_release_approvals WHERE package_id = ?1 AND package_version = ?2)",
                params![command.package_id, command.package_version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if existing {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let approval_id = random_identifier(&transaction)?;
        let approved_at = sqlite_timestamp(&transaction)?;
        let canonical_manifest_root = canonical_manifest_root(&package)?;
        let mut approval = SubmissionReleaseApproval {
            approval_id: approval_id.clone(),
            package_id: package.package.package_id.clone(),
            package_version: package.package.version,
            package_manifest_sha256: package.package.manifest_sha256.clone(),
            canonical_manifest_root,
            readiness_report_id: final_review.report.report_id.clone(),
            readiness_report_version: final_review.report.version,
            readiness_report_manifest_sha256: final_review.report.manifest_sha256.clone(),
            engineer_identity: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            rationale: command.rationale.trim().into(),
            conditions: trimmed(&command.conditions),
            exceptions: trimmed(&command.exceptions),
            approved_at: approved_at.clone(),
            manifest_sha256: String::new(),
        };
        approval.manifest_sha256 = manifest_sha256(&approval)?;
        verify_exact_package_bytes(&self.root.join("content"), &package, budget)?;
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "submission_release_finally_approved",
            package.package.tender_revision,
            json!({
                "approval_id": approval.approval_id,
                "canonical_manifest_root": approval.canonical_manifest_root,
                "package_id": approval.package_id,
                "package_version": approval.package_version.to_string(),
                "package_manifest_sha256": approval.package_manifest_sha256,
                "readiness_report_id": approval.readiness_report_id,
                "readiness_report_version": approval.readiness_report_version.to_string(),
                "approval_manifest_sha256": approval.manifest_sha256,
                "submission_claimed": false,
            }),
            &approved_at,
        )?;
        transaction
            .execute(
                "INSERT INTO submission_release_approvals (
                   approval_id, package_id, package_version, package_manifest_sha256,
                   canonical_manifest_root, readiness_report_id, readiness_report_version,
                   readiness_report_manifest_sha256, approval_json, manifest_sha256,
                   audit_sequence, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    approval.approval_id,
                    approval.package_id,
                    approval.package_version,
                    approval.package_manifest_sha256,
                    approval.canonical_manifest_root,
                    approval.readiness_report_id,
                    approval.readiness_report_version,
                    approval.readiness_report_manifest_sha256,
                    canonical_json(&approval)?,
                    approval.manifest_sha256,
                    audit_sequence,
                    approval.approved_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        self.inspect_submission_release(budget)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
    }

    fn inspect_submission_release(
        &mut self,
        budget: BidPackageOperationBudget,
    ) -> Result<Option<SubmissionReleaseInspection>, TenderCommandError> {
        let head: Option<(String, u32, String)> = self
            .connection
            .query_row(
                "SELECT package_id, current_version, versions.manifest_sha256
                 FROM submission_package_head AS head
                 JOIN submission_package_versions AS versions
                   ON versions.package_id = head.package_id AND versions.version = head.current_version
                 WHERE head.singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((package_id, package_version, package_manifest_sha256)) = head else {
            return Ok(None);
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(sql_error)?;
        let package = load_submission_package_for_review_transaction(
            &transaction,
            &package_id,
            package_version,
            &package_manifest_sha256,
            false,
            budget,
        )?;
        let final_review = load_final_review_for_transaction(&transaction, &package, None, budget)?;
        let approval = load_approval(&transaction, &package_id, package_version)?;
        let exports = approval
            .as_ref()
            .map(|approval| load_exports(&transaction, &approval.approval_id))
            .transpose()?
            .unwrap_or_default();
        let bytes_current =
            verify_exact_package_bytes(&self.root.join("content"), &package, budget).is_ok();
        let state = match approval.as_ref() {
            None => SubmissionReleaseState::NotApproved,
            Some(approval)
                if final_review.current
                    && final_review.ready
                    && bytes_current
                    && deadline_is_current(&transaction, &final_review).unwrap_or(false)
                    && approval.package_manifest_sha256 == package.package.manifest_sha256
                    && approval.readiness_report_manifest_sha256
                        == final_review.report.manifest_sha256 =>
            {
                SubmissionReleaseState::ReadyForSubmission
            }
            Some(_) => SubmissionReleaseState::Revoked,
        };
        transaction.commit().map_err(sql_error)?;
        Ok(Some(SubmissionReleaseInspection {
            final_review,
            approval,
            state,
            exports,
            submission_claimed: false,
        }))
    }

    fn export_release_copy(
        &mut self,
        application_home: &Path,
        tender_id: &TenderId,
        command: &ExportReleaseCopyCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<SubmissionReleaseInspection, TenderCommandError> {
        self.require_storage_writable()?;
        let inspection = self
            .inspect_submission_release(budget)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let approval = inspection
            .approval
            .as_ref()
            .filter(|approval| {
                approval.approval_id == command.approval_id
                    && approval.manifest_sha256 == command.approval_manifest_sha256
                    && inspection.state == SubmissionReleaseState::ReadyForSubmission
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM release_copy_exports WHERE approval_id = ?1",
                [&approval.approval_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if count >= MAX_RELEASE_EXPORTS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(sql_error)?;
        let package = load_submission_package_for_review_transaction(
            &transaction,
            &approval.package_id,
            approval.package_version,
            &approval.package_manifest_sha256,
            true,
            budget,
        )?;
        let bytes = package
            .items
            .iter()
            .map(|item| {
                budget.check()?;
                let content = load_exact_submission_package_item_bytes_for_transaction(
                    &self.root.join("content"),
                    item,
                )?;
                Ok((item.item.clone(), content))
            })
            .collect::<Result<Vec<_>, TenderCommandError>>()?;
        transaction.commit().map_err(sql_error)?;

        let export_id = random_identifier(&self.connection)?;
        let relative_path = format!(
            "{}/release-{}-{}",
            tender_id.as_str(),
            approval.approval_id,
            export_id
        );
        let exports_root = application_home.join("exports");
        let tender_exports_root = exports_root.join(tender_id.as_str());
        let destination =
            tender_exports_root.join(format!("release-{}-{}", approval.approval_id, export_id));
        let staging_root = application_home.join("staging");
        let staging = staging_root.join(format!("release-copy-{export_id}"));
        ensure_managed_directory(application_home, &exports_root)?;
        ensure_managed_directory(application_home, &staging_root)?;
        ensure_managed_directory(&exports_root, &tender_exports_root)?;
        if destination.exists() || staging.exists() {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
        fs::create_dir(&staging).map_err(store_unavailable)?;
        let staged = (|| {
            for (item, content) in &bytes {
                budget.check()?;
                let relative = safe_release_path(&item.package_path)?;
                let path = staging.join(relative);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(store_unavailable)?;
                }
                let mut file = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .map_err(store_unavailable)?;
                file.write_all(content).map_err(store_unavailable)?;
                file.sync_all().map_err(store_unavailable)?;
            }
            verify_release_directory(&staging, &bytes, budget)
        })();
        if let Err(error) = staged {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        fs::rename(&staging, &destination).map_err(store_unavailable)?;
        if let Err(error) = verify_release_directory(&destination, &bytes, budget) {
            let _ = fs::remove_dir_all(&destination);
            return Err(error);
        }

        let publication = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let exact = load_submission_package_for_review_transaction(
                &transaction,
                &approval.package_id,
                approval.package_version,
                &approval.package_manifest_sha256,
                true,
                budget,
            )?;
            let final_review =
                load_final_review_for_transaction(&transaction, &exact, None, budget)?;
            ensure_deadline_current(&transaction, &final_review)?;
            if !final_review.ready
                || final_review.report.manifest_sha256 != approval.readiness_report_manifest_sha256
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            verify_release_directory(&destination, &bytes, budget)?;
            let created_at = sqlite_timestamp(&transaction)?;
            let items = bytes
                .iter()
                .map(|(item, content)| ReleaseCopyItem {
                    package_path: item.package_path.clone(),
                    content_sha256: sha256_hex(content),
                    size_bytes: u64::try_from(content.len()).unwrap_or(u64::MAX),
                })
                .collect::<Vec<_>>();
            let mut export = ReleaseCopyExport {
                export_id: export_id.clone(),
                approval_id: approval.approval_id.clone(),
                relative_path: relative_path.clone(),
                items,
                verified: true,
                created_at: created_at.clone(),
                manifest_sha256: String::new(),
            };
            export.manifest_sha256 = manifest_sha256(&export)?;
            let audit_sequence = append_audit_event_with_sequence(
                &transaction,
                tender_id.as_str(),
                "release_copy_verified",
                exact.package.tender_revision,
                json!({
                    "approval_id": approval.approval_id,
                    "export_id": export.export_id,
                    "manifest_sha256": export.manifest_sha256,
                    "relative_path": export.relative_path,
                    "submission_claimed": false,
                }),
                &created_at,
            )?;
            transaction
                .execute(
                    "INSERT INTO release_copy_exports (
                       export_id, approval_id, relative_path, export_json,
                       manifest_sha256, audit_sequence, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        export.export_id,
                        export.approval_id,
                        export.relative_path,
                        canonical_json(&export)?,
                        export.manifest_sha256,
                        audit_sequence,
                        export.created_at,
                    ],
                )
                .map_err(sql_error)?;
            transaction.commit().map_err(sql_error)
        })();
        if let Err(error) = publication {
            let _ = fs::remove_dir_all(&destination);
            return Err(error);
        }
        self.inspect_submission_release(budget)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
    }
}

fn load_approval(
    transaction: &Transaction<'_>,
    package_id: &str,
    package_version: u32,
) -> Result<Option<SubmissionReleaseApproval>, TenderCommandError> {
    let json = transaction
        .query_row(
            "SELECT approval_json FROM submission_release_approvals
             WHERE package_id = ?1 AND package_version = ?2",
            params![package_id, package_version],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    json.map(|json| parse_manifest(&json)).transpose()
}

fn load_exports(
    transaction: &Transaction<'_>,
    approval_id: &str,
) -> Result<Vec<ReleaseCopyExport>, TenderCommandError> {
    let mut statement = transaction
        .prepare(
            "SELECT export_json FROM release_copy_exports
             WHERE approval_id = ?1 ORDER BY audit_sequence",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([approval_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    let mut exports = Vec::new();
    for row in rows {
        exports.push(parse_manifest(&row.map_err(sql_error)?)?);
    }
    Ok(exports)
}

fn verify_exact_package_bytes(
    content_root: &Path,
    package: &ExactSubmissionPackage,
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    for item in &package.items {
        budget.check()?;
        let bytes = load_exact_submission_package_item_bytes_for_transaction(content_root, item)?;
        if sha256_hex(&bytes) != item.item.content_sha256
            || u64::try_from(bytes.len()).ok() != Some(item.item.size_bytes)
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    Ok(())
}

fn canonical_manifest_root(package: &ExactSubmissionPackage) -> Result<String, TenderCommandError> {
    let entries = package
        .items
        .iter()
        .map(|item| {
            json!({
                "content_sha256": item.item.content_sha256,
                "package_path": item.item.package_path,
                "size_bytes": item.item.size_bytes.to_string(),
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "package_id": package.package.package_id,
        "package_version": package.package.version.to_string(),
        "package_manifest_sha256": package.package.manifest_sha256,
        "items": entries,
    });
    let canonical = serde_json_canonicalizer::to_vec(&value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(sha256_hex(&canonical))
}

fn ensure_deadline_current(
    transaction: &Transaction<'_>,
    final_review: &FinalReviewInspection,
) -> Result<(), TenderCommandError> {
    if deadline_is_current(transaction, final_review)? {
        Ok(())
    } else {
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }
}

fn deadline_is_current(
    transaction: &Transaction<'_>,
    final_review: &FinalReviewInspection,
) -> Result<bool, TenderCommandError> {
    let Some(deadline) = final_review.package.submission_deadline.as_ref() else {
        return Ok(false);
    };
    let fields_json = transaction
        .query_row(
            "SELECT versions.fields_json
             FROM tender_record_versions AS versions
             JOIN tender_record_heads AS heads ON heads.record_id = versions.record_id
             JOIN tender_records AS records ON records.record_id = versions.record_id
             WHERE versions.record_id = ?1
               AND versions.version = ?2
               AND versions.manifest_sha256 = ?3
               AND versions.kind = 'deadline'
               AND records.stable_key = 'submission_deadline'
               AND heads.current_version = versions.version",
            params![
                deadline.reference_id,
                deadline.version,
                deadline.manifest_sha256
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(fields_json) = fields_json else {
        return Ok(false);
    };
    let fields: Vec<TenderRecordFieldCandidate> = serde_json::from_str(&fields_json)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if fields.len() != 1
        || fields[0].uncertainty.is_some()
        || fields[0]
            .normalized_value
            .as_deref()
            .is_none_or(|value| value.parse::<Timestamp>().is_err())
    {
        return Ok(false);
    }
    let now = Timestamp::now()
        .round(jiff::Unit::Millisecond)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let deadline = fields[0]
        .normalized_value
        .as_deref()
        .and_then(|value| value.parse::<Timestamp>().ok())
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(now < deadline)
}

fn validate_final_text(
    rationale: &str,
    conditions: &[String],
    exceptions: &[String],
) -> Result<(), TenderCommandError> {
    if rationale.trim().is_empty()
        || rationale.len() > MAX_FINAL_TEXT_BYTES
        || conditions.len() > MAX_FINAL_LIST_ITEMS
        || exceptions.len() > MAX_FINAL_LIST_ITEMS
        || conditions
            .iter()
            .chain(exceptions)
            .any(|value| value.trim().is_empty() || value.as_bytes().len() > MAX_FINAL_TEXT_BYTES)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn trimmed(values: &[String]) -> Vec<String> {
    values.iter().map(|value| value.trim().to_owned()).collect()
}

fn safe_release_path(value: &str) -> Result<PathBuf, TenderCommandError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(path.to_path_buf())
}

fn verify_release_directory(
    root: &Path,
    items: &[(super::SubmissionPackageItem, Vec<u8>)],
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    let expected_paths = items
        .iter()
        .map(|(item, _)| safe_release_path(&item.package_path))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut actual_paths = BTreeSet::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        budget.check()?;
        let entry =
            entry.map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if entry.path() == root {
            continue;
        }
        if entry.file_type().is_symlink()
            || (!entry.file_type().is_dir() && !entry.file_type().is_file())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        if entry.file_type().is_file() {
            actual_paths.insert(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
                    .to_path_buf(),
            );
        }
    }
    if actual_paths != expected_paths {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for (item, _) in items {
        budget.check()?;
        let path = root.join(safe_release_path(&item.package_path)?);
        let mut file = fs::File::open(path).map_err(store_unavailable)?;
        let mut hasher = Sha256::new();
        let mut size = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            budget.check()?;
            let read = file.read(&mut buffer).map_err(store_unavailable)?;
            if read == 0 {
                break;
            }
            size = size
                .checked_add(u64::try_from(read).unwrap_or(u64::MAX))
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if size > item.size_bytes {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            hasher.update(&buffer[..read]);
        }
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if size != item.size_bytes || digest != item.content_sha256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    Ok(())
}

fn ensure_managed_directory(
    application_home: &Path,
    directory: &Path,
) -> Result<(), TenderCommandError> {
    if directory.parent() != Some(application_home) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    if directory.exists() {
        let metadata = fs::symlink_metadata(directory).map_err(store_unavailable)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
    } else {
        fs::create_dir(directory).map_err(store_unavailable)?;
    }
    Ok(())
}

fn manifest_sha256<T: Serialize + Clone>(value: &T) -> Result<String, TenderCommandError> {
    let mut object = serde_json::to_value(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    object
        .as_object_mut()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
        .insert("manifest_sha256".into(), Value::String(String::new()));
    let bytes = serde_json_canonicalizer::to_vec(&object)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(sha256_hex(&bytes))
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn parse_manifest<T: for<'de> Deserialize<'de> + Serialize + Clone>(
    value: &str,
) -> Result<T, TenderCommandError> {
    let parsed: T = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let declared_manifest = serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|value| value.get("manifest_sha256")?.as_str().map(str::to_owned));
    let expected_manifest = manifest_sha256(&parsed)?;
    if canonical_json(&parsed)? != value
        || declared_manifest.as_deref() != Some(expected_manifest.as_str())
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

fn store_unavailable(_: std::io::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}
