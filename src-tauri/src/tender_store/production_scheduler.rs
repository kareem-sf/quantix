use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use rust_decimal::Decimal;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::{
    agent_runtime::{
        permissions::{derive_planned_task_grant, permission_duration, PlannedTaskGrantRequest},
        AgentRunInspection, AgentRunState, AgentTaskInputReference, DataClassification,
        PendingProviderEvent, PreparedAgentRun, ProviderEventKind, ProviderRateLimitState,
        ProviderUsage, TenderTaskView,
    },
    application_settings::AiExecutionSelection,
    QuantixHost,
};

use super::bid_decisions::package_dependencies_are_current;
use super::tender_queries::{
    agent_query_publication_is_valid, approved_query_treatments_for_inputs,
    approved_query_treatments_for_task, production_query_contexts_for_task,
    production_task_state_after_query_release, publish_agent_query_proposals,
    task_has_blocking_query, task_has_current_query_artifact_invalidation, AgentQueryPublication,
    AgentTenderQueryProposal, AgentTenderQueryUpdate, TenderQueryTreatment,
};
use super::{
    agent_records::{
        ensure_agent_run_capacity, insert_event, insert_task, load_profile, load_task,
        load_thread_exposure,
    },
    append_audit_event, append_audit_event_with_sequence, lock_mutex_with_check, random_identifier,
    require_setup, sha256_hex, sql_error, sqlite_timestamp, BidPackageOperationBudget,
    ChangeAssessmentStatus, MajorFindingPolicy, TenderCommandError, TenderErrorCode, TenderId,
    TenderRecordKind, TenderStore, WorkPlanDecision, WorkPlanProfileBinding,
    WorkPlanRevisionAction, WorkPlanTask,
};

const MAX_PRODUCTION_TASKS: usize = 256;
const MAX_PRODUCTION_TASK_ATTEMPTS: u32 = 8;
const MAX_PRODUCTION_FINDINGS: usize = 32;
const MAX_PRODUCTION_EVIDENCE_REFERENCES: usize = 256;
const MAX_PRODUCTION_REVIEW_SCOPE_ITEMS: usize = 16;
const MAX_PRODUCTION_REVIEW_CRITERIA: usize = 16;
const MAX_PRODUCTION_COORDINATION_ASSIGNMENTS: usize = 1_024;
const MAX_PRODUCTION_COORDINATION_OUTPUT_BYTES: usize = 192 * 1024;
const MAX_PRODUCTION_COORDINATION_CONTRACT_BYTES: usize = 1024 * 1024;

