use std::{fs, path::Path, thread, time::Duration};

use jiff::Timestamp;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use crate::agent_runtime::{
    approve_one_run_access, bootstrap_profile, bootstrap_task,
    permissions::{
        bootstrap_tool_catalogue, derive_bootstrap_grant, one_run_grant_authorizes_tool,
        permission_duration, BootstrapGrantRequest,
    },
    AccessApproval, AccessRequest, AgentAccessRequestStatus, AgentAccessRequestView,
    AgentProfileStatus, AgentProfileVersionView, AgentRunActivity, AgentRunHistoryItem,
    AgentRunHistoryPage, AgentRunInspection, AgentRunRecoveryDecision, AgentRunRecoveryDisposition,
    AgentRunState, AgentRunSummary, ApproveAgentAccessCommand, BootstrapAuthority, BootstrapRole,
    BootstrapTeamMember, DataClassification, PendingProviderEvent, PermissionGrant,
    PreparedAgentRun, ProposedAgentResult, ProviderEvent, ProviderEventKind, ProviderExecution,
    ProviderFailure, ProviderFailureCategory, ProviderUsage, RequestAgentAccessCommand,
    ResolveAgentAccessCommand, ResolveIndeterminateAgentRunCommand, TenderTaskView,
    ThreadExposureSet, VerificationStatus,
};

use super::bid_decisions::{
    bid_decision_package_review_target_is_current, bid_decision_package_review_target_is_open,
    publish_bid_decision_package_review, BidDecisionPackageReviewCandidate,
    BID_PACKAGE_REVIEW_CAPABILITY,
};
use super::production_scheduler::{
    finish_production_task, production_completion_payload_is_valid, production_task_for_run,
    work_plan_package_dependencies_are_current,
};
use super::tender_records::{
    publish_tender_record_candidates, publish_tender_record_review,
    tender_record_candidates_fit_decision_inventory, tender_record_review_target_is_open,
    TenderRecordCandidateBatch, TenderRecordReviewCandidate, RECORD_EXTRACTION_CAPABILITY,
    RECORD_REVIEW_CAPABILITY,
};
use super::{
    append_audit_event, metadata_is_unsafe_storage_link, random_identifier, sql_error,
    sqlite_timestamp, store_unavailable, TenderCommandError, TenderErrorCode, TenderId,
    TenderStore,
};

const BOOTSTRAP_STABLE_IDENTITY: &str = "quantix.bootstrap.tender-analyst";
const MAX_AGENT_RUNS_PER_TENDER: usize = 10_000;
const MAX_PROVIDER_EVENTS_PER_RUN: usize = 10_000;
const MAX_PROVIDER_EVENT_FIELD_BYTES: u64 = 64 * 1024;
const MAX_PROVIDER_EVENT_BYTES_PER_RUN: u64 = 16 * 1024 * 1024;

fn profile_supports_linked_retry(profile: &AgentProfileVersionView) -> bool {
    !profile.capabilities.iter().any(|capability| {
        matches!(
            capability.as_str(),
            RECORD_EXTRACTION_CAPABILITY | RECORD_REVIEW_CAPABILITY | BID_PACKAGE_REVIEW_CAPABILITY
        )
    })
}

