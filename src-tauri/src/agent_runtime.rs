use std::{
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use garde::Validate;
use jiff::Timestamp;
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
pub(crate) mod permissions;
pub(crate) use bootstrap_profile::{bootstrap_profile, bootstrap_task};
use codex_protocol::{
    dynamic_tool_specs, execute_typed_tool, handle_control_request, handle_notification,
    outcome_unknown, parse_wire_message, process_failure, protocol_failure,
    provider_instruction_bundle, read_expected_response, response_result,
    typed_tool_arguments_are_valid, typed_tool_is_known, validate_candidate, validate_schema,
    write_rpc, ControlRequestContext, ControlRequestLedger, NotificationOutcome,
};
use permissions::permission_duration;
pub use permissions::{
    approve_one_run_access, AccessApproval, AccessRequest, AgentAccessRequestStatus,
    AgentAccessRequestView, AgentAccessResolution, AgentRunWorkspaceManifest, DataClassification,
    DataViewManifest, OneRunAccessGrant, PermissionCeiling, PermissionDenialReason,
    PermissionGrant, ThreadExposureSet, ToolIdempotency, ToolSideEffectClass, TypedToolDefinition,
    TypedToolQuota,
};

#[cfg(not(feature = "runtime-fixture"))]
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(feature = "runtime-fixture")]
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(2);
const PROVIDER_OUTPUT_LIMIT: usize = 1024 * 1024;
const PROVIDER_READINESS_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const CODEX_VERSION: &str = "0.147.0";
pub(crate) const CODEX_PROTOCOL_SCHEMA: &str =
    include_str!("../runtime/codex_app_server_protocol.schemas.json");
