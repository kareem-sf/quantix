use std::{fs, path::Path, thread, time::Duration};

use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use crate::agent_runtime::{
    bootstrap_profile, bootstrap_task, AgentProfileVersionView, AgentRunInspection, AgentRunState,
    PendingProviderEvent, PreparedAgentRun, ProposedAgentResult, ProviderEvent, ProviderEventKind,
    ProviderExecution, ProviderFailure, ProviderFailureCategory, ProviderUsage, TenderTaskView,
    VerificationStatus,
};

use super::{
    append_audit_event, metadata_is_unsafe_storage_link, random_identifier, sql_error,
    sqlite_timestamp, store_unavailable, TenderCommandError, TenderErrorCode, TenderId,
    TenderStore,
};

const BOOTSTRAP_STABLE_IDENTITY: &str = "quantix.bootstrap.tender-analyst";

impl TenderStore {
    pub(crate) fn prepare_bootstrap_agent_run(
        &mut self,
        tender_id: &TenderId,
        retry_of_run_id: Option<&str>,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        let run_id = random_identifier(&self.connection)?;
        let workspace = self.root.join("runs").join(format!("agent-{run_id}"));
        fs::create_dir(&workspace).map_err(store_unavailable)?;

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
            let created_at = sqlite_timestamp(&transaction)?;
            let has_unresolved_indeterminate: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs WHERE status = 'indeterminate'
                     )",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if has_unresolved_indeterminate {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }

            let (profile, task) = if let Some(retry_of_run_id) = retry_of_run_id {
                let prior: Option<(String, String)> = transaction
                    .query_row(
                        "SELECT status, task_id FROM agent_runs WHERE run_id = ?1",
                        [retry_of_run_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                let (prior_status, task_id) = prior
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                if prior_status == "running" {
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
                    let profile = bootstrap_profile(random_identifier(&transaction)?);
                    insert_profile(&transaction, &profile, &created_at)?;
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
            let provider_thread_ref: Option<String> = transaction
                .query_row(
                    "SELECT thread_ref FROM provider_threads
                     WHERE profile_id = ?1 AND profile_version = ?2 AND status = 'active'",
                    params![profile.profile_id, profile.version],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            transaction
                .execute(
                    "INSERT INTO agent_runs (
                       run_id, task_id, profile_id, profile_version, retry_of_run_id,
                       status, started_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
                    params![
                        run_id,
                        task.task_id,
                        profile.profile_id,
                        profile.version,
                        retry_of_run_id,
                        created_at,
                    ],
                )
                .map_err(sql_error)?;
            transaction
                .execute(
                    "INSERT INTO provider_events (
                       run_id, sequence, kind, summary, opaque_reference, created_at
                     ) VALUES (?1, 1, 'run_started', 'Agent Run started', NULL, ?2)",
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
                provider_thread_ref,
                workspace: workspace.clone(),
            })
        })();
        if prepared.is_err() {
            let _ = fs::remove_dir(&workspace);
        }
        prepared
    }

