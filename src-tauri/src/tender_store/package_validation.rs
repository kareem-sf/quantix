use std::{
    collections::BTreeSet,
    fs,
    io::{Cursor, Read},
    path::Path,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::agent_runtime::{
    permissions::{derive_planned_task_grant, permission_duration, PlannedTaskGrantRequest},
    AgentProfileVersionView, AgentRunInspection, AgentTaskInputReference, PendingProviderEvent,
    PreparedAgentRun, ProviderEventKind, TenderTaskView,
};

use super::submission_packages::{
    load_exact_submission_package_item_bytes_for_transaction,
    load_submission_package_for_review_transaction, load_submission_package_snapshot_for_integrity,
    ExactSubmissionPackage, ExactSubmissionPackageItem,
};
use super::{
    agent_records::{
        ensure_agent_run_capacity, insert_event, insert_task, load_profile, load_thread_exposure,
    },
    append_audit_event_with_sequence, lock_mutex_with_check, random_identifier, sha256_hex,
    sql_error, sqlite_timestamp, BidPackageOperationBudget, CoordinatedBidBaselineBinding,
    CoordinatedBidBaselineBindingKind, CoordinatedBidBaselineCategory, ProductionFindingSeverity,
    QuantixHost, SubmissionCoverageRow, SubmissionPackageAssessment,
    SubmissionPackageCurrentnessFact, SubmissionPackageItem, SubmissionPackageSection,
    SubmissionPackageVersion, SubmissionProfileVersionReference, TenderCommandError,
    TenderErrorCode, TenderId, TenderStore, WorkPlanProfileBinding,
};

const VALIDATOR_VERSION: u32 = 1;
const RENDERER_VERSION: u32 = 1;
const CHECK_VERSION: u32 = 1;
const MAX_RESULTS: usize = 65_536;
const MAX_REVIEW_FINDINGS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunPackageValidationCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1, max = 32))]
    pub package_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub package_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RecordPackageManualVerificationCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1, max = 32))]
    pub package_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub package_manifest_sha256: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub validation_run_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub validation_result_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub item_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub content_sha256: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub capability: String,
    #[garde(length(min = 1, max = 32), inner(length(bytes, min = 1, max = 500)))]
    pub checks: Vec<String>,
    #[garde(length(min = 1, max = 256), inner(length(bytes, min = 1, max = 1000)))]
    pub evidence_references: Vec<String>,
    #[garde(skip)]
    pub result: ManualVerificationResult,
    #[garde(length(max = 32), inner(length(bytes, min = 1, max = 1000)))]
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunSubmissionSectionReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1, max = 32))]
    pub package_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub package_manifest_sha256: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub plan_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub plan_manifest_sha256: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub assignment_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApprovePackageFindingExceptionCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1, max = 32))]
    pub package_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub package_manifest_sha256: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub review_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub finding_id: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PackageValidationCheckCategory {
    FileStructure,
    Rendering,
    Calculation,
    CrossArtifactConsistency,
    HiddenContent,
    InformationBoundary,
    Filename,
    Hash,
    PackageWide,
}

impl PackageValidationCheckCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::FileStructure => "file_structure",
            Self::Rendering => "rendering",
            Self::Calculation => "calculation",
            Self::CrossArtifactConsistency => "cross_artifact_consistency",
            Self::HiddenContent => "hidden_content",
            Self::InformationBoundary => "information_boundary",
            Self::Filename => "filename",
            Self::Hash => "hash",
            Self::PackageWide => "package_wide",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PackageValidationOutcome {
    Passed,
    Failed,
    ManualVerificationRequired,
}

impl PackageValidationOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::ManualVerificationRequired => "manual_verification_required",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ManualVerificationResult {
    Passed,
    Failed,
}

impl ManualVerificationResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PackageReviewResult {
    Satisfied,
    RequiresRemediation,
}

