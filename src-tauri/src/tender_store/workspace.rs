use garde::Validate;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use std::{fs, path::Path, time::Duration};

use crate::tender_intake::PackageIntakeControl;
use crate::{QuantixHost, TenderPackageSourceKind};

use super::{
    append_audit_event, metadata_is_unsafe_storage_link, random_identifier, require_setup,
    sha256_hex, sql_error, sqlite_timestamp, storage_publication_failpoint, store_unavailable,
    tender_records::insert_engineer_entry, ManagerIntakeStage, ManagerIntakeStatus,
    TenderAiExecutionBinding, TenderAiSelectionReadiness, TenderCommandError, TenderErrorCode,
    TenderId, TenderLifecyclePhase, TenderStore, WorkPlanCapabilityGap, WorkspaceMessageReference,
    WorkspaceTenderDocument, MAX_TENDER_NAME_BYTES,
};

const MAX_CONVERSATION_MESSAGES: i64 = 100;
pub(super) const MAX_MESSAGE_BYTES: usize = 4_000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectManagerWorkspaceCommand {
    pub tender_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SelectManagerWorkspaceTenderCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct StartManagerTenderCommand {
    pub source_kind: TenderPackageSourceKind,
    pub local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RecordEngineerWorkspaceMessageCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub body: String,
    #[garde(skip)]
    pub attachment_refs: Vec<WorkspaceMessageReference>,
    #[garde(skip)]
    pub context_refs: Vec<WorkspaceMessageReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SearchManagerWorkspaceCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 200))]
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RetryManagerIntakeCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RebindManagerIntakeProviderCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderOfficeMessageAuthor {
    Engineer,
    Manager,
    System,
}

