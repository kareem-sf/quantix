use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[cfg(feature = "runtime-fixture")]
use std::{ffi::OsString, fs, io, path::Path};

use garde::Validate;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::{
    agent_backend::{
        execute_provider_turn as execute_chatgpt_backend_turn, BackendRequest, ReasoningEffort,
        ReqwestBackend, StreamEvent, ToolRejection, TurnContext, UsageSnapshot, BACKEND_URL,
    },
    application_settings::{
        project_approved_chatgpt_connection, refresh_approved_chatgpt_connection,
        AiExecutionSelection, AiProviderKind, ProviderReasoningSelection,
    },
    chatgpt_login::PRODUCTION_ISSUER,
    chatgpt_oauth::TokenClient,
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

#[cfg(not(feature = "runtime-fixture"))]
use crate::application_settings::project_chatgpt_connection_readiness;

#[cfg(feature = "runtime-fixture")]
use crate::{
    application_settings::{
        codex_connection_version, codex_failure_connection_status, save_codex_connection_status,
        save_live_connection, ProviderConnectionStatus, ProviderConnectionView,
        ProviderModelOption, ProviderReasoningOption, CODEX_CONNECTION_ID,
    },
    process_supervisor::{ProcessError, ProcessSpec, ProcessTermination, SupervisedConversation},
};

mod bootstrap_profile;
#[cfg(feature = "runtime-fixture")]
mod codex_actor;
mod codex_protocol;
pub(crate) mod permissions;
pub(crate) use bootstrap_profile::{bootstrap_profile, bootstrap_task};
#[cfg(feature = "runtime-fixture")]
pub(crate) use codex_actor::CodexProvider;
use codex_protocol::{
    direct_tool_specs, execute_typed_tool, outcome_unknown, process_failure, protocol_failure,
    provider_instruction_bundle, typed_tool_arguments_are_valid, typed_tool_is_known,
    validate_candidate,
};
#[cfg(feature = "runtime-fixture")]
use codex_protocol::{read_expected_response, write_rpc};
use permissions::permission_duration;

#[derive(Clone)]
#[cfg(feature = "runtime-fixture")]
pub(crate) enum AgentProvider {
    Codex(CodexProvider),
}

#[cfg(feature = "runtime-fixture")]
impl AgentProvider {
    async fn codex_readiness(
        supervisor: &crate::process_supervisor::ProcessSupervisor,
        executable: PathBuf,
        process_directory: &Path,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderFailure> {
        CodexProvider::readiness(supervisor, executable, process_directory, cancellation)
            .await
            .map(Self::Codex)
    }

    pub(crate) async fn refresh_readiness(&self) -> Result<bool, ProviderFailure> {
        match self {
            Self::Codex(provider) => provider.refresh_readiness().await,
        }
    }

    pub(crate) fn connection_snapshot(&self) -> ProviderConnectionView {
        match self {
            Self::Codex(provider) => provider.connection_snapshot(),
        }
    }

    #[cfg(feature = "runtime-fixture")]
    pub(crate) async fn delete_thread(&self, thread_ref: String) -> Result<(), ProviderFailure> {
        match self {
            Self::Codex(provider) => provider.delete_thread(thread_ref).await,
        }
    }

    #[cfg(feature = "runtime-fixture")]
    async fn run_turn(
        &self,
        prepared: PreparedAgentRun,
        operation_limit: Duration,
        cancellation: CancellationToken,
        callbacks: RunCallbacks,
    ) -> ProviderExecution {
        match self {
            Self::Codex(provider) => {
                provider
                    .run_turn(prepared, operation_limit, cancellation, callbacks)
                    .await
            }
        }
    }

    pub(crate) async fn shutdown(&self) -> Result<(), ProcessError> {
        match self {
            Self::Codex(provider) => provider.shutdown().await,
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        match self {
            Self::Codex(provider) => provider.is_closed(),
        }
    }

    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Codex(left), Self::Codex(right)) => left.same_instance(right),
        }
    }
}
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
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(5);
const PROVIDER_OUTPUT_LIMIT: usize = 1024 * 1024;
#[cfg(feature = "runtime-fixture")]
const PROVIDER_READINESS_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(any(test, feature = "runtime-fixture"))]
pub(crate) const CODEX_VERSION: &str = "0.147.0";
#[cfg(feature = "runtime-fixture")]
pub(crate) const CODEX_PROTOCOL_SCHEMA: &str =
    include_str!("../tests/fixtures/codex_app_server_protocol.schemas.json");
#[cfg(feature = "runtime-fixture")]
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
    TenderingManager,
    DocumentController,
    TenderAnalyst,
    IndependentReviewer,
}

impl BootstrapRole {
    pub(crate) const ALL: [Self; 4] = [
        Self::TenderingManager,
        Self::DocumentController,
        Self::TenderAnalyst,
        Self::IndependentReviewer,
    ];
    pub(crate) const SPECIALISTS: [Self; 3] = [
        Self::DocumentController,
        Self::TenderAnalyst,
        Self::IndependentReviewer,
    ];

