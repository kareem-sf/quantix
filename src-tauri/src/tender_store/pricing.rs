use std::{fs, path::Path};

use garde::Validate;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{
    de::{Error as _, SeqAccess},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::agent_runtime::{
    AgentProfileVersionView, AgentRunInspection, AgentTaskInputReference, PreparedAgentRun,
    TenderTaskView,
};

use super::{
    agent_records::{load_profile, load_task},
    append_audit_event_with_sequence,
    calculations::{
        approved_calculation_run_for_estimate, load_estimate_aggregate_calculation,
        load_pricing_calculation, record_pricing_calculation, PricingAdjustmentDirection,
        PricingCalculationAdjustmentInput, PricingCalculationRun, RecordPricingCalculation,
        CALCULATION_RULE_REVIEW_CAPABILITY,
    },
    estimates::{
        active_estimate_profile, audit_is_exact, estimate_run_envelope_is_valid,
        insert_planned_run, load_basis_version_with_check, EstimatePlanAssignment,
        EstimateRunIntegrityRequest, PlannedRunRequest,
    },
    lock_mutex_with_check, random_identifier, sha256_hex, sql_error, sqlite_timestamp,
    BidPackageOperationBudget, QuantixHost, TenderCommandError, TenderErrorCode, TenderId,
    TenderStore,
};

const MAX_BASELINE_VERSIONS: u32 = 32;
const MAX_PRICING_ADJUSTMENTS: u32 = 64;
const MAX_COMMERCIAL_STRATEGIES: u32 = 32;
const MAX_PRICING_SCENARIOS: u32 = 32;
const MAX_PRICING_DECISIONS: u32 = 128;
const MAX_FINDINGS: usize = 64;
const MAX_TEXT_BYTES: usize = 4_000;
const MAX_REVIEW_PAYLOAD_BYTES: usize = 1024 * 1024;
pub(crate) const PRICED_COST_BASELINE_REVIEW_CAPABILITY: &str = CALCULATION_RULE_REVIEW_CAPABILITY;

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
                .ok_or_else(|| E::custom("string exceeds the pricing command boundary"))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            (value.len() <= MAX)
                .then_some(value)
                .ok_or_else(|| E::custom("string exceeds the pricing command boundary"))
        }
    }
    deserializer.deserialize_string(Visitor::<MAX>)
}

#[derive(Deserialize)]
struct BoundedPricingText(
    #[serde(deserialize_with = "deserialize_bounded_string::<_, MAX_TEXT_BYTES>")] String,
);

