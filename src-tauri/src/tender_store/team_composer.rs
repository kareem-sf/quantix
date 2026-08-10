use garde::Validate;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::{
    agent_runtime::{
        AgentProfileStatus, AgentProfileVersionView, AgentResourceBudget, AgentRunPermissions,
        AgentTaskInputReference, BootstrapRole, DataClassification,
    },
    QuantixHost,
};

use super::{
    agent_records::{insert_profile_version, load_profile, update_profile_head},
    append_audit_event,
    bid_decisions::{
        package_dependencies_are_current, BidDecisionApprovalDecision,
        BidDecisionPackageInspection, CapabilityDemand, TenderRecordVersionReference,
    },
    lock_mutex_with_check, random_identifier, require_setup, sha256_hex, sql_error,
    sqlite_timestamp,
    tender_records::TenderRecordKind,
    BidPackageOperationBudget, TenderCommandError, TenderErrorCode, TenderId, TenderLifecyclePhase,
    TenderStore,
};

const CAPABILITY_CATALOGUE_VERSION: u32 = 1;
const PERMISSION_POLICY_VERSION: u32 = 1;
const MAX_PLAN_PROFILES: usize = 64;
const MAX_PLAN_WORKSTREAMS: usize = 64;
const MAX_PLAN_TASKS: usize = 256;
const MAX_PLAN_QUERIES: usize = 256;
const MAX_PLAN_GAPS: usize = 64;
const MAX_TASKS_PER_PROFILE: usize = 5;
const MAX_PLAN_VERSIONS: usize = 256;
const MAX_PLAN_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_STORED_AGENT_PROFILES: usize = MAX_PLAN_VERSIONS * MAX_PLAN_PROFILES + 64;

type StoredWorkPlanVersionRow = (
    String,
    u32,
    String,
    u32,
    u32,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

type StoredWorkPlanInspectionRow = (
    String,
    u32,
    String,
    u32,
    u32,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
);

struct WorkPlanIntegrityVersionState {
    profiles: Vec<WorkPlanProfileBinding>,
    package_id: String,
    package_version: u32,
    package_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ComposeTenderOfficeCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export)]
