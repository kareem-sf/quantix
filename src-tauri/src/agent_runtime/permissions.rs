use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::tender_store::{TenderCommandError, TenderErrorCode};

use super::{
    AgentProfileVersionView, AgentResourceBudget, AgentRunPermissions, AgentTaskInputReference,
    TenderTaskView,
};

const PERMISSION_POLICY_VERSION: u32 = 1;
const CAPABILITY_CATALOGUE_VERSION: u32 = 1;
const BOOTSTRAP_WORK_PLAN_VERSION: u32 = 1;
const TENDER_METADATA_SCOPE: &str = "tender_metadata";
const PROPOSE_INTAKE_ACTION: &str = "propose_intake_readiness";
pub(crate) const TENDER_METADATA_TOOL_NAME: &str = "quantix_read_tender_metadata";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DataClassification {
    TenderInternal,
    Sensitive,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PermissionDenialReason {
    DefaultDeny,
    ProhibitedAction,
    GrantExpired,
    SecretData,
    OutsideCeiling,
    WorkPlanAmendmentRequired,
    ToolNotGranted,
    ThreadExposureIncompatible,
    EngineerDenied,
    Superseded,
    AccessRevoked,
}

impl PermissionDenialReason {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::DefaultDeny => "default_deny",
            Self::ProhibitedAction => "prohibited_action",
            Self::GrantExpired => "grant_expired",
            Self::SecretData => "secret_data",
            Self::OutsideCeiling => "outside_ceiling",
            Self::WorkPlanAmendmentRequired => "work_plan_amendment_required",
            Self::ToolNotGranted => "tool_not_granted",
            Self::ThreadExposureIncompatible => "thread_exposure_incompatible",
            Self::EngineerDenied => "engineer_denied",
            Self::Superseded => "superseded",
            Self::AccessRevoked => "access_revoked",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "default_deny" => Ok(Self::DefaultDeny),
            "prohibited_action" => Ok(Self::ProhibitedAction),
            "grant_expired" => Ok(Self::GrantExpired),
            "secret_data" => Ok(Self::SecretData),
            "outside_ceiling" => Ok(Self::OutsideCeiling),
            "work_plan_amendment_required" => Ok(Self::WorkPlanAmendmentRequired),
            "tool_not_granted" => Ok(Self::ToolNotGranted),
            "thread_exposure_incompatible" => Ok(Self::ThreadExposureIncompatible),
            "engineer_denied" => Ok(Self::EngineerDenied),
            "superseded" => Ok(Self::Superseded),
            "access_revoked" => Ok(Self::AccessRevoked),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

impl DataClassification {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::TenderInternal => "tender_internal",
            Self::Sensitive => "sensitive",
            Self::Secret => "secret",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "tender_internal" => Ok(Self::TenderInternal),
            "sensitive" => Ok(Self::Sensitive),
            "secret" => Ok(Self::Secret),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ToolSideEffectClass {
    ReadOnly,
    WorkingAreaWrite,
    StagedOutputWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ToolIdempotency {
    Idempotent,
    IdempotencyKeyRequired,
    NeverRetry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TypedToolQuota {
    pub maximum_calls: u32,
    pub maximum_input_bytes: u32,
    pub maximum_output_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TypedToolDefinition {
    pub name: String,
    pub version: u32,
    pub input_schema_json: String,
    pub output_schema_json: String,
    pub required_capability: String,
    pub required_action: String,
    pub required_data_scopes: Vec<String>,
    pub allowed_data_classifications: Vec<DataClassification>,
    pub side_effect_class: ToolSideEffectClass,
    pub quota: TypedToolQuota,
    pub safety_limits: Vec<String>,
    pub idempotency: ToolIdempotency,
    pub audit_event_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DataViewManifest {
    pub view_id: String,
    pub schema_version: u32,
    pub relative_path: String,
    pub sha256: String,
    pub data_scope: String,
    pub data_classification: DataClassification,
    pub exact_inputs: Vec<AgentTaskInputReference>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadExposureSet {
    pub exact_inputs: Vec<AgentTaskInputReference>,
    pub data_scopes: Vec<String>,
    pub data_classifications: Vec<DataClassification>,
}

impl ThreadExposureSet {
    pub(crate) fn from_grant(grant: &PermissionGrant) -> Self {
        let mut exposure = Self {
            exact_inputs: grant
                .data_views
                .iter()
                .flat_map(|view| view.exact_inputs.clone())
                .collect(),
            data_scopes: grant.data_scopes.clone(),
            data_classifications: grant.data_classifications.clone(),
        };
        exposure.canonicalize();
        exposure
    }

    pub(crate) fn merge(&mut self, other: &Self) {
        self.exact_inputs.extend(other.exact_inputs.clone());
        self.data_scopes.extend(other.data_scopes.clone());
        self.data_classifications
            .extend(other.data_classifications.clone());
        self.canonicalize();
    }

    pub(crate) fn is_compatible_with(&self, grant: &PermissionGrant) -> bool {
        let next = Self::from_grant(grant);
        self.exact_inputs
            .iter()
            .all(|input| next.exact_inputs.contains(input))
            && self
                .data_scopes
                .iter()
                .all(|scope| next.data_scopes.contains(scope))
            && self
                .data_classifications
                .iter()
                .all(|classification| next.data_classifications.contains(classification))
            && !self
                .data_classifications
                .contains(&DataClassification::Secret)
    }

    fn canonicalize(&mut self) {
        self.exact_inputs.sort_by(|left, right| {
            (&left.kind, &left.reference, left.version).cmp(&(
                &right.kind,
                &right.reference,
                right.version,
            ))
        });
        self.exact_inputs.dedup();
        self.data_scopes.sort();
        self.data_scopes.dedup();
        self.data_classifications.sort();
        self.data_classifications.dedup();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRunWorkspaceManifest {
    pub workspace_id: String,
    pub read_only_inputs: String,
    pub working_area: String,
    pub staged_outputs: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PermissionCeiling {
    pub exact_inputs: Vec<AgentTaskInputReference>,
    pub data_scopes: Vec<String>,
    pub data_classifications: Vec<DataClassification>,
    pub allowed_actions: Vec<String>,
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AccessRequest {
    pub request_id: String,
    pub run_id: String,
    pub exact_inputs: Vec<AgentTaskInputReference>,
    pub data_scopes: Vec<String>,
    pub data_classifications: Vec<DataClassification>,
    pub allowed_actions: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub purpose: String,
    pub recurring: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AccessApproval {
    pub approval_id: String,
    pub request_id: String,
    pub run_id: String,
    pub approved_by: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OneRunAccessGrant {
    pub approval_id: String,
    pub request_id: String,
    pub run_id: String,
    pub exact_inputs: Vec<AgentTaskInputReference>,
    pub data_scopes: Vec<String>,
    pub data_classifications: Vec<DataClassification>,
    pub allowed_actions: Vec<String>,
    pub allowed_tools: Vec<String>,
    pub purpose: String,
    pub approved_by: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentAccessRequestStatus {
    Blocked,
    Approved,
    Denied,
    Expired,
    Superseded,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentAccessResolution {
    Deny,
    Supersede,
    Revoke,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentAccessRequestView {
    pub request: AccessRequest,
    pub status: AgentAccessRequestStatus,
    pub one_run_grant: Option<OneRunAccessGrant>,
    pub denial_reason: Option<PermissionDenialReason>,
    pub requested_at: String,
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PermissionGrant {
    pub grant_id: String,
    pub policy_version: u32,
    pub capability_catalogue_version: u32,
    pub work_plan_version: u32,
    pub profile_id: String,
    pub profile_version: u32,
    pub task_id: String,
    pub purpose: String,
    pub data_scopes: Vec<String>,
    pub data_classifications: Vec<DataClassification>,
    pub allowed_actions: Vec<String>,
    pub typed_tools: Vec<TypedToolDefinition>,
    pub network_allowed: bool,
    pub workspace_write_allowed: bool,
    pub data_views: Vec<DataViewManifest>,
    pub thread_exposure: ThreadExposureSet,
    pub workspace: AgentRunWorkspaceManifest,
    pub access_ceiling: PermissionCeiling,
    pub resource_budget: AgentResourceBudget,
    pub issued_at: String,
    pub expires_at: String,
}

pub(crate) struct BootstrapGrantRequest<'a> {
    pub run_id: &'a str,
    pub grant_id: String,
    pub application_home: &'a Path,
    pub tender_id: &'a str,
    pub tender_name: &'a str,
    pub tender_revision: u32,
    pub profile: &'a AgentProfileVersionView,
    pub task: &'a TenderTaskView,
    pub issued_at: &'a str,
}

pub(crate) struct PreBidDataGrantRequest<'a> {
    pub run_id: &'a str,
    pub grant_id: String,
    pub application_home: &'a Path,
    pub tender_id: &'a str,
    pub profile: &'a AgentProfileVersionView,
    pub task: &'a TenderTaskView,
    pub issued_at: &'a str,
    pub data_scope: &'a str,
    pub allowed_action: &'a str,
    pub relative_path: &'a str,
    pub view_id: &'a str,
    pub payload: &'a Value,
}

pub(crate) struct PlannedTaskGrantRequest<'a> {
    pub run_id: &'a str,
    pub grant_id: String,
    pub application_home: &'a Path,
    pub tender_id: &'a str,
    pub work_plan_version: u32,
    pub profile: &'a AgentProfileVersionView,
    pub task: &'a TenderTaskView,
    pub issued_at: &'a str,
    pub expires_at: &'a str,
    pub payload: &'a Value,
}

pub(crate) fn derive_bootstrap_grant(
    request: BootstrapGrantRequest<'_>,
) -> Result<(PermissionGrant, PathBuf), TenderCommandError> {
    let capability = bootstrap_capability_ceiling();
    let work_plan = bootstrap_work_plan_ceiling();
    let policy = permission_policy();
    let sources = [
        &capability,
        &request.profile.permissions,
        &work_plan,
        &request.task.permissions,
        &policy,
    ];
    let data_scopes = intersect_strings(&sources, |permissions| &permissions.data_scopes);
    let data_classifications = intersect_classifications(&sources);
    let allowed_actions = intersect_strings(&sources, |permissions| &permissions.allowed_actions);
    let allowed_tool_names = intersect_strings(&sources, |permissions| &permissions.allowed_tools);
    let ceiling_sources = [
        &capability,
        &request.profile.permissions,
        &work_plan,
        &policy,
    ];
    let ceiling_allowed_tools =
        intersect_strings(&ceiling_sources, |permissions| &permissions.allowed_tools);

    if data_scopes != [TENDER_METADATA_SCOPE]
        || data_classifications != [DataClassification::TenderInternal]
        || allowed_actions != [PROPOSE_INTAKE_ACTION]
        || policy.network_allowed
        || sources.iter().any(|source| source.network_allowed)
        || sources.iter().any(|source| !source.workspace_write_allowed)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }

    let typed_tools = bootstrap_tool_catalogue()
        .into_iter()
        .filter(|tool| allowed_tool_names.contains(&tool.name))
        .collect::<Vec<_>>();
    if typed_tools.len() != allowed_tool_names.len() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }

    let workspace = request
        .application_home
        .join("staging")
        .join(format!("agent-{}-{}", request.tender_id, request.run_id));
    let inputs = workspace.join("inputs");
    let working = workspace.join("working");
    let outputs = workspace.join("outputs");
    fs::create_dir(&workspace).map_err(store_unavailable)?;
    let materialized = (|| {
        fs::create_dir(&inputs).map_err(store_unavailable)?;
        fs::create_dir(&working).map_err(store_unavailable)?;
        fs::create_dir(&outputs).map_err(store_unavailable)?;

        let relative_path = "inputs/tender-metadata-v1.json";
        let payload = serde_json_canonicalizer::to_string(&json!({
            "data_classification": DataClassification::TenderInternal,
            "data_scope": TENDER_METADATA_SCOPE,
            "schema_version": 1,
            "source": {
                "kind": "tender_revision",
                "reference": request.tender_id,
                "version": request.tender_revision,
            },
            "tender": {
                "name": request.tender_name,
                "revision": request.tender_revision,
            }
        }))
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
        .into_bytes();
        let path = workspace.join(relative_path);
        fs::write(&path, &payload).map_err(store_unavailable)?;
        let mut permissions = fs::metadata(&path)
            .map_err(store_unavailable)?
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).map_err(store_unavailable)?;

        let view = DataViewManifest {
            view_id: format!("tender-metadata-v{}", request.tender_revision),
            schema_version: 1,
            relative_path: relative_path.into(),
            sha256: Sha256::digest(&payload)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            data_scope: TENDER_METADATA_SCOPE.into(),
            data_classification: DataClassification::TenderInternal,
            exact_inputs: request.task.exact_inputs.clone(),
        };
        let mut grant = PermissionGrant {
            grant_id: request.grant_id,
            policy_version: PERMISSION_POLICY_VERSION,
            capability_catalogue_version: CAPABILITY_CATALOGUE_VERSION,
            work_plan_version: BOOTSTRAP_WORK_PLAN_VERSION,
            profile_id: request.profile.profile_id.clone(),
            profile_version: request.profile.version,
            task_id: request.task.task_id.clone(),
            purpose: request.task.objective.clone(),
            data_scopes,
            data_classifications,
            allowed_actions,
            typed_tools,
            network_allowed: false,
            workspace_write_allowed: true,
            data_views: vec![view],
            thread_exposure: ThreadExposureSet::default(),
            workspace: AgentRunWorkspaceManifest {
                workspace_id: request.run_id.into(),
                read_only_inputs: "inputs".into(),
                working_area: "working".into(),
                staged_outputs: "outputs".into(),
            },
            access_ceiling: PermissionCeiling {
                exact_inputs: request.task.exact_inputs.clone(),
                data_scopes: vec![TENDER_METADATA_SCOPE.into()],
                data_classifications: vec![DataClassification::TenderInternal],
                allowed_actions: vec![PROPOSE_INTAKE_ACTION.into()],
                allowed_tools: ceiling_allowed_tools,
            },
            resource_budget: request.task.resource_budget.clone(),
            issued_at: request.issued_at.into(),
            expires_at: request.task.deadline.clone(),
        };
        grant.thread_exposure = ThreadExposureSet::from_grant(&grant);
        Ok(grant)
    })();
    match materialized {
        Ok(grant) => Ok((grant, workspace)),
        Err(error) => {
            let _ = fs::remove_dir_all(&workspace);
            Err(error)
        }
    }
}

pub(crate) fn derive_pre_bid_data_grant(
    request: PreBidDataGrantRequest<'_>,
) -> Result<(PermissionGrant, PathBuf), TenderCommandError> {
    let expected_permissions = AgentRunPermissions {
        data_scopes: vec![request.data_scope.into()],
        data_classifications: vec![DataClassification::TenderInternal],
        allowed_actions: vec![request.allowed_action.into()],
        allowed_tools: Vec::new(),
        network_allowed: false,
        workspace_write_allowed: true,
    };
    if request.profile.permissions != expected_permissions
        || request.task.permissions != expected_permissions
        || request.profile.profile_id != request.task.profile_id
        || request.profile.version != request.task.profile_version
        || request.relative_path.contains(['/', '\\'])
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }

    let workspace = request
        .application_home
        .join("staging")
        .join(format!("agent-{}-{}", request.tender_id, request.run_id));
    let inputs = workspace.join("inputs");
    let working = workspace.join("working");
    let outputs = workspace.join("outputs");
    fs::create_dir(&workspace).map_err(store_unavailable)?;
    let materialized = (|| {
        fs::create_dir(&inputs).map_err(store_unavailable)?;
        fs::create_dir(&working).map_err(store_unavailable)?;
        fs::create_dir(&outputs).map_err(store_unavailable)?;
        let payload = serde_json_canonicalizer::to_string(request.payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
            .into_bytes();
        if payload.len() > 4 * 1024 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let relative_path = format!("inputs/{}", request.relative_path);
        let path = workspace.join(&relative_path);
        fs::write(&path, &payload).map_err(store_unavailable)?;
        let mut permissions = fs::metadata(&path)
            .map_err(store_unavailable)?
            .permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).map_err(store_unavailable)?;

        let view = DataViewManifest {
            view_id: request.view_id.into(),
            schema_version: 1,
            relative_path,
            sha256: Sha256::digest(&payload)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            data_scope: request.data_scope.into(),
            data_classification: DataClassification::TenderInternal,
            exact_inputs: request.task.exact_inputs.clone(),
        };
        let mut grant = PermissionGrant {
            grant_id: request.grant_id,
            policy_version: PERMISSION_POLICY_VERSION,
            capability_catalogue_version: CAPABILITY_CATALOGUE_VERSION,
            work_plan_version: BOOTSTRAP_WORK_PLAN_VERSION,
            profile_id: request.profile.profile_id.clone(),
            profile_version: request.profile.version,
            task_id: request.task.task_id.clone(),
            purpose: request.task.objective.clone(),
            data_scopes: expected_permissions.data_scopes.clone(),
            data_classifications: expected_permissions.data_classifications.clone(),
            allowed_actions: expected_permissions.allowed_actions.clone(),
            typed_tools: Vec::new(),
            network_allowed: false,
            workspace_write_allowed: true,
            data_views: vec![view],
            thread_exposure: ThreadExposureSet::default(),
            workspace: AgentRunWorkspaceManifest {
                workspace_id: request.run_id.into(),
                read_only_inputs: "inputs".into(),
                working_area: "working".into(),
                staged_outputs: "outputs".into(),
            },
            access_ceiling: PermissionCeiling {
                exact_inputs: request.task.exact_inputs.clone(),
                data_scopes: expected_permissions.data_scopes,
                data_classifications: expected_permissions.data_classifications,
                allowed_actions: expected_permissions.allowed_actions,
                allowed_tools: Vec::new(),
            },
            resource_budget: request.task.resource_budget.clone(),
            issued_at: request.issued_at.into(),
            expires_at: request.task.deadline.clone(),
        };
        grant.thread_exposure = ThreadExposureSet::from_grant(&grant);
        Ok(grant)
    })();
    match materialized {
        Ok(grant) => Ok((grant, workspace)),
        Err(error) => {
            let _ = fs::remove_dir_all(&workspace);
            Err(error)
        }
    }
}

pub(crate) fn derive_planned_task_grant(
    request: PlannedTaskGrantRequest<'_>,
) -> Result<(PermissionGrant, PathBuf), TenderCommandError> {
    let permissions = &request.task.permissions;
    if request.work_plan_version == 0
        || request.profile.profile_id != request.task.profile_id
        || request.profile.version != request.task.profile_version
        || request.profile.permissions != *permissions
        || request.profile.resource_budget != request.task.resource_budget
        || request.task.resource_budget.provider_turns != 1
        || request.task.resource_budget.duration_seconds == 0
        || request.task.resource_budget.duration_seconds > 120
        || request.task.resource_budget.output_bytes == 0
        || request.task.resource_budget.output_bytes > 256 * 1024
        || permissions.data_scopes.is_empty()
        || permissions.data_classifications.is_empty()
        || permissions.allowed_actions.is_empty()
        || !permissions.allowed_tools.is_empty()
        || permissions.network_allowed
        || !permissions.workspace_write_allowed
        || permissions
            .data_classifications
            .contains(&DataClassification::Secret)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let workspace = request
        .application_home
        .join("staging")
        .join(format!("agent-{}-{}", request.tender_id, request.run_id));
    let inputs = workspace.join("inputs");
    let working = workspace.join("working");
    let outputs = workspace.join("outputs");
    fs::create_dir(&workspace).map_err(store_unavailable)?;
    let materialized = (|| {
        fs::create_dir(&inputs).map_err(store_unavailable)?;
        fs::create_dir(&working).map_err(store_unavailable)?;
        fs::create_dir(&outputs).map_err(store_unavailable)?;
        let payload = serde_json_canonicalizer::to_string(request.payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
            .into_bytes();
        if payload.len() > 4 * 1024 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let relative_path = "inputs/tender-metadata-v1.json";
        let path = workspace.join(relative_path);
        fs::write(&path, &payload).map_err(store_unavailable)?;
        let mut file_permissions = fs::metadata(&path)
            .map_err(store_unavailable)?
            .permissions();
        file_permissions.set_readonly(true);
        fs::set_permissions(&path, file_permissions).map_err(store_unavailable)?;
        let view = DataViewManifest {
            view_id: format!("production-task-{}", request.task.task_id),
            schema_version: 1,
            relative_path: relative_path.into(),
            sha256: Sha256::digest(&payload)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            data_scope: permissions.data_scopes.join("+"),
            data_classification: *permissions
                .data_classifications
                .iter()
                .max()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
            exact_inputs: request.task.exact_inputs.clone(),
        };
        let mut grant = PermissionGrant {
            grant_id: request.grant_id,
            policy_version: PERMISSION_POLICY_VERSION,
            capability_catalogue_version: CAPABILITY_CATALOGUE_VERSION,
            work_plan_version: request.work_plan_version,
            profile_id: request.profile.profile_id.clone(),
            profile_version: request.profile.version,
            task_id: request.task.task_id.clone(),
            purpose: request.task.objective.clone(),
            data_scopes: permissions.data_scopes.clone(),
            data_classifications: permissions.data_classifications.clone(),
            allowed_actions: permissions.allowed_actions.clone(),
            typed_tools: Vec::new(),
            network_allowed: false,
            workspace_write_allowed: true,
            data_views: vec![view],
            thread_exposure: ThreadExposureSet::default(),
            workspace: AgentRunWorkspaceManifest {
                workspace_id: request.run_id.into(),
                read_only_inputs: "inputs".into(),
                working_area: "working".into(),
                staged_outputs: "outputs".into(),
            },
            access_ceiling: PermissionCeiling {
                exact_inputs: request.task.exact_inputs.clone(),
                data_scopes: permissions.data_scopes.clone(),
                data_classifications: permissions.data_classifications.clone(),
                allowed_actions: permissions.allowed_actions.clone(),
                allowed_tools: Vec::new(),
            },
            resource_budget: request.task.resource_budget.clone(),
            issued_at: request.issued_at.into(),
            expires_at: request.expires_at.into(),
        };
        grant.thread_exposure = ThreadExposureSet::from_grant(&grant);
        Ok(grant)
    })();
    match materialized {
        Ok(grant) => Ok((grant, workspace)),
        Err(error) => {
            let _ = fs::remove_dir_all(&workspace);
            Err(error)
        }
    }
}

fn intersect_strings(
    sources: &[&AgentRunPermissions],
    select: impl Fn(&AgentRunPermissions) -> &Vec<String>,
) -> Vec<String> {
    let mut intersection = select(sources[0]).clone();
    intersection.retain(|item| {
        sources[1..]
            .iter()
            .all(|source| select(source).contains(item))
    });
    intersection.sort();
    intersection.dedup();
    intersection
}

fn intersect_classifications(sources: &[&AgentRunPermissions]) -> Vec<DataClassification> {
    let mut intersection = sources[0].data_classifications.clone();
    intersection.retain(|classification| {
        sources[1..]
            .iter()
            .all(|source| source.data_classifications.contains(classification))
    });
    intersection.sort();
    intersection.dedup();
    intersection
}

fn bootstrap_capability_ceiling() -> AgentRunPermissions {
    bootstrap_ceiling_permissions()
}

fn bootstrap_work_plan_ceiling() -> AgentRunPermissions {
    bootstrap_ceiling_permissions()
}

fn permission_policy() -> AgentRunPermissions {
    bootstrap_ceiling_permissions()
}

fn bootstrap_ceiling_permissions() -> AgentRunPermissions {
    AgentRunPermissions {
        data_scopes: vec![TENDER_METADATA_SCOPE.into()],
        data_classifications: vec![DataClassification::TenderInternal],
        allowed_actions: vec![PROPOSE_INTAKE_ACTION.into()],
        allowed_tools: vec![TENDER_METADATA_TOOL_NAME.into()],
        network_allowed: false,
        workspace_write_allowed: true,
    }
}

pub(crate) fn bootstrap_tool_catalogue() -> Vec<TypedToolDefinition> {
    vec![TypedToolDefinition {
        name: TENDER_METADATA_TOOL_NAME.into(),
        version: 1,
        input_schema_json: serde_json_canonicalizer::to_string(&json!({
            "additionalProperties": false,
            "type": "object"
        }))
        .expect("static Typed Tool input schema is canonical"),
        output_schema_json: serde_json_canonicalizer::to_string(&json!({
            "additionalProperties": true,
            "type": "object"
        }))
        .expect("static Typed Tool output schema is canonical"),
        required_capability: "analyze_tender_intake_readiness".into(),
        required_action: PROPOSE_INTAKE_ACTION.into(),
        required_data_scopes: vec![TENDER_METADATA_SCOPE.into()],
        allowed_data_classifications: vec![DataClassification::TenderInternal],
        side_effect_class: ToolSideEffectClass::ReadOnly,
        quota: TypedToolQuota {
            maximum_calls: 1,
            maximum_input_bytes: 2,
            maximum_output_bytes: 16 * 1024,
        },
        safety_limits: vec!["Return only the exact grant-bound Tender metadata Data View".into()],
        idempotency: ToolIdempotency::Idempotent,
        audit_event_type: "agent_typed_tool_executed".into(),
    }]
}

fn store_unavailable(_error: std::io::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

pub(crate) fn deny_provider_control_request(
    grant: &PermissionGrant,
    method: &str,
    params: &serde_json::Value,
    expired: bool,
) -> PermissionDenialReason {
    if expired {
        return PermissionDenialReason::GrantExpired;
    }
    match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            PermissionDenialReason::ProhibitedAction
        }
        "item/permissions/requestApproval" => {
            if params.get("permissions").is_some_and(|permissions| {
                permissions != &serde_json::Value::Object(Default::default())
            }) {
                PermissionDenialReason::ProhibitedAction
            } else {
                PermissionDenialReason::DefaultDeny
            }
        }
        "item/tool/call" => {
            let tool = params.get("tool").and_then(serde_json::Value::as_str);
            if tool.is_some_and(|tool| grant.typed_tools.iter().any(|allowed| allowed.name == tool))
            {
                PermissionDenialReason::DefaultDeny
            } else {
                PermissionDenialReason::ToolNotGranted
            }
        }
        _ => PermissionDenialReason::DefaultDeny,
    }
}

pub(crate) fn permission_duration(
    grant: &PermissionGrant,
    now: Timestamp,
) -> Result<Duration, PermissionDenialReason> {
    let issued_at = parse_utc_timestamp(&grant.issued_at)?;
    let expires_at = parse_utc_timestamp(&grant.expires_at)?;
    let remaining_until_expiry = Duration::try_from(now.duration_until(expires_at))
        .map_err(|_| PermissionDenialReason::GrantExpired)?;
    let elapsed = Duration::try_from(issued_at.duration_until(now))
        .map_err(|_| PermissionDenialReason::GrantExpired)?;
    let remaining_budget = Duration::from_secs(grant.resource_budget.duration_seconds.into())
        .checked_sub(elapsed)
        .ok_or(PermissionDenialReason::GrantExpired)?;
    Ok(remaining_until_expiry.min(remaining_budget))
}

fn parse_utc_timestamp(value: &str) -> Result<Timestamp, PermissionDenialReason> {
    if !value.contains('T') || !value.ends_with('Z') {
        return Err(PermissionDenialReason::DefaultDeny);
    }
    value
        .parse()
        .map_err(|_| PermissionDenialReason::DefaultDeny)
}

fn action_is_permitted_by_policy(action: &str) -> bool {
    action == PROPOSE_INTAKE_ACTION || action.starts_with("produce_")
}

pub fn approve_one_run_access(
    grant: &PermissionGrant,
    request: &AccessRequest,
    approval: &AccessApproval,
    now: &str,
) -> Result<OneRunAccessGrant, PermissionDenialReason> {
    if request
        .allowed_actions
        .iter()
        .chain(grant.access_ceiling.allowed_actions.iter())
        .any(|action| !action_is_permitted_by_policy(action))
    {
        return Err(PermissionDenialReason::ProhibitedAction);
    }
    if request
        .data_classifications
        .contains(&DataClassification::Secret)
    {
        return Err(PermissionDenialReason::SecretData);
    }
    if request.recurring {
        return Err(PermissionDenialReason::WorkPlanAmendmentRequired);
    }
    if request.run_id != grant.workspace.workspace_id
        || approval.run_id != request.run_id
        || approval.request_id != request.request_id
    {
        return Err(PermissionDenialReason::DefaultDeny);
    }
    let now = parse_utc_timestamp(now)?;
    let approval_expires_at = parse_utc_timestamp(&approval.expires_at)?;
    let grant_expires_at = parse_utc_timestamp(&grant.expires_at)?;
    if now >= grant_expires_at
        || approval_expires_at <= now
        || approval_expires_at > grant_expires_at
    {
        return Err(PermissionDenialReason::GrantExpired);
    }
    let within_ceiling = request
        .exact_inputs
        .iter()
        .all(|item| grant.access_ceiling.exact_inputs.contains(item))
        && request
            .data_scopes
            .iter()
            .all(|item| grant.access_ceiling.data_scopes.contains(item))
        && request.data_classifications.iter().all(|item| {
            grant.access_ceiling.data_classifications.contains(item)
                && *item != DataClassification::Secret
        })
        && request
            .allowed_actions
            .iter()
            .all(|item| grant.access_ceiling.allowed_actions.contains(item))
        && request
            .allowed_tools
            .iter()
            .all(|item| grant.access_ceiling.allowed_tools.contains(item));
    if !within_ceiling {
        return Err(PermissionDenialReason::OutsideCeiling);
    }
    if request.allowed_tools.iter().any(|tool_name| {
        bootstrap_tool_catalogue()
            .into_iter()
            .find(|definition| definition.name == *tool_name)
            .is_none_or(|definition| !request_covers_tool_authority(grant, request, &definition))
    }) {
        return Err(PermissionDenialReason::OutsideCeiling);
    }
    Ok(OneRunAccessGrant {
        approval_id: approval.approval_id.clone(),
        request_id: request.request_id.clone(),
        run_id: request.run_id.clone(),
        exact_inputs: request.exact_inputs.clone(),
        data_scopes: request.data_scopes.clone(),
        data_classifications: request.data_classifications.clone(),
        allowed_actions: request.allowed_actions.clone(),
        allowed_tools: request.allowed_tools.clone(),
        purpose: request.purpose.clone(),
        approved_by: approval.approved_by.clone(),
        expires_at: approval.expires_at.clone(),
    })
}

pub(crate) fn one_run_grant_authorizes_tool(
    grant: &PermissionGrant,
    profile_capabilities: &[String],
    supplement: &OneRunAccessGrant,
    definition: &TypedToolDefinition,
) -> bool {
    grant.policy_version == PERMISSION_POLICY_VERSION
        && grant.capability_catalogue_version == CAPABILITY_CATALOGUE_VERSION
        && grant.work_plan_version == BOOTSTRAP_WORK_PLAN_VERSION
        && supplement.run_id == grant.workspace.workspace_id
        && supplement.allowed_tools.contains(&definition.name)
        && profile_capabilities.contains(&definition.required_capability)
        && grant
            .access_ceiling
            .allowed_tools
            .contains(&definition.name)
        && request_authority_covers_tool(
            &supplement.exact_inputs,
            &supplement.data_scopes,
            &supplement.data_classifications,
            &supplement.allowed_actions,
            grant,
            definition,
        )
}

pub(crate) fn authorize_tool_call<'a>(
    grant: &PermissionGrant,
    profile_capabilities: &[String],
    approved_requests: &'a [(String, OneRunAccessGrant)],
    now: Timestamp,
    tool_name: &str,
) -> Result<&'a OneRunAccessGrant, PermissionDenialReason> {
    let Some(definition) = bootstrap_tool_catalogue()
        .into_iter()
        .find(|tool| tool.name == tool_name)
    else {
        return Err(PermissionDenialReason::ToolNotGranted);
    };
    for (request_id, supplement) in approved_requests {
        let Ok(expires_at) = supplement.expires_at.parse::<Timestamp>() else {
            continue;
        };
        if supplement.request_id == *request_id
            && expires_at > now
            && one_run_grant_authorizes_tool(grant, profile_capabilities, supplement, &definition)
        {
            return Ok(supplement);
        }
    }
    Err(PermissionDenialReason::ToolNotGranted)
}

fn request_covers_tool_authority(
    grant: &PermissionGrant,
    request: &AccessRequest,
    definition: &TypedToolDefinition,
) -> bool {
    request_authority_covers_tool(
        &request.exact_inputs,
        &request.data_scopes,
        &request.data_classifications,
        &request.allowed_actions,
        grant,
        definition,
    )
}

fn request_authority_covers_tool(
    exact_inputs: &[AgentTaskInputReference],
    data_scopes: &[String],
    data_classifications: &[DataClassification],
    allowed_actions: &[String],
    grant: &PermissionGrant,
    definition: &TypedToolDefinition,
) -> bool {
    allowed_actions.contains(&definition.required_action)
        && grant
            .access_ceiling
            .allowed_actions
            .contains(&definition.required_action)
        && action_is_permitted_by_policy(&definition.required_action)
        && definition.required_data_scopes.iter().all(|scope| {
            data_scopes.contains(scope) && grant.access_ceiling.data_scopes.contains(scope)
        })
        && grant
            .data_views
            .iter()
            .any(|view| definition.required_data_scopes.contains(&view.data_scope))
        && grant
            .data_views
            .iter()
            .filter(|view| definition.required_data_scopes.contains(&view.data_scope))
            .all(|view| {
                data_classifications.contains(&view.data_classification)
                    && grant
                        .access_ceiling
                        .data_classifications
                        .contains(&view.data_classification)
                    && definition
                        .allowed_data_classifications
                        .contains(&view.data_classification)
                    && view.exact_inputs.iter().all(|input| {
                        exact_inputs.contains(input)
                            && grant.access_ceiling.exact_inputs.contains(input)
                    })
            })
        && !data_classifications.contains(&DataClassification::Secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access_fixture(
        classification: DataClassification,
        recurring: bool,
    ) -> (PermissionGrant, AccessRequest, AccessApproval) {
        let exact_input = AgentTaskInputReference {
            kind: "tender_revision".into(),
            reference: "11111111111111111111111111111111".into(),
            version: 1,
        };
        let grant = PermissionGrant {
            grant_id: "22222222222222222222222222222222".into(),
            policy_version: 1,
            capability_catalogue_version: 1,
            work_plan_version: 1,
            profile_id: "33333333333333333333333333333333".into(),
            profile_version: 1,
            task_id: "44444444444444444444444444444444".into(),
            purpose: "Assess the Tender".into(),
            data_scopes: vec!["tender_metadata".into()],
            data_classifications: vec![DataClassification::TenderInternal],
            allowed_actions: vec!["propose_intake_readiness".into()],
            typed_tools: Vec::new(),
            network_allowed: false,
            workspace_write_allowed: true,
            data_views: Vec::new(),
            thread_exposure: ThreadExposureSet::default(),
            workspace: AgentRunWorkspaceManifest {
                workspace_id: "55555555555555555555555555555555".into(),
                read_only_inputs: "inputs".into(),
                working_area: "working".into(),
                staged_outputs: "outputs".into(),
            },
            access_ceiling: PermissionCeiling {
                exact_inputs: vec![exact_input.clone()],
                data_scopes: vec!["tender_metadata".into()],
                data_classifications: vec![
                    DataClassification::TenderInternal,
                    DataClassification::Sensitive,
                    DataClassification::Secret,
                ],
                allowed_actions: vec!["propose_intake_readiness".into()],
                allowed_tools: Vec::new(),
            },
            resource_budget: AgentResourceBudget {
                provider_turns: 1,
                duration_seconds: 3600,
                output_bytes: 1024,
            },
            issued_at: "2026-08-08T05:00:00Z".into(),
            expires_at: "2026-08-08T07:00:00Z".into(),
        };
        let request = AccessRequest {
            request_id: "66666666666666666666666666666666".into(),
            run_id: grant.workspace.workspace_id.clone(),
            exact_inputs: vec![exact_input],
            data_scopes: vec!["tender_metadata".into()],
            data_classifications: vec![classification],
            allowed_actions: vec!["propose_intake_readiness".into()],
            allowed_tools: Vec::new(),
            purpose: "Inspect one exact additional input".into(),
            recurring,
        };
        let approval = AccessApproval {
            approval_id: "77777777777777777777777777777777".into(),
            request_id: request.request_id.clone(),
            run_id: request.run_id.clone(),
            approved_by: "engineer-user".into(),
            expires_at: "2026-08-08T06:30:00Z".into(),
        };
        (grant, request, approval)
    }

    #[test]
    fn secret_data_is_never_eligible_for_one_run_access_approval() {
        let (grant, request, approval) = access_fixture(DataClassification::Secret, false);

        let decision = approve_one_run_access(&grant, &request, &approval, "2026-08-08T06:00:00Z");

        assert_eq!(decision, Err(PermissionDenialReason::SecretData));
    }

    #[test]
    fn recurring_access_requires_a_work_plan_amendment() {
        let (grant, request, approval) = access_fixture(DataClassification::Sensitive, true);

        let decision = approve_one_run_access(&grant, &request, &approval, "2026-08-08T06:00:00Z");

        assert_eq!(
            decision,
            Err(PermissionDenialReason::WorkPlanAmendmentRequired)
        );
    }

    #[test]
    fn one_run_access_approval_expires_with_the_run() {
        let (grant, request, mut approval) = access_fixture(DataClassification::Sensitive, false);
        approval.expires_at = "2026-08-08T08:00:00Z".into();

        let decision = approve_one_run_access(&grant, &request, &approval, "2026-08-08T06:00:00Z");

        assert_eq!(decision, Err(PermissionDenialReason::GrantExpired));
    }

    #[test]
    fn access_outside_the_existing_ceiling_requires_a_work_plan_change() {
        let (grant, mut request, approval) = access_fixture(DataClassification::Sensitive, false);
        request.data_scopes = vec!["commercial_markup".into()];

        let decision = approve_one_run_access(&grant, &request, &approval, "2026-08-08T06:00:00Z");

        assert_eq!(decision, Err(PermissionDenialReason::OutsideCeiling));
    }

    #[test]
    fn noncanonical_approval_expiry_is_rejected() {
        let (grant, request, mut approval) = access_fixture(DataClassification::Sensitive, false);
        approval.expires_at = "2026-08-08T08:30:00+02:00".into();

        let decision = approve_one_run_access(&grant, &request, &approval, "2026-08-08T06:00:00Z");

        assert_eq!(decision, Err(PermissionDenialReason::DefaultDeny));
    }

    #[test]
    fn prohibited_actions_cannot_be_approved_even_if_a_ceiling_is_malformed() {
        let (mut grant, mut request, approval) =
            access_fixture(DataClassification::Sensitive, false);
        grant.access_ceiling.allowed_actions = vec!["unrestricted_shell".into()];
        request.allowed_actions = vec!["unrestricted_shell".into()];

        let decision = approve_one_run_access(&grant, &request, &approval, "2026-08-08T06:00:00Z");

        assert_eq!(decision, Err(PermissionDenialReason::ProhibitedAction));
    }

    #[test]
    fn tool_only_access_request_cannot_unlock_a_data_view() {
        let (mut grant, mut request, approval) =
            access_fixture(DataClassification::TenderInternal, false);
        grant.access_ceiling.allowed_tools = vec![TENDER_METADATA_TOOL_NAME.into()];
        request.exact_inputs.clear();
        request.data_scopes.clear();
        request.data_classifications.clear();
        request.allowed_actions.clear();
        request.allowed_tools = vec![TENDER_METADATA_TOOL_NAME.into()];

        let decision = approve_one_run_access(&grant, &request, &approval, "2026-08-08T06:00:00Z");

        assert_eq!(decision, Err(PermissionDenialReason::OutsideCeiling));
    }

    #[test]
    fn permission_duration_subtracts_time_spent_before_the_provider_turn() {
        let (grant, _, _) = access_fixture(DataClassification::TenderInternal, false);

        let remaining = permission_duration(
            &grant,
            "2026-08-08T05:59:30Z".parse().expect("current timestamp"),
        )
        .expect("remaining Permission Grant duration");

        assert_eq!(remaining, Duration::from_secs(30));
    }

    fn tool_authorization_fixture() -> (PermissionGrant, OneRunAccessGrant) {
        let (mut grant, request, approval) =
            access_fixture(DataClassification::TenderInternal, false);
        let exact_input = AgentTaskInputReference {
            kind: "tender_revision".into(),
            reference: "11111111111111111111111111111111".into(),
            version: 1,
        };
        grant.access_ceiling.allowed_tools = vec![TENDER_METADATA_TOOL_NAME.into()];
        grant.data_views = vec![DataViewManifest {
            view_id: "tender-metadata-v1".into(),
            schema_version: 1,
            relative_path: "inputs/tender-metadata-v1.json".into(),
            sha256: "0".repeat(64),
            data_scope: TENDER_METADATA_SCOPE.into(),
            data_classification: DataClassification::TenderInternal,
            exact_inputs: vec![exact_input.clone()],
        }];
        let supplement = OneRunAccessGrant {
            approval_id: approval.approval_id,
            request_id: request.request_id,
            run_id: request.run_id,
            exact_inputs: vec![exact_input],
            data_scopes: vec![TENDER_METADATA_SCOPE.into()],
            data_classifications: vec![DataClassification::TenderInternal],
            allowed_actions: vec!["propose_intake_readiness".into()],
            allowed_tools: vec![TENDER_METADATA_TOOL_NAME.into()],
            purpose: request.purpose,
            approved_by: approval.approved_by,
            expires_at: approval.expires_at,
        };
        (grant, supplement)
    }

    #[test]
    fn authorize_tool_call_matches_an_active_approved_request() {
        let (grant, supplement) = tool_authorization_fixture();
        let approved = vec![("66666666666666666666666666666666".into(), supplement)];

        let authorized = authorize_tool_call(
            &grant,
            &["analyze_tender_intake_readiness".into()],
            &approved,
            "2026-08-08T06:00:00Z".parse().unwrap(),
            TENDER_METADATA_TOOL_NAME,
        )
        .expect("tool call must be authorized");

        assert_eq!(authorized.approval_id, "77777777777777777777777777777777");
    }

    #[test]
    fn authorize_tool_call_rejects_unknown_tools_and_stale_supplements() {
        let (grant, supplement) = tool_authorization_fixture();

        let unknown = authorize_tool_call(
            &grant,
            &["analyze_tender_intake_readiness".into()],
            &[],
            "2026-08-08T06:00:00Z".parse().unwrap(),
            "undeclared_shell",
        );
        assert_eq!(unknown, Err(PermissionDenialReason::ToolNotGranted));

        let expired_requests = [("66666666666666666666666666666666".to_owned(), supplement)];
        let expired = authorize_tool_call(
            &grant,
            &["analyze_tender_intake_readiness".into()],
            &expired_requests,
            "2026-08-08T07:00:00Z".parse().unwrap(),
            TENDER_METADATA_TOOL_NAME,
        );
        assert_eq!(expired, Err(PermissionDenialReason::ToolNotGranted));
    }
}
