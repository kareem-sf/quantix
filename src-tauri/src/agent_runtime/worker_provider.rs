//! Runs an Agent Turn through the AI worker instead of the Codex app server.
//!
//! The two lanes differ in what they can promise. Codex owns a conversation, so it
//! reports a thread and a turn reference and enforces its own filesystem sandbox. The
//! worker is a single request and answer with no conversation and no sandbox, so this
//! adapter has to supply what the rest of the Agent Run machinery expects:
//!
//! - **Authorization stays here.** The worker advertises tools by name and schema only;
//!   every quota, side-effect class and data scope that [`TypedToolDefinition`] carries
//!   is dropped on the way out. Each call is therefore re-checked against the same four
//!   gates the Codex lane uses before anything executes.
//! - **No thread.** `provider_thread_ref` and `provider_turn_ref` stay unset, and the
//!   thread callbacks never fire, because there is nothing to resume or archive.
//! - **One shot.** There is no partial progress to report, so usage arrives once, at
//!   the end.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::permissions::bootstrap_tool_catalogue;
use super::worker_lane::{
    run_worker_operation, WorkerApproval, WorkerDriverError, WorkerFailureCategory,
    WorkerOperation, WorkerOutcome, WorkerRunRequest, WorkerToolDescriptor,
};
use super::{
    failed_execution, AgentRunState, CandidateDisposition, PreparedAgentRun, ProviderExecution,
    ProviderFailure, ProviderFailureCategory, ProviderTransportDisposition, ProviderUsage,
    RunCallbacks,
};
use crate::agent_runtime::codex_protocol::{
    output_failure, process_failure, protocol_failure, provider_instruction_bundle,
    rate_limit_failure, validate_candidate,
};
use crate::application_settings::ProviderReasoningSelection;
use crate::process_supervisor::ProcessSupervisor;
use crate::provider_env::AiProviderConnection;

/// Describe the grant-bound tools for the worker. Names and schemas only: the worker
/// decides what to ask for, never what it is allowed to have.
fn worker_tools(prepared: &PreparedAgentRun) -> Result<Vec<WorkerToolDescriptor>, ProviderFailure> {
    let allowed = &prepared.permission_grant.access_ceiling.allowed_tools;
    bootstrap_tool_catalogue()
        .into_iter()
        .filter(|tool| allowed.contains(&tool.name))
        .map(|tool| {
            let parameters: Value = serde_json::from_str(&tool.input_schema_json)
                .map_err(|_| protocol_failure(false))?;
            Ok(WorkerToolDescriptor {
                name: tool.name,
                description: Some("Read one exact grant-bound Quantix Data View.".to_owned()),
                parameters,
            })
        })
        .collect()
}

fn provider_failure_for(category: WorkerFailureCategory) -> ProviderFailure {
    match category {
        WorkerFailureCategory::Auth => super::authentication_failure(),
        WorkerFailureCategory::RateLimited => rate_limit_failure(),
        WorkerFailureCategory::Budget => ProviderFailure::new(
            ProviderFailureCategory::RequestBudgetExceeded,
            true,
            "Reduce the scope of the task or raise its budget before retrying.",
            Some("The AI worker exceeded the budget for this Provider Turn."),
        ),
        WorkerFailureCategory::Cancelled => super::readiness_interruption_failure(),
        WorkerFailureCategory::InvalidOutput => output_failure(),
        WorkerFailureCategory::Protocol => protocol_failure(true),
        // A provider-side error and a worker that died are both "the turn did not
        // finish and we know why it stopped", which is a process failure that a
        // linked retry may resolve.
        WorkerFailureCategory::Network
        | WorkerFailureCategory::Provider
        | WorkerFailureCategory::Process => process_failure(true),
    }
}

/// A worker failure that reached the model is a failed turn. One that never got that
/// far leaves the turn unstarted, which the Agent Run records differently.
fn transport_for(category: WorkerFailureCategory) -> ProviderTransportDisposition {
    match category {
        WorkerFailureCategory::Budget => ProviderTransportDisposition::LocalRejected,
        WorkerFailureCategory::Cancelled => ProviderTransportDisposition::Interrupted,
        WorkerFailureCategory::Process => ProviderTransportDisposition::Indeterminate,
        _ => ProviderTransportDisposition::Failed,
    }
}