const SUPPORTED_CHATGPT_PLANS: [&str; 13] = [
    "go",
    "plus",
    "pro",
    "prolite",
    "team",
    "self_serve_business_prolite",
    "self_serve_business_usage_based",
    "business",
    "ent26",
    "enterprise_cbp_automation",
    "enterprise_cbp_usage_based",
    "enterprise",
    "edu",
];

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RequestAgentAccessCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub run_id: String,
    #[garde(skip)]
    pub exact_inputs: Vec<AgentTaskInputReference>,
    #[garde(skip)]
    pub data_scopes: Vec<String>,
    #[garde(skip)]
    pub data_classifications: Vec<DataClassification>,
    #[garde(skip)]
    pub allowed_actions: Vec<String>,
    #[garde(skip)]
    pub allowed_tools: Vec<String>,
    #[garde(length(bytes, min = 1, max = 500))]
    pub purpose: String,
    #[garde(skip)]
    pub recurring: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApproveAgentAccessCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub request_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub run_id: String,
    #[garde(length(bytes, min = 20, max = 40))]
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ResolveAgentAccessCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub request_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub run_id: String,
    #[garde(skip)]
    pub resolution: AgentAccessResolution,
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
    pub data_classifications: Vec<DataClassification>,
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
    TurnRequested,
    TurnStarted,
    UsageObserved,
    RateLimitObserved,
    ControlRequestResolved,
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
            Self::TurnRequested => "turn_requested",
            Self::TurnStarted => "turn_started",
            Self::UsageObserved => "usage_observed",
            Self::RateLimitObserved => "rate_limit_observed",
            Self::ControlRequestResolved => "control_request_resolved",
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
            "turn_requested" => Ok(Self::TurnRequested),
            "turn_started" => Ok(Self::TurnStarted),
            "usage_observed" => Ok(Self::UsageObserved),
            "rate_limit_observed" => Ok(Self::RateLimitObserved),
            "control_request_resolved" => Ok(Self::ControlRequestResolved),
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
    pub correlation_id: Option<String>,
    pub request_fingerprint: Option<String>,
    pub denial_reason: Option<PermissionDenialReason>,
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
    pub rate_limit: Option<ProviderRateLimit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderRateLimitState {
    Available,
    Exhausted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderRateLimitWindow {
    pub used_percent: u32,
    pub window_minutes: Option<u64>,
    pub resets_at_epoch_seconds: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderRateLimit {
    pub state: ProviderRateLimitState,
    pub primary: Option<ProviderRateLimitWindow>,
    pub secondary: Option<ProviderRateLimitWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProviderFailureCategory {
    AuthenticationRequired,
    SubscriptionRequired,
    ProtocolInvalid,
    ProcessFailed,
    RateLimited,
    OutputInvalid,
    Interrupted,
    OutcomeUnknown,
    PermissionDenied,
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
    pub data_scopes: Vec<String>,
    pub data_classification: DataClassification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunInspection {
    pub run_id: String,
    pub retry_of_run_id: Option<String>,
    pub state: AgentRunState,
    pub profile: AgentProfileVersionView,
    pub task: TenderTaskView,
    pub permission_grant: PermissionGrant,
    pub access_requests: Vec<AgentAccessRequestView>,
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
    pub permission_grant: PermissionGrant,
    pub provider_thread_ref: Option<String>,
    pub provider_thread_to_archive: Option<String>,
    pub workspace: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingProviderEvent {
    pub kind: ProviderEventKind,
    pub summary: String,
    pub correlation_id: Option<String>,
    pub request_fingerprint: Option<String>,
    pub denial_reason: Option<PermissionDenialReason>,
    pub opaque_reference: Option<String>,
}

impl PendingProviderEvent {
    fn new(kind: ProviderEventKind, summary: &str, opaque_reference: Option<&str>) -> Self {
        Self {
            kind,
            summary: summary.to_owned(),
            correlation_id: None,
            request_fingerprint: None,
            denial_reason: None,
            opaque_reference: opaque_reference.map(str::to_owned),
        }
    }

    fn with_control_denial(
        mut self,
        correlation_id: String,
        request_fingerprint: String,
        denial_reason: PermissionDenialReason,
    ) -> Self {
        self.correlation_id = Some(correlation_id);
        self.request_fingerprint = Some(request_fingerprint);
        self.denial_reason = Some(denial_reason);
        self
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

type TurnAcceptedCallback = dyn FnOnce(&str) -> Result<(), ProviderFailure> + Send;
type TurnRequestedCallback = dyn FnOnce() -> Result<(), ProviderFailure> + Send;
type TurnEventCallback =
    dyn FnMut(&PendingProviderEvent, &ProviderUsage) -> Result<(), ProviderFailure> + Send;
type TurnDeniedCallback = dyn FnMut(&PendingProviderEvent) -> Result<(), ProviderFailure> + Send;
type TurnToolCallCallback =
    dyn FnMut(&str, &str, &Value) -> Result<Option<String>, ProviderFailure> + Send;

struct RunCallbacks {
    on_requested: Box<TurnRequestedCallback>,
    on_accepted: Box<TurnAcceptedCallback>,
    on_event: Box<TurnEventCallback>,
    on_denied: Box<TurnDeniedCallback>,
    on_tool_call: Box<TurnToolCallCallback>,
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

    pub fn request_agent_access(
        &self,
        command: RequestAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.run_id)
            || !self.agent_run_is_active(tender_id.as_str(), &command.run_id)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let store = self.tender_store(&tender_id)?;
        let request = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .create_agent_access_request(&tender_id, command)?;
        Ok(request)
    }

    pub fn approve_agent_access(
        &self,
        command: ApproveAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.request_id) || !valid_identifier(&command.run_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let run_is_active = self.agent_run_is_active(tender_id.as_str(), &command.run_id);
        let store = self.tender_store(&tender_id)?;
        let decision = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .approve_agent_access_request(&tender_id, command, run_is_active)?;
        Ok(decision)
    }

    pub fn resolve_agent_access(
        &self,
        command: ResolveAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.request_id)
            || !valid_identifier(&command.run_id)
            || !self.agent_run_is_active(tender_id.as_str(), &command.run_id)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let store = self.tender_store(&tender_id)?;
        let decision = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .resolve_agent_access_request(&tender_id, command)?;
        Ok(decision)
    }

    pub fn interrupt_agent_run(
        &self,
        command: InterruptAgentRunCommand,
    ) -> Result<bool, TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.run_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if !self.agent_run_is_active(tender_id.as_str(), &command.run_id) {
            return Ok(false);
        }
        let store = self.tender_store(&tender_id)?;
        let recorded = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .request_agent_run_interruption(&tender_id, &command.run_id)?;
        if !recorded {
            return Ok(false);
        }
        Ok(self.cancel_active_agent_run(tender_id.as_str(), &command.run_id))
    }

    pub(crate) async fn inspect_codex_subscription(
        &self,
        cancellation: CancellationToken,
    ) -> CodexReadiness {
        match self.try_inspect_codex_subscription(cancellation).await {
            Ok(()) => CodexReadiness::Ready,
            Err(failure) if failure.category == ProviderFailureCategory::AuthenticationRequired => {
                CodexReadiness::AuthenticationRequired
            }
            Err(failure) if failure.category == ProviderFailureCategory::SubscriptionRequired => {
                CodexReadiness::SubscriptionRequired
            }
            Err(_) => CodexReadiness::Unavailable,
        }
    }

    async fn try_inspect_codex_subscription(
        &self,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderFailure> {
        let mut provider_slot = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(readiness_interruption_failure()),
            provider_slot = self.agent_provider().lock() => provider_slot,
        };
        if provider_slot.is_none() {
            let provider = CodexProvider::readiness(
                self.process_supervisor(),
                self.runtime_layout().codex_executable(),
                self.application_home(),
                cancellation.clone(),
            )
            .await;
            if cancellation.is_cancelled() {
                if let Ok(mut provider) = provider {
                    let _ = provider.shutdown().await;
                }
                return Err(readiness_interruption_failure());
            }
            *provider_slot = Some(provider?);
            return Ok(());
        }
        let readiness = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(readiness_interruption_failure()),
            readiness = provider_slot
                .as_mut()
                .expect("provider remains available")
                .refresh_readiness() => readiness,
        };
        if readiness.is_err() {
            shutdown_provider(&mut provider_slot).await;
        }
        readiness
    }
}

async fn execute_provider_turn(
    host: &QuantixHost,
    store: &Arc<Mutex<TenderStore>>,
    prepared: &PreparedAgentRun,
    cancellation: CancellationToken,
) -> ProviderExecution {
    let started = Instant::now();
    if let Some(execution) = cancellation_checkpoint(store, prepared, &cancellation, started) {
        return execution;
    }
    let mut provider_slot = host.agent_provider().lock().await;
    if let Some(execution) = cancellation_checkpoint(store, prepared, &cancellation, started) {
        return execution;
    }
    if provider_slot.is_none() {
        let provider = match tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                return interrupted_execution(started);
            }
            provider = CodexProvider::readiness(
                host.process_supervisor(),
                host.runtime_layout().codex_executable(),
                host.application_home(),
                cancellation.clone(),
            ) => provider,
        } {
            Ok(provider) => provider,
            Err(failure) => return failed_execution(failure, started),
        };
        *provider_slot = Some(provider);
    }
    if let Some(execution) = cancellation_checkpoint(store, prepared, &cancellation, started) {
        shutdown_provider(&mut provider_slot).await;
        return execution;
    }
    let operation_limit = match permission_duration(&prepared.permission_grant, Timestamp::now()) {
        Ok(duration) if !duration.is_zero() => duration,
        _ => {
            shutdown_provider(&mut provider_slot).await;
            return failed_execution(permission_failure(), started);
        }
    };
    let operation_deadline = Instant::now()
        .checked_add(operation_limit)
        .unwrap_or_else(Instant::now);
    match provider_slot
        .as_mut()
        .expect("provider initialized above")
        .begin_run(operation_limit)
    {
        Ok(()) => {}
        Err(failure) => {
            shutdown_provider(&mut provider_slot).await;
            return failed_execution(failure, started);
        }
    }
    if let Some(thread_ref) = prepared.provider_thread_to_archive.as_deref() {
        let archive_result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                shutdown_provider(&mut provider_slot).await;
                return interrupted_execution(started);
            }
            result = provider_slot
                .as_mut()
                .expect("provider initialized above")
                .archive_thread(thread_ref) => result,
        };
        if let Err(failure) = archive_result {
            shutdown_provider(&mut provider_slot).await;
            return failed_execution(failure, started);
        }
        if store
            .lock()
            .map_err(|_| ())
            .and_then(|mut store| {
                store
                    .checkpoint_provider_thread_archived(prepared, thread_ref)
                    .map_err(|_| ())
            })
            .is_err()
        {
            shutdown_provider(&mut provider_slot).await;
            return failed_execution(process_failure(false), started);
        }
    }
    if let Some(execution) = cancellation_checkpoint(store, prepared, &cancellation, started) {
        shutdown_provider(&mut provider_slot).await;
        return execution;
    }
    let working_area = prepared
        .workspace
        .join(&prepared.permission_grant.workspace.working_area);
    let thread_result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => {
            shutdown_provider(&mut provider_slot).await;
            return interrupted_execution(started);
        }
        result = provider_slot
            .as_mut()
            .expect("provider initialized above")
            .establish_or_resume_thread(
                &working_area,
                &prepared.permission_grant,
                prepared.provider_thread_ref.as_deref(),
            ) => result,
    };
    let (thread_ref, resumed) = match thread_result {
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
    if let Some(execution) = cancellation_checkpoint(store, prepared, &cancellation, started) {
        shutdown_provider(&mut provider_slot).await;
        return execution;
    }
    let requested_store = Arc::clone(store);
    let checkpoint_store = Arc::clone(store);
    let event_store = Arc::clone(store);
    let denial_store = Arc::clone(store);
    let tool_store = Arc::clone(store);
    let requested_run_id = prepared.run_id.clone();
    let run_id = prepared.run_id.clone();
    let event_run_id = prepared.run_id.clone();
    let denial_run_id = prepared.run_id.clone();
    let tool_run_id = prepared.run_id.clone();
    let tool_prepared = prepared.clone();
    let remaining_operation = operation_deadline.saturating_duration_since(Instant::now());
    if remaining_operation.is_zero() {
        shutdown_provider(&mut provider_slot).await;
        return failed_execution(permission_failure(), started);
    }
    let mut execution = provider_slot
        .as_mut()
        .expect("provider remains available")
        .run_turn(
            prepared,
            &thread_ref,
            remaining_operation,
            cancellation,
            RunCallbacks {
                on_requested: Box::new(move || {
                    requested_store
                        .lock()
                        .map_err(|_| process_failure(false))?
                        .checkpoint_agent_turn_requested(&requested_run_id)
                        .map_err(|_| process_failure(false))
                }),
                on_accepted: Box::new(move |turn_ref| {
                    checkpoint_store
                        .lock()
                        .map_err(|_| outcome_unknown())?
                        .checkpoint_agent_turn(&run_id, turn_ref)
                        .map_err(|_| outcome_unknown())
                }),
                on_event: Box::new(move |event, usage| {
                    event_store
                        .lock()
                        .map_err(|_| outcome_unknown())?
                        .checkpoint_agent_provider_event(&event_run_id, event, usage)
                        .map_err(|_| outcome_unknown())
                }),
                on_denied: Box::new(move |event| {
                    denial_store
                        .lock()
                        .map_err(|_| outcome_unknown())?
                        .checkpoint_agent_control_denial(&denial_run_id, event)
                        .map_err(|_| outcome_unknown())
                }),
                on_tool_call: Box::new(move |correlation_id, tool_name, arguments| {
                    if !typed_tool_is_known(tool_name) {
                        return Ok(None);
                    }
                    if !typed_tool_arguments_are_valid(tool_name, arguments)? {
                        return Ok(None);
                    }
                    let authorized = tool_store
                        .lock()
                        .map_err(|_| outcome_unknown())?
                        .authorize_agent_typed_tool(&tool_run_id, correlation_id, tool_name)
                        .map_err(|_| outcome_unknown())?;
                    if !authorized {
                        return Ok(None);
                    }
                    match execute_typed_tool(&tool_prepared, tool_name, arguments) {
                        Ok(output) => {
                            tool_store
                                .lock()
                                .map_err(|_| outcome_unknown())?
                                .record_agent_typed_tool_execution(
                                    &tool_run_id,
                                    correlation_id,
                                    tool_name,
                                    true,
                                )
                                .map_err(|_| outcome_unknown())?;
                            Ok(Some(output))
                        }
                        Err(failure) => {
                            tool_store
                                .lock()
                                .map_err(|_| outcome_unknown())?
                                .record_agent_typed_tool_execution(
                                    &tool_run_id,
                                    correlation_id,
                                    tool_name,
                                    false,
                                )
                                .map_err(|_| outcome_unknown())?;
                            Err(failure)
                        }
                    }
                }),
            },
        )
        .await;
    execution.provider_thread_ref = Some(thread_ref.clone());
    execution.usage.elapsed_milliseconds =
        Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
    if matches!(
        execution.state,
        AgentRunState::Interrupted | AgentRunState::Indeterminate
    ) || execution.failure.as_ref().is_some_and(|failure| {
        matches!(
            failure.category,
            ProviderFailureCategory::AuthenticationRequired
                | ProviderFailureCategory::SubscriptionRequired
                | ProviderFailureCategory::ProtocolInvalid
                | ProviderFailureCategory::ProcessFailed
        )
    }) {
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

fn indeterminate_execution(
    thread_ref: &str,
    turn_ref: Option<String>,
    failure: ProviderFailure,
    started: Instant,
) -> ProviderExecution {
    ProviderExecution {
        state: AgentRunState::Indeterminate,
        provider_thread_ref: Some(thread_ref.to_owned()),
        provider_turn_ref: turn_ref,
        events: vec![PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            "Provider Turn acceptance or outcome is indeterminate",
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

fn interrupted_execution(started: Instant) -> ProviderExecution {
    ProviderExecution {
        state: AgentRunState::Interrupted,
        provider_thread_ref: None,
        provider_turn_ref: None,
        events: vec![PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            "Agent Run interrupted before Provider Turn acceptance",
            None,
        )],
        usage: ProviderUsage {
            elapsed_milliseconds: Some(
                started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            ),
            ..ProviderUsage::default()
        },
        failure: Some(interruption_failure()),
        candidate_payload_json: None,
    }
}

fn run_cancellation_requested(
    store: &Arc<Mutex<TenderStore>>,
    prepared: &PreparedAgentRun,
    cancellation: &CancellationToken,
) -> Result<bool, TenderCommandError> {
    if cancellation.is_cancelled() {
        return Ok(true);
    }
    store
        .lock()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
        .agent_run_cancellation_requested(&prepared.run_id)
}

fn cancellation_checkpoint(
    store: &Arc<Mutex<TenderStore>>,
    prepared: &PreparedAgentRun,
    cancellation: &CancellationToken,
    started: Instant,
) -> Option<ProviderExecution> {
    match run_cancellation_requested(store, prepared, cancellation) {
        Ok(false) => None,
        Ok(true) => Some(interrupted_execution(started)),
        Err(_) => Some(failed_execution(process_failure(false), started)),
    }
}

fn interruption_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::Interrupted,
        false,
        "Start a new Agent Run only if the Tender Task still requires this work.",
        Some("The Engineer User interrupted the Agent Run."),
    )
}

fn readiness_interruption_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::Interrupted,
        false,
        "Check Codex subscription readiness again when preparation resumes.",
        Some("Codex subscription readiness was interrupted."),
    )
}

