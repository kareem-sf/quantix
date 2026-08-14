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
    tender_store::{
        lock_mutex_with_check, require_setup, BasisOfEstimateReviewResult,
        BidDecisionApprovalHistoryPage, BidDecisionApprovalInvalidationResult,
        BidDecisionApprovalResult, BidDecisionPackageInspection, BidDecisionPackageRecordCategory,
        BidDecisionPackageRecordPage, BidDecisionPackageReviewResult,
        BidDecisionReturnReworkResult, BidPackageOperationBudget, CalculationRuleReviewResult,
        ComplianceMatrixPage, CostEstimatorBasisResult, CostEstimatorCalculationResult,
        CreateBidDecisionPackageCommand, CreateTenderEngineerEntryCommand,
        DecideBidDecisionPackageCommand, DecideTenderRecordCommand, ExternalRfiReviewResult,
        InspectBidDecisionApprovalHistoryCommand, InvalidateBidDecisionApprovalCommand,
        PricedCostBaselineReviewResult, PricingAdjustmentReviewResult, ProductionTaskRunResult,
        ProductionTaskState, ResolveBidDecisionReturnReworkCommand,
        RunBasisOfEstimateReviewCommand, RunBidDecisionPackageReviewCommand,
        RunCalculationRuleReviewCommand, RunCostEstimatorBasisCommand,
        RunCostEstimatorCalculationCommand, RunExternalRfiReviewCommand,
        RunPricedCostBaselineReviewCommand, RunPricingAdjustmentReviewCommand,
        RunProductionTaskCommand, RunSubmissionSectionReviewCommand,
        RunTenderRecordExtractionCommand, RunTenderRecordReviewCommand,
        SubmissionSectionReviewRunResult, TenderCommandError, TenderErrorCode, TenderId,
        TenderRecordAuthority, TenderRecordDecisionResult, TenderRecordExtractionResult,
        TenderRecordPage, TenderRecordReviewResult, TenderStore,
    },
    QuantixHost,
};

mod bootstrap_profile;
mod codex_actor;
mod codex_protocol;
pub(crate) mod permissions;
pub(crate) use bootstrap_profile::{bootstrap_profile, bootstrap_task};
pub(crate) use codex_actor::CodexProvider;
use codex_protocol::{
    execute_typed_tool, outcome_unknown, process_failure, protocol_failure, read_expected_response,
    typed_tool_arguments_are_valid, typed_tool_is_known, write_rpc,
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
pub struct InspectAgentRunHistoryCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(skip)]
    pub before_sequence: Option<u64>,
    #[garde(range(min = 1, max = 4))]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectAgentRunCommand {
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
pub enum AgentRunRecoveryDisposition {
    RetryTask,
    CloseTask,
}

impl AgentRunRecoveryDisposition {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::RetryTask => "retry_task",
            Self::CloseTask => "close_task",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "retry_task" => Ok(Self::RetryTask),
            "close_task" => Ok(Self::CloseTask),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ResolveIndeterminateAgentRunCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub run_id: String,
    #[garde(skip)]
    pub disposition: AgentRunRecoveryDisposition,
    #[garde(length(bytes, min = 1, max = 500))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunRecoveryDecision {
    pub run_id: String,
    pub disposition: AgentRunRecoveryDisposition,
    pub rationale: String,
    pub decided_by: String,
    pub decided_at: String,
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
    Verified,
    Rejected,
    Stale,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BootstrapRole {
    TenderOfficeCoordinator,
    DocumentController,
    TenderAnalyst,
    IndependentReviewer,
}

impl BootstrapRole {
    pub(crate) const ALL: [Self; 4] = [
        Self::TenderOfficeCoordinator,
        Self::DocumentController,
        Self::TenderAnalyst,
        Self::IndependentReviewer,
    ];