impl TenderStore {
    pub(crate) fn inspect_bootstrap_team(
        &self,
    ) -> Result<Vec<BootstrapTeamMember>, TenderCommandError> {
        BootstrapRole::ALL
            .into_iter()
            .map(|role| {
                let profile_id: String = self
                    .connection
                    .query_row(
                        "SELECT profile_id FROM agent_profiles WHERE stable_identity = ?1",
                        [role.stable_identity()],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                Ok(BootstrapTeamMember {
                    role,
                    authority: BootstrapAuthority::PreBidAnalysis,
                    active: true,
                    profile: load_profile(&self.connection, (profile_id, 1))?,
                })
            })
            .collect()
    }

    pub(crate) fn prepare_bootstrap_agent_run(
        &mut self,
        tender_id: &TenderId,
        retry_of_run_id: Option<&str>,
        subscription_capacity_exhausted: bool,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        if let Some(retry_of_run_id) = retry_of_run_id {
            if let Some(production_task_id) =
                production_task_for_run(&self.connection, retry_of_run_id)?
            {
                return self.prepare_production_task_run(
                    tender_id,
                    &production_task_id,
                    Some(retry_of_run_id),
                    subscription_capacity_exhausted,
                );
            }
        }
        self.require_pre_bid_writable()?;
        let run_id = random_identifier(&self.connection)?;
        let application_home = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let workspace = agent_workspace_path(application_home, tender_id.as_str(), &run_id);

        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let tender_revision: u32 = transaction
                .query_row(
                    "SELECT current_revision FROM tender
                     WHERE singleton = 1 AND tender_id = ?1",
                    [tender_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let created_at = sqlite_timestamp(&transaction)?;
            let has_unresolved_indeterminate: bool = transaction
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
            if has_unresolved_indeterminate {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let pending_recovery_retry: Option<String> = transaction
                .query_row(
                    "SELECT agent_runs.run_id
                     FROM agent_runs
                     JOIN agent_run_recovery_dispositions
                       ON agent_run_recovery_dispositions.run_id = agent_runs.run_id
                     WHERE agent_runs.status = 'indeterminate'
                       AND agent_run_recovery_dispositions.disposition = 'retry_task'
                       AND NOT EXISTS (
                         SELECT 1 FROM agent_runs AS linked_retry
                         WHERE linked_retry.retry_of_run_id = agent_runs.run_id
                       )
                     ORDER BY agent_runs.run_sequence
                     LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            if pending_recovery_retry.as_deref() != retry_of_run_id
                && pending_recovery_retry.is_some()
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }

            let (profile, task) = if let Some(retry_of_run_id) = retry_of_run_id {
                let prior: Option<(String, String, Option<String>)> = transaction
                    .query_row(
                        "SELECT status, task_id,
                                (SELECT disposition FROM agent_run_recovery_dispositions
                                 WHERE agent_run_recovery_dispositions.run_id = agent_runs.run_id)
                         FROM agent_runs WHERE run_id = ?1",
                        [retry_of_run_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                let (prior_status, task_id, recovery_disposition) = prior
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                if prior_status == "running"
                    || (prior_status == "indeterminate"
                        && recovery_disposition.as_deref() != Some("retry_task"))
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                if prior_status == "indeterminate"
                    && transaction
                        .query_row(
                            "SELECT EXISTS(
                               SELECT 1 FROM agent_runs WHERE retry_of_run_id = ?1
                             )",
                            [retry_of_run_id],
                            |row| row.get::<_, bool>(0),
                        )
                        .map_err(sql_error)?
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                (
                    load_profile(&transaction, task_profile(&transaction, &task_id)?)?,
                    load_task(&transaction, &task_id)?,
                )
            } else {
                let profile_id: Option<String> = transaction
                    .query_row(
                        "SELECT profile_id FROM agent_profiles WHERE stable_identity = ?1",
                        [BOOTSTRAP_STABLE_IDENTITY],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
                let profile = if let Some(profile_id) = profile_id {
                    load_profile(&transaction, (profile_id, 1))?
                } else {
                    let profile = bootstrap_profile(
                        BootstrapRole::TenderAnalyst,
                        random_identifier(&transaction)?,
                    );
                    insert_profile(
                        &transaction,
                        BootstrapRole::TenderAnalyst.stable_identity(),
                        &profile,
                        &created_at,
                    )?;
                    profile
                };
                let deadline: String = transaction
                    .query_row(
                        "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                let task = bootstrap_task(
                    random_identifier(&transaction)?,
                    tender_id.as_str(),
                    tender_revision,
                    deadline,
                    &profile,
                );
                insert_task(&transaction, &task, &created_at)?;
                (profile, task)
            };
            let exact_tender_revision = exact_tender_revision(&task, tender_id)?;
            let tender_name: String = transaction
                .query_row(
                    "SELECT name FROM tender_revisions
                     WHERE tender_id = ?1 AND revision = ?2",
                    params![tender_id.as_str(), exact_tender_revision],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let existing_provider_thread: Option<(String, String)> = transaction
                .query_row(
                    "SELECT thread_ref, status FROM provider_threads
                     WHERE profile_id = ?1 AND profile_version = ?2
                       AND status IN ('active', 'archive_pending')",
                    params![profile.profile_id, profile.version],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (permission_grant, materialized_workspace) =
                derive_bootstrap_grant(BootstrapGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home,
                    tender_id: tender_id.as_str(),
                    tender_name: &tender_name,
                    tender_revision: exact_tender_revision,
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                })?;
            if materialized_workspace != workspace {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let remaining = permission_duration(&permission_grant, Timestamp::now())
                .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            if remaining.is_zero() {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let (provider_thread_ref, provider_thread_to_archive) = match existing_provider_thread {
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
                Some(_) => {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
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
            insert_event(
                &transaction,
                &run_id,
                1,
                PendingProviderEvent {
                    kind: ProviderEventKind::RunStarted,
                    summary: "Agent Run started".into(),
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
                "agent_run_started",
                tender_revision,
                json!({
                    "profile_id": profile.profile_id,
                    "profile_version": profile.version.to_string(),
                    "retry_of_run_id": retry_of_run_id,
                    "run_id": run_id,
                    "task_id": task.task_id,
                }),
                &created_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            Ok(PreparedAgentRun {
                run_id,
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

    pub(crate) fn complete_agent_run(
        &mut self,
        tender_id: &TenderId,
        prepared: &PreparedAgentRun,
        mut execution: ProviderExecution,
    ) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        if execution.state == AgentRunState::Running
            || (execution.state == AgentRunState::Completed
                && (execution.failure.is_some() || execution.candidate_payload_json.is_none()))
            || (execution.state != AgentRunState::Completed && execution.failure.is_none())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let mut tender_record_candidate: Option<TenderRecordCandidateBatch> = None;
        let mut tender_record_review: Option<TenderRecordReviewCandidate> = None;
        let mut bid_package_review: Option<BidDecisionPackageReviewCandidate> = None;
        let mut denied_record_publication_count = None;
        if execution.state == AgentRunState::Completed
            && prepared
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == RECORD_EXTRACTION_CAPABILITY)
        {
            let validation = execution
                .candidate_payload_json
                .as_deref()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                .and_then(|payload| self.validate_tender_record_candidate(&prepared.task, payload));
            match validation {
                Ok(candidate) => tender_record_candidate = Some(candidate),
                Err(_) => {
                    execution.state = AgentRunState::Failed;
                    execution.failure = Some(ProviderFailure::new(
                        ProviderFailureCategory::OutputInvalid,
                        true,
                        "Run the Tender Record extraction again with complete exact provenance.",
                        Some("The candidate Tender Records failed Quantix provenance validation."),
                    ));
                    execution.candidate_payload_json = None;
                    if let Some(event) = execution
                        .events
                        .iter_mut()
                        .rev()
                        .find(|event| event.kind == ProviderEventKind::Terminal)
                    {
                        event.summary =
                            "Candidate Tender Records failed provenance validation".into();
                    }
                }
            }
        }
        if execution.state == AgentRunState::Completed
            && prepared
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == BID_PACKAGE_REVIEW_CAPABILITY)
        {
            let validation = execution
                .candidate_payload_json
                .as_deref()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                .and_then(|payload| {
                    self.validate_bid_decision_package_review_candidate(&prepared.task, payload)
                });
            match validation {
                Ok(candidate) => bid_package_review = Some(candidate),
                Err(_) => {
                    execution.state = AgentRunState::Failed;
                    execution.failure = Some(ProviderFailure::new(
                        ProviderFailureCategory::OutputInvalid,
                        true,
                        "Review the exact Bid Decision Package again with attributable bounded findings.",
                        Some("The independent Bid Decision Package review failed Quantix validation."),
                    ));
                    execution.candidate_payload_json = None;
                    if let Some(event) = execution
                        .events
                        .iter_mut()
                        .rev()
                        .find(|event| event.kind == ProviderEventKind::Terminal)
                    {
                        event.summary =
                            "Independent Bid Decision Package review failed validation".into();
                    }
                }
            }
        }
        if execution.state == AgentRunState::Completed
            && prepared
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == RECORD_REVIEW_CAPABILITY)
        {
            let validation = execution
                .candidate_payload_json
                .as_deref()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                .and_then(|payload| {
                    self.validate_tender_record_review_candidate(
                        &prepared.task,
                        &prepared.profile.profile_id,
                        payload,
                    )
                });
            match validation {
                Ok(candidate) => tender_record_review = Some(candidate),
                Err(_) => {
                    execution.state = AgentRunState::Failed;
                    execution.failure = Some(ProviderFailure::new(
                        ProviderFailureCategory::OutputInvalid,
                        true,
                        "Review the exact Tender Record again without filling provenance gaps.",
                        Some(
                            "The independent review outcome failed Quantix provenance validation.",
                        ),
                    ));
                    execution.candidate_payload_json = None;
                    if let Some(event) = execution
                        .events
                        .iter_mut()
                        .rev()
                        .find(|event| event.kind == ProviderEventKind::Terminal)
                    {
                        event.summary = "Independent review failed provenance validation".into();
                    }
                }
            }
        }
        if execution.state == AgentRunState::Completed
            && bid_package_review.is_some()
            && !bid_decision_package_review_target_is_current(self, &prepared.task)?
        {
            execution.state = AgentRunState::Failed;
            execution.failure = Some(ProviderFailure::new(
                ProviderFailureCategory::OutputInvalid,
                false,
                "Inspect and review the current exact Bid Decision Package version.",
                Some("The package evidence inventory changed before review publication."),
            ));
            execution.candidate_payload_json = None;
            bid_package_review = None;
            if let Some(event) = execution
                .events
                .iter_mut()
                .rev()
                .find(|event| event.kind == ProviderEventKind::Terminal)
            {
                event.summary =
                    "Independent package review rejected because its evidence inventory is stale"
                        .into();
            }
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let interruption_committed: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_run_cancellations WHERE run_id = ?1)",
                [&prepared.run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if interruption_committed {
            execution.state = AgentRunState::Interrupted;
            execution.failure = Some(ProviderFailure::new(
                ProviderFailureCategory::Interrupted,
                false,
                "Start a new Agent Run only if the Tender Task still requires this work.",
                Some("The Engineer User interrupted the Agent Run."),
            ));
            execution.candidate_payload_json = None;
            tender_record_candidate = None;
            tender_record_review = None;
            bid_package_review = None;
            if let Some(event) = execution
                .events
                .iter_mut()
                .rev()
                .find(|event| event.kind == ProviderEventKind::Terminal)
            {
                event.summary = "Provider outcome discarded after Engineer interruption".into();
            }
        }
        if execution.state == AgentRunState::Completed
            && tender_record_review.is_some()
            && !tender_record_review_target_is_open(&transaction, &prepared.task)?
        {
            execution.state = AgentRunState::Failed;
            execution.failure = Some(ProviderFailure::new(
                ProviderFailureCategory::OutputInvalid,
                false,
                "Inspect the current exact Tender Record and its existing decision.",
                Some("The review target was decided or superseded before review publication."),
            ));
            execution.candidate_payload_json = None;
            tender_record_review = None;
            if let Some(event) = execution
                .events
                .iter_mut()
                .rev()
                .find(|event| event.kind == ProviderEventKind::Terminal)
            {
                event.summary =
                    "Independent review rejected because its exact target is no longer open".into();
            }
        }
        if execution.state == AgentRunState::Completed
            && bid_package_review.is_some()
            && !bid_decision_package_review_target_is_open(&transaction, &prepared.task)?
        {
            execution.state = AgentRunState::Failed;
            execution.failure = Some(ProviderFailure::new(
                ProviderFailureCategory::OutputInvalid,
                false,
                "Inspect the current exact Bid Decision Package and its existing review or decision.",
                Some("The package was superseded, reviewed, or decided before review publication."),
            ));
            execution.candidate_payload_json = None;
            bid_package_review = None;
            if let Some(event) = execution
                .events
                .iter_mut()
                .rev()
                .find(|event| event.kind == ProviderEventKind::Terminal)
            {
                event.summary =
                    "Independent package review rejected because its exact target is no longer open".into();
            }
        }
        if execution.state == AgentRunState::Completed
            && tender_record_candidate.is_some()
            && !tender_record_candidates_fit_decision_inventory(
                &transaction,
                tender_record_candidate
                    .as_ref()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            )?
        {
            denied_record_publication_count = tender_record_candidate
                .as_ref()
                .map(|candidate| candidate.records.len());
            execution.state = AgentRunState::Failed;
            execution.failure = Some(ProviderFailure::new(
                ProviderFailureCategory::OutputInvalid,
                false,
                "Select a bounded material-change batch that keeps the exact Bid Decision inventory within its limit.",
                Some("The candidate Tender Records would exceed the exact Bid Decision inventory limit."),
            ));
            execution.candidate_payload_json = None;
            tender_record_candidate = None;
            if let Some(event) = execution
                .events
                .iter_mut()
                .rev()
                .find(|event| event.kind == ProviderEventKind::Terminal)
            {
                event.summary =
                    "Candidate Tender Records rejected because the decision inventory is full"
                        .into();
            }
        }
        let invalid_production_payload = if execution.state == AgentRunState::Completed {
            match execution.candidate_payload_json.as_deref() {
                Some(payload) => !production_completion_payload_is_valid(
                    &transaction,
                    &prepared.task.task_id,
                    payload,
                )?,
                None => false,
            }
        } else {
            false
        };
        if invalid_production_payload {
            execution.state = AgentRunState::Failed;
            execution.failure = Some(ProviderFailure::new(
                ProviderFailureCategory::OutputInvalid,
                true,
                "Run the same exact production attempt again with attributable, verified Evidence and finding lineage.",
                Some("The production output did not satisfy the exact Artifact or Review semantics."),
            ));
            execution.candidate_payload_json = None;
            if let Some(event) = execution
                .events
                .iter_mut()
                .rev()
                .find(|event| event.kind == ProviderEventKind::Terminal)
            {
                event.summary =
                    "Production output rejected by exact validation and Evidence guards".into();
            }
        }
        dispose_workspace(
            &self.root,
            &prepared.workspace,
            &prepared.run_id,
            execution.state,
        )?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let task_tender_revision = prepared
            .task
            .exact_inputs
            .iter()
            .find(|input| input.kind == "tender_revision" && input.reference == tender_id.as_str())
            .map(|input| input.version);
        if execution.state == AgentRunState::Completed
            && prepared
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == RECORD_EXTRACTION_CAPABILITY)
            && task_tender_revision.is_some_and(|version| version != tender_revision)
        {
            execution.state = AgentRunState::Failed;
            execution.failure = Some(ProviderFailure::new(
                ProviderFailureCategory::OutputInvalid,
                true,
                "Run the task again against the current exact Tender revision.",
                Some("The Agent Run output became stale before canonical publication."),
            ));
            execution.candidate_payload_json = None;
            tender_record_candidate = None;
            tender_record_review = None;
            bid_package_review = None;
            if let Some(event) = execution
                .events
                .iter_mut()
                .rev()
                .find(|event| event.kind == ProviderEventKind::Terminal)
            {
                event.summary =
                    "Agent Run output rejected because its Tender revision is stale".into();
            }
        }
        let completed_at = sqlite_timestamp(&transaction)?;
        if let Some(candidate_count) = denied_record_publication_count {
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "tender_record_publication_denied",
                tender_revision,
                json!({
                    "candidate_record_count": candidate_count.to_string(),
                    "reason": "bid_decision_record_inventory_limit",
                    "run_id": prepared.run_id,
                }),
                &completed_at,
            )?;
        }
        if let Some(thread_ref) = execution.provider_thread_ref.as_deref() {
            transaction
                .execute(
                    "INSERT OR IGNORE INTO provider_threads (
                       profile_id, profile_version, thread_ref, status, created_at, archived_at
                     ) VALUES (?1, ?2, ?3, 'active', ?4, NULL)",
                    params![
                        prepared.profile.profile_id,
                        prepared.profile.version,
                        thread_ref,
                        completed_at,
                    ],
                )
                .map_err(sql_error)?;
            let stored_thread: String = transaction
                .query_row(
                    "SELECT thread_ref FROM provider_threads
                     WHERE profile_id = ?1 AND profile_version = ?2 AND status = 'active'",
                    params![prepared.profile.profile_id, prepared.profile.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if stored_thread != thread_ref {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }
        let usage_json = canonical_json(&execution.usage)?;
        let failure_json = execution.failure.as_ref().map(canonical_json).transpose()?;
        if transaction
            .execute(
                "UPDATE agent_runs
                 SET status = ?2,
                     provider_thread_ref = COALESCE(?3, provider_thread_ref),
                     provider_turn_ref = COALESCE(?4, provider_turn_ref),
                     usage_json = ?5, failure_json = ?6, completed_at = ?7
                 WHERE run_id = ?1 AND status = 'running'",
                params![
                    prepared.run_id,
                    execution.state.as_str(),
                    execution.provider_thread_ref,
                    execution.provider_turn_ref,
                    usage_json,
                    failure_json,
                    completed_at,
                ],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let mut sequence: u32 = transaction
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) FROM provider_events WHERE run_id = ?1",
                [&prepared.run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        for event in execution.events {
            if event.kind == ProviderEventKind::ControlRequestDenied {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            sequence = sequence
                .checked_add(1)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            insert_event(
                &transaction,
                &prepared.run_id,
                sequence,
                event,
                &completed_at,
            )?;
        }
        let production_payload = execution.candidate_payload_json.clone();
        let result_id = if let Some(payload_json) = execution.candidate_payload_json {
            ensure_canonical_value(&payload_json)?;
            let result_id = random_identifier(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO proposed_agent_results (
                       result_id, run_id, verification_status, payload_json,
                       data_scopes_json, data_classification, created_at
                     ) VALUES (?1, ?2, 'proposed', ?3, ?4, ?5, ?6)",
                    params![
                        result_id,
                        prepared.run_id,
                        payload_json,
                        canonical_json(&prepared.permission_grant.data_scopes)?,
                        highest_classification(&prepared.permission_grant)?.as_str(),
                        completed_at
                    ],
                )
                .map_err(sql_error)?;
            Some(result_id)
        } else {
            None
        };
        if let Some(candidate) = tender_record_candidate.as_ref() {
            if result_id.is_none() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            publish_tender_record_candidates(
                &transaction,
                tender_id,
                tender_revision,
                &prepared.run_id,
                candidate,
                &completed_at,
            )?;
        }
        if let Some(candidate) = tender_record_review.as_ref() {
            if result_id.is_none() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            publish_tender_record_review(
                &transaction,
                tender_id,
                tender_revision,
                &prepared.run_id,
                &prepared.task,
                candidate,
                &completed_at,
            )?;
        }
        if let Some(candidate) = bid_package_review.as_ref() {
            if result_id.is_none() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            publish_bid_decision_package_review(
                &transaction,
                tender_id,
                tender_revision,
                &prepared.run_id,
                &prepared.task,
                candidate,
                &completed_at,
            )?;
        }
        finish_production_task(
            &transaction,
            tender_id,
            &prepared.run_id,
            &prepared.task.task_id,
            execution.state,
            production_payload.as_deref(),
            &completed_at,
        )?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "agent_run_finished",
            tender_revision,
            json!({
                "provider_thread_ref": execution.provider_thread_ref,
                "provider_turn_ref": execution.provider_turn_ref,
                "result_id": result_id,
                "run_id": prepared.run_id,
                "status": execution.state.as_str(),
                "task_id": prepared.task.task_id,
            }),
            &completed_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn checkpoint_agent_thread(
        &mut self,
        prepared: &PreparedAgentRun,
        thread_ref: &str,
        resumed: bool,
    ) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO provider_threads (
                   profile_id, profile_version, thread_ref, status, created_at, archived_at
                 ) VALUES (?1, ?2, ?3, 'active', ?4, NULL)",
                params![
                    prepared.profile.profile_id,
                    prepared.profile.version,
                    thread_ref,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        let stored_thread: String = transaction
            .query_row(
                "SELECT thread_ref FROM provider_threads
                 WHERE profile_id = ?1 AND profile_version = ?2 AND status = 'active'",
                params![prepared.profile.profile_id, prepared.profile.version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if stored_thread != thread_ref
            || transaction
                .execute(
                    "UPDATE agent_runs SET provider_thread_ref = ?2
                     WHERE run_id = ?1 AND status = 'running'
                       AND (provider_thread_ref IS NULL OR provider_thread_ref = ?2)",
                    params![prepared.run_id, thread_ref],
                )
                .map_err(sql_error)?
                != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        transaction
            .execute(
                "INSERT INTO provider_thread_exposures (
                   thread_ref, run_id, exposure_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![
                    thread_ref,
                    prepared.run_id,
                    canonical_json(&prepared.permission_grant.thread_exposure)?,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        let sequence = next_provider_event_sequence(&transaction, &prepared.run_id)?;
        insert_event(
            &transaction,
            &prepared.run_id,
            sequence,
            PendingProviderEvent {
                kind: if resumed {
                    ProviderEventKind::ThreadResumed
                } else {
                    ProviderEventKind::ThreadEstablished
                },
                summary: if resumed {
                    "Provider Thread resumed".into()
                } else {
                    "Provider Thread established".into()
                },
                correlation_id: None,
                request_fingerprint: None,
                denial_reason: None,
                opaque_reference: Some(thread_ref.to_owned()),
            },
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn checkpoint_provider_thread_archived(
        &mut self,
        prepared: &PreparedAgentRun,
        thread_ref: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        if prepared.provider_thread_to_archive.as_deref() != Some(thread_ref)
            || prepared.provider_thread_ref.is_some()
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let archived_at = sqlite_timestamp(&transaction)?;
        if transaction
            .execute(
                "UPDATE provider_threads
                 SET status = 'archived', archived_at = ?4
                 WHERE thread_ref = ?1 AND profile_id = ?2 AND profile_version = ?3
                   AND status = 'archive_pending'",
                params![
                    thread_ref,
                    prepared.profile.profile_id,
                    prepared.profile.version,
                    archived_at,
                ],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        append_audit_event(
            &transaction,
            &tender_id,
            "provider_thread_archived",
            tender_revision,
            json!({
                "reason": "thread_exposure_incompatible",
                "run_id": prepared.run_id,
                "thread_ref": thread_ref,
            }),
            &archived_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn checkpoint_agent_turn(
        &mut self,
        run_id: &str,
        turn_ref: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        if transaction
            .execute(
                "UPDATE agent_runs SET provider_turn_ref = ?2
                 WHERE run_id = ?1 AND status = 'running' AND provider_turn_ref IS NULL",
                params![run_id, turn_ref],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let sequence = next_provider_event_sequence(&transaction, run_id)?;
        insert_event(
            &transaction,
            run_id,
            sequence,
            PendingProviderEvent {
                kind: ProviderEventKind::TurnStarted,
                summary: "Provider Turn started".into(),
                correlation_id: None,
                request_fingerprint: None,
                denial_reason: None,
                opaque_reference: Some(turn_ref.to_owned()),
            },
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn checkpoint_agent_turn_requested(
        &mut self,
        run_id: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let request_is_valid: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM agent_runs
                   WHERE run_id = ?1 AND status = 'running'
                     AND provider_thread_ref IS NOT NULL AND provider_turn_ref IS NULL
                 )",
                [run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !request_is_valid {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let sequence = next_provider_event_sequence(&transaction, run_id)?;
        insert_event(
            &transaction,
            run_id,
            sequence,
            PendingProviderEvent {
                kind: ProviderEventKind::TurnRequested,
                summary: "Provider Turn requested".into(),
                correlation_id: None,
                request_fingerprint: None,
                denial_reason: None,
                opaque_reference: None,
            },
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn checkpoint_agent_provider_event(
        &mut self,
        run_id: &str,
        event: &PendingProviderEvent,
        usage: &ProviderUsage,
    ) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        if !matches!(
            event.kind,
            ProviderEventKind::UsageObserved
                | ProviderEventKind::RateLimitObserved
                | ProviderEventKind::Warning
                | ProviderEventKind::Terminal
        ) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let usage_json = canonical_json(usage)?;
        if transaction
            .execute(
                "UPDATE agent_runs SET usage_json = ?2
                 WHERE run_id = ?1 AND status = 'running'",
                params![run_id, usage_json],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let sequence = next_provider_event_sequence(&transaction, run_id)?;
        insert_event(&transaction, run_id, sequence, event.clone(), &created_at)?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn checkpoint_agent_control_denial(
        &mut self,
        run_id: &str,
        event: &PendingProviderEvent,
    ) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        if event.kind != ProviderEventKind::ControlRequestDenied {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let correlation_id = event
            .correlation_id
            .as_deref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let request_fingerprint = event
            .request_fingerprint
            .as_deref()
            .filter(|value| value.len() == 64)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let denial_reason = event
            .denial_reason
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let method = event
            .opaque_reference
            .as_deref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let running: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_runs
                 WHERE run_id = ?1 AND status = 'running'
                   AND NOT EXISTS (
                     SELECT 1 FROM agent_run_cancellations
                     WHERE agent_run_cancellations.run_id = agent_runs.run_id
                   ))",
                [run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !running {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let sequence = next_provider_event_sequence(&transaction, run_id)?;
        insert_event(&transaction, run_id, sequence, event.clone(), &created_at)?;
        append_audit_event(
            &transaction,
            &tender_id,
            "provider_control_request_denied",
            tender_revision,
            json!({
                "correlation_id": correlation_id,
                "method": method,
                "reason": denial_reason.as_str(),
                "request_fingerprint": request_fingerprint,
                "run_id": run_id,
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn create_agent_access_request(
        &mut self,
        tender_id: &TenderId,
        command: RequestAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let running: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_runs
                 WHERE run_id = ?1 AND status = 'running'
                   AND NOT EXISTS (
                     SELECT 1 FROM agent_run_cancellations
                     WHERE agent_run_cancellations.run_id = agent_runs.run_id
                   ))",
                [&command.run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !running {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let request = AccessRequest {
            request_id: random_identifier(&transaction)?,
            run_id: command.run_id,
            exact_inputs: command.exact_inputs,
            data_scopes: command.data_scopes,
            data_classifications: command.data_classifications,
            allowed_actions: command.allowed_actions,
            allowed_tools: command.allowed_tools,
            purpose: command.purpose,
            recurring: command.recurring,
        };
        let requested_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO agent_access_requests (
                   request_id, run_id, request_json, status, requested_at
                 ) VALUES (?1, ?2, ?3, 'blocked', ?4)",
                params![
                    request.request_id,
                    request.run_id,
                    canonical_json(&request)?,
                    requested_at,
                ],
            )
            .map_err(sql_error)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "agent_access_requested",
            tender_revision,
            json!({
                "request_id": request.request_id,
                "run_id": request.run_id,
            }),
            &requested_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(AgentAccessRequestView {
            request,
            status: AgentAccessRequestStatus::Blocked,
            one_run_grant: None,
            denial_reason: None,
            requested_at,
            decided_at: None,
        })
    }

    pub(crate) fn approve_agent_access_request(
        &mut self,
        tender_id: &TenderId,
        command: ApproveAgentAccessCommand,
        run_is_active: bool,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let row: Option<(String, String, String, String, bool)> = transaction
            .query_row(
                "SELECT agent_access_requests.request_json, agent_access_requests.status,
                        agent_runs.permission_grant_json, agent_runs.status,
                        EXISTS(SELECT 1 FROM agent_run_cancellations
                               WHERE agent_run_cancellations.run_id = agent_runs.run_id)
                 FROM agent_access_requests
                 JOIN agent_runs ON agent_runs.run_id = agent_access_requests.run_id
                 WHERE agent_access_requests.request_id = ?1",
                [&command.request_id],
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
        let (request_json, status, grant_json, run_status, cancellation_requested) =
            row.ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        if status != "blocked" {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let request: AccessRequest = parse_canonical_json(&request_json)?;
        if request.run_id != command.run_id {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let grant: PermissionGrant = parse_canonical_json(&grant_json)?;
        let decided_at = sqlite_timestamp(&transaction)?;
        let approval = AccessApproval {
            approval_id: random_identifier(&transaction)?,
            request_id: request.request_id.clone(),
            run_id: request.run_id.clone(),
            approved_by: "engineer_user".into(),
            expires_at: command.expires_at,
        };
        let decision = if run_is_active && run_status == "running" && !cancellation_requested {
            approve_one_run_access(&grant, &request, &approval, &decided_at)
        } else {
            Err(crate::agent_runtime::PermissionDenialReason::GrantExpired)
        };
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let (view_status, one_run_grant, denial_reason) = match decision {
            Ok(one_run_grant) => {
                transaction
                    .execute(
                        "UPDATE agent_access_requests
                         SET status = 'approved', decision_json = ?2, decided_at = ?3
                         WHERE request_id = ?1 AND status = 'blocked'",
                        params![
                            request.request_id,
                            canonical_json(&one_run_grant)?,
                            decided_at,
                        ],
                    )
                    .map_err(sql_error)?;
                append_audit_event(
                    &transaction,
                    tender_id.as_str(),
                    "agent_access_approved",
                    tender_revision,
                    json!({
                        "approval_id": one_run_grant.approval_id,
                        "approved_by": one_run_grant.approved_by,
                        "expires_at": one_run_grant.expires_at,
                        "request_id": request.request_id,
                        "run_id": request.run_id,
                    }),
                    &decided_at,
                )?;
                (
                    AgentAccessRequestStatus::Approved,
                    Some(one_run_grant),
                    None,
                )
            }
            Err(reason) => {
                transaction
                    .execute(
                        "UPDATE agent_access_requests
                         SET status = 'denied', denial_reason = ?2, decided_at = ?3
                         WHERE request_id = ?1 AND status = 'blocked'",
                        params![request.request_id, reason.as_str(), decided_at],
                    )
                    .map_err(sql_error)?;
                append_audit_event(
                    &transaction,
                    tender_id.as_str(),
                    "agent_access_denied",
                    tender_revision,
                    json!({
                        "reason": reason.as_str(),
                        "request_id": request.request_id,
                        "run_id": request.run_id,
                    }),
                    &decided_at,
                )?;
                (AgentAccessRequestStatus::Denied, None, Some(reason))
            }
        };
        transaction.commit().map_err(sql_error)?;
        Ok(AgentAccessRequestView {
            request,
            status: view_status,
            one_run_grant,
            denial_reason,
            requested_at: load_access_requested_at(&self.connection, &command.request_id)?,
            decided_at: Some(decided_at),
        })
    }

    pub(crate) fn authorize_agent_typed_tool(
        &mut self,
        run_id: &str,
        correlation_id: &str,
        tool_name: &str,
    ) -> Result<bool, TenderCommandError> {
        self.require_change_intake_writable()?;
        let Some(definition) = bootstrap_tool_catalogue()
            .into_iter()
            .find(|tool| tool.name == tool_name)
        else {
            return Ok(false);
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let run_authority: Option<(String, String, String)> = transaction
            .query_row(
                "SELECT agent_runs.permission_grant_json,
                        agent_profile_versions.capabilities_json,
                        agent_runs.started_at
                 FROM agent_runs
                 JOIN agent_profile_versions
                   ON agent_profile_versions.profile_id = agent_runs.profile_id
                  AND agent_profile_versions.version = agent_runs.profile_version
                 WHERE agent_runs.run_id = ?1
                   AND agent_runs.status = 'running'
                   AND NOT EXISTS (
                     SELECT 1 FROM agent_run_cancellations
                     WHERE agent_run_cancellations.run_id = agent_runs.run_id
                   )",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((grant_json, capabilities_json, started_at)) = run_authority else {
            return Ok(false);
        };
        let now = sqlite_timestamp(&transaction)?;
        let now_timestamp: Timestamp = now
            .parse()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let started_at: Timestamp = started_at
            .parse()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let grant: PermissionGrant = parse_canonical_json(&grant_json)?;
        let profile_capabilities: Vec<String> = parse_canonical_json(&capabilities_json)?;
        let elapsed = Duration::try_from(started_at.duration_until(now_timestamp))
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let grant_expires_at: Timestamp = grant
            .expires_at
            .parse()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if now_timestamp >= grant_expires_at
            || elapsed >= Duration::from_secs(grant.resource_budget.duration_seconds.into())
        {
            return Ok(false);
        }
        let mut statement = transaction
            .prepare(
                "SELECT agent_access_requests.request_id,
                        agent_access_requests.decision_json
                 FROM agent_access_requests
                 WHERE agent_access_requests.run_id = ?1
                   AND agent_access_requests.status = 'approved'
                   AND NOT EXISTS (
                     SELECT 1 FROM agent_access_revocations
                     WHERE agent_access_revocations.request_id = agent_access_requests.request_id
                   )
                 ORDER BY agent_access_requests.rowid",
            )
            .map_err(sql_error)?;
        let grants = statement
            .query_map([run_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        drop(statement);
        let mut authorized_grant = None;
        for (request_id, supplement_json) in grants {
            let supplement: crate::agent_runtime::OneRunAccessGrant =
                parse_canonical_json(&supplement_json)?;
            let expires_at: Timestamp = supplement
                .expires_at
                .parse()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if supplement.request_id == request_id
                && supplement.run_id == run_id
                && expires_at > now_timestamp
                && one_run_grant_authorizes_tool(
                    &grant,
                    &profile_capabilities,
                    &supplement,
                    &definition,
                )
            {
                authorized_grant = Some(supplement);
                break;
            }
        }
        let Some(authorized_grant) = authorized_grant else {
            return Ok(false);
        };
        let inserted = transaction
            .execute(
                "INSERT INTO agent_tool_call_reservations (
                   run_id, correlation_id, tool_name, approval_id, authorized_at
                 )
                 SELECT ?1, ?2, ?3, ?4, ?5
                 WHERE (SELECT COUNT(*) FROM agent_tool_call_reservations
                        WHERE run_id = ?1 AND tool_name = ?3) < ?6
                 ON CONFLICT(run_id, correlation_id) DO NOTHING",
                params![
                    run_id,
                    correlation_id,
                    tool_name,
                    authorized_grant.approval_id,
                    now,
                    definition.quota.maximum_calls,
                ],
            )
            .map_err(sql_error)?;
        if inserted != 1 {
            return Ok(false);
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
            "agent_typed_tool_authorized",
            tender_revision,
            json!({
                "approval_id": authorized_grant.approval_id,
                "correlation_id": correlation_id,
                "run_id": run_id,
                "tool_name": tool_name,
                "tool_version": definition.version,
            }),
            &now,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(true)
    }

    pub(crate) fn record_agent_typed_tool_execution(
        &mut self,
        run_id: &str,
        correlation_id: &str,
        tool_name: &str,
        succeeded: bool,
    ) -> Result<(), TenderCommandError> {
        self.require_change_intake_writable()?;
        let definition = bootstrap_tool_catalogue()
            .into_iter()
            .find(|tool| tool.name == tool_name)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let completed_at = sqlite_timestamp(&transaction)?;
        if transaction
            .execute(
                "INSERT INTO agent_tool_call_results (
                   run_id, correlation_id, outcome, completed_at
                 )
                 SELECT ?1, ?2, ?3, ?4
                 WHERE EXISTS(
                   SELECT 1 FROM agent_tool_call_reservations
                   WHERE run_id = ?1 AND correlation_id = ?2 AND tool_name = ?5
                 )
                 ON CONFLICT(run_id, correlation_id) DO NOTHING",
                params![
                    run_id,
                    correlation_id,
                    if succeeded { "succeeded" } else { "failed" },
                    completed_at,
                    tool_name,
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
        let sequence = next_provider_event_sequence(&transaction, run_id)?;
        insert_event(
            &transaction,
            run_id,
            sequence,
            PendingProviderEvent {
                kind: ProviderEventKind::ControlRequestResolved,
                summary: if succeeded {
                    "Host Tool Call completed"
                } else {
                    "Host Tool Call failed"
                }
                .into(),
                correlation_id: Some(correlation_id.into()),
                request_fingerprint: None,
                denial_reason: None,
                opaque_reference: Some(tool_name.into()),
            },
            &completed_at,
        )?;
        append_audit_event(
            &transaction,
            &tender_id,
            if succeeded {
                &definition.audit_event_type
            } else {
                "agent_typed_tool_failed"
            },
            tender_revision,
            json!({
                "correlation_id": correlation_id,
                "run_id": run_id,
                "tool_name": tool_name,
                "tool_version": definition.version,
            }),
            &completed_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn resolve_indeterminate_agent_run(
        &mut self,
        tender_id: &TenderId,
        command: ResolveIndeterminateAgentRunCommand,
    ) -> Result<AgentRunRecoveryDecision, TenderCommandError> {
        self.require_storage_writable()?;
        let rationale = command.rationale.trim().to_owned();
        if command.disposition == AgentRunRecoveryDisposition::RetryTask
            && production_task_for_run(&self.connection, &command.run_id)?.is_some()
        {
            let plan_basis: Option<(String, u32)> = self
                .connection
                .query_row(
                    "SELECT activations.plan_id, activations.plan_version
                     FROM production_task_attempts AS attempts
                     JOIN production_activations AS activations
                       ON activations.activation_id = (
                         SELECT activation_id FROM production_tasks
                         WHERE production_task_id = attempts.production_task_id
                       )
                     JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                     WHERE runs.run_id = ?1",
                    [&command.run_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (plan_id, plan_version) = plan_basis
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if !work_plan_package_dependencies_are_current(self, &plan_id, plan_version)? {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let task_id: Option<String> = transaction
            .query_row(
                "SELECT task_id FROM agent_runs
                 WHERE run_id = ?1 AND status = 'indeterminate'
                   AND NOT EXISTS (
                     SELECT 1 FROM agent_run_recovery_dispositions
                     WHERE agent_run_recovery_dispositions.run_id = agent_runs.run_id
                   )",
                [&command.run_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let task_id =
            task_id.ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let profile = load_profile(&transaction, task_profile(&transaction, &task_id)?)?;
        if command.disposition == AgentRunRecoveryDisposition::RetryTask
            && !profile_supports_linked_retry(&profile)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let decided_at = sqlite_timestamp(&transaction)?;
        if transaction
            .execute(
                "INSERT INTO agent_run_recovery_dispositions (
                   run_id, disposition, rationale, decided_by, decided_at
                 ) VALUES (?1, ?2, ?3, 'engineer_user', ?4)",
                params![
                    command.run_id,
                    command.disposition.as_str(),
                    rationale,
                    decided_at
                ],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "indeterminate_agent_run_resolved",
            tender_revision,
            json!({
                "decided_by": "engineer_user",
                "disposition": command.disposition.as_str(),
                "rationale": rationale,
                "run_id": command.run_id,
                "task_id": task_id,
            }),
            &decided_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(AgentRunRecoveryDecision {
            run_id: command.run_id,
            disposition: command.disposition,
            rationale,
            decided_by: "engineer_user".into(),
            decided_at,
        })
    }

    pub(crate) fn resolve_agent_access_request(
        &mut self,
        tender_id: &TenderId,
        command: ResolveAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let row: Option<(String, String, String)> = transaction
            .query_row(
                "SELECT request_json, requested_at, status FROM agent_access_requests
                 WHERE request_id = ?1",
                [&command.request_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let (request_json, requested_at, current_status) =
            row.ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let request: AccessRequest = parse_canonical_json(&request_json)?;
        if request.run_id != command.run_id {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let decided_at = sqlite_timestamp(&transaction)?;
        let (stored_status, denial_reason, event_type, view_status) = match command.resolution {
            crate::agent_runtime::AgentAccessResolution::Deny if current_status == "blocked" => (
                "denied",
                crate::agent_runtime::PermissionDenialReason::EngineerDenied,
                "agent_access_denied",
                AgentAccessRequestStatus::Denied,
            ),
            crate::agent_runtime::AgentAccessResolution::Supersede
                if current_status == "blocked" =>
            {
                (
                    "superseded",
                    crate::agent_runtime::PermissionDenialReason::Superseded,
                    "agent_access_superseded",
                    AgentAccessRequestStatus::Superseded,
                )
            }
            crate::agent_runtime::AgentAccessResolution::Revoke if current_status == "approved" => {
                if transaction
                    .execute(
                        "INSERT INTO agent_access_revocations (
                           request_id, reason, revoked_by, revoked_at
                         ) VALUES (?1, 'engineer_revoked', 'engineer_user', ?2)
                         ON CONFLICT(request_id) DO NOTHING",
                        params![request.request_id, decided_at],
                    )
                    .map_err(sql_error)?
                    != 1
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                (
                    "approved",
                    crate::agent_runtime::PermissionDenialReason::AccessRevoked,
                    "agent_access_revoked",
                    AgentAccessRequestStatus::Revoked,
                )
            }
            _ => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        };
        if view_status != AgentAccessRequestStatus::Revoked
            && transaction
                .execute(
                    "UPDATE agent_access_requests
                     SET status = ?2, denial_reason = ?3, decided_at = ?4
                     WHERE request_id = ?1 AND status = 'blocked'",
                    params![
                        request.request_id,
                        stored_status,
                        denial_reason.as_str(),
                        decided_at
                    ],
                )
                .map_err(sql_error)?
                != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            event_type,
            tender_revision,
            json!({
                "decided_by": "engineer_user",
                "request_id": request.request_id,
                "run_id": request.run_id,
            }),
            &decided_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(AgentAccessRequestView {
            request,
            status: view_status,
            one_run_grant: None,
            denial_reason: Some(denial_reason),
            requested_at,
            decided_at: Some(decided_at),
        })
    }

    pub(crate) fn request_agent_run_interruption(
        &mut self,
        tender_id: &TenderId,
        run_id: &str,
    ) -> Result<bool, TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let running: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_runs
                 WHERE run_id = ?1 AND status = 'running')",
                [run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !running {
            return Ok(false);
        }
        let requested_at = sqlite_timestamp(&transaction)?;
        let inserted = transaction
            .execute(
                "INSERT INTO agent_run_cancellations (run_id, requested_by, requested_at)
                 VALUES (?1, 'engineer_user', ?2)
                 ON CONFLICT(run_id) DO NOTHING",
                params![run_id, requested_at],
            )
            .map_err(sql_error)?;
        if inserted == 0 {
            return Ok(true);
        }
        let revoked: u32 = transaction
            .execute(
                "INSERT INTO agent_access_revocations (
                   request_id, reason, revoked_by, revoked_at
                 )
                 SELECT request_id, 'run_interrupted', 'engineer_user', ?2
                 FROM agent_access_requests
                 WHERE run_id = ?1 AND status = 'approved'
                 ON CONFLICT(request_id) DO NOTHING",
                params![run_id, requested_at],
            )
            .map_err(sql_error)?
            .try_into()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "agent_run_interruption_requested",
            tender_revision,
            json!({
                "requested_by": "engineer_user",
                "revoked_access_grants": revoked,
                "run_id": run_id,
            }),
            &requested_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(true)
    }

    pub(crate) fn agent_run_cancellation_requested(
        &self,
        run_id: &str,
    ) -> Result<bool, TenderCommandError> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_run_cancellations WHERE run_id = ?1)",
                [run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn inspect_agent_runs(&self) -> Result<Vec<AgentRunInspection>, TenderCommandError> {
        self.inspect_agent_runs_with_check(&mut || Ok(()))
    }

    pub(crate) fn inspect_agent_run_history(
        &self,
        before_sequence: Option<u64>,
        limit: u32,
    ) -> Result<AgentRunHistoryPage, TenderCommandError> {
        if !(1..=4).contains(&limit) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let before_sequence = before_sequence
            .map(i64::try_from)
            .transpose()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let total_count: i64 = self
            .connection
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
            .map_err(sql_error)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT run_sequence, run_id
                 FROM agent_runs
                 WHERE (?1 IS NULL OR run_sequence < ?1)
                 ORDER BY run_sequence DESC
                 LIMIT ?2",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![before_sequence, i64::from(limit) + 1], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut references = Vec::with_capacity(limit as usize + 1);
        for row in rows {
            let (sequence, run_id) = row.map_err(sql_error)?;
            references.push((
                u64::try_from(sequence)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                run_id,
            ));
        }
        let has_older = references.len() > limit as usize;
        references.truncate(limit as usize);
        let next_before_sequence = has_older
            .then(|| references.last().map(|(sequence, _)| *sequence))
            .flatten();
        let items = references
            .into_iter()
            .map(|(run_sequence, run_id)| {
                Ok(AgentRunHistoryItem {
                    run_sequence,
                    run: self.inspect_agent_run_summary(&run_id)?,
                })
            })
            .collect::<Result<Vec<_>, TenderCommandError>>()?;
        Ok(AgentRunHistoryPage {
            items,
            next_before_sequence,
            total_count: u64::try_from(total_count)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        })
    }

    pub(crate) fn inspect_agent_run_activity(
        &self,
    ) -> Result<AgentRunActivity, TenderCommandError> {
        let (run_count, event_count, running_count): (i64, i64, i64) = self
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM agent_runs),
                   (SELECT COUNT(*) FROM provider_events),
                   (SELECT COUNT(*) FROM agent_runs WHERE status = 'running')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        Ok(AgentRunActivity {
            run_count: u64::try_from(run_count)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            event_count: u64::try_from(event_count)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            running_count: u64::try_from(running_count)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        })
    }

    fn inspect_agent_run_summary(
        &self,
        run_id: &str,
    ) -> Result<AgentRunSummary, TenderCommandError> {
        let row: Option<RawAgentRun> = self
            .connection
            .query_row(
                "SELECT task_id, profile_id, profile_version, retry_of_run_id,
                        permission_grant_json, status,
                        provider_thread_ref, provider_turn_ref, usage_json, failure_json,
                        started_at, completed_at
                 FROM agent_runs WHERE run_id = ?1",
                [run_id],
                |row| {
                    Ok(RawAgentRun {
                        task_id: row.get(0)?,
                        profile_id: row.get(1)?,
                        profile_version: row.get(2)?,
                        retry_of_run_id: row.get(3)?,
                        permission_grant_json: row.get(4)?,
                        status: row.get(5)?,
                        provider_thread_ref: row.get(6)?,
                        provider_turn_ref: row.get(7)?,
                        usage_json: row.get(8)?,
                        failure_json: row.get(9)?,
                        started_at: row.get(10)?,
                        completed_at: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(sql_error)?;
        let row = row.ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        let state = AgentRunState::parse(&row.status)?;
        let profile = load_profile(
            &self.connection,
            (row.profile_id.clone(), row.profile_version),
        )?;
        let usage = row
            .usage_json
            .as_deref()
            .map(parse_canonical_json)
            .transpose()?
            .unwrap_or_default();
        let failure = row
            .failure_json
            .as_deref()
            .map(parse_canonical_json)
            .transpose()?;
        let (has_proposed_result, has_linked_retry): (bool, bool) = self
            .connection
            .query_row(
                "SELECT
                   EXISTS(SELECT 1 FROM proposed_agent_results WHERE run_id = ?1),
                   EXISTS(SELECT 1 FROM agent_runs WHERE retry_of_run_id = ?1)",
                [run_id],
                |result| Ok((result.get(0)?, result.get(1)?)),
            )
            .map_err(sql_error)?;
        if (state == AgentRunState::Completed) != has_proposed_result
            || (state == AgentRunState::Running
                && (failure.is_some() || row.completed_at.is_some()))
            || (state != AgentRunState::Running && row.completed_at.is_none())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let recovery_decision = self
            .connection
            .query_row(
                "SELECT disposition, rationale, decided_by, decided_at
                 FROM agent_run_recovery_dispositions WHERE run_id = ?1",
                [run_id],
                |decision| {
                    Ok((
                        decision.get::<_, String>(0)?,
                        decision.get::<_, String>(1)?,
                        decision.get::<_, String>(2)?,
                        decision.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?
            .map(|(disposition, rationale, decided_by, decided_at)| {
                Ok(AgentRunRecoveryDecision {
                    run_id: run_id.to_owned(),
                    disposition: AgentRunRecoveryDisposition::parse(&disposition)?,
                    rationale,
                    decided_by,
                    decided_at,
                })
            })
            .transpose()?;
        if recovery_decision.is_some() && state != AgentRunState::Indeterminate {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let linked_retry_supported = profile_supports_linked_retry(&profile);
        if recovery_decision.as_ref().is_some_and(|decision| {
            decision.disposition == AgentRunRecoveryDisposition::RetryTask
                && !linked_retry_supported
        }) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(AgentRunSummary {
            run_id: run_id.to_owned(),
            retry_of_run_id: row.retry_of_run_id,
            has_linked_retry,
            linked_retry_supported,
            state,
            profile_identity: profile.identity,
            profile_profession: profile.profession,
            profile_version: profile.version,
            task_id: row.task_id,
            provider_thread_ref: row.provider_thread_ref,
            provider_turn_ref: row.provider_turn_ref,
            usage,
            failure,
            has_proposed_result,
            recovery_decision,
            started_at: row.started_at,
            completed_at: row.completed_at,
        })
    }

    pub(crate) fn inspect_agent_runs_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<Vec<AgentRunInspection>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare("SELECT run_id FROM agent_runs ORDER BY run_sequence")
            .map_err(sql_error)?;
        let run_ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut runs = Vec::new();
        for run_id in run_ids {
            check()?;
            if runs.len() >= MAX_AGENT_RUNS_PER_TENDER {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            runs.push(self.inspect_agent_run_with_check(&run_id.map_err(sql_error)?, check)?);
        }
        check()?;
        Ok(runs)
    }

    pub(crate) fn agent_run_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare("SELECT run_id FROM agent_runs ORDER BY run_sequence")
            .map_err(sql_error)?;
        let mut run_ids = statement.query([]).map_err(sql_error)?;
        let mut run_count = 0_usize;
        while let Some(row) = run_ids.next().map_err(sql_error)? {
            check()?;
            run_count = run_count
                .checked_add(1)
                .filter(|count| *count <= MAX_AGENT_RUNS_PER_TENDER)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let run_id = row.get::<_, String>(0).map_err(sql_error)?;
            if let Err(error) = self.inspect_agent_run_with_check(&run_id, check) {
                if error.code == TenderErrorCode::OperationTimedOut {
                    return Err(error);
                }
                return Ok(false);
            }
        }
        check()?;
        Ok(true)
    }

    pub(crate) fn inspect_agent_run(
        &self,
        run_id: &str,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        self.inspect_agent_run_with_check(run_id, &mut || Ok(()))
    }

    pub(crate) fn inspect_agent_run_with_check(
        &self,
        run_id: &str,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        check()?;
        let row: Option<RawAgentRun> = self
            .connection
            .query_row(
                "SELECT task_id, profile_id, profile_version, retry_of_run_id,
                        permission_grant_json, status,
                        provider_thread_ref, provider_turn_ref, usage_json, failure_json,
                        started_at, completed_at
                 FROM agent_runs WHERE run_id = ?1",
                [run_id],
                |row| {
                    Ok(RawAgentRun {
                        task_id: row.get(0)?,
                        profile_id: row.get(1)?,
                        profile_version: row.get(2)?,
                        retry_of_run_id: row.get(3)?,
                        permission_grant_json: row.get(4)?,
                        status: row.get(5)?,
                        provider_thread_ref: row.get(6)?,
                        provider_turn_ref: row.get(7)?,
                        usage_json: row.get(8)?,
                        failure_json: row.get(9)?,
                        started_at: row.get(10)?,
                        completed_at: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(sql_error)?;
        let row = row.ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        let profile = load_profile(
            &self.connection,
            (row.profile_id.clone(), row.profile_version),
        )?;
        let task = load_task(&self.connection, &row.task_id)?;
        let permission_grant: PermissionGrant = parse_canonical_json(&row.permission_grant_json)?;
        if task.profile_id != profile.profile_id || task.profile_version != profile.version {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let events = load_events_with_check(&self.connection, run_id, check)?;
        let usage = row
            .usage_json
            .as_deref()
            .map(parse_canonical_json)
            .transpose()?
            .unwrap_or_default();
        let failure = row
            .failure_json
            .as_deref()
            .map(parse_canonical_json)
            .transpose()?;
        let proposed_result = self
            .connection
            .query_row(
                "SELECT result_id, verification_status, payload_json,
                        data_scopes_json, data_classification
                 FROM proposed_agent_results WHERE run_id = ?1",
                [run_id],
                |result| {
                    Ok((
                        result.get::<_, String>(0)?,
                        result.get::<_, String>(1)?,
                        result.get::<_, String>(2)?,
                        result.get::<_, String>(3)?,
                        result.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?
            .map(
                |(result_id, status, payload_json, data_scopes_json, data_classification)| {
                    if status != "proposed" {
                        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                    }
                    ensure_canonical_value(&payload_json)?;
                    Ok(ProposedAgentResult {
                        result_id,
                        verification_status: VerificationStatus::Proposed,
                        payload_json,
                        data_scopes: parse_canonical_json(&data_scopes_json)?,
                        data_classification: DataClassification::parse(&data_classification)?,
                    })
                },
            )
            .transpose()?;
        let state = AgentRunState::parse(&row.status)?;
        if (state == AgentRunState::Completed) != proposed_result.is_some()
            || (state == AgentRunState::Running
                && (failure.is_some() || row.completed_at.is_some()))
            || (state != AgentRunState::Running && row.completed_at.is_none())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let access_requests =
            load_access_requests(&self.connection, run_id, state == AgentRunState::Running)?;
        let recovery_decision = self
            .connection
            .query_row(
                "SELECT disposition, rationale, decided_by, decided_at
                 FROM agent_run_recovery_dispositions WHERE run_id = ?1",
                [run_id],
                |decision| {
                    Ok((
                        decision.get::<_, String>(0)?,
                        decision.get::<_, String>(1)?,
                        decision.get::<_, String>(2)?,
                        decision.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?
            .map(|(disposition, rationale, decided_by, decided_at)| {
                Ok(AgentRunRecoveryDecision {
                    run_id: run_id.to_owned(),
                    disposition: AgentRunRecoveryDisposition::parse(&disposition)?,
                    rationale,
                    decided_by,
                    decided_at,
                })
            })
            .transpose()?;
        if recovery_decision.is_some() && state != AgentRunState::Indeterminate {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let linked_retry_supported = profile_supports_linked_retry(&profile);
        if recovery_decision.as_ref().is_some_and(|decision| {
            decision.disposition == AgentRunRecoveryDisposition::RetryTask
                && !linked_retry_supported
        }) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        check()?;
        Ok(AgentRunInspection {
            run_id: run_id.to_owned(),
            retry_of_run_id: row.retry_of_run_id,
            linked_retry_supported,
            state,
            profile,
            task,
            permission_grant,
            access_requests,
            provider_thread_ref: row.provider_thread_ref,
            provider_turn_ref: row.provider_turn_ref,
            events,
            usage,
            failure,
            proposed_result,
            recovery_decision,
            started_at: row.started_at,
            completed_at: row.completed_at,
        })
    }

    pub(super) fn reconcile_interrupted_agent_runs(
        &mut self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let runs = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT agent_runs.run_id, agent_runs.provider_turn_ref,
                            EXISTS(
                              SELECT 1 FROM provider_events
                              WHERE provider_events.run_id = agent_runs.run_id
                                AND provider_events.kind = 'turn_requested'
                            )
                     FROM agent_runs
                     WHERE status = 'running' ORDER BY run_sequence",
                )
                .map_err(sql_error)?;
            let runs = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, bool>(2)?,
                    ))
                })
                .map_err(sql_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sql_error)?;
            runs
        };
        if runs.is_empty() {
            return Ok(());
        }
        for (run_id, turn_ref, turn_requested) in &runs {
            if run_id.len() != 32 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let application_home = self
                .root
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let workspace = agent_workspace_path(application_home, tender_id.as_str(), run_id);
            dispose_workspace(
                &self.root,
                &workspace,
                run_id,
                if turn_ref.is_some() || *turn_requested {
                    AgentRunState::Indeterminate
                } else {
                    AgentRunState::Failed
                },
            )?;
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
        let completed_at = sqlite_timestamp(&transaction)?;
        let usage_json = canonical_json(&ProviderUsage::default())?;
        for (run_id, turn_ref, turn_requested) in runs {
            let outcome_uncertain = turn_ref.is_some() || turn_requested;
            let state = if outcome_uncertain {
                AgentRunState::Indeterminate
            } else {
                AgentRunState::Failed
            };
            let failure = if outcome_uncertain {
                ProviderFailure::new(
                    ProviderFailureCategory::OutcomeUnknown,
                    false,
                    "Resolve the quarantined Agent Run before retrying.",
                    Some("The Host restarted after Provider Turn dispatch began but before its acceptance or outcome was established."),
                )
            } else {
                ProviderFailure::new(
                    ProviderFailureCategory::ProcessFailed,
                    true,
                    "Retry the Agent Run because no Provider Turn was accepted.",
                    Some("The Host restarted before the Provider Turn was accepted."),
                )
            };
            let failure_json = canonical_json(&failure)?;
            if transaction
                .execute(
                    "UPDATE agent_runs
                     SET status = ?2, usage_json = COALESCE(usage_json, ?3), failure_json = ?4,
                         completed_at = ?5
                     WHERE run_id = ?1 AND status = 'running'",
                    params![
                        run_id,
                        state.as_str(),
                        usage_json,
                        failure_json,
                        completed_at
                    ],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let sequence = next_provider_event_sequence(&transaction, &run_id)?;
            insert_event(
                &transaction,
                &run_id,
                sequence,
                PendingProviderEvent {
                    kind: ProviderEventKind::Terminal,
                    summary: if outcome_uncertain {
                        "Agent Run outcome became indeterminate after Host restart".into()
                    } else {
                        "Agent Run failed safely before Provider Turn acceptance".into()
                    },
                    correlation_id: None,
                    request_fingerprint: None,
                    denial_reason: None,
                    opaque_reference: turn_ref,
                },
                &completed_at,
            )?;
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                if outcome_uncertain {
                    "agent_run_indeterminate"
                } else {
                    "agent_run_failed"
                },
                tender_revision,
                json!({ "reason": "host_restart", "run_id": run_id }),
                &completed_at,
            )?;
        }
        transaction.commit().map_err(sql_error)
    }
}

struct RawAgentRun {
    task_id: String,
    profile_id: String,
    profile_version: u32,
    retry_of_run_id: Option<String>,
    permission_grant_json: String,
    status: String,
    provider_thread_ref: Option<String>,
    provider_turn_ref: Option<String>,
    usage_json: Option<String>,
    failure_json: Option<String>,
    started_at: String,
    completed_at: Option<String>,
}

struct RawProfileVersion {
    identity: String,
    profession: String,
    seniority: String,
    capabilities_json: String,
    objective: String,
    behavior: String,
    skepticism: String,
    risk_tolerance: String,
    instructions: String,
    output_contract_json: String,
    review_policy: String,
    permissions_json: String,
    prohibited_actions_json: String,
    resource_budget_json: String,
}

struct RawTenderTask {
    profile_id: String,
    profile_version: u32,
    objective: String,
    exact_inputs_json: String,
    output_contract_json: String,
    review_policy: String,
    deadline: String,
    permissions_json: String,
    resource_budget_json: String,
}

pub(super) fn insert_profile(
    transaction: &rusqlite::Transaction<'_>,
    stable_identity: &str,
    profile: &AgentProfileVersionView,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO agent_profiles (profile_id, stable_identity, created_at)
             VALUES (?1, ?2, ?3)",
            params![profile.profile_id, stable_identity, created_at],
        )
        .map_err(sql_error)?;
    insert_profile_version(transaction, profile, created_at)?;
    transaction
        .execute(
            "INSERT INTO agent_profile_heads (profile_id, current_version, status)
             VALUES (?1, ?2, ?3)",
            params![
                profile.profile_id,
                profile.version,
                AgentProfileStatus::Active.as_str()
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub(super) fn insert_profile_version(
    transaction: &rusqlite::Transaction<'_>,
    profile: &AgentProfileVersionView,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO agent_profile_versions (
               profile_id, version, identity, profession, seniority, capabilities_json,
               objective, behavior, skepticism, risk_tolerance, instructions,
               output_contract_json, review_policy, permissions_json,
               prohibited_actions_json, resource_budget_json, created_at
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
               ?15, ?16, ?17
             )",
            params![
                profile.profile_id,
                profile.version,
                profile.identity,
                profile.profession,
                profile.seniority,
                canonical_json(&profile.capabilities)?,
                profile.objective,
                profile.behavior,
                profile.skepticism,
                profile.risk_tolerance,
                profile.instructions,
                profile.output_contract_json,
                profile.review_policy,
                canonical_json(&profile.permissions)?,
                canonical_json(&profile.prohibited_actions)?,
                canonical_json(&profile.resource_budget)?,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub(super) fn update_profile_head(
    transaction: &rusqlite::Transaction<'_>,
    profile_id: &str,
    version: u32,
    status: AgentProfileStatus,
) -> Result<(), TenderCommandError> {
    if transaction
        .execute(
            "UPDATE agent_profile_heads
             SET current_version = ?2, status = ?3
             WHERE profile_id = ?1 AND current_version <= ?2",
            params![profile_id, version, status.as_str()],
        )
        .map_err(sql_error)?
        == 1
        || transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM agent_profile_heads
                   WHERE profile_id = ?1 AND current_version > ?2
                 )",
                params![profile_id, version],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?
    {
        Ok(())
    } else {
        Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed))
    }
}

pub(super) fn insert_task(
    transaction: &rusqlite::Transaction<'_>,
    task: &TenderTaskView,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO tender_tasks (
               task_id, profile_id, profile_version, objective, exact_inputs_json,
               output_contract_json, review_policy, deadline, permissions_json,
               resource_budget_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task.task_id,
                task.profile_id,
                task.profile_version,
                task.objective,
                canonical_json(&task.exact_inputs)?,
                task.output_contract_json,
                task.review_policy,
                task.deadline,
                canonical_json(&task.permissions)?,
                canonical_json(&task.resource_budget)?,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn task_profile(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<(String, u32), TenderCommandError> {
    connection
        .query_row(
            "SELECT profile_id, profile_version FROM tender_tasks WHERE task_id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)
}

fn exact_tender_revision(
    task: &TenderTaskView,
    tender_id: &TenderId,
) -> Result<u32, TenderCommandError> {
    match task.exact_inputs.as_slice() {
        [input]
            if input.kind == "tender_revision"
                && input.reference == tender_id.as_str()
                && input.version > 0 =>
        {
            Ok(input.version)
        }
        _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
    }
}

pub(super) fn load_profile(
    connection: &rusqlite::Connection,
    key: (String, u32),
) -> Result<AgentProfileVersionView, TenderCommandError> {
    let raw: Option<RawProfileVersion> = connection
        .query_row(
            "SELECT identity, profession, seniority, capabilities_json, objective,
                    behavior, skepticism, risk_tolerance, instructions,
                    output_contract_json, review_policy, permissions_json,
                    prohibited_actions_json, resource_budget_json
             FROM agent_profile_versions WHERE profile_id = ?1 AND version = ?2",
            params![key.0, key.1],
            |row| {
                Ok(RawProfileVersion {
                    identity: row.get(0)?,
                    profession: row.get(1)?,
                    seniority: row.get(2)?,
                    capabilities_json: row.get(3)?,
                    objective: row.get(4)?,
                    behavior: row.get(5)?,
                    skepticism: row.get(6)?,
                    risk_tolerance: row.get(7)?,
                    instructions: row.get(8)?,
                    output_contract_json: row.get(9)?,
                    review_policy: row.get(10)?,
                    permissions_json: row.get(11)?,
                    prohibited_actions_json: row.get(12)?,
                    resource_budget_json: row.get(13)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)?;
    let raw = raw.ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    ensure_canonical_value(&raw.output_contract_json)?;
    Ok(AgentProfileVersionView {
        profile_id: key.0,
        version: key.1,
        identity: raw.identity,
        profession: raw.profession,
        seniority: raw.seniority,
        capabilities: parse_canonical_json(&raw.capabilities_json)?,
        objective: raw.objective,
        behavior: raw.behavior,
        skepticism: raw.skepticism,
        risk_tolerance: raw.risk_tolerance,
        instructions: raw.instructions,
        output_contract_json: raw.output_contract_json,
        review_policy: raw.review_policy,
        permissions: parse_canonical_json(&raw.permissions_json)?,
        prohibited_actions: parse_canonical_json(&raw.prohibited_actions_json)?,
        resource_budget: parse_canonical_json(&raw.resource_budget_json)?,
    })
}

pub(super) fn load_task(
    connection: &rusqlite::Connection,
    task_id: &str,
) -> Result<TenderTaskView, TenderCommandError> {
    let raw: Option<RawTenderTask> = connection
        .query_row(
            "SELECT profile_id, profile_version, objective, exact_inputs_json,
                    output_contract_json, review_policy, deadline, permissions_json,
                    resource_budget_json
             FROM tender_tasks WHERE task_id = ?1",
            [task_id],
            |row| {
                Ok(RawTenderTask {
                    profile_id: row.get(0)?,
                    profile_version: row.get(1)?,
                    objective: row.get(2)?,
                    exact_inputs_json: row.get(3)?,
                    output_contract_json: row.get(4)?,
                    review_policy: row.get(5)?,
                    deadline: row.get(6)?,
                    permissions_json: row.get(7)?,
                    resource_budget_json: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)?;
    let raw = raw.ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    ensure_canonical_value(&raw.output_contract_json)?;
    Ok(TenderTaskView {
        task_id: task_id.to_owned(),
        profile_id: raw.profile_id,
        profile_version: raw.profile_version,
        objective: raw.objective,
        exact_inputs: parse_canonical_json(&raw.exact_inputs_json)?,
        output_contract_json: raw.output_contract_json,
        review_policy: raw.review_policy,
        deadline: raw.deadline,
        permissions: parse_canonical_json(&raw.permissions_json)?,
        resource_budget: parse_canonical_json(&raw.resource_budget_json)?,
    })
}

fn load_events_with_check(
    connection: &rusqlite::Connection,
    run_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<ProviderEvent>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT sequence, kind, summary, correlation_id, request_fingerprint,
                    denial_reason, opaque_reference,
                    length(CAST(kind AS BLOB)), length(CAST(summary AS BLOB)),
                    COALESCE(length(CAST(correlation_id AS BLOB)), 0),
                    COALESCE(length(CAST(request_fingerprint AS BLOB)), 0),
                    COALESCE(length(CAST(denial_reason AS BLOB)), 0),
                    COALESCE(length(CAST(opaque_reference AS BLOB)), 0)
             FROM provider_events WHERE run_id = ?1 ORDER BY sequence",
        )
        .map_err(sql_error)?;
    let mut rows = statement.query([run_id]).map_err(sql_error)?;
    let mut expected = 1_u32;
    let mut events = Vec::new();
    let mut total_bytes = 0_u64;
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        if events.len() >= MAX_PROVIDER_EVENTS_PER_RUN {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let mut row_bytes = 0_u64;
        for index in 7..13 {
            let field_bytes = u64::try_from(row.get::<_, i64>(index).map_err(sql_error)?)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if field_bytes > MAX_PROVIDER_EVENT_FIELD_BYTES {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            row_bytes = row_bytes
                .checked_add(field_bytes)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        }
        total_bytes = total_bytes
            .checked_add(row_bytes)
            .filter(|total| *total <= MAX_PROVIDER_EVENT_BYTES_PER_RUN)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let sequence = row.get::<_, u32>(0).map_err(sql_error)?;
        let kind = row.get::<_, String>(1).map_err(sql_error)?;
        let summary = row.get::<_, String>(2).map_err(sql_error)?;
        let correlation_id = row.get::<_, Option<String>>(3).map_err(sql_error)?;
        let request_fingerprint = row.get::<_, Option<String>>(4).map_err(sql_error)?;
        let denial_reason = row.get::<_, Option<String>>(5).map_err(sql_error)?;
        let opaque_reference = row.get::<_, Option<String>>(6).map_err(sql_error)?;
        if sequence != expected {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        expected = expected
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        events.push(ProviderEvent {
            sequence,
            kind: ProviderEventKind::parse(&kind)?,
            summary,
            correlation_id,
            request_fingerprint,
            denial_reason: denial_reason
                .as_deref()
                .map(crate::agent_runtime::PermissionDenialReason::parse)
                .transpose()?,
            opaque_reference,
        });
    }
    check()?;
    Ok(events)
}

fn load_access_requested_at(
    connection: &rusqlite::Connection,
    request_id: &str,
) -> Result<String, TenderCommandError> {
    connection
        .query_row(
            "SELECT requested_at FROM agent_access_requests WHERE request_id = ?1",
            [request_id],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn load_access_requests(
    connection: &rusqlite::Connection,
    run_id: &str,
    run_is_active: bool,
) -> Result<Vec<AgentAccessRequestView>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT agent_access_requests.request_json,
                    agent_access_requests.status,
                    agent_access_requests.decision_json,
                    agent_access_requests.denial_reason,
                    agent_access_requests.requested_at,
                    agent_access_requests.decided_at,
                    agent_access_revocations.reason,
                    agent_access_revocations.revoked_at
             FROM agent_access_requests
             LEFT JOIN agent_access_revocations
               ON agent_access_revocations.request_id = agent_access_requests.request_id
             WHERE agent_access_requests.run_id = ?1
             ORDER BY agent_access_requests.rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    rows.into_iter()
        .map(
            |(
                request_json,
                status,
                decision_json,
                denial_reason,
                requested_at,
                decided_at,
                revocation_reason,
                revoked_at,
            )| {
                let request: AccessRequest = parse_canonical_json(&request_json)?;
                let mut one_run_grant = decision_json
                    .as_deref()
                    .map(parse_canonical_json)
                    .transpose()?;
                let mut denial_reason = denial_reason
                    .as_deref()
                    .map(crate::agent_runtime::PermissionDenialReason::parse)
                    .transpose()?;
                let status = if let Some(revocation_reason) = revocation_reason {
                    if status != "approved"
                        || one_run_grant.is_none()
                        || denial_reason.is_some()
                        || !matches!(
                            revocation_reason.as_str(),
                            "engineer_revoked" | "run_interrupted"
                        )
                        || revoked_at.is_none()
                    {
                        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                    }
                    one_run_grant = None;
                    denial_reason =
                        Some(crate::agent_runtime::PermissionDenialReason::AccessRevoked);
                    AgentAccessRequestStatus::Revoked
                } else {
                    match status.as_str() {
                        "blocked" if one_run_grant.is_none() && denial_reason.is_none() => {
                            if run_is_active {
                                AgentAccessRequestStatus::Blocked
                            } else {
                                AgentAccessRequestStatus::Expired
                            }
                        }
                        "approved" if one_run_grant.is_some() && denial_reason.is_none() => {
                            let current = one_run_grant
                                .as_ref()
                                .and_then(|grant: &crate::agent_runtime::OneRunAccessGrant| {
                                    grant.expires_at.parse::<Timestamp>().ok()
                                })
                                .is_some_and(|expires_at| Timestamp::now() < expires_at);
                            if run_is_active && current {
                                AgentAccessRequestStatus::Approved
                            } else {
                                AgentAccessRequestStatus::Expired
                            }
                        }
                        "denied" if one_run_grant.is_none() && denial_reason.is_some() => {
                            AgentAccessRequestStatus::Denied
                        }
                        "superseded"
                            if one_run_grant.is_none()
                                && denial_reason
                                    == Some(
                                        crate::agent_runtime::PermissionDenialReason::Superseded,
                                    ) =>
                        {
                            AgentAccessRequestStatus::Superseded
                        }
                        _ => {
                            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                        }
                    }
                };
                Ok(AgentAccessRequestView {
                    request,
                    status,
                    one_run_grant,
                    denial_reason,
                    requested_at,
                    decided_at: revoked_at.or(decided_at),
                })
            },
        )
        .collect()
}

pub(super) fn load_thread_exposure(
    connection: &rusqlite::Connection,
    thread_ref: &str,
) -> Result<ThreadExposureSet, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT provider_thread_exposures.exposure_json
             FROM provider_thread_exposures
             JOIN agent_runs ON agent_runs.run_id = provider_thread_exposures.run_id
             WHERE provider_thread_exposures.thread_ref = ?1
             ORDER BY agent_runs.run_sequence",
        )
        .map_err(sql_error)?;
    let exposures = statement
        .query_map([thread_ref], |row| row.get::<_, String>(0))
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    let mut cumulative = ThreadExposureSet::default();
    if exposures.is_empty() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for exposure in exposures {
        cumulative.merge(&parse_canonical_json(&exposure)?);
    }
    Ok(cumulative)
}

fn next_provider_event_sequence(
    transaction: &rusqlite::Transaction<'_>,
    run_id: &str,
) -> Result<u32, TenderCommandError> {
    transaction
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM provider_events WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

pub(super) fn ensure_agent_run_capacity(
    transaction: &rusqlite::Transaction<'_>,
) -> Result<(), TenderCommandError> {
    let run_count: i64 = transaction
        .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
        .map_err(sql_error)?;
    let run_count = u64::try_from(run_count)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if run_count
        >= u64::try_from(MAX_AGENT_RUNS_PER_TENDER)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn provider_event_field_bytes(event: &PendingProviderEvent) -> Result<u64, TenderCommandError> {
    let fields = [
        Some(event.kind.as_str()),
        Some(event.summary.as_str()),
        event.correlation_id.as_deref(),
        event.request_fingerprint.as_deref(),
        event.denial_reason.map(|reason| reason.as_str()),
        event.opaque_reference.as_deref(),
    ];
    fields.iter().flatten().try_fold(0_u64, |total, field| {
        let field_bytes = u64::try_from(field.len())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if field_bytes > MAX_PROVIDER_EVENT_FIELD_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        total
            .checked_add(field_bytes)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
    })
}

fn ensure_provider_event_capacity(
    transaction: &rusqlite::Transaction<'_>,
    run_id: &str,
    event: &PendingProviderEvent,
) -> Result<(), TenderCommandError> {
    let event_bytes = provider_event_field_bytes(event)?;
    let (event_count, existing_bytes): (i64, i64) = transaction
        .query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(
                      length(CAST(kind AS BLOB)) + length(CAST(summary AS BLOB)) +
                      COALESCE(length(CAST(correlation_id AS BLOB)), 0) +
                      COALESCE(length(CAST(request_fingerprint AS BLOB)), 0) +
                      COALESCE(length(CAST(denial_reason AS BLOB)), 0) +
                      COALESCE(length(CAST(opaque_reference AS BLOB)), 0)
                    ), 0)
             FROM provider_events WHERE run_id = ?1",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    let event_count = u64::try_from(event_count)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let existing_bytes = u64::try_from(existing_bytes)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if event_count
        >= u64::try_from(MAX_PROVIDER_EVENTS_PER_RUN)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
        || existing_bytes
            .checked_add(event_bytes)
            .filter(|total| *total <= MAX_PROVIDER_EVENT_BYTES_PER_RUN)
            .is_none()
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

pub(super) fn insert_event(
    transaction: &rusqlite::Transaction<'_>,
    run_id: &str,
    sequence: u32,
    event: PendingProviderEvent,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    ensure_provider_event_capacity(transaction, run_id, &event)?;
    transaction
        .execute(
            "INSERT INTO provider_events (
               run_id, sequence, kind, summary, correlation_id, request_fingerprint,
               denial_reason, opaque_reference, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                run_id,
                sequence,
                event.kind.as_str(),
                event.summary,
                event.correlation_id,
                event.request_fingerprint,
                event.denial_reason.map(|reason| reason.as_str()),
                event.opaque_reference,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn dispose_workspace(
    tender_root: &Path,
    workspace: &Path,
    run_id: &str,
    state: AgentRunState,
) -> Result<(), TenderCommandError> {
    let tender_id = tender_root
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let application_home = tender_root
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if workspace != agent_workspace_path(application_home, tender_id, run_id) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    match fs::symlink_metadata(workspace) {
        Ok(metadata) if !metadata_is_unsafe_storage_link(&metadata) && metadata.is_dir() => {
            if state == AgentRunState::Indeterminate {
                let quarantine = application_home
                    .join("staging")
                    .join(format!("quarantine-agent-{tender_id}-{run_id}"));
                if quarantine.exists() {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                fs::rename(workspace, quarantine).map_err(store_unavailable)?;
            } else {
                remove_directory_after_provider_exit(workspace)?;
            }
        }
        Ok(_) => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(store_unavailable(error)),
    }
    Ok(())
}

fn agent_workspace_path(
    application_home: &Path,
    tender_id: &str,
    run_id: &str,
) -> std::path::PathBuf {
    application_home
        .join("staging")
        .join(format!("agent-{tender_id}-{run_id}"))
}

fn highest_classification(
    grant: &PermissionGrant,
) -> Result<DataClassification, TenderCommandError> {
    grant
        .data_classifications
        .iter()
        .copied()
        .max()
        .filter(|classification| *classification != DataClassification::Secret)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn remove_directory_after_provider_exit(workspace: &Path) -> Result<(), TenderCommandError> {
    const WINDOWS_HANDLE_RELEASE_ATTEMPTS: usize = 20;
    const WINDOWS_HANDLE_RELEASE_DELAY: Duration = Duration::from_millis(25);

    for attempt in 0..WINDOWS_HANDLE_RELEASE_ATTEMPTS {
        match fs::remove_dir_all(workspace) {
            Ok(()) => return Ok(()),
            Err(error)
                if cfg!(windows)
                    && error.kind() == std::io::ErrorKind::PermissionDenied
                    && attempt + 1 < WINDOWS_HANDLE_RELEASE_ATTEMPTS =>
            {
                thread::sleep(WINDOWS_HANDLE_RELEASE_DELAY);
            }
            Err(error) => return Err(store_unavailable(error)),
        }
    }
    Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable))
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
}

fn parse_canonical_json<T>(value: &str) -> Result<T, TenderCommandError>
where
    T: DeserializeOwned + Serialize,
{
    let parsed: T = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

fn ensure_canonical_value(value: &str) -> Result<(), TenderCommandError> {
    let parsed: Value = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? == value {
        Ok(())
    } else {
        Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_table(connection: &rusqlite::Connection) {
        connection
            .execute_batch(
                "CREATE TABLE provider_events (
                   run_id TEXT NOT NULL,
                   sequence INTEGER NOT NULL,
                   kind TEXT NOT NULL,
                   summary TEXT NOT NULL,
                   correlation_id TEXT,
                   request_fingerprint TEXT,
                   denial_reason TEXT,
                   opaque_reference TEXT,
                   created_at TEXT NOT NULL
                 );",
            )
            .expect("create provider event table");
    }

    fn warning_event(summary: String) -> PendingProviderEvent {
        PendingProviderEvent {
            kind: ProviderEventKind::Warning,
            summary,
            correlation_id: None,
            request_fingerprint: None,
            denial_reason: None,
            opaque_reference: None,
        }
    }

    #[test]
    fn canonical_agent_run_boundary_rejects_the_tender_run_cap() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("open SQLite");
        connection
            .execute_batch(
                "CREATE TABLE agent_runs (run_id TEXT NOT NULL);
                 WITH digits(value) AS (
                   VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
                 )
                 INSERT INTO agent_runs (run_id)
                 SELECT printf('%d%d%d%d', a.value, b.value, c.value, d.value)
                 FROM digits AS a
                 CROSS JOIN digits AS b
                 CROSS JOIN digits AS c
                 CROSS JOIN digits AS d;",
            )
            .expect("fill Agent Run limit");
        let transaction = connection.transaction().expect("start transaction");

        let error = ensure_agent_run_capacity(&transaction)
            .expect_err("the canonical boundary must reject an extra Agent Run");

        assert_eq!(error.code, TenderErrorCode::InvalidCommand);
        assert_eq!(
            transaction
                .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row
                    .get::<_, i64>(0))
                .expect("count Agent Runs"),
            i64::try_from(MAX_AGENT_RUNS_PER_TENDER).expect("Agent Run limit")
        );
    }

    #[test]
    fn canonical_provider_event_boundary_rejects_count_without_inserting() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("open SQLite");
        event_table(&connection);
        connection
            .execute_batch(
                "WITH digits(value) AS (
                   VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
                 )
                 INSERT INTO provider_events (
                   run_id, sequence, kind, summary, created_at
                 )
                 SELECT 'run',
                        1 + a.value + 10 * b.value + 100 * c.value + 1000 * d.value,
                        'warning', '', '2026-08-09T00:00:00Z'
                 FROM digits AS a
                 CROSS JOIN digits AS b
                 CROSS JOIN digits AS c
                 CROSS JOIN digits AS d;",
            )
            .expect("fill provider event count limit");
        let transaction = connection.transaction().expect("start transaction");

        let error = insert_event(
            &transaction,
            "run",
            10_001,
            warning_event("one event too many".into()),
            "2026-08-09T00:00:01Z",
        )
        .expect_err("event count limit must reject the insert");

        assert_eq!(error.code, TenderErrorCode::InvalidCommand);
        assert_eq!(
            transaction
                .query_row("SELECT COUNT(*) FROM provider_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("count provider events"),
            i64::try_from(MAX_PROVIDER_EVENTS_PER_RUN).expect("provider event limit")
        );
    }

    #[test]
    fn canonical_provider_event_boundary_rejects_field_and_aggregate_bytes() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("open SQLite");
        event_table(&connection);
        let transaction = connection.transaction().expect("start transaction");
        let oversized =
            "x".repeat(usize::try_from(MAX_PROVIDER_EVENT_FIELD_BYTES).expect("field limit") + 1);
        let error = insert_event(
            &transaction,
            "run",
            1,
            warning_event(oversized),
            "2026-08-09T00:00:00Z",
        )
        .expect_err("oversized field must be rejected");
        assert_eq!(error.code, TenderErrorCode::InvalidCommand);
        assert_eq!(
            transaction
                .query_row("SELECT COUNT(*) FROM provider_events", [], |row| row
                    .get::<_, u32>(0))
                .expect("count rejected provider events"),
            0
        );
        transaction
            .execute_batch(
                "WITH RECURSIVE counter(value) AS (
                   VALUES (1)
                   UNION ALL
                   SELECT value + 1 FROM counter WHERE value < 256
                 )
                 INSERT INTO provider_events (
                   run_id, sequence, kind, summary, created_at
                 )
                 SELECT 'run', value, 'warning',
                        substr(hex(zeroblob(32765)), 1, 65529),
                        '2026-08-09T00:00:00Z'
                 FROM counter;",
            )
            .expect("fill provider event byte limit");

        let error = insert_event(
            &transaction,
            "run",
            257,
            warning_event("one byte too many".into()),
            "2026-08-09T00:00:01Z",
        )
        .expect_err("aggregate event bytes must reject the insert");

        assert_eq!(error.code, TenderErrorCode::InvalidCommand);
        assert_eq!(
            transaction
                .query_row("SELECT COUNT(*) FROM provider_events", [], |row| row
                    .get::<_, u32>(0))
                .expect("count retained provider events"),
            256
        );
    }
}