    pub(crate) const fn stable_identity(self) -> &'static str {
        match self {
            Self::TenderingManager => "quantix.agent.tendering-manager",
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
    pub repair_feedback: Option<AgentRepairFeedback>,
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
pub struct OutputValidationIssue {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRepairFeedback {
    pub rejected_run_id: String,
    pub rejected_payload_sha256: String,
    pub validation_issues: Vec<OutputValidationIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedAgentOutput {
    pub run_id: String,
    pub payload_json: String,
    pub payload_sha256: String,
    pub validation_issues: Vec<OutputValidationIssue>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderFailure {
    pub category: ProviderFailureCategory,
    pub retry_safe: bool,
    pub required_user_action: String,
    pub redacted_detail: Option<String>,
    pub validation_issues: Vec<OutputValidationIssue>,
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
            validation_issues: Vec::new(),
        }
    }

    pub(crate) fn with_validation_issues(
        mut self,
        validation_issues: Vec<OutputValidationIssue>,
    ) -> Self {
        self.validation_issues = validation_issues;
        self
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
    pub provider_selection: AiExecutionSelection,
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
    pub provider_selection: AiExecutionSelection,
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
    pub provider_selection: AiExecutionSelection,
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

#[derive(Debug, Clone, Copy)]
pub(crate) enum DeterministicProviderOutcome {
    Completed,
    Failed,
    Interrupted,
}

struct ActiveAgentRunGuard {
    host: QuantixHost,
    lease_id: String,
}

struct ActiveManagerIntakeGuard {
    host: QuantixHost,
    tender_id: String,
}

type TurnAcceptedCallback = dyn FnOnce(&str) -> Result<(), ProviderFailure> + Send;
type TurnRequestedCallback = dyn FnOnce() -> Result<(), ProviderFailure> + Send;
type TurnEventCallback =
    dyn FnMut(&PendingProviderEvent, &ProviderUsage) -> Result<(), ProviderFailure> + Send;
type TurnDeniedCallback = dyn FnMut(&PendingProviderEvent) -> Result<(), ProviderFailure> + Send;
#[cfg(feature = "runtime-fixture")]
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
    #[cfg(feature = "runtime-fixture")]
    on_tool_call: Box<TurnToolCallCallback>,
}

impl Drop for ActiveAgentRunGuard {
    fn drop(&mut self) {
        self.host.finish_active_agent_run(&self.lease_id);
    }
}

impl Drop for ActiveManagerIntakeGuard {
    fn drop(&mut self) {
        self.host.finish_manager_intake(&self.tender_id);
    }
}

fn agent_run_waits_for_provider(run: &AgentRunInspection) -> bool {
    run.state == AgentRunState::Failed
        && run.failure.as_ref().is_some_and(|failure| {
            matches!(
                failure.category,
                ProviderFailureCategory::AuthenticationRequired
                    | ProviderFailureCategory::SubscriptionRequired
                    | ProviderFailureCategory::ProtocolInvalid
                    | ProviderFailureCategory::ProcessFailed
                    | ProviderFailureCategory::RateLimited
            )
        })
}

impl QuantixHost {
    pub(crate) fn start_manager_intake_background(
        &self,
        tender_id: String,
    ) -> Result<(), TenderCommandError> {
        TenderId::parse(&tender_id)?;
        if !self.begin_manager_intake(&tender_id)? {
            return Ok(());
        }
        let operation_id = format!("manager-intake-{tender_id}");
        self.record_tender_diagnostic(
            &tender_id,
            crate::DiagnosticSeverity::Info,
            crate::DiagnosticComponent::Manager,
            "manager_intake_scheduled",
            "Manager intake work was scheduled",
            Some(operation_id.clone()),
            None,
            Some("started"),
            None,
        );
        let host = self.clone();
        tauri::async_runtime::spawn(async move {
            {
                let _active = ActiveManagerIntakeGuard {
                    host: host.clone(),
                    tender_id: tender_id.clone(),
                };
                let started = Instant::now();
                let result = host.run_manager_intake_pipeline(&tender_id).await;
                let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                match &result {
                    Ok(()) => host.record_tender_diagnostic(
                        &tender_id,
                        crate::DiagnosticSeverity::Info,
                        crate::DiagnosticComponent::Manager,
                        "manager_intake_cycle_completed",
                        "A Manager intake processing cycle completed",
                        Some(operation_id.clone()),
                        Some(elapsed_ms),
                        Some("completed"),
                        None,
                    ),
                    Err(error) => host.record_tender_diagnostic(
                        &tender_id,
                        crate::DiagnosticSeverity::Error,
                        crate::DiagnosticComponent::Manager,
                        "manager_intake_cycle_failed",
                        "A Manager intake processing cycle failed",
                        Some(operation_id.clone()),
                        Some(elapsed_ms),
                        Some("failed"),
                        Some(format!("{:?}", error.code)),
                    ),
                }
                if let Err(error) = result {
                    if let Ok(parsed) = TenderId::parse(&tender_id) {
                        if let Ok(store) = host.tender_store(&parsed) {
                            if let Ok(mut store) = store.lock() {
                                if error.code == TenderErrorCode::AiProviderRequired {
                                    let _ = store.wait_manager_intake_for_provider();
                                } else {
                                    let _ = store.fail_manager_intake(
                                        &parsed,
                                        "Quantix could not complete the Tender intake safely. Review the exact Agent Run and retry intake; the registered source package remains unchanged.",
                                    );
                                }
                            }
                        }
                    }
                }
            }
            if let Ok(parsed) = TenderId::parse(&tender_id) {
                if let Ok(store) = host.tender_store(&parsed) {
                    let pending = store
                        .lock()
                        .ok()
                        .and_then(|store| store.current_manager_intake_status().ok().flatten())
                        .is_some_and(|status| status.stage.is_active());
                    if pending {
                        let _ = host.start_manager_intake_background(tender_id);
                    }
                }
            }
        });
        Ok(())
    }

    pub(crate) fn resume_manager_intakes(&self) -> Result<(), TenderCommandError> {
        for entry in self.list_tenders()? {
            if entry.summary.is_none() {
                continue;
            }
            let tender_id = TenderId::parse(&entry.tender_id)?;
            let store = self.tender_store(&tender_id)?;
            let active = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .current_manager_intake_status()?
                .is_some_and(|status| status.stage.is_resumable());
            if active {
                self.start_manager_intake_background(entry.tender_id)?;
            }
        }
        Ok(())
    }

    pub(crate) fn retry_manager_intake(&self, tender_id: &str) -> Result<(), TenderCommandError> {
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let unresolved_run_ids = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .unresolved_manager_intake_run_ids()?;
        for run_id in unresolved_run_ids {
            self.resolve_indeterminate_agent_run(ResolveIndeterminateAgentRunCommand {
                tender_id: tender_id.as_str().into(),
                run_id,
                disposition: AgentRunRecoveryDisposition::CloseTask,
                rationale: "Tendering Engineer closed the uncertain intake turn by explicitly choosing to retry intake.".into(),
            })?;
        }
        store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .queue_manager_intake_retry()?;
        self.start_manager_intake_background(tender_id.as_str().into())
    }

    pub(crate) async fn rebind_manager_intake_provider(
        &self,
        tender_id: &str,
    ) -> Result<(), TenderCommandError> {
        let tender_id = TenderId::parse(tender_id)?;
        let selection = self
            .refresh_exact_ai_execution_selection(None)
            .await?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))?;
        let store = self.tender_store(&tender_id)?;
        store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .bind_manager_intake_provider_selection(&selection, true)?;
        self.start_manager_intake_background(tender_id.as_str().into())
    }

    async fn run_manager_intake_pipeline(&self, tender_id: &str) -> Result<(), TenderCommandError> {
        require_setup(self)?;
        let _execution = self.manager_intake_execution_guard(tender_id).await?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        if !self.document_tools_are_verified() {
            store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .wait_manager_intake_for_local_tools()?;
            return Ok(());
        }
        let stage = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .begin_manager_intake_processing()?;
        if matches!(
            stage,
            crate::tender_store::ManagerIntakeStage::WaitingForEngineer
                | crate::tender_store::ManagerIntakeStage::BidDecisionReady
        ) {
            return Ok(());
        }
        let intake_run_id = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .current_manager_intake_status()?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .intake_run_id;
        let targets = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .manager_intake_parse_targets(&tender_id)?;
        store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .refresh_manager_intake_parse_counts()?;
        for target in targets {
            let package_path = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .source_artifact_package_path(&target.artifact_id, target.version)?;
            let result = self.parse_source_artifact(target).await?;
            if result.state != crate::document_parsing::ParseState::Parsed {
                let reason = result
                    .exception
                    .map(|exception| exception.as_str())
                    .unwrap_or_else(|| result.state.as_str())
                    .replace('_', " ");
                let summary = format!(
                    "Quantix could not safely read \"{package_path}\" ({reason}). Open Files to review that document, then retry intake. The registered source package remains unchanged."
                );
                store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .fail_manager_intake(&tender_id, &summary)?;
                return Ok(());
            }
            store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .refresh_manager_intake_parse_counts()?;
        }
        let preferred = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .manager_intake_provider_selection()?;
        let Some(selection) = self
            .refresh_exact_ai_execution_selection(preferred.as_ref())
            .await?
        else {
            let mut store = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            if preferred.is_some() {
                store.wait_manager_intake_for_provider()?;
            } else {
                store.wait_manager_intake_for_provider_approval()?;
            }
            return Ok(());
        };
        store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .bind_manager_intake_provider_selection(&selection, false)?;
        let (batches, authorities) = {
            let mut store = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            store.refresh_manager_intake_parse_counts()?;
            let authorities = store.manager_intake_authority_references()?;
            let batches = store.manager_intake_evidence_batches(&authorities)?;
            (batches, authorities)
        };
        for evidence in batches {
            let result = self
                .run_tender_record_extraction_inner(
                    RunTenderRecordExtractionCommand {
                        tender_id: tender_id.as_str().into(),
                        evidence,
                        authorities: authorities.clone(),
                    },
                    Some(&intake_run_id),
                )
                .await?;
            if agent_run_waits_for_provider(&result.run) {
                store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .wait_manager_intake_for_provider()?;
                return Ok(());
            }
            if result.run.state != AgentRunState::Completed || result.published_record_count == 0 {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .record_manager_intake_extraction_count()?;
        }
        let review_targets = {
            let mut store = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            store.begin_manager_intake_reviewing()?;
            store.manager_intake_review_targets()?
        };
        for record in review_targets {
            let result = self
                .run_tender_record_review_inner(
                    RunTenderRecordReviewCommand {
                        tender_id: tender_id.as_str().into(),
                        record_id: record.record_id,
                        version: record.version,
                    },
                    Some(&intake_run_id),
                )
                .await?;
            if agent_run_waits_for_provider(&result.run) {
                store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .wait_manager_intake_for_provider()?;
                return Ok(());
            }
            if result.run.state != AgentRunState::Completed {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let run = self.run_manager_intake_outcome(&tender_id).await?;
        if agent_run_waits_for_provider(&run) {
            store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .wait_manager_intake_for_provider()?;
            return Ok(());
        }
        if run.state != AgentRunState::Completed {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(())
    }

    #[cfg(any(test, feature = "runtime-fixture"))]
    pub async fn run_manager_intake_for_verification(
        &self,
        tender_id: &str,
    ) -> Result<(), TenderCommandError> {
        self.run_manager_intake_pipeline(tender_id).await
    }

    async fn run_manager_intake_outcome(
        &self,
        tender_id: &TenderId,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_manager_intake_run(tender_id)?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        store.complete_agent_run(tender_id, &prepared, execution)?;
        store.inspect_agent_run(&prepared.run_id)
    }

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
        self.run_bootstrap_agent_inner(command, None).await
    }

    pub(crate) async fn run_bootstrap_agent_with_deterministic_provider(
        &self,
        command: RunBootstrapAgentCommand,
        outcome: DeterministicProviderOutcome,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        self.run_bootstrap_agent_inner(command, Some(outcome)).await
    }

    async fn run_bootstrap_agent_inner(
        &self,
        command: RunBootstrapAgentCommand,
        deterministic_outcome: Option<DeterministicProviderOutcome>,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let store = self.tender_store(&tender_id)?;
        if store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .has_unresolved_indeterminate_agent_run()?
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let run_started = Instant::now();
        let provider_selection = if deterministic_outcome.is_some() {
            deterministic_provider_selection()
        } else {
            self.require_current_tender_ai_selection(&tender_id).await?
        };
        if command
            .retry_of_run_id
            .as_deref()
            .is_some_and(|value| !valid_identifier(value))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_bootstrap_agent_run(
                &tender_id,
                &provider_selection,
                command.retry_of_run_id.as_deref(),
                self.provider_subscription_capacity_is_exhausted(),
            )?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;

        record_provider_turn_started(self, tender_id.as_str(), &prepared);
        let deterministic = deterministic_outcome.is_some();
        let execution = match deterministic_outcome {
            Some(outcome) => deterministic_provider_execution(&prepared, outcome),
            None => {
                execute_provider_turn_from(self, &store, &prepared, cancellation, run_started).await
            }
        };
        record_provider_turn_diagnostic(
            self,
            tender_id.as_str(),
            &prepared,
            &execution,
            run_started,
        );
        if deterministic {
            if let Some(thread_ref) = execution.provider_thread_ref.as_deref() {
                store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .checkpoint_agent_thread(&prepared, thread_ref, false)?;
            }
        }
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
        let provider_selection = self.require_current_tender_ai_selection(tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
                &provider_selection,
                production_task_id,
                None,
                self.provider_subscription_capacity_is_exhausted(),
            )?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        if !self.claim_production_scheduler(tender_id.as_str()) {
            return Ok(());
        }
        let result = self
            .schedule_ready_production_tasks(tender_id.as_str())
            .await;
        self.release_production_scheduler(tender_id.as_str());
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
        self.run_tender_record_extraction_inner(command, None).await
    }

    async fn run_tender_record_extraction_inner(
        &self,
        command: RunTenderRecordExtractionCommand,
        manager_intake_run_id: Option<&str>,
    ) -> Result<TenderRecordExtractionResult, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
                manager_intake_run_id,
            )?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;

        let execution = execute_tender_provider_turn(
            self,
            tender_id.as_str(),
            &store,
            &prepared,
            cancellation.clone(),
        )
        .await;
        let repair = {
            let mut tender_store = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            tender_store.complete_agent_run(&tender_id, &prepared, execution)?;
            let initial_run = tender_store.inspect_agent_run(&prepared.run_id)?;
            if initial_run.retry_of_run_id.is_none()
                && initial_run.state == AgentRunState::Failed
                && initial_run.failure.as_ref().is_some_and(|failure| {
                    failure.category == ProviderFailureCategory::OutputInvalid
                })
            {
                tender_store.prepare_tender_record_repair_run(&tender_id, &prepared.run_id)?
            } else {
                None
            }
        };
        if let Some(repair) = repair {
            self.identify_active_agent_run(&lease_id, &repair.run_id)?;
            let repair_execution = execute_tender_provider_turn(
                self,
                tender_id.as_str(),
                &store,
                &repair,
                cancellation,
            )
            .await;
            let mut tender_store = store
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            tender_store.complete_agent_run(&tender_id, &repair, repair_execution)?;
            return Ok(TenderRecordExtractionResult {
                run: tender_store.inspect_agent_run(&repair.run_id)?,
                published_record_count: tender_store.count_tender_records_by_run(&repair.run_id)?,
            });
        }
        let tender_store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        Ok(TenderRecordExtractionResult {
            run: tender_store.inspect_agent_run(&prepared.run_id)?,
            published_record_count: tender_store.count_tender_records_by_run(&prepared.run_id)?,
        })
    }

    pub async fn run_tender_record_review(
        &self,
        command: RunTenderRecordReviewCommand,
    ) -> Result<TenderRecordReviewResult, TenderCommandError> {
        self.run_tender_record_review_inner(command, None).await
    }

    async fn run_tender_record_review_inner(
        &self,
        command: RunTenderRecordReviewCommand,
        manager_intake_run_id: Option<&str>,
    ) -> Result<TenderRecordReviewResult, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        if !valid_identifier(&command.record_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
        let _active = ActiveAgentRunGuard {
            host: self.clone(),
            lease_id: lease_id.clone(),
        };
        let store = self.tender_store(&tender_id)?;
        let prepared = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .prepare_tender_record_review_run(
                &tender_id,
                &command.record_id,
                command.version,
                manager_intake_run_id,
            )?;
        self.identify_active_agent_run(&lease_id, &prepared.run_id)?;
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        if !valid_identifier(&command.rfi_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        if !valid_identifier(&command.package_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        self.require_current_tender_ai_selection(&tender_id).await?;
        let (lease_id, cancellation) = self.begin_active_agent_run(tender_id.as_str()).await?;
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
        let execution =
            execute_tender_provider_turn(self, tender_id.as_str(), &store, &prepared, cancellation)
                .await;
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

    pub fn rejected_agent_output(
        &self,
        tender_id: &str,
        run_id: &str,
    ) -> Result<RejectedAgentOutput, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let rejected_output = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .rejected_agent_output(run_id);
        rejected_output
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
        self.require_document_tools()?;
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
        self.require_document_tools()?;
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
        self.require_document_tools()?;
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
        #[cfg(feature = "runtime-fixture")]
        match self.try_inspect_codex_subscription(cancellation).await {
            Ok(()) => CodexReadiness::Ready,
            Err(failure) => {
                let (status, summary) = codex_failure_connection_status(failure.category);
                let readiness = match status {
                    ProviderConnectionStatus::AuthenticationRequired => {
                        CodexReadiness::AuthenticationRequired
                    }
                    ProviderConnectionStatus::SubscriptionRequired => {
                        CodexReadiness::SubscriptionRequired
                    }
                    ProviderConnectionStatus::Ready
                    | ProviderConnectionStatus::TemporarilyUnavailable
                    | ProviderConnectionStatus::Incompatible => CodexReadiness::Unavailable,
                };
                if save_codex_connection_status(self.application_home(), status, summary).is_err() {
                    CodexReadiness::Unavailable
                } else {
                    readiness
                }
            }
        }
        #[cfg(not(feature = "runtime-fixture"))]
        match self.try_inspect_chatgpt_subscription(cancellation).await {
            Ok(()) => CodexReadiness::Ready,
            Err(failure) => match failure.category {
                ProviderFailureCategory::AuthenticationRequired => {
                    CodexReadiness::AuthenticationRequired
                }
                ProviderFailureCategory::SubscriptionRequired => {
                    CodexReadiness::SubscriptionRequired
                }
                _ => CodexReadiness::Unavailable,
            },
        }
    }

    #[cfg(feature = "runtime-fixture")]
    pub(crate) async fn quiesce_agent_provider_for_update(&self) -> bool {
        let provider = self.agent_provider().lock().await.take();
        match provider {
            Some(provider) => provider.shutdown().await.is_ok(),
            None => true,
        }
    }

    #[cfg(feature = "runtime-fixture")]
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
            let provider = AgentProvider::codex_readiness(
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
            let provider = provider?;
            let connection = provider.connection_snapshot();
            save_live_connection(self.application_home(), &connection)
                .map_err(|_| process_failure(false))?;
            *provider_slot = Some(provider);
            return provider_connection_readiness(&connection);
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
        if let Err(failure) = readiness {
            let mut provider_slot = self.agent_provider().lock().await;
            *provider_slot = None;
            drop(provider_slot);
            let _ = provider.shutdown().await;
            return Err(failure);
        }
        let connection = provider.connection_snapshot();
        save_live_connection(self.application_home(), &connection)
            .map_err(|_| process_failure(false))?;
        provider_connection_readiness(&connection)
    }

    #[cfg(not(feature = "runtime-fixture"))]
    async fn try_inspect_chatgpt_subscription(
        &self,
        cancellation: CancellationToken,
    ) -> Result<(), ProviderFailure> {
        let home = self.application_home().to_path_buf();
        let readiness = tokio::task::spawn_blocking(move || {
            project_chatgpt_connection_readiness(&home, PRODUCTION_ISSUER, epoch_milliseconds())
        });
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(readiness_interruption_failure()),
            readiness = readiness => readiness.map_err(|_| process_failure(false))??,
        };
        Ok(())
    }
}

#[cfg(feature = "runtime-fixture")]
fn provider_connection_readiness(
    connection: &ProviderConnectionView,
) -> Result<(), ProviderFailure> {
    match connection.status {
        ProviderConnectionStatus::Ready => Ok(()),
        ProviderConnectionStatus::AuthenticationRequired => Err(authentication_failure()),
        ProviderConnectionStatus::SubscriptionRequired => Err(subscription_failure()),
        ProviderConnectionStatus::Incompatible => Err(protocol_failure(false)),
        ProviderConnectionStatus::TemporarilyUnavailable => Err(process_failure(false)),
    }
}

async fn execute_tender_provider_turn(
    host: &QuantixHost,
    tender_id: &str,
    store: &Arc<Mutex<TenderStore>>,
    prepared: &PreparedAgentRun,
    cancellation: CancellationToken,
) -> ProviderExecution {
    let started = Instant::now();
    record_provider_turn_started(host, tender_id, prepared);
    let execution = execute_provider_turn(host, store, prepared, cancellation).await;
    record_provider_turn_diagnostic(host, tender_id, prepared, &execution, started);
    execution
}

async fn execute_provider_turn(
    host: &QuantixHost,
    store: &Arc<Mutex<TenderStore>>,
    prepared: &PreparedAgentRun,
    cancellation: CancellationToken,
) -> ProviderExecution {
    execute_provider_turn_from(host, store, prepared, cancellation, Instant::now()).await
}

async fn execute_provider_turn_from(
    host: &QuantixHost,
    store: &Arc<Mutex<TenderStore>>,
    prepared: &PreparedAgentRun,
    cancellation: CancellationToken,
    started: Instant,
) -> ProviderExecution {
    if let Some(execution) = cancellation_checkpoint(store, prepared, &cancellation, started) {
        return execution;
    }
    let operation_limit = match permission_duration(&prepared.permission_grant, Timestamp::now()) {
        Ok(duration) if !duration.is_zero() => duration,
        _ => return failed_execution(permission_failure(), started),
    };
    #[cfg(feature = "runtime-fixture")]
    let provider = if prepared.provider_selection.provider == AiProviderKind::Codex {
        let mut provider_slot = host.agent_provider().lock().await;
        let provider = match provider_slot.take() {
            Some(provider) => match provider.refresh_readiness().await {
                Ok(_) => provider,
                Err(failure)
                    if failure.category == ProviderFailureCategory::ProcessFailed
                        && failure.retry_safe =>
                {
                    host.record_application_diagnostic(
                        crate::DiagnosticSeverity::Info,
                        crate::DiagnosticComponent::Provider,
                        "provider_readiness_retry_started",
                        "Provider readiness retry started after a retry-safe process failure",
                        Some(format!("provider-turn-{}", prepared.run_id)),
                        None,
                        Some("started"),
                        Some(format!("{:?}", failure.category)),
                    );
                    match AgentProvider::codex_readiness(
                        host.process_supervisor(),
                        host.runtime_layout().codex_executable(),
                        host.application_home(),
                        cancellation.clone(),
                    )
                    .await
                    {
                        Ok(provider) => {
                            host.record_application_diagnostic(
                                crate::DiagnosticSeverity::Info,
                                crate::DiagnosticComponent::Provider,
                                "provider_readiness_retry_completed",
                                "Provider readiness retry completed",
                                Some(format!("provider-turn-{}", prepared.run_id)),
                                None,
                                Some("completed"),
                                None,
                            );
                            provider
                        }
                        Err(failure) => {
                            host.record_application_diagnostic(
                                crate::DiagnosticSeverity::Error,
                                crate::DiagnosticComponent::Provider,
                                "provider_readiness_retry_failed",
                                "Provider readiness retry failed",
                                Some(format!("provider-turn-{}", prepared.run_id)),
                                None,
                                Some("failed"),
                                Some(format!("{:?}", failure.category)),
                            );
                            return failed_execution(failure, started);
                        }
                    }
                }
                Err(failure) => return failed_execution(failure, started),
            },
            None => match AgentProvider::codex_readiness(
                host.process_supervisor(),
                host.runtime_layout().codex_executable(),
                host.application_home(),
                cancellation.clone(),
            )
            .await
            {
                Ok(provider) => provider,
                Err(failure) => return failed_execution(failure, started),
            },
        };
        let provider = provider.clone();
        *provider_slot = Some(provider.clone());
        Some(provider)
    } else {
        None
    };
    #[cfg(feature = "runtime-fixture")]
    if let Some(provider) = provider.as_ref() {
        if let Err(failure) = provider_connection_readiness(&provider.connection_snapshot()) {
            return failed_execution(failure, started);
        }
    }
    if let Some(execution) = cancellation_checkpoint(store, prepared, &cancellation, started) {
        return execution;
    }
    let archive_store = Arc::clone(store);
    let archive_prepared = prepared.clone();
    let thread_store = Arc::clone(store);
    let thread_prepared = prepared.clone();
    let requested_store = Arc::clone(store);
    let checkpoint_store = Arc::clone(store);
    let event_store = Arc::clone(store);
    let event_host = host.clone();
    let denial_store = Arc::clone(store);
    #[cfg(feature = "runtime-fixture")]
    let tool_store = Arc::clone(store);
    let requested_run_id = prepared.run_id.clone();
    let run_id = prepared.run_id.clone();
    let event_run_id = prepared.run_id.clone();
    let denial_run_id = prepared.run_id.clone();
    #[cfg(feature = "runtime-fixture")]
    let tool_run_id = prepared.run_id.clone();
    #[cfg(feature = "runtime-fixture")]
    let tool_prepared = prepared.clone();
    let callbacks = RunCallbacks {
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
        #[cfg(feature = "runtime-fixture")]
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
    };
    let operation_limit = operation_limit.saturating_sub(started.elapsed());
    if operation_limit.is_zero() {
        return failed_execution(permission_failure(), started);
    }
    let mut execution = match prepared.provider_selection.provider {
        AiProviderKind::Codex => {
            #[cfg(feature = "runtime-fixture")]
            {
                provider
                    .as_ref()
                    .expect("Codex provider initialized above")
                    .run_turn(prepared.clone(), operation_limit, cancellation, callbacks)
                    .await
            }
            #[cfg(not(feature = "runtime-fixture"))]
            {
                chatgpt_provider_turn(
                    host,
                    store,
                    prepared.clone(),
                    operation_limit,
                    cancellation,
                    callbacks,
                    started,
                )
                .await
            }
        }
    };
    host.observe_provider_usage(&execution.usage);
    execution.usage.elapsed_milliseconds =
        Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX));
    #[cfg(feature = "runtime-fixture")]
    if let Some(provider) = provider.filter(AgentProvider::is_closed) {
        let mut provider_slot = host.agent_provider().lock().await;
        let removed = provider_slot
            .as_ref()
            .is_some_and(|current| current.same_instance(&provider) && current.is_closed());
        if removed {
            *provider_slot = None;
        }
        drop(provider_slot);
        if removed {
            host.record_application_diagnostic(
                crate::DiagnosticSeverity::Info,
                crate::DiagnosticComponent::Process,
                "provider_process_cleanup_completed",
                "Closed provider process was removed from the active supervisor slot",
                Some(format!("provider-turn-{}", prepared.run_id)),
                None,
                Some("completed"),
                None,
            );
            let (status, summary) =
                codex_failure_connection_status(ProviderFailureCategory::ProcessFailed);
            let _ = save_codex_connection_status(host.application_home(), status, summary);
        }
    }
    execution
}

fn record_provider_turn_diagnostic(
    host: &QuantixHost,
    tender_id: &str,
    prepared: &PreparedAgentRun,
    execution: &ProviderExecution,
    started: Instant,
) {
    let (event_name, severity) = match execution.state {
        AgentRunState::Completed => ("provider_turn_completed", crate::DiagnosticSeverity::Info),
        AgentRunState::Failed => ("provider_turn_failed", crate::DiagnosticSeverity::Error),
        AgentRunState::Interrupted => {
            ("provider_turn_interrupted", crate::DiagnosticSeverity::Info)
        }
        AgentRunState::Indeterminate => (
            "provider_turn_indeterminate",
            crate::DiagnosticSeverity::Warning,
        ),
        AgentRunState::Running => ("provider_turn_running", crate::DiagnosticSeverity::Info),
    };
    let error_code = execution
        .failure
        .as_ref()
        .map(|failure| format!("{:?}", failure.category));
    let mut fact = crate::RecordDiagnosticFact::new(
        severity,
        crate::DiagnosticComponent::Provider,
        event_name,
        "Provider turn reached an execution boundary",
    );
    fact.correlation.operation_id = Some(format!("provider-turn-{}", prepared.run_id));
    fact.correlation.run_id = Some(prepared.run_id.clone());
    fact.correlation.task_id = Some(prepared.task.task_id.clone());
    fact.duration_ms = Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
    fact.outcome = Some(execution.state.as_str().into());
    fact.error_code = error_code.clone();
    fact.success = Some(execution.state == AgentRunState::Completed);
    host.diagnostics().record_tender(tender_id, fact);

    let mut deep = crate::RecordDiagnosticFact::new(
        severity,
        crate::DiagnosticComponent::Provider,
        "provider_turn_protocol_boundary",
        "Redacted provider protocol boundary captured",
    );
    deep.correlation.operation_id = Some(format!("provider-turn-{}", prepared.run_id));
    deep.correlation.run_id = Some(prepared.run_id.clone());
    deep.correlation.task_id = Some(prepared.task.task_id.clone());
    deep.duration_ms = Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
    deep.outcome = Some(execution.state.as_str().into());
    deep.error_code = error_code;
    deep.deep = true;
    deep.request_id = Some(format!("run-{}", prepared.run_id));
    deep.size_bytes = execution
        .candidate_payload_json
        .as_ref()
        .map(|payload| u64::try_from(payload.len()).unwrap_or(u64::MAX));
    deep.success = Some(execution.state == AgentRunState::Completed);
    host.diagnostics().record_tender(tender_id, deep);
}

fn record_provider_turn_started(host: &QuantixHost, tender_id: &str, prepared: &PreparedAgentRun) {
    let mut fact = crate::RecordDiagnosticFact::new(
        crate::DiagnosticSeverity::Info,
        crate::DiagnosticComponent::Provider,
        "provider_turn_started",
        "Provider turn execution started",
    );
    fact.correlation.operation_id = Some(format!("provider-turn-{}", prepared.run_id));
    fact.correlation.run_id = Some(prepared.run_id.clone());
    fact.correlation.task_id = Some(prepared.task.task_id.clone());
    fact.outcome = Some("started".into());
    host.diagnostics().record_tender(tender_id, fact);
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

#[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
fn chatgpt_failure_execution(
    thread_ref: Option<String>,
    turn_ref: Option<String>,
    failure: ProviderFailure,
    started: Instant,
) -> ProviderExecution {
    let state = match failure.category {
        ProviderFailureCategory::OutcomeUnknown => AgentRunState::Indeterminate,
        ProviderFailureCategory::Interrupted => AgentRunState::Interrupted,
        _ => AgentRunState::Failed,
    };
    let summary = match state {
        AgentRunState::Indeterminate => "Provider Turn outcome is indeterminate",
        AgentRunState::Interrupted => "Provider Turn interrupted",
        _ => "Provider Turn failed",
    };
    ProviderExecution {
        state,
        provider_thread_ref: thread_ref.clone(),
        provider_turn_ref: turn_ref.clone(),
        events: vec![PendingProviderEvent::new(
            ProviderEventKind::Terminal,
            summary,
            turn_ref.as_deref(),
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

#[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
fn epoch_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
fn merge_usage_snapshot(usage: &mut ProviderUsage, snapshot: &UsageSnapshot) {
    usage.input_tokens = Some(usage.input_tokens.unwrap_or(0) + snapshot.input_tokens);
    usage.output_tokens = Some(usage.output_tokens.unwrap_or(0) + snapshot.output_tokens);
    usage.total_tokens = Some(usage.total_tokens.unwrap_or(0) + snapshot.total_tokens);
    usage.cached_input_tokens =
        Some(usage.cached_input_tokens.unwrap_or(0) + snapshot.cached_input_tokens.unwrap_or(0));
    if snapshot.reasoning_output_tokens.is_some() {
        usage.reasoning_output_tokens = Some(
            usage.reasoning_output_tokens.unwrap_or(0)
                + snapshot.reasoning_output_tokens.unwrap_or(0),
        );
    }
}

#[derive(Default)]
struct ChatGptResponseLifecycle {
    accepted_ref: Option<String>,
    active_response_ref: Option<String>,
}

impl ChatGptResponseLifecycle {
    fn created(
        &mut self,
        response_id: String,
        on_accepted: &mut Option<Box<TurnAcceptedCallback>>,
    ) -> Result<(), ProviderFailure> {
        if self.active_response_ref.is_some() {
            return Err(protocol_failure(true));
        }
        if self.accepted_ref.is_none() {
            let Some(callback) = on_accepted.take() else {
                return Err(protocol_failure(true));
            };
            callback(&response_id)?;
            self.accepted_ref = Some(response_id.clone());
        }
        self.active_response_ref = Some(response_id);
        Ok(())
    }

    fn completed(&mut self, response_id: &str) -> Result<(), ProviderFailure> {
        if self.active_response_ref.as_deref() != Some(response_id) {
            return Err(protocol_failure(true));
        }
        self.active_response_ref = None;
        Ok(())
    }
}

#[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
fn chatgpt_reasoning_effort(
    selection: &ProviderReasoningSelection,
) -> Result<Option<ReasoningEffort>, ProviderFailure> {
    let effort = match selection {
        ProviderReasoningSelection::ProviderDefault => return Ok(None),
        ProviderReasoningSelection::CodexEffort(value) => value.as_str(),
    };
    let effort = match effort {
        "none" => Some(ReasoningEffort::None),
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::XHigh),
        _ => return Err(protocol_failure(false)),
    };
    Ok(effort)
}

#[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
fn with_production_token_client<T>(
    operation: impl FnOnce(&TokenClient) -> Result<T, ProviderFailure>,
) -> Result<T, ProviderFailure> {
    let token_client = TokenClient::new(PRODUCTION_ISSUER).map_err(|_| process_failure(false))?;
    operation(&token_client)
}

#[cfg(test)]
mod direct_backend_mapping_tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn maps_every_supported_chatgpt_reasoning_selection_exactly() {
        let cases = [
            ("none", ReasoningEffort::None),
            ("low", ReasoningEffort::Low),
            ("medium", ReasoningEffort::Medium),
            ("high", ReasoningEffort::High),
            ("xhigh", ReasoningEffort::XHigh),
        ];

        assert_eq!(
            chatgpt_reasoning_effort(&ProviderReasoningSelection::ProviderDefault).unwrap(),
            None
        );
        for (selection, expected) in cases {
            assert_eq!(
                chatgpt_reasoning_effort(&ProviderReasoningSelection::CodexEffort(
                    selection.to_owned()
                ))
                .unwrap(),
                Some(expected)
            );
        }
    }

    #[test]
    fn rejects_unknown_chatgpt_reasoning_selections() {
        assert!(
            chatgpt_reasoning_effort(&ProviderReasoningSelection::CodexEffort(
                "unsupported".to_owned()
            ))
            .is_err()
        );
    }

    #[test]
    fn production_token_client_is_scoped_to_blocking_refresh_work() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("multi-thread runtime");

        runtime.block_on(async {
            tokio::task::block_in_place(|| {
                with_production_token_client(|_| Ok::<_, ProviderFailure>(()))
            })
            .expect("the production token client must be created and dropped while blocking");
            tokio::task::yield_now().await;
        });
    }

    #[test]
    fn production_sink_accepts_sequential_responses_for_tool_roundtrip() {
        let persisted = Arc::new(Mutex::new(Vec::new()));
        let persisted_by_callback = Arc::clone(&persisted);
        let mut on_accepted: Option<Box<TurnAcceptedCallback>> =
            Some(Box::new(move |response_id| {
                persisted_by_callback
                    .lock()
                    .expect("accepted refs lock")
                    .push(response_id.to_owned());
                Ok(())
            }));
        let mut lifecycle = ChatGptResponseLifecycle::default();

        lifecycle
            .created("resp_tool_1".to_owned(), &mut on_accepted)
            .expect("accept initial tool response");
        lifecycle
            .completed("resp_tool_1")
            .expect("complete initial tool response");
        lifecycle
            .created("resp_tool_2".to_owned(), &mut on_accepted)
            .expect("accept tool-output follow-up response");
        lifecycle
            .completed("resp_tool_2")
            .expect("complete tool-output follow-up response");

        assert_eq!(
            *persisted.lock().expect("accepted refs lock"),
            vec!["resp_tool_1"],
            "the first response remains the durable Provider Turn identity"
        );
        assert_eq!(lifecycle.accepted_ref.as_deref(), Some("resp_tool_1"));
        assert_eq!(lifecycle.active_response_ref, None);
    }

    #[test]
    fn production_sink_rejects_mismatched_follow_up_completion() {
        let mut on_accepted: Option<Box<TurnAcceptedCallback>> = Some(Box::new(|_| Ok(())));
        let mut lifecycle = ChatGptResponseLifecycle::default();
        lifecycle
            .created("resp_tool_1".to_owned(), &mut on_accepted)
            .expect("accept initial response");
        lifecycle
            .completed("resp_tool_1")
            .expect("complete initial response");
        lifecycle
            .created("resp_tool_2".to_owned(), &mut on_accepted)
            .expect("accept follow-up response");

        let failure = lifecycle
            .completed("resp_foreign")
            .expect_err("a different response cannot complete the follow-up");
        assert_eq!(failure.category, ProviderFailureCategory::OutcomeUnknown);
        assert!(!failure.retry_safe);
        assert_eq!(lifecycle.accepted_ref.as_deref(), Some("resp_tool_1"));
        assert_eq!(
            lifecycle.active_response_ref.as_deref(),
            Some("resp_tool_2"),
            "a foreign completion must not consume the active response"
        );
    }
}

/// Drives one Codex-provider run against the Quantix-owned ChatGPT backend.
/// Connection readiness comes from the stored token connection (refreshing an
/// expired one through the issuer), and Typed Tool calls are authorized by the
/// run's Permission Grant exactly like the previous control-request path.
///
/// Request mapping from the prepared run: the provider instruction bundle
/// becomes `instructions`, Typed Tool specs become `tools`, and the Tender
/// Task objective becomes the single user input item. Unlike the Codex
/// app-server path, Data View payloads are not inlined into the request text
/// (they remain materialized workspace inputs referenced by the bundle). The
/// task output contract is sent as the Responses structured-output format and
/// validated again locally, while the run id doubles as the session identity.
#[cfg_attr(feature = "runtime-fixture", allow(dead_code))]
async fn chatgpt_provider_turn(
    host: &QuantixHost,
    store: &Arc<Mutex<TenderStore>>,
    prepared: PreparedAgentRun,
    operation_limit: Duration,
    cancellation: CancellationToken,
    callbacks: RunCallbacks,
    started: Instant,
) -> ProviderExecution {
    let RunCallbacks {
        on_thread_archived,
        on_thread_established,
        on_requested,
        on_accepted,
        mut on_event,
        mut on_denied,
        ..
    } = callbacks;
    let mut on_accepted = Some(on_accepted);
    let home = host.application_home().to_path_buf();
    let approved_selection = prepared.provider_selection.clone();
    let readiness = tokio::task::spawn_blocking(move || {
        project_approved_chatgpt_connection(
            &home,
            PRODUCTION_ISSUER,
            epoch_milliseconds(),
            &approved_selection,
        )
    })
    .await;
    let connection = match readiness {
        Ok(Ok(connection)) => connection,
        Ok(Err(failure)) => return failed_execution(failure, started),
        Err(_) => return failed_execution(process_failure(false), started),
    };
    if let Some(archived) = prepared.provider_thread_to_archive.as_deref() {
        if let Err(failure) = on_thread_archived(archived) {
            return failed_execution(failure, started);
        }
    }
    let resumed = prepared.provider_thread_ref.is_some();
    let thread_ref = prepared
        .provider_thread_ref
        .clone()
        .unwrap_or_else(|| format!("chatgpt:{}", prepared.run_id));
    if let Err(failure) = on_thread_established(&thread_ref, resumed) {
        return failed_execution(failure, started);
    }
    if let Err(failure) = on_requested() {
        return chatgpt_failure_execution(Some(thread_ref), None, failure, started);
    }
    let instruction = match provider_instruction_bundle(&prepared) {
        Ok(instruction) => instruction,
        Err(failure) => return chatgpt_failure_execution(Some(thread_ref), None, failure, started),
    };
    let tools = match direct_tool_specs(&prepared.permission_grant) {
        Ok(tools) => tools,
        Err(failure) => return chatgpt_failure_execution(Some(thread_ref), None, failure, started),
    };
    let output_schema: Value = match serde_json::from_str(&prepared.task.output_contract_json) {
        Ok(schema) => schema,
        Err(_) => {
            return chatgpt_failure_execution(
                Some(thread_ref),
                None,
                protocol_failure(false),
                started,
            )
        }
    };
    let backend = match ReqwestBackend::new(BACKEND_URL) {
        Ok(backend) => backend,
        Err(_) => {
            return chatgpt_failure_execution(
                Some(thread_ref),
                None,
                process_failure(false),
                started,
            )
        }
    };
    let reasoning_effort = match chatgpt_reasoning_effort(&prepared.provider_selection.reasoning) {
        Ok(effort) => effort,
        Err(failure) => return chatgpt_failure_execution(Some(thread_ref), None, failure, started),
    };
    let request = BackendRequest {
        model: prepared.provider_selection.model_id.clone(),
        instructions: instruction,
        input_items: vec![json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": prepared.task.objective.clone()}],
        })],
        tools,
        output_schema: output_schema.clone(),
        store: false,
        include_reasoning: true,
        reasoning_effort,
        session_id: String::new(),
    };

    let deadline = started.checked_add(operation_limit).unwrap_or(started);
    let is_cancelled = || cancellation.is_cancelled() || Instant::now() >= deadline;

    let tool_counter = Arc::new(AtomicU64::new(0));
    let authorize_store = Arc::clone(store);
    let authorize_prepared = prepared.clone();
    let authorize_run_id = prepared.run_id.clone();
    let authorize_tool = move |tool_name: &str, arguments: &str| -> Result<Value, ToolRejection> {
        let arguments_value: Value = serde_json::from_str(arguments)
            .map_err(|_| ToolRejection::NotPermitted("malformed_tool_arguments"))?;
        if !typed_tool_is_known(tool_name) {
            return Err(ToolRejection::NotPermitted(
                PermissionDenialReason::ToolNotGranted.as_str(),
            ));
        }
        let valid = typed_tool_arguments_are_valid(tool_name, &arguments_value)
            .map_err(|_| ToolRejection::Failed("tool_validation_failed"))?;
        if !valid {
            return Err(ToolRejection::NotPermitted(
                PermissionDenialReason::ToolNotGranted.as_str(),
            ));
        }
        let correlation_id = format!(
            "chatgpt-tool-{}",
            tool_counter.fetch_add(1, Ordering::SeqCst)
        );
        let authorized = authorize_store
            .lock()
            .map_err(|_| ToolRejection::Failed("store_unavailable"))?
            .authorize_agent_typed_tool(&authorize_run_id, &correlation_id, tool_name)
            .map_err(|_| ToolRejection::Failed("store_unavailable"))?;
        if !authorized {
            return Err(ToolRejection::NotPermitted(
                PermissionDenialReason::ToolNotGranted.as_str(),
            ));
        }
        let record = |succeeded: bool| -> Result<(), ToolRejection> {
            authorize_store
                .lock()
                .map_err(|_| ToolRejection::Failed("store_unavailable"))?
                .record_agent_typed_tool_execution(
                    &authorize_run_id,
                    &correlation_id,
                    tool_name,
                    succeeded,
                )
                .map_err(|_| ToolRejection::Failed("audit_unavailable"))
        };
        match execute_typed_tool(&authorize_prepared, tool_name, &arguments_value) {
            Ok(output) => {
                let payload: Value = serde_json::from_str(&output).unwrap_or(Value::String(output));
                record(true)?;
                Ok(payload)
            }
            Err(_) => {
                record(false)?;
                Err(ToolRejection::Failed("tool_execution_failed"))
            }
        }
    };

    let mut response_lifecycle = ChatGptResponseLifecycle::default();
    let mut streamed_usage = ProviderUsage::default();
    let mut final_text: Option<String> = None;
    let mut sink = |event: StreamEvent| -> Result<(), ProviderFailure> {
        match event {
            StreamEvent::Created { response_id } => {
                response_lifecycle.created(response_id, &mut on_accepted)
            }
            StreamEvent::Completed { response_id, usage } => {
                response_lifecycle.completed(&response_id)?;
                merge_usage_snapshot(&mut streamed_usage, &usage);
                let observed = PendingProviderEvent::new(
                    ProviderEventKind::UsageObserved,
                    "ChatGPT usage observed",
                    None,
                );
                on_event(&observed, &streamed_usage)
            }
            StreamEvent::ItemDone(frame) => {
                if frame.pointer("/item/type").and_then(Value::as_str) != Some("message") {
                    return Ok(());
                }
                let mut text = String::new();
                if let Some(parts) = frame.pointer("/item/content").and_then(Value::as_array) {
                    for part in parts {
                        if part.get("type").and_then(Value::as_str) == Some("output_text") {
                            if let Some(chunk) = part.get("text").and_then(Value::as_str) {
                                text.push_str(chunk);
                            }
                        }
                    }
                }
                final_text = Some(text);
                Ok(())
            }
            StreamEvent::ItemAdded(_)
            | StreamEvent::TextDelta(_)
            | StreamEvent::FunctionCallDelta { .. }
            | StreamEvent::FunctionCallDone { .. }
            | StreamEvent::Errored(_) => Ok(()),
        }
    };
    let mut denied = |call_id: &str,
                      tool_name: &str,
                      raw_arguments: &str,
                      reason: &str|
     -> Result<(), ProviderFailure> {
        let mut fingerprint = Sha256::new();
        fingerprint.update(tool_name.as_bytes());
        fingerprint.update([0]);
        fingerprint.update(raw_arguments.as_bytes());
        let request_fingerprint = fingerprint
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let denial_reason =
            PermissionDenialReason::parse(reason).unwrap_or(PermissionDenialReason::DefaultDeny);
        let event = PendingProviderEvent::new(
            ProviderEventKind::ControlRequestDenied,
            "ChatGPT Typed Tool call denied by the Host",
            Some(tool_name),
        )
        .with_control_denial(call_id.to_owned(), request_fingerprint, denial_reason);
        on_denied(&event)
    };
    let refresh_auth = |expected_account_id: &str| {
        with_production_token_client(|token_client| {
            refresh_approved_chatgpt_connection(
                host.application_home(),
                token_client,
                epoch_milliseconds(),
                expected_account_id,
                &prepared.provider_selection,
            )
        })
    };

    let turn_context = TurnContext {
        backend: &backend,
        auth: &connection,
        refresh_auth: &refresh_auth,
        request,
        session_id: prepared.run_id.clone(),
        authorize_tool: &authorize_tool,
        is_cancelled: &is_cancelled,
        on_event: &mut sink,
        on_tool_denied: &mut denied,
    };
    let outcome = execute_chatgpt_backend_turn(turn_context).await;
    match outcome {
        Ok(result) => {
            let candidate = validate_candidate(
                final_text.as_deref(),
                &output_schema,
                prepared
                    .task
                    .resource_budget
                    .output_bytes
                    .min(PROVIDER_OUTPUT_LIMIT as u32),
            );
            match candidate {
                Ok(candidate) => {
                    let turn_ref = response_lifecycle.accepted_ref.clone();
                    ProviderExecution {
                        state: AgentRunState::Completed,
                        provider_thread_ref: Some(thread_ref),
                        provider_turn_ref: response_lifecycle.accepted_ref,
                        events: vec![PendingProviderEvent::new(
                            ProviderEventKind::Terminal,
                            "Provider Turn completed",
                            turn_ref.as_deref(),
                        )],
                        usage: result.usage,
                        failure: None,
                        candidate_payload_json: Some(candidate),
                    }
                }
                Err(failure) => chatgpt_failure_execution(
                    Some(thread_ref),
                    response_lifecycle.accepted_ref,
                    failure,
                    started,
                ),
            }
        }
        Err(failure) => chatgpt_failure_execution(
            Some(thread_ref),
            response_lifecycle.accepted_ref,
            failure,
            started,
        ),
    }
}

fn deterministic_provider_execution(
    prepared: &PreparedAgentRun,
    outcome: DeterministicProviderOutcome,
) -> ProviderExecution {
    match outcome {
        DeterministicProviderOutcome::Completed => ProviderExecution {
            state: AgentRunState::Completed,
            provider_thread_ref: Some(
                prepared
                    .provider_thread_ref
                    .clone()
                    .unwrap_or_else(|| format!("acceptance-thread-{}", prepared.run_id)),
            ),
            provider_turn_ref: Some(format!("acceptance-turn-{}", prepared.run_id)),
            events: vec![PendingProviderEvent::new(
                ProviderEventKind::Terminal,
                "Deterministic acceptance provider returned a schema-valid proposed result",
                Some("quantix-deterministic-provider-v1"),
            )],
            usage: ProviderUsage {
                input_tokens: Some(1),
                output_tokens: Some(1),
                elapsed_milliseconds: Some(0),
                ..ProviderUsage::default()
            },
            failure: None,
            candidate_payload_json: Some(
                serde_json_canonicalizer::to_string(&json!({
                    "recommended_next_action": "Continue controlled fixture processing",
                    "summary": "Deterministic provider outcome for the challenged application"
                }))
                .expect("static deterministic provider result is canonical JSON"),
            ),
        },
        DeterministicProviderOutcome::Failed => failed_execution(
            ProviderFailure::new(
                ProviderFailureCategory::OutputInvalid,
                true,
                "Retry only after the deterministic provider failure is reviewed.",
                Some("The deterministic adapter injected an invalid provider outcome."),
            ),
            Instant::now(),
        ),
        DeterministicProviderOutcome::Interrupted => interrupted_execution(Instant::now()),
    }
}

fn deterministic_provider_selection() -> AiExecutionSelection {
    AiExecutionSelection {
        connection_id: "quantix-deterministic-provider-v1".into(),
        provider: AiProviderKind::Codex,
        model_id: "quantix-deterministic-model-v1".into(),
        reasoning: ProviderReasoningSelection::ProviderDefault,
        catalogue_fetched_at: "1970-01-01T00:00:00Z".into(),
        adapter_version: "quantix-deterministic-provider-v1".into(),
    }
}

#[cfg(feature = "runtime-fixture")]
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

#[cfg(feature = "runtime-fixture")]
fn authentication_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::AuthenticationRequired,
        true,
        "Connect your ChatGPT subscription in Settings before retrying.",
        Some("No usable ChatGPT connection is available."),
    )
}