impl TenderOfficeMessageAuthor {
    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "engineer" => Ok(Self::Engineer),
            "manager" => Ok(Self::Manager),
            "system" => Ok(Self::System),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderOfficeMessageKind {
    Routine,
    Status,
    Question,
    Finding,
    Handoff,
    Blocker,
    Output,
}

impl TenderOfficeMessageKind {
    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "routine" => Ok(Self::Routine),
            "status" => Ok(Self::Status),
            "question" => Ok(Self::Question),
            "finding" => Ok(Self::Finding),
            "handoff" => Ok(Self::Handoff),
            "blocker" => Ok(Self::Blocker),
            "output" => Ok(Self::Output),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderOfficeMessage {
    pub message_id: String,
    pub sequence: u32,
    pub author: TenderOfficeMessageAuthor,
    pub kind: TenderOfficeMessageKind,
    pub body: String,
    pub created_at: String,
    pub references: Vec<WorkspaceMessageReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ManagerConversation {
    pub conversation_id: String,
    pub messages: Vec<TenderOfficeMessage>,
    pub latest_meaningful_message_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ManagerWorkspaceTenderState {
    Active,
    Archived,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ManagerWorkspaceTender {
    pub tender_id: String,
    pub name: String,
    pub revision: u32,
    pub phase: TenderLifecyclePhase,
    pub needs_engineer: bool,
    pub state: ManagerWorkspaceTenderState,
    pub can_archive: bool,
    pub can_delete: bool,
    pub last_activity_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceActionKind {
    StartTender,
    AddTenderPackage,
    ReviewIntake,
    ObserveIntake,
    ConfigureAiProvider,
    AnswerManagerQuestion,
    RetryIntake,
    ReviewBidDecision,
    PrepareWorkPlan,
    ReviewWorkPlan,
    ReviewWork,
    ReviewIntegratedWork,
    ReviewChange,
    ReviewSubmissionPackage,
    ReviewFinalPackage,
    DraftExternalRfi,
    ReviewExternalRfi,
    InterpretExternalRfiResponse,
    ReviewBasisOfEstimate,
    TenderDeclined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceExternalRfiStatus {
    AwaitingReview,
    ReviewFailed,
    AwaitingApproval,
    ApprovedForIssue,
    ResponseAwaitingInterpretation,
    QueryBasisStale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceExternalRfiSummary {
    pub rfi_id: String,
    pub version: u32,
    pub status: WorkspaceExternalRfiStatus,
    pub question_count: u32,
    pub response_count: u32,
    pub approval_pending: bool,
    pub export_pending: bool,
    pub interpretation_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceEstimateStatus {
    AwaitingReview,
    ReviewFailed,
    AwaitingApproval,
    Approved,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceEstimateSummary {
    pub basis_id: String,
    pub version: u32,
    pub status: WorkspaceEstimateStatus,
    pub boq_row_count: u32,
    pub finding_count: u32,
    pub calculation_run_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceCurrentAction {
    pub kind: WorkspaceActionKind,
    pub title: String,
    pub summary: String,
    pub action_label: String,
    pub requires_engineer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceCapabilityReadinessState {
    NotPlanned,
    Ready,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceCapabilityReadiness {
    pub state: WorkspaceCapabilityReadinessState,
    pub gaps: Vec<WorkPlanCapabilityGap>,
    pub blocker_codes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceDoctorBlockerArea {
    AiExecution,
    Capability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceDoctorBlockerSummary {
    pub code: String,
    pub area: WorkspaceDoctorBlockerArea,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceWorkSummary {
    pub needs_engineer: u32,
    pub working: u32,
    pub waiting: u32,
    pub done: u32,
    pub cancelled: u32,
    pub failed: u32,
    pub tasks: Vec<WorkspaceTaskRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceFilesSummary {
    pub tender_document_count: u32,
    pub quantix_output_count: u32,
    pub tender_documents: Vec<WorkspaceTenderDocument>,
    pub quantix_outputs: Vec<WorkspaceOutputReference>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceTeamSummary {
    pub active_agent_runs: u32,
    pub waiting_tasks: u32,
    pub needs_engineer: u32,
    pub events: Vec<TenderOfficeMessage>,
    pub agent_runs: Vec<WorkspaceAgentRunReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceTaskState {
    Waiting,
    Working,
    NeedsEngineer,
    Paused,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceAgentReference {
    pub profile_id: String,
    pub profile_version: u32,
    pub identity: String,
    pub profession: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceTaskRow {
    pub production_task_id: String,
    pub task_id: Option<String>,
    pub task_key: String,
    pub objective: Option<String>,
    pub state: WorkspaceTaskState,
    pub status_detail: String,
    pub dependencies: Vec<String>,
    pub agent: Option<WorkspaceAgentReference>,
    pub current_run_id: Option<String>,
    pub output_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceAgentRunReference {
    pub run_id: String,
    pub task_id: String,
    pub state: String,
    pub agent: WorkspaceAgentReference,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceOutputReference {
    pub artifact_id: String,
    pub version: u32,
    pub production_task_id: String,
    pub author_run_id: String,
    pub payload_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkspaceSearchResultKind {
    Conversation,
    Work,
    Files,
    Evidence,
    Agents,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceSearchHit {
    pub kind: WorkspaceSearchResultKind,
    pub reference: String,
    pub version: Option<u32>,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceSearchGroup {
    pub kind: WorkspaceSearchResultKind,
    pub hits: Vec<WorkspaceSearchHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceSearchProjection {
    pub query: String,
    pub groups: Vec<WorkspaceSearchGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ManagerWorkspaceProjection {
    pub catalogue: Vec<ManagerWorkspaceTender>,
    pub selected_tender: Option<ManagerWorkspaceTender>,
    pub conversation: Option<ManagerConversation>,
    pub current_action: WorkspaceCurrentAction,
    pub work: WorkspaceWorkSummary,
    pub files: WorkspaceFilesSummary,
    pub team: WorkspaceTeamSummary,
    pub external_rfis: Vec<WorkspaceExternalRfiSummary>,
    pub estimate: Option<WorkspaceEstimateSummary>,
    pub intake: Option<ManagerIntakeStatus>,
    pub ai_execution: Option<TenderAiExecutionBinding>,
    pub capability_readiness: Option<WorkspaceCapabilityReadiness>,
    pub doctor_blockers: Vec<WorkspaceDoctorBlockerSummary>,
}

pub(super) fn initialize_manager_workspace(
    transaction: &Transaction<'_>,
    name: &str,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let conversation_id = random_identifier(transaction)?;
    let message_id = random_identifier(transaction)?;
    transaction
        .execute(
            "INSERT INTO tender_office_conversations (conversation_id, created_at)
             VALUES (?1, ?2)",
            params![conversation_id, created_at],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "INSERT INTO manager_workspace_state (singleton, conversation_id, last_activity_at)
             VALUES (1, ?1, ?2)",
            params![conversation_id, created_at],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "INSERT INTO tender_office_messages (
               message_id, conversation_id, author, kind, body, created_at
             ) VALUES (?1, ?2, 'system', 'status', ?3, ?4)",
            params![
                message_id,
                conversation_id,
                format!("{name} workspace is ready."),
                created_at
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub(super) fn append_system_status(
    transaction: &Transaction<'_>,
    body: &str,
    created_at: &str,
) -> Result<String, TenderCommandError> {
    if body.is_empty() || body.len() > MAX_MESSAGE_BYTES {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let conversation_id = transaction
        .query_row(
            "SELECT conversation_id FROM manager_workspace_state WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(sql_error)?;
    let message_id = random_identifier(transaction)?;
    transaction
        .execute(
            "INSERT INTO tender_office_messages (
               message_id, conversation_id, author, kind, body, created_at
             ) VALUES (?1, ?2, 'system', 'status', ?3, ?4)",
            params![message_id, conversation_id, body, created_at],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE manager_workspace_state SET last_activity_at = ?1 WHERE singleton = 1",
            [created_at],
        )
        .map_err(sql_error)?;
    Ok(message_id)
}

pub(super) fn append_system_message(
    transaction: &Transaction<'_>,
    kind: TenderOfficeMessageKind,
    body: &str,
    created_at: &str,
) -> Result<String, TenderCommandError> {
    append_office_message(transaction, "system", kind, body, created_at)
}

pub(super) fn append_manager_message(
    transaction: &Transaction<'_>,
    kind: TenderOfficeMessageKind,
    body: &str,
    created_at: &str,
) -> Result<String, TenderCommandError> {
    append_office_message(transaction, "manager", kind, body, created_at)
}

fn append_office_message(
    transaction: &Transaction<'_>,
    author: &str,
    kind: TenderOfficeMessageKind,
    body: &str,
    created_at: &str,
) -> Result<String, TenderCommandError> {
    if body.is_empty() || body.len() > MAX_MESSAGE_BYTES || kind == TenderOfficeMessageKind::Routine
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let conversation_id = transaction
        .query_row(
            "SELECT conversation_id FROM manager_workspace_state WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(sql_error)?;
    let message_id = random_identifier(transaction)?;
    transaction
        .execute(
            "INSERT INTO tender_office_messages (
               message_id, conversation_id, author, kind, body, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                message_id,
                conversation_id,
                author,
                message_kind_value(kind),
                body,
                created_at
            ],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "UPDATE manager_workspace_state SET last_activity_at = ?1 WHERE singleton = 1",
            [created_at],
        )
        .map_err(sql_error)?;
    Ok(message_id)
}

struct WorkspaceSnapshot {
    tender: ManagerWorkspaceTender,
    conversation: ManagerConversation,
    current_action: WorkspaceCurrentAction,
    work: WorkspaceWorkSummary,
    files: WorkspaceFilesSummary,
    team: WorkspaceTeamSummary,
    external_rfis: Vec<WorkspaceExternalRfiSummary>,
    estimate: Option<WorkspaceEstimateSummary>,
    intake: Option<ManagerIntakeStatus>,
    ai_execution: TenderAiExecutionBinding,
    capability_readiness: WorkspaceCapabilityReadiness,
    doctor_blockers: Vec<WorkspaceDoctorBlockerSummary>,
}

impl TenderStore {
    fn workspace_snapshot(&self) -> Result<WorkspaceSnapshot, TenderCommandError> {
        let summary = self.summary()?;
        let last_activity_at = self
            .connection
            .query_row(
                "SELECT last_activity_at FROM manager_workspace_state WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        let conversation = self.manager_conversation()?;
        let work = self.workspace_work_summary()?;
        let files = self.workspace_files_summary()?;
        let team = self.workspace_team_summary(&work, &conversation)?;
        let intake = self.current_manager_intake_status()?;
        let ai_execution = self.inspect_tender_ai_execution_binding()?;
        let capability_readiness = self.workspace_capability_readiness()?;
        let doctor_blockers = workspace_doctor_blockers(&ai_execution, &capability_readiness);
        let external_rfis = self.workspace_external_rfi_summaries()?;
        let estimate = self.workspace_estimate_summary()?;
        let current_action = self.workspace_current_action(
            summary.lifecycle_phase,
            &work,
            intake.as_ref(),
            &external_rfis,
            estimate.as_ref(),
        )?;
        let safe_terminal_boundary = self.retention_boundary_is_safe()?;
        let tender = ManagerWorkspaceTender {
            tender_id: summary.tender_id,
            name: summary.name,
            revision: summary.revision,
            phase: summary.lifecycle_phase,
            needs_engineer: current_action.requires_engineer || work.needs_engineer > 0,
            state: if self.archived {
                ManagerWorkspaceTenderState::Archived
            } else {
                ManagerWorkspaceTenderState::Active
            },
            can_archive: !self.archived && safe_terminal_boundary,
            can_delete: safe_terminal_boundary,
            last_activity_at: Some(last_activity_at),
        };
        Ok(WorkspaceSnapshot {
            tender,
            conversation,
            current_action,
            work,
            files,
            team,
            external_rfis,
            estimate,
            intake,
            ai_execution,
            capability_readiness,
            doctor_blockers,
        })
    }

    fn record_engineer_message(
        &mut self,
        tender_id: &TenderId,
        command: &RecordEngineerWorkspaceMessageCommand,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let body = command.body.trim();
        if body.is_empty() || body.len() > MAX_MESSAGE_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if command.attachment_refs.len() + command.context_refs.len() > 24 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let conversation_id = transaction
            .query_row(
                "SELECT conversation_id FROM manager_workspace_state WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        let tender_revision = transaction
            .query_row(
                "SELECT current_revision FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get::<_, u32>(0),
            )
            .map_err(sql_error)?;
        let message_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO tender_office_messages (
                   message_id, conversation_id, author, kind, body, created_at
                 ) VALUES (?1, ?2, 'engineer', 'routine', ?3, ?4)",
                params![message_id, conversation_id, body, created_at],
            )
            .map_err(sql_error)?;
        for (index, reference) in command
            .attachment_refs
            .iter()
            .chain(command.context_refs.iter())
            .enumerate()
        {
            if !workspace_reference_exists(&transaction, reference)? {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            transaction
                .execute(
                    "INSERT INTO tender_office_message_references (
                       message_id, ordinal, kind, reference, version,
                       evidence_ordinal, label, detail
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        message_id,
                        i64::try_from(index + 1).map_err(|_| TenderCommandError::new(
                            TenderErrorCode::InvalidCommand
                        ))?,
                        reference.kind.as_str(),
                        reference.reference,
                        reference.version,
                        reference.evidence_ordinal,
                        reference.label,
                        reference.detail,
                    ],
                )
                .map_err(sql_error)?;
        }
        let pending_question: Option<(String, String)> = transaction
            .query_row(
                "SELECT outcomes.outcome_id, outcomes.question
                 FROM manager_intake_runs AS intake
                 JOIN manager_intake_outcomes AS outcomes
                   ON outcomes.intake_run_id = intake.intake_run_id
                 WHERE intake.intake_run_id = (
                   SELECT intake_run_id FROM manager_intake_runs
                   ORDER BY intake_run_sequence DESC LIMIT 1
                 )
                   AND intake.stage = 'waiting_for_engineer'
                   AND outcomes.kind = 'question'
                 ORDER BY outcomes.outcome_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some((outcome_id, question)) = pending_question {
            let authority = insert_engineer_entry(
                &transaction,
                tender_revision,
                body,
                &format!("Engineer answer to Tendering Manager intake question {outcome_id}."),
                &created_at,
            )?;
            let answer_id = random_identifier(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO manager_intake_answers (
                       answer_id, outcome_id, message_id, authority_id, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        answer_id,
                        outcome_id,
                        message_id,
                        authority.authority_id,
                        created_at
                    ],
                )
                .map_err(sql_error)?;
            transaction
                .execute(
                    "INSERT INTO tender_office_message_references (
                       message_id, ordinal, kind, reference, version,
                       evidence_ordinal, label, detail
                     ) VALUES (?1, ?2, 'manager_intake_outcome', ?3, 1, NULL, ?4, ?5)",
                    params![
                        message_id,
                        i64::try_from(
                            command.attachment_refs.len() + command.context_refs.len() + 1
                        )
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
                        outcome_id,
                        "Answered Manager question",
                        question,
                    ],
                )
                .map_err(sql_error)?;
            let updated = transaction
                .execute(
                    "UPDATE manager_intake_runs
                     SET stage = 'extracting_tender_facts', current_manager_run_id = NULL,
                         failure_summary = NULL, completed_at = NULL, updated_at = ?1
                     WHERE intake_run_id = (
                       SELECT intake_run_id FROM manager_intake_runs
                       ORDER BY intake_run_sequence DESC LIMIT 1
                     ) AND stage = 'waiting_for_engineer'",
                    params![created_at],
                )
                .map_err(sql_error)?;
            if updated != 1 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "manager_intake_answer_recorded",
                tender_revision,
                json!({
                    "answer_id": answer_id,
                    "authority_id": authority.authority_id,
                    "message_id": message_id,
                    "outcome_id": outcome_id,
                }),
                &created_at,
            )?;
        }
        transaction
            .execute(
                "UPDATE manager_workspace_state SET last_activity_at = ?1 WHERE singleton = 1",
                [&created_at],
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "tender_office_message_recorded",
            tender_revision,
            json!({
                "conversation_id": &conversation_id,
                "message_id": &message_id,
                "author": "engineer",
                "body_sha256": sha256_hex(body.as_bytes()),
                "attachment_reference_count": command.attachment_refs.len(),
                "context_reference_count": command.context_refs.len(),
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(())
    }

    fn manager_conversation(&self) -> Result<ManagerConversation, TenderCommandError> {
        let conversation_id = self
            .connection
            .query_row(
                "SELECT conversation_id FROM manager_workspace_state WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .map_err(sql_error)?;
        let latest_meaningful_message_id = self
            .connection
            .query_row(
                "SELECT COALESCE(
                   (SELECT message_id FROM tender_office_messages
                    WHERE conversation_id = ?1 AND kind != 'routine'
                    ORDER BY message_sequence DESC LIMIT 1),
                   (SELECT message_id FROM tender_office_messages
                    WHERE conversation_id = ?1
                    ORDER BY message_sequence DESC LIMIT 1)
                 )",
                [&conversation_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(sql_error)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT message_id, message_sequence, author, kind, body, created_at
                 FROM (
                   SELECT message_id, message_sequence, author, kind, body, created_at
                   FROM tender_office_messages
                   WHERE conversation_id = ?1
                   ORDER BY message_sequence DESC
                   LIMIT ?2
                 )
                 ORDER BY message_sequence",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![conversation_id, MAX_CONVERSATION_MESSAGES], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(sql_error)?;
        let mut messages = Vec::new();
        for row in rows {
            let (message_id, sequence, author, kind, body, created_at) = row.map_err(sql_error)?;
            messages
                .push(self.message_from_row(message_id, sequence, author, kind, body, created_at)?);
        }
        if let Some(meaningful_id) = latest_meaningful_message_id.as_deref() {
            if !messages
                .iter()
                .any(|message| message.message_id == meaningful_id)
            {
                let raw = self
                    .connection
                    .query_row(
                        "SELECT message_id, message_sequence, author, kind, body, created_at
                         FROM tender_office_messages
                         WHERE conversation_id = ?1 AND message_id = ?2",
                        params![conversation_id, meaningful_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, String>(3)?,
                                row.get::<_, String>(4)?,
                                row.get::<_, String>(5)?,
                            ))
                        },
                    )
                    .map_err(sql_error)?;
                messages.push(self.message_from_row(raw.0, raw.1, raw.2, raw.3, raw.4, raw.5)?);
                messages.sort_by_key(|message| message.sequence);
            }
        }
        Ok(ManagerConversation {
            conversation_id,
            messages,
            latest_meaningful_message_id,
        })
    }

    fn workspace_work_summary(&self) -> Result<WorkspaceWorkSummary, TenderCommandError> {
        let counts: (i64, i64, i64, i64, i64, i64) = self
            .connection
            .query_row(
                "SELECT
                   COALESCE(SUM(status IN (
                     'review_ready', 'remediation_ready', 'query_blocked',
                     'attempt_limit_reached', 'indeterminate'
                   )), 0),
                   COALESCE(SUM(status IN ('running', 'reviewing')), 0),
                   COALESCE(SUM(status IN ('blocked', 'ready', 'suspended')), 0),
                   COALESCE(SUM(status = 'ready_for_integration'), 0),
                   COALESCE(SUM(status = 'cancelled'), 0),
                   COALESCE(SUM(status = 'failed'), 0)
                 FROM production_tasks",
                [],
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
        let tasks = self.workspace_task_rows()?;
        Ok(WorkspaceWorkSummary {
            needs_engineer: to_u32(counts.0)?,
            working: to_u32(counts.1)?,
            waiting: to_u32(counts.2)?,
            done: to_u32(counts.3)?,
            cancelled: to_u32(counts.4)?,
            failed: to_u32(counts.5)?,
            tasks,
        })
    }

    fn workspace_files_summary(&self) -> Result<WorkspaceFilesSummary, TenderCommandError> {
        let counts: (i64, i64) = self
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM source_artifact_versions),
                   (SELECT COUNT(*) FROM production_artifact_versions)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        Ok(WorkspaceFilesSummary {
            tender_document_count: to_u32(counts.0)?,
            quantix_output_count: to_u32(counts.1)?,
            tender_documents: self.workspace_tender_documents()?,
            quantix_outputs: self.workspace_output_references()?,
        })
    }

    fn workspace_team_summary(
        &self,
        work: &WorkspaceWorkSummary,
        conversation: &ManagerConversation,
    ) -> Result<WorkspaceTeamSummary, TenderCommandError> {
        let active_agent_runs: i64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE status = 'running'",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        Ok(WorkspaceTeamSummary {
            active_agent_runs: to_u32(active_agent_runs)?,
            waiting_tasks: work.waiting,
            needs_engineer: work.needs_engineer.saturating_add(work.failed),
            events: conversation.messages.clone(),
            agent_runs: self.workspace_agent_run_references()?,
        })
    }

    fn workspace_capability_readiness(
        &self,
    ) -> Result<WorkspaceCapabilityReadiness, TenderCommandError> {
        let plan: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT versions.capability_gaps_json, versions.blocker_codes_json
                 FROM work_plan_heads AS heads
                 JOIN work_plan_versions AS versions
                   ON versions.plan_id = heads.plan_id
                  AND versions.version = heads.current_version
                 ORDER BY heads.plan_id
                 LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((gaps_json, blocker_codes_json)) = plan else {
            return Ok(WorkspaceCapabilityReadiness {
                state: WorkspaceCapabilityReadinessState::NotPlanned,
                gaps: Vec::new(),
                blocker_codes: Vec::new(),
            });
        };
        let mut gaps: Vec<WorkPlanCapabilityGap> = serde_json::from_str(&gaps_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let mut blocker_codes: Vec<String> = serde_json::from_str(&blocker_codes_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        for gap in &mut gaps {
            gap.affected_work.sort();
        }
        gaps.sort_by(|left, right| {
            left.capability
                .cmp(&right.capability)
                .then_with(|| left.reason.cmp(&right.reason))
                .then_with(|| left.affected_work.cmp(&right.affected_work))
        });
        blocker_codes.sort();
        blocker_codes.dedup();
        let state = if gaps.is_empty() && blocker_codes.is_empty() {
            WorkspaceCapabilityReadinessState::Ready
        } else {
            WorkspaceCapabilityReadinessState::Blocked
        };
        Ok(WorkspaceCapabilityReadiness {
            state,
            gaps,
            blocker_codes,
        })
    }

    fn workspace_task_rows(&self) -> Result<Vec<WorkspaceTaskRow>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT tasks.production_task_id, tasks.task_id, tasks.task_key,
                        tasks.task_definition_json, tasks.status,
                        tender_tasks.objective, tender_tasks.profile_id,
                        tender_tasks.profile_version, profiles.identity,
                        profiles.profession,
                        (SELECT runs.run_id FROM agent_runs AS runs
                         WHERE runs.task_id = tasks.task_id
                         ORDER BY runs.run_sequence DESC LIMIT 1),
                        (SELECT COUNT(*) FROM production_artifact_versions AS outputs
                         WHERE outputs.production_task_id = tasks.production_task_id)
                 FROM production_tasks AS tasks
                 LEFT JOIN tender_tasks
                   ON tender_tasks.task_id = tasks.task_id
                 LEFT JOIN agent_profile_versions AS profiles
                   ON profiles.profile_id = tender_tasks.profile_id
                  AND profiles.version = tender_tasks.profile_version
                 ORDER BY tasks.updated_at DESC, tasks.task_key",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<u32>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            })
            .map_err(sql_error)?;
        rows.map(|row| {
            let (
                production_task_id,
                task_id,
                task_key,
                definition_json,
                status,
                objective,
                profile_id,
                profile_version,
                identity,
                profession,
                current_run_id,
                output_count,
            ) = row.map_err(sql_error)?;
            let definition: serde_json::Value = serde_json::from_str(&definition_json)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let dependencies = definition
                .get("dependencies")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let agent = profile_id
                .zip(profile_version)
                .zip(identity)
                .zip(profession)
                .map(|(((profile_id, profile_version), identity), profession)| {
                    WorkspaceAgentReference {
                        profile_id,
                        profile_version,
                        identity,
                        profession,
                    }
                });
            Ok(WorkspaceTaskRow {
                production_task_id,
                task_id,
                task_key,
                objective,
                state: workspace_task_state(&status),
                status_detail: status,
                dependencies,
                agent,
                current_run_id,
                output_count: to_u32(output_count)?,
            })
        })
        .collect()
    }

    fn workspace_agent_run_references(
        &self,
    ) -> Result<Vec<WorkspaceAgentRunReference>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT runs.run_id, runs.task_id, runs.status,
                        profiles.profile_id, profiles.version,
                        profiles.identity, profiles.profession,
                        runs.started_at, runs.completed_at
                 FROM agent_runs AS runs
                 JOIN agent_profile_versions AS profiles
                   ON profiles.profile_id = runs.profile_id
                  AND profiles.version = runs.profile_version
                 ORDER BY runs.run_sequence DESC",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(WorkspaceAgentRunReference {
                    run_id: row.get(0)?,
                    task_id: row.get(1)?,
                    state: row.get(2)?,
                    agent: WorkspaceAgentReference {
                        profile_id: row.get(3)?,
                        profile_version: row.get(4)?,
                        identity: row.get(5)?,
                        profession: row.get(6)?,
                    },
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                })
            })
            .map_err(sql_error)?;
        rows.map(|row| row.map_err(sql_error)).collect()
    }

    fn workspace_output_references(
        &self,
    ) -> Result<Vec<WorkspaceOutputReference>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT artifact_id, version, production_task_id,
                        author_run_id, payload_sha256, created_at
                 FROM production_artifact_versions
                 ORDER BY created_at DESC, artifact_id, version",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok(WorkspaceOutputReference {
                    artifact_id: row.get(0)?,
                    version: row.get(1)?,
                    production_task_id: row.get(2)?,
                    author_run_id: row.get(3)?,
                    payload_sha256: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(sql_error)?;
        rows.map(|row| row.map_err(sql_error)).collect()
    }

    pub(crate) fn search_workspace(
        &self,
        query: &str,
    ) -> Result<WorkspaceSearchProjection, TenderCommandError> {
        let query = query.trim();
        if query.is_empty() || query.len() > 200 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");

        let conversation = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT message_id, body, author
                     FROM tender_office_messages
                     WHERE body LIKE ?1 ESCAPE '\\'
                     ORDER BY message_sequence DESC LIMIT 32",
                )
                .map_err(sql_error)?;
            let hits = statement
                .query_map([pattern.as_str()], |row| {
                    Ok(WorkspaceSearchHit {
                        kind: WorkspaceSearchResultKind::Conversation,
                        reference: row.get(0)?,
                        version: None,
                        title: format!("{} message", row.get::<_, String>(2)?),
                        detail: row.get(1)?,
                    })
                })
                .map_err(sql_error)?
                .map(|row| row.map_err(sql_error))
                .collect::<Result<Vec<_>, _>>()?;
            hits
        };

        let work = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT production_task_id, task_key, status,
                            COALESCE(tender_tasks.objective, '')
                     FROM production_tasks
                     LEFT JOIN tender_tasks ON tender_tasks.task_id = production_tasks.task_id
                     WHERE task_key LIKE ?1 ESCAPE '\\'
                        OR task_definition_json LIKE ?1 ESCAPE '\\'
                        OR COALESCE(tender_tasks.objective, '') LIKE ?1 ESCAPE '\\'
                     ORDER BY updated_at DESC LIMIT 32",
                )
                .map_err(sql_error)?;
            let hits = statement
                .query_map([pattern.as_str()], |row| {
                    let task_key: String = row.get(1)?;
                    let status: String = row.get(2)?;
                    let objective: String = row.get(3)?;
                    Ok(WorkspaceSearchHit {
                        kind: WorkspaceSearchResultKind::Work,
                        reference: row.get(0)?,
                        version: None,
                        title: task_key,
                        detail: if objective.is_empty() {
                            status
                        } else {
                            format!("{status}: {objective}")
                        },
                    })
                })
                .map_err(sql_error)?
                .map(|row| row.map_err(sql_error))
                .collect::<Result<Vec<_>, _>>()?;
            hits
        };

        let files = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT artifacts.artifact_id, versions.version,
                            artifacts.package_path, versions.document_type,
                            COALESCE(versions.sha256, '')
                     FROM source_artifacts AS artifacts
                     JOIN source_artifact_versions AS versions
                       ON versions.artifact_id = artifacts.artifact_id
                     WHERE artifacts.package_path LIKE ?1 ESCAPE '\\'
                        OR versions.document_type LIKE ?1 ESCAPE '\\'
                        OR COALESCE(versions.sha256, '') LIKE ?1 ESCAPE '\\'
                     ORDER BY versions.created_at DESC LIMIT 32",
                )
                .map_err(sql_error)?;
            let hits = statement
                .query_map([pattern.as_str()], |row| {
                    Ok(WorkspaceSearchHit {
                        kind: WorkspaceSearchResultKind::Files,
                        reference: row.get(0)?,
                        version: Some(row.get(1)?),
                        title: row.get(2)?,
                        detail: row.get(3)?,
                    })
                })
                .map_err(sql_error)?
                .map(|row| row.map_err(sql_error))
                .collect::<Result<Vec<_>, _>>()?;
            hits
        };

        let evidence = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT artifact_id, version, ordinal, original_text
                     FROM evidence_locations
                     WHERE original_text LIKE ?1 ESCAPE '\\'
                        OR COALESCE(translated_text, '') LIKE ?1 ESCAPE '\\'
                        OR COALESCE(section, '') LIKE ?1 ESCAPE '\\'
                     ORDER BY artifact_id, version, ordinal LIMIT 32",
                )
                .map_err(sql_error)?;
            let hits = statement
                .query_map([pattern.as_str()], |row| {
                    let artifact_id: String = row.get(0)?;
                    let version: u32 = row.get(1)?;
                    let ordinal: u32 = row.get(2)?;
                    Ok(WorkspaceSearchHit {
                        kind: WorkspaceSearchResultKind::Evidence,
                        reference: format!("{artifact_id}:{ordinal}"),
                        version: Some(version),
                        title: format!("Evidence location {ordinal}"),
                        detail: row.get(3)?,
                    })
                })
                .map_err(sql_error)?
                .map(|row| row.map_err(sql_error))
                .collect::<Result<Vec<_>, _>>()?;
            hits
        };

        let agents = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT profile_id, version, identity, profession
                     FROM agent_profile_versions
                     WHERE identity LIKE ?1 ESCAPE '\\'
                        OR profession LIKE ?1 ESCAPE '\\'
                        OR instructions LIKE ?1 ESCAPE '\\'
                     ORDER BY identity, version DESC LIMIT 32",
                )
                .map_err(sql_error)?;
            let hits = statement
                .query_map([pattern.as_str()], |row| {
                    let profile_id: String = row.get(0)?;
                    let version: u32 = row.get(1)?;
                    let identity: String = row.get(2)?;
                    let profession: String = row.get(3)?;
                    Ok(WorkspaceSearchHit {
                        kind: WorkspaceSearchResultKind::Agents,
                        reference: profile_id,
                        version: Some(version),
                        title: identity,
                        detail: profession,
                    })
                })
                .map_err(sql_error)?
                .map(|row| row.map_err(sql_error))
                .collect::<Result<Vec<_>, _>>()?;
            hits
        };

        Ok(WorkspaceSearchProjection {
            query: query.to_owned(),
            groups: vec![
                WorkspaceSearchGroup {
                    kind: WorkspaceSearchResultKind::Conversation,
                    hits: conversation,
                },
                WorkspaceSearchGroup {
                    kind: WorkspaceSearchResultKind::Work,
                    hits: work,
                },
                WorkspaceSearchGroup {
                    kind: WorkspaceSearchResultKind::Files,
                    hits: files,
                },
                WorkspaceSearchGroup {
                    kind: WorkspaceSearchResultKind::Evidence,
                    hits: evidence,
                },
                WorkspaceSearchGroup {
                    kind: WorkspaceSearchResultKind::Agents,
                    hits: agents,
                },
            ],
        })
    }

    fn workspace_current_action(
        &self,
        phase: TenderLifecyclePhase,
        work: &WorkspaceWorkSummary,
        intake: Option<&ManagerIntakeStatus>,
        external_rfis: &[WorkspaceExternalRfiSummary],
        estimate: Option<&WorkspaceEstimateSummary>,
    ) -> Result<WorkspaceCurrentAction, TenderCommandError> {
        if let Some(action) = self.workspace_external_rfi_action(external_rfis)? {
            return Ok(action);
        }
        if let Some(action) = self.workspace_estimate_action(estimate) {
            return Ok(action);
        }
        let action = match phase {
            TenderLifecyclePhase::Intake => {
                if let Some(intake) = intake {
                    return Ok(match intake.stage {
                        ManagerIntakeStage::WaitingForLocalTools => action(
                            WorkspaceActionKind::ObserveIntake,
                            "Prepare document tools",
                            &intake.summary,
                            "Prepare document tools",
                            true,
                        ),
                        ManagerIntakeStage::WaitingForProviderApproval => action(
                            WorkspaceActionKind::ConfigureAiProvider,
                            "Choose an AI provider",
                            &intake.summary,
                            "Use selected AI",
                            true,
                        ),
                        ManagerIntakeStage::WaitingForProvider => action(
                            WorkspaceActionKind::ConfigureAiProvider,
                            "Waiting for AI Provider",
                            "The Tender Package is safe. Quantix will continue only with the exact provider, model, and reasoning choice approved for this Tender.",
                            "Use selected AI",
                            true,
                        ),
                        ManagerIntakeStage::PackageRegistered
                        | ManagerIntakeStage::ReadingDocuments
                        | ManagerIntakeStage::ExtractingTenderFacts
                        | ManagerIntakeStage::ReviewingTenderFacts
                        | ManagerIntakeStage::PreparingFirstDecision => action(
                            WorkspaceActionKind::ObserveIntake,
                            &intake.label,
                            &intake.summary,
                            "Intake in progress",
                            false,
                        ),
                        ManagerIntakeStage::WaitingForEngineer => action(
                            WorkspaceActionKind::AnswerManagerQuestion,
                            "Answer the Manager's question",
                            "Your answer will become an attributable Engineer input for the Tender intake.",
                            "Reply to Manager",
                            true,
                        ),
                        ManagerIntakeStage::BidDecisionReady => action(
                            WorkspaceActionKind::ReviewBidDecision,
                            "Review the bid recommendation",
                            "The Tendering Manager has presented an evidence-linked recommendation.",
                            "Review recommendation",
                            true,
                        ),
                        ManagerIntakeStage::Failed => action(
                            WorkspaceActionKind::RetryIntake,
                            "Tender intake needs attention",
                            if !self.unresolved_manager_intake_run_ids()?.is_empty() {
                                "The prior Manager turn ended uncertain. Retrying will close that untrusted outcome and start a fresh exact turn."
                            } else {
                                &intake.summary
                            },
                            if !self.unresolved_manager_intake_run_ids()?.is_empty() {
                                "Close uncertain turn and retry"
                            } else {
                                "Retry intake"
                            },
                            true,
                        ),
                    });
                }
                let source_count: i64 = self
                    .connection
                    .query_row("SELECT COUNT(*) FROM source_artifact_versions", [], |row| {
                        row.get(0)
                    })
                    .map_err(sql_error)?;
                if source_count == 0 {
                    action(
                        WorkspaceActionKind::AddTenderPackage,
                        "Add the Tender Package",
                        "Give the Tender Manager the source documents to begin the review.",
                        "Choose Tender Package",
                        true,
                    )
                } else {
                    action(
                        WorkspaceActionKind::ReviewIntake,
                        "Tender Package registered",
                        "The source collection is preserved and ready for Manager-led intake.",
                        "View files",
                        false,
                    )
                }
            }
            TenderLifecyclePhase::BidDecision => action(
                WorkspaceActionKind::ReviewBidDecision,
                "Review the bid decision",
                "The Tender Manager needs your decision before planning can begin.",
                "Review recommendation",
                true,
            ),
            TenderLifecyclePhase::TenderPlanning => {
                let plan: Option<(String, i64)> = self
                    .connection
                    .query_row(
                        "SELECT plan_id, current_version FROM work_plan_heads LIMIT 1",
                        [],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                match plan {
                    None => action(
                        WorkspaceActionKind::PrepareWorkPlan,
                        "Prepare the work plan",
                        "The Tender Manager is ready to propose the team, tasks, and review path.",
                        "Prepare plan",
                        false,
                    ),
                    Some((plan_id, version)) => {
                        let decision: Option<String> = self
                            .connection
                            .query_row(
                                "SELECT decision FROM work_plan_approvals
                                 WHERE plan_id = ?1 AND plan_version = ?2",
                                params![plan_id, version],
                                |row| row.get(0),
                            )
                            .optional()
                            .map_err(sql_error)?;
                        if decision.as_deref() != Some("approve") {
                            action(
                                WorkspaceActionKind::ReviewWorkPlan,
                                "Review the Manager's plan",
                                "Approve, return, or reject the proposed team and work sequence.",
                                "Review plan",
                                true,
                            )
                        } else {
                            action(
                                WorkspaceActionKind::ReviewWork,
                                "Work plan approved",
                                "Your approval is recorded. The Tender Manager can now coordinate the approved work.",
                                "View work",
                                false,
                            )
                        }
                    }
                }
            }
            TenderLifecyclePhase::ActiveProduction => action(
                WorkspaceActionKind::ReviewWork,
                if work.needs_engineer > 0 || work.failed > 0 {
                    "Your review is needed"
                } else {
                    "Tender work is in progress"
                },
                if work.needs_engineer > 0 || work.failed > 0 {
                    "The team has work, a question, or a failure that needs your decision."
                } else {
                    "The Tender Manager is coordinating the approved plan."
                },
                if work.needs_engineer > 0 || work.failed > 0 {
                    "Review work"
                } else {
                    "View work"
                },
                work.needs_engineer > 0 || work.failed > 0,
            ),
            TenderLifecyclePhase::IntegratedReview => action(
                WorkspaceActionKind::ReviewIntegratedWork,
                "Review the coordinated Tender",
                "The team's work has been integrated and is ready for your review.",
                "Review Tender",
                true,
            ),
            TenderLifecyclePhase::ChangeAssessment => action(
                WorkspaceActionKind::ReviewChange,
                "Review the Tender change",
                "A source change needs an impact decision before work continues.",
                "Review change",
                true,
            ),
            TenderLifecyclePhase::PackageProduction => action(
                WorkspaceActionKind::ReviewSubmissionPackage,
                "Review the submission package",
                "The Tender Manager is assembling the controlled submission package.",
                "Review package",
                true,
            ),
            TenderLifecyclePhase::FinalReview => action(
                WorkspaceActionKind::ReviewFinalPackage,
                "Approve the final package",
                "The verified Release Copy is ready for the final Engineer decision.",
                "Open final review",
                true,
            ),
            TenderLifecyclePhase::Declined => action(
                WorkspaceActionKind::TenderDeclined,
                "Tender declined",
                "This Tender is closed for bid production.",
                "View decision",
                false,
            ),
        };
        Ok(action)
    }

    fn workspace_external_rfi_summaries(
        &self,
    ) -> Result<Vec<WorkspaceExternalRfiSummary>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT heads.rfi_id,
                       heads.current_version,
                       json_array_length(versions.questions_json),
                       versions.query_refs_json,
                       (
                         SELECT COUNT(*) FROM external_rfi_responses AS responses
                         WHERE responses.rfi_id = heads.rfi_id
                           AND responses.rfi_version = heads.current_version
                       ),
                       (
                         SELECT COUNT(*) FROM external_rfi_responses AS responses
                         WHERE responses.rfi_id = heads.rfi_id
                           AND responses.rfi_version = heads.current_version
                           AND (
                             SELECT json_array_length(issued.query_refs_json)
                             FROM external_rfi_versions AS issued
                             WHERE issued.rfi_id = responses.rfi_id
                               AND issued.version = responses.rfi_version
                           ) >
                           (
                             SELECT COUNT(*)
                             FROM external_rfi_response_interpretations AS interpretations
                             WHERE interpretations.response_link_id = responses.response_link_id
                           )
                       ),
                       (
                         SELECT COUNT(*) FROM external_rfi_approvals AS approvals
                         WHERE approvals.rfi_id = heads.rfi_id
                           AND approvals.rfi_version = heads.current_version
                       ),
                       (
                         SELECT reviews.outcome FROM external_rfi_reviews AS reviews
                         WHERE reviews.rfi_id = heads.rfi_id
                           AND reviews.rfi_version = heads.current_version
                       ),
                       (
                         SELECT COUNT(*)
                         FROM external_rfi_approvals AS approvals
                         JOIN external_rfi_exports AS exports
                           ON exports.approval_id = approvals.approval_id
                         WHERE approvals.rfi_id = heads.rfi_id
                           AND approvals.rfi_version = heads.current_version
                       )
                FROM external_rfi_heads AS heads
                JOIN external_rfi_versions AS versions
                  ON versions.rfi_id = heads.rfi_id
                 AND versions.version = heads.current_version
                ORDER BY versions.created_at DESC, heads.rfi_id",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })
            .map_err(sql_error)?;
        let mut summaries = Vec::new();
        for row in rows {
            let (
                rfi_id,
                version,
                question_count,
                query_refs_json,
                response_count,
                uninterpreted_response_count,
                approval_count,
                review_outcome,
                export_count,
            ) = row.map_err(sql_error)?;
            let query_refs: Vec<super::external_rfis::ExternalRfiQueryReference> =
                serde_json::from_str(&query_refs_json)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let evidence_current = super::external_rfis::external_rfi_query_refs_are_current(
                &self.connection,
                &query_refs,
            )?;
            let status = if !evidence_current {
                WorkspaceExternalRfiStatus::QueryBasisStale
            } else if uninterpreted_response_count > 0 {
                WorkspaceExternalRfiStatus::ResponseAwaitingInterpretation
            } else if approval_count > 0 {
                WorkspaceExternalRfiStatus::ApprovedForIssue
            } else if review_outcome.as_deref() == Some("passed") {
                WorkspaceExternalRfiStatus::AwaitingApproval
            } else if review_outcome.as_deref() == Some("failed") {
                WorkspaceExternalRfiStatus::ReviewFailed
            } else {
                WorkspaceExternalRfiStatus::AwaitingReview
            };
            let approved_for_issue = evidence_current
                && approval_count > 0
                && review_outcome.as_deref() == Some("passed");
            summaries.push(WorkspaceExternalRfiSummary {
                rfi_id,
                version,
                status,
                question_count: to_u32(question_count)?,
                response_count: to_u32(response_count)?,
                approval_pending: !approved_for_issue
                    && evidence_current
                    && approval_count == 0
                    && review_outcome.as_deref() == Some("passed"),
                export_pending: approved_for_issue && export_count == 0,
                interpretation_pending: uninterpreted_response_count > 0,
            });
        }
        Ok(summaries)
    }

    fn workspace_external_rfi_action(
        &self,
        summaries: &[WorkspaceExternalRfiSummary],
    ) -> Result<Option<WorkspaceCurrentAction>, TenderCommandError> {
        if let Some(pending) = summaries.iter().find(|summary| {
            matches!(
                summary.status,
                WorkspaceExternalRfiStatus::AwaitingReview
                    | WorkspaceExternalRfiStatus::ReviewFailed
                    | WorkspaceExternalRfiStatus::AwaitingApproval
            )
        }) {
            let (title, summary_text) = match pending.status {
                WorkspaceExternalRfiStatus::AwaitingReview => (
                    "Review the External RFI draft",
                    "A controlled question to the client is drafted from exact Tender questions and evidence. It needs independent review before you approve it for issue.",
                ),
                WorkspaceExternalRfiStatus::ReviewFailed => (
                    "Resolve the External RFI review findings",
                    "The independent review found problems in the draft. Revise the draft to resolve them, then run a new review.",
                ),
                WorkspaceExternalRfiStatus::AwaitingApproval => (
                    "Approve the External RFI for issue",
                    "The independent review passed. Your approval records the exact wording before it can be exported for human issue.",
                ),
                _ => unreachable!("matched only the three pending review statuses"),
            };
            return Ok(Some(action(
                WorkspaceActionKind::ReviewExternalRfi,
                title,
                summary_text,
                "Review External RFI",
                true,
            )));
        }
        if summaries
            .iter()
            .any(|summary| summary.interpretation_pending)
        {
            return Ok(Some(action(
                WorkspaceActionKind::InterpretExternalRfiResponse,
                "Interpret the received response",
                "A response to your External RFI arrived. Record one interpretation as the Manager so the answer enters the Tender record.",
                "Interpret response",
                true,
            )));
        }
        let eligible_queries: i64 = self
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
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if eligible_queries > 0 {
            return Ok(Some(action(
                WorkspaceActionKind::DraftExternalRfi,
                "Ask the client a controlled question",
                "A Tender question is routed for a controlled External RFI. Gather the exact questions, address them to the recipient, and Quantix prepares the draft for review.",
                "Start External RFI",
                true,
            )));
        }
        Ok(None)
    }

    fn workspace_estimate_summary(
        &self,
    ) -> Result<Option<WorkspaceEstimateSummary>, TenderCommandError> {
        let basis: Option<(String, u32, i64, bool, bool)> = self
            .connection
            .query_row(
                "SELECT basis_id, version,
                        json_array_length(manifest_json, '$.boq_rows'),
                        json_extract(manifest_json, '$.complete') = 1,
                        json_extract(manifest_json, '$.reconciled') = 1
                 FROM basis_of_estimate_versions
                 ORDER BY version DESC LIMIT 1",
                [],
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
        let Some((basis_id, version, boq_row_count, complete, reconciled)) = basis else {
            return Ok(None);
        };
        let review: Option<(String, i64)> = self
            .connection
            .query_row(
                "SELECT outcome, json_array_length(findings_json)
                 FROM basis_of_estimate_reviews
                 WHERE basis_id = ?1 AND basis_version = ?2",
                params![basis_id, version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let approved: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM basis_of_estimate_approvals
                   WHERE basis_id = ?1 AND basis_version = ?2
                 )",
                params![basis_id, version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let calculation_run_count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM calculation_runs", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        let status = if approved {
            WorkspaceEstimateStatus::Approved
        } else if review
            .as_ref()
            .is_some_and(|(outcome, _)| outcome == "passed")
        {
            WorkspaceEstimateStatus::AwaitingApproval
        } else if review
            .as_ref()
            .is_some_and(|(outcome, _)| outcome == "failed")
        {
            WorkspaceEstimateStatus::ReviewFailed
        } else if complete && reconciled {
            WorkspaceEstimateStatus::AwaitingReview
        } else {
            WorkspaceEstimateStatus::Incomplete
        };
        Ok(Some(WorkspaceEstimateSummary {
            basis_id,
            version,
            status,
            boq_row_count: to_u32(boq_row_count)?,
            finding_count: to_u32(review.map(|(_, findings)| findings).unwrap_or(0))?,
            calculation_run_count: to_u32(calculation_run_count)?,
        }))
    }

    fn workspace_estimate_action(
        &self,
        estimate: Option<&WorkspaceEstimateSummary>,
    ) -> Option<WorkspaceCurrentAction> {
        let estimate = estimate?;
        let (title, summary_text) = match estimate.status {
            WorkspaceEstimateStatus::AwaitingReview => (
                "Review the Basis of Estimate",
                "The Cost Estimator published a complete, reconciled Basis of Estimate. It needs independent review before you approve it for reliance.",
            ),
            WorkspaceEstimateStatus::ReviewFailed => (
                "Resolve the estimate review findings",
                "The independent review found problems in the Basis of Estimate. A revised basis must remediate them before a new review can run.",
            ),
            WorkspaceEstimateStatus::AwaitingApproval => (
                "Approve the Basis of Estimate",
                "The independent review passed. Your approval binds the exact basis version before any pricing may rely on it.",
            ),
            WorkspaceEstimateStatus::Approved | WorkspaceEstimateStatus::Incomplete => return None,
        };
        Some(action(
            WorkspaceActionKind::ReviewBasisOfEstimate,
            title,
            summary_text,
            "Review estimate basis",
            true,
        ))
    }
}

fn workspace_reference_exists(
    transaction: &Transaction<'_>,
    reference: &WorkspaceMessageReference,
) -> Result<bool, TenderCommandError> {
    if reference.reference.trim().is_empty()
        || reference.label.trim().is_empty()
        || reference.reference.len() > 500
        || reference.label.len() > 500
        || reference
            .detail
            .as_ref()
            .is_some_and(|detail| detail.len() > 2_000)
        || reference.version == 0
    {
        return Ok(false);
    }
    let exists = match reference.kind {
        super::WorkspaceMessageReferenceKind::AgentRun => transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE run_id = ?1)",
            [&reference.reference],
            |row| row.get(0),
        ),
        super::WorkspaceMessageReferenceKind::ManagerIntakeOutcome => transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM manager_intake_outcomes WHERE outcome_id = ?1)",
            [&reference.reference],
            |row| row.get(0),
        ),
        super::WorkspaceMessageReferenceKind::TenderRecord => transaction.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM tender_record_versions WHERE record_id = ?1 AND version = ?2
             )",
            params![reference.reference, reference.version],
            |row| row.get(0),
        ),
        super::WorkspaceMessageReferenceKind::SourceEvidence => {
            let Some(ordinal) = reference.evidence_ordinal else {
                return Ok(false);
            };
            transaction.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM evidence_locations
                   WHERE artifact_id = ?1 AND version = ?2 AND ordinal = ?3
                 )",
                params![reference.reference, reference.version, ordinal],
                |row| row.get(0),
            )
        }
        super::WorkspaceMessageReferenceKind::ArtifactVersion => transaction.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM source_artifact_versions
               WHERE artifact_id = ?1 AND version = ?2
             )
             OR EXISTS(
               SELECT 1 FROM production_artifact_versions
               WHERE artifact_id = ?1 AND version = ?2
             )",
            params![reference.reference, reference.version],
            |row| row.get(0),
        ),
        super::WorkspaceMessageReferenceKind::TenderTask => transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tender_tasks WHERE task_id = ?1)",
            [&reference.reference],
            |row| row.get(0),
        ),
    }
    .map_err(sql_error)?;
    Ok(exists)
}

impl QuantixHost {
    pub(crate) fn start_manager_tender_from_package_with_control(
        &self,
        source: &Path,
        control: Option<&PackageIntakeControl>,
        local_only: bool,
    ) -> Result<Option<ManagerWorkspaceProjection>, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        if !source.is_absolute() || fs::symlink_metadata(source).is_err() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if control.is_some_and(PackageIntakeControl::is_cancelled) {
            return Ok(None);
        }
        let _start_guard = self
            .manager_tender_start_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        self.reconcile_manager_tender_selection_publication()?;
        let name = tender_name_from_source(source)?;
        let tender_id = self.generate_tender_id()?;
        let stage_root = self
            .application_home()
            .join("staging")
            .join(format!("tender-{}", tender_id.as_str()));
        let final_root = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let mut store = match TenderStore::create(&stage_root, &tender_id, &name) {
            Ok(store) => store,
            Err(error) => {
                let _ = fs::remove_dir_all(&stage_root);
                return Err(error);
            }
        };
        if !local_only {
            if let Err(error) =
                store.seed_application_ai_execution_binding(self.application_home(), &tender_id)
            {
                drop(store);
                let _ = fs::remove_dir_all(&stage_root);
                return Err(error);
            }
        }
        let _imported = match store.import_package_with_control(source, control) {
            Ok(Some(imported)) => imported,
            Ok(None) => {
                drop(store);
                let _ = fs::remove_dir_all(&stage_root);
                return Ok(None);
            }
            Err(error) => {
                drop(store);
                let _ = fs::remove_dir_all(&stage_root);
                return Err(error);
            }
        };
        if let Some(control) = control {
            control.opening_workspace();
        }
        let projection = match (|| {
            let summary = store.summary()?;
            let snapshot = store.workspace_snapshot()?;
            let mut catalogue = self.manager_workspace_catalogue()?;
            catalogue.push(snapshot.tender.clone());
            sort_workspace_catalogue(&mut catalogue, Some(&tender_id));
            let projection = projection_from_snapshot(catalogue, snapshot);
            self.upsert_catalogue_summary(&summary)?;
            self.begin_manager_tender_selection_publication(&tender_id)?;
            Ok::<_, TenderCommandError>(projection)
        })() {
            Ok(projection) => projection,
            Err(error) => {
                drop(store);
                let _ = fs::remove_dir_all(&stage_root);
                let _ = self.cancel_manager_tender_selection_publication(&tender_id);
                let _ = self.remove_catalogue_entry(&tender_id);
                return Err(error);
            }
        };
        drop(store);
        storage_publication_failpoint("tender_after_stage");
        if let Err(error) = fs::rename(&stage_root, &final_root) {
            let _ = fs::remove_dir_all(&stage_root);
            let _ = self.cancel_manager_tender_selection_publication(&tender_id);
            let _ = self.remove_catalogue_entry(&tender_id);
            return Err(store_unavailable(error));
        }
        let _ = self.finish_manager_tender_selection_publication(&tender_id);
        storage_publication_failpoint("tender_after_publish");
        Ok(Some(projection))
    }

    pub fn inspect_manager_workspace(
        &self,
        command: InspectManagerWorkspaceCommand,
    ) -> Result<ManagerWorkspaceProjection, TenderCommandError> {
        require_setup(self)?;
        let requested_tender = command
            .tender_id
            .as_deref()
            .map(TenderId::parse)
            .transpose()?;
        let mut catalogue = self.manager_workspace_catalogue()?;
        sort_workspace_catalogue(&mut catalogue, None);

        let persisted_tenders = if requested_tender.is_none() {
            self.persisted_manager_workspace_selection()?
        } else {
            (None, None)
        };
        let selected_id = match requested_tender {
            Some(requested) => {
                let selected = catalogue
                    .iter()
                    .find(|tender| tender.tender_id == requested.as_str())
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
                if selected.state == ManagerWorkspaceTenderState::RecoveryRequired {
                    return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
                }
                Some(requested)
            }
            None => {
                let persisted = persisted_tenders
                    .0
                    .filter(|persisted| {
                        catalogue.iter().any(|tender| {
                            tender.state == ManagerWorkspaceTenderState::Active
                                && tender.tender_id == persisted.as_str()
                        })
                    })
                    .or_else(|| {
                        persisted_tenders.1.filter(|persisted| {
                            catalogue.iter().any(|tender| {
                                tender.state == ManagerWorkspaceTenderState::Active
                                    && tender.tender_id == persisted.as_str()
                            })
                        })
                    });
                if let Some(persisted) = persisted {
                    Some(persisted)
                } else {
                    catalogue
                        .iter()
                        .find(|tender| tender.state == ManagerWorkspaceTenderState::Active)
                        .map(|tender| TenderId::parse(&tender.tender_id))
                        .transpose()?
                }
            }
        };

        let Some(selected_id) = selected_id else {
            sort_workspace_catalogue(&mut catalogue, None);
            return Ok(empty_projection(catalogue));
        };
        sort_workspace_catalogue(&mut catalogue, Some(&selected_id));
        let store = self.tender_store(&selected_id)?;
        let snapshot = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .workspace_snapshot()?;
        Ok(projection_from_snapshot(catalogue, snapshot))
    }

    pub fn select_manager_workspace_tender(
        &self,
        command: SelectManagerWorkspaceTenderCommand,
    ) -> Result<ManagerWorkspaceProjection, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command.validate().is_err() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let projection = self.inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(command.tender_id),
        })?;
        if projection
            .selected_tender
            .as_ref()
            .is_some_and(|tender| tender.state == ManagerWorkspaceTenderState::Active)
        {
            self.persist_manager_workspace_selection(&tender_id)?;
        }
        Ok(projection)
    }

    pub fn search_manager_workspace(
        &self,
        command: SearchManagerWorkspaceCommand,
    ) -> Result<WorkspaceSearchProjection, TenderCommandError> {
        require_setup(self)?;
        if command.validate().is_err() || command.query.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let tender_id = TenderId::parse(&command.tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let result = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .search_workspace(&command.query);
        result
    }

    pub fn record_engineer_workspace_message(
        &self,
        command: RecordEngineerWorkspaceMessageCommand,
    ) -> Result<ManagerWorkspaceProjection, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command.validate().is_err()
            || command.body.trim().is_empty()
            || command.body.trim().len() > MAX_MESSAGE_BYTES
        {
            return self.reject_tender_command(&tender_id, "record_engineer_workspace_message");
        }
        let mut catalogue = self.manager_workspace_catalogue()?;
        let selected = catalogue
            .iter()
            .find(|candidate| candidate.tender_id == command.tender_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        match selected.state {
            ManagerWorkspaceTenderState::Active => {}
            ManagerWorkspaceTenderState::Archived => {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            ManagerWorkspaceTenderState::RecoveryRequired => {
                return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
            }
        }
        self.persist_manager_workspace_selection(&tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        store.record_engineer_message(&tender_id, &command)?;
        let snapshot = store.workspace_snapshot()?;
        if let Some(catalogue_tender) = catalogue
            .iter_mut()
            .find(|candidate| candidate.tender_id == command.tender_id)
        {
            *catalogue_tender = snapshot.tender.clone();
        }
        sort_workspace_catalogue(&mut catalogue, Some(&tender_id));
        Ok(projection_from_snapshot(catalogue, snapshot))
    }

    fn manager_workspace_catalogue(
        &self,
    ) -> Result<Vec<ManagerWorkspaceTender>, TenderCommandError> {
        let mut catalogue = Vec::new();
        let recovery_required = self
            .recovery_required_tenders()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .clone();
        let entries =
            fs::read_dir(self.application_home().join("tenders")).map_err(store_unavailable)?;
        for entry in entries {
            let entry = entry.map_err(store_unavailable)?;
            let tender_id = entry
                .file_name()
                .to_str()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                .and_then(TenderId::parse)?;
            if recovery_required.contains(&tender_id) {
                catalogue.push(recovery_workspace_tender(
                    self.application_home(),
                    &tender_id,
                ));
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
            if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
                self.mark_tender_recovery_required(&tender_id);
                catalogue.push(recovery_workspace_tender(
                    self.application_home(),
                    &tender_id,
                ));
                continue;
            }
            match TenderStore::read_workspace_tender(&entry.path(), &tender_id) {
                Ok(tender) => catalogue.push(tender),
                Err(error) if error.code == TenderErrorCode::RecoveryRequired => {
                    self.mark_tender_recovery_required(&tender_id);
                    catalogue.push(recovery_workspace_tender(
                        self.application_home(),
                        &tender_id,
                    ));
                }
                Err(error) => return Err(error),
            }
        }
        Ok(catalogue)
    }

    fn persisted_manager_workspace_selection(
        &self,
    ) -> Result<(Option<TenderId>, Option<TenderId>), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = manager_workspace_selection_connection(self.application_home())?;
        connection
            .query_row(
                "SELECT pending_tender_id, selected_tender_id
                 FROM manager_workspace_selection WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .map_err(sql_error)
            .and_then(|(pending, selected)| {
                Ok((
                    pending.as_deref().map(TenderId::parse).transpose()?,
                    selected.as_deref().map(TenderId::parse).transpose()?,
                ))
            })
    }

    fn persist_manager_workspace_selection(
        &self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = manager_workspace_selection_connection(self.application_home())?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO manager_workspace_selection (
                   singleton, selected_tender_id, selection_sequence, selected_at,
                   pending_tender_id, pending_at
                 ) VALUES (
                   1, ?1, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), NULL, NULL
                 )
                 ON CONFLICT(singleton) DO UPDATE SET
                   selected_tender_id = excluded.selected_tender_id,
                   selection_sequence = manager_workspace_selection.selection_sequence + 1,
                   selected_at = excluded.selected_at,
                   pending_tender_id = NULL,
                   pending_at = NULL",
                [tender_id.as_str()],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)
    }

    fn reconcile_manager_tender_selection_publication(&self) -> Result<(), TenderCommandError> {
        let (pending, _) = self.persisted_manager_workspace_selection()?;
        let Some(pending) = pending else {
            return Ok(());
        };
        if self
            .application_home()
            .join("tenders")
            .join(pending.as_str())
            .is_dir()
        {
            self.finish_manager_tender_selection_publication(&pending)
        } else {
            self.cancel_manager_tender_selection_publication(&pending)
        }
    }

    fn begin_manager_tender_selection_publication(
        &self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        self.update_manager_tender_selection_publication(
            "UPDATE manager_workspace_selection
             SET pending_tender_id = ?1,
                 pending_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE singleton = 1 AND pending_tender_id IS NULL",
            tender_id,
        )
    }

    fn cancel_manager_tender_selection_publication(
        &self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        self.update_manager_tender_selection_publication(
            "UPDATE manager_workspace_selection
             SET pending_tender_id = NULL, pending_at = NULL
             WHERE singleton = 1 AND pending_tender_id = ?1",
            tender_id,
        )
    }

    fn finish_manager_tender_selection_publication(
        &self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        self.update_manager_tender_selection_publication(
            "UPDATE manager_workspace_selection
             SET selected_tender_id = pending_tender_id,
                 selection_sequence = selection_sequence + 1,
                 selected_at = pending_at,
                 pending_tender_id = NULL,
                 pending_at = NULL
             WHERE singleton = 1 AND pending_tender_id = ?1",
            tender_id,
        )
    }

    fn update_manager_tender_selection_publication(
        &self,
        statement: &str,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = manager_workspace_selection_connection(self.application_home())?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        if transaction
            .execute(statement, [tender_id.as_str()])
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        transaction.commit().map_err(sql_error)
    }
}

fn manager_workspace_selection_connection(
    application_home: &Path,
) -> Result<Connection, TenderCommandError> {
    let connection =
        Connection::open(application_home.join("installation.sqlite")).map_err(sql_error)?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(sql_error)?;
    Ok(connection)
}

fn sort_workspace_catalogue(
    catalogue: &mut [ManagerWorkspaceTender],
    selected_tender: Option<&TenderId>,
) {
    catalogue.sort_by(|left, right| {
        let left_selected =
            selected_tender.is_some_and(|selected| left.tender_id == selected.as_str());
        let right_selected =
            selected_tender.is_some_and(|selected| right.tender_id == selected.as_str());
        right_selected
            .cmp(&left_selected)
            .then_with(|| right.last_activity_at.cmp(&left.last_activity_at))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.tender_id.cmp(&right.tender_id))
    });
}

fn projection_from_snapshot(
    catalogue: Vec<ManagerWorkspaceTender>,
    snapshot: WorkspaceSnapshot,
) -> ManagerWorkspaceProjection {
    ManagerWorkspaceProjection {
        catalogue,
        selected_tender: Some(snapshot.tender),
        conversation: Some(snapshot.conversation),
        current_action: snapshot.current_action,
        work: snapshot.work,
        files: snapshot.files,
        team: snapshot.team,
        external_rfis: snapshot.external_rfis,
        estimate: snapshot.estimate,
        intake: snapshot.intake,
        ai_execution: Some(snapshot.ai_execution),
        capability_readiness: Some(snapshot.capability_readiness),
        doctor_blockers: snapshot.doctor_blockers,
    }
}

fn recovery_workspace_tender(
    application_home: &Path,
    tender_id: &TenderId,
) -> ManagerWorkspaceTender {
    let catalogue_name = Connection::open_with_flags(
        application_home.join("installation.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .and_then(|connection| {
        connection
            .query_row(
                "SELECT name FROM tender_catalogue WHERE tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
    })
    .ok()
    .flatten()
    .filter(|name| !name.trim().is_empty() && name.len() <= MAX_TENDER_NAME_BYTES);
    ManagerWorkspaceTender {
        tender_id: tender_id.as_str().to_owned(),
        name: catalogue_name.unwrap_or_else(|| format!("Tender {}", &tender_id.as_str()[..8])),
        revision: 0,
        phase: TenderLifecyclePhase::Intake,
        needs_engineer: true,
        state: ManagerWorkspaceTenderState::RecoveryRequired,
        can_archive: false,
        can_delete: true,
        last_activity_at: None,
    }
}

impl TenderStore {
    fn message_from_row(
        &self,
        message_id: String,
        sequence: i64,
        author: String,
        kind: String,
        body: String,
        created_at: String,
    ) -> Result<TenderOfficeMessage, TenderCommandError> {
        let references = self.message_references(&message_id)?;
        Ok(TenderOfficeMessage {
            message_id,
            sequence: to_u32(sequence)?,
            author: TenderOfficeMessageAuthor::parse(&author)?,
            kind: TenderOfficeMessageKind::parse(&kind)?,
            body,
            created_at,
            references,
        })
    }

    fn message_references(
        &self,
        message_id: &str,
    ) -> Result<Vec<WorkspaceMessageReference>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT kind, reference, version, evidence_ordinal, label, detail
                 FROM tender_office_message_references WHERE message_id = ?1 ORDER BY ordinal",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([message_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, Option<u32>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .map_err(sql_error)?;
        rows.map(|row| {
            let (kind, reference, version, evidence_ordinal, label, detail) =
                row.map_err(sql_error)?;
            Ok(WorkspaceMessageReference {
                kind: super::WorkspaceMessageReferenceKind::parse(&kind)?,
                reference,
                version,
                evidence_ordinal,
                label,
                detail,
            })
        })
        .collect()
    }
}

fn workspace_task_state(status: &str) -> WorkspaceTaskState {
    match status {
        "running" | "reviewing" => WorkspaceTaskState::Working,
        "review_ready"
        | "remediation_ready"
        | "query_blocked"
        | "attempt_limit_reached"
        | "indeterminate" => WorkspaceTaskState::NeedsEngineer,
        "ready_for_integration" => WorkspaceTaskState::Done,
        "cancelled" | "suspended" => WorkspaceTaskState::Paused,
        "failed" => WorkspaceTaskState::Failed,
        "blocked" | "ready" => WorkspaceTaskState::Waiting,
        _ => WorkspaceTaskState::Waiting,
    }
}

fn workspace_doctor_blockers(
    ai_execution: &TenderAiExecutionBinding,
    capability: &WorkspaceCapabilityReadiness,
) -> Vec<WorkspaceDoctorBlockerSummary> {
    let mut blockers = Vec::new();
    if !matches!(
        ai_execution.readiness,
        TenderAiSelectionReadiness::LocalOnly | TenderAiSelectionReadiness::Ready
    ) {
        let (code, title) = match ai_execution.readiness {
            TenderAiSelectionReadiness::SelectionRequired => {
                ("ai_selection_required", "AI execution selection required")
            }
            TenderAiSelectionReadiness::ProviderUnavailable => {
                ("ai_provider_unavailable", "AI provider unavailable")
            }
            TenderAiSelectionReadiness::CatalogueStale => {
                ("ai_catalogue_stale", "AI capability catalogue is stale")
            }
            TenderAiSelectionReadiness::ModelUnavailable => (
                "ai_model_unavailable",
                "AI model or reasoning is unavailable",
            ),
            TenderAiSelectionReadiness::ApprovalRequired => {
                ("ai_approval_required", "AI execution approval required")
            }
            TenderAiSelectionReadiness::LocalOnly | TenderAiSelectionReadiness::Ready => {
                unreachable!("handled by the readiness guard")
            }
        };
        blockers.push(WorkspaceDoctorBlockerSummary {
            code: code.to_owned(),
            area: WorkspaceDoctorBlockerArea::AiExecution,
            title: title.to_owned(),
            detail: ai_execution.status_summary.clone(),
        });
    }
    for gap in &capability.gaps {
        blockers.push(WorkspaceDoctorBlockerSummary {
            code: format!("capability_gap:{}", gap.capability),
            area: WorkspaceDoctorBlockerArea::Capability,
            title: format!("Capability gap: {}", gap.capability),
            detail: gap.reason.clone(),
        });
    }
    for code in &capability.blocker_codes {
        blockers.push(WorkspaceDoctorBlockerSummary {
            code: code.clone(),
            area: WorkspaceDoctorBlockerArea::Capability,
            title: "Work Plan blocker".to_owned(),
            detail: format!("The current Work Plan reports blocker code `{code}`."),
        });
    }
    blockers.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.detail.cmp(&right.detail))
    });
    blockers
}

