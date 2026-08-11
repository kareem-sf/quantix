use std::collections::{BTreeMap, BTreeSet};

use garde::Validate;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::agent_runtime::VerificationStatus;

use super::production_scheduler::{
    canonical_coordination_observation_value, coordination_task_assignment_key,
    record_coordination_observation,
};
use super::{
    append_audit_event_with_sequence, lock_mutex_with_check, random_identifier, sha256_hex,
    sql_error, sqlite_timestamp, BidPackageOperationBudget, ProductionArtifactPayload,
    ProductionCoordinationObservationSubject, ProductionCoordinationObservationValue, QuantixHost,
    TenderCommandError, TenderErrorCode, TenderId, TenderLifecyclePhase, TenderRecordKind,
    TenderRecordTrustClass, TenderStore, WorkPlanTask,
};

const MAX_BASELINE_VERSIONS: u32 = 32;
const MAX_BASELINE_BINDINGS: usize = 1_024;
const MAX_BASELINE_CONTRADICTIONS: usize = 256;
const MAX_BASELINE_BLOCKERS: usize = 512;
const MAX_APPROVAL_ITEMS: usize = 32;
const MAX_PAGE_ITEMS: u32 = 4;
const MAX_PAGE_BYTES: usize = 8 * 1024 * 1024;
const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CoordinatedBidBaselineCategory {
    Technical,
    Programme,
    Procurement,
    Contractual,
    Risk,
    Query,
    Qualification,
    Exclusion,
    Submission,
    Commercial,
}

impl CoordinatedBidBaselineCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Technical => "technical",
            Self::Programme => "programme",
            Self::Procurement => "procurement",
            Self::Contractual => "contractual",
            Self::Risk => "risk",
            Self::Query => "query",
            Self::Qualification => "qualification",
            Self::Exclusion => "exclusion",
            Self::Submission => "submission",
            Self::Commercial => "commercial",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CoordinatedBidBaselineBindingKind {
    ProductionArtifactVersion,
    TenderRecordVersion,
    TenderQueryVersion,
    ExternalRfiVersion,
    PricedCostBaseline,
    ApprovedTenderPrice,
    CalculationManifest,
    CommercialStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoordinatedBidBaselineBinding {
    pub category: CoordinatedBidBaselineCategory,
    pub kind: CoordinatedBidBaselineBindingKind,
    pub reference_id: String,
    pub version: u32,
    pub manifest_sha256: String,
    pub source: String,
    pub summary: String,
    pub supporting_review_id: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CoordinatedBidBaselineContradictionCategory {
    Value,
    Date,
    Responsibility,
    Qualification,
    Exclusion,
    Calculation,
    Commitment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoordinatedBidBaselineContradiction {
    pub category: CoordinatedBidBaselineContradictionCategory,
    pub key: String,
    pub summary: String,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CoordinatedBidBaselineBlockerCode {
    ProductionTaskNotReady,
    OpenCriticalFinding,
    OpenMajorFinding,
    OpenMaterialQuery,
    StaleInput,
    UnverifiedInput,
    CapabilityGap,
    UnreconciledCalculation,
    PricedCostBaselineMissing,
    ApprovedTenderPriceMissing,
    WorkstreamEvidenceMissing,
    ContradictionOpen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoordinatedBidBaselineBlocker {
    pub code: CoordinatedBidBaselineBlockerCode,
    pub summary: String,
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CoordinatedBidBaselineDecision {
    Approve,
    Return,
    Reject,
}

impl CoordinatedBidBaselineDecision {
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
pub struct CoordinatedBidBaselineApproval {
    pub approval_id: String,
    pub baseline_id: String,
    pub baseline_version: u32,
    pub baseline_manifest_sha256: String,
    pub decision: CoordinatedBidBaselineDecision,
    pub rationale: String,
    pub conditions: Vec<String>,
    pub exceptions: Vec<String>,
    pub supporting_reviews_sha256: String,
    pub decided_by: String,
    pub acting_role: String,
    pub lifecycle_before: TenderLifecyclePhase,
    pub lifecycle_after: TenderLifecyclePhase,
    pub preceding_approval_hash: String,
    pub approval_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoordinatedBidBaseline {
    pub baseline_id: String,
    pub version: u32,
    pub tender_revision: u32,
    pub activation_id: String,
    pub plan_id: String,
    pub plan_version: u32,
    pub plan_manifest_sha256: String,
    pub coordinator_profile_id: String,
    pub coordinator_profile_version: u32,
    pub bindings: Vec<CoordinatedBidBaselineBinding>,
    pub contradictions: Vec<CoordinatedBidBaselineContradiction>,
    pub blockers: Vec<CoordinatedBidBaselineBlocker>,
    pub explanation: String,
    pub preceding_version_manifest_sha256: Option<String>,
    pub approval: Option<CoordinatedBidBaselineApproval>,
    pub current: bool,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoordinatedBidBaselinePage {
    pub items: Vec<CoordinatedBidBaseline>,
    pub next_before_version: Option<u32>,
    pub lifecycle_phase: TenderLifecyclePhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AssembleCoordinatedBidBaselineCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(range(min = 1, max = 32))]
    pub base_version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DecideCoordinatedBidBaselineCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub baseline_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(skip)]
    pub decision: CoordinatedBidBaselineDecision,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
    #[garde(length(max = 32), inner(length(bytes, min = 1, max = 1000)))]
    pub conditions: Vec<String>,
    #[garde(length(max = 32), inner(length(bytes, min = 1, max = 1000)))]
    pub exceptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectCoordinatedBidBaselinesCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(range(min = 1, max = 32))]
    pub before_version: Option<u32>,
    #[garde(range(min = 1, max = 4))]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BaselineManifest {
    schema_version: u32,
    baseline_id: String,
    version: u32,
    tender_revision: u32,
    activation_id: String,
    plan_id: String,
    plan_version: u32,
    plan_manifest_sha256: String,
    coordinator_profile_id: String,
    coordinator_profile_version: u32,
    bindings: Vec<CoordinatedBidBaselineBinding>,
    contradictions: Vec<CoordinatedBidBaselineContradiction>,
    blockers: Vec<CoordinatedBidBaselineBlocker>,
    explanation: String,
    preceding_version_manifest_sha256: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ApprovalManifest {
    schema_version: u32,
    approval_id: String,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    decision: CoordinatedBidBaselineDecision,
    rationale: String,
    conditions: Vec<String>,
    exceptions: Vec<String>,
    supporting_reviews_sha256: String,
    decided_by: String,
    acting_role: String,
    lifecycle_before: TenderLifecyclePhase,
    lifecycle_after: TenderLifecyclePhase,
    preceding_approval_hash: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BaselineSnapshot {
    tender_revision: u32,
    activation_id: String,
    plan_id: String,
    plan_version: u32,
    plan_manifest_sha256: String,
    coordinator_profile_id: String,
    coordinator_profile_version: u32,
    bindings: Vec<CoordinatedBidBaselineBinding>,
    contradictions: Vec<CoordinatedBidBaselineContradiction>,
    blockers: Vec<CoordinatedBidBaselineBlocker>,
    explanation: String,
}

impl BaselineSnapshot {
    fn is_ready(&self) -> bool {
        self.blockers.is_empty() && self.contradictions.is_empty()
    }
}

#[derive(Debug, Clone)]
struct FieldObservation {
    subject: ProductionCoordinationObservationSubject,
    scope: String,
    value: String,
    reference: String,
    keyed: bool,
}

struct SemanticObservationSet {
    references: BTreeMap<String, Vec<String>>,
    keyed_values: BTreeSet<String>,
    unkeyed_values: BTreeSet<String>,
}

impl QuantixHost {
    pub fn assemble_coordinated_bid_baseline(
        &self,
        command: AssembleCoordinatedBidBaselineCommand,
    ) -> Result<CoordinatedBidBaseline, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_baseline_denial(&tender_id, "assemble", "command_shape")?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        store.assemble_coordinated_bid_baseline(&tender_id, &command, budget)
    }

    pub fn decide_coordinated_bid_baseline(
        &self,
        mut command: DecideCoordinatedBidBaselineCommand,
    ) -> Result<CoordinatedBidBaseline, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        let semantic_shape_is_valid = !command.rationale.trim().is_empty()
            && validate_unique_texts(&command.conditions).is_ok()
            && validate_unique_texts(&command.exceptions).is_ok();
        if command.validate().is_err()
            || !valid_hash(&command.manifest_sha256)
            || !semantic_shape_is_valid
        {
            store.record_baseline_denial(&tender_id, "decide", "command_shape")?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        command.rationale = command.rationale.trim().to_owned();
        command.conditions = normalize_text_items(command.conditions);
        command.exceptions = normalize_text_items(command.exceptions);
        store.decide_coordinated_bid_baseline(&tender_id, &command, budget)
    }

    pub fn inspect_coordinated_bid_baselines(
        &self,
        command: InspectCoordinatedBidBaselinesCommand,
    ) -> Result<CoordinatedBidBaselinePage, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_coordinated_bid_baselines(command.before_version, command.limit, budget);
        result
    }
}

impl TenderStore {
    fn record_baseline_denial(
        &mut self,
        tender_id: &TenderId,
        command: &str,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "coordinated_bid_baseline_denied",
            revision,
            json!({"command": command, "reason": reason}),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(())
    }

    fn assemble_coordinated_bid_baseline(
        &mut self,
        tender_id: &TenderId,
        command: &AssembleCoordinatedBidBaselineCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<CoordinatedBidBaseline, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let lifecycle = self.lifecycle_phase()?;
        if !matches!(
            lifecycle,
            TenderLifecyclePhase::ActiveProduction | TenderLifecyclePhase::IntegratedReview
        ) {
            self.record_baseline_denial(tender_id, "assemble", "lifecycle_not_open")?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let head: Option<(String, u32, String, Option<String>)> = self
            .connection
            .query_row(
                "SELECT heads.baseline_id, heads.current_version, versions.manifest_sha256,
                        approvals.decision
                 FROM coordinated_bid_baseline_head AS heads
                 JOIN coordinated_bid_baseline_versions AS versions
                   ON versions.baseline_id = heads.baseline_id
                  AND versions.version = heads.current_version
                 LEFT JOIN coordinated_bid_baseline_approvals AS approvals
                   ON approvals.baseline_id = heads.baseline_id
                  AND approvals.baseline_version = heads.current_version
                 WHERE heads.singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        match (&head, command.base_version) {
            (None, None) => {}
            (Some((_, version, _, decision)), Some(base))
                if *version == base && decision.as_deref() != Some("approve") => {}
            _ => {
                self.record_baseline_denial(tender_id, "assemble", "base_version_not_current")?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let snapshot = self.derive_coordinated_baseline_snapshot(budget)?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (baseline_id, version, preceding) = match head {
            Some((baseline_id, version, manifest_sha256, _)) => (
                baseline_id,
                version
                    .checked_add(1)
                    .filter(|version| *version <= MAX_BASELINE_VERSIONS)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
                Some(manifest_sha256),
            ),
            None => (random_identifier(&transaction)?, 1, None),
        };
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = BaselineManifest {
            schema_version: 1,
            baseline_id: baseline_id.clone(),
            version,
            tender_revision: snapshot.tender_revision,
            activation_id: snapshot.activation_id.clone(),
            plan_id: snapshot.plan_id.clone(),
            plan_version: snapshot.plan_version,
            plan_manifest_sha256: snapshot.plan_manifest_sha256.clone(),
            coordinator_profile_id: snapshot.coordinator_profile_id.clone(),
            coordinator_profile_version: snapshot.coordinator_profile_version,
            bindings: snapshot.bindings.clone(),
            contradictions: snapshot.contradictions.clone(),
            blockers: snapshot.blockers.clone(),
            explanation: snapshot.explanation.clone(),
            preceding_version_manifest_sha256: preceding.clone(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        if manifest_json.len() > 4 * 1024 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        if version == 1 {
            transaction
                .execute(
                    "INSERT INTO coordinated_bid_baselines (baseline_id, created_at) VALUES (?1, ?2)",
                    params![baseline_id, created_at],
                )
                .map_err(sql_error)?;
        }
        let lifecycle_after = if snapshot.is_ready() {
            TenderLifecyclePhase::IntegratedReview
        } else {
            lifecycle
        };
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "coordinated_bid_baseline_proposed",
            snapshot.tender_revision,
            json!({
                "baseline_id": baseline_id,
                "version": version.to_string(),
                "manifest_sha256": manifest_sha256,
                "coordinator_profile_id": snapshot.coordinator_profile_id,
                "coordinator_profile_version": snapshot.coordinator_profile_version.to_string(),
                "binding_count": snapshot.bindings.len().to_string(),
                "contradiction_count": snapshot.contradictions.len().to_string(),
                "blocker_count": snapshot.blockers.len().to_string(),
                "lifecycle_before": lifecycle,
                "lifecycle_after": lifecycle_after,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO coordinated_bid_baseline_versions (
                   baseline_id, version, tender_revision, activation_id, plan_id, plan_version,
                   plan_manifest_sha256, coordinator_profile_id, coordinator_profile_version,
                   bindings_json, contradictions_json, blockers_json, explanation,
                   preceding_version_manifest_sha256, audit_sequence, manifest_json,
                   manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                           ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    baseline_id,
                    version,
                    snapshot.tender_revision,
                    snapshot.activation_id,
                    snapshot.plan_id,
                    snapshot.plan_version,
                    snapshot.plan_manifest_sha256,
                    snapshot.coordinator_profile_id,
                    snapshot.coordinator_profile_version,
                    canonical_json(&snapshot.bindings)?,
                    canonical_json(&snapshot.contradictions)?,
                    canonical_json(&snapshot.blockers)?,
                    snapshot.explanation,
                    preceding,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO coordinated_bid_baseline_head (singleton, baseline_id, current_version)
                 VALUES (1, ?1, ?2)
                 ON CONFLICT(singleton) DO UPDATE SET baseline_id = excluded.baseline_id,
                                                      current_version = excluded.current_version",
                params![baseline_id, version],
            )
            .map_err(sql_error)?;
        if lifecycle_after != lifecycle
            && transaction
                .execute(
                    "UPDATE tender SET lifecycle_phase = 'integrated_review'
                     WHERE singleton = 1 AND lifecycle_phase = 'active_production'",
                    [],
                )
                .map_err(sql_error)?
                != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_coordinated_bid_baseline(&baseline_id, version, budget)
    }

    fn decide_coordinated_bid_baseline(
        &mut self,
        tender_id: &TenderId,
        command: &DecideCoordinatedBidBaselineCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<CoordinatedBidBaseline, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let baseline =
            self.load_coordinated_bid_baseline(&command.baseline_id, command.version, budget)?;
        let snapshot = self.derive_coordinated_baseline_snapshot(budget)?;
        let current_snapshot = baseline.bindings == snapshot.bindings
            && baseline.contradictions == snapshot.contradictions
            && baseline.blockers == snapshot.blockers
            && baseline.tender_revision == snapshot.tender_revision
            && baseline.activation_id == snapshot.activation_id
            && baseline.plan_manifest_sha256 == snapshot.plan_manifest_sha256;
        if !baseline.current
            || baseline.manifest_sha256 != command.manifest_sha256
            || baseline.approval.is_some()
            || !current_snapshot
            || self.lifecycle_phase()? != TenderLifecyclePhase::IntegratedReview
            || (command.decision == CoordinatedBidBaselineDecision::Approve
                && (!baseline.blockers.is_empty() || !baseline.contradictions.is_empty()))
        {
            self.record_baseline_denial(tender_id, "decide", "exact_gate_not_ready")?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let supporting_reviews_sha256 = supporting_reviews_sha256(&baseline.bindings)?;
        let lifecycle_after = if command.decision == CoordinatedBidBaselineDecision::Approve {
            TenderLifecyclePhase::PackageProduction
        } else {
            TenderLifecyclePhase::ActiveProduction
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let exact_head: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM coordinated_bid_baseline_head AS heads
                   JOIN coordinated_bid_baseline_versions AS versions
                     ON versions.baseline_id = heads.baseline_id
                    AND versions.version = heads.current_version
                   JOIN tender ON tender.singleton = 1
                   WHERE heads.singleton = 1 AND heads.baseline_id = ?1
                     AND heads.current_version = ?2 AND versions.manifest_sha256 = ?3
                     AND tender.lifecycle_phase = 'integrated_review'
                 )",
                params![
                    command.baseline_id,
                    command.version,
                    command.manifest_sha256
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let already_decided: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM coordinated_bid_baseline_approvals
                   WHERE baseline_id = ?1 AND baseline_version = ?2
                 )",
                params![command.baseline_id, command.version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exact_head || already_decided {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let preceding_approval_hash: String = transaction
            .query_row(
                "SELECT approval_sha256 FROM coordinated_bid_baseline_approvals
                 ORDER BY audit_sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?
            .unwrap_or_else(|| ZERO_HASH.to_owned());
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let approval_manifest = ApprovalManifest {
            schema_version: 1,
            approval_id: approval_id.clone(),
            baseline_id: command.baseline_id.clone(),
            baseline_version: command.version,
            baseline_manifest_sha256: command.manifest_sha256.clone(),
            decision: command.decision,
            rationale: command.rationale.clone(),
            conditions: command.conditions.clone(),
            exceptions: command.exceptions.clone(),
            supporting_reviews_sha256: supporting_reviews_sha256.clone(),
            decided_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            lifecycle_before: TenderLifecyclePhase::IntegratedReview,
            lifecycle_after,
            preceding_approval_hash: preceding_approval_hash.clone(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&approval_manifest)?;
        let approval_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "coordinated_bid_baseline_decided",
            baseline.tender_revision,
            json!({
                "approval_id": approval_id,
                "approval_sha256": approval_sha256,
                "baseline_id": command.baseline_id,
                "baseline_version": command.version.to_string(),
                "baseline_manifest_sha256": command.manifest_sha256,
                "decision": command.decision,
                "conditions": command.conditions,
                "exceptions": command.exceptions,
                "rationale": command.rationale,
                "supporting_reviews_sha256": supporting_reviews_sha256,
                "decided_by": "engineer_user",
                "acting_role": "tendering_manager",
                "lifecycle_before": TenderLifecyclePhase::IntegratedReview,
                "lifecycle_after": lifecycle_after,
                "preceding_approval_hash": preceding_approval_hash,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO coordinated_bid_baseline_approvals (
                   approval_id, baseline_id, baseline_version, baseline_manifest_sha256,
                   decision, rationale, conditions_json, exceptions_json,
                   supporting_reviews_sha256, decided_by, acting_role, lifecycle_before,
                   lifecycle_after, preceding_approval_hash, audit_sequence, manifest_json,
                   approval_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'engineer_user',
                           'tendering_manager', 'integrated_review', ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    approval_id,
                    command.baseline_id,
                    command.version,
                    command.manifest_sha256,
                    command.decision.as_str(),
                    command.rationale,
                    canonical_json(&command.conditions)?,
                    canonical_json(&command.exceptions)?,
                    supporting_reviews_sha256,
                    lifecycle_after.as_str(),
                    preceding_approval_hash,
                    audit_sequence,
                    manifest_json,
                    approval_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        if transaction
            .execute(
                "UPDATE tender SET lifecycle_phase = ?1
                 WHERE singleton = 1 AND lifecycle_phase = 'integrated_review'",
                [lifecycle_after.as_str()],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        self.load_coordinated_bid_baseline(&command.baseline_id, command.version, budget)
    }

    fn inspect_coordinated_bid_baselines(
        &self,
        before_version: Option<u32>,
        limit: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<CoordinatedBidBaselinePage, TenderCommandError> {
        if limit == 0 || limit > MAX_PAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        budget.check()?;
        let baseline_id: Option<String> = self
            .connection
            .query_row(
                "SELECT baseline_id FROM coordinated_bid_baseline_head WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let mut items = Vec::new();
        let mut next_before_version = None;
        if let Some(baseline_id) = baseline_id {
            let before = before_version.unwrap_or(MAX_BASELINE_VERSIONS + 1);
            let mut statement = self
                .connection
                .prepare(
                    "SELECT version FROM coordinated_bid_baseline_versions
                     WHERE baseline_id = ?1 AND version < ?2
                     ORDER BY version DESC LIMIT ?3",
                )
                .map_err(sql_error)?;
            let versions = statement
                .query_map(params![baseline_id, before, limit + 1], |row| {
                    row.get::<_, u32>(0)
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            let has_more = versions.len() > limit as usize;
            for version in versions.iter().take(limit as usize) {
                budget.check()?;
                items.push(self.load_coordinated_bid_baseline(&baseline_id, *version, budget)?);
                if canonical_json(&items)?.len() > MAX_PAGE_BYTES {
                    items.pop();
                    break;
                }
            }
            if items.is_empty() && !versions.is_empty() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            if has_more || items.len() < versions.len() {
                next_before_version = items.last().map(|item| item.version);
            }
        }
        Ok(CoordinatedBidBaselinePage {
            items,
            next_before_version,
            lifecycle_phase: self.lifecycle_phase()?,
        })
    }

    fn lifecycle_phase(&self) -> Result<TenderLifecyclePhase, TenderCommandError> {
        TenderLifecyclePhase::parse(
            &self
                .connection
                .query_row(
                    "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sql_error)?,
        )
    }

    fn load_coordinated_bid_baseline(
        &self,
        baseline_id: &str,
        version: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<CoordinatedBidBaseline, TenderCommandError> {
        budget.check()?;
        type Row = (
            u32,
            String,
            String,
            u32,
            String,
            String,
            u32,
            String,
            String,
            String,
            String,
            Option<String>,
            String,
            String,
            String,
        );
        let row: Row = self
            .connection
            .query_row(
                "SELECT tender_revision, activation_id, plan_id, plan_version,
                        plan_manifest_sha256, coordinator_profile_id,
                        coordinator_profile_version, bindings_json, contradictions_json,
                        blockers_json, explanation, preceding_version_manifest_sha256,
                        manifest_json, manifest_sha256, created_at
                 FROM coordinated_bid_baseline_versions
                 WHERE baseline_id = ?1 AND version = ?2",
                params![baseline_id, version],
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
                    ))
                },
            )
            .map_err(sql_error)?;
        let bindings = parse_canonical::<Vec<CoordinatedBidBaselineBinding>>(&row.7)?;
        let contradictions = parse_canonical::<Vec<CoordinatedBidBaselineContradiction>>(&row.8)?;
        let blockers = parse_canonical::<Vec<CoordinatedBidBaselineBlocker>>(&row.9)?;
        let manifest = BaselineManifest {
            schema_version: 1,
            baseline_id: baseline_id.to_owned(),
            version,
            tender_revision: row.0,
            activation_id: row.1.clone(),
            plan_id: row.2.clone(),
            plan_version: row.3,
            plan_manifest_sha256: row.4.clone(),
            coordinator_profile_id: row.5.clone(),
            coordinator_profile_version: row.6,
            bindings: bindings.clone(),
            contradictions: contradictions.clone(),
            blockers: blockers.clone(),
            explanation: row.10.clone(),
            preceding_version_manifest_sha256: row.11.clone(),
            created_at: row.14.clone(),
        };
        if canonical_json(&manifest)? != row.12
            || sha256_hex(row.12.as_bytes()) != row.13
            || bindings.len() > MAX_BASELINE_BINDINGS
            || contradictions.len() > MAX_BASELINE_CONTRADICTIONS
            || blockers.len() > MAX_BASELINE_BLOCKERS
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let approval = self.load_coordinated_bid_baseline_approval(baseline_id, version)?;
        let head_matches: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM coordinated_bid_baseline_head
                   WHERE singleton = 1 AND baseline_id = ?1 AND current_version = ?2
                 )",
                params![baseline_id, version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let current = if head_matches {
            let snapshot = self.derive_coordinated_baseline_snapshot(budget)?;
            snapshot.tender_revision == row.0
                && snapshot.activation_id == row.1
                && snapshot.plan_id == row.2
                && snapshot.plan_version == row.3
                && snapshot.plan_manifest_sha256 == row.4
                && snapshot.coordinator_profile_id == row.5
                && snapshot.coordinator_profile_version == row.6
                && snapshot.bindings == bindings
                && snapshot.contradictions == contradictions
                && snapshot.blockers == blockers
        } else {
            false
        };
        Ok(CoordinatedBidBaseline {
            baseline_id: baseline_id.to_owned(),
            version,
            tender_revision: row.0,
            activation_id: row.1,
            plan_id: row.2,
            plan_version: row.3,
            plan_manifest_sha256: row.4,
            coordinator_profile_id: row.5,
            coordinator_profile_version: row.6,
            bindings,
            contradictions,
            blockers,
            explanation: row.10,
            preceding_version_manifest_sha256: row.11,
            approval,
            current,
            manifest_sha256: row.13,
            created_at: row.14,
        })
    }

    fn load_coordinated_bid_baseline_approval(
        &self,
        baseline_id: &str,
        version: u32,
    ) -> Result<Option<CoordinatedBidBaselineApproval>, TenderCommandError> {
        type Row = (
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
            String,
            String,
            String,
            String,
        );
        let row: Option<Row> = self
            .connection
            .query_row(
                "SELECT approval_id, baseline_manifest_sha256, decision, rationale,
                        conditions_json, exceptions_json, supporting_reviews_sha256,
                        decided_by, acting_role, lifecycle_before, lifecycle_after,
                        preceding_approval_hash, manifest_json, approval_sha256, created_at
                 FROM coordinated_bid_baseline_approvals
                 WHERE baseline_id = ?1 AND baseline_version = ?2",
                params![baseline_id, version],
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
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?;
        row.map(|row| {
            let decision = CoordinatedBidBaselineDecision::parse(&row.2)?;
            let conditions = parse_canonical::<Vec<String>>(&row.4)?;
            let exceptions = parse_canonical::<Vec<String>>(&row.5)?;
            let lifecycle_before = TenderLifecyclePhase::parse(&row.9)?;
            let lifecycle_after = TenderLifecyclePhase::parse(&row.10)?;
            let manifest = ApprovalManifest {
                schema_version: 1,
                approval_id: row.0.clone(),
                baseline_id: baseline_id.to_owned(),
                baseline_version: version,
                baseline_manifest_sha256: row.1.clone(),
                decision,
                rationale: row.3.clone(),
                conditions: conditions.clone(),
                exceptions: exceptions.clone(),
                supporting_reviews_sha256: row.6.clone(),
                decided_by: row.7.clone(),
                acting_role: row.8.clone(),
                lifecycle_before,
                lifecycle_after,
                preceding_approval_hash: row.11.clone(),
                created_at: row.14.clone(),
            };
            if canonical_json(&manifest)? != row.12 || sha256_hex(row.12.as_bytes()) != row.13 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            Ok(CoordinatedBidBaselineApproval {
                approval_id: row.0,
                baseline_id: baseline_id.to_owned(),
                baseline_version: version,
                baseline_manifest_sha256: row.1,
                decision,
                rationale: row.3,
                conditions,
                exceptions,
                supporting_reviews_sha256: row.6,
                decided_by: row.7,
                acting_role: row.8,
                lifecycle_before,
                lifecycle_after,
                preceding_approval_hash: row.11,
                approval_sha256: row.13,
                created_at: row.14,
            })
        })
        .transpose()
    }

    fn derive_coordinated_baseline_snapshot(
        &self,
        budget: BidPackageOperationBudget,
    ) -> Result<BaselineSnapshot, TenderCommandError> {
        budget.check()?;
        let tender_revision: u32 = self
            .connection
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let (activation_id, plan_id, plan_version, plan_manifest_sha256): (
            String,
            String,
            u32,
            String,
        ) = self
            .connection
            .query_row(
                "SELECT activation_id, plan_id, plan_version, plan_manifest_sha256
                 FROM production_activations WHERE status = 'active'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(sql_error)?;
        let (coordinator_profile_id, coordinator_profile_version): (String, u32) = self
            .connection
            .query_row(
                "SELECT json_extract(binding.value, '$.profile.profile_id'),
                        json_extract(binding.value, '$.profile.version')
                 FROM work_plan_versions AS plans,
                      json_each(plans.profiles_json) AS binding
                 JOIN agent_profile_heads AS heads
                   ON heads.profile_id = json_extract(binding.value, '$.profile.profile_id')
                  AND heads.current_version = json_extract(binding.value, '$.profile.version')
                 WHERE plans.plan_id = ?1 AND plans.version = ?2
                   AND json_extract(binding.value, '$.archetype') IN (
                     'tender_office_coordinator', 'tender_coordinator'
                   )
                   AND heads.status = 'active'
                 ORDER BY CASE json_extract(binding.value, '$.archetype')
                            WHEN 'tender_office_coordinator' THEN 0 ELSE 1 END
                 LIMIT 1",
                params![plan_id, plan_version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let mut bindings = Vec::new();
        let mut contradictions = Vec::new();
        let mut blockers = Vec::new();
        let mut field_observations = Vec::new();
        self.collect_production_bindings(
            &activation_id,
            &mut bindings,
            &mut blockers,
            &mut field_observations,
            budget,
        )?;
        self.collect_record_bindings(
            &mut bindings,
            &mut contradictions,
            &mut blockers,
            &mut field_observations,
            budget,
        )?;
        self.collect_query_bindings(
            &mut bindings,
            &mut blockers,
            &mut field_observations,
            budget,
        )?;
        self.collect_external_rfi_bindings(&mut bindings, budget)?;
        self.collect_pricing_bindings(
            &mut bindings,
            &mut blockers,
            &mut field_observations,
            tender_revision,
            budget,
        )?;
        self.collect_capability_gap_blockers(&plan_id, plan_version, &mut blockers)?;
        collect_field_contradictions(&field_observations, &mut contradictions)?;
        for category in [
            CoordinatedBidBaselineCategory::Technical,
            CoordinatedBidBaselineCategory::Programme,
            CoordinatedBidBaselineCategory::Procurement,
            CoordinatedBidBaselineCategory::Contractual,
            CoordinatedBidBaselineCategory::Risk,
            CoordinatedBidBaselineCategory::Query,
            CoordinatedBidBaselineCategory::Qualification,
            CoordinatedBidBaselineCategory::Exclusion,
            CoordinatedBidBaselineCategory::Submission,
            CoordinatedBidBaselineCategory::Commercial,
        ] {
            if !bindings.iter().any(|binding| binding.category == category) {
                blockers.push(CoordinatedBidBaselineBlocker {
                    code: CoordinatedBidBaselineBlockerCode::WorkstreamEvidenceMissing,
                    summary: format!(
                        "No exact reviewed {} record or artifact is bound.",
                        category.as_str()
                    ),
                    references: vec![category.as_str().into()],
                });
            }
        }
        if !contradictions.is_empty() {
            blockers.push(CoordinatedBidBaselineBlocker {
                code: CoordinatedBidBaselineBlockerCode::ContradictionOpen,
                summary: format!(
                    "{} cross-workstream contradiction(s) require reconciliation.",
                    contradictions.len()
                ),
                references: contradictions
                    .iter()
                    .flat_map(|value| value.references.iter().cloned())
                    .take(64)
                    .collect(),
            });
        }
        sort_and_validate_snapshot(&mut bindings, &mut contradictions, &mut blockers)?;
        let explanation = format!(
            "Tender Office Coordinator assembled {} exact validated binding(s) from approved Work Plan {} version {}, with {} contradiction(s) and {} blocking control(s) disclosed for the Tendering Manager.",
            bindings.len(),
            plan_id,
            plan_version,
            contradictions.len(),
            blockers.len(),
        );
        Ok(BaselineSnapshot {
            tender_revision,
            activation_id,
            plan_id,
            plan_version,
            plan_manifest_sha256,
            coordinator_profile_id,
            coordinator_profile_version,
            bindings,
            contradictions,
            blockers,
            explanation,
        })
    }

    fn collect_production_bindings(
        &self,
        activation_id: &str,
        bindings: &mut Vec<CoordinatedBidBaselineBinding>,
        blockers: &mut Vec<CoordinatedBidBaselineBlocker>,
        observations: &mut Vec<FieldObservation>,
        budget: BidPackageOperationBudget,
    ) -> Result<(), TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT production_task_id, task_key, task_definition_json, status
                 FROM production_tasks WHERE activation_id = ?1 ORDER BY task_key",
            )
            .map_err(sql_error)?;
        let tasks = statement
            .query_map([activation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        if tasks.is_empty() || tasks.len() > 256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for (production_task_id, task_key, definition_json, status) in tasks {
            budget.check()?;
            if status != "ready_for_integration" {
                blockers.push(CoordinatedBidBaselineBlocker {
                    code: CoordinatedBidBaselineBlockerCode::ProductionTaskNotReady,
                    summary: format!("Production task {task_key} is {status}."),
                    references: vec![production_task_id],
                });
                continue;
            }
            let definition: WorkPlanTask = parse_canonical(&definition_json)?;
            let workstream = definition.workstream_key.as_str();
            let objective = definition.objective.as_str();
            let plan_reference = format!("work_plan_task:{task_key}");
            let responsibility = ProductionCoordinationObservationValue::TextSet {
                values: vec![format!(
                    "{}={}",
                    coordination_task_assignment_key(&task_key),
                    definition.profile_id
                )],
            };
            observations.push(FieldObservation {
                subject: ProductionCoordinationObservationSubject::ResponsibleParty,
                scope: "global".into(),
                value: canonical_coordination_observation_value(
                    ProductionCoordinationObservationSubject::ResponsibleParty,
                    &responsibility,
                )
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                reference: plan_reference.clone(),
                keyed: true,
            });
            if coordination_deadline_is_required(workstream) {
                let deadline = ProductionCoordinationObservationValue::TextSet {
                    values: vec![format!(
                        "{}={}",
                        coordination_task_assignment_key(&task_key),
                        definition.deadline
                    )],
                };
                observations.push(FieldObservation {
                    subject: ProductionCoordinationObservationSubject::SubmissionDeadline,
                    scope: "global".into(),
                    value: canonical_coordination_observation_value(
                        ProductionCoordinationObservationSubject::SubmissionDeadline,
                        &deadline,
                    )
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    reference: plan_reference,
                    keyed: true,
                });
            }
            type ReadyRow = (String, u32, String, String, Option<String>, String);
            let ready: ReadyRow = self
                .connection
                .query_row(
                    "SELECT readiness.artifact_id, readiness.artifact_version,
                            readiness.payload_sha256, artifacts.payload_json, readiness.review_id,
                            readiness.finding_dispositions_sha256
                     FROM production_integration_readiness AS readiness
                     JOIN production_artifact_versions AS artifacts
                       ON artifacts.artifact_id = readiness.artifact_id
                      AND artifacts.version = readiness.artifact_version
                     WHERE readiness.production_task_id = ?1
                     ORDER BY readiness.rowid DESC LIMIT 1",
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
                .map_err(sql_error)?;
            let readiness_digest = sha256_hex(
                canonical_json(&json!({
                    "artifact_payload_sha256": &ready.2,
                    "review_id": &ready.4,
                    "finding_dispositions_sha256": &ready.5,
                }))?
                .as_bytes(),
            );
            bindings.push(CoordinatedBidBaselineBinding {
                category: category_for_workstream(workstream),
                kind: CoordinatedBidBaselineBindingKind::ProductionArtifactVersion,
                reference_id: ready.0.clone(),
                version: ready.1,
                manifest_sha256: readiness_digest,
                source: task_key.clone(),
                summary: objective.to_owned(),
                supporting_review_id: ready.4,
                approval_id: None,
            });
            let payload: ProductionArtifactPayload = parse_canonical(&ready.3)?;
            if payload.coordination_observations.is_empty() {
                blockers.push(CoordinatedBidBaselineBlocker {
                    code: CoordinatedBidBaselineBlockerCode::WorkstreamEvidenceMissing,
                    summary: format!(
                        "Ready-for-Integration task {task_key} has no typed coordination observation."
                    ),
                    references: vec![production_task_id.clone()],
                });
            }
            for observation in payload.coordination_observations {
                let value = canonical_coordination_observation_value(
                    observation.subject,
                    &observation.value,
                )
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                observations.push(FieldObservation {
                    subject: observation.subject,
                    scope: coordination_observation_scope(observation.subject, &task_key),
                    value,
                    reference: format!("{}:{}", ready.0, ready.1),
                    keyed: coordination_observation_is_keyed(observation.subject),
                });
            }
        }
        let mut finding_statement = self
            .connection
            .prepare(
                "SELECT findings.finding_id, findings.severity, findings.summary
                 FROM production_review_findings AS findings
                 JOIN production_reviews AS reviews ON reviews.review_id = findings.review_id
                 JOIN production_tasks AS tasks
                   ON tasks.production_task_id = reviews.production_task_id
                 LEFT JOIN production_finding_dispositions AS dispositions
                   ON dispositions.finding_id = findings.finding_id
                 WHERE tasks.activation_id = ?1 AND dispositions.finding_id IS NULL
                   AND findings.severity IN ('critical', 'major')
                 ORDER BY findings.finding_id LIMIT 257",
            )
            .map_err(sql_error)?;
        let findings = finding_statement
            .query_map([activation_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        if findings.len() > 256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for (finding_id, severity, summary) in findings {
            blockers.push(CoordinatedBidBaselineBlocker {
                code: if severity == "critical" {
                    CoordinatedBidBaselineBlockerCode::OpenCriticalFinding
                } else {
                    CoordinatedBidBaselineBlockerCode::OpenMajorFinding
                },
                summary,
                references: vec![finding_id],
            });
        }
        Ok(())
    }

    fn collect_record_bindings(
        &self,
        bindings: &mut Vec<CoordinatedBidBaselineBinding>,
        contradictions: &mut Vec<CoordinatedBidBaselineContradiction>,
        blockers: &mut Vec<CoordinatedBidBaselineBlocker>,
        observations: &mut Vec<FieldObservation>,
        budget: BidPackageOperationBudget,
    ) -> Result<(), TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT heads.record_id, heads.current_version
                 FROM tender_record_heads AS heads
                 JOIN tender_records AS records ON records.record_id = heads.record_id
                 ORDER BY records.stable_key LIMIT 257",
            )
            .map_err(sql_error)?;
        let records = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        if records.len() > 256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for (record_id, version) in records {
            budget.check()?;
            let record = self.inspect_tender_record_version(&record_id, version)?;
            if record.verification_status != VerificationStatus::Verified
                || matches!(
                    record.trust_class,
                    TenderRecordTrustClass::AiProposal
                        | TenderRecordTrustClass::UnresolvedGap
                        | TenderRecordTrustClass::PriorDecision
                )
            {
                blockers.push(CoordinatedBidBaselineBlocker {
                    code: if matches!(
                        record.verification_status,
                        VerificationStatus::Stale | VerificationStatus::Superseded
                    ) {
                        CoordinatedBidBaselineBlockerCode::StaleInput
                    } else {
                        CoordinatedBidBaselineBlockerCode::UnverifiedInput
                    },
                    summary: format!(
                        "Tender Record '{}' is not a current admitted fact.",
                        record.title
                    ),
                    references: vec![format!("{}:{}", record.record_id, record.version)],
                });
                continue;
            }
            let supporting_review_id = record.reviews.last().map(|review| review.review_id.clone());
            let immutable_record = json!({
                "record_id": record.record_id,
                "stable_key": record.stable_key,
                "version": record.version,
                "kind": record.kind,
                "title": record.title,
                "fields": record.fields,
                "contradictions": record.contradictions,
                "author_run_id": record.author_run_id,
                "author_profile_id": record.author_profile_id,
                "supporting_review_id": supporting_review_id,
                "trust_class": record.trust_class,
            });
            let digest = sha256_hex(canonical_json(&immutable_record)?.as_bytes());
            let reference = format!("{}:{}", record.record_id, record.version);
            bindings.push(CoordinatedBidBaselineBinding {
                category: category_for_record(record.kind, &record.title),
                kind: CoordinatedBidBaselineBindingKind::TenderRecordVersion,
                reference_id: record.record_id.clone(),
                version: record.version,
                manifest_sha256: digest,
                source: record.stable_key.clone(),
                summary: record.title.clone(),
                supporting_review_id,
                approval_id: None,
            });
            for contradiction in &record.contradictions {
                contradictions.push(CoordinatedBidBaselineContradiction {
                    category: contradiction_category(&contradiction.field_name),
                    key: format!("{}:{}", record.stable_key, contradiction.field_name),
                    summary: contradiction.summary.clone(),
                    references: vec![reference.clone()],
                });
            }
            for field in &record.fields {
                let Some(value) = field
                    .normalized_value
                    .as_deref()
                    .or(field.value.as_deref())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                let Some((subject, typed_value)) = record_coordination_observation(
                    record.kind,
                    &record.stable_key,
                    &field.name,
                    value,
                ) else {
                    continue;
                };
                let value = canonical_coordination_observation_value(subject, &typed_value)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                observations.push(FieldObservation {
                    subject,
                    scope: "global".into(),
                    value,
                    reference: reference.clone(),
                    keyed: true,
                });
            }
        }
        Ok(())
    }

    fn collect_query_bindings(
        &self,
        bindings: &mut Vec<CoordinatedBidBaselineBinding>,
        blockers: &mut Vec<CoordinatedBidBaselineBlocker>,
        observations: &mut Vec<FieldObservation>,
        budget: BidPackageOperationBudget,
    ) -> Result<(), TenderCommandError> {
        type Row = (
            String,
            u32,
            String,
            bool,
            bool,
            Option<String>,
            Option<String>,
            Option<bool>,
            Option<String>,
            Option<String>,
            String,
        );
        let mut statement = self
            .connection
            .prepare(
                "SELECT versions.query_id, versions.version, versions.manifest_sha256,
                        versions.material, versions.release_blocking, decisions.decision_id,
                        decisions.treatment, decisions.closes_query, decisions.manifest_sha256,
                        decisions.treatment_details, versions.affected_task_keys_json
                 FROM tender_query_heads AS heads
                 JOIN tender_query_versions AS versions
                   ON versions.query_id = heads.query_id
                  AND versions.version = heads.current_version
                 LEFT JOIN tender_query_treatment_decisions AS decisions
                   ON decisions.query_id = versions.query_id
                  AND decisions.query_version = versions.version
                 ORDER BY versions.query_id LIMIT 257",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
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
                ))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<Row>, _>>()
            .map_err(sql_error)?;
        if rows.len() > 256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for row in rows {
            budget.check()?;
            let unresolved = matches!(row.6.as_deref(), Some("external_rfi_drafting" | "blocked"))
                || ((row.3 || row.4) && (row.5.is_none() || row.7 != Some(true)));
            if unresolved {
                blockers.push(CoordinatedBidBaselineBlocker {
                    code: CoordinatedBidBaselineBlockerCode::OpenMaterialQuery,
                    summary: "A material or release-blocking Query remains unresolved.".into(),
                    references: vec![format!("{}:{}", row.0, row.1)],
                });
            }
            let digest = sha256_hex(
                canonical_json(&json!({
                    "query_manifest_sha256": row.2,
                    "decision_id": row.5,
                    "decision_manifest_sha256": row.8,
                }))?
                .as_bytes(),
            );
            if let (Some(treatment), Some(treatment_details)) = (row.6.as_deref(), row.9.as_deref())
            {
                let affected_task_keys: Vec<String> = parse_canonical(&row.10)?;
                let subject = ProductionCoordinationObservationSubject::QueryTreatment;
                let value = canonical_coordination_observation_value(
                    subject,
                    &ProductionCoordinationObservationValue::Text {
                        text: format!(
                            "{treatment}:{}:{}",
                            affected_task_keys.join(","),
                            treatment_details
                        ),
                    },
                )
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                observations.push(FieldObservation {
                    subject,
                    scope: row.0.clone(),
                    value,
                    reference: format!("{}:{}", row.0, row.1),
                    keyed: false,
                });
            }
            bindings.push(CoordinatedBidBaselineBinding {
                category: match row.6.as_deref() {
                    Some("qualification") | Some("approved_assumption") => {
                        CoordinatedBidBaselineCategory::Qualification
                    }
                    Some("exclusion") => CoordinatedBidBaselineCategory::Exclusion,
                    Some("allowance") => CoordinatedBidBaselineCategory::Commercial,
                    _ => CoordinatedBidBaselineCategory::Query,
                },
                kind: CoordinatedBidBaselineBindingKind::TenderQueryVersion,
                reference_id: row.0,
                version: row.1,
                manifest_sha256: digest,
                source: "query_register".into(),
                summary: row.6.unwrap_or_else(|| "unresolved_query".into()),
                supporting_review_id: None,
                approval_id: row.5,
            });
        }
        Ok(())
    }

    fn collect_external_rfi_bindings(
        &self,
        bindings: &mut Vec<CoordinatedBidBaselineBinding>,
        budget: BidPackageOperationBudget,
    ) -> Result<(), TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT versions.rfi_id, versions.version, versions.manifest_sha256,
                        approvals.approval_id, approvals.approval_sha256
                 FROM external_rfi_heads AS heads
                 JOIN external_rfi_versions AS versions
                   ON versions.rfi_id = heads.rfi_id AND versions.version = heads.current_version
                 JOIN external_rfi_approvals AS approvals
                   ON approvals.rfi_id = versions.rfi_id AND approvals.rfi_version = versions.version
                 ORDER BY versions.rfi_id LIMIT 65",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        if rows.len() > 64 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for row in rows {
            budget.check()?;
            bindings.push(CoordinatedBidBaselineBinding {
                category: CoordinatedBidBaselineCategory::Query,
                kind: CoordinatedBidBaselineBindingKind::ExternalRfiVersion,
                reference_id: row.0,
                version: row.1,
                manifest_sha256: sha256_hex(
                    canonical_json(&json!({"version": row.2, "approval": row.4}))?.as_bytes(),
                ),
                source: "external_rfi_register".into(),
                summary: "Approved exact External RFI version".into(),
                supporting_review_id: None,
                approval_id: Some(row.3),
            });
        }
        Ok(())
    }

    fn collect_pricing_bindings(
        &self,
        bindings: &mut Vec<CoordinatedBidBaselineBinding>,
        blockers: &mut Vec<CoordinatedBidBaselineBlocker>,
        observations: &mut Vec<FieldObservation>,
        tender_revision: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<(), TenderCommandError> {
        type Row = (
            String,
            u32,
            String,
            String,
            String,
            String,
            String,
            u32,
            String,
            u32,
            String,
            String,
            String,
            String,
            String,
            String,
        );
        let row: Option<Row> = self
            .connection
            .query_row(
                "SELECT scenarios.pricing_scenario_id, scenarios.version,
                        scenarios.manifest_sha256, prices.approval_id, prices.manifest_sha256,
                        prices.pricing_calculation_run_id, prices.calculation_manifest_sha256,
                        scenarios.tender_revision, baselines.baseline_id, baselines.version,
                        baselines.manifest_sha256, baseline_approvals.approval_id,
                        baseline_reviews.review_id, strategies.strategy_id,
                        strategies.manifest_json, strategy_approvals.approval_id
                 FROM pricing_selection_head AS head
                 JOIN pricing_scenario_selections AS selections
                   ON selections.selection_id = head.selection_id
                 JOIN pricing_scenario_versions AS scenarios
                   ON scenarios.pricing_scenario_id = selections.pricing_scenario_id
                  AND scenarios.version = selections.pricing_scenario_version
                 JOIN approved_tender_prices AS prices ON prices.selection_id = selections.selection_id
                 JOIN priced_cost_baseline_versions AS baselines
                   ON baselines.baseline_id = scenarios.baseline_id
                  AND baselines.version = scenarios.baseline_version
                 JOIN priced_cost_baseline_heads AS baseline_heads
                   ON baseline_heads.baseline_id = baselines.baseline_id
                  AND baseline_heads.current_version = baselines.version
                 JOIN priced_cost_baseline_approvals AS baseline_approvals
                   ON baseline_approvals.baseline_id = baselines.baseline_id
                  AND baseline_approvals.baseline_version = baselines.version
                 JOIN priced_cost_baseline_reviews AS baseline_reviews
                   ON baseline_reviews.review_id = baseline_approvals.review_id
                 JOIN commercial_strategies AS strategies
                   ON strategies.strategy_id = scenarios.strategy_id
                 JOIN commercial_strategy_approvals AS strategy_approvals
                   ON strategy_approvals.strategy_id = strategies.strategy_id
                 WHERE baseline_reviews.outcome = 'passed'",
                [],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                        row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                        row.get(10)?, row.get(11)?, row.get(12)?, row.get(13)?, row.get(14)?,
                        row.get(15)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?;
        let Some(row) = row else {
            blockers.push(CoordinatedBidBaselineBlocker {
                code: CoordinatedBidBaselineBlockerCode::PricedCostBaselineMissing,
                summary: "No current independently reviewed and approved Priced Cost Baseline is available.".into(),
                references: Vec::new(),
            });
            blockers.push(CoordinatedBidBaselineBlocker {
                code: CoordinatedBidBaselineBlockerCode::ApprovedTenderPriceMissing,
                summary: "No current Approved Tender Price is available.".into(),
                references: Vec::new(),
            });
            return Ok(());
        };
        budget.check()?;
        let pricing_workspace = self.inspect_pricing_workspace(budget)?;
        let selected_scenario = pricing_workspace
            .scenarios
            .iter()
            .find(|scenario| scenario.pricing_scenario_id == row.0 && scenario.version == row.1);
        let pricing_current = selected_scenario.is_some_and(|scenario| {
            scenario.current
                && scenario
                    .selection
                    .as_ref()
                    .is_some_and(|selection| selection.current)
                && scenario
                    .approved_tender_price
                    .as_ref()
                    .is_some_and(|price| price.current)
        });
        if row.7 != tender_revision || !pricing_current {
            blockers.push(CoordinatedBidBaselineBlocker {
                code: CoordinatedBidBaselineBlockerCode::StaleInput,
                summary:
                    "The selected Approved Tender Price or one of its exact dependencies is stale."
                        .into(),
                references: vec![format!("{}:{}", row.0, row.1)],
            });
        }
        let selected_scenario = selected_scenario
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let approved_price = selected_scenario
            .approved_tender_price
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let cost_baseline = pricing_workspace
            .baseline
            .as_ref()
            .filter(|baseline| baseline.baseline_id == row.8 && baseline.version == row.9)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let cost_subject = ProductionCoordinationObservationSubject::ExpectedDeliveryCost;
        let cost_value = canonical_coordination_observation_value(
            cost_subject,
            &ProductionCoordinationObservationValue::Amount {
                value: cost_baseline.amount.clone(),
                currency: cost_baseline.currency.clone(),
            },
        )
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        observations.push(FieldObservation {
            subject: cost_subject,
            scope: "global".into(),
            value: cost_value,
            reference: format!("{}:{}", row.8, row.9),
            keyed: false,
        });
        let price_subject = ProductionCoordinationObservationSubject::ApprovedTenderPrice;
        let price_value = canonical_coordination_observation_value(
            price_subject,
            &ProductionCoordinationObservationValue::Amount {
                value: approved_price.amount.clone(),
                currency: approved_price.currency.clone(),
            },
        )
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        observations.push(FieldObservation {
            subject: price_subject,
            scope: "global".into(),
            value: price_value,
            reference: format!("{}:{}", row.0, row.1),
            keyed: false,
        });
        bindings.push(CoordinatedBidBaselineBinding {
            category: CoordinatedBidBaselineCategory::Commercial,
            kind: CoordinatedBidBaselineBindingKind::PricedCostBaseline,
            reference_id: row.8,
            version: row.9,
            manifest_sha256: row.10,
            source: "priced_cost_baseline".into(),
            summary: "Independently reviewed and EITL-approved expected delivery cost".into(),
            supporting_review_id: Some(row.12),
            approval_id: Some(row.11),
        });
        bindings.push(CoordinatedBidBaselineBinding {
            category: CoordinatedBidBaselineCategory::Commercial,
            kind: CoordinatedBidBaselineBindingKind::ApprovedTenderPrice,
            reference_id: row.0,
            version: row.1,
            manifest_sha256: row.4,
            source: "approved_tender_price".into(),
            summary: "Exact selected and EITL-approved Final Price".into(),
            supporting_review_id: None,
            approval_id: Some(row.3),
        });
        bindings.push(CoordinatedBidBaselineBinding {
            category: CoordinatedBidBaselineCategory::Commercial,
            kind: CoordinatedBidBaselineBindingKind::CalculationManifest,
            reference_id: row.5,
            version: 1,
            manifest_sha256: row.6,
            source: "pricing_calculation".into(),
            summary: "Controlled Final Price Calculation Manifest".into(),
            supporting_review_id: None,
            approval_id: None,
        });
        let strategy: Value = parse_canonical(&row.14)?;
        let strategy_review_id = strategy
            .get("input_review_id")
            .and_then(Value::as_str)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .to_owned();
        let commercial_appetite = strategy
            .get("commercial_appetite")
            .and_then(Value::as_str)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let appetite_subject = ProductionCoordinationObservationSubject::CommercialAppetite;
        let appetite_value = canonical_coordination_observation_value(
            appetite_subject,
            &ProductionCoordinationObservationValue::Text {
                text: commercial_appetite.to_owned(),
            },
        )
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        observations.push(FieldObservation {
            subject: appetite_subject,
            scope: "global".into(),
            value: appetite_value,
            reference: row.13.clone(),
            keyed: false,
        });
        for (field, category) in [
            (
                "qualifications",
                CoordinatedBidBaselineCategory::Qualification,
            ),
            ("exclusions", CoordinatedBidBaselineCategory::Exclusion),
        ] {
            let values = strategy
                .get(field)
                .and_then(Value::as_array)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let subject = if field == "qualifications" {
                ProductionCoordinationObservationSubject::ScopeQualification
            } else {
                ProductionCoordinationObservationSubject::ScopeExclusion
            };
            let value = canonical_coordination_observation_value(
                subject,
                &ProductionCoordinationObservationValue::TextSet { values },
            )
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            observations.push(FieldObservation {
                subject,
                scope: "global".into(),
                value,
                reference: row.13.clone(),
                keyed: false,
            });
            bindings.push(CoordinatedBidBaselineBinding {
                category,
                kind: CoordinatedBidBaselineBindingKind::CommercialStrategy,
                reference_id: row.13.clone(),
                version: 1,
                manifest_sha256: sha256_hex(row.14.as_bytes()),
                source: "commercial_strategy".into(),
                summary: format!("Approved commercial {field}"),
                supporting_review_id: Some(strategy_review_id.clone()),
                approval_id: Some(row.15.clone()),
            });
        }
        let reconciled: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM basis_of_estimate_heads AS heads
                   JOIN basis_of_estimate_versions AS versions
                     ON versions.basis_id = heads.basis_id
                    AND versions.version = heads.current_version
                   JOIN basis_of_estimate_approvals AS approvals
                     ON approvals.basis_id = versions.basis_id
                    AND approvals.basis_version = versions.version
                   WHERE versions.complete = 1 AND versions.reconciled = 1
                 )",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !reconciled {
            blockers.push(CoordinatedBidBaselineBlocker {
                code: CoordinatedBidBaselineBlockerCode::UnreconciledCalculation,
                summary: "The current Basis of Estimate is not completely reconciled.".into(),
                references: Vec::new(),
            });
        }
        Ok(())
    }

    fn collect_capability_gap_blockers(
        &self,
        plan_id: &str,
        plan_version: u32,
        blockers: &mut Vec<CoordinatedBidBaselineBlocker>,
    ) -> Result<(), TenderCommandError> {
        let gaps_json: String = self
            .connection
            .query_row(
                "SELECT capability_gaps_json FROM work_plan_versions
                 WHERE plan_id = ?1 AND version = ?2",
                params![plan_id, plan_version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let gaps: Vec<Value> = parse_canonical(&gaps_json)?;
        if gaps.len() > 32 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for gap in gaps {
            let capability = gap
                .get("capability")
                .and_then(Value::as_str)
                .unwrap_or("unknown_capability");
            blockers.push(CoordinatedBidBaselineBlocker {
                code: CoordinatedBidBaselineBlockerCode::CapabilityGap,
                summary: format!("Capability Gap remains open for {capability}."),
                references: vec![capability.to_owned()],
            });
        }
        Ok(())
    }

    pub(crate) fn coordinated_baseline_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let (logical_count, version_count, head_count, approval_count): (u32, u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM coordinated_bid_baselines),
                            (SELECT COUNT(*) FROM coordinated_bid_baseline_versions),
                            (SELECT COUNT(*) FROM coordinated_bid_baseline_head),
                            (SELECT COUNT(*) FROM coordinated_bid_baseline_approvals)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(sql_error)?;
        if logical_count > 1
            || head_count != logical_count
            || version_count > MAX_BASELINE_VERSIONS
            || approval_count > version_count
        {
            return Ok(false);
        }
        if logical_count == 0 {
            return Ok(matches!(
                self.lifecycle_phase()?,
                TenderLifecyclePhase::Intake
                    | TenderLifecyclePhase::BidDecision
                    | TenderLifecyclePhase::TenderPlanning
                    | TenderLifecyclePhase::ActiveProduction
                    | TenderLifecyclePhase::Declined
            ));
        }
        let (baseline_id, head_version): (String, u32) = self
            .connection
            .query_row(
                "SELECT baseline_id, current_version FROM coordinated_bid_baseline_head
                 WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let (stored_count, maximum): (u32, u32) = self
            .connection
            .query_row(
                "SELECT COUNT(*), MAX(version) FROM coordinated_bid_baseline_versions
                 WHERE baseline_id = ?1",
                [&baseline_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if stored_count != maximum || head_version != maximum {
            return Ok(false);
        }
        let budget = BidPackageOperationBudget::for_tender(&TenderId::parse(
            &self
                .connection
                .query_row(
                    "SELECT tender_id FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sql_error)?,
        )?);
        let mut prior_manifest = None;
        let mut prior_approval = ZERO_HASH.to_owned();
        for version in 1..=head_version {
            check()?;
            let baseline = match self.load_coordinated_bid_baseline(&baseline_id, version, budget) {
                Ok(value) => value,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error)
                }
                Err(_) => return Ok(false),
            };
            if baseline.preceding_version_manifest_sha256 != prior_manifest {
                return Ok(false);
            }
            let baseline_audit_count: u32 = self.connection.query_row(
                "SELECT COUNT(*)
                 FROM coordinated_bid_baseline_versions AS versions
                 JOIN audit_events AS audit ON audit.sequence = versions.audit_sequence
                 WHERE versions.baseline_id = ?1 AND versions.version = ?2
                   AND audit.event_type = 'coordinated_bid_baseline_proposed'
                   AND audit.aggregate_revision = versions.tender_revision
                   AND audit.created_at = versions.created_at
                   AND json_extract(audit.payload_json, '$.change.baseline_id') = versions.baseline_id
                   AND json_extract(audit.payload_json, '$.change.version') = CAST(versions.version AS TEXT)
                   AND json_extract(audit.payload_json, '$.change.manifest_sha256') = versions.manifest_sha256
                   AND json_extract(audit.payload_json, '$.change.coordinator_profile_id') = versions.coordinator_profile_id
                   AND json_extract(audit.payload_json, '$.change.coordinator_profile_version') = CAST(versions.coordinator_profile_version AS TEXT)
                   AND json_extract(audit.payload_json, '$.change.binding_count') = CAST(json_array_length(versions.bindings_json) AS TEXT)
                   AND json_extract(audit.payload_json, '$.change.contradiction_count') = CAST(json_array_length(versions.contradictions_json) AS TEXT)
                   AND json_extract(audit.payload_json, '$.change.blocker_count') = CAST(json_array_length(versions.blockers_json) AS TEXT)
                   AND json_extract(audit.payload_json, '$.change.lifecycle_before') IN ('active_production', 'integrated_review')
                   AND json_extract(audit.payload_json, '$.change.lifecycle_after') =
                       CASE WHEN json_array_length(versions.blockers_json) = 0
                                  AND json_array_length(versions.contradictions_json) = 0
                            THEN 'integrated_review'
                            ELSE 'active_production' END
                   AND (json_array_length(versions.blockers_json) = 0
                        AND json_array_length(versions.contradictions_json) = 0
                        OR json_extract(audit.payload_json, '$.change.lifecycle_before') = 'active_production')",
                params![baseline_id, version],
                |row| row.get(0),
            ).map_err(sql_error)?;
            if baseline_audit_count != 1 {
                return Ok(false);
            }
            if !self.baseline_bindings_are_valid(&baseline.bindings, check)? {
                return Ok(false);
            }
            if let Some(approval) = &baseline.approval {
                let expected_lifecycle_after = match approval.decision {
                    CoordinatedBidBaselineDecision::Approve => {
                        TenderLifecyclePhase::PackageProduction
                    }
                    CoordinatedBidBaselineDecision::Return
                    | CoordinatedBidBaselineDecision::Reject => {
                        TenderLifecyclePhase::ActiveProduction
                    }
                };
                if approval.preceding_approval_hash != prior_approval
                    || approval.baseline_manifest_sha256 != baseline.manifest_sha256
                    || approval.supporting_reviews_sha256
                        != supporting_reviews_sha256(&baseline.bindings)?
                    || approval.decided_by != "engineer_user"
                    || approval.acting_role != "tendering_manager"
                    || approval.lifecycle_before != TenderLifecyclePhase::IntegratedReview
                    || approval.lifecycle_after != expected_lifecycle_after
                    || !valid_hash(&approval.approval_sha256)
                {
                    return Ok(false);
                }
                let approval_audit_count: u32 = self.connection.query_row(
                    "SELECT COUNT(*)
                     FROM coordinated_bid_baseline_approvals AS approvals
                     JOIN coordinated_bid_baseline_versions AS versions
                       ON versions.baseline_id = approvals.baseline_id
                      AND versions.version = approvals.baseline_version
                     JOIN audit_events AS audit ON audit.sequence = approvals.audit_sequence
                     WHERE approvals.approval_id = ?1
                       AND audit.event_type = 'coordinated_bid_baseline_decided'
                       AND audit.aggregate_revision = versions.tender_revision
                       AND audit.created_at = approvals.created_at
                       AND json_extract(audit.payload_json, '$.change.approval_id') = approvals.approval_id
                       AND json_extract(audit.payload_json, '$.change.approval_sha256') = approvals.approval_sha256
                       AND json_extract(audit.payload_json, '$.change.baseline_id') = approvals.baseline_id
                       AND json_extract(audit.payload_json, '$.change.baseline_version') = CAST(approvals.baseline_version AS TEXT)
                       AND json_extract(audit.payload_json, '$.change.baseline_manifest_sha256') = approvals.baseline_manifest_sha256
                       AND json_extract(audit.payload_json, '$.change.decision') = approvals.decision
                       AND json_extract(audit.payload_json, '$.change.rationale') = approvals.rationale
                       AND json_extract(audit.payload_json, '$.change.conditions') = json(approvals.conditions_json)
                       AND json_extract(audit.payload_json, '$.change.exceptions') = json(approvals.exceptions_json)
                       AND json_extract(audit.payload_json, '$.change.supporting_reviews_sha256') = approvals.supporting_reviews_sha256
                       AND json_extract(audit.payload_json, '$.change.decided_by') = approvals.decided_by
                       AND json_extract(audit.payload_json, '$.change.acting_role') = approvals.acting_role
                       AND json_extract(audit.payload_json, '$.change.lifecycle_before') = approvals.lifecycle_before
                       AND json_extract(audit.payload_json, '$.change.lifecycle_after') = approvals.lifecycle_after
                       AND json_extract(audit.payload_json, '$.change.preceding_approval_hash') = approvals.preceding_approval_hash",
                    [&approval.approval_id],
                    |row| row.get(0),
                ).map_err(sql_error)?;
                if approval_audit_count != 1 {
                    return Ok(false);
                }
                prior_approval = approval.approval_sha256.clone();
            }
            prior_manifest = Some(baseline.manifest_sha256);
        }
        let lifecycle = self.lifecycle_phase()?;
        let head = self.load_coordinated_bid_baseline(&baseline_id, head_version, budget)?;
        let expected_lifecycle = match head.approval.as_ref().map(|approval| approval.decision) {
            Some(CoordinatedBidBaselineDecision::Approve) => {
                if !head.current || !head.blockers.is_empty() || !head.contradictions.is_empty() {
                    return Ok(false);
                }
                TenderLifecyclePhase::PackageProduction
            }
            Some(CoordinatedBidBaselineDecision::Return)
            | Some(CoordinatedBidBaselineDecision::Reject) => {
                TenderLifecyclePhase::ActiveProduction
            }
            None if head.blockers.is_empty() && head.contradictions.is_empty() => {
                if !head.current {
                    return Ok(false);
                }
                TenderLifecyclePhase::IntegratedReview
            }
            None => TenderLifecyclePhase::ActiveProduction,
        };
        if lifecycle != expected_lifecycle {
            return Ok(false);
        }
        Ok(true)
    }

    fn baseline_bindings_are_valid(
        &self,
        bindings: &[CoordinatedBidBaselineBinding],
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        if bindings.len() > MAX_BASELINE_BINDINGS {
            return Ok(false);
        }
        for binding in bindings {
            check()?;
            if !valid_hash(&binding.manifest_sha256)
                || binding.reference_id.is_empty()
                || binding.version == 0
            {
                return Ok(false);
            }
            if !self.baseline_binding_is_valid(binding)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn baseline_binding_is_valid(
        &self,
        binding: &CoordinatedBidBaselineBinding,
    ) -> Result<bool, TenderCommandError> {
        match binding.kind {
            CoordinatedBidBaselineBindingKind::ProductionArtifactVersion => {
                type Row = (String, String, Option<String>, String, String);
                let row: Option<Row> = self
                    .connection
                    .query_row(
                        "SELECT tasks.task_key, tasks.task_definition_json, readiness.review_id,
                                readiness.finding_dispositions_sha256, artifacts.payload_sha256
                         FROM production_artifact_versions AS artifacts
                         JOIN production_integration_readiness AS readiness
                           ON readiness.artifact_id = artifacts.artifact_id
                          AND readiness.artifact_version = artifacts.version
                         JOIN production_tasks AS tasks
                           ON tasks.production_task_id = readiness.production_task_id
                         WHERE artifacts.artifact_id = ?1 AND artifacts.version = ?2",
                        params![binding.reference_id, binding.version],
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
                let Some((task_key, definition_json, review_id, dispositions, payload)) = row
                else {
                    return Ok(false);
                };
                let definition: Value = parse_canonical(&definition_json)?;
                let workstream = definition
                    .get("workstream_key")
                    .and_then(Value::as_str)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                let objective = definition
                    .get("objective")
                    .and_then(Value::as_str)
                    .unwrap_or(workstream);
                let digest = sha256_hex(
                    canonical_json(&json!({
                        "artifact_payload_sha256": payload,
                        "review_id": review_id,
                        "finding_dispositions_sha256": dispositions,
                    }))?
                    .as_bytes(),
                );
                Ok(binding.category == category_for_workstream(workstream)
                    && binding.manifest_sha256 == digest
                    && binding.source == task_key
                    && binding.summary == objective
                    && binding.supporting_review_id == review_id
                    && binding.approval_id.is_none())
            }
            CoordinatedBidBaselineBindingKind::TenderRecordVersion => {
                let record = match self
                    .inspect_tender_record_version(&binding.reference_id, binding.version)
                {
                    Ok(record) => record,
                    Err(_) => return Ok(false),
                };
                let supporting_review_id =
                    record.reviews.last().map(|review| review.review_id.clone());
                let immutable_record = json!({
                    "record_id": record.record_id,
                    "stable_key": record.stable_key,
                    "version": record.version,
                    "kind": record.kind,
                    "title": record.title,
                    "fields": record.fields,
                    "contradictions": record.contradictions,
                    "author_run_id": record.author_run_id,
                    "author_profile_id": record.author_profile_id,
                    "supporting_review_id": supporting_review_id,
                    "trust_class": record.trust_class,
                });
                let digest = sha256_hex(canonical_json(&immutable_record)?.as_bytes());
                Ok(
                    binding.category == category_for_record(record.kind, &record.title)
                        && binding.manifest_sha256 == digest
                        && binding.source == record.stable_key
                        && binding.summary == record.title
                        && binding.supporting_review_id == supporting_review_id
                        && binding.approval_id.is_none(),
                )
            }
            CoordinatedBidBaselineBindingKind::TenderQueryVersion => {
                type Row = (
                    String,
                    bool,
                    bool,
                    Option<String>,
                    Option<String>,
                    Option<bool>,
                    Option<String>,
                );
                let row: Option<Row> = self
                    .connection
                    .query_row(
                        "SELECT versions.manifest_sha256, versions.material,
                            versions.release_blocking, decisions.decision_id,
                            decisions.treatment, decisions.closes_query,
                            decisions.manifest_sha256
                     FROM tender_query_versions AS versions
                     LEFT JOIN tender_query_treatment_decisions AS decisions
                       ON decisions.query_id = versions.query_id
                      AND decisions.query_version = versions.version
                     WHERE versions.query_id = ?1 AND versions.version = ?2",
                        params![binding.reference_id, binding.version],
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
                let Some((query_manifest, _, _, decision_id, treatment, _, decision_manifest)) =
                    row
                else {
                    return Ok(false);
                };
                let digest = sha256_hex(
                    canonical_json(&json!({
                        "query_manifest_sha256": query_manifest,
                        "decision_id": decision_id,
                        "decision_manifest_sha256": decision_manifest,
                    }))?
                    .as_bytes(),
                );
                let category = match treatment.as_deref() {
                    Some("qualification") | Some("approved_assumption") => {
                        CoordinatedBidBaselineCategory::Qualification
                    }
                    Some("exclusion") => CoordinatedBidBaselineCategory::Exclusion,
                    Some("allowance") => CoordinatedBidBaselineCategory::Commercial,
                    _ => CoordinatedBidBaselineCategory::Query,
                };
                Ok(binding.category == category
                    && binding.manifest_sha256 == digest
                    && binding.source == "query_register"
                    && binding.summary == treatment.unwrap_or_else(|| "unresolved_query".into())
                    && binding.supporting_review_id.is_none()
                    && binding.approval_id == decision_id)
            }
            CoordinatedBidBaselineBindingKind::ExternalRfiVersion => {
                let row: Option<(String, String, String)> = self
                    .connection
                    .query_row(
                        "SELECT versions.manifest_sha256, approvals.approval_id,
                            approvals.approval_sha256
                     FROM external_rfi_versions AS versions
                     JOIN external_rfi_approvals AS approvals
                       ON approvals.rfi_id = versions.rfi_id
                      AND approvals.rfi_version = versions.version
                     WHERE versions.rfi_id = ?1 AND versions.version = ?2",
                        params![binding.reference_id, binding.version],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                let Some((version_manifest, approval_id, approval_manifest)) = row else {
                    return Ok(false);
                };
                let digest = sha256_hex(
                    canonical_json(&json!({
                        "version": version_manifest,
                        "approval": approval_manifest,
                    }))?
                    .as_bytes(),
                );
                Ok(binding.category == CoordinatedBidBaselineCategory::Query
                    && binding.manifest_sha256 == digest
                    && binding.source == "external_rfi_register"
                    && binding.summary == "Approved exact External RFI version"
                    && binding.supporting_review_id.is_none()
                    && binding.approval_id.as_deref() == Some(approval_id.as_str()))
            }
            CoordinatedBidBaselineBindingKind::PricedCostBaseline => {
                let row: Option<(String, String, String)> = self
                    .connection
                    .query_row(
                        "SELECT versions.manifest_sha256, approvals.approval_id, reviews.review_id
                     FROM priced_cost_baseline_versions AS versions
                     JOIN priced_cost_baseline_approvals AS approvals
                       ON approvals.baseline_id = versions.baseline_id
                      AND approvals.baseline_version = versions.version
                     JOIN priced_cost_baseline_reviews AS reviews
                       ON reviews.review_id = approvals.review_id
                     WHERE versions.baseline_id = ?1 AND versions.version = ?2
                       AND reviews.outcome = 'passed'",
                        params![binding.reference_id, binding.version],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                Ok(row.is_some_and(|(manifest, approval_id, review_id)| {
                    binding.category == CoordinatedBidBaselineCategory::Commercial
                        && binding.manifest_sha256 == manifest
                        && binding.source == "priced_cost_baseline"
                        && binding.summary
                            == "Independently reviewed and EITL-approved expected delivery cost"
                        && binding.supporting_review_id.as_deref() == Some(review_id.as_str())
                        && binding.approval_id.as_deref() == Some(approval_id.as_str())
                }))
            }
            CoordinatedBidBaselineBindingKind::ApprovedTenderPrice => {
                let row: Option<(String, String)> = self
                    .connection
                    .query_row(
                        "SELECT manifest_sha256, approval_id FROM approved_tender_prices
                     WHERE pricing_scenario_id = ?1 AND pricing_scenario_version = ?2",
                        params![binding.reference_id, binding.version],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                Ok(row.is_some_and(|(manifest, approval_id)| {
                    binding.category == CoordinatedBidBaselineCategory::Commercial
                        && binding.manifest_sha256 == manifest
                        && binding.source == "approved_tender_price"
                        && binding.summary == "Exact selected and EITL-approved Final Price"
                        && binding.supporting_review_id.is_none()
                        && binding.approval_id.as_deref() == Some(approval_id.as_str())
                }))
            }
            CoordinatedBidBaselineBindingKind::CalculationManifest => {
                let manifest: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT manifest_sha256 FROM pricing_calculation_runs
                     WHERE pricing_calculation_run_id = ?1",
                        [binding.reference_id.as_str()],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
                Ok(manifest.is_some_and(|manifest| {
                    binding.category == CoordinatedBidBaselineCategory::Commercial
                        && binding.version == 1
                        && binding.manifest_sha256 == manifest
                        && binding.source == "pricing_calculation"
                        && binding.summary == "Controlled Final Price Calculation Manifest"
                        && binding.supporting_review_id.is_none()
                        && binding.approval_id.is_none()
                }))
            }
            CoordinatedBidBaselineBindingKind::CommercialStrategy => {
                let row: Option<(String, String)> = self
                    .connection
                    .query_row(
                        "SELECT strategies.manifest_json, approvals.approval_id
                     FROM commercial_strategies AS strategies
                     JOIN commercial_strategy_approvals AS approvals
                       ON approvals.strategy_id = strategies.strategy_id
                     WHERE strategies.strategy_id = ?1",
                        [binding.reference_id.as_str()],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                let Some((manifest_json, approval_id)) = row else {
                    return Ok(false);
                };
                let manifest: Value = parse_canonical(&manifest_json)?;
                let field = match binding.category {
                    CoordinatedBidBaselineCategory::Qualification => "qualifications",
                    CoordinatedBidBaselineCategory::Exclusion => "exclusions",
                    _ => return Ok(false),
                };
                let review_id = manifest.get("input_review_id").and_then(Value::as_str);
                Ok(manifest.get(field).and_then(Value::as_array).is_some()
                    && binding.version == 1
                    && binding.manifest_sha256 == sha256_hex(manifest_json.as_bytes())
                    && binding.source == "commercial_strategy"
                    && binding.summary == format!("Approved commercial {field}")
                    && binding.supporting_review_id.as_deref() == review_id
                    && binding.approval_id.as_deref() == Some(approval_id.as_str()))
            }
        }
    }
}

fn category_for_workstream(value: &str) -> CoordinatedBidBaselineCategory {
    let value = value.to_ascii_lowercase();
    if value.contains("cost") || value.contains("commercial") || value.contains("pricing") {
        CoordinatedBidBaselineCategory::Commercial
    } else if value.contains("document") || value.contains("submission") {
        CoordinatedBidBaselineCategory::Submission
    } else if value.contains("programme") || value.contains("schedule") {
        CoordinatedBidBaselineCategory::Programme
    } else if value.contains("procurement") || value.contains("supplier") {
        CoordinatedBidBaselineCategory::Procurement
    } else if value.contains("contract") {
        CoordinatedBidBaselineCategory::Contractual
    } else if value.contains("risk") || value.contains("assurance") {
        CoordinatedBidBaselineCategory::Risk
    } else if value.contains("query") || value.contains("rfi") {
        CoordinatedBidBaselineCategory::Query
    } else {
        CoordinatedBidBaselineCategory::Technical
    }
}

fn coordination_deadline_is_required(workstream: &str) -> bool {
    let workstream = workstream.to_ascii_lowercase();
    workstream.contains("programme")
        || workstream.contains("schedule")
        || workstream.contains("coordination")
        || workstream.contains("document")
        || workstream.contains("submission")
}

fn category_for_record(kind: TenderRecordKind, title: &str) -> CoordinatedBidBaselineCategory {
    match kind {
        TenderRecordKind::Deadline => CoordinatedBidBaselineCategory::Programme,
        TenderRecordKind::Requirement => {
            if title.to_ascii_lowercase().contains("contract") {
                CoordinatedBidBaselineCategory::Contractual
            } else {
                CoordinatedBidBaselineCategory::Procurement
            }
        }
        TenderRecordKind::Clause => CoordinatedBidBaselineCategory::Contractual,
        TenderRecordKind::Risk => CoordinatedBidBaselineCategory::Risk,
        TenderRecordKind::Assumption => CoordinatedBidBaselineCategory::Qualification,
        TenderRecordKind::TenderQuery => CoordinatedBidBaselineCategory::Query,
        TenderRecordKind::Deliverable | TenderRecordKind::Form => {
            CoordinatedBidBaselineCategory::Submission
        }
        TenderRecordKind::EvaluationCriterion | TenderRecordKind::ProjectCharacteristic => {
            CoordinatedBidBaselineCategory::Technical
        }
    }
}

fn recognized_contradiction_category(
    name: &str,
) -> Option<CoordinatedBidBaselineContradictionCategory> {
    let value = name.to_ascii_lowercase();
    if value.contains("date") || value.contains("deadline") || value.contains("time") {
        Some(CoordinatedBidBaselineContradictionCategory::Date)
    } else if value.contains("responsib") || value.contains("owner") || value.contains("party") {
        Some(CoordinatedBidBaselineContradictionCategory::Responsibility)
    } else if value.contains("qualification") || value.contains("assumption") {
        Some(CoordinatedBidBaselineContradictionCategory::Qualification)
    } else if value.contains("exclusion") {
        Some(CoordinatedBidBaselineContradictionCategory::Exclusion)
    } else if value.contains("amount")
        || value.contains("currency")
        || value.contains("rate")
        || value.contains("quantity")
        || value.contains("price")
        || value.contains("cost")
        || value.contains("calculation")
        || value.contains("total")
    {
        Some(CoordinatedBidBaselineContradictionCategory::Calculation)
    } else if value == "value" {
        Some(CoordinatedBidBaselineContradictionCategory::Value)
    } else if value.contains("commitment") || value.contains("requirement") {
        Some(CoordinatedBidBaselineContradictionCategory::Commitment)
    } else {
        None
    }
}

fn contradiction_category(name: &str) -> CoordinatedBidBaselineContradictionCategory {
    recognized_contradiction_category(name)
        .unwrap_or(CoordinatedBidBaselineContradictionCategory::Commitment)
}

fn coordination_observation_scope(
    subject: ProductionCoordinationObservationSubject,
    task_key: &str,
) -> String {
    match subject {
        ProductionCoordinationObservationSubject::QueryTreatment => normalize_token(task_key),
        _ => "global".into(),
    }
}

fn coordination_observation_is_keyed(subject: ProductionCoordinationObservationSubject) -> bool {
    matches!(
        subject,
        ProductionCoordinationObservationSubject::SubmissionDeadline
            | ProductionCoordinationObservationSubject::ResponsibleParty
            | ProductionCoordinationObservationSubject::ScopeQualification
            | ProductionCoordinationObservationSubject::ScopeExclusion
            | ProductionCoordinationObservationSubject::TechnicalCommitment
            | ProductionCoordinationObservationSubject::ProgrammeCommitment
            | ProductionCoordinationObservationSubject::ProcurementCommitment
            | ProductionCoordinationObservationSubject::ContractualCommitment
            | ProductionCoordinationObservationSubject::RiskCommitment
            | ProductionCoordinationObservationSubject::SubmissionCommitment
    )
}

fn observation_category(
    subject: ProductionCoordinationObservationSubject,
) -> CoordinatedBidBaselineContradictionCategory {
    match subject {
        ProductionCoordinationObservationSubject::SubmissionDeadline => {
            CoordinatedBidBaselineContradictionCategory::Date
        }
        ProductionCoordinationObservationSubject::ResponsibleParty => {
            CoordinatedBidBaselineContradictionCategory::Responsibility
        }
        ProductionCoordinationObservationSubject::ScopeQualification => {
            CoordinatedBidBaselineContradictionCategory::Qualification
        }
        ProductionCoordinationObservationSubject::ScopeExclusion => {
            CoordinatedBidBaselineContradictionCategory::Exclusion
        }
        ProductionCoordinationObservationSubject::ExpectedDeliveryCost
        | ProductionCoordinationObservationSubject::ApprovedTenderPrice => {
            CoordinatedBidBaselineContradictionCategory::Calculation
        }
        ProductionCoordinationObservationSubject::CommercialAppetite
        | ProductionCoordinationObservationSubject::TechnicalCommitment
        | ProductionCoordinationObservationSubject::ProgrammeCommitment
        | ProductionCoordinationObservationSubject::ProcurementCommitment
        | ProductionCoordinationObservationSubject::ContractualCommitment
        | ProductionCoordinationObservationSubject::RiskCommitment
        | ProductionCoordinationObservationSubject::SubmissionCommitment
        | ProductionCoordinationObservationSubject::QueryTreatment => {
            CoordinatedBidBaselineContradictionCategory::Commitment
        }
    }
}

fn observation_key(subject: ProductionCoordinationObservationSubject, scope: &str) -> String {
    let subject = match subject {
        ProductionCoordinationObservationSubject::SubmissionDeadline => "submission_deadline",
        ProductionCoordinationObservationSubject::ResponsibleParty => "responsible_party",
        ProductionCoordinationObservationSubject::ScopeQualification => "scope_qualification",
        ProductionCoordinationObservationSubject::ScopeExclusion => "scope_exclusion",
        ProductionCoordinationObservationSubject::ExpectedDeliveryCost => "expected_delivery_cost",
        ProductionCoordinationObservationSubject::ApprovedTenderPrice => "approved_tender_price",
        ProductionCoordinationObservationSubject::CommercialAppetite => "commercial_appetite",
        ProductionCoordinationObservationSubject::TechnicalCommitment => "technical_commitment",
        ProductionCoordinationObservationSubject::ProgrammeCommitment => "programme_commitment",
        ProductionCoordinationObservationSubject::ProcurementCommitment => "procurement_commitment",
        ProductionCoordinationObservationSubject::ContractualCommitment => "contractual_commitment",
        ProductionCoordinationObservationSubject::RiskCommitment => "risk_commitment",
        ProductionCoordinationObservationSubject::SubmissionCommitment => "submission_commitment",
        ProductionCoordinationObservationSubject::QueryTreatment => "query_treatment",
    };
    if scope == "global" {
        subject.into()
    } else {
        format!("{subject}:{scope}")
    }
}

fn scalar_observation_subject(subject: ProductionCoordinationObservationSubject) -> bool {
    matches!(
        subject,
        ProductionCoordinationObservationSubject::ExpectedDeliveryCost
            | ProductionCoordinationObservationSubject::ApprovedTenderPrice
            | ProductionCoordinationObservationSubject::CommercialAppetite
            | ProductionCoordinationObservationSubject::QueryTreatment
    )
}

fn collect_field_contradictions(
    observations: &[FieldObservation],
    contradictions: &mut Vec<CoordinatedBidBaselineContradiction>,
) -> Result<(), TenderCommandError> {
    let mut groups: BTreeMap<
        (ProductionCoordinationObservationSubject, String),
        Vec<&FieldObservation>,
    > = BTreeMap::new();
    for observation in observations {
        if !scalar_observation_subject(observation.subject) {
            continue;
        }
        groups
            .entry((observation.subject, observation.scope.clone()))
            .or_default()
            .push(observation);
    }
    for ((subject, scope), values) in groups {
        let distinct = values
            .iter()
            .map(|value| value.value.as_str())
            .collect::<BTreeSet<_>>();
        if distinct.len() <= 1 {
            continue;
        }
        if contradictions.len() >= MAX_BASELINE_CONTRADICTIONS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let category = observation_category(subject);
        let key = observation_key(subject, &scope);
        contradictions.push(CoordinatedBidBaselineContradiction {
            category,
            key: key.clone(),
            summary: format!("Conflicting current values were found for {key}."),
            references: values.iter().map(|value| value.reference.clone()).collect(),
        });
    }
    let mut deadlines: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for observation in observations.iter().filter(|observation| {
        observation.subject == ProductionCoordinationObservationSubject::SubmissionDeadline
    }) {
        let assignments: Vec<String> = parse_canonical(&observation.value)?;
        for assignment in assignments {
            let (milestone, timestamp) = assignment
                .split_once('=')
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            match deadlines.get_mut(milestone) {
                Some((expected_timestamp, references)) if expected_timestamp != timestamp => {
                    let mut references = references.clone();
                    references.push(observation.reference.clone());
                    push_coordination_contradiction(
                        contradictions,
                        CoordinatedBidBaselineContradictionCategory::Date,
                        format!("deadline:{milestone}"),
                        format!("Conflicting current dates were found for {milestone}."),
                        references,
                    )?;
                }
                Some((_, references)) => references.push(observation.reference.clone()),
                None => {
                    deadlines.insert(
                        milestone.into(),
                        (timestamp.into(), vec![observation.reference.clone()]),
                    );
                }
            }
        }
    }
    let mut responsibilities: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    for observation in observations.iter().filter(|observation| {
        observation.subject == ProductionCoordinationObservationSubject::ResponsibleParty
    }) {
        let assignments: Vec<String> = parse_canonical(&observation.value)?;
        for assignment in assignments {
            let (responsibility, party) = assignment
                .split_once('=')
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            match responsibilities.get_mut(responsibility) {
                Some((expected_party, references)) if expected_party != party => {
                    let mut references = references.clone();
                    references.push(observation.reference.clone());
                    push_coordination_contradiction(
                        contradictions,
                        CoordinatedBidBaselineContradictionCategory::Responsibility,
                        format!("responsibility:{responsibility}"),
                        format!(
                            "Conflicting accountable parties were assigned for {responsibility}."
                        ),
                        references,
                    )?;
                }
                Some((_, references)) => references.push(observation.reference.clone()),
                None => {
                    responsibilities.insert(
                        responsibility.into(),
                        (party.into(), vec![observation.reference.clone()]),
                    );
                }
            }
        }
    }
    let keyed_exclusion_items = observation_set_by_representation(
        observations,
        ProductionCoordinationObservationSubject::ScopeExclusion,
        true,
    )?;
    let unkeyed_exclusion_items = observation_set_by_representation(
        observations,
        ProductionCoordinationObservationSubject::ScopeExclusion,
        false,
    )?;
    let keyed_qualification_items = observation_set_by_representation(
        observations,
        ProductionCoordinationObservationSubject::ScopeQualification,
        true,
    )?;
    let unkeyed_qualification_items = observation_set_by_representation(
        observations,
        ProductionCoordinationObservationSubject::ScopeQualification,
        false,
    )?;
    let SemanticObservationSet {
        references: semantic_exclusions,
        keyed_values: keyed_exclusion_values,
        unkeyed_values: unkeyed_exclusion_values,
    } = semantic_observation_set(
        observations,
        ProductionCoordinationObservationSubject::ScopeExclusion,
    )?;
    let SemanticObservationSet {
        references: semantic_qualifications,
        keyed_values: keyed_qualification_values,
        unkeyed_values: unkeyed_qualification_values,
    } = semantic_observation_set(
        observations,
        ProductionCoordinationObservationSubject::ScopeQualification,
    )?;
    let mut scoped_dispositions: BTreeMap<
        String,
        BTreeMap<(ProductionCoordinationObservationSubject, String), Vec<String>>,
    > = BTreeMap::new();
    for (subject, values) in [
        (
            ProductionCoordinationObservationSubject::ScopeQualification,
            &keyed_qualification_items,
        ),
        (
            ProductionCoordinationObservationSubject::ScopeExclusion,
            &keyed_exclusion_items,
        ),
    ] {
        for (item, references) in values {
            let Some((scope_key, disposition)) = item.split_once('=') else {
                continue;
            };
            scoped_dispositions
                .entry(scope_key.into())
                .or_default()
                .entry((subject, disposition.into()))
                .or_default()
                .extend(references.iter().cloned());
        }
    }
    for (scope_key, dispositions) in scoped_dispositions {
        if dispositions.len() <= 1 {
            continue;
        }
        let categories = dispositions
            .keys()
            .map(|(subject, _)| *subject)
            .collect::<BTreeSet<_>>();
        let category = if categories.len() > 1 {
            CoordinatedBidBaselineContradictionCategory::Qualification
        } else {
            observation_category(
                *categories
                    .first()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            )
        };
        push_coordination_contradiction(
            contradictions,
            category,
            format!("scope_disposition:{scope_key}"),
            format!("Conflicting current scope dispositions were found for {scope_key}."),
            dispositions.into_values().flatten().collect(),
        )?;
    }
    for (item, exclusion_references) in &unkeyed_exclusion_items {
        if let Some(qualification_references) = unkeyed_qualification_items.get(item) {
            let mut references = exclusion_references.clone();
            references.extend(qualification_references.iter().cloned());
            push_coordination_contradiction(
                contradictions,
                CoordinatedBidBaselineContradictionCategory::Qualification,
                format!("scope_disposition:{}", sha256_hex(item.as_bytes())),
                "The same exact scope item is both qualified and excluded.".into(),
                references,
            )?;
        }
    }
    for (item, exclusion_references) in &semantic_exclusions {
        let Some(qualification_references) = semantic_qualifications.get(item) else {
            continue;
        };
        let crosses_representation = (keyed_exclusion_values.contains(item)
            && unkeyed_qualification_values.contains(item))
            || (unkeyed_exclusion_values.contains(item)
                && keyed_qualification_values.contains(item));
        if !crosses_representation {
            continue;
        }
        let mut references = exclusion_references.clone();
        references.extend(qualification_references.iter().cloned());
        push_coordination_contradiction(
            contradictions,
            CoordinatedBidBaselineContradictionCategory::Qualification,
            format!("scope_disposition:semantic:{}", sha256_hex(item.as_bytes())),
            "The same exact normalized scope item is both qualified and excluded.".into(),
            references,
        )?;
    }
    let mut commitments_by_key: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for subject in [
        ProductionCoordinationObservationSubject::TechnicalCommitment,
        ProductionCoordinationObservationSubject::ProgrammeCommitment,
        ProductionCoordinationObservationSubject::ProcurementCommitment,
        ProductionCoordinationObservationSubject::ContractualCommitment,
        ProductionCoordinationObservationSubject::RiskCommitment,
        ProductionCoordinationObservationSubject::SubmissionCommitment,
    ] {
        let commitments = observation_set(observations, subject)?;
        for (item, commitment_references) in commitments {
            let (commitment_key, commitment_value) = item
                .split_once('=')
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            commitments_by_key
                .entry(commitment_key.into())
                .or_default()
                .entry(commitment_value.into())
                .or_default()
                .extend(commitment_references.iter().cloned());
            if let Some(exclusion_references) = keyed_exclusion_items.get(&item) {
                let mut references = commitment_references.clone();
                references.extend(exclusion_references.iter().cloned());
                push_coordination_contradiction(
                    contradictions,
                    CoordinatedBidBaselineContradictionCategory::Commitment,
                    format!("commitment_exclusion:{}", sha256_hex(item.as_bytes())),
                    "An exact current commitment is also excluded by the commercial strategy."
                        .into(),
                    references,
                )?;
            }
            if let Some(exclusion_references) = unkeyed_exclusion_values
                .contains(commitment_value)
                .then(|| semantic_exclusions.get(commitment_value))
                .flatten()
            {
                let mut references = commitment_references.clone();
                references.extend(exclusion_references.iter().cloned());
                push_coordination_contradiction(
                    contradictions,
                    CoordinatedBidBaselineContradictionCategory::Commitment,
                    format!(
                        "commitment_exclusion:semantic:{}",
                        sha256_hex(commitment_value.as_bytes())
                    ),
                    "An exact current commitment is excluded by the commercial strategy.".into(),
                    references,
                )?;
            }
        }
    }
    for (commitment_key, values) in commitments_by_key {
        if values.len() <= 1 {
            continue;
        }
        push_coordination_contradiction(
            contradictions,
            CoordinatedBidBaselineContradictionCategory::Commitment,
            format!("commitment:{commitment_key}"),
            format!("Conflicting current commitments were found for {commitment_key}."),
            values.into_values().flatten().collect(),
        )?;
    }
    Ok(())
}

fn observation_set(
    observations: &[FieldObservation],
    subject: ProductionCoordinationObservationSubject,
) -> Result<BTreeMap<String, Vec<String>>, TenderCommandError> {
    let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for observation in observations
        .iter()
        .filter(|observation| observation.subject == subject)
    {
        let values: Vec<String> = parse_canonical(&observation.value)?;
        for value in values {
            result
                .entry(value)
                .or_default()
                .push(observation.reference.clone());
        }
    }
    Ok(result)
}

fn observation_set_by_representation(
    observations: &[FieldObservation],
    subject: ProductionCoordinationObservationSubject,
    keyed: bool,
) -> Result<BTreeMap<String, Vec<String>>, TenderCommandError> {
    let mut result = BTreeMap::<String, Vec<String>>::new();
    for observation in observations
        .iter()
        .filter(|observation| observation.subject == subject && observation.keyed == keyed)
    {
        let values: Vec<String> = parse_canonical(&observation.value)?;
        for value in values {
            result
                .entry(value)
                .or_default()
                .push(observation.reference.clone());
        }
    }
    Ok(result)
}

fn semantic_observation_set(
    observations: &[FieldObservation],
    subject: ProductionCoordinationObservationSubject,
) -> Result<SemanticObservationSet, TenderCommandError> {
    let mut semantic = BTreeMap::<String, Vec<String>>::new();
    let mut keyed = BTreeSet::new();
    let mut unkeyed = BTreeSet::new();
    for observation in observations
        .iter()
        .filter(|observation| observation.subject == subject)
    {
        let values: Vec<String> = parse_canonical(&observation.value)?;
        for item in values {
            let value = if observation.keyed {
                item.split_once('=')
                    .map(|(_, assigned)| assigned.to_owned())
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            } else {
                item
            };
            if observation.keyed {
                keyed.insert(value.clone());
            } else {
                unkeyed.insert(value.clone());
            }
            semantic
                .entry(value)
                .or_default()
                .push(observation.reference.clone());
        }
    }
    Ok(SemanticObservationSet {
        references: semantic,
        keyed_values: keyed,
        unkeyed_values: unkeyed,
    })
}

fn push_coordination_contradiction(
    contradictions: &mut Vec<CoordinatedBidBaselineContradiction>,
    category: CoordinatedBidBaselineContradictionCategory,
    key: String,
    summary: String,
    mut references: Vec<String>,
) -> Result<(), TenderCommandError> {
    if contradictions.len() >= MAX_BASELINE_CONTRADICTIONS {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    references.sort();
    references.dedup();
    contradictions.push(CoordinatedBidBaselineContradiction {
        category,
        key,
        summary,
        references,
    });
    Ok(())
}

fn sort_and_validate_snapshot(
    bindings: &mut Vec<CoordinatedBidBaselineBinding>,
    contradictions: &mut Vec<CoordinatedBidBaselineContradiction>,
    blockers: &mut Vec<CoordinatedBidBaselineBlocker>,
) -> Result<(), TenderCommandError> {
    bindings.sort_by(|left, right| {
        (left.category, left.kind, &left.reference_id, left.version).cmp(&(
            right.category,
            right.kind,
            &right.reference_id,
            right.version,
        ))
    });
    bindings.dedup();
    contradictions.sort_by(|left, right| {
        (left.category, &left.key, &left.references).cmp(&(
            right.category,
            &right.key,
            &right.references,
        ))
    });
    contradictions.dedup();
    blockers.sort_by(|left, right| {
        (blocker_rank(left.code), &left.summary, &left.references).cmp(&(
            blocker_rank(right.code),
            &right.summary,
            &right.references,
        ))
    });
    blockers.dedup();
    if bindings.len() > MAX_BASELINE_BINDINGS
        || contradictions.len() > MAX_BASELINE_CONTRADICTIONS
        || blockers.len() > MAX_BASELINE_BLOCKERS
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn blocker_rank(value: CoordinatedBidBaselineBlockerCode) -> u8 {
    match value {
        CoordinatedBidBaselineBlockerCode::ProductionTaskNotReady => 0,
        CoordinatedBidBaselineBlockerCode::OpenCriticalFinding => 1,
        CoordinatedBidBaselineBlockerCode::OpenMajorFinding => 2,
        CoordinatedBidBaselineBlockerCode::OpenMaterialQuery => 3,
        CoordinatedBidBaselineBlockerCode::StaleInput => 4,
        CoordinatedBidBaselineBlockerCode::UnverifiedInput => 5,
        CoordinatedBidBaselineBlockerCode::CapabilityGap => 6,
        CoordinatedBidBaselineBlockerCode::UnreconciledCalculation => 7,
        CoordinatedBidBaselineBlockerCode::PricedCostBaselineMissing => 8,
        CoordinatedBidBaselineBlockerCode::ApprovedTenderPriceMissing => 9,
        CoordinatedBidBaselineBlockerCode::WorkstreamEvidenceMissing => 10,
        CoordinatedBidBaselineBlockerCode::ContradictionOpen => 11,
    }
}

fn supporting_reviews_sha256(
    bindings: &[CoordinatedBidBaselineBinding],
) -> Result<String, TenderCommandError> {
    let reviews = bindings
        .iter()
        .filter_map(|binding| binding.supporting_review_id.clone())
        .collect::<BTreeSet<_>>();
    Ok(sha256_hex(canonical_json(&reviews)?.as_bytes()))
}

fn validate_unique_texts(values: &[String]) -> Result<(), TenderCommandError> {
    if values.len() > MAX_APPROVAL_ITEMS {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > 1_000 || !seen.insert(trimmed.to_owned()) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    Ok(())
}

fn normalize_text_items(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .collect()
}

fn normalize_token(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
}

fn parse_canonical<T: DeserializeOwned + Serialize>(value: &str) -> Result<T, TenderCommandError> {
    let parsed = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

#[cfg(test)]
mod coordination_reconciliation_tests {
    use super::*;

    fn observation(
        subject: ProductionCoordinationObservationSubject,
        values: &[&str],
        reference: &str,
        keyed: bool,
    ) -> FieldObservation {
        FieldObservation {
            subject,
            scope: "global".into(),
            value: canonical_json(&values).expect("canonical observation values"),
            reference: reference.into(),
            keyed,
        }
    }

    #[test]
    fn unkeyed_strategy_qualification_conflicts_with_keyed_record_exclusion() {
        let observations = vec![
            observation(
                ProductionCoordinationObservationSubject::ScopeExclusion,
                &["v:scope.v:disposition=night work"],
                "record",
                true,
            ),
            observation(
                ProductionCoordinationObservationSubject::ScopeQualification,
                &["night work"],
                "strategy",
                false,
            ),
        ];
        let mut contradictions = Vec::new();
        collect_field_contradictions(&observations, &mut contradictions)
            .expect("reconcile exact scope disposition");
        assert!(contradictions.iter().any(|contradiction| {
            contradiction.category == CoordinatedBidBaselineContradictionCategory::Qualification
                && contradiction.references == vec!["record", "strategy"]
        }));
    }

    #[test]
    fn unkeyed_strategy_exclusion_conflicts_with_keyed_record_commitment() {
        let observations = vec![
            observation(
                ProductionCoordinationObservationSubject::TechnicalCommitment,
                &["v:scope.v:commitment=night work"],
                "record",
                true,
            ),
            observation(
                ProductionCoordinationObservationSubject::ScopeExclusion,
                &["night work"],
                "strategy",
                false,
            ),
        ];
        let mut contradictions = Vec::new();
        collect_field_contradictions(&observations, &mut contradictions)
            .expect("reconcile exact commitment exclusion");
        assert!(contradictions.iter().any(|contradiction| {
            contradiction.category == CoordinatedBidBaselineContradictionCategory::Commitment
                && contradiction.references == vec!["record", "strategy"]
        }));
    }

    #[test]
    fn strategy_text_containing_equals_remains_unkeyed() {
        let observations = vec![
            observation(
                ProductionCoordinationObservationSubject::ScopeExclusion,
                &["v:scope.v:disposition=night=work"],
                "record",
                true,
            ),
            observation(
                ProductionCoordinationObservationSubject::ScopeQualification,
                &["night=work"],
                "strategy",
                false,
            ),
        ];
        let mut contradictions = Vec::new();
        collect_field_contradictions(&observations, &mut contradictions)
            .expect("preserve free-form strategy text");
        assert_eq!(contradictions.len(), 1);
        assert_eq!(contradictions[0].references, vec!["record", "strategy"]);
    }

    #[test]
    fn identical_unkeyed_scope_disposition_is_reported_once() {
        let observations = vec![
            observation(
                ProductionCoordinationObservationSubject::ScopeExclusion,
                &["night work"],
                "strategy-exclusion",
                false,
            ),
            observation(
                ProductionCoordinationObservationSubject::ScopeQualification,
                &["night work"],
                "strategy-qualification",
                false,
            ),
        ];
        let mut contradictions = Vec::new();
        collect_field_contradictions(&observations, &mut contradictions)
            .expect("reconcile one exact unkeyed scope conflict");
        assert_eq!(contradictions.len(), 1);
    }

    #[test]
    fn multiple_unkeyed_strategy_entries_with_equals_are_not_assignments() {
        let observations = vec![observation(
            ProductionCoordinationObservationSubject::ScopeQualification,
            &[
                "equipment=client supply",
                "equipment=contractor installation",
            ],
            "strategy",
            false,
        )];
        let mut contradictions = Vec::new();
        collect_field_contradictions(&observations, &mut contradictions)
            .expect("preserve multiple free-form strategy entries");
        assert!(contradictions.is_empty());
    }
}