    pub(crate) const fn stable_identity(self) -> &'static str {
        match self {
            Self::TenderOfficeCoordinator => "quantix.bootstrap.tender-office-coordinator",
            Self::DocumentController => "quantix.bootstrap.document-controller",
            Self::TenderAnalyst => "quantix.bootstrap.tender-analyst",
            Self::IndependentReviewer => "quantix.bootstrap.independent-reviewer",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BootstrapAuthority {
    PreBidAnalysis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BootstrapTeamMember {
    pub role: BootstrapRole,
    pub authority: BootstrapAuthority,
    pub active: bool,
    pub profile: AgentProfileVersionView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentProfileVersionView {
    pub profile_id: String,
    pub version: u32,
    pub identity: String,
    pub profession: String,
    pub seniority: String,
    pub capabilities: Vec<String>,
    pub objective: String,
    pub behavior: String,
    pub skepticism: String,
    pub risk_tolerance: String,
    pub instructions: String,
    pub output_contract_json: String,
    pub review_policy: String,
    pub permissions: AgentRunPermissions,
    pub prohibited_actions: Vec<String>,
    pub resource_budget: AgentResourceBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentProfileStatus {
    Proposed,
    Active,
    Suspended,
    Retired,
}

impl AgentProfileStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Retired => "retired",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "proposed" => Ok(Self::Proposed),
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            "retired" => Ok(Self::Retired),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
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
    pub linked_retry_supported: bool,
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
    pub recovery_decision: Option<AgentRunRecoveryDecision>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunHistoryItem {
    pub run_sequence: u64,
    pub run: AgentRunSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunHistoryPage {
    pub items: Vec<AgentRunHistoryItem>,
    pub next_before_sequence: Option<u64>,
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunActivity {
    pub run_count: u64,
    pub event_count: u64,
    pub running_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunSummary {
    pub run_id: String,
    pub retry_of_run_id: Option<String>,
    pub has_linked_retry: bool,
    pub linked_retry_supported: bool,
    pub state: AgentRunState,
    pub profile_identity: String,
    pub profile_profession: String,
    pub profile_version: u32,
    pub task_id: String,
    pub provider_thread_ref: Option<String>,
    pub provider_turn_ref: Option<String>,
    pub usage: ProviderUsage,
    pub failure: Option<ProviderFailure>,
    pub has_proposed_result: bool,
    pub recovery_decision: Option<AgentRunRecoveryDecision>,
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
    lease_id: String,
}

type TurnAcceptedCallback = dyn FnOnce(&str) -> Result<(), ProviderFailure> + Send;
type TurnRequestedCallback = dyn FnOnce() -> Result<(), ProviderFailure> + Send;
type TurnEventCallback =
    dyn FnMut(&PendingProviderEvent, &ProviderUsage) -> Result<(), ProviderFailure> + Send;
type TurnDeniedCallback = dyn FnMut(&PendingProviderEvent) -> Result<(), ProviderFailure> + Send;
type TurnToolCallCallback =
    dyn FnMut(&str, &str, &Value) -> Result<Option<String>, ProviderFailure> + Send;
type ThreadArchivedCallback = dyn FnOnce(&str) -> Result<(), ProviderFailure> + Send;
type ThreadEstablishedCallback = dyn FnOnce(&str, bool) -> Result<(), ProviderFailure> + Send;

struct RunCallbacks {
    on_thread_archived: Box<ThreadArchivedCallback>,
    on_thread_established: Box<ThreadEstablishedCallback>,
    on_requested: Box<TurnRequestedCallback>,
    on_accepted: Box<TurnAcceptedCallback>,
    on_event: Box<TurnEventCallback>,
    on_denied: Box<TurnDeniedCallback>,
    on_tool_call: Box<TurnToolCallCallback>,
}

impl Drop for ActiveAgentRunGuard {
    fn drop(&mut self) {
        self.host.finish_active_agent_run(&self.lease_id);
    }
}

impl QuantixHost {
    pub fn inspect_bootstrap_team(
        &self,
        tender_id: &str,
    ) -> Result<Vec<BootstrapTeamMember>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let team = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_bootstrap_team()?;
        Ok(team)
    }

    pub async fn run_bootstrap_agent(
        &self,
        command: RunBootstrapAgentCommand,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command
            .retry_of_run_id
            .as_deref()
            .is_some_and(|value| !valid_identifier(value))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(&tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_bootstrap_agent_run(
                &tender_id,
                command.retry_of_run_id.as_deref(),
                self.provider_subscription_capacity_is_exhausted(),
            )?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;

        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .complete_agent_run(&tender_id, &prepared, execution)?;
        let inspection = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_agent_run(&prepared.run_id)?;
        let production_task = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .production_task_and_state_for_run(&prepared.run_id)?;
        let production_retry = production_task.is_some();
        drop(_active);
        if let Some((production_task_id, state)) = production_task {
            if matches!(
                state,
                ProductionTaskState::ReviewReady | ProductionTaskState::RemediationReady
            ) {
                if let Err(error) = self
                    .run_production_task_attempt(&tender_id, &production_task_id)
                    .await
                {
                    let exhausted = store
                        .lock()
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                        .production_task_state(&production_task_id)?
                        == Some(ProductionTaskState::AttemptLimitReached);
                    if !exhausted {
                        return Err(error);
                    }
                }
            }
        }
        if production_retry {
            self.start_production_scheduler(tender_id.as_str().to_owned());
        }
        Ok(inspection)
    }

    pub async fn run_production_task(
        &self,
        command: RunProductionTaskCommand,
    ) -> Result<ProductionTaskRunResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.run_production_task_attempt(&tender_id, &command.production_task_id)
            .await
    }

    async fn run_production_task_attempt(
        &self,
        tender_id: &TenderId,
        production_task_id: &str,
    ) -> Result<ProductionTaskRunResult, TenderCommandError> {
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), true)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_production_task_run(
                tender_id,
                production_task_id,
                None,
                self.provider_subscription_capacity_is_exhausted(),
            )?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        store.complete_agent_run(tender_id, &prepared, execution)?;
        let run = store.inspect_agent_run(&prepared.run_id)?;
        let budget = BidPackageOperationBudget::for_tender(tender_id);
        let production = store
            .inspect_tender_production(budget)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let task = production
            .tasks
            .into_iter()
            .find(|task| task.production_task_id == production_task_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        Ok(ProductionTaskRunResult { run, task })
    }

    pub(crate) fn start_production_scheduler(&self, tender_id: String) {
        let host = self.clone();
        tokio::spawn(async move {
            let _ = host.schedule_tender_production(&tender_id).await;
        });
    }

    pub async fn schedule_tender_production(
        &self,
        tender_id: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        TenderId::parse(tender_id)?;
        if !self.claim_production_scheduler(tender_id) {
            return Ok(());
        }
        let result = self.schedule_ready_production_tasks(tender_id).await;
        self.release_production_scheduler(tender_id);
        result
    }

    async fn schedule_ready_production_tasks(
        &self,
        tender_id: &str,
    ) -> Result<(), TenderCommandError> {
        loop {
            if self.provider_subscription_capacity_is_exhausted() {
                return Ok(());
            }
            let production = self.inspect_tender_production(tender_id)?;
            let Some(production) = production.filter(|production| production.active) else {
                return Ok(());
            };
            let mut scheduled_profiles = Vec::with_capacity(2);
            let mut schedulable = Vec::with_capacity(2);
            for task in production.tasks.into_iter().filter(|task| {
                matches!(
                    task.state,
                    ProductionTaskState::Ready
                        | ProductionTaskState::ReviewReady
                        | ProductionTaskState::RemediationReady
                )
            }) {
                let profile_id = if task.state == ProductionTaskState::ReviewReady {
                    task.task
                        .review_profile_id
                        .as_ref()
                        .unwrap_or(&task.task.profile_id)
                } else {
                    &task.task.profile_id
                };
                if scheduled_profiles
                    .iter()
                    .any(|profile| profile == profile_id)
                {
                    continue;
                }
                scheduled_profiles.push(profile_id.clone());
                schedulable.push(task.production_task_id);
                if schedulable.len() == 2 {
                    break;
                }
            }
            if schedulable.is_empty() {
                return Ok(());
            }
            let mut tasks = tokio::task::JoinSet::new();
            let first_production_task_id = schedulable[0].clone();
            for (index, production_task_id) in schedulable.into_iter().enumerate() {
                if index == 1 {
                    let mut first_turn_accepted = false;
                    for _ in 0..400 {
                        first_turn_accepted = self
                            .production_task_turn_accepted(tender_id, &first_production_task_id)?;
                        if first_turn_accepted {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    if !first_turn_accepted {
                        break;
                    }
                }
                let host = self.clone();
                let tender_id = tender_id.to_owned();
                tasks.spawn(async move {
                    host.run_production_task(RunProductionTaskCommand {
                        tender_id,
                        production_task_id,
                    })
                    .await
                });
            }
            let mut progressed = false;
            while let Some(result) = tasks.join_next().await {
                progressed |= result.is_ok_and(|result| result.is_ok());
            }
            if !progressed {
                return Ok(());
            }
        }
    }

    fn production_task_turn_accepted(
        &self,
        tender_id: &str,
        production_task_id: &str,
    ) -> Result<bool, TenderCommandError> {
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let accepted = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .production_task_turn_accepted(production_task_id)?;
        Ok(accepted)
    }

    pub async fn run_tender_record_extraction(
        &self,
        command: RunTenderRecordExtractionCommand,
    ) -> Result<TenderRecordExtractionResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(&tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_tender_record_extraction_run(
                &tender_id,
                &command.evidence,
                &command.authorities,
            )?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;

        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        store.complete_agent_run(&tender_id, &prepared, execution)?;
        Ok(TenderRecordExtractionResult {
            run: store.inspect_agent_run(&prepared.run_id)?,
            published_record_count: store.count_tender_records_by_run(&prepared.run_id)?,
        })
    }

    pub async fn run_tender_record_review(
        &self,
        command: RunTenderRecordReviewCommand,
    ) -> Result<TenderRecordReviewResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.record_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(&tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_tender_record_review_run(&tender_id, &command.record_id, command.version)?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        store.complete_agent_run(&tender_id, &prepared, execution)?;
        Ok(TenderRecordReviewResult {
            run: store.inspect_agent_run(&prepared.run_id)?,
            record: store.inspect_tender_record_version(&command.record_id, command.version)?,
        })
    }

    pub async fn run_external_rfi_review(
        &self,
        command: RunExternalRfiReviewCommand,
    ) -> Result<ExternalRfiReviewResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.rfi_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(&tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_external_rfi_review_run(&tender_id, &command.rfi_id, command.version)?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        store.complete_agent_run(&tender_id, &prepared, execution)?;
        Ok(ExternalRfiReviewResult {
            run: store.inspect_agent_run(&prepared.run_id)?,
            rfi: store.load_external_rfi(
                &command.rfi_id,
                command.version,
                BidPackageOperationBudget::for_tender(&tender_id),
            )?,
        })
    }

    pub async fn run_calculation_rule_review(
        &self,
        command: RunCalculationRuleReviewCommand,
    ) -> Result<CalculationRuleReviewResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let prepare_host = self.clone();
        let prepare_tender_id = tender_id.clone();
        let prepare_command = command.clone();
        let (store, prepared) = tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&prepare_tender_id);
            let store =
                prepare_host.tender_store_with_check(&prepare_tender_id, &mut || budget.check())?;
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            if prepare_command.validate().is_err() {
                locked.record_calculation_denial(
                    &prepare_tender_id,
                    "run_calculation_rule_review",
                    Some(&prepare_command.rule_id),
                    "command_shape",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            match locked.prepare_calculation_rule_review_run(
                &prepare_tender_id,
                &prepare_command.rule_id,
                prepare_command.version,
            ) {
                Ok(prepared) => Ok((store.clone(), prepared)),
                Err(error) => {
                    if error.code == TenderErrorCode::InvalidCommand {
                        locked.record_calculation_denial(
                            &prepare_tender_id,
                            "run_calculation_rule_review",
                            Some(&prepare_command.rule_id),
                            "guard_denied",
                        )?;
                    }
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let complete_tender_id = tender_id.clone();
        let complete_prepared = prepared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&complete_tender_id);
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            locked.complete_agent_run(&complete_tender_id, &complete_prepared, execution)?;
            budget.check()?;
            Ok(CalculationRuleReviewResult {
                run: locked.inspect_agent_run(&complete_prepared.run_id)?,
                rule: locked.load_calculation_rule(&command.rule_id, command.version)?,
            })
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    pub async fn run_cost_estimator_calculation(
        &self,
        command: RunCostEstimatorCalculationCommand,
    ) -> Result<CostEstimatorCalculationResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let prepare_host = self.clone();
        let prepare_tender_id = tender_id.clone();
        let prepare_command = command.clone();
        let (store, prepared) = tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&prepare_tender_id);
            let store =
                prepare_host.tender_store_with_check(&prepare_tender_id, &mut || budget.check())?;
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            if prepare_command.validate().is_err() {
                locked.record_calculation_denial(
                    &prepare_tender_id,
                    "run_cost_estimator_calculation",
                    Some(&prepare_command.scenario_id),
                    "command_shape",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            match locked.prepare_cost_estimator_calculation_run(
                &prepare_tender_id,
                &prepare_command,
                budget,
            ) {
                Ok(prepared) => Ok((store.clone(), prepared)),
                Err(error) => {
                    if error.code == TenderErrorCode::InvalidCommand {
                        locked.record_calculation_denial(
                            &prepare_tender_id,
                            "run_cost_estimator_calculation",
                            Some(&prepare_command.scenario_id),
                            "guard_denied",
                        )?;
                    }
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let complete_tender_id = tender_id.clone();
        let complete_prepared = prepared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&complete_tender_id);
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            locked.complete_agent_run(&complete_tender_id, &complete_prepared, execution)?;
            budget.check()?;
            let calculation =
                match locked.load_calculation_for_cost_estimator_run(&complete_prepared.run_id) {
                    Ok(calculation) => Some(calculation),
                    Err(error) if error.code == TenderErrorCode::NotFound => None,
                    Err(error) => return Err(error),
                };
            Ok(CostEstimatorCalculationResult {
                run: locked.inspect_agent_run(&complete_prepared.run_id)?,
                calculation,
            })
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    pub async fn run_cost_estimator_basis(
        &self,
        command: RunCostEstimatorBasisCommand,
    ) -> Result<CostEstimatorBasisResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let prepare_host = self.clone();
        let prepare_tender_id = tender_id.clone();
        let prepare_command = command.clone();
        let (store, prepared) = tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&prepare_tender_id);
            let store =
                prepare_host.tender_store_with_check(&prepare_tender_id, &mut || budget.check())?;
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            if prepare_command.validate().is_err() {
                locked.record_estimate_denial(
                    &prepare_tender_id,
                    "run_cost_estimator_basis",
                    None,
                    "command_shape",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            match locked.prepare_cost_estimator_basis_run(
                &prepare_tender_id,
                &prepare_command,
                budget,
            ) {
                Ok(prepared) => Ok((store.clone(), prepared)),
                Err(error) => {
                    if error.code == TenderErrorCode::InvalidCommand {
                        locked.record_estimate_denial(
                            &prepare_tender_id,
                            "run_cost_estimator_basis",
                            None,
                            "guard_denied",
                        )?;
                    }
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let complete_tender_id = tender_id.clone();
        let complete_prepared = prepared.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&complete_tender_id);
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            locked.complete_agent_run(&complete_tender_id, &complete_prepared, execution)?;
            budget.check()?;
            let basis = match locked.load_basis_for_author_run(&complete_prepared.run_id) {
                Ok(basis) => Some(basis),
                Err(error) if error.code == TenderErrorCode::NotFound => None,
                Err(error) => return Err(error),
            };
            Ok(CostEstimatorBasisResult {
                run: locked.inspect_agent_run(&complete_prepared.run_id)?,
                basis,
            })
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    pub async fn run_basis_of_estimate_review(
        &self,
        command: RunBasisOfEstimateReviewCommand,
    ) -> Result<BasisOfEstimateReviewResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let prepare_host = self.clone();
        let prepare_tender_id = tender_id.clone();
        let prepare_command = command.clone();
        let (store, prepared) = tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&prepare_tender_id);
            let store =
                prepare_host.tender_store_with_check(&prepare_tender_id, &mut || budget.check())?;
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            if prepare_command.validate().is_err() {
                locked.record_estimate_denial(
                    &prepare_tender_id,
                    "run_basis_of_estimate_review",
                    Some(&prepare_command.basis_id),
                    "command_shape",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            match locked.prepare_basis_review_run(
                &prepare_tender_id,
                &prepare_command.basis_id,
                prepare_command.version,
                budget,
            ) {
                Ok(prepared) => Ok((store.clone(), prepared)),
                Err(error) => {
                    if error.code == TenderErrorCode::InvalidCommand {
                        locked.record_estimate_denial(
                            &prepare_tender_id,
                            "run_basis_of_estimate_review",
                            Some(&prepare_command.basis_id),
                            "guard_denied",
                        )?;
                    }
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let complete_tender_id = tender_id.clone();
        let complete_prepared = prepared.clone();
        let result_basis_id = command.basis_id.clone();
        let result_version = command.version;
        tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&complete_tender_id);
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            locked.complete_agent_run(&complete_tender_id, &complete_prepared, execution)?;
            budget.check()?;
            Ok(BasisOfEstimateReviewResult {
                run: locked.inspect_agent_run(&complete_prepared.run_id)?,
                basis: locked.load_basis_of_estimate(&result_basis_id, result_version)?,
            })
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    pub async fn run_priced_cost_baseline_review(
        &self,
        command: RunPricedCostBaselineReviewCommand,
    ) -> Result<PricedCostBaselineReviewResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let prepare_host = self.clone();
        let prepare_tender_id = tender_id.clone();
        let prepare_command = command.clone();
        let (store, prepared) = tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&prepare_tender_id);
            let store =
                prepare_host.tender_store_with_check(&prepare_tender_id, &mut || budget.check())?;
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            if prepare_command.validate().is_err() {
                locked.record_pricing_denial(
                    &prepare_tender_id,
                    "run_priced_cost_baseline_review",
                    Some(&prepare_command.baseline_id),
                    "command_shape",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            match locked.prepare_priced_cost_baseline_review_run(
                &prepare_tender_id,
                &prepare_command.baseline_id,
                prepare_command.version,
                budget,
            ) {
                Ok(prepared) => Ok((store.clone(), prepared)),
                Err(error) => {
                    if error.code == TenderErrorCode::InvalidCommand {
                        locked.record_pricing_denial(
                            &prepare_tender_id,
                            "run_priced_cost_baseline_review",
                            Some(&prepare_command.baseline_id),
                            "guard_denied",
                        )?;
                    }
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let complete_tender_id = tender_id.clone();
        let complete_prepared = prepared.clone();
        let result_baseline_id = command.baseline_id.clone();
        let result_version = command.version;
        tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&complete_tender_id);
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            locked.complete_agent_run(&complete_tender_id, &complete_prepared, execution)?;
            budget.check()?;
            Ok(PricedCostBaselineReviewResult {
                run: locked.inspect_agent_run(&complete_prepared.run_id)?,
                baseline: locked.load_priced_cost_baseline(&result_baseline_id, result_version)?,
            })
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    pub async fn run_pricing_adjustment_review(
        &self,
        command: RunPricingAdjustmentReviewCommand,
    ) -> Result<PricingAdjustmentReviewResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let prepare_host = self.clone();
        let prepare_tender_id = tender_id.clone();
        let prepare_command = command.clone();
        let (store, prepared) = tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&prepare_tender_id);
            let store =
                prepare_host.tender_store_with_check(&prepare_tender_id, &mut || budget.check())?;
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            if prepare_command.validate().is_err() {
                locked.record_pricing_denial(
                    &prepare_tender_id,
                    "run_pricing_adjustment_review",
                    Some(&prepare_command.adjustment_id),
                    "command_shape",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            match locked.prepare_pricing_adjustment_review_run(
                &prepare_tender_id,
                &prepare_command.adjustment_id,
                prepare_command.version,
                budget,
            ) {
                Ok(prepared) => Ok((store.clone(), prepared)),
                Err(error) => {
                    if error.code == TenderErrorCode::InvalidCommand {
                        locked.record_pricing_denial(
                            &prepare_tender_id,
                            "run_pricing_adjustment_review",
                            Some(&prepare_command.adjustment_id),
                            "guard_denied",
                        )?;
                    }
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))??;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let complete_tender_id = tender_id.clone();
        let complete_prepared = prepared.clone();
        let result_adjustment_id = command.adjustment_id.clone();
        let result_version = command.version;
        tauri::async_runtime::spawn_blocking(move || {
            let budget = BidPackageOperationBudget::for_tender(&complete_tender_id);
            let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
            locked.complete_agent_run(&complete_tender_id, &complete_prepared, execution)?;
            Ok(PricingAdjustmentReviewResult {
                run: locked.inspect_agent_run(&complete_prepared.run_id)?,
                adjustment: locked
                    .load_pricing_adjustment(&result_adjustment_id, result_version)?,
            })
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    pub fn inspect_tender_record_page(
        &self,
        tender_id: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<TenderRecordPage, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let page = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_tender_record_page(cursor, limit)?;
        Ok(page)
    }

    pub fn create_tender_engineer_entry(
        &self,
        command: CreateTenderEngineerEntryCommand,
    ) -> Result<TenderRecordAuthority, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let authority = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .create_tender_engineer_entry(&tender_id, &command)?;
        Ok(authority)
    }

    pub fn inspect_tender_record_authorities(
        &self,
        tender_id: &str,
    ) -> Result<Vec<TenderRecordAuthority>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let authorities = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_tender_record_authorities()?;
        Ok(authorities)
    }

    pub fn decide_tender_record(
        &self,
        command: DecideTenderRecordCommand,
    ) -> Result<TenderRecordDecisionResult, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.record_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let store = self.tender_store(&tender_id)?;
        let result = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .decide_tender_record(&tender_id, &command)?;
        Ok(result)
    }

    pub fn create_bid_decision_package(
        &self,
        command: CreateBidDecisionPackageCommand,
    ) -> Result<BidDecisionPackageInspection, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let package = lock_mutex_with_check(&store, &mut || budget.check())?
            .create_bid_decision_package(&tender_id, &command, budget)?;
        Ok(package)
    }

    pub fn inspect_current_bid_decision_package(
        &self,
        tender_id: &str,
    ) -> Result<Option<BidDecisionPackageInspection>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let package = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_current_bid_decision_package()?;
        Ok(package)
    }

    pub fn decide_bid_decision_package(
        &self,
        command: DecideBidDecisionPackageCommand,
    ) -> Result<BidDecisionApprovalResult, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .decide_bid_decision_package(&tender_id, &command, budget)?;
        Ok(result)
    }

    pub fn inspect_bid_decision_approval_history(
        &self,
        command: InspectBidDecisionApprovalHistoryCommand,
    ) -> Result<BidDecisionApprovalHistoryPage, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let page = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_bid_decision_approval_history(
                command.before_sequence,
                command.limit,
                budget,
            )?;
        Ok(page)
    }

    pub fn resolve_bid_decision_return_rework(
        &self,
        command: ResolveBidDecisionReturnReworkCommand,
    ) -> Result<BidDecisionReturnReworkResult, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .resolve_bid_decision_return_rework(&tender_id, &command, budget)?;
        Ok(result)
    }

    pub fn invalidate_bid_decision_approval(
        &self,
        command: InvalidateBidDecisionApprovalCommand,
    ) -> Result<BidDecisionApprovalInvalidationResult, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .invalidate_bid_decision_approval(&tender_id, &command, budget)?;
        Ok(result)
    }

    pub fn inspect_compliance_matrix_page(
        &self,
        tender_id: &str,
        package_id: &str,
        version: u32,
        after_ordinal: Option<u32>,
        limit: u32,
    ) -> Result<ComplianceMatrixPage, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let page = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_compliance_matrix_page(package_id, version, after_ordinal, limit)?;
        Ok(page)
    }

    pub fn inspect_bid_decision_package_record_page(
        &self,
        tender_id: &str,
        package_id: &str,
        version: u32,
        category: BidDecisionPackageRecordCategory,
        after_ordinal: Option<u32>,
        limit: u32,
    ) -> Result<BidDecisionPackageRecordPage, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let page = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_bid_decision_package_record_page(
                package_id,
                version,
                category,
                after_ordinal,
                limit,
            )?;
        Ok(page)
    }

    pub async fn run_bid_decision_package_review(
        &self,
        command: RunBidDecisionPackageReviewCommand,
    ) -> Result<BidDecisionPackageReviewResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.package_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(&tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_bid_decision_package_review_run(
                &tender_id,
                &command.package_id,
                command.version,
            )?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        store.complete_agent_run(&tender_id, &prepared, execution)?;
        Ok(BidDecisionPackageReviewResult {
            run: store.inspect_agent_run(&prepared.run_id)?,
            package: store.inspect_bid_decision_package(&command.package_id, command.version)?,
        })
    }

    pub async fn run_submission_section_review(
        &self,
        command: RunSubmissionSectionReviewCommand,
    ) -> Result<SubmissionSectionReviewRunResult, TenderCommandError> {
        self.require_runtime_verified()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str(), false)?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(&tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_submission_section_review_run(&tender_id, &command)?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution = execute_provider_turn(self, &store, &prepared, cancellation).await;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        store.complete_agent_run(&tender_id, &prepared, execution)?;
        Ok(SubmissionSectionReviewRunResult {
            run: store.inspect_agent_run(&prepared.run_id)?,
            final_review: store
                .inspect_final_review(BidPackageOperationBudget::for_tender(&tender_id))?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        })
    }

    pub fn inspect_agent_runs(
        &self,
        tender_id: &str,
    ) -> Result<Vec<AgentRunInspection>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let runs = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_agent_runs()?;
        Ok(runs)
    }

    pub fn inspect_agent_run_history(
        &self,
        command: InspectAgentRunHistoryCommand,
    ) -> Result<AgentRunHistoryPage, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let page = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_agent_run_history(command.before_sequence, command.limit)?;
        Ok(page)
    }

    pub fn inspect_agent_run(
        &self,
        command: InspectAgentRunCommand,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let run = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_agent_run(&command.run_id)?;
        Ok(run)
    }

    pub fn inspect_agent_run_activity(
        &self,
        tender_id: &str,
    ) -> Result<AgentRunActivity, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let activity = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspect_agent_run_activity()?;
        Ok(activity)
    }

    pub fn resolve_indeterminate_agent_run(
        &self,
        command: ResolveIndeterminateAgentRunCommand,
    ) -> Result<AgentRunRecoveryDecision, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if !valid_identifier(&command.run_id) || command.rationale.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let store = self.tender_store(&tender_id)?;
        let decision = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .resolve_indeterminate_agent_run(&tender_id, command)?;
        Ok(decision)
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

    #[doc(hidden)]
    pub async fn inspect_codex_subscription(
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

    pub(crate) async fn quiesce_agent_provider_for_update(&self) -> bool {
        let provider = self.agent_provider().lock().await.take();
        match provider {
            Some(provider) => provider.shutdown().await.is_ok(),
            None => true,
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
                if let Ok(provider) = provider {
                    let _ = provider.shutdown().await;
                }
                return Err(readiness_interruption_failure());
            }
            *provider_slot = Some(provider?);
            return Ok(());
        }
        let provider = provider_slot
            .as_ref()
            .expect("provider remains available")
            .clone();
        drop(provider_slot);
        let readiness = tokio::select! {
            biased;
            _ = cancellation.cancelled() => Err(readiness_interruption_failure()),
            readiness = provider.refresh_readiness() => readiness,
        };
        if readiness.is_err() {
            let mut provider_slot = self.agent_provider().lock().await;
            *provider_slot = None;
            drop(provider_slot);
            let _ = provider.shutdown().await;
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
    let provider = {
        let mut provider_slot = host.agent_provider().lock().await;
        if provider_slot.is_none() {
            let provider = match CodexProvider::readiness(
                host.process_supervisor(),
                host.runtime_layout().codex_executable(),
                host.application_home(),
                cancellation.clone(),
            )
            .await
            {
                Ok(provider) => provider,
                Err(failure) => return failed_execution(failure, started),
            };
            *provider_slot = Some(provider);
        }
        provider_slot
            .as_ref()
            .expect("provider initialized above")
            .clone()
    };
    if let Some(execution) = cancellation_checkpoint(store, prepared, &cancellation, started) {
        return execution;
    }
    let operation_limit = match permission_duration(&prepared.permission_grant, Timestamp::now()) {
        Ok(duration) if !duration.is_zero() => duration,
        _ => return failed_execution(permission_failure(), started),
    };
    let archive_store = Arc::clone(store);
    let archive_prepared = prepared.clone();
    let thread_store = Arc::clone(store);
    let thread_prepared = prepared.clone();
    let requested_store = Arc::clone(store);
    let checkpoint_store = Arc::clone(store);
    let event_store = Arc::clone(store);
    let event_host = host.clone();
    let denial_store = Arc::clone(store);
    let tool_store = Arc::clone(store);
    let requested_run_id = prepared.run_id.clone();
    let run_id = prepared.run_id.clone();
    let event_run_id = prepared.run_id.clone();
    let denial_run_id = prepared.run_id.clone();
    let tool_run_id = prepared.run_id.clone();
    let tool_prepared = prepared.clone();
    let mut execution = provider
        .run_turn(
            prepared.clone(),
            operation_limit,
            cancellation,
            RunCallbacks {
                on_thread_archived: Box::new(move |thread_ref| {
                    archive_store
                        .lock()
                        .map_err(|_| process_failure(false))?
                        .checkpoint_provider_thread_archived(&archive_prepared, thread_ref)
                        .map_err(|_| process_failure(false))
                }),
                on_thread_established: Box::new(move |thread_ref, resumed| {
                    thread_store
                        .lock()
                        .map_err(|_| process_failure(false))?
                        .checkpoint_agent_thread(&thread_prepared, thread_ref, resumed)
                        .map_err(|_| process_failure(false))
                }),
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
                    event_host.observe_provider_usage(usage);
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
    host.observe_provider_usage(&execution.usage);
    execution.usage.elapsed_milliseconds =
        Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
    if provider.is_closed() {
        let mut provider_slot = host.agent_provider().lock().await;
        if provider_slot.as_ref().is_some_and(CodexProvider::is_closed) {
            *provider_slot = None;
        }
    }
    execution
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
#[doc(hidden)]
pub enum CodexReadiness {
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

pub(super) struct CodexProviderProcess {
    conversation: Option<SupervisedConversation>,
}

impl CodexProviderProcess {
    pub(super) async fn readiness(
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

    pub(super) async fn refresh_readiness(&mut self) -> Result<(), ProviderFailure> {
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

    pub(super) async fn shutdown(&mut self) -> Result<(), ProcessError> {
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

    pub(super) fn conversation_mut(
        &mut self,
    ) -> Result<&mut SupervisedConversation, ProviderFailure> {
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
        let provider = CodexProvider::readiness(
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