#[cfg(feature = "runtime-fixture")]
fn subscription_failure() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::SubscriptionRequired,
        true,
        "Connect an eligible Codex-managed ChatGPT subscription, then retry.",
        Some("The active Codex account is not an eligible ChatGPT subscription."),
    )
}

#[cfg(feature = "runtime-fixture")]
fn turn_acceptance_unknown() -> ProviderFailure {
    ProviderFailure::new(
        ProviderFailureCategory::OutcomeUnknown,
        false,
        "Resolve the quarantined Agent Run before retrying.",
        Some(
            "The Provider Turn may have started, but its identity and outcome could not be established.",
        ),
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

#[cfg(feature = "runtime-fixture")]
pub(crate) fn chatgpt_subscription_is_supported(
    account_type: Option<&str>,
    plan_type: Option<&str>,
) -> bool {
    account_type == Some("chatgpt")
        && plan_type.is_some_and(|plan| SUPPORTED_CHATGPT_PLANS.contains(&plan))
}

#[cfg(feature = "runtime-fixture")]
fn codex_user_agent_is_supported(user_agent: &str) -> bool {
    let server_identity = user_agent
        .split_once(" (")
        .map_or(user_agent, |(identity, _)| identity);
    server_identity
        .rsplit_once('/')
        .is_some_and(|(product, version)| !product.is_empty() && version == CODEX_VERSION)
}

#[cfg(feature = "runtime-fixture")]
fn controlled_codex_environment(
    application_home: &Path,
) -> io::Result<(PathBuf, Vec<(OsString, OsString)>)> {
    let engineer_home = application_home.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Application Home must have an Engineer home parent",
        )
    })?;
    // Codex may grant its Windows sandbox identity access to its process
    // directory. Keep that disposable provider directory outside Application
    // Home so starting the provider cannot broaden access to Tender Stores.
    let process_directory = engineer_home.join(".quantix-provider");
    let staging = process_directory.join("staging");
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
    Ok((process_directory, environment))
}