type StoredProductionActivation = (String, String, u32, String, String, String, String, String);
type PreparedProductionTaskRow = (
    String,
    u32,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
);
type LatestProductionAttempt = (u32, String, String, Option<String>, Option<String>, String);
type StoredProductionReviewTarget = (String, u32, String, String, String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProductionTaskCarryForwardManifest {
    schema_version: u32,
    carry_forward_id: String,
    assessment_id: String,
    source_production_task_id: String,
    source_task_definition_sha256: String,
    source_readiness_id: String,
    source_artifact_id: String,
    source_artifact_version: u32,
    source_payload_sha256: String,
    source_review_id: Option<String>,
    source_finding_dispositions_sha256: String,
    source_plan_manifest_sha256: String,
    target_production_task_id: String,
    target_task_definition_sha256: String,
    target_readiness_id: String,
    target_plan_manifest_sha256: String,
    compatibility_sha256: String,
    carried_forward_by: String,
    acting_role: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct ProductionRecoveryContext {
    assessment_id: String,
    source_activation_id: String,
    source_plan_manifest_sha256: String,
}

struct ProductionCarryForwardTarget<'a> {
    production_task_id: &'a str,
    definition: &'a WorkPlanTask,
    definition_sha256: &'a str,
    plan_manifest_sha256: &'a str,
    tender_revision: u32,
    created_at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ActivateTenderProductionCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub plan_id: String,
    #[garde(range(min = 1))]
    pub plan_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub plan_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunProductionTaskCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub production_task_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApproveProductionFindingExceptionCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub production_task_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub finding_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub review_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub artifact_id: String,
    #[garde(range(min = 1, max = 8))]
    pub artifact_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub payload_sha256: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub consequence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectProductionTaskReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub production_task_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProductionTaskState {
    Blocked,
    Ready,
    Running,
    ReviewReady,
    Reviewing,
    RemediationReady,
    QueryBlocked,
    ReadyForIntegration,
    AttemptLimitReached,
    Failed,
    Cancelled,
    Indeterminate,
    Suspended,
}

impl ProductionTaskState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::ReviewReady => "review_ready",
            Self::Reviewing => "reviewing",
            Self::RemediationReady => "remediation_ready",
            Self::QueryBlocked => "query_blocked",
            Self::ReadyForIntegration => "ready_for_integration",
            Self::AttemptLimitReached => "attempt_limit_reached",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Indeterminate => "indeterminate",
            Self::Suspended => "suspended",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "blocked" => Ok(Self::Blocked),
            "ready" => Ok(Self::Ready),
            "running" => Ok(Self::Running),
            "review_ready" => Ok(Self::ReviewReady),
            "reviewing" => Ok(Self::Reviewing),
            "remediation_ready" => Ok(Self::RemediationReady),
            "query_blocked" => Ok(Self::QueryBlocked),
            "ready_for_integration" => Ok(Self::ReadyForIntegration),
            "attempt_limit_reached" => Ok(Self::AttemptLimitReached),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "indeterminate" => Ok(Self::Indeterminate),
            "suspended" => Ok(Self::Suspended),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionArtifactVersionSummary {
    pub artifact_id: String,
    pub version: u32,
    pub author_run_id: String,
    pub prior_version: Option<u32>,
    pub remediation_review_id: Option<String>,
    pub payload_sha256: String,
    pub output_validation_passed: bool,
    pub evidence_verified: bool,
    pub data_scopes: Vec<String>,
    pub data_classifications: Vec<DataClassification>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionArtifactVersion {
    #[serde(flatten)]
    pub summary: ProductionArtifactVersionSummary,
    pub payload: ProductionArtifactPayload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProductionReviewResult {
    Satisfied,
    RequiresRemediation,
}

impl ProductionReviewResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::RequiresRemediation => "requires_remediation",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "satisfied" => Ok(Self::Satisfied),
            "requires_remediation" => Ok(Self::RequiresRemediation),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProductionFindingSeverity {
    Critical,
    Major,
    Minor,
}

impl ProductionFindingSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Major => "major",
            Self::Minor => "minor",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "critical" => Ok(Self::Critical),
            "major" => Ok(Self::Major),
            "minor" => Ok(Self::Minor),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }

    fn blocks_integration(self) -> bool {
        matches!(self, Self::Critical | Self::Major)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProductionFindingDispositionKind {
    RemediationVerified,
    ExceptionApproved,
}

impl ProductionFindingDispositionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::RemediationVerified => "remediation_verified",
            Self::ExceptionApproved => "exception_approved",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "remediation_verified" => Ok(Self::RemediationVerified),
            "exception_approved" => Ok(Self::ExceptionApproved),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionFindingDisposition {
    pub disposition_id: String,
    pub kind: ProductionFindingDispositionKind,
    pub target_artifact_id: String,
    pub target_version: u32,
    pub verifying_review_id: Option<String>,
    pub decided_by: String,
    pub acting_role: String,
    pub rationale: String,
    pub consequence: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionReviewFinding {
    pub finding_id: String,
    pub review_id: String,
    pub sequence: u32,
    pub severity: ProductionFindingSeverity,
    pub summary: String,
    pub evidence_references: Vec<String>,
    pub disposition: Option<ProductionFindingDisposition>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionReview {
    pub review_id: String,
    pub target_artifact_id: String,
    pub target_version: u32,
    pub target_payload_sha256: String,
    pub reviewer_run_id: String,
    pub reviewer_profile_id: String,
    pub reviewer_profile_version: u32,
    pub capability: String,
    pub scope: Vec<String>,
    pub criteria: Vec<String>,
    pub inputs: Vec<AgentTaskInputReference>,
    pub result: ProductionReviewResult,
    pub resolved_finding_ids: Vec<String>,
    pub findings: Vec<ProductionReviewFinding>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionIntegrationReadiness {
    pub readiness_id: String,
    pub artifact_id: String,
    pub artifact_version: u32,
    pub payload_sha256: String,
    pub review_id: Option<String>,
    pub output_validation_passed: bool,
    pub evidence_verified: bool,
    pub dependencies_satisfied: bool,
    pub approval_gates: Vec<String>,
    pub finding_dispositions_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionTaskReviewInspection {
    pub production_task_id: String,
    pub artifact_versions: Vec<ProductionArtifactVersion>,
    pub reviews: Vec<ProductionReview>,
    pub readiness: Option<ProductionIntegrationReadiness>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionTaskInspection {
    pub production_task_id: String,
    pub plan_manifest_sha256: String,
    pub task: WorkPlanTask,
    pub state: ProductionTaskState,
    pub run_ids: Vec<String>,
    pub artifact_version_count: u32,
    pub review_count: u32,
    pub finding_count: u32,
    pub open_blocking_finding_count: u32,
    pub latest_artifact: Option<ProductionArtifactVersionSummary>,
    pub latest_review_result: Option<ProductionReviewResult>,
    pub query_control_available: bool,
    pub ready_for_integration: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderProductionInspection {
    pub activation_id: String,
    pub plan_id: String,
    pub plan_version: u32,
    pub plan_manifest_sha256: String,
    pub active: bool,
    pub tasks: Vec<ProductionTaskInspection>,
    pub activated_by: String,
    pub acting_role: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionTaskRunResult {
    pub run: AgentRunInspection,
    pub task: ProductionTaskInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProductionArtifactPayload {
    pub summary: String,
    pub evidence_references: Vec<String>,
    pub gaps: Vec<String>,
    pub coordination_observations: Vec<ProductionCoordinationObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remediations: Vec<ProductionRemediation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_treatment_applications: Vec<ProductionQueryTreatmentApplication>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_proposals: Vec<AgentTenderQueryProposal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub query_updates: Vec<AgentTenderQueryUpdate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProductionCoordinationObservationSubject {
    SubmissionDeadline,
    ResponsibleParty,
    ScopeQualification,
    ScopeExclusion,
    ExpectedDeliveryCost,
    ApprovedTenderPrice,
    CommercialAppetite,
    TechnicalCommitment,
    ProgrammeCommitment,
    ProcurementCommitment,
    ContractualCommitment,
    RiskCommitment,
    SubmissionCommitment,
    QueryTreatment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[ts(export)]
pub enum ProductionCoordinationObservationValue {
    Text { text: String },
    Amount { value: String, currency: String },
    TextSet { values: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProductionCoordinationObservation {
    pub subject: ProductionCoordinationObservationSubject,
    pub value: ProductionCoordinationObservationValue,
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProductionCoordinationSourceObservation {
    subject: ProductionCoordinationObservationSubject,
    value: ProductionCoordinationObservationValue,
    reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProductionCoordinationAssignmentContract {
    subject: ProductionCoordinationObservationSubject,
    required_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProductionCoordinationContract {
    required_subjects: Vec<ProductionCoordinationObservationSubject>,
    assignment_contracts: Vec<ProductionCoordinationAssignmentContract>,
    source_observations: Vec<ProductionCoordinationSourceObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProductionRemediation {
    pub finding_id: String,
    pub treatment: String,
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProductionQueryTreatmentApplication {
    pub decision_id: String,
    pub query_id: String,
    pub query_version: u32,
    pub treatment: TenderQueryTreatment,
    pub application: String,
    pub evidence_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionQueryControlCandidate {
    query_updates: Vec<AgentTenderQueryUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionReviewCandidate {
    result: ProductionReviewResult,
    resolved_finding_ids: Vec<String>,
    findings: Vec<ProductionReviewFindingCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionReviewFindingCandidate {
    severity: ProductionFindingSeverity,
    summary: String,
    evidence_references: Vec<String>,
}

impl QuantixHost {
    pub fn activate_tender_production(
        &self,
        command: ActivateTenderProductionCommand,
    ) -> Result<TenderProductionInspection, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .activate_tender_production(&tender_id, &command, budget);
        result
    }

    pub fn inspect_tender_production(
        &self,
        tender_id: &str,
    ) -> Result<Option<TenderProductionInspection>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_tender_production(budget);
        result
    }

    pub fn inspect_production_task_review(
        &self,
        command: InspectProductionTaskReviewCommand,
    ) -> Result<ProductionTaskReviewInspection, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_production_task_review(&command.production_task_id, budget);
        result
    }

    pub fn approve_production_finding_exception(
        &self,
        command: ApproveProductionFindingExceptionCommand,
    ) -> Result<ProductionTaskReviewInspection, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .approve_production_finding_exception(&tender_id, &command, budget);
        result
    }
}

impl TenderStore {
    pub(super) fn production_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let (
            activation_count,
            task_count,
            attempt_count,
            artifact_count,
            review_count,
            finding_count,
            disposition_count,
            readiness_count,
            carry_forward_count,
        ): (u32, u32, u32, u32, u32, u32, u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM production_activations),
                    (SELECT COUNT(*) FROM production_tasks),
                    (SELECT COUNT(*) FROM production_task_attempts),
                    (SELECT COUNT(*) FROM production_artifact_versions),
                    (SELECT COUNT(*) FROM production_reviews),
                    (SELECT COUNT(*) FROM production_review_findings),
                    (SELECT COUNT(*) FROM production_finding_dispositions),
                    (SELECT COUNT(*) FROM production_integration_readiness),
                    (SELECT COUNT(*) FROM production_task_carry_forwards)",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        if activation_count > 256
            || task_count > activation_count.saturating_mul(MAX_PRODUCTION_TASKS as u32)
            || attempt_count > task_count.saturating_mul(MAX_PRODUCTION_TASK_ATTEMPTS)
            || artifact_count > task_count.saturating_mul(MAX_PRODUCTION_TASK_ATTEMPTS)
            || review_count > task_count.saturating_mul(MAX_PRODUCTION_TASK_ATTEMPTS)
            || finding_count > review_count.saturating_mul(MAX_PRODUCTION_FINDINGS as u32)
            || disposition_count > finding_count
            || readiness_count > task_count.saturating_mul(MAX_PRODUCTION_TASK_ATTEMPTS)
            || carry_forward_count > task_count
        {
            return Ok(false);
        }
        let active_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM production_activations WHERE status = 'active'",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if active_count > 1 {
            return Ok(false);
        }
        let mut activation_statement = self
            .connection
            .prepare(
                "SELECT activation_id, plan_id, plan_version, plan_manifest_sha256,
                        status, activated_by, acting_role, created_at
                 FROM production_activations ORDER BY rowid",
            )
            .map_err(sql_error)?;
        let activations = activation_statement
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
                ))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        for activation in activations {
            check()?;
            if activation.5 != "engineer_user"
                || activation.6 != "tendering_manager"
                || !matches!(activation.4.as_str(), "active" | "suspended" | "superseded")
            {
                return Ok(false);
            }
            let plan_row: Option<(String, String, String, String)> = self
                .connection
                .query_row(
                    "SELECT versions.manifest_sha256, versions.tasks_json,
                            versions.profiles_json, approvals.decision
                     FROM work_plan_versions AS versions
                     JOIN work_plan_approvals AS approvals
                       ON approvals.plan_id = versions.plan_id
                      AND approvals.plan_version = versions.version
                     WHERE versions.plan_id = ?1 AND versions.version = ?2",
                    params![activation.1, activation.2],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let Some((manifest_sha256, plan_tasks_json, plan_profiles_json, decision)) = plan_row
            else {
                return Ok(false);
            };
            if manifest_sha256 != activation.3 || decision != "approve" {
                return Ok(false);
            }
            let plan_tasks: Vec<WorkPlanTask> = parse_canonical_json(&plan_tasks_json)?;
            let plan_profiles: Vec<WorkPlanProfileBinding> =
                parse_canonical_json(&plan_profiles_json)?;
            if plan_tasks.is_empty()
                || plan_tasks.len() > MAX_PRODUCTION_TASKS
                || plan_profiles.is_empty()
                || plan_profiles.len() > MAX_PRODUCTION_TASKS
            {
                return Ok(false);
            }
            if activation.4 == "active"
                && !self
                    .connection
                    .query_row(
                        "SELECT EXISTS(
                           SELECT 1 FROM work_plan_heads
                           JOIN tender ON tender.singleton = 1
                           WHERE plan_id = ?1 AND current_version = ?2
                              AND tender.lifecycle_phase IN (
                                'active_production', 'integrated_review', 'package_production',
                                'final_review'
                              )
                         )",
                        params![activation.1, activation.2],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(sql_error)?
            {
                return Ok(false);
            }
            if activation.4 == "active" {
                for binding in &plan_profiles {
                    check()?;
                    if !self
                        .connection
                        .query_row(
                            "SELECT EXISTS(
                               SELECT 1 FROM agent_profile_heads
                               WHERE profile_id = ?1 AND current_version = ?2
                                 AND status = 'active'
                             )",
                            params![binding.profile.profile_id, binding.profile.version],
                            |row| row.get::<_, bool>(0),
                        )
                        .map_err(sql_error)?
                    {
                        return Ok(false);
                    }
                }
            }
            let stored_task_count: u32 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM production_tasks WHERE activation_id = ?1",
                    [&activation.0],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if stored_task_count as usize != plan_tasks.len() {
                return Ok(false);
            }
            let mut audit_statement = self
                .connection
                .prepare(
                    "SELECT payload_json FROM audit_events
                     WHERE event_type = 'tender_production_activated' ORDER BY sequence",
                )
                .map_err(sql_error)?;
            let audit_payloads = audit_statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            if audit_payloads.len() > 256 {
                return Ok(false);
            }
            let audit_exists = audit_payloads.iter().any(|payload| {
                serde_json::from_str::<serde_json::Value>(payload).is_ok_and(|payload| {
                    payload
                        .pointer("/change/activation_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(activation.0.as_str())
                        && payload
                            .pointer("/change/plan_manifest_sha256")
                            .and_then(serde_json::Value::as_str)
                            == Some(activation.3.as_str())
                })
            });
            if !audit_exists {
                return Ok(false);
            }
            for definition in plan_tasks {
                check()?;
                if production_coordination_contract(&self.connection, &definition).is_err() {
                    return Ok(false);
                }
                let stored: Option<(String, String, String, String, Option<String>)> = self
                    .connection
                    .query_row(
                        "SELECT production_task_id, task_definition_json,
                                task_definition_sha256, status, task_id
                         FROM production_tasks WHERE activation_id = ?1 AND task_key = ?2",
                        params![activation.0, definition.task_key],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(sql_error)?;
                let Some((production_task_id, task_json, task_definition_sha256, status, task_id)) =
                    stored
                else {
                    return Ok(false);
                };
                if parse_canonical_json::<WorkPlanTask>(&task_json)? != definition
                    || sha256_hex(task_json.as_bytes()) != task_definition_sha256
                {
                    return Ok(false);
                }
                let state = ProductionTaskState::parse(&status)?;
                let carried_forward = match production_task_carry_forward_is_valid(
                    &self.connection,
                    &production_task_id,
                    &definition,
                    &task_definition_sha256,
                    &activation.3,
                )? {
                    Some(valid) if !valid => return Ok(false),
                    Some(true) => true,
                    None => false,
                    Some(false) => unreachable!(),
                };
                let profile = load_profile(
                    &self.connection,
                    (definition.profile_id.clone(), definition.profile_version),
                )?;
                if activation.4 == "active"
                    && !self
                        .connection
                        .query_row(
                            "SELECT EXISTS(
                               SELECT 1 FROM agent_profile_heads
                               WHERE profile_id = ?1 AND current_version = ?2
                                 AND status = 'active'
                             )",
                            params![definition.profile_id, definition.profile_version],
                            |row| row.get::<_, bool>(0),
                        )
                        .map_err(sql_error)?
                {
                    return Ok(false);
                }
                let mut dependencies_ready = true;
                for dependency in &definition.dependencies {
                    check()?;
                    if !self
                        .connection
                        .query_row(
                            "SELECT EXISTS(
                               SELECT 1 FROM production_tasks
                               WHERE activation_id = ?1 AND task_key = ?2
                                 AND status = 'ready_for_integration'
                             )",
                            params![activation.0, dependency],
                            |row| row.get::<_, bool>(0),
                        )
                        .map_err(sql_error)?
                    {
                        dependencies_ready = false;
                        break;
                    }
                }
                if activation.4 == "active"
                    && ((state == ProductionTaskState::Ready && !dependencies_ready)
                        || (state == ProductionTaskState::Blocked && dependencies_ready))
                {
                    return Ok(false);
                }
                let query_blocked =
                    task_has_blocking_query(&self.connection, &definition.task_key)?;
                if activation.4 == "active"
                    && ((state == ProductionTaskState::QueryBlocked && !query_blocked)
                        || (query_blocked
                            && !matches!(
                                state,
                                ProductionTaskState::QueryBlocked
                                    | ProductionTaskState::Running
                                    | ProductionTaskState::Reviewing
                                    | ProductionTaskState::AttemptLimitReached
                                    | ProductionTaskState::Failed
                                    | ProductionTaskState::Cancelled
                                    | ProductionTaskState::Indeterminate
                            )))
                {
                    return Ok(false);
                }
                let mut attempt_statement = self
                    .connection
                    .prepare(
                        "SELECT attempt_number, attempt_kind, task_id FROM production_task_attempts
                         WHERE production_task_id = ?1 ORDER BY attempt_number",
                    )
                    .map_err(sql_error)?;
                let attempts = attempt_statement
                    .query_map([&production_task_id], |row| {
                        Ok((
                            row.get::<_, u32>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })
                    .map_err(sql_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sql_error)?;
                if attempts.len() > MAX_PRODUCTION_TASK_ATTEMPTS as usize
                    || attempts
                        .iter()
                        .enumerate()
                        .any(|(index, attempt)| attempt.0 as usize != index + 1)
                    || attempts.last().map(|attempt| attempt.2.as_str()) != task_id.as_deref()
                    || (attempts.is_empty()
                        && !matches!(
                            state,
                            ProductionTaskState::Blocked
                                | ProductionTaskState::Ready
                                | ProductionTaskState::QueryBlocked
                                | ProductionTaskState::Suspended
                        )
                        && !(carried_forward && state == ProductionTaskState::ReadyForIntegration))
                    || (!attempts.is_empty()
                        && matches!(
                            state,
                            ProductionTaskState::Blocked | ProductionTaskState::Ready
                        )
                        && !attempts.iter().all(|attempt| attempt.1 == "query_control"))
                {
                    return Ok(false);
                }
                let mut prior_run_id: Option<String> = None;
                let mut prior_attempt_kind: Option<String> = None;
                for (attempt_index, (_, attempt_kind, attempt_task_id)) in
                    attempts.iter().enumerate()
                {
                    check()?;
                    let task = load_task(&self.connection, attempt_task_id)?;
                    let attempt_profile = if attempt_kind == "query_control" {
                        let query_input = task
                            .exact_inputs
                            .iter()
                            .find(|input| input.kind == "tender_query_version")
                            .ok_or_else(|| {
                                TenderCommandError::new(TenderErrorCode::IntegrityFailed)
                            })?;
                        let owner: (String, u32) = self
                            .connection
                            .query_row(
                                "SELECT owner_profile_id, owner_profile_version
                                 FROM tender_query_versions
                                 WHERE query_id = ?1 AND version = ?2",
                                params![query_input.reference, query_input.version],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .map_err(sql_error)?;
                        load_profile(&self.connection, owner)?
                    } else if attempt_kind == "review" {
                        load_profile(
                            &self.connection,
                            (
                                definition.review_profile_id.clone().ok_or_else(|| {
                                    TenderCommandError::new(TenderErrorCode::IntegrityFailed)
                                })?,
                                definition.review_profile_version.ok_or_else(|| {
                                    TenderCommandError::new(TenderErrorCode::IntegrityFailed)
                                })?,
                            ),
                        )?
                    } else {
                        profile.clone()
                    };
                    let input_set = task
                        .exact_inputs
                        .iter()
                        .map(|input| (&input.kind, &input.reference, input.version))
                        .collect::<std::collections::HashSet<_>>();
                    let inputs_valid = input_set.len() == task.exact_inputs.len()
                        && if attempt_kind == "query_control" {
                            task.exact_inputs
                                .iter()
                                .filter(|input| input.kind == "tender_query_version")
                                .count()
                                == 1
                        } else if attempt_kind == "review" {
                            task.exact_inputs
                                .iter()
                                .filter(|input| input.kind == "production_artifact_version")
                                .count()
                                == 1
                                && task.exact_inputs.iter().all(|input| {
                                    matches!(
                                        input.kind.as_str(),
                                        "production_artifact_version"
                                            | "tender_query_version"
                                            | "approved_query_treatment"
                                    )
                                })
                        } else {
                            definition.exact_inputs.iter().all(|input| {
                                input_set.contains(&(&input.kind, &input.reference, input.version))
                            }) && (attempt_kind != "remediation"
                                || task.exact_inputs.iter().any(|input| {
                                    matches!(
                                        input.kind.as_str(),
                                        "production_review_finding"
                                            | "approved_query_treatment"
                                            | "tender_query_version"
                                            | "change_assessment"
                                    )
                                }))
                        };
                    let expected_objective = if attempt_kind == "query_control" {
                        format!(
                            "Add attributable Evidence or propose a treatment for the exact blocked Tender Query affecting {}; do not decide or close it.",
                            definition.task_key
                        )
                    } else if attempt_kind == "review" {
                        format!(
                            "Independently review the exact candidate for {} without editing or approving it.",
                            definition.task_key
                        )
                    } else {
                        definition.objective.clone()
                    };
                    let expected_contract = if attempt_kind == "query_control" {
                        production_query_control_output_contract()?
                    } else if attempt_kind == "review" {
                        production_review_output_contract()?
                    } else if attempt_kind == "remediation" {
                        production_remediation_output_contract()?
                    } else {
                        definition.output_contract_json.clone()
                    };
                    let expected_review_policy = if attempt_kind == "query_control" {
                        "The specialist may add exact Evidence and propose treatments, but cannot approve a treatment or close the Query."
                    } else if attempt_kind == "review" {
                        "This separate review must report whether the exact candidate satisfies the approved review policy; it cannot edit or approve the work."
                    } else {
                        profile.review_policy.as_str()
                    };
                    if task.profile_id != attempt_profile.profile_id
                        || task.profile_version != attempt_profile.version
                        || task.objective != expected_objective
                        || !inputs_valid
                        || task.output_contract_json != expected_contract
                        || task.review_policy != expected_review_policy
                        || task.deadline != definition.deadline
                        || task.permissions != attempt_profile.permissions
                        || task.resource_budget != attempt_profile.resource_budget
                    {
                        return Ok(false);
                    }
                    let mut run_statement = self
                        .connection
                        .prepare(
                            "SELECT run_id, status, retry_of_run_id FROM agent_runs
                             WHERE task_id = ?1 ORDER BY run_sequence LIMIT 2",
                        )
                        .map_err(sql_error)?;
                    let runs = run_statement
                        .query_map([attempt_task_id], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<String>>(2)?,
                            ))
                        })
                        .map_err(sql_error)?
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(sql_error)?;
                    let [(run_id, run_status, retry_of_run_id)] = runs.as_slice() else {
                        return Ok(false);
                    };
                    if attempt_kind == "query_control" && run_status == "completed" {
                        let Some(query_input) = task
                            .exact_inputs
                            .iter()
                            .find(|input| input.kind == "tender_query_version")
                        else {
                            return Ok(false);
                        };
                        let successor_version = query_input.version.saturating_add(1);
                        let publication_count: u32 = self
                            .connection
                            .query_row(
                                "SELECT COUNT(*) FROM tender_query_versions
                                 WHERE source_run_id = ?1 AND query_id = ?2 AND version = ?3",
                                params![run_id, query_input.reference, successor_version],
                                |row| row.get(0),
                            )
                            .map_err(sql_error)?;
                        if publication_count != 1 {
                            return Ok(false);
                        }
                    }
                    let latest = attempt_index + 1 == attempts.len();
                    let expected_retry = (prior_attempt_kind.as_deref()
                        == Some(attempt_kind.as_str()))
                    .then_some(prior_run_id.as_deref())
                    .flatten();
                    let next_kind = attempts
                        .get(attempt_index + 1)
                        .map(|attempt| attempt.1.as_str());
                    let query_remediation_next = if matches!(
                        (attempt_kind.as_str(), next_kind),
                        ("author", Some("remediation"))
                    ) {
                        let next_task =
                            load_task(&self.connection, &attempts[attempt_index + 1].2)?;
                        next_task.exact_inputs.iter().any(|input| {
                            matches!(
                                input.kind.as_str(),
                                "approved_query_treatment" | "tender_query_version"
                            )
                        })
                    } else {
                        false
                    };
                    let retry_terminal = matches!(
                        run_status.as_str(),
                        "failed" | "interrupted" | "indeterminate"
                    ) && (run_status != "indeterminate"
                        || self
                            .connection
                            .query_row(
                                "SELECT EXISTS(
                                   SELECT 1 FROM agent_run_recovery_dispositions
                                   WHERE run_id = ?1 AND disposition = 'retry_task'
                                 )",
                                [run_id],
                                |row| row.get::<_, bool>(0),
                            )
                            .map_err(sql_error)?);
                    let terminal_prior = !latest
                        && ((run_status == "completed"
                            && matches!(
                                (attempt_kind.as_str(), next_kind),
                                ("author" | "remediation", Some("review"))
                                    | ("review", Some("remediation"))
                            ))
                            || (run_status == "completed"
                                && (attempt_kind == "query_control"
                                    || next_kind == Some("query_control")))
                            || (run_status == "completed" && query_remediation_next)
                            || retry_terminal);
                    let latest_matches = latest
                        && match state {
                            ProductionTaskState::Running => {
                                matches!(
                                    attempt_kind.as_str(),
                                    "author" | "remediation" | "query_control"
                                ) && run_status == "running"
                            }
                            ProductionTaskState::ReviewReady => {
                                matches!(attempt_kind.as_str(), "author" | "remediation")
                                    && run_status == "completed"
                            }
                            ProductionTaskState::Reviewing => {
                                attempt_kind == "review" && run_status == "running"
                            }
                            ProductionTaskState::RemediationReady => {
                                if TenderStore::task_has_active_change_rework(
                                    &self.connection,
                                    &production_task_id,
                                )? {
                                    matches!(
                                        (attempt_kind.as_str(), run_status.as_str()),
                                        ("author" | "remediation" | "review", "completed")
                                    )
                                } else if attempt_kind == "query_control" && run_status == "failed"
                                {
                                    production_task_state_after_query_release(
                                        &self.connection,
                                        &production_task_id,
                                    )? == "remediation_ready"
                                } else if run_status == "completed" && attempt_kind == "review" {
                                    let result: Option<String> = self
                                        .connection
                                        .query_row(
                                            "SELECT result FROM production_reviews
                                                 WHERE reviewer_run_id = ?1",
                                            [run_id],
                                            |row| row.get(0),
                                        )
                                        .optional()
                                        .map_err(sql_error)?;
                                    if result.as_deref() == Some("requires_remediation") {
                                        true
                                    } else if result.as_deref() == Some("satisfied") {
                                        let latest_artifact: Option<(String, u32)> = self
                                            .connection
                                            .query_row(
                                                "SELECT artifact_id, version
                                                     FROM production_artifact_versions
                                                     WHERE production_task_id = ?1
                                                     ORDER BY version DESC LIMIT 1",
                                                [&production_task_id],
                                                |row| Ok((row.get(0)?, row.get(1)?)),
                                            )
                                            .optional()
                                            .map_err(sql_error)?;
                                        latest_artifact.is_some_and(|(artifact_id, version)| {
                                            production_artifact_applies_current_query_treatments(
                                                &self.connection,
                                                &definition.task_key,
                                                &artifact_id,
                                                version,
                                            )
                                            .is_ok_and(|current| !current)
                                        })
                                    } else {
                                        false
                                    }
                                } else if run_status == "completed"
                                    && matches!(attempt_kind.as_str(), "author" | "remediation")
                                {
                                    let latest_artifact: Option<(String, u32)> = self
                                        .connection
                                        .query_row(
                                            "SELECT artifact_id, version
                                                 FROM production_artifact_versions
                                                 WHERE production_task_id = ?1
                                                 ORDER BY version DESC LIMIT 1",
                                            [&production_task_id],
                                            |row| Ok((row.get(0)?, row.get(1)?)),
                                        )
                                        .optional()
                                        .map_err(sql_error)?;
                                    latest_artifact.is_some_and(|(artifact_id, version)| {
                                        production_artifact_applies_current_query_treatments(
                                            &self.connection,
                                            &definition.task_key,
                                            &artifact_id,
                                            version,
                                        )
                                        .is_ok_and(|current| !current)
                                    })
                                } else {
                                    false
                                }
                            }
                            ProductionTaskState::QueryBlocked => {
                                matches!(
                                    run_status.as_str(),
                                    "completed" | "failed" | "interrupted"
                                )
                            }
                            ProductionTaskState::ReadyForIntegration => run_status == "completed",
                            ProductionTaskState::AttemptLimitReached => {
                                attempts.len() == MAX_PRODUCTION_TASK_ATTEMPTS as usize
                                    && matches!(
                                        run_status.as_str(),
                                        "completed" | "failed" | "interrupted" | "indeterminate"
                                    )
                                    && audit_event_matches_production_task(
                                        &self.connection,
                                        "production_task_attempt_limit_reached",
                                        &production_task_id,
                                        run_id,
                                    )?
                            }
                            ProductionTaskState::Failed => run_status == "failed",
                            ProductionTaskState::Cancelled => run_status == "interrupted",
                            ProductionTaskState::Indeterminate => run_status == "indeterminate",
                            ProductionTaskState::Suspended => matches!(
                                run_status.as_str(),
                                "completed" | "failed" | "interrupted" | "indeterminate"
                            ),
                            ProductionTaskState::Blocked | ProductionTaskState::Ready => {
                                attempt_kind == "query_control"
                                    && matches!(run_status.as_str(), "completed" | "failed")
                            }
                        };
                    if retry_of_run_id.as_deref() != expected_retry
                        || (!terminal_prior && !latest_matches)
                        || !audit_event_matches_production_task(
                            &self.connection,
                            "production_task_started",
                            &production_task_id,
                            run_id,
                        )?
                        || (run_status != "running"
                            && !audit_event_matches_production_task(
                                &self.connection,
                                "production_task_finished",
                                &production_task_id,
                                run_id,
                            )?
                            && !audit_event_matches_production_task(
                                &self.connection,
                                "production_task_reconciled",
                                &production_task_id,
                                run_id,
                            )?)
                    {
                        return Ok(false);
                    }
                    prior_run_id = Some(run_id.clone());
                    prior_attempt_kind = Some(attempt_kind.clone());
                }
                if !carried_forward
                    && !production_task_records_are_valid(
                        &self.connection,
                        &production_task_id,
                        &definition,
                        state,
                        check,
                    )?
                {
                    return Ok(false);
                }
            }
        }
        check()?;
        Ok(true)
    }

    pub(super) fn reconcile_interrupted_production_tasks(
        &mut self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let interrupted = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT tasks.production_task_id, tasks.task_key, runs.run_id, runs.status,
                            tasks.status
                     FROM production_tasks AS tasks
                     JOIN agent_runs AS runs ON runs.task_id = tasks.task_id
                     WHERE tasks.status IN ('running', 'reviewing') AND runs.status != 'running'
                     ORDER BY runs.run_sequence",
                )
                .map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            rows
        };
        if interrupted.is_empty() {
            return Ok(());
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let completed_at = sqlite_timestamp(&transaction)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        for (production_task_id, task_key, run_id, run_status, prior_status) in interrupted {
            let state = match run_status.as_str() {
                "interrupted" => ProductionTaskState::Cancelled,
                "failed" => ProductionTaskState::Failed,
                "indeterminate" => ProductionTaskState::Indeterminate,
                _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
            };
            if transaction
                .execute(
                    "UPDATE production_tasks SET status = ?2, updated_at = ?3
                     WHERE production_task_id = ?1 AND status = ?4",
                    params![
                        production_task_id,
                        state.as_str(),
                        completed_at,
                        prior_status
                    ],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "production_task_reconciled",
                tender_revision,
                json!({
                    "production_task_id": production_task_id,
                    "reason": "host_restart",
                    "run_id": run_id,
                    "status": state.as_str(),
                    "task_key": task_key,
                }),
                &completed_at,
            )?;
        }
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn prepare_production_task_run(
        &mut self,
        tender_id: &TenderId,
        provider_selection: &AiExecutionSelection,
        production_task_id: &str,
        expected_retry_of_run_id: Option<&str>,
        subscription_capacity_exhausted: bool,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        self.require_storage_writable()?;
        if self
            .unresolved_change_assessment()?
            .is_some_and(|(_, status)| status == ChangeAssessmentStatus::Pending)
        {
            self.record_production_denial(
                tender_id,
                "run_production_task",
                Some(production_task_id),
                "change_assessment_pending",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let plan_basis: Option<(String, u32)> = self
            .connection
            .query_row(
                "SELECT activations.plan_id, activations.plan_version
                 FROM production_tasks AS tasks
                 JOIN production_activations AS activations
                   ON activations.activation_id = tasks.activation_id
                 WHERE tasks.production_task_id = ?1",
                [production_task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some((plan_id, plan_version)) = plan_basis.as_ref() {
            let targeted_change_rework =
                TenderStore::task_has_active_change_rework(&self.connection, production_task_id)?;
            if !targeted_change_rework
                && !work_plan_package_dependencies_are_current(self, plan_id, *plan_version)?
            {
                self.record_production_denial(
                    tender_id,
                    "run_production_task",
                    Some(production_task_id),
                    "work_plan_dependencies_stale",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let run_id = random_identifier(&self.connection)?;
        let application_home = self
            .root
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let workspace = application_home.join("staging").join(format!(
            "agent-{}-{}",
            tender_id.as_str(),
            run_id
        ));
        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let row: Option<PreparedProductionTaskRow> = transaction
                .query_row(
                    "SELECT activations.plan_id, activations.plan_version,
                            activations.plan_manifest_sha256, tasks.task_definition_json,
                            activations.activation_id, tasks.status, activations.status, tasks.task_id
                     FROM production_tasks AS tasks
                     JOIN production_activations AS activations
                       ON activations.activation_id = tasks.activation_id
                     WHERE tasks.production_task_id = ?1",
                    [production_task_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                        ))
                    },
                )
                .optional()
                .map_err(sql_error)?;
            let (
                plan_id,
                plan_version,
                plan_manifest_sha256,
                task_json,
                activation_id,
                task_status,
                activation_status,
                current_task_id,
            ) = row.ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let latest_attempt: Option<LatestProductionAttempt> = transaction
                .query_row(
                    "SELECT attempts.attempt_number, runs.run_id, runs.status, runs.failure_json,
                            (SELECT disposition FROM agent_run_recovery_dispositions
                             WHERE run_id = runs.run_id), attempts.attempt_kind
                     FROM production_task_attempts AS attempts
                     JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                     WHERE attempts.production_task_id = ?1
                     ORDER BY attempts.attempt_number DESC LIMIT 1",
                    [production_task_id],
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
                .optional()
                .map_err(sql_error)?;
            let definition: WorkPlanTask = parse_canonical_json(&task_json)?;
            let dependencies_ready: bool = transaction
                .query_row(
                    "SELECT NOT EXISTS(
                       SELECT 1 FROM json_each(?1, '$.dependencies') AS dependency
                       LEFT JOIN production_tasks AS prerequisite
                         ON prerequisite.activation_id = ?2
                        AND prerequisite.task_key = dependency.value
                       WHERE prerequisite.production_task_id IS NULL
                          OR prerequisite.status != 'ready_for_integration'
                     )",
                    params![task_json, activation_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if task_status == "ready" && !dependencies_ready {
                append_production_denial(
                    &transaction,
                    tender_id,
                    "run_production_task",
                    Some(production_task_id),
                    "dependencies_not_ready",
                )?;
                transaction
                    .execute(
                        "UPDATE production_tasks SET status = 'blocked', updated_at = ?2
                         WHERE production_task_id = ?1 AND status = 'ready'",
                        params![production_task_id, sqlite_timestamp(&transaction)?],
                    )
                    .map_err(sql_error)?;
                transaction.commit().map_err(sql_error)?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let all_production_query_contexts =
                production_query_contexts_for_task(&transaction, &definition.task_key)?;
            let query_control_context = (task_status == "query_blocked"
                || latest_attempt
                    .as_ref()
                    .is_some_and(|attempt| attempt.5 == "query_control"))
            .then(|| {
                all_production_query_contexts
                    .iter()
                    .find(|context| {
                        context.blocks_dependent_work()
                            && context
                                .affected_task_keys
                                .iter()
                                .any(|key| key == &definition.task_key || key == "*")
                    })
                    .cloned()
            })
            .flatten();
            let query_remediation_ready =
                !approved_query_treatments_for_task(&transaction, &definition.task_key)?.is_empty()
                    || task_has_current_query_artifact_invalidation(
                        &transaction,
                        production_task_id,
                    )?;
            let change_rework_ready =
                TenderStore::task_has_active_change_rework(&transaction, production_task_id)?;
            let attempt_kind = match task_status.as_str() {
                "review_ready" => "review",
                "remediation_ready" => "remediation",
                "query_blocked" => "query_control",
                "failed" | "cancelled" | "indeterminate" => latest_attempt
                    .as_ref()
                    .map(|attempt| attempt.5.as_str())
                    .unwrap_or("author"),
                _ => "author",
            };
            let retry_of_run_id = matches!(
                task_status.as_str(),
                "failed" | "cancelled" | "indeterminate"
            )
            .then(|| latest_attempt.as_ref().map(|attempt| attempt.1.clone()))
            .flatten();
            let retry_eligible = match (task_status.as_str(), latest_attempt.as_ref()) {
                ("ready", None) => expected_retry_of_run_id.is_none() && current_task_id.is_none(),
                ("ready", Some((_, _, prior_status, _, _, prior_kind))) => {
                    expected_retry_of_run_id.is_none()
                        && matches!(prior_status.as_str(), "completed" | "failed")
                        && prior_kind == "query_control"
                }
                ("query_blocked", None) => {
                    expected_retry_of_run_id.is_none() && query_control_context.is_some()
                }
                ("query_blocked", Some((_, _, prior_status, _, _, _))) => {
                    expected_retry_of_run_id.is_none()
                        && prior_status == "completed"
                        && query_control_context.is_some()
                }
                ("review_ready", Some((_, _, prior_status, _, _, prior_kind))) => {
                    expected_retry_of_run_id.is_none()
                        && prior_status == "completed"
                        && matches!(prior_kind.as_str(), "author" | "remediation")
                }
                ("remediation_ready", Some((_, _, prior_status, _, _, prior_kind))) => {
                    expected_retry_of_run_id.is_none()
                        && (change_rework_ready
                            || (prior_status == "completed"
                            && (prior_kind == "review" || query_remediation_ready))
                            || (prior_status == "failed"
                                && prior_kind == "query_control"
                                && query_remediation_ready))
                }
                ("failed", Some((_, prior_run_id, prior_status, failure_json, _, _)))
                    if prior_status == "failed" => {
                    expected_retry_of_run_id.is_none_or(|expected| expected == prior_run_id)
                        && failure_json
                            .as_deref()
                            .and_then(|failure| serde_json::from_str::<crate::agent_runtime::ProviderFailure>(failure).ok())
                            .is_some_and(|failure| failure.retry_safe)
                }
                ("cancelled", Some((_, prior_run_id, prior_status, _, _, _)))
                    if prior_status == "interrupted" => {
                    expected_retry_of_run_id.is_none_or(|expected| expected == prior_run_id)
                }
                ("indeterminate", Some((_, prior_run_id, prior_status, _, disposition, _)))
                    if prior_status == "indeterminate" => {
                    expected_retry_of_run_id.is_none_or(|expected| expected == prior_run_id)
                        && disposition.as_deref() == Some("retry_task")
                        && !transaction
                            .query_row(
                                "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE retry_of_run_id = ?1)",
                                [prior_run_id],
                                |row| row.get::<_, bool>(0),
                            )
                            .map_err(sql_error)?
                }
                _ => false,
            };
            let attempt_number = latest_attempt
                .as_ref()
                .map_or(1, |attempt| attempt.0.saturating_add(1));
            if retry_eligible
                && attempt_number > MAX_PRODUCTION_TASK_ATTEMPTS
                && activation_status == "active"
            {
                let latest_run_id = latest_attempt
                    .as_ref()
                    .map(|attempt| attempt.1.as_str())
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                let created_at = sqlite_timestamp(&transaction)?;
                if transaction
                    .execute(
                        "UPDATE production_tasks
                         SET status = 'attempt_limit_reached', updated_at = ?2
                         WHERE production_task_id = ?1 AND status = ?3",
                        params![production_task_id, created_at, task_status],
                    )
                    .map_err(sql_error)?
                    != 1
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let tender_revision: u32 = transaction
                    .query_row(
                        "SELECT current_revision FROM tender WHERE singleton = 1",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                append_audit_event(
                    &transaction,
                    tender_id.as_str(),
                    "production_task_attempt_limit_reached",
                    tender_revision,
                    json!({
                        "attempt_limit": MAX_PRODUCTION_TASK_ATTEMPTS.to_string(),
                        "production_task_id": production_task_id,
                        "run_id": latest_run_id,
                        "status": "attempt_limit_reached",
                    }),
                    &created_at,
                )?;
                append_production_denial(
                    &transaction,
                    tender_id,
                    "run_production_task",
                    Some(production_task_id),
                    "task_attempt_limit",
                )?;
                transaction.commit().map_err(sql_error)?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            if !retry_eligible || activation_status != "active" {
                append_production_denial(
                    &transaction,
                    tender_id,
                    "run_production_task",
                    Some(production_task_id),
                    if activation_status != "active" {
                        "production_not_active"
                    } else {
                        "task_not_retryable"
                    },
                )?;
                transaction.commit().map_err(sql_error)?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            if attempt_kind != "query_control"
                && task_has_blocking_query(&transaction, &definition.task_key)?
            {
                append_production_denial(
                    &transaction,
                    tender_id,
                    "run_production_task",
                    Some(production_task_id),
                    "material_query_unresolved",
                )?;
                transaction.commit().map_err(sql_error)?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let (profile_id, profile_version) = if attempt_kind == "query_control" {
                let context = query_control_context
                    .as_ref()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                (
                    context.owner_profile_id.clone(),
                    context.owner_profile_version,
                )
            } else if attempt_kind == "review" {
                (
                    definition
                        .review_profile_id
                        .clone()
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    definition
                        .review_profile_version
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                )
            } else {
                (definition.profile_id.clone(), definition.profile_version)
            };
            let profile = load_profile(&transaction, (profile_id.clone(), profile_version))?;
            let profile_active: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_profile_heads
                       WHERE profile_id = ?1 AND current_version = ?2 AND status = 'active'
                     )",
                    params![profile_id, profile_version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if !profile_active {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let (running_count, same_profile_running): (u32, bool) = transaction
                .query_row(
                    "SELECT
                       (SELECT COUNT(*) FROM production_tasks WHERE status IN ('running', 'reviewing')),
                       EXISTS(
                         SELECT 1 FROM production_tasks AS active_tasks
                         JOIN tender_tasks ON tender_tasks.task_id = active_tasks.task_id
                          WHERE active_tasks.status IN ('running', 'reviewing')
                           AND tender_tasks.profile_id = ?1
                       )",
                    [&profile.profile_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(sql_error)?;
            if running_count >= 2
                || same_profile_running
                || subscription_capacity_exhausted
                || subscription_capacity_is_exhausted(&transaction)?
            {
                let reason = if running_count >= 2 {
                    "concurrency_limit"
                } else if same_profile_running {
                    "profile_already_running"
                } else {
                    "subscription_capacity_exhausted"
                };
                append_production_denial(
                    &transaction,
                    tender_id,
                    "run_production_task",
                    Some(production_task_id),
                    reason,
                )?;
                transaction.commit().map_err(sql_error)?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let mut exact_inputs = if attempt_kind == "review" {
                Vec::new()
            } else {
                definition.exact_inputs.clone()
            };
            let production_query_contexts = if attempt_kind == "query_control" {
                vec![query_control_context
                    .clone()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?]
            } else {
                all_production_query_contexts
            };
            let approved_query_treatments = production_query_contexts
                .iter()
                .filter_map(|context| context.approved_treatment.clone())
                .filter(|decision| {
                    attempt_kind == "query_control" || decision.treatment.permits_dependent_work()
                })
                .collect::<Vec<_>>();
            for context in &production_query_contexts {
                exact_inputs.push(AgentTaskInputReference {
                    kind: "tender_query_version".into(),
                    reference: context.query_id.clone(),
                    version: context.query_version,
                });
                if attempt_kind == "query_control" {
                    exact_inputs.extend(context.evidence.iter().cloned());
                }
            }
            for decision in &approved_query_treatments {
                exact_inputs.push(AgentTaskInputReference {
                    kind: "approved_query_treatment".into(),
                    reference: decision.decision_id.clone(),
                    version: decision.query_version,
                });
            }
            let (change_assessment_inputs, change_assessment) =
                TenderStore::active_change_assessment_inputs_for_task(
                    &transaction,
                    production_task_id,
                )?;
            exact_inputs.extend(change_assessment_inputs);
            let mut dependency_outputs = Vec::new();
            if matches!(attempt_kind, "author" | "remediation") {
                for dependency in &definition.dependencies {
                    let output: Option<(String, u32, String, String, String, String)> = transaction
                    .query_row(
                        "SELECT artifacts.artifact_id, artifacts.version, artifacts.payload_sha256,
                                artifacts.payload_json, artifacts.data_scopes_json,
                                artifacts.data_classifications_json
                         FROM production_tasks AS tasks
                         JOIN production_integration_readiness AS readiness
                           ON readiness.production_task_id = tasks.production_task_id
                         JOIN production_artifact_versions AS artifacts
                           ON artifacts.artifact_id = readiness.artifact_id
                          AND artifacts.version = readiness.artifact_version
                         WHERE tasks.activation_id = ?1 AND tasks.task_key = ?2
                           AND tasks.status = 'ready_for_integration'
                           AND NOT EXISTS(
                             SELECT 1 FROM tender_query_target_invalidations AS invalidations
                             JOIN tender_query_heads AS query_heads
                               ON query_heads.query_id = invalidations.query_id
                              AND query_heads.current_version = invalidations.query_version
                             WHERE invalidations.target_kind = 'approval'
                               AND invalidations.target_id = readiness.readiness_id
                           )
                           AND readiness.rowid = (
                             SELECT MAX(candidate.rowid)
                             FROM production_integration_readiness AS candidate
                             WHERE candidate.production_task_id = tasks.production_task_id
                               AND NOT EXISTS(
                                 SELECT 1 FROM tender_query_target_invalidations AS invalidations
                                 JOIN tender_query_heads AS query_heads
                                   ON query_heads.query_id = invalidations.query_id
                                  AND query_heads.current_version = invalidations.query_version
                                 WHERE invalidations.target_kind = 'approval'
                                   AND invalidations.target_id = candidate.readiness_id
                               )
                           )",
                        params![activation_id, dependency],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                    let (
                        artifact_id,
                        artifact_version,
                        payload_sha256,
                        payload_json,
                        scopes_json,
                        classifications_json,
                    ) = output
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                    let output_scopes: Vec<String> = parse_canonical_json(&scopes_json)?;
                    let output_classifications: Vec<DataClassification> =
                        parse_canonical_json(&classifications_json)?;
                    if output_scopes
                        .iter()
                        .any(|scope| !profile.permissions.data_scopes.contains(scope))
                        || output_classifications.iter().any(|classification| {
                            !profile
                                .permissions
                                .data_classifications
                                .contains(classification)
                        })
                    {
                        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                    }
                    exact_inputs.push(AgentTaskInputReference {
                        kind: "production_artifact_version".into(),
                        reference: artifact_id.clone(),
                        version: artifact_version,
                    });
                    dependency_outputs.push(json!({
                    "data_classifications": output_classifications,
                    "data_scopes": output_scopes,
                    "artifact_id": artifact_id,
                    "artifact_version": artifact_version,
                    "payload": serde_json::from_str::<serde_json::Value>(&payload_json)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    "payload_sha256": payload_sha256,
                    "task_key": dependency,
                }));
                }
            }
            let mut remediation_target = None;
            let remediation_findings = if attempt_kind == "remediation" {
                let latest_artifact: (String, u32, String) = transaction
                    .query_row(
                        "SELECT artifact_id, version, payload_sha256
                         FROM production_artifact_versions
                         WHERE production_task_id = ?1 ORDER BY version DESC LIMIT 1",
                        [production_task_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .map_err(sql_error)?;
                exact_inputs.push(AgentTaskInputReference {
                    kind: "production_artifact_version".into(),
                    reference: latest_artifact.0.clone(),
                    version: latest_artifact.1,
                });
                remediation_target = Some(json!({
                    "artifact_id": latest_artifact.0,
                    "artifact_version": latest_artifact.1,
                    "payload_sha256": latest_artifact.2,
                }));
                let mut statement = transaction
                    .prepare(
                        "SELECT findings.finding_id, findings.review_id, findings.severity,
                                findings.summary, findings.evidence_references_json
                         FROM production_review_findings AS findings
                         JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
                         LEFT JOIN production_finding_dispositions AS dispositions
                           ON dispositions.finding_id = findings.finding_id
                         WHERE reviews.production_task_id = ?1
                           AND findings.severity IN ('critical', 'major')
                           AND dispositions.finding_id IS NULL
                         ORDER BY reviews.rowid, findings.finding_sequence",
                    )
                    .map_err(sql_error)?;
                let findings = statement
                    .query_map([production_task_id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    })
                    .map_err(sql_error)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sql_error)?;
                if (findings.is_empty()
                    && approved_query_treatments.is_empty()
                    && production_query_contexts.is_empty())
                    || findings.len() > MAX_PRODUCTION_FINDINGS
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                for finding in &findings {
                    exact_inputs.push(AgentTaskInputReference {
                        kind: "production_review_finding".into(),
                        reference: finding.0.clone(),
                        version: latest_artifact.1,
                    });
                }
                findings
                    .into_iter()
                    .map(|finding| {
                        Ok(json!({
                            "evidence_references": parse_canonical_json::<Vec<String>>(&finding.4)?,
                            "finding_id": finding.0,
                            "review_id": finding.1,
                            "severity": finding.2,
                            "summary": finding.3,
                        }))
                    })
                    .collect::<Result<Vec<_>, TenderCommandError>>()?
            } else {
                Vec::new()
            };
            let review_candidate = if attempt_kind == "review" {
                let candidate: (String, u32, String, String, String, String, String) = transaction
                    .query_row(
                        "SELECT artifacts.artifact_id, artifacts.version, artifacts.author_run_id,
                                artifacts.payload_json, artifacts.payload_sha256,
                                artifacts.data_scopes_json, artifacts.data_classifications_json
                         FROM production_artifact_versions AS artifacts
                         WHERE artifacts.production_task_id = ?1
                         ORDER BY artifacts.version DESC LIMIT 1",
                        [production_task_id],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                                row.get(6)?,
                            ))
                        },
                    )
                    .map_err(sql_error)?;
                if definition
                    .permissions
                    .data_scopes
                    .iter()
                    .any(|scope| !profile.permissions.data_scopes.contains(scope))
                    || definition
                        .permissions
                        .data_classifications
                        .iter()
                        .any(|classification| {
                            !profile
                                .permissions
                                .data_classifications
                                .contains(classification)
                        })
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                exact_inputs.push(AgentTaskInputReference {
                    kind: "production_artifact_version".into(),
                    reference: candidate.0.clone(),
                    version: candidate.1,
                });
                let review_scope = production_review_scope(&definition)?;
                let review_criteria = production_review_criteria(&definition)?;
                Some(json!({
                    "artifact_id": candidate.0,
                    "artifact_version": candidate.1,
                    "author_run_id": candidate.2,
                    "data_classifications": parse_canonical_json::<Vec<DataClassification>>(&candidate.6)?,
                    "data_scopes": parse_canonical_json::<Vec<String>>(&candidate.5)?,
                    "payload": serde_json::from_str::<serde_json::Value>(&candidate.3)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    "payload_sha256": candidate.4,
                    "review_capability": production_review_capability(&definition, &profile)?,
                    "review_criteria": review_criteria,
                    "review_scope": review_scope,
                }))
            } else {
                None
            };
            exact_inputs.sort_by(|left, right| {
                (&left.kind, &left.reference, left.version).cmp(&(
                    &right.kind,
                    &right.reference,
                    right.version,
                ))
            });
            exact_inputs.dedup();
            let task = TenderTaskView {
                task_id: random_identifier(&transaction)?,
                profile_id: profile.profile_id.clone(),
                profile_version: profile.version,
                objective: if attempt_kind == "query_control" {
                    format!(
                        "Add attributable Evidence or propose a treatment for the exact blocked Tender Query affecting {}; do not decide or close it.",
                        definition.task_key
                    )
                } else if attempt_kind == "review" {
                    format!(
                        "Independently review the exact candidate for {} without editing or approving it.",
                        definition.task_key
                    )
                } else {
                    definition.objective.clone()
                },
                exact_inputs,
                output_contract_json: if attempt_kind == "query_control" {
                    production_query_control_output_contract()?
                } else if attempt_kind == "review" {
                    production_review_output_contract()?
                } else if attempt_kind == "remediation" {
                    production_remediation_output_contract()?
                } else {
                    definition.output_contract_json.clone()
                },
                review_policy: if attempt_kind == "query_control" {
                    "The specialist may add exact Evidence and propose treatments, but cannot approve a treatment or close the Query.".into()
                } else if attempt_kind == "review" {
                    "This separate review must report whether the exact candidate satisfies the approved review policy; it cannot edit or approve the work.".into()
                } else {
                    profile.review_policy.clone()
                },
                deadline: definition.deadline.clone(),
                permissions: profile.permissions.clone(),
                resource_budget: profile.resource_budget.clone(),
                repair_feedback: None,
            };
            let created_at = sqlite_timestamp(&transaction)?;
            let grant_expires_at: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
                    [format!(
                        "+{} seconds",
                        task.resource_budget.duration_seconds
                    )],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            insert_task(&transaction, &task, &created_at)?;
            let tender_name: String = transaction
                .query_row(
                    "SELECT revisions.name FROM tender
                     JOIN tender_revisions AS revisions
                       ON revisions.tender_id = tender.tender_id
                      AND revisions.revision = tender.current_revision
                     WHERE tender.singleton = 1 AND tender.tender_id = ?1",
                    [tender_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let coordination_contract = matches!(attempt_kind, "author" | "remediation")
                .then(|| production_coordination_contract(&transaction, &definition))
                .transpose()?;
            let payload = json!({
                "coordination_contract": coordination_contract,
                "change_assessment": change_assessment,
                "data_classification": task.permissions.data_classifications
                    .iter()
                    .max()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
                "data_scope": task.permissions.data_scopes.join("+"),
                "approved_query_treatments": approved_query_treatments,
                "tender_queries": production_query_contexts,
                "dependency_outputs": dependency_outputs,
                "remediation_findings": remediation_findings,
                "remediation_target": remediation_target,
                "review_candidate": review_candidate,
                "plan": {
                    "manifest_sha256": plan_manifest_sha256,
                    "plan_id": plan_id,
                    "version": plan_version,
                },
                "production_task": definition,
                "query_control": attempt_kind == "query_control",
                "schema_version": 1,
                "tender": { "name": tender_name },
            });
            let (permission_grant, materialized_workspace) =
                derive_planned_task_grant(PlannedTaskGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home,
                    tender_id: tender_id.as_str(),
                    work_plan_version: plan_version,
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                    expires_at: &grant_expires_at,
                    payload: &payload,
                })?;
            let remaining = permission_duration(&permission_grant, Timestamp::now())
                .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            if materialized_workspace != workspace || remaining.is_zero() {
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
                Some((thread_ref, status)) if status == "active" => {
                    if load_thread_exposure(&transaction, &thread_ref)?
                        .is_compatible_with(&permission_grant)
                    {
                        (Some(thread_ref), None)
                    } else {
                        transaction
                            .execute(
                                "UPDATE provider_threads SET status = 'archive_pending'
                                 WHERE thread_ref = ?1 AND status = 'active'",
                                [&thread_ref],
                            )
                            .map_err(sql_error)?;
                        (None, Some(thread_ref))
                    }
                }
                Some((thread_ref, status)) if status == "archive_pending" => {
                    (None, Some(thread_ref))
                }
                Some(_) => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
                None => (None, None),
            };
            ensure_agent_run_capacity(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO agent_runs (
                       run_id, task_id, profile_id, profile_version, retry_of_run_id,
                       permission_grant_json, status, started_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7)",
                    params![
                        run_id,
                        task.task_id,
                        profile.profile_id,
                        profile.version,
                        retry_of_run_id,
                        canonical_json(&permission_grant)?,
                        created_at,
                    ],
                )
                .map_err(sql_error)?;
            super::record_agent_run_provider_binding(
                &transaction,
                &run_id,
                provider_selection,
                &created_at,
            )?;
            insert_event(
                &transaction,
                &run_id,
                1,
                PendingProviderEvent {
                    kind: ProviderEventKind::RunStarted,
                    summary: "Production Agent Run started".into(),
                    correlation_id: None,
                    request_fingerprint: None,
                    denial_reason: None,
                    opaque_reference: None,
                },
                &created_at,
            )?;
            transaction
                .execute(
                    "INSERT INTO production_task_attempts (
                       production_task_id, attempt_number, attempt_kind, task_id, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        production_task_id,
                        attempt_number,
                        attempt_kind,
                        task.task_id,
                        created_at
                    ],
                )
                .map_err(sql_error)?;
            if transaction
                .execute(
                    "UPDATE production_tasks SET task_id = ?2, status = ?6, updated_at = ?3
                     WHERE production_task_id = ?1 AND status = ?4
                       AND task_id IS ?5",
                    params![
                        production_task_id,
                        task.task_id,
                        created_at,
                        task_status,
                        current_task_id,
                        if attempt_kind == "review" {
                            "reviewing"
                        } else {
                            "running"
                        }
                    ],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let tender_revision: u32 = transaction
                .query_row(
                    "SELECT current_revision FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "production_task_started",
                tender_revision,
                json!({
                    "activation_id": activation_id,
                    "plan_id": plan_id,
                    "plan_version": plan_version.to_string(),
                    "production_task_id": production_task_id,
                    "attempt_number": attempt_number.to_string(),
                    "attempt_kind": attempt_kind,
                    "scheduled_by": "tender_office_coordinator",
                    "retry_of_run_id": retry_of_run_id,
                    "run_id": run_id,
                    "task_id": task.task_id,
                    "task_key": definition.task_key,
                }),
                &created_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            Ok(PreparedAgentRun {
                run_id,
                provider_selection: provider_selection.clone(),
                profile,
                task,
                permission_grant,
                provider_thread_ref,
                provider_thread_to_archive,
                workspace: workspace.clone(),
            })
        })();
        if prepared.is_err() {
            let _ = std::fs::remove_dir_all(&workspace);
        }
        prepared
    }

    fn production_recovery_context(
        transaction: &Transaction<'_>,
        plan_id: &str,
        plan_version: u32,
    ) -> Result<Option<ProductionRecoveryContext>, TenderCommandError> {
        let revision_actions_json: String = transaction
            .query_row(
                "SELECT revision_actions_json FROM work_plan_versions
                 WHERE plan_id = ?1 AND version = ?2",
                params![plan_id, plan_version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let revision_actions: Vec<WorkPlanRevisionAction> =
            parse_canonical_json(&revision_actions_json)?;
        if !matches!(
            revision_actions.as_slice(),
            [WorkPlanRevisionAction::RebasePackageBasis]
        ) {
            return Ok(None);
        }
        let mut statement = transaction
            .prepare(
                "SELECT assessments.assessment_id, activations.activation_id,
                        activations.plan_manifest_sha256
                 FROM change_assessments AS assessments
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 JOIN change_assessment_impacts AS impacts USING (assessment_id)
                 JOIN production_activations AS activations
                   ON activations.plan_id = impacts.object_id
                  AND activations.plan_version = impacts.object_version
                 WHERE decisions.classification = 'material'
                   AND resolutions.assessment_id IS NULL
                   AND impacts.kind = 'work_plan'
                   AND activations.status = 'suspended'
                   AND EXISTS(
                     SELECT 1
                     FROM work_plan_versions AS target_plan
                     JOIN bid_decision_approval_records AS approvals
                       ON approvals.package_id = target_plan.bid_package_id
                      AND approvals.package_version = target_plan.bid_package_version
                      AND approvals.decision = 'accept'
                     JOIN bid_decision_approval_invalidations AS invalidations
                       ON invalidations.approval_id != approvals.approval_id
                     JOIN json_each(invalidations.affected_areas_json) AS area
                       ON area.value = 'change_assessment:' || assessments.assessment_id
                     WHERE target_plan.plan_id = ?1 AND target_plan.version = ?2
                   )
                 ORDER BY assessments.assessment_sequence DESC LIMIT 2",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![plan_id, plan_version], |row| {
                Ok(ProductionRecoveryContext {
                    assessment_id: row.get(0)?,
                    source_activation_id: row.get(1)?,
                    source_plan_manifest_sha256: row.get(2)?,
                })
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        match rows.as_slice() {
            [context] => Ok(Some(context.clone())),
            [] => Ok(None),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }

    fn carry_forward_compatibility_sha256(
        connection: &rusqlite::Connection,
        source: &WorkPlanTask,
        target: &WorkPlanTask,
    ) -> Result<Option<String>, TenderCommandError> {
        let exact_input_shape = |task: &WorkPlanTask| {
            task.exact_inputs.len() == 2
                && task
                    .exact_inputs
                    .iter()
                    .filter(|input| input.kind == "tender_revision")
                    .count()
                    == 1
                && task
                    .exact_inputs
                    .iter()
                    .filter(|input| input.kind == "bid_decision_package")
                    .count()
                    == 1
        };
        if !exact_input_shape(source) || !exact_input_shape(target) {
            return Ok(None);
        }
        let mut source_semantics = source.clone();
        let mut target_semantics = target.clone();
        source_semantics.exact_inputs.clear();
        target_semantics.exact_inputs.clear();
        if source_semantics != target_semantics {
            return Ok(None);
        }
        let source_contract = production_coordination_contract(connection, source)?;
        let target_contract = production_coordination_contract(connection, target)?;
        if source_contract != target_contract {
            return Ok(None);
        }
        Ok(Some(sha256_hex(
            canonical_json(&json!({
                "task_semantics": source_semantics,
                "coordination_contract": source_contract,
            }))?
            .as_bytes(),
        )))
    }

    fn carry_forward_unaffected_production_task(
        transaction: &Transaction<'_>,
        tender_id: &TenderId,
        recovery: &ProductionRecoveryContext,
        target: ProductionCarryForwardTarget<'_>,
    ) -> Result<bool, TenderCommandError> {
        if task_has_blocking_query(transaction, &target.definition.task_key)? {
            return Ok(false);
        }
        let source: Option<(String, String, String, String)> = transaction
            .query_row(
                "SELECT tasks.production_task_id, tasks.task_definition_json,
                        tasks.task_definition_sha256, tasks.status
                 FROM production_tasks AS tasks
                 WHERE tasks.activation_id = ?1 AND tasks.task_key = ?2",
                params![recovery.source_activation_id, target.definition.task_key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((
            source_production_task_id,
            source_definition_json,
            source_definition_sha256,
            source_status,
        )) = source
        else {
            return Ok(false);
        };
        if source_status != ProductionTaskState::ReadyForIntegration.as_str()
            || transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM change_assessment_impacts
                       WHERE assessment_id = ?1 AND (
                         (kind = 'production_task' AND object_id = ?2)
                         OR (kind = 'production_artifact' AND object_id IN (
                           SELECT artifact_id FROM production_artifact_versions
                           WHERE production_task_id = ?2
                         ))
                         OR (kind = 'review' AND object_id IN (
                           SELECT review_id FROM production_reviews
                           WHERE production_task_id = ?2
                         ))
                       )
                     )",
                    params![recovery.assessment_id, source_production_task_id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
        {
            return Ok(false);
        }
        let source_definition: WorkPlanTask = parse_canonical_json(&source_definition_json)?;
        let Some(compatibility_sha256) = Self::carry_forward_compatibility_sha256(
            transaction,
            &source_definition,
            target.definition,
        )?
        else {
            return Ok(false);
        };
        let Some(source_readiness) =
            load_production_readiness(transaction, &source_production_task_id)?
        else {
            return Ok(false);
        };
        let source_record_task_id =
            production_record_source_task_id(transaction, &source_production_task_id)?;
        let source_exact: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM production_artifact_versions AS artifacts
                   WHERE artifacts.artifact_id = ?1 AND artifacts.version = ?2
                     AND artifacts.production_task_id = ?3
                     AND artifacts.payload_sha256 = ?4
                 ) AND (
                   (?5 IS NULL AND EXISTS(
                     SELECT 1 FROM work_plan_versions AS plans
                     JOIN json_each(plans.tasks_json) AS task
                       ON json_extract(task.value, '$.task_key') = ?6
                      AND json_extract(task.value, '$.review_profile_id') IS NULL
                     WHERE plans.manifest_sha256 = ?7
                   )) OR EXISTS(
                     SELECT 1 FROM production_reviews AS reviews
                     WHERE reviews.review_id = ?5 AND reviews.production_task_id = ?3
                       AND reviews.target_artifact_id = ?1 AND reviews.target_version = ?2
                       AND reviews.target_payload_sha256 = ?4 AND reviews.result = 'satisfied'
                   )
                 )",
                params![
                    source_readiness.artifact_id,
                    source_readiness.artifact_version,
                    source_record_task_id,
                    source_readiness.payload_sha256,
                    source_readiness.review_id,
                    target.definition.task_key,
                    recovery.source_plan_manifest_sha256,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !source_exact
            || !source_readiness.output_validation_passed
            || !source_readiness.evidence_verified
            || !source_readiness.dependencies_satisfied
        {
            return Ok(false);
        }
        let carry_forward_id = random_identifier(transaction)?;
        let target_readiness_id = random_identifier(transaction)?;
        let manifest = ProductionTaskCarryForwardManifest {
            schema_version: 1,
            carry_forward_id: carry_forward_id.clone(),
            assessment_id: recovery.assessment_id.clone(),
            source_production_task_id: source_production_task_id.clone(),
            source_task_definition_sha256: source_definition_sha256.clone(),
            source_readiness_id: source_readiness.readiness_id.clone(),
            source_artifact_id: source_readiness.artifact_id.clone(),
            source_artifact_version: source_readiness.artifact_version,
            source_payload_sha256: source_readiness.payload_sha256.clone(),
            source_review_id: source_readiness.review_id.clone(),
            source_finding_dispositions_sha256: source_readiness
                .finding_dispositions_sha256
                .clone(),
            source_plan_manifest_sha256: recovery.source_plan_manifest_sha256.clone(),
            target_production_task_id: target.production_task_id.to_owned(),
            target_task_definition_sha256: target.definition_sha256.to_owned(),
            target_readiness_id: target_readiness_id.clone(),
            target_plan_manifest_sha256: target.plan_manifest_sha256.to_owned(),
            compatibility_sha256: compatibility_sha256.clone(),
            carried_forward_by: "host_policy".into(),
            acting_role: "integration_gate".into(),
            created_at: target.created_at.to_owned(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            transaction,
            tender_id.as_str(),
            "production_task_carried_forward",
            target.tender_revision,
            json!({
                "assessment_id": recovery.assessment_id,
                "carry_forward_id": carry_forward_id,
                "manifest_sha256": manifest_sha256,
                "source_production_task_id": source_production_task_id,
                "target_production_task_id": target.production_task_id,
            }),
            target.created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO production_integration_readiness (
                   readiness_id, production_task_id, artifact_id, artifact_version,
                   payload_sha256, review_id, output_validation_passed, evidence_verified,
                   dependencies_satisfied, approval_gates_json,
                   finding_dispositions_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 1, 1, ?7, ?8, ?9)",
                params![
                    target_readiness_id,
                    target.production_task_id,
                    source_readiness.artifact_id,
                    source_readiness.artifact_version,
                    source_readiness.payload_sha256,
                    source_readiness.review_id,
                    canonical_json(&source_readiness.approval_gates)?,
                    source_readiness.finding_dispositions_sha256,
                    target.created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO production_task_carry_forwards (
                   carry_forward_id, assessment_id, source_production_task_id,
                   target_production_task_id, source_readiness_id, target_readiness_id,
                   source_artifact_id, source_artifact_version, source_review_id,
                   source_plan_manifest_sha256, target_plan_manifest_sha256,
                   compatibility_sha256, manifest_json, manifest_sha256,
                   audit_sequence, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                           ?12, ?13, ?14, ?15, ?16)",
                params![
                    carry_forward_id,
                    recovery.assessment_id,
                    source_production_task_id,
                    target.production_task_id,
                    source_readiness.readiness_id,
                    target_readiness_id,
                    source_readiness.artifact_id,
                    source_readiness.artifact_version,
                    source_readiness.review_id,
                    recovery.source_plan_manifest_sha256,
                    target.plan_manifest_sha256,
                    compatibility_sha256,
                    manifest_json,
                    manifest_sha256,
                    audit_sequence,
                    target.created_at,
                ],
            )
            .map_err(sql_error)?;
        Ok(true)
    }

    fn activate_tender_production(
        &mut self,
        tender_id: &TenderId,
        command: &ActivateTenderProductionCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<TenderProductionInspection, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let plan =
            self.inspect_work_plan_version(&command.plan_id, command.plan_version, budget)?;
        let package_dependencies_current = work_plan_package_dependencies_are_current(
            self,
            &command.plan_id,
            command.plan_version,
        )?;
        let denial_reason = if !package_dependencies_current {
            Some("work_plan_dependencies_stale")
        } else if !plan.current {
            Some("work_plan_not_current")
        } else if plan.manifest_sha256 != command.plan_manifest_sha256 {
            Some("plan_manifest_mismatch")
        } else if plan.approval.as_ref().map(|approval| approval.decision)
            != Some(WorkPlanDecision::Approve)
        {
            Some("plan_not_approved")
        } else if plan.tasks.is_empty() || plan.tasks.len() > MAX_PRODUCTION_TASKS {
            Some("plan_task_boundary")
        } else {
            None
        };
        if let Some(reason) = denial_reason {
            self.record_production_denial(tender_id, "activate_tender_production", None, reason)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let already_active: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM production_activations WHERE status = 'active')",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let unresolved_indeterminate: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM production_tasks AS tasks
                   JOIN production_task_attempts AS attempts
                     ON attempts.production_task_id = tasks.production_task_id
                    AND attempts.task_id = tasks.task_id
                   JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                   WHERE tasks.status = 'indeterminate'
                     AND NOT EXISTS (
                       SELECT 1 FROM agent_run_recovery_dispositions AS dispositions
                       WHERE dispositions.run_id = runs.run_id
                         AND dispositions.disposition = 'close_task'
                     )
                 )",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let exact_plan: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM work_plan_heads AS heads
                   JOIN tender ON tender.singleton = 1
                   JOIN work_plan_versions AS versions
                     ON versions.plan_id = heads.plan_id
                    AND versions.version = heads.current_version
                   JOIN work_plan_approvals AS approvals
                     ON approvals.plan_id = versions.plan_id
                    AND approvals.plan_version = versions.version
                    AND approvals.decision = 'approve'
                  WHERE heads.plan_id = ?1 AND heads.current_version = ?2
                    AND versions.manifest_sha256 = ?3
                    AND approvals.plan_manifest_sha256 = ?3
                    AND tender.tender_id = ?4
                    AND tender.lifecycle_phase = 'tender_planning'
                 )",
                params![
                    command.plan_id,
                    command.plan_version,
                    command.plan_manifest_sha256,
                    tender_id.as_str()
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        if already_active || unresolved_indeterminate || !exact_plan {
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "production_activation_denied",
                tender_revision,
                json!({
                    "plan_id": command.plan_id,
                    "plan_version": command.plan_version.to_string(),
                    "reason": if already_active {
                        "production_already_active"
                    } else if unresolved_indeterminate {
                        "indeterminate_production_task"
                    } else {
                        "plan_not_exact"
                    },
                }),
                &created_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let recovery = Self::production_recovery_context(
            &transaction,
            &command.plan_id,
            command.plan_version,
        )?;

        for task in &plan.tasks {
            budget.check()?;
            match production_coordination_contract(&transaction, task) {
                Ok(_) => {}
                Err(error) if error.code == TenderErrorCode::InvalidCommand => {
                    append_audit_event(
                        &transaction,
                        tender_id.as_str(),
                        "production_activation_denied",
                        tender_revision,
                        json!({
                            "plan_id": command.plan_id,
                            "plan_version": command.plan_version.to_string(),
                            "reason": "coordination_contract_boundary",
                            "task_key": task.task_key,
                        }),
                        &created_at,
                    )?;
                    transaction.commit().map_err(sql_error)?;
                    return Err(error);
                }
                Err(error) => return Err(error),
            }
        }

        let activation_id = random_identifier(&transaction)?;
        transaction
            .execute(
                "INSERT INTO production_activations (
                   activation_id, plan_id, plan_version, plan_manifest_sha256, status,
                   activated_by, acting_role, created_at
                 ) VALUES (?1, ?2, ?3, ?4, 'active', 'engineer_user',
                           'tendering_manager', ?5)",
                params![
                    activation_id,
                    command.plan_id,
                    command.plan_version,
                    command.plan_manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;

        transaction
            .execute(
                "UPDATE agent_profile_heads SET status = 'retired'
                 WHERE status IN ('active', 'proposed')",
                [],
            )
            .map_err(sql_error)?;
        for binding in &plan.profiles {
            budget.check()?;
            if transaction
                .execute(
                    "UPDATE agent_profile_heads SET current_version = ?2, status = 'active'
                     WHERE profile_id = ?1 AND current_version = ?2",
                    params![binding.profile.profile_id, binding.profile.version],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }

        let mut carried_forward_task_count = 0usize;
        for task in &plan.tasks {
            budget.check()?;
            let task_definition_json = canonical_json(task)?;
            let task_definition_sha256 = sha256_hex(task_definition_json.as_bytes());
            let initial_state = if task_has_blocking_query(&transaction, &task.task_key)? {
                ProductionTaskState::QueryBlocked
            } else if task.dependencies.is_empty() {
                ProductionTaskState::Ready
            } else {
                ProductionTaskState::Blocked
            };
            let production_task_id = random_identifier(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO production_tasks (
                       production_task_id, activation_id, task_key, task_definition_json,
                       task_definition_sha256, task_id, status, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?7)",
                    params![
                        production_task_id,
                        activation_id,
                        task.task_key,
                        task_definition_json,
                        task_definition_sha256,
                        initial_state.as_str(),
                        created_at,
                    ],
                )
                .map_err(sql_error)?;
            if let Some(recovery) = recovery.as_ref() {
                if Self::carry_forward_unaffected_production_task(
                    &transaction,
                    tender_id,
                    recovery,
                    ProductionCarryForwardTarget {
                        production_task_id: &production_task_id,
                        definition: task,
                        definition_sha256: &task_definition_sha256,
                        plan_manifest_sha256: &command.plan_manifest_sha256,
                        tender_revision,
                        created_at: &created_at,
                    },
                )? {
                    if transaction
                        .execute(
                            "UPDATE production_tasks
                             SET status = 'ready_for_integration', updated_at = ?2
                             WHERE production_task_id = ?1
                               AND status IN ('blocked', 'ready')",
                            params![production_task_id, created_at],
                        )
                        .map_err(sql_error)?
                        != 1
                    {
                        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                    }
                    carried_forward_task_count = carried_forward_task_count
                        .checked_add(1)
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                }
            }
        }
        for task in &plan.tasks {
            budget.check()?;
            if task_has_blocking_query(&transaction, &task.task_key)? {
                transaction
                    .execute(
                        "UPDATE production_tasks SET status = 'query_blocked', updated_at = ?3
                         WHERE activation_id = ?1 AND task_key = ?2
                           AND status IN ('blocked', 'ready')",
                        params![activation_id, task.task_key, created_at],
                    )
                    .map_err(sql_error)?;
            }
        }
        refresh_ready_frontier(&transaction, &activation_id, &created_at)?;
        if transaction
            .execute(
                "UPDATE tender SET lifecycle_phase = 'active_production'
                 WHERE singleton = 1 AND tender_id = ?1
                   AND lifecycle_phase = 'tender_planning'",
                [tender_id.as_str()],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "tender_production_activated",
            tender_revision,
            json!({
                "activated_by": "engineer_user",
                "acting_role": "tendering_manager",
                "activation_id": activation_id,
                "assessment_id": recovery.as_ref().map(|value| value.assessment_id.as_str()),
                "carried_forward_task_count": carried_forward_task_count.to_string(),
                "lifecycle_after": "active_production",
                "lifecycle_before": "tender_planning",
                "plan_id": command.plan_id,
                "plan_manifest_sha256": command.plan_manifest_sha256,
                "plan_version": command.plan_version.to_string(),
                "task_count": plan.tasks.len().to_string(),
            }),
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.inspect_tender_production(budget)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
    }

    pub(crate) fn inspect_tender_production(
        &self,
        budget: BidPackageOperationBudget,
    ) -> Result<Option<TenderProductionInspection>, TenderCommandError> {
        budget.check()?;
        let activation: Option<StoredProductionActivation> = self
            .connection
            .query_row(
                "SELECT activation_id, plan_id, plan_version, plan_manifest_sha256,
                        status, activated_by, acting_role, created_at
                 FROM production_activations ORDER BY rowid DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?;
        let Some(activation) = activation else {
            return Ok(None);
        };
        let mut statement = self
            .connection
            .prepare(
                "SELECT production_task_id, task_definition_json, status, created_at, updated_at
                 FROM production_tasks WHERE activation_id = ?1 ORDER BY rowid",
            )
            .map_err(sql_error)?;
        let mut rows = statement.query([&activation.0]).map_err(sql_error)?;
        let mut tasks = Vec::new();
        while let Some(row) = rows.next().map_err(sql_error)? {
            budget.check()?;
            if tasks.len() >= MAX_PRODUCTION_TASKS {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let production_task_id: String = row.get(0).map_err(sql_error)?;
            let record_source_task_id =
                production_record_source_task_id(&self.connection, &production_task_id)?;
            let task_json: String = row.get(1).map_err(sql_error)?;
            let mut run_statement = self
                .connection
                .prepare(
                    "SELECT agent_runs.run_id
                     FROM production_task_attempts AS attempts
                     JOIN agent_runs ON agent_runs.task_id = attempts.task_id
                     WHERE attempts.production_task_id = ?1
                     ORDER BY attempts.attempt_number",
                )
                .map_err(sql_error)?;
            let run_ids = run_statement
                .query_map([&production_task_id], |run| run.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            let (artifact_version_count, review_count, finding_count, open_blocking_finding_count):
                (u32, u32, u32, u32) = self
                .connection
                .query_row(
                    "SELECT
                       (SELECT COUNT(*) FROM production_artifact_versions
                        WHERE production_task_id = ?1),
                       (SELECT COUNT(*) FROM production_reviews
                        WHERE production_task_id = ?1),
                       (SELECT COUNT(*) FROM production_review_findings AS findings
                        JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
                        WHERE reviews.production_task_id = ?1),
                       (SELECT COUNT(*) FROM production_review_findings AS findings
                        JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
                        LEFT JOIN production_finding_dispositions AS dispositions
                          ON dispositions.finding_id = findings.finding_id
                        WHERE reviews.production_task_id = ?1
                          AND findings.severity IN ('critical', 'major')
                          AND dispositions.finding_id IS NULL)",
                    [&record_source_task_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(sql_error)?;
            let latest_artifact =
                load_latest_artifact_summary(&self.connection, &record_source_task_id)?;
            let latest_review_result = self
                .connection
                .query_row(
                    "SELECT result FROM production_reviews
                     WHERE production_task_id = ?1 ORDER BY rowid DESC LIMIT 1",
                    [&record_source_task_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(sql_error)?
                .map(|value| ProductionReviewResult::parse(&value))
                .transpose()?;
            let state = ProductionTaskState::parse(&row.get::<_, String>(2).map_err(sql_error)?)?;
            let task: WorkPlanTask = parse_canonical_json(&task_json)?;
            let query_control_available = state == ProductionTaskState::QueryBlocked
                && production_query_contexts_for_task(&self.connection, &task.task_key)?
                    .iter()
                    .any(|context| {
                        context.blocks_dependent_work()
                            && context
                                .affected_task_keys
                                .iter()
                                .any(|key| key == &task.task_key || key == "*")
                    });
            tasks.push(ProductionTaskInspection {
                production_task_id,
                plan_manifest_sha256: activation.3.clone(),
                task,
                state,
                run_ids,
                artifact_version_count,
                review_count,
                finding_count,
                open_blocking_finding_count,
                latest_artifact,
                latest_review_result,
                query_control_available,
                ready_for_integration: state == ProductionTaskState::ReadyForIntegration,
                created_at: row.get(3).map_err(sql_error)?,
                updated_at: row.get(4).map_err(sql_error)?,
            });
        }
        Ok(Some(TenderProductionInspection {
            activation_id: activation.0,
            plan_id: activation.1,
            plan_version: activation.2,
            plan_manifest_sha256: activation.3,
            active: activation.4 == "active",
            tasks,
            activated_by: activation.5,
            acting_role: activation.6,
            created_at: activation.7,
        }))
    }

    pub(crate) fn inspect_production_task_review(
        &self,
        production_task_id: &str,
        budget: BidPackageOperationBudget,
    ) -> Result<ProductionTaskReviewInspection, TenderCommandError> {
        budget.check()?;
        let exists: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM production_tasks WHERE production_task_id = ?1)",
                [production_task_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let record_source_task_id =
            production_record_source_task_id(&self.connection, production_task_id)?;
        let artifacts = load_production_artifact_versions(
            &self.connection,
            &record_source_task_id,
            &mut || budget.check(),
        )?;
        let reviews =
            load_production_reviews(&self.connection, &record_source_task_id, &mut || {
                budget.check()
            })?;
        let readiness = load_production_readiness(&self.connection, production_task_id)?;
        Ok(ProductionTaskReviewInspection {
            production_task_id: production_task_id.to_owned(),
            artifact_versions: artifacts,
            reviews,
            readiness,
        })
    }

    pub(crate) fn approve_production_finding_exception(
        &mut self,
        tender_id: &TenderId,
        command: &ApproveProductionFindingExceptionCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ProductionTaskReviewInspection, TenderCommandError> {
        struct ExceptionApprovalTarget {
            severity: String,
            artifact_id: String,
            artifact_version: u32,
            payload_sha256: String,
            review_result: String,
            task_state: String,
            criteria_json: String,
            task_definition_json: String,
        }

        self.require_storage_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let target: Option<ExceptionApprovalTarget> = transaction
            .query_row(
                "SELECT findings.severity, reviews.target_artifact_id, reviews.target_version,
                        reviews.target_payload_sha256, reviews.result, tasks.status,
                        reviews.criteria_json, tasks.task_definition_json
                 FROM production_review_findings AS findings
                 JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
                 JOIN production_tasks AS tasks
                   ON tasks.production_task_id = reviews.production_task_id
                 WHERE findings.finding_id = ?1 AND findings.review_id = ?2
                   AND reviews.production_task_id = ?3",
                params![
                    command.finding_id,
                    command.review_id,
                    command.production_task_id
                ],
                |row| {
                    Ok(ExceptionApprovalTarget {
                        severity: row.get(0)?,
                        artifact_id: row.get(1)?,
                        artifact_version: row.get(2)?,
                        payload_sha256: row.get(3)?,
                        review_result: row.get(4)?,
                        task_state: row.get(5)?,
                        criteria_json: row.get(6)?,
                        task_definition_json: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(sql_error)?;
        let valid = target.as_ref().is_some_and(|target| {
            let criteria = parse_canonical_json::<Vec<String>>(&target.criteria_json).ok();
            target.severity == "major"
                && target.artifact_id == command.artifact_id
                && target.artifact_version == command.artifact_version
                && target.payload_sha256 == command.payload_sha256
                && target.review_result == "requires_remediation"
                && target.task_state == "remediation_ready"
                && criteria.is_some_and(|criteria| {
                    criteria
                        .iter()
                        .any(|criterion| criterion == "major_exception_requires_engineer_approval")
                })
                && parse_canonical_json::<WorkPlanTask>(&target.task_definition_json)
                    .ok()
                    .is_some_and(|definition| {
                        definition.major_finding_policy
                            == MajorFindingPolicy::EngineerExceptionAllowed
                    })
        }) && !transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM production_finding_dispositions WHERE finding_id = ?1
                 )",
                [&command.finding_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
        if !valid {
            append_production_denial(
                &transaction,
                tender_id,
                "approve_production_finding_exception",
                Some(&command.production_task_id),
                if target
                    .as_ref()
                    .is_some_and(|target| target.severity == "critical")
                {
                    "critical_finding_nonwaivable"
                } else {
                    "finding_exception_guard_failed"
                },
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let blocking_open: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM production_review_findings AS findings
                   JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
                   LEFT JOIN production_finding_dispositions AS dispositions
                     ON dispositions.finding_id = findings.finding_id
                    WHERE reviews.production_task_id = ?1
                      AND findings.severity IN ('critical', 'major')
                      AND dispositions.finding_id IS NULL
                      AND findings.finding_id != ?2
                  )",
                params![command.production_task_id, command.finding_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let disposition_id = random_identifier(&transaction)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "production_finding_exception_approved",
            tender_revision,
            json!({
                "acting_role": "tendering_manager",
                "artifact_id": command.artifact_id,
                "artifact_version": command.artifact_version.to_string(),
                "consequence": command.consequence.trim(),
                "decided_by": "engineer_user",
                "disposition": "exception_approved",
                "disposition_id": disposition_id.clone(),
                "finding_id": command.finding_id,
                "production_task_id": command.production_task_id,
                "rationale": command.rationale.trim(),
                "review_id": command.review_id,
                "task_ready_for_integration": !blocking_open,
                "verifying_review_id": serde_json::Value::Null,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO production_finding_dispositions (
                   disposition_id, finding_id, audit_sequence, disposition,
                   target_artifact_id, target_version, verifying_review_id,
                   decided_by, acting_role, rationale, consequence, created_at
                 ) VALUES (?1, ?2, ?3, 'exception_approved', ?4, ?5, NULL,
                           'engineer_user', 'tendering_manager', ?6, ?7, ?8)",
                params![
                    disposition_id,
                    command.finding_id,
                    audit_sequence,
                    command.artifact_id,
                    command.artifact_version,
                    command.rationale.trim(),
                    command.consequence.trim(),
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        if !blocking_open {
            let readiness_target: (String, u32, String, String) = transaction
                .query_row(
                    "SELECT artifacts.artifact_id, artifacts.version,
                            artifacts.payload_sha256, reviews.review_id
                     FROM production_artifact_versions AS artifacts
                     JOIN production_reviews AS reviews
                       ON reviews.production_task_id = artifacts.production_task_id
                      AND reviews.target_artifact_id = artifacts.artifact_id
                      AND reviews.target_version = artifacts.version
                      AND reviews.target_payload_sha256 = artifacts.payload_sha256
                     WHERE artifacts.production_task_id = ?1
                     ORDER BY artifacts.version DESC, reviews.rowid DESC LIMIT 1",
                    [&command.production_task_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(sql_error)?;
            insert_production_integration_readiness(
                &transaction,
                &command.production_task_id,
                &readiness_target.0,
                readiness_target.1,
                &readiness_target.2,
                Some(&readiness_target.3),
                &created_at,
            )?;
            if transaction
                .execute(
                    "UPDATE production_tasks
                     SET status = 'ready_for_integration', updated_at = ?2
                     WHERE production_task_id = ?1 AND status = 'remediation_ready'",
                    params![command.production_task_id, created_at],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let activation_id: String = transaction
                .query_row(
                    "SELECT activation_id FROM production_tasks WHERE production_task_id = ?1",
                    [&command.production_task_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            refresh_ready_frontier(&transaction, &activation_id, &created_at)?;
        }
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.inspect_production_task_review(&command.production_task_id, budget)
    }

    fn record_production_denial(
        &mut self,
        tender_id: &TenderId,
        command: &str,
        production_task_id: Option<&str>,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        append_production_denial(&transaction, tender_id, command, production_task_id, reason)?;
        transaction.commit().map_err(sql_error)
    }
}

fn load_latest_artifact_summary(
    connection: &rusqlite::Connection,
    production_task_id: &str,
) -> Result<Option<ProductionArtifactVersionSummary>, TenderCommandError> {
    connection
        .query_row(
            "SELECT artifact_id, version, author_run_id, prior_version,
                    remediation_review_id, payload_sha256, output_validation_passed,
                    evidence_verified, data_scopes_json, data_classifications_json, created_at
             FROM production_artifact_versions
             WHERE production_task_id = ?1 ORDER BY version DESC LIMIT 1",
            [production_task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<u32>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .map(|row| {
            Ok(ProductionArtifactVersionSummary {
                artifact_id: row.0,
                version: row.1,
                author_run_id: row.2,
                prior_version: row.3,
                remediation_review_id: row.4,
                payload_sha256: row.5,
                output_validation_passed: row.6,
                evidence_verified: row.7,
                data_scopes: parse_canonical_json(&row.8)?,
                data_classifications: parse_canonical_json(&row.9)?,
                created_at: row.10,
            })
        })
        .transpose()
}

fn load_production_artifact_versions(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<ProductionArtifactVersion>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT artifact_id, version, author_run_id, prior_version,
                    remediation_review_id, payload_json, payload_sha256,
                    output_validation_passed, evidence_verified, data_scopes_json,
                    data_classifications_json, created_at
             FROM production_artifact_versions
             WHERE production_task_id = ?1 ORDER BY version",
        )
        .map_err(sql_error)?;
    let mut rows = statement.query([production_task_id]).map_err(sql_error)?;
    let mut artifacts = Vec::new();
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        if artifacts.len() >= MAX_PRODUCTION_TASK_ATTEMPTS as usize {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let payload_json: String = row.get(5).map_err(sql_error)?;
        let payload = parse_canonical_json::<ProductionArtifactPayload>(&payload_json)?;
        artifacts.push(ProductionArtifactVersion {
            summary: ProductionArtifactVersionSummary {
                artifact_id: row.get(0).map_err(sql_error)?,
                version: row.get(1).map_err(sql_error)?,
                author_run_id: row.get(2).map_err(sql_error)?,
                prior_version: row.get(3).map_err(sql_error)?,
                remediation_review_id: row.get(4).map_err(sql_error)?,
                payload_sha256: row.get(6).map_err(sql_error)?,
                output_validation_passed: row.get(7).map_err(sql_error)?,
                evidence_verified: row.get(8).map_err(sql_error)?,
                data_scopes: parse_canonical_json(&row.get::<_, String>(9).map_err(sql_error)?)?,
                data_classifications: parse_canonical_json(
                    &row.get::<_, String>(10).map_err(sql_error)?,
                )?,
                created_at: row.get(11).map_err(sql_error)?,
            },
            payload,
        });
    }
    Ok(artifacts)
}

fn load_production_reviews(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<ProductionReview>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT review_id, target_artifact_id, target_version, target_payload_sha256,
                    reviewer_run_id, reviewer_profile_id, reviewer_profile_version,
                    capability, scope_json, criteria_json, inputs_json, result,
                    resolved_finding_ids_json, created_at
             FROM production_reviews WHERE production_task_id = ?1 ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let mut rows = statement.query([production_task_id]).map_err(sql_error)?;
    let mut reviews = Vec::new();
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        if reviews.len() >= MAX_PRODUCTION_TASK_ATTEMPTS as usize {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let review_id: String = row.get(0).map_err(sql_error)?;
        let mut finding_statement = connection
            .prepare(
                "SELECT findings.finding_id, findings.finding_sequence, findings.severity,
                         findings.summary, findings.evidence_references_json, findings.created_at,
                         dispositions.disposition_id, dispositions.disposition,
                         dispositions.target_artifact_id,
                         dispositions.target_version, dispositions.verifying_review_id,
                         dispositions.decided_by, dispositions.acting_role,
                         dispositions.rationale, dispositions.consequence,
                         dispositions.created_at
                 FROM production_review_findings AS findings
                 LEFT JOIN production_finding_dispositions AS dispositions
                   ON dispositions.finding_id = findings.finding_id
                 WHERE findings.review_id = ?1 ORDER BY findings.finding_sequence",
            )
            .map_err(sql_error)?;
        let mut finding_rows = finding_statement.query([&review_id]).map_err(sql_error)?;
        let mut findings = Vec::new();
        while let Some(finding) = finding_rows.next().map_err(sql_error)? {
            check()?;
            if findings.len() >= MAX_PRODUCTION_FINDINGS {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let disposition_id: Option<String> = finding.get(6).map_err(sql_error)?;
            let disposition = disposition_id
                .map(|disposition_id| {
                    Ok(ProductionFindingDisposition {
                        disposition_id,
                        kind: ProductionFindingDispositionKind::parse(
                            &finding.get::<_, String>(7).map_err(sql_error)?,
                        )?,
                        target_artifact_id: finding.get(8).map_err(sql_error)?,
                        target_version: finding.get(9).map_err(sql_error)?,
                        verifying_review_id: finding.get(10).map_err(sql_error)?,
                        decided_by: finding.get(11).map_err(sql_error)?,
                        acting_role: finding.get(12).map_err(sql_error)?,
                        rationale: finding.get(13).map_err(sql_error)?,
                        consequence: finding.get(14).map_err(sql_error)?,
                        created_at: finding.get(15).map_err(sql_error)?,
                    })
                })
                .transpose()?;
            findings.push(ProductionReviewFinding {
                finding_id: finding.get(0).map_err(sql_error)?,
                review_id: review_id.clone(),
                sequence: finding.get(1).map_err(sql_error)?,
                severity: ProductionFindingSeverity::parse(
                    &finding.get::<_, String>(2).map_err(sql_error)?,
                )?,
                summary: finding.get(3).map_err(sql_error)?,
                evidence_references: parse_canonical_json(
                    &finding.get::<_, String>(4).map_err(sql_error)?,
                )?,
                disposition,
                created_at: finding.get(5).map_err(sql_error)?,
            });
        }
        reviews.push(ProductionReview {
            review_id,
            target_artifact_id: row.get(1).map_err(sql_error)?,
            target_version: row.get(2).map_err(sql_error)?,
            target_payload_sha256: row.get(3).map_err(sql_error)?,
            reviewer_run_id: row.get(4).map_err(sql_error)?,
            reviewer_profile_id: row.get(5).map_err(sql_error)?,
            reviewer_profile_version: row.get(6).map_err(sql_error)?,
            capability: row.get(7).map_err(sql_error)?,
            scope: parse_canonical_json(&row.get::<_, String>(8).map_err(sql_error)?)?,
            criteria: parse_canonical_json(&row.get::<_, String>(9).map_err(sql_error)?)?,
            inputs: parse_canonical_json(&row.get::<_, String>(10).map_err(sql_error)?)?,
            result: ProductionReviewResult::parse(&row.get::<_, String>(11).map_err(sql_error)?)?,
            resolved_finding_ids: parse_canonical_json(
                &row.get::<_, String>(12).map_err(sql_error)?,
            )?,
            findings,
            created_at: row.get(13).map_err(sql_error)?,
        });
    }
    Ok(reviews)
}

fn production_record_source_task_id(
    connection: &rusqlite::Connection,
    production_task_id: &str,
) -> Result<String, TenderCommandError> {
    let mut current = production_task_id.to_owned();
    let mut visited = std::collections::HashSet::new();
    for _ in 0..128 {
        if !visited.insert(current.clone()) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let source = connection
            .query_row(
                "SELECT source_production_task_id FROM production_task_carry_forwards
                 WHERE target_production_task_id = ?1",
                [&current],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(source) = source else {
            return Ok(current);
        };
        current = source;
    }
    Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn production_task_carry_forward_is_valid(
    connection: &rusqlite::Connection,
    target_production_task_id: &str,
    target_definition: &WorkPlanTask,
    target_definition_sha256: &str,
    target_plan_manifest_sha256: &str,
) -> Result<Option<bool>, TenderCommandError> {
    type Row = (
        String,
        String,
        String,
        String,
        String,
        String,
        u32,
        Option<String>,
        String,
        String,
        String,
        String,
        i64,
        String,
    );
    let row: Option<Row> = connection
        .query_row(
            "SELECT carry_forward_id, assessment_id, source_production_task_id,
                    source_readiness_id, target_readiness_id, source_artifact_id,
                    source_artifact_version, source_review_id,
                    source_plan_manifest_sha256, target_plan_manifest_sha256,
                    compatibility_sha256, manifest_json, audit_sequence, created_at
             FROM production_task_carry_forwards
             WHERE target_production_task_id = ?1",
            [target_production_task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let manifest_sha256: String = connection
        .query_row(
            "SELECT manifest_sha256 FROM production_task_carry_forwards
             WHERE carry_forward_id = ?1",
            [&row.0],
            |stored| stored.get(0),
        )
        .map_err(sql_error)?;
    if sha256_hex(row.11.as_bytes()) != manifest_sha256 {
        return Ok(Some(false));
    }
    let manifest: ProductionTaskCarryForwardManifest = parse_canonical_json(&row.11)?;
    let source: Option<(String, String, String, String)> = connection
        .query_row(
            "SELECT tasks.task_definition_json, tasks.task_definition_sha256, tasks.status,
                    activations.plan_manifest_sha256
             FROM production_tasks AS tasks
             JOIN production_activations AS activations USING (activation_id)
             WHERE tasks.production_task_id = ?1
               AND activations.status IN ('suspended', 'superseded')",
            [&row.2],
            |source| {
                Ok((
                    source.get(0)?,
                    source.get(1)?,
                    source.get(2)?,
                    source.get(3)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some((source_definition_json, source_definition_sha256, source_status, source_plan_sha)) =
        source
    else {
        return Ok(Some(false));
    };
    let source_definition: WorkPlanTask = parse_canonical_json(&source_definition_json)?;
    let compatibility_sha256 = TenderStore::carry_forward_compatibility_sha256(
        connection,
        &source_definition,
        target_definition,
    )?;
    let source_readiness = connection
        .query_row(
            "SELECT production_task_id, artifact_id, artifact_version, payload_sha256,
                    review_id, output_validation_passed, evidence_verified,
                    dependencies_satisfied, approval_gates_json,
                    finding_dispositions_sha256, created_at
             FROM production_integration_readiness WHERE readiness_id = ?1",
            [&row.3],
            |ready| {
                Ok((
                    ready.get::<_, String>(0)?,
                    ready.get::<_, String>(1)?,
                    ready.get::<_, u32>(2)?,
                    ready.get::<_, String>(3)?,
                    ready.get::<_, Option<String>>(4)?,
                    ready.get::<_, bool>(5)?,
                    ready.get::<_, bool>(6)?,
                    ready.get::<_, bool>(7)?,
                    ready.get::<_, String>(8)?,
                    ready.get::<_, String>(9)?,
                    ready.get::<_, String>(10)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let target_readiness = connection
        .query_row(
            "SELECT production_task_id, artifact_id, artifact_version, payload_sha256,
                    review_id, output_validation_passed, evidence_verified,
                    dependencies_satisfied, approval_gates_json,
                    finding_dispositions_sha256, created_at
             FROM production_integration_readiness WHERE readiness_id = ?1",
            [&row.4],
            |ready| {
                Ok((
                    ready.get::<_, String>(0)?,
                    ready.get::<_, String>(1)?,
                    ready.get::<_, u32>(2)?,
                    ready.get::<_, String>(3)?,
                    ready.get::<_, Option<String>>(4)?,
                    ready.get::<_, bool>(5)?,
                    ready.get::<_, bool>(6)?,
                    ready.get::<_, bool>(7)?,
                    ready.get::<_, String>(8)?,
                    ready.get::<_, String>(9)?,
                    ready.get::<_, String>(10)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let (Some(source_readiness), Some(target_readiness), Some(compatibility_sha256)) =
        (source_readiness, target_readiness, compatibility_sha256)
    else {
        return Ok(Some(false));
    };
    let audit_exact: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM audit_events
               WHERE sequence = ?1 AND event_type = 'production_task_carried_forward'
                 AND json_extract(payload_json, '$.change.carry_forward_id') = ?2
                 AND json_extract(payload_json, '$.change.manifest_sha256') = ?3
                 AND json_extract(payload_json, '$.change.assessment_id') = ?4
                 AND json_extract(payload_json, '$.change.target_production_task_id') = ?5
             )",
            params![
                row.12,
                row.0,
                manifest_sha256,
                row.1,
                target_production_task_id
            ],
            |audit| audit.get(0),
        )
        .map_err(sql_error)?;
    let assessment_exact: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM change_assessments AS assessments
               JOIN change_assessment_decisions AS decisions USING (assessment_id)
               WHERE assessments.assessment_id = ?1
                 AND decisions.classification = 'material'
                 AND NOT EXISTS(
                   SELECT 1 FROM change_assessment_impacts AS impacts
                   WHERE impacts.assessment_id = assessments.assessment_id AND (
                     (impacts.kind = 'production_task' AND impacts.object_id = ?2)
                     OR (impacts.kind = 'production_artifact' AND impacts.object_id = ?3
                         AND impacts.object_version = ?4)
                     OR (impacts.kind = 'review' AND impacts.object_id = ?5)
                   )
                 )
             )",
            params![row.1, row.2, row.5, row.6, row.7],
            |assessment| assessment.get(0),
        )
        .map_err(sql_error)?;
    let target_record_count: u32 = connection
        .query_row(
            "SELECT (SELECT COUNT(*) FROM production_task_attempts
                     WHERE production_task_id = ?1)
                    + (SELECT COUNT(*) FROM production_artifact_versions
                       WHERE production_task_id = ?1)
                    + (SELECT COUNT(*) FROM production_reviews
                       WHERE production_task_id = ?1)",
            [target_production_task_id],
            |count| count.get(0),
        )
        .map_err(sql_error)?;
    let canonical_readiness_equal = source_readiness.1 == target_readiness.1
        && source_readiness.2 == target_readiness.2
        && source_readiness.3 == target_readiness.3
        && source_readiness.4 == target_readiness.4
        && source_readiness.5 == target_readiness.5
        && source_readiness.6 == target_readiness.6
        && source_readiness.7 == target_readiness.7
        && source_readiness.8 == target_readiness.8
        && source_readiness.9 == target_readiness.9;
    let manifest_exact = manifest.schema_version == 1
        && manifest.carry_forward_id == row.0
        && manifest.assessment_id == row.1
        && manifest.source_production_task_id == row.2
        && manifest.source_task_definition_sha256 == source_definition_sha256
        && manifest.source_readiness_id == row.3
        && manifest.source_artifact_id == row.5
        && manifest.source_artifact_version == row.6
        && manifest.source_payload_sha256 == source_readiness.3
        && manifest.source_review_id == row.7
        && manifest.source_finding_dispositions_sha256 == source_readiness.9
        && manifest.source_plan_manifest_sha256 == row.8
        && manifest.target_production_task_id == target_production_task_id
        && manifest.target_task_definition_sha256 == target_definition_sha256
        && manifest.target_readiness_id == row.4
        && manifest.target_plan_manifest_sha256 == row.9
        && manifest.compatibility_sha256 == row.10
        && manifest.carried_forward_by == "host_policy"
        && manifest.acting_role == "integration_gate"
        && manifest.created_at == row.13;
    Ok(Some(
        source_status == ProductionTaskState::ReadyForIntegration.as_str()
            && source_plan_sha == row.8
            && row.9 == target_plan_manifest_sha256
            && source_readiness.0 == row.2
            && target_readiness.0 == target_production_task_id
            && target_readiness.10 == row.13
            && source_readiness.1 == row.5
            && source_readiness.2 == row.6
            && source_readiness.4 == row.7
            && compatibility_sha256 == row.10
            && canonical_readiness_equal
            && target_record_count == 0
            && audit_exact
            && assessment_exact
            && manifest_exact,
    ))
}

fn load_production_readiness(
    connection: &rusqlite::Connection,
    production_task_id: &str,
) -> Result<Option<ProductionIntegrationReadiness>, TenderCommandError> {
    connection
        .query_row(
            "SELECT readiness_id, artifact_id, artifact_version, payload_sha256, review_id,
                    output_validation_passed, evidence_verified, dependencies_satisfied,
                    approval_gates_json, finding_dispositions_sha256, created_at
             FROM production_integration_readiness AS readiness
             WHERE production_task_id = ?1
               AND NOT EXISTS(
                 SELECT 1 FROM tender_query_target_invalidations AS invalidations
                 JOIN tender_query_heads AS query_heads
                   ON query_heads.query_id = invalidations.query_id
                  AND query_heads.current_version = invalidations.query_version
                 WHERE invalidations.target_kind = 'approval'
                   AND invalidations.target_id = readiness.readiness_id
               )
               AND NOT EXISTS(
                 SELECT 1
                 FROM change_assessment_impacts AS impacts
                 JOIN change_assessment_decisions AS decisions
                   ON decisions.assessment_id = impacts.assessment_id
                 WHERE impacts.kind = 'production_task'
                   AND impacts.object_id = readiness.production_task_id
                   AND decisions.classification = 'material'
               )
             ORDER BY readiness.rowid DESC LIMIT 1",
            [production_task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, bool>(5)?,
                    row.get::<_, bool>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .map(|row| {
            Ok(ProductionIntegrationReadiness {
                readiness_id: row.0,
                artifact_id: row.1,
                artifact_version: row.2,
                payload_sha256: row.3,
                review_id: row.4,
                output_validation_passed: row.5,
                evidence_verified: row.6,
                dependencies_satisfied: row.7,
                approval_gates: parse_canonical_json(&row.8)?,
                finding_dispositions_sha256: row.9,
                created_at: row.10,
            })
        })
        .transpose()
}

fn load_all_production_readiness(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<ProductionIntegrationReadiness>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT readiness_id, artifact_id, artifact_version, payload_sha256, review_id,
                    output_validation_passed, evidence_verified, dependencies_satisfied,
                    approval_gates_json, finding_dispositions_sha256, created_at
             FROM production_integration_readiness
             WHERE production_task_id = ?1 ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([production_task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, bool>(6)?,
                row.get::<_, bool>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(sql_error)?;
    let mut readiness = Vec::new();
    for row in rows {
        check()?;
        let row = row.map_err(sql_error)?;
        readiness.push(ProductionIntegrationReadiness {
            readiness_id: row.0,
            artifact_id: row.1,
            artifact_version: row.2,
            payload_sha256: row.3,
            review_id: row.4,
            output_validation_passed: row.5,
            evidence_verified: row.6,
            dependencies_satisfied: row.7,
            approval_gates: parse_canonical_json(&row.8)?,
            finding_dispositions_sha256: row.9,
            created_at: row.10,
        });
    }
    Ok(readiness)
}

fn production_task_records_are_valid(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    definition: &WorkPlanTask,
    state: ProductionTaskState,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let artifacts = load_production_artifact_versions(connection, production_task_id, check)?;
    let reviews = load_production_reviews(connection, production_task_id, check)?;
    let readiness = load_production_readiness(connection, production_task_id)?;
    let all_readiness = load_all_production_readiness(connection, production_task_id, check)?;
    if artifacts.iter().enumerate().any(|(index, artifact)| {
        artifact.summary.artifact_id != production_task_id
            || artifact.summary.version as usize != index + 1
            || artifact.summary.prior_version != (index > 0).then_some(index as u32)
            || !artifact.summary.output_validation_passed
            || !artifact.summary.evidence_verified
            || artifact.summary.data_scopes != definition.permissions.data_scopes
            || artifact.summary.data_classifications != definition.permissions.data_classifications
            || canonical_json(&artifact.payload).map_or(true, |payload| {
                sha256_hex(payload.as_bytes()) != artifact.summary.payload_sha256
            })
    }) {
        return Ok(false);
    }
    for (index, artifact) in artifacts.iter().enumerate() {
        check()?;
        let expected_kind = if index == 0 { "author" } else { "remediation" };
        let author_exact: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM production_task_attempts AS attempts
                   JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                   JOIN proposed_agent_results AS results ON results.run_id = runs.run_id
                   WHERE attempts.production_task_id = ?1
                     AND attempts.attempt_kind = ?2
                     AND runs.run_id = ?3 AND runs.status = 'completed'
                     AND runs.profile_id = ?4 AND runs.profile_version = ?5
                     AND results.payload_json = ?6
                 )",
                params![
                    production_task_id,
                    expected_kind,
                    artifact.summary.author_run_id,
                    definition.profile_id,
                    definition.profile_version,
                    canonical_json(&artifact.payload)?,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !author_exact {
            return Ok(false);
        }
        if index == 0 {
            if artifact.summary.remediation_review_id.is_some() {
                return Ok(false);
            }
        } else {
            let review_based = artifact
                .summary
                .remediation_review_id
                .as_deref()
                .is_some_and(|remediation_review_id| {
                    reviews.iter().any(|review| {
                        review.review_id == remediation_review_id
                            && review.target_artifact_id == artifact.summary.artifact_id
                            && review.target_version == artifact.summary.version - 1
                            && review.result == ProductionReviewResult::RequiresRemediation
                    })
                });
            let query_based: bool = connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs AS runs
                       JOIN tender_tasks AS tasks ON tasks.task_id = runs.task_id
                       JOIN json_each(tasks.exact_inputs_json) AS input
                       WHERE runs.run_id = ?1
                         AND json_extract(input.value, '$.kind') IN (
                           'approved_query_treatment', 'tender_query_version'
                         )
                     )",
                    [&artifact.summary.author_run_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if artifact.summary.remediation_review_id.is_some() {
                if !review_based {
                    return Ok(false);
                }
            } else if !query_based {
                return Ok(false);
            }
        }
    }
    let expected_scope = production_review_scope(definition)?;
    let expected_criteria = production_review_criteria(definition)?;
    for review in &reviews {
        check()?;
        let Some(target) = artifacts.iter().find(|artifact| {
            artifact.summary.artifact_id == review.target_artifact_id
                && artifact.summary.version == review.target_version
        }) else {
            return Ok(false);
        };
        let reviewer_profile = load_profile(
            connection,
            (
                review.reviewer_profile_id.clone(),
                review.reviewer_profile_version,
            ),
        )?;
        let run_task_id: Option<String> = connection
            .query_row(
                "SELECT task_id FROM agent_runs
                 WHERE run_id = ?1 AND status = 'completed'
                   AND profile_id = ?2 AND profile_version = ?3",
                params![
                    review.reviewer_run_id,
                    review.reviewer_profile_id,
                    review.reviewer_profile_version,
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(run_task_id) = run_task_id else {
            return Ok(false);
        };
        let run_task = load_task(connection, &run_task_id)?;
        let result_payload: String = connection
            .query_row(
                "SELECT payload_json FROM proposed_agent_results WHERE run_id = ?1",
                [&review.reviewer_run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let result_candidate: ProductionReviewCandidate = parse_canonical_json(&result_payload)?;
        let stored_candidates = review
            .findings
            .iter()
            .map(|finding| ProductionReviewFindingCandidate {
                severity: finding.severity,
                summary: finding.summary.clone(),
                evidence_references: finding.evidence_references.clone(),
            })
            .collect::<Vec<_>>();
        let resolved = review
            .resolved_finding_ids
            .iter()
            .collect::<std::collections::HashSet<_>>();
        if review.target_payload_sha256 != target.summary.payload_sha256
            || review.reviewer_profile_id == definition.profile_id
            || definition.review_profile_id.as_deref() != Some(review.reviewer_profile_id.as_str())
            || definition.review_profile_version != Some(review.reviewer_profile_version)
            || review.capability != production_review_capability(definition, &reviewer_profile)?
            || review.scope != expected_scope
            || review.criteria != expected_criteria
            || review.inputs != run_task.exact_inputs
            || !production_review_inputs_are_valid(
                connection,
                definition,
                review,
                &run_task.exact_inputs,
            )?
            || review.result != result_candidate.result
            || review.resolved_finding_ids != result_candidate.resolved_finding_ids
            || resolved.len() != review.resolved_finding_ids.len()
            || review.resolved_finding_ids.iter().any(|finding_id| {
                !reviews
                    .iter()
                    .flat_map(|candidate| &candidate.findings)
                    .any(|finding| {
                        &finding.finding_id == finding_id
                            && finding.disposition.as_ref().is_some_and(|disposition| {
                                disposition.kind
                                    == ProductionFindingDispositionKind::RemediationVerified
                                    && disposition.verifying_review_id.as_deref()
                                        == Some(review.review_id.as_str())
                            })
                    })
            })
            || stored_candidates != result_candidate.findings
            || review.findings.iter().enumerate().any(|(index, finding)| {
                finding.review_id != review.review_id
                    || finding.sequence as usize != index + 1
                    || (finding.severity == ProductionFindingSeverity::Critical
                        && finding.disposition.as_ref().is_some_and(|disposition| {
                            disposition.kind
                                != ProductionFindingDispositionKind::RemediationVerified
                        }))
                    || (finding.severity == ProductionFindingSeverity::Minor
                        && finding.disposition.is_some())
            })
        {
            return Ok(false);
        }
        for finding in &review.findings {
            let Some(disposition) = finding.disposition.as_ref() else {
                continue;
            };
            let disposition_valid = match disposition.kind {
                ProductionFindingDispositionKind::RemediationVerified => {
                    disposition.decided_by == "host_policy"
                        && disposition.acting_role == "integration_gate"
                        && disposition.target_artifact_id == review.target_artifact_id
                        && disposition.target_version > review.target_version
                        && disposition
                            .verifying_review_id
                            .as_ref()
                            .is_some_and(|review_id| {
                                reviews.iter().any(|verifying| {
                                    &verifying.review_id == review_id
                                        && verifying.target_version == disposition.target_version
                                        && verifying
                                            .resolved_finding_ids
                                            .contains(&finding.finding_id)
                                })
                            })
                }
                ProductionFindingDispositionKind::ExceptionApproved => {
                    finding.severity == ProductionFindingSeverity::Major
                        && definition.major_finding_policy
                            == MajorFindingPolicy::EngineerExceptionAllowed
                        && disposition.decided_by == "engineer_user"
                        && disposition.acting_role == "tendering_manager"
                        && disposition.target_artifact_id == review.target_artifact_id
                        && disposition.target_version == review.target_version
                        && disposition.verifying_review_id.is_none()
                }
            };
            if !disposition_valid
                || !production_disposition_audit_is_valid(
                    connection,
                    production_task_id,
                    review,
                    finding,
                    disposition,
                )?
            {
                return Ok(false);
            }
        }
    }
    let blockers_open = reviews
        .iter()
        .flat_map(|review| &review.findings)
        .any(|finding| finding.severity.blocks_integration() && finding.disposition.is_none());
    let expected_approval_gates = vec![
        "bid_decision_accepted".to_owned(),
        "work_plan_approved".to_owned(),
        "production_activation_active".to_owned(),
        "dependencies_ready_for_integration".to_owned(),
        "approved_query_treatments_applied".to_owned(),
    ];
    for stored in &all_readiness {
        check()?;
        let Some(artifact) = artifacts.iter().find(|artifact| {
            artifact.summary.artifact_id == stored.artifact_id
                && artifact.summary.version == stored.artifact_version
        }) else {
            return Ok(false);
        };
        let review_gate_satisfied = if definition.review_profile_id.is_none() {
            stored.review_id.is_none()
        } else {
            stored.review_id.as_ref().is_some_and(|readiness_review_id| {
                reviews.iter().any(|review| {
                    &review.review_id == readiness_review_id
                        && review.target_artifact_id == stored.artifact_id
                        && review.target_version == stored.artifact_version
                        && (review.result == ProductionReviewResult::Satisfied
                            || (review.result == ProductionReviewResult::RequiresRemediation
                                && review.findings.iter().all(|finding| {
                                    !finding.severity.blocks_integration()
                                        || finding.disposition.as_ref().is_some_and(|disposition| {
                                            disposition.kind
                                                == ProductionFindingDispositionKind::ExceptionApproved
                                        })
                                })))
                })
            })
        };
        if stored.payload_sha256 != artifact.summary.payload_sha256
            || !stored.output_validation_passed
            || !stored.evidence_verified
            || !stored.dependencies_satisfied
            || stored.approval_gates != expected_approval_gates
            || stored.finding_dispositions_sha256
                != production_finding_dispositions_sha256_at(
                    connection,
                    production_task_id,
                    stored.artifact_version,
                )?
            || !review_gate_satisfied
        {
            return Ok(false);
        }
    }
    let latest_artifact = artifacts.last();
    let latest_review = reviews.last();
    let readiness_expected = state == ProductionTaskState::ReadyForIntegration;
    if readiness_expected != readiness.is_some() || blockers_open && readiness.is_some() {
        return Ok(false);
    }
    if let Some(ref readiness) = readiness {
        let Some(artifact) = latest_artifact else {
            return Ok(false);
        };
        let review_gate_satisfied = if definition.review_profile_id.is_none() {
            readiness.review_id.is_none()
        } else {
            readiness.review_id.as_ref().is_some_and(|readiness_review_id| {
                latest_review.is_some_and(|review| {
                    &review.review_id == readiness_review_id
                        && (review.result == ProductionReviewResult::Satisfied
                            || (review.result == ProductionReviewResult::RequiresRemediation
                                && review.findings.iter().all(|finding| {
                                    !finding.severity.blocks_integration()
                                        || finding.disposition.as_ref().is_some_and(|disposition| {
                                            disposition.kind
                                                == ProductionFindingDispositionKind::ExceptionApproved
                                        })
                                })))
                })
            })
        };
        if readiness.artifact_id != artifact.summary.artifact_id
            || readiness.artifact_version != artifact.summary.version
            || readiness.payload_sha256 != artifact.summary.payload_sha256
            || !readiness.output_validation_passed
            || !readiness.evidence_verified
            || !readiness.dependencies_satisfied
            || readiness.approval_gates != expected_approval_gates
            || readiness.finding_dispositions_sha256
                != production_finding_dispositions_sha256_at(
                    connection,
                    production_task_id,
                    readiness.artifact_version,
                )?
            || !review_gate_satisfied
        {
            return Ok(false);
        }
    }
    match state {
        ProductionTaskState::ReviewReady => Ok(latest_artifact.is_some()
            && latest_artifact.is_some_and(|artifact| {
                !reviews.iter().any(|review| {
                    review.target_artifact_id == artifact.summary.artifact_id
                        && review.target_version == artifact.summary.version
                })
            })),
        ProductionTaskState::RemediationReady => {
            let review_remediation = latest_review.is_some_and(|review| {
                review.result == ProductionReviewResult::RequiresRemediation && blockers_open
            });
            let query_remediation = latest_artifact.is_some()
                && (task_has_current_query_artifact_invalidation(connection, production_task_id)?
                    || !production_artifact_applies_current_query_treatments(
                        connection,
                        &definition.task_key,
                        &latest_artifact.expect("checked").summary.artifact_id,
                        latest_artifact.expect("checked").summary.version,
                    )?);
            let change_remediation =
                TenderStore::task_has_active_change_rework(connection, production_task_id)?;
            Ok(review_remediation || query_remediation || change_remediation)
        }
        ProductionTaskState::ReadyForIntegration => Ok(true),
        _ => Ok(readiness.is_none()),
    }
}

fn production_finding_dispositions_sha256_at(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    through_artifact_version: u32,
) -> Result<String, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT findings.finding_id, reviews.review_id, findings.severity,
                    dispositions.disposition_id, dispositions.audit_sequence,
                    dispositions.disposition, dispositions.target_artifact_id,
                    dispositions.target_version, dispositions.verifying_review_id,
                    dispositions.decided_by, dispositions.acting_role,
                    dispositions.rationale, dispositions.consequence,
                    dispositions.created_at
             FROM production_review_findings AS findings
             JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
             LEFT JOIN production_finding_dispositions AS dispositions
               ON dispositions.finding_id = findings.finding_id
             WHERE reviews.production_task_id = ?1
               AND reviews.target_version <= ?2
             ORDER BY reviews.rowid, findings.finding_sequence",
        )
        .map_err(sql_error)?;
    let observations = statement
        .query_map(
            params![production_task_id, through_artifact_version],
            |row| {
                Ok(json!({
                    "finding_id": row.get::<_, String>(0)?,
                    "review_id": row.get::<_, String>(1)?,
                    "severity": row.get::<_, String>(2)?,
                    "disposition_id": row.get::<_, Option<String>>(3)?,
                    "audit_sequence": row.get::<_, Option<i64>>(4)?,
                    "disposition": row.get::<_, Option<String>>(5)?,
                    "target_artifact_id": row.get::<_, Option<String>>(6)?,
                    "target_version": row.get::<_, Option<u32>>(7)?,
                    "verifying_review_id": row.get::<_, Option<String>>(8)?,
                    "decided_by": row.get::<_, Option<String>>(9)?,
                    "acting_role": row.get::<_, Option<String>>(10)?,
                    "rationale": row.get::<_, Option<String>>(11)?,
                    "consequence": row.get::<_, Option<String>>(12)?,
                    "created_at": row.get::<_, Option<String>>(13)?,
                }))
            },
        )
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(sha256_hex(canonical_json(&observations)?.as_bytes()))
}

fn production_disposition_audit_is_valid(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    review: &ProductionReview,
    finding: &ProductionReviewFinding,
    disposition: &ProductionFindingDisposition,
) -> Result<bool, TenderCommandError> {
    let audit_sequence: Option<i64> = connection
        .query_row(
            "SELECT audit_sequence FROM production_finding_dispositions
             WHERE disposition_id = ?1",
            [&disposition.disposition_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(audit_sequence) = audit_sequence else {
        return Ok(false);
    };
    let task_ready_for_integration: bool = connection
        .query_row(
            "SELECT NOT EXISTS(
               SELECT 1 FROM production_review_findings AS candidate
               JOIN production_reviews AS candidate_review
                 ON candidate_review.review_id = candidate.review_id
               LEFT JOIN production_finding_dispositions AS candidate_disposition
                 ON candidate_disposition.finding_id = candidate.finding_id
               WHERE candidate_review.production_task_id = ?1
                 AND candidate.severity IN ('critical', 'major')
                 AND (candidate_disposition.finding_id IS NULL
                      OR candidate_disposition.audit_sequence > ?2)
             )",
            params![production_task_id, audit_sequence],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let audit: Option<(String, String, String)> = connection
        .query_row(
            "SELECT event_type, payload_json, created_at FROM audit_events WHERE sequence = ?1",
            [audit_sequence],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let Some((event_type, payload_json, audit_created_at)) = audit else {
        return Ok(false);
    };
    let payload: serde_json::Value = parse_canonical_json(&payload_json)?;
    let expected_event_type = match disposition.kind {
        ProductionFindingDispositionKind::RemediationVerified => {
            "production_finding_remediation_verified"
        }
        ProductionFindingDispositionKind::ExceptionApproved => {
            "production_finding_exception_approved"
        }
    };
    let expected_change = json!({
        "acting_role": disposition.acting_role,
        "artifact_id": disposition.target_artifact_id,
        "artifact_version": disposition.target_version.to_string(),
        "consequence": disposition.consequence,
        "decided_by": disposition.decided_by,
        "disposition": disposition.kind.as_str(),
        "disposition_id": disposition.disposition_id,
        "finding_id": finding.finding_id,
        "production_task_id": production_task_id,
        "rationale": disposition.rationale,
        "review_id": review.review_id,
        "task_ready_for_integration": task_ready_for_integration,
        "verifying_review_id": disposition.verifying_review_id,
    });
    let valid = event_type == expected_event_type
        && audit_created_at == disposition.created_at
        && payload.get("change") == Some(&expected_change);
    Ok(valid)
}

pub(super) fn production_task_for_run(
    connection: &rusqlite::Connection,
    run_id: &str,
) -> Result<Option<String>, TenderCommandError> {
    connection
        .query_row(
            "SELECT attempts.production_task_id
             FROM production_task_attempts AS attempts
             JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
             WHERE runs.run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)
}

pub(crate) fn production_task_and_state_for_run(
    connection: &rusqlite::Connection,
    run_id: &str,
) -> Result<Option<(String, ProductionTaskState)>, TenderCommandError> {
    connection
        .query_row(
            "SELECT tasks.production_task_id, tasks.status
             FROM production_tasks AS tasks
             JOIN production_task_attempts AS attempts
               ON attempts.production_task_id = tasks.production_task_id
             JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
             WHERE runs.run_id = ?1",
            [run_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .map_err(sql_error)?
        .map(|(production_task_id, state)| {
            Ok((production_task_id, ProductionTaskState::parse(&state)?))
        })
        .transpose()
}

pub(super) fn production_completion_payload_is_valid(
    connection: &rusqlite::Connection,
    task_id: &str,
    payload_json: &str,
) -> Result<bool, TenderCommandError> {
    let production: Option<(String, String)> = connection
        .query_row(
            "SELECT attempts.production_task_id, attempts.attempt_kind
             FROM production_task_attempts AS attempts
             WHERE attempts.task_id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let Some((production_task_id, attempt_kind)) = production else {
        return Ok(true);
    };
    let valid = if matches!(attempt_kind.as_str(), "author" | "remediation") {
        match parse_canonical_json::<ProductionArtifactPayload>(payload_json) {
            Ok(candidate) => {
                validate_artifact_candidate(connection, task_id, &candidate, &attempt_kind).is_ok()
            }
            Err(_) => false,
        }
    } else if attempt_kind == "query_control" {
        parse_canonical_json::<ProductionQueryControlCandidate>(payload_json)
            .ok()
            .is_some_and(|candidate| {
                validate_query_control_candidate(connection, task_id, &candidate).is_ok()
            })
    } else if attempt_kind == "review" {
        let artifact =
            load_exact_production_review_target(connection, &production_task_id, task_id)?;
        match (
            artifact,
            parse_canonical_json::<ProductionReviewCandidate>(payload_json).ok(),
        ) {
            (Some((_, version, _, artifact_payload_json, _)), Some(candidate)) => {
                validate_review_candidate(
                    connection,
                    &production_task_id,
                    &candidate,
                    &artifact_payload_json,
                    version,
                )
                .is_ok()
            }
            _ => false,
        }
    } else {
        false
    };
    Ok(valid)
}

impl TenderStore {
    pub(crate) fn production_task_turn_accepted(
        &self,
        production_task_id: &str,
    ) -> Result<bool, TenderCommandError> {
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1
                   FROM production_tasks AS tasks
                   JOIN production_task_attempts AS attempts
                     ON attempts.production_task_id = tasks.production_task_id
                    AND attempts.task_id = tasks.task_id
                   JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                   WHERE tasks.production_task_id = ?1
                     AND tasks.status IN ('running', 'reviewing')
                     AND runs.status = 'running'
                     AND runs.provider_turn_ref IS NOT NULL
                 )",
                [production_task_id],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn production_task_and_state_for_run(
        &self,
        run_id: &str,
    ) -> Result<Option<(String, ProductionTaskState)>, TenderCommandError> {
        production_task_and_state_for_run(&self.connection, run_id)
    }

    pub(crate) fn production_task_state(
        &self,
        production_task_id: &str,
    ) -> Result<Option<ProductionTaskState>, TenderCommandError> {
        self.connection
            .query_row(
                "SELECT status FROM production_tasks WHERE production_task_id = ?1",
                [production_task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|state| ProductionTaskState::parse(&state))
            .transpose()
    }
}

pub(super) fn finish_production_task(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    run_id: &str,
    task_id: &str,
    run_state: AgentRunState,
    payload_json: Option<&str>,
    completed_at: &str,
) -> Result<(), TenderCommandError> {
    let production: Option<(String, String, String, String, u32, String, String)> = transaction
        .query_row(
            "SELECT tasks.production_task_id, tasks.status, tasks.task_key,
                    activations.activation_id, activations.plan_version,
                    attempts.attempt_kind, tasks.task_definition_json
             FROM production_tasks AS tasks
             JOIN production_activations AS activations
               ON activations.activation_id = tasks.activation_id
             JOIN production_task_attempts AS attempts ON attempts.task_id = tasks.task_id
             WHERE tasks.task_id = ?1",
            [task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some((
        production_task_id,
        prior_status,
        task_key,
        activation_id,
        plan_version,
        attempt_kind,
        task_definition_json,
    )) = production
    else {
        return Ok(());
    };
    let expected_running_state = if attempt_kind == "review" {
        ProductionTaskState::Reviewing
    } else {
        ProductionTaskState::Running
    };
    if prior_status != expected_running_state.as_str() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let definition: WorkPlanTask = parse_canonical_json(&task_definition_json)?;
    let self_validating_review = attempt_kind == "author"
        && definition.review_profile_id.is_none()
        && definition.review_profile_version.is_none();
    let mut review_id = None;
    let next_state = match run_state {
        AgentRunState::Completed if attempt_kind == "query_control" => {
            let payload_json = payload_json
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let candidate: ProductionQueryControlCandidate = parse_canonical_json(payload_json)?;
            validate_query_control_candidate(transaction, task_id, &candidate)?;
            publish_agent_query_proposals(
                transaction,
                AgentQueryPublication {
                    tender_id,
                    run_id,
                    task_id,
                    current_task_key: &task_key,
                    proposals: &[],
                    updates: &candidate.query_updates,
                    query_control: true,
                    created_at: completed_at,
                },
            )?;
            if !task_has_blocking_query(transaction, &task_key)? {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            ProductionTaskState::QueryBlocked
        }
        AgentRunState::Completed if matches!(attempt_kind.as_str(), "author" | "remediation") => {
            let payload_json = payload_json
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let candidate: ProductionArtifactPayload = parse_canonical_json(payload_json)?;
            validate_artifact_candidate(transaction, task_id, &candidate, &attempt_kind)?;
            let prior: Option<(String, u32)> = transaction
                .query_row(
                    "SELECT artifact_id, version FROM production_artifact_versions
                     WHERE production_task_id = ?1 ORDER BY version DESC LIMIT 1",
                    [&production_task_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let version = match (attempt_kind.as_str(), prior.as_ref()) {
                ("author", None) => 1,
                ("remediation", Some((_, prior_version))) => prior_version
                    .checked_add(1)
                    .filter(|version| *version <= MAX_PRODUCTION_TASK_ATTEMPTS)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
            };
            let artifact_id = prior
                .as_ref()
                .map(|prior| prior.0.clone())
                .unwrap_or_else(|| production_task_id.clone());
            let remediation_review_id = if attempt_kind == "remediation" {
                transaction
                    .query_row(
                        "SELECT review_id FROM production_reviews
                         WHERE production_task_id = ?1
                           AND result = 'requires_remediation'
                         ORDER BY rowid DESC LIMIT 1",
                        [&production_task_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
            } else {
                None
            };
            transaction
                .execute(
                    "INSERT INTO production_artifact_versions (
                       artifact_id, production_task_id, version, author_run_id, prior_version,
                       remediation_review_id, payload_json, payload_sha256,
                       output_validation_passed, evidence_verified, data_scopes_json,
                       data_classifications_json, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 1, ?9, ?10, ?11)",
                    params![
                        artifact_id,
                        production_task_id,
                        version,
                        run_id,
                        prior.as_ref().map(|prior| prior.1),
                        remediation_review_id,
                        payload_json,
                        sha256_hex(payload_json.as_bytes()),
                        canonical_json(&definition.permissions.data_scopes)?,
                        canonical_json(&definition.permissions.data_classifications)?,
                        completed_at,
                    ],
                )
                .map_err(sql_error)?;
            publish_agent_query_proposals(
                transaction,
                AgentQueryPublication {
                    tender_id,
                    run_id,
                    task_id,
                    current_task_key: &task_key,
                    proposals: &candidate.query_proposals,
                    updates: &candidate.query_updates,
                    query_control: false,
                    created_at: completed_at,
                },
            )?;
            if task_has_blocking_query(transaction, &task_key)? {
                ProductionTaskState::QueryBlocked
            } else if !production_artifact_applies_current_query_treatments(
                transaction,
                &task_key,
                &artifact_id,
                version,
            )? {
                ProductionTaskState::RemediationReady
            } else if self_validating_review {
                insert_production_integration_readiness(
                    transaction,
                    &production_task_id,
                    &artifact_id,
                    version,
                    &sha256_hex(payload_json.as_bytes()),
                    None,
                    completed_at,
                )?;
                ProductionTaskState::ReadyForIntegration
            } else {
                ProductionTaskState::ReviewReady
            }
        }
        AgentRunState::Completed if attempt_kind == "review" => {
            let payload_json = payload_json
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let candidate: ProductionReviewCandidate = parse_canonical_json(payload_json)?;
            let artifact =
                load_exact_production_review_target(transaction, &production_task_id, task_id)?
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            validate_review_candidate(
                transaction,
                &production_task_id,
                &candidate,
                &artifact.3,
                artifact.1,
            )?;
            let reviewer: (String, u32) = transaction
                .query_row(
                    "SELECT profile_id, profile_version FROM agent_runs WHERE run_id = ?1",
                    [run_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(sql_error)?;
            if reviewer.0 == definition.profile_id
                || definition.review_profile_id.as_deref() != Some(reviewer.0.as_str())
                || definition.review_profile_version != Some(reviewer.1)
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let reviewer_profile = load_profile(transaction, reviewer.clone())?;
            let capability = production_review_capability(&definition, &reviewer_profile)?;
            let review_scope = production_review_scope(&definition)?;
            let review_criteria = production_review_criteria(&definition)?;
            let review_inputs = load_task(transaction, task_id)?.exact_inputs;
            let id = random_identifier(transaction)?;
            transaction
                .execute(
                    "INSERT INTO production_reviews (
                       review_id, production_task_id, target_artifact_id, target_version,
                       target_payload_sha256, reviewer_run_id, reviewer_profile_id,
                       reviewer_profile_version, capability, scope_json, criteria_json,
                       inputs_json, result, resolved_finding_ids_json, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        id,
                        production_task_id,
                        artifact.0,
                        artifact.1,
                        artifact.2,
                        run_id,
                        reviewer.0,
                        reviewer.1,
                        capability,
                        canonical_json(&review_scope)?,
                        canonical_json(&review_criteria)?,
                        canonical_json(&review_inputs)?,
                        candidate.result.as_str(),
                        canonical_json(&candidate.resolved_finding_ids)?,
                        completed_at,
                    ],
                )
                .map_err(sql_error)?;
            for (index, finding) in candidate.findings.iter().enumerate() {
                transaction
                    .execute(
                        "INSERT INTO production_review_findings (
                           finding_id, review_id, finding_sequence, severity, summary,
                           evidence_references_json, created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            random_identifier(transaction)?,
                            id,
                            u32::try_from(index + 1).map_err(|_| TenderCommandError::new(
                                TenderErrorCode::IntegrityFailed
                            ))?,
                            finding.severity.as_str(),
                            finding.summary,
                            canonical_json(&finding.evidence_references)?,
                            completed_at,
                        ],
                    )
                    .map_err(sql_error)?;
            }
            review_id = Some(id.clone());
            if !candidate.resolved_finding_ids.is_empty() {
                insert_remediation_dispositions(
                    transaction,
                    RemediationDispositionContext {
                        tender_id,
                        production_task_id: &production_task_id,
                        artifact_id: &artifact.0,
                        artifact_version: artifact.1,
                        verifying_review_id: &id,
                        task_ready_for_integration: candidate.result
                            == ProductionReviewResult::Satisfied,
                        created_at: completed_at,
                    },
                    &candidate.resolved_finding_ids,
                )?;
            }
            if candidate.result == ProductionReviewResult::Satisfied {
                if task_has_blocking_query(transaction, &task_key)? {
                    ProductionTaskState::QueryBlocked
                } else if !production_artifact_applies_current_query_treatments(
                    transaction,
                    &task_key,
                    &artifact.0,
                    artifact.1,
                )? {
                    ProductionTaskState::RemediationReady
                } else {
                    insert_production_integration_readiness(
                        transaction,
                        &production_task_id,
                        &artifact.0,
                        artifact.1,
                        &artifact.2,
                        Some(&id),
                        completed_at,
                    )?;
                    ProductionTaskState::ReadyForIntegration
                }
            } else {
                ProductionTaskState::RemediationReady
            }
        }
        AgentRunState::Completed => {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        AgentRunState::Interrupted => ProductionTaskState::Cancelled,
        AgentRunState::Indeterminate => ProductionTaskState::Indeterminate,
        AgentRunState::Failed if attempt_kind == "query_control" => {
            if task_has_blocking_query(transaction, &task_key)? {
                ProductionTaskState::QueryBlocked
            } else {
                ProductionTaskState::parse(production_task_state_after_query_release(
                    transaction,
                    &production_task_id,
                )?)?
            }
        }
        AgentRunState::Failed => ProductionTaskState::Failed,
        AgentRunState::Running => {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    };
    if run_state != AgentRunState::Completed && payload_json.is_some() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    if transaction
        .execute(
            "UPDATE production_tasks SET status = ?2, updated_at = ?3
             WHERE production_task_id = ?1 AND status = ?4",
            params![
                production_task_id,
                next_state.as_str(),
                completed_at,
                expected_running_state.as_str()
            ],
        )
        .map_err(sql_error)?
        != 1
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    if next_state == ProductionTaskState::ReadyForIntegration {
        refresh_ready_frontier(transaction, &activation_id, completed_at)?;
    }
    let tender_revision: u32 = transaction
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "production_task_finished",
        tender_revision,
        json!({
            "activation_id": activation_id,
            "attempt_kind": attempt_kind,
            "plan_version": plan_version.to_string(),
            "production_task_id": production_task_id,
            "run_id": run_id,
            "review_id": review_id,
            "status": next_state.as_str(),
            "task_key": task_key,
        }),
        completed_at,
    )
}

fn validate_artifact_candidate(
    transaction: &rusqlite::Connection,
    task_id: &str,
    candidate: &ProductionArtifactPayload,
    attempt_kind: &str,
) -> Result<(), TenderCommandError> {
    if candidate.summary.trim().is_empty()
        || candidate.summary.len() > 4_000
        || candidate.evidence_references.is_empty()
        || candidate.evidence_references.len() > MAX_PRODUCTION_EVIDENCE_REFERENCES
        || candidate.coordination_observations.is_empty()
        || candidate.coordination_observations.len() > 32
        || !candidate.gaps.is_empty()
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let task = load_task(transaction, task_id)?;
    let definition_json: String = transaction
        .query_row(
            "SELECT task_definition_json FROM production_tasks WHERE task_id = ?1",
            [task_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let definition: WorkPlanTask = parse_canonical_json(&definition_json)?;
    let coordination_contract = production_coordination_contract(transaction, &definition)?;
    let allowed = task
        .exact_inputs
        .iter()
        .map(production_evidence_reference)
        .collect::<std::collections::HashSet<_>>();
    let evidence = candidate
        .evidence_references
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let observation_subjects = candidate
        .coordination_observations
        .iter()
        .map(|observation| observation.subject)
        .collect::<std::collections::BTreeSet<_>>();
    let mut observed_assignment_keys =
        std::collections::BTreeMap::<ProductionCoordinationObservationSubject, Vec<String>>::new();
    for observation in &candidate.coordination_observations {
        if !coordination_contract
            .assignment_contracts
            .iter()
            .any(|contract| contract.subject == observation.subject)
        {
            continue;
        }
        let ProductionCoordinationObservationValue::TextSet { values } = &observation.value else {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        };
        observed_assignment_keys
            .entry(observation.subject)
            .or_default()
            .extend(
                values
                    .iter()
                    .filter_map(|value| value.split_once('=').map(|(key, _)| key.to_owned())),
            );
    }
    let assignments_are_exact = coordination_contract
        .assignment_contracts
        .iter()
        .all(|contract| {
            let observed = observed_assignment_keys
                .get(&contract.subject)
                .cloned()
                .unwrap_or_default();
            let observed_set = observed
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            observed.len() == observed_set.len()
                && observed_set
                    == contract
                        .required_keys
                        .iter()
                        .cloned()
                        .collect::<std::collections::BTreeSet<_>>()
        });
    if evidence.len() != candidate.evidence_references.len()
        || evidence
            .iter()
            .any(|reference| !allowed.contains(*reference))
        || !coordination_contract
            .required_subjects
            .iter()
            .all(|subject| observation_subjects.contains(subject))
        || !assignments_are_exact
        || candidate
            .coordination_observations
            .iter()
            .any(|observation| {
                canonical_coordination_observation_value(observation.subject, &observation.value)
                    .is_none()
                    || !coordination_subject_is_allowed(&coordination_contract, observation.subject)
                    || observation.evidence_references.is_empty()
                    || observation.evidence_references.len() > MAX_PRODUCTION_EVIDENCE_REFERENCES
                    || observation
                        .evidence_references
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != observation.evidence_references.len()
                    || observation
                        .evidence_references
                        .iter()
                        .any(|reference| !allowed.contains(reference))
            })
        || !agent_query_publication_is_valid(
            transaction,
            task_id,
            &candidate.query_proposals,
            &candidate.query_updates,
            false,
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let expected_treatments =
        approved_query_treatments_for_inputs(transaction, &task.exact_inputs)?;
    let applications = candidate
        .query_treatment_applications
        .iter()
        .map(|application| application.decision_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let expected_decision_ids = expected_treatments
        .iter()
        .map(|decision| decision.decision_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if applications.len() != candidate.query_treatment_applications.len()
        || applications != expected_decision_ids
        || candidate
            .query_treatment_applications
            .iter()
            .any(|application| {
                let expected = expected_treatments
                    .iter()
                    .find(|decision| decision.decision_id == application.decision_id);
                let exact_reference = format!(
                    "approved_query_treatment:{}:{}",
                    application.decision_id, application.query_version
                );
                let exact_query_reference = format!(
                    "tender_query_version:{}:{}",
                    application.query_id, application.query_version
                );
                expected.is_none_or(|decision| {
                    decision.query_id != application.query_id
                        || decision.query_version != application.query_version
                        || decision.treatment != application.treatment
                        || application.application.trim().is_empty()
                        || application.application.len() > 4_000
                        || application.evidence_references.is_empty()
                        || application.evidence_references.len()
                            > MAX_PRODUCTION_EVIDENCE_REFERENCES
                        || !application.evidence_references.contains(&exact_reference)
                        || !application
                            .evidence_references
                            .contains(&exact_query_reference)
                        || application
                            .evidence_references
                            .iter()
                            .any(|reference| !allowed.contains(reference))
                })
            })
        || candidate.query_proposals.len() > 16
        || candidate.query_updates.len() > 16
        || candidate.query_proposals.iter().any(|proposal| {
            proposal.evidence.is_empty()
                || proposal
                    .evidence
                    .iter()
                    .any(|reference| !task.exact_inputs.contains(reference))
        })
        || candidate.query_updates.iter().any(|update| {
            update.query_id.len() != 32
                || update.base_version == 0
                || update
                    .added_evidence
                    .iter()
                    .any(|reference| !task.exact_inputs.contains(reference))
                || update
                    .response_evidence
                    .iter()
                    .any(|reference| !task.exact_inputs.contains(reference))
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let open_findings = if attempt_kind == "remediation" {
        let mut statement = transaction
            .prepare(
                "SELECT findings.finding_id
                 FROM production_review_findings AS findings
                 JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
                 LEFT JOIN production_finding_dispositions AS dispositions
                   ON dispositions.finding_id = findings.finding_id
                 JOIN production_task_attempts AS attempts
                   ON attempts.production_task_id = reviews.production_task_id
                  AND attempts.task_id = ?1
                 WHERE findings.severity IN ('critical', 'major')
                   AND dispositions.finding_id IS NULL
                 ORDER BY reviews.rowid, findings.finding_sequence",
            )
            .map_err(sql_error)?;
        let findings = statement
            .query_map([task_id], |row| row.get::<_, String>(0))
            .map_err(sql_error)?
            .collect::<Result<std::collections::BTreeSet<_>, _>>()
            .map_err(sql_error)?;
        findings
    } else {
        std::collections::BTreeSet::new()
    };
    let remediated = candidate
        .remediations
        .iter()
        .map(|remediation| remediation.finding_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let has_query_remediation_input = task
        .exact_inputs
        .iter()
        .any(|input| input.kind == "tender_query_version");
    let has_change_remediation_input = task
        .exact_inputs
        .iter()
        .any(|input| input.kind == "change_assessment");
    if (attempt_kind == "remediation"
        && ((open_findings.is_empty()
            && expected_treatments.is_empty()
            && !has_query_remediation_input
            && !has_change_remediation_input)
            || remediated != open_findings))
        || (attempt_kind != "remediation" && !candidate.remediations.is_empty())
        || candidate.remediations.len() != remediated.len()
        || candidate.remediations.iter().any(|remediation| {
            remediation.treatment.trim().is_empty()
                || remediation.treatment.len() > 4_000
                || remediation.evidence_references.is_empty()
                || remediation.evidence_references.len() > MAX_PRODUCTION_EVIDENCE_REFERENCES
                || remediation
                    .evidence_references
                    .iter()
                    .any(|reference| !allowed.contains(reference))
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn required_coordination_subject(workstream_key: &str) -> ProductionCoordinationObservationSubject {
    let workstream_key = workstream_key.to_ascii_lowercase();
    if workstream_key.contains("cost") || workstream_key.contains("commercial") {
        ProductionCoordinationObservationSubject::ExpectedDeliveryCost
    } else if workstream_key.contains("document") || workstream_key.contains("submission") {
        ProductionCoordinationObservationSubject::SubmissionCommitment
    } else if workstream_key.contains("assurance") || workstream_key.contains("risk") {
        ProductionCoordinationObservationSubject::RiskCommitment
    } else if workstream_key.contains("analysis") || workstream_key.contains("technical") {
        ProductionCoordinationObservationSubject::TechnicalCommitment
    } else if workstream_key.contains("programme") || workstream_key.contains("schedule") {
        ProductionCoordinationObservationSubject::ProgrammeCommitment
    } else if workstream_key.contains("procurement") || workstream_key.contains("supplier") {
        ProductionCoordinationObservationSubject::ProcurementCommitment
    } else if workstream_key.contains("contract") {
        ProductionCoordinationObservationSubject::ContractualCommitment
    } else {
        ProductionCoordinationObservationSubject::TechnicalCommitment
    }
}

fn production_coordination_contract(
    connection: &rusqlite::Connection,
    task: &WorkPlanTask,
) -> Result<ProductionCoordinationContract, TenderCommandError> {
    let mut required_subjects = required_coordination_subjects(task);
    let task_deadline_required =
        required_subjects.contains(&ProductionCoordinationObservationSubject::SubmissionDeadline);
    let primary_subject = required_coordination_subject(&task.workstream_key);
    let mut assignment_keys = std::collections::BTreeMap::<
        ProductionCoordinationObservationSubject,
        std::collections::BTreeSet<String>,
    >::new();
    let mut source_observations = Vec::new();
    let task_assignment_key = coordination_task_assignment_key(&task.task_key);
    if coordination_subject_uses_assignments(primary_subject) {
        assignment_keys
            .entry(primary_subject)
            .or_default()
            .insert(task_assignment_key.clone());
    }
    let package = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "bid_decision_package")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut statement = connection
        .prepare(
            "WITH package_records AS (
                   SELECT record_id, record_version
                   FROM bid_compliance_rows
                   WHERE package_id = ?1 AND package_version = ?2
                     AND verification_status = 'verified'
                     AND trust_class IN (
                       'deterministic_fact', 'verified', 'engineer_verified', 'approved_assumption'
                     )
                   UNION
                   SELECT record_id, record_version
                   FROM bid_decision_package_record_bindings
                   WHERE package_id = ?1 AND package_version = ?2
                 )
                 SELECT records.stable_key, versions.kind, versions.fields_json,
                        versions.record_id, versions.version
                 FROM package_records
                 JOIN tender_records AS records USING (record_id)
                 JOIN tender_record_versions AS versions USING (record_id)
                 WHERE versions.version = package_records.record_version
                 ORDER BY records.stable_key, versions.record_id LIMIT 257",
        )
        .map_err(sql_error)?;
    let records = statement
        .query_map(params![package.reference, package.version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, u32>(4)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    if records.len() > 256 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for (stable_key, kind, fields_json, record_id, version) in records {
        let kind = TenderRecordKind::parse(&kind)?;
        let fields: Vec<serde_json::Value> = parse_canonical_json(&fields_json)?;
        if fields.len() > 64 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for field in fields {
            let Some(field_name) = field.get("name").and_then(serde_json::Value::as_str) else {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            };
            let Some(value) = field
                .get("normalized_value")
                .and_then(serde_json::Value::as_str)
                .or_else(|| field.get("value").and_then(serde_json::Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Some((subject, typed_value)) =
                record_coordination_observation(kind, &stable_key, field_name, value)
            else {
                continue;
            };
            if !record_observation_is_relevant_to_task(kind, subject, task) {
                continue;
            }
            if canonical_coordination_observation_value(subject, &typed_value).is_none() {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let ProductionCoordinationObservationValue::TextSet { values } = &typed_value else {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            };
            if values.len() != 1 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let key = values[0]
                .split_once('=')
                .map(|(key, _)| key)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if key.is_empty()
                || key.len() > 512
                || !assignment_keys
                    .entry(subject)
                    .or_default()
                    .insert(key.to_owned())
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            required_subjects.insert(subject);
            source_observations.push(ProductionCoordinationSourceObservation {
                subject,
                value: typed_value,
                reference: format!("tender_record_version:{record_id}:{version}"),
            });
        }
    }
    assignment_keys
        .entry(ProductionCoordinationObservationSubject::ResponsibleParty)
        .or_default()
        .insert(task_assignment_key.clone());
    source_observations.push(ProductionCoordinationSourceObservation {
        subject: ProductionCoordinationObservationSubject::ResponsibleParty,
        value: ProductionCoordinationObservationValue::TextSet {
            values: vec![format!("{task_assignment_key}={}", task.profile_id)],
        },
        reference: format!("work_plan_task:{}", task.task_key),
    });
    if task_deadline_required {
        let deadline_key = coordination_task_assignment_key(&task.task_key);
        let deadline = ProductionCoordinationObservationValue::TextSet {
            values: vec![format!("{deadline_key}={}", task.deadline)],
        };
        if canonical_coordination_observation_value(
            ProductionCoordinationObservationSubject::SubmissionDeadline,
            &deadline,
        )
        .is_none()
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        assignment_keys
            .entry(ProductionCoordinationObservationSubject::SubmissionDeadline)
            .or_default()
            .insert(deadline_key);
        source_observations.push(ProductionCoordinationSourceObservation {
            subject: ProductionCoordinationObservationSubject::SubmissionDeadline,
            value: deadline,
            reference: format!("work_plan_task:{}", task.task_key),
        });
    }
    let assignment_contracts = assignment_keys
        .into_iter()
        .map(
            |(subject, required_keys)| ProductionCoordinationAssignmentContract {
                subject,
                required_keys: required_keys.into_iter().collect(),
            },
        )
        .collect();
    let contract = ProductionCoordinationContract {
        required_subjects: required_subjects.into_iter().collect(),
        assignment_contracts,
        source_observations,
    };
    if !coordination_contract_is_representable(&contract)? {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(contract)
}

fn coordination_contract_is_representable(
    contract: &ProductionCoordinationContract,
) -> Result<bool, TenderCommandError> {
    let assignment_count = contract
        .assignment_contracts
        .iter()
        .map(|assignment| assignment.required_keys.len())
        .sum::<usize>();
    let assignment_subjects = contract
        .assignment_contracts
        .iter()
        .map(|assignment| assignment.subject)
        .collect::<std::collections::BTreeSet<_>>();
    let observation_count = contract
        .assignment_contracts
        .iter()
        .map(|assignment| assignment.required_keys.len().div_ceil(32))
        .sum::<usize>()
        + contract
            .required_subjects
            .iter()
            .filter(|subject| !assignment_subjects.contains(subject))
            .count();
    let assignment_bytes = contract
        .source_observations
        .iter()
        .map(|observation| canonical_json(&observation.value).map(|value| value.len() + 32))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<usize>()
        .saturating_add(
            contract
                .assignment_contracts
                .iter()
                .flat_map(|assignment| assignment.required_keys.iter())
                .map(|key| key.len().saturating_add("=validated".len() + 32))
                .sum::<usize>(),
        );
    Ok(assignment_count <= MAX_PRODUCTION_COORDINATION_ASSIGNMENTS
        && observation_count <= 32
        && assignment_bytes <= MAX_PRODUCTION_COORDINATION_OUTPUT_BYTES
        && canonical_json(contract)?.len() <= MAX_PRODUCTION_COORDINATION_CONTRACT_BYTES)
}

fn required_coordination_subjects(
    task: &WorkPlanTask,
) -> std::collections::BTreeSet<ProductionCoordinationObservationSubject> {
    let mut subjects = std::collections::BTreeSet::from([
        required_coordination_subject(&task.workstream_key),
        ProductionCoordinationObservationSubject::ResponsibleParty,
    ]);
    let workstream = task.workstream_key.to_ascii_lowercase();
    if workstream.contains("programme")
        || workstream.contains("schedule")
        || workstream.contains("coordination")
        || workstream.contains("document")
        || workstream.contains("submission")
    {
        subjects.insert(ProductionCoordinationObservationSubject::SubmissionDeadline);
    }
    subjects
}

fn record_primary_coordination_subject(
    kind: TenderRecordKind,
) -> Option<ProductionCoordinationObservationSubject> {
    match kind {
        TenderRecordKind::Requirement => {
            Some(ProductionCoordinationObservationSubject::ProcurementCommitment)
        }
        TenderRecordKind::Clause => {
            Some(ProductionCoordinationObservationSubject::ContractualCommitment)
        }
        TenderRecordKind::Risk => Some(ProductionCoordinationObservationSubject::RiskCommitment),
        TenderRecordKind::Deliverable | TenderRecordKind::Form => {
            Some(ProductionCoordinationObservationSubject::SubmissionCommitment)
        }
        TenderRecordKind::EvaluationCriterion | TenderRecordKind::ProjectCharacteristic => {
            Some(ProductionCoordinationObservationSubject::TechnicalCommitment)
        }
        TenderRecordKind::Deadline => {
            Some(ProductionCoordinationObservationSubject::SubmissionDeadline)
        }
        TenderRecordKind::Assumption => {
            Some(ProductionCoordinationObservationSubject::ScopeQualification)
        }
        TenderRecordKind::TenderQuery => None,
    }
}

fn record_observation_is_relevant_to_task(
    kind: TenderRecordKind,
    subject: ProductionCoordinationObservationSubject,
    task: &WorkPlanTask,
) -> bool {
    if matches!(
        subject,
        ProductionCoordinationObservationSubject::ScopeQualification
            | ProductionCoordinationObservationSubject::ScopeExclusion
    ) {
        return true;
    }
    let required = required_coordination_subjects(task);
    if subject == ProductionCoordinationObservationSubject::ResponsibleParty {
        return record_primary_coordination_subject(kind)
            .is_some_and(|primary| required.contains(&primary));
    }
    required.contains(&subject) || subject == required_coordination_subject(&task.workstream_key)
}

pub(crate) fn record_version_is_relevant_to_production_task(
    connection: &rusqlite::Connection,
    record_id: &str,
    version: u32,
    task: &WorkPlanTask,
) -> Result<bool, TenderCommandError> {
    let (stable_key, kind, fields_json): (String, String, String) = connection
        .query_row(
            "SELECT records.stable_key, versions.kind, versions.fields_json
             FROM tender_records AS records
             JOIN tender_record_versions AS versions USING (record_id)
             WHERE versions.record_id = ?1 AND versions.version = ?2",
            params![record_id, version],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    let kind = TenderRecordKind::parse(&kind)?;
    let fields: Vec<serde_json::Value> = parse_canonical_json(&fields_json)?;
    if fields.len() > 64 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for field in fields {
        let Some(field_name) = field.get("name").and_then(serde_json::Value::as_str) else {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        };
        let Some(value) = field
            .get("normalized_value")
            .and_then(serde_json::Value::as_str)
            .or_else(|| field.get("value").and_then(serde_json::Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let Some((subject, _)) =
            record_coordination_observation(kind, &stable_key, field_name, value)
        else {
            continue;
        };
        if record_observation_is_relevant_to_task(kind, subject, task) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn record_coordination_observation(
    kind: TenderRecordKind,
    stable_key: &str,
    field_name: &str,
    value: &str,
) -> Option<(
    ProductionCoordinationObservationSubject,
    ProductionCoordinationObservationValue,
)> {
    let normalized_field_name = normalize_coordination_identifier(field_name);
    let assignment_key = coordination_assignment_key(stable_key, field_name);
    let (subject, assigned_value) = if kind == TenderRecordKind::Deadline
        || normalized_field_name.contains("deadline")
        || normalized_field_name.contains("due_date")
        || normalized_field_name.contains("cutoff")
    {
        (
            ProductionCoordinationObservationSubject::SubmissionDeadline,
            value.to_owned(),
        )
    } else if normalized_field_name.contains("responsib")
        || normalized_field_name.contains("owner")
        || normalized_field_name.contains("party")
    {
        (
            ProductionCoordinationObservationSubject::ResponsibleParty,
            normalize_coordination_text(value),
        )
    } else if normalized_field_name.contains("qualification")
        || normalized_field_name.contains("assumption")
        || kind == TenderRecordKind::Assumption
    {
        (
            ProductionCoordinationObservationSubject::ScopeQualification,
            normalize_coordination_text(value),
        )
    } else if normalized_field_name.contains("exclusion") {
        (
            ProductionCoordinationObservationSubject::ScopeExclusion,
            normalize_coordination_text(value),
        )
    } else {
        let subject = match kind {
            TenderRecordKind::Requirement => {
                ProductionCoordinationObservationSubject::ProcurementCommitment
            }
            TenderRecordKind::Clause => {
                ProductionCoordinationObservationSubject::ContractualCommitment
            }
            TenderRecordKind::Risk => ProductionCoordinationObservationSubject::RiskCommitment,
            TenderRecordKind::Deliverable | TenderRecordKind::Form => {
                ProductionCoordinationObservationSubject::SubmissionCommitment
            }
            TenderRecordKind::EvaluationCriterion | TenderRecordKind::ProjectCharacteristic => {
                ProductionCoordinationObservationSubject::TechnicalCommitment
            }
            TenderRecordKind::Assumption
            | TenderRecordKind::TenderQuery
            | TenderRecordKind::Deadline => return None,
        };
        (subject, normalize_coordination_text(value))
    };
    let typed_value = ProductionCoordinationObservationValue::TextSet {
        values: vec![format!("{assignment_key}={assigned_value}")],
    };
    Some((subject, typed_value))
}

fn normalize_coordination_identifier(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-')
            {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn coordination_subject_uses_assignments(
    subject: ProductionCoordinationObservationSubject,
) -> bool {
    matches!(
        subject,
        ProductionCoordinationObservationSubject::SubmissionDeadline
            | ProductionCoordinationObservationSubject::ResponsibleParty
            | ProductionCoordinationObservationSubject::ScopeQualification
            | ProductionCoordinationObservationSubject::ScopeExclusion
            | ProductionCoordinationObservationSubject::TechnicalCommitment
            | ProductionCoordinationObservationSubject::ProgrammeCommitment
            | ProductionCoordinationObservationSubject::ProcurementCommitment
            | ProductionCoordinationObservationSubject::ContractualCommitment
            | ProductionCoordinationObservationSubject::RiskCommitment
            | ProductionCoordinationObservationSubject::SubmissionCommitment
    )
}

pub(crate) fn coordination_assignment_key(stable_key: &str, field_name: &str) -> String {
    format!(
        "{}.{}",
        coordination_assignment_key_component(stable_key),
        coordination_assignment_key_component(field_name)
    )
}

pub(crate) fn coordination_task_assignment_key(task_key: &str) -> String {
    format!("task:{}", coordination_assignment_key_component(task_key))
}

fn coordination_assignment_key_component(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return format!("v:{value}");
    }
    let mut encoded = String::with_capacity(2 + value.len() * 2);
    encoded.push_str("x:");
    for byte in value.as_bytes() {
        use std::fmt::Write;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn coordination_subject_is_allowed(
    contract: &ProductionCoordinationContract,
    subject: ProductionCoordinationObservationSubject,
) -> bool {
    contract.required_subjects.contains(&subject)
        || contract
            .assignment_contracts
            .iter()
            .any(|assignment| assignment.subject == subject)
        || (matches!(
            subject,
            ProductionCoordinationObservationSubject::CommercialAppetite
                | ProductionCoordinationObservationSubject::ApprovedTenderPrice
        ) && contract
            .required_subjects
            .contains(&ProductionCoordinationObservationSubject::ExpectedDeliveryCost))
}

pub(crate) fn canonical_coordination_observation_value(
    subject: ProductionCoordinationObservationSubject,
    value: &ProductionCoordinationObservationValue,
) -> Option<String> {
    match (subject, value) {
        (
            ProductionCoordinationObservationSubject::SubmissionDeadline,
            ProductionCoordinationObservationValue::TextSet { values },
        ) if !values.is_empty()
            && values.len() <= 32
            && values.iter().all(valid_coordination_deadline_assignment) =>
        {
            let mut values = values
                .iter()
                .map(|value| {
                    let (key, timestamp) = value
                        .split_once('=')
                        .expect("validated deadline assignment");
                    let timestamp = timestamp
                        .parse::<Timestamp>()
                        .expect("validated deadline timestamp");
                    format!("{key}={timestamp}")
                })
                .collect::<Vec<_>>();
            values.sort();
            values.dedup();
            canonical_json(&values).ok()
        }
        (
            ProductionCoordinationObservationSubject::ExpectedDeliveryCost
            | ProductionCoordinationObservationSubject::ApprovedTenderPrice,
            ProductionCoordinationObservationValue::Amount { value, currency },
        ) if currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase()) => value
            .parse::<Decimal>()
            .ok()
            .map(|value| format!("{} {currency}", value.normalize())),
        (
            ProductionCoordinationObservationSubject::ResponsibleParty
            | ProductionCoordinationObservationSubject::ScopeQualification
            | ProductionCoordinationObservationSubject::ScopeExclusion,
            ProductionCoordinationObservationValue::TextSet { values },
        ) if values.len() <= 32
            && (subject != ProductionCoordinationObservationSubject::ResponsibleParty
                || !values.is_empty())
            && values.iter().all(|value| {
                !value.trim().is_empty()
                    && value.trim() == value
                    && value.len() <= 4_608
                    && (subject != ProductionCoordinationObservationSubject::ResponsibleParty
                        || value
                            .split_once('=')
                            .is_some_and(|(responsibility, party)| {
                                !responsibility.trim().is_empty()
                                    && responsibility.trim() == responsibility
                                    && !party.trim().is_empty()
                                    && party.trim() == party
                            }))
            }) =>
        {
            let mut values = values
                .iter()
                .map(|value| normalize_coordination_text(value))
                .collect::<Vec<_>>();
            values.sort();
            values.dedup();
            canonical_json(&values).ok()
        }
        (
            ProductionCoordinationObservationSubject::TechnicalCommitment
            | ProductionCoordinationObservationSubject::ProgrammeCommitment
            | ProductionCoordinationObservationSubject::ProcurementCommitment
            | ProductionCoordinationObservationSubject::ContractualCommitment
            | ProductionCoordinationObservationSubject::RiskCommitment
            | ProductionCoordinationObservationSubject::SubmissionCommitment,
            ProductionCoordinationObservationValue::TextSet { values },
        ) if !values.is_empty()
            && values.len() <= 32
            && values.iter().all(valid_coordination_assignment) =>
        {
            let mut values = values
                .iter()
                .map(|value| {
                    let (key, assigned) = value
                        .split_once('=')
                        .expect("validated coordination assignment");
                    format!("{key}={}", normalize_coordination_text(assigned))
                })
                .collect::<Vec<_>>();
            values.sort();
            values.dedup();
            canonical_json(&values).ok()
        }
        (
            ProductionCoordinationObservationSubject::CommercialAppetite
            | ProductionCoordinationObservationSubject::QueryTreatment,
            ProductionCoordinationObservationValue::Text { text },
        ) if !text.trim().is_empty() && text.trim() == text && text.len() <= 4_000 => {
            Some(normalize_coordination_text(text))
        }
        _ => None,
    }
}

fn valid_coordination_assignment(value: &String) -> bool {
    if value.trim() != value || value.len() > 4_608 {
        return false;
    }
    value.split_once('=').is_some_and(|(key, assigned)| {
        !key.is_empty()
            && key.len() <= 512
            && key.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'-' | b'.' | b':')
            })
            && !assigned.trim().is_empty()
            && assigned.trim() == assigned
    })
}

fn valid_coordination_deadline_assignment(value: &String) -> bool {
    if !valid_coordination_assignment(value) {
        return false;
    }
    value
        .split_once('=')
        .is_some_and(|(_, timestamp)| timestamp.parse::<Timestamp>().is_ok())
}

fn normalize_coordination_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn validate_query_control_candidate(
    connection: &rusqlite::Connection,
    task_id: &str,
    candidate: &ProductionQueryControlCandidate,
) -> Result<(), TenderCommandError> {
    let task = load_task(connection, task_id)?;
    let query_inputs = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "tender_query_version")
        .collect::<Vec<_>>();
    if query_inputs.len() != 1
        || !agent_query_publication_is_valid(
            connection,
            task_id,
            &[],
            &candidate.query_updates,
            true,
        )?
        || candidate.query_updates.is_empty()
        || candidate.query_updates.len() > 16
        || candidate.query_updates.iter().any(|update| {
            update.query_id != query_inputs[0].reference
                || update.base_version != query_inputs[0].version
                || (update.added_evidence.is_empty()
                    && update.proposed_treatments.is_empty()
                    && update.response.is_none())
                || update
                    .added_evidence
                    .iter()
                    .chain(update.response_evidence.iter())
                    .any(|reference| !task.exact_inputs.contains(reference))
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn validate_review_candidate(
    transaction: &rusqlite::Connection,
    production_task_id: &str,
    candidate: &ProductionReviewCandidate,
    artifact_payload_json: &str,
    target_version: u32,
) -> Result<(), TenderCommandError> {
    if candidate.findings.len() > MAX_PRODUCTION_FINDINGS {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let artifact: ProductionArtifactPayload = parse_canonical_json(artifact_payload_json)?;
    let evidence = artifact
        .evidence_references
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let new_blocking_count = candidate
        .findings
        .iter()
        .filter(|finding| finding.severity.blocks_integration())
        .count();
    let has_blocking = new_blocking_count > 0;
    let resolved = candidate
        .resolved_finding_ids
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut statement = transaction
        .prepare(
            "SELECT findings.finding_id
             FROM production_review_findings AS findings
             JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
             LEFT JOIN production_finding_dispositions AS dispositions
               ON dispositions.finding_id = findings.finding_id
             WHERE reviews.production_task_id = ?1
               AND findings.severity IN ('critical', 'major')
               AND dispositions.finding_id IS NULL
             ORDER BY reviews.rowid, findings.finding_sequence",
        )
        .map_err(sql_error)?;
    let open = statement
        .query_map([production_task_id], |row| row.get::<_, String>(0))
        .map_err(sql_error)?
        .collect::<Result<std::collections::BTreeSet<_>, _>>()
        .map_err(sql_error)?;
    let unresolved_prior_count = open.difference(&resolved).count();
    let unresolved_prior = unresolved_prior_count > 0;
    let query_remediation: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM production_artifact_versions AS artifacts
               JOIN agent_runs AS runs ON runs.run_id = artifacts.author_run_id
               JOIN tender_tasks AS tasks ON tasks.task_id = runs.task_id
               JOIN json_each(tasks.exact_inputs_json) AS input
               WHERE artifacts.production_task_id = ?1 AND artifacts.version = ?2
                 AND json_extract(input.value, '$.kind') = 'tender_query_version'
             )",
            params![production_task_id, target_version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if candidate.resolved_finding_ids.len() > MAX_PRODUCTION_FINDINGS
        || candidate.resolved_finding_ids.len() != resolved.len()
        || !resolved.is_subset(&open)
        || unresolved_prior_count.saturating_add(new_blocking_count) > MAX_PRODUCTION_FINDINGS
        || (target_version == 1 && !resolved.is_empty())
        || (target_version > 1
            && open.is_empty()
            && artifact.query_treatment_applications.is_empty()
            && !query_remediation)
        || (candidate.result == ProductionReviewResult::Satisfied
            && (has_blocking || unresolved_prior))
        || (candidate.result == ProductionReviewResult::RequiresRemediation
            && !has_blocking
            && !unresolved_prior)
        || candidate.findings.iter().any(|finding| {
            finding.summary.trim().is_empty()
                || finding.summary.len() > 4_000
                || finding.evidence_references.is_empty()
                || finding.evidence_references.len() > MAX_PRODUCTION_EVIDENCE_REFERENCES
                || finding
                    .evidence_references
                    .iter()
                    .any(|reference| !evidence.contains(reference))
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn load_exact_production_review_target(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    task_id: &str,
) -> Result<Option<StoredProductionReviewTarget>, TenderCommandError> {
    let task = load_task(connection, task_id)?;
    let mut artifact_inputs = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "production_artifact_version");
    let Some(input) = artifact_inputs.next() else {
        return Ok(None);
    };
    if artifact_inputs.next().is_some() {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT artifact_id, version, payload_sha256, payload_json, author_run_id
             FROM production_artifact_versions
             WHERE production_task_id = ?1 AND artifact_id = ?2 AND version = ?3
               AND version = (
                 SELECT MAX(candidate.version) FROM production_artifact_versions AS candidate
                 WHERE candidate.production_task_id = ?1
               )",
            params![production_task_id, input.reference, input.version],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)
}

fn production_evidence_reference(input: &AgentTaskInputReference) -> String {
    format!("{}:{}:{}", input.kind, input.reference, input.version)
}

fn production_review_inputs_are_valid(
    connection: &rusqlite::Connection,
    definition: &WorkPlanTask,
    review: &ProductionReview,
    inputs: &[AgentTaskInputReference],
) -> Result<bool, TenderCommandError> {
    let unique = inputs
        .iter()
        .map(|input| (&input.kind, &input.reference, input.version))
        .collect::<std::collections::HashSet<_>>();
    if unique.len() != inputs.len()
        || inputs
            .iter()
            .filter(|input| input.kind == "production_artifact_version")
            .count()
            != 1
        || !inputs.iter().any(|input| {
            input.kind == "production_artifact_version"
                && input.reference == review.target_artifact_id
                && input.version == review.target_version
        })
        || inputs.iter().any(|input| {
            !matches!(
                input.kind.as_str(),
                "production_artifact_version" | "tender_query_version" | "approved_query_treatment"
            )
        })
    {
        return Ok(false);
    }
    for query_input in inputs
        .iter()
        .filter(|input| input.kind == "tender_query_version")
    {
        let attributable: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM tender_query_versions AS versions
                   JOIN json_each(versions.affected_task_keys_json) AS affected
                   WHERE versions.query_id = ?1 AND versions.version = ?2
                     AND (affected.value = ?3 OR affected.value = '*')
                   UNION ALL
                   SELECT 1 FROM tender_query_target_invalidations AS invalidations
                   WHERE invalidations.query_id = ?1 AND invalidations.query_version = ?2
                     AND invalidations.target_kind = 'artifact'
                     AND invalidations.target_id = ?4 AND invalidations.target_version = ?5
                   UNION ALL
                   SELECT 1 FROM tender_query_target_invalidations AS invalidations
                   JOIN production_artifact_versions AS artifacts
                     ON artifacts.production_task_id = invalidations.target_id
                   WHERE invalidations.query_id = ?1 AND invalidations.query_version = ?2
                     AND invalidations.target_kind = 'production_task'
                     AND artifacts.artifact_id = ?4 AND artifacts.version = ?5
                  )",
                params![
                    query_input.reference,
                    query_input.version,
                    definition.task_key,
                    review.target_artifact_id,
                    review.target_version,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !attributable {
            return Ok(false);
        }
    }
    for decision_input in inputs
        .iter()
        .filter(|input| input.kind == "approved_query_treatment")
    {
        let query: Option<String> = connection
            .query_row(
                "SELECT query_id FROM tender_query_treatment_decisions
                 WHERE decision_id = ?1 AND query_version = ?2",
                params![decision_input.reference, decision_input.version],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if query.is_none_or(|query_id| {
            !inputs.iter().any(|input| {
                input.kind == "tender_query_version"
                    && input.reference == query_id
                    && input.version == decision_input.version
            })
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn production_review_scope(definition: &WorkPlanTask) -> Result<Vec<String>, TenderCommandError> {
    let scope = vec![
        format!("task:{}", definition.task_key),
        format!("workstream:{}", definition.workstream_key),
        format!("milestone:{}", definition.milestone),
    ];
    if scope.len() > MAX_PRODUCTION_REVIEW_SCOPE_ITEMS {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(scope)
}

fn production_review_criteria(
    definition: &WorkPlanTask,
) -> Result<Vec<String>, TenderCommandError> {
    let mut criteria = vec![
        "exact_target_bytes_unchanged".into(),
        "approved_output_contract_satisfied".into(),
        "evidence_references_verified".into(),
        "approved_scope_and_dependencies_satisfied".into(),
        format!("objective:{}", definition.objective),
    ];
    if definition.major_finding_policy == MajorFindingPolicy::EngineerExceptionAllowed {
        criteria.push("major_exception_requires_engineer_approval".into());
    }
    if criteria.len() > MAX_PRODUCTION_REVIEW_CRITERIA
        || criteria.iter().any(|criterion| criterion.len() > 4_000)
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(criteria)
}

fn production_review_capability(
    definition: &WorkPlanTask,
    reviewer: &crate::agent_runtime::AgentProfileVersionView,
) -> Result<String, TenderCommandError> {
    let capability = reviewer
        .capabilities
        .iter()
        .find(|capability| capability.starts_with("review_"))
        .cloned()
        .or_else(|| {
            reviewer
                .capabilities
                .iter()
                .find(|capability| capability.as_str() == "independent_review")
                .cloned()
        })
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if definition.review_profile_id.as_deref() != Some(reviewer.profile_id.as_str()) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(capability)
}

struct RemediationDispositionContext<'a> {
    tender_id: &'a TenderId,
    production_task_id: &'a str,
    artifact_id: &'a str,
    artifact_version: u32,
    verifying_review_id: &'a str,
    task_ready_for_integration: bool,
    created_at: &'a str,
}

fn insert_remediation_dispositions(
    transaction: &Transaction<'_>,
    context: RemediationDispositionContext<'_>,
    finding_ids: &[String],
) -> Result<(), TenderCommandError> {
    if finding_ids.len() > MAX_PRODUCTION_FINDINGS * MAX_PRODUCTION_TASK_ATTEMPTS as usize {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let tender_revision: u32 = transaction
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    for (index, finding_id) in finding_ids.iter().enumerate() {
        let source_review_id: String = transaction
            .query_row(
                "SELECT review_id FROM production_review_findings WHERE finding_id = ?1",
                [finding_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let disposition_id = random_identifier(transaction)?;
        let rationale = "A new immutable Artifact Version was independently reviewed as satisfied.";
        let consequence =
            "The prior finding remains visible and is linked to the verified remediation.";
        let audit_sequence = append_audit_event_with_sequence(
            transaction,
            context.tender_id.as_str(),
            "production_finding_remediation_verified",
            tender_revision,
            json!({
                "acting_role": "integration_gate",
                "artifact_id": context.artifact_id,
                "artifact_version": context.artifact_version.to_string(),
                "consequence": consequence,
                "decided_by": "host_policy",
                "disposition": "remediation_verified",
                "disposition_id": disposition_id.clone(),
                "finding_id": finding_id,
                "production_task_id": context.production_task_id,
                "rationale": rationale,
                "review_id": source_review_id,
                "task_ready_for_integration": context.task_ready_for_integration
                    && index + 1 == finding_ids.len(),
                "verifying_review_id": context.verifying_review_id,
            }),
            context.created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO production_finding_dispositions (
                   disposition_id, finding_id, audit_sequence, disposition,
                   target_artifact_id, target_version, verifying_review_id,
                   decided_by, acting_role, rationale, consequence, created_at
                 ) VALUES (?1, ?2, ?3, 'remediation_verified', ?4, ?5, ?6,
                           'host_policy', 'integration_gate', ?7, ?8, ?9)",
                params![
                    disposition_id,
                    finding_id,
                    audit_sequence,
                    context.artifact_id,
                    context.artifact_version,
                    context.verifying_review_id,
                    rationale,
                    consequence,
                    context.created_at,
                ],
            )
            .map_err(sql_error)?;
    }
    Ok(())
}

fn insert_production_integration_readiness(
    transaction: &Transaction<'_>,
    production_task_id: &str,
    artifact_id: &str,
    artifact_version: u32,
    payload_sha256: &str,
    review_id: Option<&str>,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let task_key: String = transaction
        .query_row(
            "SELECT task_key FROM production_tasks WHERE production_task_id = ?1",
            [production_task_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if task_has_blocking_query(transaction, &task_key)?
        || !production_artifact_applies_current_query_treatments(
            transaction,
            &task_key,
            artifact_id,
            artifact_version,
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let gates_hold: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1
               FROM production_tasks AS tasks
               JOIN production_activations AS activations
                 ON activations.activation_id = tasks.activation_id
               JOIN work_plan_heads AS heads ON heads.plan_id = activations.plan_id
               JOIN work_plan_approvals AS plan_approvals
                 ON plan_approvals.plan_id = activations.plan_id
                AND plan_approvals.plan_version = activations.plan_version
               JOIN work_plan_versions AS plans
                 ON plans.plan_id = activations.plan_id
                AND plans.version = activations.plan_version
               JOIN bid_decision_approval_records AS bid_approvals
                 ON bid_approvals.package_id = plans.bid_package_id
                AND bid_approvals.package_version = plans.bid_package_version
               JOIN tender ON tender.singleton = 1
               LEFT JOIN bid_decision_approval_invalidations AS invalidations
                 ON invalidations.approval_id = bid_approvals.approval_id
               WHERE tasks.production_task_id = ?1
                 AND activations.status = 'active'
                 AND heads.current_version = activations.plan_version
                 AND plan_approvals.decision = 'approve'
                 AND bid_approvals.decision = 'accept'
                 AND invalidations.approval_id IS NULL
                 AND tender.lifecycle_phase = 'active_production'
             )",
            [production_task_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let dependencies_satisfied: bool = transaction
        .query_row(
            "SELECT NOT EXISTS(
               SELECT 1
               FROM production_tasks AS task
               JOIN json_each(task.task_definition_json, '$.dependencies') AS dependency
               LEFT JOIN production_tasks AS prerequisite
                 ON prerequisite.activation_id = task.activation_id
                AND prerequisite.task_key = dependency.value
               WHERE task.production_task_id = ?1
                 AND (prerequisite.production_task_id IS NULL
                      OR prerequisite.status != 'ready_for_integration')
             )",
            [production_task_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let blocking_open: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM production_review_findings AS findings
               JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
               LEFT JOIN production_finding_dispositions AS dispositions
                 ON dispositions.finding_id = findings.finding_id
               WHERE reviews.production_task_id = ?1
                 AND findings.severity IN ('critical', 'major')
                 AND dispositions.finding_id IS NULL
             )",
            [production_task_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if !gates_hold || !dependencies_satisfied || blocking_open {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let disposition_sha256 = production_finding_dispositions_sha256_at(
        transaction,
        production_task_id,
        artifact_version,
    )?;
    let approval_gates = vec![
        "bid_decision_accepted".to_owned(),
        "work_plan_approved".to_owned(),
        "production_activation_active".to_owned(),
        "dependencies_ready_for_integration".to_owned(),
        "approved_query_treatments_applied".to_owned(),
    ];
    transaction
        .execute(
            "INSERT INTO production_integration_readiness (
               readiness_id, production_task_id, artifact_id, artifact_version,
               payload_sha256, review_id, output_validation_passed, evidence_verified,
               dependencies_satisfied, approval_gates_json,
               finding_dispositions_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 1, 1, ?7, ?8, ?9)",
            params![
                random_identifier(transaction)?,
                production_task_id,
                artifact_id,
                artifact_version,
                payload_sha256,
                review_id,
                canonical_json(&approval_gates)?,
                disposition_sha256,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn production_artifact_applies_current_query_treatments(
    connection: &rusqlite::Connection,
    task_key: &str,
    artifact_id: &str,
    artifact_version: u32,
) -> Result<bool, TenderCommandError> {
    let (payload_json, author_run_id, author_task_id): (String, String, String) = connection
        .query_row(
            "SELECT artifacts.payload_json, artifacts.author_run_id, runs.task_id
             FROM production_artifact_versions AS artifacts
             JOIN agent_runs AS runs ON runs.run_id = artifacts.author_run_id
             WHERE artifacts.artifact_id = ?1 AND artifacts.version = ?2",
            params![artifact_id, artifact_version],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    let payload: ProductionArtifactPayload = parse_canonical_json(&payload_json)?;
    let author_inputs = load_task(connection, &author_task_id)?.exact_inputs;
    let query_contexts = production_query_contexts_for_task(connection, task_key)?;
    if query_contexts.iter().any(|context| {
        context.source_run_id.as_deref() != Some(author_run_id.as_str())
            && !author_inputs.iter().any(|input| {
                input.kind == "tender_query_version"
                    && input.reference == context.query_id
                    && input.version == context.query_version
            })
    }) {
        return Ok(false);
    }
    let expected = approved_query_treatments_for_inputs(connection, &author_inputs)?
        .into_iter()
        .filter(|decision| decision.treatment.permits_dependent_work())
        .collect::<Vec<_>>();
    if payload.query_treatment_applications.len() != expected.len() {
        return Ok(false);
    }
    Ok(expected.iter().all(|decision| {
        payload
            .query_treatment_applications
            .iter()
            .any(|application| {
                application.decision_id == decision.decision_id
                    && application.query_id == decision.query_id
                    && application.query_version == decision.query_version
                    && application.treatment == decision.treatment
            })
    }))
}

fn refresh_ready_frontier(
    transaction: &Transaction<'_>,
    activation_id: &str,
    updated_at: &str,
) -> Result<(), TenderCommandError> {
    let mut statement = transaction
        .prepare(
            "SELECT production_task_id, task_definition_json FROM production_tasks
             WHERE activation_id = ?1 AND status = 'blocked' ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let blocked = statement
        .query_map([activation_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    for (production_task_id, task_json) in blocked {
        let task: WorkPlanTask = parse_canonical_json(&task_json)?;
        let mut ready = true;
        for dependency in &task.dependencies {
            if !transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM production_tasks
                       WHERE activation_id = ?1 AND task_key = ?2 AND status = 'ready_for_integration'
                     )",
                    params![activation_id, dependency],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
            {
                ready = false;
                break;
            }
        }
        if ready {
            if task_has_blocking_query(transaction, &task.task_key)? {
                transaction
                    .execute(
                        "UPDATE production_tasks SET status = 'query_blocked', updated_at = ?2
                         WHERE production_task_id = ?1 AND status = 'blocked'",
                        params![production_task_id, updated_at],
                    )
                    .map_err(sql_error)?;
                continue;
            }
            transaction
                .execute(
                    "UPDATE production_tasks SET status = 'ready', updated_at = ?2
                     WHERE production_task_id = ?1 AND status = 'blocked'",
                    params![production_task_id, updated_at],
                )
                .map_err(sql_error)?;
        }
    }
    Ok(())
}

fn append_production_denial(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    command: &str,
    production_task_id: Option<&str>,
    reason: &str,
) -> Result<(), TenderCommandError> {
    let tender_revision: u32 = transaction
        .query_row(
            "SELECT current_revision FROM tender
             WHERE singleton = 1 AND tender_id = ?1",
            [tender_id.as_str()],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let created_at = sqlite_timestamp(transaction)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "production_command_denied",
        tender_revision,
        json!({
            "command": command,
            "production_task_id": production_task_id,
            "reason": reason,
        }),
        &created_at,
    )
}

fn subscription_capacity_is_exhausted(
    transaction: &Transaction<'_>,
) -> Result<bool, TenderCommandError> {
    let usage_json: Option<String> = transaction
        .query_row(
            "SELECT usage_json FROM agent_runs
             WHERE usage_json IS NOT NULL
               AND json_extract(usage_json, '$.rate_limit.state') IS NOT NULL
             ORDER BY run_sequence DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(usage_json) = usage_json else {
        return Ok(false);
    };
    let usage: ProviderUsage = parse_canonical_json(&usage_json)?;
    let Some(rate_limit) = usage.rate_limit else {
        return Ok(false);
    };
    if rate_limit.state == ProviderRateLimitState::Available {
        return Ok(false);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
        .as_secs();
    let now = i64::try_from(now)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let reset_times = [rate_limit.primary.as_ref(), rate_limit.secondary.as_ref()]
        .into_iter()
        .flatten()
        .map(|window| window.resets_at_epoch_seconds)
        .collect::<Vec<_>>();
    Ok(reset_times.is_empty()
        || reset_times
            .into_iter()
            .any(|reset_at| reset_at.is_none_or(|reset_at| reset_at > now)))
}

fn audit_event_matches_production_task(
    connection: &rusqlite::Connection,
    event_type: &str,
    production_task_id: &str,
    run_id: &str,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM audit_events
               WHERE event_type = ?1
                 AND json_extract(payload_json, '$.change.production_task_id') = ?2
                 AND json_extract(payload_json, '$.change.run_id') = ?3
             )",
            params![event_type, production_task_id, run_id],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn production_review_output_contract() -> Result<String, TenderCommandError> {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "findings": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "evidence_references": {
                            "items": { "maxLength": 400, "minLength": 1, "type": "string" },
                            "maxItems": MAX_PRODUCTION_EVIDENCE_REFERENCES,
                            "minItems": 1,
                            "type": "array"
                        },
                        "severity": { "enum": ["critical", "major", "minor"], "type": "string" },
                        "summary": { "maxLength": 4000, "minLength": 1, "type": "string" }
                    },
                    "required": ["severity", "summary", "evidence_references"],
                    "type": "object"
                },
                "maxItems": 32,
                "type": "array"
            },
            "resolved_finding_ids": {
                "items": { "maxLength": 32, "minLength": 32, "type": "string" },
                "maxItems": MAX_PRODUCTION_FINDINGS,
                "type": "array"
            },
            "result": { "enum": ["satisfied", "requires_remediation"], "type": "string" }
        },
        "required": ["result", "resolved_finding_ids", "findings"],
        "type": "object"
    }))
}

fn production_query_control_output_contract() -> Result<String, TenderCommandError> {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "query_updates": {
                "items": { "type": "object" },
                "maxItems": 16,
                "minItems": 1,
                "type": "array"
            }
        },
        "required": ["query_updates"],
        "type": "object"
    }))
}

fn production_remediation_output_contract() -> Result<String, TenderCommandError> {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "coordination_observations": {
                "description": "Emit every subject in coordination_contract.required_subjects. For each entry in coordination_contract.assignment_contracts, emit every required key exactly once as key=value across one or more observations; copy Host source observations exactly unless the artifact must surface a contradiction.",
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "subject": {
                            "enum": ["submission_deadline", "responsible_party", "scope_qualification", "scope_exclusion", "expected_delivery_cost", "approved_tender_price", "commercial_appetite", "technical_commitment", "programme_commitment", "procurement_commitment", "contractual_commitment", "risk_commitment", "submission_commitment", "query_treatment"],
                            "type": "string"
                        },
                        "evidence_references": {
                            "items": { "maxLength": 400, "minLength": 1, "type": "string" },
                            "maxItems": MAX_PRODUCTION_EVIDENCE_REFERENCES,
                            "minItems": 1,
                            "type": "array"
                        },
                        "value": {
                            "additionalProperties": false,
                            "properties": {
                                "currency": { "maxLength": 3, "minLength": 3, "type": "string" },
                                "kind": { "enum": ["text", "amount", "text_set"], "type": "string" },
                                "text": { "maxLength": 4000, "minLength": 1, "type": "string" },
                                "value": { "maxLength": 100, "minLength": 1, "type": "string" },
                                "values": {
                                    "items": {
                                        "description": "A Host-authorized lowercase coordination key followed by '=' and the exact current value.",
                                        "maxLength": 4608,
                                        "minLength": 3,
                                        "pattern": "^[a-z0-9_.:-]{1,512}=.+$",
                                        "type": "string"
                                    },
                                    "maxItems": 32,
                                    "type": "array"
                                }
                            },
                            "required": ["kind"],
                            "type": "object"
                        }
                    },
                    "required": ["subject", "value", "evidence_references"],
                    "type": "object"
                },
                "maxItems": 32,
                "minItems": 1,
                "type": "array"
            },
            "evidence_references": {
                "items": { "maxLength": 400, "minLength": 1, "type": "string" },
                "maxItems": MAX_PRODUCTION_EVIDENCE_REFERENCES,
                "minItems": 1,
                "type": "array"
            },
            "gaps": {
                "items": { "maxLength": 4000, "minLength": 1, "type": "string" },
                "maxItems": 0,
                "type": "array"
            },
            "query_proposals": {
                "items": { "type": "object" },
                "maxItems": 16,
                "type": "array"
            },
            "query_treatment_applications": {
                "items": { "type": "object" },
                "maxItems": 64,
                "type": "array"
            },
            "query_updates": {
                "items": { "type": "object" },
                "maxItems": 16,
                "type": "array"
            },
            "remediations": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "evidence_references": {
                            "items": { "maxLength": 400, "minLength": 1, "type": "string" },
                            "maxItems": MAX_PRODUCTION_EVIDENCE_REFERENCES,
                            "minItems": 1,
                            "type": "array"
                        },
                        "finding_id": { "maxLength": 32, "minLength": 32, "type": "string" },
                        "treatment": { "maxLength": 4000, "minLength": 1, "type": "string" }
                    },
                    "required": ["finding_id", "treatment", "evidence_references"],
                    "type": "object"
                },
                "maxItems": MAX_PRODUCTION_FINDINGS,
                "type": "array"
            },
            "summary": { "maxLength": 4000, "minLength": 1, "type": "string" }
        },
        "required": ["summary", "evidence_references", "gaps", "coordination_observations"],
        "type": "object"
    }))
}

pub(super) fn work_plan_package_dependencies_are_current(
    store: &TenderStore,
    plan_id: &str,
    plan_version: u32,
) -> Result<bool, TenderCommandError> {
    let basis: Option<(String, u32, u32, String)> = store
        .connection
        .query_row(
            "SELECT plans.bid_package_id, plans.bid_package_version,
                    packages.tender_revision, packages.record_inventory_sha256
             FROM work_plan_versions AS plans
             JOIN bid_decision_package_versions AS packages
               ON packages.package_id = plans.bid_package_id
              AND packages.version = plans.bid_package_version
              AND packages.manifest_sha256 = plans.bid_package_manifest_sha256
             WHERE plans.plan_id = ?1 AND plans.version = ?2",
            params![plan_id, plan_version],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let Some((package_id, package_version, tender_revision, inventory_sha256)) = basis else {
        return Ok(false);
    };
    package_dependencies_are_current(
        store,
        &package_id,
        package_version,
        tender_revision,
        &inventory_sha256,
    )
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))
}

fn parse_canonical_json<T>(value: &str) -> Result<T, TenderCommandError>
where
    T: DeserializeOwned + Serialize,
{
    let parsed = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
        != value
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

#[cfg(test)]
mod coordination_contract_tests {
    use super::*;

    fn contract_with_assignments(count: usize, value_len: usize) -> ProductionCoordinationContract {
        let required_keys = (0..count)
            .map(|index| format!("source_{index:04}"))
            .collect::<Vec<_>>();
        let source_observations = required_keys
            .iter()
            .map(|key| ProductionCoordinationSourceObservation {
                subject: ProductionCoordinationObservationSubject::TechnicalCommitment,
                value: ProductionCoordinationObservationValue::TextSet {
                    values: vec![format!("{key}={}", "v".repeat(value_len))],
                },
                reference: "tender_record_version:00000000000000000000000000000000:1".into(),
            })
            .collect();
        ProductionCoordinationContract {
            required_subjects: vec![ProductionCoordinationObservationSubject::TechnicalCommitment],
            assignment_contracts: vec![ProductionCoordinationAssignmentContract {
                subject: ProductionCoordinationObservationSubject::TechnicalCommitment,
                required_keys,
            }],
            source_observations,
        }
    }

    #[test]
    fn coordination_contract_accepts_exact_assignment_capacity_and_rejects_overflow() {
        assert!(
            coordination_contract_is_representable(&contract_with_assignments(
                MAX_PRODUCTION_COORDINATION_ASSIGNMENTS,
                1,
            ))
            .expect("exact coordination capacity")
        );
        assert!(
            !coordination_contract_is_representable(&contract_with_assignments(
                MAX_PRODUCTION_COORDINATION_ASSIGNMENTS + 1,
                1,
            ))
            .expect("overflowing coordination capacity")
        );
    }

    #[test]
    fn coordination_contract_rejects_the_first_output_byte_overflow() {
        let boundary = (1..=MAX_PRODUCTION_COORDINATION_ASSIGNMENTS)
            .find(|count| {
                !coordination_contract_is_representable(&contract_with_assignments(*count, 4_000))
                    .expect("coordination byte boundary")
            })
            .expect("bounded output byte ceiling");
        assert!(boundary > 1);
        assert!(
            coordination_contract_is_representable(
                &contract_with_assignments(boundary - 1, 4_000,)
            )
            .expect("last representable coordination byte payload")
        );
    }

    #[test]
    fn coordination_keys_preserve_hyphen_and_underscore_identity() {
        assert_ne!(
            coordination_assignment_key("fire-rating", "value"),
            coordination_assignment_key("fire_rating", "value")
        );
    }

    #[test]
    fn remediation_contract_instructions_name_the_serialized_coordination_fields() {
        let contract = production_remediation_output_contract().expect("remediation contract");
        assert!(contract.contains("coordination_contract.required_subjects"));
        assert!(contract.contains("coordination_contract.assignment_contracts"));
    }
}