fn state_for(category: WorkerFailureCategory) -> AgentRunState {
    match category {
        WorkerFailureCategory::Cancelled => AgentRunState::Interrupted,
        WorkerFailureCategory::Process => AgentRunState::Indeterminate,
        _ => AgentRunState::Failed,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_worker_turn(
    supervisor: &ProcessSupervisor,
    worker_python: &Path,
    application_home: &Path,
    connection: &AiProviderConnection,
    prepared: PreparedAgentRun,
    operation_limit: Duration,
    cancellation: CancellationToken,
    mut callbacks: RunCallbacks,
) -> ProviderExecution {
    let started = Instant::now();

    let output_schema: Value = match serde_json::from_str(&prepared.task.output_contract_json) {
        Ok(schema) => schema,
        Err(_) => return failed_execution(protocol_failure(false), started),
    };
    let instructions = match provider_instruction_bundle(&prepared) {
        Ok(bundle) => bundle,
        Err(failure) => return failed_execution(failure, started),
    };
    let tools = match worker_tools(&prepared) {
        Ok(tools) => tools,
        Err(failure) => return failed_execution(failure, started),
    };

    let on_requested = std::mem::replace(
        &mut callbacks.on_requested,
        Box::new(|| Ok(())) as Box<super::TurnRequestedCallback>,
    );
    if let Err(failure) = on_requested() {
        return failed_execution(failure, started);
    }

    let request = WorkerRunRequest {
        route: connection.route.as_str().to_owned(),
        base_url: connection.base_url.clone(),
        api_key: connection.api_key.clone(),
        model_id: connection.model_id.clone(),
        reasoning: match &prepared.provider_selection.reasoning {
            ProviderReasoningSelection::ProviderDefault => None,
            ProviderReasoningSelection::Effort(effort) => Some(effort.clone()),
        },
        instructions,
        output_schema: Some(output_schema.clone()),
        tools,
        input: prepared.task.objective.clone(),
        operation: WorkerOperation::Turn,
        timeout: operation_limit,
    };

    // Every tool call is re-authorized here against the grant, because the descriptor
    // the worker received carries none of that. A failure to consult the ledger is not
    // a denial: it is an unknown outcome, so it stops the turn rather than quietly
    // continuing without an audit record.
    let mut tool_failure: Option<ProviderFailure> = None;
    let outcome = {
        let on_tool_call = &mut callbacks.on_tool_call;
        let tool_failure = &mut tool_failure;
        run_worker_operation(
            supervisor,
            worker_python,
            application_home,
            &request,
            cancellation,
            |correlation_id, tool_name, arguments| match on_tool_call(
                correlation_id,
                tool_name,
                arguments,
            ) {
                Ok(Some(output)) => WorkerApproval::Approved(Value::String(output)),
                Ok(None) => WorkerApproval::Denied(
                    "Quantix did not authorize this tool for this Agent Run.".to_owned(),
                ),
                Err(failure) => {
                    *tool_failure = Some(failure);
                    WorkerApproval::Denied("Quantix could not authorize this tool.".to_owned())
                }
            },
        )
        .await
    };

    if let Some(failure) = tool_failure {
        return failed_execution(failure, started);
    }

    let mut execution = match outcome {
        Ok(WorkerOutcome::Turn {
            output,
            text,
            usage,
        }) => {
            // The worker returns structured output when the contract asked for it and
            // free text otherwise; either way the candidate is validated against the
            // same contract the Codex lane enforces.
            let candidate = match output {
                Some(value) => serde_json_canonicalizer::to_string(&value).ok(),
                None => Some(text),
            };
            let mut execution = ProviderExecution {
                state: AgentRunState::Completed,
                transport_disposition: ProviderTransportDisposition::Completed,
                candidate_disposition: CandidateDisposition::NotEvaluated,
                provider_thread_ref: None,
                provider_turn_ref: None,
                events: Vec::new(),
                usage: ProviderUsage {
                    input_tokens: Some(usage.input_tokens),
                    output_tokens: Some(usage.output_tokens),
                    total_tokens: Some(usage.input_tokens + usage.output_tokens),
                    ..ProviderUsage::default()
                },
                failure: None,
                candidate_payload_json: None,
            };
            match validate_candidate(
                candidate.as_deref(),
                &output_schema,
                prepared.task.resource_budget.output_bytes,
            ) {
                Ok(payload) => {
                    execution.candidate_disposition = CandidateDisposition::Validated;
                    execution.candidate_payload_json = Some(payload);
                }
                Err(failure) => {
                    execution.state = AgentRunState::Failed;
                    execution.failure = Some(failure);
                    execution.candidate_disposition =
                        CandidateDisposition::rejected(vec!["schema_rejection".into()]);
                }
            }
            execution
        }
        // A probe answer to a turn request means the worker and the host disagree about
        // the protocol, which is never a completed turn.
        Ok(WorkerOutcome::Probe { .. }) => failed_execution(protocol_failure(true), started),
        Err(WorkerDriverError { category, .. }) => ProviderExecution {
            state: state_for(category),
            transport_disposition: transport_for(category),
            candidate_disposition: CandidateDisposition::NotEvaluated,
            provider_thread_ref: None,
            provider_turn_ref: None,
            events: Vec::new(),
            usage: ProviderUsage::default(),
            failure: Some(provider_failure_for(category)),
            candidate_payload_json: None,
        },
    };

    execution.usage.elapsed_milliseconds =
        Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
    execution
}
