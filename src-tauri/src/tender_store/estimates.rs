use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
    str::FromStr,
};

use garde::Validate;
use jiff::{civil::Date, Timestamp};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{
    de::{Error as _, SeqAccess},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::agent_runtime::{
    permissions::{
        derive_planned_task_grant, permission_duration, DataClassification, PermissionGrant,
        PlannedTaskGrantRequest, ThreadExposureSet,
    },
    AgentProfileVersionView, AgentRunInspection, AgentTaskInputReference, PendingProviderEvent,
    PreparedAgentRun, ProviderEventKind, TenderTaskView,
};

use super::{
    agent_records::{
        ensure_agent_run_capacity, insert_event, insert_task, load_profile, load_task,
        load_thread_exposure,
    },
    append_audit_event_with_sequence,
    calculations::{
        approve_estimate_aggregate_calculation, approved_calculation_run_for_estimate,
        calculation_evidence_view_for_estimate, evaluate_estimate_aggregate,
        load_estimate_aggregate_calculation, record_estimate_aggregate_calculation,
        valid_estimate_currency, ApproveEstimateAggregateCalculation, ControlledBoqCalculationRun,
        ControlledBoqCalculationStatus, EstimateAggregateCalculationInput,
        EstimateAggregateCalculationRun, RecordEstimateAggregateCalculation,
        CALCULATION_RULE_REVIEW_CAPABILITY, COST_ESTIMATION_CAPABILITY,
    },
    lock_mutex_with_check, random_identifier, sha256_hex, sql_error, sqlite_timestamp,
    tender_queries::{load_query_decision, ApprovedQueryTreatment, TenderQueryTreatment},
    tender_records::TenderEvidenceReference,
    valid_identifier, BidPackageOperationBudget, QuantixHost, TenderCommandError, TenderErrorCode,
    TenderId, TenderStore, WorkPlanProfileBinding, WorkPlanTask,
};

const MAX_BASIS_VERSIONS: u32 = 32;
const MAX_BOQ_TABLES: u32 = 64;
const MAX_BOQ_ROWS: usize = 256;
const MAX_CBS_COMPONENTS: usize = 256;
const MAX_QUOTES: usize = 64;
const MAX_QUERIES: usize = 256;
const MAX_CALCULATION_RUNS: usize = 256;
const MAX_LIST_ITEMS: usize = 128;
const MAX_FINDINGS: usize = 64;
const MAX_BASIS_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 4_000;
const BOQ_CANDIDATE_PAGE_SIZE: usize = 16;
const BASIS_REVIEW_CAPABILITY: &str = CALCULATION_RULE_REVIEW_CAPABILITY;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BoqRowDisposition {
    Priced,
    Excluded,
    Provisional,
    Duplicated,
    Missing,
    NotApplicable,
    Blocked,
}

impl BoqRowDisposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Priced => "priced",
            Self::Excluded => "excluded",
            Self::Provisional => "provisional",
            Self::Duplicated => "duplicated",
            Self::Missing => "missing",
            Self::NotApplicable => "not_applicable",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CostComponentCategory {
    Labor,
    Plant,
    Material,
    Subcontract,
    Indirect,
    TemporaryWorks,
    Overhead,
    Risk,
    OtherApproved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EstimateQuotationKind {
    Supplier,
    Subcontractor,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateQueryReference {
    #[garde(length(bytes, min = 32, max = 32))]
    pub query_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateQueryObservation {
    pub query_id: String,
    pub version: u32,
    pub treatment_decision_manifest_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BoqTableDesignation {
    pub designation_id: String,
    pub artifact_id: String,
    pub artifact_version: u32,
    pub table_number: u32,
    pub header_row_count: u32,
    pub row_count: u32,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BoqTableCandidate {
    pub artifact_id: String,
    pub artifact_version: u32,
    pub table_number: u32,
    pub document_name: String,
    pub row_count: u32,
    pub sample_text: String,
    pub designation: Option<BoqTableDesignation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BoqInventoryRow {
    pub row_key: String,
    pub evidence: Vec<TenderEvidenceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BoqAccountRow {
    pub row_key: String,
    pub description: String,
    pub disposition: BoqRowDisposition,
    pub evidence: Vec<TenderEvidenceReference>,
    pub calculation_run_id: Option<String>,
    pub affected_queries: Vec<EstimateQueryReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CostBreakdownComponent {
    pub component_id: String,
    pub cost_code: String,
    pub work_package: String,
    pub category: CostComponentCategory,
    pub description: String,
    pub boq_row_keys: Vec<String>,
    pub resource_build_up_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateResourceBuildUp {
    pub build_up_id: String,
    pub cbs_component_id: String,
    pub category: CostComponentCategory,
    pub description: String,
    pub calculation_run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateQuotation {
    pub quotation_id: String,
    pub kind: EstimateQuotationKind,
    pub counterparty: String,
    pub exact_scope: String,
    pub quotation_date: String,
    pub currency: String,
    pub exclusions: Vec<String>,
    pub valid_until: String,
    pub evidence: TenderEvidenceReference,
    pub normalization_calculation_run_id: String,
    pub covered_boq_row_keys: Vec<String>,
    pub comparison_assumptions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateAllowance {
    pub allowance_id: String,
    pub description: String,
    pub cbs_component_id: String,
    pub resource_build_up_id: String,
    pub query_id: String,
    pub query_version: u32,
    pub decision_id: String,
    pub evidence: Vec<TenderEvidenceReference>,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateMaterialAssumption {
    pub query_id: String,
    pub query_version: u32,
    pub decision_id: String,
    pub treatment: TenderQueryTreatment,
    pub rationale: String,
    pub treatment_details: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BasisOfEstimateReviewOutcome {
    Passed,
    Failed,
}

impl BasisOfEstimateReviewOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BasisOfEstimateReviewFinding {
    pub code: String,
    pub summary: String,
    pub affected_boq_row_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BasisOfEstimateReview {
    pub review_id: String,
    pub reviewer_run_id: String,
    pub reviewer_profile_id: String,
    pub reviewer_profile_version: u32,
    pub outcome: BasisOfEstimateReviewOutcome,
    pub findings: Vec<BasisOfEstimateReviewFinding>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BasisOfEstimateApproval {
    pub approval_id: String,
    pub basis_id: String,
    pub basis_version: u32,
    pub basis_manifest_sha256: String,
    pub review_id: String,
    pub rationale: String,
    pub approved_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BasisOfEstimateVersion {
    pub basis_id: String,
    pub version: u32,
    pub tender_revision: u32,
    pub author_run_id: String,
    pub author_profile_id: String,
    pub author_profile_version: u32,
    pub scope: String,
    pub pricing_date: String,
    pub currencies: Vec<String>,
    pub taxes: Vec<String>,
    pub rate_sources: Vec<String>,
    pub productivity: Vec<String>,
    pub design_maturity: String,
    pub gaps: Vec<String>,
    pub exclusions: Vec<String>,
    pub supersedes_basis_manifest_sha256: Option<String>,
    pub remediates_review_manifest_sha256: Option<String>,
    pub boq_inventory_sha256: String,
    pub query_inventory_sha256: String,
    pub query_inventory: Vec<EstimateQueryObservation>,
    pub boq_rows: Vec<BoqAccountRow>,
    pub cbs_components: Vec<CostBreakdownComponent>,
    pub resource_build_ups: Vec<EstimateResourceBuildUp>,
    pub quotations: Vec<EstimateQuotation>,
    pub allowances: Vec<EstimateAllowance>,
    pub material_assumptions: Vec<EstimateMaterialAssumption>,
    pub comparison_total_calculation_run_id: String,
    pub aggregate_calculation: EstimateAggregateCalculationRun,
    pub total_amount: String,
    pub total_currency: String,
    pub complete: bool,
    pub reconciled: bool,
    pub blockers: Vec<String>,
    pub current: bool,
    pub relied_upon: bool,
    pub review: Option<BasisOfEstimateReview>,
    pub approval: Option<BasisOfEstimateApproval>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateWorkspaceInspection {
    pub basis: Option<BasisOfEstimateVersion>,
    pub boq_table_candidates: Vec<BoqTableCandidate>,
    pub boq_table_candidate_next_cursor: Option<String>,
    pub basis_offset: u32,
    pub total_basis_version_count: u32,
    pub has_newer_basis: bool,
    pub has_older_basis: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DesignateBoqTableCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub artifact_id: String,
    #[garde(range(min = 1))]
    pub artifact_version: u32,
    #[garde(range(min = 1))]
    pub table_number: u32,
    #[garde(range(max = 8))]
    pub header_row_count: u32,
}

fn deserialize_bounded_string<'de, D, const MAX: usize>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor<const MAX: usize>;
    impl<const MAX: usize> serde::de::Visitor<'_> for Visitor<MAX> {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "a string no longer than {MAX} bytes")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            (value.len() <= MAX)
                .then(|| value.to_owned())
                .ok_or_else(|| E::custom("string exceeds the estimate command boundary"))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            (value.len() <= MAX)
                .then_some(value)
                .ok_or_else(|| E::custom("string exceeds the estimate command boundary"))
        }
    }
    deserializer.deserialize_string(Visitor::<MAX>)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundedEstimateEvidenceReference {
    #[serde(deserialize_with = "deserialize_bounded_string::<_, 32>")]
    artifact_id: String,
    version: u32,
    ordinal: u32,
}

#[derive(Deserialize)]
struct BoundedEstimateCalculationId(
    #[serde(deserialize_with = "deserialize_bounded_string::<_, 32>")] String,
);

fn deserialize_estimate_evidence<'de, D>(
    deserializer: D,
) -> Result<Vec<TenderEvidenceReference>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<TenderEvidenceReference>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 64 bounded quotation Evidence references")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_QUOTES));
            while let Some(value) = sequence.next_element::<BoundedEstimateEvidenceReference>()? {
                if values.len() == MAX_QUOTES {
                    return Err(A::Error::custom("too many quotation Evidence references"));
                }
                values.push(TenderEvidenceReference {
                    artifact_id: value.artifact_id,
                    version: value.version,
                    ordinal: value.ordinal,
                });
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

fn deserialize_estimate_calculation_ids<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 256 bounded Calculation Run identifiers")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values =
                Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_CALCULATION_RUNS));
            while let Some(value) = sequence.next_element::<BoundedEstimateCalculationId>()? {
                if values.len() == MAX_CALCULATION_RUNS {
                    return Err(A::Error::custom("invalid Calculation Run identifier list"));
                }
                values.push(value.0);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunCostEstimatorBasisCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[serde(deserialize_with = "deserialize_estimate_evidence")]
    #[garde(length(max = 64), dive)]
    pub quotation_evidence: Vec<TenderEvidenceReference>,
    #[serde(deserialize_with = "deserialize_estimate_calculation_ids")]
    #[garde(skip)]
    pub calculation_run_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CostEstimatorBasisResult {
    pub run: AgentRunInspection,
    pub basis: Option<BasisOfEstimateVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunBasisOfEstimateReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub basis_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct BasisOfEstimateReviewResult {
    pub run: AgentRunInspection,
    pub basis: BasisOfEstimateVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApproveBasisOfEstimateCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub basis_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectEstimateWorkspaceCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(range(max = 31))]
    pub basis_offset: u32,
    #[garde(length(bytes, max = 80))]
    pub boq_candidate_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BasisOfEstimateCandidate {
    pub scope: String,
    pub pricing_date: String,
    pub currencies: Vec<String>,
    pub taxes: Vec<String>,
    pub rate_sources: Vec<String>,
    pub productivity: Vec<String>,
    pub design_maturity: String,
    pub gaps: Vec<String>,
    pub exclusions: Vec<String>,
    pub boq_rows: Vec<BoqAccountRow>,
    pub cbs_components: Vec<CostBreakdownComponent>,
    pub resource_build_ups: Vec<EstimateResourceBuildUp>,
    pub quotations: Vec<EstimateQuotation>,
    pub allowances: Vec<EstimateAllowance>,
    pub material_assumptions: Vec<EstimateQueryReference>,
    pub comparison_total_calculation_run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BasisOfEstimateReviewCandidate {
    pub outcome: BasisOfEstimateReviewOutcome,
    pub findings: Vec<BasisOfEstimateReviewFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BasisOfEstimateManifest {
    schema_version: u32,
    basis_id: String,
    version: u32,
    tender_revision: u32,
    plan_id: String,
    plan_version: u32,
    author_run_id: String,
    author_profile_id: String,
    author_profile_version: u32,
    scope: String,
    pricing_date: String,
    currencies: Vec<String>,
    taxes: Vec<String>,
    rate_sources: Vec<String>,
    productivity: Vec<String>,
    design_maturity: String,
    gaps: Vec<String>,
    exclusions: Vec<String>,
    supersedes_basis_manifest_sha256: Option<String>,
    remediates_review_manifest_sha256: Option<String>,
    boq_inventory_sha256: String,
    query_inventory_sha256: String,
    query_inventory: Vec<EstimateQueryObservation>,
    boq_rows: Vec<BoqAccountRow>,
    cbs_components: Vec<CostBreakdownComponent>,
    resource_build_ups: Vec<EstimateResourceBuildUp>,
    quotations: Vec<EstimateQuotation>,
    allowances: Vec<EstimateAllowance>,
    material_assumptions: Vec<EstimateMaterialAssumption>,
    comparison_total_calculation_run_id: String,
    aggregate_calculation: EstimateAggregateCalculationRun,
    total_amount: String,
    total_currency: String,
    complete: bool,
    reconciled: bool,
    blockers: Vec<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BasisReviewManifest {
    schema_version: u32,
    review_id: String,
    basis_id: String,
    basis_version: u32,
    basis_manifest_sha256: String,
    reviewer_run_id: String,
    reviewer_profile_id: String,
    reviewer_profile_version: u32,
    outcome: BasisOfEstimateReviewOutcome,
    findings: Vec<BasisOfEstimateReviewFinding>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BasisApprovalManifest {
    schema_version: u32,
    approval_id: String,
    basis_id: String,
    basis_version: u32,
    basis_manifest_sha256: String,
    review_id: String,
    review_manifest_sha256: String,
    aggregate_calculation_run_id: String,
    aggregate_calculation_manifest_sha256: String,
    rationale: String,
    approved_by: String,
    acting_role: String,
    tender_revision: u32,
    created_at: String,
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn parse_canonical<T: serde::de::DeserializeOwned + Serialize>(
    value: &str,
) -> Result<T, TenderCommandError> {
    let parsed = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && value.trim() == value
}

fn valid_optional_list(values: &[String], max_items: usize) -> bool {
    if values.len() > max_items
        || values
            .iter()
            .any(|value| !valid_text(value, MAX_TEXT_BYTES))
    {
        return false;
    }
    let mut normalized = values.to_vec();
    normalized.sort();
    normalized.dedup();
    normalized.len() == values.len()
}

fn valid_iso_date(value: &str) -> bool {
    value.len() == 10 && Date::from_str(value).is_ok_and(|date| date.to_string() == value)
}

fn current_tender_revision_for_estimate(
    connection: &rusqlite::Connection,
) -> Result<u32, TenderCommandError> {
    connection
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn evidence_reference(reference: &TenderEvidenceReference) -> AgentTaskInputReference {
    AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!("{}#{}", reference.artifact_id, reference.ordinal),
        version: reference.version,
    }
}

fn table_row_number(cell_range: &str) -> Option<u32> {
    let digits: String = cell_range
        .chars()
        .skip_while(|character| !character.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BoqTableDesignationManifest {
    schema_version: u32,
    designation_id: String,
    artifact_id: String,
    artifact_version: u32,
    table_number: u32,
    header_row_count: u32,
    designated_by: String,
    acting_role: String,
    created_at: String,
}

type BoqTableKey = (String, u32, u32, u32);
type BoqTableRows = BTreeMap<u32, Vec<u32>>;

fn derive_current_boq_inventory(
    connection: &rusqlite::Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<BoqInventoryRow>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT designations.artifact_id, designations.artifact_version,
                    designations.table_number, designations.header_row_count,
                    locations.ordinal, locations.cell_range
             FROM boq_table_designations AS designations
             JOIN evidence_locations AS locations
               ON locations.artifact_id = designations.artifact_id
              AND locations.version = designations.artifact_version
              AND locations.table_number = designations.table_number
             WHERE locations.kind = 'cell'
               AND locations.cell_range IS NOT NULL
               AND NOT EXISTS (
                 SELECT 1 FROM source_relationships AS relationships
                 JOIN change_assessments AS assessments USING (relationship_id)
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 WHERE relationships.prior_artifact_id = designations.artifact_id
                   AND relationships.prior_version = designations.artifact_version
                   AND relationships.relationship_kind = 'replacement'
                   AND decisions.classification = 'material'
               )
             ORDER BY designations.artifact_id, designations.artifact_version,
                      designations.table_number, locations.ordinal",
        )
        .map_err(sql_error)?;
    let mut rows = statement.query([]).map_err(sql_error)?;
    let mut tables: BTreeMap<BoqTableKey, BoqTableRows> = BTreeMap::new();
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        let artifact_id: String = row.get(0).map_err(sql_error)?;
        let version: u32 = row.get(1).map_err(sql_error)?;
        let table_number: u32 = row.get(2).map_err(sql_error)?;
        let header_row_count: u32 = row.get(3).map_err(sql_error)?;
        let ordinal: u32 = row.get(4).map_err(sql_error)?;
        let cell_range: String = row.get(5).map_err(sql_error)?;
        let row_number = table_row_number(&cell_range)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        tables
            .entry((artifact_id, version, table_number, header_row_count))
            .or_default()
            .entry(row_number)
            .or_default()
            .push(ordinal);
    }
    let mut inventory = Vec::new();
    for ((artifact_id, version, table_number, header_row_count), rows) in tables {
        check()?;
        for (row_index, (row_number, mut ordinals)) in rows.into_iter().enumerate() {
            if row_index < header_row_count as usize {
                continue;
            }
            if inventory.len() >= MAX_BOQ_ROWS {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            ordinals.sort_unstable();
            let evidence = ordinals
                .into_iter()
                .map(|ordinal| TenderEvidenceReference {
                    artifact_id: artifact_id.clone(),
                    version,
                    ordinal,
                })
                .collect::<Vec<_>>();
            let row_identity = canonical_json(&json!({
                "artifact_id": artifact_id,
                "version": version,
                "table_number": table_number,
                "row_number": row_number,
            }))?;
            inventory.push(BoqInventoryRow {
                row_key: sha256_hex(row_identity.as_bytes())[..32].into(),
                evidence,
            });
        }
    }
    if inventory.is_empty() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(inventory)
}

fn boq_inventory_sha256(inventory: &[BoqInventoryRow]) -> Result<String, TenderCommandError> {
    Ok(sha256_hex(canonical_json(&inventory)?.as_bytes()))
}

fn load_boq_table_designation_with_check(
    connection: &rusqlite::Connection,
    artifact_id: &str,
    artifact_version: u32,
    table_number: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Option<BoqTableDesignation>, TenderCommandError> {
    check()?;
    let stored: Option<(String, u32, String, String, i64, String)> = connection
        .query_row(
            "SELECT designation_id, header_row_count, manifest_json,
                    manifest_sha256, audit_sequence, created_at
             FROM boq_table_designations
             WHERE artifact_id = ?1 AND artifact_version = ?2 AND table_number = ?3",
            params![artifact_id, artifact_version, table_number],
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
        designation_id,
        header_row_count,
        manifest_json,
        manifest_sha256,
        audit_sequence,
        created_at,
    )) = stored
    else {
        return Ok(None);
    };
    let manifest: BoqTableDesignationManifest = parse_canonical(&manifest_json)?;
    let expected_change = json!({
        "artifact_id": artifact_id,
        "artifact_version": artifact_version.to_string(),
        "designation_id": designation_id,
        "header_row_count": header_row_count.to_string(),
        "manifest_sha256": manifest_sha256,
        "table_number": table_number.to_string(),
    });
    if manifest.schema_version != 1
        || manifest.designation_id != designation_id
        || manifest.artifact_id != artifact_id
        || manifest.artifact_version != artifact_version
        || manifest.table_number != table_number
        || manifest.header_row_count != header_row_count
        || manifest.designated_by != "engineer_user"
        || manifest.acting_role != "engineer_in_the_loop"
        || manifest.created_at != created_at
        || sha256_hex(manifest_json.as_bytes()) != manifest_sha256
        || !audit_is_exact(
            connection,
            audit_sequence,
            "boq_table_designated",
            &created_at,
            &expected_change,
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let row_count = table_row_count_with_check(
        connection,
        artifact_id,
        artifact_version,
        table_number,
        check,
    )?;
    Ok(Some(BoqTableDesignation {
        designation_id,
        artifact_id: artifact_id.into(),
        artifact_version,
        table_number,
        header_row_count,
        row_count,
        manifest_sha256,
        created_at,
    }))
}

fn table_row_count_with_check(
    connection: &rusqlite::Connection,
    artifact_id: &str,
    artifact_version: u32,
    table_number: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<u32, TenderCommandError> {
    check()?;
    let mut statement = connection
        .prepare(
            "SELECT cell_range FROM evidence_locations
             WHERE artifact_id = ?1 AND version = ?2 AND table_number = ?3
               AND kind = 'cell' AND cell_range IS NOT NULL
             ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let mut rows = statement
        .query(params![artifact_id, artifact_version, table_number])
        .map_err(sql_error)?;
    let mut row_numbers = HashSet::new();
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        let range: String = row.get(0).map_err(sql_error)?;
        row_numbers.insert(
            table_row_number(&range)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        );
        if row_numbers.len() > MAX_BOQ_ROWS + 8 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    u32::try_from(row_numbers.len())
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn inspect_boq_table_candidates(
    connection: &rusqlite::Connection,
    cursor: Option<&str>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(Vec<BoqTableCandidate>, Option<String>), TenderCommandError> {
    let parsed_cursor = cursor
        .map(|cursor| {
            let mut parts = cursor.split(':');
            let artifact_id = parts.next().unwrap_or_default();
            let artifact_version = parts
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| *value > 0);
            let table_number = parts
                .next()
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|value| *value > 0);
            if !valid_identifier(artifact_id)
                || artifact_version.is_none()
                || table_number.is_none()
                || parts.next().is_some()
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            Ok((
                artifact_id.to_owned(),
                artifact_version.unwrap_or_default(),
                table_number.unwrap_or_default(),
            ))
        })
        .transpose()?;
    let mut statement = connection
        .prepare(
            "SELECT locations.artifact_id, locations.version, locations.table_number,
                    artifacts.package_path, MIN(locations.original_text)
             FROM evidence_locations AS locations
             JOIN source_artifacts AS artifacts ON artifacts.artifact_id = locations.artifact_id
              WHERE locations.kind = 'cell' AND locations.table_number IS NOT NULL
               AND (?1 IS NULL
                    OR locations.artifact_id > ?1
                    OR (locations.artifact_id = ?1 AND locations.version > ?2)
                    OR (locations.artifact_id = ?1 AND locations.version = ?2
                        AND locations.table_number > ?3))
               AND NOT EXISTS (
                 SELECT 1 FROM source_relationships AS relationships
                 JOIN change_assessments AS assessments USING (relationship_id)
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 WHERE relationships.prior_artifact_id = locations.artifact_id
                   AND relationships.prior_version = locations.version
                   AND relationships.relationship_kind = 'replacement'
                   AND decisions.classification = 'material'
               )
             GROUP BY locations.artifact_id, locations.version, locations.table_number
             ORDER BY locations.artifact_id, locations.version, locations.table_number
             LIMIT 17",
        )
        .map_err(sql_error)?;
    let mut rows = statement
        .query(params![
            parsed_cursor.as_ref().map(|cursor| cursor.0.as_str()),
            parsed_cursor.as_ref().map(|cursor| cursor.1),
            parsed_cursor.as_ref().map(|cursor| cursor.2),
        ])
        .map_err(sql_error)?;
    let mut candidates = Vec::new();
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        let artifact_id: String = row.get(0).map_err(sql_error)?;
        let artifact_version: u32 = row.get(1).map_err(sql_error)?;
        let table_number: u32 = row.get(2).map_err(sql_error)?;
        let row_count = table_row_count_with_check(
            connection,
            &artifact_id,
            artifact_version,
            table_number,
            check,
        )?;
        candidates.push(BoqTableCandidate {
            designation: load_boq_table_designation_with_check(
                connection,
                &artifact_id,
                artifact_version,
                table_number,
                check,
            )?,
            artifact_id,
            artifact_version,
            table_number,
            document_name: row.get(3).map_err(sql_error)?,
            row_count,
            sample_text: row.get(4).map_err(sql_error)?,
        });
    }
    let has_more = candidates.len() > BOQ_CANDIDATE_PAGE_SIZE;
    if has_more {
        candidates.pop();
    }
    let next_cursor = has_more.then(|| {
        let last = candidates
            .last()
            .expect("a BOQ candidate page with a successor is non-empty");
        format!(
            "{}:{}:{}",
            last.artifact_id, last.artifact_version, last.table_number
        )
    });
    Ok((candidates, next_cursor))
}

fn exact_input_reference(kind: &str, reference: &str, version: u32) -> AgentTaskInputReference {
    AgentTaskInputReference {
        kind: kind.into(),
        reference: reference.into(),
        version,
    }
}

struct EstimateTaskTarget {
    tender_id: String,
    tender_revision: u32,
    plan_id: String,
    plan_version: u32,
    basis_id: String,
    basis_version: u32,
    supersedes_basis_manifest_sha256: Option<String>,
    remediates_review_manifest_sha256: Option<String>,
    boq_rows: Vec<BoqInventoryRow>,
    boq_inventory_sha256: String,
    quotation_evidence: Vec<TenderEvidenceReference>,
    calculation_run_ids: Vec<String>,
    query_references: Vec<EstimateQueryReference>,
    query_inventory: Vec<EstimateQueryObservation>,
    query_inventory_sha256: String,
}

struct NormalizedEstimate {
    material_assumptions: Vec<EstimateMaterialAssumption>,
    aggregate_inputs: Vec<EstimateAggregateCalculationInput>,
    total_amount: String,
    total_currency: String,
    complete: bool,
    reconciled: bool,
    blockers: Vec<String>,
}

fn validate_basis_command(
    command: &RunCostEstimatorBasisCommand,
) -> Result<(), TenderCommandError> {
    if command.calculation_run_ids.is_empty()
        || command.calculation_run_ids.len() > MAX_CALCULATION_RUNS
        || command
            .calculation_run_ids
            .iter()
            .any(|id| !valid_identifier(id))
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let unique_calculations: HashSet<_> = command.calculation_run_ids.iter().collect();
    let unique_quotes: HashSet<_> = command.quotation_evidence.iter().collect();
    if unique_calculations.len() != command.calculation_run_ids.len()
        || unique_quotes.len() != command.quotation_evidence.len()
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn basis_output_contract() -> Result<String, TenderCommandError> {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "scope": { "type": "string", "minLength": 1, "maxLength": 4000 },
            "pricing_date": { "type": "string", "minLength": 10, "maxLength": 10 },
            "currencies": { "type": "array", "minItems": 1, "maxItems": 16, "items": { "type": "string", "minLength": 3, "maxLength": 3 } },
            "taxes": { "type": "array", "maxItems": MAX_LIST_ITEMS, "items": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES } },
            "rate_sources": { "type": "array", "minItems": 1, "maxItems": MAX_LIST_ITEMS, "items": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES } },
            "productivity": { "type": "array", "minItems": 1, "maxItems": MAX_LIST_ITEMS, "items": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES } },
            "design_maturity": { "type": "string", "minLength": 1, "maxLength": 4000 },
            "gaps": { "type": "array", "maxItems": MAX_LIST_ITEMS, "items": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES } },
            "exclusions": { "type": "array", "maxItems": MAX_LIST_ITEMS, "items": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES } },
            "boq_rows": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_BOQ_ROWS,
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "row_key": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "description": { "type": "string", "minLength": 1, "maxLength": 2000 },
                        "disposition": { "enum": ["priced", "excluded", "provisional", "duplicated", "missing", "not_applicable", "blocked"] },
                        "evidence": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 64,
                            "items": {
                                "additionalProperties": false,
                                "properties": {
                                    "artifact_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                                    "version": { "type": "integer", "minimum": 1 },
                                    "ordinal": { "type": "integer", "minimum": 0 }
                                },
                                "required": ["artifact_id", "version", "ordinal"],
                                "type": "object"
                            },
                        },
                        "calculation_run_id": { "type": ["string", "null"], "minLength": 32, "maxLength": 32 },
                        "affected_queries": {
                            "type": "array",
                            "maxItems": MAX_QUERIES,
                            "items": {
                                "additionalProperties": false,
                                "properties": {
                                    "query_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                                    "version": { "type": "integer", "minimum": 1, "maximum": 32 }
                                },
                                "required": ["query_id", "version"],
                                "type": "object"
                            }
                        }
                    },
                    "required": ["row_key", "description", "disposition", "evidence", "calculation_run_id", "affected_queries"],
                    "type": "object"
                }
            },
            "cbs_components": {
                "type": "array",
                "maxItems": MAX_CBS_COMPONENTS,
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "component_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "cost_code": { "type": "string", "minLength": 1, "maxLength": 100 },
                        "work_package": { "type": "string", "minLength": 1, "maxLength": 200 },
                        "category": { "enum": ["labor", "plant", "material", "subcontract", "indirect", "temporary_works", "overhead", "risk", "other_approved"] },
                        "description": { "type": "string", "minLength": 1, "maxLength": 2000 },
                        "boq_row_keys": { "type": "array", "maxItems": MAX_BOQ_ROWS, "items": { "type": "string", "minLength": 1, "maxLength": 100 } },
                        "resource_build_up_ids": { "type": "array", "minItems": 1, "maxItems": MAX_CBS_COMPONENTS, "items": { "type": "string", "minLength": 32, "maxLength": 32 } }
                    },
                    "required": ["component_id", "cost_code", "work_package", "category", "description", "boq_row_keys", "resource_build_up_ids"],
                    "type": "object"
                }
            },
            "resource_build_ups": {
                "type": "array",
                "maxItems": MAX_CBS_COMPONENTS,
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "build_up_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "cbs_component_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "category": { "enum": ["labor", "plant", "material", "subcontract", "indirect", "temporary_works", "overhead", "risk", "other_approved"] },
                        "description": { "type": "string", "minLength": 1, "maxLength": 2000 },
                        "calculation_run_id": { "type": "string", "minLength": 32, "maxLength": 32 }
                    },
                    "required": ["build_up_id", "cbs_component_id", "category", "description", "calculation_run_id"],
                    "type": "object"
                }
            },
            "quotations": {
                "type": "array",
                "maxItems": MAX_QUOTES,
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "quotation_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "kind": { "enum": ["supplier", "subcontractor"] },
                        "counterparty": { "type": "string", "minLength": 1, "maxLength": 200 },
                        "exact_scope": { "type": "string", "minLength": 1, "maxLength": 2000 },
                        "quotation_date": { "type": "string", "minLength": 10, "maxLength": 10 },
                        "currency": { "type": "string", "minLength": 3, "maxLength": 3 },
                        "exclusions": { "type": "array", "maxItems": MAX_LIST_ITEMS, "items": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES } },
                        "valid_until": { "type": "string", "minLength": 10, "maxLength": 10 },
                        "evidence": {
                            "additionalProperties": false,
                            "properties": {
                                "artifact_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                                "version": { "type": "integer", "minimum": 1 },
                                "ordinal": { "type": "integer", "minimum": 0 }
                            },
                            "required": ["artifact_id", "version", "ordinal"],
                            "type": "object"
                        },
                        "normalization_calculation_run_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "covered_boq_row_keys": { "type": "array", "minItems": 1, "maxItems": MAX_BOQ_ROWS, "items": { "type": "string", "minLength": 1, "maxLength": 100 } },
                        "comparison_assumptions": { "type": "array", "minItems": 1, "maxItems": MAX_LIST_ITEMS, "items": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES } }
                    },
                    "required": ["quotation_id", "kind", "counterparty", "exact_scope", "quotation_date", "currency", "exclusions", "valid_until", "evidence", "normalization_calculation_run_id", "covered_boq_row_keys", "comparison_assumptions"],
                    "type": "object"
                }
            },
            "allowances": {
                "type": "array",
                "maxItems": MAX_CBS_COMPONENTS,
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "allowance_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "description": { "type": "string", "minLength": 1, "maxLength": 2000 },
                        "cbs_component_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "resource_build_up_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "query_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "query_version": { "type": "integer", "minimum": 1, "maximum": 32 },
                        "decision_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "evidence": {
                            "type": "array", "minItems": 1, "maxItems": 32,
                            "items": {
                                "additionalProperties": false,
                                "properties": {
                                    "artifact_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                                    "version": { "type": "integer", "minimum": 1 },
                                    "ordinal": { "type": "integer", "minimum": 0 }
                                },
                                "required": ["artifact_id", "version", "ordinal"],
                                "type": "object"
                            }
                        },
                        "rationale": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_BYTES }
                    },
                    "required": ["allowance_id", "description", "cbs_component_id", "resource_build_up_id", "query_id", "query_version", "decision_id", "evidence", "rationale"],
                    "type": "object"
                }
            },
            "material_assumptions": {
                "type": "array",
                "maxItems": MAX_QUERIES,
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "query_id": { "type": "string", "minLength": 32, "maxLength": 32 },
                        "version": { "type": "integer", "minimum": 1, "maximum": 32 }
                    },
                    "required": ["query_id", "version"],
                    "type": "object"
                }
            },
            "comparison_total_calculation_run_id": { "type": "string", "minLength": 32, "maxLength": 32 }
        },
        "required": [
            "scope", "pricing_date", "currencies", "taxes", "rate_sources",
            "productivity", "design_maturity", "gaps", "exclusions", "boq_rows",
            "cbs_components", "resource_build_ups", "quotations", "allowances",
            "material_assumptions", "comparison_total_calculation_run_id"
        ],
        "type": "object"
    }))
}

