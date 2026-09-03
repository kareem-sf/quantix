use std::{collections::HashSet, fs, path::Path};

use jiff::Timestamp;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::{
    agent_runtime::{
        permissions::{derive_pre_bid_data_grant, permission_duration, PreBidDataGrantRequest},
        AgentProfileStatus, AgentProfileVersionView, AgentResourceBudget, AgentRunInspection,
        AgentRunPermissions, AgentTaskInputReference, BootstrapRole, DataClassification,
        PendingProviderEvent, PermissionGrant, PreparedAgentRun, ProviderEventKind,
        ProviderFailureCategory, TenderTaskView, VerificationStatus,
    },
    application_settings::AiExecutionSelection,
    document_parsing::{ParseSourceArtifactCommand, ParseState},
};

use super::{
    agent_records::{
        ensure_agent_run_capacity, insert_event, insert_profile, insert_profile_version,
        insert_task, load_profile, load_task, load_thread_exposure, update_profile_head,
    },
    append_audit_event, random_identifier, sha256_hex, sql_error, sqlite_timestamp,
    tender_records::{
        estimate_prepared_record_extraction_request_bytes,
        estimate_record_extraction_request_bytes, estimated_record_extraction_request_bytes,
        record_extraction_request_context_for_prepared_with_stored_data_views,
        record_extraction_request_hard_cap_bytes, record_extraction_request_plan_context_is_valid,
        RecordExtractionRequestEstimate, RecordExtractionRequestPlanContext,
    },
    TenderCommandError, TenderErrorCode, TenderEvidenceReference, TenderId, TenderRecordBasisKind,
    TenderRecordInspection, TenderRecordKind, TenderRecordVersionReference, TenderStore,
};

pub(crate) const MANAGER_INTAKE_CAPABILITY: &str = "present_manager_intake_outcome";
const MANAGER_INTAKE_SCOPE: &str = "manager_intake";
const MANAGER_INTAKE_ACTION: &str = "present_first_tender_decision";
const MANAGER_STABLE_IDENTITY: &str = BootstrapRole::TenderingManager.stable_identity();
const MAX_SUPPORTING_RECORDS: usize = 16;
const MAX_SUPPORTING_EVIDENCE: usize = 32;
const MAX_MESSAGE_REFERENCE_DETAIL: usize = 2_000;
const RECORD_EXTRACTION_ESTIMATOR_VERSION: &str = "canonical-provider-request-v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ManagerIntakeStage {
    WaitingForLocalTools,
    WaitingForProviderApproval,
    WaitingForProvider,
    PackageRegistered,
    ReadingDocuments,
    ExtractingTenderFacts,
    ReviewingTenderFacts,
    PreparingFirstDecision,
    WaitingForEngineer,
    BidDecisionReady,
    Failed,
}