pub enum WorkPlanRevisionAction {
    RebasePackageBasis,
    AddProfile {
        archetype: String,
        identity: String,
    },
    RemoveProfile {
        profile_id: String,
    },
    SplitProfile {
        profile_id: String,
        identities: Vec<String>,
    },
    CombineProfiles {
        profile_ids: Vec<String>,
        identity: String,
    },
    RenameProfile {
        profile_id: String,
        identity: String,
    },
    AdjustProfile {
        profile_id: String,
        objective: String,
        behavior: String,
        skepticism: String,
        risk_tolerance: String,
        resource_budget: AgentResourceBudget,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ReviseWorkPlanProposalCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub plan_id: String,
    #[garde(range(min = 1))]
    pub base_version: u32,
    #[garde(length(max = 32))]
    pub actions: Vec<WorkPlanRevisionAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DecideWorkPlanProposalCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub plan_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
    #[garde(skip)]
    pub decision: WorkPlanDecision,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkPlanProfileBinding {
    pub archetype: String,
    pub status: AgentProfileStatus,
    pub profile: AgentProfileVersionView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkPlanWorkstream {
    pub workstream_key: String,
    pub name: String,
    pub capability: String,
    pub accountable_profile_id: Option<String>,
    pub dependencies: Vec<String>,
    pub deadlines: Vec<String>,
    pub milestones: Vec<String>,
    pub resource_budget: AgentResourceBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum MajorFindingPolicy {
    RemediationRequired,
    EngineerExceptionAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkPlanTask {
    pub task_key: String,
    pub workstream_key: String,
    pub profile_id: String,
    pub profile_version: u32,
    pub objective: String,
    pub exact_inputs: Vec<AgentTaskInputReference>,
    pub dependencies: Vec<String>,
    pub deadline: String,
    pub milestone: String,
    pub review_profile_id: Option<String>,
    pub review_profile_version: Option<u32>,
    pub major_finding_policy: MajorFindingPolicy,
    pub permissions: AgentRunPermissions,
    pub resource_budget: AgentResourceBudget,
    pub output_contract_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkPlanCapabilityGap {
    pub capability: String,
    pub reason: String,
    pub affected_work: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WorkPlanDecision {
    Approve,
    Return,
    Reject,
}

impl WorkPlanDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Return => "return",
            Self::Reject => "reject",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "approve" => Ok(Self::Approve),
            "return" => Ok(Self::Return),
            "reject" => Ok(Self::Reject),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkPlanApprovalRecord {
    pub approval_id: String,
    pub plan_id: String,
    pub plan_version: u32,
    pub decision: WorkPlanDecision,
    pub rationale: String,
    pub plan_manifest_sha256: String,
    pub decided_by: String,
    pub acting_role: String,
    pub approval_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkPlanProposalInspection {
    pub plan_id: String,
    pub version: u32,
    pub bid_package_id: String,
    pub bid_package_version: u32,
    pub bid_package_manifest_sha256: String,
    pub capability_catalogue_version: u32,
    pub permission_policy_version: u32,
    pub profiles: Vec<WorkPlanProfileBinding>,
    pub workstreams: Vec<WorkPlanWorkstream>,
    pub tasks: Vec<WorkPlanTask>,
    pub query_bindings: Vec<TenderRecordVersionReference>,
    pub capability_gaps: Vec<WorkPlanCapabilityGap>,
    pub blocker_codes: Vec<String>,
    pub approval: Option<WorkPlanApprovalRecord>,
    pub current: bool,
    pub created_by: String,
    pub created_at: String,
    pub manifest_sha256: String,
}

#[derive(Serialize)]
struct WorkPlanManifest<'a> {
    schema_version: u32,
    plan_id: &'a str,
    version: u32,
    bid_package_id: &'a str,
    bid_package_version: u32,
    bid_package_manifest_sha256: &'a str,
    capability_catalogue_version: u32,
    permission_policy_version: u32,
    profiles: &'a [WorkPlanProfileBinding],
    workstreams: &'a [WorkPlanWorkstream],
    tasks: &'a [WorkPlanTask],
    query_bindings: &'a [TenderRecordVersionReference],
    capability_gaps: &'a [WorkPlanCapabilityGap],
    blocker_codes: &'a [String],
    revision_actions: &'a [Value],
    created_by: &'a str,
    created_at: &'a str,
}

#[derive(Serialize)]
struct WorkPlanApprovalManifest<'a> {
    schema_version: u32,
    approval_id: &'a str,
    plan_id: &'a str,
    plan_version: u32,
    decision: WorkPlanDecision,
    rationale: &'a str,
    plan_manifest_sha256: &'a str,
    profiles: Vec<(&'a str, u32)>,
    decided_by: &'a str,
    acting_role: &'a str,
    created_at: &'a str,
}

impl QuantixHost {
    pub fn compose_tender_office(
        &self,
        command: ComposeTenderOfficeCommand,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        store.compose_tender_office(&tender_id, budget)
    }

    pub fn inspect_current_work_plan(
        &self,
        tender_id: &str,
    ) -> Result<Option<WorkPlanProposalInspection>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let store = lock_mutex_with_check(&store, &mut || budget.check())?;
        store.inspect_current_work_plan(budget)
    }

    pub fn revise_work_plan_proposal(
        &self,
        command: ReviseWorkPlanProposalCommand,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() || validate_revision_actions(&command.actions).is_err() {
            store.record_work_plan_denial(
                &tender_id,
                "revise_work_plan_proposal",
                Some(&command.plan_id),
                Some(command.base_version),
                "command_shape_invalid",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.revise_work_plan_proposal(&tender_id, &command, budget)
    }

    pub fn decide_work_plan_proposal(
        &self,
        command: DecideWorkPlanProposalCommand,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_work_plan_denial(
                &tender_id,
                "decide_work_plan_proposal",
                Some(&command.plan_id),
                Some(command.version),
                "command_shape_invalid",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.decide_work_plan_proposal(&tender_id, &command, budget)
    }
}

impl TenderStore {
    pub(crate) fn compose_tender_office(
        &mut self,
        tender_id: &TenderId,
        budget: BidPackageOperationBudget,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        if self.inspect_current_work_plan(budget)?.is_some() {
            self.record_work_plan_denial(
                tender_id,
                "compose_tender_office",
                None,
                None,
                "work_plan_already_exists",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let package = match accepted_current_package(self, budget) {
            Ok(package) => package,
            Err(error) if error.code == TenderErrorCode::InvalidCommand => {
                self.record_work_plan_denial(
                    tender_id,
                    "compose_tender_office",
                    None,
                    None,
                    "accepted_package_missing_or_stale",
                )?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let deadlines = work_plan_deadlines(self, &package, budget)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        if ensure_exact_proceed_still_current(&transaction, &package).is_err() {
            append_work_plan_denial(
                &transaction,
                tender_id,
                package.tender_revision,
                "compose_tender_office",
                None,
                None,
                "accepted_package_became_stale",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let plan_id = random_identifier(&transaction)?;
        let mut profiles = compose_profiles(&transaction, &package, &created_at)?;
        budget.check()?;
        profiles.sort_by(|left, right| left.archetype.cmp(&right.archetype));
        let capability_gaps = capability_gaps(&package.capability_demands, &profiles);
        let blocker_codes = if capability_gaps.is_empty() {
            Vec::new()
        } else {
            vec!["capability_gap".into()]
        };
        let workstreams = compose_workstreams(&profiles, &deadlines)?;
        let tasks = compose_tasks(
            tender_id,
            package.tender_revision,
            &package,
            &profiles,
            &workstreams,
            &deadlines,
        )?;
        let query_bindings = load_package_query_bindings(
            &transaction,
            &package.package_id,
            package.version,
            budget,
        )?;
        budget.check()?;
        validate_plan_shape(
            &profiles,
            &workstreams,
            &tasks,
            &query_bindings,
            &capability_gaps,
        )?;
        let revision_actions = Vec::new();
        let manifest = WorkPlanManifest {
            schema_version: 1,
            plan_id: &plan_id,
            version: 1,
            bid_package_id: &package.package_id,
            bid_package_version: package.version,
            bid_package_manifest_sha256: &package.manifest_sha256,
            capability_catalogue_version: CAPABILITY_CATALOGUE_VERSION,
            permission_policy_version: PERMISSION_POLICY_VERSION,
            profiles: &profiles,
            workstreams: &workstreams,
            tasks: &tasks,
            query_bindings: &query_bindings,
            capability_gaps: &capability_gaps,
            blocker_codes: &blocker_codes,
            revision_actions: &revision_actions,
            created_by: "engineer_user",
            created_at: &created_at,
        };
        let manifest_json = canonical_json(&manifest)?;
        if manifest_json.len() > MAX_PLAN_MANIFEST_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        transaction
            .execute(
                "INSERT INTO work_plans (plan_id, created_at) VALUES (?1, ?2)",
                params![plan_id, created_at],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO work_plan_versions (
                   plan_id, version, bid_package_id, bid_package_version,
                   bid_package_manifest_sha256, capability_catalogue_version,
                   permission_policy_version, profiles_json, workstreams_json, tasks_json,
                   query_bindings_json, capability_gaps_json, blocker_codes_json, revision_actions_json,
                   manifest_json, manifest_sha256, created_by, created_at
                 ) VALUES (
                   ?1, 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                   ?14, ?15, 'engineer_user', ?16
                 )",
                params![
                    plan_id,
                    package.package_id,
                    package.version,
                    package.manifest_sha256,
                    CAPABILITY_CATALOGUE_VERSION,
                    PERMISSION_POLICY_VERSION,
                    canonical_json(&profiles)?,
                    canonical_json(&workstreams)?,
                    canonical_json(&tasks)?,
                    canonical_json(&query_bindings)?,
                    canonical_json(&capability_gaps)?,
                    canonical_json(&blocker_codes)?,
                    canonical_json(&revision_actions)?,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO work_plan_heads (plan_id, current_version) VALUES (?1, 1)",
                [&plan_id],
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "work_plan_proposed",
            package.tender_revision,
            json!({
                "bid_package_id": package.package_id,
                "bid_package_version": package.version.to_string(),
                "capability_gap_count": capability_gaps.len().to_string(),
                "manifest_sha256": manifest_sha256,
                "plan_id": plan_id,
                "plan_version": "1",
            }),
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.inspect_current_work_plan(budget)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
    }

    pub(crate) fn inspect_current_work_plan(
        &self,
        budget: BidPackageOperationBudget,
    ) -> Result<Option<WorkPlanProposalInspection>, TenderCommandError> {
        budget.check()?;
        let head = self
            .connection
            .query_row(
                "SELECT plan_id, current_version FROM work_plan_heads ORDER BY rowid LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        head.map(|(plan_id, version)| self.inspect_work_plan_version(&plan_id, version, budget))
            .transpose()
    }

    pub(crate) fn revise_work_plan_proposal(
        &mut self,
        tender_id: &TenderId,
        command: &ReviseWorkPlanProposalCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let rebase_package_basis = matches!(
            command.actions.as_slice(),
            [WorkPlanRevisionAction::RebasePackageBasis]
        );
        let production_amendment = !rebase_package_basis
            && self
                .connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM production_activations
                       WHERE plan_id = ?1 AND plan_version = ?2 AND status = 'active'
                     )",
                    params![command.plan_id, command.base_version],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?;
        let base = self
            .inspect_current_work_plan(budget)?
            .filter(|plan| {
                plan.plan_id == command.plan_id
                    && plan.version == command.base_version
                    && if rebase_package_basis {
                        !plan.current
                    } else {
                        plan.current
                            && (production_amendment
                                || plan.approval.as_ref().is_none_or(|approval| {
                                    approval.decision != WorkPlanDecision::Approve
                                }))
                    }
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand));
        let base = match base {
            Ok(base) => base,
            Err(error) => {
                self.record_work_plan_denial(
                    tender_id,
                    "revise_work_plan_proposal",
                    Some(&command.plan_id),
                    Some(command.base_version),
                    "proposal_not_current_or_approved",
                )?;
                return Err(error);
            }
        };
        if validate_revision_compatibility(&base.profiles, &command.actions).is_err() {
            self.record_work_plan_denial(
                tender_id,
                "revise_work_plan_proposal",
                Some(&command.plan_id),
                Some(command.base_version),
                "profile_conflict_or_target_invalid",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if command.base_version as usize >= MAX_PLAN_VERSIONS {
            self.record_work_plan_denial(
                tender_id,
                "revise_work_plan_proposal",
                Some(&command.plan_id),
                Some(command.base_version),
                "work_plan_version_limit",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let package = match accepted_current_package(self, budget) {
            Ok(package) => package,
            Err(error) if error.code == TenderErrorCode::InvalidCommand => {
                self.record_work_plan_denial(
                    tender_id,
                    "revise_work_plan_proposal",
                    Some(&command.plan_id),
                    Some(command.base_version),
                    "accepted_package_missing_or_stale",
                )?;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        if base.bid_package_id != package.package_id
            || base.bid_package_version != package.version
            || base.bid_package_manifest_sha256 != package.manifest_sha256
        {
            if !rebase_package_basis {
                self.record_work_plan_denial(
                    tender_id,
                    "revise_work_plan_proposal",
                    Some(&command.plan_id),
                    Some(command.base_version),
                    "package_binding_stale",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        } else if rebase_package_basis {
            self.record_work_plan_denial(
                tender_id,
                "revise_work_plan_proposal",
                Some(&command.plan_id),
                Some(command.base_version),
                "replacement_package_not_new",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let deadlines = if rebase_package_basis {
            work_plan_deadlines(self, &package, budget)?
        } else {
            deadlines_from_plan(&base)?
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (head_exact, prior_approved): (bool, bool) = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM work_plan_heads JOIN tender ON tender.singleton = 1
                   WHERE work_plan_heads.plan_id = ?1
                     AND work_plan_heads.current_version = ?2
                     AND tender.lifecycle_phase IN ('tender_planning', 'active_production')
                 ), EXISTS(
                   SELECT 1 FROM work_plan_approvals
                   WHERE plan_id = ?1 AND plan_version = ?2 AND decision = 'approve'
                 )",
                params![command.plan_id, command.base_version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let replacement_exact =
            ensure_exact_accepted_package_still_current(&transaction, &package).is_ok();
        if !head_exact
            || !replacement_exact
            || (!rebase_package_basis && prior_approved && !production_amendment)
            || command.base_version as usize >= MAX_PLAN_VERSIONS
        {
            append_work_plan_denial(
                &transaction,
                tender_id,
                package.tender_revision,
                "revise_work_plan_proposal",
                Some(&command.plan_id),
                Some(command.base_version),
                "proposal_became_stale",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let mut profiles = base.profiles;
        if production_amendment {
            let active_or_unresolved: Option<String> = transaction
                .query_row(
                    "SELECT CASE
                       WHEN EXISTS(
                         SELECT 1 FROM production_tasks AS tasks
                         JOIN production_activations AS activations
                           ON activations.activation_id = tasks.activation_id
                         WHERE activations.plan_id = ?1 AND activations.plan_version = ?2
                            AND activations.status = 'active'
                            AND tasks.status IN ('running', 'reviewing')
                       ) THEN 'active_production_run'
                       WHEN EXISTS(
                         SELECT 1 FROM production_tasks AS tasks
                         JOIN production_activations AS activations
                           ON activations.activation_id = tasks.activation_id
                         JOIN production_task_attempts AS attempts
                           ON attempts.production_task_id = tasks.production_task_id
                          AND attempts.task_id = tasks.task_id
                         JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                         WHERE activations.plan_id = ?1 AND activations.plan_version = ?2
                           AND activations.status = 'active' AND tasks.status = 'indeterminate'
                           AND NOT EXISTS (
                             SELECT 1 FROM agent_run_recovery_dispositions AS dispositions
                             WHERE dispositions.run_id = runs.run_id
                               AND dispositions.disposition = 'close_task'
                           )
                       ) THEN 'indeterminate_production_task'
                       ELSE NULL END",
                    params![command.plan_id, command.base_version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if let Some(reason) = active_or_unresolved {
                append_work_plan_denial(
                    &transaction,
                    tender_id,
                    package.tender_revision,
                    "revise_work_plan_proposal",
                    Some(&command.plan_id),
                    Some(command.base_version),
                    &reason,
                )?;
                transaction.commit().map_err(sql_error)?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            transaction
                .execute(
                    "UPDATE production_activations SET status = 'suspended'
                     WHERE plan_id = ?1 AND plan_version = ?2 AND status = 'active'",
                    params![command.plan_id, command.base_version],
                )
                .map_err(sql_error)?;
            transaction
                .execute(
                    "UPDATE production_tasks SET status = 'suspended', updated_at = ?3
                     WHERE activation_id IN (
                       SELECT activation_id FROM production_activations
                       WHERE plan_id = ?1 AND plan_version = ?2 AND status = 'suspended'
                      ) AND status IN (
                        'blocked', 'ready', 'review_ready', 'remediation_ready', 'query_blocked',
                        'attempt_limit_reached', 'failed', 'cancelled', 'indeterminate'
                      )",
                    params![command.plan_id, command.base_version, created_at],
                )
                .map_err(sql_error)?;
            for binding in &mut profiles {
                transaction
                    .execute(
                        "UPDATE agent_profile_heads SET status = 'proposed'
                         WHERE profile_id = ?1 AND current_version = ?2 AND status = 'active'",
                        params![binding.profile.profile_id, binding.profile.version],
                    )
                    .map_err(sql_error)?;
                binding.status = AgentProfileStatus::Proposed;
            }
            if transaction
                .execute(
                    "UPDATE tender SET lifecycle_phase = 'tender_planning'
                     WHERE singleton = 1 AND tender_id = ?1
                       AND lifecycle_phase = 'active_production'",
                    [tender_id.as_str()],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }
        if rebase_package_basis {
            for binding in &mut profiles {
                if transaction
                    .execute(
                        "UPDATE agent_profile_heads SET status = 'proposed'
                         WHERE profile_id = ?1 AND current_version = ?2
                           AND status IN ('proposed', 'suspended')",
                        params![binding.profile.profile_id, binding.profile.version],
                    )
                    .map_err(sql_error)?
                    != 1
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                binding.status = AgentProfileStatus::Proposed;
            }
        }
        apply_revision_actions(&transaction, &mut profiles, &command.actions, &created_at)?;
        budget.check()?;
        profiles.sort_by(|left, right| {
            left.archetype
                .cmp(&right.archetype)
                .then_with(|| left.profile.identity.cmp(&right.profile.identity))
        });
        let capability_gaps = capability_gaps(&package.capability_demands, &profiles);
        let blocker_codes = if capability_gaps.is_empty() {
            Vec::new()
        } else {
            vec!["capability_gap".into()]
        };
        let workstreams = compose_workstreams(&profiles, &deadlines)?;
        let tasks = compose_tasks(
            tender_id,
            package.tender_revision,
            &package,
            &profiles,
            &workstreams,
            &deadlines,
        )?;
        let query_bindings = load_package_query_bindings(
            &transaction,
            &package.package_id,
            package.version,
            budget,
        )?;
        budget.check()?;
        validate_plan_shape(
            &profiles,
            &workstreams,
            &tasks,
            &query_bindings,
            &capability_gaps,
        )?;
        let version = next_work_plan_version(command.base_version)?;
        let revision_actions = command
            .actions
            .iter()
            .map(|action| {
                serde_json::to_value(action)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let manifest = WorkPlanManifest {
            schema_version: 1,
            plan_id: &command.plan_id,
            version,
            bid_package_id: &package.package_id,
            bid_package_version: package.version,
            bid_package_manifest_sha256: &package.manifest_sha256,
            capability_catalogue_version: CAPABILITY_CATALOGUE_VERSION,
            permission_policy_version: PERMISSION_POLICY_VERSION,
            profiles: &profiles,
            workstreams: &workstreams,
            tasks: &tasks,
            query_bindings: &query_bindings,
            capability_gaps: &capability_gaps,
            blocker_codes: &blocker_codes,
            revision_actions: &revision_actions,
            created_by: "engineer_user",
            created_at: &created_at,
        };
        let manifest_json = canonical_json(&manifest)?;
        if manifest_json.len() > MAX_PLAN_MANIFEST_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        transaction
            .execute(
                "INSERT INTO work_plan_versions (
                   plan_id, version, bid_package_id, bid_package_version,
                   bid_package_manifest_sha256, capability_catalogue_version,
                   permission_policy_version, profiles_json, workstreams_json, tasks_json,
                   query_bindings_json, capability_gaps_json, blocker_codes_json, revision_actions_json,
                   manifest_json, manifest_sha256, created_by, created_at
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                   ?15, ?16, 'engineer_user', ?17
                 )",
                params![
                    command.plan_id,
                    version,
                    package.package_id,
                    package.version,
                    package.manifest_sha256,
                    CAPABILITY_CATALOGUE_VERSION,
                    PERMISSION_POLICY_VERSION,
                    canonical_json(&profiles)?,
                    canonical_json(&workstreams)?,
                    canonical_json(&tasks)?,
                    canonical_json(&query_bindings)?,
                    canonical_json(&capability_gaps)?,
                    canonical_json(&blocker_codes)?,
                    canonical_json(&revision_actions)?,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        if transaction
            .execute(
                "UPDATE work_plan_heads SET current_version = ?2
                 WHERE plan_id = ?1 AND current_version = ?3",
                params![command.plan_id, version, command.base_version],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "work_plan_revised",
            package.tender_revision,
            json!({
                "action_count": command.actions.len().to_string(),
                "capability_gap_count": capability_gaps.len().to_string(),
                "manifest_sha256": manifest_sha256,
                "plan_id": command.plan_id,
                "plan_version": version.to_string(),
                "prior_version": command.base_version.to_string(),
            }),
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.inspect_work_plan_version(&command.plan_id, version, budget)
    }

    pub(crate) fn decide_work_plan_proposal(
        &mut self,
        tender_id: &TenderId,
        command: &DecideWorkPlanProposalCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let proposal = self.inspect_work_plan_version(&command.plan_id, command.version, budget)?;
        let transaction = self.connection.unchecked_transaction().map_err(sql_error)?;
        let (package_tender_revision, record_inventory_sha256): (u32, String) = transaction
            .query_row(
                "SELECT tender_revision, record_inventory_sha256
                 FROM bid_decision_package_versions
                 WHERE package_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
                params![
                    proposal.bid_package_id,
                    proposal.bid_package_version,
                    proposal.bid_package_manifest_sha256
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let package_dependencies_current = package_dependencies_are_current(
            self,
            &proposal.bid_package_id,
            proposal.bid_package_version,
            package_tender_revision,
            &record_inventory_sha256,
        )?;
        budget.check()?;
        let (tender_revision, lifecycle): (u32, String) = transaction
            .query_row(
                "SELECT current_revision, lifecycle_phase FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let decided_at = sqlite_timestamp(&transaction)?;
        let exact: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM work_plan_heads AS plan_heads
                   JOIN work_plan_versions AS plans
                     ON plans.plan_id = plan_heads.plan_id
                    AND plans.version = plan_heads.current_version
                    AND plans.manifest_sha256 = ?3
                   JOIN bid_decision_package_heads AS package_heads
                     ON package_heads.package_id = plans.bid_package_id
                    AND package_heads.current_version = plans.bid_package_version
                   JOIN bid_decision_approval_records AS approvals
                     ON approvals.package_id = plans.bid_package_id
                    AND approvals.package_version = plans.bid_package_version
                    AND approvals.decision = 'accept'
                   WHERE plan_heads.plan_id = ?1
                     AND plan_heads.current_version = ?2
                     AND NOT EXISTS (
                       SELECT 1 FROM work_plan_approvals
                       WHERE work_plan_approvals.plan_id = ?1
                         AND work_plan_approvals.plan_version = ?2
                     )
                     AND NOT EXISTS (
                       SELECT 1 FROM bid_decision_approval_invalidations
                       WHERE bid_decision_approval_invalidations.approval_id = approvals.approval_id
                     )
                 )",
                params![command.plan_id, command.version, proposal.manifest_sha256],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exact
            || !package_dependencies_current
            || lifecycle != TenderLifecyclePhase::TenderPlanning.as_str()
        {
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "work_plan_decision_denied",
                tender_revision,
                json!({
                    "decision": command.decision.as_str(),
                    "plan_id": command.plan_id,
                    "plan_version": command.version.to_string(),
                    "reason": if !package_dependencies_current {
                        "accepted_package_dependencies_stale"
                    } else {
                        "stale_or_already_decided"
                    },
                }),
                &decided_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let active_work: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM agent_runs WHERE status = 'running'
                   UNION ALL
                   SELECT 1 FROM parse_attempts WHERE status = 'running'
                 )",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let approval_blocked = command.decision == WorkPlanDecision::Approve
            && (!proposal.blocker_codes.is_empty()
                || !proposal.capability_gaps.is_empty()
                || !approval_invariants_hold(&proposal));
        if active_work || approval_blocked {
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "work_plan_decision_denied",
                tender_revision,
                json!({
                    "decision": command.decision.as_str(),
                    "plan_id": command.plan_id,
                    "plan_version": command.version.to_string(),
                    "reason": if active_work { "active_work" } else { "plan_blocked" },
                }),
                &decided_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let approval_id = random_identifier(&transaction)?;
        let approval_manifest = WorkPlanApprovalManifest {
            schema_version: 1,
            approval_id: &approval_id,
            plan_id: &command.plan_id,
            plan_version: command.version,
            decision: command.decision,
            rationale: command.rationale.trim(),
            plan_manifest_sha256: &proposal.manifest_sha256,
            profiles: proposal
                .profiles
                .iter()
                .map(|binding| (binding.profile.profile_id.as_str(), binding.profile.version))
                .collect(),
            decided_by: "engineer_user",
            acting_role: "tendering_manager",
            created_at: &decided_at,
        };
        let approval_manifest_json = canonical_json(&approval_manifest)?;
        let approval_sha256 = sha256_hex(approval_manifest_json.as_bytes());
        transaction
            .execute(
                "INSERT INTO work_plan_approvals (
                   approval_id, plan_id, plan_version, decision, rationale,
                   plan_manifest_sha256, decided_by, acting_role,
                   approval_manifest_json, approval_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'engineer_user',
                           'tendering_manager', ?7, ?8, ?9)",
                params![
                    approval_id,
                    command.plan_id,
                    command.version,
                    command.decision.as_str(),
                    command.rationale.trim(),
                    proposal.manifest_sha256,
                    approval_manifest_json,
                    approval_sha256,
                    decided_at,
                ],
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "work_plan_decided",
            tender_revision,
            json!({
                "acting_role": "tendering_manager",
                "approval_id": approval_id,
                "approval_sha256": approval_sha256,
                "decided_by": "engineer_user",
                "decision": command.decision.as_str(),
                "plan_id": command.plan_id,
                "plan_version": command.version.to_string(),
            }),
            &decided_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.inspect_work_plan_version(&command.plan_id, command.version, budget)
    }

    fn record_work_plan_denial(
        &mut self,
        tender_id: &TenderId,
        command: &str,
        plan_id: Option<&str>,
        plan_version: Option<u32>,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let tender_revision = transaction
            .query_row(
                "SELECT current_revision FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        append_work_plan_denial(
            &transaction,
            tender_id,
            tender_revision,
            command,
            plan_id,
            plan_version,
            reason,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(super) fn inspect_work_plan_version(
        &self,
        plan_id: &str,
        version: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        budget.check()?;
        let row: StoredWorkPlanInspectionRow = self
            .connection
            .query_row(
                "SELECT bid_package_id, bid_package_version, bid_package_manifest_sha256,
                        capability_catalogue_version, permission_policy_version, profiles_json,
                        workstreams_json, tasks_json, query_bindings_json, capability_gaps_json,
                        blocker_codes_json,
                        manifest_sha256, created_at
                 FROM work_plan_versions WHERE plan_id = ?1 AND version = ?2",
                params![plan_id, version],
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
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        let current = self
            .connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM work_plan_heads
                   JOIN bid_decision_package_heads
                     ON bid_decision_package_heads.package_id = ?3
                    AND bid_decision_package_heads.current_version = ?4
                   JOIN tender ON tender.singleton = 1
                   WHERE work_plan_heads.plan_id = ?1
                     AND work_plan_heads.current_version = ?2
                     AND tender.lifecycle_phase IN ('tender_planning', 'active_production')
                 )",
                params![plan_id, version, row.0, row.1],
                |value| value.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
        let approval = self
            .connection
            .query_row(
                "SELECT approval_id, decision, rationale, plan_manifest_sha256,
                        decided_by, acting_role, approval_sha256, created_at,
                        approval_manifest_json
                 FROM work_plan_approvals
                 WHERE plan_id = ?1 AND plan_version = ?2",
                params![plan_id, version],
                |approval| {
                    Ok((
                        approval.get::<_, String>(0)?,
                        approval.get::<_, String>(1)?,
                        approval.get::<_, String>(2)?,
                        approval.get::<_, String>(3)?,
                        approval.get::<_, String>(4)?,
                        approval.get::<_, String>(5)?,
                        approval.get::<_, String>(6)?,
                        approval.get::<_, String>(7)?,
                        approval.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?
            .map(
                |(
                    approval_id,
                    decision,
                    rationale,
                    plan_manifest_sha256,
                    decided_by,
                    acting_role,
                    approval_sha256,
                    created_at,
                    approval_manifest_json,
                )| {
                    let _: Value = parse_canonical_json(&approval_manifest_json)?;
                    Ok(WorkPlanApprovalRecord {
                        approval_id,
                        plan_id: plan_id.into(),
                        plan_version: version,
                        decision: WorkPlanDecision::parse(&decision)?,
                        rationale,
                        plan_manifest_sha256,
                        decided_by,
                        acting_role,
                        approval_sha256,
                        created_at,
                    })
                },
            )
            .transpose()?;
        let mut profiles: Vec<WorkPlanProfileBinding> = parse_canonical_json(&row.5)?;
        budget.check()?;
        for binding in &mut profiles {
            let head = self
                .connection
                .query_row(
                    "SELECT current_version, status FROM agent_profile_heads WHERE profile_id = ?1",
                    [&binding.profile.profile_id],
                    |head| Ok((head.get::<_, u32>(0)?, head.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            if let Some((head_version, status)) = head {
                if head_version == binding.profile.version {
                    binding.status = AgentProfileStatus::parse(&status)?;
                }
            }
        }
        Ok(WorkPlanProposalInspection {
            plan_id: plan_id.into(),
            version,
            bid_package_id: row.0,
            bid_package_version: row.1,
            bid_package_manifest_sha256: row.2,
            capability_catalogue_version: row.3,
            permission_policy_version: row.4,
            profiles,
            workstreams: parse_canonical_json(&row.6)?,
            tasks: parse_canonical_json(&row.7)?,
            query_bindings: parse_canonical_json(&row.8)?,
            capability_gaps: parse_canonical_json(&row.9)?,
            blocker_codes: parse_canonical_json(&row.10)?,
            approval,
            current,
            created_by: "engineer_user".into(),
            created_at: row.12,
            manifest_sha256: row.11,
        })
    }

    pub(crate) fn work_plan_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        if !self.agent_profile_heads_are_valid_with_check(check)? {
            return Ok(false);
        }
        let (plan_count, head_count, version_count, approval_count): (u32, u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM work_plans),
                        (SELECT COUNT(*) FROM work_plan_heads),
                        (SELECT COUNT(*) FROM work_plan_versions),
                        (SELECT COUNT(*) FROM work_plan_approvals)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(sql_error)?;
        if plan_count > 1
            || head_count != plan_count
            || usize::try_from(version_count)
                .ok()
                .is_none_or(|count| count > MAX_PLAN_VERSIONS)
            || approval_count > version_count
        {
            return Ok(false);
        }
        if plan_count == 0 {
            return Ok(version_count == 0 && approval_count == 0);
        }
        let (head_plan_id, head_version): (String, u32) = self
            .connection
            .query_row(
                "SELECT plan_id, current_version FROM work_plan_heads",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if head_version != version_count || head_version == 0 {
            return Ok(false);
        }

        let mut current_profiles = None;
        let mut current_decision = None;
        let mut current_package = None;
        let mut prior_version_state: Option<WorkPlanIntegrityVersionState> = None;
        for expected_version in 1..=head_version {
            check()?;
            let row: StoredWorkPlanVersionRow = self
                .connection
                .query_row(
                    "SELECT bid_package_id, bid_package_version,
                            bid_package_manifest_sha256, capability_catalogue_version,
                            permission_policy_version, profiles_json, workstreams_json,
                            tasks_json, query_bindings_json, capability_gaps_json,
                            blocker_codes_json, revision_actions_json, manifest_json, manifest_sha256,
                            created_by, created_at
                     FROM work_plan_versions
                     WHERE plan_id = ?1 AND version = ?2",
                    params![head_plan_id, expected_version],
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
                            row.get(8)?,
                            row.get(9)?,
                            row.get(10)?,
                            row.get(11)?,
                            row.get(12)?,
                            row.get(13)?,
                            row.get(14)?,
                            row.get(15)?,
                        ))
                    },
                )
                .optional()
                .map_err(sql_error)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let aggregate_bytes = row
                .5
                .len()
                .checked_add(row.6.len())
                .and_then(|size| size.checked_add(row.7.len()))
                .and_then(|size| size.checked_add(row.8.len()))
                .and_then(|size| size.checked_add(row.9.len()))
                .and_then(|size| size.checked_add(row.10.len()))
                .and_then(|size| size.checked_add(row.11.len()));
            if row.12.len() > MAX_PLAN_MANIFEST_BYTES
                || aggregate_bytes.is_none_or(|size| size > MAX_PLAN_MANIFEST_BYTES)
                || row.3 != CAPABILITY_CATALOGUE_VERSION
                || row.4 != PERMISSION_POLICY_VERSION
                || row.14 != "engineer_user"
            {
                return Ok(false);
            }
            let profiles: Vec<WorkPlanProfileBinding> = parse_canonical_json(&row.5)?;
            let workstreams: Vec<WorkPlanWorkstream> = parse_canonical_json(&row.6)?;
            let tasks: Vec<WorkPlanTask> = parse_canonical_json(&row.7)?;
            let query_bindings: Vec<TenderRecordVersionReference> = parse_canonical_json(&row.8)?;
            let capability_gaps: Vec<WorkPlanCapabilityGap> = parse_canonical_json(&row.9)?;
            let blocker_codes: Vec<String> = parse_canonical_json(&row.10)?;
            let revision_actions: Vec<Value> = parse_canonical_json(&row.11)?;
            let typed_actions = revision_actions
                .iter()
                .cloned()
                .map(|action| {
                    serde_json::from_value::<WorkPlanRevisionAction>(action)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if (expected_version == 1 && !typed_actions.is_empty())
                || (expected_version > 1 && validate_revision_actions(&typed_actions).is_err())
                || validate_plan_shape(
                    &profiles,
                    &workstreams,
                    &tasks,
                    &query_bindings,
                    &capability_gaps,
                )
                .is_err()
                || profiles
                    .iter()
                    .any(|binding| binding.status != AgentProfileStatus::Proposed)
            {
                return Ok(false);
            }
            if let Some(prior) = prior_version_state.as_ref() {
                let package_changed = prior.package_id != row.0
                    || prior.package_version != row.1
                    || prior.package_manifest_sha256 != row.2;
                let rebase = matches!(
                    typed_actions.as_slice(),
                    [WorkPlanRevisionAction::RebasePackageBasis]
                );
                if package_changed != rebase
                    || (rebase
                        && !accepted_package_is_invalidated(
                            &self.connection,
                            &prior.package_id,
                            prior.package_version,
                        )?)
                    || !revision_transition_is_valid(&prior.profiles, &profiles, &typed_actions)
                {
                    return Ok(false);
                }
            }
            for binding in &profiles {
                check()?;
                if load_profile(
                    &self.connection,
                    (binding.profile.profile_id.clone(), binding.profile.version),
                )? != binding.profile
                {
                    return Ok(false);
                }
            }
            let package_is_exact: bool = self
                .connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM bid_decision_package_versions
                       WHERE package_id = ?1 AND version = ?2 AND manifest_sha256 = ?3
                     )",
                    params![row.0, row.1, row.2],
                    |value| value.get(0),
                )
                .map_err(sql_error)?;
            if !package_is_exact {
                return Ok(false);
            }
            if load_package_query_bindings_with_check(&self.connection, &row.0, row.1, check)?
                != query_bindings
            {
                return Ok(false);
            }
            let manifest = WorkPlanManifest {
                schema_version: 1,
                plan_id: &head_plan_id,
                version: expected_version,
                bid_package_id: &row.0,
                bid_package_version: row.1,
                bid_package_manifest_sha256: &row.2,
                capability_catalogue_version: row.3,
                permission_policy_version: row.4,
                profiles: &profiles,
                workstreams: &workstreams,
                tasks: &tasks,
                query_bindings: &query_bindings,
                capability_gaps: &capability_gaps,
                blocker_codes: &blocker_codes,
                revision_actions: &revision_actions,
                created_by: &row.14,
                created_at: &row.15,
            };
            let manifest_json = canonical_json(&manifest)?;
            if manifest_json != row.12 || sha256_hex(manifest_json.as_bytes()) != row.13 {
                return Ok(false);
            }
            let expected_event = if expected_version == 1 {
                "work_plan_proposed"
            } else {
                "work_plan_revised"
            };
            if !work_plan_version_audit_exists(
                &self.connection,
                expected_event,
                &head_plan_id,
                expected_version,
                &row.13,
            )? {
                return Ok(false);
            }

            let approval = self
                .connection
                .query_row(
                    "SELECT approval_id, decision, rationale, plan_manifest_sha256,
                            decided_by, acting_role, approval_manifest_json,
                            approval_sha256, created_at
                     FROM work_plan_approvals
                     WHERE plan_id = ?1 AND plan_version = ?2",
                    params![head_plan_id, expected_version],
                    |approval| {
                        Ok((
                            approval.get::<_, String>(0)?,
                            approval.get::<_, String>(1)?,
                            approval.get::<_, String>(2)?,
                            approval.get::<_, String>(3)?,
                            approval.get::<_, String>(4)?,
                            approval.get::<_, String>(5)?,
                            approval.get::<_, String>(6)?,
                            approval.get::<_, String>(7)?,
                            approval.get::<_, String>(8)?,
                        ))
                    },
                )
                .optional()
                .map_err(sql_error)?;
            let decision = if let Some(approval) = approval {
                check()?;
                let decision = WorkPlanDecision::parse(&approval.1)?;
                if approval.3 != row.13
                    || approval.4 != "engineer_user"
                    || approval.5 != "tendering_manager"
                    || approval.6.len() > MAX_PLAN_MANIFEST_BYTES
                {
                    return Ok(false);
                }
                let approval_manifest = WorkPlanApprovalManifest {
                    schema_version: 1,
                    approval_id: &approval.0,
                    plan_id: &head_plan_id,
                    plan_version: expected_version,
                    decision,
                    rationale: &approval.2,
                    plan_manifest_sha256: &approval.3,
                    profiles: profiles
                        .iter()
                        .map(|binding| {
                            (binding.profile.profile_id.as_str(), binding.profile.version)
                        })
                        .collect(),
                    decided_by: &approval.4,
                    acting_role: &approval.5,
                    created_at: &approval.8,
                };
                let approval_manifest_json = canonical_json(&approval_manifest)?;
                if approval_manifest_json != approval.6
                    || sha256_hex(approval_manifest_json.as_bytes()) != approval.7
                    || (decision == WorkPlanDecision::Approve
                        && (!blocker_codes.is_empty()
                            || !capability_gaps.is_empty()
                            || !approval_invariants_hold(&WorkPlanProposalInspection {
                                plan_id: head_plan_id.clone(),
                                version: expected_version,
                                bid_package_id: row.0.clone(),
                                bid_package_version: row.1,
                                bid_package_manifest_sha256: row.2.clone(),
                                capability_catalogue_version: row.3,
                                permission_policy_version: row.4,
                                profiles: profiles.clone(),
                                workstreams: workstreams.clone(),
                                tasks: tasks.clone(),
                                query_bindings: query_bindings.clone(),
                                capability_gaps: capability_gaps.clone(),
                                blocker_codes: blocker_codes.clone(),
                                approval: None,
                                current: false,
                                created_by: row.14.clone(),
                                created_at: row.15.clone(),
                                manifest_sha256: row.13.clone(),
                            })))
                    || !work_plan_approval_audit_exists(
                        &self.connection,
                        &head_plan_id,
                        expected_version,
                        decision,
                        &approval.0,
                        &approval.7,
                    )?
                {
                    return Ok(false);
                }
                Some(decision)
            } else {
                None
            };
            if expected_version == head_version {
                current_profiles = Some(profiles.clone());
                current_decision = decision;
                current_package = Some((row.0.clone(), row.1));
            }
            prior_version_state = Some(WorkPlanIntegrityVersionState {
                profiles,
                package_id: row.0,
                package_version: row.1,
                package_manifest_sha256: row.2,
            });
        }

        let current_profiles = current_profiles
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let current_package = current_package
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let production_status: Option<String> = self
            .connection
            .query_row(
                "SELECT status FROM production_activations
                 WHERE plan_id = ?1 AND plan_version = ?2",
                params![head_plan_id, head_version],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let approved_package_invalidated = current_decision == Some(WorkPlanDecision::Approve)
            && accepted_package_is_invalidated(
                &self.connection,
                &current_package.0,
                current_package.1,
            )?;
        let expected_status =
            if approved_package_invalidated || production_status.as_deref() == Some("suspended") {
                AgentProfileStatus::Suspended
            } else if production_status.as_deref() == Some("active") {
                AgentProfileStatus::Active
            } else {
                AgentProfileStatus::Proposed
            };
        for binding in &current_profiles {
            check()?;
            let (head_profile_version, head_status): (u32, String) = self
                .connection
                .query_row(
                    "SELECT current_version, status FROM agent_profile_heads
                     WHERE profile_id = ?1",
                    [&binding.profile.profile_id],
                    |head| Ok((head.get(0)?, head.get(1)?)),
                )
                .map_err(sql_error)?;
            if head_profile_version != binding.profile.version
                || AgentProfileStatus::parse(&head_status)? != expected_status
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn agent_profile_heads_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        let (profile_count, head_count, version_count): (u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM agent_profiles),
                        (SELECT COUNT(*) FROM agent_profile_heads),
                        (SELECT COUNT(*) FROM agent_profile_versions)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        if profile_count != head_count
            || usize::try_from(profile_count)
                .ok()
                .is_none_or(|count| count > MAX_STORED_AGENT_PROFILES)
            || usize::try_from(version_count)
                .ok()
                .is_none_or(|count| count > MAX_STORED_AGENT_PROFILES)
        {
            return Ok(false);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT agent_profiles.profile_id, agent_profile_heads.current_version,
                        agent_profile_heads.status, COUNT(agent_profile_versions.version),
                        MAX(agent_profile_versions.version)
                 FROM agent_profiles
                 JOIN agent_profile_heads USING (profile_id)
                 JOIN agent_profile_versions USING (profile_id)
                 GROUP BY agent_profiles.profile_id, agent_profile_heads.current_version,
                          agent_profile_heads.status
                 ORDER BY agent_profiles.profile_id",
            )
            .map_err(sql_error)?;
        let mut rows = statement.query([]).map_err(sql_error)?;
        let mut seen = 0_usize;
        while let Some(row) = rows.next().map_err(sql_error)? {
            check()?;
            seen = seen
                .checked_add(1)
                .filter(|count| *count <= MAX_STORED_AGENT_PROFILES)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let profile_id = row.get::<_, String>(0).map_err(sql_error)?;
            let current_version = row.get::<_, u32>(1).map_err(sql_error)?;
            let status = AgentProfileStatus::parse(&row.get::<_, String>(2).map_err(sql_error)?)?;
            let stored_versions = row.get::<_, u32>(3).map_err(sql_error)?;
            let maximum_version = row.get::<_, u32>(4).map_err(sql_error)?;
            if current_version == 0 || current_version != maximum_version || stored_versions == 0 {
                return Ok(false);
            }
            let profile = load_profile(&self.connection, (profile_id, current_version))?;
            if !profile_shape_is_valid(&profile, status) {
                return Ok(false);
            }
        }
        Ok(seen == profile_count as usize)
    }
}

fn accepted_current_package(
    store: &TenderStore,
    budget: BidPackageOperationBudget,
) -> Result<BidDecisionPackageInspection, TenderCommandError> {
    budget.check()?;
    let package = store
        .inspect_current_bid_decision_package()?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if !package.current
        || !matches!(
            package.lifecycle_phase,
            TenderLifecyclePhase::TenderPlanning | TenderLifecyclePhase::ActiveProduction
        )
        || package.approval.as_ref().map(|approval| approval.decision)
            != Some(BidDecisionApprovalDecision::Accept)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    budget.check()?;
    Ok(package)
}

fn work_plan_deadlines(
    store: &TenderStore,
    package: &BidDecisionPackageInspection,
    budget: BidPackageOperationBudget,
) -> Result<Vec<String>, TenderCommandError> {
    budget.check()?;
    let mut statement = store
        .connection
        .prepare(
            "SELECT rows.record_id, rows.record_version
             FROM bid_compliance_rows AS rows
             JOIN tender_record_versions AS records
               ON records.record_id = rows.record_id
              AND records.version = rows.record_version
             WHERE rows.package_id = ?1 AND rows.package_version = ?2
               AND records.kind = 'deadline'
             ORDER BY rows.ordinal",
        )
        .map_err(sql_error)?;
    let mapped = statement
        .query_map(params![package.package_id, package.version], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(sql_error)?;
    let mut deadlines = Vec::new();
    for reference in mapped {
        budget.check()?;
        if deadlines.len() >= MAX_PLAN_QUERIES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (record_id, version) = reference.map_err(sql_error)?;
        let record = store.inspect_tender_record_version(&record_id, version)?;
        if record.kind != TenderRecordKind::Deadline {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for field in record.fields {
            let value = field.normalized_value.or(field.value);
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                validate_text(&value, 200)?;
                if deadlines.len() >= MAX_PLAN_QUERIES {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                deadlines.push(value);
            }
        }
    }
    deadlines.sort();
    deadlines.dedup();
    if deadlines.is_empty() || deadlines.len() > MAX_PLAN_QUERIES {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(deadlines)
}

fn deadlines_from_plan(
    plan: &WorkPlanProposalInspection,
) -> Result<Vec<String>, TenderCommandError> {
    let mut deadlines = plan
        .workstreams
        .iter()
        .flat_map(|workstream| workstream.deadlines.iter().cloned())
        .collect::<Vec<_>>();
    deadlines.sort();
    deadlines.dedup();
    if deadlines.is_empty()
        || deadlines.len() > MAX_PLAN_QUERIES
        || deadlines
            .iter()
            .any(|deadline| validate_text(deadline, 200).is_err())
        || plan
            .tasks
            .iter()
            .any(|task| !deadlines.contains(&task.deadline))
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(deadlines)
}

fn load_package_query_bindings(
    connection: &rusqlite::Connection,
    package_id: &str,
    package_version: u32,
    budget: BidPackageOperationBudget,
) -> Result<Vec<TenderRecordVersionReference>, TenderCommandError> {
    load_package_query_bindings_with_check(connection, package_id, package_version, &mut || {
        budget.check()
    })
}

fn load_package_query_bindings_with_check(
    connection: &rusqlite::Connection,
    package_id: &str,
    package_version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<TenderRecordVersionReference>, TenderCommandError> {
    check()?;
    let count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM bid_decision_package_record_bindings
             WHERE package_id = ?1 AND package_version = ?2
               AND category = 'unresolved_query'",
            params![package_id, package_version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if usize::try_from(count)
        .ok()
        .is_none_or(|count| count > MAX_PLAN_QUERIES)
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let mut statement = connection
        .prepare(
            "SELECT record_id, record_version
             FROM bid_decision_package_record_bindings
             WHERE package_id = ?1 AND package_version = ?2
               AND category = 'unresolved_query'
             ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let mut rows = statement
        .query(params![package_id, package_version])
        .map_err(sql_error)?;
    let mut queries = Vec::with_capacity(count as usize);
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        if queries.len() >= MAX_PLAN_QUERIES {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        queries.push(TenderRecordVersionReference {
            record_id: row.get(0).map_err(sql_error)?,
            version: row.get(1).map_err(sql_error)?,
        });
    }
    if queries.len() != count as usize {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(queries)
}

fn append_work_plan_denial(
    transaction: &rusqlite::Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    command: &str,
    plan_id: Option<&str>,
    plan_version: Option<u32>,
    reason: &str,
) -> Result<(), TenderCommandError> {
    let created_at = sqlite_timestamp(transaction)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "work_plan_command_denied",
        tender_revision,
        json!({
            "command": command,
            "plan_id": plan_id,
            "plan_version": plan_version.map(|version| version.to_string()),
            "reason": reason,
        }),
        &created_at,
    )
}

fn accepted_package_is_invalidated(
    connection: &rusqlite::Connection,
    package_id: &str,
    package_version: u32,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM bid_decision_approval_records AS approvals
               JOIN bid_decision_approval_invalidations AS invalidations
                 ON invalidations.approval_id = approvals.approval_id
               WHERE approvals.package_id = ?1 AND approvals.package_version = ?2
                 AND approvals.decision = 'accept'
             )",
            params![package_id, package_version],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn work_plan_version_audit_exists(
    connection: &rusqlite::Connection,
    event_type: &str,
    plan_id: &str,
    version: u32,
    manifest_sha256: &str,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM audit_events
               WHERE event_type = ?1
                 AND json_extract(payload_json, '$.change.plan_id') = ?2
                 AND json_extract(payload_json, '$.change.plan_version') = ?3
                 AND json_extract(payload_json, '$.change.manifest_sha256') = ?4
             )",
            params![event_type, plan_id, version.to_string(), manifest_sha256],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn work_plan_approval_audit_exists(
    connection: &rusqlite::Connection,
    plan_id: &str,
    version: u32,
    decision: WorkPlanDecision,
    approval_id: &str,
    approval_sha256: &str,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM audit_events
               WHERE event_type = 'work_plan_decided'
                 AND json_extract(payload_json, '$.change.plan_id') = ?1
                 AND json_extract(payload_json, '$.change.plan_version') = ?2
                 AND json_extract(payload_json, '$.change.decision') = ?3
                 AND json_extract(payload_json, '$.change.approval_id') = ?4
                 AND json_extract(payload_json, '$.change.approval_sha256') = ?5
             )",
            params![
                plan_id,
                version.to_string(),
                decision.as_str(),
                approval_id,
                approval_sha256
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn revision_transition_is_valid(
    prior: &[WorkPlanProfileBinding],
    current: &[WorkPlanProfileBinding],
    actions: &[WorkPlanRevisionAction],
) -> bool {
    if matches!(actions, [WorkPlanRevisionAction::RebasePackageBasis]) {
        return prior == current;
    }
    let prior_by_id = prior
        .iter()
        .map(|binding| (binding.profile.profile_id.as_str(), binding))
        .collect::<std::collections::HashMap<_, _>>();
    let current_by_id = current
        .iter()
        .map(|binding| (binding.profile.profile_id.as_str(), binding))
        .collect::<std::collections::HashMap<_, _>>();
    let mut consumed_prior = std::collections::HashSet::new();
    let mut consumed_current = std::collections::HashSet::new();

    for action in actions {
        let valid = match action {
            WorkPlanRevisionAction::RebasePackageBasis => false,
            WorkPlanRevisionAction::AddProfile {
                archetype,
                identity,
            } => {
                let added = current.iter().find(|binding| {
                    !prior_by_id.contains_key(binding.profile.profile_id.as_str())
                        && !consumed_current.contains(binding.profile.profile_id.as_str())
                        && binding.archetype == *archetype
                        && binding.profile.identity == identity.trim()
                        && binding.profile.version == 1
                        && profile_permissions_are_coherent(&binding.profile)
                });
                let Some(added) = added else { return false };
                consumed_current.insert(added.profile.profile_id.as_str());
                let review_capability = added
                    .profile
                    .capabilities
                    .iter()
                    .find(|capability| review_profile_spec(capability).is_some())
                    .map(|capability| review_capability(capability));
                if let Some(review_capability) = review_capability {
                    let prior_had_reviewer = prior
                        .iter()
                        .any(|binding| binding.profile.capabilities.contains(&review_capability));
                    if !prior_had_reviewer {
                        let reviewer = current.iter().find(|binding| {
                            !prior_by_id.contains_key(binding.profile.profile_id.as_str())
                                && !consumed_current.contains(binding.profile.profile_id.as_str())
                                && binding.profile.capabilities == vec![review_capability.clone()]
                                && binding.profile.version == 1
                                && profile_permissions_are_coherent(&binding.profile)
                        });
                        let Some(reviewer) = reviewer else {
                            return false;
                        };
                        consumed_current.insert(reviewer.profile.profile_id.as_str());
                    }
                }
                true
            }
            WorkPlanRevisionAction::RemoveProfile { profile_id } => {
                prior_by_id.contains_key(profile_id.as_str())
                    && !current_by_id.contains_key(profile_id.as_str())
                    && consumed_prior.insert(profile_id.as_str())
            }
            WorkPlanRevisionAction::RenameProfile {
                profile_id,
                identity,
            } => {
                let (Some(before), Some(after)) = (
                    prior_by_id.get(profile_id.as_str()),
                    current_by_id.get(profile_id.as_str()),
                ) else {
                    return false;
                };
                let mut expected = (*before).clone();
                expected.profile.version = expected.profile.version.saturating_add(1);
                expected.profile.identity = identity.trim().into();
                expected == **after
                    && consumed_prior.insert(profile_id.as_str())
                    && consumed_current.insert(profile_id.as_str())
            }
            WorkPlanRevisionAction::AdjustProfile {
                profile_id,
                objective,
                behavior,
                skepticism,
                risk_tolerance,
                resource_budget,
            } => {
                let (Some(before), Some(after)) = (
                    prior_by_id.get(profile_id.as_str()),
                    current_by_id.get(profile_id.as_str()),
                ) else {
                    return false;
                };
                let mut expected = (*before).clone();
                expected.profile.version = expected.profile.version.saturating_add(1);
                expected.profile.objective = objective.trim().into();
                expected.profile.behavior = behavior.trim().into();
                expected.profile.skepticism = skepticism.trim().into();
                expected.profile.risk_tolerance = risk_tolerance.trim().into();
                expected.profile.instructions = format!(
                    "{} {} {}",
                    expected.profile.objective,
                    expected.profile.behavior,
                    expected.profile.skepticism
                );
                expected.profile.resource_budget = resource_budget.clone();
                expected == **after
                    && consumed_prior.insert(profile_id.as_str())
                    && consumed_current.insert(profile_id.as_str())
            }
            WorkPlanRevisionAction::SplitProfile {
                profile_id,
                identities,
            } => {
                let Some(source) = prior_by_id.get(profile_id.as_str()) else {
                    return false;
                };
                if current_by_id.contains_key(profile_id.as_str())
                    || !consumed_prior.insert(profile_id.as_str())
                {
                    return false;
                }
                let mut expected_capabilities = source.profile.capabilities.clone();
                expected_capabilities.sort();
                let mut actual_capabilities = Vec::new();
                for identity in identities {
                    let candidate = current.iter().find(|binding| {
                        !prior_by_id.contains_key(binding.profile.profile_id.as_str())
                            && !consumed_current.contains(binding.profile.profile_id.as_str())
                            && binding.archetype == source.archetype
                            && binding.profile.identity == identity.trim()
                            && binding.profile.version == 1
                            && binding.profile.capabilities.len() == 1
                            && profile_permissions_are_coherent(&binding.profile)
                    });
                    let Some(candidate) = candidate else {
                        return false;
                    };
                    actual_capabilities.push(candidate.profile.capabilities[0].clone());
                    consumed_current.insert(candidate.profile.profile_id.as_str());
                }
                actual_capabilities.sort();
                actual_capabilities == expected_capabilities
            }
            WorkPlanRevisionAction::CombineProfiles {
                profile_ids,
                identity,
            } => {
                let selected = profile_ids
                    .iter()
                    .map(|profile_id| prior_by_id.get(profile_id.as_str()).copied())
                    .collect::<Option<Vec<_>>>();
                let Some(selected) = selected else {
                    return false;
                };
                if selected.is_empty()
                    || selected
                        .iter()
                        .any(|binding| binding.archetype != selected[0].archetype)
                    || profile_ids.iter().any(|profile_id| {
                        current_by_id.contains_key(profile_id.as_str())
                            || !consumed_prior.insert(profile_id.as_str())
                    })
                {
                    return false;
                }
                let mut expected_capabilities = selected
                    .iter()
                    .flat_map(|binding| binding.profile.capabilities.iter().cloned())
                    .collect::<Vec<_>>();
                expected_capabilities.sort();
                expected_capabilities.dedup();
                let candidate = current.iter().find(|binding| {
                    !prior_by_id.contains_key(binding.profile.profile_id.as_str())
                        && !consumed_current.contains(binding.profile.profile_id.as_str())
                        && binding.archetype == selected[0].archetype
                        && binding.profile.identity == identity.trim()
                        && binding.profile.version == 1
                        && binding.profile.capabilities == expected_capabilities
                        && profile_permissions_are_coherent(&binding.profile)
                });
                let Some(candidate) = candidate else {
                    return false;
                };
                consumed_current.insert(candidate.profile.profile_id.as_str());
                true
            }
        };
        if !valid {
            return false;
        }
    }

    prior.iter().all(|binding| {
        consumed_prior.contains(binding.profile.profile_id.as_str())
            || current_by_id
                .get(binding.profile.profile_id.as_str())
                .is_some_and(|candidate| **candidate == *binding)
    }) && current.iter().all(|binding| {
        consumed_current.contains(binding.profile.profile_id.as_str())
            || prior_by_id
                .get(binding.profile.profile_id.as_str())
                .is_some_and(|candidate| **candidate == *binding)
    })
}

fn ensure_exact_proceed_still_current(
    transaction: &rusqlite::Transaction<'_>,
    package: &BidDecisionPackageInspection,
) -> Result<(), TenderCommandError> {
    let current: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM tender
               JOIN bid_decision_package_heads AS heads
                 ON heads.package_id = ?1 AND heads.current_version = ?2
               JOIN bid_decision_package_versions AS versions
                 ON versions.package_id = heads.package_id
                AND versions.version = heads.current_version
                AND versions.manifest_sha256 = ?3
               JOIN bid_decision_approval_records AS approvals
                 ON approvals.package_id = heads.package_id
                AND approvals.package_version = heads.current_version
                AND approvals.decision = 'accept'
               WHERE tender.singleton = 1
                     AND tender.lifecycle_phase IN ('tender_planning', 'active_production')
                 AND NOT EXISTS (
                   SELECT 1 FROM bid_decision_approval_invalidations
                   WHERE approval_id = approvals.approval_id
                 )
                 AND NOT EXISTS (SELECT 1 FROM work_plan_heads)
             )",
            params![package.package_id, package.version, package.manifest_sha256],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if current {
        Ok(())
    } else {
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }
}

fn ensure_exact_accepted_package_still_current(
    transaction: &rusqlite::Transaction<'_>,
    package: &BidDecisionPackageInspection,
) -> Result<(), TenderCommandError> {
    let current: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM tender
               JOIN bid_decision_package_heads AS heads
                 ON heads.package_id = ?1 AND heads.current_version = ?2
               JOIN bid_decision_package_versions AS versions
                 ON versions.package_id = heads.package_id
                AND versions.version = heads.current_version
                AND versions.manifest_sha256 = ?3
               JOIN bid_decision_approval_records AS approvals
                 ON approvals.package_id = heads.package_id
                AND approvals.package_version = heads.current_version
                AND approvals.decision = 'accept'
               WHERE tender.singleton = 1
                 AND tender.lifecycle_phase IN ('tender_planning', 'active_production')
                 AND NOT EXISTS (
                   SELECT 1 FROM bid_decision_approval_invalidations
                   WHERE approval_id = approvals.approval_id
                 )
             )",
            params![package.package_id, package.version, package.manifest_sha256],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if current {
        Ok(())
    } else {
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }
}

fn compose_profiles(
    transaction: &rusqlite::Transaction<'_>,
    package: &BidDecisionPackageInspection,
    created_at: &str,
) -> Result<Vec<WorkPlanProfileBinding>, TenderCommandError> {
    let mut profiles = Vec::new();
    for role in BootstrapRole::ALL {
        let profile_id: String = transaction
            .query_row(
                "SELECT profile_id FROM agent_profiles WHERE stable_identity = ?1",
                [role.stable_identity()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let next_version: u32 = transaction
            .query_row(
                "SELECT COALESCE(MAX(version), 0) + 1 FROM agent_profile_versions
                 WHERE profile_id = ?1",
                [&profile_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let (archetype, identity, profession, capability, objective, scope) = match role {
            BootstrapRole::TenderOfficeCoordinator => (
                "tender_office_coordinator",
                "Tender Office Coordinator",
                "Tender Coordination Engineer",
                vec!["tender_coordination", "query_rfi_control"],
                "Coordinate approved Tender production work, dependencies, milestones, and escalation without making Tendering Manager decisions.",
                "tender_coordination",
            ),
            BootstrapRole::DocumentController => (
                "document_controller",
                "Document Controller",
                "Tender Document Controller",
                vec!["document_control"],
                "Control exact sources, registers, relationships, and production document versions.",
                "tender_sources",
            ),
            BootstrapRole::TenderAnalyst => (
                "tender_analyst",
                "Tender Analyst",
                "Tender Engineer",
                vec!["tender_analysis"],
                "Maintain the evidence-linked Tender analysis basis and coordinate unresolved assumptions and requirements.",
                "tender_analysis",
            ),
            BootstrapRole::IndependentReviewer => (
                "independent_reviewer",
                "Independent Reviewer",
                "Tender Assurance Engineer",
                vec![
                    "independent_review",
                    "review_tender_coordination",
                    "review_document_control",
                    "review_tender_analysis",
                    "review_query_rfi_control",
                ],
                "Independently review exact production outputs without editing or approving them.",
                "tender_assurance",
            ),
        };
        let profile = production_profile(
            profile_id,
            next_version,
            identity,
            profession,
            capability,
            objective,
            scope,
        );
        insert_profile_version(transaction, &profile, created_at)?;
        update_profile_head(
            transaction,
            &profile.profile_id,
            profile.version,
            AgentProfileStatus::Proposed,
        )?;
        profiles.push(WorkPlanProfileBinding {
            archetype: archetype.into(),
            status: AgentProfileStatus::Proposed,
            profile,
        });
    }
    let cost_profile_id = random_identifier(transaction)?;
    let cost_profile = production_profile(
        cost_profile_id.clone(),
        1,
        "Cost Estimator",
        "Senior Construction Cost Estimator",
        vec!["cost_estimation"],
        "Develop evidence-linked quantities, rate build-ups, costs, risk provisions, and pricing scenarios through controlled calculations.",
        "commercial_estimate",
    );
    transaction
        .execute(
            "INSERT INTO agent_profiles (profile_id, stable_identity, created_at)
             VALUES (?1, ?2, ?3)",
            params![
                cost_profile_id,
                format!("quantix.production.{cost_profile_id}"),
                created_at
            ],
        )
        .map_err(sql_error)?;
    insert_profile_version(transaction, &cost_profile, created_at)?;
    transaction
        .execute(
            "INSERT INTO agent_profile_heads (profile_id, current_version, status)
             VALUES (?1, 1, 'proposed')",
            [&cost_profile_id],
        )
        .map_err(sql_error)?;
    profiles.push(WorkPlanProfileBinding {
        archetype: "cost_estimator".into(),
        status: AgentProfileStatus::Proposed,
        profile: cost_profile,
    });
    profiles.push(compose_specialist_reviewer(
        transaction,
        "cost_estimation",
        created_at,
    )?);
    for demand in &package.capability_demands {
        if profiles.iter().any(|binding| {
            binding
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == &demand.capability)
        }) {
            continue;
        }
        let Some((archetype, identity, profession, objective, scope)) =
            conditional_profile_spec(&demand.capability)
        else {
            continue;
        };
        let profile_id = random_identifier(transaction)?;
        let profile = production_profile(
            profile_id.clone(),
            1,
            identity,
            profession,
            vec![demand.capability.as_str()],
            objective,
            scope,
        );
        transaction
            .execute(
                "INSERT INTO agent_profiles (profile_id, stable_identity, created_at)
                 VALUES (?1, ?2, ?3)",
                params![
                    profile_id,
                    format!("quantix.production.{profile_id}"),
                    created_at
                ],
            )
            .map_err(sql_error)?;
        insert_profile_version(transaction, &profile, created_at)?;
        transaction
            .execute(
                "INSERT INTO agent_profile_heads (profile_id, current_version, status)
                 VALUES (?1, 1, 'proposed')",
                [&profile_id],
            )
            .map_err(sql_error)?;
        profiles.push(WorkPlanProfileBinding {
            archetype: archetype.into(),
            status: AgentProfileStatus::Proposed,
            profile,
        });
        profiles.push(compose_specialist_reviewer(
            transaction,
            &demand.capability,
            created_at,
        )?);
    }
    Ok(profiles)
}

fn compose_specialist_reviewer(
    transaction: &rusqlite::Transaction<'_>,
    capability: &str,
    created_at: &str,
) -> Result<WorkPlanProfileBinding, TenderCommandError> {
    let (archetype, identity, profession, objective, scope) = review_profile_spec(capability)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let profile_id = random_identifier(transaction)?;
    let review_capability = review_capability(capability);
    let profile = production_profile(
        profile_id,
        1,
        identity,
        profession,
        vec![review_capability.as_str()],
        objective,
        scope,
    );
    insert_new_proposed_profile(transaction, archetype, profile, created_at)
}

fn review_profile_spec(
    capability: &str,
) -> Option<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    match capability {
        "cost_estimation" => Some((
            "independent_cost_reviewer",
            "Independent Cost Reviewer",
            "Senior Cost Assurance Surveyor",
            "Independently review exact estimating outputs, quantities, rates, allowances, and commercial uncertainty without editing them.",
            "commercial_estimate_review",
        )),
        "programme_planning" => Some((
            "independent_planning_reviewer",
            "Independent Planning Reviewer",
            "Senior Schedule Assurance Engineer",
            "Independently review exact programme logic, milestones, float, constraints, and schedule risks without editing them.",
            "tender_programme_review",
        )),
        "contracts_review" => Some((
            "independent_contracts_reviewer",
            "Independent Contracts Reviewer",
            "Senior Contracts Assurance Engineer",
            "Independently review exact contractual analysis, qualifications, exceptions, and risk allocation without editing them.",
            "contracts_analysis_review",
        )),
        "procurement_analysis" => Some((
            "independent_procurement_reviewer",
            "Independent Procurement Reviewer",
            "Senior Procurement Assurance Engineer",
            "Independently review exact supplier, quotation, lead-time, and procurement analysis without editing it.",
            "procurement_analysis_review",
        )),
        "technical_review" => Some((
            "independent_technical_reviewer",
            "Independent Technical Reviewer",
            "Senior Technical Assurance Engineer",
            "Independently review exact technical Tender outputs and their evidence without editing them.",
            "technical_analysis_review",
        )),
        _ => None,
    }
}

fn review_capability(capability: &str) -> String {
    format!("review_{capability}")
}

fn conditional_profile_spec(
    capability: &str,
) -> Option<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    match capability {
        "programme_planning" => Some((
            "planning_engineer",
            "Planning Engineer",
            "Planning Engineer",
            "Develop the evidence-linked Tender programme, sequencing, milestones, and schedule risk analysis.",
            "tender_programme",
        )),
        "contracts_review" => Some((
            "contracts_specialist",
            "Contracts Specialist",
            "Senior Contracts Engineer",
            "Analyze contractual obligations, qualifications, exceptions, and allocation of Tender risk.",
            "contracts_analysis",
        )),
        "procurement_analysis" => Some((
            "procurement_specialist",
            "Procurement Specialist",
            "Senior Procurement Engineer",
            "Develop supplier, quotation, lead-time, and procurement coverage for the Tender.",
            "procurement_analysis",
        )),
        "technical_review" => Some((
            "technical_specialist",
            "Technical Specialist",
            "Senior Technical Engineer",
            "Analyze the verified technical scope and produce evidence-linked technical Tender outputs.",
            "technical_analysis",
        )),
        _ => None,
    }
}

fn validate_revision_actions(actions: &[WorkPlanRevisionAction]) -> Result<(), TenderCommandError> {
    if actions.is_empty() || actions.len() > 32 {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    for action in actions {
        match action {
            WorkPlanRevisionAction::RebasePackageBasis => {
                if actions.len() != 1 {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            WorkPlanRevisionAction::AddProfile {
                archetype,
                identity,
            } => {
                validate_text(archetype, 100)?;
                validate_text(identity, 200)?;
                if archetype_profile_spec(archetype).is_none() {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            WorkPlanRevisionAction::RemoveProfile { profile_id }
            | WorkPlanRevisionAction::RenameProfile { profile_id, .. }
            | WorkPlanRevisionAction::AdjustProfile { profile_id, .. } => {
                validate_identifier(profile_id)?;
            }
            WorkPlanRevisionAction::SplitProfile {
                profile_id,
                identities,
            } => {
                validate_identifier(profile_id)?;
                if identities.len() < 2 || identities.len() > 8 {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                for identity in identities {
                    validate_text(identity, 200)?;
                }
            }
            WorkPlanRevisionAction::CombineProfiles {
                profile_ids,
                identity,
            } => {
                if profile_ids.len() < 2 || profile_ids.len() > 8 {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                let unique = profile_ids.iter().collect::<std::collections::HashSet<_>>();
                if unique.len() != profile_ids.len() {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                for profile_id in profile_ids {
                    validate_identifier(profile_id)?;
                }
                validate_text(identity, 200)?;
            }
        }
        if let WorkPlanRevisionAction::RenameProfile { identity, .. } = action {
            validate_text(identity, 200)?;
        }
        if let WorkPlanRevisionAction::AdjustProfile {
            objective,
            behavior,
            skepticism,
            risk_tolerance,
            resource_budget,
            ..
        } = action
        {
            for value in [objective, behavior, skepticism, risk_tolerance] {
                validate_text(value, 4_000)?;
            }
            if resource_budget.provider_turns != 1
                || resource_budget.duration_seconds == 0
                || resource_budget.duration_seconds > 600
                || resource_budget.output_bytes == 0
                || resource_budget.output_bytes > 1024 * 1024
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
    }
    Ok(())
}

fn validate_revision_compatibility(
    profiles: &[WorkPlanProfileBinding],
    actions: &[WorkPlanRevisionAction],
) -> Result<(), TenderCommandError> {
    let profiles_by_id = profiles
        .iter()
        .map(|binding| (binding.profile.profile_id.as_str(), binding))
        .collect::<std::collections::HashMap<_, _>>();
    let mut targeted = std::collections::HashSet::new();
    let mut projected_count = profiles.len();
    for action in actions {
        match action {
            WorkPlanRevisionAction::RebasePackageBasis => {
                if actions.len() != 1 {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            WorkPlanRevisionAction::AddProfile { archetype, .. } => {
                let additional_profiles = archetype_profile_spec(archetype)
                    .map(|(_, _, capabilities, _, _)| {
                        if capabilities
                            .iter()
                            .any(|capability| review_profile_spec(capability).is_some())
                        {
                            2
                        } else {
                            1
                        }
                    })
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                projected_count = projected_count
                    .checked_add(additional_profiles)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            }
            WorkPlanRevisionAction::RemoveProfile { profile_id } => {
                if !profiles_by_id.contains_key(profile_id.as_str())
                    || !targeted.insert(profile_id.as_str())
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                projected_count = projected_count
                    .checked_sub(1)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            }
            WorkPlanRevisionAction::RenameProfile { profile_id, .. }
            | WorkPlanRevisionAction::AdjustProfile { profile_id, .. } => {
                if !profiles_by_id.contains_key(profile_id.as_str())
                    || !targeted.insert(profile_id.as_str())
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            WorkPlanRevisionAction::SplitProfile {
                profile_id,
                identities,
            } => {
                let source = profiles_by_id
                    .get(profile_id.as_str())
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                if !targeted.insert(profile_id.as_str())
                    || source.profile.capabilities.len() != identities.len()
                    || source
                        .profile
                        .capabilities
                        .iter()
                        .any(|capability| capability == "independent_review")
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                projected_count = projected_count
                    .checked_sub(1)
                    .and_then(|count| count.checked_add(identities.len()))
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            }
            WorkPlanRevisionAction::CombineProfiles { profile_ids, .. } => {
                let selected = profile_ids
                    .iter()
                    .map(|profile_id| {
                        if !targeted.insert(profile_id.as_str()) {
                            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                        }
                        profiles_by_id
                            .get(profile_id.as_str())
                            .copied()
                            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let archetype = &selected[0].archetype;
                if selected.iter().any(|binding| {
                    binding.archetype != *archetype
                        || binding
                            .profile
                            .capabilities
                            .iter()
                            .any(|capability| capability == "independent_review")
                }) {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                projected_count = projected_count
                    .checked_sub(selected.len())
                    .and_then(|count| count.checked_add(1))
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            }
        }
        if projected_count == 0 || projected_count > MAX_PLAN_PROFILES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    Ok(())
}

fn apply_revision_actions(
    transaction: &rusqlite::Transaction<'_>,
    profiles: &mut Vec<WorkPlanProfileBinding>,
    actions: &[WorkPlanRevisionAction],
    created_at: &str,
) -> Result<(), TenderCommandError> {
    for action in actions {
        match action {
            WorkPlanRevisionAction::RebasePackageBasis => {}
            WorkPlanRevisionAction::AddProfile {
                archetype,
                identity,
            } => {
                let (default_identity, profession, capabilities, objective, scope) =
                    archetype_profile_spec(archetype)
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                let profile_id = random_identifier(transaction)?;
                let profile = production_profile(
                    profile_id,
                    1,
                    if identity.trim().is_empty() {
                        default_identity
                    } else {
                        identity.trim()
                    },
                    profession,
                    capabilities,
                    objective,
                    scope,
                );
                let reviewed_capability = profile
                    .capabilities
                    .iter()
                    .find(|capability| review_profile_spec(capability).is_some())
                    .cloned();
                profiles.push(insert_new_proposed_profile(
                    transaction,
                    archetype,
                    profile,
                    created_at,
                )?);
                if let Some(capability) = reviewed_capability {
                    let required_review_capability = review_capability(&capability);
                    if !profiles.iter().any(|binding| {
                        binding
                            .profile
                            .capabilities
                            .iter()
                            .any(|candidate| candidate == &required_review_capability)
                    }) {
                        profiles.push(compose_specialist_reviewer(
                            transaction,
                            &capability,
                            created_at,
                        )?);
                    }
                }
            }
            WorkPlanRevisionAction::RemoveProfile { profile_id } => {
                let prior = profiles.len();
                profiles.retain(|binding| binding.profile.profile_id != *profile_id);
                if profiles.len() == prior {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                retire_profile_head(transaction, profile_id)?;
            }
            WorkPlanRevisionAction::RenameProfile {
                profile_id,
                identity,
            } => {
                let binding = profiles
                    .iter_mut()
                    .find(|binding| binding.profile.profile_id == *profile_id)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                let mut profile = binding.profile.clone();
                profile.version = next_profile_version(transaction, profile_id)?;
                profile.identity = identity.trim().into();
                insert_profile_version(transaction, &profile, created_at)?;
                update_profile_head(
                    transaction,
                    profile_id,
                    profile.version,
                    AgentProfileStatus::Proposed,
                )?;
                binding.profile = profile;
            }
            WorkPlanRevisionAction::AdjustProfile {
                profile_id,
                objective,
                behavior,
                skepticism,
                risk_tolerance,
                resource_budget,
            } => {
                let binding = profiles
                    .iter_mut()
                    .find(|binding| binding.profile.profile_id == *profile_id)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                let mut profile = binding.profile.clone();
                profile.version = next_profile_version(transaction, profile_id)?;
                profile.objective = objective.trim().into();
                profile.behavior = behavior.trim().into();
                profile.skepticism = skepticism.trim().into();
                profile.risk_tolerance = risk_tolerance.trim().into();
                profile.instructions = format!(
                    "{} {} {}",
                    profile.objective, profile.behavior, profile.skepticism
                );
                profile.resource_budget = resource_budget.clone();
                insert_profile_version(transaction, &profile, created_at)?;
                update_profile_head(
                    transaction,
                    profile_id,
                    profile.version,
                    AgentProfileStatus::Proposed,
                )?;
                binding.profile = profile;
            }
            WorkPlanRevisionAction::SplitProfile {
                profile_id,
                identities,
            } => {
                let index = profiles
                    .iter()
                    .position(|binding| binding.profile.profile_id == *profile_id)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                let source = profiles.remove(index);
                if source.profile.capabilities.len() != identities.len()
                    || source
                        .profile
                        .capabilities
                        .iter()
                        .any(|capability| capability == "independent_review")
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                retire_profile_head(transaction, profile_id)?;
                let capabilities = source.profile.capabilities.clone();
                for (identity, capability) in identities.iter().zip(capabilities) {
                    let mut profile = source.profile.clone();
                    profile.profile_id = random_identifier(transaction)?;
                    profile.version = 1;
                    profile.identity = identity.trim().into();
                    profile.capabilities = vec![capability];
                    apply_capability_contract(&mut profile)?;
                    profiles.push(insert_new_proposed_profile(
                        transaction,
                        &source.archetype,
                        profile,
                        created_at,
                    )?);
                }
            }
            WorkPlanRevisionAction::CombineProfiles {
                profile_ids,
                identity,
            } => {
                let selected = profile_ids
                    .iter()
                    .map(|profile_id| {
                        profiles
                            .iter()
                            .find(|binding| binding.profile.profile_id == *profile_id)
                            .cloned()
                            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let archetype = selected
                    .first()
                    .map(|binding| binding.archetype.clone())
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                if selected
                    .iter()
                    .any(|binding| binding.archetype != archetype)
                    || selected.iter().any(|binding| {
                        binding
                            .profile
                            .capabilities
                            .iter()
                            .any(|capability| capability == "independent_review")
                    })
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                let mut profile = selected[0].profile.clone();
                profile.profile_id = random_identifier(transaction)?;
                profile.version = 1;
                profile.identity = identity.trim().into();
                profile.capabilities = selected
                    .iter()
                    .flat_map(|binding| binding.profile.capabilities.iter().cloned())
                    .collect();
                profile.capabilities.sort();
                profile.capabilities.dedup();
                apply_capability_contract(&mut profile)?;
                profiles.retain(|binding| !profile_ids.contains(&binding.profile.profile_id));
                for profile_id in profile_ids {
                    retire_profile_head(transaction, profile_id)?;
                }
                profiles.push(insert_new_proposed_profile(
                    transaction,
                    &archetype,
                    profile,
                    created_at,
                )?);
            }
        }
    }
    Ok(())
}

fn retire_profile_head(
    transaction: &rusqlite::Transaction<'_>,
    profile_id: &str,
) -> Result<(), TenderCommandError> {
    if transaction
        .execute(
            "UPDATE agent_profile_heads SET status = 'retired' WHERE profile_id = ?1",
            [profile_id],
        )
        .map_err(sql_error)?
        == 1
    {
        Ok(())
    } else {
        Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed))
    }
}

fn next_profile_version(
    transaction: &rusqlite::Transaction<'_>,
    profile_id: &str,
) -> Result<u32, TenderCommandError> {
    transaction
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM agent_profile_versions
             WHERE profile_id = ?1",
            [profile_id],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn insert_new_proposed_profile(
    transaction: &rusqlite::Transaction<'_>,
    archetype: &str,
    profile: AgentProfileVersionView,
    created_at: &str,
) -> Result<WorkPlanProfileBinding, TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO agent_profiles (profile_id, stable_identity, created_at)
             VALUES (?1, ?2, ?3)",
            params![
                profile.profile_id,
                format!("quantix.production.{}", profile.profile_id),
                created_at
            ],
        )
        .map_err(sql_error)?;
    insert_profile_version(transaction, &profile, created_at)?;
    transaction
        .execute(
            "INSERT INTO agent_profile_heads (profile_id, current_version, status)
             VALUES (?1, ?2, 'proposed')",
            params![profile.profile_id, profile.version],
        )
        .map_err(sql_error)?;
    Ok(WorkPlanProfileBinding {
        archetype: archetype.into(),
        status: AgentProfileStatus::Proposed,
        profile,
    })
}

fn archetype_profile_spec(
    archetype: &str,
) -> Option<(
    &'static str,
    &'static str,
    Vec<&'static str>,
    &'static str,
    &'static str,
)> {
    match archetype {
        "tender_office_coordinator" => Some((
            "Tender Office Coordinator",
            "Tender Coordination Engineer",
            vec!["tender_coordination", "query_rfi_control"],
            "Coordinate approved Tender production work, dependencies, milestones, and escalation without making Tendering Manager decisions.",
            "tender_coordination",
        )),
        "tender_coordinator" => Some((
            "Tender Coordinator",
            "Tender Coordination Engineer",
            vec!["tender_coordination"],
            "Coordinate approved Tender production work, dependencies, milestones, and escalation without making Tendering Manager decisions.",
            "tender_coordination",
        )),
        "query_rfi_controller" => Some((
            "Query and RFI Controller",
            "Tender Coordination Engineer",
            vec!["query_rfi_control"],
            "Control evidence-linked Tender Queries and RFIs, their dependencies, deadlines, responses, and unresolved gaps.",
            "tender_queries",
        )),
        "document_controller" => Some((
            "Document Controller",
            "Tender Document Controller",
            vec!["document_control"],
            "Control exact sources, registers, relationships, and production document versions.",
            "tender_sources",
        )),
        "tender_analyst" => Some((
            "Tender Analyst",
            "Tender Engineer",
            vec!["tender_analysis"],
            "Maintain the evidence-linked Tender analysis basis and coordinate unresolved assumptions and requirements.",
            "tender_analysis",
        )),
        "independent_reviewer" => Some((
            "Independent Reviewer",
            "Tender Assurance Engineer",
            vec![
                "independent_review",
                "review_tender_coordination",
                "review_document_control",
                "review_tender_analysis",
                "review_query_rfi_control",
            ],
            "Independently review exact production outputs without editing or approving them.",
            "tender_assurance",
        )),
        "cost_estimator" => Some((
            "Cost Estimator",
            "Senior Construction Cost Estimator",
            vec!["cost_estimation"],
            "Develop evidence-linked quantities, rate build-ups, costs, risk provisions, and pricing scenarios through controlled calculations.",
            "commercial_estimate",
        )),
        "independent_cost_reviewer" => Some((
            "Independent Cost Reviewer",
            "Senior Cost Assurance Surveyor",
            vec!["review_cost_estimation"],
            "Independently review exact estimating outputs, quantities, rates, allowances, and commercial uncertainty without editing them.",
            "commercial_estimate_review",
        )),
        "planning_engineer" => Some((
            "Planning Engineer",
            "Planning Engineer",
            vec!["programme_planning"],
            "Develop the evidence-linked Tender programme, sequencing, milestones, and schedule risk analysis.",
            "tender_programme",
        )),
        "independent_planning_reviewer" => Some((
            "Independent Planning Reviewer",
            "Senior Schedule Assurance Engineer",
            vec!["review_programme_planning"],
            "Independently review exact programme logic, milestones, float, constraints, and schedule risks without editing them.",
            "tender_programme_review",
        )),
        "contracts_specialist" => Some((
            "Contracts Specialist",
            "Senior Contracts Engineer",
            vec!["contracts_review"],
            "Analyze contractual obligations, qualifications, exceptions, and allocation of Tender risk.",
            "contracts_analysis",
        )),
        "independent_contracts_reviewer" => Some((
            "Independent Contracts Reviewer",
            "Senior Contracts Assurance Engineer",
            vec!["review_contracts_review"],
            "Independently review exact contractual analysis, qualifications, exceptions, and risk allocation without editing them.",
            "contracts_analysis_review",
        )),
        "procurement_specialist" => Some((
            "Procurement Specialist",
            "Senior Procurement Engineer",
            vec!["procurement_analysis"],
            "Develop supplier, quotation, lead-time, and procurement coverage for the Tender.",
            "procurement_analysis",
        )),
        "independent_procurement_reviewer" => Some((
            "Independent Procurement Reviewer",
            "Senior Procurement Assurance Engineer",
            vec!["review_procurement_analysis"],
            "Independently review exact supplier, quotation, lead-time, and procurement analysis without editing it.",
            "procurement_analysis_review",
        )),
        "technical_specialist" => Some((
            "Technical Specialist",
            "Senior Technical Engineer",
            vec!["technical_review"],
            "Analyze the verified technical scope and produce evidence-linked technical Tender outputs.",
            "technical_analysis",
        )),
        "independent_technical_reviewer" => Some((
            "Independent Technical Reviewer",
            "Senior Technical Assurance Engineer",
            vec!["review_technical_review"],
            "Independently review exact technical Tender outputs and their evidence without editing them.",
            "technical_analysis_review",
        )),
        _ => None,
    }
}

fn validate_identifier(value: &str) -> Result<(), TenderCommandError> {
    if value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }
}

fn next_work_plan_version(base_version: u32) -> Result<u32, TenderCommandError> {
    base_version
        .checked_add(1)
        .filter(|version| *version as usize <= MAX_PLAN_VERSIONS)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
}

fn validate_text(value: &str, maximum: usize) -> Result<(), TenderCommandError> {
    if value.trim().is_empty() || value.len() > maximum {
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    } else {
        Ok(())
    }
}

fn capability_scope(capability: &str) -> Option<String> {
    let scope = match capability {
        "tender_coordination" => "tender_coordination",
        "query_rfi_control" => "tender_queries",
        "document_control" => "tender_sources",
        "tender_analysis" => "tender_analysis",
        "cost_estimation" => "commercial_estimate",
        "independent_review" => "tender_assurance",
        "programme_planning" => "tender_programme",
        "contracts_review" => "contracts_analysis",
        "procurement_analysis" => "procurement_analysis",
        "technical_review" => "technical_analysis",
        value if value.starts_with("review_") => {
            return capability_scope(value.trim_start_matches("review_"))
                .map(|scope| format!("{scope}_review"));
        }
        _ => return None,
    };
    Some(scope.into())
}

fn permissions_for_capabilities(
    capabilities: &[String],
) -> Result<AgentRunPermissions, TenderCommandError> {
    let mut scopes = Vec::new();
    for capability in capabilities {
        scopes.extend(capability_read_scopes(capability)?);
    }
    scopes.sort();
    scopes.dedup();
    let mut data_classifications = scopes
        .iter()
        .map(|scope| {
            if scope.starts_with("commercial_estimate") {
                DataClassification::Sensitive
            } else {
                DataClassification::TenderInternal
            }
        })
        .collect::<Vec<_>>();
    data_classifications.sort_by_key(|classification| format!("{classification:?}"));
    data_classifications.dedup();
    let mut allowed_actions = capabilities
        .iter()
        .map(|capability| {
            capability_scope(capability)
                .map(|scope| format!("produce_{scope}_output"))
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
        })
        .collect::<Result<Vec<_>, _>>()?;
    allowed_actions.sort();
    allowed_actions.dedup();
    Ok(AgentRunPermissions {
        data_scopes: scopes,
        data_classifications,
        allowed_actions,
        allowed_tools: Vec::new(),
        network_allowed: false,
        workspace_write_allowed: true,
    })
}

fn capability_read_scopes(capability: &str) -> Result<Vec<String>, TenderCommandError> {
    let own = capability_scope(capability)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let mut scopes = vec![own];
    if let Some(reviewed) = capability.strip_prefix("review_") {
        scopes.extend(capability_read_scopes(reviewed)?);
    } else {
        match capability {
            "tender_analysis" | "query_rfi_control" => scopes.push("tender_sources".into()),
            "cost_estimation"
            | "programme_planning"
            | "contracts_review"
            | "procurement_analysis"
            | "technical_review" => {
                scopes.push("tender_analysis".into());
                scopes.push("tender_sources".into());
            }
            "independent_review" => {
                scopes.push("commercial_estimate".into());
                scopes.push("tender_analysis".into());
                scopes.push("tender_sources".into());
            }
            _ => {}
        }
    }
    scopes.sort();
    scopes.dedup();
    Ok(scopes)
}

fn apply_capability_contract(
    profile: &mut AgentProfileVersionView,
) -> Result<(), TenderCommandError> {
    profile.permissions = permissions_for_capabilities(&profile.capabilities)?;
    Ok(())
}

fn profile_permissions_are_coherent(profile: &AgentProfileVersionView) -> bool {
    let known = profile
        .capabilities
        .iter()
        .filter(|capability| capability_scope(capability).is_some())
        .count();
    known == 0
        || (known == profile.capabilities.len()
            && permissions_for_capabilities(&profile.capabilities)
                .is_ok_and(|permissions| permissions == profile.permissions))
}

fn production_profile(
    profile_id: String,
    version: u32,
    identity: &str,
    profession: &str,
    capabilities: Vec<&str>,
    objective: &str,
    _scope: &str,
) -> AgentProfileVersionView {
    let behavior = "Work only within the approved Work Plan, keep exact inputs and uncertainty visible, and escalate blocked decisions.";
    let skepticism = "Challenge unsupported claims, silent scope changes, optimistic assumptions, and missing independent review.";
    let risk_tolerance = "Low tolerance for unverified commitments, unbounded work, or irreversible external effects.";
    let mut profile = AgentProfileVersionView {
        profile_id,
        version,
        identity: identity.into(),
        profession: profession.into(),
        seniority: "Senior".into(),
        capabilities: capabilities.into_iter().map(str::to_owned).collect(),
        objective: objective.into(),
        behavior: behavior.into(),
        skepticism: skepticism.into(),
        risk_tolerance: risk_tolerance.into(),
        instructions: format!("{objective} {behavior} {skepticism}"),
        output_contract_json: production_output_contract(),
        review_policy: "Material output requires review by a separate Active Agent Profile with the matching approved review Capability; the author cannot edit or close the review.".into(),
        permissions: AgentRunPermissions {
            data_scopes: Vec::new(),
            data_classifications: Vec::new(),
            allowed_actions: Vec::new(),
            allowed_tools: Vec::new(),
            network_allowed: false,
            workspace_write_allowed: true,
        },
        prohibited_actions: vec![
            "approve_tender_decision".into(),
            "mutate_tender_store_directly".into(),
            "perform_external_action".into(),
            "access_secret_data".into(),
            "use_unrestricted_shell".into(),
        ],
        resource_budget: AgentResourceBudget {
            provider_turns: 1,
            duration_seconds: 120,
            output_bytes: 256 * 1024,
        },
    };
    apply_capability_contract(&mut profile).expect("catalogued production capability contract");
    profile
}

fn production_output_contract() -> String {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "evidence_references": {
                "items": { "maxLength": 400, "minLength": 1, "type": "string" },
                "maxItems": 256,
                "minItems": 1,
                "type": "array"
            },
            "gaps": {
                "items": { "maxLength": 4000, "minLength": 1, "type": "string" },
                "maxItems": 64,
                "type": "array"
            },
            "query_proposals": {
                "items": { "type": "object" },
                "maxItems": 16,
                "type": "array"
            },
            "query_treatment_applications": {
                "items": { "type": "object" },
                "maxItems": 64,
                "type": "array"
            },
            "query_updates": {
                "items": { "type": "object" },
                "maxItems": 16,
                "type": "array"
            },
            "summary": { "maxLength": 4000, "minLength": 1, "type": "string" }
        },
        "required": ["summary", "evidence_references", "gaps"],
        "type": "object"
    }))
    .expect("static production output contract")
}

fn capability_gaps(
    demands: &[CapabilityDemand],
    profiles: &[WorkPlanProfileBinding],
) -> Vec<WorkPlanCapabilityGap> {
    let mut required = vec![
        ("tender_coordination".to_owned(), true),
        ("document_control".to_owned(), true),
        ("tender_analysis".to_owned(), true),
        ("independent_review".to_owned(), true),
        ("cost_estimation".to_owned(), true),
        ("query_rfi_control".to_owned(), true),
    ];
    required.extend(
        demands
            .iter()
            .map(|demand| (demand.capability.clone(), demand.supported)),
    );
    let review_requirements = required
        .iter()
        .filter(|(capability, supported)| {
            *supported
                && capability_requires_review(capability)
                && profiles.iter().any(|binding| {
                    binding
                        .profile
                        .capabilities
                        .iter()
                        .any(|candidate| candidate == capability)
                })
        })
        .map(|(capability, _)| (review_capability(capability), true))
        .collect::<Vec<_>>();
    required.extend(review_requirements);
    required.sort();
    required.dedup();
    required
        .into_iter()
        .filter(|(capability, supported)| {
            !*supported
                || !profiles.iter().any(|binding| {
                    binding
                        .profile
                        .capabilities
                        .iter()
                        .any(|candidate| candidate == capability)
                })
        })
        .map(|(capability, supported)| WorkPlanCapabilityGap {
            reason: if supported {
                "No qualified separate Agent Profile is assigned to this Capability Demand.".into()
            } else {
                "The current Capability Catalogue does not support this Capability Demand.".into()
            },
            affected_work: vec![capability.clone()],
            capability,
        })
        .collect()
}

fn capability_requires_review(capability: &str) -> bool {
    capability != "independent_review" && !capability.starts_with("review_")
}

fn compose_workstreams(
    profiles: &[WorkPlanProfileBinding],
    deadlines: &[String],
) -> Result<Vec<WorkPlanWorkstream>, TenderCommandError> {
    let definitions = [
        (
            "tender_coordination",
            "Tender Coordination",
            "tender_coordination",
            Vec::new(),
        ),
        (
            "document_control",
            "Document Control",
            "document_control",
            Vec::new(),
        ),
        (
            "tender_analysis",
            "Tender Analysis",
            "tender_analysis",
            vec!["document_control"],
        ),
        (
            "cost_estimation",
            "Cost Estimating",
            "cost_estimation",
            vec!["tender_analysis"],
        ),
        (
            "query_rfi_control",
            "Query and RFI Control",
            "query_rfi_control",
            vec!["document_control"],
        ),
        (
            "independent_assurance",
            "Independent Assurance",
            "independent_review",
            vec!["tender_analysis", "cost_estimation"],
        ),
    ];
    let mut workstreams = definitions
        .into_iter()
        .map(|(key, name, capability, dependencies)| {
            let profile = profiles.iter().find(|binding| {
                binding
                    .profile
                    .capabilities
                    .iter()
                    .any(|candidate| candidate == capability)
            });
            Ok(WorkPlanWorkstream {
                workstream_key: key.into(),
                name: name.into(),
                capability: capability.into(),
                accountable_profile_id: profile.map(|binding| binding.profile.profile_id.clone()),
                dependencies: dependencies.into_iter().map(str::to_owned).collect(),
                deadlines: deadlines.to_vec(),
                milestones: vec![format!("{key}_ready")],
                resource_budget: profile
                    .map(|binding| binding.profile.resource_budget.clone())
                    .unwrap_or(AgentResourceBudget {
                        provider_turns: 0,
                        duration_seconds: 0,
                        output_bytes: 0,
                    }),
            })
        })
        .collect::<Result<Vec<_>, TenderCommandError>>()?;
    for binding in profiles {
        for capability in &binding.profile.capabilities {
            if workstreams
                .iter()
                .any(|workstream| workstream.capability == *capability)
                || capability == "independent_review"
                || capability.starts_with("review_")
            {
                continue;
            }
            workstreams.push(WorkPlanWorkstream {
                workstream_key: capability.clone(),
                name: binding.profile.identity.clone(),
                capability: capability.clone(),
                accountable_profile_id: Some(binding.profile.profile_id.clone()),
                dependencies: vec!["tender_analysis".into()],
                deadlines: deadlines.to_vec(),
                milestones: vec![format!("{capability}_ready")],
                resource_budget: binding.profile.resource_budget.clone(),
            });
        }
    }
    Ok(workstreams)
}

fn compose_tasks(
    tender_id: &TenderId,
    tender_revision: u32,
    package: &BidDecisionPackageInspection,
    profiles: &[WorkPlanProfileBinding],
    workstreams: &[WorkPlanWorkstream],
    deadlines: &[String],
) -> Result<Vec<WorkPlanTask>, TenderCommandError> {
    let deadline = deadlines
        .first()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    workstreams
        .iter()
        .filter_map(|workstream| {
            let profile_id = workstream.accountable_profile_id.as_ref()?;
            let owner = profiles
                .iter()
                .find(|binding| binding.profile.profile_id == *profile_id)?;
            let requires_review = workstream.capability != "independent_review";
            let required_review_capability = review_capability(&workstream.capability);
            let reviewer = requires_review.then(|| {
                profiles.iter().find(|binding| {
                    binding
                        .profile
                        .capabilities
                        .iter()
                        .any(|capability| capability == &required_review_capability)
                })
            });
            Some(Ok(WorkPlanTask {
                task_key: format!("{}_production", workstream.workstream_key),
                workstream_key: workstream.workstream_key.clone(),
                profile_id: owner.profile.profile_id.clone(),
                profile_version: owner.profile.version,
                objective: owner.profile.objective.clone(),
                exact_inputs: vec![
                    AgentTaskInputReference {
                        kind: "tender_revision".into(),
                        reference: tender_id.as_str().into(),
                        version: tender_revision,
                    },
                    AgentTaskInputReference {
                        kind: "bid_decision_package".into(),
                        reference: package.package_id.clone(),
                        version: package.version,
                    },
                ],
                dependencies: workstream
                    .dependencies
                    .iter()
                    .map(|dependency| format!("{dependency}_production"))
                    .collect(),
                deadline: deadline.clone(),
                milestone: format!("{}_ready", workstream.workstream_key),
                review_profile_id: reviewer
                    .flatten()
                    .map(|candidate| candidate.profile.profile_id.clone()),
                review_profile_version: reviewer
                    .flatten()
                    .map(|candidate| candidate.profile.version),
                major_finding_policy: major_finding_policy(&workstream.capability),
                permissions: owner.profile.permissions.clone(),
                resource_budget: owner.profile.resource_budget.clone(),
                output_contract_json: owner.profile.output_contract_json.clone(),
            }))
        })
        .collect()
}

fn validate_plan_shape(
    profiles: &[WorkPlanProfileBinding],
    workstreams: &[WorkPlanWorkstream],
    tasks: &[WorkPlanTask],
    query_bindings: &[TenderRecordVersionReference],
    gaps: &[WorkPlanCapabilityGap],
) -> Result<(), TenderCommandError> {
    if profiles.is_empty()
        || profiles.len() > MAX_PLAN_PROFILES
        || workstreams.is_empty()
        || workstreams.len() > MAX_PLAN_WORKSTREAMS
        || tasks.is_empty()
        || tasks.len() > MAX_PLAN_TASKS
        || query_bindings.len() > MAX_PLAN_QUERIES
        || gaps.len() > MAX_PLAN_GAPS
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let profile_ids = profiles
        .iter()
        .map(|binding| binding.profile.profile_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    if profile_ids.len() != profiles.len() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let queries = query_bindings
        .iter()
        .map(|reference| (reference.record_id.as_str(), reference.version))
        .collect::<std::collections::HashSet<_>>();
    if queries.len() != query_bindings.len() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut deadlines = std::collections::HashSet::new();
    for workstream in workstreams {
        if workstream.deadlines.is_empty()
            || workstream.deadlines.len() > MAX_PLAN_QUERIES
            || workstream
                .deadlines
                .iter()
                .any(|deadline| validate_text(deadline, 200).is_err())
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        deadlines.extend(workstream.deadlines.iter().map(String::as_str));
        if deadlines.len() > MAX_PLAN_QUERIES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    if tasks
        .iter()
        .any(|task| !deadlines.contains(task.deadline.as_str()))
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut workload = std::collections::HashMap::new();
    for task in tasks {
        let count = workload.entry(task.profile_id.as_str()).or_insert(0_usize);
        *count = count
            .checked_add(1)
            .filter(|count| *count <= MAX_TASKS_PER_PROFILE)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if let Some(reviewer) = task.review_profile_id.as_deref() {
            let count = workload.entry(reviewer).or_insert(0_usize);
            *count = count
                .checked_add(1)
                .filter(|count| *count <= MAX_TASKS_PER_PROFILE)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        }
    }
    Ok(())
}

fn approval_invariants_hold(plan: &WorkPlanProposalInspection) -> bool {
    let profiles = plan
        .profiles
        .iter()
        .map(|binding| (binding.profile.profile_id.as_str(), binding))
        .collect::<std::collections::HashMap<_, _>>();
    if profiles.len() != plan.profiles.len()
        || plan.profiles.iter().any(|binding| {
            binding.status != AgentProfileStatus::Proposed
                || binding.archetype.trim().is_empty()
                || binding.profile.version == 0
                || binding.profile.identity.trim().is_empty()
                || binding.profile.profession.trim().is_empty()
                || binding.profile.seniority.trim().is_empty()
                || binding.profile.capabilities.is_empty()
                || binding.profile.objective.trim().is_empty()
                || binding.profile.behavior.trim().is_empty()
                || binding.profile.skepticism.trim().is_empty()
                || binding.profile.risk_tolerance.trim().is_empty()
                || binding.profile.instructions.trim().is_empty()
                || binding.profile.review_policy.trim().is_empty()
                || binding.profile.permissions.data_scopes.is_empty()
                || binding.profile.permissions.data_classifications.is_empty()
                || binding.profile.permissions.allowed_actions.is_empty()
                || !profile_permissions_are_coherent(&binding.profile)
                || binding.profile.prohibited_actions.is_empty()
                || binding.profile.resource_budget.provider_turns == 0
                || binding.profile.resource_budget.duration_seconds == 0
                || binding.profile.resource_budget.output_bytes == 0
                || parse_canonical_json::<Value>(&binding.profile.output_contract_json).is_err()
        })
    {
        return false;
    }

    let reviewers = plan
        .profiles
        .iter()
        .filter(|binding| {
            binding
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == "independent_review")
        })
        .collect::<Vec<_>>();
    if reviewers.len() != 1
        || plan.profiles.iter().any(|binding| {
            binding
                .profile
                .capabilities
                .iter()
                .any(|capability| capability.starts_with("review_"))
                && binding
                    .profile
                    .capabilities
                    .iter()
                    .any(|capability| capability_requires_review(capability))
        })
    {
        return false;
    }

    let workstreams = plan
        .workstreams
        .iter()
        .map(|workstream| (workstream.workstream_key.as_str(), workstream))
        .collect::<std::collections::HashMap<_, _>>();
    if workstreams.len() != plan.workstreams.len()
        || plan.workstreams.iter().any(|workstream| {
            workstream.workstream_key.trim().is_empty()
                || workstream.name.trim().is_empty()
                || workstream.capability.trim().is_empty()
                || workstream.deadlines.is_empty()
                || workstream.milestones.is_empty()
                || workstream
                    .accountable_profile_id
                    .as_ref()
                    .is_none_or(|profile_id| !profiles.contains_key(profile_id.as_str()))
        })
    {
        return false;
    }

    let task_keys = plan
        .tasks
        .iter()
        .map(|task| task.task_key.as_str())
        .collect::<std::collections::HashSet<_>>();
    let valid_tasks = task_keys.len() == plan.tasks.len()
        && plan.tasks.iter().all(|task| {
            let Some(author) = profiles.get(task.profile_id.as_str()) else {
                return false;
            };
            let Some(workstream) = workstreams.get(task.workstream_key.as_str()) else {
                return false;
            };
            task.profile_version == author.profile.version
                && task.major_finding_policy == major_finding_policy(&workstream.capability)
                && !task.objective.trim().is_empty()
                && !task.exact_inputs.is_empty()
                && !task.deadline.trim().is_empty()
                && !task.milestone.trim().is_empty()
                && parse_canonical_json::<Value>(&task.output_contract_json).is_ok()
                && if workstream.capability == "independent_review" {
                    author
                        .profile
                        .capabilities
                        .iter()
                        .any(|capability| capability == "independent_review")
                        && task.review_profile_id.is_none()
                        && task.review_profile_version.is_none()
                } else {
                    let Some(reviewer_id) = task.review_profile_id.as_deref() else {
                        return false;
                    };
                    let Some(reviewer) = profiles.get(reviewer_id) else {
                        return false;
                    };
                    reviewer.profile.profile_id != author.profile.profile_id
                        && task.review_profile_version == Some(reviewer.profile.version)
                        && reviewer.profile.capabilities.iter().any(|capability| {
                            capability == &review_capability(&workstream.capability)
                        })
                }
        });
    if !valid_tasks {
        return false;
    }
    plan.profiles.iter().all(|binding| {
        plan.tasks.iter().any(|task| {
            task.profile_id == binding.profile.profile_id
                || task.review_profile_id.as_deref() == Some(binding.profile.profile_id.as_str())
        })
    })
}

fn major_finding_policy(capability: &str) -> MajorFindingPolicy {
    if matches!(capability, "document_control" | "cost_estimation") {
        MajorFindingPolicy::EngineerExceptionAllowed
    } else {
        MajorFindingPolicy::RemediationRequired
    }
}

fn profile_shape_is_valid(profile: &AgentProfileVersionView, _status: AgentProfileStatus) -> bool {
    let capabilities = profile
        .capabilities
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let scopes = profile
        .permissions
        .data_scopes
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let prohibited = profile
        .prohibited_actions
        .iter()
        .collect::<std::collections::HashSet<_>>();
    profile.profile_id.len() == 32
        && profile.version > 0
        && validate_text(&profile.identity, 200).is_ok()
        && validate_text(&profile.profession, 200).is_ok()
        && validate_text(&profile.seniority, 100).is_ok()
        && !profile.capabilities.is_empty()
        && profile.capabilities.len() <= 64
        && capabilities.len() == profile.capabilities.len()
        && profile
            .capabilities
            .iter()
            .all(|capability| validate_text(capability, 100).is_ok())
        && validate_text(&profile.objective, 4_000).is_ok()
        && validate_text(&profile.behavior, 4_000).is_ok()
        && validate_text(&profile.skepticism, 4_000).is_ok()
        && validate_text(&profile.risk_tolerance, 4_000).is_ok()
        && validate_text(&profile.instructions, 16_000).is_ok()
        && validate_text(&profile.review_policy, 4_000).is_ok()
        && parse_canonical_json::<Value>(&profile.output_contract_json).is_ok()
        && !profile.permissions.data_scopes.is_empty()
        && profile.permissions.data_scopes.len() <= 64
        && scopes.len() == profile.permissions.data_scopes.len()
        && !profile.permissions.data_classifications.is_empty()
        && profile.permissions.data_classifications.len() <= 8
        && !profile.permissions.allowed_actions.is_empty()
        && profile.permissions.allowed_actions.len() <= 64
        && profile_permissions_are_coherent(profile)
        && profile.permissions.allowed_tools.len() <= 64
        && !profile.prohibited_actions.is_empty()
        && profile.prohibited_actions.len() <= 64
        && prohibited.len() == profile.prohibited_actions.len()
        && profile.resource_budget.provider_turns > 0
        && profile.resource_budget.provider_turns <= 8
        && profile.resource_budget.duration_seconds > 0
        && profile.resource_budget.duration_seconds <= 600
        && profile.resource_budget.output_bytes > 0
        && profile.resource_budget.output_bytes <= 1024 * 1024
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
}

fn parse_canonical_json<T>(value: &str) -> Result<T, TenderCommandError>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let parsed = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::{next_work_plan_version, MAX_PLAN_VERSIONS};
    use crate::TenderErrorCode;

    #[test]
    fn work_plan_version_limit_rejects_the_first_uninspectable_successor() {
        assert_eq!(
            next_work_plan_version((MAX_PLAN_VERSIONS - 1) as u32).expect("last valid version"),
            MAX_PLAN_VERSIONS as u32
        );
        assert_eq!(
            next_work_plan_version(MAX_PLAN_VERSIONS as u32)
                .expect_err("version beyond integrity bound")
                .code,
            TenderErrorCode::InvalidCommand
        );
    }
}