fn basis_review_output_contract() -> Result<String, TenderCommandError> {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "outcome": { "enum": ["passed", "failed"] },
            "findings": {
                "type": "array",
                "maxItems": MAX_FINDINGS,
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "code": { "type": "string", "minLength": 1, "maxLength": 100, "pattern": "^[a-z0-9_]+$" },
                        "summary": { "type": "string", "minLength": 1, "maxLength": 2000 },
                        "affected_boq_row_keys": { "type": "array", "maxItems": MAX_BOQ_ROWS, "items": { "type": "string", "minLength": 1, "maxLength": 100 } }
                    },
                    "required": ["code", "summary", "affected_boq_row_keys"],
                    "type": "object"
                }
            }
        },
        "required": ["outcome", "findings"],
        "type": "object"
    }))
}

fn review_candidate_is_valid(
    boq_rows: &[BoqAccountRow],
    complete: bool,
    reconciled: bool,
    candidate: &BasisOfEstimateReviewCandidate,
) -> bool {
    let row_keys: HashSet<_> = boq_rows.iter().map(|row| &row.row_key).collect();
    let mut codes = HashSet::new();
    let findings_valid = candidate.findings.len() <= MAX_FINDINGS
        && candidate.findings.iter().all(|finding| {
            !finding.code.is_empty()
                && finding.code.len() <= 100
                && finding
                    .code
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
                && codes.insert(finding.code.clone())
                && valid_text(&finding.summary, 2_000)
                && finding.affected_boq_row_keys.len() <= MAX_BOQ_ROWS
                && finding
                    .affected_boq_row_keys
                    .iter()
                    .all(|key| row_keys.contains(key))
        });
    findings_valid
        && match candidate.outcome {
            BasisOfEstimateReviewOutcome::Passed => {
                candidate.findings.is_empty() && complete && reconciled
            }
            BasisOfEstimateReviewOutcome::Failed => !candidate.findings.is_empty(),
        }
}

fn basis_task(
    task_id: String,
    target: &EstimateTaskTarget,
    profile: &AgentProfileVersionView,
    deadline: String,
) -> Result<TenderTaskView, TenderCommandError> {
    let mut exact_inputs = vec![
        exact_input_reference("tender_revision", &target.tender_id, target.tender_revision),
        exact_input_reference("work_plan_version", &target.plan_id, target.plan_version),
        exact_input_reference(
            "basis_of_estimate_request",
            &target.basis_id,
            target.basis_version,
        ),
        exact_input_reference(
            "estimate_query_inventory",
            &target.query_inventory_sha256,
            target.basis_version,
        ),
    ];
    if let Some(manifest_sha256) = &target.supersedes_basis_manifest_sha256 {
        exact_inputs.push(exact_input_reference(
            "superseded_basis_manifest",
            manifest_sha256,
            target.basis_version.saturating_sub(1),
        ));
    }
    if let Some(manifest_sha256) = &target.remediates_review_manifest_sha256 {
        exact_inputs.push(exact_input_reference(
            "basis_review_findings",
            manifest_sha256,
            target.basis_version.saturating_sub(1),
        ));
    }
    for row in &target.boq_rows {
        for reference in &row.evidence {
            let reference = evidence_reference(reference);
            exact_inputs.push(exact_input_reference(
                &format!("estimate_boq_row_{}", row.row_key),
                &reference.reference,
                reference.version,
            ));
        }
    }
    exact_inputs.extend(target.quotation_evidence.iter().map(|reference| {
        let reference = evidence_reference(reference);
        exact_input_reference(
            "estimate_quotation_evidence",
            &reference.reference,
            reference.version,
        )
    }));
    exact_inputs.extend(
        target.calculation_run_ids.iter().map(|id| {
            exact_input_reference("approved_calculation_run", id, target.tender_revision)
        }),
    );
    exact_inputs.extend(target.query_references.iter().map(|reference| {
        exact_input_reference(
            "tender_query_version",
            &reference.query_id,
            reference.version,
        )
    }));
    exact_inputs.sort_by(|left, right| {
        (&left.kind, &left.reference, left.version).cmp(&(
            &right.kind,
            &right.reference,
            right.version,
        ))
    });
    Ok(TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: "Build one complete evidence-linked BOQ account, Cost Breakdown Structure, quotation register, resource build-ups and versioned Basis of Estimate. Use only supplied approved Calculation Runs for arithmetic; expose every gap and affected Query.".into(),
        exact_inputs,
        output_contract_json: basis_output_contract()?,
        review_policy: "The Cost Estimator proposes structure and basis only. It cannot calculate, approve assumptions, review its own work, choose margin, or approve reliance.".into(),
        deadline,
        permissions: profile.permissions.clone(),
        resource_budget: profile.resource_budget.clone(),
    })
}

struct BasisReviewTaskRequest<'a> {
    task_id: String,
    tender_id: &'a str,
    tender_revision: u32,
    plan_id: &'a str,
    plan_version: u32,
    basis: &'a BasisOfEstimateVersion,
    profile: &'a AgentProfileVersionView,
    deadline: String,
}