fn empty_projection(catalogue: Vec<ManagerWorkspaceTender>) -> ManagerWorkspaceProjection {
    ManagerWorkspaceProjection {
        catalogue,
        selected_tender: None,
        conversation: None,
        current_action: action(
            WorkspaceActionKind::StartTender,
            "Start a Tender",
            "Choose the Tender Package and the Tender Manager will take it from there.",
            "Choose Tender Package",
            true,
        ),
        work: WorkspaceWorkSummary::default(),
        files: WorkspaceFilesSummary::default(),
        team: WorkspaceTeamSummary::default(),
        external_rfis: Vec::new(),
        estimate: None,
        intake: None,
        ai_execution: None,
        capability_readiness: None,
        doctor_blockers: Vec::new(),
    }
}

fn message_kind_value(kind: TenderOfficeMessageKind) -> &'static str {
    match kind {
        TenderOfficeMessageKind::Routine => "routine",
        TenderOfficeMessageKind::Status => "status",
        TenderOfficeMessageKind::Question => "question",
        TenderOfficeMessageKind::Finding => "finding",
        TenderOfficeMessageKind::Handoff => "handoff",
        TenderOfficeMessageKind::Blocker => "blocker",
        TenderOfficeMessageKind::Output => "output",
    }
}

fn action(
    kind: WorkspaceActionKind,
    title: &str,
    summary: &str,
    action_label: &str,
    requires_engineer: bool,
) -> WorkspaceCurrentAction {
    WorkspaceCurrentAction {
        kind,
        title: title.to_owned(),
        summary: summary.to_owned(),
        action_label: action_label.to_owned(),
        requires_engineer,
    }
}