fn permission_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::PermissionDenied,
        false,
        "Create a new Agent Run with a current Permission Grant.",
        Some("The Agent Run Permission Grant expired."),
    )
}

fn authentication_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::AuthenticationRequired,
        true,
        "Connect the Engineer User's Codex-managed ChatGPT subscription, then retry.",
        Some("Codex-managed ChatGPT authentication is required."),
    )
}

fn subscription_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::SubscriptionRequired,
        true,
        "Connect an eligible Codex-managed ChatGPT subscription, then retry.",
        Some("The active Codex account is not an eligible ChatGPT subscription."),
    )
}

fn turn_acceptance_unknown() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::OutcomeUnknown,
        false,
        "Resolve the quarantined Agent Run before retrying.",
        Some("The Provider Turn may have started, but its identity and outcome could not be established."),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodexReadiness {
    Ready,
    AuthenticationRequired,
    SubscriptionRequired,
    Unavailable,
}

fn chatgpt_subscription_is_supported(account_type: Option<&str>, plan_type: Option<&str>) -> bool {
    account_type == Some("chatgpt")
        && plan_type.is_some_and(|plan| SUPPORTED_CHATGPT_PLANS.contains(&plan))
}

fn codex_user_agent_is_supported(user_agent: &str) -> bool {
    let server_identity = user_agent
        .split_once(" (")
        .map_or(user_agent, |(identity, _)| identity);
    server_identity
        .rsplit_once('/')
        .is_some_and(|(product, version)| !product.is_empty() && version == CODEX_VERSION)
}