fn review_task(request: BasisReviewTaskRequest<'_>) -> Result<TenderTaskView, TenderCommandError> {
    let mut exact_inputs = vec![
        exact_input_reference(
            "tender_revision",
            request.tender_id,
            request.tender_revision,
        ),
        exact_input_reference("work_plan_version", request.plan_id, request.plan_version),
        exact_input_reference(
            "basis_of_estimate_version",
            &request.basis.basis_id,
            request.basis.version,
        ),
        exact_input_reference(
            "estimate_aggregate_calculation",
            &request.basis.aggregate_calculation.aggregate_run_id,
            request.basis.version,
        ),
        exact_input_reference(
            "estimate_query_inventory",
            &request.basis.query_inventory_sha256,
            request.basis.version,
        ),
    ];
    exact_inputs.extend(
        request
            .basis
            .boq_rows
            .iter()
            .flat_map(|row| row.evidence.iter().map(evidence_reference)),
    );
    exact_inputs.extend(
        request
            .basis
            .quotations
            .iter()
            .map(|quote| evidence_reference(&quote.evidence)),
    );
    let mut calculation_run_ids = HashSet::new();
    calculation_run_ids.insert(request.basis.comparison_total_calculation_run_id.clone());
    calculation_run_ids.extend(
        request
            .basis
            .resource_build_ups
            .iter()
            .map(|build_up| build_up.calculation_run_id.clone()),
    );
    calculation_run_ids.extend(
        request
            .basis
            .quotations
            .iter()
            .map(|quote| quote.normalization_calculation_run_id.clone()),
    );
    exact_inputs.extend(calculation_run_ids.into_iter().map(|run_id| {
        exact_input_reference("approved_calculation_run", &run_id, request.tender_revision)
    }));
    exact_inputs.extend(request.basis.query_inventory.iter().map(|observation| {
        exact_input_reference(
            "tender_query_version",
            &observation.query_id,
            observation.version,
        )
    }));
    exact_inputs.sort_by(|left, right| {
        (&left.kind, &left.reference, left.version).cmp(&(
            &right.kind,
            &right.reference,
            right.version,
        ))
    });
    exact_inputs.dedup();
    Ok(TenderTaskView {
        task_id: request.task_id,
        profile_id: request.profile.profile_id.clone(),
        profile_version: request.profile.version,
        objective: "Independently reproduce the exact Basis of Estimate, verify BOQ completeness, quote scope, query/assumption authority, CBS coverage and Calculation Run reconciliation, and return findings without editing the estimate.".into(),
        exact_inputs,
        output_contract_json: basis_review_output_contract()?,
        review_policy: "Pass only if the exact immutable Basis is complete and reconciled, every numerical build-up and total is an approved Calculation Run, every quotation is scope-bound, and every material assumption has an exact EITL decision. The reviewer cannot edit or approve the Basis.".into(),
        deadline: request.deadline,
        permissions: request.profile.permissions.clone(),
        resource_budget: request.profile.resource_budget.clone(),
    })
}

fn exact_basis_target(task: &TenderTaskView) -> Result<EstimateTaskTarget, TenderCommandError> {
    let one = |kind: &str| -> Result<&AgentTaskInputReference, TenderCommandError> {
        let matches: Vec<_> = task
            .exact_inputs
            .iter()
            .filter(|input| input.kind == kind)
            .collect();
        if matches.len() == 1 {
            Ok(matches[0])
        } else {
            Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed))
        }
    };
    let optional_one =
        |kind: &str| -> Result<Option<&AgentTaskInputReference>, TenderCommandError> {
            let mut matches = task.exact_inputs.iter().filter(|input| input.kind == kind);
            let first = matches.next();
            if matches.next().is_some() {
                Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed))
            } else {
                Ok(first)
            }
        };
    let tender = one("tender_revision")?;
    let plan = one("work_plan_version")?;
    let request = one("basis_of_estimate_request")?;
    let query_inventory = one("estimate_query_inventory")?;
    if query_inventory.version != request.version
        || query_inventory.reference.len() != 64
        || !query_inventory
            .reference
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let parse_evidence = |kind: &str| -> Result<Vec<TenderEvidenceReference>, TenderCommandError> {
        task.exact_inputs
            .iter()
            .filter(|input| input.kind == kind)
            .map(|input| {
                let (artifact_id, ordinal) = input
                    .reference
                    .rsplit_once('#')
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                Ok(TenderEvidenceReference {
                    artifact_id: artifact_id.into(),
                    version: input.version,
                    ordinal: ordinal
                        .parse()
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                })
            })
            .collect()
    };
    let mut boq_by_row: BTreeMap<String, Vec<TenderEvidenceReference>> = BTreeMap::new();
    for input in task
        .exact_inputs
        .iter()
        .filter(|input| input.kind.starts_with("estimate_boq_row_"))
    {
        let row_key = input
            .kind
            .strip_prefix("estimate_boq_row_")
            .filter(|value| valid_identifier(value))
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let (artifact_id, ordinal) = input
            .reference
            .rsplit_once('#')
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        boq_by_row
            .entry(row_key.into())
            .or_default()
            .push(TenderEvidenceReference {
                artifact_id: artifact_id.into(),
                version: input.version,
                ordinal: ordinal
                    .parse()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            });
    }
    let boq_rows = boq_by_row
        .into_iter()
        .map(|(row_key, mut evidence)| {
            evidence.sort_by(|left, right| {
                (&left.artifact_id, left.version, left.ordinal).cmp(&(
                    &right.artifact_id,
                    right.version,
                    right.ordinal,
                ))
            });
            BoqInventoryRow { row_key, evidence }
        })
        .collect::<Vec<_>>();
    let boq_inventory_sha256 = boq_inventory_sha256(&boq_rows)?;
    Ok(EstimateTaskTarget {
        tender_id: tender.reference.clone(),
        tender_revision: tender.version,
        plan_id: plan.reference.clone(),
        plan_version: plan.version,
        basis_id: request.reference.clone(),
        basis_version: request.version,
        supersedes_basis_manifest_sha256: optional_one("superseded_basis_manifest")?
            .map(|input| input.reference.clone()),
        remediates_review_manifest_sha256: optional_one("basis_review_findings")?
            .map(|input| input.reference.clone()),
        boq_rows,
        boq_inventory_sha256,
        quotation_evidence: parse_evidence("estimate_quotation_evidence")?,
        calculation_run_ids: task
            .exact_inputs
            .iter()
            .filter(|input| input.kind == "approved_calculation_run")
            .map(|input| input.reference.clone())
            .collect(),
        query_references: task
            .exact_inputs
            .iter()
            .filter(|input| input.kind == "tender_query_version")
            .map(|input| EstimateQueryReference {
                query_id: input.reference.clone(),
                version: input.version,
            })
            .collect(),
        query_inventory: Vec::new(),
        query_inventory_sha256: query_inventory.reference.clone(),
    })
}

fn exact_review_target(
    task: &TenderTaskView,
) -> Result<(String, u32, String, u32), TenderCommandError> {
    let bases: Vec<_> = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "basis_of_estimate_version")
        .collect();
    let plans: Vec<_> = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "work_plan_version")
        .collect();
    if bases.len() != 1 || plans.len() != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((
        bases[0].reference.clone(),
        bases[0].version,
        plans[0].reference.clone(),
        plans[0].version,
    ))
}