fn to_u32(value: i64) -> Result<u32, TenderCommandError> {
    value
        .try_into()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn tender_name_from_source(source: &Path) -> Result<String, TenderCommandError> {
    let candidate = if source.is_file() {
        source.file_stem()
    } else {
        source.file_name()
    }
    .and_then(|value| value.to_str())
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let mut name = String::new();
    for character in candidate.chars() {
        if name.len() + character.len_utf8() > MAX_TENDER_NAME_BYTES {
            break;
        }
        name.push(character);
    }
    if name.is_empty() {
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    } else {
        Ok(name)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, io, path::Path, sync::Arc};

    use super::{
        workspace_doctor_blockers, workspace_task_state, TenderAiExecutionBinding,
        TenderAiSelectionReadiness, WorkPlanCapabilityGap, WorkspaceCapabilityReadiness,
        WorkspaceCapabilityReadinessState, WorkspaceDoctorBlockerArea, WorkspaceTaskState,
    };
    use crate::{
        setup::{SetupPlatform, StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES},
        InspectManagerWorkspaceCommand, ManagerWorkspaceTenderState, PackageIntakeOperationKind,
        QuantixHost, SetupState, TenderIntegrityState, TenderPackageSourceKind,
    };

    struct ReadySetupPlatform;

    impl SetupPlatform for ReadySetupPlatform {
        fn available_space(&self, _path: &Path) -> io::Result<u64> {
            Ok(MINIMUM_SETUP_FREE_SPACE_BYTES)
        }

        fn is_writable(&self, _path: &Path) -> io::Result<bool> {
            Ok(true)
        }

        fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
            Ok(StoragePermissions::Restrictive)
        }
    }

    #[test]
    fn manager_tender_start_reaches_package_intake_after_host_sqlite_initialization() {
        let user_home = tempfile::tempdir().expect("temporary user home");
        let application_home = user_home.path().join(".quantix");
        let package = user_home.path().join("Juhayna");
        fs::create_dir(&package).expect("create Tender Package");
        fs::write(package.join("scope.txt"), b"unsupported fixture document")
            .expect("write Tender Package entry");

        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(host.ensure_setup().state, SetupState::Ready);
        let control = host
            .begin_package_intake(
                PackageIntakeOperationKind::StartTender,
                TenderPackageSourceKind::Directory,
                "Juhayna",
            )
            .expect("claim package intake");
        let operation_id = control.snapshot().operation_id.clone();

        let projection = host
            .start_manager_tender_from_package_with_control(&package, Some(&control), true)
            .expect("start Manager Tender")
            .expect("package intake should complete");
        host.finish_package_intake(&operation_id);

        let selected = projection.selected_tender.expect("selected Tender");
        assert_eq!(selected.name, "Juhayna");
        let selected_id = selected.tender_id.clone();
        let progress = control.snapshot();
        assert_eq!(
            progress.stage,
            crate::tender_intake::PackageIntakeStage::OpeningWorkspace
        );
        assert_eq!(progress.discovered_count, 1);
        assert_eq!(progress.processed_count, 1);
        assert!(!progress.cancellable);
        assert!(application_home.join("tenders").join(&selected_id).is_dir());
        assert!(!application_home
            .join("staging")
            .join(format!("tender-{selected_id}"))
            .exists());
    }

    #[test]
    fn manager_created_recovery_tender_keeps_its_name_after_catalogue_refresh_and_restart() {
        let user_home = tempfile::tempdir().expect("temporary user home");
        let application_home = user_home.path().join(".quantix");
        let package = user_home.path().join("Manager Recovery Fixture");
        fs::create_dir(&package).expect("create disposable Tender Package");
        fs::write(package.join("Tender Package.txt"), b"recovery fixture")
            .expect("write disposable Tender Package");
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        let setup = host.ensure_setup();
        assert_eq!(setup.state, SetupState::Ready, "setup outcome: {setup:#?}");
        let created = host
            .start_manager_tender_from_package_with_control(&package, None, true)
            .expect("start Manager Tender")
            .expect("completed Manager Tender intake");
        let tender = created.selected_tender.expect("selected Manager Tender");
        assert_eq!(tender.name, "Manager Recovery Fixture");
        host.close_tender(&tender.tender_id)
            .expect("close Manager-created Tender");
        rusqlite::Connection::open(
            application_home
                .join("tenders")
                .join(&tender.tender_id)
                .join("tender.sqlite"),
        )
        .expect("Manager Tender Store")
        .execute_batch("DROP TRIGGER audit_events_no_update")
        .expect("alter exact Tender Store schema");
        drop(host);

        let restarted =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        let setup = restarted.ensure_setup();
        assert_eq!(setup.state, SetupState::Ready, "setup outcome: {setup:#?}");
        let listed = restarted
            .list_tenders()
            .expect("refresh catalogue without erasing recovery metadata");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].summary.is_none());
        assert_eq!(
            listed[0].integrity.state,
            TenderIntegrityState::RecoveryRequired
        );
        let projection = restarted
            .inspect_manager_workspace(InspectManagerWorkspaceCommand { tender_id: None })
            .expect("inspect cold Manager workspace");
        let recovery = projection
            .catalogue
            .iter()
            .find(|candidate| candidate.tender_id == tender.tender_id)
            .expect("Manager-created recovery Tender remains visible");
        assert_eq!(recovery.name, "Manager Recovery Fixture");
        assert_eq!(
            recovery.state,
            ManagerWorkspaceTenderState::RecoveryRequired
        );
        assert!(recovery.can_delete);
        assert!(projection.selected_tender.is_none());
    }

    #[test]
    fn workspace_task_state_preserves_authoritative_status_semantics() {
        assert_eq!(workspace_task_state("blocked"), WorkspaceTaskState::Waiting);
        assert_eq!(workspace_task_state("ready"), WorkspaceTaskState::Waiting);
        assert_eq!(workspace_task_state("running"), WorkspaceTaskState::Working);
        assert_eq!(
            workspace_task_state("review_ready"),
            WorkspaceTaskState::NeedsEngineer
        );
        assert_eq!(
            workspace_task_state("ready_for_integration"),
            WorkspaceTaskState::Done
        );
        assert_eq!(
            workspace_task_state("cancelled"),
            WorkspaceTaskState::Paused
        );
        assert_eq!(workspace_task_state("failed"), WorkspaceTaskState::Failed);
    }

    #[test]
    fn workspace_doctor_blockers_are_redacted_and_deterministically_ordered() {
        let ai_execution = TenderAiExecutionBinding {
            revision: 2,
            selection: None,
            readiness: TenderAiSelectionReadiness::ProviderUnavailable,
            status_summary: "Provider connection is unavailable.".to_owned(),
        };
        let capability = WorkspaceCapabilityReadiness {
            state: WorkspaceCapabilityReadinessState::Blocked,
            gaps: vec![WorkPlanCapabilityGap {
                capability: "quantity-survey".to_owned(),
                reason: "No approved profile covers this capability.".to_owned(),
                affected_work: vec!["boq".to_owned()],
            }],
            blocker_codes: vec!["zeta".to_owned(), "alpha".to_owned()],
        };
        let blockers = workspace_doctor_blockers(&ai_execution, &capability);
        assert_eq!(blockers.len(), 4);
        assert_eq!(blockers[0].code, "ai_provider_unavailable");
        assert_eq!(blockers[0].area, WorkspaceDoctorBlockerArea::AiExecution);
        assert_eq!(blockers[1].code, "alpha");
        assert_eq!(blockers[2].code, "capability_gap:quantity-survey");
        assert_eq!(blockers[3].code, "zeta");
        assert!(!blockers[0].detail.contains("token"));
    }
}