impl PackageReviewResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::RequiresRemediation => "requires_remediation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageValidationRule {
    pub rule_id: String,
    pub category: PackageValidationCheckCategory,
    pub severity: ProductionFindingSeverity,
    pub deterministic: bool,
    pub source: Option<super::SubmissionValidationContextInput>,
    pub manual_checklist: Vec<String>,
    pub major_exception_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageValidationPolicy {
    pub policy_id: String,
    pub version: u32,
    pub fixed_rules: Vec<PackageValidationRule>,
    pub tender_rules: Vec<PackageValidationRule>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageValidationResult {
    pub result_id: String,
    pub item_id: Option<String>,
    pub content_sha256: Option<String>,
    pub validation_context_sha256: String,
    pub check_id: String,
    pub check_version: u32,
    pub category: PackageValidationCheckCategory,
    pub outcome: PackageValidationOutcome,
    pub detail: String,
    pub evidence_references: Vec<String>,
    pub reused_from_result_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageValidationRun {
    pub run_id: String,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub policy_id: String,
    pub policy_version: u32,
    pub policy_manifest_sha256: String,
    pub validator_version: u32,
    pub renderer_version: u32,
    pub context_sha256: String,
    pub results: Vec<PackageValidationResult>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageManualVerification {
    pub verification_id: String,
    pub validation_result_id: String,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub item_id: String,
    pub content_sha256: String,
    pub verifier_identity: String,
    pub capability: String,
    pub checks: Vec<String>,
    pub evidence_references: Vec<String>,
    pub result: ManualVerificationResult,
    pub limitations: Vec<String>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FinalReviewReviewer {
    pub profile_id: String,
    pub profile_version: u32,
    pub identity: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FinalReviewAssignment {
    pub assignment_id: String,
    pub section_key: String,
    pub envelope_key: String,
    pub language: String,
    pub item_ids: Vec<String>,
    pub required_capability: String,
    pub risk_references: Vec<super::SubmissionPackageDependency>,
    pub author_profile_versions: Vec<SubmissionProfileVersionReference>,
    pub reviewer: Option<FinalReviewReviewer>,
    pub criteria: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FinalReviewPlan {
    pub plan_id: String,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub validation_run_id: String,
    pub policy_manifest_sha256: String,
    pub assignments: Vec<FinalReviewAssignment>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageFindingExceptionApproval {
    pub approval_id: String,
    pub finding_id: String,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub decided_by: String,
    pub acting_role: String,
    pub rationale: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageReviewFinding {
    pub finding_id: String,
    pub review_id: String,
    pub sequence: u32,
    pub severity: ProductionFindingSeverity,
    pub policy_rule_id: String,
    pub summary: String,
    pub evidence_references: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionSectionReview {
    pub review_id: String,
    pub assignment_id: String,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub reviewer_run_id: String,
    pub reviewer: FinalReviewReviewer,
    pub required_capability: String,
    pub criteria: Vec<String>,
    pub result: PackageReviewResult,
    pub findings: Vec<PackageReviewFinding>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ReleaseReadinessBlockerCode {
    PackageIncomplete,
    ValidationFailed,
    ManualVerificationMissing,
    ManualVerificationFailed,
    ReviewerUnqualified,
    ReviewMissing,
    CriticalFinding,
    MajorFinding,
    StaleInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReleaseReadinessBlocker {
    pub code: ReleaseReadinessBlockerCode,
    pub reference_id: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReleaseReadinessCategorySummary {
    pub category: String,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReleaseReadinessReport {
    pub report_id: String,
    pub version: u32,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub policy_manifest_sha256: String,
    pub validation_run_id: String,
    pub validation_manifest_sha256: String,
    pub review_plan_id: String,
    pub review_plan_manifest_sha256: String,
    pub review_ids: Vec<String>,
    pub manual_verification_ids: Vec<String>,
    pub exception_approval_ids: Vec<String>,
    pub through_event_sequence: u64,
    pub summaries: Vec<ReleaseReadinessCategorySummary>,
    pub deadline: Option<super::SubmissionPackageDependency>,
    pub changes: Vec<SubmissionPackageCurrentnessFact>,
    pub blockers: Vec<ReleaseReadinessBlocker>,
    pub current: bool,
    pub ready: bool,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FinalReviewDecisionEvidence {
    pub category: FinalReviewDecisionEvidenceCategory,
    pub binding: CoordinatedBidBaselineBinding,
    pub question: Option<String>,
    pub ambiguity_or_gap: Option<String>,
    pub treatment: Option<String>,
    pub rationale: Option<String>,
    pub treatment_details: Option<String>,
    pub closed: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FinalReviewDecisionEvidenceCategory {
    Assumption,
    Qualification,
    Exclusion,
    OpenQuery,
    OtherDecision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FinalReviewInspection {
    pub package: SubmissionPackageVersion,
    pub decision_evidence: Vec<FinalReviewDecisionEvidence>,
    pub policy: PackageValidationPolicy,
    pub validation_run: PackageValidationRun,
    pub manual_verifications: Vec<PackageManualVerification>,
    pub review_plan: FinalReviewPlan,
    pub reviews: Vec<SubmissionSectionReview>,
    pub exceptions: Vec<PackageFindingExceptionApproval>,
    pub report: ReleaseReadinessReport,
    pub current: bool,
    pub ready: bool,
    pub live_blockers: Vec<ReleaseReadinessBlocker>,
    pub live_changes: Vec<SubmissionPackageCurrentnessFact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionSectionReviewRunResult {
    pub run: AgentRunInspection,
    pub final_review: FinalReviewInspection,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubmissionSectionReviewCandidate {
    pub assignment_id: String,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub result: PackageReviewResult,
    pub findings: Vec<SubmissionSectionReviewFindingCandidate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubmissionSectionReviewFindingCandidate {
    pub severity: ProductionFindingSeverity,
    pub policy_rule_id: String,
    pub summary: String,
    pub evidence_references: Vec<String>,
}

impl QuantixHost {
    pub fn run_package_validation(
        &self,
        command: RunPackageValidationCommand,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .run_package_validation(&tender_id, &command, budget);
        result
    }

    pub fn record_package_manual_verification(
        &self,
        command: RecordPackageManualVerificationCommand,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .record_package_manual_verification(&tender_id, &command, budget);
        result
    }

    pub fn approve_package_finding_exception(
        &self,
        command: ApprovePackageFindingExceptionCommand,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .approve_package_finding_exception(&tender_id, &command, budget);
        result
    }

    pub fn inspect_final_review(
        &self,
        tender_id: &str,
    ) -> Result<Option<FinalReviewInspection>, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result =
            lock_mutex_with_check(&store, &mut || budget.check())?.inspect_final_review(budget);
        result
    }
}

impl TenderStore {
    fn run_package_validation(
        &mut self,
        tender_id: &TenderId,
        command: &RunPackageValidationCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
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
        if package.package.assessment != SubmissionPackageAssessment::Complete {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let lifecycle: String = transaction
            .query_row(
                "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if lifecycle != "final_review" {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if let Some(run_id) = transaction
            .query_row(
                "SELECT run_id FROM package_validation_runs
                 WHERE package_id = ?1 AND package_version = ?2
                   AND package_manifest_sha256 = ?3",
                params![
                    command.package_id,
                    command.package_version,
                    command.package_manifest_sha256
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
        {
            let inspection =
                load_final_review_for_transaction(&transaction, &package, Some(&run_id), budget)?;
            transaction.commit().map_err(sql_error)?;
            return Ok(inspection);
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let policy = build_and_persist_policy(&transaction, &package.package, &created_at)?;
        let run = execute_and_persist_validation(
            &transaction,
            tender_id,
            &self.root.join("content"),
            &package,
            &policy,
            &created_at,
            budget,
        )?;
        let plan = build_and_persist_review_plan(
            &transaction,
            &package.package,
            &run,
            &policy,
            &created_at,
        )?;
        let through_event_sequence: i64 = transaction
            .query_row(
                "SELECT audit_sequence FROM package_validation_runs WHERE run_id = ?1",
                [&run.run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        persist_release_readiness_report(
            &transaction,
            &package.package,
            &policy,
            &run,
            &plan,
            through_event_sequence,
            &created_at,
        )?;
        let inspection =
            load_final_review_for_transaction(&transaction, &package, Some(&run.run_id), budget)?;
        transaction.commit().map_err(sql_error)?;
        Ok(inspection)
    }

    fn record_package_manual_verification(
        &mut self,
        tender_id: &TenderId,
        command: &RecordPackageManualVerificationCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
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
        let (policy, run, plan) = load_policy_run_plan(&transaction, &package.package)?;
        if run.run_id != command.validation_run_id {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let result_json: String = transaction
            .query_row(
                "SELECT result_json FROM package_validation_item_results
                 WHERE result_id = ?1 AND run_id = ?2 AND item_id = ?3
                   AND content_sha256 = ?4 AND outcome = 'manual_verification_required'",
                params![
                    command.validation_result_id,
                    command.validation_run_id,
                    command.item_id,
                    command.content_sha256
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let result: PackageValidationResult = parse_canonical(&result_json)?;
        if result.result_id != command.validation_result_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let rule = policy
            .fixed_rules
            .iter()
            .chain(&policy.tender_rules)
            .find(|rule| rule.rule_id == result.check_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let exact_checks = normalized_nonempty(&rule.manual_checklist)?;
        let supplied_checks = normalized_nonempty(&command.checks)?;
        let item = package
            .package
            .items
            .iter()
            .find(|item| item.item_id == command.item_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let section = package
            .package
            .sections
            .iter()
            .find(|section| section.item_ids.contains(&item.item_id))
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let evidence_references = normalized_nonempty(&command.evidence_references)?;
        let allowed_evidence = item
            .evidence
            .iter()
            .map(|evidence| {
                format!(
                    "source:{}:{}:{}",
                    evidence.reference.artifact_id,
                    evidence.reference.version,
                    evidence.reference.ordinal
                )
            })
            .collect::<BTreeSet<_>>();
        if supplied_checks != exact_checks
            || !section.required_capabilities.contains(&command.capability)
            || evidence_references
                .iter()
                .any(|reference| !allowed_evidence.contains(reference))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let mut verification = PackageManualVerification {
            verification_id: random_identifier(&transaction)?,
            validation_result_id: command.validation_result_id.clone(),
            package_id: command.package_id.clone(),
            package_version: command.package_version,
            package_manifest_sha256: command.package_manifest_sha256.clone(),
            item_id: command.item_id.clone(),
            content_sha256: command.content_sha256.clone(),
            verifier_identity: "engineer_user".into(),
            capability: command.capability.trim().into(),
            checks: supplied_checks,
            evidence_references,
            result: command.result,
            limitations: normalized(&command.limitations)?,
            manifest_sha256: String::new(),
            created_at: created_at.clone(),
        };
        verification.manifest_sha256 = manifest_sha256(&verification)?;
        transaction
            .execute(
                "INSERT INTO package_manual_verifications (
                   verification_id, validation_result_id, package_id, package_version,
                   package_manifest_sha256, item_id, content_sha256, capability, result,
                   verification_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    verification.verification_id,
                    verification.validation_result_id,
                    verification.package_id,
                    verification.package_version,
                    verification.package_manifest_sha256,
                    verification.item_id,
                    verification.content_sha256,
                    verification.capability,
                    verification.result.as_str(),
                    canonical_json(&verification)?,
                    verification.manifest_sha256,
                    verification.created_at,
                ],
            )
            .map_err(|error| {
                if error.to_string().contains("UNIQUE") {
                    TenderCommandError::new(TenderErrorCode::InvalidCommand)
                } else {
                    sql_error(error)
                }
            })?;
        let through_event_sequence = append_review_audit(
            &transaction,
            tender_id,
            package.package.tender_revision,
            "package_manual_verification_recorded",
            &verification.verification_id,
            &created_at,
        )?;
        persist_release_readiness_report(
            &transaction,
            &package.package,
            &policy,
            &run,
            &plan,
            through_event_sequence,
            &created_at,
        )?;
        let inspection =
            load_final_review_for_transaction(&transaction, &package, Some(&run.run_id), budget)?;
        transaction.commit().map_err(sql_error)?;
        Ok(inspection)
    }

    fn approve_package_finding_exception(
        &mut self,
        tender_id: &TenderId,
        command: &ApprovePackageFindingExceptionCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
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
        let (policy, run, plan) = load_policy_run_plan(&transaction, &package.package)?;
        let (severity, rule_id): (String, String) = transaction
            .query_row(
                "SELECT findings.severity, findings.policy_rule_id
                 FROM submission_section_review_findings AS findings
                 JOIN submission_section_reviews AS reviews USING (review_id)
                 WHERE findings.finding_id = ?1 AND findings.review_id = ?2
                   AND reviews.package_id = ?3 AND reviews.package_version = ?4
                   AND reviews.package_manifest_sha256 = ?5",
                params![
                    command.finding_id,
                    command.review_id,
                    command.package_id,
                    command.package_version,
                    command.package_manifest_sha256
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let exception_allowed = policy
            .fixed_rules
            .iter()
            .chain(&policy.tender_rules)
            .any(|rule| rule.rule_id == rule_id && rule.major_exception_allowed);
        if severity != "major" || !exception_allowed {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let mut approval = PackageFindingExceptionApproval {
            approval_id: random_identifier(&transaction)?,
            finding_id: command.finding_id.clone(),
            package_id: command.package_id.clone(),
            package_version: command.package_version,
            package_manifest_sha256: command.package_manifest_sha256.clone(),
            decided_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            rationale: command.rationale.trim().into(),
            manifest_sha256: String::new(),
            created_at: created_at.clone(),
        };
        approval.manifest_sha256 = manifest_sha256(&approval)?;
        transaction
            .execute(
                "INSERT INTO package_finding_exception_approvals (
                   approval_id, finding_id, package_id, package_version,
                   package_manifest_sha256, approval_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    approval.approval_id,
                    approval.finding_id,
                    approval.package_id,
                    approval.package_version,
                    approval.package_manifest_sha256,
                    canonical_json(&approval)?,
                    approval.manifest_sha256,
                    approval.created_at
                ],
            )
            .map_err(sql_error)?;
        let through_event_sequence = append_review_audit(
            &transaction,
            tender_id,
            package.package.tender_revision,
            "package_finding_exception_approved",
            &approval.approval_id,
            &created_at,
        )?;
        persist_release_readiness_report(
            &transaction,
            &package.package,
            &policy,
            &run,
            &plan,
            through_event_sequence,
            &created_at,
        )?;
        let inspection =
            load_final_review_for_transaction(&transaction, &package, Some(&run.run_id), budget)?;
        transaction.commit().map_err(sql_error)?;
        Ok(inspection)
    }

    pub(crate) fn inspect_final_review(
        &mut self,
        budget: BidPackageOperationBudget,
    ) -> Result<Option<FinalReviewInspection>, TenderCommandError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(sql_error)?;
        let head: Option<(String, u32, String, String)> = transaction
            .query_row(
                "SELECT runs.package_id, runs.package_version, runs.package_manifest_sha256, runs.run_id
                 FROM package_validation_runs AS runs
                 JOIN submission_package_head AS head
                   ON head.package_id = runs.package_id AND head.current_version = runs.package_version",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let inspection = head
            .map(|(package_id, version, manifest_sha256, run_id)| {
                let package = load_submission_package_for_review_transaction(
                    &transaction,
                    &package_id,
                    version,
                    &manifest_sha256,
                    false,
                    budget,
                )?;
                load_final_review_for_transaction(&transaction, &package, Some(&run_id), budget)
            })
            .transpose()?;
        transaction.commit().map_err(sql_error)?;
        Ok(inspection)
    }

    pub(crate) fn package_validation_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        for (table, json_column) in [
            ("package_validation_policies", "manifest_json"),
            ("package_validation_runs", "manifest_json"),
            ("package_manual_verifications", "verification_json"),
            ("final_review_plans", "plan_json"),
            ("submission_section_reviews", "review_json"),
            ("package_finding_exception_approvals", "approval_json"),
            ("release_readiness_reports", "report_json"),
        ] {
            check()?;
            let sql = format!("SELECT {json_column}, manifest_sha256 FROM {table}");
            let mut statement = self.connection.prepare(&sql).map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(sql_error)?;
            for row in rows {
                check()?;
                let (json, expected): (String, String) = row.map_err(sql_error)?;
                if !stored_manifest_hash_is_valid(&json, &expected) {
                    return Ok(false);
                }
            }
        }
        package_validation_relations_are_valid(&self.connection, check)
    }
}

fn package_validation_relations_are_valid(
    connection: &Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    check()?;
    let mut policies = connection
        .prepare(
            "SELECT policy_id, version, manifest_json, manifest_sha256, created_at
             FROM package_validation_policies ORDER BY policy_id, version",
        )
        .map_err(sql_error)?;
    let policy_rows = policies
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(sql_error)?;
    for row in policy_rows {
        check()?;
        let (policy_id, version, json, manifest_sha256, created_at) = row.map_err(sql_error)?;
        let Ok(policy) = parse_canonical::<PackageValidationPolicy>(&json) else {
            return Ok(false);
        };
        if policy.policy_id != policy_id
            || policy.version != version
            || policy.fixed_rules != fixed_policy_rules()
            || policy.manifest_sha256 != manifest_sha256
            || policy.created_at != created_at
        {
            return Ok(false);
        }
    }
    let policy_chain_is_valid: bool = connection
        .query_row(
            "SELECT CASE WHEN COUNT(*) = 0 THEN 1 ELSE
               COUNT(DISTINCT policy_id) = 1 AND MIN(version) = 1 AND MAX(version) = COUNT(*) END
             FROM package_validation_policies",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if !policy_chain_is_valid {
        return Ok(false);
    }

    let mut runs = connection
        .prepare(
            "SELECT run_id, package_id, package_version, package_manifest_sha256,
                    policy_id, policy_version, policy_manifest_sha256, validator_version,
                    renderer_version, context_sha256, manifest_json, manifest_sha256,
                    audit_sequence, created_at
             FROM package_validation_runs ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let run_rows = runs
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, u32>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, u32>(7)?,
                row.get::<_, u32>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, String>(13)?,
            ))
        })
        .map_err(sql_error)?;
    for row in run_rows {
        check()?;
        let (
            run_id,
            package_id,
            package_version,
            package_manifest_sha256,
            policy_id,
            policy_version,
            policy_manifest_sha256,
            validator_version,
            renderer_version,
            context_sha256,
            json,
            manifest_sha256,
            audit_sequence,
            created_at,
        ) = row.map_err(sql_error)?;
        let Ok(run) = parse_canonical::<PackageValidationRun>(&json) else {
            return Ok(false);
        };
        let policy_json: Option<String> = connection
            .query_row(
                "SELECT manifest_json FROM package_validation_policies
                 WHERE policy_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
                params![policy_id, policy_version, policy_manifest_sha256],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(policy_json) = policy_json else {
            return Ok(false);
        };
        let policy = parse_canonical::<PackageValidationPolicy>(&policy_json)?;
        let package_context: Option<String> = connection
            .query_row(
                "SELECT validation_context_sha256 FROM submission_package_versions
                 WHERE package_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
                params![package_id, package_version, package_manifest_sha256],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(package_context) = package_context else {
            return Ok(false);
        };
        let package_coverage = load_package_coverage(connection, &package_id, package_version)?;
        let expected_context_sha256 = sha256_hex(
            canonical_json(&json!({
                "package_validation_context": package_context,
                "policy": policy_manifest_sha256,
                "validator_version": validator_version,
                "renderer_version": renderer_version,
            }))?
            .as_bytes(),
        );
        let audit_matches: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM audit_events AS audit
                   JOIN submission_package_versions AS package
                     ON package.package_id = ?2 AND package.version = ?3
                   WHERE audit.sequence = ?1
                     AND audit.event_type = 'package_validation_run_published'
                     AND audit.aggregate_revision = package.tender_revision
                     AND audit.created_at = ?4
                     AND json_extract(audit.payload_json, '$.change.run_id') = ?8
                     AND json_extract(audit.payload_json, '$.change.package_version') = CAST(?3 AS TEXT)
                     AND json_extract(audit.payload_json, '$.change.package_manifest_sha256') = ?5
                     AND json_extract(audit.payload_json, '$.change.policy_manifest_sha256') = ?6
                     AND json_extract(audit.payload_json, '$.change.manifest_sha256') = ?7
                 )",
                params![
                    audit_sequence,
                    package_id,
                    package_version,
                    created_at,
                    package_manifest_sha256,
                    policy_manifest_sha256,
                    manifest_sha256,
                    run_id
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let results = load_validation_results_for_integrity(
            connection,
            &run_id,
            &package_id,
            package_version,
            &policy_manifest_sha256,
        )?;
        if run.run_id != run_id
            || run.package_id != package_id
            || run.package_version != package_version
            || run.package_manifest_sha256 != package_manifest_sha256
            || run.policy_id != policy_id
            || run.policy_version != policy_version
            || run.policy_manifest_sha256 != policy_manifest_sha256
            || run.validator_version != validator_version
            || run.renderer_version != renderer_version
            || run.context_sha256 != context_sha256
            || run.results != results
            || run.manifest_sha256 != manifest_sha256
            || run.created_at != created_at
            || policy.tender_rules != tender_policy_rules(&package_coverage)
            || context_sha256 != expected_context_sha256
            || !audit_matches
        {
            return Ok(false);
        }
    }

    if !review_plans_are_valid(connection, check)?
        || !manual_verifications_are_valid(connection, check)?
        || !section_reviews_are_valid(connection, check)?
        || !finding_exceptions_are_valid(connection, check)?
        || !release_reports_are_valid(connection, check)?
    {
        return Ok(false);
    }
    Ok(true)
}

fn load_package_coverage(
    connection: &Connection,
    package_id: &str,
    package_version: u32,
) -> Result<Vec<SubmissionCoverageRow>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT coverage_json FROM submission_package_coverage
             WHERE package_id = ?1 AND package_version = ?2 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![package_id, package_version], |row| {
            row.get::<_, String>(0)
        })
        .map_err(sql_error)?;
    rows.map(|row| parse_canonical(&row.map_err(sql_error)?))
        .collect()
}

fn load_validation_results_for_integrity(
    connection: &Connection,
    run_id: &str,
    package_id: &str,
    package_version: u32,
    policy_manifest_sha256: &str,
) -> Result<Vec<PackageValidationResult>, TenderCommandError> {
    let mut results = Vec::new();
    let mut items = connection
        .prepare(
            "SELECT result_id, item_id, content_sha256, validation_context_sha256,
                    check_id, check_version, category, outcome, policy_manifest_sha256,
                    reused_from_result_id, result_json
             FROM package_validation_item_results WHERE run_id = ?1 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let item_rows = items
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, u32>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(sql_error)?;
    for row in item_rows {
        let (
            result_id,
            item_id,
            content_sha256,
            validation_context_sha256,
            check_id,
            check_version,
            category,
            outcome,
            stored_policy_sha256,
            reused_from_result_id,
            json,
        ) = row.map_err(sql_error)?;
        let result = parse_canonical::<PackageValidationResult>(&json)?;
        let item_json: Option<String> = connection
            .query_row(
                "SELECT item_json FROM submission_package_items
                 WHERE package_id = ?1 AND package_version = ?2 AND item_id = ?3
                   AND content_sha256 = ?4",
                params![package_id, package_version, item_id, content_sha256],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let item_matches = item_json
            .as_deref()
            .map(parse_canonical::<SubmissionPackageItem>)
            .transpose()?
            .is_some_and(|item| item.validation_context_sha256 == validation_context_sha256);
        let reuse_matches = if let Some(reused) = reused_from_result_id.as_deref() {
            connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM package_validation_item_results AS prior
                       JOIN package_validation_runs AS prior_run ON prior_run.run_id = prior.run_id
                       JOIN package_validation_runs AS current_run ON current_run.run_id = ?8
                       WHERE prior.result_id = ?1 AND prior.result_id <> ?2 AND prior.content_sha256 = ?3
                       AND prior.validation_context_sha256 = ?4 AND prior.check_id = ?5
                       AND prior.check_version = ?6 AND prior.policy_manifest_sha256 = ?7
                       AND prior.outcome = 'passed' AND prior_run.rowid < current_run.rowid)",
                    params![
                        reused,
                        result_id,
                        content_sha256,
                        validation_context_sha256,
                        check_id,
                        check_version,
                        stored_policy_sha256,
                        run_id
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
        } else {
            true
        };
        if result.result_id != result_id
            || result.item_id.as_deref() != Some(item_id.as_str())
            || result.content_sha256.as_deref() != Some(content_sha256.as_str())
            || result.validation_context_sha256 != validation_context_sha256
            || result.check_id != check_id
            || result.check_version != check_version
            || result.category.as_str() != category
            || result.outcome.as_str() != outcome
            || stored_policy_sha256 != policy_manifest_sha256
            || result.reused_from_result_id != reused_from_result_id
            || !item_matches
            || !reuse_matches
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        results.push(result);
    }
    let mut package_results = connection
        .prepare(
            "SELECT result_id, check_id, check_version, category, outcome, result_json
             FROM package_validation_package_results WHERE run_id = ?1 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let package_rows = package_results
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(sql_error)?;
    for row in package_rows {
        let (result_id, check_id, check_version, category, outcome, json) =
            row.map_err(sql_error)?;
        let result = parse_canonical::<PackageValidationResult>(&json)?;
        if result.result_id != result_id
            || result.item_id.is_some()
            || result.content_sha256.is_some()
            || result.check_id != check_id
            || result.check_version != check_version
            || result.category.as_str() != category
            || result.outcome.as_str() != outcome
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        results.push(result);
    }
    Ok(results)
}

fn review_plans_are_valid(
    connection: &Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT plan_id, package_id, package_version, package_manifest_sha256,
                    validation_run_id, policy_manifest_sha256, plan_json,
                    manifest_sha256, created_at
             FROM final_review_plans ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let (
            plan_id,
            package_id,
            package_version,
            package_manifest_sha256,
            validation_run_id,
            policy_manifest_sha256,
            json,
            manifest_sha256,
            created_at,
        ) = row.map_err(sql_error)?;
        let Ok(plan) = parse_canonical::<FinalReviewPlan>(&json) else {
            return Ok(false);
        };
        let run_matches: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM package_validation_runs
                 WHERE run_id = ?1 AND package_id = ?2 AND package_version = ?3
                   AND package_manifest_sha256 = ?4 AND policy_manifest_sha256 = ?5)",
                params![
                    validation_run_id,
                    package_id,
                    package_version,
                    package_manifest_sha256,
                    policy_manifest_sha256
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let mut assignments_statement = connection
            .prepare(
                "SELECT assignment_id, section_key, required_capability,
                        reviewer_profile_id, reviewer_profile_version, assignment_json
                 FROM final_review_assignments WHERE plan_id = ?1 ORDER BY ordinal",
            )
            .map_err(sql_error)?;
        let assignment_rows = assignments_statement
            .query_map([&plan_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<u32>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(sql_error)?;
        let mut assignments = Vec::new();
        for assignment_row in assignment_rows {
            let (
                assignment_id,
                section_key,
                required_capability,
                reviewer_profile_id,
                reviewer_profile_version,
                assignment_json,
            ) = assignment_row.map_err(sql_error)?;
            let assignment = parse_canonical::<FinalReviewAssignment>(&assignment_json)?;
            let reviewer_tuple = assignment
                .reviewer
                .as_ref()
                .map(|reviewer| (reviewer.profile_id.as_str(), reviewer.profile_version));
            let section_matches: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM submission_package_sections
                     WHERE package_id = ?1 AND package_version = ?2 AND section_key = ?3)",
                    params![package_id, package_version, section_key],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if assignment.assignment_id != assignment_id
                || assignment.section_key != section_key
                || assignment.required_capability != required_capability
                || reviewer_tuple != reviewer_profile_id.as_deref().zip(reviewer_profile_version)
                || !section_matches
            {
                return Ok(false);
            }
            assignments.push(assignment);
        }
        let expected_sections = load_package_sections(connection, &package_id, package_version)?;
        let expected_assignments = expected_sections
            .iter()
            .flat_map(|section| {
                let capabilities = if section.required_capabilities.is_empty() {
                    BTreeSet::from(["independent_review".to_owned()])
                } else {
                    section
                        .required_capabilities
                        .iter()
                        .cloned()
                        .collect::<BTreeSet<_>>()
                };
                capabilities
                    .into_iter()
                    .map(|capability| {
                        (
                            section.section_key.clone(),
                            section.envelope_key.clone(),
                            section.language.clone(),
                            capability,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<BTreeSet<_>>();
        let actual_assignments = assignments
            .iter()
            .map(|assignment| {
                (
                    assignment.section_key.clone(),
                    assignment.envelope_key.clone(),
                    assignment.language.clone(),
                    assignment.required_capability.clone(),
                )
            })
            .collect::<BTreeSet<_>>();
        let reviewers_valid = assignments.iter().all(|assignment| {
            expected_sections
                .iter()
                .find(|section| {
                    section.section_key == assignment.section_key
                        && section.envelope_key == assignment.envelope_key
                        && section.language == assignment.language
                })
                .is_some_and(|section| {
                    assignment.item_ids == section.item_ids
                        && assignment.risk_references == section.risk_context.risk_references
                        && assignment.author_profile_versions
                            == section.independence_context.author_profile_versions
                        && assignment.reviewer.as_ref().is_none_or(|reviewer| {
                            exact_reviewer_is_valid(connection, section, assignment, reviewer)
                                .unwrap_or(false)
                        })
                })
        });
        if plan.plan_id != plan_id
            || plan.package_id != package_id
            || plan.package_version != package_version
            || plan.package_manifest_sha256 != package_manifest_sha256
            || plan.validation_run_id != validation_run_id
            || plan.policy_manifest_sha256 != policy_manifest_sha256
            || plan.assignments != assignments
            || plan.manifest_sha256 != manifest_sha256
            || plan.created_at != created_at
            || !run_matches
            || actual_assignments != expected_assignments
            || assignments.len() != actual_assignments.len()
            || !reviewers_valid
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn load_package_sections(
    connection: &Connection,
    package_id: &str,
    package_version: u32,
) -> Result<Vec<SubmissionPackageSection>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT section_json FROM submission_package_sections
             WHERE package_id = ?1 AND package_version = ?2 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![package_id, package_version], |row| {
            row.get::<_, String>(0)
        })
        .map_err(sql_error)?;
    rows.map(|row| parse_canonical(&row.map_err(sql_error)?))
        .collect()
}

fn exact_reviewer_is_valid(
    connection: &Connection,
    section: &SubmissionPackageSection,
    assignment: &FinalReviewAssignment,
    reviewer: &FinalReviewReviewer,
) -> Result<bool, TenderCommandError> {
    let qualification = if assignment.required_capability == "independent_review" {
        "independent_review".to_owned()
    } else {
        format!("review_{}", assignment.required_capability)
    };
    let profile: Option<(String, String)> = connection
        .query_row(
            "SELECT identity, capabilities_json FROM agent_profile_versions
             WHERE profile_id = ?1 AND version = ?2",
            params![reviewer.profile_id, reviewer.profile_version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let Some((identity, capabilities_json)) = profile else {
        return Ok(false);
    };
    let capabilities = parse_canonical::<Vec<String>>(&capabilities_json)?;
    Ok(identity == reviewer.identity
        && capabilities == reviewer.capabilities
        && capabilities.contains(&qualification)
        && section
            .independence_context
            .authorized_profile_versions
            .iter()
            .any(|authorized| {
                authorized.profile_id == reviewer.profile_id
                    && authorized.profile_version == reviewer.profile_version
            })
        && !assignment.author_profile_versions.iter().any(|author| {
            author.profile_id == reviewer.profile_id
                && author.profile_version == reviewer.profile_version
        }))
}

fn manual_verifications_are_valid(
    connection: &Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT verification_id, validation_result_id, package_id, package_version,
                    package_manifest_sha256, item_id, content_sha256, capability, result,
                    verification_json, manifest_sha256, created_at
             FROM package_manual_verifications ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let (
            verification_id,
            validation_result_id,
            package_id,
            package_version,
            package_manifest_sha256,
            item_id,
            content_sha256,
            capability,
            result,
            json,
            manifest_sha256,
            created_at,
        ) = row.map_err(sql_error)?;
        let Ok(verification) = parse_canonical::<PackageManualVerification>(&json) else {
            return Ok(false);
        };
        let result_json: Option<String> = connection
            .query_row(
                "SELECT result.result_json FROM package_validation_item_results AS result
                 JOIN package_validation_runs AS run USING (run_id)
                 WHERE result.result_id = ?1 AND run.package_id = ?2
                   AND run.package_version = ?3 AND run.package_manifest_sha256 = ?4",
                params![
                    validation_result_id,
                    package_id,
                    package_version,
                    package_manifest_sha256
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(result_json) = result_json else {
            return Ok(false);
        };
        let validation_result = parse_canonical::<PackageValidationResult>(&result_json)?;
        let policy_json: String = connection
            .query_row(
                "SELECT policy.manifest_json FROM package_validation_policies AS policy
                 JOIN package_validation_runs AS run
                   ON run.policy_id = policy.policy_id AND run.policy_version = policy.version
                 WHERE run.run_id = (
                   SELECT result.run_id FROM package_validation_item_results AS result
                   WHERE result.result_id = ?1
                 )",
                [&validation_result_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let policy = parse_canonical::<PackageValidationPolicy>(&policy_json)?;
        let policy_rule = policy
            .fixed_rules
            .iter()
            .chain(&policy.tender_rules)
            .find(|rule| rule.rule_id == validation_result.check_id);
        let item_json: String = connection
            .query_row(
                "SELECT item_json FROM submission_package_items
                 WHERE package_id = ?1 AND package_version = ?2 AND item_id = ?3
                   AND content_sha256 = ?4",
                params![package_id, package_version, item_id, content_sha256],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let item = parse_canonical::<SubmissionPackageItem>(&item_json)?;
        let section_json: String = connection
            .query_row(
                "SELECT section_json FROM submission_package_sections
                 WHERE package_id = ?1 AND package_version = ?2
                   AND section_key = ?3 AND envelope_key = ?4 AND language = ?5",
                params![
                    package_id,
                    package_version,
                    item.section_key,
                    item.envelope_key,
                    item.language
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let section = parse_canonical::<SubmissionPackageSection>(&section_json)?;
        let allowed_evidence = item
            .evidence
            .iter()
            .map(|evidence| {
                format!(
                    "source:{}:{}:{}",
                    evidence.reference.artifact_id,
                    evidence.reference.version,
                    evidence.reference.ordinal
                )
            })
            .collect::<BTreeSet<_>>();
        let result_matches: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM package_validation_item_results AS result
                   JOIN package_validation_runs AS run USING (run_id)
                   WHERE result.result_id = ?1 AND result.item_id = ?2
                     AND result.content_sha256 = ?3
                     AND result.outcome = 'manual_verification_required'
                     AND run.package_id = ?4 AND run.package_version = ?5
                     AND run.package_manifest_sha256 = ?6
                 )",
                params![
                    validation_result_id,
                    item_id,
                    content_sha256,
                    package_id,
                    package_version,
                    package_manifest_sha256
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if verification.verification_id != verification_id
            || verification.validation_result_id != validation_result_id
            || verification.package_id != package_id
            || verification.package_version != package_version
            || verification.package_manifest_sha256 != package_manifest_sha256
            || verification.item_id != item_id
            || verification.content_sha256 != content_sha256
            || verification.capability != capability
            || verification.result.as_str() != result
            || verification.manifest_sha256 != manifest_sha256
            || verification.created_at != created_at
            || verification.verifier_identity != "engineer_user"
            || policy_rule.is_none_or(|rule| verification.checks != rule.manual_checklist)
            || !section.required_capabilities.contains(&capability)
            || verification
                .evidence_references
                .iter()
                .any(|reference| !allowed_evidence.contains(reference))
            || !result_matches
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn section_reviews_are_valid(
    connection: &Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT review_id, assignment_id, package_id, package_version,
                    package_manifest_sha256, reviewer_run_id, reviewer_profile_id,
                    reviewer_profile_version, result, review_json, manifest_sha256, created_at
             FROM submission_section_reviews ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, u32>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let (
            review_id,
            assignment_id,
            package_id,
            package_version,
            package_manifest_sha256,
            reviewer_run_id,
            reviewer_profile_id,
            reviewer_profile_version,
            result,
            json,
            manifest_sha256,
            created_at,
        ) = row.map_err(sql_error)?;
        let Ok(review) = parse_canonical::<SubmissionSectionReview>(&json) else {
            return Ok(false);
        };
        let assignment_matches: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM final_review_assignments AS assignment
                   JOIN final_review_plans AS plan USING (plan_id)
                   WHERE assignment.assignment_id = ?1
                     AND assignment.reviewer_profile_id = ?2
                     AND assignment.reviewer_profile_version = ?3
                     AND plan.package_id = ?4 AND plan.package_version = ?5
                     AND plan.package_manifest_sha256 = ?6
                 )",
                params![
                    assignment_id,
                    reviewer_profile_id,
                    reviewer_profile_version,
                    package_id,
                    package_version,
                    package_manifest_sha256
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let run_matches: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE run_id = ?1
                 AND profile_id = ?2 AND profile_version = ?3 AND status = 'completed')",
                params![
                    reviewer_run_id,
                    reviewer_profile_id,
                    reviewer_profile_version
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let assignment_json: Option<String> = connection
            .query_row(
                "SELECT assignment.assignment_json
                 FROM final_review_assignments AS assignment
                 JOIN final_review_plans AS plan USING (plan_id)
                 WHERE assignment.assignment_id = ?1
                   AND plan.package_id = ?2 AND plan.package_version = ?3
                   AND plan.package_manifest_sha256 = ?4",
                params![
                    assignment_id,
                    package_id,
                    package_version,
                    package_manifest_sha256
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let runtime_evidence: Option<(String, String)> = connection
            .query_row(
                "SELECT task.exact_inputs_json, proposed.payload_json
                 FROM agent_runs AS run
                 JOIN tender_tasks AS task ON task.task_id = run.task_id
                 JOIN proposed_agent_results AS proposed ON proposed.run_id = run.run_id
                 WHERE run.run_id = ?1 AND run.status = 'completed'
                   AND run.profile_id = ?2 AND run.profile_version = ?3
                   AND task.profile_id = run.profile_id
                   AND task.profile_version = run.profile_version",
                params![
                    reviewer_run_id,
                    reviewer_profile_id,
                    reviewer_profile_version
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let mut findings_statement = connection
            .prepare(
                "SELECT finding_id, ordinal, severity, policy_rule_id, finding_json
                 FROM submission_section_review_findings
                 WHERE review_id = ?1 ORDER BY ordinal",
            )
            .map_err(sql_error)?;
        let finding_rows = findings_statement
            .query_map([&review_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(sql_error)?;
        let mut findings = Vec::new();
        for finding_row in finding_rows {
            let (finding_id, ordinal, severity, policy_rule_id, finding_json) =
                finding_row.map_err(sql_error)?;
            let finding = parse_canonical::<PackageReviewFinding>(&finding_json)?;
            if finding.finding_id != finding_id
                || finding.review_id != review_id
                || finding.sequence != ordinal
                || finding_severity_str(finding.severity) != severity
                || finding.policy_rule_id != policy_rule_id
                || finding.created_at != created_at
            {
                return Ok(false);
            }
            findings.push(finding);
        }
        let runtime_matches = if let (Some(assignment_json), Some((inputs_json, payload_json))) =
            (assignment_json, runtime_evidence)
        {
            let assignment = parse_canonical::<FinalReviewAssignment>(&assignment_json)?;
            let exact_inputs = parse_canonical::<Vec<AgentTaskInputReference>>(&inputs_json)?;
            let candidate = parse_canonical::<SubmissionSectionReviewCandidate>(&payload_json)?;
            let package = load_submission_package_snapshot_for_integrity(
                connection,
                &package_id,
                package_version,
                &package_manifest_sha256,
            )?;
            let (policy, validation_run, plan) = load_policy_run_plan(connection, &package)?;
            let exact_input = |kind: &str, reference: &str, version: u32| {
                exact_inputs
                    .iter()
                    .filter(|input| input.kind == kind)
                    .count()
                    == 1
                    && exact_inputs.iter().any(|input| {
                        input.kind == kind
                            && input.reference == reference
                            && input.version == version
                    })
            };
            let allowed_evidence = review_allowed_evidence(&package, &validation_run, &assignment);
            let candidate_findings_match =
                candidate.findings.len() == findings.len()
                    && candidate
                        .findings
                        .iter()
                        .zip(&findings)
                        .all(|(candidate, stored)| {
                            candidate.severity == stored.severity
                                && candidate.policy_rule_id == stored.policy_rule_id
                                && candidate.summary.trim() == stored.summary
                                && normalized_nonempty(&candidate.evidence_references).is_ok_and(
                                    |references| {
                                        references == stored.evidence_references
                                            && references.iter().all(|reference| {
                                                allowed_evidence.contains(reference)
                                            })
                                    },
                                )
                                && policy.fixed_rules.iter().chain(&policy.tender_rules).any(
                                    |rule| {
                                        rule.rule_id == candidate.policy_rule_id
                                            && severity_is_policy_permitted(
                                                candidate.severity,
                                                rule.severity,
                                            )
                                    },
                                )
                        });
            assignment.assignment_id == assignment_id
                && plan.assignments.contains(&assignment)
                && assignment.reviewer.as_ref().is_some_and(|reviewer| {
                    reviewer.profile_id == reviewer_profile_id
                        && reviewer.profile_version == reviewer_profile_version
                        && reviewer == &review.reviewer
                })
                && review.required_capability == assignment.required_capability
                && review.criteria == assignment.criteria
                && candidate.assignment_id == assignment_id
                && candidate.package_id == package_id
                && candidate.package_version == package_version
                && candidate.package_manifest_sha256 == package_manifest_sha256
                && candidate.result.as_str() == result
                && (candidate.result != PackageReviewResult::RequiresRemediation
                    || !candidate.findings.is_empty())
                && (candidate.result != PackageReviewResult::Satisfied
                    || candidate
                        .findings
                        .iter()
                        .all(|finding| finding.severity == ProductionFindingSeverity::Minor))
                && candidate_findings_match
                && exact_input("submission_package", &package_id, package_version)
                && exact_input("submission_package_manifest", &package_manifest_sha256, 1)
                && exact_input("package_validation_run", &validation_run.run_id, 1)
                && exact_input("submission_review_plan", &plan.plan_id, 1)
                && exact_input("submission_review_assignment", &assignment_id, 1)
        } else {
            false
        };
        if review.review_id != review_id
            || review.assignment_id != assignment_id
            || review.package_id != package_id
            || review.package_version != package_version
            || review.package_manifest_sha256 != package_manifest_sha256
            || review.reviewer_run_id != reviewer_run_id
            || review.reviewer.profile_id != reviewer_profile_id
            || review.reviewer.profile_version != reviewer_profile_version
            || review.result.as_str() != result
            || review.findings != findings
            || review.manifest_sha256 != manifest_sha256
            || review.created_at != created_at
            || !assignment_matches
            || !run_matches
            || !runtime_matches
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn finding_exceptions_are_valid(
    connection: &Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT approval_id, finding_id, package_id, package_version,
                    package_manifest_sha256, approval_json, manifest_sha256, created_at
             FROM package_finding_exception_approvals ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let (
            approval_id,
            finding_id,
            package_id,
            package_version,
            package_manifest_sha256,
            json,
            manifest_sha256,
            created_at,
        ) = row.map_err(sql_error)?;
        let Ok(approval) = parse_canonical::<PackageFindingExceptionApproval>(&json) else {
            return Ok(false);
        };
        let finding_rule: Option<(String, String)> = connection
            .query_row(
                "SELECT finding.policy_rule_id, policy.manifest_json
                   FROM submission_section_review_findings AS finding
                   JOIN submission_section_reviews AS review USING (review_id)
                   JOIN final_review_assignments AS assignment
                     ON assignment.assignment_id = review.assignment_id
                   JOIN final_review_plans AS plan ON plan.plan_id = assignment.plan_id
                   JOIN package_validation_runs AS run ON run.run_id = plan.validation_run_id
                   JOIN package_validation_policies AS policy
                     ON policy.policy_id = run.policy_id AND policy.version = run.policy_version
                   WHERE finding.finding_id = ?1 AND finding.severity = 'major'
                     AND review.package_id = ?2 AND review.package_version = ?3
                     AND review.package_manifest_sha256 = ?4",
                params![
                    finding_id,
                    package_id,
                    package_version,
                    package_manifest_sha256
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let finding_rule_allows_exception = finding_rule
            .map(|(rule_id, policy_json)| {
                parse_canonical::<PackageValidationPolicy>(&policy_json).map(|policy| {
                    policy
                        .fixed_rules
                        .iter()
                        .chain(&policy.tender_rules)
                        .any(|rule| rule.rule_id == rule_id && rule.major_exception_allowed)
                })
            })
            .transpose()?
            .unwrap_or(false);
        if approval.approval_id != approval_id
            || approval.finding_id != finding_id
            || approval.package_id != package_id
            || approval.package_version != package_version
            || approval.package_manifest_sha256 != package_manifest_sha256
            || approval.manifest_sha256 != manifest_sha256
            || approval.created_at != created_at
            || approval.decided_by != "engineer_user"
            || approval.acting_role != "tendering_manager"
            || approval.rationale.trim().is_empty()
            || !finding_rule_allows_exception
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn release_reports_are_valid(
    connection: &Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT report_id, version, package_id, package_version,
                    package_manifest_sha256, through_event_sequence, report_json,
                    manifest_sha256, audit_sequence, created_at
             FROM release_readiness_reports ORDER BY report_id, version",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u32>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let (
            report_id,
            version,
            package_id,
            package_version,
            package_manifest_sha256,
            through_event_sequence,
            json,
            manifest_sha256,
            audit_sequence,
            created_at,
        ) = row.map_err(sql_error)?;
        let Ok(report) = parse_canonical::<ReleaseReadinessReport>(&json) else {
            return Ok(false);
        };
        let lineage_matches: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM package_validation_runs AS run
                   JOIN final_review_plans AS plan ON plan.validation_run_id = run.run_id
                   WHERE run.run_id = ?1 AND run.manifest_sha256 = ?2
                     AND run.policy_manifest_sha256 = ?3
                     AND run.package_id = ?4 AND run.package_version = ?5
                     AND run.package_manifest_sha256 = ?6
                     AND plan.plan_id = ?7 AND plan.manifest_sha256 = ?8
                 )",
                params![
                    report.validation_run_id,
                    report.validation_manifest_sha256,
                    report.policy_manifest_sha256,
                    package_id,
                    package_version,
                    package_manifest_sha256,
                    report.review_plan_id,
                    report.review_plan_manifest_sha256
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let report_audit_matches: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM audit_events
                   WHERE sequence = ?1
                     AND event_type = 'release_readiness_report_published'
                     AND aggregate_revision = (
                       SELECT tender_revision FROM submission_package_versions
                       WHERE package_id = ?2 AND version = ?3
                     )
                     AND created_at = ?4
                     AND json_extract(payload_json, '$.change.report_id') = ?5
                     AND json_extract(payload_json, '$.change.version') = ?6
                     AND json_extract(payload_json, '$.change.package_id') = ?2
                     AND json_extract(payload_json, '$.change.package_version') = ?7
                     AND json_extract(payload_json, '$.change.package_manifest_sha256') = ?8
                     AND json_extract(payload_json, '$.change.through_event_sequence') = ?9
                     AND json_extract(payload_json, '$.change.manifest_sha256') = ?10
                 )",
                params![
                    audit_sequence,
                    package_id,
                    package_version,
                    created_at,
                    report_id,
                    version.to_string(),
                    package_version.to_string(),
                    package_manifest_sha256,
                    through_event_sequence.to_string(),
                    manifest_sha256,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if report.report_id != report_id
            || report.version != version
            || report.package_id != package_id
            || report.package_version != package_version
            || report.package_manifest_sha256 != package_manifest_sha256
            || report.through_event_sequence
                != u64::try_from(through_event_sequence)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            || report.manifest_sha256 != manifest_sha256
            || report.created_at != created_at
            || audit_sequence <= through_event_sequence
            || report.ready != (report.current && report.blockers.is_empty())
            || !lineage_matches
            || !report_audit_matches
            || !report_reference_ids_are_valid(connection, &report)?
            || !report_semantics_are_valid(connection, &report)?
        {
            return Ok(false);
        }
    }
    let head_matches_latest: bool = connection
        .query_row(
            "SELECT (SELECT COUNT(DISTINCT report_id) FROM release_readiness_reports) <= 1
             AND NOT EXISTS(
               SELECT 1 FROM release_readiness_report_head AS head
               WHERE head.current_version <> (
                 SELECT MAX(report.version) FROM release_readiness_reports AS report
                 WHERE report.report_id = head.report_id
               )
             )
             AND NOT EXISTS(
               SELECT 1 FROM release_readiness_reports AS report
               WHERE report.version <> (
                 SELECT COUNT(*) FROM release_readiness_reports AS prior
                 WHERE prior.report_id = report.report_id AND prior.version <= report.version
               )
             )",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    Ok(head_matches_latest)
}

fn report_reference_ids_are_valid(
    connection: &Connection,
    report: &ReleaseReadinessReport,
) -> Result<bool, TenderCommandError> {
    let review_ids = query_report_ids(
        connection,
        "SELECT review.review_id FROM submission_section_reviews AS review
         JOIN final_review_assignments AS assignment
           ON assignment.assignment_id = review.assignment_id
         JOIN audit_events AS event
           ON event.event_type = 'submission_section_review_published'
          AND json_extract(event.payload_json, '$.change.reference_id') = review.review_id
         WHERE review.package_id = ?1 AND review.package_version = ?2
           AND review.package_manifest_sha256 = ?3 AND event.sequence <= ?4
         ORDER BY assignment.ordinal",
        report,
    )?;
    let manual_ids = query_report_ids(
        connection,
        "SELECT verification.verification_id FROM package_manual_verifications AS verification
         JOIN package_validation_item_results AS result
           ON result.result_id = verification.validation_result_id
         JOIN audit_events AS event
           ON event.event_type = 'package_manual_verification_recorded'
          AND json_extract(event.payload_json, '$.change.reference_id') = verification.verification_id
         WHERE verification.package_id = ?1 AND verification.package_version = ?2
           AND verification.package_manifest_sha256 = ?3 AND event.sequence <= ?4
         ORDER BY result.ordinal",
        report,
    )?;
    let exception_ids = query_report_ids(
        connection,
        "SELECT approval.approval_id FROM package_finding_exception_approvals AS approval
         JOIN audit_events AS event
           ON event.event_type = 'package_finding_exception_approved'
          AND json_extract(event.payload_json, '$.change.reference_id') = approval.approval_id
         WHERE approval.package_id = ?1 AND approval.package_version = ?2
           AND approval.package_manifest_sha256 = ?3 AND event.sequence <= ?4
         ORDER BY event.sequence",
        report,
    )?;
    if report.review_ids.iter().collect::<BTreeSet<_>>()
        != review_ids.iter().collect::<BTreeSet<_>>()
        || report
            .manual_verification_ids
            .iter()
            .collect::<BTreeSet<_>>()
            != manual_ids.iter().collect::<BTreeSet<_>>()
        || report
            .exception_approval_ids
            .iter()
            .collect::<BTreeSet<_>>()
            != exception_ids.iter().collect::<BTreeSet<_>>()
        || report.review_ids.len() != review_ids.len()
        || report.manual_verification_ids.len() != manual_ids.len()
        || report.exception_approval_ids.len() != exception_ids.len()
    {
        return Ok(false);
    }
    let mut findings = Vec::new();
    for review_id in &report.review_ids {
        let mut statement = connection
            .prepare(
                "SELECT finding_id FROM submission_section_review_findings
                 WHERE review_id = ?1 ORDER BY ordinal",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([review_id], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        findings.extend(rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)?);
    }
    let summary = |category: &str| {
        report
            .summaries
            .iter()
            .find(|summary| summary.category == category)
            .map(|summary| summary.references.as_slice())
    };
    Ok(summary("findings") == Some(findings.as_slice())
        && summary("exceptions") == Some(report.exception_approval_ids.as_slice())
        && summary("manual_verifications") == Some(report.manual_verification_ids.as_slice()))
}

fn report_semantics_are_valid(
    connection: &Connection,
    report: &ReleaseReadinessReport,
) -> Result<bool, TenderCommandError> {
    let mut package = load_submission_package_snapshot_for_integrity(
        connection,
        &report.package_id,
        report.package_version,
        &report.package_manifest_sha256,
    )?;
    let historical_changes = package
        .currentness_facts
        .iter()
        .cloned()
        .map(|mut fact| {
            fact.current = true;
            fact.actual_value = Some(fact.expected_value.clone());
            fact
        })
        .collect::<Vec<_>>();
    package.current = true;
    package.currentness_facts = historical_changes.clone();
    let (policy, run, plan) = load_policy_run_plan(connection, &package)?;
    let manual = load_manual_verifications(connection, &run.run_id)?
        .into_iter()
        .filter(|verification| {
            report
                .manual_verification_ids
                .contains(&verification.verification_id)
        })
        .collect::<Vec<_>>();
    let reviews = load_reviews(connection, &plan)?
        .into_iter()
        .filter(|review| report.review_ids.contains(&review.review_id))
        .collect::<Vec<_>>();
    let exceptions = load_exceptions(connection, &package)?
        .into_iter()
        .filter(|approval| {
            report
                .exception_approval_ids
                .contains(&approval.approval_id)
        })
        .collect::<Vec<_>>();
    if report.manual_verification_ids
        != manual
            .iter()
            .map(|verification| verification.verification_id.clone())
            .collect::<Vec<_>>()
        || report.review_ids
            != reviews
                .iter()
                .map(|review| review.review_id.clone())
                .collect::<Vec<_>>()
        || report.exception_approval_ids
            != exceptions
                .iter()
                .map(|approval| approval.approval_id.clone())
                .collect::<Vec<_>>()
    {
        return Ok(false);
    }
    let baseline_bindings: Vec<CoordinatedBidBaselineBinding> = connection
        .query_row(
            "SELECT bindings_json FROM coordinated_bid_baseline_versions
             WHERE baseline_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
            params![
                package.baseline_id,
                package.baseline_version,
                package.baseline_manifest_sha256
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(sql_error)
        .and_then(|json| parse_canonical(&json))?;
    let summaries = readiness_summaries(
        &package,
        &run,
        &manual,
        &plan,
        &reviews,
        &exceptions,
        &baseline_bindings,
    );
    let blockers = readiness_blockers(
        connection,
        &package,
        &run,
        ReviewRecords {
            manual: &manual,
            plan: &plan,
            reviews: &reviews,
            exceptions: &exceptions,
        },
        false,
    )?;
    Ok(report.policy_manifest_sha256 == policy.manifest_sha256
        && report.validation_run_id == run.run_id
        && report.validation_manifest_sha256 == run.manifest_sha256
        && report.review_plan_id == plan.plan_id
        && report.review_plan_manifest_sha256 == plan.manifest_sha256
        && report.summaries == summaries
        && report.deadline == package.submission_deadline
        && report.changes == historical_changes
        && report.blockers == blockers
        && report.current
        && report.ready == blockers.is_empty())
}

fn query_report_ids(
    connection: &Connection,
    sql: &str,
    report: &ReleaseReadinessReport,
) -> Result<Vec<String>, TenderCommandError> {
    let mut statement = connection.prepare(sql).map_err(sql_error)?;
    let rows = statement
        .query_map(
            params![
                report.package_id,
                report.package_version,
                report.package_manifest_sha256,
                i64::try_from(report.through_event_sequence)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(sql_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sql_error)
}

fn build_and_persist_policy(
    transaction: &Transaction<'_>,
    package: &SubmissionPackageVersion,
    created_at: &str,
) -> Result<PackageValidationPolicy, TenderCommandError> {
    let fixed_rules = fixed_policy_rules();
    let tender_rules = tender_policy_rules(&package.coverage);
    let prior: Option<PackageValidationPolicy> = transaction
        .query_row(
            "SELECT manifest_json FROM package_validation_policies ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
        .map(|json| parse_canonical(&json))
        .transpose()?;
    if let Some(prior) = prior.as_ref() {
        if prior.fixed_rules == fixed_rules && prior.tender_rules == tender_rules {
            return Ok(prior.clone());
        }
    }
    let policy_id = prior
        .as_ref()
        .map(|policy| policy.policy_id.clone())
        .unwrap_or(random_identifier(transaction)?);
    let version = prior.as_ref().map_or(Ok(1), |policy| {
        policy
            .version
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
    })?;
    let mut policy = PackageValidationPolicy {
        policy_id,
        version,
        fixed_rules,
        tender_rules,
        manifest_sha256: String::new(),
        created_at: created_at.into(),
    };
    policy.manifest_sha256 = manifest_sha256(&policy)?;
    transaction
        .execute(
            "INSERT INTO package_validation_policies (
           policy_id, version, manifest_json, manifest_sha256, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                policy.policy_id,
                policy.version,
                canonical_json(&policy)?,
                policy.manifest_sha256,
                policy.created_at
            ],
        )
        .map_err(sql_error)?;
    Ok(policy)
}

fn tender_policy_rules(coverage: &[SubmissionCoverageRow]) -> Vec<PackageValidationRule> {
    coverage
        .iter()
        .map(|coverage| PackageValidationRule {
            rule_id: format!("tender.requirement.{}", coverage.requirement.requirement_id),
            category: if coverage.manual_validation_required {
                PackageValidationCheckCategory::Rendering
            } else {
                PackageValidationCheckCategory::CrossArtifactConsistency
            },
            severity: ProductionFindingSeverity::Critical,
            deterministic: !coverage.manual_validation_required,
            source: Some(super::SubmissionValidationContextInput {
                input_kind: "verified_tender_requirement".into(),
                reference_id: coverage.requirement.record.record_id.clone(),
                version: coverage.requirement.record.version,
                sha256: coverage.requirement.record.manifest_sha256.clone(),
            }),
            manual_checklist: if coverage.manual_validation_required {
                vec!["Verify the exact registered file hash against the Tender-prescribed visual, signature, and form checklist.".into()]
            } else {
                Vec::new()
            },
            major_exception_allowed: false,
        })
        .collect()
}

fn fixed_policy_rules() -> Vec<PackageValidationRule> {
    [
        (
            "quantix.file_structure",
            PackageValidationCheckCategory::FileStructure,
            ProductionFindingSeverity::Critical,
        ),
        (
            "quantix.rendered_text_content",
            PackageValidationCheckCategory::Rendering,
            ProductionFindingSeverity::Critical,
        ),
        (
            "quantix.rendering_model",
            PackageValidationCheckCategory::Rendering,
            ProductionFindingSeverity::Major,
        ),
        (
            "quantix.calculation_binding",
            PackageValidationCheckCategory::Calculation,
            ProductionFindingSeverity::Critical,
        ),
        (
            "quantix.cross_artifact_consistency",
            PackageValidationCheckCategory::CrossArtifactConsistency,
            ProductionFindingSeverity::Critical,
        ),
        (
            "quantix.hidden_content",
            PackageValidationCheckCategory::HiddenContent,
            ProductionFindingSeverity::Critical,
        ),
        (
            "quantix.information_boundary",
            PackageValidationCheckCategory::InformationBoundary,
            ProductionFindingSeverity::Critical,
        ),
        (
            "quantix.filename",
            PackageValidationCheckCategory::Filename,
            ProductionFindingSeverity::Major,
        ),
        (
            "quantix.content_hash",
            PackageValidationCheckCategory::Hash,
            ProductionFindingSeverity::Critical,
        ),
        (
            "quantix.package_conditions",
            PackageValidationCheckCategory::PackageWide,
            ProductionFindingSeverity::Critical,
        ),
    ]
    .into_iter()
    .map(|(rule_id, category, severity)| PackageValidationRule {
        rule_id: rule_id.into(),
        category,
        severity,
        deterministic: rule_id != "quantix.rendering_model",
        source: None,
        manual_checklist: match rule_id {
            "quantix.rendered_text_content" => vec!["Verify the exact supplied-file hash presents all required Tender text without omission or substitution.".into()],
            "quantix.rendering_model" => vec!["Verify the exact registered file hash against the Quantix visual, signature, and form checklist.".into()],
            "quantix.hidden_content" => vec!["Verify the exact PDF hash has no hidden layers, comments, attachments, scripts, actions, or external references.".into()],
            "quantix.information_boundary" => vec!["Verify short or structurally encoded values in the exact file hash do not cross Submission Section information envelopes.".into()],
            _ => Vec::new(),
        },
        major_exception_allowed: rule_id == "quantix.rendering_model",
    })
    .collect()
}

fn execute_and_persist_validation(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    content_root: &std::path::Path,
    package: &ExactSubmissionPackage,
    policy: &PackageValidationPolicy,
    created_at: &str,
    budget: BidPackageOperationBudget,
) -> Result<PackageValidationRun, TenderCommandError> {
    let run_id = random_identifier(transaction)?;
    let mut results = Vec::new();
    for exact_item in &package.items {
        budget.check()?;
        let bytes =
            load_exact_submission_package_item_bytes_for_transaction(content_root, exact_item)?;
        let mut item_results = validate_item(
            transaction,
            &run_id,
            &package.package,
            exact_item,
            &bytes,
            policy,
        )?;
        results.append(&mut item_results);
    }
    results.push(package_wide_result(transaction, &run_id, &package.package)?);
    if results.len() > MAX_RESULTS {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let context_sha256 = sha256_hex(
        canonical_json(&json!({
            "package_validation_context": package.package.validation_context_sha256,
            "policy": policy.manifest_sha256,
            "validator_version": VALIDATOR_VERSION,
            "renderer_version": RENDERER_VERSION,
        }))?
        .as_bytes(),
    );
    let mut run = PackageValidationRun {
        run_id,
        package_id: package.package.package_id.clone(),
        package_version: package.package.version,
        package_manifest_sha256: package.package.manifest_sha256.clone(),
        policy_id: policy.policy_id.clone(),
        policy_version: policy.version,
        policy_manifest_sha256: policy.manifest_sha256.clone(),
        validator_version: VALIDATOR_VERSION,
        renderer_version: RENDERER_VERSION,
        context_sha256,
        results,
        manifest_sha256: String::new(),
        created_at: created_at.into(),
    };
    run.manifest_sha256 = manifest_sha256(&run)?;
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        "package_validation_run_published",
        package.package.tender_revision,
        json!({
            "run_id": run.run_id,
            "package_id": run.package_id,
            "package_version": run.package_version.to_string(),
            "package_manifest_sha256": run.package_manifest_sha256,
            "policy_manifest_sha256": run.policy_manifest_sha256,
            "manifest_sha256": run.manifest_sha256,
        }),
        created_at,
    )?;
    transaction
        .execute(
            "INSERT INTO package_validation_runs (
           run_id, package_id, package_version, package_manifest_sha256,
           policy_id, policy_version, policy_manifest_sha256, validator_version,
           renderer_version, context_sha256, manifest_json, manifest_sha256,
           audit_sequence, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                run.run_id,
                run.package_id,
                run.package_version,
                run.package_manifest_sha256,
                run.policy_id,
                run.policy_version,
                run.policy_manifest_sha256,
                run.validator_version,
                run.renderer_version,
                run.context_sha256,
                canonical_json(&run)?,
                run.manifest_sha256,
                audit_sequence,
                run.created_at
            ],
        )
        .map_err(sql_error)?;
    let mut item_ordinal = 0_u32;
    let mut package_ordinal = 0_u32;
    for result in &run.results {
        if let (Some(item_id), Some(content_sha256)) = (&result.item_id, &result.content_sha256) {
            item_ordinal += 1;
            transaction
                .execute(
                    "INSERT INTO package_validation_item_results (
                   result_id, run_id, ordinal, item_id, content_sha256,
                   validation_context_sha256, check_id, check_version, category,
                   outcome, policy_manifest_sha256, reused_from_result_id, result_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        result.result_id,
                        run.run_id,
                        item_ordinal,
                        item_id,
                        content_sha256,
                        result.validation_context_sha256,
                        result.check_id,
                        result.check_version,
                        result.category.as_str(),
                        result.outcome.as_str(),
                        policy.manifest_sha256,
                        result.reused_from_result_id,
                        canonical_json(result)?
                    ],
                )
                .map_err(sql_error)?;
        } else {
            package_ordinal += 1;
            transaction
                .execute(
                    "INSERT INTO package_validation_package_results (
                   result_id, run_id, ordinal, check_id, check_version, category,
                   outcome, result_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        result.result_id,
                        run.run_id,
                        package_ordinal,
                        result.check_id,
                        result.check_version,
                        result.category.as_str(),
                        result.outcome.as_str(),
                        canonical_json(result)?
                    ],
                )
                .map_err(sql_error)?;
        }
    }
    Ok(run)
}

fn validate_item(
    connection: &Connection,
    _run_id: &str,
    package: &SubmissionPackageVersion,
    exact_item: &ExactSubmissionPackageItem,
    bytes: &[u8],
    policy: &PackageValidationPolicy,
) -> Result<Vec<PackageValidationResult>, TenderCommandError> {
    let item = &exact_item.item;
    let document_text = package_document_text(&item.media_type, bytes).unwrap_or_default();
    let requirements = package
        .coverage
        .iter()
        .filter(|coverage| {
            item.requirement_ids
                .contains(&coverage.requirement.requirement_id)
        })
        .collect::<Vec<_>>();
    let structure_passed = supported_structure(&item.media_type, bytes);
    let generated = matches!(item.source, super::SubmissionItemSource::Generated { .. });
    let required_rendered_values = requirements
        .iter()
        .flat_map(|coverage| {
            std::iter::once(coverage.requirement.record.title.as_str()).chain(
                coverage
                    .requirement
                    .authored_fields
                    .iter()
                    .filter_map(|field| {
                        field.value.as_deref().or(field.normalized_value.as_deref())
                    }),
            )
        })
        .collect::<Vec<_>>();
    let rendering_passed = rendered_values_are_present(&document_text, &required_rendered_values);
    let calculation_passed = !item.package_path.ends_with(".xlsx")
        || calculation_bindings_are_present(connection, bytes, &item.calculation_references)?;
    let hidden_passed = !contains_hidden_content(&document_text);
    let other_envelope_values = package
        .coverage
        .iter()
        .filter(|coverage| coverage.requirement.envelope_key != item.envelope_key)
        .flat_map(|coverage| {
            coverage
                .requirement
                .authored_fields
                .iter()
                .filter_map(|field| field.value.as_deref().or(field.normalized_value.as_deref()))
        })
        .collect::<Vec<_>>();
    let leaks_other_envelope =
        contains_cross_envelope_value(&document_text, &other_envelope_values);
    let short_other_envelope_values_require_manual = other_envelope_values
        .iter()
        .any(|value| !value.is_empty() && value.len() < 8);
    let boundary_passed = !document_text.contains("QUANTIX_BOUNDARY_LEAK:")
        && !leaks_other_envelope
        && package.sections.iter().any(|section| {
            section.section_key == item.section_key
                && section.envelope_key == item.envelope_key
                && section.item_ids.contains(&item.item_id)
        });
    let filename_passed = valid_package_filename(&item.package_path);
    let hash_passed =
        sha256_hex(bytes) == item.content_sha256 && bytes.len() as u64 == item.size_bytes;
    let cross_passed = !requirements.is_empty()
        && requirements.iter().all(|coverage| {
            let requirement = &coverage.requirement;
            requirement.package_path == item.package_path
                && requirement.section_key == item.section_key
                && requirement.envelope_key == item.envelope_key
        });
    let checks = [
        ("quantix.file_structure", PackageValidationCheckCategory::FileStructure,
            bool_outcome(structure_passed), "The exact file has the required registered container and primary document parts."),
        ("quantix.rendered_text_content", PackageValidationCheckCategory::Rendering,
            if generated { bool_outcome(rendering_passed) } else { PackageValidationOutcome::ManualVerificationRequired },
            if generated { "The deterministic document model contains every exact authored Tender value." } else { "The unchanged supplied file requires exact-hash rendered-text verification." }),
        ("quantix.rendering_model", PackageValidationCheckCategory::Rendering,
            PackageValidationOutcome::ManualVerificationRequired,
            "Visual layout, clipping, overlap, pagination, signatures, and form appearance require exact-hash Manual Verification."),
        ("quantix.calculation_binding", PackageValidationCheckCategory::Calculation,
            bool_outcome(calculation_passed), "Rendered calculation identifiers and values bind to the approved Calculation Manifest."),
        ("quantix.cross_artifact_consistency", PackageValidationCheckCategory::CrossArtifactConsistency,
            bool_outcome(cross_passed), "The item purpose, section, envelope, and requirement membership are mutually consistent."),
        ("quantix.hidden_content", PackageValidationCheckCategory::HiddenContent,
            if item.media_type == "application/pdf" { PackageValidationOutcome::ManualVerificationRequired } else { bool_outcome(hidden_passed) },
            if item.media_type == "application/pdf" { "PDF hidden layers, comments, attachments, scripts, actions, and external references require exact-hash Manual Verification." } else { "No hidden OOXML content, hidden sheets, tracked changes, macro payload, or external relationship was detected." }),
        ("quantix.information_boundary", PackageValidationCheckCategory::InformationBoundary,
            if !boundary_passed { PackageValidationOutcome::Failed } else if short_other_envelope_values_require_manual { PackageValidationOutcome::ManualVerificationRequired } else { PackageValidationOutcome::Passed },
            if !boundary_passed { "A deterministic cross-envelope value or invalid section membership was detected." } else if short_other_envelope_values_require_manual { "Short or structurally encoded cross-envelope values require exact-hash Manual Verification." } else { "The item remains inside its exact Submission Section and information envelope." }),
        ("quantix.filename", PackageValidationCheckCategory::Filename,
            bool_outcome(filename_passed), "The exact package filename is portable, normalized, and traversal-safe."),
        ("quantix.content_hash", PackageValidationCheckCategory::Hash,
            bool_outcome(hash_passed), "The loaded bytes match the immutable package item digest and size."),
    ];
    let mut results = checks
        .into_iter()
        .map(|(check_id, category, outcome, detail)| {
            let reused_from_result_id = if outcome == PackageValidationOutcome::Passed {
                find_reusable_item_result(
                    connection,
                    &item.content_sha256,
                    check_id,
                    CHECK_VERSION,
                    &policy.manifest_sha256,
                    &item.validation_context_sha256,
                )?
            } else {
                None
            };
            Ok(PackageValidationResult {
                result_id: random_identifier(connection)?,
                item_id: Some(item.item_id.clone()),
                content_sha256: Some(item.content_sha256.clone()),
                validation_context_sha256: item.validation_context_sha256.clone(),
                check_id: check_id.into(),
                check_version: CHECK_VERSION,
                category,
                outcome,
                detail: detail.into(),
                evidence_references: vec![format!(
                    "submission_package_item:{}:{}",
                    item.item_id, item.content_sha256
                )],
                reused_from_result_id,
            })
        })
        .collect::<Result<Vec<_>, TenderCommandError>>()?;
    for coverage in requirements {
        let rule_id = format!("tender.requirement.{}", coverage.requirement.requirement_id);
        let rule = policy
            .tender_rules
            .iter()
            .find(|rule| rule.rule_id == rule_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let outcome = if rule.deterministic {
            bool_outcome(rendering_passed && cross_passed)
        } else {
            PackageValidationOutcome::ManualVerificationRequired
        };
        let reused_from_result_id = if outcome == PackageValidationOutcome::Passed {
            find_reusable_item_result(
                connection,
                &item.content_sha256,
                &rule.rule_id,
                CHECK_VERSION,
                &policy.manifest_sha256,
                &item.validation_context_sha256,
            )?
        } else {
            None
        };
        results.push(PackageValidationResult {
            result_id: random_identifier(connection)?,
            item_id: Some(item.item_id.clone()),
            content_sha256: Some(item.content_sha256.clone()),
            validation_context_sha256: item.validation_context_sha256.clone(),
            check_id: rule.rule_id.clone(),
            check_version: CHECK_VERSION,
            category: rule.category,
            outcome,
            detail: if rule.deterministic {
                "The exact item satisfies this Verified Tender-specific authored-value and package-binding rule."
            } else {
                "This Verified Tender-specific property requires its policy-defined exact-hash Manual Verification."
            }
            .into(),
            evidence_references: vec![format!(
                "tender_record:{}:{}",
                coverage.requirement.record.record_id, coverage.requirement.record.version
            )],
            reused_from_result_id,
        });
    }
    Ok(results)
}

fn rendered_values_are_present(document_text: &str, exact_values: &[&str]) -> bool {
    !exact_values.is_empty()
        && exact_values
            .iter()
            .all(|value| !value.is_empty() && document_text.contains(value))
}

fn calculation_bindings_are_present(
    connection: &Connection,
    bytes: &[u8],
    references: &[String],
) -> Result<bool, TenderCommandError> {
    if references.is_empty() {
        return Ok(false);
    }
    for reference in references {
        let parts = reference.split(':').collect::<Vec<_>>();
        if parts.len() != 3 || parts[0].is_empty() || parts[2].len() != 64 {
            return Ok(false);
        }
        let expected: Option<(String, String)> = connection
            .query_row(
                "SELECT final_amount, currency FROM approved_tender_prices
                 WHERE pricing_calculation_run_id = ?1
                   AND calculation_manifest_sha256 = ?2",
                params![parts[0], parts[2]],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((amount, currency)) = expected else {
            return Ok(false);
        };
        if !approved_price_row_is_exact(bytes, &amount, &currency, parts[0], parts[2])? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn approved_price_row_is_exact(
    bytes: &[u8],
    expected_amount: &str,
    expected_currency: &str,
    calculation_run_id: &str,
    calculation_manifest_sha256: &str,
) -> Result<bool, TenderCommandError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let shared_strings = match archive.by_name("xl/sharedStrings.xml") {
        Ok(mut part) => {
            let mut xml = String::new();
            part.read_to_string(&mut xml)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            parse_shared_strings(&xml)
        }
        Err(_) => Vec::new(),
    };
    let mut worksheet = archive
        .by_name("xl/worksheets/sheet1.xml")
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut xml = String::new();
    worksheet
        .read_to_string(&mut xml)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let Some(row) = xml_element_with_attribute(&xml, "row", "r", "3") else {
        return Ok(false);
    };
    let cells = parse_worksheet_cells(row, &shared_strings);
    let amount_matches = cells
        .get("B3")
        .and_then(|value| Decimal::from_str_exact(value).ok())
        .zip(Decimal::from_str_exact(expected_amount).ok())
        .is_some_and(|(actual, expected)| actual.normalize() == expected.normalize());
    Ok(cells
        .get("A3")
        .is_some_and(|value| value == "Approved Tender Price")
        && amount_matches
        && cells
            .get("C3")
            .is_some_and(|value| value == expected_currency)
        && cells
            .get("D3")
            .is_some_and(|value| value == calculation_run_id)
        && cells
            .get("E3")
            .is_some_and(|value| value == calculation_manifest_sha256)
        && cells
            .get("F3")
            .is_some_and(|value| value == expected_amount)
        && !row.contains("<f"))
}

fn xml_element_with_attribute<'a>(
    xml: &'a str,
    element: &str,
    attribute: &str,
    expected: &str,
) -> Option<&'a str> {
    let opening = format!("<{element}");
    let closing = format!("</{element}>");
    let mut cursor = 0;
    while let Some(offset) = xml[cursor..].find(&opening) {
        let start = cursor + offset;
        let open_end = start + xml[start..].find('>')? + 1;
        let open_tag = &xml[start..open_end];
        if xml_attribute(open_tag, attribute) == Some(expected) {
            let end = open_end + xml[open_end..].find(&closing)? + closing.len();
            return Some(&xml[start..end]);
        }
        cursor = open_end;
    }
    None
}

fn xml_attribute<'a>(opening_tag: &'a str, attribute: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let marker = format!("{attribute}={quote}");
        if let Some(start) = opening_tag.find(&marker) {
            let value_start = start + marker.len();
            let value_end = value_start + opening_tag[value_start..].find(quote)?;
            return Some(&opening_tag[value_start..value_end]);
        }
    }
    None
}

fn parse_shared_strings(xml: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = xml[cursor..].find("<si") {
        let start = cursor + offset;
        let Some(open_end) = xml[start..].find('>').map(|offset| start + offset + 1) else {
            break;
        };
        let Some(close_offset) = xml[open_end..].find("</si>") else {
            break;
        };
        let end = open_end + close_offset;
        let mut value = String::new();
        let mut text_cursor = open_end;
        while let Some(text_offset) = xml[text_cursor..end].find("<t") {
            let text_start = text_cursor + text_offset;
            let Some(tag_end) = xml[text_start..end]
                .find('>')
                .map(|offset| text_start + offset + 1)
            else {
                break;
            };
            let Some(text_end_offset) = xml[tag_end..end].find("</t>") else {
                break;
            };
            let text_end = tag_end + text_end_offset;
            value.push_str(&xml_unescape(&xml[tag_end..text_end]));
            text_cursor = text_end + 4;
        }
        values.push(value);
        cursor = end + 5;
    }
    values
}

fn parse_worksheet_cells(
    row: &str,
    shared_strings: &[String],
) -> std::collections::BTreeMap<String, String> {
    let mut cells = std::collections::BTreeMap::new();
    let mut cursor = 0;
    while let Some(offset) = row[cursor..].find("<c") {
        let start = cursor + offset;
        let Some(open_end) = row[start..].find('>').map(|offset| start + offset + 1) else {
            break;
        };
        let open_tag = &row[start..open_end];
        let Some(reference) = xml_attribute(open_tag, "r") else {
            cursor = open_end;
            continue;
        };
        let Some(close_offset) = row[open_end..].find("</c>") else {
            break;
        };
        let end = open_end + close_offset;
        let body = &row[open_end..end];
        let raw = xml_tag_value(body, "v").or_else(|| xml_tag_value(body, "t"));
        if let Some(raw) = raw {
            let value = if xml_attribute(open_tag, "t") == Some("s") {
                raw.parse::<usize>()
                    .ok()
                    .and_then(|index| shared_strings.get(index))
                    .cloned()
            } else {
                Some(xml_unescape(raw))
            };
            if let Some(value) = value {
                cells.insert(reference.to_owned(), value);
            }
        }
        cursor = end + 4;
    }
    cells
}

fn xml_tag_value<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let opening = format!("<{tag}");
    let start = xml.find(&opening)?;
    let value_start = start + xml[start..].find('>')? + 1;
    let closing = format!("</{tag}>");
    let value_end = value_start + xml[value_start..].find(&closing)?;
    Some(&xml[value_start..value_end])
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn contains_cross_envelope_value(document_text: &str, other_values: &[&str]) -> bool {
    other_values
        .iter()
        .any(|value| value.len() >= 8 && document_text.contains(value))
}

fn find_reusable_item_result(
    connection: &Connection,
    content_sha256: &str,
    check_id: &str,
    check_version: u32,
    policy_manifest_sha256: &str,
    validation_context_sha256: &str,
) -> Result<Option<String>, TenderCommandError> {
    connection
        .query_row(
            "SELECT result_id FROM package_validation_item_results
             WHERE content_sha256 = ?1 AND check_id = ?2 AND check_version = ?3
               AND policy_manifest_sha256 = ?4 AND validation_context_sha256 = ?5
               AND outcome = 'passed' ORDER BY rowid DESC LIMIT 1",
            params![
                content_sha256,
                check_id,
                check_version,
                policy_manifest_sha256,
                validation_context_sha256
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)
}

fn package_wide_result(
    connection: &Connection,
    run_id: &str,
    package: &SubmissionPackageVersion,
) -> Result<PackageValidationResult, TenderCommandError> {
    let complete = package.assessment == SubmissionPackageAssessment::Complete
        && package.current
        && package
            .coverage
            .iter()
            .all(|coverage| coverage.item_id.is_some() && coverage.blockers.is_empty())
        && !package.items.is_empty()
        && !package.sections.is_empty()
        && package.submission_deadline.is_some();
    Ok(PackageValidationResult {
        result_id: random_identifier(connection)?,
        item_id: None,
        content_sha256: None,
        validation_context_sha256: package.validation_context_sha256.clone(),
        check_id: "quantix.package_conditions".into(),
        check_version: CHECK_VERSION,
        category: PackageValidationCheckCategory::PackageWide,
        outcome: bool_outcome(complete),
        detail: "Package-wide coverage, section membership, deadline, and exact-current conditions were rerun for this package version.".into(),
        evidence_references: vec![format!("package_validation_run:{run_id}")],
        reused_from_result_id: None,
    })
}

fn build_and_persist_review_plan(
    transaction: &Transaction<'_>,
    package: &SubmissionPackageVersion,
    run: &PackageValidationRun,
    policy: &PackageValidationPolicy,
    created_at: &str,
) -> Result<FinalReviewPlan, TenderCommandError> {
    let profiles_json: String = transaction
        .query_row(
            "SELECT profiles_json FROM work_plan_versions
         WHERE plan_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
            params![
                package.work_plan.plan_id,
                package.work_plan.plan_version,
                package.work_plan.plan_manifest_sha256
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let profiles: Vec<WorkPlanProfileBinding> = parse_canonical(&profiles_json)?;
    let authorized = package
        .work_plan
        .authorized_profile_versions
        .iter()
        .map(|profile| (profile.profile_id.as_str(), profile.profile_version))
        .collect::<BTreeSet<_>>();
    let plan_id = random_identifier(transaction)?;
    let mut assignments = Vec::new();
    for section in &package.sections {
        let required = if section.required_capabilities.is_empty() {
            BTreeSet::from(["independent_review".to_owned()])
        } else {
            section
                .required_capabilities
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        for capability in required {
            let qualification = if capability == "independent_review" {
                capability.clone()
            } else {
                format!("review_{capability}")
            };
            let reviewer = profiles
                .iter()
                .filter(|binding| {
                    authorized
                        .contains(&(binding.profile.profile_id.as_str(), binding.profile.version))
                        && binding.profile.capabilities.contains(&qualification)
                        && !section
                            .independence_context
                            .author_profile_versions
                            .iter()
                            .any(|author| {
                                author.profile_id == binding.profile.profile_id
                                    && author.profile_version == binding.profile.version
                            })
                })
                .find_map(|binding| {
                    let active: bool = transaction
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM agent_profile_heads
                     WHERE profile_id = ?1 AND current_version = ?2 AND status = 'active')",
                            params![binding.profile.profile_id, binding.profile.version],
                            |row| row.get(0),
                        )
                        .ok()?;
                    active.then(|| FinalReviewReviewer {
                        profile_id: binding.profile.profile_id.clone(),
                        profile_version: binding.profile.version,
                        identity: binding.profile.identity.clone(),
                        capabilities: binding.profile.capabilities.clone(),
                    })
                });
            assignments.push(FinalReviewAssignment {
                assignment_id: random_identifier(transaction)?,
                section_key: section.section_key.clone(),
                envelope_key: section.envelope_key.clone(),
                language: section.language.clone(),
                item_ids: section.item_ids.clone(),
                required_capability: capability,
                risk_references: section.risk_context.risk_references.clone(),
                author_profile_versions: section.independence_context.author_profile_versions.clone(),
                reviewer,
                criteria: [
                    "Confirm exact requirement coverage and baseline fidelity without editing target bytes.".into(),
                    "Challenge validation, calculation, hidden-content, information-boundary, and Tender-specific results.".into(),
                    "Record every Critical, Major, and Minor finding; never silently hide a finding.".into(),
                ]
                .into_iter()
                .chain(section.risk_context.risk_references.iter().map(|risk| {
                    format!(
                        "Challenge exact section risk {}:{}:{}.",
                        risk.reference_id, risk.version, risk.manifest_sha256
                    )
                }))
                .collect(),
            });
        }
    }
    let mut plan = FinalReviewPlan {
        plan_id,
        package_id: package.package_id.clone(),
        package_version: package.version,
        package_manifest_sha256: package.manifest_sha256.clone(),
        validation_run_id: run.run_id.clone(),
        policy_manifest_sha256: policy.manifest_sha256.clone(),
        assignments,
        manifest_sha256: String::new(),
        created_at: created_at.into(),
    };
    plan.manifest_sha256 = manifest_sha256(&plan)?;
    transaction
        .execute(
            "INSERT INTO final_review_plans (
           plan_id, package_id, package_version, package_manifest_sha256,
           validation_run_id, policy_manifest_sha256, plan_json, manifest_sha256, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                plan.plan_id,
                plan.package_id,
                plan.package_version,
                plan.package_manifest_sha256,
                plan.validation_run_id,
                plan.policy_manifest_sha256,
                canonical_json(&plan)?,
                plan.manifest_sha256,
                plan.created_at
            ],
        )
        .map_err(sql_error)?;
    for (index, assignment) in plan.assignments.iter().enumerate() {
        transaction
            .execute(
                "INSERT INTO final_review_assignments (
               assignment_id, plan_id, ordinal, section_key, required_capability,
               reviewer_profile_id, reviewer_profile_version, assignment_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    assignment.assignment_id,
                    plan.plan_id,
                    u32::try_from(index + 1)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
                    assignment.section_key,
                    assignment.required_capability,
                    assignment
                        .reviewer
                        .as_ref()
                        .map(|reviewer| reviewer.profile_id.as_str()),
                    assignment
                        .reviewer
                        .as_ref()
                        .map(|reviewer| reviewer.profile_version),
                    canonical_json(assignment)?
                ],
            )
            .map_err(sql_error)?;
    }
    Ok(plan)
}

fn persist_release_readiness_report(
    transaction: &Transaction<'_>,
    package: &SubmissionPackageVersion,
    policy: &PackageValidationPolicy,
    run: &PackageValidationRun,
    plan: &FinalReviewPlan,
    through_event_sequence: i64,
    created_at: &str,
) -> Result<ReleaseReadinessReport, TenderCommandError> {
    let tender_id: String = transaction
        .query_row(
            "SELECT tender_id FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let manual = load_manual_verifications(transaction, &run.run_id)?;
    let reviews = load_reviews(transaction, plan)?;
    let exceptions = load_exceptions(transaction, package)?;
    let review_records = ReviewRecords {
        manual: &manual,
        plan,
        reviews: &reviews,
        exceptions: &exceptions,
    };
    let blockers = readiness_blockers(transaction, package, run, review_records, true)?;
    let (report_id, version) = transaction.query_row(
        "SELECT report_id, current_version + 1 FROM release_readiness_report_head WHERE singleton = 1",
        [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)),
    ).optional().map_err(sql_error)?.unwrap_or((random_identifier(transaction)?, 1));
    let baseline_bindings: Vec<CoordinatedBidBaselineBinding> = transaction
        .query_row(
            "SELECT bindings_json FROM coordinated_bid_baseline_versions
             WHERE baseline_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
            params![
                package.baseline_id,
                package.baseline_version,
                package.baseline_manifest_sha256
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(sql_error)
        .and_then(|json| parse_canonical(&json))?;
    let summaries = readiness_summaries(
        package,
        run,
        &manual,
        plan,
        &reviews,
        &exceptions,
        &baseline_bindings,
    );
    let mut report = ReleaseReadinessReport {
        report_id: report_id.clone(),
        version,
        package_id: package.package_id.clone(),
        package_version: package.version,
        package_manifest_sha256: package.manifest_sha256.clone(),
        policy_manifest_sha256: policy.manifest_sha256.clone(),
        validation_run_id: run.run_id.clone(),
        validation_manifest_sha256: run.manifest_sha256.clone(),
        review_plan_id: plan.plan_id.clone(),
        review_plan_manifest_sha256: plan.manifest_sha256.clone(),
        review_ids: reviews
            .iter()
            .map(|review| review.review_id.clone())
            .collect(),
        manual_verification_ids: manual
            .iter()
            .map(|verification| verification.verification_id.clone())
            .collect(),
        exception_approval_ids: exceptions
            .iter()
            .map(|exception| exception.approval_id.clone())
            .collect(),
        through_event_sequence: u64::try_from(through_event_sequence)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        summaries,
        deadline: package.submission_deadline.clone(),
        changes: package.currentness_facts.clone(),
        blockers,
        current: package.current,
        ready: false,
        manifest_sha256: String::new(),
        created_at: created_at.into(),
    };
    report.ready = report.current && report.blockers.is_empty();
    report.manifest_sha256 = manifest_sha256(&report)?;
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        &tender_id,
        "release_readiness_report_published",
        package.tender_revision,
        json!({
            "report_id": report.report_id,
            "version": report.version.to_string(),
            "package_id": report.package_id,
            "package_version": report.package_version.to_string(),
            "package_manifest_sha256": report.package_manifest_sha256,
            "through_event_sequence": report.through_event_sequence.to_string(),
            "manifest_sha256": report.manifest_sha256,
        }),
        created_at,
    )?;
    transaction
        .execute(
            "INSERT INTO release_readiness_reports (
           report_id, version, package_id, package_version, package_manifest_sha256,
           through_event_sequence, report_json, manifest_sha256, audit_sequence, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                report.report_id,
                report.version,
                report.package_id,
                report.package_version,
                report.package_manifest_sha256,
                i64::try_from(report.through_event_sequence)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                canonical_json(&report)?,
                report.manifest_sha256,
                audit_sequence,
                report.created_at
            ],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "INSERT INTO release_readiness_report_head (singleton, report_id, current_version)
         VALUES (1, ?1, ?2)
         ON CONFLICT(singleton) DO UPDATE SET current_version = excluded.current_version",
            params![report_id, version],
        )
        .map_err(sql_error)?;
    Ok(report)
}

#[derive(Clone, Copy)]
struct ReviewRecords<'a> {
    manual: &'a [PackageManualVerification],
    plan: &'a FinalReviewPlan,
    reviews: &'a [SubmissionSectionReview],
    exceptions: &'a [PackageFindingExceptionApproval],
}

fn readiness_blockers(
    connection: &Connection,
    package: &SubmissionPackageVersion,
    run: &PackageValidationRun,
    records: ReviewRecords<'_>,
    require_active_reviewer: bool,
) -> Result<Vec<ReleaseReadinessBlocker>, TenderCommandError> {
    let mut blockers = Vec::new();
    if package.assessment != SubmissionPackageAssessment::Complete {
        blockers.push(blocker(
            ReleaseReadinessBlockerCode::PackageIncomplete,
            &package.package_id,
            "Submission Package coverage is incomplete.",
        ));
    }
    for result in &run.results {
        match result.outcome {
            PackageValidationOutcome::Failed => blockers.push(blocker(
                ReleaseReadinessBlockerCode::ValidationFailed,
                &result.result_id,
                &result.detail,
            )),
            PackageValidationOutcome::ManualVerificationRequired => {
                match records
                    .manual
                    .iter()
                    .find(|verification| verification.validation_result_id == result.result_id)
                {
                    None => blockers.push(blocker(
                        ReleaseReadinessBlockerCode::ManualVerificationMissing,
                        &result.result_id,
                        "Exact-hash Manual Verification is missing.",
                    )),
                    Some(verification)
                        if verification.result == ManualVerificationResult::Failed =>
                    {
                        blockers.push(blocker(
                            ReleaseReadinessBlockerCode::ManualVerificationFailed,
                            &verification.verification_id,
                            "Exact-hash Manual Verification failed.",
                        ))
                    }
                    Some(_) => {}
                }
            }
            PackageValidationOutcome::Passed => {}
        }
    }
    for assignment in &records.plan.assignments {
        let reviewer_currently_qualified = if let Some(reviewer) = assignment.reviewer.as_ref() {
            if !require_active_reviewer {
                true
            } else {
                connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM agent_profile_heads
                     WHERE profile_id = ?1 AND current_version = ?2 AND status = 'active')",
                        params![reviewer.profile_id, reviewer.profile_version],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(sql_error)?
            }
        } else {
            false
        };
        if !reviewer_currently_qualified {
            blockers.push(blocker(
                ReleaseReadinessBlockerCode::ReviewerUnqualified,
                &assignment.assignment_id,
                "No authorized, active, qualified, independent reviewer is available.",
            ));
            continue;
        }
        match records
            .reviews
            .iter()
            .find(|review| review.assignment_id == assignment.assignment_id)
        {
            None => blockers.push(blocker(
                ReleaseReadinessBlockerCode::ReviewMissing,
                &assignment.assignment_id,
                "The exact Submission Section has not been independently reviewed.",
            )),
            Some(review) => {
                for finding in &review.findings {
                    match finding.severity {
                        ProductionFindingSeverity::Critical => blockers.push(blocker(
                            ReleaseReadinessBlockerCode::CriticalFinding, &finding.finding_id,
                            "Critical findings cannot be waived and require a new corrected package version.")),
                        ProductionFindingSeverity::Major if !records.exceptions.iter().any(|approval| approval.finding_id == finding.finding_id) => blockers.push(blocker(
                            ReleaseReadinessBlockerCode::MajorFinding, &finding.finding_id,
                            "The Major finding has no exact policy-permitted Exception Approval.")),
                        ProductionFindingSeverity::Major | ProductionFindingSeverity::Minor => {}
                    }
                }
            }
        }
    }
    if !package.current {
        blockers.push(blocker(
            ReleaseReadinessBlockerCode::StaleInput,
            &package.package_id,
            "The package or an exact bound dependency is no longer current.",
        ));
    }
    Ok(blockers)
}

fn readiness_summaries(
    package: &SubmissionPackageVersion,
    run: &PackageValidationRun,
    manual: &[PackageManualVerification],
    plan: &FinalReviewPlan,
    reviews: &[SubmissionSectionReview],
    exceptions: &[PackageFindingExceptionApproval],
    baseline_bindings: &[CoordinatedBidBaselineBinding],
) -> Vec<ReleaseReadinessCategorySummary> {
    let categories = [
        (
            "coverage",
            package
                .coverage
                .iter()
                .map(|row| row.requirement.requirement_id.clone())
                .collect(),
        ),
        (
            "baselines",
            vec![format!(
                "{}:{}:{}",
                package.baseline_id, package.baseline_version, package.baseline_manifest_sha256
            )],
        ),
        (
            "validations",
            run.results
                .iter()
                .map(|result| result.result_id.clone())
                .collect(),
        ),
        (
            "calculations",
            package
                .items
                .iter()
                .flat_map(|item| item.calculation_references.iter().cloned())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        ),
        (
            "manual_verifications",
            manual
                .iter()
                .map(|item| item.verification_id.clone())
                .collect(),
        ),
        (
            "execution_and_signatures",
            package
                .coverage
                .iter()
                .filter(|row| {
                    matches!(
                        row.requirement.kind,
                        super::GenerationRequirementKind::ExecutionRequirement
                            | super::GenerationRequirementKind::Signature
                    )
                })
                .map(|row| row.requirement.requirement_id.clone())
                .collect(),
        ),
        (
            "queries_and_treatments",
            baseline_summary_references(baseline_bindings, |binding| {
                binding.source == "query_register"
            }),
        ),
        (
            "assumptions",
            baseline_summary_references(baseline_bindings, |binding| {
                binding.summary == "approved_assumption"
            }),
        ),
        (
            "qualifications",
            baseline_summary_references(baseline_bindings, |binding| {
                binding.category == CoordinatedBidBaselineCategory::Qualification
                    && binding.summary != "approved_assumption"
            }),
        ),
        (
            "exclusions",
            baseline_summary_references(baseline_bindings, |binding| {
                binding.category == CoordinatedBidBaselineCategory::Exclusion
            }),
        ),
        ("departures", Vec::new()),
        (
            "findings",
            reviews
                .iter()
                .flat_map(|review| {
                    review
                        .findings
                        .iter()
                        .map(|finding| finding.finding_id.clone())
                })
                .collect(),
        ),
        (
            "exceptions",
            exceptions
                .iter()
                .map(|exception| exception.approval_id.clone())
                .collect(),
        ),
        (
            "information_boundaries",
            run.results
                .iter()
                .filter(|result| {
                    result.category == PackageValidationCheckCategory::InformationBoundary
                })
                .map(|result| result.result_id.clone())
                .collect(),
        ),
        (
            "deadline",
            package
                .submission_deadline
                .iter()
                .map(|deadline| deadline.reference_id.clone())
                .collect(),
        ),
        (
            "changes",
            package
                .currentness_facts
                .iter()
                .filter(|fact| !fact.current)
                .map(|fact| fact.reference_id.clone())
                .collect(),
        ),
        (
            "final_review_plan",
            plan.assignments
                .iter()
                .map(|assignment| assignment.assignment_id.clone())
                .collect(),
        ),
    ];
    categories
        .into_iter()
        .map(|(category, references)| ReleaseReadinessCategorySummary {
            category: category.into(),
            references,
        })
        .collect()
}

fn baseline_summary_references(
    bindings: &[CoordinatedBidBaselineBinding],
    include: impl Fn(&CoordinatedBidBaselineBinding) -> bool,
) -> Vec<String> {
    bindings
        .iter()
        .filter(|binding| include(binding))
        .map(|binding| {
            format!(
                "{}:{}:{}",
                binding.reference_id, binding.version, binding.manifest_sha256
            )
        })
        .collect()
}

pub(crate) fn load_final_review_for_transaction(
    transaction: &Transaction<'_>,
    package: &ExactSubmissionPackage,
    run_id: Option<&str>,
    _budget: BidPackageOperationBudget,
) -> Result<FinalReviewInspection, TenderCommandError> {
    let (policy, run, plan) = load_policy_run_plan(transaction, &package.package)?;
    if run_id.is_some_and(|expected| expected != run.run_id) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manual_verifications = load_manual_verifications(transaction, &run.run_id)?;
    let reviews = load_reviews(transaction, &plan)?;
    let exceptions = load_exceptions(transaction, &package.package)?;
    let report_json: String = transaction
        .query_row(
            "SELECT reports.report_json FROM release_readiness_report_head AS head
         JOIN release_readiness_reports AS reports
           ON reports.report_id = head.report_id AND reports.version = head.current_version
         WHERE reports.package_id = ?1 AND reports.package_version = ?2
           AND reports.package_manifest_sha256 = ?3",
            params![
                package.package.package_id,
                package.package.version,
                package.package.manifest_sha256
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let report: ReleaseReadinessReport = parse_canonical(&report_json)?;
    let live_blockers = readiness_blockers(
        transaction,
        &package.package,
        &run,
        ReviewRecords {
            manual: &manual_verifications,
            plan: &plan,
            reviews: &reviews,
            exceptions: &exceptions,
        },
        true,
    )?;
    let current = package.package.current;
    let ready = current && live_blockers.is_empty();
    let all_baseline_bindings: Vec<CoordinatedBidBaselineBinding> = transaction
        .query_row(
            "SELECT bindings_json FROM coordinated_bid_baseline_versions
             WHERE baseline_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
            params![
                package.package.baseline_id,
                package.package.baseline_version,
                package.package.baseline_manifest_sha256,
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(sql_error)
        .and_then(|json| parse_canonical(&json))?;
    let decision_bindings = all_baseline_bindings
        .into_iter()
        .filter(|binding| {
            matches!(
                binding.category,
                CoordinatedBidBaselineCategory::Qualification
                    | CoordinatedBidBaselineCategory::Exclusion
                    | CoordinatedBidBaselineCategory::Query
            ) || package
                .package
                .current_decision_references
                .iter()
                .any(|reference| {
                    reference.subject_kind == binding.kind
                        && reference.subject_reference_id == binding.reference_id
                        && reference.subject_version == binding.version
                        && reference.subject_manifest_sha256 == binding.manifest_sha256
                        && (binding.approval_id.as_ref() == Some(&reference.decision_id)
                            || binding.supporting_review_id.as_ref()
                                == Some(&reference.decision_id))
                })
        })
        .collect::<Vec<_>>();
    let decision_evidence = decision_bindings
        .into_iter()
        .map(|binding| load_final_review_decision_evidence(transaction, binding))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FinalReviewInspection {
        package: package.package.clone(),
        decision_evidence,
        policy,
        validation_run: run,
        manual_verifications,
        review_plan: plan,
        reviews,
        exceptions,
        report,
        current,
        ready,
        live_blockers,
        live_changes: package.package.currentness_facts.clone(),
    })
}

fn load_final_review_decision_evidence(
    transaction: &Transaction<'_>,
    binding: CoordinatedBidBaselineBinding,
) -> Result<FinalReviewDecisionEvidence, TenderCommandError> {
    if binding.kind != CoordinatedBidBaselineBindingKind::TenderQueryVersion {
        return Ok(FinalReviewDecisionEvidence {
            category: FinalReviewDecisionEvidenceCategory::OtherDecision,
            binding,
            question: None,
            ambiguity_or_gap: None,
            treatment: None,
            rationale: None,
            treatment_details: None,
            closed: None,
        });
    }
    type QueryDecision = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<bool>,
    );
    let decision: QueryDecision = transaction
        .query_row(
            "SELECT versions.question, versions.ambiguity_or_gap,
                    decisions.treatment, decisions.rationale,
                    decisions.treatment_details, decisions.closes_query
             FROM tender_query_versions AS versions
             LEFT JOIN tender_query_treatment_decisions AS decisions
               ON decisions.query_id = versions.query_id
              AND decisions.query_version = versions.version
             WHERE versions.query_id = ?1 AND versions.version = ?2",
            params![binding.reference_id, binding.version],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .map_err(sql_error)?;
    let category = match decision.2.as_deref() {
        Some("approved_assumption") => FinalReviewDecisionEvidenceCategory::Assumption,
        Some("qualification") => FinalReviewDecisionEvidenceCategory::Qualification,
        Some("exclusion") => FinalReviewDecisionEvidenceCategory::Exclusion,
        _ if decision.5 != Some(true) => FinalReviewDecisionEvidenceCategory::OpenQuery,
        _ => FinalReviewDecisionEvidenceCategory::OtherDecision,
    };
    Ok(FinalReviewDecisionEvidence {
        category,
        binding,
        question: Some(decision.0),
        ambiguity_or_gap: Some(decision.1),
        treatment: decision.2,
        rationale: decision.3,
        treatment_details: decision.4,
        closed: decision.5,
    })
}

fn load_policy_run_plan(
    connection: &Connection,
    package: &SubmissionPackageVersion,
) -> Result<
    (
        PackageValidationPolicy,
        PackageValidationRun,
        FinalReviewPlan,
    ),
    TenderCommandError,
> {
    let (policy_json, run_json, plan_json): (String, String, String) = connection
        .query_row(
            "SELECT policies.manifest_json, runs.manifest_json, plans.plan_json
         FROM package_validation_runs AS runs
         JOIN package_validation_policies AS policies
           ON policies.policy_id = runs.policy_id AND policies.version = runs.policy_version
         JOIN final_review_plans AS plans ON plans.validation_run_id = runs.run_id
         WHERE runs.package_id = ?1 AND runs.package_version = ?2
           AND runs.package_manifest_sha256 = ?3",
            params![package.package_id, package.version, package.manifest_sha256],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    Ok((
        parse_canonical(&policy_json)?,
        parse_canonical(&run_json)?,
        parse_canonical(&plan_json)?,
    ))
}

fn load_manual_verifications(
    connection: &Connection,
    run_id: &str,
) -> Result<Vec<PackageManualVerification>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT verifications.verification_json
         FROM package_manual_verifications AS verifications
         JOIN package_validation_item_results AS results
           ON results.result_id = verifications.validation_result_id
         WHERE results.run_id = ?1 ORDER BY results.ordinal",
        )
        .map_err(sql_error)?;
    let result = statement
        .query_map([run_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?
        .map(|row| parse_canonical(&row.map_err(sql_error)?))
        .collect();
    result
}

fn load_reviews(
    connection: &Connection,
    plan: &FinalReviewPlan,
) -> Result<Vec<SubmissionSectionReview>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT reviews.review_json FROM submission_section_reviews AS reviews
         JOIN final_review_assignments AS assignments USING (assignment_id)
         WHERE assignments.plan_id = ?1 ORDER BY assignments.ordinal",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([&plan.plan_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?;
    let mut reviews = Vec::new();
    for row in rows {
        let review: SubmissionSectionReview = parse_canonical(&row.map_err(sql_error)?)?;
        reviews.push(review);
    }
    Ok(reviews)
}

fn load_exceptions(
    connection: &Connection,
    package: &SubmissionPackageVersion,
) -> Result<Vec<PackageFindingExceptionApproval>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT approval.approval_json
             FROM package_finding_exception_approvals AS approval
             JOIN audit_events AS event
               ON event.event_type = 'package_finding_exception_approved'
              AND json_extract(event.payload_json, '$.change.reference_id') = approval.approval_id
             WHERE approval.package_id = ?1 AND approval.package_version = ?2
             ORDER BY event.sequence",
        )
        .map_err(sql_error)?;
    let result = statement
        .query_map(params![package.package_id, package.version], |row| {
            row.get::<_, String>(0)
        })
        .map_err(sql_error)?
        .map(|row| parse_canonical(&row.map_err(sql_error)?))
        .collect();
    result
}

fn supported_structure(media_type: &str, bytes: &[u8]) -> bool {
    if media_type == "application/pdf" {
        return bytes.starts_with(b"%PDF-") && bytes.windows(5).any(|window| window == b"%%EOF");
    }
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        return false;
    };
    let required = if media_type.contains("wordprocessingml") {
        "word/document.xml"
    } else if media_type.contains("spreadsheetml") {
        "xl/workbook.xml"
    } else {
        return false;
    };
    let result = archive.by_name(required).is_ok();
    result
}

fn package_document_text(media_type: &str, bytes: &[u8]) -> Result<String, TenderCommandError> {
    if media_type == "application/pdf" {
        return Ok(String::from_utf8_lossy(bytes).into_owned());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut text = String::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        text.push_str(file.name());
        text.push('\n');
        if file.name().ends_with(".xml") || file.name().ends_with(".rels") {
            let mut part = String::new();
            file.read_to_string(&mut part)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            text.push_str(&part);
        }
    }
    Ok(text)
}

fn contains_hidden_content(text: &str) -> bool {
    let lowercase = text.to_ascii_lowercase();
    let mut text = String::with_capacity(lowercase.len());
    let mut chars = lowercase.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_ascii_whitespace() {
            let mut lookahead = chars.clone();
            while lookahead
                .peek()
                .is_some_and(|next| next.is_ascii_whitespace())
            {
                lookahead.next();
            }
            if lookahead.peek() == Some(&'=') || text.ends_with('=') {
                continue;
            }
        }
        text.push(character);
    }
    [
        "w:vanish",
        "w:del",
        "trackrevisions",
        "state=\"hidden\"",
        "state='hidden'",
        "state=\"veryhidden\"",
        "state='veryhidden'",
        "hidden=\"1\"",
        "hidden='1'",
        "hidden=\"true\"",
        "hidden='true'",
        "vbaproject.bin",
        "externallink",
        "targetmode=\"external\"",
        "targetmode='external'",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn valid_package_filename(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1000
        && !path.contains(['\\', '\0', ':'])
        && !path.starts_with('/')
        && path.split('/').all(|component| {
            !component.is_empty()
                && component != "."
                && component != ".."
                && !component.ends_with([' ', '.'])
                && !component.chars().any(char::is_control)
        })
}

fn bool_outcome(passed: bool) -> PackageValidationOutcome {
    if passed {
        PackageValidationOutcome::Passed
    } else {
        PackageValidationOutcome::Failed
    }
}

fn blocker(
    code: ReleaseReadinessBlockerCode,
    reference_id: &str,
    detail: &str,
) -> ReleaseReadinessBlocker {
    ReleaseReadinessBlocker {
        code,
        reference_id: reference_id.into(),
        detail: detail.into(),
    }
}

fn normalized(values: &[String]) -> Result<Vec<String>, TenderCommandError> {
    let mut values = values
        .iter()
        .map(|value| value.trim().to_owned())
        .collect::<Vec<_>>();
    if values.iter().any(String::is_empty) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    values.sort();
    values.dedup();
    Ok(values)
}

fn normalized_nonempty(values: &[String]) -> Result<Vec<String>, TenderCommandError> {
    let values = normalized(values)?;
    if values.is_empty() {
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    } else {
        Ok(values)
    }
}

fn manifest_sha256<T: Serialize + Clone>(value: &T) -> Result<String, TenderCommandError> {
    let mut canonical_value = serde_json::to_value(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
    canonical_value
        .as_object_mut()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
        .insert("manifest_sha256".into(), Value::String(String::new()));
    Ok(sha256_hex(canonical_json(&canonical_value)?.as_bytes()))
}

fn stored_manifest_hash_is_valid(value: &str, expected: &str) -> bool {
    if !is_canonical_json(value) {
        return false;
    }
    let Ok(mut parsed) = serde_json::from_str::<Value>(value) else {
        return false;
    };
    let Some(object) = parsed.as_object_mut() else {
        return false;
    };
    if object.get("manifest_sha256").and_then(Value::as_str) != Some(expected) {
        return false;
    }
    object.insert("manifest_sha256".into(), Value::String(String::new()));
    canonical_json(&parsed)
        .ok()
        .is_some_and(|canonical| sha256_hex(canonical.as_bytes()) == expected)
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
}

fn parse_canonical<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T, TenderCommandError> {
    if !is_canonical_json(value) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn is_canonical_json(value: &str) -> bool {
    serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|parsed| serde_json_canonicalizer::to_string(&parsed).ok())
        .is_some_and(|canonical| canonical == value)
}

fn append_review_audit(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    event_type: &str,
    reference_id: &str,
    created_at: &str,
) -> Result<i64, TenderCommandError> {
    append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        event_type,
        tender_revision,
        json!({"reference_id": reference_id}),
        created_at,
    )
}

impl TenderStore {
    pub(crate) fn prepare_submission_section_review_run(
        &mut self,
        tender_id: &TenderId,
        command: &RunSubmissionSectionReviewCommand,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        self.require_storage_writable()?;
        let application_home = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .to_path_buf();
        let provider_selection = self.required_tender_ai_execution_selection()?;
        let run_id = random_identifier(&self.connection)?;
        let workspace = application_home
            .join("staging")
            .join(format!("agent-{}-{run_id}", tender_id.as_str()));
        let prepared = (|| {
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
                BidPackageOperationBudget::from_connection(&transaction)?,
            )?;
            let (policy, run, plan) = load_policy_run_plan(&transaction, &package.package)?;
            if plan.plan_id != command.plan_id
                || plan.manifest_sha256 != command.plan_manifest_sha256
                || run.package_manifest_sha256 != command.package_manifest_sha256
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let assignment = plan
                .assignments
                .iter()
                .find(|assignment| assignment.assignment_id == command.assignment_id)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let expected_reviewer = assignment
                .reviewer
                .as_ref()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let already_reviewed: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM submission_section_reviews WHERE assignment_id = ?1)",
                    [&assignment.assignment_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let unresolved_indeterminate: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs AS runs
                       JOIN tender_tasks AS tasks ON tasks.task_id = runs.task_id
                       WHERE runs.status = 'indeterminate'
                         AND EXISTS (
                           SELECT 1 FROM json_each(tasks.exact_inputs_json) AS input
                           WHERE json_extract(input.value, '$.kind') = 'submission_review_assignment'
                         )
                         AND NOT EXISTS (
                           SELECT 1 FROM agent_run_recovery_dispositions AS dispositions
                           WHERE dispositions.run_id = runs.run_id
                         )
                     )",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if already_reviewed || unresolved_indeterminate {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let profile = load_profile(
                &transaction,
                (
                    expected_reviewer.profile_id.clone(),
                    expected_reviewer.profile_version,
                ),
            )?;
            let profile_is_active: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_profile_heads
                       WHERE profile_id = ?1 AND current_version = ?2 AND status = 'active'
                     )",
                    params![profile.profile_id, profile.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let profile_is_busy: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs
                       WHERE profile_id = ?1 AND profile_version = ?2 AND status = 'running'
                     )",
                    params![profile.profile_id, profile.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if !profile_is_active
                || profile_is_busy
                || profile.identity != expected_reviewer.identity
                || profile.capabilities != expected_reviewer.capabilities
                || !package
                    .package
                    .work_plan
                    .authorized_profile_versions
                    .iter()
                    .any(|authorized| {
                        authorized.profile_id == profile.profile_id
                            && authorized.profile_version == profile.version
                    })
                || assignment.author_profile_versions.iter().any(|author| {
                    author.profile_id == profile.profile_id
                        && author.profile_version == profile.version
                })
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let created_at = sqlite_timestamp(&transaction)?;
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = TenderTaskView {
                task_id: random_identifier(&transaction)?,
                profile_id: profile.profile_id.clone(),
                profile_version: profile.version,
                objective: format!(
                    "Independently review Submission Package section '{}' against its exact validation evidence without changing package bytes.",
                    assignment.section_key
                ),
                exact_inputs: vec![
                    AgentTaskInputReference {
                        kind: "tender_revision".into(),
                        reference: tender_id.as_str().into(),
                        version: package.package.tender_revision,
                    },
                    AgentTaskInputReference {
                        kind: "work_plan".into(),
                        reference: package.package.work_plan.plan_id.clone(),
                        version: package.package.work_plan.plan_version,
                    },
                    AgentTaskInputReference {
                        kind: "submission_package".into(),
                        reference: package.package.package_id.clone(),
                        version: package.package.version,
                    },
                    AgentTaskInputReference {
                        kind: "submission_package_manifest".into(),
                        reference: package.package.manifest_sha256.clone(),
                        version: 1,
                    },
                    AgentTaskInputReference {
                        kind: "package_validation_run".into(),
                        reference: run.run_id.clone(),
                        version: 1,
                    },
                    AgentTaskInputReference {
                        kind: "submission_review_plan".into(),
                        reference: plan.plan_id.clone(),
                        version: 1,
                    },
                    AgentTaskInputReference {
                        kind: "submission_review_assignment".into(),
                        reference: assignment.assignment_id.clone(),
                        version: 1,
                    },
                ],
                output_contract_json: canonical_json(&json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["assignment_id", "package_id", "package_version", "package_manifest_sha256", "result", "findings"],
                    "properties": {
                        "assignment_id": {"const": assignment.assignment_id},
                        "package_id": {"const": package.package.package_id},
                        "package_version": {"const": package.package.version},
                        "package_manifest_sha256": {"const": package.package.manifest_sha256},
                        "result": {"enum": ["satisfied", "requires_remediation"]},
                        "findings": {"type": "array", "maxItems": MAX_REVIEW_FINDINGS}
                    }
                }))?,
                review_policy: assignment.criteria.join(" "),
                deadline: deadline.clone(),
                permissions: profile.permissions.clone(),
                resource_budget: profile.resource_budget.clone(),
                repair_feedback: None,
            };
            let scoped_items = package
                .items
                .iter()
                .filter(|exact| assignment.item_ids.contains(&exact.item.item_id))
                .map(|exact| {
                    let bytes = load_exact_submission_package_item_bytes_for_transaction(
                        &self.root.join("content"),
                        exact,
                    )?;
                    Ok(json!({
                        "item": exact.item,
                        "content_base64": BASE64_STANDARD.encode(bytes),
                    }))
                })
                .collect::<Result<Vec<_>, TenderCommandError>>()?;
            let scoped_requirement_ids = package
                .package
                .items
                .iter()
                .filter(|item| assignment.item_ids.contains(&item.item_id))
                .flat_map(|item| item.requirement_ids.iter().cloned())
                .collect::<BTreeSet<_>>();
            let scoped_coverage = package
                .package
                .coverage
                .iter()
                .filter(|coverage| {
                    scoped_requirement_ids.contains(&coverage.requirement.requirement_id)
                })
                .collect::<Vec<_>>();
            let scoped_sections = package
                .package
                .sections
                .iter()
                .filter(|section| {
                    section.section_key == assignment.section_key
                        && section.envelope_key == assignment.envelope_key
                        && section.language == assignment.language
                })
                .collect::<Vec<_>>();
            let scoped_record_ids = scoped_coverage
                .iter()
                .map(|coverage| coverage.requirement.record.record_id.as_str())
                .collect::<BTreeSet<_>>();
            let scoped_rules = policy
                .fixed_rules
                .iter()
                .chain(policy.tender_rules.iter().filter(|rule| {
                    rule.source.as_ref().is_some_and(|source| {
                        scoped_record_ids.contains(source.reference_id.as_str())
                    })
                }))
                .collect::<Vec<_>>();
            let scoped_results = run
                .results
                .iter()
                .filter(|result| {
                    result
                        .item_id
                        .as_ref()
                        .is_none_or(|item_id| assignment.item_ids.contains(item_id))
                })
                .collect::<Vec<_>>();
            let scoped_manual_verifications = load_manual_verifications(&transaction, &run.run_id)?
                .into_iter()
                .filter(|verification| assignment.item_ids.contains(&verification.item_id))
                .collect::<Vec<_>>();
            let data_classification = profile
                .permissions
                .data_classifications
                .iter()
                .max()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let payload = json!({
                "schema_version": 1,
                "data_scope": profile.permissions.data_scopes.join("+"),
                "data_classification": data_classification,
                "assignment": assignment,
                "package": {
                    "package_id": package.package.package_id,
                    "version": package.package.version,
                    "manifest_sha256": package.package.manifest_sha256,
                    "assessment": package.package.assessment,
                    "coverage": scoped_coverage,
                    "sections": scoped_sections,
                    "items": scoped_items,
                    "baseline": {
                        "baseline_id": package.package.baseline_id,
                        "version": package.package.baseline_version,
                        "manifest_sha256": package.package.baseline_manifest_sha256,
                        "approval_id": package.package.baseline_approval_id,
                    },
                    "calculations": package.package.calculation_manifest_references,
                    "deadline": package.package.submission_deadline,
                },
                "policy": {
                    "manifest_sha256": policy.manifest_sha256,
                    "rules": scoped_rules,
                },
                "validation_run": {
                    "run_id": run.run_id,
                    "manifest_sha256": run.manifest_sha256,
                    "results": scoped_results,
                },
                "manual_verifications": scoped_manual_verifications,
            });
            insert_task(&transaction, &task, &created_at)?;
            let (permission_grant, materialized_workspace) =
                derive_planned_task_grant(PlannedTaskGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home: &application_home,
                    tender_id: tender_id.as_str(),
                    work_plan_version: package.package.work_plan.plan_version,
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                    expires_at: &deadline,
                    payload: &payload,
                })?;
            if materialized_workspace != workspace
                || permission_duration(&permission_grant, Timestamp::now())
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
                    .is_zero()
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let existing_thread: Option<(String, String)> = transaction
                .query_row(
                    "SELECT thread_ref, status FROM provider_threads
                     WHERE profile_id = ?1 AND profile_version = ?2
                       AND status IN ('active', 'archive_pending')",
                    params![profile.profile_id, profile.version],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (provider_thread_ref, provider_thread_to_archive) = match existing_thread {
                Some((thread_ref, status)) if status == "archive_pending" => {
                    (None, Some(thread_ref))
                }
                Some((thread_ref, status)) if status == "active" => {
                    let exposure = load_thread_exposure(&transaction, &thread_ref)?;
                    if exposure.is_compatible_with(&permission_grant) {
                        (Some(thread_ref), None)
                    } else {
                        if transaction
                            .execute(
                                "UPDATE provider_threads SET status = 'archive_pending'
                                 WHERE thread_ref = ?1 AND status = 'active'",
                                [&thread_ref],
                            )
                            .map_err(sql_error)?
                            != 1
                        {
                            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                        }
                        append_audit_event_with_sequence(
                            &transaction,
                            tender_id.as_str(),
                            "provider_thread_archive_requested",
                            package.package.tender_revision,
                            json!({
                                "reason": "thread_exposure_incompatible",
                                "run_id": run_id,
                                "thread_ref": thread_ref,
                            }),
                            &created_at,
                        )?;
                        (None, Some(thread_ref))
                    }
                }
                Some(_) => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
                None => (None, None),
            };
            ensure_agent_run_capacity(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO agent_runs (
                       run_id, task_id, profile_id, profile_version,
                       permission_grant_json, status, started_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
                    params![
                        run_id,
                        task.task_id,
                        profile.profile_id,
                        profile.version,
                        canonical_json(&permission_grant)?,
                        created_at,
                    ],
                )
                .map_err(sql_error)?;
            super::record_agent_run_provider_binding(
                &transaction,
                &run_id,
                &provider_selection,
                &created_at,
            )?;
            insert_event(
                &transaction,
                &run_id,
                1,
                PendingProviderEvent {
                    kind: ProviderEventKind::RunStarted,
                    summary: "Independent Submission Package section review started".into(),
                    correlation_id: None,
                    request_fingerprint: None,
                    denial_reason: None,
                    opaque_reference: None,
                },
                &created_at,
            )?;
            append_audit_event_with_sequence(
                &transaction,
                tender_id.as_str(),
                "submission_section_review_started",
                package.package.tender_revision,
                json!({
                    "assignment_id": assignment.assignment_id,
                    "package_id": package.package.package_id,
                    "package_version": package.package.version.to_string(),
                    "reviewer_profile_id": profile.profile_id,
                    "run_id": run_id,
                    "task_id": task.task_id,
                }),
                &created_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            Ok(PreparedAgentRun {
                run_id,
                provider_selection,
                profile,
                task,
                permission_grant,
                provider_thread_ref,
                provider_thread_to_archive,
                workspace: workspace.clone(),
            })
        })();
        if prepared.is_err() {
            let _ = fs::remove_dir_all(&workspace);
        }
        prepared
    }
}

// Agent Runtime integration is implemented at the exact task-input seam in agent_records.rs.
pub(crate) fn submission_section_review_task(task: &TenderTaskView) -> bool {
    task.exact_inputs
        .iter()
        .any(|input| input.kind == "submission_review_assignment")
}

pub(crate) fn validate_submission_section_review_candidate(
    task: &TenderTaskView,
    payload: &str,
) -> Result<SubmissionSectionReviewCandidate, TenderCommandError> {
    if payload.len() > 64 * 1024 {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let candidate: SubmissionSectionReviewCandidate = serde_json::from_str(payload)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if candidate.findings.len() > MAX_REVIEW_FINDINGS
        || (candidate.result == PackageReviewResult::RequiresRemediation
            && candidate.findings.is_empty())
        || candidate.assignment_id.is_empty()
        || candidate.package_id.is_empty()
        || candidate.package_manifest_sha256.len() != 64
        || candidate.findings.iter().any(|finding| {
            finding.policy_rule_id.trim().is_empty()
                || finding.summary.trim().is_empty()
                || finding.evidence_references.is_empty()
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let assignment = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "submission_review_assignment")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let package = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "submission_package")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let package_manifest = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "submission_package_manifest")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if assignment.reference != candidate.assignment_id
        || package.reference != candidate.package_id
        || package.version != candidate.package_version
        || package_manifest.reference != candidate.package_manifest_sha256
        || (candidate.result == PackageReviewResult::Satisfied
            && candidate
                .findings
                .iter()
                .any(|finding| !matches!(finding.severity, ProductionFindingSeverity::Minor)))
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(candidate)
}

pub(crate) fn submission_section_review_target_is_open(
    connection: &Transaction<'_>,
    task: &TenderTaskView,
) -> Result<bool, TenderCommandError> {
    let assignment = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "submission_review_assignment")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let package_input = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "submission_package")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let manifest_input = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "submission_package_manifest")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let open: bool = connection
        .query_row(
            "SELECT EXISTS(
           SELECT 1 FROM final_review_assignments AS assignments
           JOIN final_review_plans AS plans USING (plan_id)
           JOIN submission_package_head AS head
             ON head.package_id = plans.package_id AND head.current_version = plans.package_version
           WHERE assignments.assignment_id = ?1
             AND assignments.reviewer_profile_id = ?2
             AND assignments.reviewer_profile_version = ?3
             AND NOT EXISTS(SELECT 1 FROM submission_section_reviews WHERE assignment_id = ?1)
         )",
            params![assignment.reference, task.profile_id, task.profile_version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if !open {
        return Ok(false);
    }
    match load_submission_package_for_review_transaction(
        connection,
        &package_input.reference,
        package_input.version,
        &manifest_input.reference,
        true,
        BidPackageOperationBudget::from_connection(connection)?,
    ) {
        Ok(_) => Ok(true),
        Err(error) if error.code == TenderErrorCode::InvalidCommand => Ok(false),
        Err(error) => Err(error),
    }
}

pub(crate) fn publish_submission_section_review(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    reviewer_run_id: &str,
    reviewer_profile: &AgentProfileVersionView,
    task: &TenderTaskView,
    candidate: &SubmissionSectionReviewCandidate,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let package = load_submission_package_for_review_transaction(
        transaction,
        &candidate.package_id,
        candidate.package_version,
        &candidate.package_manifest_sha256,
        true,
        BidPackageOperationBudget::from_connection(transaction)?,
    )?;
    let (policy, run, plan) = load_policy_run_plan(transaction, &package.package)?;
    let assignment_json: String = transaction
        .query_row(
            "SELECT assignment_json FROM final_review_assignments WHERE assignment_id = ?1",
            [&candidate.assignment_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let assignment: FinalReviewAssignment = parse_canonical(&assignment_json)?;
    if !plan
        .assignments
        .iter()
        .any(|planned| planned == &assignment)
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let expected_reviewer = assignment
        .reviewer
        .as_ref()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let qualification = if assignment.required_capability == "independent_review" {
        "independent_review".to_owned()
    } else {
        format!("review_{}", assignment.required_capability)
    };
    let profile_is_active: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM agent_profile_heads
             WHERE profile_id = ?1 AND current_version = ?2 AND status = 'active')",
            params![reviewer_profile.profile_id, reviewer_profile.version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if expected_reviewer.profile_id != reviewer_profile.profile_id
        || expected_reviewer.profile_version != reviewer_profile.version
        || expected_reviewer.identity != reviewer_profile.identity
        || expected_reviewer.capabilities != reviewer_profile.capabilities
        || !profile_is_active
        || !reviewer_profile.capabilities.contains(&qualification)
        || !package
            .package
            .work_plan
            .authorized_profile_versions
            .iter()
            .any(|authorized| {
                authorized.profile_id == reviewer_profile.profile_id
                    && authorized.profile_version == reviewer_profile.version
            })
        || assignment.author_profile_versions.iter().any(|author| {
            author.profile_id == reviewer_profile.profile_id
                && author.profile_version == reviewer_profile.version
        })
        || !submission_section_review_target_is_open(transaction, task)?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let allowed_evidence = review_allowed_evidence(&package.package, &run, &assignment);
    for finding in &candidate.findings {
        let rule = policy
            .fixed_rules
            .iter()
            .chain(&policy.tender_rules)
            .find(|rule| rule.rule_id == finding.policy_rule_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !severity_is_policy_permitted(finding.severity, rule.severity)
            || finding
                .evidence_references
                .iter()
                .any(|reference| !allowed_evidence.contains(reference))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    let review_id = random_identifier(transaction)?;
    let mut findings = candidate
        .findings
        .iter()
        .enumerate()
        .map(|(index, finding)| {
            Ok(PackageReviewFinding {
                finding_id: random_identifier(transaction)?,
                review_id: review_id.clone(),
                sequence: u32::try_from(index + 1)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
                severity: finding.severity,
                policy_rule_id: finding.policy_rule_id.trim().into(),
                summary: finding.summary.trim().into(),
                evidence_references: normalized_nonempty(&finding.evidence_references)?,
                created_at: created_at.into(),
            })
        })
        .collect::<Result<Vec<_>, TenderCommandError>>()?;
    let reviewer = FinalReviewReviewer {
        profile_id: reviewer_profile.profile_id.clone(),
        profile_version: reviewer_profile.version,
        identity: reviewer_profile.identity.clone(),
        capabilities: reviewer_profile.capabilities.clone(),
    };
    let mut review = SubmissionSectionReview {
        review_id: review_id.clone(),
        assignment_id: assignment.assignment_id.clone(),
        package_id: candidate.package_id.clone(),
        package_version: candidate.package_version,
        package_manifest_sha256: candidate.package_manifest_sha256.clone(),
        reviewer_run_id: reviewer_run_id.into(),
        reviewer,
        required_capability: assignment.required_capability.clone(),
        criteria: assignment.criteria.clone(),
        result: candidate.result,
        findings: findings.clone(),
        manifest_sha256: String::new(),
        created_at: created_at.into(),
    };
    review.manifest_sha256 = manifest_sha256(&review)?;
    transaction
        .execute(
            "INSERT INTO submission_section_reviews (
           review_id, assignment_id, package_id, package_version, package_manifest_sha256,
           reviewer_run_id, reviewer_profile_id, reviewer_profile_version, result,
           review_json, manifest_sha256, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                review.review_id,
                review.assignment_id,
                review.package_id,
                review.package_version,
                review.package_manifest_sha256,
                review.reviewer_run_id,
                review.reviewer.profile_id,
                review.reviewer.profile_version,
                review.result.as_str(),
                canonical_json(&review)?,
                review.manifest_sha256,
                review.created_at
            ],
        )
        .map_err(sql_error)?;
    for finding in &mut findings {
        transaction
            .execute(
                "INSERT INTO submission_section_review_findings (
               finding_id, review_id, ordinal, severity, policy_rule_id, finding_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    finding.finding_id,
                    finding.review_id,
                    finding.sequence,
                    finding_severity_str(finding.severity),
                    finding.policy_rule_id,
                    canonical_json(finding)?
                ],
            )
            .map_err(sql_error)?;
    }
    let through_event_sequence = append_review_audit(
        transaction,
        tender_id,
        package.package.tender_revision,
        "submission_section_review_published",
        &review_id,
        created_at,
    )?;
    persist_release_readiness_report(
        transaction,
        &package.package,
        &policy,
        &run,
        &plan,
        through_event_sequence,
        created_at,
    )?;
    Ok(())
}

fn review_allowed_evidence(
    package: &SubmissionPackageVersion,
    run: &PackageValidationRun,
    assignment: &FinalReviewAssignment,
) -> BTreeSet<String> {
    std::iter::once(package.manifest_sha256.clone())
        .chain(
            package
                .items
                .iter()
                .filter(|item| assignment.item_ids.contains(&item.item_id))
                .flat_map(|item| [item.item_id.clone(), item.content_sha256.clone()]),
        )
        .chain(
            run.results
                .iter()
                .filter(|result| {
                    result
                        .item_id
                        .as_ref()
                        .is_none_or(|item_id| assignment.item_ids.contains(item_id))
                })
                .map(|result| result.result_id.clone()),
        )
        .chain(
            package
                .coverage
                .iter()
                .filter(|coverage| {
                    coverage
                        .item_id
                        .as_ref()
                        .is_some_and(|item_id| assignment.item_ids.contains(item_id))
                })
                .flat_map(|coverage| {
                    [
                        coverage.requirement.record.record_id.clone(),
                        coverage.requirement.record.manifest_sha256.clone(),
                    ]
                }),
        )
        .collect()
}

fn finding_severity_str(severity: ProductionFindingSeverity) -> &'static str {
    match severity {
        ProductionFindingSeverity::Critical => "critical",
        ProductionFindingSeverity::Major => "major",
        ProductionFindingSeverity::Minor => "minor",
    }
}

fn severity_is_policy_permitted(
    proposed: ProductionFindingSeverity,
    policy: ProductionFindingSeverity,
) -> bool {
    if policy == ProductionFindingSeverity::Critical {
        return proposed == ProductionFindingSeverity::Critical;
    }
    severity_rank(proposed) <= severity_rank(policy)
}

fn severity_rank(severity: ProductionFindingSeverity) -> u8 {
    match severity {
        ProductionFindingSeverity::Minor => 1,
        ProductionFindingSeverity::Major => 2,
        ProductionFindingSeverity::Critical => 3,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use rusqlite::{params, Connection};
    use zip::{write::SimpleFileOptions, ZipWriter};

    use super::{
        calculation_bindings_are_present, contains_cross_envelope_value, contains_hidden_content,
        find_reusable_item_result, package_document_text, rendered_values_are_present,
    };

    #[test]
    fn rendered_text_and_calculation_mismatches_fail_exact_comparison() {
        assert!(rendered_values_are_present(
            "Deadline 31 December Total 125.00",
            &["Deadline 31 December", "Total 125.00"]
        ));
        assert!(!rendered_values_are_present(
            "Deadline 30 December Total 125.00",
            &["Deadline 31 December", "Total 125.00"]
        ));
        let connection = Connection::open_in_memory().expect("calculation fixture");
        connection
            .execute_batch(
                "CREATE TABLE approved_tender_prices (
                   pricing_calculation_run_id TEXT,
                   calculation_manifest_sha256 TEXT,
                   final_amount TEXT,
                   currency TEXT
                 );",
            )
            .expect("calculation schema");
        let run_id = "a".repeat(32);
        let manifest = "b".repeat(64);
        connection
            .execute(
                "INSERT INTO approved_tender_prices VALUES (?1, ?2, '125.00', 'AED')",
                params![run_id, manifest],
            )
            .expect("approved price");
        let references = vec![format!("{run_id}:1:{manifest}")];
        let workbook = |amount: &str| {
            let mut bytes = Cursor::new(Vec::new());
            {
                let mut archive = ZipWriter::new(&mut bytes);
                archive
                    .start_file("xl/worksheets/sheet1.xml", SimpleFileOptions::default())
                    .expect("worksheet part");
                write!(
                    archive,
                    "<worksheet><sheetData><row r=\"3\">\
                     <c r=\"A3\" t=\"inlineStr\"><is><t>Approved Tender Price</t></is></c>\
                     <c r=\"B3\"><v>{amount}</v></c>\
                     <c r=\"C3\" t=\"inlineStr\"><is><t>AED</t></is></c>\
                     <c r=\"D3\" t=\"inlineStr\"><is><t>{run_id}</t></is></c>\
                     <c r=\"E3\" t=\"inlineStr\"><is><t>{manifest}</t></is></c>\
                     <c r=\"F3\" t=\"inlineStr\"><is><t>125.00</t></is></c>\
                     </row><row r=\"9\"><c r=\"A9\" t=\"inlineStr\"><is><t>125.00 AED {run_id} {manifest}</t></is></c></row></sheetData></worksheet>"
                )
                .expect("worksheet XML");
                archive.finish().expect("finish workbook");
            }
            bytes.into_inner()
        };
        assert!(
            calculation_bindings_are_present(&connection, &workbook("125"), &references)
                .expect("matching displayed calculation")
        );
        assert!(
            !calculation_bindings_are_present(&connection, &workbook("130"), &references)
                .expect("mismatched displayed calculation")
        );
    }

    #[test]
    fn exact_value_from_another_information_envelope_is_a_leak() {
        let values = ["Commercial total 125.00", "ALT"];
        assert!(contains_cross_envelope_value(
            "Technical answer. Commercial total 125.00",
            &values
        ));
        assert!(!contains_cross_envelope_value(
            "Technical answer only.",
            &values
        ));
    }

    #[test]
    fn item_reuse_requires_identical_hash_check_version_policy_and_context() {
        let connection = Connection::open_in_memory().expect("cache fixture");
        connection
            .execute_batch(
                "CREATE TABLE package_validation_item_results (
                   result_id TEXT, content_sha256 TEXT, check_id TEXT, check_version INTEGER,
                   policy_manifest_sha256 TEXT, validation_context_sha256 TEXT, outcome TEXT
                 );",
            )
            .expect("cache schema");
        let content = "a".repeat(64);
        let policy = "b".repeat(64);
        let context = "c".repeat(64);
        connection
            .execute(
                "INSERT INTO package_validation_item_results VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'passed')",
                params!["result-1", content, "quantix.content_hash", 1, policy, context],
            )
            .expect("seed reusable result");
        assert_eq!(
            find_reusable_item_result(
                &connection,
                &content,
                "quantix.content_hash",
                1,
                &policy,
                &context
            )
            .expect("lookup exact cache key")
            .as_deref(),
            Some("result-1")
        );
        for (changed_content, changed_check, changed_version, changed_policy, changed_context) in [
            (
                "d".repeat(64),
                "quantix.content_hash",
                1,
                policy.clone(),
                context.clone(),
            ),
            (
                content.clone(),
                "quantix.filename",
                1,
                policy.clone(),
                context.clone(),
            ),
            (
                content.clone(),
                "quantix.content_hash",
                2,
                policy.clone(),
                context.clone(),
            ),
            (
                content.clone(),
                "quantix.content_hash",
                1,
                "e".repeat(64),
                context.clone(),
            ),
            (
                content.clone(),
                "quantix.content_hash",
                1,
                policy.clone(),
                "f".repeat(64),
            ),
        ] {
            assert!(find_reusable_item_result(
                &connection,
                &changed_content,
                changed_check,
                changed_version,
                &changed_policy,
                &changed_context
            )
            .expect("lookup changed cache key")
            .is_none());
        }
    }

    #[test]
    fn hidden_content_recognizes_ooxml_variants_and_macro_entry_names() {
        for marker in [
            "<sheet state='veryHidden'/>",
            "<row hidden='true'/>",
            "<Relationship TargetMode='External'/>",
            "<sheet state = \"veryHidden\"/>",
            "<row hidden = 'true'/>",
        ] {
            assert!(contains_hidden_content(marker), "missed marker: {marker}");
        }
        assert!(!contains_hidden_content(
            "<sheet state='visible'/><row hidden='false'/><Relationship TargetMode='Internal'/>"
        ));

        let mut bytes = Cursor::new(Vec::new());
        {
            let mut archive = ZipWriter::new(&mut bytes);
            archive
                .start_file("word/document.xml", SimpleFileOptions::default())
                .expect("start document part");
            archive.write_all(b"<w:document/>").expect("write part");
            archive
                .start_file("word/vbaProject.bin", SimpleFileOptions::default())
                .expect("start macro entry");
            archive.write_all(b"macro").expect("write macro entry");
            archive.finish().expect("finish OOXML fixture");
        }
        let text = package_document_text(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            bytes.get_ref(),
        )
        .expect("inspect OOXML package");
        assert!(contains_hidden_content(&text));
    }
}