#[cfg(feature = "runtime-fixture")]
pub(super) struct CodexProviderProcess {
    conversation: Option<SupervisedConversation>,
    connection: ProviderConnectionView,
}

#[cfg(feature = "runtime-fixture")]
impl CodexProviderProcess {
    pub(super) async fn readiness(
        supervisor: &crate::process_supervisor::ProcessSupervisor,
        executable: PathBuf,
        application_home: &Path,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderFailure> {
        let (process_directory, environment) =
            controlled_codex_environment(application_home).map_err(|_| process_failure(false))?;
        let mut conversation = supervisor
            .start_conversation(
                ProcessSpec {
                    executable,
                    arguments: restricted_codex_arguments(),
                    current_directory: Some(process_directory),
                    environment,
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
        let connection = match Self::initialize(&mut conversation).await {
            Ok(connection) => connection,
            Err(failure) => {
                let termination = conversation
                    .failure_termination()
                    .unwrap_or(ProcessTermination::Cancelled);
                let _ = conversation.finish(Some(termination)).await;
                return Err(failure);
            }
        };
        Ok(Self {
            conversation: Some(conversation),
            connection,
        })
    }

    pub(super) async fn refresh_readiness(
        &mut self,
    ) -> Result<ProviderConnectionView, ProviderFailure> {
        let conversation = self.conversation_mut()?;
        conversation
            .begin_operation(
                PROVIDER_READINESS_TIMEOUT,
                PROVIDER_OUTPUT_LIMIT,
                PROVIDER_OUTPUT_LIMIT,
            )
            .map_err(|_| process_failure(false))?;
        let connection = Self::verify_subscription(conversation).await?;
        self.connection = connection.clone();
        Ok(connection)
    }

    pub(super) fn connection_snapshot(&self) -> ProviderConnectionView {
        self.connection.clone()
    }

    async fn initialize(
        conversation: &mut SupervisedConversation,
    ) -> Result<ProviderConnectionView, ProviderFailure> {
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
    ) -> Result<ProviderConnectionView, ProviderFailure> {
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
        let account = match read_expected_response(conversation, &json!(1), "v2/GetAccountResponse")
            .await
        {
            Ok(account) => account,
            Err(failure) if failure.category == ProviderFailureCategory::AuthenticationRequired => {
                return Ok(codex_connection_without_catalog(
                    ProviderConnectionStatus::AuthenticationRequired,
                    None,
                    None,
                    "Connect an OpenAI account to use Codex intelligence.",
                ));
            }
            Err(failure) if failure.category == ProviderFailureCategory::SubscriptionRequired => {
                return Ok(codex_connection_without_catalog(
                    ProviderConnectionStatus::SubscriptionRequired,
                    None,
                    None,
                    "The connected OpenAI account does not provide an eligible Codex subscription.",
                ));
            }
            Err(failure) => return Err(failure),
        };
        let account_type = account.pointer("/account/type").and_then(Value::as_str);
        if account_type.is_none() {
            return Ok(codex_connection_without_catalog(
                ProviderConnectionStatus::AuthenticationRequired,
                None,
                None,
                "Connect an OpenAI account to use Codex intelligence.",
            ));
        }
        let plan_type = account.pointer("/account/planType").and_then(Value::as_str);
        if !chatgpt_subscription_is_supported(account_type, plan_type) {
            return Ok(codex_connection_without_catalog(
                ProviderConnectionStatus::SubscriptionRequired,
                account
                    .pointer("/account/email")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                plan_type.map(str::to_owned),
                "The connected OpenAI account does not provide an eligible Codex subscription.",
            ));
        }
        let account_label = account
            .pointer("/account/email")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let account_plan = plan_type.map(str::to_owned);
        let mut cursor: Option<String> = None;
        let mut available_models = Vec::new();
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
            let page_models = models
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| protocol_failure(false))?;
            for model in page_models {
                if let Some(model) = parse_codex_model(model) {
                    if available_models
                        .iter()
                        .any(|existing: &ProviderModelOption| existing.model_id == model.model_id)
                    {
                        return Err(protocol_failure(false));
                    }
                    available_models.push(model);
                }
            }
            cursor = models
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        if available_models.is_empty() || cursor.is_some() {
            return Err(protocol_failure(false));
        }
        let default_count = available_models
            .iter()
            .filter(|model| model.is_default)
            .count();
        if default_count != 1 {
            return Err(protocol_failure(false));
        }
        Ok(ProviderConnectionView {
            connection_id: CODEX_CONNECTION_ID.to_owned(),
            provider: AiProviderKind::Codex,
            display_name: "OpenAI account via Codex".to_owned(),
            status: ProviderConnectionStatus::Ready,
            account_label,
            account_plan,
            models: available_models,
            catalogue_fetched_at: Some(Timestamp::now().to_string()),
            adapter_version: codex_connection_version(),
            status_summary: "Ready to run Tender work.".to_owned(),
        })
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

#[cfg(feature = "runtime-fixture")]
fn codex_connection_without_catalog(
    status: ProviderConnectionStatus,
    account_label: Option<String>,
    account_plan: Option<String>,
    status_summary: &str,
) -> ProviderConnectionView {
    ProviderConnectionView {
        connection_id: CODEX_CONNECTION_ID.to_owned(),
        provider: AiProviderKind::Codex,
        display_name: "OpenAI account via Codex".to_owned(),
        status,
        account_label,
        account_plan,
        models: Vec::new(),
        catalogue_fetched_at: None,
        adapter_version: codex_connection_version(),
        status_summary: status_summary.to_owned(),
    }
}

#[cfg(feature = "runtime-fixture")]
fn parse_codex_model(model: &Value) -> Option<ProviderModelOption> {
    if model.get("hidden").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    let model_id = model.get("id")?.as_str()?.trim();
    if model_id.is_empty() || model_id.len() > 200 {
        return None;
    }
    let input_modalities = model
        .get("inputModalities")?
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    if !input_modalities.iter().any(|modality| modality == "text") {
        return None;
    }
    let default_effort = model.get("defaultReasoningEffort")?.as_str()?.trim();
    if default_effort.is_empty() {
        return None;
    }
    let reasoning_options = model
        .get("supportedReasoningEfforts")?
        .as_array()?
        .iter()
        .map(|option| {
            let effort = option.get("reasoningEffort")?.as_str()?.trim();
            let description = option.get("description")?.as_str()?.trim();
            if effort.is_empty()
                || effort.len() > 100
                || description.is_empty()
                || description.len() > 1_000
            {
                return None;
            }
            Some(ProviderReasoningOption {
                selection: ProviderReasoningSelection::CodexEffort(effort.to_owned()),
                label: effort.to_owned(),
                description: description.to_owned(),
                is_default: effort == default_effort,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    if reasoning_options.is_empty()
        || reasoning_options
            .iter()
            .filter(|option| option.is_default)
            .count()
            != 1
    {
        return None;
    }
    let display_name = model.get("displayName")?.as_str()?.trim().to_owned();
    if display_name.is_empty() || display_name.len() > 300 {
        return None;
    }
    let description = model
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if description.len() > 2_000 {
        return None;
    }
    Some(ProviderModelOption {
        model_id: model_id.to_owned(),
        display_name,
        description,
        is_default: model.get("isDefault").and_then(Value::as_bool) == Some(true),
        input_modalities,
        reasoning_options,
    })
}

#[cfg(feature = "runtime-fixture")]
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

#[cfg(feature = "runtime-fixture")]
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
    use crate::tender_store::CreateTenderCommand;

    #[tokio::test]
    async fn deterministic_provider_accepts_a_new_default_local_only_tender() {
        let root = tempfile::tempdir().expect("temporary deterministic acceptance home");
        let application_home = root.path().join(".quantix");
        let resources = root.path().join("resources");
        let host = QuantixHost::new(&application_home, &resources);
        assert!(matches!(
            crate::ensure_quantix_setup(&host).state,
            crate::SetupState::Ready | crate::SetupState::Warning
        ));
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Deterministic local-only Tender".into(),
            })
            .expect("create default local-only Tender");
        let public_result = host
            .run_bootstrap_agent(RunBootstrapAgentCommand {
                tender_id: tender.tender_id.clone(),
                retry_of_run_id: None,
            })
            .await;
        assert_eq!(
            public_result
                .expect_err("public bootstrap must require the Tender AI binding")
                .code,
            TenderErrorCode::AiProviderRequired
        );

        let expected_selection = deterministic_provider_selection();
        let completed = host
            .run_bootstrap_agent_with_deterministic_provider(
                RunBootstrapAgentCommand {
                    tender_id: tender.tender_id.clone(),
                    retry_of_run_id: None,
                },
                DeterministicProviderOutcome::Completed,
            )
            .await
            .expect("deterministic helper must not require provider binding");
        assert_eq!(completed.state, AgentRunState::Completed, "{completed:#?}");
        assert_eq!(completed.provider_selection, expected_selection);

        let failed = host
            .run_bootstrap_agent_with_deterministic_provider(
                RunBootstrapAgentCommand {
                    tender_id: tender.tender_id.clone(),
                    retry_of_run_id: None,
                },
                DeterministicProviderOutcome::Failed,
            )
            .await
            .expect("persist deterministic failed outcome");
        assert_eq!(failed.state, AgentRunState::Failed, "{failed:#?}");
        assert_eq!(failed.provider_selection, expected_selection);
        assert_eq!(
            failed.failure.as_ref().map(|failure| failure.category),
            Some(ProviderFailureCategory::OutputInvalid)
        );

        let interrupted = host
            .run_bootstrap_agent_with_deterministic_provider(
                RunBootstrapAgentCommand {
                    tender_id: tender.tender_id,
                    retry_of_run_id: None,
                },
                DeterministicProviderOutcome::Interrupted,
            )
            .await
            .expect("persist deterministic interrupted outcome");
        assert_eq!(
            interrupted.state,
            AgentRunState::Interrupted,
            "{interrupted:#?}"
        );
        assert_eq!(interrupted.provider_selection, expected_selection);
        assert!(interrupted.failure.is_some());
    }
}
