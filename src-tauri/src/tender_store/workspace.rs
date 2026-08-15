use garde::Validate;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use std::{fs, path::Path, time::Duration};

use crate::{
    application_settings::load_preferred_ai_execution_selection, QuantixHost,
    TenderPackageSourceKind,
};

use super::{
    append_audit_event, random_identifier, require_setup, sha256_hex, sql_error, sqlite_timestamp,
    storage_publication_failpoint, store_unavailable, tender_records::insert_engineer_entry,
    ManagerIntakeStage, ManagerIntakeStatus, TenderCommandError, TenderErrorCode, TenderId,
    TenderLifecyclePhase, TenderStore, WorkspaceMessageReference, WorkspaceTenderDocument,
    MAX_TENDER_NAME_BYTES,
};

const MAX_CONVERSATION_MESSAGES: i64 = 100;
const MAX_MESSAGE_BYTES: usize = 4_000;

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
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RecordEngineerWorkspaceMessageCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub body: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ManagerWorkspaceTender {
    pub tender_id: String,
    pub name: String,
    pub revision: u32,
    pub phase: TenderLifecyclePhase,
    pub needs_engineer: bool,
    pub available: bool,
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
    TenderDeclined,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceWorkSummary {
    pub needs_engineer: u32,
    pub working: u32,
    pub waiting: u32,
    pub done: u32,
    pub cancelled: u32,
    pub failed: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceFilesSummary {
    pub tender_document_count: u32,
    pub quantix_output_count: u32,
    pub tender_documents: Vec<WorkspaceTenderDocument>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceTeamSummary {
    pub active_agent_runs: u32,
    pub waiting_tasks: u32,
    pub needs_engineer: u32,
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
    pub intake: Option<ManagerIntakeStatus>,
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
             ) VALUES (?1, ?2, 'system', ?3, ?4, ?5)",
            params![
                message_id,
                conversation_id,
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
    intake: Option<ManagerIntakeStatus>,
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
        let work = self.workspace_work_summary()?;
        let files = self.workspace_files_summary()?;
        let team = self.workspace_team_summary(&work)?;
        let intake = self.current_manager_intake_status()?;
        let current_action =
            self.workspace_current_action(summary.lifecycle_phase, &work, intake.as_ref())?;
        let tender = ManagerWorkspaceTender {
            tender_id: summary.tender_id,
            name: summary.name,
            revision: summary.revision,
            phase: summary.lifecycle_phase,
            needs_engineer: current_action.requires_engineer || work.needs_engineer > 0,
            available: !self.archived,
            last_activity_at: Some(last_activity_at),
        };
        Ok(WorkspaceSnapshot {
            tender,
            conversation: self.manager_conversation()?,
            current_action,
            work,
            files,
            team,
            intake,
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
                     ) VALUES (?1, 1, 'manager_intake_outcome', ?2, 1, NULL, ?3, ?4)",
                    params![
                        message_id,
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
        Ok(WorkspaceWorkSummary {
            needs_engineer: to_u32(counts.0)?,
            working: to_u32(counts.1)?,
            waiting: to_u32(counts.2)?,
            done: to_u32(counts.3)?,
            cancelled: to_u32(counts.4)?,
            failed: to_u32(counts.5)?,
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
        })
    }

    fn workspace_team_summary(
        &self,
        work: &WorkspaceWorkSummary,
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
        })
    }

    fn workspace_current_action(
        &self,
        phase: TenderLifecyclePhase,
        work: &WorkspaceWorkSummary,
        intake: Option<&ManagerIntakeStatus>,
    ) -> Result<WorkspaceCurrentAction, TenderCommandError> {
        let action = match phase {
            TenderLifecyclePhase::Intake => {
                if let Some(intake) = intake {
                    return Ok(match intake.stage {
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
}

impl QuantixHost {
    pub(crate) fn start_manager_tender_from_package(
        &self,
        source: &Path,
    ) -> Result<ManagerWorkspaceProjection, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        if !source.is_absolute() || fs::symlink_metadata(source).is_err() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
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
        let preferred_selection = load_preferred_ai_execution_selection(self.application_home())?;
        let mut store = match TenderStore::create(&stage_root, &tender_id, &name) {
            Ok(store) => store,
            Err(error) => {
                let _ = fs::remove_dir_all(&stage_root);
                return Err(error);
            }
        };
        if let Err(error) = store.import_package(source) {
            drop(store);
            let _ = fs::remove_dir_all(&stage_root);
            return Err(error);
        }
        if let Some(selection) = preferred_selection {
            if let Err(error) = store.bind_manager_intake_provider_selection(&selection, false) {
                drop(store);
                let _ = fs::remove_dir_all(&stage_root);
                return Err(error);
            }
        }
        let projection = match (|| {
            let snapshot = store.workspace_snapshot()?;
            let mut catalogue = self.manager_workspace_catalogue()?;
            catalogue.push(snapshot.tender.clone());
            sort_workspace_catalogue(&mut catalogue, Some(&tender_id));
            let projection = projection_from_snapshot(catalogue, snapshot);
            self.begin_manager_tender_selection_publication(&tender_id)?;
            Ok::<_, TenderCommandError>(projection)
        })() {
            Ok(projection) => projection,
            Err(error) => {
                drop(store);
                let _ = fs::remove_dir_all(&stage_root);
                return Err(error);
            }
        };
        drop(store);
        storage_publication_failpoint("tender_after_stage");
        if let Err(error) = fs::rename(&stage_root, &final_root) {
            let _ = fs::remove_dir_all(&stage_root);
            let _ = self.cancel_manager_tender_selection_publication(&tender_id);
            return Err(store_unavailable(error));
        }
        let _ = self.finish_manager_tender_selection_publication(&tender_id);
        storage_publication_failpoint("tender_after_publish");
        Ok(projection)
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
                if !selected.available {
                    return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
                }
                Some(requested)
            }
            None => {
                let persisted = persisted_tenders
                    .0
                    .filter(|persisted| {
                        catalogue.iter().any(|tender| {
                            tender.available && tender.tender_id == persisted.as_str()
                        })
                    })
                    .or_else(|| {
                        persisted_tenders.1.filter(|persisted| {
                            catalogue.iter().any(|tender| {
                                tender.available && tender.tender_id == persisted.as_str()
                            })
                        })
                    });
                if let Some(persisted) = persisted {
                    Some(persisted)
                } else {
                    catalogue
                        .iter()
                        .find(|tender| tender.available)
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
        self.persist_manager_workspace_selection(&tender_id)?;
        Ok(projection)
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
        if !selected.available {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
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
        for entry in self.list_tenders()? {
            if entry.summary.is_some() {
                let tender_id = TenderId::parse(&entry.tender_id)?;
                let store = self.tender_store(&tender_id)?;
                let snapshot = store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .workspace_snapshot()?;
                catalogue.push(snapshot.tender);
            } else {
                catalogue.push(ManagerWorkspaceTender {
                    tender_id: entry.tender_id.clone(),
                    name: format!("Tender {}", &entry.tender_id[..8]),
                    revision: 0,
                    phase: TenderLifecyclePhase::Intake,
                    needs_engineer: true,
                    available: false,
                    last_activity_at: None,
                });
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
        intake: snapshot.intake,
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
        intake: None,
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
