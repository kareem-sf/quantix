use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    str::FromStr,
};

use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tempfile::NamedTempFile;
use ts_rs::TS;

use crate::agent_runtime::{
    permissions::{derive_planned_task_grant, permission_duration, PlannedTaskGrantRequest},
    AgentProfileVersionView, AgentRunInspection, AgentTaskInputReference, DataClassification,
    PendingProviderEvent, PermissionGrant, PreparedAgentRun, ProviderEventKind, TenderTaskView,
    ThreadExposureSet,
};

use super::{
    agent_records::{
        ensure_agent_run_capacity, insert_event, insert_task, load_profile, load_task,
        load_thread_exposure,
    },
    append_audit_event, append_audit_event_with_sequence, lock_mutex_with_check,
    metadata_is_unsafe_storage_link, random_identifier, sha256_hex, sql_error, sqlite_timestamp,
    store_unavailable,
    tender_queries::{query_evidence_reference_exists, ExternalRfiInterpretationInput},
    BidPackageOperationBudget, QuantixHost, TenderCommandError, TenderErrorCode, TenderId,
    TenderStore, WorkPlanProfileBinding,
};

pub(crate) const EXTERNAL_RFI_REVIEW_QUALIFICATION: &str = "review_query_rfi_control";
const MAX_EXTERNAL_RFIS: u32 = 256;
const MAX_EXTERNAL_RFI_VERSIONS: u32 = 32;
const MAX_RFI_QUERY_REFS: usize = 16;
const MAX_RFI_SOURCE_EVIDENCE: usize = 128;
const MAX_RFI_ATTACHMENTS: usize = 32;
const MAX_RFI_AFFECTED_TASKS: usize = 256;
const MAX_RFI_COMMITMENTS: usize = 64;
const MAX_RFI_FINDINGS: usize = 64;
const MAX_RFI_PAGE_ITEMS: u32 = 8;
const MAX_RFI_RECORDS_PER_APPROVAL: u32 = 64;
const MAX_RFI_REVIEW_DATA_VIEW_BYTES: usize = 4 * 1024 * 1024;
const RFI_REVIEW_GRANT_METADATA_RESERVE_BYTES: usize = 16 * 1024;
const MAX_EXPORT_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiQueryReference {
    pub query_id: String,
    pub version: u32,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiQuestion {
    pub query_id: String,
    pub query_version: u32,
    pub question: String,
    pub ambiguity_or_gap: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ExternalRfiRecipient {
    pub organization: String,
    pub attention: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExternalRfiReviewOutcome {
    Passed,
    Failed,
}

impl ExternalRfiReviewOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExternalRfiFindingSeverity {
    Critical,
    Major,
    Minor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ExternalRfiReviewFinding {
    pub severity: ExternalRfiFindingSeverity,
    pub code: String,
    pub summary: String,
    pub evidence_references: Vec<AgentTaskInputReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiReview {
    pub review_id: String,
    pub rfi_id: String,
    pub rfi_version: u32,
    pub rfi_manifest_sha256: String,
    pub reviewer_run_id: String,
    pub reviewer_profile_id: String,
    pub reviewer_profile_version: u32,
    pub outcome: ExternalRfiReviewOutcome,
    pub findings: Vec<ExternalRfiReviewFinding>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiApproval {
    pub approval_id: String,
    pub rfi_id: String,
    pub rfi_version: u32,
    pub rfi_manifest_sha256: String,
    pub review_id: String,
    pub review_manifest_sha256: String,
    pub rationale: String,
    pub approved_by: String,
    pub acting_role: String,
    pub approval_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiExportRecord {
    pub export_id: String,
    pub approval_id: String,
    pub path: String,
    pub bytes_sha256: String,
    pub size_bytes: u64,
    pub bytes_verified: bool,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiResponseLink {
    pub response_link_id: String,
    pub rfi_id: String,
    pub rfi_version: u32,
    pub approval_id: String,
    pub source_artifact_id: String,
    pub source_artifact_version: u32,
    pub registered_by: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiResponseInterpretation {
    pub interpretation_id: String,
    pub response_link_id: String,
    pub query_id: String,
    pub source_query_version: u32,
    pub base_query_version: u32,
    pub resulting_query_version: u32,
    pub query_decision_id: String,
    pub material: bool,
    pub interpretation: String,
    pub decided_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiDraft {
    pub rfi_id: String,
    pub version: u32,
    pub query_refs: Vec<ExternalRfiQueryReference>,
    pub current_query_refs: Vec<ExternalRfiQueryReference>,
    pub questions: Vec<ExternalRfiQuestion>,
    pub source_evidence: Vec<AgentTaskInputReference>,
    pub contractual_context: String,
    pub response_need: String,
    pub attachments: Vec<AgentTaskInputReference>,
    pub due_at: String,
    pub recipient: ExternalRfiRecipient,
    pub affected_task_keys: Vec<String>,
    pub affected_commitments: Vec<String>,
    pub review: Option<ExternalRfiReview>,
    pub approval: Option<ExternalRfiApproval>,
    pub exports: Vec<ExternalRfiExportRecord>,
    pub responses: Vec<ExternalRfiResponseLink>,
    pub interpretations: Vec<ExternalRfiResponseInterpretation>,
    pub current: bool,
    pub evidence_current: bool,
    pub revision_allowed: bool,
    pub approved_for_issue: bool,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiPage {
    pub items: Vec<ExternalRfiDraft>,
    pub next_cursor: Option<String>,
    pub total_current_count: u32,
    pub approved_for_issue_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiResponseCandidate {
    pub source_artifact_id: String,
    pub source_artifact_version: u32,
    pub package_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiResponseCandidatePage {
    pub items: Vec<ExternalRfiResponseCandidate>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiEligibleQuery {
    pub query_ref: ExternalRfiQueryReference,
    pub question: String,
    pub ambiguity_or_gap: String,
    pub due_at: String,
    pub affected_task_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiEligibleQueryPage {
    pub items: Vec<ExternalRfiEligibleQuery>,
    pub next_cursor: Option<String>,
    pub total_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateExternalRfiDraftCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(skip)]
    pub query_refs: Vec<ExternalRfiQueryReference>,
    #[garde(skip)]
    pub additional_evidence: Vec<AgentTaskInputReference>,
    #[garde(length(bytes, min = 1, max = 8000))]
    pub contractual_context: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub response_need: String,
    #[garde(skip)]
    pub attachments: Vec<AgentTaskInputReference>,
    #[garde(length(bytes, min = 20, max = 32))]
    pub due_at: String,
    #[garde(skip)]
    pub recipient: ExternalRfiRecipient,
    #[garde(skip)]
    pub affected_commitments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ReviseExternalRfiDraftCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub rfi_id: String,
    #[garde(range(min = 1, max = 32))]
    pub base_version: u32,
    #[garde(skip)]
    pub query_refs: Vec<ExternalRfiQueryReference>,
    #[garde(skip)]
    pub additional_evidence: Vec<AgentTaskInputReference>,
    #[garde(length(bytes, min = 1, max = 8000))]
    pub contractual_context: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub response_need: String,
    #[garde(skip)]
    pub attachments: Vec<AgentTaskInputReference>,
    #[garde(length(bytes, min = 20, max = 32))]
    pub due_at: String,
    #[garde(skip)]
    pub recipient: ExternalRfiRecipient,
    #[garde(skip)]
    pub affected_commitments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunExternalRfiReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub rfi_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExternalRfiReviewResult {
    pub run: AgentRunInspection,
    pub rfi: ExternalRfiDraft,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApproveExternalRfiForIssueCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub rfi_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ExportApprovedExternalRfiCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub rfi_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub approval_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RegisterExternalRfiResponseCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub rfi_id: String,
    #[garde(range(min = 1, max = 32))]
    pub rfi_version: u32,
    #[garde(length(bytes, min = 32, max = 32))]
    pub approval_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub source_artifact_id: String,
    #[garde(range(min = 1))]
    pub source_artifact_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InterpretExternalRfiResponseCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub response_link_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub query_id: String,
    #[garde(range(min = 1, max = 32))]
    pub issued_query_version: u32,
    #[garde(range(min = 1, max = 32))]
    pub base_query_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub base_query_manifest_sha256: String,
    #[garde(skip)]
    pub material: bool,
    #[garde(length(bytes, min = 1, max = 8000))]
    pub interpretation: String,
    #[garde(skip)]
    pub treatment: super::TenderQueryTreatment,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub treatment_details: String,
    #[garde(skip)]
    pub closes_query: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectExternalRfisCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, max = 32))]
    pub cursor: Option<String>,
    #[garde(range(min = 1, max = 8))]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectExternalRfiEligibleQueriesCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, max = 32))]
    pub cursor: Option<String>,
    #[garde(range(min = 1, max = 8))]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectExternalRfiResponseCandidatesCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub approval_id: String,
    #[garde(length(bytes, max = 48))]
    pub cursor: Option<String>,
    #[garde(range(min = 1, max = 64))]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExternalRfiReviewCandidate {
    pub outcome: ExternalRfiReviewOutcome,
    pub findings: Vec<ExternalRfiReviewFinding>,
}

#[derive(Serialize)]
struct ExternalRfiVersionManifest<'a> {
    schema_version: u32,
    rfi_id: &'a str,
    version: u32,
    query_refs: &'a [ExternalRfiQueryReference],
    questions: &'a [ExternalRfiQuestion],
    source_evidence: &'a [AgentTaskInputReference],
    contractual_context: &'a str,
    response_need: &'a str,
    attachments: &'a [AgentTaskInputReference],
    due_at: &'a str,
    recipient: &'a ExternalRfiRecipient,
    affected_task_keys: &'a [String],
    affected_commitments: &'a [String],
    created_by: &'a str,
    created_at: &'a str,
}

#[derive(Serialize)]
struct ExternalRfiApprovalManifest<'a> {
    schema_version: u32,
    approval_id: &'a str,
    rfi_id: &'a str,
    rfi_version: u32,
    rfi_manifest_sha256: &'a str,
    review_id: &'a str,
    review_manifest_sha256: &'a str,
    rationale: &'a str,
    approved_by: &'a str,
    acting_role: &'a str,
    created_at: &'a str,
}

#[derive(Serialize)]
struct ExternalRfiReviewManifest<'a> {
    schema_version: u32,
    review_id: &'a str,
    rfi_id: &'a str,
    rfi_version: u32,
    rfi_manifest_sha256: &'a str,
    reviewer_run_id: &'a str,
    reviewer_profile_id: &'a str,
    reviewer_profile_version: u32,
    outcome: ExternalRfiReviewOutcome,
    findings: &'a [ExternalRfiReviewFinding],
    created_at: &'a str,
}

#[derive(Serialize)]
struct ExternalRfiExportManifest<'a> {
    schema_version: u32,
    export_id: &'a str,
    approval_id: &'a str,
    relative_path: &'a str,
    bytes_sha256: &'a str,
    size_bytes: u64,
    created_at: &'a str,
}

#[derive(Serialize)]
struct ExternalRfiResponseManifest<'a> {
    schema_version: u32,
    response_link_id: &'a str,
    rfi_id: &'a str,
    rfi_version: u32,
    approval_id: &'a str,
    source_artifact_id: &'a str,
    source_artifact_version: u32,
    registered_by: &'a str,
    created_at: &'a str,
}

#[derive(Serialize)]
struct ExternalRfiInterpretationManifest<'a> {
    schema_version: u32,
    interpretation_id: &'a str,
    response_link_id: &'a str,
    query_id: &'a str,
    source_query_version: u32,
    base_query_version: u32,
    resulting_query_version: u32,
    query_decision_id: &'a str,
    material: bool,
    interpretation: &'a str,
    decided_by: &'a str,
    acting_role: &'a str,
    created_at: &'a str,
}

#[derive(Clone, Copy)]
struct ExternalRfiCandidate<'a> {
    query_refs: &'a [ExternalRfiQueryReference],
    additional_evidence: &'a [AgentTaskInputReference],
    contractual_context: &'a str,
    response_need: &'a str,
    attachments: &'a [AgentTaskInputReference],
    due_at: &'a str,
    recipient: &'a ExternalRfiRecipient,
    affected_commitments: &'a [String],
}

struct MaterializedExternalRfiCandidate {
    query_refs: Vec<ExternalRfiQueryReference>,
    questions: Vec<ExternalRfiQuestion>,
    source_evidence: Vec<AgentTaskInputReference>,
    affected_task_keys: Vec<String>,
}

fn external_rfi_candidate_preview(
    rfi_id: &str,
    version: u32,
    materialized: &MaterializedExternalRfiCandidate,
    candidate: ExternalRfiCandidate<'_>,
    created_at: &str,
) -> ExternalRfiDraft {
    ExternalRfiDraft {
        rfi_id: rfi_id.into(),
        version,
        query_refs: materialized.query_refs.clone(),
        current_query_refs: materialized.query_refs.clone(),
        questions: materialized.questions.clone(),
        source_evidence: materialized.source_evidence.clone(),
        contractual_context: candidate.contractual_context.trim().into(),
        response_need: candidate.response_need.trim().into(),
        attachments: candidate.attachments.to_vec(),
        due_at: candidate.due_at.into(),
        recipient: candidate.recipient.clone(),
        affected_task_keys: materialized.affected_task_keys.clone(),
        affected_commitments: candidate
            .affected_commitments
            .iter()
            .map(|value| value.trim().to_owned())
            .collect(),
        review: None,
        approval: None,
        exports: Vec::new(),
        responses: Vec::new(),
        interpretations: Vec::new(),
        current: true,
        evidence_current: true,
        revision_allowed: true,
        approved_for_issue: false,
        manifest_sha256: "0".repeat(64),
        created_at: created_at.into(),
    }
}

struct ExternalRfiReviewDraftRow {
    query_refs_json: String,
    questions_json: String,
    source_evidence_json: String,
    contractual_context: String,
    response_need: String,
    attachments_json: String,
    due_at: String,
    recipient_json: String,
    affected_task_keys_json: String,
    affected_commitments_json: String,
    manifest_sha256: String,
    created_at: String,
}

fn external_rfi_review_basis_draft(
    connection: &rusqlite::Connection,
    rfi_id: &str,
    version: u32,
) -> Result<Option<ExternalRfiDraft>, TenderCommandError> {
    let stored: Option<ExternalRfiReviewDraftRow> = connection
        .query_row(
            "SELECT query_refs_json, questions_json, source_evidence_json,
                    contractual_context, response_need, attachments_json, due_at,
                    recipient_json, affected_task_keys_json, affected_commitments_json,
                    manifest_sha256, created_at
             FROM external_rfi_versions WHERE rfi_id = ?1 AND version = ?2",
            params![rfi_id, version],
            |row| {
                Ok(ExternalRfiReviewDraftRow {
                    query_refs_json: row.get(0)?,
                    questions_json: row.get(1)?,
                    source_evidence_json: row.get(2)?,
                    contractual_context: row.get(3)?,
                    response_need: row.get(4)?,
                    attachments_json: row.get(5)?,
                    due_at: row.get(6)?,
                    recipient_json: row.get(7)?,
                    affected_task_keys_json: row.get(8)?,
                    affected_commitments_json: row.get(9)?,
                    manifest_sha256: row.get(10)?,
                    created_at: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)?;
    stored
        .map(|stored| {
            let query_refs: Vec<ExternalRfiQueryReference> =
                parse_canonical(&stored.query_refs_json)?;
            Ok(ExternalRfiDraft {
                rfi_id: rfi_id.into(),
                version,
                current_query_refs: query_refs.clone(),
                query_refs,
                questions: parse_canonical(&stored.questions_json)?,
                source_evidence: parse_canonical(&stored.source_evidence_json)?,
                contractual_context: stored.contractual_context,
                response_need: stored.response_need,
                attachments: parse_canonical(&stored.attachments_json)?,
                due_at: stored.due_at,
                recipient: parse_canonical(&stored.recipient_json)?,
                affected_task_keys: parse_canonical(&stored.affected_task_keys_json)?,
                affected_commitments: parse_canonical(&stored.affected_commitments_json)?,
                review: None,
                approval: None,
                exports: Vec::new(),
                responses: Vec::new(),
                interpretations: Vec::new(),
                current: true,
                evidence_current: true,
                revision_allowed: true,
                approved_for_issue: false,
                manifest_sha256: stored.manifest_sha256,
                created_at: stored.created_at,
            })
        })
        .transpose()
}

impl QuantixHost {
    pub fn create_external_rfi_draft(
        &self,
        command: CreateExternalRfiDraftCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_external_rfi_denial(
                &tender_id,
                "create_external_rfi_draft",
                None,
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.create_external_rfi_draft(&tender_id, &command, budget)
    }

    pub fn revise_external_rfi_draft(
        &self,
        command: ReviseExternalRfiDraftCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_external_rfi_denial(
                &tender_id,
                "revise_external_rfi_draft",
                Some(&command.rfi_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.revise_external_rfi_draft(&tender_id, &command, budget)
    }

    pub fn inspect_external_rfis(
        &self,
        command: InspectExternalRfisCommand,
    ) -> Result<ExternalRfiPage, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_external_rfis(&command, budget);
        result
    }

    pub fn inspect_external_rfi_eligible_queries(
        &self,
        command: InspectExternalRfiEligibleQueriesCommand,
    ) -> Result<ExternalRfiEligibleQueryPage, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_external_rfi_eligible_queries(&command, budget);
        result
    }

    pub fn inspect_external_rfi_response_candidates(
        &self,
        command: InspectExternalRfiResponseCandidatesCommand,
    ) -> Result<ExternalRfiResponseCandidatePage, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_external_rfi_response_candidates(&command, budget);
        result
    }

    pub fn approve_external_rfi_for_issue(
        &self,
        command: ApproveExternalRfiForIssueCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_external_rfi_denial(
                &tender_id,
                "approve_external_rfi_for_issue",
                Some(&command.rfi_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.approve_external_rfi_for_issue(&tender_id, &command, budget)
    }

    pub fn export_approved_external_rfi(
        &self,
        command: ExportApprovedExternalRfiCommand,
    ) -> Result<ExternalRfiExportRecord, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .export_approved_external_rfi(self.application_home(), &tender_id, &command, budget);
        result
    }

    pub fn register_external_rfi_response(
        &self,
        command: RegisterExternalRfiResponseCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_external_rfi_denial(
                &tender_id,
                "register_external_rfi_response",
                Some(&command.rfi_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.register_external_rfi_response(&tender_id, &command, budget)
    }

    pub fn interpret_external_rfi_response(
        &self,
        command: InterpretExternalRfiResponseCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_external_rfi_denial(
                &tender_id,
                "interpret_external_rfi_response",
                Some(&command.response_link_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.interpret_external_rfi_response(&tender_id, &command, budget)
    }
}

fn external_rfi_review_output_contract() -> String {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "findings": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "code": { "minLength": 1, "maxLength": 100, "type": "string" },
                        "evidence_references": {
                            "items": {
                                "additionalProperties": false,
                                "properties": {
                                    "kind": { "minLength": 1, "maxLength": 100, "type": "string" },
                                    "reference": { "minLength": 1, "maxLength": 200, "type": "string" },
                                    "version": { "minimum": 1, "type": "integer" }
                                },
                                "required": ["kind", "reference", "version"],
                                "type": "object"
                            },
                            "maxItems": 32,
                            "type": "array"
                        },
                        "severity": { "enum": ["critical", "major", "minor"] },
                        "summary": { "minLength": 1, "maxLength": 2000, "type": "string" }
                    },
                    "required": ["severity", "code", "summary", "evidence_references"],
                    "type": "object"
                },
                "maxItems": MAX_RFI_FINDINGS,
                "type": "array"
            },
            "outcome": { "enum": ["passed", "failed"] }
        },
        "required": ["outcome", "findings"],
        "type": "object"
    }))
    .expect("static External RFI review output contract is canonical")
}

fn external_rfi_review_task(
    task_id: String,
    tender_basis: (&str, u32),
    work_plan_basis: (&str, u32),
    draft: &ExternalRfiDraft,
    deadline: String,
    profile: &AgentProfileVersionView,
) -> TenderTaskView {
    let mut exact_inputs = vec![
        AgentTaskInputReference {
            kind: "tender_revision".into(),
            reference: tender_basis.0.into(),
            version: tender_basis.1,
        },
        AgentTaskInputReference {
            kind: "external_rfi_version".into(),
            reference: draft.rfi_id.clone(),
            version: draft.version,
        },
        AgentTaskInputReference {
            kind: "work_plan_version".into(),
            reference: work_plan_basis.0.into(),
            version: work_plan_basis.1,
        },
    ];
    exact_inputs.extend(
        draft
            .query_refs
            .iter()
            .map(|reference| AgentTaskInputReference {
                kind: "tender_query_version".into(),
                reference: reference.query_id.clone(),
                version: reference.version,
            }),
    );
    exact_inputs.extend(draft.source_evidence.iter().cloned());
    exact_inputs.extend(draft.attachments.iter().cloned());
    exact_inputs.sort_by(|a, b| {
        (&a.kind, &a.reference, a.version).cmp(&(&b.kind, &b.reference, b.version))
    });
    exact_inputs.dedup();
    TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: "Independently review the exact External RFI draft and report attributable findings without editing or approving it.".into(),
        exact_inputs,
        output_contract_json: external_rfi_review_output_contract(),
        review_policy: "A pass may contain only disclosed Minor findings. Critical or Major findings require a failed review and a revised draft. The reviewer cannot edit or approve the RFI.".into(),
        deadline,
        permissions: profile.permissions.clone(),
        resource_budget: profile.resource_budget.clone(),
        repair_feedback: None,
    }
}

fn external_rfi_review_data_view(
    connection: &rusqlite::Connection,
    draft: &ExternalRfiDraft,
    data_scope: &str,
    data_classification: DataClassification,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<serde_json::Value, TenderCommandError> {
    let mut expanded_bytes = canonical_json(draft)?.len().saturating_add(1_024);
    let mut evidence_details = Vec::new();
    for reference in &draft.source_evidence {
        check()?;
        if reference.kind == "source_evidence" {
            let Some((artifact_id, ordinal)) = reference.reference.rsplit_once('#') else {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            };
            let ordinal = ordinal
                .parse::<u32>()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let detail: Option<(String, String, Option<String>)> = connection
                .query_row(
                    "SELECT structural_path, original_text, translated_text
                     FROM evidence_locations
                     WHERE artifact_id = ?1 AND version = ?2 AND ordinal = ?3",
                    params![artifact_id, reference.version, ordinal],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let detail =
                detail.ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let detail = json!({
                "reference": reference,
                "structural_path": detail.0,
                "original_text": detail.1,
                "translated_text": detail.2,
            });
            expanded_bytes = expanded_bytes.saturating_add(canonical_json(&detail)?.len());
            evidence_details.push(detail);
        } else {
            let detail = json!({ "reference": reference });
            expanded_bytes = expanded_bytes.saturating_add(canonical_json(&detail)?.len());
            evidence_details.push(detail);
        }
        if expanded_bytes > MAX_RFI_REVIEW_DATA_VIEW_BYTES - RFI_REVIEW_GRANT_METADATA_RESERVE_BYTES
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    let payload = json!({
        "data_classification": data_classification,
        "data_scope": data_scope,
        "external_rfi": draft,
        "evidence_details": evidence_details,
        "review_rules": {
            "author_target_is_immutable": true,
            "external_action_allowed": false,
            "manager_approval_required_after_pass": true
        }
    });
    if canonical_json(&payload)?.len()
        > MAX_RFI_REVIEW_DATA_VIEW_BYTES - RFI_REVIEW_GRANT_METADATA_RESERVE_BYTES
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(payload)
}

impl TenderStore {
    pub(crate) fn prepare_external_rfi_review_run(
        &mut self,
        tender_id: &TenderId,
        rfi_id: &str,
        version: u32,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        self.require_change_intake_writable()?;
        if !super::valid_identifier(rfi_id) || version == 0 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let budget = BidPackageOperationBudget::for_tender(tender_id);
        let draft = self.load_external_rfi(rfi_id, version, budget)?;
        if !draft.current
            || !draft.evidence_current
            || draft.review.is_some()
            || draft.approval.is_some()
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let run_id = random_identifier(&self.connection)?;
        let application_home = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let provider_selection = self.required_tender_ai_execution_selection()?;
        let workspace = application_home
            .join("staging")
            .join(format!("agent-{}-{run_id}", tender_id.as_str()));
        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let current_version: Option<u32> = transaction
                .query_row(
                    "SELECT current_version FROM external_rfi_heads WHERE rfi_id = ?1",
                    [rfi_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            let has_review: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM external_rfi_reviews
                     WHERE rfi_id = ?1 AND rfi_version = ?2)",
                    params![rfi_id, version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let has_approval: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM external_rfi_approvals
                     WHERE rfi_id = ?1 AND rfi_version = ?2)",
                    params![rfi_id, version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let unresolved_indeterminate: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs
                       WHERE status = 'indeterminate'
                         AND NOT EXISTS (
                           SELECT 1 FROM agent_run_recovery_dispositions
                           WHERE agent_run_recovery_dispositions.run_id = agent_runs.run_id
                         )
                     )",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if current_version != Some(version)
                || has_review
                || has_approval
                || unresolved_indeterminate
                || !external_rfi_query_refs_are_current(&transaction, &draft.query_refs)?
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let tender_revision = current_tender_revision(&transaction)?;
            let created_at = sqlite_timestamp(&transaction)?;
            let approved_plan: Option<(String, u32, String)> = transaction
                .query_row(
                    "SELECT activations.plan_id, activations.plan_version, plans.profiles_json
                     FROM production_activations AS activations
                     JOIN work_plan_versions AS plans
                       ON plans.plan_id = activations.plan_id
                      AND plans.version = activations.plan_version
                     JOIN work_plan_approvals AS approvals
                       ON approvals.plan_id = activations.plan_id
                      AND approvals.plan_version = activations.plan_version
                      AND approvals.decision = 'approve'
                     WHERE activations.status = 'active'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (plan_id, plan_version, profiles_json) = approved_plan
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let plan_profiles: Vec<WorkPlanProfileBinding> = parse_canonical(&profiles_json)?;
            let approved_profile = plan_profiles
                .iter()
                .map(|binding| &binding.profile)
                .find(|profile| {
                    profile
                        .capabilities
                        .iter()
                        .any(|capability| capability == EXTERNAL_RFI_REVIEW_QUALIFICATION)
                })
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let profile = load_profile(
                &transaction,
                (
                    approved_profile.profile_id.clone(),
                    approved_profile.version,
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
            if !profile_is_active || profile != *approved_profile {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let review_data_scope = profile.permissions.data_scopes.join("+");
            let review_data_classification = *profile
                .permissions
                .data_classifications
                .iter()
                .max()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let payload = external_rfi_review_data_view(
                &transaction,
                &draft,
                &review_data_scope,
                review_data_classification,
                &mut || budget.check(),
            )?;
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
            if profile_is_busy {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = external_rfi_review_task(
                random_identifier(&transaction)?,
                (tender_id.as_str(), tender_revision),
                (&plan_id, plan_version),
                &draft,
                deadline.clone(),
                &profile,
            );
            insert_task(&transaction, &task, &created_at)?;
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
                        append_audit_event(
                            &transaction,
                            tender_id.as_str(),
                            "provider_thread_archive_requested",
                            tender_revision,
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
                    summary: "Independent External RFI review started".into(),
                    correlation_id: None,
                    request_fingerprint: None,
                    denial_reason: None,
                    opaque_reference: None,
                },
                &created_at,
            )?;
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "external_rfi_review_started",
                tender_revision,
                json!({
                    "reviewer_profile_id": profile.profile_id,
                    "rfi_id": rfi_id,
                    "rfi_version": version.to_string(),
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

    pub(crate) fn validate_external_rfi_review_candidate(
        &self,
        task: &TenderTaskView,
        payload: &str,
    ) -> Result<ExternalRfiReviewCandidate, TenderCommandError> {
        if payload.len() > 64 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let candidate: ExternalRfiReviewCandidate = serde_json::from_str(payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let (_, _, _) = exact_external_rfi_target(task)?;
        let mut finding_codes = HashSet::new();
        let valid_findings = candidate.findings.len() <= MAX_RFI_FINDINGS
            && candidate.findings.iter().all(|finding| {
                valid_external_rfi_finding_code(&finding.code)
                    && finding_codes.insert(finding.code.clone())
                    && !finding.summary.trim().is_empty()
                    && finding.summary.len() <= 2_000
                    && !finding.evidence_references.is_empty()
                    && finding.evidence_references.len() <= 32
                    && finding
                        .evidence_references
                        .iter()
                        .all(|reference| task.exact_inputs.contains(reference))
            });
        let valid_outcome = match candidate.outcome {
            ExternalRfiReviewOutcome::Passed => candidate
                .findings
                .iter()
                .all(|finding| finding.severity == ExternalRfiFindingSeverity::Minor),
            ExternalRfiReviewOutcome::Failed => candidate.findings.iter().any(|finding| {
                matches!(
                    finding.severity,
                    ExternalRfiFindingSeverity::Critical | ExternalRfiFindingSeverity::Major
                )
            }),
        };
        if !valid_findings || !valid_outcome {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(candidate)
    }

    pub(crate) fn external_rfi_review_target_is_current(
        &self,
        task: &TenderTaskView,
    ) -> Result<bool, TenderCommandError> {
        let (rfi_id, version, _) = exact_external_rfi_target(task)?;
        let query_refs_json: Option<String> = self
            .connection
            .query_row(
                "SELECT versions.query_refs_json FROM external_rfi_versions AS versions
                 JOIN external_rfi_heads AS heads ON heads.rfi_id = versions.rfi_id
                 WHERE versions.rfi_id = ?1 AND versions.version = ?2
                   AND heads.current_version = versions.version",
                params![rfi_id, version],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        query_refs_json
            .as_deref()
            .map(parse_canonical::<Vec<ExternalRfiQueryReference>>)
            .transpose()?
            .map(|refs| external_rfi_query_refs_are_current(&self.connection, &refs))
            .transpose()
            .map(|value| value.unwrap_or(false))
    }

    fn record_external_rfi_denial(
        &mut self,
        tender_id: &TenderId,
        command: &str,
        target_id: Option<&str>,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let tender_revision = current_tender_revision(&transaction)?;
        append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "external_rfi_command_denied",
            tender_revision,
            json!({
                "command": command,
                "reason": reason,
                "target_id": target_id,
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    fn create_external_rfi_draft(
        &mut self,
        tender_id: &TenderId,
        command: &CreateExternalRfiDraftCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let count: u32 = transaction
            .query_row("SELECT COUNT(*) FROM external_rfis", [], |row| row.get(0))
            .map_err(sql_error)?;
        if count >= MAX_EXTERNAL_RFIS {
            append_external_rfi_denial(
                &transaction,
                tender_id,
                "create_external_rfi_draft",
                None,
                "rfi_limit",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let candidate = ExternalRfiCandidate {
            query_refs: &command.query_refs,
            additional_evidence: &command.additional_evidence,
            contractual_context: &command.contractual_context,
            response_need: &command.response_need,
            attachments: &command.attachments,
            due_at: &command.due_at,
            recipient: &command.recipient,
            affected_commitments: &command.affected_commitments,
        };
        let rfi_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let materialized =
            match materialize_external_rfi_candidate(&transaction, candidate, &mut || {
                budget.check()
            }) {
                Ok(candidate) => candidate,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error)
                }
                Err(error) if error.code != TenderErrorCode::InvalidCommand => return Err(error),
                Err(_) => {
                    append_external_rfi_denial(
                        &transaction,
                        tender_id,
                        "create_external_rfi_draft",
                        None,
                        "candidate_invalid",
                    )?;
                    transaction.commit().map_err(sql_error)?;
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            };
        if let Err(error) = external_rfi_review_data_view(
            &transaction,
            &external_rfi_candidate_preview(&rfi_id, 1, &materialized, candidate, &created_at),
            "tender_queries_review",
            DataClassification::TenderInternal,
            &mut || budget.check(),
        ) {
            if error.code != TenderErrorCode::InvalidCommand {
                return Err(error);
            }
            append_external_rfi_denial(
                &transaction,
                tender_id,
                "create_external_rfi_draft",
                None,
                "review_view_limit",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        transaction
            .execute(
                "INSERT INTO external_rfis (rfi_id, created_at) VALUES (?1, ?2)",
                params![rfi_id, created_at],
            )
            .map_err(sql_error)?;
        let manifest_sha256 = insert_external_rfi_version(
            &transaction,
            &rfi_id,
            1,
            &materialized,
            candidate,
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO external_rfi_heads (rfi_id, current_version) VALUES (?1, 1)",
                [&rfi_id],
            )
            .map_err(sql_error)?;
        append_external_rfi_event(
            &transaction,
            tender_id,
            "external_rfi_draft_created",
            &rfi_id,
            1,
            &manifest_sha256,
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_external_rfi(&rfi_id, 1, budget)
    }

    fn revise_external_rfi_draft(
        &mut self,
        tender_id: &TenderId,
        command: &ReviseExternalRfiDraftCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let head: Option<u32> = transaction
            .query_row(
                "SELECT current_version FROM external_rfi_heads WHERE rfi_id = ?1",
                [&command.rfi_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let approved: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM external_rfi_approvals
                 WHERE rfi_id = ?1 AND rfi_version = ?2)",
                params![command.rfi_id, command.base_version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if head != Some(command.base_version)
            || command.base_version >= MAX_EXTERNAL_RFI_VERSIONS
            || approved
        {
            append_external_rfi_denial(
                &transaction,
                tender_id,
                "revise_external_rfi_draft",
                Some(&command.rfi_id),
                "version_not_current",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let candidate = ExternalRfiCandidate {
            query_refs: &command.query_refs,
            additional_evidence: &command.additional_evidence,
            contractual_context: &command.contractual_context,
            response_need: &command.response_need,
            attachments: &command.attachments,
            due_at: &command.due_at,
            recipient: &command.recipient,
            affected_commitments: &command.affected_commitments,
        };
        let version = command.base_version + 1;
        let created_at = sqlite_timestamp(&transaction)?;
        let materialized =
            match materialize_external_rfi_candidate(&transaction, candidate, &mut || {
                budget.check()
            }) {
                Ok(candidate) => candidate,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error)
                }
                Err(error) if error.code != TenderErrorCode::InvalidCommand => return Err(error),
                Err(_) => {
                    append_external_rfi_denial(
                        &transaction,
                        tender_id,
                        "revise_external_rfi_draft",
                        Some(&command.rfi_id),
                        "candidate_invalid",
                    )?;
                    transaction.commit().map_err(sql_error)?;
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            };
        if let Err(error) = external_rfi_review_data_view(
            &transaction,
            &external_rfi_candidate_preview(
                &command.rfi_id,
                version,
                &materialized,
                candidate,
                &created_at,
            ),
            "tender_queries_review",
            DataClassification::TenderInternal,
            &mut || budget.check(),
        ) {
            if error.code != TenderErrorCode::InvalidCommand {
                return Err(error);
            }
            append_external_rfi_denial(
                &transaction,
                tender_id,
                "revise_external_rfi_draft",
                Some(&command.rfi_id),
                "review_view_limit",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = insert_external_rfi_version(
            &transaction,
            &command.rfi_id,
            version,
            &materialized,
            candidate,
            &created_at,
        )?;
        if transaction
            .execute(
                "UPDATE external_rfi_heads SET current_version = ?2
                 WHERE rfi_id = ?1 AND current_version = ?3",
                params![command.rfi_id, version, command.base_version],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        append_external_rfi_event(
            &transaction,
            tender_id,
            "external_rfi_draft_revised",
            &command.rfi_id,
            version,
            &manifest_sha256,
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_external_rfi(&command.rfi_id, version, budget)
    }

    pub(crate) fn inspect_external_rfis(
        &self,
        command: &InspectExternalRfisCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiPage, TenderCommandError> {
        if command.limit == 0 || command.limit > MAX_RFI_PAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        budget.check()?;
        let cursor_rowid = command
            .cursor
            .as_deref()
            .map(|cursor| {
                if !super::valid_identifier(cursor) {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                self.connection
                    .query_row(
                        "SELECT rowid FROM external_rfis WHERE rfi_id = ?1",
                        [cursor],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
            })
            .transpose()?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT rfis.rfi_id, heads.current_version
                 FROM external_rfis AS rfis
                 JOIN external_rfi_heads AS heads USING (rfi_id)
                 WHERE (?1 IS NULL OR rfis.rowid < ?1)
                 ORDER BY rfis.rowid DESC LIMIT ?2",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![cursor_rowid, command.limit + 1], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        let has_more = rows.len() > command.limit as usize;
        let selected = rows
            .into_iter()
            .take(command.limit as usize)
            .collect::<Vec<_>>();
        let mut items = Vec::with_capacity(selected.len());
        for (rfi_id, version) in &selected {
            budget.check()?;
            items.push(self.load_external_rfi(rfi_id, *version, budget)?);
        }
        let total_current_count = self
            .connection
            .query_row("SELECT COUNT(*) FROM external_rfi_heads", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        let mut approved_for_issue_count = 0_u32;
        let mut approval_statement = self
            .connection
            .prepare(
                "SELECT versions.query_refs_json
                 FROM external_rfi_heads AS heads
                 JOIN external_rfi_versions AS versions
                   ON versions.rfi_id = heads.rfi_id
                  AND versions.version = heads.current_version
                 JOIN external_rfi_reviews AS reviews
                   ON reviews.rfi_id = versions.rfi_id
                  AND reviews.rfi_version = versions.version
                  AND reviews.outcome = 'passed'
                 JOIN external_rfi_approvals AS approvals
                   ON approvals.rfi_id = versions.rfi_id
                  AND approvals.rfi_version = versions.version
                 ORDER BY versions.rowid LIMIT ?1",
            )
            .map_err(sql_error)?;
        let approval_rows = approval_statement
            .query_map([MAX_EXTERNAL_RFIS + 1], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        for query_refs_json in approval_rows {
            budget.check()?;
            let query_refs = parse_canonical::<Vec<ExternalRfiQueryReference>>(
                &query_refs_json.map_err(sql_error)?,
            )?;
            if external_rfi_query_refs_are_current(&self.connection, &query_refs)? {
                approved_for_issue_count = approved_for_issue_count
                    .checked_add(1)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            }
        }
        Ok(ExternalRfiPage {
            next_cursor: has_more
                .then(|| selected.last().map(|row| row.0.clone()))
                .flatten(),
            items,
            total_current_count,
            approved_for_issue_count,
        })
    }

    fn inspect_external_rfi_eligible_queries(
        &self,
        command: &InspectExternalRfiEligibleQueriesCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiEligibleQueryPage, TenderCommandError> {
        budget.check()?;
        let cursor_rowid = command
            .cursor
            .as_deref()
            .map(|cursor| {
                if !super::valid_identifier(cursor) {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                self.connection
                    .query_row(
                        "SELECT rowid FROM tender_queries WHERE query_id = ?1",
                        [cursor],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
            })
            .transpose()?;
        let total_count = self
            .connection
            .query_row(
                "SELECT COUNT(*)
             FROM tender_query_heads AS heads
             JOIN tender_query_treatment_decisions AS decisions
               ON decisions.query_id = heads.query_id
              AND decisions.query_version = heads.current_version
             WHERE decisions.treatment = 'external_rfi_drafting'
               AND decisions.closes_query = 0",
                [],
                |row| row.get::<_, u32>(0),
            )
            .map_err(sql_error)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT queries.query_id, versions.version, versions.manifest_sha256,
                    versions.question, versions.ambiguity_or_gap, versions.due_at,
                    versions.affected_task_keys_json
             FROM tender_queries AS queries
             JOIN tender_query_heads AS heads USING (query_id)
             JOIN tender_query_versions AS versions
               ON versions.query_id = heads.query_id
              AND versions.version = heads.current_version
             JOIN tender_query_treatment_decisions AS decisions
               ON decisions.query_id = versions.query_id
              AND decisions.query_version = versions.version
             WHERE decisions.treatment = 'external_rfi_drafting'
               AND decisions.closes_query = 0
               AND (?1 IS NULL OR queries.rowid < ?1)
             ORDER BY queries.rowid DESC LIMIT ?2",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![cursor_rowid, command.limit + 1], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        let has_more = rows.len() > command.limit as usize;
        let selected = rows
            .into_iter()
            .take(command.limit as usize)
            .collect::<Vec<_>>();
        let mut items = Vec::with_capacity(selected.len());
        for row in &selected {
            budget.check()?;
            items.push(ExternalRfiEligibleQuery {
                query_ref: ExternalRfiQueryReference {
                    query_id: row.0.clone(),
                    version: row.1,
                    manifest_sha256: row.2.clone(),
                },
                question: row.3.clone(),
                ambiguity_or_gap: row.4.clone(),
                due_at: row.5.clone(),
                affected_task_keys: parse_canonical(&row.6)?,
            });
        }
        Ok(ExternalRfiEligibleQueryPage {
            next_cursor: has_more
                .then(|| selected.last().map(|row| row.0.clone()))
                .flatten(),
            items,
            total_count,
        })
    }

    fn inspect_external_rfi_response_candidates(
        &self,
        command: &InspectExternalRfiResponseCandidatesCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiResponseCandidatePage, TenderCommandError> {
        budget.check()?;
        if command.limit == 0 || command.limit > MAX_RFI_RECORDS_PER_APPROVAL {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let cursor = command
            .cursor
            .as_deref()
            .map(|cursor| {
                let (artifact_id, version) = cursor
                    .rsplit_once(':')
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                if !super::valid_identifier(artifact_id) {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                let version = version
                    .parse::<u32>()
                    .ok()
                    .filter(|version| *version > 0)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                self.connection
                    .query_row(
                        "SELECT artifacts.rowid
                         FROM source_artifacts AS artifacts
                         JOIN source_artifact_versions AS sources
                           ON sources.artifact_id = artifacts.artifact_id
                          AND sources.version = ?2
                         WHERE artifacts.artifact_id = ?1",
                        params![artifact_id, version],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .map(|rowid| (rowid, version))
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
            })
            .transpose()?;
        let (cursor_rowid, cursor_version) = cursor
            .map(|(rowid, version)| (Some(rowid), Some(version)))
            .unwrap_or((None, None));
        let mut statement = self
            .connection
            .prepare(
                "SELECT sources.artifact_id, sources.version, artifacts.package_path,
                        artifacts.rowid
                 FROM external_rfi_approvals AS approvals
                 CROSS JOIN source_artifacts AS artifacts
                 JOIN source_artifact_versions AS sources
                   ON sources.artifact_id = artifacts.artifact_id
                 JOIN intake_runs AS intake ON intake.intake_id = artifacts.intake_id
                 WHERE approvals.approval_id = ?1
                   AND (?2 IS NULL OR artifacts.rowid > ?2
                     OR (artifacts.rowid = ?2 AND sources.version > ?3))
                   AND sources.registration_state = 'registered'
                   AND EXISTS (
                     SELECT 1 FROM audit_events AS intake_events
                     WHERE intake_events.event_type = 'tender_package_imported'
                       AND intake_events.sequence > approvals.audit_sequence
                       AND json_extract(intake_events.payload_json, '$.change.intake_id') = intake.intake_id
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM external_rfi_responses AS existing
                     WHERE existing.source_artifact_id = sources.artifact_id
                       AND existing.source_artifact_version = sources.version
                 )
                 ORDER BY artifacts.rowid, sources.version
                 LIMIT ?4",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(
                params![
                    command.approval_id,
                    cursor_rowid,
                    cursor_version,
                    command.limit + 1
                ],
                |row| {
                    Ok((
                        ExternalRfiResponseCandidate {
                            source_artifact_id: row.get(0)?,
                            source_artifact_version: row.get(1)?,
                            package_path: row.get(2)?,
                        },
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        budget.check()?;
        let has_more = rows.len() > command.limit as usize;
        let selected = rows
            .into_iter()
            .take(command.limit as usize)
            .collect::<Vec<_>>();
        Ok(ExternalRfiResponseCandidatePage {
            next_cursor: has_more.then(|| {
                let (candidate, _) = selected
                    .last()
                    .expect("a response-candidate page with a successor is nonempty");
                format!(
                    "{}:{}",
                    candidate.source_artifact_id, candidate.source_artifact_version
                )
            }),
            items: selected
                .into_iter()
                .map(|(candidate, _)| candidate)
                .collect(),
        })
    }

    pub(crate) fn load_external_rfi(
        &self,
        rfi_id: &str,
        version: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        budget.check()?;
        let row = self
            .connection
            .query_row(
                "SELECT query_refs_json, questions_json, source_evidence_json,
                        contractual_context, response_need, attachments_json, due_at,
                        recipient_json, affected_task_keys_json, affected_commitments_json,
                        manifest_sha256, created_at
                 FROM external_rfi_versions WHERE rfi_id = ?1 AND version = ?2",
                params![rfi_id, version],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, String>(11)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        let query_refs: Vec<ExternalRfiQueryReference> = parse_canonical(&row.0)?;
        let questions: Vec<ExternalRfiQuestion> = parse_canonical(&row.1)?;
        let source_evidence: Vec<AgentTaskInputReference> = parse_canonical(&row.2)?;
        let attachments: Vec<AgentTaskInputReference> = parse_canonical(&row.5)?;
        let recipient: ExternalRfiRecipient = parse_canonical(&row.7)?;
        let affected_task_keys: Vec<String> = parse_canonical(&row.8)?;
        let affected_commitments: Vec<String> = parse_canonical(&row.9)?;
        let head: Option<u32> = self
            .connection
            .query_row(
                "SELECT current_version FROM external_rfi_heads WHERE rfi_id = ?1",
                [rfi_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let current = head == Some(version);
        let evidence_current = external_rfi_query_refs_are_current(&self.connection, &query_refs)?;
        let (current_query_refs, revision_allowed) =
            current_external_rfi_query_refs(&self.connection, &query_refs, &mut || budget.check())?;
        let review = load_external_rfi_review(&self.connection, rfi_id, version)?;
        let approval = load_external_rfi_approval(&self.connection, rfi_id, version)?;
        let revision_allowed = revision_allowed && approval.is_none();
        let exports = load_external_rfi_exports(&self.connection, &self.root, approval.as_ref())?;
        let responses = load_external_rfi_responses(&self.connection, rfi_id, version)?;
        let interpretations = load_external_rfi_interpretations(&self.connection, &responses)?;
        let approved_for_issue = current
            && evidence_current
            && review
                .as_ref()
                .is_some_and(|review| review.outcome == ExternalRfiReviewOutcome::Passed)
            && approval.is_some();
        Ok(ExternalRfiDraft {
            rfi_id: rfi_id.to_owned(),
            version,
            query_refs,
            current_query_refs,
            questions,
            source_evidence,
            contractual_context: row.3,
            response_need: row.4,
            attachments,
            due_at: row.6,
            recipient,
            affected_task_keys,
            affected_commitments,
            review,
            approval,
            exports,
            responses,
            interpretations,
            current,
            evidence_current,
            revision_allowed,
            approved_for_issue,
            manifest_sha256: row.10,
            created_at: row.11,
        })
    }

    fn approve_external_rfi_for_issue(
        &mut self,
        tender_id: &TenderId,
        command: &ApproveExternalRfiForIssueCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let stored: Option<(u32, String)> = transaction
            .query_row(
                "SELECT heads.current_version, versions.manifest_sha256
                 FROM external_rfi_heads AS heads
                 JOIN external_rfi_versions AS versions
                   ON versions.rfi_id = heads.rfi_id
                  AND versions.version = heads.current_version
                 WHERE heads.rfi_id = ?1",
                [&command.rfi_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let review = load_external_rfi_review(&transaction, &command.rfi_id, command.version)?;
        let existing_approval: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM external_rfi_approvals
                 WHERE rfi_id = ?1 AND rfi_version = ?2)",
                params![command.rfi_id, command.version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let query_refs_json: Option<String> = transaction
            .query_row(
                "SELECT query_refs_json FROM external_rfi_versions
                 WHERE rfi_id = ?1 AND version = ?2",
                params![command.rfi_id, command.version],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let query_refs = query_refs_json
            .as_deref()
            .map(parse_canonical::<Vec<ExternalRfiQueryReference>>)
            .transpose()?;
        let query_refs_current = query_refs
            .as_ref()
            .map(|refs| external_rfi_query_refs_are_current(&transaction, refs))
            .transpose()?
            .unwrap_or(false);
        let guard_passed = stored.as_ref().is_some_and(|(version, manifest)| {
            *version == command.version && manifest == &command.manifest_sha256
        }) && query_refs_current
            && review
                .as_ref()
                .is_some_and(|review| review.outcome == ExternalRfiReviewOutcome::Passed)
            && !existing_approval;
        if !guard_passed {
            append_external_rfi_denial(
                &transaction,
                tender_id,
                "approve_external_rfi_for_issue",
                Some(&command.rfi_id),
                "approval_guard_failed",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let review =
            review.ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = ExternalRfiApprovalManifest {
            schema_version: 1,
            approval_id: &approval_id,
            rfi_id: &command.rfi_id,
            rfi_version: command.version,
            rfi_manifest_sha256: &command.manifest_sha256,
            review_id: &review.review_id,
            review_manifest_sha256: &review.manifest_sha256,
            rationale: command.rationale.trim(),
            approved_by: "engineer_user",
            acting_role: "tendering_manager",
            created_at: &created_at,
        };
        let manifest_json = canonical_json(&manifest)?;
        let approval_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "external_rfi_approved_for_issue",
            current_tender_revision(&transaction)?,
            json!({
                "acting_role": "tendering_manager",
                "approval_id": approval_id,
                "approval_sha256": approval_sha256,
                "approved_by": "engineer_user",
                "review_id": review.review_id,
                "rfi_id": command.rfi_id,
                "rfi_manifest_sha256": command.manifest_sha256,
                "rfi_version": command.version.to_string(),
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO external_rfi_approvals (
                   approval_id, rfi_id, rfi_version, rfi_manifest_sha256,
                   review_id, review_manifest_sha256, rationale, approved_by,
                   acting_role, audit_sequence, manifest_json, approval_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                           'engineer_user', 'tendering_manager', ?8, ?9, ?10, ?11)",
                params![
                    approval_id,
                    command.rfi_id,
                    command.version,
                    command.manifest_sha256,
                    review.review_id,
                    review.manifest_sha256,
                    command.rationale.trim(),
                    audit_sequence,
                    manifest_json,
                    approval_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_external_rfi(&command.rfi_id, command.version, budget)
    }

    fn export_approved_external_rfi(
        &mut self,
        application_home: &Path,
        tender_id: &TenderId,
        command: &ExportApprovedExternalRfiCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiExportRecord, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let draft = self.load_external_rfi(&command.rfi_id, command.version, budget)?;
        let approval = draft
            .approval
            .as_ref()
            .filter(|approval| approval.approval_sha256 == command.approval_sha256)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !draft.approved_for_issue {
            self.record_external_rfi_denial(
                tender_id,
                "export_approved_external_rfi",
                Some(&command.rfi_id),
                "approval_not_current",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let bytes = render_external_rfi_text(&draft)?;
        if bytes.is_empty() || bytes.len() > MAX_EXPORT_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let exports_root = application_home.join("exports");
        validate_managed_export_root(application_home, &exports_root)?;
        let tender_export_root = exports_root.join(tender_id.as_str());
        if !tender_export_root.exists() {
            fs::create_dir(&tender_export_root).map_err(store_unavailable)?;
        }
        validate_export_directory(&exports_root, &tender_export_root)?;
        let required_space = u64::try_from(bytes.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(2);
        if fs4::available_space(&tender_export_root)
            .map_or(true, |available| available < required_space)
        {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
        let export_id = random_identifier(&self.connection)?;
        let relative_path = format!(
            "{}/external-rfi-{}-v{}-{}.txt",
            tender_id.as_str(),
            command.rfi_id,
            command.version,
            export_id
        );
        let output_path = exports_root.join(&relative_path);
        let mut staged =
            NamedTempFile::new_in(application_home.join("staging")).map_err(store_unavailable)?;
        staged.write_all(&bytes).map_err(store_unavailable)?;
        staged.as_file().sync_all().map_err(store_unavailable)?;
        let staged_hash = hash_file_bounded(staged.path(), MAX_EXPORT_BYTES)?;
        let bytes_sha256 = sha256_hex(&bytes);
        if staged_hash.0 != bytes_sha256 || staged_hash.1 != bytes.len() as u64 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        staged
            .persist_noclobber(&output_path)
            .map_err(|error| store_unavailable(error.error))?;
        let published_hash = hash_file_bounded(&output_path, MAX_EXPORT_BYTES)?;
        if published_hash != staged_hash {
            let _ = fs::remove_file(&output_path);
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let publication = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let current: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM external_rfi_heads AS heads
                       JOIN external_rfi_approvals AS approvals
                         ON approvals.rfi_id = heads.rfi_id
                        AND approvals.rfi_version = heads.current_version
                       WHERE heads.rfi_id = ?1 AND heads.current_version = ?2
                         AND approvals.approval_id = ?3
                         AND approvals.approval_sha256 = ?4
                     )",
                    params![
                        command.rfi_id,
                        command.version,
                        approval.approval_id,
                        command.approval_sha256,
                    ],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let query_refs_current =
                external_rfi_query_refs_are_current(&transaction, &draft.query_refs)?;
            if !current || !query_refs_current {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let export_count: u32 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM external_rfi_exports WHERE approval_id = ?1",
                    [&approval.approval_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if export_count >= MAX_RFI_RECORDS_PER_APPROVAL {
                append_external_rfi_denial(
                    &transaction,
                    tender_id,
                    "export_approved_external_rfi",
                    Some(&command.rfi_id),
                    "export_limit",
                )?;
                transaction.commit().map_err(sql_error)?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let created_at = sqlite_timestamp(&transaction)?;
            let manifest = ExternalRfiExportManifest {
                schema_version: 1,
                export_id: &export_id,
                approval_id: &approval.approval_id,
                relative_path: &relative_path,
                bytes_sha256: &bytes_sha256,
                size_bytes: published_hash.1,
                created_at: &created_at,
            };
            let manifest_json = canonical_json(&manifest)?;
            let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
            let audit_sequence = append_audit_event_with_sequence(
                &transaction,
                tender_id.as_str(),
                "external_rfi_exported_for_human_issue",
                current_tender_revision(&transaction)?,
                json!({
                    "approval_id": approval.approval_id,
                    "bytes_sha256": bytes_sha256,
                    "export_id": export_id,
                    "manifest_sha256": manifest_sha256,
                    "rfi_id": command.rfi_id,
                    "rfi_version": command.version.to_string(),
                    "size_bytes": published_hash.1.to_string(),
                }),
                &created_at,
            )?;
            transaction
                .execute(
                    "INSERT INTO external_rfi_exports (
                       export_id, approval_id, relative_path, bytes_sha256,
                       size_bytes, audit_sequence, manifest_json, manifest_sha256, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        export_id,
                        approval.approval_id,
                        relative_path,
                        bytes_sha256,
                        i64::try_from(published_hash.1).map_err(|_| TenderCommandError::new(
                            TenderErrorCode::IntegrityFailed
                        ))?,
                        audit_sequence,
                        manifest_json,
                        manifest_sha256,
                        created_at,
                    ],
                )
                .map_err(sql_error)?;
            budget.check()?;
            transaction.commit().map_err(sql_error)?;
            Ok(ExternalRfiExportRecord {
                export_id: export_id.clone(),
                approval_id: approval.approval_id.clone(),
                path: output_path.to_string_lossy().into_owned(),
                bytes_sha256: bytes_sha256.clone(),
                size_bytes: published_hash.1,
                bytes_verified: true,
                manifest_sha256,
                created_at,
            })
        })();
        if publication.is_err() {
            let _ = fs::remove_file(&output_path);
        }
        publication
    }

    fn register_external_rfi_response(
        &mut self,
        tender_id: &TenderId,
        command: &RegisterExternalRfiResponseCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let response_count: u32 = transaction
            .query_row(
                "SELECT COUNT(*) FROM external_rfi_responses WHERE approval_id = ?1",
                [&command.approval_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if response_count >= MAX_RFI_RECORDS_PER_APPROVAL {
            append_external_rfi_denial(
                &transaction,
                tender_id,
                "register_external_rfi_response",
                Some(&command.rfi_id),
                "response_limit",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let valid: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM external_rfi_approvals AS approvals
                   JOIN source_artifact_versions AS sources
                     ON sources.artifact_id = ?4 AND sources.version = ?5
                   JOIN source_artifacts AS artifacts
                     ON artifacts.artifact_id = sources.artifact_id
                   JOIN intake_runs AS intake ON intake.intake_id = artifacts.intake_id
                   WHERE approvals.rfi_id = ?1 AND approvals.rfi_version = ?2
                     AND approvals.approval_id = ?3
                     AND sources.registration_state = 'registered'
                     AND EXISTS (
                       SELECT 1 FROM audit_events AS intake_events
                       WHERE intake_events.event_type = 'tender_package_imported'
                         AND intake_events.sequence > approvals.audit_sequence
                         AND json_extract(intake_events.payload_json, '$.change.intake_id') = intake.intake_id
                     )
                     AND NOT EXISTS (
                       SELECT 1 FROM external_rfi_responses AS existing
                       WHERE existing.source_artifact_id = sources.artifact_id
                         AND existing.source_artifact_version = sources.version
                     )
                 )",
                params![
                    command.rfi_id,
                    command.rfi_version,
                    command.approval_id,
                    command.source_artifact_id,
                    command.source_artifact_version,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !valid {
            append_external_rfi_denial(
                &transaction,
                tender_id,
                "register_external_rfi_response",
                Some(&command.rfi_id),
                "response_source_not_registered_through_intake",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let response_link_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = ExternalRfiResponseManifest {
            schema_version: 1,
            response_link_id: &response_link_id,
            rfi_id: &command.rfi_id,
            rfi_version: command.rfi_version,
            approval_id: &command.approval_id,
            source_artifact_id: &command.source_artifact_id,
            source_artifact_version: command.source_artifact_version,
            registered_by: "engineer_user",
            created_at: &created_at,
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "external_rfi_response_registered",
            current_tender_revision(&transaction)?,
            json!({
                "approval_id": command.approval_id,
                "manifest_sha256": manifest_sha256,
                "response_link_id": response_link_id,
                "rfi_id": command.rfi_id,
                "rfi_version": command.rfi_version.to_string(),
                "source_artifact_id": command.source_artifact_id,
                "source_artifact_version": command.source_artifact_version.to_string(),
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO external_rfi_responses (
                   response_link_id, rfi_id, rfi_version, approval_id,
                   source_artifact_id, source_artifact_version, registered_by,
                   audit_sequence, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'engineer_user', ?7, ?8, ?9, ?10)",
                params![
                    response_link_id,
                    command.rfi_id,
                    command.rfi_version,
                    command.approval_id,
                    command.source_artifact_id,
                    command.source_artifact_version,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_external_rfi(&command.rfi_id, command.rfi_version, budget)
    }

    fn interpret_external_rfi_response(
        &mut self,
        tender_id: &TenderId,
        command: &InterpretExternalRfiResponseCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let response: Option<(String, u32, String, u32)> = transaction
            .query_row(
                "SELECT responses.rfi_id, responses.rfi_version,
                        responses.source_artifact_id, responses.source_artifact_version
                 FROM external_rfi_responses AS responses
                 JOIN external_rfi_versions AS versions
                   ON versions.rfi_id = responses.rfi_id
                  AND versions.version = responses.rfi_version
                 WHERE responses.response_link_id = ?1
                   AND EXISTS (
                     SELECT 1 FROM json_each(versions.query_refs_json) AS refs
                     WHERE json_extract(refs.value, '$.query_id') = ?2
                       AND json_extract(refs.value, '$.version') = ?3
                   )",
                params![
                    command.response_link_id,
                    command.query_id,
                    command.issued_query_version
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((rfi_id, rfi_version, source_artifact_id, source_artifact_version)) = response
        else {
            append_external_rfi_denial(
                &transaction,
                tender_id,
                "interpret_external_rfi_response",
                Some(&command.response_link_id),
                "response_or_query_basis_invalid",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        let created_at = sqlite_timestamp(&transaction)?;
        let publication = super::tender_queries::publish_external_rfi_interpretation(
            &transaction,
            tender_id,
            ExternalRfiInterpretationInput {
                response_link_id: &command.response_link_id,
                query_id: &command.query_id,
                issued_query_version: command.issued_query_version,
                base_query_version: command.base_query_version,
                base_query_manifest_sha256: &command.base_query_manifest_sha256,
                response_source_artifact_id: &source_artifact_id,
                response_source_artifact_version: source_artifact_version,
                material: command.material,
                interpretation: command.interpretation.trim(),
                treatment: command.treatment,
                rationale: command.rationale.trim(),
                treatment_details: command.treatment_details.trim(),
                closes_query: command.closes_query,
                created_at: &created_at,
            },
            &mut || budget.check(),
        );
        let publication = match publication {
            Ok(publication) => publication,
            Err(error) if error.code == TenderErrorCode::OperationTimedOut => return Err(error),
            Err(error) => {
                transaction.rollback().map_err(sql_error)?;
                if error.code == TenderErrorCode::InvalidCommand {
                    self.record_external_rfi_denial(
                        tender_id,
                        "interpret_external_rfi_response",
                        Some(&command.response_link_id),
                        "interpretation_guard_failed",
                    )?;
                }
                return Err(error);
            }
        };
        let interpretation_id = random_identifier(&transaction)?;
        let manifest = ExternalRfiInterpretationManifest {
            schema_version: 1,
            interpretation_id: &interpretation_id,
            response_link_id: &command.response_link_id,
            query_id: &command.query_id,
            source_query_version: command.issued_query_version,
            base_query_version: command.base_query_version,
            resulting_query_version: publication.query_version,
            query_decision_id: &publication.decision_id,
            material: command.material,
            interpretation: command.interpretation.trim(),
            decided_by: "engineer_user",
            acting_role: "tendering_manager",
            created_at: &created_at,
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "external_rfi_response_interpreted",
            current_tender_revision(&transaction)?,
            json!({
                "acting_role": "tendering_manager",
                "base_query_version": command.base_query_version.to_string(),
                "decided_by": "engineer_user",
                "interpretation_id": interpretation_id,
                "manifest_sha256": manifest_sha256,
                "material": command.material,
                "query_decision_id": publication.decision_id,
                "query_id": command.query_id,
                "response_link_id": command.response_link_id,
                "source_query_version": command.issued_query_version.to_string(),
                "resulting_query_version": publication.query_version.to_string(),
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO external_rfi_response_interpretations (
                   interpretation_id, response_link_id, query_id, source_query_version,
                   base_query_version, resulting_query_version, query_decision_id, material, interpretation,
                   decided_by, acting_role, audit_sequence, manifest_json,
                   manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                           'engineer_user', 'tendering_manager', ?10, ?11, ?12, ?13)",
                params![
                    interpretation_id,
                    command.response_link_id,
                    command.query_id,
                    command.issued_query_version,
                    command.base_query_version,
                    publication.query_version,
                    publication.decision_id,
                    command.material,
                    command.interpretation.trim(),
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_external_rfi(&rfi_id, rfi_version, budget)
    }

    pub(crate) fn external_rfi_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let counts = self
            .connection
            .query_row(
                "SELECT
               (SELECT COUNT(*) FROM external_rfis),
               (SELECT COUNT(*) FROM external_rfi_heads),
               (SELECT COUNT(*) FROM external_rfi_versions),
               (SELECT COUNT(*) FROM external_rfi_reviews),
               (SELECT COUNT(*) FROM external_rfi_approvals),
               (SELECT COUNT(*) FROM external_rfi_exports),
               (SELECT COUNT(*) FROM external_rfi_responses),
               (SELECT COUNT(*) FROM external_rfi_response_interpretations)",
                [],
                |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, u32>(3)?,
                        row.get::<_, u32>(4)?,
                        row.get::<_, u32>(5)?,
                        row.get::<_, u32>(6)?,
                        row.get::<_, u32>(7)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        if counts.0 > MAX_EXTERNAL_RFIS
            || counts.0 != counts.1
            || counts.2 > MAX_EXTERNAL_RFIS.saturating_mul(MAX_EXTERNAL_RFI_VERSIONS)
            || counts.3 > counts.2
            || counts.4 > counts.3
            || counts.5 > counts.4.saturating_mul(MAX_RFI_RECORDS_PER_APPROVAL)
            || counts.6 > counts.4.saturating_mul(MAX_RFI_RECORDS_PER_APPROVAL)
            || counts.7 > counts.6.saturating_mul(MAX_RFI_QUERY_REFS as u32)
        {
            return Ok(false);
        }
        let per_approval_record_caps_are_valid = self
            .connection
            .query_row(
                "SELECT
                    NOT EXISTS (
                        SELECT 1
                        FROM external_rfi_exports
                        GROUP BY approval_id
                        HAVING COUNT(*) > ?1
                    )
                    AND NOT EXISTS (
                        SELECT 1
                        FROM external_rfi_responses
                        GROUP BY approval_id
                        HAVING COUNT(*) > ?1
                    )",
                [MAX_RFI_RECORDS_PER_APPROVAL],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
        if !per_approval_record_caps_are_valid {
            return Ok(false);
        }

        let mut identity_statement = self
            .connection
            .prepare(
                "SELECT rfis.rfi_id, heads.current_version, COUNT(versions.version),
                    MIN(versions.version), MAX(versions.version)
             FROM external_rfis AS rfis
             JOIN external_rfi_heads AS heads USING (rfi_id)
             JOIN external_rfi_versions AS versions USING (rfi_id)
             GROUP BY rfis.rfi_id, heads.current_version
             ORDER BY rfis.rowid",
            )
            .map_err(sql_error)?;
        let identities = identity_statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, u32>(4)?,
                ))
            })
            .map_err(sql_error)?;
        let mut identity_count = 0_u32;
        for identity in identities {
            check()?;
            let (rfi_id, head, version_count, minimum, maximum) = identity.map_err(sql_error)?;
            identity_count = identity_count.saturating_add(1);
            if !super::valid_identifier(&rfi_id)
                || version_count == 0
                || version_count > MAX_EXTERNAL_RFI_VERSIONS
                || minimum != 1
                || maximum != version_count
                || head != maximum
            {
                return Ok(false);
            }
        }
        if identity_count != counts.0 {
            return Ok(false);
        }

        if !external_rfi_versions_are_valid(&self.connection, check)?
            || !external_rfi_reviews_are_valid(&self.connection, check)?
            || !external_rfi_approvals_are_valid(&self.connection, check)?
            || !external_rfi_exports_are_valid(&self.connection, check)?
            || !external_rfi_responses_are_valid(&self.connection, check)?
            || !external_rfi_interpretations_are_valid(&self.connection, check)?
        {
            return Ok(false);
        }
        Ok(true)
    }
}

fn external_rfi_versions_are_valid(
    connection: &rusqlite::Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    type StoredVersion = (
        String,
        u32,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    );
    let mut statement = connection
        .prepare(
            "SELECT rfi_id, version, query_refs_json, questions_json,
                source_evidence_json, contractual_context, response_need,
                attachments_json, due_at, recipient_json,
                affected_task_keys_json, affected_commitments_json, created_by,
                manifest_json, manifest_sha256, created_at
         FROM external_rfi_versions ORDER BY rfi_id, version",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
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
                row.get(14)?,
                row.get(15)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let row: StoredVersion = row.map_err(sql_error)?;
        let query_refs: Vec<ExternalRfiQueryReference> = parse_canonical(&row.2)?;
        let questions: Vec<ExternalRfiQuestion> = parse_canonical(&row.3)?;
        let source_evidence: Vec<AgentTaskInputReference> = parse_canonical(&row.4)?;
        let attachments: Vec<AgentTaskInputReference> = parse_canonical(&row.7)?;
        let recipient: ExternalRfiRecipient = parse_canonical(&row.9)?;
        let affected_task_keys: Vec<String> = parse_canonical(&row.10)?;
        let affected_commitments: Vec<String> = parse_canonical(&row.11)?;
        if row.12 != "engineer_user"
            || query_refs.is_empty()
            || query_refs.len() > MAX_RFI_QUERY_REFS
            || questions.len() != query_refs.len()
            || source_evidence.is_empty()
            || source_evidence.len() > MAX_RFI_SOURCE_EVIDENCE
            || attachments.len() > MAX_RFI_ATTACHMENTS
            || affected_task_keys.is_empty()
            || affected_task_keys.len() > MAX_RFI_AFFECTED_TASKS
            || affected_commitments.len() > MAX_RFI_COMMITMENTS
            || row.5.trim().is_empty()
            || row.5.len() > 8_000
            || row.6.trim().is_empty()
            || row.6.len() > 4_000
            || Timestamp::from_str(&row.8).is_err()
            || !valid_recipient(&recipient)
            || !valid_bounded_unique_text(&affected_commitments, 4_000)
            || !references_are_sorted_unique(&source_evidence)
            || !references_are_sorted_unique(&attachments)
            || !strings_are_sorted_unique(&affected_task_keys)
        {
            return Ok(false);
        }
        let mut expected_questions = Vec::with_capacity(query_refs.len());
        let mut required_evidence = Vec::new();
        let mut expected_tasks = Vec::new();
        let mut seen_queries = HashSet::new();
        for reference in &query_refs {
            check()?;
            if !super::valid_identifier(&reference.query_id)
                || reference.version == 0
                || reference.version > 32
                || reference.manifest_sha256.len() != 64
                || !seen_queries.insert(reference.query_id.clone())
            {
                return Ok(false);
            }
            let basis: Option<(String, String, String, String, String)> = connection
                .query_row(
                    "SELECT question, ambiguity_or_gap, evidence_json,
                        affected_task_keys_json, manifest_sha256
                 FROM tender_query_versions
                 WHERE query_id = ?1 AND version = ?2
                   AND EXISTS (
                     SELECT 1 FROM tender_query_treatment_decisions
                     WHERE query_id = ?1 AND query_version = ?2
                       AND treatment = 'external_rfi_drafting' AND closes_query = 0
                   )",
                    params![reference.query_id, reference.version],
                    |record| {
                        Ok((
                            record.get(0)?,
                            record.get(1)?,
                            record.get(2)?,
                            record.get(3)?,
                            record.get(4)?,
                        ))
                    },
                )
                .optional()
                .map_err(sql_error)?;
            let Some((question, gap, evidence_json, tasks_json, manifest_sha256)) = basis else {
                return Ok(false);
            };
            if manifest_sha256 != reference.manifest_sha256 {
                return Ok(false);
            }
            expected_questions.push(ExternalRfiQuestion {
                query_id: reference.query_id.clone(),
                query_version: reference.version,
                question,
                ambiguity_or_gap: gap,
            });
            required_evidence.extend(parse_canonical::<Vec<AgentTaskInputReference>>(
                &evidence_json,
            )?);
            expected_tasks.extend(parse_canonical::<Vec<String>>(&tasks_json)?);
        }
        required_evidence.sort_by(reference_order);
        required_evidence.dedup();
        expected_tasks.sort();
        expected_tasks.dedup();
        if questions != expected_questions
            || required_evidence
                .iter()
                .any(|reference| !source_evidence.contains(reference))
            || affected_task_keys != expected_tasks
        {
            return Ok(false);
        }
        for evidence in &source_evidence {
            check()?;
            if !query_evidence_reference_exists(connection, evidence)? {
                return Ok(false);
            }
        }
        for attachment in &attachments {
            check()?;
            if !attachment_reference_exists(connection, attachment)? {
                return Ok(false);
            }
        }
        let expected_manifest = canonical_json(&ExternalRfiVersionManifest {
            schema_version: 1,
            rfi_id: &row.0,
            version: row.1,
            query_refs: &query_refs,
            questions: &questions,
            source_evidence: &source_evidence,
            contractual_context: &row.5,
            response_need: &row.6,
            attachments: &attachments,
            due_at: &row.8,
            recipient: &recipient,
            affected_task_keys: &affected_task_keys,
            affected_commitments: &affected_commitments,
            created_by: &row.12,
            created_at: &row.15,
        })?;
        if row.13 != expected_manifest
            || row.14 != sha256_hex(expected_manifest.as_bytes())
            || !external_rfi_version_audit_is_valid(connection, &row.0, row.1, &row.14, &row.15)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn external_rfi_version_audit_is_valid(
    connection: &rusqlite::Connection,
    rfi_id: &str,
    version: u32,
    manifest_sha256: &str,
    created_at: &str,
) -> Result<bool, TenderCommandError> {
    let event_type = if version == 1 {
        "external_rfi_draft_created"
    } else {
        "external_rfi_draft_revised"
    };
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload_json FROM audit_events
         WHERE event_type = ?1 AND created_at = ?2
           AND json_extract(payload_json, '$.change.rfi_id') = ?3
           AND json_extract(payload_json, '$.change.rfi_version') = ?4
         LIMIT 1",
            params![event_type, created_at, rfi_id, version.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let expected = json!({
        "created_by": "engineer_user",
        "manifest_sha256": manifest_sha256,
        "rfi_id": rfi_id,
        "rfi_version": version.to_string(),
    });
    Ok(payload.is_some_and(|payload| {
        serde_json::from_str::<serde_json::Value>(&payload)
            .is_ok_and(|payload| payload.get("change") == Some(&expected))
    }))
}

struct ExternalRfiReviewRunBasisRow {
    status: String,
    task_id: String,
    permission_grant_json: String,
    result_status: String,
    payload_json: String,
    result_scopes_json: String,
    result_classification: String,
    result_created_at: String,
    run_started_at: String,
}

fn external_rfi_reviews_are_valid(
    connection: &rusqlite::Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    type StoredReview = (
        String,
        String,
        u32,
        String,
        String,
        String,
        u32,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let mut statement = connection
        .prepare(
            "SELECT review_id, rfi_id, rfi_version, rfi_manifest_sha256,
                reviewer_run_id, reviewer_profile_id, reviewer_profile_version,
                outcome, findings_json, audit_sequence, manifest_json,
                manifest_sha256, created_at
         FROM external_rfi_reviews ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
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
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let row: StoredReview = row.map_err(sql_error)?;
        let outcome = ExternalRfiReviewOutcome::parse(&row.7)?;
        let findings: Vec<ExternalRfiReviewFinding> = parse_canonical(&row.8)?;
        let rfi_manifest: Option<String> = connection
            .query_row(
                "SELECT manifest_sha256 FROM external_rfi_versions
             WHERE rfi_id = ?1 AND version = ?2",
                params![row.1, row.2],
                |record| record.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if rfi_manifest.as_deref() != Some(row.3.as_str()) {
            return Ok(false);
        }
        let run_basis: Option<ExternalRfiReviewRunBasisRow> = connection
            .query_row(
                "SELECT runs.status, runs.task_id, runs.permission_grant_json,
                    results.verification_status, results.payload_json,
                    results.data_scopes_json, results.data_classification, results.created_at,
                    runs.started_at
             FROM agent_runs AS runs
             JOIN proposed_agent_results AS results ON results.run_id = runs.run_id
             WHERE runs.run_id = ?1 AND runs.profile_id = ?2 AND runs.profile_version = ?3",
                params![row.4, row.5, row.6],
                |record| {
                    Ok(ExternalRfiReviewRunBasisRow {
                        status: record.get(0)?,
                        task_id: record.get(1)?,
                        permission_grant_json: record.get(2)?,
                        result_status: record.get(3)?,
                        payload_json: record.get(4)?,
                        result_scopes_json: record.get(5)?,
                        result_classification: record.get(6)?,
                        result_created_at: record.get(7)?,
                        run_started_at: record.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(sql_error)?;
        let Some(ExternalRfiReviewRunBasisRow {
            status,
            task_id,
            permission_grant_json,
            result_status,
            payload_json,
            result_scopes_json,
            result_classification,
            result_created_at,
            run_started_at,
        }) = run_basis
        else {
            return Ok(false);
        };
        let profile = load_profile(connection, (row.5.clone(), row.6))?;
        let task = load_task(connection, &task_id)?;
        let permission_grant: PermissionGrant = parse_canonical(&permission_grant_json)?;
        let result_scopes: Vec<String> = parse_canonical(&result_scopes_json)?;
        let result_classification = DataClassification::parse(&result_classification)?;
        let candidate: ExternalRfiReviewCandidate = serde_json::from_str(&payload_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let external_targets = task
            .exact_inputs
            .iter()
            .filter(|input| {
                input.kind == "external_rfi_version"
                    && input.reference == row.1
                    && input.version == row.2
            })
            .count();
        let tender_revisions = task
            .exact_inputs
            .iter()
            .filter(|input| input.kind == "tender_revision")
            .collect::<Vec<_>>();
        let approved_work_plan_profile = task
            .exact_inputs
            .iter()
            .filter(|input| input.kind == "work_plan_version")
            .collect::<Vec<_>>();
        let rfi_basis: Option<(String, String, String)> = connection
            .query_row(
                "SELECT query_refs_json, source_evidence_json, attachments_json
                 FROM external_rfi_versions WHERE rfi_id = ?1 AND version = ?2",
                params![row.1, row.2],
                |record| Ok((record.get(0)?, record.get(1)?, record.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((query_refs_json, source_evidence_json, attachments_json)) = rfi_basis else {
            return Ok(false);
        };
        let query_refs: Vec<ExternalRfiQueryReference> = parse_canonical(&query_refs_json)?;
        let source_evidence: Vec<AgentTaskInputReference> = parse_canonical(&source_evidence_json)?;
        let attachments: Vec<AgentTaskInputReference> = parse_canonical(&attachments_json)?;
        let started_event: (u32, Option<String>, Option<u32>, Option<String>) = connection
            .query_row(
                "SELECT COUNT(*), MIN(json_extract(payload_json, '$.tender_id')),
                        MIN(aggregate_revision), MIN(payload_json)
                 FROM audit_events
                 WHERE event_type = 'external_rfi_review_started'
                   AND json_extract(payload_json, '$.change.run_id') = ?1",
                [&row.4],
                |record| {
                    Ok((
                        record.get(0)?,
                        record.get(1)?,
                        record.get(2)?,
                        record.get(3)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        if started_event.0 != 1
            || tender_revisions.len() != 1
            || approved_work_plan_profile.len() != 1
        {
            return Ok(false);
        }
        let Some(started_tender_id) = started_event.1 else {
            return Ok(false);
        };
        let Some(started_tender_revision) = started_event.2 else {
            return Ok(false);
        };
        let Some(started_payload_json) = started_event.3 else {
            return Ok(false);
        };
        let expected_started_change = json!({
            "reviewer_profile_id": row.5,
            "rfi_id": row.1,
            "rfi_version": row.2.to_string(),
            "run_id": row.4,
            "task_id": task_id,
        });
        let started_payload: serde_json::Value = serde_json::from_str(&started_payload_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let mut expected_inputs = vec![
            AgentTaskInputReference {
                kind: "external_rfi_version".into(),
                reference: row.1.clone(),
                version: row.2,
            },
            tender_revisions[0].clone(),
            approved_work_plan_profile[0].clone(),
        ];
        expected_inputs.extend(query_refs.iter().map(|reference| AgentTaskInputReference {
            kind: "tender_query_version".into(),
            reference: reference.query_id.clone(),
            version: reference.version,
        }));
        expected_inputs.extend(source_evidence);
        expected_inputs.extend(attachments);
        expected_inputs.sort_by(|a, b| {
            (&a.kind, &a.reference, a.version).cmp(&(&b.kind, &b.reference, b.version))
        });
        expected_inputs.dedup();
        let expected_classification = profile
            .permissions
            .data_classifications
            .iter()
            .copied()
            .max()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let Some(review_basis_draft) = external_rfi_review_basis_draft(connection, &row.1, row.2)?
        else {
            return Ok(false);
        };
        let expected_review_payload = match external_rfi_review_data_view(
            connection,
            &review_basis_draft,
            &profile.permissions.data_scopes.join("+"),
            expected_classification,
            check,
        ) {
            Ok(payload) => payload,
            Err(error) if error.code == TenderErrorCode::OperationTimedOut => return Err(error),
            Err(_) => return Ok(false),
        };
        let expected_view_sha256 = sha256_hex(canonical_json(&expected_review_payload)?.as_bytes());
        let data_view = permission_grant.data_views.first();
        let mut finding_codes = HashSet::new();
        let valid_findings = findings.len() <= MAX_RFI_FINDINGS
            && findings.iter().all(|finding| {
                valid_external_rfi_finding_code(&finding.code)
                    && finding_codes.insert(finding.code.clone())
                    && !finding.summary.trim().is_empty()
                    && finding.summary.len() <= 2_000
                    && !finding.evidence_references.is_empty()
                    && finding.evidence_references.len() <= 32
                    && finding
                        .evidence_references
                        .iter()
                        .all(|reference| task.exact_inputs.contains(reference))
            });
        let valid_outcome = match outcome {
            ExternalRfiReviewOutcome::Passed => findings
                .iter()
                .all(|finding| finding.severity == ExternalRfiFindingSeverity::Minor),
            ExternalRfiReviewOutcome::Failed => findings.iter().any(|finding| {
                matches!(
                    finding.severity,
                    ExternalRfiFindingSeverity::Critical | ExternalRfiFindingSeverity::Major
                )
            }),
        };
        if status != "completed"
            || result_status != "proposed"
            || task.profile_id != row.5
            || task.profile_version != row.6
            || task.output_contract_json != external_rfi_review_output_contract()
            || task.permissions != profile.permissions
            || !profile
                .capabilities
                .iter()
                .any(|capability| capability == EXTERNAL_RFI_REVIEW_QUALIFICATION)
            || external_targets != 1
            || approved_work_plan_profile.len() != 1
            || tender_revisions[0].reference != started_tender_id
            || tender_revisions[0].version != started_tender_revision
            || started_payload.get("change") != Some(&expected_started_change)
            || task.exact_inputs != expected_inputs
            || !external_rfi_reviewer_was_plan_approved(
                connection,
                &approved_work_plan_profile[0].reference,
                approved_work_plan_profile[0].version,
                &row.5,
                row.6,
            )?
            || permission_grant.profile_id != row.5
            || permission_grant.profile_version != row.6
            || permission_grant.task_id != task_id
            || permission_grant.grant_id.len() != 32
            || !permission_grant
                .grant_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || permission_grant.policy_version != 1
            || permission_grant.capability_catalogue_version != 1
            || permission_grant.work_plan_version != approved_work_plan_profile[0].version
            || permission_grant.purpose != task.objective
            || permission_grant.data_scopes != profile.permissions.data_scopes
            || permission_grant.data_classifications != profile.permissions.data_classifications
            || permission_grant.allowed_actions != profile.permissions.allowed_actions
            || !permission_grant.typed_tools.is_empty()
            || permission_grant.network_allowed
            || !permission_grant.workspace_write_allowed
            || permission_grant.thread_exposure != ThreadExposureSet::from_grant(&permission_grant)
            || permission_grant.workspace.workspace_id != row.4
            || permission_grant.workspace.read_only_inputs != "inputs"
            || permission_grant.workspace.working_area != "working"
            || permission_grant.workspace.staged_outputs != "outputs"
            || permission_grant.access_ceiling.exact_inputs != task.exact_inputs
            || permission_grant.access_ceiling.data_scopes != profile.permissions.data_scopes
            || permission_grant.access_ceiling.data_classifications
                != profile.permissions.data_classifications
            || permission_grant.access_ceiling.allowed_actions
                != profile.permissions.allowed_actions
            || !permission_grant.access_ceiling.allowed_tools.is_empty()
            || permission_grant.resource_budget != task.resource_budget
            || permission_grant.issued_at != run_started_at
            || permission_grant.expires_at != task.deadline
            || permission_grant.data_views.len() != 1
            || data_view.is_none_or(|view| {
                view.exact_inputs != task.exact_inputs
                    || view.view_id != format!("production-task-{task_id}")
                    || view.schema_version != 1
                    || view.relative_path != "inputs/tender-metadata-v1.json"
                    || view.sha256 != expected_view_sha256
                    || view.data_scope != profile.permissions.data_scopes.join("+")
                    || view.data_classification != expected_classification
            })
            || result_scopes != permission_grant.data_scopes
            || result_classification != expected_classification
            || result_created_at != row.12
            || candidate.outcome != outcome
            || candidate.findings != findings
            || !valid_findings
            || !valid_outcome
        {
            return Ok(false);
        }
        let expected_manifest = canonical_json(&ExternalRfiReviewManifest {
            schema_version: 1,
            review_id: &row.0,
            rfi_id: &row.1,
            rfi_version: row.2,
            rfi_manifest_sha256: &row.3,
            reviewer_run_id: &row.4,
            reviewer_profile_id: &row.5,
            reviewer_profile_version: row.6,
            outcome,
            findings: &findings,
            created_at: &row.12,
        })?;
        let expected_change = json!({
            "manifest_sha256": row.11,
            "outcome": outcome.as_str(),
            "review_id": row.0,
            "reviewer_profile_id": row.5,
            "reviewer_profile_version": row.6.to_string(),
            "reviewer_run_id": row.4,
            "rfi_id": row.1,
            "rfi_manifest_sha256": row.3,
            "rfi_version": row.2.to_string(),
        });
        if row.10 != expected_manifest
            || row.11 != sha256_hex(expected_manifest.as_bytes())
            || !external_rfi_audit_is_exact(
                connection,
                row.9,
                "external_rfi_review_completed",
                &row.12,
                &expected_change,
            )?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn external_rfi_reviewer_was_plan_approved(
    connection: &rusqlite::Connection,
    plan_id: &str,
    plan_version: u32,
    profile_id: &str,
    profile_version: u32,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM work_plan_versions AS plans
               JOIN work_plan_approvals AS approvals
                 ON approvals.plan_id = plans.plan_id
                AND approvals.plan_version = plans.version
                AND approvals.decision = 'approve'
               JOIN production_activations AS activations
                 ON activations.plan_id = plans.plan_id
                AND activations.plan_version = plans.version
               WHERE plans.plan_id = ?1 AND plans.version = ?2
                 AND EXISTS (
                   SELECT 1 FROM json_each(plans.profiles_json) AS bindings
                   WHERE json_extract(bindings.value, '$.profile.profile_id') = ?3
                     AND json_extract(bindings.value, '$.profile.version') = ?4
                     AND EXISTS (
                       SELECT 1 FROM json_each(
                         json_extract(bindings.value, '$.profile.capabilities')
                       ) AS capabilities
                       WHERE capabilities.value = ?5
                     )
                 )
             )",
            params![
                plan_id,
                plan_version,
                profile_id,
                profile_version,
                EXTERNAL_RFI_REVIEW_QUALIFICATION,
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn external_rfi_audit_is_exact(
    connection: &rusqlite::Connection,
    sequence: i64,
    expected_event_type: &str,
    expected_created_at: &str,
    expected_change: &serde_json::Value,
) -> Result<bool, TenderCommandError> {
    let audit: Option<(String, String, String)> = connection
        .query_row(
            "SELECT event_type, payload_json, created_at FROM audit_events WHERE sequence = ?1",
            [sequence],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(audit.is_some_and(|(event_type, payload_json, created_at)| {
        event_type == expected_event_type
            && created_at == expected_created_at
            && serde_json::from_str::<serde_json::Value>(&payload_json)
                .is_ok_and(|payload| payload.get("change") == Some(expected_change))
    }))
}

fn external_rfi_approvals_are_valid(
    connection: &rusqlite::Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    type StoredApproval = (
        String,
        String,
        u32,
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let mut statement = connection
        .prepare(
            "SELECT approval_id, rfi_id, rfi_version, rfi_manifest_sha256,
                review_id, review_manifest_sha256, rationale, approved_by,
                acting_role, audit_sequence, manifest_json, approval_sha256,
                created_at
         FROM external_rfi_approvals ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
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
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let row: StoredApproval = row.map_err(sql_error)?;
        let review_basis: Option<(String, String, String, u32, u32)> = connection
            .query_row(
                "SELECT reviews.outcome, reviews.manifest_sha256, versions.manifest_sha256,
                        heads.current_version,
                        (SELECT MAX(candidate.version) FROM external_rfi_versions AS candidate
                         WHERE candidate.rfi_id = versions.rfi_id)
             FROM external_rfi_reviews AS reviews
             JOIN external_rfi_versions AS versions
               ON versions.rfi_id = reviews.rfi_id
              AND versions.version = reviews.rfi_version
             JOIN external_rfi_heads AS heads ON heads.rfi_id = versions.rfi_id
             WHERE reviews.review_id = ?1 AND reviews.rfi_id = ?2
               AND reviews.rfi_version = ?3",
                params![row.4, row.1, row.2],
                |record| {
                    Ok((
                        record.get(0)?,
                        record.get(1)?,
                        record.get(2)?,
                        record.get(3)?,
                        record.get(4)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?;
        if review_basis.as_ref().is_none_or(|basis| {
            basis.0 != "passed"
                || basis.1 != row.5
                || basis.2 != row.3
                || basis.3 != row.2
                || basis.4 != row.2
        }) || row.6.trim().is_empty()
            || row.6.len() > 4_000
            || row.7 != "engineer_user"
            || row.8 != "tendering_manager"
        {
            return Ok(false);
        }
        let expected_manifest = canonical_json(&ExternalRfiApprovalManifest {
            schema_version: 1,
            approval_id: &row.0,
            rfi_id: &row.1,
            rfi_version: row.2,
            rfi_manifest_sha256: &row.3,
            review_id: &row.4,
            review_manifest_sha256: &row.5,
            rationale: &row.6,
            approved_by: &row.7,
            acting_role: &row.8,
            created_at: &row.12,
        })?;
        let expected_change = json!({
            "acting_role": "tendering_manager",
            "approval_id": row.0,
            "approval_sha256": row.11,
            "approved_by": "engineer_user",
            "review_id": row.4,
            "rfi_id": row.1,
            "rfi_manifest_sha256": row.3,
            "rfi_version": row.2.to_string(),
        });
        if row.10 != expected_manifest
            || row.11 != sha256_hex(expected_manifest.as_bytes())
            || !external_rfi_audit_is_exact(
                connection,
                row.9,
                "external_rfi_approved_for_issue",
                &row.12,
                &expected_change,
            )?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn external_rfi_exports_are_valid(
    connection: &rusqlite::Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    type StoredExport = (
        String,
        String,
        String,
        String,
        i64,
        i64,
        String,
        String,
        String,
    );
    let mut statement = connection
        .prepare(
            "SELECT export_id, approval_id, relative_path, bytes_sha256, size_bytes,
                audit_sequence, manifest_json, manifest_sha256, created_at
         FROM external_rfi_exports ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
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
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let row: StoredExport = row.map_err(sql_error)?;
        let approval_basis: Option<(String, u32)> = connection
            .query_row(
                "SELECT rfi_id, rfi_version FROM external_rfi_approvals WHERE approval_id = ?1",
                [&row.1],
                |record| Ok((record.get(0)?, record.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((rfi_id, rfi_version)) = approval_basis else {
            return Ok(false);
        };
        let size_bytes = u64::try_from(row.4).ok();
        let relative_path = Path::new(&row.2);
        if size_bytes.is_none_or(|size| size == 0 || size > MAX_EXPORT_BYTES as u64)
            || row.3.len() != 64
            || relative_path.is_absolute()
            || relative_path
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Ok(false);
        }
        let size_bytes = size_bytes.expect("checked External RFI export size");
        let expected_manifest = canonical_json(&ExternalRfiExportManifest {
            schema_version: 1,
            export_id: &row.0,
            approval_id: &row.1,
            relative_path: &row.2,
            bytes_sha256: &row.3,
            size_bytes,
            created_at: &row.8,
        })?;
        let expected_change = json!({
            "approval_id": row.1,
            "bytes_sha256": row.3,
            "export_id": row.0,
            "manifest_sha256": row.7,
            "rfi_id": rfi_id,
            "rfi_version": rfi_version.to_string(),
            "size_bytes": size_bytes.to_string(),
        });
        if row.6 != expected_manifest
            || row.7 != sha256_hex(expected_manifest.as_bytes())
            || !external_rfi_audit_is_exact(
                connection,
                row.5,
                "external_rfi_exported_for_human_issue",
                &row.8,
                &expected_change,
            )?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn external_rfi_responses_are_valid(
    connection: &rusqlite::Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    type StoredResponse = (
        String,
        String,
        u32,
        String,
        String,
        u32,
        String,
        i64,
        String,
        String,
        String,
    );
    let mut statement = connection
        .prepare(
            "SELECT response_link_id, rfi_id, rfi_version, approval_id,
                source_artifact_id, source_artifact_version, registered_by,
                audit_sequence, manifest_json, manifest_sha256, created_at
         FROM external_rfi_responses ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
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
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let row: StoredResponse = row.map_err(sql_error)?;
        let valid_basis: bool = connection
            .query_row(
                "SELECT EXISTS(
               SELECT 1 FROM external_rfi_approvals AS approvals
               JOIN source_artifact_versions AS sources
                 ON sources.artifact_id = ?4 AND sources.version = ?5
               JOIN source_artifacts AS artifacts ON artifacts.artifact_id = sources.artifact_id
               JOIN intake_runs AS intake ON intake.intake_id = artifacts.intake_id
               WHERE approvals.approval_id = ?1 AND approvals.rfi_id = ?2
                 AND approvals.rfi_version = ?3 AND sources.registration_state = 'registered'
                 AND EXISTS (
                   SELECT 1 FROM audit_events AS intake_events
                   WHERE intake_events.event_type = 'tender_package_imported'
                     AND intake_events.sequence > approvals.audit_sequence
                     AND json_extract(intake_events.payload_json, '$.change.intake_id') = intake.intake_id
                 )
                 AND (
                   SELECT COUNT(*) FROM external_rfi_responses AS linked
                   WHERE linked.source_artifact_id = sources.artifact_id
                     AND linked.source_artifact_version = sources.version
                 ) = 1
             )",
                params![row.3, row.1, row.2, row.4, row.5],
                |record| record.get(0),
            )
            .map_err(sql_error)?;
        if !valid_basis || row.6 != "engineer_user" {
            return Ok(false);
        }
        let expected_manifest = canonical_json(&ExternalRfiResponseManifest {
            schema_version: 1,
            response_link_id: &row.0,
            rfi_id: &row.1,
            rfi_version: row.2,
            approval_id: &row.3,
            source_artifact_id: &row.4,
            source_artifact_version: row.5,
            registered_by: &row.6,
            created_at: &row.10,
        })?;
        let expected_change = json!({
            "approval_id": row.3,
            "manifest_sha256": row.9,
            "response_link_id": row.0,
            "rfi_id": row.1,
            "rfi_version": row.2.to_string(),
            "source_artifact_id": row.4,
            "source_artifact_version": row.5.to_string(),
        });
        if row.8 != expected_manifest
            || row.9 != sha256_hex(expected_manifest.as_bytes())
            || !external_rfi_audit_is_exact(
                connection,
                row.7,
                "external_rfi_response_registered",
                &row.10,
                &expected_change,
            )?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn external_rfi_interpretations_are_valid(
    connection: &rusqlite::Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    type StoredInterpretation = (
        String,
        String,
        String,
        u32,
        u32,
        u32,
        String,
        bool,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let mut statement = connection
        .prepare(
            "SELECT interpretation_id, response_link_id, query_id,
                source_query_version, base_query_version, resulting_query_version, query_decision_id,
                material, interpretation, decided_by, acting_role,
                audit_sequence, manifest_json, manifest_sha256, created_at
         FROM external_rfi_response_interpretations ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
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
                row.get(14)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        check()?;
        let row: StoredInterpretation = row.map_err(sql_error)?;
        let basis: Option<(String, u32, String, u32, String, String, bool)> = connection
            .query_row(
                "SELECT responses.rfi_id, responses.rfi_version,
                    responses.source_artifact_id, responses.source_artifact_version,
                    versions.evidence_json, versions.responses_json, versions.material
             FROM external_rfi_responses AS responses
             JOIN external_rfi_versions AS rfi_versions
               ON rfi_versions.rfi_id = responses.rfi_id
              AND rfi_versions.version = responses.rfi_version
             JOIN tender_query_versions AS versions
               ON versions.query_id = ?2 AND versions.version = ?3
             JOIN tender_query_treatment_decisions AS decisions
               ON decisions.decision_id = ?4
              AND decisions.query_id = ?2
              AND decisions.query_version = ?3
             WHERE responses.response_link_id = ?1
               AND EXISTS (
                 SELECT 1 FROM json_each(rfi_versions.query_refs_json) AS refs
                 WHERE json_extract(refs.value, '$.query_id') = ?2
                   AND json_extract(refs.value, '$.version') = ?5
               )",
                params![row.1, row.2, row.5, row.6, row.3],
                |record| {
                    Ok((
                        record.get(0)?,
                        record.get(1)?,
                        record.get(2)?,
                        record.get(3)?,
                        record.get(4)?,
                        record.get(5)?,
                        record.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?;
        let Some((
            _rfi_id,
            _rfi_version,
            source_artifact_id,
            source_artifact_version,
            evidence_json,
            responses_json,
            query_material,
        )) = basis
        else {
            return Ok(false);
        };
        let evidence: Vec<AgentTaskInputReference> = parse_canonical(&evidence_json)?;
        let responses: Vec<super::TenderQueryResponse> = parse_canonical(&responses_json)?;
        let response_reference = AgentTaskInputReference {
            kind: "source_artifact".into(),
            reference: source_artifact_id,
            version: source_artifact_version,
        };
        if row.5 != row.4.saturating_add(1)
            || row.4 < row.3
            || row.8.trim().is_empty()
            || row.8.len() > 8_000
            || row.9 != "engineer_user"
            || row.10 != "tendering_manager"
            || query_material != row.7
            || !evidence.contains(&response_reference)
            || !responses.iter().any(|response| {
                response.response == row.8
                    && response.registered_by == "engineer_user"
                    && response.created_at == row.14
                    && response.evidence == [response_reference.clone()]
            })
        {
            return Ok(false);
        }
        let expected_manifest = canonical_json(&ExternalRfiInterpretationManifest {
            schema_version: 1,
            interpretation_id: &row.0,
            response_link_id: &row.1,
            query_id: &row.2,
            source_query_version: row.3,
            base_query_version: row.4,
            resulting_query_version: row.5,
            query_decision_id: &row.6,
            material: row.7,
            interpretation: &row.8,
            decided_by: &row.9,
            acting_role: &row.10,
            created_at: &row.14,
        })?;
        let expected_change = json!({
            "acting_role": "tendering_manager",
            "base_query_version": row.4.to_string(),
            "decided_by": "engineer_user",
            "interpretation_id": row.0,
            "manifest_sha256": row.13,
            "material": row.7,
            "query_decision_id": row.6,
            "query_id": row.2,
            "response_link_id": row.1,
            "source_query_version": row.3.to_string(),
            "resulting_query_version": row.5.to_string(),
        });
        if row.12 != expected_manifest
            || row.13 != sha256_hex(expected_manifest.as_bytes())
            || !external_rfi_audit_is_exact(
                connection,
                row.11,
                "external_rfi_response_interpreted",
                &row.14,
                &expected_change,
            )?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn reference_order(
    left: &AgentTaskInputReference,
    right: &AgentTaskInputReference,
) -> std::cmp::Ordering {
    (&left.kind, &left.reference, left.version).cmp(&(&right.kind, &right.reference, right.version))
}

fn references_are_sorted_unique(references: &[AgentTaskInputReference]) -> bool {
    references
        .windows(2)
        .all(|pair| reference_order(&pair[0], &pair[1]).is_lt())
}

fn strings_are_sorted_unique(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn exact_external_rfi_target(
    task: &TenderTaskView,
) -> Result<(&str, u32, u32), TenderCommandError> {
    let targets = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "external_rfi_version")
        .collect::<Vec<_>>();
    let tender_revisions = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "tender_revision")
        .collect::<Vec<_>>();
    if targets.len() != 1 || tender_revisions.len() != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((
        targets[0].reference.as_str(),
        targets[0].version,
        tender_revisions[0].version,
    ))
}

pub(crate) fn external_rfi_review_target_is_open(
    transaction: &Transaction<'_>,
    task: &TenderTaskView,
) -> Result<bool, TenderCommandError> {
    let (rfi_id, version, _) = exact_external_rfi_target(task)?;
    let query_refs_json: Option<String> = transaction
        .query_row(
            "SELECT versions.query_refs_json FROM external_rfi_versions AS versions
             JOIN external_rfi_heads AS heads ON heads.rfi_id = versions.rfi_id
             WHERE versions.rfi_id = ?1 AND versions.version = ?2
               AND heads.current_version = versions.version
               AND NOT EXISTS (
                 SELECT 1 FROM external_rfi_reviews
                 WHERE external_rfi_reviews.rfi_id = versions.rfi_id
                   AND external_rfi_reviews.rfi_version = versions.version
               )
               AND NOT EXISTS (
                 SELECT 1 FROM external_rfi_approvals
                 WHERE external_rfi_approvals.rfi_id = versions.rfi_id
                   AND external_rfi_approvals.rfi_version = versions.version
               )",
            params![rfi_id, version],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(query_refs_json) = query_refs_json else {
        return Ok(false);
    };
    let query_refs: Vec<ExternalRfiQueryReference> = parse_canonical(&query_refs_json)?;
    Ok(
        external_rfi_query_refs_are_current(transaction, &query_refs)?
            && external_rfi_review_authority_is_active(transaction, task)?,
    )
}

fn external_rfi_review_authority_is_active(
    connection: &rusqlite::Connection,
    task: &TenderTaskView,
) -> Result<bool, TenderCommandError> {
    let work_plan_inputs = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "work_plan_version")
        .collect::<Vec<_>>();
    if work_plan_inputs.len() != 1 {
        return Ok(false);
    }
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM production_activations AS activations
               JOIN agent_profile_heads AS profile_heads
                 ON profile_heads.profile_id = ?3
                AND profile_heads.current_version = ?4
                AND profile_heads.status = 'active'
               WHERE activations.plan_id = ?1
                 AND activations.plan_version = ?2
                 AND activations.status = 'active'
             )",
            params![
                work_plan_inputs[0].reference,
                work_plan_inputs[0].version,
                task.profile_id,
                task.profile_version,
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

pub(crate) struct ExternalRfiReviewPublication<'a> {
    pub tender_id: &'a TenderId,
    pub tender_revision: u32,
    pub reviewer_run_id: &'a str,
    pub reviewer_profile: &'a AgentProfileVersionView,
    pub task: &'a TenderTaskView,
    pub created_at: &'a str,
}

pub(crate) fn publish_external_rfi_review(
    transaction: &Transaction<'_>,
    publication: ExternalRfiReviewPublication<'_>,
    candidate: &ExternalRfiReviewCandidate,
) -> Result<(), TenderCommandError> {
    let ExternalRfiReviewPublication {
        tender_id,
        tender_revision,
        reviewer_run_id,
        reviewer_profile,
        task,
        created_at,
    } = publication;
    let (rfi_id, rfi_version, bound_tender_revision) = exact_external_rfi_target(task)?;
    if bound_tender_revision != tender_revision
        || !reviewer_profile
            .capabilities
            .iter()
            .any(|capability| capability == EXTERNAL_RFI_REVIEW_QUALIFICATION)
        || reviewer_profile.profile_id != task.profile_id
        || reviewer_profile.version != task.profile_version
        || !external_rfi_review_target_is_open(transaction, task)?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let rfi_manifest_sha256: String = transaction
        .query_row(
            "SELECT manifest_sha256 FROM external_rfi_versions
             WHERE rfi_id = ?1 AND version = ?2",
            params![rfi_id, rfi_version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let review_id = random_identifier(transaction)?;
    let manifest = ExternalRfiReviewManifest {
        schema_version: 1,
        review_id: &review_id,
        rfi_id,
        rfi_version,
        rfi_manifest_sha256: &rfi_manifest_sha256,
        reviewer_run_id,
        reviewer_profile_id: &reviewer_profile.profile_id,
        reviewer_profile_version: reviewer_profile.version,
        outcome: candidate.outcome,
        findings: &candidate.findings,
        created_at,
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        "external_rfi_review_completed",
        tender_revision,
        json!({
            "manifest_sha256": manifest_sha256,
            "outcome": candidate.outcome.as_str(),
            "review_id": review_id,
            "reviewer_profile_id": reviewer_profile.profile_id,
            "reviewer_profile_version": reviewer_profile.version.to_string(),
            "reviewer_run_id": reviewer_run_id,
            "rfi_id": rfi_id,
            "rfi_manifest_sha256": rfi_manifest_sha256,
            "rfi_version": rfi_version.to_string(),
        }),
        created_at,
    )?;
    transaction
        .execute(
            "INSERT INTO external_rfi_reviews (
               review_id, rfi_id, rfi_version, rfi_manifest_sha256,
               reviewer_run_id, reviewer_profile_id, reviewer_profile_version,
               outcome, findings_json, audit_sequence, manifest_json,
               manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                review_id,
                rfi_id,
                rfi_version,
                rfi_manifest_sha256,
                reviewer_run_id,
                reviewer_profile.profile_id,
                reviewer_profile.version,
                candidate.outcome.as_str(),
                canonical_json(&candidate.findings)?,
                audit_sequence,
                manifest_json,
                manifest_sha256,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

struct ExternalRfiQueryBasisRow {
    question: String,
    ambiguity_or_gap: String,
    evidence_json: String,
    tasks_json: String,
    manifest_sha256: String,
    head: u32,
    treatment: String,
    closes_query: bool,
}

fn materialize_external_rfi_candidate(
    transaction: &Transaction<'_>,
    candidate: ExternalRfiCandidate<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<MaterializedExternalRfiCandidate, TenderCommandError> {
    check()?;
    if candidate.query_refs.is_empty()
        || candidate.query_refs.len() > MAX_RFI_QUERY_REFS
        || candidate.additional_evidence.len() > MAX_RFI_SOURCE_EVIDENCE
        || candidate.attachments.len() > MAX_RFI_ATTACHMENTS
        || candidate.affected_commitments.len() > MAX_RFI_COMMITMENTS
        || candidate.contractual_context.trim().is_empty()
        || candidate.contractual_context.len() > 8_000
        || candidate.response_need.trim().is_empty()
        || candidate.response_need.len() > 4_000
        || Timestamp::from_str(candidate.due_at).is_err()
        || !valid_recipient(candidate.recipient)
        || !valid_bounded_unique_text(candidate.affected_commitments, 4_000)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut seen_queries = HashSet::new();
    let mut normalized_refs = Vec::with_capacity(candidate.query_refs.len());
    let mut questions = Vec::with_capacity(candidate.query_refs.len());
    let mut source_evidence = Vec::new();
    let mut affected_task_keys = Vec::new();
    for reference in candidate.query_refs {
        check()?;
        if !super::valid_identifier(&reference.query_id)
            || reference.version == 0
            || reference.version > 32
            || reference.manifest_sha256.len() != 64
            || !seen_queries.insert(reference.query_id.clone())
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let row: Option<ExternalRfiQueryBasisRow> = transaction
            .query_row(
                "SELECT versions.question, versions.ambiguity_or_gap, versions.evidence_json,
                        versions.affected_task_keys_json, versions.manifest_sha256,
                        heads.current_version, decisions.treatment, decisions.closes_query
                 FROM tender_query_versions AS versions
                 JOIN tender_query_heads AS heads ON heads.query_id = versions.query_id
                 LEFT JOIN tender_query_treatment_decisions AS decisions
                   ON decisions.query_id = versions.query_id
                  AND decisions.query_version = versions.version
                 WHERE versions.query_id = ?1 AND versions.version = ?2",
                params![reference.query_id, reference.version],
                |row| {
                    Ok(ExternalRfiQueryBasisRow {
                        question: row.get(0)?,
                        ambiguity_or_gap: row.get(1)?,
                        evidence_json: row.get(2)?,
                        tasks_json: row.get(3)?,
                        manifest_sha256: row.get(4)?,
                        head: row.get(5)?,
                        treatment: row.get(6)?,
                        closes_query: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(sql_error)?;
        let Some(row) = row else {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        if row.head != reference.version
            || row.manifest_sha256 != reference.manifest_sha256
            || row.treatment != "external_rfi_drafting"
            || row.closes_query
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let evidence: Vec<AgentTaskInputReference> = parse_canonical(&row.evidence_json)?;
        let tasks: Vec<String> = parse_canonical(&row.tasks_json)?;
        for item in evidence {
            if !source_evidence.contains(&item) {
                source_evidence.push(item);
            }
        }
        for task in tasks {
            if !affected_task_keys.contains(&task) {
                affected_task_keys.push(task);
            }
        }
        normalized_refs.push(reference.clone());
        questions.push(ExternalRfiQuestion {
            query_id: reference.query_id.clone(),
            query_version: reference.version,
            question: row.question,
            ambiguity_or_gap: row.ambiguity_or_gap,
        });
    }
    for reference in candidate.additional_evidence {
        check()?;
        if !query_evidence_reference_exists(transaction, reference)? {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if !source_evidence.contains(reference) {
            source_evidence.push(reference.clone());
        }
    }
    for reference in candidate.attachments {
        check()?;
        if !attachment_reference_exists(transaction, reference)? {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    if source_evidence.is_empty()
        || source_evidence.len() > MAX_RFI_SOURCE_EVIDENCE
        || affected_task_keys.is_empty()
        || affected_task_keys.len() > MAX_RFI_AFFECTED_TASKS
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    normalized_refs.sort_by(|a, b| a.query_id.cmp(&b.query_id));
    questions.sort_by(|a, b| a.query_id.cmp(&b.query_id));
    source_evidence.sort_by(|a, b| {
        (&a.kind, &a.reference, a.version).cmp(&(&b.kind, &b.reference, b.version))
    });
    affected_task_keys.sort();
    Ok(MaterializedExternalRfiCandidate {
        query_refs: normalized_refs,
        questions,
        source_evidence,
        affected_task_keys,
    })
}

fn insert_external_rfi_version(
    transaction: &Transaction<'_>,
    rfi_id: &str,
    version: u32,
    materialized: &MaterializedExternalRfiCandidate,
    candidate: ExternalRfiCandidate<'_>,
    created_at: &str,
) -> Result<String, TenderCommandError> {
    let mut attachments = candidate.attachments.to_vec();
    attachments.sort_by(|a, b| {
        (&a.kind, &a.reference, a.version).cmp(&(&b.kind, &b.reference, b.version))
    });
    attachments.dedup();
    let mut commitments = candidate
        .affected_commitments
        .iter()
        .map(|value| value.trim().to_owned())
        .collect::<Vec<_>>();
    commitments.sort();
    commitments.dedup();
    let recipient = ExternalRfiRecipient {
        organization: candidate.recipient.organization.trim().to_owned(),
        attention: candidate.recipient.attention.trim().to_owned(),
        email: candidate
            .recipient
            .email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    };
    let manifest = ExternalRfiVersionManifest {
        schema_version: 1,
        rfi_id,
        version,
        query_refs: &materialized.query_refs,
        questions: &materialized.questions,
        source_evidence: &materialized.source_evidence,
        contractual_context: candidate.contractual_context.trim(),
        response_need: candidate.response_need.trim(),
        attachments: &attachments,
        due_at: candidate.due_at,
        recipient: &recipient,
        affected_task_keys: &materialized.affected_task_keys,
        affected_commitments: &commitments,
        created_by: "engineer_user",
        created_at,
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    transaction
        .execute(
            "INSERT INTO external_rfi_versions (
               rfi_id, version, query_refs_json, questions_json, source_evidence_json,
               contractual_context, response_need, attachments_json, due_at,
               recipient_json, affected_task_keys_json, affected_commitments_json,
               created_by, manifest_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
                       ?10, ?11, ?12, 'engineer_user', ?13, ?14, ?15)",
            params![
                rfi_id,
                version,
                canonical_json(&materialized.query_refs)?,
                canonical_json(&materialized.questions)?,
                canonical_json(&materialized.source_evidence)?,
                candidate.contractual_context.trim(),
                candidate.response_need.trim(),
                canonical_json(&attachments)?,
                candidate.due_at,
                canonical_json(&recipient)?,
                canonical_json(&materialized.affected_task_keys)?,
                canonical_json(&commitments)?,
                manifest_json,
                manifest_sha256,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(manifest_sha256)
}

fn valid_recipient(recipient: &ExternalRfiRecipient) -> bool {
    let organization = recipient.organization.trim();
    let attention = recipient.attention.trim();
    let email_valid = recipient.email.as_deref().is_none_or(|email| {
        let email = email.trim();
        email.is_empty()
            || (email.len() <= 320
                && !email.chars().any(char::is_whitespace)
                && email.split_once('@').is_some_and(|(local, domain)| {
                    !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
                }))
    });
    !organization.is_empty()
        && organization.len() <= 500
        && !attention.is_empty()
        && attention.len() <= 500
        && email_valid
}

fn valid_external_rfi_finding_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_bounded_unique_text(values: &[String], max_bytes: usize) -> bool {
    let mut seen = HashSet::new();
    values.iter().all(|value| {
        let value = value.trim();
        !value.is_empty() && value.len() <= max_bytes && seen.insert(value.to_owned())
    })
}

fn attachment_reference_exists(
    connection: &rusqlite::Connection,
    reference: &AgentTaskInputReference,
) -> Result<bool, TenderCommandError> {
    if reference.kind != "source_artifact" || !super::valid_identifier(&reference.reference) {
        return Ok(false);
    }
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM source_artifact_versions
             WHERE artifact_id = ?1 AND version = ?2 AND registration_state = 'registered')",
            params![reference.reference, reference.version],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn external_rfi_query_refs_are_current(
    connection: &rusqlite::Connection,
    references: &[ExternalRfiQueryReference],
) -> Result<bool, TenderCommandError> {
    if references.is_empty() || references.len() > MAX_RFI_QUERY_REFS {
        return Ok(false);
    }
    for reference in references {
        let current: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM tender_query_versions AS versions
                   JOIN tender_query_heads AS heads ON heads.query_id = versions.query_id
                   JOIN tender_query_treatment_decisions AS decisions
                     ON decisions.query_id = versions.query_id
                    AND decisions.query_version = versions.version
                   WHERE versions.query_id = ?1 AND versions.version = ?2
                     AND versions.manifest_sha256 = ?3
                     AND heads.current_version = versions.version
                     AND decisions.treatment = 'external_rfi_drafting'
                     AND decisions.closes_query = 0
                 )",
                params![
                    reference.query_id,
                    reference.version,
                    reference.manifest_sha256,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !current {
            return Ok(false);
        }
    }
    Ok(true)
}

fn current_external_rfi_query_refs(
    connection: &rusqlite::Connection,
    issued_references: &[ExternalRfiQueryReference],
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(Vec<ExternalRfiQueryReference>, bool), TenderCommandError> {
    if issued_references.is_empty() || issued_references.len() > MAX_RFI_QUERY_REFS {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let mut current_references = Vec::with_capacity(issued_references.len());
    let mut revision_allowed = true;
    for issued in issued_references {
        check()?;
        let current: Option<(u32, String, Option<String>, Option<bool>)> = connection
            .query_row(
                "SELECT heads.current_version, versions.manifest_sha256,
                        decisions.treatment, decisions.closes_query
                 FROM tender_query_heads AS heads
                 JOIN tender_query_versions AS versions
                   ON versions.query_id = heads.query_id
                  AND versions.version = heads.current_version
                 LEFT JOIN tender_query_treatment_decisions AS decisions
                   ON decisions.query_id = versions.query_id
                  AND decisions.query_version = versions.version
                 WHERE heads.query_id = ?1",
                [&issued.query_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((version, manifest_sha256, treatment, closes_query)) = current else {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        };
        revision_allowed &=
            treatment.as_deref() == Some("external_rfi_drafting") && closes_query == Some(false);
        current_references.push(ExternalRfiQueryReference {
            query_id: issued.query_id.clone(),
            version,
            manifest_sha256,
        });
    }
    Ok((current_references, revision_allowed))
}

fn load_external_rfi_review(
    connection: &rusqlite::Connection,
    rfi_id: &str,
    version: u32,
) -> Result<Option<ExternalRfiReview>, TenderCommandError> {
    let row = connection
        .query_row(
            "SELECT review_id, rfi_manifest_sha256, reviewer_run_id,
                    reviewer_profile_id, reviewer_profile_version, outcome,
                    findings_json, manifest_sha256, created_at
             FROM external_rfi_reviews WHERE rfi_id = ?1 AND rfi_version = ?2",
            params![rfi_id, version],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    row.map(|row| {
        Ok(ExternalRfiReview {
            review_id: row.0,
            rfi_id: rfi_id.to_owned(),
            rfi_version: version,
            rfi_manifest_sha256: row.1,
            reviewer_run_id: row.2,
            reviewer_profile_id: row.3,
            reviewer_profile_version: row.4,
            outcome: ExternalRfiReviewOutcome::parse(&row.5)?,
            findings: parse_canonical(&row.6)?,
            manifest_sha256: row.7,
            created_at: row.8,
        })
    })
    .transpose()
}

fn load_external_rfi_approval(
    connection: &rusqlite::Connection,
    rfi_id: &str,
    version: u32,
) -> Result<Option<ExternalRfiApproval>, TenderCommandError> {
    connection
        .query_row(
            "SELECT approval_id, rfi_manifest_sha256, review_id,
                    review_manifest_sha256, rationale, approved_by,
                    acting_role, approval_sha256, created_at
             FROM external_rfi_approvals WHERE rfi_id = ?1 AND rfi_version = ?2",
            params![rfi_id, version],
            |row| {
                Ok(ExternalRfiApproval {
                    approval_id: row.get(0)?,
                    rfi_id: rfi_id.to_owned(),
                    rfi_version: version,
                    rfi_manifest_sha256: row.get(1)?,
                    review_id: row.get(2)?,
                    review_manifest_sha256: row.get(3)?,
                    rationale: row.get(4)?,
                    approved_by: row.get(5)?,
                    acting_role: row.get(6)?,
                    approval_sha256: row.get(7)?,
                    created_at: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)
}

fn load_external_rfi_exports(
    connection: &rusqlite::Connection,
    tender_root: &Path,
    approval: Option<&ExternalRfiApproval>,
) -> Result<Vec<ExternalRfiExportRecord>, TenderCommandError> {
    let Some(approval) = approval else {
        return Ok(Vec::new());
    };
    let application_home = tender_root
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut statement = connection
        .prepare(
            "SELECT export_id, relative_path, bytes_sha256, size_bytes,
                    manifest_sha256, created_at
             FROM external_rfi_exports WHERE approval_id = ?1
             ORDER BY rowid DESC LIMIT 64",
        )
        .map_err(sql_error)?;
    let exports = statement
        .query_map([&approval.approval_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(sql_error)?
        .map(|row| {
            let row = row.map_err(sql_error)?;
            let size_bytes = u64::try_from(row.3)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let path = application_home.join("exports").join(&row.1);
            let bytes_verified = hash_file_bounded(&path, MAX_EXPORT_BYTES)
                .is_ok_and(|value| value == (row.2.clone(), size_bytes));
            Ok(ExternalRfiExportRecord {
                export_id: row.0,
                approval_id: approval.approval_id.clone(),
                path: path.to_string_lossy().into_owned(),
                bytes_sha256: row.2,
                size_bytes,
                bytes_verified,
                manifest_sha256: row.4,
                created_at: row.5,
            })
        })
        .collect::<Result<Vec<_>, TenderCommandError>>()?;
    Ok(exports)
}

fn load_external_rfi_responses(
    connection: &rusqlite::Connection,
    rfi_id: &str,
    version: u32,
) -> Result<Vec<ExternalRfiResponseLink>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT response_link_id, approval_id, source_artifact_id,
                    source_artifact_version, registered_by, manifest_sha256, created_at
             FROM external_rfi_responses WHERE rfi_id = ?1 AND rfi_version = ?2
             ORDER BY rowid LIMIT 64",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![rfi_id, version], |row| {
            Ok(ExternalRfiResponseLink {
                response_link_id: row.get(0)?,
                rfi_id: rfi_id.to_owned(),
                rfi_version: version,
                approval_id: row.get(1)?,
                source_artifact_id: row.get(2)?,
                source_artifact_version: row.get(3)?,
                registered_by: row.get(4)?,
                manifest_sha256: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok(rows)
}

fn load_external_rfi_interpretations(
    connection: &rusqlite::Connection,
    responses: &[ExternalRfiResponseLink],
) -> Result<Vec<ExternalRfiResponseInterpretation>, TenderCommandError> {
    let mut result = Vec::new();
    for response in responses {
        let mut statement = connection
            .prepare(
                "SELECT interpretation_id, query_id, source_query_version,
                        base_query_version, resulting_query_version, query_decision_id, material,
                        interpretation, decided_by, acting_role, manifest_sha256, created_at
                 FROM external_rfi_response_interpretations
                 WHERE response_link_id = ?1 ORDER BY rowid LIMIT 16",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([&response.response_link_id], |row| {
                Ok(ExternalRfiResponseInterpretation {
                    interpretation_id: row.get(0)?,
                    response_link_id: response.response_link_id.clone(),
                    query_id: row.get(1)?,
                    source_query_version: row.get(2)?,
                    base_query_version: row.get(3)?,
                    resulting_query_version: row.get(4)?,
                    query_decision_id: row.get(5)?,
                    material: row.get(6)?,
                    interpretation: row.get(7)?,
                    decided_by: row.get(8)?,
                    acting_role: row.get(9)?,
                    manifest_sha256: row.get(10)?,
                    created_at: row.get(11)?,
                })
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        result.extend(rows);
    }
    Ok(result)
}

fn render_external_rfi_text(draft: &ExternalRfiDraft) -> Result<Vec<u8>, TenderCommandError> {
    let approval = draft
        .approval
        .as_ref()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let mut text = String::new();
    text.push_str("EXTERNAL REQUEST FOR INFORMATION\n");
    text.push_str(&format!(
        "RFI ID: {}\nVersion: {}\n",
        draft.rfi_id, draft.version
    ));
    text.push_str(&format!("To: {}\n", draft.recipient.organization));
    text.push_str(&format!("Attention: {}\n", draft.recipient.attention));
    if let Some(email) = draft.recipient.email.as_deref() {
        if !email.is_empty() {
            text.push_str(&format!("Proposed email: {email}\n"));
        }
    }
    text.push_str(&format!("Response due: {}\n\n", draft.due_at));
    text.push_str("QUESTIONS\n");
    for (index, question) in draft.questions.iter().enumerate() {
        text.push_str(&format!(
            "{}. {}\n   Controlled gap: {}\n   Query: {} v{}\n",
            index + 1,
            question.question,
            question.ambiguity_or_gap,
            question.query_id,
            question.query_version,
        ));
    }
    text.push_str("\nCONTRACTUAL CONTEXT\n");
    text.push_str(&draft.contractual_context);
    text.push_str("\n\nRESPONSE NEEDED\n");
    text.push_str(&draft.response_need);
    text.push_str("\n\nEXACT EVIDENCE\n");
    for evidence in &draft.source_evidence {
        text.push_str(&format!(
            "- {}:{}:v{}\n",
            evidence.kind, evidence.reference, evidence.version
        ));
    }
    text.push_str("\nATTACHMENTS\n");
    if draft.attachments.is_empty() {
        text.push_str("- None\n");
    } else {
        for attachment in &draft.attachments {
            text.push_str(&format!(
                "- {} v{}\n",
                attachment.reference, attachment.version
            ));
        }
    }
    text.push_str("\nAFFECTED WORK\n");
    for task in &draft.affected_task_keys {
        text.push_str(&format!("- {task}\n"));
    }
    text.push_str("\nAFFECTED COMMITMENTS\n");
    for commitment in &draft.affected_commitments {
        text.push_str(&format!("- {commitment}\n"));
    }
    text.push_str("\nQUANTIX CONTROL\n");
    text.push_str(&format!(
        "Draft manifest SHA-256: {}\n",
        draft.manifest_sha256
    ));
    text.push_str(&format!("Approval SHA-256: {}\n", approval.approval_sha256));
    text.push_str("Approved for human issue only. Quantix did not send or submit this RFI.\n");
    let bytes = text.into_bytes();
    if bytes.len() > MAX_EXPORT_BYTES {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(bytes)
}

fn validate_managed_export_root(
    application_home: &Path,
    exports_root: &Path,
) -> Result<(), TenderCommandError> {
    let home = fs::canonicalize(application_home).map_err(store_unavailable)?;
    let exports = fs::canonicalize(exports_root).map_err(store_unavailable)?;
    if exports.parent() != Some(home.as_path()) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let metadata = fs::symlink_metadata(exports_root).map_err(store_unavailable)?;
    if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn validate_export_directory(parent: &Path, directory: &Path) -> Result<(), TenderCommandError> {
    let metadata = fs::symlink_metadata(directory).map_err(store_unavailable)?;
    if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let canonical_parent = fs::canonicalize(parent).map_err(store_unavailable)?;
    let canonical_directory = fs::canonicalize(directory).map_err(store_unavailable)?;
    if canonical_directory.parent() != Some(canonical_parent.as_path()) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn hash_file_bounded(path: &Path, maximum: usize) -> Result<(String, u64), TenderCommandError> {
    let metadata = fs::symlink_metadata(path).map_err(store_unavailable)?;
    if metadata_is_unsafe_storage_link(&metadata)
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > maximum as u64
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let file = File::open(path).map_err(store_unavailable)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(store_unavailable)?;
    if bytes.len() as u64 != metadata.len() || bytes.len() > maximum {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((sha256_hex(&bytes), metadata.len()))
}

fn append_external_rfi_denial(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    command: &str,
    target_id: Option<&str>,
    reason: &str,
) -> Result<(), TenderCommandError> {
    let created_at = sqlite_timestamp(transaction)?;
    append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        "external_rfi_command_denied",
        current_tender_revision(transaction)?,
        json!({
            "command": command,
            "reason": reason,
            "target_id": target_id,
        }),
        &created_at,
    )?;
    Ok(())
}

fn append_external_rfi_event(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    event_type: &str,
    rfi_id: &str,
    version: u32,
    manifest_sha256: &str,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        event_type,
        current_tender_revision(transaction)?,
        json!({
            "created_by": "engineer_user",
            "manifest_sha256": manifest_sha256,
            "rfi_id": rfi_id,
            "rfi_version": version.to_string(),
        }),
        created_at,
    )?;
    Ok(())
}

fn current_tender_revision(transaction: &Transaction<'_>) -> Result<u32, TenderCommandError> {
    transaction
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn parse_canonical<T>(value: &str) -> Result<T, TenderCommandError>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let parsed = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}