impl ManagerIntakeStage {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::WaitingForLocalTools => "waiting_for_local_tools",
            Self::WaitingForProviderApproval => "waiting_for_provider_approval",
            Self::WaitingForProvider => "waiting_for_provider",
            Self::PackageRegistered => "package_registered",
            Self::ReadingDocuments => "reading_documents",
            Self::ExtractingTenderFacts => "extracting_tender_facts",
            Self::ReviewingTenderFacts => "reviewing_tender_facts",
            Self::PreparingFirstDecision => "preparing_first_decision",
            Self::WaitingForEngineer => "waiting_for_engineer",
            Self::BidDecisionReady => "bid_decision_ready",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "waiting_for_local_tools" => Ok(Self::WaitingForLocalTools),
            "waiting_for_provider_approval" => Ok(Self::WaitingForProviderApproval),
            "waiting_for_provider" => Ok(Self::WaitingForProvider),
            "package_registered" => Ok(Self::PackageRegistered),
            "reading_documents" => Ok(Self::ReadingDocuments),
            "extracting_tender_facts" => Ok(Self::ExtractingTenderFacts),
            "reviewing_tender_facts" => Ok(Self::ReviewingTenderFacts),
            "preparing_first_decision" => Ok(Self::PreparingFirstDecision),
            "waiting_for_engineer" => Ok(Self::WaitingForEngineer),
            "bid_decision_ready" => Ok(Self::BidDecisionReady),
            "failed" => Ok(Self::Failed),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }

    pub(crate) fn is_active(self) -> bool {
        matches!(
            self,
            Self::PackageRegistered
                | Self::ReadingDocuments
                | Self::ExtractingTenderFacts
                | Self::ReviewingTenderFacts
                | Self::PreparingFirstDecision
        )
    }

    pub(crate) fn is_resumable(self) -> bool {
        matches!(
            self,
            Self::WaitingForLocalTools
                | Self::WaitingForProviderApproval
                | Self::WaitingForProvider
        ) || self.is_active()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ManagerIntakeStatusKind {
    Waiting,
    Working,
    NeedsEngineer,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ManagerIntakeStatus {
    pub intake_run_id: String,
    pub stage: ManagerIntakeStage,
    pub status: ManagerIntakeStatusKind,
    pub label: String,
    pub summary: String,
    pub parseable_document_count: u32,
    pub parsed_document_count: u32,
    pub extraction_run_count: u32,
    pub blocking_agent_run_id: Option<String>,
    pub retry_not_before_epoch_seconds: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceMessageReferenceKind {
    AgentRun,
    ManagerIntakeOutcome,
    TenderRecord,
    SourceEvidence,
    ArtifactVersion,
    TenderTask,
}

impl WorkspaceMessageReferenceKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AgentRun => "agent_run",
            Self::ManagerIntakeOutcome => "manager_intake_outcome",
            Self::TenderRecord => "tender_record",
            Self::SourceEvidence => "source_evidence",
            Self::ArtifactVersion => "artifact_version",
            Self::TenderTask => "tender_task",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "agent_run" => Ok(Self::AgentRun),
            "manager_intake_outcome" => Ok(Self::ManagerIntakeOutcome),
            "tender_record" => Ok(Self::TenderRecord),
            "source_evidence" => Ok(Self::SourceEvidence),
            "artifact_version" => Ok(Self::ArtifactVersion),
            "tender_task" => Ok(Self::TenderTask),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceMessageReference {
    pub kind: WorkspaceMessageReferenceKind,
    pub reference: String,
    pub version: u32,
    pub evidence_ordinal: Option<u32>,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceTenderDocument {
    pub artifact_id: String,
    pub version: u32,
    pub package_path: String,
    pub document_type: String,
    pub media_type: Option<String>,
    pub sha256: Option<String>,
    pub size_bytes: u64,
    pub registration_state: super::RegistrationState,
    pub parse_state: ParseState,
    pub exception: Option<super::IntakeExceptionCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ManagerIntakeOutcomeKind {
    Question,
    BidDecision,
}

impl ManagerIntakeOutcomeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::BidDecision => "bid_decision",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ManagerBidRecommendation {
    Proceed,
    Hold,
    Decline,
}

impl ManagerBidRecommendation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Proceed => "proceed",
            Self::Hold => "hold",
            Self::Decline => "decline",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManagerIntakeCandidate {
    pub kind: ManagerIntakeOutcomeKind,
    pub question: Option<String>,
    pub recommendation: Option<ManagerBidRecommendation>,
    pub rationale: Option<String>,
    pub supporting_records: Vec<TenderRecordVersionReference>,
    pub supporting_evidence: Vec<TenderEvidenceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ManagerIntakeOutcomeManifest<'a> {
    schema_version: u32,
    outcome_id: &'a str,
    intake_run_id: &'a str,
    manager_run_id: &'a str,
    message_id: &'a str,
    kind: ManagerIntakeOutcomeKind,
    body: &'a str,
    question: &'a Option<String>,
    recommendation: &'a Option<ManagerBidRecommendation>,
    supporting_records: &'a [TenderRecordVersionReference],
    supporting_evidence: &'a [TenderEvidenceReference],
    created_at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ManagerIntakeBatchInputs {
    authorities: Vec<super::TenderRecordAuthorityReference>,
    evidence: Vec<TenderEvidenceReference>,
    request_context: RecordExtractionRequestPlanContext,
}

struct ManagerIntakePlanRequest<'a> {
    transaction: &'a Transaction<'a>,
    tender_id: &'a TenderId,
    tender_revision: u32,
    intake_run_id: &'a str,
    evidence: &'a [TenderEvidenceReference],
    authorities: &'a [super::TenderRecordAuthorityReference],
    selection: &'a AiExecutionSelection,
    byte_budget: u64,
}

struct ManagerIntakeExtractionPlanRunBinding {
    intake_run_id: String,
    batch_fingerprint: String,
    task_id: String,
    provider_selection_json: String,
    plan_request_context_sha256: String,
    run_request_context_json: String,
    run_request_context_sha256: String,
    estimated_request_bytes: i64,
    lineage_kind: String,
    source_run_id: Option<String>,
}

type ManagerIntakeTaskBatchInputs = (
    String,
    Vec<TenderEvidenceReference>,
    Vec<super::TenderRecordAuthorityReference>,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedManagerIntakeExtractionBatch {
    pub evidence: Vec<TenderEvidenceReference>,
    pub authorities: Vec<super::TenderRecordAuthorityReference>,
    pub fingerprint: String,
    pub estimated_request_bytes: u64,
    pub request_context: RecordExtractionRequestPlanContext,
}

pub(super) fn initialize_manager_intake_run(
    transaction: &Transaction<'_>,
    package_intake_id: &str,
    created_at: &str,
) -> Result<String, TenderCommandError> {
    let active: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM manager_intake_runs
               WHERE stage IN (
                 'waiting_for_local_tools', 'waiting_for_provider_approval',
                 'waiting_for_provider', 'package_registered', 'reading_documents',
                 'extracting_tender_facts',
                 'reviewing_tender_facts',
                 'preparing_first_decision'
               )
             )",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if active {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let intake_run_id = random_identifier(transaction)?;
    transaction
        .execute(
            "INSERT INTO manager_intake_runs (
               intake_run_id, package_intake_id, stage, created_at, updated_at
             ) VALUES (?1, ?2, 'waiting_for_local_tools', ?3, ?3)",
            params![intake_run_id, package_intake_id, created_at],
        )
        .map_err(sql_error)?;
    Ok(intake_run_id)
}

pub(super) fn initialize_manager_profile(
    transaction: &Transaction<'_>,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let profile = manager_intake_profile(random_identifier(transaction)?);
    insert_profile(transaction, MANAGER_STABLE_IDENTITY, &profile, created_at)
}

impl TenderStore {
    pub(crate) fn current_manager_intake_status(
        &self,
    ) -> Result<Option<ManagerIntakeStatus>, TenderCommandError> {
        let row = self
            .connection
            .query_row(
                "SELECT intake_run_id, stage, parseable_document_count,
                        parsed_document_count, extraction_run_count, failure_summary,
                        blocking_agent_run_id, retry_not_before_epoch_seconds
                 FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?;
        let Some((
            intake_run_id,
            stage,
            parseable,
            parsed,
            extraction_runs,
            failure,
            blocking_agent_run_id,
            retry_not_before_epoch_seconds,
        )) = row
        else {
            return Ok(None);
        };
        let stage = ManagerIntakeStage::parse(&stage)?;
        let (status, label, summary) = match stage {
            ManagerIntakeStage::WaitingForLocalTools => (
                ManagerIntakeStatusKind::Waiting,
                "Prepare document tools",
                "The Tender Package is registered safely. Prepare local document tools to begin inventory, parsing, OCR, and indexing.",
            ),
            ManagerIntakeStage::WaitingForProviderApproval => (
                ManagerIntakeStatusKind::Waiting,
                "Choose an AI provider",
                "Local document work is complete. Confirm the exact provider, model, and reasoning choice before any Tender content is sent.",
            ),
            ManagerIntakeStage::WaitingForProvider => (
                ManagerIntakeStatusKind::Waiting,
                "Waiting for AI provider",
                "The approved AI provider is unavailable. Tender records remain accessible while Quantix waits for that exact connection.",
            ),
            ManagerIntakeStage::PackageRegistered => (
                ManagerIntakeStatusKind::Working,
                "Package registered",
                "The Tender Office is preparing the source collection.",
            ),
            ManagerIntakeStage::ReadingDocuments => (
                ManagerIntakeStatusKind::Working,
                "Reading Tender documents",
                "Quantix is deriving exact source evidence from the registered documents.",
            ),
            ManagerIntakeStage::ExtractingTenderFacts => (
                ManagerIntakeStatusKind::Working,
                "Deriving Tender facts",
                "The Tender Analyst is extracting requirements, risks, deadlines, and gaps.",
            ),
            ManagerIntakeStage::ReviewingTenderFacts => (
                ManagerIntakeStatusKind::Working,
                "Reviewing Tender facts",
                "The Independent Reviewer is checking extracted facts against their exact Evidence.",
            ),
            ManagerIntakeStage::PreparingFirstDecision => (
                ManagerIntakeStatusKind::Working,
                "Preparing the first decision",
                "The Tendering Manager is reviewing the exact intake record.",
            ),
            ManagerIntakeStage::WaitingForEngineer => (
                ManagerIntakeStatusKind::NeedsEngineer,
                "Waiting for your answer",
                "The Tendering Manager found information that is genuinely missing.",
            ),
            ManagerIntakeStage::BidDecisionReady => (
                ManagerIntakeStatusKind::Ready,
                "Bid Decision ready",
                "The Tendering Manager has prepared an evidence-linked recommendation.",
            ),
            ManagerIntakeStage::Failed => (
                ManagerIntakeStatusKind::Failed,
                "Intake needs attention",
                failure
                    .as_deref()
                    .unwrap_or("The Tender intake could not be completed safely."),
            ),
        };
        Ok(Some(ManagerIntakeStatus {
            intake_run_id,
            stage,
            status,
            label: label.into(),
            summary: summary.into(),
            parseable_document_count: checked_u32(parseable)?,
            parsed_document_count: checked_u32(parsed)?,
            extraction_run_count: checked_u32(extraction_runs)?,
            blocking_agent_run_id,
            retry_not_before_epoch_seconds,
        }))
    }

    pub(crate) fn manager_intake_provider_selection(
        &self,
    ) -> Result<Option<AiExecutionSelection>, TenderCommandError> {
        let row = self
            .connection
            .query_row(
                "SELECT provider_selection_json FROM manager_intake_runs
                 ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(sql_error)?
            .flatten();
        row.as_deref().map(parse_canonical).transpose()
    }

    pub(crate) fn manager_intake_cooldown_is_active(&mut self) -> Result<bool, TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let deadline = transaction
            .query_row(
                "SELECT retry_not_before_epoch_seconds
                 FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map_err(sql_error)?;
        let now = sqlite_epoch_seconds(&transaction)?;
        let active = deadline.is_some_and(|deadline| deadline > now);
        transaction.commit().map_err(sql_error)?;
        Ok(active)
    }

    pub(crate) fn bind_manager_intake_provider_selection(
        &mut self,
        selection: &AiExecutionSelection,
        allow_choice_change: bool,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (intake_run_id, stage, prior_json, retry_not_before): (
            String,
            String,
            Option<String>,
            Option<i64>,
        ) = transaction
            .query_row(
                "SELECT intake_run_id, stage, provider_selection_json,
                        retry_not_before_epoch_seconds
                 FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(sql_error)?;
        let stage = ManagerIntakeStage::parse(&stage)?;
        let now = sqlite_epoch_seconds(&transaction)?;
        if retry_not_before.is_some_and(|deadline| deadline > now) {
            transaction.commit().map_err(sql_error)?;
            return Ok(());
        }
        if !stage.is_resumable()
            || (allow_choice_change
                && !matches!(
                    stage,
                    ManagerIntakeStage::WaitingForProvider
                        | ManagerIntakeStage::WaitingForProviderApproval
                ))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let prior = prior_json.as_deref().map(parse_canonical).transpose()?;
        let choice_changed = prior
            .as_ref()
            .is_some_and(|prior| !same_provider_choice(prior, selection));
        if choice_changed && !allow_choice_change {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let binding_json = canonical_json(selection)?;
        let updated_at = sqlite_timestamp(&transaction)?;
        if transaction
            .execute(
                "UPDATE manager_intake_runs
                 SET provider_selection_json = ?2, updated_at = ?3
                 WHERE intake_run_id = ?1 AND stage IN (
                   'waiting_for_local_tools', 'waiting_for_provider_approval',
                   'waiting_for_provider', 'package_registered', 'reading_documents',
                   'extracting_tender_facts', 'reviewing_tender_facts',
                   'preparing_first_decision'
                 )",
                params![intake_run_id, binding_json, updated_at],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        if choice_changed {
            transaction
                .execute(
                    "UPDATE provider_threads SET status = 'archive_pending'
                     WHERE status = 'active'",
                    [],
                )
                .map_err(sql_error)?;
        }
        let (tender_id, tender_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            &tender_id,
            if choice_changed {
                "manager_intake_provider_rebound"
            } else if prior.is_some() {
                "manager_intake_provider_refreshed"
            } else {
                "manager_intake_provider_bound"
            },
            tender_revision,
            json!({
                "intake_run_id": intake_run_id,
                "connection_id": selection.connection_id,
                "provider": selection.provider,
                "model_id": selection.model_id,
                "reasoning": selection.reasoning,
                "catalogue_fetched_at": selection.catalogue_fetched_at,
                "adapter_version": selection.adapter_version,
                "engineer_confirmed_change": choice_changed && allow_choice_change,
            }),
            &updated_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn wait_manager_intake_for_provider(
        &mut self,
        source_run: Option<&AgentRunInspection>,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (intake_run_id, attempts, current_deadline): (String, u32, Option<i64>) = transaction
            .query_row(
                "SELECT intake_run_id, provider_retry_attempt_count,
                        retry_not_before_epoch_seconds
                 FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        let updated_at = sqlite_timestamp(&transaction)?;
        let now = sqlite_epoch_seconds(&transaction)?;
        let rate_limited = source_run.filter(|run| {
            run.state == crate::agent_runtime::AgentRunState::Failed
                && run.completed_at.is_some()
                && run
                    .failure
                    .as_ref()
                    .is_some_and(|failure| failure.category == ProviderFailureCategory::RateLimited)
        });
        if let Some(source_run) = rate_limited {
            let already_consumed: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM manager_intake_provider_rate_limit_consumptions
                       WHERE intake_run_id = ?1 AND source_run_id = ?2
                     )",
                    params![intake_run_id, source_run.run_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if already_consumed {
                transaction.commit().map_err(sql_error)?;
                return Ok(());
            }
            let next_attempt = attempts
                .checked_add(1)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let automatic_deadline = if next_attempt <= 3 {
                Some(manager_provider_retry_deadline(
                    source_run,
                    now,
                    next_attempt,
                )?)
            } else {
                None
            };
            let (stage, failure_summary, completed_at) = if automatic_deadline.is_some() {
                ("waiting_for_provider", None, None)
            } else {
                (
                    "failed",
                    Some("AI capacity remained unavailable after three automatic retries. Retry intake when you are ready."),
                    Some(updated_at.as_str()),
                )
            };
            if transaction
                .execute(
                    "INSERT INTO manager_intake_provider_rate_limit_consumptions (
                       intake_run_id, source_run_id, provider_retry_attempt_count,
                       retry_not_before_epoch_seconds, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        intake_run_id,
                        source_run.run_id,
                        next_attempt,
                        automatic_deadline,
                        updated_at,
                    ],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            if transaction
                .execute(
                    "UPDATE manager_intake_runs
                     SET stage = ?2, current_manager_run_id = NULL,
                         blocking_agent_run_id = ?3,
                         retry_not_before_epoch_seconds = ?4,
                         provider_retry_attempt_count = ?5,
                         failure_summary = ?6, completed_at = ?7, updated_at = ?8
                     WHERE intake_run_id = ?1 AND stage IN (
                       'waiting_for_local_tools', 'waiting_for_provider_approval',
                       'waiting_for_provider', 'package_registered', 'reading_documents',
                       'extracting_tender_facts', 'reviewing_tender_facts',
                       'preparing_first_decision'
                     )",
                    params![
                        intake_run_id,
                        stage,
                        source_run.run_id,
                        automatic_deadline,
                        next_attempt,
                        failure_summary,
                        completed_at,
                        updated_at,
                    ],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (tender_id, tender_revision): (String, u32) = transaction
                .query_row(
                    "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(sql_error)?;
            append_audit_event(
                &transaction,
                &tender_id,
                if automatic_deadline.is_some() {
                    "manager_intake_provider_cooldown_started"
                } else {
                    "manager_intake_provider_retry_exhausted"
                },
                tender_revision,
                json!({
                    "intake_run_id": intake_run_id,
                    "blocking_agent_run_id": source_run.run_id,
                    "provider_retry_attempt_count": next_attempt,
                    "retry_not_before_epoch_seconds": automatic_deadline,
                }),
                &updated_at,
            )?;
        } else if current_deadline.is_some_and(|deadline| deadline > now) {
            transaction.commit().map_err(sql_error)?;
            return Ok(());
        } else if transaction
            .execute(
                "UPDATE manager_intake_runs
                 SET stage = 'waiting_for_provider', current_manager_run_id = NULL,
                     blocking_agent_run_id = NULL,
                     retry_not_before_epoch_seconds = NULL,
                     failure_summary = NULL, completed_at = NULL, updated_at = ?2
                 WHERE intake_run_id = ?1 AND stage IN (
                   'waiting_for_local_tools', 'waiting_for_provider_approval',
                   'waiting_for_provider', 'package_registered', 'reading_documents',
                   'extracting_tender_facts', 'reviewing_tender_facts',
                   'preparing_first_decision'
                 )",
                params![intake_run_id, updated_at],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn wait_manager_intake_for_local_tools(&mut self) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let deadline: Option<i64> = transaction
            .query_row(
                "SELECT retry_not_before_epoch_seconds FROM manager_intake_runs
                 ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let now = sqlite_epoch_seconds(&transaction)?;
        if deadline.is_some_and(|deadline| deadline > now) {
            transaction.commit().map_err(sql_error)?;
            return Ok(());
        }
        let updated_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "UPDATE manager_intake_runs
                 SET stage = 'waiting_for_local_tools', current_manager_run_id = NULL,
                     blocking_agent_run_id = NULL, retry_not_before_epoch_seconds = NULL,
                     failure_summary = NULL, completed_at = NULL, updated_at = ?1
                 WHERE intake_run_id = (
                   SELECT intake_run_id FROM manager_intake_runs
                   ORDER BY intake_run_sequence DESC LIMIT 1
                 ) AND stage IN (
                   'waiting_for_local_tools', 'package_registered', 'reading_documents',
                   'waiting_for_provider_approval', 'waiting_for_provider',
                   'extracting_tender_facts', 'reviewing_tender_facts',
                   'preparing_first_decision'
                 )",
                [updated_at],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn wait_manager_intake_for_provider_approval(
        &mut self,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let deadline: Option<i64> = transaction
            .query_row(
                "SELECT retry_not_before_epoch_seconds FROM manager_intake_runs
                 ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let now = sqlite_epoch_seconds(&transaction)?;
        if deadline.is_some_and(|deadline| deadline > now) {
            transaction.commit().map_err(sql_error)?;
            return Ok(());
        }
        let updated_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "UPDATE manager_intake_runs
                 SET stage = 'waiting_for_provider_approval', current_manager_run_id = NULL,
                     blocking_agent_run_id = NULL, retry_not_before_epoch_seconds = NULL,
                     failure_summary = NULL, completed_at = NULL, updated_at = ?1
                 WHERE intake_run_id = (
                   SELECT intake_run_id FROM manager_intake_runs
                   ORDER BY intake_run_sequence DESC LIMIT 1
                 ) AND stage IN (
                   'waiting_for_provider_approval', 'waiting_for_provider',
                   'reading_documents', 'extracting_tender_facts',
                   'reviewing_tender_facts', 'preparing_first_decision'
                 )",
                [updated_at],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn unresolved_manager_intake_run_ids(
        &self,
    ) -> Result<Vec<String>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT DISTINCT ar.run_id
                 FROM manager_intake_runs mir
                 JOIN agent_runs ar ON (
                   ar.run_id = mir.current_manager_run_id
                   OR EXISTS (
                     SELECT 1
                     FROM tender_tasks task, json_each(task.exact_inputs_json)
                     WHERE task.task_id = ar.task_id
                       AND json_extract(value, '$.kind') = 'manager_intake_run'
                       AND json_extract(value, '$.reference') = mir.intake_run_id
                       AND json_extract(value, '$.version') = 1
                   )
                 )
                 WHERE mir.intake_run_sequence = (
                   SELECT MAX(intake_run_sequence) FROM manager_intake_runs
                 )
                   AND ar.status = 'indeterminate'
                   AND NOT EXISTS (
                     SELECT 1 FROM agent_run_recovery_dispositions recovery
                     WHERE recovery.run_id = ar.run_id
                   )
                 ORDER BY ar.run_id",
            )
            .map_err(sql_error)?;
        let run_ids = statement
            .query_map([], |row| row.get(0))
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        Ok(run_ids)
    }

    pub(crate) fn workspace_tender_documents(
        &self,
    ) -> Result<Vec<WorkspaceTenderDocument>, TenderCommandError> {
        let register = self.document_register()?;
        register
            .documents
            .into_iter()
            .map(|document| {
                Ok(WorkspaceTenderDocument {
                    artifact_id: document.artifact_id,
                    version: document.version,
                    package_path: document.package_path,
                    document_type: document.document_type,
                    media_type: document.media_type,
                    sha256: document.sha256,
                    size_bytes: document.size_bytes,
                    registration_state: document.registration_state,
                    parse_state: document.parse_state,
                    exception: document.exception,
                })
            })
            .collect()
    }

    pub(crate) fn begin_manager_intake_processing(
        &mut self,
    ) -> Result<ManagerIntakeStage, TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (intake_run_id, stage, retry_not_before): (String, String, Option<i64>) = transaction
            .query_row(
                "SELECT intake_run_id, stage, retry_not_before_epoch_seconds FROM manager_intake_runs
                 ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        let prior = ManagerIntakeStage::parse(&stage)?;
        let now = sqlite_epoch_seconds(&transaction)?;
        if retry_not_before.is_some_and(|deadline| deadline > now) {
            transaction.commit().map_err(sql_error)?;
            return Ok(ManagerIntakeStage::WaitingForProvider);
        }
        if matches!(
            prior,
            ManagerIntakeStage::WaitingForEngineer
                | ManagerIntakeStage::BidDecisionReady
                | ManagerIntakeStage::Failed
        ) {
            return Ok(prior);
        }
        let updated_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "UPDATE manager_intake_runs
                 SET stage = 'reading_documents', failure_summary = NULL,
                     blocking_agent_run_id = NULL,
                     retry_not_before_epoch_seconds = NULL,
                     completed_at = NULL, updated_at = ?2
                 WHERE intake_run_id = ?1",
                params![intake_run_id, updated_at],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        Ok(ManagerIntakeStage::ReadingDocuments)
    }

    pub(crate) fn queue_manager_intake_retry(&mut self) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        let updated_at = connection_timestamp(&self.connection)?;
        let updated = self
            .connection
            .execute(
                "UPDATE manager_intake_runs
                 SET stage = CASE WHEN provider_selection_json IS NULL
                                  THEN 'waiting_for_local_tools'
                                  ELSE 'waiting_for_provider' END,
                     blocking_agent_run_id = NULL,
                     retry_not_before_epoch_seconds = NULL,
                     provider_retry_attempt_count = CASE
                       WHEN provider_retry_attempt_count = 4 THEN 0
                       ELSE provider_retry_attempt_count
                     END,
                     failure_summary = NULL,
                     completed_at = NULL, updated_at = ?1
                 WHERE intake_run_id = (
                   SELECT intake_run_id FROM manager_intake_runs
                   ORDER BY intake_run_sequence DESC LIMIT 1
                 ) AND stage = 'failed'",
                [updated_at],
            )
            .map_err(sql_error)?;
        if updated != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(())
    }

    pub(crate) fn manager_intake_parse_targets(
        &mut self,
        tender_id: &TenderId,
    ) -> Result<Vec<ParseSourceArtifactCommand>, TenderCommandError> {
        let intake_id: String = self
            .connection
            .query_row(
                "SELECT package_intake_id FROM manager_intake_runs
                 ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let documents = super::DocumentRegister {
            query_register_open: false,
            documents: self.document_register_entries(Some(&intake_id))?,
        };
        let targets = documents
            .documents
            .iter()
            .filter(|document| {
                document.registration_state == super::RegistrationState::Registered
                    && document.supersession_state != super::SupersessionState::Superseded
                    && matches!(
                        document.document_type.as_str(),
                        "pdf_document" | "word_document" | "spreadsheet"
                    )
                    && !matches!(
                        document.parse_state,
                        ParseState::Parsed | ParseState::Running
                    )
            })
            .map(|document| ParseSourceArtifactCommand {
                tender_id: tender_id.as_str().into(),
                artifact_id: document.artifact_id.clone(),
                version: document.version,
            })
            .collect::<Vec<_>>();
        let parseable = documents
            .documents
            .iter()
            .filter(|document| {
                document.registration_state == super::RegistrationState::Registered
                    && document.supersession_state != super::SupersessionState::Superseded
                    && matches!(
                        document.document_type.as_str(),
                        "pdf_document" | "word_document" | "spreadsheet"
                    )
            })
            .count();
        let parsed = documents
            .documents
            .iter()
            .filter(|document| {
                document.registration_state == super::RegistrationState::Registered
                    && document.supersession_state != super::SupersessionState::Superseded
                    && matches!(
                        document.document_type.as_str(),
                        "pdf_document" | "word_document" | "spreadsheet"
                    )
                    && document.parse_state == ParseState::Parsed
            })
            .count();
        self.update_manager_intake_counts(parseable, parsed, None)?;
        Ok(targets)
    }

    pub(crate) fn source_artifact_package_path(
        &self,
        artifact_id: &str,
        version: u32,
    ) -> Result<String, TenderCommandError> {
        self.connection
            .query_row(
                "SELECT source_artifacts.package_path
                 FROM source_artifacts
                 JOIN source_artifact_versions
                   ON source_artifact_versions.artifact_id = source_artifacts.artifact_id
                 WHERE source_artifacts.artifact_id = ?1
                   AND source_artifact_versions.version = ?2",
                params![artifact_id, version],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn refresh_manager_intake_parse_counts(&mut self) -> Result<(), TenderCommandError> {
        let intake_id: String = self
            .connection
            .query_row(
                "SELECT package_intake_id FROM manager_intake_runs
                 ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let (parseable, parsed): (i64, i64) = self
            .connection
            .query_row(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN COALESCE((
                          SELECT pa.status
                          FROM parse_attempts pa
                          WHERE pa.artifact_id = sav.artifact_id
                            AND pa.version = sav.version
                          ORDER BY pa.attempt_sequence DESC
                          LIMIT 1
                        ), 'not_requested') = 'parsed' THEN 1 ELSE 0 END), 0)
                 FROM source_artifacts sa
                 JOIN source_artifact_versions sav
                   ON sav.artifact_id = sa.artifact_id
                 WHERE sa.intake_id = ?1
                   AND sav.registration_state = 'registered'
                   AND sav.document_type IN (
                     'pdf_document', 'word_document', 'spreadsheet'
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM source_relationships sr
                     WHERE sr.prior_artifact_id = sav.artifact_id
                       AND sr.prior_version = sav.version
                       AND sr.relationship_kind = 'replacement'
                   )",
                [intake_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        self.update_manager_intake_counts(
            usize::try_from(parseable)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            usize::try_from(parsed)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            None,
        )
    }

    fn update_manager_intake_counts(
        &mut self,
        parseable: usize,
        parsed: usize,
        extraction_runs: Option<usize>,
    ) -> Result<(), TenderCommandError> {
        let updated_at = connection_timestamp(&self.connection)?;
        let parseable = i64::try_from(parseable)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let parsed = i64::try_from(parsed)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let extraction_runs = extraction_runs
            .map(i64::try_from)
            .transpose()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        self.connection
            .execute(
                "UPDATE manager_intake_runs
                 SET parseable_document_count = ?1, parsed_document_count = ?2,
                     extraction_run_count = COALESCE(?3, extraction_run_count), updated_at = ?4
                 WHERE intake_run_id = (
                   SELECT intake_run_id FROM manager_intake_runs
                   ORDER BY intake_run_sequence DESC LIMIT 1
                 )",
                params![parseable, parsed, extraction_runs, updated_at],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    pub(crate) fn manager_intake_evidence_batches(
        &mut self,
    ) -> Result<Vec<PlannedManagerIntakeExtractionBatch>, TenderCommandError> {
        self.persist_manager_intake_byte_plan(record_extraction_request_hard_cap_bytes())
    }

    fn persist_manager_intake_byte_plan(
        &mut self,
        byte_budget: u64,
    ) -> Result<Vec<PlannedManagerIntakeExtractionBatch>, TenderCommandError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (intake_run_id, tender_id, tender_revision, selection) =
            current_manager_intake_plan_context(&transaction)?;
        let mut planned = load_manager_intake_plan(&transaction, &intake_run_id)?;
        if planned.is_empty() {
            let stage: String = transaction
                .query_row(
                    "SELECT stage FROM manager_intake_runs WHERE intake_run_id = ?1",
                    [&intake_run_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if stage != ManagerIntakeStage::ReadingDocuments.as_str() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (evidence, authorities) = current_manager_intake_plan_inputs(&transaction)?;
            planned = build_manager_intake_plan(ManagerIntakePlanRequest {
                transaction: &transaction,
                tender_id: &tender_id,
                tender_revision,
                intake_run_id: &intake_run_id,
                evidence: &evidence,
                authorities: &authorities,
                selection: &selection,
                byte_budget,
            })?;
            let created_at = sqlite_timestamp(&transaction)?;
            for (index, batch) in planned.iter().enumerate() {
                let ordinal = i64::try_from(index + 1)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                let inputs = canonical_json(&ManagerIntakeBatchInputs {
                    authorities: batch.authorities.clone(),
                    evidence: batch.evidence.clone(),
                    request_context: batch.request_context.clone(),
                })?;
                let estimated_request_bytes = i64::try_from(batch.estimated_request_bytes)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                transaction
                    .execute(
                        "INSERT INTO manager_intake_extraction_plan_batches (
                           intake_run_id, ordinal, batch_fingerprint, canonical_inputs_json,
                           request_context_sha256, estimated_request_bytes, estimator_version,
                           created_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![
                            intake_run_id,
                            ordinal,
                            batch.fingerprint,
                            inputs,
                            batch.request_context.request_context_sha256,
                            estimated_request_bytes,
                            RECORD_EXTRACTION_ESTIMATOR_VERSION,
                            created_at,
                        ],
                    )
                    .map_err(sql_error)?;
            }
        }
        let completed = completed_manager_intake_batch_fingerprints(&transaction, &intake_run_id)?;
        planned.retain(|batch| !completed.contains(&batch.fingerprint));
        let updated_at = sqlite_timestamp(&transaction)?;
        if transaction
            .execute(
                "UPDATE manager_intake_runs
                 SET stage = 'extracting_tender_facts', failure_summary = NULL,
                     completed_at = NULL, updated_at = ?2
                 WHERE intake_run_id = ?1
                   AND stage IN ('reading_documents', 'extracting_tender_facts')",
                params![intake_run_id, updated_at],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        transaction.commit().map_err(sql_error)?;
        Ok(planned)
    }

    #[cfg(any(test, feature = "runtime-fixture"))]
    pub(crate) fn preview_manager_intake_byte_plan(
        &mut self,
        byte_budget: u64,
    ) -> Result<Vec<PlannedManagerIntakeExtractionBatch>, TenderCommandError> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (intake_run_id, tender_id, tender_revision, selection) =
            current_manager_intake_plan_context(&transaction)?;
        let (evidence, authorities) = current_manager_intake_plan_inputs(&transaction)?;
        build_manager_intake_plan(ManagerIntakePlanRequest {
            transaction: &transaction,
            tender_id: &tender_id,
            tender_revision,
            intake_run_id: &intake_run_id,
            evidence: &evidence,
            authorities: &authorities,
            selection: &selection,
            byte_budget,
        })
    }

    #[cfg(any(test, feature = "runtime-fixture"))]
    pub(crate) fn persist_manager_intake_byte_plan_for_verification(
        &mut self,
        byte_budget: u64,
    ) -> Result<Vec<PlannedManagerIntakeExtractionBatch>, TenderCommandError> {
        self.persist_manager_intake_byte_plan(byte_budget)
    }

    pub(crate) fn manager_intake_review_targets(
        &self,
    ) -> Result<Vec<TenderRecordVersionReference>, TenderCommandError> {
        Ok(current_record_inspections(self)?
            .into_iter()
            .filter(|record| record.verification_status == VerificationStatus::Proposed)
            .map(|record| TenderRecordVersionReference {
                record_id: record.record_id,
                version: record.version,
            })
            .collect())
    }

    pub(crate) fn begin_manager_intake_reviewing(&mut self) -> Result<(), TenderCommandError> {
        self.update_manager_intake_stage(ManagerIntakeStage::ReviewingTenderFacts, None)
    }

    pub(crate) fn record_manager_intake_extraction_count(
        &mut self,
    ) -> Result<(), TenderCommandError> {
        let status = self
            .current_manager_intake_status()?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let count: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM manager_intake_extraction_batches
                 WHERE intake_run_id = ?1",
                [&status.intake_run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let count = usize::try_from(count)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        self.update_manager_intake_counts(
            status.parseable_document_count as usize,
            status.parsed_document_count as usize,
            Some(count),
        )
    }

    pub(crate) fn fail_manager_intake(
        &mut self,
        tender_id: &TenderId,
        summary: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let summary = summary.trim();
        if summary.is_empty() || summary.len() > 2_000 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let updated_at = sqlite_timestamp(&transaction)?;
        let (intake_run_id, stage): (String, String) = transaction
            .query_row(
                "SELECT intake_run_id, stage FROM manager_intake_runs
                 ORDER BY intake_run_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if !ManagerIntakeStage::parse(&stage)?.is_active() {
            transaction.commit().map_err(sql_error)?;
            return Ok(());
        }
        let updated = transaction
            .execute(
                "UPDATE manager_intake_runs
                 SET stage = 'failed', failure_summary = ?2,
                     updated_at = ?3, completed_at = ?3
                 WHERE intake_run_id = ?1 AND stage IN (
                   'waiting_for_local_tools', 'waiting_for_provider_approval',
                   'package_registered', 'reading_documents', 'extracting_tender_facts',
                   'reviewing_tender_facts',
                   'preparing_first_decision'
                 )",
                params![intake_run_id, summary, updated_at],
            )
            .map_err(sql_error)?;
        if updated != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let message_id = super::workspace::append_system_message(
            &transaction,
            super::TenderOfficeMessageKind::Blocker,
            summary,
            &updated_at,
        )?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "manager_intake_failed",
            tender_revision,
            json!({
                "intake_run_id": intake_run_id,
                "message_id": message_id,
                "summary_sha256": sha256_hex(summary.as_bytes()),
            }),
            &updated_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn prepare_manager_intake_run(
        &mut self,
        tender_id: &TenderId,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        self.require_pre_bid_writable()?;
        let run_id = random_identifier(&self.connection)?;
        let application_home = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .to_path_buf();
        let provider_selection = self
            .manager_intake_provider_selection()?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        let workspace = application_home
            .join("staging")
            .join(format!("agent-{}-{run_id}", tender_id.as_str()));
        let records = current_record_inspections(self)?;
        if records.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let tender_revision: u32 = transaction
                .query_row(
                    "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                    [tender_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let has_unresolved_indeterminate: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1
                       FROM manager_intake_runs mir
                       JOIN agent_runs ar ON ar.run_id = mir.current_manager_run_id
                       WHERE mir.intake_run_sequence = (
                         SELECT MAX(intake_run_sequence) FROM manager_intake_runs
                       )
                         AND ar.status = 'indeterminate'
                         AND NOT EXISTS (
                           SELECT 1 FROM agent_run_recovery_dispositions
                           WHERE agent_run_recovery_dispositions.run_id = ar.run_id
                         )
                     )",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if has_unresolved_indeterminate {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let created_at = sqlite_timestamp(&transaction)?;
            let profile_id: String = transaction
                .query_row(
                    "SELECT profile_id FROM agent_profiles WHERE stable_identity = ?1",
                    [MANAGER_STABLE_IDENTITY],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let profile = manager_intake_profile(profile_id.clone());
            let stored_profile: Option<AgentProfileVersionView> = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_profile_versions
                       WHERE profile_id = ?1 AND version = ?2
                     )",
                    params![profile_id, profile.version],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
                .then(|| load_profile(&transaction, (profile_id.clone(), profile.version)))
                .transpose()?;
            if let Some(stored_profile) = stored_profile {
                if stored_profile != profile {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
            } else {
                insert_profile_version(&transaction, &profile, &created_at)?;
            }
            update_profile_head(
                &transaction,
                &profile.profile_id,
                profile.version,
                AgentProfileStatus::Active,
            )?;
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = manager_intake_task(
                random_identifier(&transaction)?,
                tender_id,
                tender_revision,
                &records,
                deadline,
                &profile,
            );
            insert_task(&transaction, &task, &created_at)?;
            let payload = manager_intake_data_view(tender_id, tender_revision, &records)?;
            let (permission_grant, materialized_workspace) =
                derive_pre_bid_data_grant(PreBidDataGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home: &application_home,
                    tender_id: tender_id.as_str(),
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                    data_scope: MANAGER_INTAKE_SCOPE,
                    allowed_action: MANAGER_INTAKE_ACTION,
                    relative_path: "manager-intake-v1.json",
                    view_id: "manager-intake-v1",
                    payload: &payload,
                    additional_data_views: &[],
                })?;
            if materialized_workspace != workspace
                || permission_duration(&permission_grant, Timestamp::now())
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
                    .is_zero()
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
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
                    summary: "Tendering Manager intake review started".into(),
                    correlation_id: None,
                    request_fingerprint: None,
                    denial_reason: None,
                    opaque_reference: None,
                },
                &created_at,
            )?;
            transaction
                .execute(
                    "UPDATE manager_intake_runs
                     SET stage = 'preparing_first_decision', current_manager_run_id = ?1,
                         failure_summary = NULL, completed_at = NULL, updated_at = ?2
                     WHERE intake_run_id = (
                       SELECT intake_run_id FROM manager_intake_runs
                       ORDER BY intake_run_sequence DESC LIMIT 1
                     )",
                    params![run_id, created_at],
                )
                .map_err(sql_error)?;
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "agent_run_started",
                tender_revision,
                json!({
                    "profile_id": profile.profile_id,
                    "profile_version": profile.version.to_string(),
                    "retry_of_run_id": Value::Null,
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

    pub(crate) fn validate_manager_intake_candidate(
        &self,
        task: &TenderTaskView,
        payload_json: &str,
    ) -> Result<ManagerIntakeCandidate, TenderCommandError> {
        let candidate: ManagerIntakeCandidate = serde_json::from_str(payload_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if canonical_json(&candidate)? != payload_json
            || candidate.supporting_records.is_empty()
            || candidate.supporting_records.len() > MAX_SUPPORTING_RECORDS
            || candidate.supporting_evidence.is_empty()
            || candidate.supporting_evidence.len() > MAX_SUPPORTING_EVIDENCE
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match candidate.kind {
            ManagerIntakeOutcomeKind::Question
                if candidate
                    .question
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty() && value.len() <= 4_000)
                    && candidate.recommendation.is_none()
                    && candidate.rationale.is_none() => {}
            ManagerIntakeOutcomeKind::BidDecision
                if candidate.question.is_none()
                    && candidate.recommendation.is_some()
                    && candidate
                        .rationale
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty() && value.len() <= 3_500) => {}
            _ => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        }
        let allowed_records = task
            .exact_inputs
            .iter()
            .filter(|input| input.kind == "tender_record_version")
            .map(|input| (input.reference.clone(), input.version))
            .collect::<HashSet<_>>();
        let record_set = candidate
            .supporting_records
            .iter()
            .map(|reference| (reference.record_id.clone(), reference.version))
            .collect::<HashSet<_>>();
        if record_set.len() != candidate.supporting_records.len()
            || record_set
                .iter()
                .any(|reference| !allowed_records.contains(reference))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let evidence_set = candidate
            .supporting_evidence
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        if evidence_set.len() != candidate.supporting_evidence.len() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut supported_evidence = HashSet::new();
        let mut has_real_gap = false;
        for reference in &candidate.supporting_records {
            let record =
                self.inspect_tender_record_version(&reference.record_id, reference.version)?;
            has_real_gap |=
                record_has_unresolved_gap(&record) || !record_is_bid_admissible(&record);
            collect_record_evidence(&record, &mut supported_evidence);
        }
        if evidence_set
            .iter()
            .any(|reference| !supported_evidence.contains(reference))
            || (candidate.kind == ManagerIntakeOutcomeKind::Question && !has_real_gap)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if candidate.kind == ManagerIntakeOutcomeKind::BidDecision
            && current_record_inspections(self)?.iter().any(|record| {
                record_has_unresolved_gap(record) || !record_is_bid_admissible(record)
            })
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(candidate)
    }

    pub(crate) fn manager_intake_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT intake_run_id, stage, provider_selection_json,
                        blocking_agent_run_id, retry_not_before_epoch_seconds,
                        provider_retry_attempt_count
                 FROM manager_intake_runs ORDER BY intake_run_sequence",
            )
            .map_err(sql_error)?;
        let mut rows = statement.query([]).map_err(sql_error)?;
        while let Some(row) = rows.next().map_err(sql_error)? {
            check()?;
            let intake_run_id = row.get::<_, String>(0).map_err(sql_error)?;
            let stage = ManagerIntakeStage::parse(&row.get::<_, String>(1).map_err(sql_error)?)?;
            let binding = row.get::<_, Option<String>>(2).map_err(sql_error)?;
            let blocking_run_id = row.get::<_, Option<String>>(3).map_err(sql_error)?;
            let retry_deadline = row.get::<_, Option<i64>>(4).map_err(sql_error)?;
            let projected_attempt = row.get::<_, u32>(5).map_err(sql_error)?;
            if !matches!(
                stage,
                ManagerIntakeStage::WaitingForLocalTools
                    | ManagerIntakeStage::WaitingForProviderApproval
                    | ManagerIntakeStage::PackageRegistered
                    | ManagerIntakeStage::ReadingDocuments
            ) && binding.is_none()
            {
                return Ok(false);
            }
            if let Some(binding) = binding {
                let selection: AiExecutionSelection = parse_canonical(&binding)?;
                if selection.connection_id.trim().is_empty()
                    || selection.model_id.trim().is_empty()
                    || selection.catalogue_fetched_at.trim().is_empty()
                    || selection.adapter_version.trim().is_empty()
                {
                    return Ok(false);
                }
            }
            let consumptions = self
                .connection
                .prepare(
                    "SELECT consumption.source_run_id,
                            consumption.provider_retry_attempt_count,
                            consumption.retry_not_before_epoch_seconds,
                            runs.failure_json
                     FROM manager_intake_provider_rate_limit_consumptions AS consumption
                     JOIN agent_runs AS runs ON runs.run_id = consumption.source_run_id
                     WHERE consumption.intake_run_id = ?1
                     ORDER BY consumption.rowid",
                )
                .and_then(|mut statement| {
                    statement
                        .query_map([&intake_run_id], |consumption| {
                            Ok((
                                consumption.get::<_, String>(0)?,
                                consumption.get::<_, u32>(1)?,
                                consumption.get::<_, Option<i64>>(2)?,
                                consumption.get::<_, Option<String>>(3)?,
                            ))
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .map_err(sql_error)?;
            let mut expected_attempt = 1_u32;
            for (_, attempt, deadline, failure_json) in &consumptions {
                let failure = failure_json
                    .as_deref()
                    .map(parse_canonical::<crate::agent_runtime::ProviderFailure>)
                    .transpose()?;
                if *attempt != expected_attempt
                    || failure.as_ref().is_none_or(|failure| {
                        failure.category != ProviderFailureCategory::RateLimited
                    })
                    || ((*attempt <= 3) != deadline.is_some())
                {
                    return Ok(false);
                }
                expected_attempt = if *attempt == 4 { 1 } else { attempt + 1 };
            }
            let latest_consumption = consumptions.last();
            if projected_attempt == 0 {
                if latest_consumption.is_some_and(|(_, attempt, _, _)| *attempt != 4) {
                    return Ok(false);
                }
            } else if latest_consumption
                .is_none_or(|(_, attempt, _, _)| *attempt != projected_attempt)
            {
                return Ok(false);
            }
            if let Some(deadline) = retry_deadline {
                if stage != ManagerIntakeStage::WaitingForProvider
                    || !(1..=3).contains(&projected_attempt)
                    || latest_consumption.is_none_or(
                        |(source_run_id, attempt, consumed_deadline, _)| {
                            blocking_run_id.as_deref() != Some(source_run_id.as_str())
                                || *attempt != projected_attempt
                                || *consumed_deadline != Some(deadline)
                        },
                    )
                {
                    return Ok(false);
                }
            } else if blocking_run_id.is_some()
                || (projected_attempt == 4
                    && (stage != ManagerIntakeStage::Failed
                        || latest_consumption.is_none_or(
                            |(source_run_id, attempt, deadline, _)| {
                                blocking_run_id.as_deref() != Some(source_run_id.as_str())
                                    || *attempt != 4
                                    || deadline.is_some()
                            },
                        )))
            {
                return Ok(false);
            }

            let plan_count: u32 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM manager_intake_extraction_plan_batches
                     WHERE intake_run_id = ?1",
                    [&intake_run_id],
                    |plan| plan.get(0),
                )
                .map_err(sql_error)?;
            if matches!(
                stage,
                ManagerIntakeStage::ExtractingTenderFacts
                    | ManagerIntakeStage::ReviewingTenderFacts
                    | ManagerIntakeStage::PreparingFirstDecision
                    | ManagerIntakeStage::WaitingForEngineer
                    | ManagerIntakeStage::BidDecisionReady
            ) && plan_count == 0
            {
                return Ok(false);
            }
        }
        drop(rows);
        drop(statement);
        let mut statement = self
            .connection
            .prepare(
                "SELECT outcome_id, intake_run_id, manager_run_id, message_id, kind, body,
                        question, recommendation, supporting_records_json,
                        supporting_evidence_json, manifest_json, manifest_sha256, created_at
                 FROM manager_intake_outcomes ORDER BY outcome_sequence",
            )
            .map_err(sql_error)?;
        let mut rows = statement.query([]).map_err(sql_error)?;
        while let Some(row) = rows.next().map_err(sql_error)? {
            check()?;
            let kind = match row.get::<_, String>(4).map_err(sql_error)?.as_str() {
                "question" => ManagerIntakeOutcomeKind::Question,
                "bid_decision" => ManagerIntakeOutcomeKind::BidDecision,
                _ => return Ok(false),
            };
            let recommendation = match row
                .get::<_, Option<String>>(7)
                .map_err(sql_error)?
                .as_deref()
            {
                Some("proceed") => Some(ManagerBidRecommendation::Proceed),
                Some("hold") => Some(ManagerBidRecommendation::Hold),
                Some("decline") => Some(ManagerBidRecommendation::Decline),
                None => None,
                _ => return Ok(false),
            };
            let outcome_id = row.get::<_, String>(0).map_err(sql_error)?;
            let intake_run_id = row.get::<_, String>(1).map_err(sql_error)?;
            let manager_run_id = row.get::<_, String>(2).map_err(sql_error)?;
            let message_id = row.get::<_, String>(3).map_err(sql_error)?;
            let body = row.get::<_, String>(5).map_err(sql_error)?;
            let question = row.get::<_, Option<String>>(6).map_err(sql_error)?;
            let supporting_records_json = row.get::<_, String>(8).map_err(sql_error)?;
            let supporting_evidence_json = row.get::<_, String>(9).map_err(sql_error)?;
            let supporting_records: Vec<TenderRecordVersionReference> =
                parse_canonical(&supporting_records_json)?;
            let supporting_evidence: Vec<TenderEvidenceReference> =
                parse_canonical(&supporting_evidence_json)?;
            let created_at = row.get::<_, String>(12).map_err(sql_error)?;
            let expected = canonical_json(&ManagerIntakeOutcomeManifest {
                schema_version: 1,
                outcome_id: &outcome_id,
                intake_run_id: &intake_run_id,
                manager_run_id: &manager_run_id,
                message_id: &message_id,
                kind,
                body: &body,
                question: &question,
                recommendation: &recommendation,
                supporting_records: &supporting_records,
                supporting_evidence: &supporting_evidence,
                created_at: &created_at,
            })?;
            if expected != row.get::<_, String>(10).map_err(sql_error)?
                || sha256_hex(expected.as_bytes()) != row.get::<_, String>(11).map_err(sql_error)?
            {
                return Ok(false);
            }
        }
        drop(rows);
        drop(statement);
        let mut plan_statement = self
            .connection
            .prepare(
                "SELECT intake_run_id, ordinal, batch_fingerprint, canonical_inputs_json,
                        request_context_sha256, estimated_request_bytes, estimator_version
                 FROM manager_intake_extraction_plan_batches
                 ORDER BY intake_run_id, ordinal",
            )
            .map_err(sql_error)?;
        let mut plan_rows = plan_statement.query([]).map_err(sql_error)?;
        let mut expected_ordinal = 0_i64;
        let mut prior_intake = String::new();
        while let Some(row) = plan_rows.next().map_err(sql_error)? {
            check()?;
            let intake_run_id = row.get::<_, String>(0).map_err(sql_error)?;
            let ordinal = row.get::<_, i64>(1).map_err(sql_error)?;
            if intake_run_id != prior_intake {
                prior_intake = intake_run_id;
                expected_ordinal = 1;
            } else {
                expected_ordinal += 1;
            }
            let fingerprint = row.get::<_, String>(2).map_err(sql_error)?;
            let inputs_json = row.get::<_, String>(3).map_err(sql_error)?;
            let context_sha256 = row.get::<_, String>(4).map_err(sql_error)?;
            let estimated_bytes = row.get::<_, i64>(5).map_err(sql_error)?;
            let estimator_version = row.get::<_, String>(6).map_err(sql_error)?;
            let inputs: ManagerIntakeBatchInputs = parse_canonical(&inputs_json)?;
            if ordinal != expected_ordinal
                || estimated_bytes <= 0
                || estimator_version != RECORD_EXTRACTION_ESTIMATOR_VERSION
                || canonical_json(&inputs)? != inputs_json
                || inputs.request_context.request_context_sha256 != context_sha256
                || !record_extraction_request_plan_context_is_valid(&inputs.request_context)?
                || u64::try_from(inputs.request_context.request_body_json.len()).ok()
                    != Some(inputs.request_context.request_body_bytes)
                || u64::try_from(estimated_bytes).ok()
                    != Some(estimated_record_extraction_request_bytes(
                        inputs.request_context.request_body_bytes,
                    )?)
                || u64::try_from(estimated_bytes)
                    .ok()
                    .is_none_or(|bytes| bytes > record_extraction_request_hard_cap_bytes())
                || manager_intake_batch_fingerprint(&inputs)? != fingerprint
            {
                return Ok(false);
            }
        }
        drop(plan_rows);
        drop(plan_statement);
        let intake_ids = self
            .connection
            .prepare(
                "SELECT DISTINCT intake_run_id
                 FROM manager_intake_extraction_plan_batches ORDER BY intake_run_id",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(sql_error)?;
        for intake_run_id in intake_ids {
            check()?;
            let planned_inputs = self
                .connection
                .prepare(
                    "SELECT canonical_inputs_json
                     FROM manager_intake_extraction_plan_batches
                     WHERE intake_run_id = ?1 ORDER BY ordinal",
                )
                .and_then(|mut statement| {
                    statement
                        .query_map([&intake_run_id], |row| row.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .map_err(sql_error)?
                .into_iter()
                .map(|inputs| parse_canonical::<ManagerIntakeBatchInputs>(&inputs))
                .collect::<Result<Vec<_>, _>>()?;
            let flattened = planned_inputs
                .iter()
                .flat_map(|inputs| inputs.evidence.iter().cloned())
                .collect::<Vec<_>>();
            let expected_evidence = self
                .connection
                .prepare(
                    "SELECT el.artifact_id, el.version, el.ordinal
                     FROM evidence_locations AS el
                     JOIN source_artifacts AS artifacts
                       ON artifacts.artifact_id = el.artifact_id
                     JOIN manager_intake_runs AS intake
                       ON intake.package_intake_id = artifacts.intake_id
                     WHERE intake.intake_run_id = ?1
                       AND NOT EXISTS (
                         SELECT 1 FROM source_relationships AS relationships
                         WHERE relationships.prior_artifact_id = el.artifact_id
                           AND relationships.prior_version = el.version
                           AND relationships.relationship_kind = 'replacement'
                       )
                     ORDER BY el.artifact_id, el.version, el.ordinal",
                )
                .and_then(|mut statement| {
                    statement
                        .query_map([&intake_run_id], |row| {
                            Ok(TenderEvidenceReference {
                                artifact_id: row.get(0)?,
                                version: row.get(1)?,
                                ordinal: row.get(2)?,
                            })
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .map_err(sql_error)?;
            if flattened != expected_evidence
                || flattened.iter().collect::<HashSet<_>>().len() != flattened.len()
                || planned_inputs.iter().any(|inputs| {
                    inputs.evidence.iter().collect::<HashSet<_>>().len() != inputs.evidence.len()
                        || inputs
                            .authorities
                            .iter()
                            .map(|authority| authority.authority_id.as_str())
                            .collect::<HashSet<_>>()
                            .len()
                            != inputs.authorities.len()
                })
            {
                return Ok(false);
            }
        }
        let mut binding_statement = self
            .connection
            .prepare(
                "SELECT extraction_run_id, source_run_id, lineage_kind, intake_run_id,
                        batch_fingerprint, task_id, provider_selection_json,
                        plan_request_context_sha256, run_request_context_json,
                        run_request_context_sha256, estimated_request_bytes
                 FROM manager_intake_extraction_plan_run_bindings
                 ORDER BY extraction_run_id",
            )
            .map_err(sql_error)?;
        let mut binding_rows = binding_statement.query([]).map_err(sql_error)?;
        while let Some(row) = binding_rows.next().map_err(sql_error)? {
            check()?;
            let run_id = row.get::<_, String>(0).map_err(sql_error)?;
            let source_run_id = row.get::<_, Option<String>>(1).map_err(sql_error)?;
            let lineage_kind = row.get::<_, String>(2).map_err(sql_error)?;
            let intake_run_id = row.get::<_, String>(3).map_err(sql_error)?;
            let fingerprint = row.get::<_, String>(4).map_err(sql_error)?;
            let task_id = row.get::<_, String>(5).map_err(sql_error)?;
            let provider_selection_json = row.get::<_, String>(6).map_err(sql_error)?;
            let plan_context_sha = row.get::<_, String>(7).map_err(sql_error)?;
            let run_context_json = row.get::<_, String>(8).map_err(sql_error)?;
            let run_context_sha = row.get::<_, String>(9).map_err(sql_error)?;
            let estimated_request_bytes = row.get::<_, i64>(10).map_err(sql_error)?;
            let provider_selection: AiExecutionSelection =
                parse_canonical(&provider_selection_json)?;
            let run_context: RecordExtractionRequestPlanContext =
                parse_canonical(&run_context_json)?;
            if canonical_json(&provider_selection)? != provider_selection_json
                || canonical_json(&run_context)? != run_context_json
                || !record_extraction_request_plan_context_is_valid(&run_context)?
                || run_context.request_context_sha256 != run_context_sha
                || u64::try_from(estimated_request_bytes).ok()
                    != Some(estimated_record_extraction_request_bytes(
                        run_context.request_body_bytes,
                    )?)
                || u64::try_from(estimated_request_bytes)
                    .ok()
                    .is_none_or(|bytes| bytes > record_extraction_request_hard_cap_bytes())
            {
                return Ok(false);
            }
            let planned_inputs_json: Option<String> = self
                .connection
                .query_row(
                    "SELECT canonical_inputs_json
                     FROM manager_intake_extraction_plan_batches
                     WHERE intake_run_id = ?1 AND batch_fingerprint = ?2
                       AND request_context_sha256 = ?3 AND estimator_version = ?4",
                    params![
                        intake_run_id,
                        fingerprint,
                        plan_context_sha,
                        RECORD_EXTRACTION_ESTIMATOR_VERSION,
                    ],
                    |plan| plan.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            let Some(planned_inputs_json) = planned_inputs_json else {
                return Ok(false);
            };
            let planned_inputs: ManagerIntakeBatchInputs = parse_canonical(&planned_inputs_json)?;
            if planned_inputs.request_context.provider_selection_json != provider_selection_json
                || planned_inputs.request_context.request_context_sha256 != plan_context_sha
                || manager_intake_batch_fingerprint(&planned_inputs)? != fingerprint
            {
                return Ok(false);
            }
            let persisted_run: Option<(String, Option<String>, String, u32, String, String)> = self
                .connection
                .query_row(
                    "SELECT agent_runs.task_id, agent_runs.retry_of_run_id,
                            agent_runs.profile_id, agent_runs.profile_version,
                            agent_runs.permission_grant_json,
                            agent_run_provider_bindings.binding_json
                     FROM agent_runs JOIN agent_run_provider_bindings USING (run_id)
                     WHERE agent_runs.run_id = ?1",
                    [&run_id],
                    |run| {
                        Ok((
                            run.get(0)?,
                            run.get(1)?,
                            run.get(2)?,
                            run.get(3)?,
                            run.get(4)?,
                            run.get(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(sql_error)?;
            let Some((
                persisted_task_id,
                persisted_source_run_id,
                profile_id,
                profile_version,
                grant_json,
                binding_json,
            )) = persisted_run
            else {
                return Ok(false);
            };
            if persisted_task_id != task_id
                || persisted_source_run_id != source_run_id
                || binding_json != provider_selection_json
            {
                return Ok(false);
            }
            let task = load_task(&self.connection, &task_id)?;
            let profile = load_profile(&self.connection, (profile_id, profile_version))?;
            let permission_grant: PermissionGrant = parse_canonical(&grant_json)?;
            let prepared = PreparedAgentRun {
                run_id: run_id.clone(),
                provider_selection: provider_selection.clone(),
                profile,
                task: task.clone(),
                permission_grant,
                provider_thread_ref: None,
                provider_thread_to_archive: None,
                workspace: std::path::PathBuf::new(),
            };
            if record_extraction_request_context_for_prepared_with_stored_data_views(
                &prepared,
                &run_context,
            )? != run_context
            {
                return Ok(false);
            }
            let Some((task_intake, task_evidence, task_authorities)) =
                manager_intake_task_batch_inputs(&task)?
            else {
                return Ok(false);
            };
            if task_intake != intake_run_id
                || task_evidence != planned_inputs.evidence
                || task_authorities != planned_inputs.authorities
            {
                return Ok(false);
            }
            let lineage_is_valid = match (lineage_kind.as_str(), source_run_id.as_deref()) {
                ("initial", None) => {
                    task.repair_feedback.is_none() && run_context == planned_inputs.request_context
                }
                ("semantic_repair", Some(source_run_id)) => {
                    let source_task_id: Option<String> = self
                        .connection
                        .query_row(
                            "SELECT task_id
                             FROM manager_intake_extraction_plan_run_bindings
                             WHERE extraction_run_id = ?1 AND intake_run_id = ?2
                               AND batch_fingerprint = ?3
                               AND lineage_kind IN ('initial', 'transport_retry')
                               AND plan_request_context_sha256 = ?4",
                            params![source_run_id, intake_run_id, fingerprint, plan_context_sha,],
                            |source| source.get(0),
                        )
                        .optional()
                        .map_err(sql_error)?;
                    let Some(source_task_id) = source_task_id else {
                        return Ok(false);
                    };
                    semantic_repair_lineage_is_valid(
                        &self.connection,
                        &prepared,
                        source_run_id,
                        &load_task(&self.connection, &source_task_id)?,
                    )? && run_context != planned_inputs.request_context
                }
                ("transport_retry", Some(source_run_id)) => {
                    let source: Option<(String, String, String)> = self
                        .connection
                        .query_row(
                            "SELECT task_id, provider_selection_json, run_request_context_json
                             FROM manager_intake_extraction_plan_run_bindings
                             WHERE extraction_run_id = ?1 AND intake_run_id = ?2
                               AND batch_fingerprint = ?3
                               AND lineage_kind IN ('initial', 'semantic_repair', 'transport_retry')
                               AND plan_request_context_sha256 = ?4",
                            params![source_run_id, intake_run_id, fingerprint, plan_context_sha,],
                            |source| Ok((source.get(0)?, source.get(1)?, source.get(2)?)),
                        )
                        .optional()
                        .map_err(sql_error)?;
                    source.as_ref().is_some_and(
                        |(source_task_id, source_selection, source_context_json)| {
                            source_selection == &provider_selection_json
                                && source_context_json == &run_context_json
                                && load_task(&self.connection, source_task_id).ok().as_ref()
                                    == Some(&task)
                        },
                    )
                }
                _ => false,
            };
            if !lineage_is_valid {
                return Ok(false);
            }
        }
        drop(binding_rows);
        drop(binding_statement);
        let mut statement = self
            .connection
            .prepare(
                "SELECT intake_run_id, batch_fingerprint, extraction_run_id, evidence_json
                 FROM manager_intake_extraction_batches ORDER BY intake_run_id, batch_fingerprint",
            )
            .map_err(sql_error)?;
        let mut rows = statement.query([]).map_err(sql_error)?;
        while let Some(row) = rows.next().map_err(sql_error)? {
            check()?;
            let intake_run_id = row.get::<_, String>(0).map_err(sql_error)?;
            let fingerprint = row.get::<_, String>(1).map_err(sql_error)?;
            let run_id = row.get::<_, String>(2).map_err(sql_error)?;
            let inputs_json = row.get::<_, String>(3).map_err(sql_error)?;
            let inputs: ManagerIntakeBatchInputs = parse_canonical(&inputs_json)?;
            if canonical_json(&inputs)? != inputs_json
                || manager_intake_batch_fingerprint(&inputs)? != fingerprint
            {
                return Ok(false);
            }
            let planned_inputs: Option<String> = self
                .connection
                .query_row(
                    "SELECT canonical_inputs_json
                     FROM manager_intake_extraction_plan_batches
                     WHERE intake_run_id = ?1 AND batch_fingerprint = ?2",
                    params![intake_run_id, fingerprint],
                    |plan| plan.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            if planned_inputs.as_deref() != Some(inputs_json.as_str()) {
                return Ok(false);
            }
            let run: Option<(String, String)> = self
                .connection
                .query_row(
                    "SELECT task_id, status FROM agent_runs WHERE run_id = ?1",
                    [&run_id],
                    |run| Ok((run.get(0)?, run.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let Some((task_id, status)) = run else {
                return Ok(false);
            };
            if status != "completed" {
                return Ok(false);
            }
            let task = load_task(&self.connection, &task_id)?;
            let task_intake = task
                .exact_inputs
                .iter()
                .find(|input| input.kind == "manager_intake_run" && input.version == 1)
                .map(|input| input.reference.as_str());
            let task_evidence = task
                .exact_inputs
                .iter()
                .filter(|input| input.kind == "source_evidence")
                .map(|input| {
                    let (artifact_id, ordinal) = input.reference.split_once('#')?;
                    Some(TenderEvidenceReference {
                        artifact_id: artifact_id.to_owned(),
                        version: input.version,
                        ordinal: ordinal.parse().ok()?,
                    })
                })
                .collect::<Option<Vec<_>>>();
            let task_authorities = task
                .exact_inputs
                .iter()
                .filter(|input| {
                    matches!(
                        input.kind.as_str(),
                        "engineer_entry" | "approved_calculation_run"
                    )
                })
                .map(|input| super::TenderRecordAuthorityReference {
                    authority_id: input.reference.clone(),
                })
                .collect::<Vec<_>>();
            if task_intake != Some(intake_run_id.as_str())
                || task_evidence.as_deref() != Some(inputs.evidence.as_slice())
                || task_authorities != inputs.authorities
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn update_manager_intake_stage(
        &mut self,
        stage: ManagerIntakeStage,
        manager_run_id: Option<&str>,
    ) -> Result<(), TenderCommandError> {
        let updated_at = connection_timestamp(&self.connection)?;
        self.connection
            .execute(
                "UPDATE manager_intake_runs
                 SET stage = ?1, current_manager_run_id = COALESCE(?2, current_manager_run_id),
                     failure_summary = NULL, completed_at = NULL, updated_at = ?3
                 WHERE intake_run_id = (
                   SELECT intake_run_id FROM manager_intake_runs
                   ORDER BY intake_run_sequence DESC LIMIT 1
                 )",
                params![stage.as_str(), manager_run_id, updated_at],
            )
            .map_err(sql_error)?;
        Ok(())
    }
}

fn current_manager_intake_plan_context(
    transaction: &Transaction<'_>,
) -> Result<(String, TenderId, u32, AiExecutionSelection), TenderCommandError> {
    let (intake_run_id, stage, selection_json): (String, String, Option<String>) = transaction
        .query_row(
            "SELECT intake_run_id, stage, provider_selection_json
             FROM manager_intake_runs ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    if !matches!(
        stage.as_str(),
        "reading_documents" | "extracting_tender_facts"
    ) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let selection = selection_json
        .as_deref()
        .map(parse_canonical)
        .transpose()?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
    let (tender_id, tender_revision): (String, u32) = transaction
        .query_row(
            "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    Ok((
        intake_run_id,
        TenderId::parse(&tender_id)?,
        tender_revision,
        selection,
    ))
}

fn current_manager_intake_plan_inputs(
    transaction: &Transaction<'_>,
) -> Result<
    (
        Vec<TenderEvidenceReference>,
        Vec<super::TenderRecordAuthorityReference>,
    ),
    TenderCommandError,
> {
    let mut evidence_statement = transaction
        .prepare(
            "SELECT el.artifact_id, el.version, el.ordinal
             FROM evidence_locations el
             JOIN source_artifacts sa ON sa.artifact_id = el.artifact_id
             JOIN manager_intake_runs mir ON mir.package_intake_id = sa.intake_id
             WHERE mir.intake_run_sequence = (
               SELECT MAX(intake_run_sequence) FROM manager_intake_runs
             )
               AND NOT EXISTS (
                 SELECT 1 FROM source_relationships sr
                 WHERE sr.prior_artifact_id = el.artifact_id
                   AND sr.prior_version = el.version
                   AND sr.relationship_kind = 'replacement'
               )
             ORDER BY el.artifact_id, el.version, el.ordinal",
        )
        .map_err(sql_error)?;
    let evidence = evidence_statement
        .query_map([], |row| {
            Ok(TenderEvidenceReference {
                artifact_id: row.get(0)?,
                version: row.get(1)?,
                ordinal: row.get(2)?,
            })
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    drop(evidence_statement);
    if evidence.is_empty() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut authority_statement = transaction
        .prepare("SELECT authority_id FROM tender_record_authorities ORDER BY authority_id")
        .map_err(sql_error)?;
    let authorities = authority_statement
        .query_map([], |row| {
            Ok(super::TenderRecordAuthorityReference {
                authority_id: row.get(0)?,
            })
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    Ok((evidence, authorities))
}

fn build_manager_intake_plan(
    request: ManagerIntakePlanRequest<'_>,
) -> Result<Vec<PlannedManagerIntakeExtractionBatch>, TenderCommandError> {
    let ManagerIntakePlanRequest {
        transaction,
        tender_id,
        tender_revision,
        intake_run_id,
        evidence,
        authorities,
        selection,
        byte_budget,
    } = request;
    if byte_budget == 0 {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut result = Vec::new();
    let mut current = Vec::new();
    for reference in evidence {
        current.push(reference.clone());
        let estimate = estimate_record_extraction_request_bytes(
            transaction,
            tender_id,
            tender_revision,
            intake_run_id,
            &current,
            authorities,
            selection,
        )?;
        if estimate.estimated_request_bytes <= byte_budget {
            continue;
        }
        let overflow = current
            .pop()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if current.is_empty() {
            return Err(TenderCommandError::new(
                TenderErrorCode::RequestBudgetExceeded,
            ));
        }
        let completed_evidence = std::mem::take(&mut current);
        let completed_estimate = estimate_record_extraction_request_bytes(
            transaction,
            tender_id,
            tender_revision,
            intake_run_id,
            &completed_evidence,
            authorities,
            selection,
        )?;
        result.push(planned_manager_intake_batch(
            completed_evidence,
            authorities,
            completed_estimate,
        )?);
        current.push(overflow);
        let single_estimate = estimate_record_extraction_request_bytes(
            transaction,
            tender_id,
            tender_revision,
            intake_run_id,
            &current,
            authorities,
            selection,
        )?;
        if single_estimate.estimated_request_bytes > byte_budget {
            return Err(TenderCommandError::new(
                TenderErrorCode::RequestBudgetExceeded,
            ));
        }
    }
    if !current.is_empty() {
        let estimate = estimate_record_extraction_request_bytes(
            transaction,
            tender_id,
            tender_revision,
            intake_run_id,
            &current,
            authorities,
            selection,
        )?;
        result.push(planned_manager_intake_batch(
            current,
            authorities,
            estimate,
        )?);
    }
    Ok(result)
}

fn planned_manager_intake_batch(
    evidence: Vec<TenderEvidenceReference>,
    authorities: &[super::TenderRecordAuthorityReference],
    estimate: RecordExtractionRequestEstimate,
) -> Result<PlannedManagerIntakeExtractionBatch, TenderCommandError> {
    let authorities = authorities.to_vec();
    let inputs = ManagerIntakeBatchInputs {
        evidence: evidence.clone(),
        authorities: authorities.clone(),
        request_context: estimate.context.clone(),
    };
    Ok(PlannedManagerIntakeExtractionBatch {
        fingerprint: manager_intake_batch_fingerprint(&inputs)?,
        evidence,
        authorities,
        estimated_request_bytes: estimate.estimated_request_bytes,
        request_context: estimate.context,
    })
}

fn load_manager_intake_plan(
    transaction: &Transaction<'_>,
    intake_run_id: &str,
) -> Result<Vec<PlannedManagerIntakeExtractionBatch>, TenderCommandError> {
    let mut statement = transaction
        .prepare(
            "SELECT batch_fingerprint, canonical_inputs_json, request_context_sha256,
                    estimated_request_bytes, estimator_version
             FROM manager_intake_extraction_plan_batches
             WHERE intake_run_id = ?1 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let mut rows = statement.query([intake_run_id]).map_err(sql_error)?;
    let mut planned = Vec::new();
    while let Some(row) = rows.next().map_err(sql_error)? {
        let fingerprint: String = row.get(0).map_err(sql_error)?;
        let inputs_json: String = row.get(1).map_err(sql_error)?;
        let context_sha256: String = row.get(2).map_err(sql_error)?;
        let estimated_request_bytes = u64::try_from(row.get::<_, i64>(3).map_err(sql_error)?)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let estimator_version: String = row.get(4).map_err(sql_error)?;
        let inputs: ManagerIntakeBatchInputs = parse_canonical(&inputs_json)?;
        if estimator_version != RECORD_EXTRACTION_ESTIMATOR_VERSION
            || inputs.evidence.is_empty()
            || estimated_request_bytes == 0
            || canonical_json(&inputs)? != inputs_json
            || inputs.request_context.request_context_sha256 != context_sha256
            || !record_extraction_request_plan_context_is_valid(&inputs.request_context)?
            || u64::try_from(inputs.request_context.request_body_json.len()).ok()
                != Some(inputs.request_context.request_body_bytes)
            || manager_intake_batch_fingerprint(&inputs)? != fingerprint
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        planned.push(PlannedManagerIntakeExtractionBatch {
            evidence: inputs.evidence,
            authorities: inputs.authorities,
            fingerprint,
            estimated_request_bytes,
            request_context: inputs.request_context,
        });
    }
    Ok(planned)
}

fn completed_manager_intake_batch_fingerprints(
    transaction: &Transaction<'_>,
    intake_run_id: &str,
) -> Result<HashSet<String>, TenderCommandError> {
    transaction
        .prepare(
            "SELECT batch_fingerprint FROM manager_intake_extraction_batches
             WHERE intake_run_id = ?1",
        )
        .map_err(sql_error)?
        .query_map([intake_run_id], |row| row.get(0))
        .map_err(sql_error)?
        .collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(sql_error)
}

pub(super) fn record_manager_intake_extraction_batch(
    transaction: &Transaction<'_>,
    prepared: &PreparedAgentRun,
    completed_at: &str,
) -> Result<(), TenderCommandError> {
    let run_id = prepared.run_id.as_str();
    let Some((expected_intake_run_id, evidence, authorities)) =
        manager_intake_task_batch_inputs(&prepared.task)?
    else {
        return Ok(());
    };
    let Some(intake_run_id) = transaction
        .query_row(
            "SELECT intake_run_id FROM manager_intake_runs
             WHERE stage = 'extracting_tender_facts'
             ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(sql_error)?
    else {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    };
    if intake_run_id != expected_intake_run_id {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    if evidence.is_empty() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let (batch_fingerprint, evidence_json) =
        require_manager_intake_extraction_plan(transaction, prepared, &evidence, &authorities)?;
    transaction
        .execute(
            "INSERT INTO manager_intake_extraction_batches (
               intake_run_id, batch_fingerprint, extraction_run_id,
               evidence_json, completed_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                intake_run_id,
                batch_fingerprint,
                run_id,
                evidence_json,
                completed_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub(super) fn authorize_manager_intake_extraction_completion(
    transaction: &Transaction<'_>,
    prepared: &PreparedAgentRun,
) -> Result<(), TenderCommandError> {
    let Some((_, evidence, authorities)) = manager_intake_task_batch_inputs(&prepared.task)? else {
        return Ok(());
    };
    require_manager_intake_extraction_plan(transaction, prepared, &evidence, &authorities)
        .map(|_| ())
}

pub(super) fn bind_manager_intake_extraction_plan(
    transaction: &Transaction<'_>,
    prepared: &PreparedAgentRun,
    source_run_id: Option<&str>,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let Some((intake_run_id, evidence, authorities)) =
        manager_intake_task_batch_inputs(&prepared.task)?
    else {
        return Ok(());
    };
    let provider_selection_json = canonical_json(&prepared.provider_selection)?;
    let persisted_source_run_id: Option<Option<String>> = transaction
        .query_row(
            "SELECT retry_of_run_id FROM agent_runs WHERE run_id = ?1",
            [&prepared.run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if persisted_source_run_id != Some(source_run_id.map(str::to_owned)) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let run_estimate = estimate_prepared_record_extraction_request_bytes(prepared)?;
    if run_estimate.estimated_request_bytes > record_extraction_request_hard_cap_bytes() {
        return Err(TenderCommandError::new(
            TenderErrorCode::RequestBudgetExceeded,
        ));
    }
    let run_request_context_json = canonical_json(&run_estimate.context)?;
    let (lineage_kind, batch_fingerprint, plan_context_sha, planned_inputs_json) =
        if let Some(source_run_id) = source_run_id {
            let source: Option<(String, String, String, String, String, String, String)> =
                transaction
                    .query_row(
                        "SELECT intake_run_id, batch_fingerprint, plan_request_context_sha256,
                            provider_selection_json, task_id, lineage_kind,
                            run_request_context_json
                     FROM manager_intake_extraction_plan_run_bindings
                     WHERE extraction_run_id = ?1",
                        [source_run_id],
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
                source_intake,
                fingerprint,
                plan_context_sha,
                source_selection,
                source_task_id,
                source_lineage,
                source_context_json,
            )) = source
            else {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            };
            if source_intake != intake_run_id || source_selection != provider_selection_json {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let source_task = load_task(transaction, &source_task_id)?;
            let lineage_kind = if matches!(source_lineage.as_str(), "initial" | "transport_retry")
                && semantic_repair_lineage_is_valid(
                    transaction,
                    prepared,
                    source_run_id,
                    &source_task,
                )? {
                if source_context_json == run_request_context_json {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                "semantic_repair"
            } else if matches!(
                source_lineage.as_str(),
                "initial" | "semantic_repair" | "transport_retry"
            ) && source_task == prepared.task
                && source_context_json == run_request_context_json
            {
                "transport_retry"
            } else {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            };
            let planned_inputs_json: String = transaction
                .query_row(
                    "SELECT canonical_inputs_json FROM manager_intake_extraction_plan_batches
                     WHERE intake_run_id = ?1 AND batch_fingerprint = ?2
                       AND request_context_sha256 = ?3 AND estimator_version = ?4",
                    params![
                        source_intake,
                        fingerprint,
                        plan_context_sha,
                        RECORD_EXTRACTION_ESTIMATOR_VERSION,
                    ],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            (
                lineage_kind,
                fingerprint,
                plan_context_sha,
                planned_inputs_json,
            )
        } else {
            let mut statement = transaction
                .prepare(
                    "SELECT batch_fingerprint, canonical_inputs_json, estimated_request_bytes
                     FROM manager_intake_extraction_plan_batches
                     WHERE intake_run_id = ?1 AND request_context_sha256 = ?2
                       AND estimator_version = ?3",
                )
                .map_err(sql_error)?;
            let matches = statement
                .query_map(
                    params![
                        intake_run_id,
                        run_estimate.context.request_context_sha256,
                        RECORD_EXTRACTION_ESTIMATOR_VERSION,
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .map_err(sql_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sql_error)?;
            let [(fingerprint, planned_inputs_json, planned_estimate)] = matches.as_slice() else {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            };
            if u64::try_from(*planned_estimate).ok() != Some(run_estimate.estimated_request_bytes) {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            (
                "initial",
                fingerprint.clone(),
                run_estimate.context.request_context_sha256.clone(),
                planned_inputs_json.clone(),
            )
        };
    let planned_inputs: ManagerIntakeBatchInputs = parse_canonical(&planned_inputs_json)?;
    if planned_inputs.evidence != evidence
        || planned_inputs.authorities != authorities
        || planned_inputs.request_context.provider_selection_json != provider_selection_json
        || planned_inputs.request_context.request_context_sha256 != plan_context_sha
        || manager_intake_batch_fingerprint(&planned_inputs)? != batch_fingerprint
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    transaction
        .execute(
            "INSERT INTO manager_intake_extraction_plan_run_bindings (
               extraction_run_id, source_run_id, lineage_kind, intake_run_id,
               batch_fingerprint, task_id, provider_selection_json,
               plan_request_context_sha256, run_request_context_json,
               run_request_context_sha256, estimated_request_bytes, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                prepared.run_id,
                source_run_id,
                lineage_kind,
                intake_run_id,
                batch_fingerprint,
                prepared.task.task_id,
                provider_selection_json,
                plan_context_sha,
                run_request_context_json,
                run_estimate.context.request_context_sha256,
                i64::try_from(run_estimate.estimated_request_bytes)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn semantic_repair_lineage_is_valid(
    connection: &rusqlite::Connection,
    prepared: &PreparedAgentRun,
    source_run_id: &str,
    source_task: &TenderTaskView,
) -> Result<bool, TenderCommandError> {
    let Some(feedback) = prepared.task.repair_feedback.as_ref() else {
        return Ok(false);
    };
    if source_task.repair_feedback.is_some() || feedback.rejected_run_id != source_run_id {
        return Ok(false);
    }
    let mut expected_task = source_task.clone();
    expected_task.task_id = prepared.task.task_id.clone();
    expected_task.repair_feedback = Some(feedback.clone());
    if expected_task != prepared.task {
        return Ok(false);
    }
    let rejected: Option<(String, String)> = connection
        .query_row(
            "SELECT payload_sha256, validation_issues_json
             FROM agent_run_rejected_outputs WHERE run_id = ?1",
            [source_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(rejected.as_ref().is_some_and(|(payload_sha, issues_json)| {
        payload_sha == &feedback.rejected_payload_sha256
            && canonical_json(&feedback.validation_issues).ok().as_ref() == Some(issues_json)
    }))
}

fn require_manager_intake_extraction_plan(
    transaction: &Transaction<'_>,
    prepared: &PreparedAgentRun,
    evidence: &[TenderEvidenceReference],
    authorities: &[super::TenderRecordAuthorityReference],
) -> Result<(String, String), TenderCommandError> {
    let provider_selection_json = canonical_json(&prepared.provider_selection)?;
    let binding: Option<ManagerIntakeExtractionPlanRunBinding> = transaction
        .query_row(
            "SELECT intake_run_id, batch_fingerprint, task_id, provider_selection_json,
                    plan_request_context_sha256, run_request_context_json,
                    run_request_context_sha256, estimated_request_bytes,
                    lineage_kind, source_run_id
             FROM manager_intake_extraction_plan_run_bindings
             WHERE extraction_run_id = ?1",
            [&prepared.run_id],
            |row| {
                Ok(ManagerIntakeExtractionPlanRunBinding {
                    intake_run_id: row.get(0)?,
                    batch_fingerprint: row.get(1)?,
                    task_id: row.get(2)?,
                    provider_selection_json: row.get(3)?,
                    plan_request_context_sha256: row.get(4)?,
                    run_request_context_json: row.get(5)?,
                    run_request_context_sha256: row.get(6)?,
                    estimated_request_bytes: row.get(7)?,
                    lineage_kind: row.get(8)?,
                    source_run_id: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some(binding) = binding else {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    };
    if binding.task_id != prepared.task.task_id
        || binding.provider_selection_json != provider_selection_json
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let persisted_binding: Option<(String, Option<String>, String)> = transaction
        .query_row(
            "SELECT agent_runs.task_id, agent_runs.retry_of_run_id,
                    agent_run_provider_bindings.binding_json
             FROM agent_runs
             JOIN agent_run_provider_bindings USING (run_id)
             WHERE agent_runs.run_id = ?1",
            [&prepared.run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    if persisted_binding.as_ref()
        != Some(&(
            binding.task_id.clone(),
            binding.source_run_id.clone(),
            binding.provider_selection_json.clone(),
        ))
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let run_context: RecordExtractionRequestPlanContext =
        parse_canonical(&binding.run_request_context_json)?;
    let recomputed_context = record_extraction_request_context_for_prepared_with_stored_data_views(
        prepared,
        &run_context,
    )?;
    if canonical_json(&run_context)? != binding.run_request_context_json
        || !record_extraction_request_plan_context_is_valid(&run_context)?
        || run_context.request_context_sha256 != binding.run_request_context_sha256
        || recomputed_context != run_context
        || u64::try_from(binding.estimated_request_bytes).ok()
            != Some(estimated_record_extraction_request_bytes(
                run_context.request_body_bytes,
            )?)
        || u64::try_from(binding.estimated_request_bytes)
            .ok()
            .is_none_or(|bytes| bytes > record_extraction_request_hard_cap_bytes())
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let planned_inputs_json: String = transaction
        .query_row(
            "SELECT canonical_inputs_json FROM manager_intake_extraction_plan_batches
             WHERE intake_run_id = ?1 AND batch_fingerprint = ?2
               AND request_context_sha256 = ?3 AND estimator_version = ?4",
            params![
                binding.intake_run_id,
                binding.batch_fingerprint,
                binding.plan_request_context_sha256,
                RECORD_EXTRACTION_ESTIMATOR_VERSION,
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let planned_inputs: ManagerIntakeBatchInputs = parse_canonical(&planned_inputs_json)?;
    if planned_inputs.evidence != evidence
        || planned_inputs.authorities != authorities
        || planned_inputs.request_context.provider_selection_json != binding.provider_selection_json
        || planned_inputs.request_context.request_context_sha256
            != binding.plan_request_context_sha256
        || manager_intake_batch_fingerprint(&planned_inputs)? != binding.batch_fingerprint
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let lineage_is_valid = match (
        binding.lineage_kind.as_str(),
        binding.source_run_id.as_deref(),
    ) {
        ("initial", None) => {
            prepared.task.repair_feedback.is_none() && run_context == planned_inputs.request_context
        }
        ("semantic_repair", Some(source_run_id)) => {
            let source_task_id: Option<String> = transaction
                .query_row(
                    "SELECT task_id FROM manager_intake_extraction_plan_run_bindings
                     WHERE extraction_run_id = ?1 AND intake_run_id = ?2
                       AND batch_fingerprint = ?3
                       AND lineage_kind IN ('initial', 'transport_retry')
                       AND plan_request_context_sha256 = ?4",
                    params![
                        source_run_id,
                        binding.intake_run_id,
                        binding.batch_fingerprint,
                        binding.plan_request_context_sha256
                    ],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            let Some(source_task_id) = source_task_id else {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            };
            semantic_repair_lineage_is_valid(
                transaction,
                prepared,
                source_run_id,
                &load_task(transaction, &source_task_id)?,
            )? && run_context != planned_inputs.request_context
        }
        ("transport_retry", Some(source_run_id)) => {
            let source: Option<(String, String, String)> = transaction
                .query_row(
                    "SELECT task_id, provider_selection_json, run_request_context_json
                     FROM manager_intake_extraction_plan_run_bindings
                     WHERE extraction_run_id = ?1 AND intake_run_id = ?2
                       AND batch_fingerprint = ?3
                       AND lineage_kind IN ('initial', 'semantic_repair', 'transport_retry')
                       AND plan_request_context_sha256 = ?4",
                    params![
                        source_run_id,
                        binding.intake_run_id,
                        binding.batch_fingerprint,
                        binding.plan_request_context_sha256
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?;
            source.as_ref().is_some_and(
                |(source_task_id, source_selection, source_context_json)| {
                    source_selection == &binding.provider_selection_json
                        && source_context_json == &binding.run_request_context_json
                        && load_task(transaction, source_task_id).ok().as_ref()
                            == Some(&prepared.task)
                },
            )
        }
        _ => false,
    };
    if !lineage_is_valid {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((binding.batch_fingerprint, planned_inputs_json))
}

fn manager_intake_task_batch_inputs(
    task: &TenderTaskView,
) -> Result<Option<ManagerIntakeTaskBatchInputs>, TenderCommandError> {
    let Some(intake_run_id) = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "manager_intake_run" && input.version == 1)
        .map(|input| input.reference.clone())
    else {
        return Ok(None);
    };
    let evidence = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "source_evidence")
        .map(|input| {
            let (artifact_id, ordinal) = input
                .reference
                .split_once('#')
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            Ok(TenderEvidenceReference {
                artifact_id: artifact_id.to_owned(),
                version: input.version,
                ordinal: ordinal
                    .parse()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            })
        })
        .collect::<Result<Vec<_>, TenderCommandError>>()?;
    let authorities = task
        .exact_inputs
        .iter()
        .filter(|input| {
            matches!(
                input.kind.as_str(),
                "engineer_entry" | "approved_calculation_run"
            )
        })
        .map(|input| super::TenderRecordAuthorityReference {
            authority_id: input.reference.clone(),
        })
        .collect();
    Ok(Some((intake_run_id, evidence, authorities)))
}

fn manager_intake_batch_fingerprint(
    inputs: &ManagerIntakeBatchInputs,
) -> Result<String, TenderCommandError> {
    let input_json = canonical_json(inputs)?;
    Ok(sha256_hex(input_json.as_bytes()))
}

pub(super) fn publish_manager_intake_outcome(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    manager_run_id: &str,
    task: &TenderTaskView,
    candidate: &ManagerIntakeCandidate,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let exact_revision = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "tender_revision" && input.reference == tender_id.as_str())
        .map(|input| input.version);
    if exact_revision != Some(tender_revision) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let exact_heads = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "tender_record_version")
        .map(|input| (input.reference.as_str(), input.version))
        .collect::<HashSet<_>>();
    let current_heads = transaction
        .prepare("SELECT record_id, current_version FROM tender_record_heads")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
                })?
                .collect::<rusqlite::Result<HashSet<_>>>()
        })
        .map_err(sql_error)?;
    if exact_heads.len() != current_heads.len()
        || current_heads
            .iter()
            .any(|(record_id, version)| !exact_heads.contains(&(record_id.as_str(), *version)))
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let body = canonical_outcome_body(candidate)?;
    let (intake_run_id, current_manager_run_id): (String, Option<String>) = transaction
        .query_row(
            "SELECT intake_run_id, current_manager_run_id FROM manager_intake_runs
             ORDER BY intake_run_sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    if current_manager_run_id.as_deref() != Some(manager_run_id) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let conversation_id: String = transaction
        .query_row(
            "SELECT conversation_id FROM manager_workspace_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let message_id = random_identifier(transaction)?;
    let updated = transaction
        .execute(
            "INSERT INTO tender_office_messages (
               message_id, conversation_id, author, kind, body, created_at
             ) VALUES (?1, ?2, 'manager', ?3, ?4, ?5)",
            params![
                message_id,
                conversation_id,
                if candidate.kind == ManagerIntakeOutcomeKind::Question {
                    "question"
                } else {
                    "output"
                },
                body,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    if updated != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let outcome_id = random_identifier(transaction)?;
    let manifest = ManagerIntakeOutcomeManifest {
        schema_version: 1,
        outcome_id: &outcome_id,
        intake_run_id: &intake_run_id,
        manager_run_id,
        message_id: &message_id,
        kind: candidate.kind,
        body: &body,
        question: &candidate.question,
        recommendation: &candidate.recommendation,
        supporting_records: &candidate.supporting_records,
        supporting_evidence: &candidate.supporting_evidence,
        created_at,
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    transaction
        .execute(
            "INSERT INTO manager_intake_outcomes (
               outcome_id, intake_run_id, manager_run_id, message_id, kind, body,
               question, recommendation, supporting_records_json,
               supporting_evidence_json, manifest_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                outcome_id,
                intake_run_id,
                manager_run_id,
                message_id,
                candidate.kind.as_str(),
                body,
                candidate.question,
                candidate
                    .recommendation
                    .map(ManagerBidRecommendation::as_str),
                canonical_json(&candidate.supporting_records)?,
                canonical_json(&candidate.supporting_evidence)?,
                manifest_json,
                manifest_sha256,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    let mut ordinal = 1_u32;
    insert_message_reference(
        transaction,
        &message_id,
        ordinal,
        &WorkspaceMessageReference {
            kind: WorkspaceMessageReferenceKind::AgentRun,
            reference: manager_run_id.to_owned(),
            version: 1,
            evidence_ordinal: None,
            label: "Tendering Manager Agent Run".into(),
            detail: None,
        },
    )?;
    ordinal += 1;
    insert_message_reference(
        transaction,
        &message_id,
        ordinal,
        &WorkspaceMessageReference {
            kind: WorkspaceMessageReferenceKind::ManagerIntakeOutcome,
            reference: outcome_id.clone(),
            version: 1,
            evidence_ordinal: None,
            label: "Manager intake outcome".into(),
            detail: Some(candidate.kind.as_str().into()),
        },
    )?;
    ordinal += 1;
    for reference in &candidate.supporting_records {
        let record = load_record_label(transaction, reference)?;
        insert_message_reference(
            transaction,
            &message_id,
            ordinal,
            &WorkspaceMessageReference {
                kind: WorkspaceMessageReferenceKind::TenderRecord,
                reference: reference.record_id.clone(),
                version: reference.version,
                evidence_ordinal: None,
                label: record.0,
                detail: Some(record.1),
            },
        )?;
        ordinal += 1;
    }
    for reference in &candidate.supporting_evidence {
        let evidence = load_evidence_label(transaction, reference)?;
        insert_message_reference(
            transaction,
            &message_id,
            ordinal,
            &WorkspaceMessageReference {
                kind: WorkspaceMessageReferenceKind::SourceEvidence,
                reference: reference.artifact_id.clone(),
                version: reference.version,
                evidence_ordinal: Some(reference.ordinal),
                label: evidence.0,
                detail: Some(evidence.1),
            },
        )?;
        ordinal += 1;
    }
    let terminal_stage = if candidate.kind == ManagerIntakeOutcomeKind::Question {
        ManagerIntakeStage::WaitingForEngineer
    } else {
        ManagerIntakeStage::BidDecisionReady
    };
    let updated = transaction
        .execute(
            "UPDATE manager_intake_runs
             SET stage = ?2, failure_summary = NULL, updated_at = ?3, completed_at = ?3
             WHERE intake_run_id = ?1 AND current_manager_run_id = ?4",
            params![
                intake_run_id,
                terminal_stage.as_str(),
                created_at,
                manager_run_id
            ],
        )
        .map_err(sql_error)?;
    if updated != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    transaction
        .execute(
            "UPDATE manager_workspace_state SET last_activity_at = ?1 WHERE singleton = 1",
            [created_at],
        )
        .map_err(sql_error)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "manager_intake_outcome_presented",
        tender_revision,
        json!({
            "intake_run_id": intake_run_id,
            "kind": candidate.kind.as_str(),
            "manager_run_id": manager_run_id,
            "message_id": message_id,
            "outcome_id": outcome_id,
            "supporting_evidence_count": candidate.supporting_evidence.len().to_string(),
            "supporting_record_count": candidate.supporting_records.len().to_string(),
        }),
        created_at,
    )
}

fn manager_intake_profile(profile_id: String) -> AgentProfileVersionView {
    AgentProfileVersionView {
        profile_id,
        version: 1,
        identity: "Tendering Manager".into(),
        profession: "Tendering Manager Agent".into(),
        seniority: "Senior".into(),
        capabilities: vec![MANAGER_INTAKE_CAPABILITY.into()],
        objective: "Lead the restricted Tender intake and present exactly one evidence-linked next decision to the Tendering Engineer.".into(),
        behavior: "Use the controlled Tender record, ask only for genuinely missing information, and keep routine orchestration out of the Engineer's way.".into(),
        skepticism: "Treat every unsupported fact as unresolved and preserve exact provenance.".into(),
        risk_tolerance: "Low tolerance for unsupported bid commitments or invented certainty.".into(),
        instructions: "Review only the supplied exact current Tender Records. Treat proposed, rejected, stale, unresolved, contradictory, or materially uncertain records as requiring the Engineer. An Engineer-approved assumption is resolved unless it still contains a contradiction. Ask exactly one concise question when a genuine gap remains. Otherwise present one concise Bid Decision recommendation and rationale. Cite at least one supplied exact record and one Evidence reference. Do not write display copy: Quantix derives it canonically from the structured question or recommendation. Make no approval decision.".into(),
        output_contract_json: manager_intake_output_contract(),
        review_policy: "The result remains an attributable Agent proposal. The Tendering Engineer retains every formal approval.".into(),
        permissions: manager_intake_permissions(),
        prohibited_actions: vec![
            "approve_tender_decision".into(),
            "mutate_tender_store_directly".into(),
            "perform_external_action".into(),
            "access_secret_data".into(),
        ],
        resource_budget: manager_intake_budget(),
    }
}

fn manager_intake_task(
    task_id: String,
    tender_id: &TenderId,
    tender_revision: u32,
    records: &[TenderRecordInspection],
    deadline: String,
    profile: &AgentProfileVersionView,
) -> TenderTaskView {
    let mut exact_inputs = vec![AgentTaskInputReference {
        kind: "tender_revision".into(),
        reference: tender_id.as_str().into(),
        version: tender_revision,
    }];
    exact_inputs.extend(records.iter().map(|record| AgentTaskInputReference {
        kind: "tender_record_version".into(),
        reference: record.record_id.clone(),
        version: record.version,
    }));
    TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: "Present the first genuinely required Engineer question or an evidence-linked Bid Decision recommendation from the current exact Tender intake record.".into(),
        exact_inputs,
        output_contract_json: profile.output_contract_json.clone(),
        review_policy: profile.review_policy.clone(),
        deadline,
        permissions: manager_intake_permissions(),
        resource_budget: profile.resource_budget.clone(),
        repair_feedback: None,
    }
}

fn manager_intake_permissions() -> AgentRunPermissions {
    AgentRunPermissions {
        data_scopes: vec![MANAGER_INTAKE_SCOPE.into()],
        data_classifications: vec![DataClassification::TenderInternal],
        allowed_actions: vec![MANAGER_INTAKE_ACTION.into()],
        allowed_tools: Vec::new(),
        network_allowed: false,
        workspace_write_allowed: true,
    }
}

fn manager_intake_budget() -> AgentResourceBudget {
    #[cfg(feature = "runtime-fixture")]
    let duration_seconds = 8;
    #[cfg(not(feature = "runtime-fixture"))]
    let duration_seconds = 120;
    AgentResourceBudget {
        provider_turns: 1,
        duration_seconds,
        output_bytes: 32 * 1024,
    }
}

fn manager_intake_output_contract() -> String {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "kind": { "enum": ["question", "bid_decision"] },
            "question": { "type": ["string", "null"], "maxLength": 4000 },
            "recommendation": { "type": ["string", "null"], "enum": ["proceed", "hold", "decline", null] },
            "rationale": { "type": ["string", "null"], "maxLength": 3500 },
            "supporting_evidence": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "artifact_id": { "maxLength": 32, "minLength": 32, "type": "string" },
                        "ordinal": { "minimum": 1, "type": "integer" },
                        "version": { "minimum": 1, "type": "integer" }
                    },
                    "required": ["artifact_id", "version", "ordinal"],
                    "type": "object"
                },
                "maxItems": MAX_SUPPORTING_EVIDENCE,
                "minItems": 1,
                "type": "array"
            },
            "supporting_records": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "record_id": { "maxLength": 32, "minLength": 32, "type": "string" },
                        "version": { "minimum": 1, "type": "integer" }
                    },
                    "required": ["record_id", "version"],
                    "type": "object"
                },
                "maxItems": MAX_SUPPORTING_RECORDS,
                "minItems": 1,
                "type": "array"
            }
        },
        "required": ["kind", "question", "recommendation", "rationale", "supporting_records", "supporting_evidence"],
        "type": "object"
    }))
    .expect("static Manager intake output contract is canonical JSON")
}

fn record_has_unresolved_gap(record: &TenderRecordInspection) -> bool {
    if !record.contradictions.is_empty() {
        return true;
    }
    if record.trust_class == super::TenderRecordTrustClass::ApprovedAssumption {
        return false;
    }
    matches!(
        record.kind,
        TenderRecordKind::TenderQuery | TenderRecordKind::Assumption
    ) || record.fields.iter().any(|field| {
        matches!(
            field.basis_kind,
            TenderRecordBasisKind::TenderQuery | TenderRecordBasisKind::Assumption
        ) || field.uncertainty.is_some()
    })
}

fn record_is_bid_admissible(record: &TenderRecordInspection) -> bool {
    record.verification_status == VerificationStatus::Verified
        && matches!(
            record.trust_class,
            super::TenderRecordTrustClass::Verified
                | super::TenderRecordTrustClass::EngineerVerified
                | super::TenderRecordTrustClass::ApprovedAssumption
                | super::TenderRecordTrustClass::DeterministicFact
        )
}

fn canonical_outcome_body(
    candidate: &ManagerIntakeCandidate,
) -> Result<String, TenderCommandError> {
    match candidate.kind {
        ManagerIntakeOutcomeKind::Question => candidate
            .question
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        ManagerIntakeOutcomeKind::BidDecision => {
            let recommendation = candidate
                .recommendation
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let rationale = candidate
                .rationale
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let label = match recommendation {
                ManagerBidRecommendation::Proceed => "Proceed",
                ManagerBidRecommendation::Hold => "Hold",
                ManagerBidRecommendation::Decline => "Decline",
            };
            Ok(format!("Recommendation: {label}.\n\n{rationale}"))
        }
    }
}

fn manager_intake_data_view(
    tender_id: &TenderId,
    tender_revision: u32,
    records: &[TenderRecordInspection],
) -> Result<Value, TenderCommandError> {
    let records = records
        .iter()
        .map(|record| {
            json!({
                "record_id": record.record_id,
                "version": record.version,
                "kind": record.kind,
                "title": record.title,
                "verification_status": record.verification_status,
                "trust_class": record.trust_class,
                "fields": record.fields.iter().map(|field| json!({
                    "name": field.name,
                    "value": field.value,
                    "basis_kind": field.basis_kind,
                    "basis_reference": field.basis_reference,
                    "basis_description": field.basis_description,
                    "uncertainty": field.uncertainty,
                    "evidence": field.evidence.iter().map(|evidence| json!({
                        "reference": evidence.reference,
                        "package_path": evidence.package_path,
                        "structural_path": evidence.location.structural_path,
                        "excerpt": bounded_detail(&evidence.location.original_text, 600),
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "contradictions": record.contradictions.iter().map(|contradiction| json!({
                    "field_name": contradiction.field_name,
                    "summary": contradiction.summary,
                    "evidence": contradiction.evidence.iter().map(|evidence| evidence.reference.clone()).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "data_classification": DataClassification::TenderInternal,
        "data_scope": MANAGER_INTAKE_SCOPE,
        "schema_version": 1,
        "tender_id": tender_id.as_str(),
        "tender_revision": tender_revision,
        "instruction": "Return one question only for a genuine unresolved gap. If no genuine gap exists, return one Bid Decision recommendation. Use only exact supplied references.",
        "records": records,
    }))
}

fn current_record_inspections(
    store: &TenderStore,
) -> Result<Vec<TenderRecordInspection>, TenderCommandError> {
    let references = store
        .connection
        .prepare("SELECT record_id, current_version FROM tender_record_heads ORDER BY record_id")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok(TenderRecordVersionReference {
                        record_id: row.get(0)?,
                        version: row.get(1)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(sql_error)?;
    references
        .into_iter()
        .map(|reference| {
            store.inspect_tender_record_version(&reference.record_id, reference.version)
        })
        .collect()
}

fn collect_record_evidence(
    record: &TenderRecordInspection,
    target: &mut HashSet<TenderEvidenceReference>,
) {
    target.extend(record.fields.iter().flat_map(|field| {
        field
            .evidence
            .iter()
            .map(|evidence| evidence.reference.clone())
    }));
    target.extend(record.contradictions.iter().flat_map(|contradiction| {
        contradiction
            .evidence
            .iter()
            .map(|evidence| evidence.reference.clone())
    }));
    if let Some(instruction) = &record.generation_instruction {
        target.extend(
            instruction
                .evidence
                .iter()
                .map(|evidence| evidence.reference.clone()),
        );
    }
}

fn insert_message_reference(
    transaction: &Transaction<'_>,
    message_id: &str,
    ordinal: u32,
    reference: &WorkspaceMessageReference,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO tender_office_message_references (
               message_id, ordinal, kind, reference, version, evidence_ordinal, label, detail
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                message_id,
                ordinal,
                reference.kind.as_str(),
                &reference.reference,
                reference.version,
                reference.evidence_ordinal,
                &reference.label,
                reference.detail.as_deref(),
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn load_record_label(
    transaction: &Transaction<'_>,
    reference: &TenderRecordVersionReference,
) -> Result<(String, String), TenderCommandError> {
    transaction
        .query_row(
            "SELECT title, kind FROM tender_record_versions
             WHERE record_id = ?1 AND version = ?2",
            params![reference.record_id, reference.version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)
}

fn load_evidence_label(
    transaction: &Transaction<'_>,
    reference: &TenderEvidenceReference,
) -> Result<(String, String), TenderCommandError> {
    let (package_path, structural_path, original_text): (String, String, String) = transaction
        .query_row(
            "SELECT source_artifacts.package_path, evidence_locations.structural_path,
                    evidence_locations.original_text
             FROM evidence_locations JOIN source_artifacts USING (artifact_id)
             WHERE evidence_locations.artifact_id = ?1
               AND evidence_locations.version = ?2
               AND evidence_locations.ordinal = ?3",
            params![reference.artifact_id, reference.version, reference.ordinal],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    Ok((
        package_path,
        bounded_detail(
            &format!("{structural_path} — {original_text}"),
            MAX_MESSAGE_REFERENCE_DETAIL,
        ),
    ))
}

fn bounded_detail(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_owned();
    }
    let mut end = maximum;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn connection_timestamp(connection: &rusqlite::Connection) -> Result<String, TenderCommandError> {
    connection
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)
}

fn sqlite_epoch_seconds(transaction: &Transaction<'_>) -> Result<i64, TenderCommandError> {
    transaction
        .query_row("SELECT unixepoch('now')", [], |row| row.get(0))
        .map_err(sql_error)
}

fn manager_provider_retry_deadline(
    source_run: &AgentRunInspection,
    now: i64,
    attempt: u32,
) -> Result<i64, TenderCommandError> {
    if !(1..=3).contains(&attempt) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let usage_reset = source_run
        .usage
        .rate_limit
        .as_ref()
        .into_iter()
        .flat_map(|rate_limit| [rate_limit.primary.as_ref(), rate_limit.secondary.as_ref()])
        .flatten()
        .filter_map(|window| window.resets_at_epoch_seconds)
        .filter(|reset| *reset > now)
        .max();
    if let Some(reset) = usage_reset {
        return Ok(reset);
    }
    let retry_after_seconds = source_run
        .failure
        .as_ref()
        .and_then(|failure| failure.retry_after_milliseconds)
        .map(|milliseconds| milliseconds.saturating_add(999) / 1_000)
        .filter(|seconds| *seconds > 0);
    let delay = retry_after_seconds.unwrap_or(match attempt {
        1 => 60,
        2 => 120,
        3 => 240,
        _ => unreachable!(),
    });
    let delay = i64::try_from(delay)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    now.checked_add(delay)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))
}

fn parse_canonical<T: for<'de> Deserialize<'de> + Serialize>(
    value: &str,
) -> Result<T, TenderCommandError> {
    let parsed = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

fn same_provider_choice(left: &AiExecutionSelection, right: &AiExecutionSelection) -> bool {
    left.connection_id == right.connection_id
        && left.provider == right.provider
        && left.model_id == right.model_id
        && left.reasoning == right.reasoning
}

fn checked_u32(value: i64) -> Result<u32, TenderCommandError> {
    value
        .try_into()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}