fn controlled_codex_environment(application_home: &Path) -> io::Result<Vec<(OsString, OsString)>> {
    let engineer_home = application_home.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Application Home must have an Engineer home parent",
        )
    })?;
    let staging = application_home.join("staging").join("provider-codex");
    fs::create_dir_all(&staging)?;
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| engineer_home.join(".codex"));
    let mut environment = vec![
        (OsString::from("CODEX_HOME"), codex_home.into_os_string()),
        (OsString::from("HOME"), engineer_home.as_os_str().to_owned()),
        (
            OsString::from("USERPROFILE"),
            engineer_home.as_os_str().to_owned(),
        ),
        (OsString::from("TEMP"), staging.clone().into_os_string()),
        (OsString::from("TMP"), staging.clone().into_os_string()),
        (OsString::from("TMPDIR"), staging.into_os_string()),
    ];
    for name in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            environment.push((OsString::from(name), value));
        }
    }
    Ok(environment)
}

pub(crate) struct CodexProvider {
    conversation: Option<SupervisedConversation>,
}

impl CodexProvider {
    async fn readiness(
        supervisor: &crate::process_supervisor::ProcessSupervisor,
        executable: PathBuf,
        process_directory: &Path,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderFailure> {
        let mut conversation = supervisor
            .start_conversation(
                ProcessSpec {
                    executable,
                    arguments: restricted_codex_arguments(),
                    current_directory: Some(process_directory.to_path_buf()),
                    environment: controlled_codex_environment(process_directory)
                        .map_err(|_| process_failure(false))?,
                    inherit_environment: false,
                    stdin: Vec::new(),
                    timeout: PROVIDER_TIMEOUT,
                    stdout_limit: PROVIDER_OUTPUT_LIMIT,
                    stderr_limit: PROVIDER_OUTPUT_LIMIT,
                },
                cancellation,
            )
            .await
            .map_err(|_| process_failure(false))?;
        if let Err(failure) = Self::initialize(&mut conversation).await {
            let termination = conversation
                .failure_termination()
                .unwrap_or(ProcessTermination::Cancelled);
            let _ = conversation.finish(Some(termination)).await;
            return Err(failure);
        }
        Ok(Self {
            conversation: Some(conversation),
        })
    }

    async fn refresh_readiness(&mut self) -> Result<(), ProviderFailure> {
        let conversation = self.conversation_mut()?;
        conversation
            .begin_operation(
                PROVIDER_READINESS_TIMEOUT,
                PROVIDER_OUTPUT_LIMIT,
                PROVIDER_OUTPUT_LIMIT,
            )
            .map_err(|_| process_failure(false))?;
        Self::verify_subscription(conversation).await
    }

    async fn initialize(conversation: &mut SupervisedConversation) -> Result<(), ProviderFailure> {
        write_rpc(
            conversation,
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
                        "experimentalApi": true,
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
            read_expected_response(conversation, &json!(0), "InitializeResponse").await?;
        if !response
            .get("userAgent")
            .and_then(Value::as_str)
            .is_some_and(codex_user_agent_is_supported)
        {
            return Err(protocol_failure(false));
        }
        write_rpc(
            conversation,
            &json!({ "method": "initialized", "params": {} }),
        )
        .await
        .map_err(|_| process_failure(false))?;
        Self::verify_subscription(conversation).await
    }

    async fn verify_subscription(
        conversation: &mut SupervisedConversation,
    ) -> Result<(), ProviderFailure> {
        write_rpc(
            conversation,
            &json!({
                "method": "account/read",
                "id": 1,
                "params": { "refreshToken": true }
            }),
        )
        .await
        .map_err(|_| process_failure(false))?;
        let account =
            read_expected_response(conversation, &json!(1), "v2/GetAccountResponse").await?;
        let account_type = account.pointer("/account/type").and_then(Value::as_str);
        if account_type.is_none() {
            return Err(authentication_failure());
        }
        let plan_type = account.pointer("/account/planType").and_then(Value::as_str);
        if !chatgpt_subscription_is_supported(account_type, plan_type) {
            return Err(subscription_failure());
        }
        let mut cursor: Option<String> = None;
        for page in 0_u32..100 {
            let id = json!(2 + page);
            write_rpc(
                conversation,
                &json!({
                    "method": "model/list",
                    "id": id,
                    "params": {
                        "cursor": cursor,
                        "limit": 100,
                        "includeHidden": false
                    }
                }),
            )
            .await
            .map_err(|_| process_failure(false))?;
            let models = read_expected_response(conversation, &id, "v2/ModelListResponse").await?;
            let has_usable_model =
                models
                    .get("data")
                    .and_then(Value::as_array)
                    .is_some_and(|models| {
                        models.iter().any(|model| {
                            model.get("hidden").and_then(Value::as_bool) == Some(false)
                                && model
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .is_some_and(|id| !id.is_empty())
                                && model
                                    .get("inputModalities")
                                    .and_then(Value::as_array)
                                    .is_none_or(|modalities| {
                                        modalities
                                            .iter()
                                            .any(|value| value.as_str() == Some("text"))
                                    })
                        })
                    });
            if has_usable_model {
                return Ok(());
            }
            cursor = models
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Err(protocol_failure(false))
    }

    async fn establish_or_resume_thread(
        &mut self,
        workspace: &Path,
        grant: &PermissionGrant,
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
            let dynamic_tools = dynamic_tool_specs(grant)?;
            (
                "thread/start",
                json!({
                    "cwd": workspace,
                    "approvalPolicy": "never",
                    "sandbox": "workspaceWrite",
                    "serviceName": "quantix"
                    ,"dynamicTools": dynamic_tools
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

    fn begin_run(&mut self, operation_limit: Duration) -> Result<(), ProviderFailure> {
        self.conversation_mut()?
            .begin_operation(
                operation_limit,
                PROVIDER_OUTPUT_LIMIT,
                PROVIDER_OUTPUT_LIMIT,
            )
            .map_err(|_| process_failure(false))
    }

    async fn run_turn(
        &mut self,
        prepared: &PreparedAgentRun,
        thread_ref: &str,
        operation_limit: Duration,
        cancellation: CancellationToken,
        callbacks: RunCallbacks,
    ) -> ProviderExecution {
        if cancellation.is_cancelled() {
            return interrupted_execution(Instant::now());
        }
        let RunCallbacks {
            on_requested,
            on_accepted,
            mut on_event,
            mut on_denied,
            mut on_tool_call,
        } = callbacks;
        let operation_started = Instant::now();
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
        let turn_start = json!({
            "method": "turn/start",
            "id": 2,
            "params": {
                "threadId": thread_ref,
                "input": [{ "type": "text", "text": instruction_bundle }],
                "cwd": prepared.workspace.join(&prepared.permission_grant.workspace.working_area),
                "approvalPolicy": "never",
                "sandboxPolicy": {
                    "type": "workspaceWrite",
                    "networkAccess": false,
                    "writableRoots": [
                        prepared.workspace.join(&prepared.permission_grant.workspace.staged_outputs)
                    ],
                },
                "outputSchema": output_schema,
            }
        });
        if let Err(failure) = on_requested() {
            return failed_execution(failure, operation_started);
        }
        let write_result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                return indeterminate_execution(
                    thread_ref,
                    None,
                    turn_acceptance_unknown(),
                    operation_started,
                );
            }
            result = write_rpc(conversation, &turn_start) => result,
        };
        if write_result.is_err() {
            return indeterminate_execution(
                thread_ref,
                None,
                turn_acceptance_unknown(),
                operation_started,
            );
        }
        let turn_start_id = json!(2);
        let turn_response = match tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                return indeterminate_execution(
                    thread_ref,
                    None,
                    turn_acceptance_unknown(),
                    operation_started,
                );
            }
            response = read_expected_response(conversation, &turn_start_id, "v2/TurnStartResponse") => response,
        } {
            Ok(response) => response,
            Err(_) => {
                return indeterminate_execution(
                    thread_ref,
                    None,
                    turn_acceptance_unknown(),
                    operation_started,
                )
            }
        };
        let turn_ref = match turn_response
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            Some(turn_ref) => turn_ref.to_owned(),
            None => {
                return indeterminate_execution(
                    thread_ref,
                    None,
                    turn_acceptance_unknown(),
                    operation_started,
                )
            }
        };
        if let Err(failure) = on_accepted(&turn_ref) {
            return indeterminate_execution(thread_ref, Some(turn_ref), failure, operation_started);
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
        let mut control_requests = ControlRequestLedger::default();
        loop {
            let line = tokio::select! {
                biased;
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
                    if operation_started.elapsed() >= operation_limit {
                        execution.state = AgentRunState::Failed;
                        execution.failure = Some(permission_failure());
                    } else {
                        execution.state = AgentRunState::Indeterminate;
                        execution.failure = Some(outcome_unknown());
                    }
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
                    if response_result(&message, "v2/TurnInterruptResponse", true).is_err() {
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
                if handle_control_request(
                    conversation,
                    &message,
                    ControlRequestContext {
                        grant: &prepared.permission_grant,
                        expected_thread_ref: thread_ref,
                        expected_turn_ref: &turn_ref,
                        expired: operation_started.elapsed() >= operation_limit,
                        ledger: &mut control_requests,
                        on_denied: &mut on_denied,
                        on_tool_call: &mut on_tool_call,
                    },
                )
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
            let outcome = match handle_notification(
                method,
                message.get("params").unwrap_or(&Value::Null),
                &turn_ref,
                &mut execution,
                &mut final_candidate,
            ) {
                Ok(outcome) => outcome,
                Err(_) => {
                    execution.state = AgentRunState::Indeterminate;
                    execution.failure = Some(protocol_failure(true));
                    break;
                }
            };
            if stream_provider_events(&mut execution, &mut on_event).is_err() {
                execution.state = AgentRunState::Indeterminate;
                execution.failure = Some(outcome_unknown());
                break;
            }
            if matches!(outcome, NotificationOutcome::Terminal) {
                break;
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
        if read_expected_response(conversation, &json!(4), "v2/ThreadArchiveResponse")
            .await
            .is_ok()
        {
            return Ok(());
        }
        Self::confirm_thread_archived(conversation, thread_ref).await
    }

    async fn confirm_thread_archived(
        conversation: &mut SupervisedConversation,
        thread_ref: &str,
    ) -> Result<(), ProviderFailure> {
        let mut cursor: Option<String> = None;
        for page in 0_u32..100 {
            let id = json!(5 + page);
            write_rpc(
                conversation,
                &json!({
                    "method": "thread/list",
                    "id": id,
                    "params": {
                        "archived": true,
                        "cursor": cursor,
                        "limit": 100
                    }
                }),
            )
            .await
            .map_err(|_| outcome_unknown())?;
            let result = read_expected_response(conversation, &id, "v2/ThreadListResponse").await?;
            let threads = result
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| protocol_failure(false))?;
            if threads
                .iter()
                .any(|thread| thread.get("id").and_then(Value::as_str) == Some(thread_ref))
            {
                return Ok(());
            }
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Err(protocol_failure(false))
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

fn stream_provider_events(
    execution: &mut ProviderExecution,
    on_event: &mut TurnEventCallback,
) -> Result<(), ProviderFailure> {
    let events = std::mem::take(&mut execution.events);
    for event in events {
        if let Err(failure) = on_event(&event, &execution.usage) {
            execution.events.push(event);
            return Err(failure);
        }
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    value.len() == 32
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, 'a'..='f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "explicitly contacts the Engineer User's local Codex app-server"]
    async fn codex_subscription_live_smoke() {
        let home_variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let engineer_home = std::env::var_os(home_variable)
            .map(PathBuf::from)
            .expect("the Engineer User home directory must be available");
        let application_home = engineer_home.join(".quantix");
        let executable = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("runtime")
            .join("bin")
            .join(if cfg!(windows) { "codex.exe" } else { "codex" });
        assert!(
            executable.is_file(),
            "prepare the pinned Codex runtime before running the live smoke check"
        );

        let supervisor = crate::process_supervisor::ProcessSupervisor;
        let mut provider = CodexProvider::readiness(
            &supervisor,
            executable,
            &application_home,
            CancellationToken::new(),
        )
        .await
        .unwrap_or_else(|failure| panic!("Codex subscription is not ready: {failure:?}"));
        provider
            .shutdown()
            .await
            .expect("the Codex app-server must shut down cleanly");
    }
}
