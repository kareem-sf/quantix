use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::{
    process_supervisor::{ProcessError, ProcessSpec, ProcessTermination, SupervisedConversation},
    tender_store::{require_setup, TenderCommandError, TenderErrorCode, TenderId, TenderStore},
    QuantixHost,
};

mod bootstrap_profile;
mod codex_protocol;
pub(crate) use bootstrap_profile::{bootstrap_profile, bootstrap_task};
use codex_protocol::{
    handle_control_request, handle_notification, outcome_unknown, parse_wire_message,
    process_failure, protocol_failure, provider_instruction_bundle, read_expected_response,
    response_result, validate_candidate, validate_schema, write_rpc, NotificationOutcome,
};

#[cfg(not(feature = "runtime-fixture"))]
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(feature = "runtime-fixture")]
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(2);
const PROVIDER_OUTPUT_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunBootstrapAgentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(skip)]
    pub retry_of_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InterruptAgentRunCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub run_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentRunState {
    Running,
    Completed,
    Interrupted,
    Failed,
    Indeterminate,
}

impl AgentRunState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
            Self::Indeterminate => "indeterminate",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "interrupted" => Ok(Self::Interrupted),
            "failed" => Ok(Self::Failed),
            "indeterminate" => Ok(Self::Indeterminate),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum VerificationStatus {
    Proposed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentProfileVersionView {
    pub profile_id: String,
    pub version: u32,
    pub identity: String,
    pub profession: String,
    pub capabilities: Vec<String>,
    pub instructions: String,
    pub output_contract_json: String,
    pub review_policy: String,
    pub permissions: AgentRunPermissions,
    pub resource_budget: AgentResourceBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentTaskInputReference {
    pub kind: String,
    pub reference: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunPermissions {
    pub data_scopes: Vec<String>,
    pub allowed_actions: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub network_allowed: bool,
    pub workspace_write_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentResourceBudget {
    pub provider_turns: u32,
    pub duration_seconds: u32,
    pub output_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderTaskView {
    pub task_id: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub objective: String,
    pub exact_inputs: Vec<AgentTaskInputReference>,
    pub output_contract_json: String,
    pub review_policy: String,
    pub deadline: String,
    pub permissions: AgentRunPermissions,
    pub resource_budget: AgentResourceBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderEventKind {
    RunStarted,
    ThreadEstablished,
    ThreadResumed,
    TurnStarted,
    UsageObserved,
    ControlRequestDenied,
    Warning,
    Terminal,
}

impl ProviderEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunStarted => "run_started",
            Self::ThreadEstablished => "thread_established",
            Self::ThreadResumed => "thread_resumed",
            Self::TurnStarted => "turn_started",
            Self::UsageObserved => "usage_observed",
            Self::ControlRequestDenied => "control_request_denied",
            Self::Warning => "warning",
            Self::Terminal => "terminal",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "run_started" => Ok(Self::RunStarted),
            "thread_established" => Ok(Self::ThreadEstablished),
            "thread_resumed" => Ok(Self::ThreadResumed),
            "turn_started" => Ok(Self::TurnStarted),
            "usage_observed" => Ok(Self::UsageObserved),
            "control_request_denied" => Ok(Self::ControlRequestDenied),
            "warning" => Ok(Self::Warning),
            "terminal" => Ok(Self::Terminal),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderEvent {
    pub sequence: u32,
    pub kind: ProviderEventKind,
    pub summary: String,
    pub opaque_reference: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderUsage {
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub context_window: Option<u64>,
    pub elapsed_milliseconds: Option<u64>,
    pub rate_limit_reached: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderFailureCategory {
    ProtocolInvalid,
    ProcessFailed,
    RateLimited,
    OutputInvalid,
    Interrupted,
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderFailure {
    pub category: ProviderFailureCategory,
    pub retry_safe: bool,
    pub required_user_action: String,
    pub redacted_detail: Option<String>,
}

impl ProviderFailure {
    pub(crate) fn new(
        category: ProviderFailureCategory,
        retry_safe: bool,
        required_user_action: &str,
        redacted_detail: Option<&str>,
    ) -> Self {
        Self {
            category,
            retry_safe,
            required_user_action: required_user_action.to_owned(),
            redacted_detail: redacted_detail.map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProposedAgentResult {
    pub result_id: String,
    pub verification_status: VerificationStatus,
    pub payload_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunInspection {
    pub run_id: String,
    pub retry_of_run_id: Option<String>,
    pub state: AgentRunState,
    pub profile: AgentProfileVersionView,
    pub task: TenderTaskView,
    pub provider_thread_ref: Option<String>,
    pub provider_turn_ref: Option<String>,
    pub events: Vec<ProviderEvent>,
    pub usage: ProviderUsage,
    pub failure: Option<ProviderFailure>,
    pub proposed_result: Option<ProposedAgentResult>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedAgentRun {
    pub run_id: String,
    pub profile: AgentProfileVersionView,
    pub task: TenderTaskView,
    pub provider_thread_ref: Option<String>,
    pub workspace: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingProviderEvent {
    pub kind: ProviderEventKind,
    pub summary: String,
    pub opaque_reference: Option<String>,
}

impl PendingProviderEvent {
    fn new(kind: ProviderEventKind, summary: &str, opaque_reference: Option<&str>) -> Self {
        Self {
            kind,
            summary: summary.to_owned(),
            opaque_reference: opaque_reference.map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderExecution {
    pub state: AgentRunState,
    pub provider_thread_ref: Option<String>,
    pub provider_turn_ref: Option<String>,
    pub events: Vec<PendingProviderEvent>,
    pub usage: ProviderUsage,
    pub failure: Option<ProviderFailure>,
    pub candidate_payload_json: Option<String>,
}

struct ActiveAgentRunGuard {
    host: QuantixHost,
}

impl Drop for ActiveAgentRunGuard {
    fn drop(&mut self) {
        self.host.finish_active_agent_run();
    }
}

impl QuantixHost {
    pub async fn run_bootstrap_agent(
        &self,
        command: RunBootstrapAgentCommand,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command
            .retry_of_run_id
            .as_deref()
            .is_some_and(|value| !valid_identifier(value))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let cancellation = self.begin_active_agent_run(tender_id.as_str())?;
        let _active = ActiveAgentRunGuard { host: self.clone() };
        let store = self.tender_store(&tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_bootstrap_agent_run(&tender_id, command.retry_of_run_id.as_deref())?;
        self.identify_active_agent_run(&prepared.run_id)?;

        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .complete_agent_run(&tender_id, &prepared, execution)?;
        let inspection = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_agent_run(&prepared.run_id)?;
        Ok(inspection)
    }

    pub fn inspect_agent_runs(
        &self,
        tender_id: &str,
    ) -> Result<Vec<AgentRunInspection>, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let runs = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_agent_runs()?;
        Ok(runs)
    }

    pub fn interrupt_agent_run(
        &self,
        command: InterruptAgentRunCommand,
    ) -> Result<bool, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.run_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(self.cancel_active_agent_run(&command.tender_id, &command.run_id))
    }
}

async fn execute_provider_turn(
    host: &QuantixHost,
    store: &Arc<Mutex<TenderStore>>,
    prepared: &PreparedAgentRun,
    cancellation: CancellationToken,
) -> ProviderExecution {
    let started = Instant::now();
    let mut provider_slot = host.agent_provider().lock().await;
    if provider_slot.is_none() {
        let provider = match CodexProvider::readiness(
            host.process_supervisor(),
            host.runtime_layout().codex_executable(),
            host.application_home(),
        )
        .await
        {
            Ok(provider) => provider,
            Err(failure) => return failed_execution(failure, started),
        };
        *provider_slot = Some(provider);
    }
    match provider_slot
        .as_mut()
        .expect("provider initialized above")
        .begin_run(&prepared.task)
    {
        Ok(()) => {}
        Err(failure) => {
            shutdown_provider(&mut provider_slot).await;
            return failed_execution(failure, started);
        }
    }
    let (thread_ref, resumed) = match provider_slot
        .as_mut()
        .expect("provider initialized above")
        .establish_or_resume_thread(&prepared.workspace, prepared.provider_thread_ref.as_deref())
        .await
    {
        Ok(thread) => thread,
        Err(failure) => {
            shutdown_provider(&mut provider_slot).await;
            return failed_execution(failure, started);
        }
    };
    if store
        .lock()
        .map_err(|_| ())
        .and_then(|mut store| {
            store
                .checkpoint_agent_thread(prepared, &thread_ref, resumed)
                .map_err(|_| ())
        })
        .is_err()
    {
        shutdown_provider(&mut provider_slot).await;
        return failed_execution(process_failure(false), started);
    }
    let checkpoint_store = Arc::clone(store);
    let run_id = prepared.run_id.clone();
    let mut execution = provider_slot
        .as_mut()
        .expect("provider remains available")
        .run_turn(prepared, &thread_ref, cancellation, move |turn_ref| {
            checkpoint_store
                .lock()
                .map_err(|_| outcome_unknown())?
                .checkpoint_agent_turn(&run_id, turn_ref)
                .map_err(|_| outcome_unknown())
        })
        .await;
    execution.provider_thread_ref = Some(thread_ref.clone());
    execution.usage.elapsed_milliseconds =
        Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
    if execution.state == AgentRunState::Indeterminate
        || execution.failure.as_ref().is_some_and(|failure| {
            matches!(
                failure.category,
                ProviderFailureCategory::ProtocolInvalid | ProviderFailureCategory::ProcessFailed
            )
        })
    {
        shutdown_provider(&mut provider_slot).await;
    }
    execution
}

async fn shutdown_provider(provider_slot: &mut Option<CodexProvider>) {
    if let Some(mut provider) = provider_slot.take() {
        let _ = provider.shutdown().await;
    }
}

fn failed_execution(failure: ProviderFailure, started: Instant) -> ProviderExecution {
    ProviderExecution {
        state: AgentRunState::Failed,
        provider_thread_ref: None,
        provider_turn_ref: None,
        events: vec![PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            "Agent Run failed before a Provider Turn completed",
            None,
        )],
        usage: ProviderUsage {
            elapsed_milliseconds: Some(
                started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            ),
            ..ProviderUsage::default()
        },
        failure: Some(failure),
        candidate_payload_json: None,
    }
}

pub(crate) struct CodexProvider {
    conversation: Option<SupervisedConversation>,
}

impl CodexProvider {
    async fn readiness(
        supervisor: &crate::process_supervisor::ProcessSupervisor,
        executable: PathBuf,
        process_directory: &Path,
    ) -> Result<Self, ProviderFailure> {
        let mut conversation = supervisor
            .start_conversation(
                ProcessSpec {
                    executable,
                    arguments: restricted_codex_arguments(),
                    current_directory: Some(process_directory.to_path_buf()),
                    environment: restricted_codex_environment(process_directory)?,
                    inherit_environment: false,
                    stdin: Vec::new(),
                    timeout: PROVIDER_TIMEOUT,
                    stdout_limit: PROVIDER_OUTPUT_LIMIT,
                    stderr_limit: PROVIDER_OUTPUT_LIMIT,
                },
                CancellationToken::new(),
            )
            .await
            .map_err(|_| process_failure(false))?;
        write_rpc(
            &mut conversation,
            &json!({
                "method": "initialize",
                "id": 0,
                "params": {
                    "clientInfo": {
                        "name": "quantix",
                        "title": "Quantix",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {
                        "optOutNotificationMethods": [
                            "item/agentMessage/delta",
                            "item/reasoning/summaryTextDelta",
                            "item/reasoning/summaryPartAdded",
                            "item/reasoning/textDelta"
                        ]
                    }
                }
            }),
        )
        .await
        .map_err(|_| process_failure(false))?;
        let response =
            read_expected_response(&mut conversation, &json!(0), "InitializeResponse").await?;
        if response.get("userAgent").and_then(Value::as_str).is_none() {
            return Err(protocol_failure(false));
        }
        write_rpc(
            &mut conversation,
            &json!({ "method": "initialized", "params": {} }),
        )
        .await
        .map_err(|_| process_failure(false))?;
        Ok(Self {
            conversation: Some(conversation),
        })
    }

    async fn establish_or_resume_thread(
        &mut self,
        workspace: &Path,
        existing_thread_ref: Option<&str>,
    ) -> Result<(String, bool), ProviderFailure> {
        let workspace = workspace.to_string_lossy().into_owned();
        let conversation = self.conversation_mut()?;
        let (method, params, definition, resumed) = if let Some(thread_ref) = existing_thread_ref {
            (
                "thread/resume",
                json!({ "threadId": thread_ref, "excludeTurns": true }),
                "v2/ThreadResumeResponse",
                true,
            )
        } else {
            (
                "thread/start",
                json!({
                    "cwd": workspace,
                    "approvalPolicy": "never",
                    "sandbox": "read-only",
                    "serviceName": "quantix"
                }),
                "v2/ThreadStartResponse",
                false,
            )
        };
        write_rpc(
            conversation,
            &json!({ "method": method, "id": 1, "params": params }),
        )
        .await
        .map_err(|_| process_failure(false))?;
        let result = read_expected_response(conversation, &json!(1), definition).await?;
        let thread_ref = result
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| protocol_failure(false))?;
        Ok((thread_ref.to_owned(), resumed))
    }

    fn begin_run(&mut self, task: &TenderTaskView) -> Result<(), ProviderFailure> {
        self.conversation_mut()?
            .begin_operation(
                Duration::from_secs(task.resource_budget.duration_seconds.into()),
                PROVIDER_OUTPUT_LIMIT,
                PROVIDER_OUTPUT_LIMIT,
            )
            .map_err(|_| process_failure(false))
    }

    async fn run_turn<F>(
        &mut self,
        prepared: &PreparedAgentRun,
        thread_ref: &str,
        cancellation: CancellationToken,
        on_accepted: F,
    ) -> ProviderExecution
    where
        F: FnOnce(&str) -> Result<(), ProviderFailure>,
    {
        let output_schema: Value = match serde_json::from_str(&prepared.task.output_contract_json) {
            Ok(schema) => schema,
            Err(_) => return failed_execution(protocol_failure(false), Instant::now()),
        };
        let instruction_bundle = match provider_instruction_bundle(prepared) {
            Ok(bundle) => bundle,
            Err(failure) => return failed_execution(failure, Instant::now()),
        };
        let conversation = match self.conversation_mut() {
            Ok(conversation) => conversation,
            Err(failure) => return failed_execution(failure, Instant::now()),
        };
        if write_rpc(
            conversation,
            &json!({
                "method": "turn/start",
                "id": 2,
                "params": {
                    "threadId": thread_ref,
                    "input": [{ "type": "text", "text": instruction_bundle }],
                    "cwd": prepared.workspace,
                    "approvalPolicy": "never",
                    "sandboxPolicy": {
                        "type": "readOnly",
                        "networkAccess": false,
                    },
                    "outputSchema": output_schema,
                }
            }),
        )
        .await
        .is_err()
        {
            return failed_execution(process_failure(false), Instant::now());
        }
        let turn_response =
            match read_expected_response(conversation, &json!(2), "v2/TurnStartResponse").await {
                Ok(response) => response,
                Err(failure) => return failed_execution(failure, Instant::now()),
            };
        let turn_ref = match turn_response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            Some(turn_ref) => turn_ref.to_owned(),
            None => return failed_execution(protocol_failure(false), Instant::now()),
        };
        if let Err(failure) = on_accepted(&turn_ref) {
            return ProviderExecution {
                state: AgentRunState::Indeterminate,
                provider_thread_ref: Some(thread_ref.to_owned()),
                provider_turn_ref: Some(turn_ref),
                events: vec![PendingProviderEvent::new(
                    ProviderEventKind::Terminal,
                    "Provider Turn outcome is indeterminate",
                    None,
                )],
                usage: ProviderUsage::default(),
                failure: Some(failure),
                candidate_payload_json: None,
            };
        }
        let mut execution = ProviderExecution {
            state: AgentRunState::Running,
            provider_thread_ref: Some(thread_ref.to_owned()),
            provider_turn_ref: Some(turn_ref.clone()),
            events: Vec::new(),
            usage: ProviderUsage::default(),
            failure: None,
            candidate_payload_json: None,
        };
        let mut interrupt_sent = false;
        let mut final_candidate = None;
        loop {
            let line = tokio::select! {
                _ = cancellation.cancelled(), if !interrupt_sent => {
                    if Self::interrupt(conversation, thread_ref, &turn_ref).await.is_err() {
                        execution.state = AgentRunState::Indeterminate;
                        execution.failure = Some(outcome_unknown());
                        break;
                    }
                    interrupt_sent = true;
                    continue;
                }
                line = conversation.read_line() => line,
            };
            let line = match line {
                Ok(line) => line,
                Err(_) => {
                    execution.state = AgentRunState::Indeterminate;
                    execution.failure = Some(outcome_unknown());
                    break;
                }
            };
            let message = match parse_wire_message(&line) {
                Ok(message) => message,
                Err(_) => {
                    execution.state = AgentRunState::Indeterminate;
                    execution.failure = Some(protocol_failure(true));
                    break;
                }
            };
            if message.get("id").is_some() && message.get("method").is_none() {
                if message.get("id") == Some(&json!(3)) {
                    if response_result(&message, "v2/TurnInterruptResponse").is_err() {
                        execution.state = AgentRunState::Indeterminate;
                        execution.failure = Some(protocol_failure(true));
                        break;
                    }
                    continue;
                }
                execution.state = AgentRunState::Indeterminate;
                execution.failure = Some(protocol_failure(true));
                break;
            }
            let method = match message.get("method").and_then(Value::as_str) {
                Some(method) => method,
                None => {
                    execution.state = AgentRunState::Indeterminate;
                    execution.failure = Some(protocol_failure(true));
                    break;
                }
            };
            if message.get("id").is_some() {
                if handle_control_request(conversation, &message, &mut execution)
                    .await
                    .is_err()
                {
                    execution.state = AgentRunState::Indeterminate;
                    execution.failure = Some(protocol_failure(true));
                    break;
                }
                continue;
            }
            if validate_schema("ServerNotification", &message).is_err() {
                execution.state = AgentRunState::Indeterminate;
                execution.failure = Some(protocol_failure(true));
                break;
            }
            match handle_notification(
                method,
                message.get("params").unwrap_or(&Value::Null),
                &turn_ref,
                &mut execution,
                &mut final_candidate,
            ) {
                Ok(NotificationOutcome::Continue) => {}
                Ok(NotificationOutcome::Terminal) => break,
                Err(_) => {
                    execution.state = AgentRunState::Indeterminate;
                    execution.failure = Some(protocol_failure(true));
                    break;
                }
            }
        }
        let provider_terminal_state = execution.state;
        if execution.state == AgentRunState::Completed {
            match validate_candidate(
                final_candidate.as_deref(),
                &output_schema,
                prepared.task.resource_budget.output_bytes,
            ) {
                Ok(payload) => execution.candidate_payload_json = Some(payload),
                Err(failure) => {
                    execution.state = AgentRunState::Failed;
                    execution.failure = Some(failure);
                }
            }
        }
        execution.events.push(PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            match provider_terminal_state {
                AgentRunState::Completed => "Provider Turn completed",
                AgentRunState::Interrupted => "Provider Turn interrupted",
                AgentRunState::Failed => "Provider Turn failed",
                AgentRunState::Indeterminate => "Provider Turn outcome is indeterminate",
                AgentRunState::Running => "Provider Turn ended without a terminal outcome",
            },
            Some(&turn_ref),
        ));
        if execution.state == AgentRunState::Running {
            execution.state = AgentRunState::Indeterminate;
            execution.failure = Some(outcome_unknown());
        }
        execution
    }

    async fn interrupt(
        conversation: &mut SupervisedConversation,
        thread_ref: &str,
        turn_ref: &str,
    ) -> Result<(), ProviderFailure> {
        write_rpc(
            conversation,
            &json!({
                "method": "turn/interrupt",
                "id": 3,
                "params": { "threadId": thread_ref, "turnId": turn_ref }
            }),
        )
        .await
        .map_err(|_| outcome_unknown())
    }

    #[allow(dead_code)]
    async fn archive_thread(&mut self, thread_ref: &str) -> Result<(), ProviderFailure> {
        let conversation = self.conversation_mut()?;
        write_rpc(
            conversation,
            &json!({
                "method": "thread/archive",
                "id": 4,
                "params": { "threadId": thread_ref }
            }),
        )
        .await
        .map_err(|_| process_failure(false))?;
        read_expected_response(conversation, &json!(4), "v2/ThreadArchiveResponse")
            .await
            .map(|_| ())
    }

    async fn shutdown(&mut self) -> Result<(), ProcessError> {
        let conversation = self
            .conversation
            .take()
            .ok_or(ProcessError::ObservationFailed)?;
        let abort_reason = conversation.failure_termination();
        let output = conversation.finish(abort_reason).await?;
        if output.termination == ProcessTermination::Exited && output.exit_code == Some(0) {
            Ok(())
        } else {
            Err(ProcessError::ObservationFailed)
        }
    }

    fn conversation_mut(&mut self) -> Result<&mut SupervisedConversation, ProviderFailure> {
        self.conversation
            .as_mut()
            .ok_or_else(|| process_failure(false))
    }
}

fn restricted_codex_arguments() -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("app-server"),
        OsString::from("--listen"),
        OsString::from("stdio://"),
        OsString::from("--strict-config"),
        OsString::from("-c"),
        OsString::from("mcp_servers={}"),
        OsString::from("-c"),
        OsString::from("web_search=\"disabled\""),
    ];
    for feature in [
        "apps",
        "browser_use",
        "browser_use_external",
        "browser_use_full_cdp_access",
        "computer_use",
        "hooks",
        "image_generation",
        "in_app_browser",
        "multi_agent",
        "multi_agent_v2",
        "plugins",
        "shell_tool",
        "skill_mcp_dependency_install",
        "skill_search",
        "tool_suggest",
        "unified_exec",
        "workspace_dependencies",
    ] {
        arguments.push(OsString::from("--disable"));
        arguments.push(OsString::from(feature));
    }
    arguments
}

fn restricted_codex_environment(
    application_home: &Path,
) -> Result<Vec<(OsString, OsString)>, ProviderFailure> {
    let engineer_home = application_home
        .parent()
        .ok_or_else(|| process_failure(false))?;
    let codex_home = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| engineer_home.join(".codex"));
    let temporary = application_home.join("staging").join("provider-codex");
    fs::create_dir_all(&temporary).map_err(|_| process_failure(false))?;
    let mut environment = vec![
        (OsString::from("CODEX_HOME"), codex_home.into_os_string()),
        (
            OsString::from("HOME"),
            engineer_home.as_os_str().to_os_string(),
        ),
        (
            OsString::from("USERPROFILE"),
            engineer_home.as_os_str().to_os_string(),
        ),
        (OsString::from("TEMP"), temporary.as_os_str().to_os_string()),
        (OsString::from("TMP"), temporary.into_os_string()),
    ];
    if let Some(system_root) = env::var_os("SYSTEMROOT") {
        environment.push((OsString::from("SYSTEMROOT"), system_root));
    }
    Ok(environment)
}

fn valid_identifier(value: &str) -> bool {
    value.len() == 32
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, 'a'..='f'))
}