fn deserialize_pricing_texts<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 64 bounded pricing statements")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(64));
            while let Some(value) = sequence.next_element::<BoundedPricingText>()? {
                if values.len() == 64 {
                    return Err(A::Error::custom("too many pricing statements"));
                }
                values.push(value.0);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundedPricingAdjustmentReference {
    #[serde(deserialize_with = "deserialize_bounded_string::<_, 32>")]
    adjustment_id: String,
    version: u32,
    #[serde(deserialize_with = "deserialize_bounded_string::<_, 64>")]
    manifest_sha256: String,
}

fn deserialize_pricing_adjustments<'de, D>(
    deserializer: D,
) -> Result<Vec<PricingAdjustmentReference>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<PricingAdjustmentReference>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 64 bounded pricing adjustment references")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(64));
            while let Some(value) = sequence.next_element::<BoundedPricingAdjustmentReference>()? {
                if values.len() == 64 {
                    return Err(A::Error::custom("too many pricing adjustment references"));
                }
                values.push(PricingAdjustmentReference {
                    adjustment_id: value.adjustment_id,
                    version: value.version,
                    manifest_sha256: value.manifest_sha256,
                });
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PricedCostBaselineReviewOutcome {
    Passed,
    Failed,
}

impl PricedCostBaselineReviewOutcome {
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
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricedCostBaselineReviewFinding {
    pub code: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricedCostBaselineReview {
    pub review_id: String,
    pub reviewer_run_id: String,
    pub reviewer_profile_id: String,
    pub reviewer_profile_version: u32,
    pub outcome: PricedCostBaselineReviewOutcome,
    pub findings: Vec<PricedCostBaselineReviewFinding>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricedCostBaselineApproval {
    pub approval_id: String,
    pub rationale: String,
    pub approved_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricedCostBaselineVersion {
    pub baseline_id: String,
    pub version: u32,
    pub tender_revision: u32,
    pub basis_id: String,
    pub basis_version: u32,
    pub basis_manifest_sha256: String,
    pub aggregate_calculation_run_id: String,
    pub aggregate_calculation_manifest_sha256: String,
    pub amount: String,
    pub currency: String,
    pub rationale: String,
    pub supersedes_baseline_manifest_sha256: Option<String>,
    pub remediates_review_manifest_sha256: Option<String>,
    pub current: bool,
    pub approved_for_commercial_pricing: bool,
    pub review: Option<PricedCostBaselineReview>,
    pub approval: Option<PricedCostBaselineApproval>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreatePricedCostBaselineCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub basis_id: String,
    #[garde(range(min = 1, max = 32))]
    pub basis_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub basis_manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunPricedCostBaselineReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub baseline_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricedCostBaselineReviewResult {
    pub run: AgentRunInspection,
    pub baseline: PricedCostBaselineVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApprovePricedCostBaselineCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub baseline_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PricingAdjustmentKind {
    Contingency,
    Markup,
    Exclusion,
    Qualification,
    CommercialStrategy,
    Other,
}

impl PricingAdjustmentKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Contingency => "contingency",
            Self::Markup => "markup",
            Self::Exclusion => "exclusion",
            Self::Qualification => "qualification",
            Self::CommercialStrategy => "commercial_strategy",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingAdjustmentApproval {
    pub approval_id: String,
    pub rationale: String,
    pub approved_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingAdjustmentVersion {
    pub adjustment_id: String,
    pub version: u32,
    pub tender_revision: u32,
    pub baseline_id: String,
    pub baseline_version: u32,
    pub calculation_run_id: String,
    pub calculation_manifest_sha256: String,
    pub amount: String,
    pub currency: String,
    pub kind: PricingAdjustmentKind,
    pub direction: PricingAdjustmentDirection,
    pub scope: String,
    pub rationale: String,
    pub commercial_appetite: Option<String>,
    pub exclusions: Vec<String>,
    pub qualifications: Vec<String>,
    pub supersedes_adjustment_manifest_sha256: Option<String>,
    pub remediates_review_manifest_sha256: Option<String>,
    pub current: bool,
    pub review: Option<PricedCostBaselineReview>,
    pub approval: Option<PricingAdjustmentApproval>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreatePricingAdjustmentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub baseline_id: String,
    #[garde(range(min = 1, max = 32))]
    pub baseline_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub baseline_manifest_sha256: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub calculation_run_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub calculation_manifest_sha256: String,
    #[garde(skip)]
    pub kind: PricingAdjustmentKind,
    #[garde(skip)]
    pub direction: PricingAdjustmentDirection,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub scope: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub rationale: String,
    #[garde(skip)]
    pub commercial_appetite: Option<String>,
    #[serde(deserialize_with = "deserialize_pricing_texts")]
    #[garde(length(max = 64))]
    pub exclusions: Vec<String>,
    #[serde(deserialize_with = "deserialize_pricing_texts")]
    #[garde(length(max = 64))]
    pub qualifications: Vec<String>,
    #[serde(deserialize_with = "deserialize_pricing_adjustments")]
    #[garde(length(max = 1), dive)]
    pub remediates: Vec<PricingAdjustmentReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunPricingAdjustmentReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub adjustment_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingAdjustmentReviewResult {
    pub run: AgentRunInspection,
    pub adjustment: PricingAdjustmentVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApprovePricingAdjustmentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub adjustment_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CommercialStrategyApproval {
    pub approval_id: String,
    pub rationale: String,
    pub approved_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CommercialStrategy {
    pub strategy_id: String,
    pub tender_revision: u32,
    pub baseline_id: String,
    pub baseline_version: u32,
    pub reviewed_input: PricingAdjustmentReference,
    pub input_review_id: String,
    pub input_approval_id: String,
    pub commercial_appetite: String,
    pub exclusions: Vec<String>,
    pub qualifications: Vec<String>,
    pub current: bool,
    pub approval: Option<CommercialStrategyApproval>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateCommercialStrategyCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub baseline_id: String,
    #[garde(range(min = 1, max = 32))]
    pub baseline_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub baseline_manifest_sha256: String,
    #[serde(deserialize_with = "deserialize_pricing_adjustments")]
    #[garde(length(min = 1, max = 1), dive)]
    pub reviewed_inputs: Vec<PricingAdjustmentReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApproveCommercialStrategyCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub strategy_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingAdjustmentReference {
    #[garde(length(bytes, min = 32, max = 32))]
    pub adjustment_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingScenarioSelection {
    pub selection_id: String,
    pub supersedes_selection_id: Option<String>,
    pub rationale: String,
    pub selected_by: String,
    pub acting_role: String,
    pub current: bool,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApprovedTenderPrice {
    pub approval_id: String,
    pub amount: String,
    pub currency: String,
    pub calculation_manifest_sha256: String,
    pub rationale: String,
    pub approved_by: String,
    pub acting_role: String,
    pub current: bool,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingScenarioVersion {
    pub pricing_scenario_id: String,
    pub version: u32,
    pub tender_revision: u32,
    pub name: String,
    pub baseline_id: String,
    pub baseline_version: u32,
    pub baseline_manifest_sha256: String,
    pub strategy_id: String,
    pub strategy_manifest_sha256: String,
    pub adjustments: Vec<PricingAdjustmentReference>,
    pub calculation: PricingCalculationRun,
    pub current: bool,
    pub selection: Option<PricingScenarioSelection>,
    pub approved_tender_price: Option<ApprovedTenderPrice>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreatePricingScenarioCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 200))]
    pub name: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub baseline_id: String,
    #[garde(range(min = 1, max = 32))]
    pub baseline_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub baseline_manifest_sha256: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub strategy_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub strategy_manifest_sha256: String,
    #[serde(deserialize_with = "deserialize_pricing_adjustments")]
    #[garde(length(max = 64), dive)]
    pub adjustments: Vec<PricingAdjustmentReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct SelectPricingScenarioCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub pricing_scenario_id: String,
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
pub struct ApproveTenderPriceCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub pricing_scenario_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub calculation_manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectPricingWorkspaceCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingWorkspaceInspection {
    pub baseline: Option<PricedCostBaselineVersion>,
    pub adjustments: Vec<PricingAdjustmentVersion>,
    pub strategies: Vec<CommercialStrategy>,
    pub scenarios: Vec<PricingScenarioVersion>,
    pub decision_history: Vec<PricingDecisionHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingDecisionHistoryEntry {
    pub pricing_scenario_id: String,
    pub pricing_scenario_version: u32,
    pub scenario_name: String,
    pub selection: PricingScenarioSelection,
    pub approved_tender_price: Option<ApprovedTenderPrice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PricedCostBaselineReviewCandidate {
    pub outcome: PricedCostBaselineReviewOutcome,
    pub findings: Vec<PricedCostBaselineReviewFinding>,
}

pub(crate) struct PricingReviewPublication<'a> {
    pub tender_id: &'a TenderId,
    pub tender_revision: u32,
    pub reviewer_run_id: &'a str,
    pub profile: &'a AgentProfileVersionView,
    pub task: &'a TenderTaskView,
    pub created_at: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricedCostBaselineManifest {
    schema_version: u32,
    baseline_id: String,
    version: u32,
    tender_revision: u32,
    basis_id: String,
    basis_version: u32,
    basis_manifest_sha256: String,
    aggregate_calculation_run_id: String,
    aggregate_calculation_manifest_sha256: String,
    amount: String,
    currency: String,
    rationale: String,
    supersedes_baseline_manifest_sha256: Option<String>,
    remediates_review_manifest_sha256: Option<String>,
    created_by: String,
    acting_role: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricedCostBaselineReviewManifest {
    schema_version: u32,
    review_id: String,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    reviewer_run_id: String,
    reviewer_profile_id: String,
    reviewer_profile_version: u32,
    outcome: PricedCostBaselineReviewOutcome,
    findings: Vec<PricedCostBaselineReviewFinding>,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricedCostBaselineApprovalManifest {
    schema_version: u32,
    approval_id: String,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    review_id: String,
    review_manifest_sha256: String,
    rationale: String,
    approved_by: String,
    acting_role: String,
    tender_revision: u32,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricingAdjustmentManifest {
    schema_version: u32,
    adjustment_id: String,
    version: u32,
    tender_revision: u32,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    calculation_run_id: String,
    calculation_manifest_sha256: String,
    amount: String,
    currency: String,
    kind: PricingAdjustmentKind,
    direction: PricingAdjustmentDirection,
    scope: String,
    rationale: String,
    commercial_appetite: Option<String>,
    exclusions: Vec<String>,
    qualifications: Vec<String>,
    supersedes_adjustment_manifest_sha256: Option<String>,
    remediates_review_manifest_sha256: Option<String>,
    created_by: String,
    acting_role: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricingAdjustmentReviewManifest {
    schema_version: u32,
    review_id: String,
    adjustment_id: String,
    adjustment_version: u32,
    adjustment_manifest_sha256: String,
    reviewer_run_id: String,
    reviewer_profile_id: String,
    reviewer_profile_version: u32,
    outcome: PricedCostBaselineReviewOutcome,
    findings: Vec<PricedCostBaselineReviewFinding>,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricingAdjustmentApprovalManifest {
    schema_version: u32,
    approval_id: String,
    adjustment_id: String,
    adjustment_version: u32,
    adjustment_manifest_sha256: String,
    review_id: String,
    review_manifest_sha256: String,
    rationale: String,
    approved_by: String,
    acting_role: String,
    tender_revision: u32,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommercialStrategyManifest {
    schema_version: u32,
    strategy_id: String,
    tender_revision: u32,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    reviewed_input: PricingAdjustmentReference,
    input_review_id: String,
    input_review_manifest_sha256: String,
    input_approval_id: String,
    input_approval_manifest_sha256: String,
    commercial_appetite: String,
    exclusions: Vec<String>,
    qualifications: Vec<String>,
    created_by: String,
    acting_role: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommercialStrategyApprovalManifest {
    schema_version: u32,
    approval_id: String,
    strategy_id: String,
    strategy_manifest_sha256: String,
    rationale: String,
    approved_by: String,
    acting_role: String,
    tender_revision: u32,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricingScenarioManifest {
    schema_version: u32,
    pricing_scenario_id: String,
    version: u32,
    tender_revision: u32,
    name: String,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    strategy_id: String,
    strategy_manifest_sha256: String,
    adjustments: Vec<PricingAdjustmentReference>,
    pricing_calculation_run_id: String,
    calculation_manifest_sha256: String,
    created_by: String,
    acting_role: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricingScenarioSelectionManifest {
    schema_version: u32,
    selection_id: String,
    supersedes_selection_id: Option<String>,
    pricing_scenario_id: String,
    pricing_scenario_version: u32,
    scenario_manifest_sha256: String,
    calculation_manifest_sha256: String,
    rationale: String,
    selected_by: String,
    acting_role: String,
    tender_revision: u32,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovedTenderPriceManifest {
    schema_version: u32,
    approval_id: String,
    pricing_scenario_id: String,
    pricing_scenario_version: u32,
    scenario_manifest_sha256: String,
    selection_id: String,
    strategy_approval_id: String,
    pricing_calculation_run_id: String,
    calculation_manifest_sha256: String,
    final_amount: String,
    currency: String,
    rationale: String,
    approved_by: String,
    acting_role: String,
    tender_revision: u32,
    created_at: String,
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
}

fn parse_canonical<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T, TenderCommandError> {
    serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.len() <= max && !trimmed.chars().any(char::is_control)
}

fn strategy_content_is_valid(
    kind: PricingAdjustmentKind,
    commercial_appetite: Option<&str>,
    exclusions: &[String],
    qualifications: &[String],
) -> bool {
    let mut canonical_exclusions = exclusions.to_vec();
    canonical_exclusions.sort();
    canonical_exclusions.dedup();
    let mut canonical_qualifications = qualifications.to_vec();
    canonical_qualifications.sort();
    canonical_qualifications.dedup();
    let content_is_canonical = canonical_exclusions == exclusions
        && canonical_qualifications == qualifications
        && exclusions.len() <= 64
        && qualifications.len() <= 64
        && exclusions
            .iter()
            .chain(qualifications)
            .all(|value| valid_text(value, MAX_TEXT_BYTES));
    match kind {
        PricingAdjustmentKind::CommercialStrategy => {
            commercial_appetite.is_some_and(|value| valid_text(value, MAX_TEXT_BYTES))
                && content_is_canonical
        }
        _ => commercial_appetite.is_none() && exclusions.is_empty() && qualifications.is_empty(),
    }
}

fn input(kind: &str, reference: &str, version: u32) -> AgentTaskInputReference {
    AgentTaskInputReference {
        kind: kind.into(),
        reference: reference.into(),
        version,
    }
}

fn review_output_contract() -> Result<String, TenderCommandError> {
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
                        "summary": { "type": "string", "minLength": 1, "maxLength": 2000 }
                    },
                    "required": ["code", "summary"],
                    "type": "object"
                }
            }
        },
        "required": ["outcome", "findings"],
        "type": "object"
    }))
}

fn baseline_review_payload(
    tender_id: &str,
    tender_name: &str,
    baseline: &PricedCostBaselineVersion,
    basis: &super::estimates::BasisOfEstimateVersion,
    profile: &AgentProfileVersionView,
) -> Result<Value, TenderCommandError> {
    let classification = profile
        .permissions
        .data_classifications
        .iter()
        .max()
        .copied()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(json!({
        "priced_cost_baseline": baseline,
        "approved_basis_of_estimate": basis,
        "data_classification": classification,
        "data_scope": profile.permissions.data_scopes.join("+"),
        "review_rules": {
            "approval_allowed": false,
            "edit_target_allowed": false,
            "reproduce_controlled_aggregate": true,
            "sell_price_decision_allowed": false,
        },
        "tender": {
            "tender_id": tender_id,
            "name": tender_name,
            "revision": baseline.tender_revision,
        },
    }))
}

fn adjustment_review_payload(
    tender_id: &str,
    tender_name: &str,
    adjustment: &PricingAdjustmentVersion,
    baseline: &PricedCostBaselineVersion,
    calculation: &super::calculations::ControlledBoqCalculationRun,
    profile: &AgentProfileVersionView,
) -> Result<Value, TenderCommandError> {
    let classification = profile
        .permissions
        .data_classifications
        .iter()
        .max()
        .copied()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(json!({
        "pricing_adjustment": adjustment,
        "priced_cost_baseline": baseline,
        "approved_calculation_run": calculation,
        "data_classification": classification,
        "data_scope": profile.permissions.data_scopes.join("+"),
        "review_rules": {
            "approval_allowed": false,
            "edit_target_allowed": false,
            "select_scenario_allowed": false,
            "select_margin_or_final_price_allowed": false,
        },
        "tender": {
            "tender_id": tender_id,
            "name": tender_name,
            "revision": adjustment.tender_revision,
        },
    }))
}

fn baseline_review_snapshot(mut baseline: PricedCostBaselineVersion) -> PricedCostBaselineVersion {
    baseline.current = true;
    baseline.approved_for_commercial_pricing = false;
    baseline.review = None;
    baseline.approval = None;
    baseline
}

fn approved_baseline_snapshot(
    mut baseline: PricedCostBaselineVersion,
) -> PricedCostBaselineVersion {
    baseline.current = true;
    baseline.approved_for_commercial_pricing = true;
    baseline
}

fn approved_basis_snapshot(
    mut basis: super::estimates::BasisOfEstimateVersion,
) -> super::estimates::BasisOfEstimateVersion {
    basis.current = true;
    basis.relied_upon = true;
    basis.aggregate_calculation.approved_for_reliance = true;
    basis
}

fn adjustment_review_snapshot(
    mut adjustment: PricingAdjustmentVersion,
) -> PricingAdjustmentVersion {
    adjustment.current = true;
    adjustment.review = None;
    adjustment.approval = None;
    adjustment
}

fn candidate_is_valid(candidate: &PricedCostBaselineReviewCandidate) -> bool {
    let mut codes = std::collections::HashSet::new();
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
        });
    findings_valid
        && match candidate.outcome {
            PricedCostBaselineReviewOutcome::Passed => candidate.findings.is_empty(),
            PricedCostBaselineReviewOutcome::Failed => !candidate.findings.is_empty(),
        }
}

fn exact_review_target(
    task: &TenderTaskView,
) -> Result<(String, u32, String, u32), TenderCommandError> {
    let baselines = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "priced_cost_baseline_version")
        .collect::<Vec<_>>();
    let plans = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "work_plan_version")
        .collect::<Vec<_>>();
    if baselines.len() != 1 || plans.len() != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((
        baselines[0].reference.clone(),
        baselines[0].version,
        plans[0].reference.clone(),
        plans[0].version,
    ))
}

fn exact_adjustment_review_target(
    task: &TenderTaskView,
) -> Result<(String, u32, String, u32), TenderCommandError> {
    let adjustments = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "pricing_adjustment_version")
        .collect::<Vec<_>>();
    let plans = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "work_plan_version")
        .collect::<Vec<_>>();
    if adjustments.len() != 1 || plans.len() != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((
        adjustments[0].reference.clone(),
        adjustments[0].version,
        plans[0].reference.clone(),
        plans[0].version,
    ))
}

fn adjustment_review_target_is_open(
    connection: &rusqlite::Connection,
    task: &TenderTaskView,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let (adjustment_id, version, plan_id, plan_version) = exact_adjustment_review_target(task)?;
    let adjustment =
        match load_pricing_adjustment_with_check(connection, &adjustment_id, version, check) {
            Ok(value) => value,
            Err(error) if error.code == TenderErrorCode::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
    if !adjustment.current || adjustment.review.is_some() || adjustment.approval.is_some() {
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

fn adjustment_review_task(
    task_id: String,
    tender_id: &str,
    adjustment: &PricingAdjustmentVersion,
    plan_id: &str,
    plan_version: u32,
    profile: &AgentProfileVersionView,
    deadline: String,
) -> Result<TenderTaskView, TenderCommandError> {
    let mut exact_inputs = vec![
        input("tender_revision", tender_id, adjustment.tender_revision),
        input("work_plan_version", plan_id, plan_version),
        input(
            "priced_cost_baseline_version",
            &adjustment.baseline_id,
            adjustment.baseline_version,
        ),
        input(
            "pricing_adjustment_version",
            &adjustment.adjustment_id,
            adjustment.version,
        ),
        input(
            "approved_calculation_run",
            &adjustment.calculation_run_id,
            adjustment.tender_revision,
        ),
    ];
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
        objective: "Independently review one exact commercial adjustment, its controlled Calculation Run, direction, scope, provenance, and impact without choosing margin, appetite, scenario, or Final Price.".into(),
        exact_inputs,
        output_contract_json: review_output_contract()?,
        review_policy: "Pass only when the adjustment is separately visible, fully attributable, uses one approved Calculation Run, and does not override a calculated result. The reviewer cannot edit or approve it.".into(),
        deadline,
        permissions: profile.permissions.clone(),
        resource_budget: profile.resource_budget.clone(),
        repair_feedback: None,
    })
}

fn load_review(
    connection: &rusqlite::Connection,
    baseline_id: &str,
    version: u32,
    baseline_manifest_sha256: &str,
) -> Result<Option<PricedCostBaselineReview>, TenderCommandError> {
    type Stored = (
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
    );
    let stored: Option<Stored> = connection
        .query_row(
            "SELECT review_id, reviewer_run_id, reviewer_profile_id,
                    reviewer_profile_version, outcome, findings_json,
                    baseline_manifest_sha256, manifest_json, manifest_sha256, created_at
             FROM priced_cost_baseline_reviews
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
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    stored
        .map(
            |(
                review_id,
                reviewer_run_id,
                reviewer_profile_id,
                reviewer_profile_version,
                outcome,
                findings_json,
                stored_baseline_sha,
                manifest_json,
                manifest_sha256,
                created_at,
            )| {
                if sha256_hex(manifest_json.as_bytes()) != manifest_sha256 {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let manifest: PricedCostBaselineReviewManifest = parse_canonical(&manifest_json)?;
                let findings: Vec<PricedCostBaselineReviewFinding> =
                    parse_canonical(&findings_json)?;
                if manifest.schema_version != 1
                    || manifest.review_id != review_id
                    || manifest.baseline_id != baseline_id
                    || manifest.baseline_version != version
                    || manifest.baseline_manifest_sha256 != baseline_manifest_sha256
                    || stored_baseline_sha != baseline_manifest_sha256
                    || manifest.reviewer_run_id != reviewer_run_id
                    || manifest.reviewer_profile_id != reviewer_profile_id
                    || manifest.reviewer_profile_version != reviewer_profile_version
                    || manifest.outcome.as_str() != outcome
                    || manifest.findings != findings
                    || manifest.created_at != created_at
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Ok(PricedCostBaselineReview {
                    review_id,
                    reviewer_run_id,
                    reviewer_profile_id,
                    reviewer_profile_version,
                    outcome: PricedCostBaselineReviewOutcome::parse(&outcome)?,
                    findings,
                    manifest_sha256,
                    created_at,
                })
            },
        )
        .transpose()
}

fn load_approval(
    connection: &rusqlite::Connection,
    baseline_id: &str,
    version: u32,
    baseline_manifest_sha256: &str,
    review: Option<&PricedCostBaselineReview>,
) -> Result<Option<PricedCostBaselineApproval>, TenderCommandError> {
    type Stored = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        u32,
        String,
    );
    let stored: Option<Stored> = connection
        .query_row(
            "SELECT approval_id, baseline_manifest_sha256, review_id,
                    review_manifest_sha256, rationale, approved_by, acting_role,
                    manifest_json, manifest_sha256, tender_revision, created_at
             FROM priced_cost_baseline_approvals
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
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    stored
        .map(
            |(
                approval_id,
                stored_baseline_sha,
                review_id,
                review_sha,
                rationale,
                approved_by,
                acting_role,
                manifest_json,
                stored_manifest_sha256,
                tender_revision,
                created_at,
            )| {
                let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
                let manifest: PricedCostBaselineApprovalManifest = parse_canonical(&manifest_json)?;
                if manifest_sha256 != stored_manifest_sha256
                    || manifest.schema_version != 1
                    || manifest.approval_id != approval_id
                    || manifest.baseline_id != baseline_id
                    || manifest.baseline_version != version
                    || manifest.baseline_manifest_sha256 != baseline_manifest_sha256
                    || stored_baseline_sha != baseline_manifest_sha256
                    || manifest.review_id != review_id
                    || manifest.review_manifest_sha256 != review_sha
                    || review.as_ref().map(|value| value.review_id.as_str())
                        != Some(review_id.as_str())
                    || review.as_ref().map(|value| value.manifest_sha256.as_str())
                        != Some(review_sha.as_str())
                    || manifest.rationale != rationale
                    || manifest.approved_by != approved_by
                    || manifest.acting_role != acting_role
                    || manifest.tender_revision != tender_revision
                    || manifest.created_at != created_at
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Ok(PricedCostBaselineApproval {
                    approval_id,
                    rationale,
                    approved_by,
                    acting_role,
                    manifest_sha256,
                    created_at,
                })
            },
        )
        .transpose()
}

pub(crate) fn load_priced_cost_baseline_with_check(
    connection: &rusqlite::Connection,
    baseline_id: &str,
    version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<PricedCostBaselineVersion, TenderCommandError> {
    check()?;
    type Stored = (
        u32,
        String,
        u32,
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let stored: Stored = connection
        .query_row(
            "SELECT tender_revision, basis_id, basis_version, basis_manifest_sha256,
                    aggregate_run_id, aggregate_manifest_sha256, amount, currency,
                    audit_sequence, manifest_json, manifest_sha256, created_at
             FROM priced_cost_baseline_versions
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
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(stored.9.as_bytes()) != stored.10 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manifest: PricedCostBaselineManifest = parse_canonical(&stored.9)?;
    if manifest.schema_version != 1
        || manifest.baseline_id != baseline_id
        || manifest.version != version
        || manifest.tender_revision != stored.0
        || manifest.basis_id != stored.1
        || manifest.basis_version != stored.2
        || manifest.basis_manifest_sha256 != stored.3
        || manifest.aggregate_calculation_run_id != stored.4
        || manifest.aggregate_calculation_manifest_sha256 != stored.5
        || manifest.amount != stored.6
        || manifest.currency != stored.7
        || manifest.created_by != "engineer_user"
        || manifest.acting_role != "engineer_in_the_loop"
        || manifest.created_at != stored.11
        || !audit_is_exact(
            connection,
            stored.8,
            "priced_cost_baseline_created",
            &stored.11,
            &json!({
                "baseline_id": baseline_id,
                "basis_id": manifest.basis_id,
                "basis_manifest_sha256": manifest.basis_manifest_sha256,
                "basis_version": manifest.basis_version.to_string(),
                "manifest_sha256": stored.10,
                "remediates_review_manifest_sha256": manifest.remediates_review_manifest_sha256,
                "supersedes_baseline_manifest_sha256": manifest.supersedes_baseline_manifest_sha256,
                "version": version.to_string(),
            }),
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    if version == 1 {
        if manifest.supersedes_baseline_manifest_sha256.is_some()
            || manifest.remediates_review_manifest_sha256.is_some()
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    } else {
        let prior =
            load_priced_cost_baseline_with_check(connection, baseline_id, version - 1, check)?;
        let expected_remediation = prior.review.as_ref().and_then(|review| {
            (review.outcome == PricedCostBaselineReviewOutcome::Failed)
                .then(|| review.manifest_sha256.clone())
        });
        if manifest.supersedes_baseline_manifest_sha256.as_deref()
            != Some(prior.manifest_sha256.as_str())
            || manifest.remediates_review_manifest_sha256 != expected_remediation
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    let basis = load_basis_version_with_check(
        connection,
        &manifest.basis_id,
        manifest.basis_version,
        check,
    )?;
    if basis.manifest_sha256 != manifest.basis_manifest_sha256
        || basis.aggregate_calculation.aggregate_run_id != manifest.aggregate_calculation_run_id
        || basis.aggregate_calculation.manifest_sha256
            != manifest.aggregate_calculation_manifest_sha256
        || basis.total_amount != manifest.amount
        || basis.total_currency != manifest.currency
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let head: Option<u32> = connection
        .query_row(
            "SELECT current_version FROM priced_cost_baseline_heads WHERE baseline_id = ?1",
            [baseline_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let current = head == Some(version) && basis.current && basis.relied_upon;
    let review = load_review(connection, baseline_id, version, &stored.10)?;
    let approval = load_approval(
        connection,
        baseline_id,
        version,
        &stored.10,
        review.as_ref(),
    )?;
    Ok(PricedCostBaselineVersion {
        baseline_id: manifest.baseline_id,
        version: manifest.version,
        tender_revision: manifest.tender_revision,
        basis_id: manifest.basis_id,
        basis_version: manifest.basis_version,
        basis_manifest_sha256: manifest.basis_manifest_sha256,
        aggregate_calculation_run_id: manifest.aggregate_calculation_run_id,
        aggregate_calculation_manifest_sha256: manifest.aggregate_calculation_manifest_sha256,
        amount: manifest.amount,
        currency: manifest.currency,
        rationale: manifest.rationale,
        supersedes_baseline_manifest_sha256: manifest.supersedes_baseline_manifest_sha256,
        remediates_review_manifest_sha256: manifest.remediates_review_manifest_sha256,
        current,
        approved_for_commercial_pricing: current && approval.is_some(),
        review,
        approval,
        manifest_sha256: stored.10,
        created_at: manifest.created_at,
    })
}

fn load_adjustment_review(
    connection: &rusqlite::Connection,
    adjustment_id: &str,
    version: u32,
    adjustment_manifest_sha256: &str,
) -> Result<Option<PricedCostBaselineReview>, TenderCommandError> {
    type Stored = (
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
    );
    let stored: Option<Stored> = connection
        .query_row(
            "SELECT review_id, reviewer_run_id, reviewer_profile_id,
                    reviewer_profile_version, outcome, findings_json,
                    adjustment_manifest_sha256, manifest_json, manifest_sha256, created_at
             FROM pricing_adjustment_reviews
             WHERE adjustment_id = ?1 AND adjustment_version = ?2",
            params![adjustment_id, version],
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
    stored
        .map(
            |(
                review_id,
                run_id,
                profile_id,
                profile_version,
                outcome,
                findings_json,
                stored_adjustment_sha,
                manifest_json,
                manifest_sha256,
                created_at,
            )| {
                if sha256_hex(manifest_json.as_bytes()) != manifest_sha256 {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let manifest: PricingAdjustmentReviewManifest = parse_canonical(&manifest_json)?;
                let findings: Vec<PricedCostBaselineReviewFinding> =
                    parse_canonical(&findings_json)?;
                if manifest.schema_version != 1
                    || manifest.review_id != review_id
                    || manifest.adjustment_id != adjustment_id
                    || manifest.adjustment_version != version
                    || manifest.adjustment_manifest_sha256 != adjustment_manifest_sha256
                    || stored_adjustment_sha != adjustment_manifest_sha256
                    || manifest.reviewer_run_id != run_id
                    || manifest.reviewer_profile_id != profile_id
                    || manifest.reviewer_profile_version != profile_version
                    || manifest.outcome.as_str() != outcome
                    || manifest.findings != findings
                    || manifest.created_at != created_at
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Ok(PricedCostBaselineReview {
                    review_id,
                    reviewer_run_id: run_id,
                    reviewer_profile_id: profile_id,
                    reviewer_profile_version: profile_version,
                    outcome: PricedCostBaselineReviewOutcome::parse(&outcome)?,
                    findings,
                    manifest_sha256,
                    created_at,
                })
            },
        )
        .transpose()
}

fn load_adjustment_approval(
    connection: &rusqlite::Connection,
    adjustment_id: &str,
    version: u32,
    tender_revision: u32,
    adjustment_manifest_sha256: &str,
    review: Option<&PricedCostBaselineReview>,
) -> Result<Option<PricingAdjustmentApproval>, TenderCommandError> {
    type Stored = (
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
    let stored: Option<Stored> = connection
        .query_row(
            "SELECT approval_id, adjustment_manifest_sha256, review_id,
                    review_manifest_sha256, rationale, approved_by, acting_role,
                    manifest_json, manifest_sha256, created_at
             FROM pricing_adjustment_approvals
             WHERE adjustment_id = ?1 AND adjustment_version = ?2",
            params![adjustment_id, version],
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
    stored
        .map(
            |(
                approval_id,
                adjustment_sha,
                review_id,
                review_sha,
                rationale,
                approved_by,
                acting_role,
                manifest_json,
                manifest_sha256,
                created_at,
            )| {
                if sha256_hex(manifest_json.as_bytes()) != manifest_sha256 {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let manifest: PricingAdjustmentApprovalManifest = parse_canonical(&manifest_json)?;
                if manifest.schema_version != 1
                    || manifest.approval_id != approval_id
                    || manifest.adjustment_id != adjustment_id
                    || manifest.adjustment_version != version
                    || manifest.tender_revision != tender_revision
                    || manifest.adjustment_manifest_sha256 != adjustment_manifest_sha256
                    || adjustment_sha != adjustment_manifest_sha256
                    || manifest.review_id != review_id
                    || manifest.review_manifest_sha256 != review_sha
                    || review.as_ref().map(|value| value.review_id.as_str())
                        != Some(review_id.as_str())
                    || review.as_ref().map(|value| value.manifest_sha256.as_str())
                        != Some(review_sha.as_str())
                    || manifest.rationale != rationale
                    || manifest.approved_by != approved_by
                    || manifest.acting_role != acting_role
                    || manifest.created_at != created_at
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Ok(PricingAdjustmentApproval {
                    approval_id,
                    rationale,
                    approved_by,
                    acting_role,
                    manifest_sha256,
                    created_at,
                })
            },
        )
        .transpose()
}

pub(crate) fn load_pricing_adjustment_with_check(
    connection: &rusqlite::Connection,
    adjustment_id: &str,
    version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<PricingAdjustmentVersion, TenderCommandError> {
    check()?;
    type Stored = (
        u32,
        String,
        u32,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let stored: Stored = connection
        .query_row(
            "SELECT tender_revision, baseline_id, baseline_version,
                    calculation_run_id, calculation_manifest_sha256, kind, direction,
                    audit_sequence, manifest_json, manifest_sha256, created_at
             FROM pricing_adjustment_versions
             WHERE adjustment_id = ?1 AND version = ?2",
            params![adjustment_id, version],
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
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(stored.8.as_bytes()) != stored.9 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manifest: PricingAdjustmentManifest = parse_canonical(&stored.8)?;
    if manifest.schema_version != 1
        || manifest.adjustment_id != adjustment_id
        || manifest.version != version
        || manifest.tender_revision != stored.0
        || manifest.baseline_id != stored.1
        || manifest.baseline_version != stored.2
        || manifest.calculation_run_id != stored.3
        || manifest.calculation_manifest_sha256 != stored.4
        || manifest.kind.as_str() != stored.5
        || manifest.direction.as_str() != stored.6
        || manifest.created_by != "engineer_user"
        || manifest.acting_role != "tendering_manager"
        || manifest.created_at != stored.10
        || !strategy_content_is_valid(
            manifest.kind,
            manifest.commercial_appetite.as_deref(),
            &manifest.exclusions,
            &manifest.qualifications,
        )
        || !audit_is_exact(
            connection,
            stored.7,
            "pricing_adjustment_created",
            &stored.10,
            &json!({
                "adjustment_id": adjustment_id,
                "baseline_id": manifest.baseline_id,
                "calculation_run_id": manifest.calculation_run_id,
                "kind": manifest.kind.as_str(),
                "manifest_sha256": stored.9,
                "remediates_review_manifest_sha256": manifest.remediates_review_manifest_sha256,
                "supersedes_adjustment_manifest_sha256": manifest.supersedes_adjustment_manifest_sha256,
                "version": version.to_string(),
            }),
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    match (
        manifest.supersedes_adjustment_manifest_sha256.as_deref(),
        manifest.remediates_review_manifest_sha256.as_deref(),
    ) {
        (None, None) => {}
        (Some(prior_sha), Some(review_sha)) => {
            let prior_key: Option<(String, u32, i64)> = connection
                .query_row(
                    "SELECT adjustment_id, version, audit_sequence FROM pricing_adjustment_versions
                     WHERE manifest_sha256 = ?1",
                    [prior_sha],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (prior_id, prior_version, prior_audit_sequence) = prior_key
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if prior_audit_sequence >= stored.7 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let prior =
                load_pricing_adjustment_with_check(connection, &prior_id, prior_version, check)?;
            if prior.adjustment_id == adjustment_id
                || prior.manifest_sha256 != prior_sha
                || prior
                    .review
                    .as_ref()
                    .filter(|review| review.outcome == PricedCostBaselineReviewOutcome::Failed)
                    .map(|review| review.manifest_sha256.as_str())
                    != Some(review_sha)
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }
        _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
    }
    let baseline = load_priced_cost_baseline_with_check(
        connection,
        &manifest.baseline_id,
        manifest.baseline_version,
        check,
    )?;
    let calculation =
        approved_calculation_run_for_estimate(connection, &manifest.calculation_run_id, check)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if baseline.manifest_sha256 != manifest.baseline_manifest_sha256
        || calculation.manifest_sha256 != manifest.calculation_manifest_sha256
        || calculation.final_amount.as_deref() != Some(manifest.amount.as_str())
        || calculation.output_currency != manifest.currency
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let head: Option<u32> = connection
        .query_row(
            "SELECT current_version FROM pricing_adjustment_heads WHERE adjustment_id = ?1",
            [adjustment_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let superseded: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM pricing_adjustment_versions
               WHERE json_extract(manifest_json, '$.supersedes_adjustment_manifest_sha256') = ?1
             )",
            [&stored.9],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let current = head == Some(version)
        && !superseded
        && baseline.current
        && baseline.approved_for_commercial_pricing
        && calculation.tender_revision == manifest.tender_revision;
    let review = load_adjustment_review(connection, adjustment_id, version, &stored.9)?;
    let approval = load_adjustment_approval(
        connection,
        adjustment_id,
        version,
        manifest.tender_revision,
        &stored.9,
        review.as_ref(),
    )?;
    Ok(PricingAdjustmentVersion {
        adjustment_id: manifest.adjustment_id,
        version: manifest.version,
        tender_revision: manifest.tender_revision,
        baseline_id: manifest.baseline_id,
        baseline_version: manifest.baseline_version,
        calculation_run_id: manifest.calculation_run_id,
        calculation_manifest_sha256: manifest.calculation_manifest_sha256,
        amount: manifest.amount,
        currency: manifest.currency,
        kind: manifest.kind,
        direction: manifest.direction,
        scope: manifest.scope,
        rationale: manifest.rationale,
        commercial_appetite: manifest.commercial_appetite,
        exclusions: manifest.exclusions,
        qualifications: manifest.qualifications,
        supersedes_adjustment_manifest_sha256: manifest.supersedes_adjustment_manifest_sha256,
        remediates_review_manifest_sha256: manifest.remediates_review_manifest_sha256,
        current,
        review,
        approval,
        manifest_sha256: stored.9,
        created_at: manifest.created_at,
    })
}

fn load_strategy_with_check(
    connection: &rusqlite::Connection,
    strategy_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<CommercialStrategy, TenderCommandError> {
    check()?;
    type StoredStrategy = (String, u32, u32, i64, String, String, String);
    let stored: StoredStrategy = connection
        .query_row(
            "SELECT baseline_id, baseline_version, tender_revision, audit_sequence,
                    manifest_json, manifest_sha256, created_at
             FROM commercial_strategies WHERE strategy_id = ?1",
            [strategy_id],
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
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(stored.4.as_bytes()) != stored.5 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manifest: CommercialStrategyManifest = parse_canonical(&stored.4)?;
    let baseline = load_priced_cost_baseline_with_check(
        connection,
        &manifest.baseline_id,
        manifest.baseline_version,
        check,
    )?;
    let reviewed_input = load_pricing_adjustment_with_check(
        connection,
        &manifest.reviewed_input.adjustment_id,
        manifest.reviewed_input.version,
        check,
    )?;
    let input_review = reviewed_input
        .review
        .as_ref()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let input_approval = reviewed_input
        .approval
        .as_ref()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if manifest.schema_version != 1
        || manifest.strategy_id != strategy_id
        || manifest.baseline_id != stored.0
        || manifest.baseline_version != stored.1
        || manifest.tender_revision != stored.2
        || baseline.manifest_sha256 != manifest.baseline_manifest_sha256
        || reviewed_input.manifest_sha256 != manifest.reviewed_input.manifest_sha256
        || reviewed_input.kind != PricingAdjustmentKind::CommercialStrategy
        || reviewed_input.baseline_id != manifest.baseline_id
        || reviewed_input.baseline_version != manifest.baseline_version
        || input_review.outcome != PricedCostBaselineReviewOutcome::Passed
        || input_review.review_id != manifest.input_review_id
        || input_review.manifest_sha256 != manifest.input_review_manifest_sha256
        || input_approval.approval_id != manifest.input_approval_id
        || input_approval.manifest_sha256 != manifest.input_approval_manifest_sha256
        || reviewed_input.commercial_appetite.as_deref()
            != Some(manifest.commercial_appetite.as_str())
        || reviewed_input.exclusions != manifest.exclusions
        || reviewed_input.qualifications != manifest.qualifications
        || manifest.created_by != "engineer_user"
        || manifest.acting_role != "tendering_manager"
        || manifest.created_at != stored.6
        || manifest.exclusions.len() > 64
        || manifest.qualifications.len() > 64
        || !valid_text(&manifest.commercial_appetite, MAX_TEXT_BYTES)
        || manifest
            .exclusions
            .iter()
            .chain(&manifest.qualifications)
            .any(|value| !valid_text(value, MAX_TEXT_BYTES))
        || !audit_is_exact(
            connection,
            stored.3,
            "commercial_strategy_created",
            &stored.6,
            &json!({
                "baseline_id": manifest.baseline_id,
                "input_adjustment_id": manifest.reviewed_input.adjustment_id,
                "input_approval_id": manifest.input_approval_id,
                "input_review_id": manifest.input_review_id,
                "manifest_sha256": stored.5,
                "strategy_id": strategy_id,
            }),
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    type StoredApproval = (
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let approval: Option<StoredApproval> = connection
        .query_row(
            "SELECT approval_id, strategy_manifest_sha256, rationale, approved_by,
                    acting_role, audit_sequence, manifest_json, manifest_sha256, created_at
             FROM commercial_strategy_approvals WHERE strategy_id = ?1",
            [strategy_id],
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
    let approval = approval
        .map(
            |(
                approval_id,
                strategy_sha,
                rationale,
                approved_by,
                acting_role,
                audit_sequence,
                approval_json,
                approval_sha,
                created_at,
            )| {
                if sha256_hex(approval_json.as_bytes()) != approval_sha {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let decision: CommercialStrategyApprovalManifest = parse_canonical(&approval_json)?;
                if decision.schema_version != 1
                    || decision.approval_id != approval_id
                    || decision.strategy_id != strategy_id
                    || decision.strategy_manifest_sha256 != stored.5
                    || strategy_sha != stored.5
                    || decision.rationale != rationale
                    || decision.approved_by != approved_by
                    || decision.acting_role != acting_role
                    || decision.tender_revision != manifest.tender_revision
                    || decision.created_at != created_at
                    || !audit_is_exact(
                        connection,
                        audit_sequence,
                        "commercial_strategy_approved",
                        &created_at,
                        &json!({
                            "approval_id": approval_id,
                            "manifest_sha256": approval_sha,
                            "strategy_id": strategy_id,
                            "strategy_manifest_sha256": stored.5,
                        }),
                    )?
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Ok(CommercialStrategyApproval {
                    approval_id,
                    rationale,
                    approved_by,
                    acting_role,
                    manifest_sha256: approval_sha,
                    created_at,
                })
            },
        )
        .transpose()?;
    Ok(CommercialStrategy {
        strategy_id: manifest.strategy_id,
        tender_revision: manifest.tender_revision,
        baseline_id: manifest.baseline_id,
        baseline_version: manifest.baseline_version,
        reviewed_input: manifest.reviewed_input,
        input_review_id: manifest.input_review_id,
        input_approval_id: manifest.input_approval_id,
        commercial_appetite: manifest.commercial_appetite,
        exclusions: manifest.exclusions,
        qualifications: manifest.qualifications,
        current: baseline.current
            && baseline.approved_for_commercial_pricing
            && reviewed_input.current
            && reviewed_input.approval.is_some(),
        approval,
        manifest_sha256: stored.5,
        created_at: manifest.created_at,
    })
}

fn load_selection_with_check(
    connection: &rusqlite::Connection,
    selection_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(String, u32, PricingScenarioSelection), TenderCommandError> {
    check()?;
    type StoredSelection = (
        String,
        u32,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let stored: StoredSelection = connection
        .query_row(
            "SELECT pricing_scenario_id, pricing_scenario_version,
                    scenario_manifest_sha256, rationale, selected_by, acting_role,
                    audit_sequence, manifest_json, manifest_sha256, created_at
             FROM pricing_scenario_selections WHERE selection_id = ?1",
            [selection_id],
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
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(stored.7.as_bytes()) != stored.8 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let decision: PricingScenarioSelectionManifest = parse_canonical(&stored.7)?;
    let scenario: (u32, String, String, String) = connection
        .query_row(
            "SELECT tender_revision, pricing_calculation_run_id, manifest_json,
                    manifest_sha256
             FROM pricing_scenario_versions
             WHERE pricing_scenario_id = ?1 AND version = ?2",
            params![stored.0, stored.1],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(sql_error)?;
    if sha256_hex(scenario.2.as_bytes()) != scenario.3 || scenario.3 != stored.2 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let scenario_manifest: PricingScenarioManifest = parse_canonical(&scenario.2)?;
    let calculation = load_pricing_calculation(connection, &scenario.1, check)?;
    let expected_supersedes: Option<String> = connection
        .query_row(
            "SELECT selection_id FROM pricing_scenario_selections
             WHERE audit_sequence < ?1 ORDER BY audit_sequence DESC LIMIT 1",
            [stored.6],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let head: Option<String> = connection
        .query_row(
            "SELECT selection_id FROM pricing_selection_head WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if scenario_manifest.pricing_scenario_id != stored.0
        || scenario_manifest.version != stored.1
        || scenario_manifest.tender_revision != scenario.0
        || scenario_manifest.calculation_manifest_sha256 != calculation.manifest_sha256
        || decision.schema_version != 1
        || decision.selection_id != selection_id
        || decision.supersedes_selection_id != expected_supersedes
        || decision.pricing_scenario_id != stored.0
        || decision.pricing_scenario_version != stored.1
        || decision.scenario_manifest_sha256 != stored.2
        || decision.calculation_manifest_sha256 != calculation.manifest_sha256
        || decision.rationale != stored.3
        || decision.selected_by != stored.4
        || decision.acting_role != stored.5
        || decision.tender_revision != scenario.0
        || decision.created_at != stored.9
        || !audit_is_exact(
            connection,
            stored.6,
            "pricing_scenario_selected",
            &stored.9,
            &json!({
                "manifest_sha256": stored.8,
                "pricing_scenario_id": stored.0,
                "scenario_manifest_sha256": stored.2,
                "selection_id": selection_id,
                "supersedes_selection_id": decision.supersedes_selection_id,
                "version": stored.1.to_string(),
            }),
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((
        stored.0,
        stored.1,
        PricingScenarioSelection {
            selection_id: selection_id.into(),
            supersedes_selection_id: decision.supersedes_selection_id,
            rationale: stored.3,
            selected_by: stored.4,
            acting_role: stored.5,
            current: head.as_deref() == Some(selection_id),
            manifest_sha256: stored.8,
            created_at: stored.9,
        },
    ))
}

fn load_price_with_check(
    connection: &rusqlite::Connection,
    approval_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(String, u32, ApprovedTenderPrice), TenderCommandError> {
    check()?;
    type StoredPrice = (
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
        i64,
        String,
        String,
        String,
    );
    let stored: StoredPrice = connection
        .query_row(
            "SELECT pricing_scenario_id, pricing_scenario_version,
                    scenario_manifest_sha256, selection_id, strategy_approval_id,
                    pricing_calculation_run_id, calculation_manifest_sha256,
                    final_amount, currency, rationale, approved_by, acting_role, audit_sequence,
                    manifest_json, manifest_sha256, created_at
             FROM approved_tender_prices WHERE approval_id = ?1",
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
                    row.get(12)?,
                    row.get(13)?,
                    row.get(14)?,
                    row.get(15)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(stored.13.as_bytes()) != stored.14 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let decision: ApprovedTenderPriceManifest = parse_canonical(&stored.13)?;
    let (selection_scenario_id, selection_version, selection) =
        load_selection_with_check(connection, &stored.3, check)?;
    let scenario_json: String = connection
        .query_row(
            "SELECT manifest_json FROM pricing_scenario_versions
             WHERE pricing_scenario_id = ?1 AND version = ?2",
            params![stored.0, stored.1],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let scenario: PricingScenarioManifest = parse_canonical(&scenario_json)?;
    let strategy = load_strategy_with_check(connection, &scenario.strategy_id, check)?;
    let calculation = load_pricing_calculation(connection, &stored.5, check)?;
    if selection_scenario_id != stored.0
        || selection_version != stored.1
        || selection.manifest_sha256.is_empty()
        || scenario.pricing_scenario_id != stored.0
        || scenario.version != stored.1
        || scenario.calculation_manifest_sha256 != calculation.manifest_sha256
        || decision.schema_version != 1
        || decision.approval_id != approval_id
        || decision.pricing_scenario_id != stored.0
        || decision.pricing_scenario_version != stored.1
        || decision.scenario_manifest_sha256 != stored.2
        || scenario_manifest_sha256(&scenario_json) != stored.2
        || decision.selection_id != stored.3
        || strategy
            .approval
            .as_ref()
            .map(|value| value.approval_id.as_str())
            != Some(stored.4.as_str())
        || decision.strategy_approval_id != stored.4
        || decision.pricing_calculation_run_id != stored.5
        || decision.calculation_manifest_sha256 != stored.6
        || calculation.manifest_sha256 != stored.6
        || decision.final_amount != stored.7
        || decision.currency != stored.8
        || decision.rationale != stored.9
        || decision.approved_by != stored.10
        || decision.acting_role != stored.11
        || decision.tender_revision != scenario.tender_revision
        || decision.created_at != stored.15
        || stored.7 != calculation.final_amount
        || stored.8 != calculation.currency
        || !audit_is_exact(
            connection,
            stored.12,
            "approved_tender_price_recorded",
            &stored.15,
            &json!({
                "approval_id": approval_id,
                "calculation_manifest_sha256": stored.6,
                "manifest_sha256": stored.14,
                "pricing_scenario_id": stored.0,
                "scenario_manifest_sha256": stored.2,
                "version": stored.1.to_string(),
            }),
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((
        stored.0,
        stored.1,
        ApprovedTenderPrice {
            approval_id: approval_id.into(),
            amount: stored.7,
            currency: stored.8,
            calculation_manifest_sha256: stored.6,
            rationale: stored.9,
            approved_by: stored.10,
            acting_role: stored.11,
            current: selection.current,
            manifest_sha256: stored.14,
            created_at: stored.15,
        },
    ))
}

fn scenario_manifest_sha256(json: &str) -> String {
    sha256_hex(json.as_bytes())
}

fn load_scenario_with_check(
    connection: &rusqlite::Connection,
    scenario_id: &str,
    version: u32,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<PricingScenarioVersion, TenderCommandError> {
    check()?;
    type StoredScenario = (
        u32,
        String,
        u32,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let stored: StoredScenario = connection
        .query_row(
            "SELECT tender_revision, baseline_id, baseline_version, strategy_id,
                    pricing_calculation_run_id, audit_sequence, manifest_json,
                    manifest_sha256, created_at
             FROM pricing_scenario_versions
             WHERE pricing_scenario_id = ?1 AND version = ?2",
            params![scenario_id, version],
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
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(stored.6.as_bytes()) != stored.7 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manifest: PricingScenarioManifest = parse_canonical(&stored.6)?;
    let mut canonical_adjustments = manifest.adjustments.clone();
    canonical_adjustments.sort_by(|left, right| {
        (&left.adjustment_id, left.version).cmp(&(&right.adjustment_id, right.version))
    });
    let mut seen_adjustments = std::collections::HashSet::new();
    if manifest.schema_version != 1
        || manifest.pricing_scenario_id != scenario_id
        || manifest.version != version
        || manifest.tender_revision != stored.0
        || manifest.baseline_id != stored.1
        || manifest.baseline_version != stored.2
        || manifest.strategy_id != stored.3
        || manifest.pricing_calculation_run_id != stored.4
        || manifest.created_by != "engineer_user"
        || manifest.acting_role != "tendering_manager"
        || manifest.created_at != stored.8
        || !valid_text(&manifest.name, 200)
        || manifest.adjustments.len() > 64
        || canonical_adjustments != manifest.adjustments
        || manifest.adjustments.iter().any(|reference| {
            !seen_adjustments.insert((&reference.adjustment_id, reference.version))
        })
        || !audit_is_exact(
            connection,
            stored.5,
            "pricing_scenario_created",
            &stored.8,
            &json!({
                "calculation_manifest_sha256": manifest.calculation_manifest_sha256,
                "manifest_sha256": stored.7,
                "pricing_calculation_run_id": manifest.pricing_calculation_run_id,
                "pricing_scenario_id": scenario_id,
                "version": version.to_string(),
            }),
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let baseline = load_priced_cost_baseline_with_check(
        connection,
        &manifest.baseline_id,
        manifest.baseline_version,
        check,
    )?;
    let strategy = load_strategy_with_check(connection, &manifest.strategy_id, check)?;
    if baseline.manifest_sha256 != manifest.baseline_manifest_sha256
        || strategy.manifest_sha256 != manifest.strategy_manifest_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let mut expected_calculation_inputs = Vec::with_capacity(manifest.adjustments.len());
    let mut adjustments_are_current = true;
    for reference in &manifest.adjustments {
        check()?;
        let adjustment = load_pricing_adjustment_with_check(
            connection,
            &reference.adjustment_id,
            reference.version,
            check,
        )?;
        if adjustment.manifest_sha256 != reference.manifest_sha256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        adjustments_are_current &= adjustment.current && adjustment.approval.is_some();
        expected_calculation_inputs.push(PricingCalculationAdjustmentInput {
            adjustment_id: adjustment.adjustment_id,
            adjustment_version: adjustment.version,
            adjustment_manifest_sha256: adjustment.manifest_sha256,
            calculation_run_id: adjustment.calculation_run_id,
            calculation_manifest_sha256: adjustment.calculation_manifest_sha256,
            direction: adjustment.direction,
            amount: adjustment.amount,
            currency: adjustment.currency,
        });
    }
    let calculation =
        load_pricing_calculation(connection, &manifest.pricing_calculation_run_id, check)?;
    expected_calculation_inputs.sort_by(|left, right| {
        (&left.adjustment_id, left.adjustment_version)
            .cmp(&(&right.adjustment_id, right.adjustment_version))
    });
    if calculation.manifest_sha256 != manifest.calculation_manifest_sha256
        || calculation.tender_revision != manifest.tender_revision
        || calculation.baseline_aggregate_run_id != baseline.aggregate_calculation_run_id
        || calculation.baseline_aggregate_manifest_sha256
            != baseline.aggregate_calculation_manifest_sha256
        || calculation.baseline_amount != baseline.amount
        || calculation.currency != baseline.currency
        || calculation.adjustments != expected_calculation_inputs
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let current = baseline.current
        && baseline.approved_for_commercial_pricing
        && strategy.current
        && strategy.approval.is_some()
        && adjustments_are_current;
    type StoredSelection = (
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let selection_head: Option<String> = connection
        .query_row(
            "SELECT selection_id FROM pricing_selection_head WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let selection: Option<StoredSelection> = connection
        .query_row(
            "SELECT selection_id, scenario_manifest_sha256, rationale, selected_by,
                    acting_role, audit_sequence, manifest_json, manifest_sha256, created_at
             FROM pricing_scenario_selections
             WHERE pricing_scenario_id = ?1 AND pricing_scenario_version = ?2
             ORDER BY audit_sequence DESC LIMIT 1",
            params![scenario_id, version],
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
    let selection = selection
        .map(
            |(
                id,
                scenario_sha,
                rationale,
                selected_by,
                acting_role,
                audit_sequence,
                json,
                sha,
                created_at,
            )| {
                if sha256_hex(json.as_bytes()) != sha {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let decision: PricingScenarioSelectionManifest = parse_canonical(&json)?;
                let expected_supersedes: Option<String> = connection
                    .query_row(
                        "SELECT selection_id FROM pricing_scenario_selections
                         WHERE audit_sequence < ?1
                         ORDER BY audit_sequence DESC LIMIT 1",
                        [audit_sequence],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
                if decision.schema_version != 1
                    || decision.selection_id != id
                    || decision.supersedes_selection_id != expected_supersedes
                    || decision.pricing_scenario_id != scenario_id
                    || decision.pricing_scenario_version != version
                    || decision.scenario_manifest_sha256 != stored.7
                    || decision.scenario_manifest_sha256 != scenario_sha
                    || decision.calculation_manifest_sha256 != calculation.manifest_sha256
                    || decision.rationale != rationale
                    || decision.selected_by != selected_by
                    || decision.acting_role != acting_role
                    || decision.tender_revision != manifest.tender_revision
                    || decision.created_at != created_at
                    || !audit_is_exact(
                        connection,
                        audit_sequence,
                        "pricing_scenario_selected",
                        &created_at,
                        &json!({
                            "manifest_sha256": sha,
                            "pricing_scenario_id": scenario_id,
                            "scenario_manifest_sha256": stored.7,
                            "selection_id": id,
                            "supersedes_selection_id": decision.supersedes_selection_id,
                            "version": version.to_string(),
                        }),
                    )?
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Ok(PricingScenarioSelection {
                    current: current && selection_head.as_deref() == Some(id.as_str()),
                    selection_id: id,
                    supersedes_selection_id: decision.supersedes_selection_id,
                    rationale,
                    selected_by,
                    acting_role,
                    manifest_sha256: sha,
                    created_at,
                })
            },
        )
        .transpose()?;
    type StoredPrice = (
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
        i64,
        String,
        String,
        String,
    );
    let price: Option<StoredPrice> = connection
        .query_row(
            "SELECT approval_id, scenario_manifest_sha256, selection_id,
                    strategy_approval_id, pricing_calculation_run_id,
                    calculation_manifest_sha256, final_amount, currency, rationale,
                    approved_by, acting_role, audit_sequence, manifest_json,
                    manifest_sha256, created_at
             FROM approved_tender_prices
             WHERE pricing_scenario_id = ?1 AND pricing_scenario_version = ?2
               AND selection_id = ?3",
            params![
                scenario_id,
                version,
                selection
                    .as_ref()
                    .map(|value| value.selection_id.as_str())
                    .unwrap_or("")
            ],
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
    let approved_tender_price = price
        .map(
            |(
                id,
                scenario_sha,
                selection_id,
                strategy_approval_id,
                pricing_calculation_run_id,
                calc_sha,
                amount,
                currency,
                rationale,
                approved_by,
                acting_role,
                audit_sequence,
                json,
                sha,
                created_at,
            )| {
                if sha256_hex(json.as_bytes()) != sha {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let decision: ApprovedTenderPriceManifest = parse_canonical(&json)?;
                if decision.schema_version != 1
                    || decision.approval_id != id
                    || decision.pricing_scenario_id != scenario_id
                    || decision.pricing_scenario_version != version
                    || decision.scenario_manifest_sha256 != stored.7
                    || decision.scenario_manifest_sha256 != scenario_sha
                    || selection.as_ref().map(|value| value.selection_id.as_str())
                        != Some(selection_id.as_str())
                    || strategy
                        .approval
                        .as_ref()
                        .map(|value| value.approval_id.as_str())
                        != Some(strategy_approval_id.as_str())
                    || decision.selection_id != selection_id
                    || decision.strategy_approval_id != strategy_approval_id
                    || decision.pricing_calculation_run_id != pricing_calculation_run_id
                    || pricing_calculation_run_id != calculation.pricing_calculation_run_id
                    || decision.calculation_manifest_sha256 != calc_sha
                    || decision.final_amount != amount
                    || decision.currency != currency
                    || decision.rationale != rationale
                    || decision.approved_by != approved_by
                    || decision.acting_role != acting_role
                    || decision.tender_revision != manifest.tender_revision
                    || decision.created_at != created_at
                    || amount != calculation.final_amount
                    || currency != calculation.currency
                    || !audit_is_exact(
                        connection,
                        audit_sequence,
                        "approved_tender_price_recorded",
                        &created_at,
                        &json!({
                            "approval_id": id,
                            "calculation_manifest_sha256": calculation.manifest_sha256,
                            "manifest_sha256": sha,
                            "pricing_scenario_id": scenario_id,
                            "scenario_manifest_sha256": stored.7,
                            "version": version.to_string(),
                        }),
                    )?
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Ok(ApprovedTenderPrice {
                    approval_id: id,
                    amount,
                    currency,
                    calculation_manifest_sha256: calc_sha,
                    rationale,
                    approved_by,
                    acting_role,
                    current: current
                        && selection
                            .as_ref()
                            .is_some_and(|selection| selection.current),
                    manifest_sha256: sha,
                    created_at,
                })
            },
        )
        .transpose()?;
    Ok(PricingScenarioVersion {
        pricing_scenario_id: manifest.pricing_scenario_id,
        version: manifest.version,
        tender_revision: manifest.tender_revision,
        name: manifest.name,
        baseline_id: manifest.baseline_id,
        baseline_version: manifest.baseline_version,
        baseline_manifest_sha256: manifest.baseline_manifest_sha256,
        strategy_id: manifest.strategy_id,
        strategy_manifest_sha256: manifest.strategy_manifest_sha256,
        adjustments: manifest.adjustments,
        calculation,
        current,
        selection,
        approved_tender_price,
        manifest_sha256: stored.7,
        created_at: manifest.created_at,
    })
}

fn review_target_is_open(
    connection: &rusqlite::Connection,
    task: &TenderTaskView,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    let (baseline_id, version, plan_id, plan_version) = exact_review_target(task)?;
    let baseline =
        match load_priced_cost_baseline_with_check(connection, &baseline_id, version, check) {
            Ok(value) => value,
            Err(error) if error.code == TenderErrorCode::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
    if !baseline.current || baseline.review.is_some() || baseline.approval.is_some() {
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

fn review_task(
    task_id: String,
    tender_id: &str,
    baseline: &PricedCostBaselineVersion,
    plan_id: &str,
    plan_version: u32,
    profile: &AgentProfileVersionView,
    deadline: String,
) -> Result<TenderTaskView, TenderCommandError> {
    let mut exact_inputs = vec![
        input("tender_revision", tender_id, baseline.tender_revision),
        input("work_plan_version", plan_id, plan_version),
        input(
            "priced_cost_baseline_version",
            &baseline.baseline_id,
            baseline.version,
        ),
        input(
            "basis_of_estimate_version",
            &baseline.basis_id,
            baseline.basis_version,
        ),
        input(
            "estimate_aggregate_calculation",
            &baseline.aggregate_calculation_run_id,
            baseline.basis_version,
        ),
    ];
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
        objective: "Independently reproduce the exact expected delivery-cost baseline from the approved Basis and controlled aggregate Calculation Run. Return bounded findings without changing cost or selecting sell-price strategy.".into(),
        exact_inputs,
        output_contract_json: review_output_contract()?,
        review_policy: "Pass only when the immutable Priced Cost Baseline equals the approved Basis aggregate, its provenance is exact, and cost remains distinct from commercial sell-price decisions. The reviewer cannot edit or approve the baseline.".into(),
        deadline,
        permissions: profile.permissions.clone(),
        resource_budget: profile.resource_budget.clone(),
        repair_feedback: None,
    })
}

fn reviewer_task(
    connection: &rusqlite::Connection,
    reviewer_run_id: &str,
) -> Result<Option<TenderTaskView>, TenderCommandError> {
    let task_id: Option<String> = connection
        .query_row(
            "SELECT task_id FROM agent_runs WHERE run_id = ?1",
            [reviewer_run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    task_id
        .map(|task_id| load_task(connection, &task_id))
        .transpose()
}

fn tender_name_at_revision(
    connection: &rusqlite::Connection,
    tender_id: &str,
    tender_revision: u32,
) -> Result<String, TenderCommandError> {
    connection
        .query_row(
            "SELECT name FROM tender_revisions WHERE tender_id = ?1 AND revision = ?2",
            params![tender_id, tender_revision],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn baseline_review_and_approval_are_valid(
    connection: &rusqlite::Connection,
    tender_id: &str,
    baseline: &PricedCostBaselineVersion,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    check()?;
    let Some(review) = baseline.review.as_ref() else {
        return Ok(baseline.approval.is_none());
    };
    let candidate = PricedCostBaselineReviewCandidate {
        outcome: review.outcome,
        findings: review.findings.clone(),
    };
    if !candidate_is_valid(&candidate) {
        return Ok(false);
    }
    let Some(stored_task) = reviewer_task(connection, &review.reviewer_run_id)? else {
        return Ok(false);
    };
    let (target_id, target_version, plan_id, plan_version) = exact_review_target(&stored_task)?;
    if target_id != baseline.baseline_id || target_version != baseline.version {
        return Ok(false);
    }
    let reviewer = load_profile(
        connection,
        (
            review.reviewer_profile_id.clone(),
            review.reviewer_profile_version,
        ),
    )?;
    let basis = approved_basis_snapshot(load_basis_version_with_check(
        connection,
        &baseline.basis_id,
        baseline.basis_version,
        check,
    )?);
    let baseline_snapshot = baseline_review_snapshot(baseline.clone());
    let tender_name = tender_name_at_revision(connection, tender_id, baseline.tender_revision)?;
    let expected_task = review_task(
        stored_task.task_id.clone(),
        tender_id,
        &baseline_snapshot,
        &plan_id,
        plan_version,
        &reviewer,
        stored_task.deadline.clone(),
    )?;
    let expected_payload = baseline_review_payload(
        tender_id,
        &tender_name,
        &baseline_snapshot,
        &basis,
        &reviewer,
    )?;
    let review_audit_sequence: i64 = connection
        .query_row(
            "SELECT audit_sequence FROM priced_cost_baseline_reviews WHERE review_id = ?1",
            [&review.review_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let candidate_json = canonical_json(&candidate)?;
    let run_is_exact = estimate_run_envelope_is_valid(
        connection,
        EstimateRunIntegrityRequest {
            run_id: &review.reviewer_run_id,
            expected_profile: &reviewer,
            expected_task: &expected_task,
            expected_payload: &expected_payload,
            expected_candidate_json: &candidate_json,
            result_created_at: &review.created_at,
            plan_id: &plan_id,
            plan_version,
            capability: PRICED_COST_BASELINE_REVIEW_CAPABILITY,
            assignment: EstimatePlanAssignment::Reviewer,
            started_event_type: "priced_cost_baseline_review_started",
            expected_started_change: json!({
                "baseline_id": baseline.baseline_id,
                "baseline_version": baseline.version.to_string(),
                "reviewer_profile_id": review.reviewer_profile_id,
                "reviewer_profile_version": review.reviewer_profile_version.to_string(),
                "run_id": review.reviewer_run_id,
                "task_id": stored_task.task_id,
            }),
        },
        check,
    )?;
    let review_audit_is_exact = audit_is_exact(
        connection,
        review_audit_sequence,
        "priced_cost_baseline_review_completed",
        &review.created_at,
        &json!({
            "baseline_id": baseline.baseline_id,
            "baseline_manifest_sha256": baseline.manifest_sha256,
            "baseline_version": baseline.version.to_string(),
            "manifest_sha256": review.manifest_sha256,
            "outcome": review.outcome.as_str(),
            "review_id": review.review_id,
            "reviewer_profile_id": review.reviewer_profile_id,
            "reviewer_profile_version": review.reviewer_profile_version.to_string(),
            "reviewer_run_id": review.reviewer_run_id,
        }),
    )?;
    if !run_is_exact
        || !review_audit_is_exact
        || review.reviewer_profile_id == basis.author_profile_id
    {
        return Ok(false);
    }
    let Some(approval) = baseline.approval.as_ref() else {
        return Ok(true);
    };
    if review.outcome != PricedCostBaselineReviewOutcome::Passed {
        return Ok(false);
    }
    let approval_audit_sequence: i64 = connection
        .query_row(
            "SELECT audit_sequence FROM priced_cost_baseline_approvals WHERE approval_id = ?1",
            [&approval.approval_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    audit_is_exact(
        connection,
        approval_audit_sequence,
        "priced_cost_baseline_approved",
        &approval.created_at,
        &json!({
            "approval_id": approval.approval_id,
            "baseline_id": baseline.baseline_id,
            "baseline_manifest_sha256": baseline.manifest_sha256,
            "baseline_version": baseline.version.to_string(),
            "manifest_sha256": approval.manifest_sha256,
            "review_id": review.review_id,
            "review_manifest_sha256": review.manifest_sha256,
        }),
    )
}

fn adjustment_review_and_approval_are_valid(
    connection: &rusqlite::Connection,
    tender_id: &str,
    adjustment: &PricingAdjustmentVersion,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    check()?;
    let Some(review) = adjustment.review.as_ref() else {
        return Ok(adjustment.approval.is_none());
    };
    let candidate = PricedCostBaselineReviewCandidate {
        outcome: review.outcome,
        findings: review.findings.clone(),
    };
    if !candidate_is_valid(&candidate) {
        return Ok(false);
    }
    let Some(stored_task) = reviewer_task(connection, &review.reviewer_run_id)? else {
        return Ok(false);
    };
    let (target_id, target_version, plan_id, plan_version) =
        exact_adjustment_review_target(&stored_task)?;
    if target_id != adjustment.adjustment_id || target_version != adjustment.version {
        return Ok(false);
    }
    let reviewer = load_profile(
        connection,
        (
            review.reviewer_profile_id.clone(),
            review.reviewer_profile_version,
        ),
    )?;
    let baseline = approved_baseline_snapshot(load_priced_cost_baseline_with_check(
        connection,
        &adjustment.baseline_id,
        adjustment.baseline_version,
        check,
    )?);
    let calculation =
        approved_calculation_run_for_estimate(connection, &adjustment.calculation_run_id, check)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let calculation_author_profile_id: String = connection
        .query_row(
            "SELECT profile_id FROM agent_runs WHERE run_id = ?1",
            [&calculation.cost_estimator_run_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let adjustment_snapshot = adjustment_review_snapshot(adjustment.clone());
    let tender_name = tender_name_at_revision(connection, tender_id, adjustment.tender_revision)?;
    let expected_task = adjustment_review_task(
        stored_task.task_id.clone(),
        tender_id,
        &adjustment_snapshot,
        &plan_id,
        plan_version,
        &reviewer,
        stored_task.deadline.clone(),
    )?;
    let expected_payload = adjustment_review_payload(
        tender_id,
        &tender_name,
        &adjustment_snapshot,
        &baseline,
        &calculation,
        &reviewer,
    )?;
    let review_audit_sequence: i64 = connection
        .query_row(
            "SELECT audit_sequence FROM pricing_adjustment_reviews WHERE review_id = ?1",
            [&review.review_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let candidate_json = canonical_json(&candidate)?;
    let run_is_exact = estimate_run_envelope_is_valid(
        connection,
        EstimateRunIntegrityRequest {
            run_id: &review.reviewer_run_id,
            expected_profile: &reviewer,
            expected_task: &expected_task,
            expected_payload: &expected_payload,
            expected_candidate_json: &candidate_json,
            result_created_at: &review.created_at,
            plan_id: &plan_id,
            plan_version,
            capability: PRICED_COST_BASELINE_REVIEW_CAPABILITY,
            assignment: EstimatePlanAssignment::Reviewer,
            started_event_type: "pricing_adjustment_review_started",
            expected_started_change: json!({
                "adjustment_id": adjustment.adjustment_id,
                "adjustment_version": adjustment.version.to_string(),
                "reviewer_profile_id": review.reviewer_profile_id,
                "reviewer_profile_version": review.reviewer_profile_version.to_string(),
                "run_id": review.reviewer_run_id,
                "task_id": stored_task.task_id,
            }),
        },
        check,
    )?;
    let review_audit_is_exact = audit_is_exact(
        connection,
        review_audit_sequence,
        "pricing_adjustment_review_completed",
        &review.created_at,
        &json!({
            "adjustment_id": adjustment.adjustment_id,
            "adjustment_manifest_sha256": adjustment.manifest_sha256,
            "adjustment_version": adjustment.version.to_string(),
            "manifest_sha256": review.manifest_sha256,
            "outcome": review.outcome.as_str(),
            "review_id": review.review_id,
            "reviewer_profile_id": review.reviewer_profile_id,
            "reviewer_profile_version": review.reviewer_profile_version.to_string(),
            "reviewer_run_id": review.reviewer_run_id,
        }),
    )?;
    if !run_is_exact
        || !review_audit_is_exact
        || review.reviewer_profile_id == calculation_author_profile_id
    {
        return Ok(false);
    }
    let Some(approval) = adjustment.approval.as_ref() else {
        return Ok(true);
    };
    if review.outcome != PricedCostBaselineReviewOutcome::Passed {
        return Ok(false);
    }
    let approval_audit_sequence: i64 = connection
        .query_row(
            "SELECT audit_sequence FROM pricing_adjustment_approvals WHERE approval_id = ?1",
            [&approval.approval_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    audit_is_exact(
        connection,
        approval_audit_sequence,
        "pricing_adjustment_approved",
        &approval.created_at,
        &json!({
            "adjustment_id": adjustment.adjustment_id,
            "adjustment_version": adjustment.version.to_string(),
            "approval_id": approval.approval_id,
            "manifest_sha256": approval.manifest_sha256,
            "review_id": review.review_id,
        }),
    )
}

impl TenderStore {
    pub(crate) fn record_pricing_denial(
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
            "pricing_command_denied",
            tender_revision,
            json!({ "command": command, "reason": reason, "target_id": target_id }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn create_priced_cost_baseline(
        &mut self,
        tender_id: &TenderId,
        command: &CreatePricedCostBaselineCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PricedCostBaselineVersion, TenderCommandError> {
        if !self.active_change_allows_estimate(&command.basis_id, command.basis_version)? {
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
            command.basis_version,
            &mut || budget.check(),
        )?;
        let existing_head: Option<(String, u32)> = transaction
            .query_row(
                "SELECT baseline_id, current_version FROM priced_cost_baseline_heads",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        if !basis.current
            || !basis.relied_upon
            || basis.manifest_sha256 != command.basis_manifest_sha256
            || !valid_text(&command.rationale, MAX_TEXT_BYTES)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (baseline_id, version, supersedes, remediates) = match existing_head {
            Some((baseline_id, current_version)) => {
                let prior = load_priced_cost_baseline_with_check(
                    &transaction,
                    &baseline_id,
                    current_version,
                    &mut || budget.check(),
                )?;
                let version = current_version
                    .checked_add(1)
                    .filter(|version| *version <= MAX_BASELINE_VERSIONS)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                let failed_review = prior
                    .review
                    .as_ref()
                    .filter(|review| review.outcome == PricedCostBaselineReviewOutcome::Failed);
                if prior.current && failed_review.is_none()
                    || prior.basis_id == basis.basis_id && prior.basis_version == basis.version
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                let supersedes = Some(prior.manifest_sha256.clone());
                let remediates = failed_review.map(|review| review.manifest_sha256.clone());
                (baseline_id, version, supersedes, remediates)
            }
            None => (random_identifier(&transaction)?, 1, None, None),
        };
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = PricedCostBaselineManifest {
            schema_version: 1,
            baseline_id: baseline_id.clone(),
            version,
            tender_revision: basis.tender_revision,
            basis_id: basis.basis_id.clone(),
            basis_version: basis.version,
            basis_manifest_sha256: basis.manifest_sha256.clone(),
            aggregate_calculation_run_id: basis.aggregate_calculation.aggregate_run_id.clone(),
            aggregate_calculation_manifest_sha256: basis
                .aggregate_calculation
                .manifest_sha256
                .clone(),
            amount: basis.total_amount.clone(),
            currency: basis.total_currency.clone(),
            rationale: command.rationale.trim().into(),
            supersedes_baseline_manifest_sha256: supersedes,
            remediates_review_manifest_sha256: remediates,
            created_by: "engineer_user".into(),
            acting_role: "engineer_in_the_loop".into(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "priced_cost_baseline_created",
            basis.tender_revision,
            json!({
                "baseline_id": baseline_id,
                "basis_id": basis.basis_id,
                "basis_manifest_sha256": basis.manifest_sha256,
                "basis_version": basis.version.to_string(),
                "manifest_sha256": manifest_sha256,
                "remediates_review_manifest_sha256": manifest.remediates_review_manifest_sha256,
                "supersedes_baseline_manifest_sha256": manifest.supersedes_baseline_manifest_sha256,
                "version": version.to_string(),
            }),
            &created_at,
        )?;
        if version == 1 {
            transaction
                .execute(
                    "INSERT INTO priced_cost_baselines (baseline_id, created_at) VALUES (?1, ?2)",
                    params![baseline_id, created_at],
                )
                .map_err(sql_error)?;
        }
        transaction
            .execute(
                "INSERT INTO priced_cost_baseline_versions (
                   baseline_id, version, tender_revision, basis_id, basis_version,
                   basis_manifest_sha256, aggregate_run_id, aggregate_manifest_sha256,
                   amount, currency, audit_sequence, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    baseline_id,
                    version,
                    basis.tender_revision,
                    basis.basis_id,
                    basis.version,
                    basis.manifest_sha256,
                    basis.aggregate_calculation.aggregate_run_id,
                    basis.aggregate_calculation.manifest_sha256,
                    basis.total_amount,
                    basis.total_currency,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        if version == 1 {
            transaction
                .execute(
                    "INSERT INTO priced_cost_baseline_heads (baseline_id, current_version) VALUES (?1, 1)",
                    [&baseline_id],
                )
                .map_err(sql_error)?;
        } else {
            transaction
                .execute(
                    "UPDATE priced_cost_baseline_heads SET current_version = ?2 WHERE baseline_id = ?1",
                    params![baseline_id, version],
                )
                .map_err(sql_error)?;
        }
        transaction.commit().map_err(sql_error)?;
        load_priced_cost_baseline_with_check(&self.connection, &baseline_id, version, &mut || {
            budget.check()
        })
    }

    pub(crate) fn prepare_priced_cost_baseline_review_run(
        &mut self,
        tender_id: &TenderId,
        baseline_id: &str,
        version: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        if !self.active_change_allows_pricing_object(baseline_id, version)? {
            self.require_change_intake_writable()?;
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
            let baseline = load_priced_cost_baseline_with_check(
                &transaction,
                baseline_id,
                version,
                &mut || budget.check(),
            )?;
            if !baseline.current || baseline.review.is_some() || baseline.approval.is_some() {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let basis = load_basis_version_with_check(
                &transaction,
                &baseline.basis_id,
                baseline.basis_version,
                &mut || budget.check(),
            )?;
            let active = active_estimate_profile(
                &transaction,
                PRICED_COST_BASELINE_REVIEW_CAPABILITY,
                Some(&basis.author_profile_id),
            )?;
            if active.tender_revision != baseline.tender_revision {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = review_task(
                random_identifier(&transaction)?,
                tender_id.as_str(),
                &baseline,
                &active.plan_id,
                active.plan_version,
                &active.profile,
                deadline.clone(),
            )?;
            let payload = baseline_review_payload(
                tender_id.as_str(),
                &active.tender_name,
                &baseline_review_snapshot(baseline.clone()),
                &approved_basis_snapshot(basis.clone()),
                &active.profile,
            )?;
            if canonical_json(&payload)?.len() > MAX_REVIEW_PAYLOAD_BYTES {
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
                    started_event_summary: "Independent Priced Cost Baseline review started",
                    audit_event_type: "priced_cost_baseline_review_started",
                    audit_payload: json!({
                        "baseline_id": baseline.baseline_id,
                        "baseline_version": baseline.version.to_string(),
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
            let _ = fs::remove_dir_all(workspace);
        }
        prepared
    }

    pub(crate) fn validate_priced_cost_baseline_review_candidate(
        &self,
        task: &TenderTaskView,
        payload: &str,
    ) -> Result<PricedCostBaselineReviewCandidate, TenderCommandError> {
        if payload.len() > 128 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        exact_review_target(task)?;
        let candidate: PricedCostBaselineReviewCandidate = serde_json::from_str(payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        candidate_is_valid(&candidate)
            .then_some(candidate)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }

    pub(crate) fn priced_cost_baseline_review_target_is_open(
        &self,
        task: &TenderTaskView,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        review_target_is_open(&self.connection, task, check)
    }

    pub(crate) fn load_priced_cost_baseline(
        &self,
        baseline_id: &str,
        version: u32,
    ) -> Result<PricedCostBaselineVersion, TenderCommandError> {
        load_priced_cost_baseline_with_check(&self.connection, baseline_id, version, &mut || Ok(()))
    }

    pub(crate) fn approve_priced_cost_baseline(
        &mut self,
        tender_id: &TenderId,
        command: &ApprovePricedCostBaselineCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PricedCostBaselineVersion, TenderCommandError> {
        if !self.active_change_allows_pricing_object(&command.baseline_id, command.version)? {
            self.require_change_intake_writable()?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let baseline = load_priced_cost_baseline_with_check(
            &transaction,
            &command.baseline_id,
            command.version,
            &mut || budget.check(),
        )?;
        let review = baseline
            .review
            .as_ref()
            .filter(|review| review.outcome == PricedCostBaselineReviewOutcome::Passed)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !baseline.current
            || baseline.approval.is_some()
            || baseline.manifest_sha256 != command.manifest_sha256
            || !valid_text(&command.rationale, MAX_TEXT_BYTES)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = PricedCostBaselineApprovalManifest {
            schema_version: 1,
            approval_id: approval_id.clone(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            review_id: review.review_id.clone(),
            review_manifest_sha256: review.manifest_sha256.clone(),
            rationale: command.rationale.trim().into(),
            approved_by: "engineer_user".into(),
            acting_role: "engineer_in_the_loop".into(),
            tender_revision: baseline.tender_revision,
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "priced_cost_baseline_approved",
            baseline.tender_revision,
            json!({
                "approval_id": approval_id,
                "baseline_id": baseline.baseline_id,
                "baseline_manifest_sha256": baseline.manifest_sha256,
                "baseline_version": baseline.version.to_string(),
                "manifest_sha256": manifest_sha256,
                "review_id": review.review_id,
                "review_manifest_sha256": review.manifest_sha256,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO priced_cost_baseline_approvals (
                   approval_id, baseline_id, baseline_version, baseline_manifest_sha256,
                   review_id, review_manifest_sha256, rationale, approved_by, acting_role,
                   tender_revision, audit_sequence, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'engineer_user',
                           'engineer_in_the_loop', ?8, ?9, ?10, ?11, ?12)",
                params![
                    approval_id,
                    baseline.baseline_id,
                    baseline.version,
                    baseline.manifest_sha256,
                    review.review_id,
                    review.manifest_sha256,
                    command.rationale.trim(),
                    baseline.tender_revision,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        load_priced_cost_baseline_with_check(
            &self.connection,
            &command.baseline_id,
            command.version,
            &mut || budget.check(),
        )
    }

    pub(crate) fn pricing_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let baseline_counts: (u32, u32, u32, u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM priced_cost_baselines),
                   (SELECT COUNT(*) FROM priced_cost_baseline_heads),
                   (SELECT COUNT(*) FROM priced_cost_baseline_versions),
                   (SELECT COUNT(*) FROM priced_cost_baseline_reviews),
                   (SELECT COUNT(*) FROM priced_cost_baseline_approvals),
                   (SELECT COUNT(*) FROM pricing_calculation_runs)",
                [],
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
        let other_counts: (u32, u32, u32, u32, u32, u32, u32, u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM pricing_adjustments),
                   (SELECT COUNT(*) FROM pricing_adjustment_heads),
                   (SELECT COUNT(*) FROM pricing_adjustment_versions),
                   (SELECT COUNT(*) FROM pricing_adjustment_reviews),
                   (SELECT COUNT(*) FROM pricing_adjustment_approvals),
                   (SELECT COUNT(*) FROM commercial_strategies),
                   (SELECT COUNT(*) FROM commercial_strategy_approvals),
                   (SELECT COUNT(*) FROM pricing_scenarios),
                   (SELECT COUNT(*) FROM pricing_scenario_versions),
                   (SELECT COUNT(*) FROM pricing_scenario_selections)",
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
                        row.get(8)?,
                        row.get(9)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        let price_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM approved_tender_prices", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        let selection_head_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM pricing_selection_head", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        if baseline_counts.0 != baseline_counts.1
            || baseline_counts.0 > 1
            || (baseline_counts.0 == 0) != (baseline_counts.2 == 0)
            || baseline_counts.2 > MAX_BASELINE_VERSIONS
            || baseline_counts.3 > baseline_counts.2
            || baseline_counts.4 > baseline_counts.3
            || other_counts.0 != other_counts.1
            || other_counts.0 != other_counts.2
            || other_counts.0 > MAX_PRICING_ADJUSTMENTS
            || other_counts.3 > other_counts.2
            || other_counts.4 > other_counts.3
            || other_counts.5 > MAX_COMMERCIAL_STRATEGIES
            || other_counts.6 > other_counts.5
            || other_counts.7 != other_counts.8
            || other_counts.7 > MAX_PRICING_SCENARIOS
            || baseline_counts.5 != other_counts.8
            || other_counts.9 > MAX_PRICING_DECISIONS
            || price_count > other_counts.9
            || selection_head_count > 1
            || (other_counts.9 == 0) != (selection_head_count == 0)
        {
            return Ok(false);
        }
        let tender_id: String = self
            .connection
            .query_row(
                "SELECT tender_id FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT baseline_id, version FROM priced_cost_baseline_versions
                 ORDER BY baseline_id, version",
            )
            .map_err(sql_error)?;
        let mut rows = statement.query([]).map_err(sql_error)?;
        let mut expected_version = 1_u32;
        let mut observed_baseline_reviews = 0_u32;
        let mut observed_baseline_approvals = 0_u32;
        while let Some(row) = rows.next().map_err(sql_error)? {
            check()?;
            let baseline_id: String = row.get(0).map_err(sql_error)?;
            let version: u32 = row.get(1).map_err(sql_error)?;
            if version != expected_version {
                return Ok(false);
            }
            let baseline = match load_priced_cost_baseline_with_check(
                &self.connection,
                &baseline_id,
                version,
                check,
            ) {
                Ok(value) => value,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error)
                }
                Err(_) => return Ok(false),
            };
            if version == 1 {
                let identity_created_at: String = self
                    .connection
                    .query_row(
                        "SELECT created_at FROM priced_cost_baselines WHERE baseline_id = ?1",
                        [&baseline_id],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if identity_created_at != baseline.created_at {
                    return Ok(false);
                }
            }
            if !baseline_review_and_approval_are_valid(
                &self.connection,
                &tender_id,
                &baseline,
                check,
            )? {
                return Ok(false);
            }
            observed_baseline_reviews += u32::from(baseline.review.is_some());
            observed_baseline_approvals += u32::from(baseline.approval.is_some());
            expected_version += 1;
        }
        if observed_baseline_reviews != baseline_counts.3
            || observed_baseline_approvals != baseline_counts.4
        {
            return Ok(false);
        }
        if baseline_counts.2 > 0 {
            let head: u32 = self
                .connection
                .query_row(
                    "SELECT current_version FROM priced_cost_baseline_heads",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if head != baseline_counts.2 {
                return Ok(false);
            }
        }

        let calculation_ids = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT pricing_calculation_run_id FROM pricing_calculation_runs
                     ORDER BY pricing_calculation_run_id",
                )
                .map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            rows
        };
        for calculation_id in calculation_ids {
            check()?;
            if let Err(error) = load_pricing_calculation(&self.connection, &calculation_id, check) {
                if error.code == TenderErrorCode::OperationTimedOut {
                    return Err(error);
                }
                return Ok(false);
            }
        }

        let adjustment_heads = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT adjustment_id, current_version FROM pricing_adjustment_heads
                     ORDER BY adjustment_id",
                )
                .map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            rows
        };
        let mut observed_adjustment_reviews = 0_u32;
        let mut observed_adjustment_approvals = 0_u32;
        for (adjustment_id, version) in adjustment_heads {
            check()?;
            if version != 1 {
                return Ok(false);
            }
            let adjustment = match load_pricing_adjustment_with_check(
                &self.connection,
                &adjustment_id,
                version,
                check,
            ) {
                Ok(value) => value,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error)
                }
                Err(_) => return Ok(false),
            };
            let identity_created_at: String = self
                .connection
                .query_row(
                    "SELECT created_at FROM pricing_adjustments WHERE adjustment_id = ?1",
                    [&adjustment_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if identity_created_at != adjustment.created_at
                || !adjustment_review_and_approval_are_valid(
                    &self.connection,
                    &tender_id,
                    &adjustment,
                    check,
                )?
            {
                return Ok(false);
            }
            observed_adjustment_reviews += u32::from(adjustment.review.is_some());
            observed_adjustment_approvals += u32::from(adjustment.approval.is_some());
        }
        if observed_adjustment_reviews != other_counts.3
            || observed_adjustment_approvals != other_counts.4
        {
            return Ok(false);
        }

        let strategy_ids = {
            let mut statement = self
                .connection
                .prepare("SELECT strategy_id FROM commercial_strategies ORDER BY strategy_id")
                .map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            rows
        };
        let mut observed_strategy_approvals = 0_u32;
        for strategy_id in strategy_ids {
            check()?;
            let strategy = match load_strategy_with_check(&self.connection, &strategy_id, check) {
                Ok(value) => value,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error)
                }
                Err(_) => return Ok(false),
            };
            observed_strategy_approvals += u32::from(strategy.approval.is_some());
        }
        if observed_strategy_approvals != other_counts.6 {
            return Ok(false);
        }

        let scenario_ids = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT pricing_scenario_id, version FROM pricing_scenario_versions
                     ORDER BY pricing_scenario_id, version",
                )
                .map_err(sql_error)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
                })
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            rows
        };
        for (scenario_id, version) in scenario_ids {
            check()?;
            if version != 1 {
                return Ok(false);
            }
            let scenario =
                match load_scenario_with_check(&self.connection, &scenario_id, version, check) {
                    Ok(value) => value,
                    Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                        return Err(error)
                    }
                    Err(_) => return Ok(false),
                };
            let identity_created_at: String = self
                .connection
                .query_row(
                    "SELECT created_at FROM pricing_scenarios WHERE pricing_scenario_id = ?1",
                    [&scenario_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if identity_created_at != scenario.created_at {
                return Ok(false);
            }
        }

        let selection_ids = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT selection_id FROM pricing_scenario_selections
                     ORDER BY audit_sequence",
                )
                .map_err(sql_error)?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            values
        };
        for selection_id in &selection_ids {
            check()?;
            if let Err(error) = load_selection_with_check(&self.connection, selection_id, check) {
                if error.code == TenderErrorCode::OperationTimedOut {
                    return Err(error);
                }
                return Ok(false);
            }
        }
        if let Some(expected_head) = selection_ids.last() {
            let actual_head: String = self
                .connection
                .query_row(
                    "SELECT selection_id FROM pricing_selection_head WHERE singleton = 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if &actual_head != expected_head {
                return Ok(false);
            }
        }
        let price_ids = {
            let mut statement = self
                .connection
                .prepare("SELECT approval_id FROM approved_tender_prices ORDER BY audit_sequence")
                .map_err(sql_error)?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_error)?;
            values
        };
        for approval_id in price_ids {
            check()?;
            if let Err(error) = load_price_with_check(&self.connection, &approval_id, check) {
                if error.code == TenderErrorCode::OperationTimedOut {
                    return Err(error);
                }
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl QuantixHost {
    pub fn create_pricing_adjustment(
        &self,
        command: CreatePricingAdjustmentCommand,
    ) -> Result<PricingAdjustmentVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "create_pricing_adjustment",
                Some(&command.baseline_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.create_pricing_adjustment(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "create_pricing_adjustment",
                        Some(&command.baseline_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn approve_pricing_adjustment(
        &self,
        command: ApprovePricingAdjustmentCommand,
    ) -> Result<PricingAdjustmentVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "approve_pricing_adjustment",
                Some(&command.adjustment_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.approve_pricing_adjustment(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "approve_pricing_adjustment",
                        Some(&command.adjustment_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn create_commercial_strategy(
        &self,
        command: CreateCommercialStrategyCommand,
    ) -> Result<CommercialStrategy, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "create_commercial_strategy",
                Some(&command.baseline_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.create_commercial_strategy(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "create_commercial_strategy",
                        Some(&command.baseline_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn approve_commercial_strategy(
        &self,
        command: ApproveCommercialStrategyCommand,
    ) -> Result<CommercialStrategy, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "approve_commercial_strategy",
                Some(&command.strategy_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.approve_commercial_strategy(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "approve_commercial_strategy",
                        Some(&command.strategy_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn create_pricing_scenario(
        &self,
        command: CreatePricingScenarioCommand,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "create_pricing_scenario",
                Some(&command.baseline_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.create_pricing_scenario(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "create_pricing_scenario",
                        Some(&command.baseline_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn select_pricing_scenario(
        &self,
        command: SelectPricingScenarioCommand,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "select_pricing_scenario",
                Some(&command.pricing_scenario_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.select_pricing_scenario(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "select_pricing_scenario",
                        Some(&command.pricing_scenario_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn approve_tender_price(
        &self,
        command: ApproveTenderPriceCommand,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "approve_tender_price",
                Some(&command.pricing_scenario_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.approve_tender_price(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "approve_tender_price",
                        Some(&command.pricing_scenario_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn inspect_pricing_workspace(
        &self,
        command: InspectPricingWorkspaceCommand,
    ) -> Result<PricingWorkspaceInspection, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_pricing_workspace(budget);
        result
    }
}

pub(crate) fn publish_pricing_adjustment_review(
    transaction: &Transaction<'_>,
    publication: PricingReviewPublication<'_>,
    candidate: &PricedCostBaselineReviewCandidate,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(), TenderCommandError> {
    let PricingReviewPublication {
        tender_id,
        tender_revision,
        reviewer_run_id,
        profile,
        task,
        created_at,
    } = publication;
    if !candidate_is_valid(candidate)
        || !profile
            .capabilities
            .iter()
            .any(|capability| capability == PRICED_COST_BASELINE_REVIEW_CAPABILITY)
        || profile.profile_id != task.profile_id
        || profile.version != task.profile_version
        || !adjustment_review_target_is_open(transaction, task, check)?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let (adjustment_id, version, _, _) = exact_adjustment_review_target(task)?;
    let adjustment =
        load_pricing_adjustment_with_check(transaction, &adjustment_id, version, check)?;
    if adjustment.tender_revision != tender_revision {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let review_id = random_identifier(transaction)?;
    let manifest = PricingAdjustmentReviewManifest {
        schema_version: 1,
        review_id: review_id.clone(),
        adjustment_id: adjustment.adjustment_id.clone(),
        adjustment_version: adjustment.version,
        adjustment_manifest_sha256: adjustment.manifest_sha256.clone(),
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
        "pricing_adjustment_review_completed",
        tender_revision,
        json!({
            "adjustment_id": adjustment.adjustment_id,
            "adjustment_manifest_sha256": adjustment.manifest_sha256,
            "adjustment_version": adjustment.version.to_string(),
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
            "INSERT INTO pricing_adjustment_reviews (
               review_id, adjustment_id, adjustment_version, adjustment_manifest_sha256,
               reviewer_run_id, reviewer_profile_id, reviewer_profile_version,
               outcome, findings_json, audit_sequence, manifest_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                review_id,
                adjustment.adjustment_id,
                adjustment.version,
                adjustment.manifest_sha256,
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

impl TenderStore {
    pub(crate) fn create_commercial_strategy(
        &mut self,
        tender_id: &TenderId,
        command: &CreateCommercialStrategyCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<CommercialStrategy, TenderCommandError> {
        let mut change_authorized = false;
        for input in &command.reviewed_inputs {
            if self.active_change_allows_pricing_object(&input.adjustment_id, input.version)? {
                change_authorized = true;
                break;
            }
        }
        if !change_authorized {
            self.require_change_intake_writable()?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let strategy_count: u32 = transaction
            .query_row("SELECT COUNT(*) FROM commercial_strategies", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        if strategy_count >= MAX_COMMERCIAL_STRATEGIES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let baseline = load_priced_cost_baseline_with_check(
            &transaction,
            &command.baseline_id,
            command.baseline_version,
            &mut || budget.check(),
        )?;
        let reviewed_reference = command
            .reviewed_inputs
            .first()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let reviewed_input = load_pricing_adjustment_with_check(
            &transaction,
            &reviewed_reference.adjustment_id,
            reviewed_reference.version,
            &mut || budget.check(),
        )?;
        let input_review = reviewed_input
            .review
            .as_ref()
            .filter(|review| review.outcome == PricedCostBaselineReviewOutcome::Passed)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let input_approval = reviewed_input
            .approval
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !baseline.current
            || !baseline.approved_for_commercial_pricing
            || baseline.manifest_sha256 != command.baseline_manifest_sha256
            || !reviewed_input.current
            || reviewed_input.kind != PricingAdjustmentKind::CommercialStrategy
            || reviewed_input.baseline_id != baseline.baseline_id
            || reviewed_input.baseline_version != baseline.version
            || reviewed_input.manifest_sha256 != reviewed_reference.manifest_sha256
            || !strategy_content_is_valid(
                reviewed_input.kind,
                reviewed_input.commercial_appetite.as_deref(),
                &reviewed_input.exclusions,
                &reviewed_input.qualifications,
            )
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let strategy_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = CommercialStrategyManifest {
            schema_version: 1,
            strategy_id: strategy_id.clone(),
            tender_revision: baseline.tender_revision,
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            reviewed_input: reviewed_reference.clone(),
            input_review_id: input_review.review_id.clone(),
            input_review_manifest_sha256: input_review.manifest_sha256.clone(),
            input_approval_id: input_approval.approval_id.clone(),
            input_approval_manifest_sha256: input_approval.manifest_sha256.clone(),
            commercial_appetite: reviewed_input
                .commercial_appetite
                .clone()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            exclusions: reviewed_input.exclusions.clone(),
            qualifications: reviewed_input.qualifications.clone(),
            created_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "commercial_strategy_created",
            baseline.tender_revision,
            json!({
                "baseline_id": baseline.baseline_id,
                "input_adjustment_id": reviewed_reference.adjustment_id,
                "input_approval_id": input_approval.approval_id,
                "input_review_id": input_review.review_id,
                "manifest_sha256": manifest_sha256,
                "strategy_id": strategy_id,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO commercial_strategies (
                   strategy_id, baseline_id, baseline_version, tender_revision,
                   audit_sequence, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    strategy_id,
                    baseline.baseline_id,
                    baseline.version,
                    baseline.tender_revision,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        load_strategy_with_check(&self.connection, &strategy_id, &mut || budget.check())
    }

    pub(crate) fn approve_commercial_strategy(
        &mut self,
        tender_id: &TenderId,
        command: &ApproveCommercialStrategyCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<CommercialStrategy, TenderCommandError> {
        if !self.active_change_allows_pricing_object(&command.strategy_id, 1)? {
            self.require_change_intake_writable()?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let strategy =
            load_strategy_with_check(&transaction, &command.strategy_id, &mut || budget.check())?;
        if !strategy.current
            || strategy.approval.is_some()
            || strategy.manifest_sha256 != command.manifest_sha256
            || !valid_text(&command.rationale, MAX_TEXT_BYTES)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = CommercialStrategyApprovalManifest {
            schema_version: 1,
            approval_id: approval_id.clone(),
            strategy_id: strategy.strategy_id.clone(),
            strategy_manifest_sha256: strategy.manifest_sha256.clone(),
            rationale: command.rationale.trim().into(),
            approved_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            tender_revision: strategy.tender_revision,
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "commercial_strategy_approved",
            strategy.tender_revision,
            json!({
                "approval_id": approval_id,
                "manifest_sha256": manifest_sha256,
                "strategy_id": strategy.strategy_id,
                "strategy_manifest_sha256": strategy.manifest_sha256,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO commercial_strategy_approvals (
                   approval_id, strategy_id, strategy_manifest_sha256, rationale,
                   approved_by, acting_role, audit_sequence, manifest_json,
                   manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, 'engineer_user', 'tendering_manager',
                           ?5, ?6, ?7, ?8)",
                params![
                    approval_id,
                    strategy.strategy_id,
                    strategy.manifest_sha256,
                    command.rationale.trim(),
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        load_strategy_with_check(&self.connection, &command.strategy_id, &mut || {
            budget.check()
        })
    }

    pub(crate) fn create_pricing_scenario(
        &mut self,
        tender_id: &TenderId,
        command: &CreatePricingScenarioCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        if !self
            .active_change_allows_pricing_object(&command.baseline_id, command.baseline_version)?
            || !self.active_change_allows_pricing_object(&command.strategy_id, 1)?
        {
            self.require_change_intake_writable()?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let scenario_count: u32 = transaction
            .query_row("SELECT COUNT(*) FROM pricing_scenarios", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        if scenario_count >= MAX_PRICING_SCENARIOS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let baseline = load_priced_cost_baseline_with_check(
            &transaction,
            &command.baseline_id,
            command.baseline_version,
            &mut || budget.check(),
        )?;
        let strategy =
            load_strategy_with_check(&transaction, &command.strategy_id, &mut || budget.check())?;
        if !baseline.current
            || !baseline.approved_for_commercial_pricing
            || baseline.manifest_sha256 != command.baseline_manifest_sha256
            || !strategy.current
            || strategy.approval.is_none()
            || strategy.manifest_sha256 != command.strategy_manifest_sha256
            || strategy.baseline_id != baseline.baseline_id
            || strategy.baseline_version != baseline.version
            || command.adjustments.len() > 64
            || !valid_text(&command.name, 200)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut references = command.adjustments.clone();
        references.sort_by(|left, right| {
            (&left.adjustment_id, left.version).cmp(&(&right.adjustment_id, right.version))
        });
        let mut inputs = Vec::with_capacity(references.len());
        let mut seen = std::collections::HashSet::new();
        for reference in &references {
            budget.check()?;
            if !seen.insert((&reference.adjustment_id, reference.version)) {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let adjustment = load_pricing_adjustment_with_check(
                &transaction,
                &reference.adjustment_id,
                reference.version,
                &mut || budget.check(),
            )?;
            if !adjustment.current
                || adjustment.approval.is_none()
                || adjustment.manifest_sha256 != reference.manifest_sha256
                || adjustment.baseline_id != baseline.baseline_id
                || adjustment.baseline_version != baseline.version
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            inputs.push(PricingCalculationAdjustmentInput {
                adjustment_id: adjustment.adjustment_id,
                adjustment_version: adjustment.version,
                adjustment_manifest_sha256: adjustment.manifest_sha256,
                calculation_run_id: adjustment.calculation_run_id,
                calculation_manifest_sha256: adjustment.calculation_manifest_sha256,
                direction: adjustment.direction,
                amount: adjustment.amount,
                currency: adjustment.currency,
            });
        }
        let pricing_scenario_id = random_identifier(&transaction)?;
        let pricing_calculation_run_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let calculation = record_pricing_calculation(
            &transaction,
            RecordPricingCalculation {
                pricing_calculation_run_id: &pricing_calculation_run_id,
                tender_revision: baseline.tender_revision,
                baseline_aggregate_run_id: &baseline.aggregate_calculation_run_id,
                baseline_aggregate_manifest_sha256: &baseline.aggregate_calculation_manifest_sha256,
                baseline_amount: &baseline.amount,
                adjustments: inputs,
                created_at: &created_at,
            },
            &mut || budget.check(),
        )?;
        let manifest = PricingScenarioManifest {
            schema_version: 1,
            pricing_scenario_id: pricing_scenario_id.clone(),
            version: 1,
            tender_revision: baseline.tender_revision,
            name: command.name.trim().into(),
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            strategy_id: strategy.strategy_id.clone(),
            strategy_manifest_sha256: strategy.manifest_sha256.clone(),
            adjustments: references,
            pricing_calculation_run_id: calculation.pricing_calculation_run_id.clone(),
            calculation_manifest_sha256: calculation.manifest_sha256.clone(),
            created_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "pricing_scenario_created",
            baseline.tender_revision,
            json!({
                "calculation_manifest_sha256": calculation.manifest_sha256,
                "manifest_sha256": manifest_sha256,
                "pricing_calculation_run_id": calculation.pricing_calculation_run_id,
                "pricing_scenario_id": pricing_scenario_id,
                "version": "1",
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO pricing_scenarios (pricing_scenario_id, created_at) VALUES (?1, ?2)",
                params![pricing_scenario_id, created_at],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO pricing_scenario_versions (
                   pricing_scenario_id, version, tender_revision, baseline_id,
                   baseline_version, strategy_id, pricing_calculation_run_id,
                   audit_sequence, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    pricing_scenario_id,
                    baseline.tender_revision,
                    baseline.baseline_id,
                    baseline.version,
                    strategy.strategy_id,
                    calculation.pricing_calculation_run_id,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        load_scenario_with_check(&self.connection, &pricing_scenario_id, 1, &mut || {
            budget.check()
        })
    }

    pub(crate) fn select_pricing_scenario(
        &mut self,
        tender_id: &TenderId,
        command: &SelectPricingScenarioCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        if !self
            .active_change_allows_pricing_object(&command.pricing_scenario_id, command.version)?
        {
            self.require_change_intake_writable()?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let scenario = load_scenario_with_check(
            &transaction,
            &command.pricing_scenario_id,
            command.version,
            &mut || budget.check(),
        )?;
        let selection_count: u32 = transaction
            .query_row(
                "SELECT COUNT(*) FROM pricing_scenario_selections",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let prior_selection: Option<(String, String, u32)> = transaction
            .query_row(
                "SELECT heads.selection_id, selections.pricing_scenario_id,
                        selections.pricing_scenario_version
                 FROM pricing_selection_head AS heads
                 JOIN pricing_scenario_selections AS selections
                   ON selections.selection_id = heads.selection_id
                 WHERE heads.singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        if selection_count >= MAX_PRICING_DECISIONS
            || !scenario.current
            || prior_selection.as_ref().is_some_and(|(_, id, version)| {
                id == &scenario.pricing_scenario_id && *version == scenario.version
            })
            || scenario.manifest_sha256 != command.manifest_sha256
            || !valid_text(&command.rationale, MAX_TEXT_BYTES)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let selection_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = PricingScenarioSelectionManifest {
            schema_version: 1,
            selection_id: selection_id.clone(),
            supersedes_selection_id: prior_selection.map(|value| value.0),
            pricing_scenario_id: scenario.pricing_scenario_id.clone(),
            pricing_scenario_version: scenario.version,
            scenario_manifest_sha256: scenario.manifest_sha256.clone(),
            calculation_manifest_sha256: scenario.calculation.manifest_sha256.clone(),
            rationale: command.rationale.trim().into(),
            selected_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            tender_revision: scenario.tender_revision,
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "pricing_scenario_selected",
            scenario.tender_revision,
            json!({
                "manifest_sha256": manifest_sha256,
                "pricing_scenario_id": scenario.pricing_scenario_id,
                "scenario_manifest_sha256": scenario.manifest_sha256,
                "selection_id": selection_id,
                "supersedes_selection_id": manifest.supersedes_selection_id,
                "version": scenario.version.to_string(),
            }),
            &created_at,
        )?;
        transaction.execute(
            "INSERT INTO pricing_scenario_selections (selection_id, pricing_scenario_id, pricing_scenario_version, scenario_manifest_sha256, rationale, selected_by, acting_role, audit_sequence, manifest_json, manifest_sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'engineer_user', 'tendering_manager', ?6, ?7, ?8, ?9)",
            params![selection_id, scenario.pricing_scenario_id, scenario.version, scenario.manifest_sha256, command.rationale.trim(), audit_sequence, manifest_json, manifest_sha256, created_at],
        ).map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO pricing_selection_head (singleton, selection_id)
                 VALUES (1, ?1)
                 ON CONFLICT(singleton) DO UPDATE SET selection_id = excluded.selection_id",
                [&selection_id],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        load_scenario_with_check(
            &self.connection,
            &command.pricing_scenario_id,
            command.version,
            &mut || budget.check(),
        )
    }

    pub(crate) fn approve_tender_price(
        &mut self,
        tender_id: &TenderId,
        command: &ApproveTenderPriceCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        if !self
            .active_change_allows_pricing_object(&command.pricing_scenario_id, command.version)?
        {
            self.require_change_intake_writable()?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let scenario = load_scenario_with_check(
            &transaction,
            &command.pricing_scenario_id,
            command.version,
            &mut || budget.check(),
        )?;
        let selection = scenario
            .selection
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let strategy =
            load_strategy_with_check(&transaction, &scenario.strategy_id, &mut || budget.check())?;
        let strategy_approval = strategy
            .approval
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !scenario.current
            || !selection.current
            || scenario.approved_tender_price.is_some()
            || scenario.manifest_sha256 != command.manifest_sha256
            || scenario.calculation.manifest_sha256 != command.calculation_manifest_sha256
            || !valid_text(&command.rationale, MAX_TEXT_BYTES)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = ApprovedTenderPriceManifest {
            schema_version: 1,
            approval_id: approval_id.clone(),
            pricing_scenario_id: scenario.pricing_scenario_id.clone(),
            pricing_scenario_version: scenario.version,
            scenario_manifest_sha256: scenario.manifest_sha256.clone(),
            selection_id: selection.selection_id.clone(),
            strategy_approval_id: strategy_approval.approval_id.clone(),
            pricing_calculation_run_id: scenario.calculation.pricing_calculation_run_id.clone(),
            calculation_manifest_sha256: scenario.calculation.manifest_sha256.clone(),
            final_amount: scenario.calculation.final_amount.clone(),
            currency: scenario.calculation.currency.clone(),
            rationale: command.rationale.trim().into(),
            approved_by: "engineer_user".into(),
            acting_role: "engineer_in_the_loop".into(),
            tender_revision: scenario.tender_revision,
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "approved_tender_price_recorded",
            scenario.tender_revision,
            json!({
                "approval_id": approval_id,
                "calculation_manifest_sha256": scenario.calculation.manifest_sha256,
                "manifest_sha256": manifest_sha256,
                "pricing_scenario_id": scenario.pricing_scenario_id,
                "scenario_manifest_sha256": scenario.manifest_sha256,
                "version": scenario.version.to_string(),
            }),
            &created_at,
        )?;
        transaction.execute(
            "INSERT INTO approved_tender_prices (approval_id, pricing_scenario_id, pricing_scenario_version, scenario_manifest_sha256, selection_id, strategy_approval_id, pricing_calculation_run_id, calculation_manifest_sha256, final_amount, currency, rationale, approved_by, acting_role, audit_sequence, manifest_json, manifest_sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'engineer_user', 'engineer_in_the_loop', ?12, ?13, ?14, ?15)",
            params![approval_id, scenario.pricing_scenario_id, scenario.version, scenario.manifest_sha256, selection.selection_id, strategy_approval.approval_id, scenario.calculation.pricing_calculation_run_id, scenario.calculation.manifest_sha256, scenario.calculation.final_amount, scenario.calculation.currency, command.rationale.trim(), audit_sequence, manifest_json, manifest_sha256, created_at],
        ).map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        load_scenario_with_check(
            &self.connection,
            &command.pricing_scenario_id,
            command.version,
            &mut || budget.check(),
        )
    }

    pub(crate) fn inspect_pricing_workspace(
        &self,
        budget: BidPackageOperationBudget,
    ) -> Result<PricingWorkspaceInspection, TenderCommandError> {
        inspect_pricing_workspace_in_connection(&self.connection, budget)
    }
}

pub(crate) fn inspect_pricing_workspace_in_connection(
    connection: &rusqlite::Connection,
    budget: BidPackageOperationBudget,
) -> Result<PricingWorkspaceInspection, TenderCommandError> {
    budget.check()?;
    let baseline_key: Option<(String, u32)> = connection
        .query_row(
            "SELECT baseline_id, current_version FROM priced_cost_baseline_heads LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let baseline = baseline_key
        .map(|(id, version)| {
            load_priced_cost_baseline_with_check(connection, &id, version, &mut || budget.check())
        })
        .transpose()?;
    let adjustment_keys = {
        let mut statement = connection.prepare("SELECT adjustment_id, current_version FROM pricing_adjustment_heads ORDER BY adjustment_id LIMIT 64").map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows
    };
    let mut adjustments = Vec::with_capacity(adjustment_keys.len());
    for (id, version) in adjustment_keys {
        budget.check()?;
        adjustments.push(load_pricing_adjustment_with_check(
            connection,
            &id,
            version,
            &mut || budget.check(),
        )?);
    }
    let strategy_ids = {
        let mut statement = connection
            .prepare(
                "SELECT strategy_id FROM commercial_strategies ORDER BY created_at DESC LIMIT 32",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows
    };
    let mut strategies = Vec::with_capacity(strategy_ids.len());
    for id in strategy_ids {
        budget.check()?;
        strategies.push(load_strategy_with_check(connection, &id, &mut || {
            budget.check()
        })?);
    }
    let scenario_keys = {
        let mut statement = connection.prepare("SELECT pricing_scenario_id, version FROM pricing_scenario_versions ORDER BY created_at DESC LIMIT 32").map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        rows
    };
    let mut scenarios = Vec::with_capacity(scenario_keys.len());
    for (id, version) in scenario_keys {
        budget.check()?;
        scenarios.push(load_scenario_with_check(
            connection,
            &id,
            version,
            &mut || budget.check(),
        )?);
    }
    let decision_ids = {
        let mut statement = connection
            .prepare(
                "SELECT selection_id FROM pricing_scenario_selections
                     ORDER BY audit_sequence DESC LIMIT 128",
            )
            .map_err(sql_error)?;
        let values = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_error)?;
        values
    };
    let mut decision_history = Vec::with_capacity(decision_ids.len());
    for selection_id in decision_ids {
        budget.check()?;
        let (scenario_id, scenario_version, mut selection) =
            load_selection_with_check(connection, &selection_id, &mut || budget.check())?;
        let scenario = scenarios
            .iter()
            .find(|value| {
                value.pricing_scenario_id == scenario_id && value.version == scenario_version
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        selection.current &= scenario.current;
        let price_id: Option<String> = connection
            .query_row(
                "SELECT approval_id FROM approved_tender_prices WHERE selection_id = ?1",
                [&selection_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let approved_tender_price = price_id
            .map(|approval_id| {
                load_price_with_check(connection, &approval_id, &mut || budget.check()).map(
                    |(_, _, mut price)| {
                        price.current &= scenario.current;
                        price
                    },
                )
            })
            .transpose()?;
        decision_history.push(PricingDecisionHistoryEntry {
            pricing_scenario_id: scenario_id,
            pricing_scenario_version: scenario_version,
            scenario_name: scenario.name.clone(),
            selection,
            approved_tender_price,
        });
    }
    Ok(PricingWorkspaceInspection {
        baseline,
        adjustments,
        strategies,
        scenarios,
        decision_history,
    })
}

pub(crate) fn publish_priced_cost_baseline_review(
    transaction: &Transaction<'_>,
    publication: PricingReviewPublication<'_>,
    candidate: &PricedCostBaselineReviewCandidate,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(), TenderCommandError> {
    let PricingReviewPublication {
        tender_id,
        tender_revision,
        reviewer_run_id,
        profile,
        task,
        created_at,
    } = publication;
    if !candidate_is_valid(candidate)
        || !profile
            .capabilities
            .iter()
            .any(|capability| capability == PRICED_COST_BASELINE_REVIEW_CAPABILITY)
        || profile.profile_id != task.profile_id
        || profile.version != task.profile_version
        || !review_target_is_open(transaction, task, check)?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let (baseline_id, version, _, _) = exact_review_target(task)?;
    let baseline = load_priced_cost_baseline_with_check(transaction, &baseline_id, version, check)?;
    if baseline.tender_revision != tender_revision {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let review_id = random_identifier(transaction)?;
    let manifest = PricedCostBaselineReviewManifest {
        schema_version: 1,
        review_id: review_id.clone(),
        baseline_id: baseline.baseline_id.clone(),
        baseline_version: baseline.version,
        baseline_manifest_sha256: baseline.manifest_sha256.clone(),
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
        "priced_cost_baseline_review_completed",
        tender_revision,
        json!({
            "baseline_id": baseline.baseline_id,
            "baseline_manifest_sha256": baseline.manifest_sha256,
            "baseline_version": baseline.version.to_string(),
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
            "INSERT INTO priced_cost_baseline_reviews (
               review_id, baseline_id, baseline_version, baseline_manifest_sha256,
               reviewer_run_id, reviewer_profile_id, reviewer_profile_version,
               outcome, findings_json, audit_sequence, manifest_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                review_id,
                baseline.baseline_id,
                baseline.version,
                baseline.manifest_sha256,
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
    pub fn create_priced_cost_baseline(
        &self,
        command: CreatePricedCostBaselineCommand,
    ) -> Result<PricedCostBaselineVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "create_priced_cost_baseline",
                Some(&command.basis_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.create_priced_cost_baseline(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "create_priced_cost_baseline",
                        Some(&command.basis_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }

    pub fn approve_priced_cost_baseline(
        &self,
        command: ApprovePricedCostBaselineCommand,
    ) -> Result<PricedCostBaselineVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut locked = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            locked.record_pricing_denial(
                &tender_id,
                "approve_priced_cost_baseline",
                Some(&command.baseline_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match locked.approve_priced_cost_baseline(&tender_id, &command, budget) {
            Ok(value) => Ok(value),
            Err(error) => {
                if error.code == TenderErrorCode::InvalidCommand {
                    locked.record_pricing_denial(
                        &tender_id,
                        "approve_priced_cost_baseline",
                        Some(&command.baseline_id),
                        "guard_denied",
                    )?;
                }
                Err(error)
            }
        }
    }
}

impl TenderStore {
    pub(crate) fn create_pricing_adjustment(
        &mut self,
        tender_id: &TenderId,
        command: &CreatePricingAdjustmentCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PricingAdjustmentVersion, TenderCommandError> {
        if !self.active_change_allows_calculation_run(&command.calculation_run_id)?
            || !self.active_change_allows_pricing_object(
                &command.baseline_id,
                command.baseline_version,
            )?
        {
            self.require_change_intake_writable()?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let adjustment_count: u32 = transaction
            .query_row("SELECT COUNT(*) FROM pricing_adjustments", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        if adjustment_count >= MAX_PRICING_ADJUSTMENTS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let baseline = load_priced_cost_baseline_with_check(
            &transaction,
            &command.baseline_id,
            command.baseline_version,
            &mut || budget.check(),
        )?;
        let calculation = approved_calculation_run_for_estimate(
            &transaction,
            &command.calculation_run_id,
            &mut || budget.check(),
        )?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let baseline_aggregate = load_estimate_aggregate_calculation(
            &transaction,
            &baseline.aggregate_calculation_run_id,
            &mut || budget.check(),
        )?
        .filter(|run| run.approved_for_reliance)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let calculation_is_baseline_input = baseline_aggregate
            .inputs
            .iter()
            .any(|input| input.calculation_run_id == command.calculation_run_id)
            || baseline_aggregate.comparison_total_calculation_run_id == command.calculation_run_id;
        let commercial_appetite = command
            .commercial_appetite
            .as_deref()
            .map(str::trim)
            .map(str::to_owned);
        let mut exclusions = command
            .exclusions
            .iter()
            .map(|value| value.trim().to_owned())
            .collect::<Vec<_>>();
        exclusions.sort();
        exclusions.dedup();
        let mut qualifications = command
            .qualifications
            .iter()
            .map(|value| value.trim().to_owned())
            .collect::<Vec<_>>();
        qualifications.sort();
        qualifications.dedup();
        let remediation = command
            .remediates
            .first()
            .map(|reference| {
                let prior = load_pricing_adjustment_with_check(
                    &transaction,
                    &reference.adjustment_id,
                    reference.version,
                    &mut || budget.check(),
                )?;
                let Some(failed_review) = prior.review.as_ref().filter(|review| {
                    review.outcome == PricedCostBaselineReviewOutcome::Failed
                }) else {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                };
                let successor_exists: bool = transaction
                    .query_row(
                        "SELECT EXISTS(
                           SELECT 1 FROM pricing_adjustment_versions
                           WHERE json_extract(manifest_json, '$.supersedes_adjustment_manifest_sha256') = ?1
                         )",
                        [&reference.manifest_sha256],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if prior.manifest_sha256 != reference.manifest_sha256
                    || successor_exists
                    || prior.baseline_id != baseline.baseline_id
                    || prior.baseline_version != baseline.version
                    || prior.calculation_run_id == calculation.calculation_run_id
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                Ok((
                    prior.manifest_sha256,
                    failed_review.manifest_sha256.clone(),
                ))
            })
            .transpose()?;
        if !baseline.current
            || !baseline.approved_for_commercial_pricing
            || baseline.manifest_sha256 != command.baseline_manifest_sha256
            || calculation.tender_revision != baseline.tender_revision
            || calculation.manifest_sha256 != command.calculation_manifest_sha256
            || calculation.output_currency != baseline.currency
            || calculation_is_baseline_input
            || !valid_text(&command.scope, MAX_TEXT_BYTES)
            || !valid_text(&command.rationale, MAX_TEXT_BYTES)
            || !strategy_content_is_valid(
                command.kind,
                commercial_appetite.as_deref(),
                &exclusions,
                &qualifications,
            )
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let amount = calculation
            .final_amount
            .clone()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let adjustment_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = PricingAdjustmentManifest {
            schema_version: 1,
            adjustment_id: adjustment_id.clone(),
            version: 1,
            tender_revision: baseline.tender_revision,
            baseline_id: baseline.baseline_id.clone(),
            baseline_version: baseline.version,
            baseline_manifest_sha256: baseline.manifest_sha256.clone(),
            calculation_run_id: calculation.calculation_run_id.clone(),
            calculation_manifest_sha256: calculation.manifest_sha256.clone(),
            amount: amount.clone(),
            currency: calculation.output_currency.clone(),
            kind: command.kind,
            direction: command.direction,
            scope: command.scope.trim().into(),
            rationale: command.rationale.trim().into(),
            commercial_appetite,
            exclusions,
            qualifications,
            supersedes_adjustment_manifest_sha256: remediation
                .as_ref()
                .map(|value| value.0.clone()),
            remediates_review_manifest_sha256: remediation.map(|value| value.1),
            created_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "pricing_adjustment_created",
            baseline.tender_revision,
            json!({
                "adjustment_id": adjustment_id,
                "baseline_id": baseline.baseline_id,
                "calculation_run_id": calculation.calculation_run_id,
                "kind": command.kind.as_str(),
                "manifest_sha256": manifest_sha256,
                "remediates_review_manifest_sha256": manifest.remediates_review_manifest_sha256,
                "supersedes_adjustment_manifest_sha256": manifest.supersedes_adjustment_manifest_sha256,
                "version": "1",
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO pricing_adjustments (adjustment_id, created_at) VALUES (?1, ?2)",
                params![adjustment_id, created_at],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO pricing_adjustment_versions (
                   adjustment_id, version, tender_revision, baseline_id, baseline_version,
                   calculation_run_id, calculation_manifest_sha256, kind, direction,
                   audit_sequence, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    adjustment_id,
                    baseline.tender_revision,
                    baseline.baseline_id,
                    baseline.version,
                    calculation.calculation_run_id,
                    calculation.manifest_sha256,
                    command.kind.as_str(),
                    command.direction.as_str(),
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO pricing_adjustment_heads (adjustment_id, current_version) VALUES (?1, 1)",
                [&adjustment_id],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        load_pricing_adjustment_with_check(&self.connection, &adjustment_id, 1, &mut || {
            budget.check()
        })
    }

    pub(crate) fn prepare_pricing_adjustment_review_run(
        &mut self,
        tender_id: &TenderId,
        adjustment_id: &str,
        version: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        if !self.active_change_allows_pricing_object(adjustment_id, version)? {
            self.require_change_intake_writable()?;
        }
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
            let adjustment = load_pricing_adjustment_with_check(
                &transaction,
                adjustment_id,
                version,
                &mut || budget.check(),
            )?;
            if !adjustment.current || adjustment.review.is_some() || adjustment.approval.is_some() {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let baseline = load_priced_cost_baseline_with_check(
                &transaction,
                &adjustment.baseline_id,
                adjustment.baseline_version,
                &mut || budget.check(),
            )?;
            let calculation = approved_calculation_run_for_estimate(
                &transaction,
                &adjustment.calculation_run_id,
                &mut || budget.check(),
            )?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let calculation_author_profile_id: String = transaction
                .query_row(
                    "SELECT profile_id FROM agent_runs WHERE run_id = ?1",
                    [&calculation.cost_estimator_run_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let active = active_estimate_profile(
                &transaction,
                PRICED_COST_BASELINE_REVIEW_CAPABILITY,
                Some(&calculation_author_profile_id),
            )?;
            if active.tender_revision != adjustment.tender_revision {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = adjustment_review_task(
                random_identifier(&transaction)?,
                tender_id.as_str(),
                &adjustment,
                &active.plan_id,
                active.plan_version,
                &active.profile,
                deadline.clone(),
            )?;
            let payload = adjustment_review_payload(
                tender_id.as_str(),
                &active.tender_name,
                &adjustment_review_snapshot(adjustment.clone()),
                &approved_baseline_snapshot(baseline.clone()),
                &calculation,
                &active.profile,
            )?;
            if canonical_json(&payload)?.len() > MAX_REVIEW_PAYLOAD_BYTES {
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
                    started_event_summary: "Independent Pricing Adjustment review started",
                    audit_event_type: "pricing_adjustment_review_started",
                    audit_payload: json!({
                        "adjustment_id": adjustment.adjustment_id,
                        "adjustment_version": adjustment.version.to_string(),
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
            let _ = fs::remove_dir_all(workspace);
        }
        prepared
    }

    pub(crate) fn validate_pricing_adjustment_review_candidate(
        &self,
        task: &TenderTaskView,
        payload: &str,
    ) -> Result<PricedCostBaselineReviewCandidate, TenderCommandError> {
        if payload.len() > 128 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        exact_adjustment_review_target(task)?;
        let candidate: PricedCostBaselineReviewCandidate = serde_json::from_str(payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        candidate_is_valid(&candidate)
            .then_some(candidate)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }

    pub(crate) fn pricing_adjustment_review_target_is_open(
        &self,
        task: &TenderTaskView,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        adjustment_review_target_is_open(&self.connection, task, check)
    }

    pub(crate) fn load_pricing_adjustment(
        &self,
        adjustment_id: &str,
        version: u32,
    ) -> Result<PricingAdjustmentVersion, TenderCommandError> {
        load_pricing_adjustment_with_check(&self.connection, adjustment_id, version, &mut || Ok(()))
    }

    pub(crate) fn approve_pricing_adjustment(
        &mut self,
        tender_id: &TenderId,
        command: &ApprovePricingAdjustmentCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PricingAdjustmentVersion, TenderCommandError> {
        if !self.active_change_allows_pricing_object(&command.adjustment_id, command.version)? {
            self.require_change_intake_writable()?;
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let adjustment = load_pricing_adjustment_with_check(
            &transaction,
            &command.adjustment_id,
            command.version,
            &mut || budget.check(),
        )?;
        let review = adjustment
            .review
            .as_ref()
            .filter(|review| review.outcome == PricedCostBaselineReviewOutcome::Passed)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if !adjustment.current
            || adjustment.approval.is_some()
            || adjustment.manifest_sha256 != command.manifest_sha256
            || !valid_text(&command.rationale, MAX_TEXT_BYTES)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = PricingAdjustmentApprovalManifest {
            schema_version: 1,
            approval_id: approval_id.clone(),
            adjustment_id: adjustment.adjustment_id.clone(),
            adjustment_version: adjustment.version,
            adjustment_manifest_sha256: adjustment.manifest_sha256.clone(),
            review_id: review.review_id.clone(),
            review_manifest_sha256: review.manifest_sha256.clone(),
            rationale: command.rationale.trim().into(),
            approved_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            tender_revision: adjustment.tender_revision,
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "pricing_adjustment_approved",
            adjustment.tender_revision,
            json!({
                "adjustment_id": adjustment.adjustment_id,
                "adjustment_version": adjustment.version.to_string(),
                "approval_id": approval_id,
                "manifest_sha256": manifest_sha256,
                "review_id": review.review_id,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO pricing_adjustment_approvals (
                   approval_id, adjustment_id, adjustment_version,
                   adjustment_manifest_sha256, review_id, review_manifest_sha256,
                   rationale, approved_by, acting_role, audit_sequence,
                   manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'engineer_user',
                           'tendering_manager', ?8, ?9, ?10, ?11)",
                params![
                    approval_id,
                    adjustment.adjustment_id,
                    adjustment.version,
                    adjustment.manifest_sha256,
                    review.review_id,
                    review.manifest_sha256,
                    command.rationale.trim(),
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        load_pricing_adjustment_with_check(
            &self.connection,
            &command.adjustment_id,
            command.version,
            &mut || budget.check(),
        )
    }
}