    pub(crate) fn complete_agent_run(
        &mut self,
        tender_id: &TenderId,
        prepared: &PreparedAgentRun,
        execution: ProviderExecution,
    ) -> Result<(), TenderCommandError> {
        if execution.state == AgentRunState::Running
            || (execution.state == AgentRunState::Completed
                && (execution.failure.is_some() || execution.candidate_payload_json.is_none()))
            || (execution.state != AgentRunState::Completed && execution.failure.is_none())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        dispose_workspace(
            &self.root,
            &prepared.workspace,
            &prepared.run_id,
            execution.state,
        )?;

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
                     WHERE profile_id = ?1 AND profile_version = ?2",
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
                 SET status = ?2, provider_thread_ref = ?3, provider_turn_ref = ?4,
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
        let result_id = if let Some(payload_json) = execution.candidate_payload_json {
            ensure_canonical_value(&payload_json)?;
            let result_id = random_identifier(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO proposed_agent_results (
                       result_id, run_id, verification_status, payload_json, created_at
                     ) VALUES (?1, ?2, 'proposed', ?3, ?4)",
                    params![result_id, prepared.run_id, payload_json, completed_at],
                )
                .map_err(sql_error)?;
            Some(result_id)
        } else {
            None
        };
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
        let sequence: u32 = transaction
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) + 1 FROM provider_events WHERE run_id = ?1",
                [&prepared.run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
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
                opaque_reference: Some(thread_ref.to_owned()),
            },
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn checkpoint_agent_turn(
        &mut self,
        run_id: &str,
        turn_ref: &str,
    ) -> Result<(), TenderCommandError> {
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
        let sequence: u32 = transaction
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) + 1 FROM provider_events WHERE run_id = ?1",
                [run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        insert_event(
            &transaction,
            run_id,
            sequence,
            PendingProviderEvent {
                kind: ProviderEventKind::TurnStarted,
                summary: "Provider Turn started".into(),
                opaque_reference: Some(turn_ref.to_owned()),
            },
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn inspect_agent_runs(&self) -> Result<Vec<AgentRunInspection>, TenderCommandError> {
        let run_ids = {
            let mut statement = self
                .connection
                .prepare("SELECT run_id FROM agent_runs ORDER BY run_sequence")
                .map_err(sql_error)?;
            let run_ids = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sql_error)?;
            run_ids
        };
        run_ids
            .iter()
            .map(|run_id| self.inspect_agent_run(run_id))
            .collect()
    }

    pub(crate) fn inspect_agent_run(
        &self,
        run_id: &str,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        let row: Option<RawAgentRun> = self
            .connection
            .query_row(
                "SELECT task_id, profile_id, profile_version, retry_of_run_id, status,
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
                        status: row.get(4)?,
                        provider_thread_ref: row.get(5)?,
                        provider_turn_ref: row.get(6)?,
                        usage_json: row.get(7)?,
                        failure_json: row.get(8)?,
                        started_at: row.get(9)?,
                        completed_at: row.get(10)?,
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
        if task.profile_id != profile.profile_id || task.profile_version != profile.version {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let events = load_events(&self.connection, run_id)?;
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
                "SELECT result_id, verification_status, payload_json
                 FROM proposed_agent_results WHERE run_id = ?1",
                [run_id],
                |result| {
                    Ok((
                        result.get::<_, String>(0)?,
                        result.get::<_, String>(1)?,
                        result.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?
            .map(|(result_id, status, payload_json)| {
                if status != "proposed" {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                ensure_canonical_value(&payload_json)?;
                Ok(ProposedAgentResult {
                    result_id,
                    verification_status: VerificationStatus::Proposed,
                    payload_json,
                })
            })
            .transpose()?;
        let state = AgentRunState::parse(&row.status)?;
        if (state == AgentRunState::Completed) != proposed_result.is_some()
            || (state == AgentRunState::Running
                && (failure.is_some() || row.completed_at.is_some()))
            || (state != AgentRunState::Running && row.completed_at.is_none())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(AgentRunInspection {
            run_id: run_id.to_owned(),
            retry_of_run_id: row.retry_of_run_id,
            state,
            profile,
            task,
            provider_thread_ref: row.provider_thread_ref,
            provider_turn_ref: row.provider_turn_ref,
            events,
            usage,
            failure,
            proposed_result,
            started_at: row.started_at,
            completed_at: row.completed_at,
        })
    }

    pub(super) fn reconcile_interrupted_agent_runs(
        &mut self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        let runs = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT run_id, provider_turn_ref FROM agent_runs
                     WHERE status = 'running' ORDER BY run_sequence",
                )
                .map_err(sql_error)?;
            let runs = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .map_err(sql_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sql_error)?;
            runs
        };
        if runs.is_empty() {
            return Ok(());
        }
        for (run_id, turn_ref) in &runs {
            if run_id.len() != 32 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let workspace = self.root.join("runs").join(format!("agent-{run_id}"));
            dispose_workspace(
                &self.root,
                &workspace,
                run_id,
                if turn_ref.is_some() {
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
        for (run_id, turn_ref) in runs {
            let accepted = turn_ref.is_some();
            let state = if accepted {
                AgentRunState::Indeterminate
            } else {
                AgentRunState::Failed
            };
            let failure = if accepted {
                ProviderFailure::new(
                    ProviderFailureCategory::OutcomeUnknown,
                    false,
                    "Resolve the quarantined Agent Run before retrying.",
                    Some("The Host restarted after the Provider Turn was accepted but before its outcome was established."),
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
                     SET status = ?2, usage_json = ?3, failure_json = ?4,
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
            let sequence: u32 = transaction
                .query_row(
                    "SELECT COALESCE(MAX(sequence), 0) + 1 FROM provider_events WHERE run_id = ?1",
                    [&run_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            insert_event(
                &transaction,
                &run_id,
                sequence,
                PendingProviderEvent {
                    kind: ProviderEventKind::Terminal,
                    summary: if accepted {
                        "Agent Run outcome became indeterminate after Host restart".into()
                    } else {
                        "Agent Run failed safely before Provider Turn acceptance".into()
                    },
                    opaque_reference: turn_ref,
                },
                &completed_at,
            )?;
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                if accepted {
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
    capabilities_json: String,
    instructions: String,
    output_contract_json: String,
    review_policy: String,
    permissions_json: String,
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

fn insert_profile(
    transaction: &rusqlite::Transaction<'_>,
    profile: &AgentProfileVersionView,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO agent_profiles (profile_id, stable_identity, created_at)
             VALUES (?1, ?2, ?3)",
            params![profile.profile_id, BOOTSTRAP_STABLE_IDENTITY, created_at],
        )
        .map_err(sql_error)?;
    transaction
        .execute(
            "INSERT INTO agent_profile_versions (
               profile_id, version, identity, profession, capabilities_json, instructions,
               output_contract_json, review_policy, permissions_json, resource_budget_json,
               created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                profile.profile_id,
                profile.version,
                profile.identity,
                profile.profession,
                canonical_json(&profile.capabilities)?,
                profile.instructions,
                profile.output_contract_json,
                profile.review_policy,
                canonical_json(&profile.permissions)?,
                canonical_json(&profile.resource_budget)?,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn insert_task(
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

fn load_profile(
    connection: &rusqlite::Connection,
    key: (String, u32),
) -> Result<AgentProfileVersionView, TenderCommandError> {
    let raw: Option<RawProfileVersion> = connection
        .query_row(
            "SELECT identity, profession, capabilities_json, instructions,
                    output_contract_json, review_policy, permissions_json, resource_budget_json
             FROM agent_profile_versions WHERE profile_id = ?1 AND version = ?2",
            params![key.0, key.1],
            |row| {
                Ok(RawProfileVersion {
                    identity: row.get(0)?,
                    profession: row.get(1)?,
                    capabilities_json: row.get(2)?,
                    instructions: row.get(3)?,
                    output_contract_json: row.get(4)?,
                    review_policy: row.get(5)?,
                    permissions_json: row.get(6)?,
                    resource_budget_json: row.get(7)?,
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
        capabilities: parse_canonical_json(&raw.capabilities_json)?,
        instructions: raw.instructions,
        output_contract_json: raw.output_contract_json,
        review_policy: raw.review_policy,
        permissions: parse_canonical_json(&raw.permissions_json)?,
        resource_budget: parse_canonical_json(&raw.resource_budget_json)?,
    })
}

fn load_task(
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

fn load_events(
    connection: &rusqlite::Connection,
    run_id: &str,
) -> Result<Vec<ProviderEvent>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT sequence, kind, summary, opaque_reference
             FROM provider_events WHERE run_id = ?1 ORDER BY sequence",
        )
        .map_err(sql_error)?;
    let events = statement
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    let mut expected = 1_u32;
    events
        .into_iter()
        .map(|(sequence, kind, summary, opaque_reference)| {
            if sequence != expected {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            expected = expected
                .checked_add(1)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            Ok(ProviderEvent {
                sequence,
                kind: ProviderEventKind::parse(&kind)?,
                summary,
                opaque_reference,
            })
        })
        .collect()
}

fn insert_event(
    transaction: &rusqlite::Transaction<'_>,
    run_id: &str,
    sequence: u32,
    event: PendingProviderEvent,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO provider_events (
               run_id, sequence, kind, summary, opaque_reference, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                run_id,
                sequence,
                event.kind.as_str(),
                event.summary,
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
    if workspace != tender_root.join("runs").join(format!("agent-{run_id}")) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    match fs::symlink_metadata(workspace) {
        Ok(metadata) if !metadata_is_unsafe_storage_link(&metadata) && metadata.is_dir() => {
            if state == AgentRunState::Indeterminate {
                let quarantine = tender_root
                    .join("staging")
                    .join(format!("quarantine-agent-{run_id}"));
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