fn validate_candidate_shape(
    candidate: &BasisOfEstimateCandidate,
    target: &EstimateTaskTarget,
) -> Result<(), TenderCommandError> {
    if (target.basis_version == 1
        && (target.supersedes_basis_manifest_sha256.is_some()
            || target.remediates_review_manifest_sha256.is_some()))
        || (target.basis_version > 1
            && target
                .supersedes_basis_manifest_sha256
                .as_ref()
                .is_none_or(|hash| hash.len() != 64))
        || target
            .remediates_review_manifest_sha256
            .as_ref()
            .is_some_and(|hash| hash.len() != 64)
        || boq_inventory_sha256(&target.boq_rows)? != target.boq_inventory_sha256
        || !valid_text(&candidate.scope, MAX_TEXT_BYTES)
        || !valid_iso_date(&candidate.pricing_date)
        || candidate.currencies.is_empty()
        || candidate.currencies.len() > 16
        || candidate
            .currencies
            .iter()
            .any(|currency| !valid_estimate_currency(currency))
        || !valid_optional_list(&candidate.taxes, MAX_LIST_ITEMS)
        || candidate.rate_sources.is_empty()
        || !valid_optional_list(&candidate.rate_sources, MAX_LIST_ITEMS)
        || candidate.productivity.is_empty()
        || !valid_optional_list(&candidate.productivity, MAX_LIST_ITEMS)
        || !valid_text(&candidate.design_maturity, MAX_TEXT_BYTES)
        || !valid_optional_list(&candidate.gaps, MAX_LIST_ITEMS)
        || !valid_optional_list(&candidate.exclusions, MAX_LIST_ITEMS)
        || candidate.boq_rows.is_empty()
        || candidate.boq_rows.len() > MAX_BOQ_ROWS
        || candidate.cbs_components.len() > MAX_CBS_COMPONENTS
        || candidate.resource_build_ups.len() > MAX_CBS_COMPONENTS
        || candidate.quotations.len() > MAX_QUOTES
        || candidate.allowances.len() > MAX_CBS_COMPONENTS
        || candidate.material_assumptions.len() > MAX_QUERIES
        || !valid_identifier(&candidate.comparison_total_calculation_run_id)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }

    let allowed_boq: BTreeMap<_, _> = target
        .boq_rows
        .iter()
        .map(|row| (row.row_key.clone(), row.evidence.clone()))
        .collect();
    let observed_boq: BTreeMap<_, _> = candidate
        .boq_rows
        .iter()
        .map(|row| (row.row_key.clone(), row.evidence.clone()))
        .collect();
    let allowed_quotes: HashSet<_> = target.quotation_evidence.iter().cloned().collect();
    let observed_quotes: HashSet<_> = candidate
        .quotations
        .iter()
        .map(|quote| quote.evidence.clone())
        .collect();
    if observed_boq != allowed_boq
        || observed_boq.len() != candidate.boq_rows.len()
        || observed_quotes != allowed_quotes
        || observed_quotes.len() != candidate.quotations.len()
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }

    let allowed_queries: HashSet<_> = target.query_references.iter().cloned().collect();
    let allowed_calculations: HashSet<_> = target.calculation_run_ids.iter().cloned().collect();
    let mut used_queries = HashSet::new();
    let mut used_calculations = HashSet::new();
    let mut row_keys = HashSet::new();
    let mut priced_row_keys = HashSet::new();
    for row in &candidate.boq_rows {
        if !valid_text(&row.row_key, 100)
            || !valid_text(&row.description, 2_000)
            || !row_keys.insert(row.row_key.clone())
            || row.evidence.is_empty()
            || row.evidence.len() > 64
            || {
                let unique: HashSet<_> = row.evidence.iter().collect();
                unique.len() != row.evidence.len()
            }
            || row
                .affected_queries
                .iter()
                .any(|query| !allowed_queries.contains(query))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        used_queries.extend(row.affected_queries.iter().cloned());
        match row.disposition {
            BoqRowDisposition::Priced | BoqRowDisposition::Provisional => {
                let calculation = row
                    .calculation_run_id
                    .as_ref()
                    .filter(|id| allowed_calculations.contains(*id))
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                used_calculations.insert(calculation.clone());
                priced_row_keys.insert(row.row_key.clone());
            }
            BoqRowDisposition::Missing | BoqRowDisposition::Blocked => {
                if row.calculation_run_id.is_some() || row.affected_queries.is_empty() {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            BoqRowDisposition::Excluded
            | BoqRowDisposition::Duplicated
            | BoqRowDisposition::NotApplicable => {
                if row.calculation_run_id.is_some() {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
        }
    }

    let mut component_ids = HashSet::new();
    let mut cost_codes = HashSet::new();
    let mut mapped_priced_rows = HashSet::new();
    let mut component_build_up_ids: HashMap<String, HashSet<String>> = HashMap::new();
    let mut row_component = HashMap::new();
    for component in &candidate.cbs_components {
        if !valid_identifier(&component.component_id)
            || !valid_text(&component.cost_code, 100)
            || !valid_text(&component.work_package, 200)
            || !valid_text(&component.description, 2_000)
            || !component_ids.insert(component.component_id.clone())
            || !cost_codes.insert(component.cost_code.clone())
            || component.boq_row_keys.len() > MAX_BOQ_ROWS
            || component.resource_build_up_ids.is_empty()
            || component.resource_build_up_ids.len() > MAX_CBS_COMPONENTS
            || component.boq_row_keys.iter().any(|key| {
                !row_keys.contains(key)
                    || !mapped_priced_rows.insert(key.clone())
                    || row_component
                        .insert(key.clone(), component.component_id.clone())
                        .is_some()
            })
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let declared: HashSet<_> = component.resource_build_up_ids.iter().cloned().collect();
        if declared.len() != component.resource_build_up_ids.len()
            || declared.iter().any(|id| !valid_identifier(id))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        component_build_up_ids.insert(component.component_id.clone(), declared);
    }
    if mapped_priced_rows != priced_row_keys {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }

    let mut build_up_ids = HashSet::new();
    let mut build_up_calculations = HashSet::new();
    let mut observed_component_build_ups: HashMap<String, HashSet<String>> = HashMap::new();
    for build_up in &candidate.resource_build_ups {
        let component = candidate
            .cbs_components
            .iter()
            .find(|component| component.component_id == build_up.cbs_component_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !valid_identifier(&build_up.build_up_id)
            || !build_up_ids.insert(build_up.build_up_id.clone())
            || !valid_text(&build_up.description, 2_000)
            || component.category != build_up.category
            || !allowed_calculations.contains(&build_up.calculation_run_id)
            || !build_up_calculations.insert(build_up.calculation_run_id.clone())
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        observed_component_build_ups
            .entry(build_up.cbs_component_id.clone())
            .or_default()
            .insert(build_up.build_up_id.clone());
        used_calculations.insert(build_up.calculation_run_id.clone());
    }
    if component_build_up_ids != observed_component_build_ups {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    for row in candidate.boq_rows.iter().filter(|row| {
        matches!(
            row.disposition,
            BoqRowDisposition::Priced | BoqRowDisposition::Provisional
        )
    }) {
        let component_id = row_component
            .get(&row.row_key)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let calculation_run_id = row
            .calculation_run_id
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !candidate.resource_build_ups.iter().any(|build_up| {
            &build_up.cbs_component_id == component_id
                && &build_up.calculation_run_id == calculation_run_id
        }) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }

    let mut quote_ids = HashSet::new();
    for quote in &candidate.quotations {
        if !valid_identifier(&quote.quotation_id)
            || !quote_ids.insert(quote.quotation_id.clone())
            || !valid_text(&quote.counterparty, 200)
            || !valid_text(&quote.exact_scope, 2_000)
            || !valid_iso_date(&quote.quotation_date)
            || !valid_estimate_currency(&quote.currency)
            || !valid_iso_date(&quote.valid_until)
            || quote.valid_until < quote.quotation_date
            || !valid_optional_list(&quote.exclusions, MAX_LIST_ITEMS)
            || quote.covered_boq_row_keys.is_empty()
            || quote.covered_boq_row_keys.len() > MAX_BOQ_ROWS
            || quote
                .covered_boq_row_keys
                .iter()
                .any(|key| !row_keys.contains(key))
            || quote.comparison_assumptions.is_empty()
            || !valid_optional_list(&quote.comparison_assumptions, MAX_LIST_ITEMS)
            || !allowed_calculations.contains(&quote.normalization_calculation_run_id)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if build_up_calculations.contains(&quote.normalization_calculation_run_id)
            || quote.normalization_calculation_run_id
                == candidate.comparison_total_calculation_run_id
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        used_calculations.insert(quote.normalization_calculation_run_id.clone());
    }

    let mut allowance_ids = HashSet::new();
    let allowed_evidence = target
        .boq_rows
        .iter()
        .flat_map(|row| row.evidence.iter())
        .chain(target.quotation_evidence.iter())
        .collect::<HashSet<_>>();
    for allowance in &candidate.allowances {
        let build_up = candidate
            .resource_build_ups
            .iter()
            .find(|build_up| build_up.build_up_id == allowance.resource_build_up_id);
        let query = EstimateQueryReference {
            query_id: allowance.query_id.clone(),
            version: allowance.query_version,
        };
        if !valid_identifier(&allowance.allowance_id)
            || !allowance_ids.insert(allowance.allowance_id.clone())
            || !valid_text(&allowance.description, 2_000)
            || !valid_text(&allowance.rationale, MAX_TEXT_BYTES)
            || !valid_identifier(&allowance.decision_id)
            || !component_ids.contains(&allowance.cbs_component_id)
            || build_up.is_none_or(|build_up| {
                build_up.cbs_component_id != allowance.cbs_component_id
                    || !matches!(
                        build_up.category,
                        CostComponentCategory::Risk | CostComponentCategory::OtherApproved
                    )
            })
            || !allowed_queries.contains(&query)
            || allowance.evidence.is_empty()
            || allowance.evidence.len() > 32
            || allowance.evidence.iter().collect::<HashSet<_>>().len() != allowance.evidence.len()
            || allowance
                .evidence
                .iter()
                .any(|evidence| !allowed_evidence.contains(evidence))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        used_queries.insert(query);
    }
    for assumption in &candidate.material_assumptions {
        if !allowed_queries.contains(assumption) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        used_queries.insert(assumption.clone());
    }
    if build_up_calculations.contains(&candidate.comparison_total_calculation_run_id) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    used_calculations.insert(candidate.comparison_total_calculation_run_id.clone());
    if used_calculations != allowed_calculations || used_queries != allowed_queries {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn calculation_consumes_evidence(
    run: &super::calculations::ControlledBoqCalculationRun,
    required: impl IntoIterator<Item = TenderEvidenceReference>,
) -> bool {
    let consumed = run
        .quantity
        .evidence
        .iter()
        .chain(&run.unit_rate.evidence)
        .chain(&run.exchange_rate.evidence)
        .collect::<Vec<_>>();
    required
        .into_iter()
        .map(|reference| evidence_reference(&reference))
        .all(|reference| consumed.iter().any(|candidate| **candidate == reference))
}

fn normalize_candidate(
    connection: &rusqlite::Connection,
    candidate: &BasisOfEstimateCandidate,
    target: &EstimateTaskTarget,
    require_current_queries: bool,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<NormalizedEstimate, TenderCommandError> {
    validate_candidate_shape(candidate, target)?;
    let mut calculations = HashMap::new();
    for calculation_run_id in &target.calculation_run_ids {
        check()?;
        let run = approved_calculation_run_for_estimate(connection, calculation_run_id, check)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if run.tender_revision != target.tender_revision
            || run.status != ControlledBoqCalculationStatus::Completed
            || run.final_amount.is_none()
            || run.approval.is_none()
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        calculations.insert(calculation_run_id.clone(), run);
    }

    let mut material_assumptions = Vec::with_capacity(candidate.material_assumptions.len());
    let mut query_decisions = HashMap::new();
    for observation in &target.query_inventory {
        check()?;
        let current: bool = connection
            .query_row(
                if require_current_queries {
                    "SELECT EXISTS(SELECT 1 FROM tender_query_heads WHERE query_id = ?1 AND current_version = ?2)"
                } else {
                    "SELECT EXISTS(SELECT 1 FROM tender_query_versions WHERE query_id = ?1 AND version = ?2)"
                },
                params![observation.query_id, observation.version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !current {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let query = EstimateQueryReference {
            query_id: observation.query_id.clone(),
            version: observation.version,
        };
        query_decisions.insert(
            query,
            query_decision_at_observation(connection, observation)?,
        );
    }
    for reference in &candidate.material_assumptions {
        let decision = query_decisions
            .get(reference)
            .and_then(Clone::clone)
            .filter(|decision| decision.treatment == TenderQueryTreatment::ApprovedAssumption)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        material_assumptions.push(material_assumption(&decision));
    }

    for row in candidate.boq_rows.iter().filter(|row| {
        matches!(
            row.disposition,
            BoqRowDisposition::Priced | BoqRowDisposition::Provisional
        )
    }) {
        let run = calculations
            .get(
                row.calculation_run_id
                    .as_ref()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            )
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if !calculation_consumes_evidence(run, row.evidence.clone()) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    let rows_by_key = candidate
        .boq_rows
        .iter()
        .map(|row| (row.row_key.as_str(), row))
        .collect::<HashMap<_, _>>();
    for quotation in &candidate.quotations {
        let run = calculations
            .get(&quotation.normalization_calculation_run_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let required = std::iter::once(quotation.evidence.clone()).chain(
            quotation
                .covered_boq_row_keys
                .iter()
                .filter_map(|key| rows_by_key.get(key.as_str()))
                .flat_map(|row| row.evidence.iter().cloned()),
        );
        if !calculation_consumes_evidence(run, required) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    for allowance in &candidate.allowances {
        let reference = EstimateQueryReference {
            query_id: allowance.query_id.clone(),
            version: allowance.query_version,
        };
        let decision = query_decisions
            .get(&reference)
            .and_then(Clone::clone)
            .filter(|decision| {
                decision.decision_id == allowance.decision_id
                    && decision.treatment == TenderQueryTreatment::Allowance
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if decision.rationale != allowance.rationale {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let build_up = candidate
            .resource_build_ups
            .iter()
            .find(|build_up| build_up.build_up_id == allowance.resource_build_up_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let run = calculations
            .get(&build_up.calculation_run_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if !calculation_consumes_evidence(run, allowance.evidence.clone()) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }

    let comparison_total_run = calculations
        .get(&candidate.comparison_total_calculation_run_id)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let comparison_total_amount = comparison_total_run
        .final_amount
        .as_ref()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut aggregate_inputs = Vec::with_capacity(candidate.resource_build_ups.len());
    for build_up in &candidate.resource_build_ups {
        check()?;
        let run = calculations
            .get(&build_up.calculation_run_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if run.output_currency != comparison_total_run.output_currency {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let amount = run
            .final_amount
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        aggregate_inputs.push(EstimateAggregateCalculationInput {
            build_up_id: build_up.build_up_id.clone(),
            cbs_component_id: build_up.cbs_component_id.clone(),
            calculation_run_id: run.calculation_run_id.clone(),
            calculation_manifest_sha256: run.manifest_sha256.clone(),
            amount: amount.clone(),
            currency: run.output_currency.clone(),
        });
    }
    aggregate_inputs.sort_by(|left, right| {
        (
            &left.cbs_component_id,
            &left.build_up_id,
            &left.calculation_run_id,
        )
            .cmp(&(
                &right.cbs_component_id,
                &right.build_up_id,
                &right.calculation_run_id,
            ))
    });
    let total_amount = evaluate_estimate_aggregate(
        &aggregate_inputs,
        comparison_total_run.precision,
        comparison_total_run.rounding_mode,
    )?;
    let reconciled = &total_amount == comparison_total_amount;

    let mut blockers = Vec::new();
    for row in &candidate.boq_rows {
        if matches!(
            row.disposition,
            BoqRowDisposition::Missing | BoqRowDisposition::Blocked
        ) {
            blockers.push(format!("boq_row_{}", row.disposition.as_str()));
        }
        for query in &row.affected_queries {
            if !query_decisions
                .get(query)
                .and_then(|decision| decision.as_ref())
                .is_some_and(|decision| decision.treatment.permits_dependent_work())
            {
                blockers.push("unresolved_affected_query".into());
            }
        }
    }
    if !candidate.gaps.is_empty() {
        blockers.push("basis_gaps".into());
    }
    if !reconciled {
        blockers.push("calculation_reconciliation".into());
    }
    blockers.sort();
    blockers.dedup();
    Ok(NormalizedEstimate {
        material_assumptions,
        aggregate_inputs,
        total_amount,
        total_currency: comparison_total_run.output_currency.clone(),
        complete: blockers.is_empty(),
        reconciled,
        blockers,
    })
}

fn candidate_materially_remediates_prior_review(
    connection: &rusqlite::Connection,
    candidate: &BasisOfEstimateCandidate,
    target: &EstimateTaskTarget,
) -> Result<bool, TenderCommandError> {
    let Some(review_manifest_sha256) = &target.remediates_review_manifest_sha256 else {
        return Ok(true);
    };
    if target.basis_version <= 1 {
        return Ok(false);
    }
    let (manifest_json, stored_review_hash): (String, String) = connection
        .query_row(
            "SELECT versions.manifest_json, reviews.manifest_sha256
             FROM basis_of_estimate_versions AS versions
             JOIN basis_of_estimate_reviews AS reviews
               ON reviews.basis_id = versions.basis_id
              AND reviews.basis_version = versions.version
              AND reviews.outcome = 'failed'
             WHERE versions.basis_id = ?1 AND versions.version = ?2",
            params![target.basis_id, target.basis_version - 1],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    if stored_review_hash != *review_manifest_sha256 {
        return Ok(false);
    }
    let prior: BasisOfEstimateManifest = parse_canonical(&manifest_json)?;
    let (prior_candidate, _) = candidate_and_target_from_manifest(&prior);
    Ok(&prior_candidate != candidate)
}

fn material_assumption(decision: &ApprovedQueryTreatment) -> EstimateMaterialAssumption {
    EstimateMaterialAssumption {
        query_id: decision.query_id.clone(),
        query_version: decision.query_version,
        decision_id: decision.decision_id.clone(),
        treatment: decision.treatment,
        rationale: decision.rationale.clone(),
        treatment_details: decision.treatment_details.clone(),
        manifest_sha256: decision.manifest_sha256.clone(),
    }
}

pub(crate) struct ActiveEstimateProfile {
    pub(crate) tender_revision: u32,
    pub(crate) tender_name: String,
    pub(crate) plan_id: String,
    pub(crate) plan_version: u32,
    pub(crate) profile: AgentProfileVersionView,
    pub(crate) task_keys: Vec<String>,
}

fn estimate_profile_task_keys(
    tasks_json: &str,
    profile_id: &str,
    profile_version: u32,
) -> Result<Vec<String>, TenderCommandError> {
    let tasks: Vec<WorkPlanTask> = parse_canonical(tasks_json)?;
    let task_keys = tasks
        .into_iter()
        .filter(|task| {
            (task.profile_id == profile_id && task.profile_version == profile_version)
                || (task.review_profile_id.as_deref() == Some(profile_id)
                    && task.review_profile_version == Some(profile_version))
        })
        .map(|task| task.task_key)
        .collect::<Vec<_>>();
    if task_keys.is_empty() || task_keys.len() > 32 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(task_keys)
}

fn estimate_task_keys_for_plan(
    connection: &rusqlite::Connection,
    plan_id: &str,
    plan_version: u32,
    profile_id: &str,
    profile_version: u32,
) -> Result<Vec<String>, TenderCommandError> {
    let tasks_json: String = connection
        .query_row(
            "SELECT tasks_json FROM work_plan_versions WHERE plan_id = ?1 AND version = ?2",
            params![plan_id, plan_version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    estimate_profile_task_keys(&tasks_json, profile_id, profile_version)
}

pub(crate) fn active_estimate_profile(
    connection: &rusqlite::Connection,
    capability: &str,
    excluded_profile_id: Option<&str>,
) -> Result<ActiveEstimateProfile, TenderCommandError> {
    let unresolved_indeterminate: bool = connection
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
    if unresolved_indeterminate {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let tender_revision: u32 = connection
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let tender_name: String = connection
        .query_row(
            "SELECT name FROM tender_revisions WHERE revision = ?1",
            [tender_revision],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let (plan_id, plan_version, profiles_json, tasks_json): (String, u32, String, String) =
        connection
            .query_row(
                "SELECT activations.plan_id, activations.plan_version,
                    plans.profiles_json, plans.tasks_json
             FROM production_activations AS activations
             JOIN work_plan_versions AS plans
               ON plans.plan_id = activations.plan_id AND plans.version = activations.plan_version
             JOIN work_plan_approvals AS approvals
               ON approvals.plan_id = activations.plan_id
              AND approvals.plan_version = activations.plan_version
              AND approvals.decision = 'approve'
             WHERE activations.status = 'active'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let plan_profiles: Vec<WorkPlanProfileBinding> = parse_canonical(&profiles_json)?;
    let approved_profile = plan_profiles
        .iter()
        .map(|binding| &binding.profile)
        .find(|profile| {
            excluded_profile_id != Some(profile.profile_id.as_str())
                && profile
                    .capabilities
                    .iter()
                    .any(|candidate| candidate == capability)
        })
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let profile = load_profile(
        connection,
        (
            approved_profile.profile_id.clone(),
            approved_profile.version,
        ),
    )?;
    let (active, busy): (bool, bool) = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM agent_profile_heads
               WHERE profile_id = ?1 AND current_version = ?2 AND status = 'active'
             ), EXISTS(
               SELECT 1 FROM agent_runs
               WHERE profile_id = ?1 AND profile_version = ?2 AND status = 'running'
             )",
            params![profile.profile_id, profile.version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    if !active || busy || profile != *approved_profile {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let task_keys =
        estimate_profile_task_keys(&tasks_json, profile.profile_id.as_str(), profile.version)?;
    Ok(ActiveEstimateProfile {
        tender_revision,
        tender_name,
        plan_id,
        plan_version,
        profile,
        task_keys,
    })
}

fn derive_estimate_query_references(
    connection: &rusqlite::Connection,
    task_keys: &[String],
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<EstimateQueryReference>, TenderCommandError> {
    let task_keys_json = canonical_json(&task_keys.to_vec())?;
    let mut statement = connection
        .prepare(
            "SELECT heads.query_id, heads.current_version
             FROM tender_query_heads AS heads
             JOIN tender_query_versions AS versions
               ON versions.query_id = heads.query_id
              AND versions.version = heads.current_version
             WHERE EXISTS (
                 SELECT 1 FROM json_each(versions.affected_task_keys_json) AS affected
                 WHERE affected.value = '*'
                    OR affected.value IN (SELECT value FROM json_each(?1))
               )
                OR EXISTS (
                 SELECT 1
                 FROM tender_query_target_invalidations AS invalidations
                 JOIN production_tasks AS tasks
                   ON invalidations.target_kind = 'production_task'
                  AND invalidations.target_id = tasks.production_task_id
                 WHERE invalidations.query_id = versions.query_id
                   AND invalidations.query_version = versions.version
                   AND tasks.task_key IN (SELECT value FROM json_each(?1))
               )
             ORDER BY heads.query_id",
        )
        .map_err(sql_error)?;
    let mut rows = statement.query([task_keys_json]).map_err(sql_error)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(sql_error)? {
        check()?;
        if result.len() == MAX_QUERIES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        result.push(EstimateQueryReference {
            query_id: row.get(0).map_err(sql_error)?,
            version: row.get(1).map_err(sql_error)?,
        });
    }
    Ok(result)
}

fn canonical_estimate_query_references(
    references: &[EstimateQueryReference],
) -> Result<Vec<EstimateQueryReference>, TenderCommandError> {
    let mut references = references.to_vec();
    references.sort_by(|left, right| {
        (&left.query_id, left.version).cmp(&(&right.query_id, right.version))
    });
    if references.len() > MAX_QUERIES
        || references
            .windows(2)
            .any(|pair| pair.first() == pair.get(1))
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(references)
}

fn observe_estimate_queries(
    connection: &rusqlite::Connection,
    references: &[EstimateQueryReference],
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<EstimateQueryObservation>, TenderCommandError> {
    canonical_estimate_query_references(references)?
        .into_iter()
        .map(|reference| {
            check()?;
            Ok(EstimateQueryObservation {
                treatment_decision_manifest_sha256: load_query_decision(
                    connection,
                    &reference.query_id,
                    reference.version,
                )?
                .map(|decision| decision.manifest_sha256),
                query_id: reference.query_id,
                version: reference.version,
            })
        })
        .collect()
}

fn canonical_estimate_query_inventory(
    inventory: &[EstimateQueryObservation],
) -> Result<Vec<EstimateQueryObservation>, TenderCommandError> {
    let mut inventory = inventory.to_vec();
    inventory.sort_by(|left, right| {
        (&left.query_id, left.version).cmp(&(&right.query_id, right.version))
    });
    if inventory.len() > MAX_QUERIES
        || inventory.windows(2).any(|pair| {
            pair.first().map(|item| (&item.query_id, item.version))
                == pair.get(1).map(|item| (&item.query_id, item.version))
        })
        || inventory.iter().any(|observation| {
            !valid_identifier(&observation.query_id)
                || observation.version == 0
                || observation.version > 32
                || observation
                    .treatment_decision_manifest_sha256
                    .as_deref()
                    .is_some_and(|digest| {
                        digest.len() != 64
                            || !digest
                                .bytes()
                                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    })
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(inventory)
}

fn estimate_query_inventory_sha256(
    inventory: &[EstimateQueryObservation],
) -> Result<String, TenderCommandError> {
    Ok(sha256_hex(
        canonical_json(&canonical_estimate_query_inventory(inventory)?)?.as_bytes(),
    ))
}

fn query_references_from_inventory(
    inventory: &[EstimateQueryObservation],
) -> Vec<EstimateQueryReference> {
    inventory
        .iter()
        .map(|observation| EstimateQueryReference {
            query_id: observation.query_id.clone(),
            version: observation.version,
        })
        .collect()
}

pub(crate) struct PlannedRunRequest<'a> {
    pub(crate) tender_id: &'a TenderId,
    pub(crate) root: &'a Path,
    pub(crate) run_id: &'a str,
    pub(crate) plan_version: u32,
    pub(crate) tender_revision: u32,
    pub(crate) profile: &'a AgentProfileVersionView,
    pub(crate) task: &'a TenderTaskView,
    pub(crate) payload: &'a Value,
    pub(crate) created_at: &'a str,
    pub(crate) deadline: &'a str,
    pub(crate) started_event_summary: &'a str,
    pub(crate) audit_event_type: &'a str,
    pub(crate) audit_payload: Value,
}

pub(crate) fn insert_planned_run(
    transaction: &Transaction<'_>,
    request: PlannedRunRequest<'_>,
) -> Result<PreparedAgentRun, TenderCommandError> {
    let application_home = request
        .root
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let provider_selection =
        crate::application_settings::load_current_ai_execution_selection(application_home)?;
    let workspace = application_home.join("staging").join(format!(
        "agent-{}-{}",
        request.tender_id.as_str(),
        request.run_id
    ));
    insert_task(transaction, request.task, request.created_at)?;
    let (permission_grant, materialized_workspace) =
        derive_planned_task_grant(PlannedTaskGrantRequest {
            run_id: request.run_id,
            grant_id: random_identifier(transaction)?,
            application_home,
            tender_id: request.tender_id.as_str(),
            work_plan_version: request.plan_version,
            profile: request.profile,
            task: request.task,
            issued_at: request.created_at,
            expires_at: request.deadline,
            payload: request.payload,
        })?;
    if materialized_workspace != workspace
        || permission_duration(&permission_grant, Timestamp::now())
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
            params![request.profile.profile_id, request.profile.version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let (provider_thread_ref, provider_thread_to_archive) = match existing_thread {
        Some((thread_ref, status)) if status == "archive_pending" => (None, Some(thread_ref)),
        Some((thread_ref, status)) if status == "active" => {
            let exposure = load_thread_exposure(transaction, &thread_ref)?;
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
                append_audit_event_with_sequence(
                    transaction,
                    request.tender_id.as_str(),
                    "provider_thread_archive_requested",
                    request.tender_revision,
                    json!({
                        "reason": "thread_exposure_incompatible",
                        "run_id": request.run_id,
                        "thread_ref": thread_ref,
                    }),
                    request.created_at,
                )?;
                (None, Some(thread_ref))
            }
        }
        Some(_) => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        None => (None, None),
    };
    ensure_agent_run_capacity(transaction)?;
    transaction
        .execute(
            "INSERT INTO agent_runs (
               run_id, task_id, profile_id, profile_version,
               permission_grant_json, status, started_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
            params![
                request.run_id,
                request.task.task_id,
                request.profile.profile_id,
                request.profile.version,
                canonical_json(&permission_grant)?,
                request.created_at,
            ],
        )
        .map_err(sql_error)?;
    super::record_agent_run_provider_binding(
        transaction,
        request.run_id,
        &provider_selection,
        request.created_at,
    )?;
    insert_event(
        transaction,
        request.run_id,
        1,
        PendingProviderEvent {
            kind: ProviderEventKind::RunStarted,
            summary: request.started_event_summary.into(),
            correlation_id: None,
            request_fingerprint: None,
            denial_reason: None,
            opaque_reference: None,
        },
        request.created_at,
    )?;
    append_audit_event_with_sequence(
        transaction,
        request.tender_id.as_str(),
        request.audit_event_type,
        request.tender_revision,
        request.audit_payload,
        request.created_at,
    )?;
    Ok(PreparedAgentRun {
        run_id: request.run_id.into(),
        provider_selection,
        profile: request.profile.clone(),
        task: request.task.clone(),
        permission_grant,
        provider_thread_ref,
        provider_thread_to_archive,
        workspace,
    })
}

#[derive(Clone, Copy)]
pub(crate) enum EstimatePlanAssignment {
    Author,
    Reviewer,
}

pub(crate) struct EstimateRunIntegrityRequest<'a> {
    pub run_id: &'a str,
    pub expected_profile: &'a AgentProfileVersionView,
    pub expected_task: &'a TenderTaskView,
    pub expected_payload: &'a Value,
    pub expected_candidate_json: &'a str,
    pub result_created_at: &'a str,
    pub plan_id: &'a str,
    pub plan_version: u32,
    pub capability: &'a str,
    pub assignment: EstimatePlanAssignment,
    pub started_event_type: &'a str,
    pub expected_started_change: Value,
}

pub(crate) fn estimate_run_envelope_is_valid(
    connection: &rusqlite::Connection,
    request: EstimateRunIntegrityRequest<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    type RunBasis = (
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
    );
    check()?;
    let run: Option<RunBasis> = connection
        .query_row(
            "SELECT runs.status, runs.profile_id, runs.profile_version, runs.task_id,
                    runs.permission_grant_json, runs.started_at,
                    results.verification_status, results.payload_json,
                    results.data_scopes_json, results.data_classification,
                    results.created_at
             FROM agent_runs AS runs
             JOIN proposed_agent_results AS results ON results.run_id = runs.run_id
             WHERE runs.run_id = ?1",
            [request.run_id],
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
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some((
        status,
        profile_id,
        profile_version,
        task_id,
        permission_grant_json,
        run_started_at,
        result_status,
        result_payload_json,
        result_scopes_json,
        result_classification,
        result_created_at,
    )) = run
    else {
        return Ok(false);
    };
    let stored_profile = load_profile(connection, (profile_id.clone(), profile_version))?;
    let stored_task = load_task(connection, &task_id)?;
    let permission_grant: PermissionGrant = parse_canonical(&permission_grant_json)?;
    let result_scopes: Vec<String> = parse_canonical(&result_scopes_json)?;
    let result_classification = DataClassification::parse(&result_classification)?;
    let classification = *request
        .expected_profile
        .permissions
        .data_classifications
        .iter()
        .max()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let expected_view_sha256 = sha256_hex(canonical_json(request.expected_payload)?.as_bytes());
    let plan: Option<(String, String)> = connection
        .query_row(
            "SELECT plans.profiles_json, plans.tasks_json
             FROM work_plan_versions AS plans
             JOIN work_plan_approvals AS approvals
               ON approvals.plan_id = plans.plan_id
              AND approvals.plan_version = plans.version
              AND approvals.decision = 'approve'
             WHERE plans.plan_id = ?1 AND plans.version = ?2",
            params![request.plan_id, request.plan_version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let Some((profiles_json, tasks_json)) = plan else {
        return Ok(false);
    };
    let profiles: Vec<WorkPlanProfileBinding> = parse_canonical(&profiles_json)?;
    let tasks: Vec<WorkPlanTask> = parse_canonical(&tasks_json)?;
    let profile_is_approved = profiles.iter().any(|binding| {
        binding.profile == *request.expected_profile
            && binding
                .profile
                .capabilities
                .iter()
                .any(|capability| capability == request.capability)
    });
    let assignment_is_approved = tasks.iter().any(|task| match request.assignment {
        EstimatePlanAssignment::Author => {
            task.profile_id == profile_id && task.profile_version == profile_version
        }
        EstimatePlanAssignment::Reviewer => {
            task.review_profile_id.as_deref() == Some(profile_id.as_str())
                && task.review_profile_version == Some(profile_version)
        }
    });
    let started: (u32, Option<String>) = connection
        .query_row(
            "SELECT COUNT(*), MIN(payload_json) FROM audit_events
             WHERE event_type = ?1
               AND json_extract(payload_json, '$.change.run_id') = ?2",
            params![request.started_event_type, request.run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    let started_payload: Option<Value> = started
        .1
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let data_view = permission_grant.data_views.first();
    check()?;
    Ok(status == "completed"
        && profile_id == request.expected_profile.profile_id
        && profile_version == request.expected_profile.version
        && stored_profile == *request.expected_profile
        && task_id == request.expected_task.task_id
        && stored_task == *request.expected_task
        && result_status == "proposed"
        && result_payload_json == request.expected_candidate_json
        && result_created_at == request.result_created_at
        && result_scopes == permission_grant.data_scopes
        && result_classification == classification
        && profile_is_approved
        && assignment_is_approved
        && permission_grant.profile_id == profile_id
        && permission_grant.profile_version == profile_version
        && permission_grant.task_id == task_id
        && permission_grant.work_plan_version == request.plan_version
        && permission_grant.purpose == stored_task.objective
        && permission_grant.data_scopes == stored_profile.permissions.data_scopes
        && permission_grant.data_classifications == stored_profile.permissions.data_classifications
        && permission_grant.allowed_actions == stored_profile.permissions.allowed_actions
        && permission_grant.typed_tools.is_empty()
        && !permission_grant.network_allowed
        && permission_grant.workspace_write_allowed
        && permission_grant.thread_exposure == ThreadExposureSet::from_grant(&permission_grant)
        && permission_grant.workspace.workspace_id == request.run_id
        && permission_grant.workspace.read_only_inputs == "inputs"
        && permission_grant.workspace.working_area == "working"
        && permission_grant.workspace.staged_outputs == "outputs"
        && permission_grant.access_ceiling.exact_inputs == stored_task.exact_inputs
        && permission_grant.access_ceiling.data_scopes == stored_profile.permissions.data_scopes
        && permission_grant.access_ceiling.data_classifications
            == stored_profile.permissions.data_classifications
        && permission_grant.access_ceiling.allowed_actions
            == stored_profile.permissions.allowed_actions
        && permission_grant.access_ceiling.allowed_tools.is_empty()
        && permission_grant.resource_budget == stored_task.resource_budget
        && permission_grant.issued_at == run_started_at
        && permission_grant.expires_at == stored_task.deadline
        && permission_grant.data_views.len() == 1
        && data_view.is_some_and(|view| {
            view.exact_inputs == stored_task.exact_inputs
                && view.view_id == format!("production-task-{task_id}")
                && view.schema_version == 1
                && view.relative_path == "inputs/tender-metadata-v1.json"
                && view.sha256 == expected_view_sha256
                && view.data_scope == stored_profile.permissions.data_scopes.join("+")
                && view.data_classification == classification
        })
        && started.0 == 1
        && started_payload
            .as_ref()
            .and_then(|payload| payload.get("change"))
            == Some(&request.expected_started_change))
}

fn query_decision_at_observation(
    connection: &rusqlite::Connection,
    observation: &EstimateQueryObservation,
) -> Result<Option<ApprovedQueryTreatment>, TenderCommandError> {
    match &observation.treatment_decision_manifest_sha256 {
        Some(expected_manifest_sha256) => Ok(Some(
            load_query_decision(connection, &observation.query_id, observation.version)?
                .filter(|decision| decision.manifest_sha256 == *expected_manifest_sha256)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        )),
        None => Ok(None),
    }
}

fn estimate_query_view(
    connection: &rusqlite::Connection,
    observation: &EstimateQueryObservation,
) -> Result<Value, TenderCommandError> {
    let manifest: String = connection
        .query_row(
            "SELECT manifest_json FROM tender_query_versions WHERE query_id = ?1 AND version = ?2",
            params![observation.query_id, observation.version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let query: Value = serde_json::from_str(&manifest)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let approved_treatment = query_decision_at_observation(connection, observation)?;
    Ok(json!({
        "query": query,
        "approved_treatment": approved_treatment,
    }))
}

struct BasisAuthorPayloadRequest<'a> {
    target: &'a EstimateTaskTarget,
    prior_basis: Option<&'a BasisOfEstimateVersion>,
    calculation_runs: &'a [ControlledBoqCalculationRun],
    queries: &'a [Value],
    profile: &'a AgentProfileVersionView,
    tender_name: &'a str,
}

fn basis_author_payload(
    connection: &rusqlite::Connection,
    request: BasisAuthorPayloadRequest<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Value, TenderCommandError> {
    let mut boq_row_views = Vec::with_capacity(request.target.boq_rows.len());
    for row in &request.target.boq_rows {
        check()?;
        let inputs = row
            .evidence
            .iter()
            .map(evidence_reference)
            .collect::<Vec<_>>();
        boq_row_views.push(json!({
            "row_key": row.row_key,
            "evidence": calculation_evidence_view_for_estimate(connection, &inputs, check)?,
        }));
    }
    let quotation_inputs = request
        .target
        .quotation_evidence
        .iter()
        .map(evidence_reference)
        .collect::<Vec<_>>();
    let classification = *request
        .profile
        .permissions
        .data_classifications
        .iter()
        .max()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(json!({
        "basis_identity": {
            "basis_id": request.target.basis_id,
            "version": request.target.basis_version,
        },
        "superseded_basis": request.prior_basis,
        "boq_rows": boq_row_views,
        "quotation_evidence": calculation_evidence_view_for_estimate(
            connection,
            &quotation_inputs,
            check,
        )?,
        "approved_calculation_runs": request.calculation_runs,
        "tender_queries": request.queries,
        "data_classification": classification,
        "data_scope": request.profile.permissions.data_scopes.join("+"),
        "rules": {
            "account_every_host_designated_boq_row_once": true,
            "approved_calculation_runs_are_sole_arithmetic_authority": true,
            "cost_estimator_cannot_approve": true,
            "missing_or_blocked_rows_require_affected_queries": true,
        },
        "tender": {
            "tender_id": request.target.tender_id,
            "name": request.tender_name,
            "revision": request.target.tender_revision,
        },
    }))
}

struct BasisReviewPayloadRequest<'a> {
    tender_id: &'a str,
    tender_name: &'a str,
    basis: &'a BasisOfEstimateVersion,
    calculations: &'a [ControlledBoqCalculationRun],
    query_views: &'a [Value],
    profile: &'a AgentProfileVersionView,
}

fn historical_basis_snapshot(mut basis: BasisOfEstimateVersion) -> BasisOfEstimateVersion {
    basis.current = false;
    basis.relied_upon = false;
    basis
}

fn review_target_snapshot(mut basis: BasisOfEstimateVersion) -> BasisOfEstimateVersion {
    basis.current = true;
    basis.review = None;
    basis.approval = None;
    basis.relied_upon = false;
    basis.aggregate_calculation.approved_for_reliance = false;
    basis
}

fn basis_review_payload(
    connection: &rusqlite::Connection,
    request: BasisReviewPayloadRequest<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Value, TenderCommandError> {
    let boq_inputs = request
        .basis
        .boq_rows
        .iter()
        .flat_map(|row| row.evidence.iter().map(evidence_reference))
        .collect::<Vec<_>>();
    let quotation_inputs = request
        .basis
        .quotations
        .iter()
        .map(|quote| evidence_reference(&quote.evidence))
        .collect::<Vec<_>>();
    let classification = *request
        .profile
        .permissions
        .data_classifications
        .iter()
        .max()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(json!({
        "basis_of_estimate": request.basis,
        "approved_calculation_runs": request.calculations,
        "boq_evidence": calculation_evidence_view_for_estimate(connection, &boq_inputs, check)?,
        "quotation_evidence": calculation_evidence_view_for_estimate(
            connection,
            &quotation_inputs,
            check,
        )?,
        "tender_queries": request.query_views,
        "data_classification": classification,
        "data_scope": request.profile.permissions.data_scopes.join("+"),
        "review_rules": {
            "edit_target_allowed": false,
            "approval_allowed": false,
            "reproduce_every_calculation": true,
            "verify_boq_quote_query_and_reconciliation": true,
        },
        "tender": {
            "tender_id": request.tender_id,
            "name": request.tender_name,
            "revision": request.basis.tender_revision,
        },
    }))
}

impl TenderStore {
    pub(crate) fn record_estimate_denial(
        &mut self,
        tender_id: &TenderId,
        command: &str,
        target_id: Option<&str>,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "estimate_command_denied",
            tender_revision,
            json!({ "command": command, "reason": reason, "target_id": target_id }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn designate_boq_table(
        &mut self,
        tender_id: &TenderId,
        command: &DesignateBoqTableCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<BoqTableDesignation, TenderCommandError> {
        if !self.active_change_replacement_is(&command.artifact_id, command.artifact_version)? {
            self.require_change_intake_writable()?;
        }
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let current_source: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM source_artifact_versions AS versions
                   WHERE versions.artifact_id = ?1 AND versions.version = ?2
                     AND versions.registration_state = 'registered'
                     AND NOT EXISTS (
                       SELECT 1 FROM source_relationships AS relationships
                       JOIN change_assessments AS assessments USING (relationship_id)
                       JOIN change_assessment_decisions AS decisions USING (assessment_id)
                       WHERE relationships.prior_artifact_id = versions.artifact_id
                         AND relationships.prior_version = versions.version
                         AND relationships.relationship_kind = 'replacement'
                         AND decisions.classification = 'material'
                     )
                 )",
                params![command.artifact_id, command.artifact_version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let row_count = table_row_count_with_check(
            &transaction,
            &command.artifact_id,
            command.artifact_version,
            command.table_number,
            &mut || budget.check(),
        )?;
        let existing: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM boq_table_designations
                   WHERE artifact_id = ?1 AND artifact_version = ?2 AND table_number = ?3
                 )",
                params![
                    command.artifact_id,
                    command.artifact_version,
                    command.table_number
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let designation_count: u32 = transaction
            .query_row("SELECT COUNT(*) FROM boq_table_designations", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        if !current_source
            || existing
            || designation_count >= MAX_BOQ_TABLES
            || row_count <= command.header_row_count
            || row_count.saturating_sub(command.header_row_count) as usize > MAX_BOQ_ROWS
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let designation_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = BoqTableDesignationManifest {
            schema_version: 1,
            designation_id: designation_id.clone(),
            artifact_id: command.artifact_id.clone(),
            artifact_version: command.artifact_version,
            table_number: command.table_number,
            header_row_count: command.header_row_count,
            designated_by: "engineer_user".into(),
            acting_role: "engineer_in_the_loop".into(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "boq_table_designated",
            current_tender_revision_for_estimate(&transaction)?,
            json!({
                "artifact_id": command.artifact_id,
                "artifact_version": command.artifact_version.to_string(),
                "designation_id": designation_id,
                "header_row_count": command.header_row_count.to_string(),
                "manifest_sha256": manifest_sha256,
                "table_number": command.table_number.to_string(),
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO boq_table_designations (
                   designation_id, artifact_id, artifact_version, table_number,
                   header_row_count, designated_by, acting_role, audit_sequence,
                   manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'engineer_user',
                           'engineer_in_the_loop', ?6, ?7, ?8, ?9)",
                params![
                    designation_id,
                    command.artifact_id,
                    command.artifact_version,
                    command.table_number,
                    command.header_row_count,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        budget.check()?;
        load_boq_table_designation_with_check(
            &self.connection,
            &command.artifact_id,
            command.artifact_version,
            command.table_number,
            &mut || budget.check(),
        )?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
    }

    pub(crate) fn prepare_cost_estimator_basis_run(
        &mut self,
        tender_id: &TenderId,
        command: &RunCostEstimatorBasisCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        let mut change_authorized = false;
        for calculation_run_id in &command.calculation_run_ids {
            if self.active_change_allows_calculation_run(calculation_run_id)? {
                change_authorized = true;
                break;
            }
        }
        if !change_authorized {
            self.require_change_intake_writable()?;
        }
        validate_basis_command(command)?;
        budget.check()?;
        let run_id = random_identifier(&self.connection)?;
        let workspace = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .join("staging")
            .join(format!("agent-{}-{run_id}", tender_id.as_str()));
        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            budget.check()?;
            let active = active_estimate_profile(&transaction, COST_ESTIMATION_CAPABILITY, None)?;
            let basis_count: u32 = transaction
                .query_row(
                    "SELECT COUNT(*) FROM basis_of_estimate_versions",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if basis_count >= MAX_BASIS_VERSIONS {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let current: Option<(String, u32)> = transaction
                .query_row(
                    "SELECT basis_id, current_version FROM basis_of_estimate_heads LIMIT 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let prior_basis = current
                .as_ref()
                .map(|(basis_id, version)| {
                    load_basis_version_with_check(&transaction, basis_id, *version, &mut || {
                        budget.check()
                    })
                })
                .transpose()?;
            if prior_basis
                .as_ref()
                .is_some_and(|basis| basis.current && basis.review.is_none())
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            for evidence in &command.quotation_evidence {
                budget.check()?;
                let evidence_exists = super::tender_queries::query_evidence_reference_exists(
                    &transaction,
                    &evidence_reference(evidence),
                )?;
                if !evidence_exists {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            let mut calculation_runs = Vec::with_capacity(command.calculation_run_ids.len());
            for calculation_run_id in &command.calculation_run_ids {
                budget.check()?;
                let inspected = approved_calculation_run_for_estimate(
                    &transaction,
                    calculation_run_id,
                    &mut || budget.check(),
                );
                let run = inspected?
                    .filter(|run| run.tender_revision == active.tender_revision)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                calculation_runs.push(run);
            }
            calculation_runs
                .sort_by(|left, right| left.calculation_run_id.cmp(&right.calculation_run_id));
            let calculation_run_ids = calculation_runs
                .iter()
                .map(|run| run.calculation_run_id.clone())
                .collect::<Vec<_>>();
            let mut quotation_evidence = command.quotation_evidence.clone();
            quotation_evidence.sort_by(|left, right| {
                (&left.artifact_id, left.version, left.ordinal).cmp(&(
                    &right.artifact_id,
                    right.version,
                    right.ordinal,
                ))
            });
            let query_references =
                derive_estimate_query_references(&transaction, &active.task_keys, &mut || {
                    budget.check()
                })?;
            let query_inventory =
                observe_estimate_queries(&transaction, &query_references, &mut || budget.check())?;
            let query_inventory_sha256 = estimate_query_inventory_sha256(&query_inventory)?;
            let mut queries = Vec::with_capacity(query_inventory.len());
            for observation in &query_inventory {
                budget.check()?;
                queries.push(estimate_query_view(&transaction, observation)?);
            }
            let (basis_id, basis_version) = current
                .map(|(id, version)| (id, version + 1))
                .unwrap_or((random_identifier(&transaction)?, 1));
            let boq_rows = derive_current_boq_inventory(&transaction, &mut || budget.check())?;
            let boq_inventory_sha256 = boq_inventory_sha256(&boq_rows)?;
            let target = EstimateTaskTarget {
                tender_id: tender_id.as_str().into(),
                tender_revision: active.tender_revision,
                plan_id: active.plan_id.clone(),
                plan_version: active.plan_version,
                basis_id,
                basis_version,
                supersedes_basis_manifest_sha256: prior_basis
                    .as_ref()
                    .map(|basis| basis.manifest_sha256.clone()),
                remediates_review_manifest_sha256: prior_basis.as_ref().and_then(|basis| {
                    basis
                        .review
                        .as_ref()
                        .filter(|review| review.outcome == BasisOfEstimateReviewOutcome::Failed)
                        .map(|review| review.manifest_sha256.clone())
                }),
                boq_rows,
                boq_inventory_sha256,
                quotation_evidence,
                calculation_run_ids,
                query_references,
                query_inventory,
                query_inventory_sha256,
            };
            let created_at = sqlite_timestamp(&transaction)?;
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = basis_task(
                random_identifier(&transaction)?,
                &target,
                &active.profile,
                deadline.clone(),
            )?;
            let prior_basis_payload = prior_basis.clone().map(historical_basis_snapshot);
            let payload = basis_author_payload(
                &transaction,
                BasisAuthorPayloadRequest {
                    target: &target,
                    prior_basis: prior_basis_payload.as_ref(),
                    calculation_runs: &calculation_runs,
                    queries: &queries,
                    profile: &active.profile,
                    tender_name: &active.tender_name,
                },
                &mut || budget.check(),
            )?;
            if canonical_json(&payload)?.len() > MAX_BASIS_PAYLOAD_BYTES {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let prepared = insert_planned_run(
                &transaction,
                PlannedRunRequest {
                    tender_id,
                    root: &self.root,
                    run_id: &run_id,
                    plan_version: active.plan_version,
                    tender_revision: active.tender_revision,
                    profile: &active.profile,
                    task: &task,
                    payload: &payload,
                    created_at: &created_at,
                    deadline: &deadline,
                    started_event_summary: "Cost Estimator Basis of Estimate run started",
                    audit_event_type: "basis_of_estimate_started",
                    audit_payload: json!({
                        "basis_id": target.basis_id,
                        "basis_version": target.basis_version.to_string(),
                        "profile_id": active.profile.profile_id,
                        "profile_version": active.profile.version.to_string(),
                        "run_id": run_id,
                        "task_id": task.task_id,
                    }),
                },
            )?;
            transaction.commit().map_err(sql_error)?;
            Ok(prepared)
        })();
        if prepared.is_err() {
            let _ = fs::remove_dir_all(&workspace);
        }
        prepared
    }

    pub(crate) fn validate_basis_candidate(
        &self,
        task: &TenderTaskView,
        payload: &str,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<BasisOfEstimateCandidate, TenderCommandError> {
        if payload.len() > MAX_BASIS_PAYLOAD_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut target = exact_basis_target(task)?;
        let candidate: BasisOfEstimateCandidate = serde_json::from_str(payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        validate_candidate_shape(&candidate, &target)?;
        target.query_inventory =
            observe_estimate_queries(&self.connection, &target.query_references, check)?;
        if estimate_query_inventory_sha256(&target.query_inventory)?
            != target.query_inventory_sha256
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        normalize_candidate(&self.connection, &candidate, &target, true, check)?;
        if !candidate_materially_remediates_prior_review(&self.connection, &candidate, &target)? {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(candidate)
    }

    pub(crate) fn basis_target_is_open(
        &self,
        task: &TenderTaskView,
        run_id: &str,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        if !basis_target_is_open(&self.connection, task, run_id)? {
            return Ok(false);
        }
        let target = exact_basis_target(task)?;
        estimate_target_dependencies_are_current(
            &self.connection,
            &target,
            &task.profile_id,
            task.profile_version,
            check,
        )
    }
}

fn basis_target_is_open(
    connection: &rusqlite::Connection,
    task: &TenderTaskView,
    run_id: &str,
) -> Result<bool, TenderCommandError> {
    let target = exact_basis_target(task)?;
    let current_revision: u32 = connection
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let expected_version: u32 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM basis_of_estimate_versions",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let plan_and_profile_active: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM production_activations AS activations
               JOIN agent_profile_heads AS profiles
                 ON profiles.profile_id = ?3
                AND profiles.current_version = ?4
                AND profiles.status = 'active'
               WHERE activations.plan_id = ?1 AND activations.plan_version = ?2
                 AND activations.status = 'active'
             )",
            params![
                target.plan_id,
                target.plan_version,
                task.profile_id,
                task.profile_version
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let already_published: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM basis_of_estimate_versions WHERE author_run_id = ?1)",
            [run_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let prior_basis_matches = if target.basis_version == 1 {
        target.supersedes_basis_manifest_sha256.is_none()
            && target.remediates_review_manifest_sha256.is_none()
    } else {
        let prior: Option<(String, Option<String>, Option<String>)> = connection
            .query_row(
                "SELECT versions.manifest_sha256, reviews.outcome, reviews.manifest_sha256
                 FROM basis_of_estimate_versions AS versions
                 LEFT JOIN basis_of_estimate_reviews AS reviews
                   ON reviews.basis_id = versions.basis_id
                  AND reviews.basis_version = versions.version
                 WHERE versions.basis_id = ?1 AND versions.version = ?2",
                params![target.basis_id, target.basis_version - 1],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        prior.is_some_and(
            |(manifest_sha256, review_outcome, review_manifest_sha256)| {
                target.supersedes_basis_manifest_sha256.as_deref() == Some(&manifest_sha256)
                    && target.remediates_review_manifest_sha256
                        == review_outcome
                            .filter(|outcome| outcome == "failed")
                            .and(review_manifest_sha256)
            },
        )
    };
    Ok(target.tender_revision == current_revision
        && target.basis_version == expected_version
        && prior_basis_matches
        && plan_and_profile_active
        && !already_published)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn publish_basis_of_estimate(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    author_run_id: &str,
    profile: &AgentProfileVersionView,
    task: &TenderTaskView,
    candidate: &BasisOfEstimateCandidate,
    created_at: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(), TenderCommandError> {
    check()?;
    if !profile
        .capabilities
        .iter()
        .any(|capability| capability == COST_ESTIMATION_CAPABILITY)
        || profile.profile_id != task.profile_id
        || profile.version != task.profile_version
        || !basis_target_is_open(transaction, task, author_run_id)?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut target = exact_basis_target(task)?;
    if target.tender_id != tender_id.as_str() || target.tender_revision != tender_revision {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    if !estimate_target_dependencies_are_current(
        transaction,
        &target,
        &profile.profile_id,
        profile.version,
        check,
    )? {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    if !candidate_materially_remediates_prior_review(transaction, candidate, &target)? {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    target.query_inventory =
        observe_estimate_queries(transaction, &target.query_references, check)?;
    if estimate_query_inventory_sha256(&target.query_inventory)? != target.query_inventory_sha256 {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let normalized = normalize_candidate(transaction, candidate, &target, true, check)?;
    let aggregate_run_id = random_identifier(transaction)?;
    let aggregate_calculation = record_estimate_aggregate_calculation(
        transaction,
        RecordEstimateAggregateCalculation {
            aggregate_run_id: &aggregate_run_id,
            author_run_id,
            comparison_total_calculation_run_id: &candidate.comparison_total_calculation_run_id,
            tender_revision,
            inputs: normalized.aggregate_inputs.clone(),
            tender_id: tender_id.as_str(),
            created_at,
        },
        check,
    )?;
    if aggregate_calculation.final_amount != normalized.total_amount
        || aggregate_calculation.currency != normalized.total_currency
        || aggregate_calculation.inputs != normalized.aggregate_inputs
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manifest = BasisOfEstimateManifest {
        schema_version: 1,
        basis_id: target.basis_id.clone(),
        version: target.basis_version,
        tender_revision,
        plan_id: target.plan_id,
        plan_version: target.plan_version,
        author_run_id: author_run_id.into(),
        author_profile_id: profile.profile_id.clone(),
        author_profile_version: profile.version,
        scope: candidate.scope.clone(),
        pricing_date: candidate.pricing_date.clone(),
        currencies: candidate.currencies.clone(),
        taxes: candidate.taxes.clone(),
        rate_sources: candidate.rate_sources.clone(),
        productivity: candidate.productivity.clone(),
        design_maturity: candidate.design_maturity.clone(),
        gaps: candidate.gaps.clone(),
        exclusions: candidate.exclusions.clone(),
        supersedes_basis_manifest_sha256: target.supersedes_basis_manifest_sha256.clone(),
        remediates_review_manifest_sha256: target.remediates_review_manifest_sha256.clone(),
        boq_inventory_sha256: target.boq_inventory_sha256.clone(),
        query_inventory_sha256: target.query_inventory_sha256,
        query_inventory: target.query_inventory,
        boq_rows: candidate.boq_rows.clone(),
        cbs_components: candidate.cbs_components.clone(),
        resource_build_ups: candidate.resource_build_ups.clone(),
        quotations: candidate.quotations.clone(),
        allowances: candidate.allowances.clone(),
        material_assumptions: normalized.material_assumptions,
        comparison_total_calculation_run_id: candidate.comparison_total_calculation_run_id.clone(),
        aggregate_calculation,
        total_amount: normalized.total_amount,
        total_currency: normalized.total_currency,
        complete: normalized.complete,
        reconciled: normalized.reconciled,
        blockers: normalized.blockers,
        created_at: created_at.into(),
    };
    let manifest_json = canonical_json(&manifest)?;
    if manifest_json.len() > MAX_BASIS_PAYLOAD_BYTES {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        "basis_of_estimate_recorded",
        tender_revision,
        json!({
            "author_profile_id": profile.profile_id,
            "author_profile_version": profile.version.to_string(),
            "author_run_id": author_run_id,
            "basis_id": manifest.basis_id,
            "basis_version": manifest.version.to_string(),
            "complete": manifest.complete,
            "manifest_sha256": manifest_sha256,
            "reconciled": manifest.reconciled,
        }),
        created_at,
    )?;
    if manifest.version == 1 {
        transaction
            .execute(
                "INSERT INTO basis_of_estimates (basis_id, created_at) VALUES (?1, ?2)",
                params![manifest.basis_id, created_at],
            )
            .map_err(sql_error)?;
    }
    transaction
        .execute(
            "INSERT INTO basis_of_estimate_versions (
               basis_id, version, tender_revision, author_run_id,
               author_profile_id, author_profile_version, complete, reconciled,
               audit_sequence, manifest_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                manifest.basis_id,
                manifest.version,
                manifest.tender_revision,
                manifest.author_run_id,
                manifest.author_profile_id,
                manifest.author_profile_version,
                manifest.complete,
                manifest.reconciled,
                audit_sequence,
                manifest_json,
                manifest_sha256,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    if manifest.version == 1 {
        transaction
            .execute(
                "INSERT INTO basis_of_estimate_heads (basis_id, current_version) VALUES (?1, 1)",
                [&manifest.basis_id],
            )
            .map_err(sql_error)?;
    } else if transaction
        .execute(
            "UPDATE basis_of_estimate_heads SET current_version = ?2
             WHERE basis_id = ?1 AND current_version = ?3",
            params![manifest.basis_id, manifest.version, manifest.version - 1],
        )
        .map_err(sql_error)?
        != 1
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn load_review(
    connection: &rusqlite::Connection,
    basis_id: &str,
    version: u32,
) -> Result<Option<BasisOfEstimateReview>, TenderCommandError> {
    connection
        .query_row(
            "SELECT review_id, reviewer_run_id, reviewer_profile_id,
                    reviewer_profile_version, outcome, findings_json,
                    manifest_sha256, created_at
             FROM basis_of_estimate_reviews
             WHERE basis_id = ?1 AND basis_version = ?2",
            params![basis_id, version],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .map(|row| {
            Ok(BasisOfEstimateReview {
                review_id: row.0,
                reviewer_run_id: row.1,
                reviewer_profile_id: row.2,
                reviewer_profile_version: row.3,
                outcome: match row.4.as_str() {
                    "passed" => BasisOfEstimateReviewOutcome::Passed,
                    "failed" => BasisOfEstimateReviewOutcome::Failed,
                    _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
                },
                findings: parse_canonical(&row.5)?,
                manifest_sha256: row.6,
                created_at: row.7,
            })
        })
        .transpose()
}

fn load_approval(
    connection: &rusqlite::Connection,
    basis_id: &str,
    version: u32,
) -> Result<Option<BasisOfEstimateApproval>, TenderCommandError> {
    connection
        .query_row(
            "SELECT approval_id, basis_manifest_sha256, review_id, rationale,
                    approved_by, acting_role, manifest_sha256, created_at
             FROM basis_of_estimate_approvals
             WHERE basis_id = ?1 AND basis_version = ?2",
            params![basis_id, version],
            |row| {
                Ok(BasisOfEstimateApproval {
                    approval_id: row.get(0)?,
                    basis_id: basis_id.into(),
                    basis_version: version,
                    basis_manifest_sha256: row.get(1)?,
                    review_id: row.get(2)?,
                    rationale: row.get(3)?,
                    approved_by: row.get(4)?,
                    acting_role: row.get(5)?,
                    manifest_sha256: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)
}

fn source_evidence_is_current(
    connection: &rusqlite::Connection,
    reference: &TenderEvidenceReference,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM evidence_locations AS locations
               WHERE locations.artifact_id = ?1
                 AND locations.version = ?2
                 AND locations.ordinal = ?3
                 AND NOT EXISTS (
                   SELECT 1 FROM source_relationships AS relationships
                   JOIN change_assessments AS assessments USING (relationship_id)
                   JOIN change_assessment_decisions AS decisions USING (assessment_id)
                   WHERE relationships.prior_artifact_id = locations.artifact_id
                     AND relationships.prior_version = locations.version
                     AND relationships.relationship_kind = 'replacement'
                     AND decisions.classification = 'material'
                 )
             )",
            params![reference.artifact_id, reference.version, reference.ordinal],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn input_evidence_is_current(
    connection: &rusqlite::Connection,
    reference: &AgentTaskInputReference,
) -> Result<bool, TenderCommandError> {
    if reference.kind != "source_evidence" {
        return Ok(false);
    }
    let (artifact_id, ordinal) = reference
        .reference
        .rsplit_once('#')
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let ordinal = ordinal
        .parse()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    source_evidence_is_current(
        connection,
        &TenderEvidenceReference {
            artifact_id: artifact_id.into(),
            version: reference.version,
            ordinal,
        },
    )
}

fn estimate_target_dependencies_are_current(
    connection: &rusqlite::Connection,
    target: &EstimateTaskTarget,
    profile_id: &str,
    profile_version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    check()?;
    let current_revision: u32 = connection
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if target.tender_revision != current_revision {
        return Ok(false);
    }
    let inventory = match derive_current_boq_inventory(connection, check) {
        Ok(inventory) => inventory,
        Err(error) if error.code == TenderErrorCode::InvalidCommand => return Ok(false),
        Err(error) => return Err(error),
    };
    if boq_inventory_sha256(&inventory)? != target.boq_inventory_sha256 {
        return Ok(false);
    }
    for reference in &target.quotation_evidence {
        check()?;
        if !source_evidence_is_current(connection, reference)? {
            return Ok(false);
        }
    }
    for run_id in &target.calculation_run_ids {
        check()?;
        let Some(run) = approved_calculation_run_for_estimate(connection, run_id, check)? else {
            return Ok(false);
        };
        if run.tender_revision != target.tender_revision {
            return Ok(false);
        }
        for reference in run
            .quantity
            .evidence
            .iter()
            .chain(run.unit_rate.evidence.iter())
            .chain(run.exchange_rate.evidence.iter())
        {
            check()?;
            if !input_evidence_is_current(connection, reference)? {
                return Ok(false);
            }
        }
    }
    let task_keys = estimate_task_keys_for_plan(
        connection,
        &target.plan_id,
        target.plan_version,
        profile_id,
        profile_version,
    )?;
    let current_queries = derive_estimate_query_references(connection, &task_keys, check)?;
    let current_query_inventory = observe_estimate_queries(connection, &current_queries, check)?;
    Ok(estimate_query_inventory_sha256(&current_query_inventory)? == target.query_inventory_sha256)
}

fn basis_dependencies_are_current_with_check(
    connection: &rusqlite::Connection,
    manifest: &BasisOfEstimateManifest,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    check()?;
    let current_revision: u32 = connection
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if manifest.tender_revision != current_revision {
        return Ok(false);
    }
    let inventory = match derive_current_boq_inventory(connection, check) {
        Ok(inventory) => inventory,
        Err(error) if error.code == TenderErrorCode::InvalidCommand => return Ok(false),
        Err(error) => return Err(error),
    };
    if boq_inventory_sha256(&inventory)? != manifest.boq_inventory_sha256 {
        return Ok(false);
    }
    let task_keys = estimate_task_keys_for_plan(
        connection,
        &manifest.plan_id,
        manifest.plan_version,
        &manifest.author_profile_id,
        manifest.author_profile_version,
    )?;
    let current_queries = derive_estimate_query_references(connection, &task_keys, check)?;
    let current_query_inventory = observe_estimate_queries(connection, &current_queries, check)?;
    if estimate_query_inventory_sha256(&current_query_inventory)? != manifest.query_inventory_sha256
    {
        return Ok(false);
    }
    for quotation in &manifest.quotations {
        check()?;
        if !source_evidence_is_current(connection, &quotation.evidence)? {
            return Ok(false);
        }
    }
    let mut calculation_run_ids = HashSet::new();
    calculation_run_ids.insert(manifest.comparison_total_calculation_run_id.clone());
    for row in &manifest.boq_rows {
        if let Some(run_id) = &row.calculation_run_id {
            calculation_run_ids.insert(run_id.clone());
        }
    }
    for build_up in &manifest.resource_build_ups {
        calculation_run_ids.insert(build_up.calculation_run_id.clone());
    }
    for quotation in &manifest.quotations {
        calculation_run_ids.insert(quotation.normalization_calculation_run_id.clone());
    }
    for run_id in calculation_run_ids {
        check()?;
        let Some(run) = approved_calculation_run_for_estimate(connection, &run_id, check)? else {
            return Ok(false);
        };
        if run.tender_revision != current_revision {
            return Ok(false);
        }
        for reference in run
            .quantity
            .evidence
            .iter()
            .chain(run.unit_rate.evidence.iter())
            .chain(run.exchange_rate.evidence.iter())
        {
            check()?;
            if !input_evidence_is_current(connection, reference)? {
                return Ok(false);
            }
        }
    }
    for assumption in &manifest.material_assumptions {
        check()?;
        let current: bool = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM tender_query_heads AS heads
                   JOIN tender_query_treatment_decisions AS decisions
                     ON decisions.query_id = heads.query_id
                    AND decisions.query_version = heads.current_version
                   WHERE heads.query_id = ?1 AND heads.current_version = ?2
                     AND decisions.decision_id = ?3 AND decisions.manifest_sha256 = ?4
                 )",
                params![
                    assumption.query_id,
                    assumption.query_version,
                    assumption.decision_id,
                    assumption.manifest_sha256,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !current {
            return Ok(false);
        }
    }
    for row in &manifest.boq_rows {
        for query in &row.affected_queries {
            check()?;
            let current: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM tender_query_heads WHERE query_id = ?1 AND current_version = ?2)",
                    params![query.query_id, query.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if !current {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn basis_from_manifest_with_check(
    connection: &rusqlite::Connection,
    manifest: BasisOfEstimateManifest,
    manifest_sha256: String,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<BasisOfEstimateVersion, TenderCommandError> {
    check()?;
    let head: Option<u32> = connection
        .query_row(
            "SELECT current_version FROM basis_of_estimate_heads WHERE basis_id = ?1",
            [&manifest.basis_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let current = head == Some(manifest.version)
        && basis_dependencies_are_current_with_check(connection, &manifest, check)?;
    let review = load_review(connection, &manifest.basis_id, manifest.version)?;
    let approval = load_approval(connection, &manifest.basis_id, manifest.version)?;
    let mut aggregate_calculation = load_estimate_aggregate_calculation(
        connection,
        &manifest.aggregate_calculation.aggregate_run_id,
        check,
    )?
    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let aggregate_is_approved = aggregate_calculation.approved_for_reliance;
    aggregate_calculation.approved_for_reliance = false;
    if aggregate_calculation != manifest.aggregate_calculation {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    aggregate_calculation.approved_for_reliance = aggregate_is_approved;
    Ok(BasisOfEstimateVersion {
        basis_id: manifest.basis_id,
        version: manifest.version,
        tender_revision: manifest.tender_revision,
        author_run_id: manifest.author_run_id,
        author_profile_id: manifest.author_profile_id,
        author_profile_version: manifest.author_profile_version,
        scope: manifest.scope,
        pricing_date: manifest.pricing_date,
        currencies: manifest.currencies,
        taxes: manifest.taxes,
        rate_sources: manifest.rate_sources,
        productivity: manifest.productivity,
        design_maturity: manifest.design_maturity,
        gaps: manifest.gaps,
        exclusions: manifest.exclusions,
        supersedes_basis_manifest_sha256: manifest.supersedes_basis_manifest_sha256,
        remediates_review_manifest_sha256: manifest.remediates_review_manifest_sha256,
        boq_inventory_sha256: manifest.boq_inventory_sha256,
        query_inventory_sha256: manifest.query_inventory_sha256,
        query_inventory: manifest.query_inventory,
        boq_rows: manifest.boq_rows,
        cbs_components: manifest.cbs_components,
        resource_build_ups: manifest.resource_build_ups,
        quotations: manifest.quotations,
        allowances: manifest.allowances,
        material_assumptions: manifest.material_assumptions,
        comparison_total_calculation_run_id: manifest.comparison_total_calculation_run_id,
        aggregate_calculation,
        total_amount: manifest.total_amount,
        total_currency: manifest.total_currency,
        complete: manifest.complete,
        reconciled: manifest.reconciled,
        blockers: manifest.blockers,
        current,
        relied_upon: current && approval.is_some() && aggregate_is_approved,
        review,
        approval,
        manifest_sha256,
        created_at: manifest.created_at,
    })
}

fn load_basis_version(
    connection: &rusqlite::Connection,
    basis_id: &str,
    version: u32,
) -> Result<BasisOfEstimateVersion, TenderCommandError> {
    load_basis_version_with_check(connection, basis_id, version, &mut || Ok(()))
}

pub(crate) fn load_basis_version_with_check(
    connection: &rusqlite::Connection,
    basis_id: &str,
    version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<BasisOfEstimateVersion, TenderCommandError> {
    check()?;
    let (manifest_json, manifest_sha256): (String, String) = connection
        .query_row(
            "SELECT manifest_json, manifest_sha256 FROM basis_of_estimate_versions
             WHERE basis_id = ?1 AND version = ?2",
            params![basis_id, version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(manifest_json.as_bytes()) != manifest_sha256 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    basis_from_manifest_with_check(
        connection,
        parse_canonical(&manifest_json)?,
        manifest_sha256,
        check,
    )
}

impl TenderStore {
    pub(crate) fn load_basis_of_estimate(
        &self,
        basis_id: &str,
        version: u32,
    ) -> Result<BasisOfEstimateVersion, TenderCommandError> {
        load_basis_version(&self.connection, basis_id, version)
    }

    pub(crate) fn load_basis_for_author_run(
        &self,
        run_id: &str,
    ) -> Result<BasisOfEstimateVersion, TenderCommandError> {
        let key: Option<(String, u32)> = self
            .connection
            .query_row(
                "SELECT basis_id, version FROM basis_of_estimate_versions WHERE author_run_id = ?1",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let (basis_id, version) =
            key.ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        load_basis_version(&self.connection, &basis_id, version)
    }

    pub(crate) fn prepare_basis_review_run(
        &mut self,
        tender_id: &TenderId,
        basis_id: &str,
        version: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        if !self.active_change_allows_estimate(basis_id, version)? {
            self.require_change_intake_writable()?;
        }
        if !valid_identifier(basis_id) || version == 0 || version > MAX_BASIS_VERSIONS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        budget.check()?;
        let run_id = random_identifier(&self.connection)?;
        let workspace = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .join("staging")
            .join(format!("agent-{}-{run_id}", tender_id.as_str()));
        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            budget.check()?;
            let basis =
                load_basis_version_with_check(&transaction, basis_id, version, &mut || {
                    budget.check()
                })?;
            if !basis.current || basis.review.is_some() || basis.approval.is_some() {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let active = active_estimate_profile(
                &transaction,
                BASIS_REVIEW_CAPABILITY,
                Some(&basis.author_profile_id),
            )?;
            if active.tender_revision != basis.tender_revision {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = review_task(BasisReviewTaskRequest {
                task_id: random_identifier(&transaction)?,
                tender_id: tender_id.as_str(),
                tender_revision: basis.tender_revision,
                plan_id: &active.plan_id,
                plan_version: active.plan_version,
                basis: &basis,
                profile: &active.profile,
                deadline: deadline.clone(),
            })?;
            let mut run_ids = HashSet::new();
            run_ids.insert(basis.comparison_total_calculation_run_id.clone());
            for row in &basis.boq_rows {
                if let Some(id) = &row.calculation_run_id {
                    run_ids.insert(id.clone());
                }
            }
            for build_up in &basis.resource_build_ups {
                run_ids.insert(build_up.calculation_run_id.clone());
            }
            for quote in &basis.quotations {
                run_ids.insert(quote.normalization_calculation_run_id.clone());
            }
            let mut calculations = Vec::with_capacity(run_ids.len());
            for id in run_ids {
                budget.check()?;
                calculations.push(
                    approved_calculation_run_for_estimate(&transaction, &id, &mut || {
                        budget.check()
                    })?
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
                );
            }
            calculations
                .sort_by(|left, right| left.calculation_run_id.cmp(&right.calculation_run_id));
            let query_inventory = canonical_estimate_query_inventory(&basis.query_inventory)?;
            let mut query_views = Vec::with_capacity(query_inventory.len());
            for observation in &query_inventory {
                budget.check()?;
                query_views.push(estimate_query_view(&transaction, observation)?);
            }
            let review_basis = review_target_snapshot(basis.clone());
            let payload = basis_review_payload(
                &transaction,
                BasisReviewPayloadRequest {
                    tender_id: tender_id.as_str(),
                    tender_name: &active.tender_name,
                    basis: &review_basis,
                    calculations: &calculations,
                    query_views: &query_views,
                    profile: &active.profile,
                },
                &mut || budget.check(),
            )?;
            if canonical_json(&payload)?.len() > MAX_BASIS_PAYLOAD_BYTES {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let created_at = sqlite_timestamp(&transaction)?;
            let prepared = insert_planned_run(
                &transaction,
                PlannedRunRequest {
                    tender_id,
                    root: &self.root,
                    run_id: &run_id,
                    plan_version: active.plan_version,
                    tender_revision: active.tender_revision,
                    profile: &active.profile,
                    task: &task,
                    payload: &payload,
                    created_at: &created_at,
                    deadline: &deadline,
                    started_event_summary: "Independent Basis of Estimate review started",
                    audit_event_type: "basis_of_estimate_review_started",
                    audit_payload: json!({
                        "basis_id": basis.basis_id,
                        "basis_version": basis.version.to_string(),
                        "reviewer_profile_id": active.profile.profile_id,
                        "reviewer_profile_version": active.profile.version.to_string(),
                        "run_id": run_id,
                        "task_id": task.task_id,
                    }),
                },
            )?;
            transaction.commit().map_err(sql_error)?;
            Ok(prepared)
        })();
        if prepared.is_err() {
            let _ = fs::remove_dir_all(&workspace);
        }
        prepared
    }

    pub(crate) fn validate_basis_review_candidate(
        &self,
        task: &TenderTaskView,
        payload: &str,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<BasisOfEstimateReviewCandidate, TenderCommandError> {
        if payload.len() > 128 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (basis_id, version, _, _) = exact_review_target(task)?;
        let basis = load_basis_version_with_check(&self.connection, &basis_id, version, check)?;
        let candidate: BasisOfEstimateReviewCandidate = serde_json::from_str(payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !review_candidate_is_valid(
            &basis.boq_rows,
            basis.complete,
            basis.reconciled,
            &candidate,
        ) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(candidate)
    }

    pub(crate) fn basis_review_target_is_open(
        &self,
        task: &TenderTaskView,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        basis_review_target_is_open(&self.connection, task, check)
    }

    pub(crate) fn approve_basis_of_estimate(
        &mut self,
        tender_id: &TenderId,
        command: &ApproveBasisOfEstimateCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<BasisOfEstimateVersion, TenderCommandError> {
        if !self.active_change_allows_estimate(&command.basis_id, command.version)? {
            self.require_change_intake_writable()?;
        }
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let basis = load_basis_version_with_check(
            &transaction,
            &command.basis_id,
            command.version,
            &mut || budget.check(),
        )?;
        if !basis.current
            || !basis.complete
            || !basis.reconciled
            || basis.manifest_sha256 != command.manifest_sha256
            || basis.approval.is_some()
            || basis.review.as_ref().map(|review| review.outcome)
                != Some(BasisOfEstimateReviewOutcome::Passed)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (stored_manifest_json, stored_manifest_sha256): (String, String) = transaction
            .query_row(
                "SELECT manifest_json, manifest_sha256
                 FROM basis_of_estimate_versions
                 WHERE basis_id = ?1 AND version = ?2",
                params![command.basis_id, command.version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let stored_manifest: BasisOfEstimateManifest = parse_canonical(&stored_manifest_json)?;
        if stored_manifest_sha256 != command.manifest_sha256
            || stored_manifest_sha256 != sha256_hex(stored_manifest_json.as_bytes())
            || !basis_review_and_approval_are_valid(
                &transaction,
                &stored_manifest,
                &stored_manifest_sha256,
                &mut || budget.check(),
            )?
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (stored_candidate, mut stored_target) =
            candidate_and_target_from_manifest(&stored_manifest);
        stored_target.tender_id = tender_id.as_str().into();
        let normalized = normalize_candidate(
            &transaction,
            &stored_candidate,
            &stored_target,
            true,
            &mut || budget.check(),
        )?;
        if normalized.material_assumptions != stored_manifest.material_assumptions
            || normalized.aggregate_inputs != stored_manifest.aggregate_calculation.inputs
            || normalized.total_amount != stored_manifest.total_amount
            || normalized.total_currency != stored_manifest.total_currency
            || stored_manifest.aggregate_calculation.approved_for_reliance
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let review = basis
            .review
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let created_at = sqlite_timestamp(&transaction)?;
        let approval_id = random_identifier(&transaction)?;
        let manifest = BasisApprovalManifest {
            schema_version: 1,
            approval_id: approval_id.clone(),
            basis_id: basis.basis_id.clone(),
            basis_version: basis.version,
            basis_manifest_sha256: basis.manifest_sha256.clone(),
            review_id: review.review_id.clone(),
            review_manifest_sha256: review.manifest_sha256.clone(),
            aggregate_calculation_run_id: basis.aggregate_calculation.aggregate_run_id.clone(),
            aggregate_calculation_manifest_sha256: basis
                .aggregate_calculation
                .manifest_sha256
                .clone(),
            rationale: command.rationale.trim().into(),
            approved_by: "engineer_user".into(),
            acting_role: "engineer_in_the_loop".into(),
            tender_revision: basis.tender_revision,
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "basis_of_estimate_approved",
            basis.tender_revision,
            json!({
                "approval_id": approval_id,
                "basis_id": basis.basis_id,
                "basis_manifest_sha256": basis.manifest_sha256,
                "basis_version": basis.version.to_string(),
                "manifest_sha256": manifest_sha256,
                "review_id": review.review_id,
                "review_manifest_sha256": review.manifest_sha256,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO basis_of_estimate_approvals (
                   approval_id, basis_id, basis_version, basis_manifest_sha256,
                   review_id, review_manifest_sha256, rationale, approved_by,
                   acting_role, tender_revision, audit_sequence, manifest_json,
                   manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'engineer_user',
                           'engineer_in_the_loop', ?8, ?9, ?10, ?11, ?12)",
                params![
                    approval_id,
                    basis.basis_id,
                    basis.version,
                    basis.manifest_sha256,
                    review.review_id,
                    review.manifest_sha256,
                    command.rationale.trim(),
                    basis.tender_revision,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        approve_estimate_aggregate_calculation(
            &transaction,
            ApproveEstimateAggregateCalculation {
                aggregate_run_id: &basis.aggregate_calculation.aggregate_run_id,
                aggregate_manifest_sha256: &basis.aggregate_calculation.manifest_sha256,
                basis_id: &basis.basis_id,
                basis_version: basis.version,
                basis_manifest_sha256: &basis.manifest_sha256,
                rationale: command.rationale.trim(),
                tender_id: tender_id.as_str(),
                tender_revision: basis.tender_revision,
                created_at: &created_at,
            },
            &mut || budget.check(),
        )?;
        transaction.commit().map_err(sql_error)?;
        budget.check()?;
        load_basis_version_with_check(
            &self.connection,
            &command.basis_id,
            command.version,
            &mut || budget.check(),
        )
    }

    pub(crate) fn inspect_estimate_workspace(
        &self,
        basis_offset: u32,
        boq_candidate_cursor: Option<&str>,
        budget: BidPackageOperationBudget,
    ) -> Result<EstimateWorkspaceInspection, TenderCommandError> {
        budget.check()?;
        let count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM basis_of_estimate_versions",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let key: Option<(String, u32)> = self
            .connection
            .query_row(
                "SELECT basis_id, version FROM basis_of_estimate_versions
                 ORDER BY version DESC LIMIT 1 OFFSET ?1",
                [basis_offset],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let basis = key
            .map(|(basis_id, version)| {
                load_basis_version_with_check(&self.connection, &basis_id, version, &mut || {
                    budget.check()
                })
            })
            .transpose()?;
        let (boq_table_candidates, boq_table_candidate_next_cursor) =
            inspect_boq_table_candidates(&self.connection, boq_candidate_cursor, &mut || {
                budget.check()
            })?;
        Ok(EstimateWorkspaceInspection {
            basis,
            boq_table_candidates,
            boq_table_candidate_next_cursor,
            basis_offset,
            total_basis_version_count: count,
            has_newer_basis: basis_offset > 0,
            has_older_basis: basis_offset.saturating_add(1) < count,
        })
    }
}

fn basis_review_target_is_open(
    connection: &rusqlite::Connection,
    task: &TenderTaskView,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let (basis_id, version, plan_id, plan_version) = exact_review_target(task)?;
    let basis = match load_basis_version_with_check(connection, &basis_id, version, check) {
        Ok(basis) => basis,
        Err(error) if error.code == TenderErrorCode::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !basis.current || basis.review.is_some() || basis.approval.is_some() {
        return Ok(false);
    }
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM production_activations AS activations
               JOIN agent_profile_heads AS profiles
                 ON profiles.profile_id = ?3
                AND profiles.current_version = ?4
                AND profiles.status = 'active'
               WHERE activations.plan_id = ?1 AND activations.plan_version = ?2
                 AND activations.status = 'active'
             )",
            params![plan_id, plan_version, task.profile_id, task.profile_version],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn publish_basis_of_estimate_review(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    reviewer_run_id: &str,
    profile: &AgentProfileVersionView,
    task: &TenderTaskView,
    candidate: &BasisOfEstimateReviewCandidate,
    created_at: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(), TenderCommandError> {
    check()?;
    if !profile
        .capabilities
        .iter()
        .any(|capability| capability == BASIS_REVIEW_CAPABILITY)
        || profile.profile_id != task.profile_id
        || profile.version != task.profile_version
        || !basis_review_target_is_open(transaction, task, check)?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let (basis_id, version, _, _) = exact_review_target(task)?;
    let basis = load_basis_version_with_check(transaction, &basis_id, version, check)?;
    if basis.tender_revision != tender_revision || basis.author_profile_id == profile.profile_id {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    if !review_candidate_is_valid(&basis.boq_rows, basis.complete, basis.reconciled, candidate) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let review_id = random_identifier(transaction)?;
    let manifest = BasisReviewManifest {
        schema_version: 1,
        review_id: review_id.clone(),
        basis_id: basis.basis_id.clone(),
        basis_version: basis.version,
        basis_manifest_sha256: basis.manifest_sha256.clone(),
        reviewer_run_id: reviewer_run_id.into(),
        reviewer_profile_id: profile.profile_id.clone(),
        reviewer_profile_version: profile.version,
        outcome: candidate.outcome,
        findings: candidate.findings.clone(),
        created_at: created_at.into(),
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let findings_json = canonical_json(&candidate.findings)?;
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        "basis_of_estimate_review_completed",
        tender_revision,
        json!({
            "basis_id": basis.basis_id,
            "basis_manifest_sha256": basis.manifest_sha256,
            "basis_version": basis.version.to_string(),
            "manifest_sha256": manifest_sha256,
            "outcome": candidate.outcome.as_str(),
            "review_id": review_id,
            "reviewer_profile_id": profile.profile_id,
            "reviewer_profile_version": profile.version.to_string(),
            "reviewer_run_id": reviewer_run_id,
        }),
        created_at,
    )?;
    transaction
        .execute(
            "INSERT INTO basis_of_estimate_reviews (
               review_id, basis_id, basis_version, basis_manifest_sha256,
               reviewer_run_id, reviewer_profile_id, reviewer_profile_version,
               outcome, findings_json, audit_sequence, manifest_json,
               manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                review_id,
                basis.basis_id,
                basis.version,
                basis.manifest_sha256,
                reviewer_run_id,
                profile.profile_id,
                profile.version,
                candidate.outcome.as_str(),
                findings_json,
                audit_sequence,
                manifest_json,
                manifest_sha256,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

impl QuantixHost {
    pub fn designate_boq_table(
        &self,
        command: DesignateBoqTableCommand,
    ) -> Result<BoqTableDesignation, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .designate_boq_table(&tender_id, &command, budget);
        result
    }

    pub fn approve_basis_of_estimate(
        &self,
        command: ApproveBasisOfEstimateCommand,
    ) -> Result<BasisOfEstimateVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_estimate_denial(
                &tender_id,
                "approve_basis_of_estimate",
                Some(&command.basis_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.approve_basis_of_estimate(&tender_id, &command, budget) {
            Ok(basis) => Ok(basis),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_estimate_denial(
                        &tender_id,
                        "approve_basis_of_estimate",
                        Some(&command.basis_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn inspect_estimate_workspace(
        &self,
        command: InspectEstimateWorkspaceCommand,
    ) -> Result<EstimateWorkspaceInspection, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_estimate_workspace(
                command.basis_offset,
                command.boq_candidate_cursor.as_deref(),
                budget,
            );
        result
    }
}

pub(crate) fn audit_is_exact(
    connection: &rusqlite::Connection,
    sequence: i64,
    event_type: &str,
    created_at: &str,
    expected_payload: &Value,
) -> Result<bool, TenderCommandError> {
    let row: Option<(String, String, String)> = connection
        .query_row(
            "SELECT event_type, payload_json, created_at FROM audit_events WHERE sequence = ?1",
            [sequence],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(row.is_some_and(|row| {
        row.0 == event_type
            && row.2 == created_at
            && serde_json::from_str::<Value>(&row.1)
                .is_ok_and(|payload| payload.get("change") == Some(expected_payload))
    }))
}

fn candidate_and_target_from_manifest(
    manifest: &BasisOfEstimateManifest,
) -> (BasisOfEstimateCandidate, EstimateTaskTarget) {
    let mut calculation_run_ids = HashSet::new();
    calculation_run_ids.insert(manifest.comparison_total_calculation_run_id.clone());
    for row in &manifest.boq_rows {
        if let Some(id) = &row.calculation_run_id {
            calculation_run_ids.insert(id.clone());
        }
    }
    for build_up in &manifest.resource_build_ups {
        calculation_run_ids.insert(build_up.calculation_run_id.clone());
    }
    for quote in &manifest.quotations {
        calculation_run_ids.insert(quote.normalization_calculation_run_id.clone());
    }
    let mut calculation_run_ids: Vec<_> = calculation_run_ids.into_iter().collect();
    calculation_run_ids.sort();
    let query_references = query_references_from_inventory(&manifest.query_inventory);
    let candidate = BasisOfEstimateCandidate {
        scope: manifest.scope.clone(),
        pricing_date: manifest.pricing_date.clone(),
        currencies: manifest.currencies.clone(),
        taxes: manifest.taxes.clone(),
        rate_sources: manifest.rate_sources.clone(),
        productivity: manifest.productivity.clone(),
        design_maturity: manifest.design_maturity.clone(),
        gaps: manifest.gaps.clone(),
        exclusions: manifest.exclusions.clone(),
        boq_rows: manifest.boq_rows.clone(),
        cbs_components: manifest.cbs_components.clone(),
        resource_build_ups: manifest.resource_build_ups.clone(),
        quotations: manifest.quotations.clone(),
        allowances: manifest.allowances.clone(),
        material_assumptions: manifest
            .material_assumptions
            .iter()
            .map(|assumption| EstimateQueryReference {
                query_id: assumption.query_id.clone(),
                version: assumption.query_version,
            })
            .collect(),
        comparison_total_calculation_run_id: manifest.comparison_total_calculation_run_id.clone(),
    };
    let target = EstimateTaskTarget {
        tender_id: String::new(),
        tender_revision: manifest.tender_revision,
        plan_id: manifest.plan_id.clone(),
        plan_version: manifest.plan_version,
        basis_id: manifest.basis_id.clone(),
        basis_version: manifest.version,
        supersedes_basis_manifest_sha256: manifest.supersedes_basis_manifest_sha256.clone(),
        remediates_review_manifest_sha256: manifest.remediates_review_manifest_sha256.clone(),
        boq_rows: manifest
            .boq_rows
            .iter()
            .map(|row| BoqInventoryRow {
                row_key: row.row_key.clone(),
                evidence: row.evidence.clone(),
            })
            .collect(),
        boq_inventory_sha256: manifest.boq_inventory_sha256.clone(),
        quotation_evidence: manifest
            .quotations
            .iter()
            .map(|quote| quote.evidence.clone())
            .collect(),
        calculation_run_ids,
        query_references,
        query_inventory: manifest.query_inventory.clone(),
        query_inventory_sha256: manifest.query_inventory_sha256.clone(),
    };
    (candidate, target)
}

impl TenderStore {
    pub(crate) fn estimate_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let (
            identity_count,
            version_count,
            head_count,
            review_count,
            approval_count,
            designation_count,
            aggregate_count,
            aggregate_approval_count,
        ): (u32, u32, u32, u32, u32, u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM basis_of_estimates),
                        (SELECT COUNT(*) FROM basis_of_estimate_versions),
                        (SELECT COUNT(*) FROM basis_of_estimate_heads),
                        (SELECT COUNT(*) FROM basis_of_estimate_reviews),
                        (SELECT COUNT(*) FROM basis_of_estimate_approvals),
                        (SELECT COUNT(*) FROM boq_table_designations),
                        (SELECT COUNT(*) FROM estimate_aggregate_calculation_runs),
                        (SELECT COUNT(*) FROM estimate_aggregate_calculation_approvals)",
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
            .map_err(sql_error)?;
        if designation_count > MAX_BOQ_TABLES {
            return Ok(false);
        }
        let mut designation_statement = self
            .connection
            .prepare(
                "SELECT artifact_id, artifact_version, table_number
                 FROM boq_table_designations
                 ORDER BY artifact_id, artifact_version, table_number",
            )
            .map_err(sql_error)?;
        let mut designation_rows = designation_statement.query([]).map_err(sql_error)?;
        let mut observed_designations = 0_u32;
        while let Some(row) = designation_rows.next().map_err(sql_error)? {
            check()?;
            observed_designations += 1;
            if load_boq_table_designation_with_check(
                &self.connection,
                &row.get::<_, String>(0).map_err(sql_error)?,
                row.get(1).map_err(sql_error)?,
                row.get(2).map_err(sql_error)?,
                check,
            )?
            .is_none()
            {
                return Ok(false);
            }
        }
        if observed_designations != designation_count {
            return Ok(false);
        }
        if version_count == 0 {
            return Ok(identity_count == 0
                && head_count == 0
                && review_count == 0
                && approval_count == 0
                && aggregate_count == 0
                && aggregate_approval_count == 0);
        }
        if identity_count != 1
            || head_count != 1
            || version_count > MAX_BASIS_VERSIONS
            || review_count > version_count
            || approval_count > review_count
            || aggregate_count != version_count
            || aggregate_approval_count != approval_count
        {
            return Ok(false);
        }
        let (head_id, head_version): (String, u32) = self
            .connection
            .query_row(
                "SELECT basis_id, current_version FROM basis_of_estimate_heads",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT basis_id, version, tender_revision, author_run_id,
                        author_profile_id, author_profile_version, complete, reconciled,
                        audit_sequence, manifest_json, manifest_sha256, created_at
                 FROM basis_of_estimate_versions ORDER BY version",
            )
            .map_err(sql_error)?;
        let mut rows = statement.query([]).map_err(sql_error)?;
        let mut expected_version = 1_u32;
        while let Some(row) = rows.next().map_err(sql_error)? {
            check()?;
            let basis_id: String = row.get(0).map_err(sql_error)?;
            let version: u32 = row.get(1).map_err(sql_error)?;
            let tender_revision: u32 = row.get(2).map_err(sql_error)?;
            let author_run_id: String = row.get(3).map_err(sql_error)?;
            let author_profile_id: String = row.get(4).map_err(sql_error)?;
            let author_profile_version: u32 = row.get(5).map_err(sql_error)?;
            let complete: bool = row.get(6).map_err(sql_error)?;
            let reconciled: bool = row.get(7).map_err(sql_error)?;
            let audit_sequence: i64 = row.get(8).map_err(sql_error)?;
            let manifest_json: String = row.get(9).map_err(sql_error)?;
            let manifest_sha256: String = row.get(10).map_err(sql_error)?;
            let created_at: String = row.get(11).map_err(sql_error)?;
            if basis_id != head_id
                || version != expected_version
                || manifest_sha256 != sha256_hex(manifest_json.as_bytes())
            {
                return Ok(false);
            }
            let manifest: BasisOfEstimateManifest = parse_canonical(&manifest_json)?;
            if manifest.schema_version != 1
                || manifest.basis_id != basis_id
                || manifest.version != version
                || manifest.tender_revision != tender_revision
                || manifest.author_run_id != author_run_id
                || manifest.author_profile_id != author_profile_id
                || manifest.author_profile_version != author_profile_version
                || manifest.complete != complete
                || manifest.reconciled != reconciled
                || manifest.created_at != created_at
            {
                return Ok(false);
            }
            let (candidate, mut target) = candidate_and_target_from_manifest(&manifest);
            if estimate_query_inventory_sha256(&manifest.query_inventory)?
                != manifest.query_inventory_sha256
            {
                return Ok(false);
            }
            if version == 1 {
                if manifest.supersedes_basis_manifest_sha256.is_some()
                    || manifest.remediates_review_manifest_sha256.is_some()
                {
                    return Ok(false);
                }
            } else {
                let prior: (String, Option<String>, Option<String>) = self
                    .connection
                    .query_row(
                        "SELECT versions.manifest_sha256, reviews.outcome, reviews.manifest_sha256
                         FROM basis_of_estimate_versions AS versions
                         LEFT JOIN basis_of_estimate_reviews AS reviews
                           ON reviews.basis_id = versions.basis_id
                          AND reviews.basis_version = versions.version
                         WHERE versions.basis_id = ?1 AND versions.version = ?2",
                        params![basis_id, version - 1],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .map_err(sql_error)?;
                let expected_review = prior.1.filter(|outcome| outcome == "failed").and(prior.2);
                if manifest.supersedes_basis_manifest_sha256.as_deref() != Some(prior.0.as_str())
                    || manifest.remediates_review_manifest_sha256 != expected_review
                {
                    return Ok(false);
                }
            }
            let tender_id: String = self
                .connection
                .query_row(
                    "SELECT tender_id FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            target.tender_id = tender_id;
            let normalized =
                match normalize_candidate(&self.connection, &candidate, &target, false, check) {
                    Ok(normalized) => normalized,
                    Err(error) if error.code == TenderErrorCode::InvalidCommand => {
                        return Ok(false)
                    }
                    Err(error) => return Err(error),
                };
            if !candidate_materially_remediates_prior_review(&self.connection, &candidate, &target)?
            {
                return Ok(false);
            }
            let Some(mut stored_aggregate) = load_estimate_aggregate_calculation(
                &self.connection,
                &manifest.aggregate_calculation.aggregate_run_id,
                check,
            )?
            else {
                return Ok(false);
            };
            stored_aggregate.approved_for_reliance = false;
            if normalized.material_assumptions != manifest.material_assumptions
                || normalized.aggregate_inputs != manifest.aggregate_calculation.inputs
                || normalized.total_amount != manifest.total_amount
                || normalized.total_currency != manifest.total_currency
                || normalized.total_amount != manifest.aggregate_calculation.final_amount
                || normalized.total_currency != manifest.aggregate_calculation.currency
                || manifest.aggregate_calculation != stored_aggregate
                || normalized.complete != manifest.complete
                || normalized.reconciled != manifest.reconciled
                || normalized.blockers != manifest.blockers
            {
                return Ok(false);
            }
            let task_id: Option<String> = self
                .connection
                .query_row(
                    "SELECT task_id FROM agent_runs WHERE run_id = ?1",
                    [&author_run_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            let Some(task_id) = task_id else {
                return Ok(false);
            };
            let task = load_task(&self.connection, &task_id)?;
            let exact = exact_basis_target(&task)?;
            let profile = load_profile(
                &self.connection,
                (author_profile_id.clone(), author_profile_version),
            )?;
            let expected_task = basis_task(
                task.task_id.clone(),
                &target,
                &profile,
                task.deadline.clone(),
            )?;
            let mut calculation_runs = Vec::with_capacity(target.calculation_run_ids.len());
            for calculation_run_id in &target.calculation_run_ids {
                check()?;
                calculation_runs.push(
                    approved_calculation_run_for_estimate(
                        &self.connection,
                        calculation_run_id,
                        check,
                    )?
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                );
            }
            calculation_runs
                .sort_by(|left, right| left.calculation_run_id.cmp(&right.calculation_run_id));
            let mut query_views = Vec::with_capacity(manifest.query_inventory.len());
            for observation in &manifest.query_inventory {
                check()?;
                query_views.push(estimate_query_view(&self.connection, observation)?);
            }
            let prior_basis = if version > 1 {
                Some(historical_basis_snapshot(load_basis_version_with_check(
                    &self.connection,
                    &basis_id,
                    version - 1,
                    check,
                )?))
            } else {
                None
            };
            let tender_name: String = self
                .connection
                .query_row(
                    "SELECT name FROM tender_revisions
                     WHERE tender_id = ?1 AND revision = ?2",
                    params![target.tender_id, target.tender_revision],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let expected_payload = basis_author_payload(
                &self.connection,
                BasisAuthorPayloadRequest {
                    target: &target,
                    prior_basis: prior_basis.as_ref(),
                    calculation_runs: &calculation_runs,
                    queries: &query_views,
                    profile: &profile,
                    tender_name: &tender_name,
                },
                check,
            )?;
            let candidate_json = canonical_json(&candidate)?;
            let run_envelope_matches = estimate_run_envelope_is_valid(
                &self.connection,
                EstimateRunIntegrityRequest {
                    run_id: &author_run_id,
                    expected_profile: &profile,
                    expected_task: &expected_task,
                    expected_payload: &expected_payload,
                    expected_candidate_json: &candidate_json,
                    result_created_at: &created_at,
                    plan_id: &manifest.plan_id,
                    plan_version: manifest.plan_version,
                    capability: COST_ESTIMATION_CAPABILITY,
                    assignment: EstimatePlanAssignment::Author,
                    started_event_type: "basis_of_estimate_started",
                    expected_started_change: json!({
                        "basis_id": basis_id,
                        "basis_version": version.to_string(),
                        "profile_id": author_profile_id,
                        "profile_version": author_profile_version.to_string(),
                        "run_id": author_run_id,
                        "task_id": task_id,
                    }),
                },
                check,
            )?;
            let author_audit_matches = audit_is_exact(
                &self.connection,
                audit_sequence,
                "basis_of_estimate_recorded",
                &created_at,
                &json!({
                    "author_profile_id": author_profile_id,
                    "author_profile_version": author_profile_version.to_string(),
                    "author_run_id": author_run_id,
                    "basis_id": basis_id,
                    "basis_version": version.to_string(),
                    "complete": complete,
                    "manifest_sha256": manifest_sha256,
                    "reconciled": reconciled,
                }),
            )?;
            let review_and_approval_match = basis_review_and_approval_are_valid(
                &self.connection,
                &manifest,
                &manifest_sha256,
                check,
            )?;
            if !run_envelope_matches
                || exact.basis_id != basis_id
                || exact.basis_version != version
                || exact.tender_revision != tender_revision
                || exact.plan_id != manifest.plan_id
                || exact.plan_version != manifest.plan_version
                || !author_audit_matches
                || !review_and_approval_match
            {
                return Ok(false);
            }
            expected_version += 1;
        }
        Ok(head_version == version_count && expected_version == version_count + 1)
    }
}

fn basis_review_and_approval_are_valid(
    connection: &rusqlite::Connection,
    basis: &BasisOfEstimateManifest,
    basis_manifest_sha256: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    check()?;
    type StoredReview = (
        String,
        String,
        String,
        u32,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let review: Option<StoredReview> = connection
        .query_row(
            "SELECT review_id, reviewer_run_id, reviewer_profile_id,
                    reviewer_profile_version, outcome, findings_json,
                    audit_sequence, manifest_json, manifest_sha256, created_at
             FROM basis_of_estimate_reviews WHERE basis_id = ?1 AND basis_version = ?2",
            params![basis.basis_id, basis.version],
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
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    type StoredApproval = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        u32,
        i64,
        String,
        String,
        String,
    );
    let approval: Option<StoredApproval> = connection
        .query_row(
            "SELECT approval_id, basis_manifest_sha256, review_id,
                    review_manifest_sha256, rationale, approved_by, acting_role, tender_revision,
                    audit_sequence, manifest_json, manifest_sha256, created_at
             FROM basis_of_estimate_approvals WHERE basis_id = ?1 AND basis_version = ?2",
            params![basis.basis_id, basis.version],
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
    let Some(review) = review else {
        return Ok(approval.is_none());
    };
    if review.8 != sha256_hex(review.7.as_bytes()) {
        return Ok(false);
    }
    let review_manifest: BasisReviewManifest = parse_canonical(&review.7)?;
    let candidate = BasisOfEstimateReviewCandidate {
        outcome: review_manifest.outcome,
        findings: review_manifest.findings.clone(),
    };
    let review_task_id: Option<String> = connection
        .query_row(
            "SELECT task_id FROM agent_runs WHERE run_id = ?1",
            [&review.1],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(review_task_id) = review_task_id else {
        return Ok(false);
    };
    let stored_review_task = load_task(connection, &review_task_id)?;
    let (target_basis_id, target_version, target_plan_id, target_plan_version) =
        exact_review_target(&stored_review_task)?;
    let reviewer = load_profile(connection, (review.2.clone(), review.3))?;
    let review_basis = review_target_snapshot(load_basis_version_with_check(
        connection,
        &basis.basis_id,
        basis.version,
        check,
    )?);
    let tender_id: String = connection
        .query_row(
            "SELECT tender_id FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let expected_task = review_task(BasisReviewTaskRequest {
        task_id: stored_review_task.task_id.clone(),
        tender_id: &tender_id,
        tender_revision: review_basis.tender_revision,
        plan_id: &basis.plan_id,
        plan_version: basis.plan_version,
        basis: &review_basis,
        profile: &reviewer,
        deadline: stored_review_task.deadline.clone(),
    })?;
    let mut run_ids = HashSet::new();
    run_ids.insert(basis.comparison_total_calculation_run_id.clone());
    run_ids.extend(
        basis
            .boq_rows
            .iter()
            .filter_map(|row| row.calculation_run_id.clone()),
    );
    run_ids.extend(
        basis
            .resource_build_ups
            .iter()
            .map(|build_up| build_up.calculation_run_id.clone()),
    );
    run_ids.extend(
        basis
            .quotations
            .iter()
            .map(|quote| quote.normalization_calculation_run_id.clone()),
    );
    let mut calculations = Vec::with_capacity(run_ids.len());
    for run_id in run_ids {
        check()?;
        calculations.push(
            approved_calculation_run_for_estimate(connection, &run_id, check)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        );
    }
    calculations.sort_by(|left, right| left.calculation_run_id.cmp(&right.calculation_run_id));
    let query_inventory = canonical_estimate_query_inventory(&basis.query_inventory)?;
    let mut query_views = Vec::with_capacity(query_inventory.len());
    for observation in &query_inventory {
        check()?;
        query_views.push(estimate_query_view(connection, observation)?);
    }
    let tender_name: String = connection
        .query_row(
            "SELECT name FROM tender_revisions WHERE tender_id = ?1 AND revision = ?2",
            params![tender_id, basis.tender_revision],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let expected_payload = basis_review_payload(
        connection,
        BasisReviewPayloadRequest {
            tender_id: &tender_id,
            tender_name: &tender_name,
            basis: &review_basis,
            calculations: &calculations,
            query_views: &query_views,
            profile: &reviewer,
        },
        check,
    )?;
    let candidate_json = canonical_json(&candidate)?;
    let run_envelope_matches = estimate_run_envelope_is_valid(
        connection,
        EstimateRunIntegrityRequest {
            run_id: &review.1,
            expected_profile: &reviewer,
            expected_task: &expected_task,
            expected_payload: &expected_payload,
            expected_candidate_json: &candidate_json,
            result_created_at: &review.9,
            plan_id: &basis.plan_id,
            plan_version: basis.plan_version,
            capability: BASIS_REVIEW_CAPABILITY,
            assignment: EstimatePlanAssignment::Reviewer,
            started_event_type: "basis_of_estimate_review_started",
            expected_started_change: json!({
                "basis_id": basis.basis_id,
                "basis_version": basis.version.to_string(),
                "reviewer_profile_id": review.2,
                "reviewer_profile_version": review.3.to_string(),
                "run_id": review.1,
                "task_id": review_task_id,
            }),
        },
        check,
    )?;
    let review_audit_matches = audit_is_exact(
        connection,
        review.6,
        "basis_of_estimate_review_completed",
        &review.9,
        &json!({
            "basis_id": basis.basis_id,
            "basis_manifest_sha256": basis_manifest_sha256,
            "basis_version": basis.version.to_string(),
            "manifest_sha256": review.8,
            "outcome": review.4,
            "review_id": review.0,
            "reviewer_profile_id": review.2,
            "reviewer_profile_version": review.3.to_string(),
            "reviewer_run_id": review.1,
        }),
    )?;
    if review_manifest.schema_version != 1
        || review_manifest.review_id != review.0
        || review_manifest.basis_id != basis.basis_id
        || review_manifest.basis_version != basis.version
        || review_manifest.basis_manifest_sha256 != basis_manifest_sha256
        || review_manifest.reviewer_run_id != review.1
        || review_manifest.reviewer_profile_id != review.2
        || review_manifest.reviewer_profile_version != review.3
        || review_manifest.outcome.as_str() != review.4
        || canonical_json(&review_manifest.findings)? != review.5
        || review_manifest.created_at != review.9
        || !run_envelope_matches
        || review.2 == basis.author_profile_id
        || !reviewer
            .capabilities
            .iter()
            .any(|capability| capability == BASIS_REVIEW_CAPABILITY)
        || target_basis_id != basis.basis_id
        || target_version != basis.version
        || target_plan_id != basis.plan_id
        || target_plan_version != basis.plan_version
        || !review_candidate_is_valid(
            &basis.boq_rows,
            basis.complete,
            basis.reconciled,
            &candidate,
        )
        || !review_audit_matches
    {
        return Ok(false);
    }
    let Some(approval) = approval else {
        return Ok(true);
    };
    if review.4 != "passed" || !basis.complete || !basis.reconciled {
        return Ok(false);
    }
    let approval_manifest: BasisApprovalManifest = parse_canonical(&approval.9)?;
    let approval_valid = approval.10 == sha256_hex(approval.9.as_bytes())
        && approval_manifest.schema_version == 1
        && approval_manifest.approval_id == approval.0
        && approval_manifest.basis_id == basis.basis_id
        && approval_manifest.basis_version == basis.version
        && approval_manifest.basis_manifest_sha256 == basis_manifest_sha256
        && approval_manifest.review_id == review.0
        && approval_manifest.review_manifest_sha256 == review.8
        && approval_manifest.aggregate_calculation_run_id
            == basis.aggregate_calculation.aggregate_run_id
        && approval_manifest.aggregate_calculation_manifest_sha256
            == basis.aggregate_calculation.manifest_sha256
        && approval_manifest.rationale == approval.4
        && approval_manifest.approved_by == approval.5
        && approval_manifest.acting_role == approval.6
        && approval_manifest.tender_revision == approval.7
        && approval_manifest.created_at == approval.11
        && approval.1 == basis_manifest_sha256
        && approval.2 == review.0
        && approval.3 == review.8
        && approval.5 == "engineer_user"
        && approval.6 == "engineer_in_the_loop"
        && approval.7 == basis.tender_revision
        && audit_is_exact(
            connection,
            approval.8,
            "basis_of_estimate_approved",
            &approval.11,
            &json!({
                "approval_id": approval.0,
                "basis_id": basis.basis_id,
                "basis_manifest_sha256": basis_manifest_sha256,
                "basis_version": basis.version.to_string(),
                "manifest_sha256": approval.10,
                "review_id": review.0,
                "review_manifest_sha256": review.8,
            }),
        )?;
    Ok(approval_valid)
}
