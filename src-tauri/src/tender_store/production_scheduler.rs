use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
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
    QuantixHost,
};

use super::bid_decisions::package_dependencies_are_current;
use super::{
    agent_records::{
        ensure_agent_run_capacity, insert_event, insert_task, load_profile, load_task,
        load_thread_exposure,
    },
    append_audit_event, lock_mutex_with_check, random_identifier, require_setup, sha256_hex,
    sql_error, sqlite_timestamp, BidPackageOperationBudget, TenderCommandError, TenderErrorCode,
    TenderId, TenderStore, WorkPlanDecision, WorkPlanProfileBinding, WorkPlanTask,
};

const MAX_PRODUCTION_TASKS: usize = 256;
const MAX_PRODUCTION_TASK_ATTEMPTS: u32 = 8;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProductionTaskState {
    Blocked,
    Ready,
    Running,
    ReviewReady,
    Reviewing,
    Completed,
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
            Self::Completed => "completed",
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
            "completed" => Ok(Self::Completed),
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
pub struct ProductionTaskOutput {
    pub output_id: String,
    pub run_id: String,
    pub reviewer_run_id: String,
    pub payload_sha256: String,
    pub data_scopes: Vec<String>,
    pub data_classifications: Vec<DataClassification>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProductionTaskInspection {
    pub production_task_id: String,
    pub plan_manifest_sha256: String,
    pub task: WorkPlanTask,
    pub state: ProductionTaskState,
    pub run_ids: Vec<String>,
    pub registered_outputs: Vec<ProductionTaskOutput>,
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
}

impl TenderStore {
    pub(super) fn production_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let (activation_count, task_count, attempt_count, output_count): (u32, u32, u32, u32) =
            self.connection
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM production_activations),
                        (SELECT COUNT(*) FROM production_tasks),
                        (SELECT COUNT(*) FROM production_task_attempts),
                        (SELECT COUNT(*) FROM production_task_outputs)",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(sql_error)?;
        if activation_count > 256
            || task_count > activation_count.saturating_mul(MAX_PRODUCTION_TASKS as u32)
            || attempt_count > task_count.saturating_mul(MAX_PRODUCTION_TASK_ATTEMPTS)
            || output_count > task_count
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
                             AND tender.lifecycle_phase = 'active_production'
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
                let stored: Option<(String, String, String, Option<String>)> = self
                    .connection
                    .query_row(
                        "SELECT production_task_id, task_definition_json, status, task_id
                         FROM production_tasks WHERE activation_id = ?1 AND task_key = ?2",
                        params![activation.0, definition.task_key],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                let Some((production_task_id, task_json, status, task_id)) = stored else {
                    return Ok(false);
                };
                if parse_canonical_json::<WorkPlanTask>(&task_json)? != definition
                    || sha256_hex(task_json.as_bytes())
                        != self
                            .connection
                            .query_row(
                                "SELECT task_definition_sha256 FROM production_tasks
                                 WHERE production_task_id = ?1",
                                [&production_task_id],
                                |row| row.get::<_, String>(0),
                            )
                            .map_err(sql_error)?
                {
                    return Ok(false);
                }
                let state = ProductionTaskState::parse(&status)?;
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
                                 AND status = 'completed'
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
                                | ProductionTaskState::Suspended
                        ))
                    || (!attempts.is_empty()
                        && matches!(
                            state,
                            ProductionTaskState::Blocked | ProductionTaskState::Ready
                        ))
                {
                    return Ok(false);
                }
                let mut prior_run_id: Option<String> = None;
                let mut prior_attempt_kind: Option<String> = None;
                let mut latest_author_run: Option<(String, String)> = None;
                let mut latest_review_run: Option<(String, String)> = None;
                for (attempt_index, (_, attempt_kind, attempt_task_id)) in
                    attempts.iter().enumerate()
                {
                    check()?;
                    let task = load_task(&self.connection, attempt_task_id)?;
                    let attempt_profile = if attempt_kind == "review" {
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
                    let mut expected_inputs = if attempt_kind == "review" {
                        let (author_run_id, _) = latest_author_run.as_ref().ok_or_else(|| {
                            TenderCommandError::new(TenderErrorCode::IntegrityFailed)
                        })?;
                        vec![AgentTaskInputReference {
                            kind: "production_task_candidate".into(),
                            reference: author_run_id.clone(),
                            version: 1,
                        }]
                    } else {
                        let mut inputs = definition.exact_inputs.clone();
                        inputs.extend(definition.dependencies.iter().map(|dependency| {
                            AgentTaskInputReference {
                                kind: "production_task_output".into(),
                                reference: dependency.clone(),
                                version: 1,
                            }
                        }));
                        inputs
                    };
                    expected_inputs.sort_by(|left, right| {
                        (&left.kind, &left.reference, left.version).cmp(&(
                            &right.kind,
                            &right.reference,
                            right.version,
                        ))
                    });
                    expected_inputs.dedup();
                    let expected_objective = if attempt_kind == "review" {
                        format!(
                            "Independently review the exact candidate for {} without editing or approving it.",
                            definition.task_key
                        )
                    } else {
                        definition.objective.clone()
                    };
                    let expected_contract = if attempt_kind == "review" {
                        production_review_output_contract()?
                    } else {
                        definition.output_contract_json.clone()
                    };
                    let expected_review_policy = if attempt_kind == "review" {
                        "This separate review must report whether the exact candidate satisfies the approved review policy; it cannot edit or approve the work."
                    } else {
                        profile.review_policy.as_str()
                    };
                    if task.profile_id != attempt_profile.profile_id
                        || task.profile_version != attempt_profile.version
                        || task.objective != expected_objective
                        || task.exact_inputs != expected_inputs
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
                    let latest = attempt_index + 1 == attempts.len();
                    let expected_retry = (prior_attempt_kind.as_deref()
                        == Some(attempt_kind.as_str()))
                    .then_some(prior_run_id.as_deref())
                    .flatten();
                    let next_kind = attempts
                        .get(attempt_index + 1)
                        .map(|attempt| attempt.1.as_str());
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
                            && attempt_kind == "author"
                            && next_kind == Some("review"))
                            || retry_terminal);
                    let review_satisfied = attempt_kind == "review"
                        && run_status == "completed"
                        && self
                            .connection
                            .query_row(
                                "SELECT json_extract(payload_json, '$.verdict') = 'satisfied'
                                 FROM proposed_agent_results WHERE run_id = ?1",
                                [run_id],
                                |row| row.get::<_, bool>(0),
                            )
                            .optional()
                            .map_err(sql_error)?
                            .unwrap_or(false);
                    let latest_matches = latest
                        && match state {
                            ProductionTaskState::Running => {
                                attempt_kind == "author" && run_status == "running"
                            }
                            ProductionTaskState::ReviewReady => {
                                attempt_kind == "author" && run_status == "completed"
                            }
                            ProductionTaskState::Reviewing => {
                                attempt_kind == "review" && run_status == "running"
                            }
                            ProductionTaskState::Completed => {
                                review_satisfied
                                    || (attempt_kind == "author"
                                        && run_status == "completed"
                                        && definition.review_profile_id.is_none()
                                        && definition.review_profile_version.is_none())
                            }
                            ProductionTaskState::Failed => {
                                run_status == "failed"
                                    || (attempt_kind == "review"
                                        && run_status == "completed"
                                        && !review_satisfied)
                            }
                            ProductionTaskState::Cancelled => run_status == "interrupted",
                            ProductionTaskState::Indeterminate => run_status == "indeterminate",
                            ProductionTaskState::Suspended => matches!(
                                run_status.as_str(),
                                "completed" | "failed" | "interrupted" | "indeterminate"
                            ),
                            ProductionTaskState::Blocked | ProductionTaskState::Ready => false,
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
                    if attempt_kind == "author" && run_status == "completed" {
                        latest_author_run = Some((run_id.clone(), attempt_task_id.clone()));
                    } else if attempt_kind == "review" && run_status == "completed" {
                        latest_review_run = Some((run_id.clone(), attempt_task_id.clone()));
                    }
                }
                let output: Option<(String, String, String, String, String, String)> = self
                    .connection
                    .query_row(
                        "SELECT outputs.run_id, outputs.reviewer_run_id, outputs.payload_json,
                                outputs.payload_sha256, outputs.data_scopes_json,
                                outputs.data_classifications_json
                         FROM production_task_outputs AS outputs
                         WHERE outputs.production_task_id = ?1",
                        [&production_task_id],
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
                if state == ProductionTaskState::Completed {
                    let Some((
                        author_run_id,
                        reviewer_run_id,
                        payload_json,
                        payload_sha256,
                        scopes_json,
                        classifications_json,
                    )) = output
                    else {
                        return Ok(false);
                    };
                    let Some((latest_author_run_id, latest_author_task_id)) =
                        latest_author_run.as_ref()
                    else {
                        return Ok(false);
                    };
                    let expected_reviewer_run_id = latest_review_run
                        .as_ref()
                        .map(|run| run.0.as_str())
                        .or_else(|| {
                            (definition.review_profile_id.is_none()
                                && definition.review_profile_version.is_none())
                            .then_some(latest_author_run_id.as_str())
                        })
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                    if sha256_hex(payload_json.as_bytes()) != payload_sha256
                        || &author_run_id != latest_author_run_id
                        || reviewer_run_id != expected_reviewer_run_id
                        || parse_canonical_json::<Vec<String>>(&scopes_json)?
                            != definition.permissions.data_scopes
                        || parse_canonical_json::<Vec<DataClassification>>(&classifications_json)?
                            != definition.permissions.data_classifications
                        || canonical_json(
                            &serde_json::from_str::<serde_json::Value>(&payload_json).map_err(
                                |_| TenderCommandError::new(TenderErrorCode::IntegrityFailed),
                            )?,
                        )
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
                            != payload_json
                        || !self
                                .connection
                                .query_row(
                                "SELECT EXISTS(
                                   SELECT 1 FROM agent_runs AS runs
                                   JOIN proposed_agent_results AS results
                                     ON results.run_id = runs.run_id
                                  WHERE runs.run_id = ?1 AND runs.task_id = ?2
                                    AND runs.status = 'completed'
                                    AND results.verification_status = 'proposed'
                                    AND results.payload_json = ?3
                                 )",
                                params![author_run_id, latest_author_task_id, payload_json],
                                |row| row.get::<_, bool>(0),
                            )
                            .map_err(sql_error)?
                        || (definition.review_profile_id.is_some()
                            && !self
                            .connection
                            .query_row(
                                "SELECT EXISTS(
                                   SELECT 1 FROM agent_runs AS runs
                                   JOIN proposed_agent_results AS results
                                     ON results.run_id = runs.run_id
                                  WHERE runs.run_id = ?1 AND runs.status = 'completed'
                                    AND json_extract(results.payload_json, '$.verdict') = 'satisfied'
                                 )",
                                [reviewer_run_id],
                                |row| row.get::<_, bool>(0),
                            )
                            .map_err(sql_error)?)
                    {
                        return Ok(false);
                    }
                } else if output.is_some() {
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
        production_task_id: &str,
        expected_retry_of_run_id: Option<&str>,
        subscription_capacity_exhausted: bool,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        self.require_storage_writable()?;
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
            if !work_plan_package_dependencies_are_current(self, plan_id, *plan_version)? {
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
            let attempt_kind = match task_status.as_str() {
                "review_ready" => "review",
                "failed" | "cancelled" | "indeterminate" => latest_attempt
                    .as_ref()
                    .map(|attempt| attempt.5.as_str())
                    .unwrap_or("author"),
                _ => "author",
            };
            let retry_of_run_id = (!matches!(task_status.as_str(), "ready" | "review_ready"))
                .then(|| latest_attempt.as_ref().map(|attempt| attempt.1.clone()))
                .flatten();
            let retry_eligible = match (task_status.as_str(), latest_attempt.as_ref()) {
                ("ready", None) => expected_retry_of_run_id.is_none() && current_task_id.is_none(),
                ("review_ready", Some((_, _, prior_status, _, _, prior_kind))) => {
                    expected_retry_of_run_id.is_none()
                        && prior_status == "completed"
                        && prior_kind == "author"
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
            if !retry_eligible
                || attempt_number > MAX_PRODUCTION_TASK_ATTEMPTS
                || activation_status != "active"
            {
                append_production_denial(
                    &transaction,
                    tender_id,
                    "run_production_task",
                    Some(production_task_id),
                    if activation_status != "active" {
                        "production_not_active"
                    } else if attempt_number > MAX_PRODUCTION_TASK_ATTEMPTS {
                        "task_attempt_limit"
                    } else {
                        "task_not_retryable"
                    },
                )?;
                transaction.commit().map_err(sql_error)?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let definition: WorkPlanTask = parse_canonical_json(&task_json)?;
            let (profile_id, profile_version) = if attempt_kind == "review" {
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
            let mut dependency_outputs = Vec::new();
            if attempt_kind == "author" {
                for dependency in &definition.dependencies {
                    let output: Option<(String, String, String, String, String)> = transaction
                    .query_row(
                        "SELECT outputs.output_id, outputs.payload_sha256, outputs.payload_json,
                                outputs.data_scopes_json, outputs.data_classifications_json
                         FROM production_tasks AS tasks
                         JOIN production_task_outputs AS outputs
                           ON outputs.production_task_id = tasks.production_task_id
                         WHERE tasks.activation_id = ?1 AND tasks.task_key = ?2
                           AND tasks.status = 'completed'",
                        params![activation_id, dependency],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                    let (
                        output_id,
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
                        kind: "production_task_output".into(),
                        reference: dependency.clone(),
                        version: 1,
                    });
                    dependency_outputs.push(json!({
                    "data_classifications": output_classifications,
                    "data_scopes": output_scopes,
                    "output_id": output_id,
                    "payload": serde_json::from_str::<serde_json::Value>(&payload_json)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    "payload_sha256": payload_sha256,
                    "task_key": dependency,
                }));
                }
            }
            let review_candidate = if attempt_kind == "review" {
                let candidate: (String, String) = transaction
                    .query_row(
                        "SELECT runs.run_id, results.payload_json
                         FROM production_task_attempts AS attempts
                         JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                         JOIN proposed_agent_results AS results ON results.run_id = runs.run_id
                         WHERE attempts.production_task_id = ?1
                           AND attempts.attempt_kind = 'author'
                           AND runs.status = 'completed'
                         ORDER BY attempts.attempt_number DESC LIMIT 1",
                        [production_task_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
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
                    kind: "production_task_candidate".into(),
                    reference: candidate.0.clone(),
                    version: 1,
                });
                Some(json!({
                    "author_run_id": candidate.0,
                    "data_classifications": definition.permissions.data_classifications,
                    "data_scopes": definition.permissions.data_scopes,
                    "payload": serde_json::from_str::<serde_json::Value>(&candidate.1)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    "payload_sha256": sha256_hex(candidate.1.as_bytes()),
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
                objective: if attempt_kind == "review" {
                    format!(
                        "Independently review the exact candidate for {} without editing or approving it.",
                        definition.task_key
                    )
                } else {
                    definition.objective.clone()
                },
                exact_inputs,
                output_contract_json: if attempt_kind == "review" {
                    production_review_output_contract()?
                } else {
                    definition.output_contract_json.clone()
                },
                review_policy: if attempt_kind == "review" {
                    "This separate review must report whether the exact candidate satisfies the approved review policy; it cannot edit or approve the work.".into()
                } else {
                    profile.review_policy.clone()
                },
                deadline: definition.deadline.clone(),
                permissions: profile.permissions.clone(),
                resource_budget: profile.resource_budget.clone(),
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
            let payload = json!({
                "data_classification": task.permissions.data_classifications
                    .iter()
                    .max()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
                "data_scope": task.permissions.data_scopes.join("+"),
                "dependency_outputs": dependency_outputs,
                "review_candidate": review_candidate,
                "plan": {
                    "manifest_sha256": plan_manifest_sha256,
                    "plan_id": plan_id,
                    "version": plan_version,
                },
                "production_task": definition,
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

        for task in &plan.tasks {
            budget.check()?;
            let task_definition_json = canonical_json(task)?;
            let task_definition_sha256 = sha256_hex(task_definition_json.as_bytes());
            transaction
                .execute(
                    "INSERT INTO production_tasks (
                       production_task_id, activation_id, task_key, task_definition_json,
                       task_definition_sha256, task_id, status, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?7)",
                    params![
                        random_identifier(&transaction)?,
                        activation_id,
                        task.task_key,
                        task_definition_json,
                        task_definition_sha256,
                        if task.dependencies.is_empty() {
                            ProductionTaskState::Ready.as_str()
                        } else {
                            ProductionTaskState::Blocked.as_str()
                        },
                        created_at,
                    ],
                )
                .map_err(sql_error)?;
        }
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
            let output = self
                .connection
                .query_row(
                    "SELECT output_id, run_id, reviewer_run_id, payload_sha256,
                            data_scopes_json, data_classifications_json, created_at
                     FROM production_task_outputs WHERE production_task_id = ?1",
                    [&production_task_id],
                    |output| {
                        Ok((
                            output.get::<_, String>(0)?,
                            output.get::<_, String>(1)?,
                            output.get::<_, String>(2)?,
                            output.get::<_, String>(3)?,
                            output.get::<_, String>(4)?,
                            output.get::<_, String>(5)?,
                            output.get::<_, String>(6)?,
                        ))
                    },
                )
                .optional()
                .map_err(sql_error)?
                .map(|output| {
                    Ok(ProductionTaskOutput {
                        output_id: output.0,
                        run_id: output.1,
                        reviewer_run_id: output.2,
                        payload_sha256: output.3,
                        data_scopes: parse_canonical_json(&output.4)?,
                        data_classifications: parse_canonical_json(&output.5)?,
                        created_at: output.6,
                    })
                })
                .transpose()?;
            tasks.push(ProductionTaskInspection {
                production_task_id,
                plan_manifest_sha256: activation.3.clone(),
                task: parse_canonical_json(&task_json)?,
                state: ProductionTaskState::parse(&row.get::<_, String>(2).map_err(sql_error)?)?,
                run_ids,
                registered_outputs: output.into_iter().collect(),
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
    let review_satisfied = if run_state == AgentRunState::Completed && attempt_kind == "review" {
        let payload = payload_json
            .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        payload.get("verdict").and_then(serde_json::Value::as_str) == Some("satisfied")
    } else {
        false
    };
    let next_state = match run_state {
        AgentRunState::Completed if self_validating_review => ProductionTaskState::Completed,
        AgentRunState::Completed if attempt_kind == "author" => ProductionTaskState::ReviewReady,
        AgentRunState::Completed if review_satisfied => ProductionTaskState::Completed,
        AgentRunState::Completed => ProductionTaskState::Failed,
        AgentRunState::Interrupted => ProductionTaskState::Cancelled,
        AgentRunState::Indeterminate => ProductionTaskState::Indeterminate,
        AgentRunState::Failed => ProductionTaskState::Failed,
        AgentRunState::Running => {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    };
    if run_state == AgentRunState::Completed
        && ((attempt_kind == "review" && review_satisfied) || self_validating_review)
    {
        let (author_run_id, author_payload): (String, String) = if self_validating_review {
            (
                run_id.to_owned(),
                payload_json
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
                    .to_owned(),
            )
        } else {
            transaction
                .query_row(
                    "SELECT runs.run_id, results.payload_json
                 FROM production_task_attempts AS attempts
                 JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                 JOIN proposed_agent_results AS results ON results.run_id = runs.run_id
                 WHERE attempts.production_task_id = ?1
                   AND attempts.attempt_kind = 'author'
                   AND runs.status = 'completed'
                 ORDER BY attempts.attempt_number DESC LIMIT 1",
                    [&production_task_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(sql_error)?
        };
        transaction
            .execute(
                "INSERT INTO production_task_outputs (
                   output_id, production_task_id, run_id, reviewer_run_id, payload_json,
                   payload_sha256, data_scopes_json, data_classifications_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    random_identifier(transaction)?,
                    production_task_id,
                    author_run_id,
                    if self_validating_review {
                        &author_run_id
                    } else {
                        run_id
                    },
                    author_payload,
                    sha256_hex(author_payload.as_bytes()),
                    canonical_json(&definition.permissions.data_scopes)?,
                    canonical_json(&definition.permissions.data_classifications)?,
                    completed_at,
                ],
            )
            .map_err(sql_error)?;
    } else if run_state != AgentRunState::Completed && payload_json.is_some() {
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
    if next_state == ProductionTaskState::Completed {
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
            "status": next_state.as_str(),
            "task_key": task_key,
        }),
        completed_at,
    )
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
                       WHERE activation_id = ?1 AND task_key = ?2 AND status = 'completed'
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
                "items": { "type": "string", "maxLength": 4000 },
                "maxItems": 32,
                "type": "array"
            },
            "verdict": { "const": "satisfied", "type": "string" }
        },
        "required": ["verdict", "findings"],
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
