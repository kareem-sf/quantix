use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    time::{Duration, Instant},
};

use garde::Validate;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::agent_runtime::{
    permissions::{derive_pre_bid_data_grant, permission_duration, PreBidDataGrantRequest},
    AgentProfileVersionView, AgentResourceBudget, AgentRunInspection, AgentRunPermissions,
    AgentRunState, AgentTaskInputReference, BootstrapRole, DataClassification,
    PendingProviderEvent, PreparedAgentRun, ProviderEventKind, TenderTaskView, VerificationStatus,
};

use super::tender_records::{
    TenderEvidenceReference, TenderRecordInspection, TenderRecordKind, TenderRecordTrustClass,
    MAX_DECISION_RECORD_INVENTORY,
};
use super::{
    agent_records::{
        ensure_agent_run_capacity, insert_event, insert_profile_version, insert_task, load_profile,
        load_thread_exposure, update_profile_head,
    },
    append_audit_event, random_identifier, sql_error, sqlite_timestamp, valid_identifier,
    TenderCommandError, TenderErrorCode, TenderId, TenderLifecyclePhase, TenderStore,
};

pub(crate) const BID_PACKAGE_REVIEW_CAPABILITY: &str = "independently_review_bid_decision_package";
const BID_PACKAGE_REVIEW_SCOPE: &str = "bid_decision_package";
const BID_PACKAGE_REVIEW_ACTION: &str = "review_exact_bid_decision_package";
const MAX_PACKAGE_VERSIONS: usize = 1_000;
const MAX_COMPLIANCE_ROWS: usize = MAX_DECISION_RECORD_INVENTORY;
const MAX_CAPABILITY_DEMANDS: usize = 256;
const MAX_MANAGER_DEMANDS: usize = 64;
const MAX_REVIEW_FINDINGS: usize = 64;
const MAX_PAGE_ITEMS: u32 = 4;
const MAX_PAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_VISIBLE_BLOCKERS: usize = 64;
const MAX_PACKAGE_MANIFEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_REVIEW_DATA_VIEW_BYTES: usize = 4 * 1024 * 1024;
const MAX_PACKAGE_OPERATION_DURATION: Duration = Duration::from_secs(30);
const MAX_APPROVAL_LIST_ITEMS: usize = 32;
const MAX_APPROVAL_ITEM_BYTES: usize = 1_000;
const MAX_APPROVAL_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_APPROVAL_HISTORY_PAGE_ITEMS: u32 = 10;
const MAX_APPROVAL_RECORDS: usize = 64;
const MAX_MATERIAL_CHANGE_RECORDS: usize = MAX_COMPLIANCE_ROWS * 2;
const ZERO_APPROVAL_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct TenderRecordVersionReference {
    #[garde(length(bytes, min = 32, max = 32))]
    pub record_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ComplianceDisposition {
    Comply,
    ComplyWithQualification,
    Deviation,
    NotApplicable,
    Unresolved,
}

impl ComplianceDisposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Comply => "comply",
            Self::ComplyWithQualification => "comply_with_qualification",
            Self::Deviation => "deviation",
            Self::NotApplicable => "not_applicable",
            Self::Unresolved => "unresolved",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "comply" => Ok(Self::Comply),
            "comply_with_qualification" => Ok(Self::ComplyWithQualification),
            "deviation" => Ok(Self::Deviation),
            "not_applicable" => Ok(Self::NotApplicable),
            "unresolved" => Ok(Self::Unresolved),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ComplianceDispositionUpdate {
    #[garde(dive)]
    pub record: TenderRecordVersionReference,
    #[garde(skip)]
    pub disposition: ComplianceDisposition,
    #[garde(length(bytes, min = 1, max = 200))]
    pub responsibility: String,
    #[garde(length(bytes, min = 1, max = 2000))]
    pub planned_treatment: String,
    #[garde(skip)]
    pub affected_work: Vec<String>,
    #[garde(length(bytes, min = 1, max = 2000))]
    pub uncertainty: Option<String>,
    #[garde(skip)]
    pub related_records: Vec<TenderRecordVersionReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ManagerCapabilityDemandInput {
    #[garde(length(bytes, min = 1, max = 100))]
    pub capability: String,
    #[garde(length(bytes, min = 1, max = 1000))]
    pub rationale: String,
    #[garde(dive)]
    pub triggering_record: Option<TenderRecordVersionReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateBidDecisionPackageCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(range(min = 1))]
    pub base_version: Option<u32>,
    #[garde(length(max = 10000), dive)]
    pub disposition_updates: Vec<ComplianceDispositionUpdate>,
    #[garde(length(max = 64), dive)]
    pub manager_capability_demands: Vec<ManagerCapabilityDemandInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunBidDecisionPackageReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BidDecisionApprovalDecision {
    Accept,
    Return,
    Reject,
}

impl BidDecisionApprovalDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Return => "return",
            Self::Reject => "reject",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "accept" => Ok(Self::Accept),
            "return" => Ok(Self::Return),
            "reject" => Ok(Self::Reject),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }

    fn lifecycle_after(self) -> TenderLifecyclePhase {
        match self {
            Self::Accept => TenderLifecyclePhase::TenderPlanning,
            Self::Return => TenderLifecyclePhase::BidDecision,
            Self::Reject => TenderLifecyclePhase::Declined,
        }
    }

    fn consequence(self) -> &'static str {
        match self {
            Self::Accept => {
                "Proceed: this exact package advances to Tender Planning; production remains blocked until the controlled Work Plan is approved."
            }
            Self::Return => {
                "Return: the Bid Decision gate remains pending until the required rework is published as a new exact package version and reviewed."
            }
            Self::Reject => {
                "Decline: Tender pursuit ends while all source, analysis, Evidence, findings, and decision history remain preserved."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DecideBidDecisionPackageCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(skip)]
    pub decision: BidDecisionApprovalDecision,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
    #[garde(skip)]
    pub conditions: Vec<String>,
    #[garde(skip)]
    pub exceptions: Vec<String>,
    #[garde(skip)]
    pub required_rework: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectBidDecisionApprovalHistoryCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(range(min = 1))]
    pub before_sequence: Option<u32>,
    #[garde(range(min = 1, max = 10))]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ResolveBidDecisionReturnReworkCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub approval_id: String,
    #[garde(skip)]
    pub resolutions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InvalidateBidDecisionApprovalCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub approval_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub approval_sha256: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub material_change_summary: String,
    #[garde(length(max = 32))]
    pub affected_areas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectComplianceMatrixCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
    #[garde(range(min = 1))]
    pub after_ordinal: Option<u32>,
    #[garde(range(min = 1, max = 4))]
    pub limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BidDecisionPackageRecordCategory {
    ProjectFingerprint,
    Risk,
    Opportunity,
    Assumption,
    UnresolvedQuery,
}

impl BidDecisionPackageRecordCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::ProjectFingerprint => "project_fingerprint",
            Self::Risk => "risk",
            Self::Opportunity => "opportunity",
            Self::Assumption => "assumption",
            Self::UnresolvedQuery => "unresolved_query",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "project_fingerprint" => Ok(Self::ProjectFingerprint),
            "risk" => Ok(Self::Risk),
            "opportunity" => Ok(Self::Opportunity),
            "assumption" => Ok(Self::Assumption),
            "unresolved_query" => Ok(Self::UnresolvedQuery),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectBidDecisionPackageRecordsCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
    #[garde(skip)]
    pub category: BidDecisionPackageRecordCategory,
    #[garde(range(min = 1))]
    pub after_ordinal: Option<u32>,
    #[garde(range(min = 1, max = 4))]
    pub limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CapabilityDemandClassification {
    PolicyRequired,
    TenderRequired,
    RiskRecommended,
    ManagerAdded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CapabilityDemand {
    pub capability: String,
    pub classification: CapabilityDemandClassification,
    pub supported: bool,
    pub rationale: String,
    pub triggering_record: Option<TenderRecordVersionReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResourceImplication {
    pub affected_work: Vec<String>,
    pub responsibility: String,
    pub planned_treatment: String,
    pub uncertainty: Option<String>,
    pub triggering_record: TenderRecordVersionReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BidRecommendationOutcome {
    Proceed,
    Hold,
    Decline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidRecommendation {
    pub outcome: BidRecommendationOutcome,
    pub rationale: String,
    pub evidence_records: Vec<TenderRecordVersionReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionGateBlocker {
    pub code: String,
    pub summary: String,
    pub record: Option<TenderRecordVersionReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ComplianceMatrixRow {
    pub ordinal: u32,
    pub record: TenderRecordInspection,
    pub disposition: ComplianceDisposition,
    pub responsibility: String,
    pub planned_treatment: String,
    pub affected_work: Vec<String>,
    pub uncertainty: Option<String>,
    pub related_records: Vec<TenderRecordVersionReference>,
    pub blocker_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ComplianceMatrixPage {
    pub rows: Vec<ComplianceMatrixRow>,
    pub next_ordinal: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionPackageRecordBinding {
    pub ordinal: u32,
    pub category: BidDecisionPackageRecordCategory,
    pub record: TenderRecordInspection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionPackageRecordPage {
    pub records: Vec<BidDecisionPackageRecordBinding>,
    pub next_ordinal: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ReviewFindingSeverity {
    Critical,
    Major,
    Minor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionPackageReviewFinding {
    pub severity: ReviewFindingSeverity,
    pub code: String,
    pub summary: String,
    pub affected_records: Vec<TenderRecordVersionReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BidDecisionPackageReviewOutcome {
    Passed,
    Failed,
}

impl BidDecisionPackageReviewOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionPackageReview {
    pub review_id: String,
    pub reviewer_run_id: String,
    pub outcome: BidDecisionPackageReviewOutcome,
    pub findings: Vec<BidDecisionPackageReviewFinding>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionApprovalRecord {
    pub approval_sequence: u32,
    pub approval_id: String,
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub tender_revision: u32,
    pub decision: BidDecisionApprovalDecision,
    pub decided_by: String,
    pub acting_role: String,
    pub rationale: String,
    pub evidence_count: u32,
    pub evidence_sha256: String,
    pub conditions: Vec<String>,
    pub exceptions: Vec<String>,
    pub required_rework: Vec<String>,
    pub lifecycle_before: TenderLifecyclePhase,
    pub lifecycle_after: TenderLifecyclePhase,
    pub consequence: String,
    pub preceding_approval_hash: String,
    pub approval_sha256: String,
    pub created_at: String,
    pub invalidation: Option<BidDecisionApprovalInvalidation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionApprovalInvalidation {
    pub invalidation_id: String,
    pub approval_id: String,
    pub approval_sha256: String,
    pub material_change_summary: String,
    pub affected_areas: Vec<String>,
    pub changed_records: Vec<TenderRecordVersionReference>,
    pub invalidated_by: String,
    pub acting_role: String,
    pub lifecycle_before: TenderLifecyclePhase,
    pub lifecycle_after: TenderLifecyclePhase,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionApprovalInvalidationResult {
    pub invalidation: BidDecisionApprovalInvalidation,
    pub package: BidDecisionPackageInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionReturnReworkItem {
    pub required_rework: String,
    pub resolution: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionReturnReworkDisposition {
    pub disposition_id: String,
    pub approval_id: String,
    pub approval_sha256: String,
    pub items: Vec<BidDecisionReturnReworkItem>,
    pub resolved_by: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionReturnReworkResult {
    pub disposition: BidDecisionReturnReworkDisposition,
    pub package: BidDecisionPackageInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionApprovalHistoryPage {
    pub approvals: Vec<BidDecisionApprovalRecord>,
    pub next_sequence: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionApprovalResult {
    pub approval: BidDecisionApprovalRecord,
    pub package: BidDecisionPackageInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionPackageChangeSummary {
    pub prior_version: Option<u32>,
    pub added_record_count: u32,
    pub removed_record_count: u32,
    pub changed_compliance_row_count: u32,
    pub capability_demands_changed: bool,
    pub resource_implications_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionPackageInspection {
    pub package_id: String,
    pub version: u32,
    pub tender_revision: u32,
    pub manifest_sha256: String,
    pub compliance_row_count: u32,
    pub compliance_blocker_count: u32,
    pub project_fingerprint_count: u32,
    pub risk_count: u32,
    pub opportunity_count: u32,
    pub assumption_count: u32,
    pub unresolved_query_count: u32,
    pub capability_demands: Vec<CapabilityDemand>,
    pub capability_gap_count: u32,
    pub resource_implications: Vec<ResourceImplication>,
    pub recommendation: BidRecommendation,
    pub blocker_count: u32,
    pub blockers: Vec<BidDecisionGateBlocker>,
    pub review: Option<BidDecisionPackageReview>,
    pub approval: Option<BidDecisionApprovalRecord>,
    pub return_rework: Option<BidDecisionReturnReworkDisposition>,
    pub return_rework_basis: Option<BidDecisionReturnReworkDisposition>,
    pub material_change_basis: Option<BidDecisionApprovalInvalidation>,
    pub prior_approval_count: u32,
    pub change_summary: BidDecisionPackageChangeSummary,
    pub lifecycle_phase: TenderLifecyclePhase,
    pub decision_gate_ready: bool,
    pub current: bool,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BidDecisionPackageReviewResult {
    pub run: AgentRunInspection,
    pub package: BidDecisionPackageInspection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BidDecisionPackageReviewCandidate {
    pub outcome: BidDecisionPackageReviewOutcome,
    pub findings: Vec<BidDecisionPackageReviewFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredComplianceRow {
    record: TenderRecordVersionReference,
    disposition: ComplianceDisposition,
    responsibility: String,
    planned_treatment: String,
    affected_work: Vec<String>,
    uncertainty: Option<String>,
    related_records: Vec<TenderRecordVersionReference>,
    verification_status: VerificationStatus,
    trust_class: TenderRecordTrustClass,
    evidence_count: u32,
    blocker_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StoredBinding {
    category: BidDecisionPackageRecordCategory,
    record: TenderRecordVersionReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecordInventoryObservation {
    record: TenderRecordVersionReference,
    kind: TenderRecordKind,
    verification_status: VerificationStatus,
    trust_class: TenderRecordTrustClass,
}

struct PackageManifest<'a> {
    package_id: &'a str,
    version: u32,
    tender_revision: u32,
    rows: &'a [StoredComplianceRow],
    bindings: &'a [StoredBinding],
    capability_demands: &'a [CapabilityDemand],
    resource_implications: &'a [ResourceImplication],
    recommendation: &'a BidRecommendation,
    analysis_blocker_count: usize,
    record_inventory_sha256: &'a str,
    return_rework_basis: Option<&'a BidDecisionReturnReworkDisposition>,
    material_change_basis: Option<&'a BidDecisionApprovalInvalidation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ApprovalManifest {
    acting_role: String,
    approval_id: String,
    approval_sequence: u32,
    conditions: Vec<String>,
    consequence: String,
    created_at: String,
    decided_by: String,
    decision: BidDecisionApprovalDecision,
    evidence_count: u32,
    evidence_sha256: String,
    exceptions: Vec<String>,
    lifecycle_after: TenderLifecyclePhase,
    lifecycle_before: TenderLifecyclePhase,
    package_id: String,
    package_manifest_sha256: String,
    package_version: u32,
    preceding_approval_hash: String,
    rationale: String,
    required_rework: Vec<String>,
    schema_version: u32,
    tender_revision: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReturnReworkManifest {
    approval_id: String,
    approval_sha256: String,
    created_at: String,
    disposition_id: String,
    items: Vec<BidDecisionReturnReworkItem>,
    resolved_by: String,
    schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ApprovalInvalidationManifest {
    acting_role: String,
    affected_areas: Vec<String>,
    approval_id: String,
    approval_sha256: String,
    created_at: String,
    changed_records: Vec<TenderRecordVersionReference>,
    invalidated_by: String,
    invalidation_id: String,
    lifecycle_after: TenderLifecyclePhase,
    lifecycle_before: TenderLifecyclePhase,
    material_change_summary: String,
    schema_version: u32,
}

type StoredPackageInspectionRow = (
    u32,
    String,
    String,
    String,
    String,
    u32,
    String,
    String,
    String,
);

#[derive(Clone, Copy)]
pub(crate) struct BidPackageOperationBudget {
    deadline: Instant,
}

impl BidPackageOperationBudget {
    pub(crate) fn for_tender(tender_id: &TenderId) -> Self {
        #[cfg(feature = "runtime-fixture")]
        if std::env::var("QUANTIX_BID_PACKAGE_OPERATION_TIMEOUT")
            .is_ok_and(|fixture_tender_id| fixture_tender_id == tender_id.as_str())
        {
            return Self {
                deadline: Instant::now(),
            };
        }
        #[cfg(not(feature = "runtime-fixture"))]
        let _ = tender_id;
        Self {
            deadline: Instant::now()
                .checked_add(MAX_PACKAGE_OPERATION_DURATION)
                .expect("the fixed Bid Decision Package duration fits Instant"),
        }
    }

    pub(crate) fn check(self) -> Result<(), TenderCommandError> {
        if Instant::now() >= self.deadline {
            Err(TenderCommandError::new(TenderErrorCode::OperationTimedOut))
        } else {
            Ok(())
        }
    }
}

impl TenderStore {
    pub(crate) fn create_bid_decision_package(
        &mut self,
        tender_id: &TenderId,
        command: &CreateBidDecisionPackageCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<BidDecisionPackageInspection, TenderCommandError> {
        self.require_pre_bid_writable()?;
        budget.check()?;
        validate_package_command(command)?;
        let lifecycle_phase = TenderLifecyclePhase::parse(
            &self
                .connection
                .query_row(
                    "SELECT lifecycle_phase FROM tender WHERE singleton = 1 AND tender_id = ?1",
                    [tender_id.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sql_error)?,
        )?;
        if !matches!(
            lifecycle_phase,
            TenderLifecyclePhase::Intake | TenderLifecyclePhase::BidDecision
        ) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let current_head: Option<(String, u32)> = self
            .connection
            .query_row(
                "SELECT package_id, current_version FROM bid_decision_package_heads LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        match (&current_head, command.base_version) {
            (None, None) => {}
            (Some((_, current)), Some(base)) if *current == base => {}
            _ => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        }
        let (return_rework_basis, material_change_basis) = if let Some((package_id, version)) =
            &current_head
        {
            match load_package_approval(&self.connection, package_id, *version)? {
                Some(approval) if approval.decision == BidDecisionApprovalDecision::Return => (
                    Some(
                        load_return_rework_disposition(&self.connection, &approval.approval_id)?
                            .ok_or_else(|| {
                                TenderCommandError::new(TenderErrorCode::InvalidCommand)
                            })?,
                    ),
                    None,
                ),
                Some(approval) if approval.decision == BidDecisionApprovalDecision::Accept => {
                    let invalidation = approval
                        .invalidation
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                    let prior_inventory =
                        load_package_record_inventory(&self.connection, package_id, *version)?;
                    let current_inventory =
                        current_record_inventory_bounded_with_check(self, &mut || budget.check())?;
                    if changed_record_references(&prior_inventory, &current_inventory)
                        != invalidation.changed_records
                    {
                        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                    }
                    (None, Some(invalidation))
                }
                Some(_) => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
                None => (None, None),
            }
        } else {
            (None, None)
        };
        let current_refs = current_record_references(&self.connection)?;
        if current_refs.len() > MAX_COMPLIANCE_ROWS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut metadata = HashMap::new();
        let mut record_inspections = HashMap::new();
        let mut obligation_refs = Vec::new();
        let mut bindings = Vec::new();
        let mut extracted_capabilities = Vec::new();
        let mut decline_risks = Vec::new();
        for reference in &current_refs {
            budget.check()?;
            let record =
                self.inspect_tender_record_version(&reference.record_id, reference.version)?;
            metadata.insert(
                (reference.record_id.clone(), reference.version),
                (record.kind, record.verification_status, record.trust_class),
            );
            if is_compliance_kind(record.kind) {
                obligation_refs.push(reference.clone());
            }
            let category = match record.kind {
                TenderRecordKind::ProjectCharacteristic
                    if record.verification_status == VerificationStatus::Verified
                        && !matches!(
                            record.trust_class,
                            TenderRecordTrustClass::ApprovedAssumption
                                | TenderRecordTrustClass::UnresolvedGap
                                | TenderRecordTrustClass::AiProposal
                        ) =>
                {
                    Some(BidDecisionPackageRecordCategory::ProjectFingerprint)
                }
                TenderRecordKind::Risk
                    if record.verification_status != VerificationStatus::Rejected =>
                {
                    Some(BidDecisionPackageRecordCategory::Risk)
                }
                TenderRecordKind::EvaluationCriterion
                    if record.verification_status == VerificationStatus::Verified =>
                {
                    Some(BidDecisionPackageRecordCategory::Opportunity)
                }
                TenderRecordKind::Assumption
                    if record.verification_status != VerificationStatus::Rejected =>
                {
                    Some(BidDecisionPackageRecordCategory::Assumption)
                }
                TenderRecordKind::TenderQuery
                    if record.verification_status != VerificationStatus::Rejected =>
                {
                    Some(BidDecisionPackageRecordCategory::UnresolvedQuery)
                }
                _ => None,
            };
            if let Some(category) = category {
                bindings.push(StoredBinding {
                    category,
                    record: reference.clone(),
                });
            }
            if record.verification_status == VerificationStatus::Verified {
                let (field_name, classification) = match record.kind {
                    TenderRecordKind::ProjectCharacteristic => (
                        "required_capability",
                        CapabilityDemandClassification::TenderRequired,
                    ),
                    TenderRecordKind::Risk => (
                        "recommended_capability",
                        CapabilityDemandClassification::RiskRecommended,
                    ),
                    _ => ("", CapabilityDemandClassification::TenderRequired),
                };
                if !field_name.is_empty() {
                    for field in &record.fields {
                        if field.name == field_name {
                            if let Some(value) = field
                                .value
                                .as_deref()
                                .filter(|value| valid_capability(value))
                            {
                                extracted_capabilities.push(CapabilityDemand {
                                    capability: value.to_owned(),
                                    classification,
                                    supported: capability_is_supported(value),
                                    rationale: format!(
                                        "Derived from verified Tender Record '{}'.",
                                        record.title
                                    ),
                                    triggering_record: Some(reference.clone()),
                                });
                            }
                        }
                        if record.kind == TenderRecordKind::Risk
                            && field.name == "bid_recommendation"
                            && field
                                .value
                                .as_deref()
                                .is_some_and(|value| value.eq_ignore_ascii_case("decline"))
                        {
                            decline_risks.push(reference.clone());
                        }
                    }
                }
            }
            record_inspections.insert((reference.record_id.clone(), reference.version), record);
        }
        let record_inventory = current_refs
            .iter()
            .map(|reference| {
                record_inspections
                    .get(&(reference.record_id.clone(), reference.version))
                    .map(|record| RecordInventoryObservation {
                        record: reference.clone(),
                        kind: record.kind,
                        verification_status: record.verification_status,
                        trust_class: record.trust_class,
                    })
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let record_inventory_json = canonical_json(&record_inventory)?;
        let record_inventory_sha256 = sha256_hex(record_inventory_json.as_bytes());
        bindings.sort_by(|left, right| {
            left.category
                .as_str()
                .cmp(right.category.as_str())
                .then_with(|| left.record.record_id.cmp(&right.record.record_id))
                .then_with(|| left.record.version.cmp(&right.record.version))
        });
        if obligation_refs.len() > MAX_COMPLIANCE_ROWS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let previous_rows = if let Some((package_id, version)) = &current_head {
            load_stored_rows(&self.connection, package_id, *version)?
                .into_iter()
                .map(|row| ((row.record.record_id.clone(), row.record.version), row))
                .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        let updates = command
            .disposition_updates
            .iter()
            .cloned()
            .map(|update| {
                (
                    (update.record.record_id.clone(), update.record.version),
                    update,
                )
            })
            .collect::<HashMap<_, _>>();
        if updates.len() != command.disposition_updates.len()
            || updates.keys().any(|key| !metadata.contains_key(key))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let mut rows = Vec::with_capacity(obligation_refs.len());
        let mut updated_keys = HashSet::new();
        for reference in &obligation_refs {
            budget.check()?;
            let record = record_inspections
                .get(&(reference.record_id.clone(), reference.version))
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let key = (reference.record_id.clone(), reference.version);
            let mut row = previous_rows
                .get(&key)
                .cloned()
                .unwrap_or_else(|| default_compliance_row(record));
            if let Some(update) = updates.get(&key) {
                validate_disposition_update(update, &metadata)?;
                row.disposition = update.disposition;
                row.responsibility = update.responsibility.trim().to_owned();
                row.planned_treatment = update.planned_treatment.trim().to_owned();
                row.affected_work = update.affected_work.clone();
                row.uncertainty = update.uncertainty.clone();
                row.related_records = update.related_records.clone();
                updated_keys.insert(key.clone());
            }
            row.verification_status = record.verification_status;
            row.trust_class = record.trust_class;
            row.evidence_count = record_evidence_count(record)?;
            row.blocker_codes = compliance_blocker_codes(record, &row);
            rows.push(row);
        }
        if updated_keys.len() != updates.len() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let mut capability_demands = vec![
            CapabilityDemand {
                capability: "cost_estimation".into(),
                classification: CapabilityDemandClassification::PolicyRequired,
                supported: true,
                rationale:
                    "Quantix v0 policy requires the Cost Estimating Workstream after Proceed."
                        .into(),
                triggering_record: None,
            },
            CapabilityDemand {
                capability: "query_rfi_control".into(),
                classification: CapabilityDemandClassification::PolicyRequired,
                supported: true,
                rationale:
                    "Quantix v0 policy requires Query and RFI Control across the Tender lifecycle."
                        .into(),
                triggering_record: None,
            },
        ];
        capability_demands.extend(extracted_capabilities);
        for demand in &command.manager_capability_demands {
            if let Some(trigger) = &demand.triggering_record {
                if !metadata.contains_key(&(trigger.record_id.clone(), trigger.version)) {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            capability_demands.push(CapabilityDemand {
                capability: demand.capability.trim().to_owned(),
                classification: CapabilityDemandClassification::ManagerAdded,
                supported: capability_is_supported(demand.capability.trim()),
                rationale: demand.rationale.trim().to_owned(),
                triggering_record: demand.triggering_record.clone(),
            });
        }
        deduplicate_capability_demands(&mut capability_demands);
        if capability_demands.len() > MAX_CAPABILITY_DEMANDS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let resource_implications = rows
            .iter()
            .map(|row| ResourceImplication {
                affected_work: row.affected_work.clone(),
                responsibility: row.responsibility.clone(),
                planned_treatment: row.planned_treatment.clone(),
                uncertainty: row.uncertainty.clone(),
                triggering_record: row.record.clone(),
            })
            .collect::<Vec<_>>();

        let fingerprint_count = bindings
            .iter()
            .filter(|binding| {
                binding.category == BidDecisionPackageRecordCategory::ProjectFingerprint
            })
            .count();
        let assumption_blockers = bindings
            .iter()
            .filter(|binding| binding.category == BidDecisionPackageRecordCategory::Assumption)
            .filter(|binding| {
                metadata
                    .get(&(binding.record.record_id.clone(), binding.record.version))
                    .is_none_or(|(_, status, trust)| {
                        *status != VerificationStatus::Verified
                            || *trust != TenderRecordTrustClass::ApprovedAssumption
                    })
            })
            .count();
        let query_blockers = bindings
            .iter()
            .filter(|binding| binding.category == BidDecisionPackageRecordCategory::UnresolvedQuery)
            .count();
        let row_blockers = rows
            .iter()
            .map(|row| row.blocker_codes.len())
            .sum::<usize>();
        let gap_count = capability_demands
            .iter()
            .filter(|demand| !demand.supported)
            .count();
        let analysis_blocker_count = row_blockers
            .checked_add(usize::from(fingerprint_count == 0))
            .and_then(|count| count.checked_add(assumption_blockers))
            .and_then(|count| count.checked_add(query_blockers))
            .and_then(|count| count.checked_add(gap_count))
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let recommendation = if !decline_risks.is_empty() {
            BidRecommendation {
                outcome: BidRecommendationOutcome::Decline,
                rationale: "Verified high-risk evidence recommends declining this Tender.".into(),
                evidence_records: decline_risks.into_iter().take(64).collect(),
            }
        } else if analysis_blocker_count > 0 {
            BidRecommendation {
                outcome: BidRecommendationOutcome::Hold,
                rationale: "Hold until every critical compliance, evidence, capability, assumption, and query blocker is resolved.".into(),
                evidence_records: rows
                    .iter()
                    .filter(|row| !row.blocker_codes.is_empty())
                    .map(|row| row.record.clone())
                    .take(64)
                    .collect(),
            }
        } else {
            BidRecommendation {
                outcome: BidRecommendationOutcome::Proceed,
                rationale: "Proceed to Tender Planning: the exact pre-bid analysis has complete dispositions, verified evidence, supported capability coverage, and no blocking gap.".into(),
                evidence_records: rows.iter().map(|row| row.record.clone()).take(64).collect(),
            }
        };

        let package_id = match current_head.as_ref() {
            Some(head) => head.0.clone(),
            None => random_identifier(&self.connection)?,
        };
        let version = current_head.as_ref().map(|head| head.1 + 1).unwrap_or(1);
        if usize::try_from(version)
            .ok()
            .is_none_or(|version| version > MAX_PACKAGE_VERSIONS)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let tender_revision: u32 = self
            .connection
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let analysis_blocker_count_u32 = u32::try_from(analysis_blocker_count)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let manifest = package_manifest_value(&PackageManifest {
            package_id: &package_id,
            version,
            tender_revision,
            rows: &rows,
            bindings: &bindings,
            capability_demands: &capability_demands,
            resource_implications: &resource_implications,
            recommendation: &recommendation,
            analysis_blocker_count,
            record_inventory_sha256: &record_inventory_sha256,
            return_rework_basis: return_rework_basis.as_ref(),
            material_change_basis: material_change_basis.as_ref(),
        })?;
        let manifest_json = canonical_json(&manifest)?;
        if manifest_json.len() > MAX_PACKAGE_MANIFEST_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let mut review_records = rows
            .iter()
            .map(|row| row.record.clone())
            .chain(bindings.iter().map(|binding| binding.record.clone()))
            .collect::<Vec<_>>();
        review_records.sort_by(|left, right| {
            left.record_id
                .cmp(&right.record_id)
                .then_with(|| left.version.cmp(&right.version))
        });
        review_records.dedup();
        let review_basis = review_records
            .iter()
            .map(|reference| {
                record_inspections
                    .get(&(reference.record_id.clone(), reference.version))
                    .map(record_evidence_basis)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let review_data_view =
            bid_package_review_data_view_value(&manifest, &manifest_sha256, review_basis);
        if canonical_json(&review_data_view)?.len() > MAX_REVIEW_DATA_VIEW_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        budget.check()?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        if lifecycle_phase == TenderLifecyclePhase::Intake
            && transaction
                .execute(
                    "UPDATE tender SET lifecycle_phase = 'bid_decision'
                     WHERE singleton = 1 AND tender_id = ?1 AND lifecycle_phase = 'intake'",
                    [tender_id.as_str()],
                )
                .map_err(sql_error)?
                != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        if current_head.is_none() {
            transaction
                .execute(
                    "INSERT INTO bid_decision_packages (package_id, created_at) VALUES (?1, ?2)",
                    params![package_id, created_at],
                )
                .map_err(sql_error)?;
        }
        transaction
            .execute(
                "INSERT INTO bid_decision_package_versions (
                   package_id, version, tender_revision, record_inventory_json,
                   record_inventory_sha256,
                   capability_demands_json, resource_implications_json,
                   recommendation_json, analysis_blocker_count, manifest_json,
                   manifest_sha256, created_by, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'engineer_user', ?12)",
                params![
                    package_id,
                    version,
                    tender_revision,
                    record_inventory_json,
                    record_inventory_sha256,
                    canonical_json(&capability_demands)?,
                    canonical_json(&resource_implications)?,
                    canonical_json(&recommendation)?,
                    analysis_blocker_count_u32,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        for (index, row) in rows.iter().enumerate() {
            budget.check()?;
            let ordinal = u32::try_from(index + 1)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            transaction
                .execute(
                    "INSERT INTO bid_compliance_rows (
                       package_id, package_version, ordinal, record_id, record_version,
                       disposition, responsibility, planned_treatment, affected_work_json,
                       uncertainty, related_records_json, verification_status, trust_class,
                       evidence_count, blocker_codes_json
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        package_id,
                        version,
                        ordinal,
                        row.record.record_id,
                        row.record.version,
                        row.disposition.as_str(),
                        row.responsibility,
                        row.planned_treatment,
                        canonical_json(&row.affected_work)?,
                        row.uncertainty,
                        canonical_json(&row.related_records)?,
                        verification_status_str(row.verification_status),
                        trust_class_str(row.trust_class),
                        row.evidence_count,
                        canonical_json(&row.blocker_codes)?,
                    ],
                )
                .map_err(sql_error)?;
        }
        let mut category_ordinals = HashMap::new();
        for binding in &bindings {
            budget.check()?;
            let ordinal = category_ordinals
                .entry(binding.category)
                .and_modify(|value: &mut u32| *value += 1)
                .or_insert(1);
            transaction
                .execute(
                    "INSERT INTO bid_decision_package_record_bindings (
                       package_id, package_version, category, ordinal, record_id, record_version
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        package_id,
                        version,
                        binding.category.as_str(),
                        *ordinal,
                        binding.record.record_id,
                        binding.record.version,
                    ],
                )
                .map_err(sql_error)?;
        }
        if current_head.is_some() {
            if transaction
                .execute(
                    "UPDATE bid_decision_package_heads SET current_version = ?2
                     WHERE package_id = ?1 AND current_version = ?3",
                    params![package_id, version, command.base_version],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        } else {
            transaction
                .execute(
                    "INSERT INTO bid_decision_package_heads (package_id, current_version)
                     VALUES (?1, ?2)",
                    params![package_id, version],
                )
                .map_err(sql_error)?;
        }
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "bid_decision_package_version_created",
            tender_revision,
            json!({
                "analysis_blocker_count": analysis_blocker_count.to_string(),
                "manifest_sha256": manifest_sha256,
                "package_id": package_id,
                "package_version": version.to_string(),
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        self.inspect_bid_decision_package(&package_id, version)
    }

    pub(crate) fn inspect_current_bid_decision_package(
        &self,
    ) -> Result<Option<BidDecisionPackageInspection>, TenderCommandError> {
        let head: Option<(String, u32)> = self
            .connection
            .query_row(
                "SELECT package_id, current_version FROM bid_decision_package_heads LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        head.map(|(package_id, version)| self.inspect_bid_decision_package(&package_id, version))
            .transpose()
    }

    pub(crate) fn decide_bid_decision_package(
        &mut self,
        tender_id: &TenderId,
        command: &DecideBidDecisionPackageCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<BidDecisionApprovalResult, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        if validate_approval_command(command).is_err() {
            self.record_bid_decision_denial(tender_id, command, "decision_shape_invalid")?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let package = match self.inspect_bid_decision_package(&command.package_id, command.version)
        {
            Ok(package) => package,
            Err(error) if error.code == TenderErrorCode::NotFound => {
                self.record_bid_decision_denial(tender_id, command, "package_not_found")?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            Err(error) => return Err(error),
        };
        let active_execution: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE status = 'running')
                        OR EXISTS(SELECT 1 FROM parse_attempts WHERE status = 'running')",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let approval_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM bid_decision_approval_records",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let denial_reason = if package.manifest_sha256 != command.manifest_sha256 {
            Some("manifest_changed")
        } else if !package.current {
            Some("package_stale")
        } else if package.lifecycle_phase != TenderLifecyclePhase::BidDecision {
            Some("lifecycle_closed")
        } else if package.approval.is_some() {
            Some("decision_already_recorded")
        } else if active_execution {
            Some("active_execution")
        } else if approval_count as usize >= MAX_APPROVAL_RECORDS {
            Some("approval_history_limit")
        } else if matches!(
            command.decision,
            BidDecisionApprovalDecision::Accept | BidDecisionApprovalDecision::Reject
        ) && !package.decision_gate_ready
        {
            Some("decision_gate_blocked")
        } else {
            None
        };
        if let Some(reason) = denial_reason {
            self.record_bid_decision_denial(tender_id, command, reason)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let evidence = approval_evidence(self, &command.package_id, command.version, &mut || {
            budget.check()
        })?;
        let evidence_json = canonical_json(&evidence)?;
        if evidence_json.len() > MAX_APPROVAL_MANIFEST_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let evidence_sha256 = sha256_hex(evidence_json.as_bytes());
        let evidence_count = u32::try_from(evidence.len())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let lifecycle_after = command.decision.lifecycle_after();
        let consequence = command.decision.consequence();
        let conditions = normalized_approval_items(&command.conditions)?;
        let exceptions = normalized_approval_items(&command.exceptions)?;
        let required_rework = normalized_approval_items(&command.required_rework)?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        budget.check()?;
        let exact_state: Option<(u32, String, u32, String, u32, Option<String>)> = transaction
            .query_row(
                "SELECT versions.tender_revision, versions.manifest_sha256,
                        versions.analysis_blocker_count, tender.lifecycle_phase,
                        heads.current_version, reviews.outcome
                 FROM bid_decision_package_versions AS versions
                 JOIN bid_decision_package_heads AS heads
                   ON heads.package_id = versions.package_id
                 JOIN tender ON tender.singleton = 1
                 LEFT JOIN bid_decision_package_reviews AS reviews
                   ON reviews.package_id = versions.package_id
                  AND reviews.package_version = versions.version
                 WHERE versions.package_id = ?1 AND versions.version = ?2
                   AND tender.tender_id = ?3",
                params![command.package_id, command.version, tender_id.as_str()],
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
        let Some((
            tender_revision,
            manifest_sha256,
            analysis_blocker_count,
            lifecycle_before,
            current_version,
            review_outcome,
        )) = exact_state
        else {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        let gate_required = matches!(
            command.decision,
            BidDecisionApprovalDecision::Accept | BidDecisionApprovalDecision::Reject
        );
        let active_execution: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE status = 'running')
                        OR EXISTS(SELECT 1 FROM parse_attempts WHERE status = 'running')",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let exact_denial = if tender_revision != package.tender_revision
            || manifest_sha256 != command.manifest_sha256
            || current_version != command.version
        {
            Some("package_changed_before_commit")
        } else if lifecycle_before != TenderLifecyclePhase::BidDecision.as_str() {
            Some("lifecycle_closed_before_commit")
        } else if active_execution {
            Some("active_execution_before_commit")
        } else if gate_required
            && (analysis_blocker_count != 0 || review_outcome.as_deref() != Some("passed"))
        {
            Some("decision_gate_changed_before_commit")
        } else {
            None
        };
        if let Some(reason) = exact_denial {
            append_bid_decision_denial(&transaction, tender_id, tender_revision, command, reason)?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let existing_approval: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM bid_decision_approval_records
                 WHERE package_id = ?1 AND package_version = ?2)",
                params![command.package_id, command.version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if existing_approval {
            append_bid_decision_denial(
                &transaction,
                tender_id,
                tender_revision,
                command,
                "decision_already_recorded_before_commit",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (previous_sequence, preceding_approval_hash): (u32, String) = transaction
            .query_row(
                "SELECT COALESCE(MAX(approval_sequence), 0),
                        COALESCE(MAX(approval_sha256) FILTER (
                          WHERE approval_sequence = (
                            SELECT MAX(approval_sequence) FROM bid_decision_approval_records
                          )
                        ), ?1)
                 FROM bid_decision_approval_records",
                [ZERO_APPROVAL_HASH],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let approval_sequence = previous_sequence
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let approval_manifest = ApprovalManifest {
            acting_role: "tendering_manager".into(),
            approval_id: approval_id.clone(),
            approval_sequence,
            conditions: conditions.clone(),
            consequence: consequence.into(),
            created_at: created_at.clone(),
            decided_by: "engineer_user".into(),
            decision: command.decision,
            evidence_count,
            evidence_sha256: evidence_sha256.clone(),
            exceptions: exceptions.clone(),
            lifecycle_after,
            lifecycle_before: TenderLifecyclePhase::BidDecision,
            package_id: command.package_id.clone(),
            package_manifest_sha256: command.manifest_sha256.clone(),
            package_version: command.version,
            preceding_approval_hash: preceding_approval_hash.clone(),
            rationale: command.rationale.trim().into(),
            required_rework: required_rework.clone(),
            schema_version: 1,
            tender_revision,
        };
        let approval_manifest_json = canonical_json(&approval_manifest)?;
        if approval_manifest_json.len() > MAX_APPROVAL_MANIFEST_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let approval_sha256 = sha256_hex(approval_manifest_json.as_bytes());
        budget.check()?;
        transaction
            .execute(
                "INSERT INTO bid_decision_approval_records (
                   approval_sequence, approval_id, package_id, package_version,
                   package_manifest_sha256, tender_revision, decision, decided_by,
                   acting_role, rationale, evidence_count, evidence_sha256,
                   conditions_json, exceptions_json, required_rework_json,
                   lifecycle_before, lifecycle_after, consequence,
                   preceding_approval_hash, approval_manifest_json, approval_sha256,
                   created_at
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'engineer_user',
                   'tendering_manager', ?8, ?9, ?10, ?11, ?12, ?13,
                   'bid_decision', ?14, ?15, ?16, ?17, ?18, ?19
                 )",
                params![
                    approval_sequence,
                    approval_id,
                    command.package_id,
                    command.version,
                    command.manifest_sha256,
                    tender_revision,
                    command.decision.as_str(),
                    command.rationale.trim(),
                    evidence_count,
                    evidence_sha256,
                    canonical_json(&conditions)?,
                    canonical_json(&exceptions)?,
                    canonical_json(&required_rework)?,
                    lifecycle_after.as_str(),
                    consequence,
                    preceding_approval_hash,
                    approval_manifest_json,
                    approval_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        if transaction
            .execute(
                "UPDATE tender SET lifecycle_phase = ?2
                 WHERE singleton = 1 AND tender_id = ?1 AND lifecycle_phase = 'bid_decision'",
                params![tender_id.as_str(), lifecycle_after.as_str()],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "bid_decision_approval_recorded",
            tender_revision,
            json!({
                "approval_id": approval_id,
                "approval_sha256": approval_sha256,
                "decision": command.decision,
                "package_id": command.package_id,
                "package_manifest_sha256": command.manifest_sha256,
                "package_version": command.version.to_string(),
            }),
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        let package = self.inspect_bid_decision_package(&command.package_id, command.version)?;
        let approval = package
            .approval
            .clone()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        Ok(BidDecisionApprovalResult { approval, package })
    }

    pub(crate) fn resolve_bid_decision_return_rework(
        &mut self,
        tender_id: &TenderId,
        command: &ResolveBidDecisionReturnReworkCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<BidDecisionReturnReworkResult, TenderCommandError> {
        self.require_pre_bid_writable()?;
        budget.check()?;
        if !valid_identifier(&command.approval_id)
            || command.resolutions.len() > MAX_APPROVAL_LIST_ITEMS
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let resolutions = normalized_approval_items(&command.resolutions)?;
        let approval = load_approval_by_id(&self.connection, &command.approval_id)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let current_version: Option<u32> = self
            .connection
            .query_row(
                "SELECT current_version FROM bid_decision_package_heads WHERE package_id = ?1",
                [&approval.package_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let active_execution: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE status = 'running')
                        OR EXISTS(SELECT 1 FROM parse_attempts WHERE status = 'running')",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if approval.decision != BidDecisionApprovalDecision::Return
            || current_version != Some(approval.package_version)
            || resolutions.len() != approval.required_rework.len()
            || resolutions.is_empty()
            || active_execution
            || load_return_rework_disposition(&self.connection, &command.approval_id)?.is_some()
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let items = approval
            .required_rework
            .iter()
            .cloned()
            .zip(resolutions)
            .map(
                |(required_rework, resolution)| BidDecisionReturnReworkItem {
                    required_rework,
                    resolution,
                },
            )
            .collect::<Vec<_>>();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        budget.check()?;
        let still_current: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM bid_decision_package_heads AS heads
                   JOIN tender ON tender.singleton = 1
                   WHERE heads.package_id = ?1 AND heads.current_version = ?2
                     AND tender.tender_id = ?3 AND tender.lifecycle_phase = 'bid_decision'
                 ) AND NOT EXISTS(
                   SELECT 1 FROM bid_decision_return_rework_dispositions
                   WHERE approval_id = ?4
                 )",
                params![
                    approval.package_id,
                    approval.package_version,
                    tender_id.as_str(),
                    approval.approval_id,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !still_current {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let disposition_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = ReturnReworkManifest {
            approval_id: approval.approval_id.clone(),
            approval_sha256: approval.approval_sha256.clone(),
            created_at: created_at.clone(),
            disposition_id: disposition_id.clone(),
            items: items.clone(),
            resolved_by: "engineer_user".into(),
            schema_version: 1,
        };
        let manifest_json = canonical_json(&manifest)?;
        if manifest_json.len() > MAX_APPROVAL_MANIFEST_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        transaction
            .execute(
                "INSERT INTO bid_decision_return_rework_dispositions (
                   disposition_id, approval_id, approval_sha256, items_json,
                   resolved_by, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, 'engineer_user', ?5, ?6, ?7)",
                params![
                    disposition_id,
                    approval.approval_id,
                    approval.approval_sha256,
                    canonical_json(&items)?,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "bid_decision_return_rework_resolved",
            approval.tender_revision,
            json!({
                "approval_id": approval.approval_id,
                "disposition_id": disposition_id,
                "item_count": items.len().to_string(),
                "manifest_sha256": manifest_sha256,
            }),
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        let disposition =
            load_return_rework_disposition(&self.connection, &command.approval_id)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let package =
            self.inspect_bid_decision_package(&approval.package_id, approval.package_version)?;
        Ok(BidDecisionReturnReworkResult {
            disposition,
            package,
        })
    }

    pub(crate) fn invalidate_bid_decision_approval(
        &mut self,
        tender_id: &TenderId,
        command: &InvalidateBidDecisionApprovalCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<BidDecisionApprovalInvalidationResult, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let material_change_summary = command.material_change_summary.trim().to_owned();
        let affected_areas = match normalized_approval_items(&command.affected_areas) {
            Ok(areas) => areas,
            Err(_) => {
                self.record_bid_decision_invalidation_denial(
                    tender_id,
                    command,
                    "invalidation_shape_invalid",
                )?;
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        };
        if !valid_identifier(&command.approval_id)
            || !valid_sha256(&command.approval_sha256)
            || material_change_summary.is_empty()
            || material_change_summary.len() > 4_000
            || affected_areas.is_empty()
        {
            self.record_bid_decision_invalidation_denial(
                tender_id,
                command,
                "invalidation_shape_invalid",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let Some(approval) = load_approval_by_id(&self.connection, &command.approval_id)? else {
            self.record_bid_decision_invalidation_denial(tender_id, command, "approval_not_found")?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        let approved_inventory = load_package_record_inventory(
            &self.connection,
            &approval.package_id,
            approval.package_version,
        )?;
        let current_inventory =
            current_record_inventory_bounded_with_check(self, &mut || budget.check())?;
        let changed_records = changed_record_references(&approved_inventory, &current_inventory);
        let current_head: Option<(String, u32)> = self
            .connection
            .query_row(
                "SELECT package_id, current_version FROM bid_decision_package_heads",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let lifecycle_phase = TenderLifecyclePhase::parse(
            &self
                .connection
                .query_row(
                    "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sql_error)?,
        )?;
        let active_execution: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agent_runs WHERE status = 'running')
                        OR EXISTS(SELECT 1 FROM parse_attempts WHERE status = 'running')",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if approval.decision != BidDecisionApprovalDecision::Accept
            || approval.approval_sha256 != command.approval_sha256
            || approval.invalidation.is_some()
            || current_head != Some((approval.package_id.clone(), approval.package_version))
            || lifecycle_phase != TenderLifecyclePhase::TenderPlanning
            || active_execution
            || !valid_material_change_record_count(&changed_records)
        {
            self.record_bid_decision_invalidation_denial(
                tender_id,
                command,
                "invalidation_guard_failed",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        budget.check()?;
        let still_exact: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM bid_decision_approval_records AS approvals
                   JOIN bid_decision_package_heads AS heads
                     ON heads.package_id = approvals.package_id
                    AND heads.current_version = approvals.package_version
                   JOIN tender ON tender.singleton = 1
                   WHERE approvals.approval_id = ?1
                     AND approvals.approval_sha256 = ?2
                     AND approvals.decision = 'accept'
                     AND tender.tender_id = ?3
                     AND tender.lifecycle_phase = 'tender_planning'
                 ) AND NOT EXISTS(
                   SELECT 1 FROM bid_decision_approval_invalidations WHERE approval_id = ?1
                 ) AND NOT EXISTS(
                   SELECT 1 FROM agent_runs WHERE status = 'running'
                 ) AND NOT EXISTS(
                   SELECT 1 FROM parse_attempts WHERE status = 'running'
                 )",
                params![
                    command.approval_id,
                    command.approval_sha256,
                    tender_id.as_str(),
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !still_exact {
            append_bid_decision_invalidation_denial(
                &transaction,
                tender_id,
                approval.tender_revision,
                command,
                "invalidation_state_changed_before_commit",
            )?;
            transaction.commit().map_err(sql_error)?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let invalidation_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = ApprovalInvalidationManifest {
            acting_role: "tendering_manager".into(),
            affected_areas: affected_areas.clone(),
            approval_id: approval.approval_id.clone(),
            approval_sha256: approval.approval_sha256.clone(),
            created_at: created_at.clone(),
            changed_records: changed_records.clone(),
            invalidated_by: "engineer_user".into(),
            invalidation_id: invalidation_id.clone(),
            lifecycle_after: TenderLifecyclePhase::BidDecision,
            lifecycle_before: TenderLifecyclePhase::TenderPlanning,
            material_change_summary: material_change_summary.clone(),
            schema_version: 1,
        };
        let manifest_json = canonical_json(&manifest)?;
        if manifest_json.len() > MAX_APPROVAL_MANIFEST_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        transaction
            .execute(
                "INSERT INTO bid_decision_approval_invalidations (
                   invalidation_id, approval_id, approval_sha256,
                   material_change_summary, affected_areas_json, changed_records_json,
                   invalidated_by,
                   acting_role, lifecycle_before, lifecycle_after, manifest_json,
                   manifest_sha256, created_at
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, 'engineer_user', 'tendering_manager',
                   'tender_planning', 'bid_decision', ?7, ?8, ?9
                 )",
                params![
                    invalidation_id,
                    approval.approval_id,
                    approval.approval_sha256,
                    material_change_summary,
                    canonical_json(&affected_areas)?,
                    canonical_json(&changed_records)?,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        let suspended_profile_count = transaction
            .execute(
                "UPDATE agent_profile_heads
                 SET status = 'suspended'
                 WHERE status = 'active'
                   AND EXISTS (
                     SELECT 1
                     FROM work_plan_heads
                     JOIN work_plan_versions AS plans
                       ON plans.plan_id = work_plan_heads.plan_id
                      AND plans.version = work_plan_heads.current_version
                     JOIN work_plan_approvals AS plan_approvals
                       ON plan_approvals.plan_id = plans.plan_id
                      AND plan_approvals.plan_version = plans.version
                      AND plan_approvals.decision = 'approve'
                     JOIN json_each(plans.profiles_json) AS bound_profile
                       ON json_extract(bound_profile.value, '$.profile.profile_id') = agent_profile_heads.profile_id
                      AND json_extract(bound_profile.value, '$.profile.version') = agent_profile_heads.current_version
                   )",
                [],
            )
            .map_err(sql_error)?;
        if transaction
            .execute(
                "UPDATE tender SET lifecycle_phase = 'bid_decision'
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
            "bid_decision_approval_invalidated",
            approval.tender_revision,
            json!({
                "affected_area_count": affected_areas.len().to_string(),
                "approval_id": approval.approval_id,
                "approval_sha256": approval.approval_sha256,
                "invalidation_id": invalidation_id,
                "manifest_sha256": manifest_sha256,
                "changed_record_count": changed_records.len().to_string(),
                "suspended_profile_count": suspended_profile_count.to_string(),
            }),
            &created_at,
        )?;
        budget.check()?;
        transaction.commit().map_err(sql_error)?;
        let invalidation = load_approval_invalidation(&self.connection, &command.approval_id)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let package =
            self.inspect_bid_decision_package(&approval.package_id, approval.package_version)?;
        Ok(BidDecisionApprovalInvalidationResult {
            invalidation,
            package,
        })
    }

    pub(crate) fn inspect_bid_decision_approval_history(
        &self,
        before_sequence: Option<u32>,
        limit: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<BidDecisionApprovalHistoryPage, TenderCommandError> {
        if limit == 0 || limit > MAX_APPROVAL_HISTORY_PAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let rows =
            load_approval_history(&self.connection, before_sequence, limit + 1, &mut || {
                budget.check()
            })?;
        let approvals = rows
            .iter()
            .take(limit as usize)
            .cloned()
            .collect::<Vec<_>>();
        let next_sequence = (rows.len() > approvals.len())
            .then(|| approvals.last().map(|approval| approval.approval_sequence))
            .flatten();
        Ok(BidDecisionApprovalHistoryPage {
            approvals,
            next_sequence,
        })
    }

    fn record_bid_decision_denial(
        &mut self,
        tender_id: &TenderId,
        command: &DecideBidDecisionPackageCommand,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
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
        append_bid_decision_denial(&transaction, tender_id, tender_revision, command, reason)?;
        transaction.commit().map_err(sql_error)
    }

    fn record_bid_decision_invalidation_denial(
        &mut self,
        tender_id: &TenderId,
        command: &InvalidateBidDecisionApprovalCommand,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
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
        append_bid_decision_invalidation_denial(
            &transaction,
            tender_id,
            tender_revision,
            command,
            reason,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn inspect_bid_decision_package(
        &self,
        package_id: &str,
        version: u32,
    ) -> Result<BidDecisionPackageInspection, TenderCommandError> {
        if !valid_identifier(package_id) || version == 0 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let raw: Option<StoredPackageInspectionRow> = self
            .connection
            .query_row(
                "SELECT tender_revision, record_inventory_sha256, capability_demands_json,
                        resource_implications_json, recommendation_json,
                        analysis_blocker_count, manifest_sha256, created_by, created_at
                 FROM bid_decision_package_versions
                 WHERE package_id = ?1 AND version = ?2",
                params![package_id, version],
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
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?;
        let Some((
            tender_revision,
            record_inventory_sha256,
            capability_json,
            resource_implications_json,
            recommendation_json,
            stored_blockers,
            manifest_sha256,
            created_by,
            created_at,
        )) = raw
        else {
            return Err(TenderCommandError::new(TenderErrorCode::NotFound));
        };
        let capability_demands: Vec<CapabilityDemand> = parse_canonical_json(&capability_json)?;
        let resource_implications: Vec<ResourceImplication> =
            parse_canonical_json(&resource_implications_json)?;
        let recommendation: BidRecommendation = parse_canonical_json(&recommendation_json)?;
        let compliance_row_count =
            count_rows(&self.connection, "bid_compliance_rows", package_id, version)?;
        let compliance_blocker_count: u32 = self
            .connection
            .query_row(
                "SELECT COALESCE(SUM(json_array_length(blocker_codes_json)), 0)
                 FROM bid_compliance_rows WHERE package_id = ?1 AND package_version = ?2",
                params![package_id, version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let category_count = |category: BidDecisionPackageRecordCategory| {
            self.connection
                .query_row(
                    "SELECT COUNT(*) FROM bid_decision_package_record_bindings
                     WHERE package_id = ?1 AND package_version = ?2 AND category = ?3",
                    params![package_id, version, category.as_str()],
                    |row| row.get::<_, u32>(0),
                )
                .map_err(sql_error)
        };
        let project_fingerprint_count =
            category_count(BidDecisionPackageRecordCategory::ProjectFingerprint)?;
        let risk_count = category_count(BidDecisionPackageRecordCategory::Risk)?;
        let opportunity_count = category_count(BidDecisionPackageRecordCategory::Opportunity)?;
        let assumption_count = category_count(BidDecisionPackageRecordCategory::Assumption)?;
        let unresolved_query_count =
            category_count(BidDecisionPackageRecordCategory::UnresolvedQuery)?;
        let review = load_package_review(&self.connection, package_id, version)?;
        let approval = load_package_approval(&self.connection, package_id, version)?;
        let return_rework = approval
            .as_ref()
            .map(|approval| load_return_rework_disposition(&self.connection, &approval.approval_id))
            .transpose()?
            .flatten();
        let return_rework_basis =
            return_rework_basis_for_package(&self.connection, package_id, version)?;
        let material_change_basis =
            material_change_basis_for_package(&self.connection, package_id, version)?;
        let prior_approval_count = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM bid_decision_approval_records
                 WHERE NOT (package_id = ?1 AND package_version = ?2)",
                params![package_id, version],
                |row| row.get::<_, u32>(0),
            )
            .map_err(sql_error)?;
        let lifecycle_phase = TenderLifecyclePhase::parse(
            &self
                .connection
                .query_row(
                    "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sql_error)?,
        )?;
        let change_summary = package_change_summary(
            &self.connection,
            package_id,
            version,
            &capability_demands,
            &resource_implications,
        )?;
        let current_version: Option<u32> = self
            .connection
            .query_row(
                "SELECT current_version FROM bid_decision_package_heads WHERE package_id = ?1",
                [package_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let current = current_version == Some(version);
        let dependencies_current = current
            && package_dependencies_are_current(
                self,
                package_id,
                version,
                tender_revision,
                &record_inventory_sha256,
            )?;
        let mut blockers = load_visible_package_blockers(
            self,
            package_id,
            version,
            project_fingerprint_count,
            &capability_demands,
        )?;
        if !dependencies_current {
            blockers.push(BidDecisionGateBlocker {
                code: "package_dependencies_stale".into(),
                summary: "One or more exact Tender Record dependencies are no longer current and verified.".into(),
                record: None,
            });
        }
        match review.as_ref().map(|review| review.outcome) {
            None => blockers.push(BidDecisionGateBlocker {
                code: "independent_review_missing".into(),
                summary:
                    "The exact Bid Decision Package version has not received Independent Review."
                        .into(),
                record: None,
            }),
            Some(BidDecisionPackageReviewOutcome::Failed) => {
                blockers.push(BidDecisionGateBlocker {
                    code: "independent_review_failed".into(),
                    summary: "Independent Review failed for this exact package version.".into(),
                    record: None,
                })
            }
            Some(BidDecisionPackageReviewOutcome::Passed) => {}
        }
        let computed_analysis_blockers = usize::try_from(stored_blockers)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let decision_gate_ready = current
            && dependencies_current
            && computed_analysis_blockers == 0
            && approval.is_none()
            && lifecycle_phase == TenderLifecyclePhase::BidDecision
            && review
                .as_ref()
                .is_some_and(|review| review.outcome == BidDecisionPackageReviewOutcome::Passed);
        let blocker_count = computed_analysis_blockers
            .checked_add(usize::from(!dependencies_current))
            .and_then(|count| {
                count.checked_add(usize::from(review.as_ref().is_none_or(|review| {
                    review.outcome != BidDecisionPackageReviewOutcome::Passed
                })))
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        blockers.truncate(MAX_VISIBLE_BLOCKERS);
        Ok(BidDecisionPackageInspection {
            package_id: package_id.to_owned(),
            version,
            tender_revision,
            manifest_sha256,
            compliance_row_count,
            compliance_blocker_count,
            project_fingerprint_count,
            risk_count,
            opportunity_count,
            assumption_count,
            unresolved_query_count,
            capability_gap_count: capability_demands
                .iter()
                .filter(|demand| !demand.supported)
                .count() as u32,
            capability_demands,
            resource_implications,
            recommendation,
            blocker_count: u32::try_from(blocker_count)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            blockers,
            review,
            approval,
            return_rework,
            return_rework_basis,
            material_change_basis,
            prior_approval_count,
            change_summary,
            lifecycle_phase,
            decision_gate_ready,
            current: dependencies_current,
            created_by,
            created_at,
        })
    }

    pub(crate) fn inspect_compliance_matrix_page(
        &self,
        package_id: &str,
        version: u32,
        after_ordinal: Option<u32>,
        limit: u32,
    ) -> Result<ComplianceMatrixPage, TenderCommandError> {
        if !valid_identifier(package_id) || version == 0 || limit == 0 || limit > MAX_PAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let rows = load_stored_rows_after(
            &self.connection,
            package_id,
            version,
            after_ordinal.unwrap_or(0),
            limit + 1,
        )?;
        let mut inspections = Vec::new();
        let mut last_ordinal = None;
        for (ordinal, row) in rows.iter().take(limit as usize) {
            inspections.push(ComplianceMatrixRow {
                ordinal: *ordinal,
                record: self
                    .inspect_tender_record_version(&row.record.record_id, row.record.version)?,
                disposition: row.disposition,
                responsibility: row.responsibility.clone(),
                planned_treatment: row.planned_treatment.clone(),
                affected_work: row.affected_work.clone(),
                uncertainty: row.uncertainty.clone(),
                related_records: row.related_records.clone(),
                blocker_codes: row.blocker_codes.clone(),
            });
            if canonical_json(&inspections)?.len() > MAX_PAGE_BYTES {
                inspections.pop();
                break;
            }
            last_ordinal = Some(*ordinal);
        }
        if inspections.is_empty() && !rows.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(ComplianceMatrixPage {
            next_ordinal: (rows.len() > inspections.len())
                .then_some(last_ordinal)
                .flatten(),
            rows: inspections,
        })
    }

    pub(crate) fn inspect_bid_decision_package_record_page(
        &self,
        package_id: &str,
        version: u32,
        category: BidDecisionPackageRecordCategory,
        after_ordinal: Option<u32>,
        limit: u32,
    ) -> Result<BidDecisionPackageRecordPage, TenderCommandError> {
        if !valid_identifier(package_id) || version == 0 || limit == 0 || limit > MAX_PAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT ordinal, record_id, record_version
                 FROM bid_decision_package_record_bindings
                 WHERE package_id = ?1 AND package_version = ?2 AND category = ?3
                   AND ordinal > ?4 ORDER BY ordinal LIMIT ?5",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(
                params![
                    package_id,
                    version,
                    category.as_str(),
                    after_ordinal.unwrap_or(0),
                    limit + 1
                ],
                |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?,
                    ))
                },
            )
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        let mut records = Vec::new();
        let mut last_ordinal = None;
        for (ordinal, record_id, record_version) in rows.iter().take(limit as usize) {
            records.push(BidDecisionPackageRecordBinding {
                ordinal: *ordinal,
                category,
                record: self.inspect_tender_record_version(record_id, *record_version)?,
            });
            if canonical_json(&records)?.len() > MAX_PAGE_BYTES {
                records.pop();
                break;
            }
            last_ordinal = Some(*ordinal);
        }
        if records.is_empty() && !rows.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(BidDecisionPackageRecordPage {
            next_ordinal: (rows.len() > records.len())
                .then_some(last_ordinal)
                .flatten(),
            records,
        })
    }
}

fn validate_approval_command(
    command: &DecideBidDecisionPackageCommand,
) -> Result<(), TenderCommandError> {
    if !valid_identifier(&command.tender_id)
        || !valid_identifier(&command.package_id)
        || command.version == 0
        || !valid_sha256(&command.manifest_sha256)
        || command.rationale.trim().is_empty()
        || command.rationale.len() > 4_000
        || command.conditions.len() > MAX_APPROVAL_LIST_ITEMS
        || command.exceptions.len() > MAX_APPROVAL_LIST_ITEMS
        || command.required_rework.len() > MAX_APPROVAL_LIST_ITEMS
        || (command.decision == BidDecisionApprovalDecision::Return
            && command.required_rework.is_empty())
        || (command.decision != BidDecisionApprovalDecision::Return
            && !command.required_rework.is_empty())
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    normalized_approval_items(&command.conditions)?;
    normalized_approval_items(&command.exceptions)?;
    normalized_approval_items(&command.required_rework)?;
    Ok(())
}

fn append_bid_decision_denial(
    transaction: &rusqlite::Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    command: &DecideBidDecisionPackageCommand,
    reason: &str,
) -> Result<(), TenderCommandError> {
    let created_at = sqlite_timestamp(transaction)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "bid_decision_approval_denied",
        tender_revision,
        json!({
            "decision": command.decision,
            "package_id": command.package_id,
            "package_manifest_sha256": command.manifest_sha256,
            "package_version": command.version.to_string(),
            "reason": reason,
        }),
        &created_at,
    )
}

fn append_bid_decision_invalidation_denial(
    transaction: &rusqlite::Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    command: &InvalidateBidDecisionApprovalCommand,
    reason: &str,
) -> Result<(), TenderCommandError> {
    let created_at = sqlite_timestamp(transaction)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "bid_decision_approval_invalidation_denied",
        tender_revision,
        json!({
            "approval_id": command.approval_id,
            "approval_sha256": command.approval_sha256,
            "reason": reason,
        }),
        &created_at,
    )
}

fn normalized_approval_items(values: &[String]) -> Result<Vec<String>, TenderCommandError> {
    if values.len() > MAX_APPROVAL_LIST_ITEMS {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let normalized = values
        .iter()
        .map(|value| value.trim().to_owned())
        .collect::<Vec<_>>();
    if normalized
        .iter()
        .any(|value| value.is_empty() || value.len() > MAX_APPROVAL_ITEM_BYTES)
        || normalized.iter().collect::<HashSet<_>>().len() != normalized.len()
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(normalized)
}

fn approval_evidence(
    store: &TenderStore,
    package_id: &str,
    version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<TenderEvidenceReference>, TenderCommandError> {
    let mut records = package_bound_records(&store.connection, package_id, version)?
        .into_iter()
        .collect::<Vec<_>>();
    records.sort();
    let mut evidence = HashSet::new();
    for (record_id, record_version) in records {
        check()?;
        let record = store.inspect_tender_record_version(&record_id, record_version)?;
        evidence.extend(
            record
                .fields
                .iter()
                .flat_map(|field| field.evidence.iter())
                .chain(
                    record
                        .contradictions
                        .iter()
                        .flat_map(|contradiction| contradiction.evidence.iter()),
                )
                .map(|item| item.reference.clone()),
        );
    }
    let mut evidence = evidence.into_iter().collect::<Vec<_>>();
    evidence.sort_by(|left, right| {
        left.artifact_id
            .cmp(&right.artifact_id)
            .then_with(|| left.version.cmp(&right.version))
            .then_with(|| left.ordinal.cmp(&right.ordinal))
    });
    Ok(evidence)
}

type StoredApprovalRow = (
    u32,
    String,
    String,
    u32,
    String,
    u32,
    String,
    String,
    String,
    String,
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

type StoredApprovalInvalidationRow = (
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

fn load_package_approval(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
) -> Result<Option<BidDecisionApprovalRecord>, TenderCommandError> {
    let row = connection
        .query_row(
            "SELECT approval_sequence, approval_id, package_id, package_version,
                    package_manifest_sha256, tender_revision, decision, decided_by,
                    acting_role, rationale, evidence_count, evidence_sha256,
                    conditions_json, exceptions_json, required_rework_json,
                    lifecycle_before, lifecycle_after, consequence,
                    preceding_approval_hash, approval_manifest_json, approval_sha256,
                    created_at
             FROM bid_decision_approval_records
             WHERE package_id = ?1 AND package_version = ?2",
            params![package_id, version],
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
                    row.get(16)?,
                    row.get(17)?,
                    row.get(18)?,
                    row.get(19)?,
                    row.get(20)?,
                    row.get(21)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let mut approval = approval_record_from_row(row)?;
    approval.invalidation = load_approval_invalidation(connection, &approval.approval_id)?;
    Ok(Some(approval))
}

fn load_approval_by_id(
    connection: &rusqlite::Connection,
    approval_id: &str,
) -> Result<Option<BidDecisionApprovalRecord>, TenderCommandError> {
    let key: Option<(String, u32)> = connection
        .query_row(
            "SELECT package_id, package_version FROM bid_decision_approval_records
             WHERE approval_id = ?1",
            [approval_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    key.map(|(package_id, version)| load_package_approval(connection, &package_id, version))
        .transpose()
        .map(Option::flatten)
}

fn load_return_rework_disposition(
    connection: &rusqlite::Connection,
    approval_id: &str,
) -> Result<Option<BidDecisionReturnReworkDisposition>, TenderCommandError> {
    let row: Option<(String, String, String, String, String, String, String)> = connection
        .query_row(
            "SELECT disposition_id, approval_sha256, items_json, resolved_by,
                    manifest_json, manifest_sha256, created_at
             FROM bid_decision_return_rework_dispositions WHERE approval_id = ?1",
            [approval_id],
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
        disposition_id,
        approval_sha256,
        items_json,
        resolved_by,
        manifest_json,
        manifest_sha256,
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    if manifest_json.len() > MAX_APPROVAL_MANIFEST_BYTES
        || sha256_hex(manifest_json.as_bytes()) != manifest_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let items: Vec<BidDecisionReturnReworkItem> = parse_canonical_json(&items_json)?;
    let manifest: ReturnReworkManifest = parse_canonical_json(&manifest_json)?;
    if manifest.schema_version != 1
        || manifest.disposition_id != disposition_id
        || manifest.approval_id != approval_id
        || manifest.approval_sha256 != approval_sha256
        || manifest.items != items
        || manifest.resolved_by != resolved_by
        || manifest.created_at != created_at
        || resolved_by != "engineer_user"
        || !valid_sha256(&approval_sha256)
        || !valid_sha256(&manifest_sha256)
        || items.is_empty()
        || items.len() > MAX_APPROVAL_LIST_ITEMS
        || items.iter().any(|item| {
            item.required_rework.trim() != item.required_rework
                || item.required_rework.is_empty()
                || item.required_rework.len() > MAX_APPROVAL_ITEM_BYTES
                || item.resolution.trim() != item.resolution
                || item.resolution.is_empty()
                || item.resolution.len() > MAX_APPROVAL_ITEM_BYTES
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(Some(BidDecisionReturnReworkDisposition {
        disposition_id,
        approval_id: approval_id.into(),
        approval_sha256,
        items,
        resolved_by,
        manifest_sha256,
        created_at,
    }))
}

fn load_approval_invalidation(
    connection: &rusqlite::Connection,
    approval_id: &str,
) -> Result<Option<BidDecisionApprovalInvalidation>, TenderCommandError> {
    let row: Option<StoredApprovalInvalidationRow> = connection
        .query_row(
            "SELECT invalidation_id, approval_sha256, material_change_summary,
                    affected_areas_json, changed_records_json, invalidated_by, acting_role, lifecycle_before,
                    lifecycle_after, manifest_json, manifest_sha256, created_at
             FROM bid_decision_approval_invalidations WHERE approval_id = ?1",
            [approval_id],
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
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some((
        invalidation_id,
        approval_sha256,
        material_change_summary,
        affected_areas_json,
        changed_records_json,
        invalidated_by,
        acting_role,
        lifecycle_before,
        lifecycle_after,
        manifest_json,
        manifest_sha256,
        created_at,
    )) = row
    else {
        return Ok(None);
    };
    if manifest_json.len() > MAX_APPROVAL_MANIFEST_BYTES
        || sha256_hex(manifest_json.as_bytes()) != manifest_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let affected_areas: Vec<String> = parse_canonical_json(&affected_areas_json)?;
    let changed_records: Vec<TenderRecordVersionReference> =
        parse_canonical_json(&changed_records_json)?;
    let manifest: ApprovalInvalidationManifest = parse_canonical_json(&manifest_json)?;
    let lifecycle_before = TenderLifecyclePhase::parse(&lifecycle_before)?;
    let lifecycle_after = TenderLifecyclePhase::parse(&lifecycle_after)?;
    if manifest.schema_version != 1
        || manifest.invalidation_id != invalidation_id
        || manifest.approval_id != approval_id
        || manifest.approval_sha256 != approval_sha256
        || manifest.material_change_summary != material_change_summary
        || manifest.affected_areas != affected_areas
        || manifest.changed_records != changed_records
        || manifest.invalidated_by != invalidated_by
        || manifest.acting_role != acting_role
        || manifest.lifecycle_before != lifecycle_before
        || manifest.lifecycle_after != lifecycle_after
        || manifest.created_at != created_at
        || invalidated_by != "engineer_user"
        || acting_role != "tendering_manager"
        || lifecycle_before != TenderLifecyclePhase::TenderPlanning
        || lifecycle_after != TenderLifecyclePhase::BidDecision
        || material_change_summary.trim() != material_change_summary
        || material_change_summary.is_empty()
        || material_change_summary.len() > 4_000
        || normalized_approval_items(&affected_areas)? != affected_areas
        || affected_areas.is_empty()
        || !valid_material_change_record_count(&changed_records)
        || changed_records
            .iter()
            .any(|record| !valid_identifier(&record.record_id) || record.version == 0)
        || !valid_identifier(&invalidation_id)
        || !valid_sha256(&approval_sha256)
        || !valid_sha256(&manifest_sha256)
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(Some(BidDecisionApprovalInvalidation {
        invalidation_id,
        approval_id: approval_id.into(),
        approval_sha256,
        material_change_summary,
        affected_areas,
        changed_records,
        invalidated_by,
        acting_role,
        lifecycle_before,
        lifecycle_after,
        manifest_sha256,
        created_at,
    }))
}

fn return_rework_basis_for_package(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
) -> Result<Option<BidDecisionReturnReworkDisposition>, TenderCommandError> {
    let Some(prior_version) = version.checked_sub(1).filter(|prior| *prior > 0) else {
        return Ok(None);
    };
    let Some(approval) = load_package_approval(connection, package_id, prior_version)? else {
        return Ok(None);
    };
    if approval.decision != BidDecisionApprovalDecision::Return {
        return Ok(None);
    }
    load_return_rework_disposition(connection, &approval.approval_id)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
        .map(Some)
}

fn material_change_basis_for_package(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
) -> Result<Option<BidDecisionApprovalInvalidation>, TenderCommandError> {
    let Some(prior_version) = version.checked_sub(1).filter(|prior| *prior > 0) else {
        return Ok(None);
    };
    let Some(approval) = load_package_approval(connection, package_id, prior_version)? else {
        return Ok(None);
    };
    match approval.decision {
        BidDecisionApprovalDecision::Accept => approval
            .invalidation
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
            .map(Some),
        BidDecisionApprovalDecision::Return => Ok(None),
        BidDecisionApprovalDecision::Reject => {
            Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed))
        }
    }
}

fn load_approval_history(
    connection: &rusqlite::Connection,
    before_sequence: Option<u32>,
    limit: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<BidDecisionApprovalRecord>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT package_id, package_version FROM bid_decision_approval_records
             WHERE (?1 IS NULL OR approval_sequence < ?1)
             ORDER BY approval_sequence DESC LIMIT ?2",
        )
        .map_err(sql_error)?;
    let keys = statement
        .query_map(params![before_sequence, limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    let mut approvals = Vec::with_capacity(keys.len());
    for (package_id, version) in keys {
        check()?;
        approvals.push(
            load_package_approval(connection, &package_id, version)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        );
    }
    Ok(approvals)
}

fn approval_record_from_row(
    row: StoredApprovalRow,
) -> Result<BidDecisionApprovalRecord, TenderCommandError> {
    let (
        approval_sequence,
        approval_id,
        package_id,
        package_version,
        package_manifest_sha256,
        tender_revision,
        decision,
        decided_by,
        acting_role,
        rationale,
        evidence_count,
        evidence_sha256,
        conditions_json,
        exceptions_json,
        required_rework_json,
        lifecycle_before,
        lifecycle_after,
        consequence,
        preceding_approval_hash,
        approval_manifest_json,
        approval_sha256,
        created_at,
    ) = row;
    if approval_manifest_json.len() > MAX_APPROVAL_MANIFEST_BYTES
        || sha256_hex(approval_manifest_json.as_bytes()) != approval_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manifest: ApprovalManifest = parse_canonical_json(&approval_manifest_json)?;
    let conditions: Vec<String> = parse_canonical_json(&conditions_json)?;
    let exceptions: Vec<String> = parse_canonical_json(&exceptions_json)?;
    let required_rework: Vec<String> = parse_canonical_json(&required_rework_json)?;
    let decision = BidDecisionApprovalDecision::parse(&decision)?;
    let lifecycle_before = TenderLifecyclePhase::parse(&lifecycle_before)?;
    let lifecycle_after = TenderLifecyclePhase::parse(&lifecycle_after)?;
    if manifest.schema_version != 1
        || manifest.approval_sequence != approval_sequence
        || manifest.approval_id != approval_id
        || manifest.package_id != package_id
        || manifest.package_version != package_version
        || manifest.package_manifest_sha256 != package_manifest_sha256
        || manifest.tender_revision != tender_revision
        || manifest.decision != decision
        || manifest.decided_by != decided_by
        || manifest.acting_role != acting_role
        || manifest.rationale != rationale
        || manifest.evidence_count != evidence_count
        || manifest.evidence_sha256 != evidence_sha256
        || manifest.conditions != conditions
        || manifest.exceptions != exceptions
        || manifest.required_rework != required_rework
        || manifest.lifecycle_before != lifecycle_before
        || manifest.lifecycle_after != lifecycle_after
        || manifest.consequence != consequence
        || manifest.preceding_approval_hash != preceding_approval_hash
        || manifest.created_at != created_at
        || decided_by != "engineer_user"
        || acting_role != "tendering_manager"
        || lifecycle_before != TenderLifecyclePhase::BidDecision
        || lifecycle_after != decision.lifecycle_after()
        || consequence != decision.consequence()
        || !valid_sha256(&approval_sha256)
        || !valid_sha256(&preceding_approval_hash)
        || !valid_sha256(&package_manifest_sha256)
        || !valid_sha256(&evidence_sha256)
        || normalized_approval_items(&conditions)? != conditions
        || normalized_approval_items(&exceptions)? != exceptions
        || normalized_approval_items(&required_rework)? != required_rework
        || (decision == BidDecisionApprovalDecision::Return && required_rework.is_empty())
        || (decision != BidDecisionApprovalDecision::Return && !required_rework.is_empty())
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(BidDecisionApprovalRecord {
        approval_sequence,
        approval_id,
        package_id,
        package_version,
        package_manifest_sha256,
        tender_revision,
        decision,
        decided_by,
        acting_role,
        rationale,
        evidence_count,
        evidence_sha256,
        conditions,
        exceptions,
        required_rework,
        lifecycle_before,
        lifecycle_after,
        consequence,
        preceding_approval_hash,
        approval_sha256,
        created_at,
        invalidation: None,
    })
}

fn validate_package_command(
    command: &CreateBidDecisionPackageCommand,
) -> Result<(), TenderCommandError> {
    if !valid_identifier(&command.tender_id)
        || command.disposition_updates.len() > MAX_COMPLIANCE_ROWS
        || command.manager_capability_demands.len() > MAX_MANAGER_DEMANDS
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    for update in &command.disposition_updates {
        if !valid_identifier(&update.record.record_id)
            || update.record.version == 0
            || update.responsibility.trim().is_empty()
            || update.responsibility.len() > 200
            || update.planned_treatment.trim().is_empty()
            || update.planned_treatment.len() > 2_000
            || update.affected_work.is_empty()
            || update.affected_work.len() > 32
            || update
                .affected_work
                .iter()
                .any(|value| value.is_empty() || value.len() > 100 || !valid_work_key(value))
            || update
                .uncertainty
                .as_deref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 2_000)
            || update.related_records.len() > 32
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let affected = update.affected_work.iter().collect::<HashSet<_>>();
        let related = update
            .related_records
            .iter()
            .map(|reference| (&reference.record_id, reference.version))
            .collect::<HashSet<_>>();
        if affected.len() != update.affected_work.len()
            || related.len() != update.related_records.len()
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    for demand in &command.manager_capability_demands {
        if !valid_capability(demand.capability.trim())
            || demand.rationale.trim().is_empty()
            || demand.rationale.len() > 1_000
            || demand.triggering_record.as_ref().is_some_and(|reference| {
                !valid_identifier(&reference.record_id) || reference.version == 0
            })
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    Ok(())
}

fn validate_disposition_update(
    update: &ComplianceDispositionUpdate,
    metadata: &HashMap<
        (String, u32),
        (TenderRecordKind, VerificationStatus, TenderRecordTrustClass),
    >,
) -> Result<(), TenderCommandError> {
    for reference in &update.related_records {
        let Some((kind, _, _)) = metadata.get(&(reference.record_id.clone(), reference.version))
        else {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        if !matches!(
            kind,
            TenderRecordKind::Assumption | TenderRecordKind::TenderQuery
        ) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    Ok(())
}

fn current_record_references(
    connection: &rusqlite::Connection,
) -> Result<Vec<TenderRecordVersionReference>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT tender_record_heads.record_id, tender_record_heads.current_version
             FROM tender_record_heads JOIN tender_records USING (record_id)
             ORDER BY tender_records.stable_key",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(TenderRecordVersionReference {
                record_id: row.get(0)?,
                version: row.get(1)?,
            })
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    Ok(rows)
}

fn is_compliance_kind(kind: TenderRecordKind) -> bool {
    matches!(
        kind,
        TenderRecordKind::Requirement
            | TenderRecordKind::EvaluationCriterion
            | TenderRecordKind::Deliverable
            | TenderRecordKind::Deadline
            | TenderRecordKind::Form
            | TenderRecordKind::Clause
    )
}

fn default_compliance_row(record: &TenderRecordInspection) -> StoredComplianceRow {
    let responsibility = match record.kind {
        TenderRecordKind::Deliverable | TenderRecordKind::Form => "Document Controller",
        TenderRecordKind::Deadline => "Tender Office Coordinator",
        _ => "Tender Analyst",
    };
    let uncertainty = record
        .fields
        .iter()
        .filter_map(|field| field.uncertainty.as_deref())
        .next()
        .map(str::to_owned);
    StoredComplianceRow {
        record: TenderRecordVersionReference {
            record_id: record.record_id.clone(),
            version: record.version,
        },
        disposition: ComplianceDisposition::Unresolved,
        responsibility: responsibility.into(),
        planned_treatment: "Resolve this exact obligation before the Bid Decision gate.".into(),
        affected_work: vec!["bid_decision".into(), "tender_planning".into()],
        uncertainty,
        related_records: Vec::new(),
        verification_status: record.verification_status,
        trust_class: record.trust_class,
        evidence_count: 0,
        blocker_codes: Vec::new(),
    }
}

fn record_evidence_count(record: &TenderRecordInspection) -> Result<u32, TenderCommandError> {
    let count = record
        .fields
        .iter()
        .flat_map(|field| field.evidence.iter().map(|evidence| &evidence.reference))
        .chain(record.contradictions.iter().flat_map(|contradiction| {
            contradiction
                .evidence
                .iter()
                .map(|evidence| &evidence.reference)
        }))
        .collect::<HashSet<_>>()
        .len();
    u32::try_from(count).map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn compliance_blocker_codes(
    record: &TenderRecordInspection,
    row: &StoredComplianceRow,
) -> Vec<String> {
    let mut codes = Vec::new();
    if row.disposition == ComplianceDisposition::Unresolved {
        codes.push("unresolved_disposition".into());
    }
    if record.verification_status != VerificationStatus::Verified {
        codes.push("record_not_verified".into());
    }
    if row.evidence_count == 0 {
        codes.push("missing_exact_evidence".into());
    }
    if !record.contradictions.is_empty()
        && record.verification_status != VerificationStatus::Verified
    {
        codes.push("unresolved_blocking_contradiction".into());
    }
    codes
}

fn capability_is_supported(value: &str) -> bool {
    matches!(
        value,
        "cost_estimation"
            | "query_rfi_control"
            | "tender_coordination"
            | "document_control"
            | "tender_analysis"
            | "independent_review"
            | "programme_planning"
            | "contracts_review"
            | "procurement_analysis"
            | "technical_review"
    )
}

fn valid_capability(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_work_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn deduplicate_capability_demands(demands: &mut Vec<CapabilityDemand>) {
    let mut seen = HashSet::new();
    demands.retain(|demand| {
        seen.insert((
            demand.capability.clone(),
            demand.classification,
            demand.triggering_record.clone(),
        ))
    });
}

fn package_manifest_value(package: &PackageManifest<'_>) -> Result<Value, TenderCommandError> {
    package_manifest_value_with_check(package, &mut || Ok(()))
}

fn package_manifest_value_with_check(
    package: &PackageManifest<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Value, TenderCommandError> {
    let mut rows = Vec::with_capacity(package.rows.len());
    for (index, row) in package.rows.iter().enumerate() {
        check()?;
        rows.push(json!({
            "affected_work": row.affected_work,
            "blocker_codes": row.blocker_codes,
            "disposition": row.disposition,
            "evidence_count": row.evidence_count,
            "ordinal": index + 1,
            "planned_treatment": row.planned_treatment,
            "record": row.record,
            "related_records": row.related_records,
            "responsibility": row.responsibility,
            "trust_class": row.trust_class,
            "uncertainty": row.uncertainty,
            "verification_status": row.verification_status,
        }));
    }
    let mut category_ordinals = HashMap::new();
    let mut bindings = Vec::with_capacity(package.bindings.len());
    for binding in package.bindings {
        check()?;
        let ordinal = category_ordinals
            .entry(binding.category)
            .and_modify(|value: &mut u32| *value += 1)
            .or_insert(1);
        bindings.push(json!({
            "category": binding.category,
            "ordinal": *ordinal,
            "record": binding.record,
        }));
    }
    for _ in package.capability_demands {
        check()?;
    }
    for _ in package.resource_implications {
        check()?;
    }
    Ok(json!({
        "analysis_blocker_count": package.analysis_blocker_count,
        "capability_demands": package.capability_demands,
        "compliance_matrix": rows,
        "package_id": package.package_id,
        "package_version": package.version,
        "recommendation": package.recommendation,
        "record_inventory_sha256": package.record_inventory_sha256,
        "record_bindings": bindings,
        "resource_implications": package.resource_implications,
        "return_rework_basis": package.return_rework_basis,
        "material_change_basis": package.material_change_basis,
        "schema_version": 1,
        "tender_revision": package.tender_revision,
    }))
}

fn load_stored_rows(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
) -> Result<Vec<StoredComplianceRow>, TenderCommandError> {
    load_stored_rows_with_check(connection, package_id, version, &mut || Ok(()))
}

fn load_stored_rows_with_check(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<StoredComplianceRow>, TenderCommandError> {
    let row_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM bid_compliance_rows
             WHERE package_id = ?1 AND package_version = ?2",
            params![package_id, version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if row_count > MAX_COMPLIANCE_ROWS as u32 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(
        load_stored_rows_after_with_check(connection, package_id, version, 0, row_count, check)?
            .into_iter()
            .map(|(_, row)| row)
            .collect(),
    )
}

fn load_stored_rows_after(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
    after_ordinal: u32,
    limit: u32,
) -> Result<Vec<(u32, StoredComplianceRow)>, TenderCommandError> {
    load_stored_rows_after_with_check(
        connection,
        package_id,
        version,
        after_ordinal,
        limit,
        &mut || Ok(()),
    )
}

fn load_stored_rows_after_with_check(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
    after_ordinal: u32,
    limit: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<(u32, StoredComplianceRow)>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT ordinal, record_id, record_version, disposition, responsibility,
                    planned_treatment, affected_work_json, uncertainty, related_records_json,
                    verification_status, trust_class, evidence_count, blocker_codes_json
             FROM bid_compliance_rows
             WHERE package_id = ?1 AND package_version = ?2 AND ordinal > ?3
             ORDER BY ordinal LIMIT ?4",
        )
        .map_err(sql_error)?;
    let mapped = statement
        .query_map(params![package_id, version, after_ordinal, limit], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, u32>(11)?,
                row.get::<_, String>(12)?,
            ))
        })
        .map_err(sql_error)?;
    let mut rows = Vec::new();
    for raw in mapped {
        check()?;
        let (
            ordinal,
            record_id,
            record_version,
            disposition,
            responsibility,
            planned_treatment,
            affected_work_json,
            uncertainty,
            related_json,
            status,
            trust,
            evidence_count,
            blocker_json,
        ) = raw.map_err(sql_error)?;
        rows.push((
            ordinal,
            StoredComplianceRow {
                record: TenderRecordVersionReference {
                    record_id,
                    version: record_version,
                },
                disposition: ComplianceDisposition::parse(&disposition)?,
                responsibility,
                planned_treatment,
                affected_work: parse_canonical_json(&affected_work_json)?,
                uncertainty,
                related_records: parse_canonical_json(&related_json)?,
                verification_status: parse_verification_status(&status)?,
                trust_class: parse_trust_class(&trust)?,
                evidence_count,
                blocker_codes: parse_canonical_json(&blocker_json)?,
            },
        ));
    }
    Ok(rows)
}

pub(super) fn package_dependencies_are_current(
    store: &TenderStore,
    package_id: &str,
    version: u32,
    tender_revision: u32,
    record_inventory_sha256: &str,
) -> Result<bool, TenderCommandError> {
    let current_tender_revision: u32 = store
        .connection
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let current_inventory = current_record_inventory(store)?;
    if current_tender_revision != tender_revision
        || sha256_hex(canonical_json(&current_inventory)?.as_bytes()) != record_inventory_sha256
    {
        return Ok(false);
    }
    for row in load_stored_rows(&store.connection, package_id, version)? {
        let current_version: Option<u32> = store
            .connection
            .query_row(
                "SELECT current_version FROM tender_record_heads WHERE record_id = ?1",
                [&row.record.record_id],
                |result| result.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if current_version != Some(row.record.version) {
            return Ok(false);
        }
        let record =
            store.inspect_tender_record_version(&row.record.record_id, row.record.version)?;
        if record.verification_status != row.verification_status
            || record.trust_class != row.trust_class
        {
            return Ok(false);
        }
    }
    let mut statement = store
        .connection
        .prepare(
            "SELECT record_id, record_version FROM bid_decision_package_record_bindings
             WHERE package_id = ?1 AND package_version = ?2",
        )
        .map_err(sql_error)?;
    let refs = statement
        .query_map(params![package_id, version], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    for (record_id, record_version) in refs {
        let current_version: Option<u32> = store
            .connection
            .query_row(
                "SELECT current_version FROM tender_record_heads WHERE record_id = ?1",
                [&record_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if current_version != Some(record_version) {
            return Ok(false);
        }
        let record = store.inspect_tender_record_version(&record_id, record_version)?;
        if matches!(
            record.verification_status,
            VerificationStatus::Stale | VerificationStatus::Superseded
        ) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn package_change_summary(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
    capabilities: &[CapabilityDemand],
    resources: &[ResourceImplication],
) -> Result<BidDecisionPackageChangeSummary, TenderCommandError> {
    let current_records = package_bound_records(connection, package_id, version)?;
    let current_rows = load_stored_rows(connection, package_id, version)?
        .into_iter()
        .map(|row| ((row.record.record_id.clone(), row.record.version), row))
        .collect::<HashMap<_, _>>();
    let Some(prior_version) = version.checked_sub(1).filter(|prior| *prior > 0) else {
        return Ok(BidDecisionPackageChangeSummary {
            prior_version: None,
            added_record_count: u32::try_from(current_records.len())
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            removed_record_count: 0,
            changed_compliance_row_count: u32::try_from(current_rows.len())
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            capability_demands_changed: !capabilities.is_empty(),
            resource_implications_changed: !resources.is_empty(),
        });
    };
    let (prior_capabilities_json, prior_resources_json): (String, String) = connection
        .query_row(
            "SELECT capability_demands_json, resource_implications_json
             FROM bid_decision_package_versions
             WHERE package_id = ?1 AND version = ?2",
            params![package_id, prior_version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    let prior_capabilities: Vec<CapabilityDemand> = parse_canonical_json(&prior_capabilities_json)?;
    let prior_resources: Vec<ResourceImplication> = parse_canonical_json(&prior_resources_json)?;
    let prior_records = package_bound_records(connection, package_id, prior_version)?;
    let prior_rows = load_stored_rows(connection, package_id, prior_version)?
        .into_iter()
        .map(|row| ((row.record.record_id.clone(), row.record.version), row))
        .collect::<HashMap<_, _>>();
    let changed_rows = current_rows
        .iter()
        .filter(|(key, row)| prior_rows.get(key) != Some(*row))
        .count()
        .checked_add(
            prior_rows
                .keys()
                .filter(|key| !current_rows.contains_key(*key))
                .count(),
        )
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(BidDecisionPackageChangeSummary {
        prior_version: Some(prior_version),
        added_record_count: u32::try_from(current_records.difference(&prior_records).count())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        removed_record_count: u32::try_from(prior_records.difference(&current_records).count())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        changed_compliance_row_count: u32::try_from(changed_rows)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        capability_demands_changed: prior_capabilities != capabilities,
        resource_implications_changed: prior_resources != resources,
    })
}

fn current_record_inventory(
    store: &TenderStore,
) -> Result<Vec<RecordInventoryObservation>, TenderCommandError> {
    current_record_references(&store.connection)?
        .into_iter()
        .map(|reference| {
            let record =
                store.inspect_tender_record_version(&reference.record_id, reference.version)?;
            Ok(RecordInventoryObservation {
                record: reference,
                kind: record.kind,
                verification_status: record.verification_status,
                trust_class: record.trust_class,
            })
        })
        .collect()
}

fn current_record_inventory_bounded_with_check(
    store: &TenderStore,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<RecordInventoryObservation>, TenderCommandError> {
    check()?;
    let count: u32 = store
        .connection
        .query_row("SELECT COUNT(*) FROM tender_record_heads", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)?;
    if usize::try_from(count)
        .ok()
        .is_none_or(|count| count > MAX_COMPLIANCE_ROWS)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut statement = store
        .connection
        .prepare(
            "SELECT tender_record_heads.record_id, tender_record_heads.current_version
             FROM tender_record_heads JOIN tender_records USING (record_id)
             ORDER BY tender_records.stable_key",
        )
        .map_err(sql_error)?;
    let mapped = statement
        .query_map([], |row| {
            Ok(TenderRecordVersionReference {
                record_id: row.get(0)?,
                version: row.get(1)?,
            })
        })
        .map_err(sql_error)?;
    let mut inventory = Vec::with_capacity(count as usize);
    for reference in mapped {
        check()?;
        if inventory.len() >= MAX_COMPLIANCE_ROWS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let reference = reference.map_err(sql_error)?;
        let record =
            store.inspect_tender_record_version(&reference.record_id, reference.version)?;
        check()?;
        inventory.push(RecordInventoryObservation {
            record: reference,
            kind: record.kind,
            verification_status: record.verification_status,
            trust_class: record.trust_class,
        });
    }
    if inventory.len() != count as usize {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(inventory)
}

fn load_package_record_inventory(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
) -> Result<Vec<RecordInventoryObservation>, TenderCommandError> {
    let (inventory_json, inventory_sha256): (String, String) = connection
        .query_row(
            "SELECT record_inventory_json, record_inventory_sha256
             FROM bid_decision_package_versions
             WHERE package_id = ?1 AND version = ?2",
            params![package_id, version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    if inventory_json.len() > MAX_PACKAGE_MANIFEST_BYTES
        || sha256_hex(inventory_json.as_bytes()) != inventory_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let inventory: Vec<RecordInventoryObservation> = parse_canonical_json(&inventory_json)?;
    if inventory.len() > MAX_COMPLIANCE_ROWS
        || inventory.iter().any(|observation| {
            !valid_identifier(&observation.record.record_id) || observation.record.version == 0
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(inventory)
}

fn changed_record_references(
    prior_inventory: &[RecordInventoryObservation],
    current_inventory: &[RecordInventoryObservation],
) -> Vec<TenderRecordVersionReference> {
    let mut changed = current_inventory
        .iter()
        .filter(|current| {
            prior_inventory
                .iter()
                .find(|prior| prior.record.record_id == current.record.record_id)
                != Some(*current)
        })
        .map(|observation| observation.record.clone())
        .chain(
            prior_inventory
                .iter()
                .filter(|prior| {
                    !current_inventory
                        .iter()
                        .any(|current| current.record.record_id == prior.record.record_id)
                })
                .map(|observation| observation.record.clone()),
        )
        .collect::<Vec<_>>();
    changed.sort_by(|left, right| {
        left.record_id
            .cmp(&right.record_id)
            .then_with(|| left.version.cmp(&right.version))
    });
    changed.dedup();
    changed
}

fn valid_material_change_record_count(records: &[TenderRecordVersionReference]) -> bool {
    !records.is_empty() && records.len() <= MAX_MATERIAL_CHANGE_RECORDS
}

fn load_visible_package_blockers(
    store: &TenderStore,
    package_id: &str,
    version: u32,
    fingerprint_count: u32,
    capability_demands: &[CapabilityDemand],
) -> Result<Vec<BidDecisionGateBlocker>, TenderCommandError> {
    let mut blockers = Vec::new();
    for row in load_stored_rows(&store.connection, package_id, version)? {
        for code in &row.blocker_codes {
            if blockers.len() >= MAX_VISIBLE_BLOCKERS {
                return Ok(blockers);
            }
            blockers.push(BidDecisionGateBlocker {
                code: code.clone(),
                summary: blocker_summary(code).into(),
                record: Some(row.record.clone()),
            });
        }
    }
    if fingerprint_count == 0 {
        blockers.push(BidDecisionGateBlocker {
            code: "project_fingerprint_missing".into(),
            summary: "No verified Project Characteristic is available for the Project Fingerprint."
                .into(),
            record: None,
        });
    }
    for demand in capability_demands.iter().filter(|demand| !demand.supported) {
        blockers.push(BidDecisionGateBlocker {
            code: "capability_gap".into(),
            summary: format!(
                "Capability '{}' is not supported by the current Capability Catalogue.",
                demand.capability
            ),
            record: demand.triggering_record.clone(),
        });
    }
    let mut statement = store
        .connection
        .prepare(
            "SELECT category, record_id, record_version
             FROM bid_decision_package_record_bindings
             WHERE package_id = ?1 AND package_version = ?2
               AND category IN ('assumption', 'unresolved_query')
             ORDER BY category, ordinal",
        )
        .map_err(sql_error)?;
    let refs = statement
        .query_map(params![package_id, version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    for (category, record_id, record_version) in refs {
        if blockers.len() >= MAX_VISIBLE_BLOCKERS {
            break;
        }
        let record = store.inspect_tender_record_version(&record_id, record_version)?;
        if category == "unresolved_query"
            || record.verification_status != VerificationStatus::Verified
            || record.trust_class != TenderRecordTrustClass::ApprovedAssumption
        {
            blockers.push(BidDecisionGateBlocker {
                code: if category == "unresolved_query" {
                    "unresolved_tender_query"
                } else {
                    "unapproved_assumption"
                }
                .into(),
                summary: if category == "unresolved_query" {
                    "A Tender Query remains unresolved for the Bid Decision."
                } else {
                    "An Assumption has not received an exact Engineer approval."
                }
                .into(),
                record: Some(TenderRecordVersionReference {
                    record_id,
                    version: record_version,
                }),
            });
        }
    }
    Ok(blockers)
}

fn blocker_summary(code: &str) -> &'static str {
    match code {
        "unresolved_disposition" => {
            "This mandatory matrix row has no complete compliance disposition."
        }
        "record_not_verified" => "The exact source record is not Verified for controlled use.",
        "missing_exact_evidence" => "The matrix row has no exact authoritative Evidence reference.",
        "unresolved_blocking_contradiction" => {
            "The exact record contains a blocking contradiction that is not verified as resolved."
        }
        _ => "The exact compliance row is blocked by an invalid stored condition.",
    }
}

fn load_package_review(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
) -> Result<Option<BidDecisionPackageReview>, TenderCommandError> {
    connection
        .query_row(
            "SELECT review_id, reviewer_run_id, outcome, findings_json, created_at
             FROM bid_decision_package_reviews
             WHERE package_id = ?1 AND package_version = ?2",
            params![package_id, version],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .map(
            |(review_id, reviewer_run_id, outcome, findings_json, created_at)| {
                Ok(BidDecisionPackageReview {
                    review_id,
                    reviewer_run_id,
                    outcome: BidDecisionPackageReviewOutcome::parse(&outcome)?,
                    findings: parse_canonical_json(&findings_json)?,
                    created_at,
                })
            },
        )
        .transpose()
}

fn package_review_is_attributable(
    store: &TenderStore,
    package_id: &str,
    version: u32,
    tender_revision: u32,
    review: &BidDecisionPackageReview,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    check()?;
    if !valid_identifier(&review.review_id) || !valid_identifier(&review.reviewer_run_id) {
        return Ok(false);
    }
    let run = match store.inspect_agent_run_with_check(&review.reviewer_run_id, check) {
        Ok(run) => run,
        Err(error) if error.code == TenderErrorCode::OperationTimedOut => return Err(error),
        Err(_) => return Ok(false),
    };
    if run.state != AgentRunState::Completed
        || run.completed_at.as_deref() != Some(review.created_at.as_str())
        || run.profile != bid_package_review_profile(run.profile.profile_id.clone())
        || run.task.permissions != bid_package_review_permissions()
        || run.task.exact_inputs.len() != 2
    {
        return Ok(false);
    }
    let (target_package_id, target_version) = match exact_package_target(&run.task) {
        Ok(target) => target,
        Err(_) => return Ok(false),
    };
    let tender_id: String = store
        .connection
        .query_row(
            "SELECT tender_id FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let tender_input_matches = run.task.exact_inputs.iter().any(|input| {
        input.kind == "tender_revision"
            && input.reference == tender_id
            && input.version == tender_revision
    });
    if target_package_id != package_id
        || target_version != version
        || !tender_input_matches
        || run
            .profile
            .capabilities
            .iter()
            .all(|capability| capability != BID_PACKAGE_REVIEW_CAPABILITY)
    {
        return Ok(false);
    }
    let Some(result) = run.proposed_result else {
        return Ok(false);
    };
    if result.verification_status != VerificationStatus::Proposed
        || result.data_scopes != [BID_PACKAGE_REVIEW_SCOPE]
        || result.data_classification != DataClassification::TenderInternal
    {
        return Ok(false);
    }
    let candidate = match store
        .validate_bid_decision_package_review_candidate(&run.task, &result.payload_json)
    {
        Ok(candidate) => candidate,
        Err(_) => return Ok(false),
    };
    if candidate.outcome != review.outcome || candidate.findings != review.findings {
        return Ok(false);
    }
    check()?;
    let audit: Option<(u32, String, String)> = store
        .connection
        .query_row(
            "SELECT aggregate_revision, payload_json, created_at FROM audit_events
             WHERE event_type = 'bid_decision_package_reviewed'
               AND json_extract(payload_json, '$.change.review_id') = ?1",
            [&review.review_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let expected_change = json!({
        "finding_count": review.findings.len().to_string(),
        "outcome": review.outcome.as_str(),
        "package_id": package_id,
        "package_version": version.to_string(),
        "review_id": review.review_id,
        "reviewer_run_id": review.reviewer_run_id,
    });
    Ok(
        audit.is_some_and(|(aggregate_revision, payload_json, created_at)| {
            aggregate_revision == tender_revision
                && parse_canonical_json::<Value>(&payload_json)
                    .ok()
                    .and_then(|payload| payload.get("change").cloned())
                    .as_ref()
                    == Some(&expected_change)
                && created_at == review.created_at
        }),
    )
}

fn count_rows(
    connection: &rusqlite::Connection,
    table: &str,
    package_id: &str,
    version: u32,
) -> Result<u32, TenderCommandError> {
    if table != "bid_compliance_rows" {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    connection
        .query_row(
            "SELECT COUNT(*) FROM bid_compliance_rows
             WHERE package_id = ?1 AND package_version = ?2",
            params![package_id, version],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn verification_status_str(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::Proposed => "proposed",
        VerificationStatus::Verified => "verified",
        VerificationStatus::Rejected => "rejected",
        VerificationStatus::Stale => "stale",
        VerificationStatus::Superseded => "superseded",
    }
}

fn parse_verification_status(value: &str) -> Result<VerificationStatus, TenderCommandError> {
    match value {
        "proposed" => Ok(VerificationStatus::Proposed),
        "verified" => Ok(VerificationStatus::Verified),
        "rejected" => Ok(VerificationStatus::Rejected),
        "stale" => Ok(VerificationStatus::Stale),
        "superseded" => Ok(VerificationStatus::Superseded),
        _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
    }
}

fn trust_class_str(trust: TenderRecordTrustClass) -> &'static str {
    match trust {
        TenderRecordTrustClass::AiProposal => "ai_proposal",
        TenderRecordTrustClass::DeterministicFact => "deterministic_fact",
        TenderRecordTrustClass::Verified => "verified",
        TenderRecordTrustClass::EngineerVerified => "engineer_verified",
        TenderRecordTrustClass::ApprovedAssumption => "approved_assumption",
        TenderRecordTrustClass::UnresolvedGap => "unresolved_gap",
        TenderRecordTrustClass::PriorDecision => "prior_decision",
    }
}

fn parse_trust_class(value: &str) -> Result<TenderRecordTrustClass, TenderCommandError> {
    match value {
        "ai_proposal" => Ok(TenderRecordTrustClass::AiProposal),
        "deterministic_fact" => Ok(TenderRecordTrustClass::DeterministicFact),
        "verified" => Ok(TenderRecordTrustClass::Verified),
        "engineer_verified" => Ok(TenderRecordTrustClass::EngineerVerified),
        "approved_assumption" => Ok(TenderRecordTrustClass::ApprovedAssumption),
        "unresolved_gap" => Ok(TenderRecordTrustClass::UnresolvedGap),
        "prior_decision" => Ok(TenderRecordTrustClass::PriorDecision),
        _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
    }
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn parse_canonical_json<T: serde::de::DeserializeOwned + Serialize>(
    value: &str,
) -> Result<T, TenderCommandError> {
    let parsed = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn bid_package_review_permissions() -> AgentRunPermissions {
    AgentRunPermissions {
        data_scopes: vec![BID_PACKAGE_REVIEW_SCOPE.into()],
        data_classifications: vec![DataClassification::TenderInternal],
        allowed_actions: vec![BID_PACKAGE_REVIEW_ACTION.into()],
        allowed_tools: Vec::new(),
        network_allowed: false,
        workspace_write_allowed: true,
    }
}

fn bid_package_review_budget() -> AgentResourceBudget {
    #[cfg(feature = "runtime-fixture")]
    let duration_seconds = 8;
    #[cfg(not(feature = "runtime-fixture"))]
    let duration_seconds = 120;
    AgentResourceBudget {
        provider_turns: 1,
        duration_seconds,
        output_bytes: 64 * 1024,
    }
}

fn bid_package_review_output_contract() -> String {
    serde_json_canonicalizer::to_string(&json!({
        "additionalProperties": false,
        "properties": {
            "findings": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "affected_records": {
                            "items": {
                                "additionalProperties": false,
                                "properties": {
                                    "record_id": { "minLength": 32, "maxLength": 32, "type": "string" },
                                    "version": { "minimum": 1, "type": "integer" }
                                },
                                "required": ["record_id", "version"],
                                "type": "object"
                            },
                            "maxItems": 64,
                            "type": "array"
                        },
                        "code": { "minLength": 1, "maxLength": 100, "type": "string" },
                        "severity": { "enum": ["critical", "major", "minor"] },
                        "summary": { "minLength": 1, "maxLength": 2000, "type": "string" }
                    },
                    "required": ["severity", "code", "summary", "affected_records"],
                    "type": "object"
                },
                "maxItems": MAX_REVIEW_FINDINGS,
                "type": "array"
            },
            "outcome": { "enum": ["passed", "failed"] }
        },
        "required": ["outcome", "findings"],
        "type": "object"
    }))
    .expect("static Bid Decision Package review output contract is canonical JSON")
}

fn bid_package_review_profile(profile_id: String) -> AgentProfileVersionView {
    AgentProfileVersionView {
        profile_id,
        version: 3,
        identity: "Independent Reviewer".into(),
        profession: "Tender Assurance Engineer".into(),
        seniority: "Senior".into(),
        capabilities: vec![BID_PACKAGE_REVIEW_CAPABILITY.into()],
        objective: "Independently review one exact Bid Decision Package and its Evidence.".into(),
        behavior: "Review without editing the package, resolving findings, or making the formal decision.".into(),
        skepticism: "Challenge blockers, dispositions, capability coverage, assumptions, and recommendation logic.".into(),
        risk_tolerance: "Very low tolerance for unsupported Proceed or Decline recommendations.".into(),
        instructions: "Independently review the exact immutable Bid Decision Package version. Check complete dispositions, exact trust/evidence bindings, Project Fingerprint trust, capability coverage, gaps, risks, assumptions, unresolved queries, resource implications, and recommendation. Record findings without editing the package or its analyzed Tender Records.".into(),
        output_contract_json: bid_package_review_output_contract(),
        review_policy: "A pass may contain only disclosed Minor findings. Critical or Major findings require a failed review and a new corrected package version; the reviewer cannot approve the Bid Decision.".into(),
        permissions: bid_package_review_permissions(),
        prohibited_actions: vec![
            "approve_tender_decision".into(),
            "mutate_tender_store_directly".into(),
            "perform_external_action".into(),
            "access_secret_data".into(),
        ],
        resource_budget: bid_package_review_budget(),
    }
}

fn bid_package_review_task(
    task_id: String,
    tender_id: &str,
    tender_revision: u32,
    package_id: &str,
    package_version: u32,
    deadline: String,
    profile: &AgentProfileVersionView,
) -> TenderTaskView {
    TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: "Independently review this exact Bid Decision Package version and record attributable findings without changing the package or any analyzed Tender Record.".into(),
        exact_inputs: vec![
            AgentTaskInputReference {
                kind: "tender_revision".into(),
                reference: tender_id.into(),
                version: tender_revision,
            },
            AgentTaskInputReference {
                kind: "bid_decision_package".into(),
                reference: package_id.into(),
                version: package_version,
            },
        ],
        output_contract_json: profile.output_contract_json.clone(),
        review_policy: profile.review_policy.clone(),
        deadline,
        permissions: bid_package_review_permissions(),
        resource_budget: profile.resource_budget.clone(),
    }
}

impl TenderStore {
    pub(crate) fn prepare_bid_decision_package_review_run(
        &mut self,
        tender_id: &TenderId,
        package_id: &str,
        version: u32,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        self.require_pre_bid_writable()?;
        if !valid_identifier(package_id) || version == 0 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let package = self.inspect_bid_decision_package(package_id, version)?;
        if !package.current || package.review.is_some() || package.approval.is_some() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let payload = bid_package_review_data_view(self, package_id, version)?;
        let run_id = random_identifier(&self.connection)?;
        let application_home = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let workspace = application_home
            .join("staging")
            .join(format!("agent-{}-{run_id}", tender_id.as_str()));
        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let current_version: u32 = transaction
                .query_row(
                    "SELECT current_version FROM bid_decision_package_heads WHERE package_id = ?1",
                    [package_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let has_review: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM bid_decision_package_reviews
                       WHERE package_id = ?1 AND package_version = ?2)",
                    params![package_id, version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let has_approval: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM bid_decision_approval_records
                       WHERE package_id = ?1 AND package_version = ?2)",
                    params![package_id, version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let has_unresolved_indeterminate: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs
                       WHERE status = 'indeterminate'
                         AND NOT EXISTS (
                           SELECT 1 FROM agent_run_recovery_dispositions
                           WHERE agent_run_recovery_dispositions.run_id = agent_runs.run_id
                         )
                     )",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if current_version != version
                || has_review
                || has_approval
                || has_unresolved_indeterminate
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let tender_revision: u32 = transaction
                .query_row(
                    "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                    [tender_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if tender_revision != package.tender_revision {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let created_at = sqlite_timestamp(&transaction)?;
            let profile_id: String = transaction
                .query_row(
                    "SELECT profile_id FROM agent_profiles WHERE stable_identity = ?1",
                    [BootstrapRole::IndependentReviewer.stable_identity()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let profile = bid_package_review_profile(profile_id.clone());
            let profile_exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM agent_profile_versions
                     WHERE profile_id = ?1 AND version = ?2)",
                    params![profile_id, profile.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if profile_exists {
                if load_profile(&transaction, (profile_id, profile.version))? != profile {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
            } else {
                insert_profile_version(&transaction, &profile, &created_at)?;
            }
            update_profile_head(
                &transaction,
                &profile.profile_id,
                profile.version,
                crate::agent_runtime::AgentProfileStatus::Active,
            )?;
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = bid_package_review_task(
                random_identifier(&transaction)?,
                tender_id.as_str(),
                tender_revision,
                package_id,
                version,
                deadline,
                &profile,
            );
            insert_task(&transaction, &task, &created_at)?;
            let (permission_grant, materialized_workspace) =
                derive_pre_bid_data_grant(PreBidDataGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home,
                    tender_id: tender_id.as_str(),
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                    data_scope: BID_PACKAGE_REVIEW_SCOPE,
                    allowed_action: BID_PACKAGE_REVIEW_ACTION,
                    relative_path: "bid-decision-package-review-v1.json",
                    view_id: "bid-decision-package-review-v1",
                    payload: &payload,
                })?;
            if materialized_workspace != workspace
                || permission_duration(&permission_grant, jiff::Timestamp::now())
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
                    .is_zero()
            {
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
                Some((thread_ref, status)) if status == "archive_pending" => {
                    (None, Some(thread_ref))
                }
                Some((thread_ref, status)) if status == "active" => {
                    let exposure = load_thread_exposure(&transaction, &thread_ref)?;
                    if exposure.is_compatible_with(&permission_grant) {
                        (Some(thread_ref), None)
                    } else {
                        if transaction
                            .execute(
                                "UPDATE provider_threads SET status = 'archive_pending'
                                 WHERE thread_ref = ?1 AND status = 'active'",
                                [&thread_ref],
                            )
                            .map_err(sql_error)?
                            != 1
                        {
                            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                        }
                        append_audit_event(
                            &transaction,
                            tender_id.as_str(),
                            "provider_thread_archive_requested",
                            tender_revision,
                            json!({
                                "reason": "thread_exposure_incompatible",
                                "run_id": run_id,
                                "thread_ref": thread_ref,
                            }),
                            &created_at,
                        )?;
                        (None, Some(thread_ref))
                    }
                }
                Some(_) => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
                None => (None, None),
            };
            ensure_agent_run_capacity(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO agent_runs (
                       run_id, task_id, profile_id, profile_version,
                       permission_grant_json, status, started_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
                    params![
                        run_id,
                        task.task_id,
                        profile.profile_id,
                        profile.version,
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
                    summary: "Independent Bid Decision Package review started".into(),
                    correlation_id: None,
                    request_fingerprint: None,
                    denial_reason: None,
                    opaque_reference: None,
                },
                &created_at,
            )?;
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "bid_decision_package_review_started",
                tender_revision,
                json!({
                    "package_id": package_id,
                    "package_version": version.to_string(),
                    "reviewer_profile_id": profile.profile_id,
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
                permission_grant,
                provider_thread_ref,
                provider_thread_to_archive,
                workspace: workspace.clone(),
            })
        })();
        if prepared.is_err() {
            let _ = fs::remove_dir_all(&workspace);
        }
        prepared
    }

    pub(crate) fn validate_bid_decision_package_review_candidate(
        &self,
        task: &TenderTaskView,
        payload: &str,
    ) -> Result<BidDecisionPackageReviewCandidate, TenderCommandError> {
        if payload.len() > 64 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let candidate: BidDecisionPackageReviewCandidate = serde_json::from_str(payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let (package_id, version) = exact_package_target(task)?;
        let bound_records = package_bound_records(&self.connection, package_id, version)?;
        if candidate.findings.len() > MAX_REVIEW_FINDINGS
            || (candidate.outcome == BidDecisionPackageReviewOutcome::Failed
                && candidate.findings.is_empty())
            || (candidate.outcome == BidDecisionPackageReviewOutcome::Passed
                && candidate.findings.iter().any(|finding| {
                    matches!(
                        finding.severity,
                        ReviewFindingSeverity::Critical | ReviewFindingSeverity::Major
                    )
                }))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut finding_codes = HashSet::new();
        for finding in &candidate.findings {
            if finding.code.is_empty()
                || finding.code.len() > 100
                || !valid_work_key(&finding.code)
                || !finding_codes.insert(finding.code.clone())
                || finding.summary.trim().is_empty()
                || finding.summary.len() > 2_000
                || finding.affected_records.len() > 64
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let affected = finding.affected_records.iter().collect::<HashSet<_>>();
            if affected.len() != finding.affected_records.len()
                || finding.affected_records.iter().any(|reference| {
                    !bound_records.contains(&(reference.record_id.clone(), reference.version))
                })
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        Ok(candidate)
    }

    pub(crate) fn bid_decision_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let package_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM bid_decision_packages", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        let head_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM bid_decision_package_heads",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if package_count > 1 || head_count != package_count {
            return Ok(false);
        }
        let version_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM bid_decision_package_versions",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if usize::try_from(version_count)
            .ok()
            .is_none_or(|count| count > MAX_PACKAGE_VERSIONS)
        {
            return Ok(false);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT package_id, version FROM bid_decision_package_versions
                 ORDER BY package_id, version",
            )
            .map_err(sql_error)?;
        let mapped = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?;
        let mut versions = Vec::with_capacity(version_count as usize);
        for row in mapped {
            check()?;
            if versions.len() >= MAX_PACKAGE_VERSIONS {
                return Ok(false);
            }
            versions.push(row.map_err(sql_error)?);
        }
        drop(statement);
        if versions.len() != version_count as usize {
            return Ok(false);
        }
        if package_count == 0 {
            if !versions.is_empty() {
                return Ok(false);
            }
        } else {
            let (head_package_id, head_version): (String, u32) = self
                .connection
                .query_row(
                    "SELECT package_id, current_version FROM bid_decision_package_heads",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(sql_error)?;
            if versions.is_empty()
                || versions
                    .iter()
                    .enumerate()
                    .any(|(index, (package_id, version))| {
                        package_id != &head_package_id || *version != (index + 1) as u32
                    })
                || head_version != version_count
            {
                return Ok(false);
            }
        }
        let mut previous_dependency_snapshot: Option<(u32, Vec<RecordInventoryObservation>)> = None;
        for (package_id, version) in versions {
            check()?;
            let (
                tender_revision,
                record_inventory_json,
                record_inventory_sha256,
                capabilities_json,
                resource_implications_json,
                recommendation_json,
                blocker_count,
                manifest_json,
                manifest_sha256,
            ): (
                u32,
                String,
                String,
                String,
                String,
                String,
                u32,
                String,
                String,
            ) = self
                .connection
                .query_row(
                    "SELECT tender_revision, record_inventory_json, record_inventory_sha256,
                            capability_demands_json, resource_implications_json,
                            recommendation_json, analysis_blocker_count,
                            manifest_json, manifest_sha256
                     FROM bid_decision_package_versions
                     WHERE package_id = ?1 AND version = ?2",
                    params![package_id, version],
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
                        ))
                    },
                )
                .map_err(sql_error)?;
            if manifest_json.len() > MAX_PACKAGE_MANIFEST_BYTES
                || record_inventory_json.len() > MAX_PACKAGE_MANIFEST_BYTES
                || capabilities_json.len() > MAX_PACKAGE_MANIFEST_BYTES
                || resource_implications_json.len() > MAX_PACKAGE_MANIFEST_BYTES
                || recommendation_json.len() > MAX_PACKAGE_MANIFEST_BYTES
                || !valid_sha256(&record_inventory_sha256)
                || !valid_sha256(&manifest_sha256)
            {
                return Ok(false);
            }
            let record_inventory: Vec<RecordInventoryObservation> =
                parse_canonical_json(&record_inventory_json)?;
            if record_inventory.len() > MAX_COMPLIANCE_ROWS
                || sha256_hex(record_inventory_json.as_bytes()) != record_inventory_sha256
            {
                return Ok(false);
            }
            let capabilities: Vec<CapabilityDemand> = parse_canonical_json(&capabilities_json)?;
            let resource_implications: Vec<ResourceImplication> =
                parse_canonical_json(&resource_implications_json)?;
            let recommendation: BidRecommendation = parse_canonical_json(&recommendation_json)?;
            if capabilities.len() > MAX_CAPABILITY_DEMANDS
                || resource_implications.len() > MAX_COMPLIANCE_ROWS
            {
                return Ok(false);
            }
            let rows = load_stored_rows_with_check(&self.connection, &package_id, version, check)?;
            let bindings =
                load_stored_bindings_with_check(&self.connection, &package_id, version, check)?;
            let return_rework_basis =
                return_rework_basis_for_package(&self.connection, &package_id, version)?;
            let material_change_basis =
                material_change_basis_for_package(&self.connection, &package_id, version)?;
            if let Some(basis) = &material_change_basis {
                let Some((prior_revision, prior_inventory)) = &previous_dependency_snapshot else {
                    return Ok(false);
                };
                let changed_records = changed_record_references(prior_inventory, &record_inventory);
                if changed_records.is_empty()
                    || changed_records != basis.changed_records
                    || (*prior_revision == tender_revision
                        && canonical_json(prior_inventory)? == record_inventory_json)
                {
                    return Ok(false);
                }
            }
            if rows.len() > MAX_COMPLIANCE_ROWS {
                return Ok(false);
            }
            let expected = package_manifest_value_with_check(
                &PackageManifest {
                    package_id: &package_id,
                    version,
                    tender_revision,
                    rows: &rows,
                    bindings: &bindings,
                    capability_demands: &capabilities,
                    resource_implications: &resource_implications,
                    recommendation: &recommendation,
                    analysis_blocker_count: usize::try_from(blocker_count)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    record_inventory_sha256: &record_inventory_sha256,
                    return_rework_basis: return_rework_basis.as_ref(),
                    material_change_basis: material_change_basis.as_ref(),
                },
                check,
            )?;
            check()?;
            let expected_json = canonical_json(&expected)?;
            if expected_json != manifest_json
                || sha256_hex(expected_json.as_bytes()) != manifest_sha256
            {
                return Ok(false);
            }
            if let Some(review) = load_package_review(&self.connection, &package_id, version)? {
                if review.findings.len() > MAX_REVIEW_FINDINGS
                    || review.outcome == BidDecisionPackageReviewOutcome::Failed
                        && review.findings.is_empty()
                    || !package_review_is_attributable(
                        self,
                        &package_id,
                        version,
                        tender_revision,
                        &review,
                        check,
                    )?
                {
                    return Ok(false);
                }
            }
            previous_dependency_snapshot = Some((tender_revision, record_inventory));
        }
        if !self.bid_decision_approvals_are_valid_with_check(package_count, check)? {
            return Ok(false);
        }
        check()?;
        Ok(true)
    }

    fn bid_decision_approvals_are_valid_with_check(
        &self,
        package_count: u32,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let approval_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM bid_decision_approval_records",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if usize::try_from(approval_count)
            .ok()
            .is_none_or(|count| count > MAX_APPROVAL_RECORDS)
        {
            return Ok(false);
        }
        let lifecycle_phase = TenderLifecyclePhase::parse(
            &self
                .connection
                .query_row(
                    "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sql_error)?,
        )?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT package_id, package_version FROM bid_decision_approval_records
                 ORDER BY approval_sequence",
            )
            .map_err(sql_error)?;
        let mapped = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?;
        let mut approvals = Vec::with_capacity(approval_count as usize);
        for key in mapped {
            check()?;
            if approvals.len() >= MAX_PACKAGE_VERSIONS {
                return Ok(false);
            }
            let (package_id, version) = key.map_err(sql_error)?;
            let Some(approval) = load_package_approval(&self.connection, &package_id, version)?
            else {
                return Ok(false);
            };
            approvals.push(approval);
        }
        drop(statement);
        if approvals.len() != approval_count as usize {
            return Ok(false);
        }
        let mut expected_hash = ZERO_APPROVAL_HASH.to_owned();
        let mut previous_version = 0;
        for (index, approval) in approvals.iter().enumerate() {
            check()?;
            if approval.approval_sequence != (index + 1) as u32
                || approval.preceding_approval_hash != expected_hash
                || approval.package_version <= previous_version
                || index + 1 < approvals.len()
                    && approval.decision != BidDecisionApprovalDecision::Return
                    && !(approval.decision == BidDecisionApprovalDecision::Accept
                        && approval.invalidation.is_some())
            {
                return Ok(false);
            }
            let package: Option<(u32, String, u32)> = self
                .connection
                .query_row(
                    "SELECT tender_revision, manifest_sha256, analysis_blocker_count
                     FROM bid_decision_package_versions
                     WHERE package_id = ?1 AND version = ?2",
                    params![approval.package_id, approval.package_version],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let Some((tender_revision, package_sha256, blocker_count)) = package else {
                return Ok(false);
            };
            if tender_revision != approval.tender_revision
                || package_sha256 != approval.package_manifest_sha256
                || matches!(
                    approval.decision,
                    BidDecisionApprovalDecision::Accept | BidDecisionApprovalDecision::Reject
                ) && (blocker_count != 0
                    || !self
                        .connection
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM bid_decision_package_reviews
                         WHERE package_id = ?1 AND package_version = ?2 AND outcome = 'passed')",
                            params![approval.package_id, approval.package_version],
                            |row| row.get::<_, bool>(0),
                        )
                        .map_err(sql_error)?)
            {
                return Ok(false);
            }
            let return_rework =
                load_return_rework_disposition(&self.connection, &approval.approval_id)?;
            if return_rework.is_some() && approval.decision != BidDecisionApprovalDecision::Return {
                return Ok(false);
            }
            if let Some(disposition) = return_rework {
                if disposition.approval_sha256 != approval.approval_sha256
                    || disposition.items.len() != approval.required_rework.len()
                    || disposition
                        .items
                        .iter()
                        .zip(&approval.required_rework)
                        .any(|(item, required)| &item.required_rework != required)
                {
                    return Ok(false);
                }
                let rework_audit_count: u32 = self
                    .connection
                    .query_row(
                        "SELECT COUNT(*) FROM audit_events
                         WHERE event_type = 'bid_decision_return_rework_resolved'
                           AND created_at = ?1
                           AND json_extract(payload_json, '$.change.approval_id') = ?2
                           AND json_extract(payload_json, '$.change.disposition_id') = ?3
                           AND json_extract(payload_json, '$.change.manifest_sha256') = ?4",
                        params![
                            disposition.created_at,
                            approval.approval_id,
                            disposition.disposition_id,
                            disposition.manifest_sha256,
                        ],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if rework_audit_count != 1 {
                    return Ok(false);
                }
            }
            if approval.invalidation.is_some()
                && approval.decision != BidDecisionApprovalDecision::Accept
            {
                return Ok(false);
            }
            if let Some(invalidation) = &approval.invalidation {
                if invalidation.approval_sha256 != approval.approval_sha256 {
                    return Ok(false);
                }
                let invalidation_audit_count: u32 = self
                    .connection
                    .query_row(
                        "SELECT COUNT(*) FROM audit_events
                         WHERE event_type = 'bid_decision_approval_invalidated'
                           AND created_at = ?1
                           AND json_extract(payload_json, '$.change.approval_id') = ?2
                           AND json_extract(payload_json, '$.change.approval_sha256') = ?3
                           AND json_extract(payload_json, '$.change.invalidation_id') = ?4
                           AND json_extract(payload_json, '$.change.manifest_sha256') = ?5",
                        params![
                            invalidation.created_at,
                            approval.approval_id,
                            approval.approval_sha256,
                            invalidation.invalidation_id,
                            invalidation.manifest_sha256,
                        ],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if invalidation_audit_count != 1 {
                    return Ok(false);
                }
            }
            let audit_count: u32 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events
                     WHERE event_type = 'bid_decision_approval_recorded'
                       AND aggregate_revision = ?1 AND created_at = ?2
                       AND json_extract(payload_json, '$.change.approval_id') = ?3
                       AND json_extract(payload_json, '$.change.approval_sha256') = ?4
                       AND json_extract(payload_json, '$.change.decision') = ?5
                       AND json_extract(payload_json, '$.change.package_id') = ?6
                       AND json_extract(payload_json, '$.change.package_manifest_sha256') = ?7
                       AND json_extract(payload_json, '$.change.package_version') = ?8",
                    params![
                        approval.tender_revision,
                        approval.created_at,
                        approval.approval_id,
                        approval.approval_sha256,
                        approval.decision.as_str(),
                        approval.package_id,
                        approval.package_manifest_sha256,
                        approval.package_version.to_string(),
                    ],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if audit_count != 1 {
                return Ok(false);
            }
            expected_hash.clone_from(&approval.approval_sha256);
            previous_version = approval.package_version;
        }
        let expected_lifecycle = approvals.last().map_or_else(
            || {
                if package_count == 0 {
                    TenderLifecyclePhase::Intake
                } else {
                    TenderLifecyclePhase::BidDecision
                }
            },
            |approval| {
                if approval.invalidation.is_some() {
                    TenderLifecyclePhase::BidDecision
                } else {
                    approval.lifecycle_after
                }
            },
        );
        if let Some(terminal) = approvals.last().filter(|approval| {
            approval.invalidation.is_none()
                && matches!(
                    approval.decision,
                    BidDecisionApprovalDecision::Accept | BidDecisionApprovalDecision::Reject
                )
        }) {
            let head: Option<(String, u32)> = self
                .connection
                .query_row(
                    "SELECT package_id, current_version FROM bid_decision_package_heads",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            if head != Some((terminal.package_id.clone(), terminal.package_version)) {
                return Ok(false);
            }
        }
        if lifecycle_phase != expected_lifecycle {
            return Ok(false);
        }
        Ok(true)
    }
}

pub(crate) fn bid_decision_package_review_target_is_open(
    transaction: &rusqlite::Transaction<'_>,
    task: &TenderTaskView,
) -> Result<bool, TenderCommandError> {
    let (package_id, version) = exact_package_target(task)?;
    let tender_revision: Option<u32> = transaction
        .query_row(
            "SELECT tender_revision
             FROM bid_decision_package_versions
             WHERE package_id = ?1 AND version = ?2",
            params![package_id, version],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(tender_revision) = tender_revision else {
        return Ok(false);
    };
    let current_tender_revision: u32 = transaction
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if current_tender_revision != tender_revision {
        return Ok(false);
    }
    transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM bid_decision_package_heads
               WHERE package_id = ?1 AND current_version = ?2
             ) AND NOT EXISTS(
               SELECT 1 FROM bid_decision_package_reviews
               WHERE package_id = ?1 AND package_version = ?2
             ) AND NOT EXISTS(
               SELECT 1 FROM bid_decision_approval_records
               WHERE package_id = ?1 AND package_version = ?2
             )",
            params![package_id, version],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

pub(crate) fn bid_decision_package_review_target_is_current(
    store: &TenderStore,
    task: &TenderTaskView,
) -> Result<bool, TenderCommandError> {
    let (package_id, version) = exact_package_target(task)?;
    let package = store.inspect_bid_decision_package(package_id, version)?;
    Ok(package.current && package.review.is_none() && package.approval.is_none())
}

pub(crate) fn publish_bid_decision_package_review(
    transaction: &rusqlite::Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    run_id: &str,
    task: &TenderTaskView,
    candidate: &BidDecisionPackageReviewCandidate,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let (package_id, version) = exact_package_target(task)?;
    if !bid_decision_package_review_target_is_open(transaction, task)? {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let review_id = random_identifier(transaction)?;
    transaction
        .execute(
            "INSERT INTO bid_decision_package_reviews (
               review_id, package_id, package_version, reviewer_run_id,
               outcome, findings_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                review_id,
                package_id,
                version,
                run_id,
                candidate.outcome.as_str(),
                canonical_json(&candidate.findings)?,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "bid_decision_package_reviewed",
        tender_revision,
        json!({
            "finding_count": candidate.findings.len().to_string(),
            "outcome": candidate.outcome.as_str(),
            "package_id": package_id,
            "package_version": version.to_string(),
            "review_id": review_id,
            "reviewer_run_id": run_id,
        }),
        created_at,
    )
}

fn exact_package_target(task: &TenderTaskView) -> Result<(&str, u32), TenderCommandError> {
    let mut targets = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "bid_decision_package");
    let target = targets
        .next()
        .filter(|input| valid_identifier(&input.reference) && input.version > 0)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if targets.next().is_some() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((&target.reference, target.version))
}

fn bid_package_review_data_view(
    store: &TenderStore,
    package_id: &str,
    version: u32,
) -> Result<Value, TenderCommandError> {
    let (manifest_json, manifest_sha256): (String, String) = store
        .connection
        .query_row(
            "SELECT manifest_json, manifest_sha256 FROM bid_decision_package_versions
             WHERE package_id = ?1 AND version = ?2",
            params![package_id, version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    let manifest: Value = parse_canonical_json(&manifest_json)?;
    let mut bound = package_bound_records(&store.connection, package_id, version)?
        .into_iter()
        .collect::<Vec<_>>();
    bound.sort();
    if bound.len() > MAX_COMPLIANCE_ROWS {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let mut records = Vec::with_capacity(bound.len());
    for (record_id, record_version) in bound {
        let record = store.inspect_tender_record_version(&record_id, record_version)?;
        records.push(record_evidence_basis(&record));
    }
    let payload = bid_package_review_data_view_value(&manifest, &manifest_sha256, records);
    if canonical_json(&payload)?.len() > MAX_REVIEW_DATA_VIEW_BYTES {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(payload)
}

fn bid_package_review_data_view_value(
    manifest: &Value,
    manifest_sha256: &str,
    records: Vec<Value>,
) -> Value {
    json!({
        "data_classification": DataClassification::TenderInternal,
        "data_scope": BID_PACKAGE_REVIEW_SCOPE,
        "manifest": manifest,
        "manifest_sha256": manifest_sha256,
        "records": records,
        "schema_version": 1,
    })
}

fn record_evidence_basis(record: &TenderRecordInspection) -> Value {
    json!({
        "contradictions": record.contradictions,
        "fields": record.fields,
        "kind": record.kind,
        "record_id": record.record_id,
        "source_relationships": record.source_relationships,
        "stable_key": record.stable_key,
        "title": record.title,
        "trust_class": record.trust_class,
        "verification_status": record.verification_status,
        "version": record.version,
    })
}

fn package_bound_records(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
) -> Result<HashSet<(String, u32)>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT record_id, record_version FROM bid_compliance_rows
             WHERE package_id = ?1 AND package_version = ?2
             UNION
             SELECT record_id, record_version FROM bid_decision_package_record_bindings
             WHERE package_id = ?1 AND package_version = ?2",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![package_id, version], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    Ok(rows.into_iter().collect())
}

fn load_stored_bindings_with_check(
    connection: &rusqlite::Connection,
    package_id: &str,
    version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<StoredBinding>, TenderCommandError> {
    let binding_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM bid_decision_package_record_bindings
             WHERE package_id = ?1 AND package_version = ?2",
            params![package_id, version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if binding_count > MAX_COMPLIANCE_ROWS as u32 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let mut statement = connection
        .prepare(
            "SELECT category, ordinal, record_id, record_version
             FROM bid_decision_package_record_bindings
             WHERE package_id = ?1 AND package_version = ?2
             ORDER BY category, ordinal",
        )
        .map_err(sql_error)?;
    let mut rows = statement
        .query(params![package_id, version])
        .map_err(sql_error)?;
    let mut expected_ordinals = HashMap::new();
    let mut bindings = Vec::with_capacity(binding_count as usize);
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        if bindings.len() >= MAX_COMPLIANCE_ROWS {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let category = row.get::<_, String>(0).map_err(sql_error)?;
        let ordinal = row.get::<_, u32>(1).map_err(sql_error)?;
        let record_id = row.get::<_, String>(2).map_err(sql_error)?;
        let record_version = row.get::<_, u32>(3).map_err(sql_error)?;
        let category = BidDecisionPackageRecordCategory::parse(&category)?;
        let expected = expected_ordinals
            .entry(category)
            .and_modify(|value: &mut u32| *value += 1)
            .or_insert(1);
        if ordinal != *expected {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        bindings.push(StoredBinding {
            category,
            record: TenderRecordVersionReference {
                record_id,
                version: record_version,
            },
        });
    }
    if bindings.len() != binding_count as usize {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(bindings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_change_lineage_accepts_a_bounded_diff_larger_than_human_text_lists() {
        let current = (0..33)
            .map(|index| RecordInventoryObservation {
                record: TenderRecordVersionReference {
                    record_id: format!("{index:032x}"),
                    version: 1,
                },
                kind: TenderRecordKind::Risk,
                verification_status: VerificationStatus::Proposed,
                trust_class: TenderRecordTrustClass::AiProposal,
            })
            .collect::<Vec<_>>();
        let changed = changed_record_references(&[], &current);
        assert_eq!(changed.len(), 33);
        assert!(valid_material_change_record_count(&changed));
    }
}
