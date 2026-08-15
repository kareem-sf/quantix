use std::{fs, path::Path, str::FromStr};

use garde::Validate;
use jiff::{civil::Date, Timestamp};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{
    de::{Error as _, SeqAccess},
    Deserialize, Deserializer, Serialize,
};
use serde_json::json;
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
    append_audit_event_with_sequence, lock_mutex_with_check, random_identifier, sha256_hex,
    sql_error, sqlite_timestamp,
    tender_queries::query_evidence_reference_exists,
    BidPackageOperationBudget, QuantixHost, TenderCommandError, TenderErrorCode, TenderId,
    TenderStore, WorkPlanProfileBinding,
};

const MAX_DECIMAL_BYTES: usize = 64;
const CALCULATION_ENGINE_VERSION: &str = "quantix-exact-decimal/1";
const BOQ_RULE_NAME: &str = "Controlled BOQ line and estimate aggregation";
const CONTROLLED_CALCULATION_RULE_FORMULA: &str = "line=convert(quantity, quantity_unit, rate_basis_unit) × unit_rate × exchange_rate; aggregate=sum(approved_line_final_amounts); pricing=round(baseline + sum(add_adjustments) - sum(deduct_adjustments), approved_precision, approved_rounding_policy)";
const MAX_CALCULATION_RUNS: u32 = 1_000;
const MAX_CALCULATION_SCENARIOS: u32 = 1_000;
pub(crate) const CALCULATION_RULE_REVIEW_CAPABILITY: &str = "review_cost_estimation";
pub(crate) const COST_ESTIMATION_CAPABILITY: &str = "cost_estimation";
const BOQ_RULE_SUPPORTED_CURRENCIES: &[&str] = &[
    "AED", "AFN", "ALL", "AMD", "ANG", "AOA", "ARS", "AUD", "AWG", "AZN", "BAM", "BBD", "BDT",
    "BGN", "BHD", "BIF", "BMD", "BND", "BOB", "BOV", "BRL", "BSD", "BTN", "BWP", "BYN", "BZD",
    "CAD", "CDF", "CHE", "CHF", "CHW", "CLF", "CLP", "CNY", "COP", "COU", "CRC", "CUP", "CVE",
    "CZK", "DJF", "DKK", "DOP", "DZD", "EGP", "ERN", "ETB", "EUR", "FJD", "FKP", "GBP", "GEL",
    "GHS", "GIP", "GMD", "GNF", "GTQ", "GYD", "HKD", "HNL", "HTG", "HUF", "IDR", "ILS", "INR",
    "IQD", "IRR", "ISK", "JMD", "JOD", "JPY", "KES", "KGS", "KHR", "KMF", "KPW", "KRW", "KWD",
    "KYD", "KZT", "LAK", "LBP", "LKR", "LRD", "LSL", "LYD", "MAD", "MDL", "MGA", "MKD", "MMK",
    "MNT", "MOP", "MRU", "MUR", "MVR", "MWK", "MXN", "MXV", "MYR", "MZN", "NAD", "NGN", "NIO",
    "NOK", "NPR", "NZD", "OMR", "PAB", "PEN", "PGK", "PHP", "PKR", "PLN", "PYG", "QAR", "RON",
    "RSD", "RUB", "RWF", "SAR", "SBD", "SCR", "SDG", "SEK", "SGD", "SHP", "SLE", "SOS", "SRD",
    "SSP", "STN", "SVC", "SYP", "SZL", "THB", "TJS", "TMT", "TND", "TOP", "TRY", "TTD", "TWD",
    "TZS", "UAH", "UGX", "USD", "USN", "UYI", "UYU", "UYW", "UZS", "VED", "VES", "VND", "VUV",
    "WST", "XAF", "XAG", "XAU", "XBA", "XBB", "XBC", "XBD", "XCD", "XDR", "XOF", "XPD", "XPF",
    "XPT", "XSU", "XTS", "XUA", "XXX", "YER", "ZAR", "ZMW", "ZWG",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CalculationRoundingMode {
    MidpointAwayFromZero,
    MidpointNearestEven,
}

impl CalculationRoundingMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::MidpointAwayFromZero => "midpoint_away_from_zero",
            Self::MidpointNearestEven => "midpoint_nearest_even",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "midpoint_away_from_zero" => Ok(Self::MidpointAwayFromZero),
            "midpoint_nearest_even" => Ok(Self::MidpointNearestEven),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExchangeRateType {
    Spot,
    Contract,
    Budget,
    CentralBank,
}

impl ExchangeRateType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Spot => "spot",
            Self::Contract => "contract",
            Self::Budget => "budget",
            Self::CentralBank => "central_bank",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "spot" => Ok(Self::Spot),
            "contract" => Ok(Self::Contract),
            "budget" => Ok(Self::Budget),
            "central_bank" => Ok(Self::CentralBank),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

fn deserialize_rounding_policy<'de, D>(
    deserializer: D,
) -> Result<Vec<CalculationRoundingMode>, D::Error>
where
    D: Deserializer<'de>,
{
    struct RoundingPolicyVisitor;

    impl<'de> serde::de::Visitor<'de> for RoundingPolicyVisitor {
        type Value = Vec<CalculationRoundingMode>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("one or two distinct controlled rounding modes")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let first = sequence
                .next_element()?
                .ok_or_else(|| serde::de::Error::custom("a rounding mode is required"))?;
            let second = sequence.next_element()?;
            if sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(serde::de::Error::custom(
                    "at most two rounding modes are allowed",
                ));
            }
            Ok(second.map_or_else(|| vec![first], |second| vec![first, second]))
        }
    }

    deserializer.deserialize_seq(RoundingPolicyVisitor)
}

fn validate_rounding_policy_command(values: &[CalculationRoundingMode], _: &()) -> garde::Result {
    if !(1..=2).contains(&values.len())
        || values.windows(2).any(|pair| pair[0] == pair[1])
        || values
            .iter()
            .any(|value| !supported_rounding().contains(value))
    {
        return Err(garde::Error::new("invalid controlled rounding policy"));
    }
    Ok(())
}

fn deserialize_bounded_string<'de, D, const MAX: usize>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoundedStringVisitor<const MAX: usize>;

    impl<const MAX: usize> serde::de::Visitor<'_> for BoundedStringVisitor<MAX> {
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
                .ok_or_else(|| E::custom("string exceeds the command boundary"))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            (value.len() <= MAX)
                .then_some(value)
                .ok_or_else(|| E::custom("string exceeds the command boundary"))
        }
    }

    deserializer.deserialize_string(BoundedStringVisitor::<MAX>)
}

fn deserialize_optional_decimal<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionalDecimalVisitor;

    impl<'de> serde::de::Visitor<'de> for OptionalDecimalVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("null or a bounded exact decimal string")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_bounded_string::<D, MAX_DECIMAL_BYTES>(deserializer).map(Some)
        }
    }

    deserializer.deserialize_option(OptionalDecimalVisitor)
}

fn deserialize_evidence_kind<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_string::<D, 64>(deserializer)
}

fn deserialize_evidence_reference<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_string::<D, 400>(deserializer)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundedCalculationEvidenceReference {
    #[serde(deserialize_with = "deserialize_evidence_kind")]
    kind: String,
    #[serde(deserialize_with = "deserialize_evidence_reference")]
    reference: String,
    version: u32,
}

impl From<BoundedCalculationEvidenceReference> for AgentTaskInputReference {
    fn from(value: BoundedCalculationEvidenceReference) -> Self {
        Self {
            kind: value.kind,
            reference: value.reference,
            version: value.version,
        }
    }
}

fn deserialize_calculation_evidence<'de, D>(
    deserializer: D,
) -> Result<Vec<AgentTaskInputReference>, D::Error>
where
    D: Deserializer<'de>,
{
    struct EvidenceVisitor;

    impl<'de> serde::de::Visitor<'de> for EvidenceVisitor {
        type Value = Vec<AgentTaskInputReference>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("at most 32 bounded Evidence references")
        }

        fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut result = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(32));
            while let Some(reference) =
                sequence.next_element::<BoundedCalculationEvidenceReference>()?
            {
                if result.len() == 32 {
                    return Err(A::Error::custom("too many Evidence references"));
                }
                result.push(reference.into());
            }
            Ok(result)
        }
    }

    deserializer.deserialize_seq(EvidenceVisitor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CalculationInputState {
    Provided,
    Missing,
    Unavailable,
    Ambiguous,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalculationDecimalInput {
    pub state: CalculationInputState,
    #[serde(deserialize_with = "deserialize_optional_decimal")]
    pub value: Option<String>,
    #[serde(deserialize_with = "deserialize_calculation_evidence")]
    pub evidence: Vec<AgentTaskInputReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ControlledBoqCalculationStatus {
    Completed,
    MissingInput,
    UnavailableInput,
    AmbiguousInput,
    InvalidInput,
    DimensionMismatch,
}

impl ControlledBoqCalculationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::MissingInput => "missing_input",
            Self::UnavailableInput => "unavailable_input",
            Self::AmbiguousInput => "ambiguous_input",
            Self::InvalidInput => "invalid_input",
            Self::DimensionMismatch => "dimension_mismatch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CalculationRuleReviewOutcome {
    Passed,
    Failed,
}

impl CalculationRuleReviewOutcome {
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
pub struct CalculationRuleReviewFinding {
    pub code: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalculationRuleReview {
    pub review_id: String,
    pub reviewer_run_id: String,
    pub reviewer_profile_id: String,
    pub reviewer_profile_version: u32,
    pub outcome: CalculationRuleReviewOutcome,
    pub findings: Vec<CalculationRuleReviewFinding>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalculationRuleApproval {
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
pub struct CalculationRuleTestResult {
    pub case_name: String,
    pub expected_final_amount: String,
    pub actual_final_amount: String,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalculationRuleVersion {
    pub rule_id: String,
    pub version: u32,
    pub name: String,
    pub formula: String,
    pub engine_version: String,
    pub supported_units: Vec<String>,
    pub supported_currencies: Vec<String>,
    pub supported_rounding: Vec<CalculationRoundingMode>,
    pub change_rationale: String,
    pub deterministic_tests: Vec<CalculationRuleTestResult>,
    pub review: Option<CalculationRuleReview>,
    pub approval: Option<CalculationRuleApproval>,
    pub current: bool,
    pub active: bool,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ControlledBoqCalculationRun {
    pub calculation_run_id: String,
    pub cost_estimator_run_id: String,
    pub tender_revision: u32,
    pub rule_id: String,
    pub rule_version: u32,
    pub rule_approval_id: String,
    pub description: String,
    pub scenario_id: String,
    pub scenario_version: u32,
    pub scenario_name: String,
    pub scenario_manifest_sha256: String,
    pub exchange_rate_id: String,
    pub exchange_rate_version: u32,
    pub rounding_policy_id: String,
    pub rounding_policy_version: u32,
    pub quantity: CalculationDecimalInput,
    pub quantity_unit: String,
    pub unit_rate: CalculationDecimalInput,
    pub rate_basis_unit: String,
    pub rate_currency: String,
    pub exchange_rate: CalculationDecimalInput,
    pub exchange_rate_effective_date: Option<String>,
    pub pricing_date: String,
    pub exchange_rate_type: Option<ExchangeRateType>,
    pub output_currency: String,
    pub precision: u32,
    pub rounding_mode: CalculationRoundingMode,
    pub engine_version: String,
    pub normalized_quantity: Option<String>,
    pub unrounded_source_amount: Option<String>,
    pub unrounded_output_amount: Option<String>,
    pub final_amount: Option<String>,
    pub status: ControlledBoqCalculationStatus,
    pub diagnostic_code: Option<String>,
    pub manifest_sha256: String,
    pub approval: Option<CalculationRunApproval>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateAggregateCalculationInput {
    pub build_up_id: String,
    pub cbs_component_id: String,
    pub calculation_run_id: String,
    pub calculation_manifest_sha256: String,
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct EstimateAggregateCalculationRun {
    pub aggregate_run_id: String,
    pub author_run_id: String,
    pub comparison_total_calculation_run_id: String,
    pub comparison_total_manifest_sha256: String,
    pub comparison_total_amount: String,
    pub rule_id: String,
    pub rule_version: u32,
    pub rule_approval_id: String,
    pub scenario_id: String,
    pub scenario_version: u32,
    pub precision: u32,
    pub rounding_mode: CalculationRoundingMode,
    pub engine_version: String,
    pub inputs: Vec<EstimateAggregateCalculationInput>,
    pub final_amount: String,
    pub currency: String,
    pub manifest_sha256: String,
    pub approved_for_reliance: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PricingAdjustmentDirection {
    Add,
    Deduct,
}

impl PricingAdjustmentDirection {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Deduct => "deduct",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingCalculationAdjustmentInput {
    pub adjustment_id: String,
    pub adjustment_version: u32,
    pub adjustment_manifest_sha256: String,
    pub calculation_run_id: String,
    pub calculation_manifest_sha256: String,
    pub direction: PricingAdjustmentDirection,
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct PricingCalculationRun {
    pub pricing_calculation_run_id: String,
    pub tender_revision: u32,
    pub baseline_aggregate_run_id: String,
    pub baseline_aggregate_manifest_sha256: String,
    pub baseline_amount: String,
    pub adjustments: Vec<PricingCalculationAdjustmentInput>,
    pub rule_id: String,
    pub rule_version: u32,
    pub rule_approval_id: String,
    pub scenario_id: String,
    pub scenario_version: u32,
    pub precision: u32,
    pub rounding_mode: CalculationRoundingMode,
    pub engine_version: String,
    pub final_amount: String,
    pub currency: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PricingCalculationManifest {
    schema_version: u32,
    pricing_calculation_run_id: String,
    tender_revision: u32,
    baseline_aggregate_run_id: String,
    baseline_aggregate_manifest_sha256: String,
    baseline_amount: String,
    adjustments: Vec<PricingCalculationAdjustmentInput>,
    rule_id: String,
    rule_version: u32,
    rule_approval_id: String,
    scenario_id: String,
    scenario_version: u32,
    precision: u32,
    rounding_mode: CalculationRoundingMode,
    engine_version: String,
    final_amount: String,
    currency: String,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalculationRunApproval {
    pub approval_id: String,
    pub calculation_run_id: String,
    pub run_manifest_sha256: String,
    pub rationale: String,
    pub approved_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalculationWorkspaceInspection {
    pub rule: Option<CalculationRuleVersion>,
    pub recent_scenarios: Vec<CalculationScenarioVersion>,
    pub recent_runs: Vec<ControlledBoqCalculationRun>,
    pub total_scenario_count: u32,
    pub total_run_count: u32,
    pub scenario_offset: u32,
    pub run_offset: u32,
    pub has_older_scenarios: bool,
    pub has_older_runs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalculationScenarioVersion {
    pub scenario_id: String,
    pub version: u32,
    pub name: String,
    pub quantity_unit: String,
    pub rate_basis_unit: String,
    pub rate_currency: String,
    pub exchange_rate_id: String,
    pub exchange_rate_version: u32,
    pub exchange_rate: CalculationDecimalInput,
    pub exchange_rate_effective_date: Option<String>,
    pub pricing_date: String,
    pub exchange_rate_type: Option<ExchangeRateType>,
    pub output_currency: String,
    pub rounding_policy_id: String,
    pub rounding_policy_version: u32,
    pub precision: u32,
    pub rounding_mode: CalculationRoundingMode,
    pub rationale: String,
    pub approved_by: String,
    pub acting_role: String,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ProposeBoqCalculationRuleCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[serde(deserialize_with = "deserialize_rounding_policy")]
    #[garde(custom(validate_rounding_policy_command))]
    pub supported_rounding: Vec<CalculationRoundingMode>,
    #[garde(length(bytes, min = 1, max = 2_000))]
    pub change_rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunCalculationRuleReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub rule_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CalculationRuleReviewResult {
    pub run: AgentRunInspection,
    pub rule: CalculationRuleVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApproveCalculationRuleCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub rule_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateCalculationScenarioCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub name: String,
    #[garde(length(bytes, min = 1, max = 16))]
    pub quantity_unit: String,
    #[garde(length(bytes, min = 1, max = 16))]
    pub rate_basis_unit: String,
    #[garde(length(bytes, min = 3, max = 3))]
    pub rate_currency: String,
    #[garde(skip)]
    pub exchange_rate: CalculationDecimalInput,
    #[garde(skip)]
    pub exchange_rate_effective_date: Option<String>,
    #[garde(length(bytes, min = 10, max = 10))]
    pub pricing_date: String,
    #[garde(skip)]
    pub exchange_rate_type: Option<ExchangeRateType>,
    #[garde(length(bytes, min = 3, max = 3))]
    pub output_currency: String,
    #[garde(range(max = 12))]
    pub precision: u32,
    #[garde(skip)]
    pub rounding_mode: CalculationRoundingMode,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ApproveControlledBoqCalculationRunCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub calculation_run_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 1, max = 4_000))]
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunCostEstimatorCalculationCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub scenario_id: String,
    #[garde(range(min = 1, max = 1))]
    pub scenario_version: u32,
    #[garde(length(bytes, min = 1, max = 1000))]
    pub description: String,
    #[serde(deserialize_with = "deserialize_calculation_evidence")]
    #[garde(skip)]
    pub quantity_evidence: Vec<AgentTaskInputReference>,
    #[serde(deserialize_with = "deserialize_calculation_evidence")]
    #[garde(skip)]
    pub unit_rate_evidence: Vec<AgentTaskInputReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CostEstimatorCalculationResult {
    pub run: AgentRunInspection,
    pub calculation: Option<ControlledBoqCalculationRun>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectCalculationWorkspaceCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(range(max = 1_000))]
    pub scenario_offset: u32,
    #[garde(range(max = 1_000))]
    pub run_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalculationRuleReviewCandidate {
    pub outcome: CalculationRuleReviewOutcome,
    pub findings: Vec<CalculationRuleReviewFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CostEstimatorCalculationCandidate {
    pub quantity: CalculationDecimalInput,
    pub unit_rate: CalculationDecimalInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExactBoqResult {
    normalized_quantity: String,
    unrounded_source_amount: String,
    unrounded_output_amount: String,
    final_amount: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExactBoqError {
    InvalidDecimal,
    NegativeValue,
    UnknownUnit,
    DimensionMismatch,
    InvalidCurrency,
    InvalidPrecision,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitDimension {
    Count,
    Length,
    Area,
    Volume,
    Mass,
    Time,
}

#[derive(Debug, Clone, Copy)]
struct UnitDefinition {
    dimension: UnitDimension,
    base_factor: Decimal,
}

fn unit_definition(unit: &str) -> Option<UnitDefinition> {
    let (dimension, numerator, scale) = match unit {
        "each" => (UnitDimension::Count, 1, 0),
        "mm" => (UnitDimension::Length, 1, 3),
        "m" => (UnitDimension::Length, 1, 0),
        "mm2" => (UnitDimension::Area, 1, 6),
        "m2" => (UnitDimension::Area, 1, 0),
        "mm3" => (UnitDimension::Volume, 1, 9),
        "m3" => (UnitDimension::Volume, 1, 0),
        "kg" => (UnitDimension::Mass, 1, 0),
        "t" => (UnitDimension::Mass, 1_000, 0),
        "min" => (UnitDimension::Time, 1, 0),
        "h" => (UnitDimension::Time, 60, 0),
        _ => return None,
    };
    Some(UnitDefinition {
        dimension,
        base_factor: Decimal::new(numerator, scale),
    })
}

fn parse_nonnegative_decimal(value: &str) -> Result<Decimal, ExactBoqError> {
    if value.is_empty()
        || value.len() > MAX_DECIMAL_BYTES
        || value.trim() != value
        || value.contains(['e', 'E'])
    {
        return Err(ExactBoqError::InvalidDecimal);
    }
    let decimal = Decimal::from_str(value).map_err(|_| ExactBoqError::InvalidDecimal)?;
    if decimal.is_sign_negative() {
        return Err(ExactBoqError::NegativeValue);
    }
    Ok(decimal)
}

fn exact_string(value: Decimal) -> String {
    value.normalize().to_string()
}

#[allow(clippy::too_many_arguments)]
fn evaluate_boq_line(
    quantity: &str,
    quantity_unit: &str,
    unit_rate: &str,
    rate_basis_unit: &str,
    rate_currency: &str,
    exchange_rate: &str,
    output_currency: &str,
    precision: u32,
    rounding_mode: CalculationRoundingMode,
) -> Result<ExactBoqResult, ExactBoqError> {
    if precision > 12 {
        return Err(ExactBoqError::InvalidPrecision);
    }
    if !valid_currency(rate_currency) || !valid_currency(output_currency) {
        return Err(ExactBoqError::InvalidCurrency);
    }
    let quantity = parse_nonnegative_decimal(quantity)?;
    let unit_rate = parse_nonnegative_decimal(unit_rate)?;
    let exchange_rate = parse_nonnegative_decimal(exchange_rate)?;
    let quantity_unit = unit_definition(quantity_unit).ok_or(ExactBoqError::UnknownUnit)?;
    let rate_basis_unit = unit_definition(rate_basis_unit).ok_or(ExactBoqError::UnknownUnit)?;
    if quantity_unit.dimension != rate_basis_unit.dimension {
        return Err(ExactBoqError::DimensionMismatch);
    }
    if exchange_rate.is_zero() {
        return Err(ExactBoqError::InvalidDecimal);
    }

    let normalized_quantity = quantity
        .checked_mul(quantity_unit.base_factor)
        .and_then(|value| value.checked_div(rate_basis_unit.base_factor))
        .ok_or(ExactBoqError::ArithmeticOverflow)?;
    let source_amount = normalized_quantity
        .checked_mul(unit_rate)
        .ok_or(ExactBoqError::ArithmeticOverflow)?;
    let output_amount = source_amount
        .checked_mul(exchange_rate)
        .ok_or(ExactBoqError::ArithmeticOverflow)?;
    let strategy = match rounding_mode {
        CalculationRoundingMode::MidpointAwayFromZero => RoundingStrategy::MidpointAwayFromZero,
        CalculationRoundingMode::MidpointNearestEven => RoundingStrategy::MidpointNearestEven,
    };
    let rounded = output_amount.round_dp_with_strategy(precision, strategy);

    Ok(ExactBoqResult {
        normalized_quantity: exact_string(normalized_quantity),
        unrounded_source_amount: exact_string(source_amount),
        unrounded_output_amount: exact_string(output_amount),
        final_amount: format!("{rounded:.precision$}", precision = precision as usize),
    })
}

fn valid_currency(value: &str) -> bool {
    BOQ_RULE_SUPPORTED_CURRENCIES.binary_search(&value).is_ok()
}

pub(crate) fn valid_estimate_currency(value: &str) -> bool {
    valid_currency(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CalculationRuleManifest {
    schema_version: u32,
    rule_id: String,
    version: u32,
    name: String,
    formula: String,
    engine_version: String,
    supported_units: Vec<String>,
    supported_currencies: Vec<String>,
    supported_rounding: Vec<CalculationRoundingMode>,
    change_rationale: String,
    deterministic_tests: Vec<CalculationRuleTestResult>,
    created_by: String,
    created_at: String,
}

#[derive(Serialize, Deserialize)]
struct CalculationRuleApprovalManifest {
    schema_version: u32,
    approval_id: String,
    rule_id: String,
    rule_version: u32,
    rule_manifest_sha256: String,
    review_id: String,
    review_manifest_sha256: String,
    rationale: String,
    approved_by: String,
    acting_role: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CalculationScenarioManifest {
    schema_version: u32,
    scenario_id: String,
    version: u32,
    name: String,
    quantity_unit: String,
    rate_basis_unit: String,
    rate_currency: String,
    exchange_rate_id: String,
    exchange_rate_version: u32,
    exchange_rate: CalculationDecimalInput,
    exchange_rate_effective_date: Option<String>,
    pricing_date: String,
    exchange_rate_type: Option<ExchangeRateType>,
    output_currency: String,
    rounding_policy_id: String,
    rounding_policy_version: u32,
    precision: u32,
    rounding_mode: CalculationRoundingMode,
    rationale: String,
    approved_by: String,
    acting_role: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CalculationRunManifest {
    schema_version: u32,
    calculation_run_id: String,
    cost_estimator_run_id: String,
    tender_revision: u32,
    rule_id: String,
    rule_version: u32,
    rule_approval_id: String,
    rule_manifest_sha256: String,
    rule_approval_manifest_sha256: String,
    description: String,
    scenario_id: String,
    scenario_version: u32,
    scenario_name: String,
    scenario_manifest_sha256: String,
    exchange_rate_id: String,
    exchange_rate_version: u32,
    rounding_policy_id: String,
    rounding_policy_version: u32,
    quantity: CalculationDecimalInput,
    quantity_unit: String,
    unit_rate: CalculationDecimalInput,
    rate_basis_unit: String,
    rate_currency: String,
    exchange_rate: CalculationDecimalInput,
    exchange_rate_effective_date: Option<String>,
    pricing_date: String,
    exchange_rate_type: Option<ExchangeRateType>,
    output_currency: String,
    precision: u32,
    rounding_mode: CalculationRoundingMode,
    engine_version: String,
    normalized_quantity: Option<String>,
    unrounded_source_amount: Option<String>,
    unrounded_output_amount: Option<String>,
    final_amount: Option<String>,
    status: ControlledBoqCalculationStatus,
    diagnostic_code: Option<String>,
    created_at: String,
}

#[derive(Serialize, Deserialize)]
struct CalculationRunApprovalManifest {
    schema_version: u32,
    approval_id: String,
    calculation_run_id: String,
    run_manifest_sha256: String,
    rationale: String,
    approved_by: String,
    acting_role: String,
    tender_revision: u32,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EstimateAggregateCalculationManifest {
    schema_version: u32,
    aggregate_run_id: String,
    author_run_id: String,
    comparison_total_calculation_run_id: String,
    comparison_total_manifest_sha256: String,
    comparison_total_amount: String,
    tender_revision: u32,
    rule_id: String,
    rule_version: u32,
    rule_approval_id: String,
    rule_manifest_sha256: String,
    rule_approval_manifest_sha256: String,
    scenario_id: String,
    scenario_version: u32,
    scenario_manifest_sha256: String,
    precision: u32,
    rounding_mode: CalculationRoundingMode,
    engine_version: String,
    inputs: Vec<EstimateAggregateCalculationInput>,
    final_amount: String,
    currency: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EstimateAggregateCalculationApprovalManifest {
    schema_version: u32,
    approval_id: String,
    aggregate_run_id: String,
    aggregate_manifest_sha256: String,
    basis_id: String,
    basis_version: u32,
    basis_manifest_sha256: String,
    rationale: String,
    approved_by: String,
    acting_role: String,
    created_at: String,
}

type StoredCalculationRuleRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    bool,
);

type StoredCalculationRuleReviewRow = (String, String, String, u32, String, String, String, String);

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn parse_canonical<T>(value: &str) -> Result<T, TenderCommandError>
where
    T: serde::de::DeserializeOwned + Serialize,
{
    let parsed: T = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(parsed)
}

fn supported_units() -> Vec<String> {
    [
        "each", "mm", "m", "mm2", "m2", "mm3", "m3", "kg", "t", "min", "h",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn supported_currencies() -> Vec<String> {
    BOQ_RULE_SUPPORTED_CURRENCIES
        .iter()
        .copied()
        .map(str::to_owned)
        .collect()
}

fn supported_rounding() -> Vec<CalculationRoundingMode> {
    vec![
        CalculationRoundingMode::MidpointAwayFromZero,
        CalculationRoundingMode::MidpointNearestEven,
    ]
}

fn normalize_rounding_policy(
    values: &[CalculationRoundingMode],
) -> Option<Vec<CalculationRoundingMode>> {
    let mut normalized = values.to_vec();
    normalized.sort_by_key(|value| value.as_str());
    normalized.dedup();
    (normalized.len() == values.len()
        && !normalized.is_empty()
        && normalized
            .iter()
            .all(|value| supported_rounding().contains(value)))
    .then_some(normalized)
}

fn valid_change_rationale(value: &str) -> bool {
    !value.is_empty() && value.len() <= 2_000 && value.trim() == value
}

fn valid_iso_date(value: &str) -> bool {
    value.len() == 10 && Date::from_str(value).is_ok_and(|date| date.to_string().as_str() == value)
}

fn evaluate_pricing_amount(
    baseline_amount: &str,
    adjustments: &[(PricingAdjustmentDirection, &str)],
    precision: u32,
    rounding: CalculationRoundingMode,
) -> Option<String> {
    let mut total = Decimal::from_str(baseline_amount).ok()?;
    for (direction, value) in adjustments {
        let amount = Decimal::from_str(value).ok()?;
        total = match direction {
            PricingAdjustmentDirection::Add => total.checked_add(amount),
            PricingAdjustmentDirection::Deduct => total.checked_sub(amount),
        }?;
    }
    if total.is_sign_negative() {
        return None;
    }
    let strategy = match rounding {
        CalculationRoundingMode::MidpointAwayFromZero => RoundingStrategy::MidpointAwayFromZero,
        CalculationRoundingMode::MidpointNearestEven => RoundingStrategy::MidpointNearestEven,
    };
    let rounded = total.round_dp_with_strategy(precision, strategy);
    Some(format!(
        "{rounded:.precision$}",
        precision = precision as usize
    ))
}

fn deterministic_rule_tests() -> Result<Vec<CalculationRuleTestResult>, TenderCommandError> {
    struct Case {
        name: &'static str,
        quantity: &'static str,
        quantity_unit: &'static str,
        rate: &'static str,
        rate_unit: &'static str,
        exchange: &'static str,
        precision: u32,
        rounding: CalculationRoundingMode,
        expected: &'static str,
    }
    let cases = [
        Case {
            name: "integer multiplication",
            quantity: "2",
            quantity_unit: "each",
            rate: "3",
            rate_unit: "each",
            exchange: "1",
            precision: 2,
            rounding: CalculationRoundingMode::MidpointAwayFromZero,
            expected: "6.00",
        },
        Case {
            name: "millimetres to metres and currency conversion",
            quantity: "1250",
            quantity_unit: "mm",
            rate: "2.40",
            rate_unit: "m",
            exchange: "50",
            precision: 2,
            rounding: CalculationRoundingMode::MidpointAwayFromZero,
            expected: "150.00",
        },
        Case {
            name: "midpoint away from zero",
            quantity: "1",
            quantity_unit: "each",
            rate: "1.005",
            rate_unit: "each",
            exchange: "1",
            precision: 2,
            rounding: CalculationRoundingMode::MidpointAwayFromZero,
            expected: "1.01",
        },
        Case {
            name: "midpoint nearest even",
            quantity: "1",
            quantity_unit: "each",
            rate: "1.005",
            rate_unit: "each",
            exchange: "1",
            precision: 2,
            rounding: CalculationRoundingMode::MidpointNearestEven,
            expected: "1.00",
        },
    ];
    let mut results = cases
        .into_iter()
        .map(|case| {
            let actual = evaluate_boq_line(
                case.quantity,
                case.quantity_unit,
                case.rate,
                case.rate_unit,
                "USD",
                case.exchange,
                "EGP",
                case.precision,
                case.rounding,
            )
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .final_amount;
            Ok(CalculationRuleTestResult {
                case_name: case.name.into(),
                expected_final_amount: case.expected.into(),
                passed: actual == case.expected,
                actual_final_amount: actual,
            })
        })
        .collect::<Result<Vec<_>, TenderCommandError>>()?;
    struct PricingCase {
        name: &'static str,
        baseline: &'static str,
        adjustments: &'static [(PricingAdjustmentDirection, &'static str)],
        precision: u32,
        rounding: CalculationRoundingMode,
        expected: &'static str,
    }
    let pricing_cases = [
        PricingCase {
            name: "pricing add and deduct",
            baseline: "100.00",
            adjustments: &[
                (PricingAdjustmentDirection::Add, "12.50"),
                (PricingAdjustmentDirection::Deduct, "2.25"),
            ],
            precision: 2,
            rounding: CalculationRoundingMode::MidpointAwayFromZero,
            expected: "110.25",
        },
        PricingCase {
            name: "pricing midpoint away from zero",
            baseline: "1.005",
            adjustments: &[],
            precision: 2,
            rounding: CalculationRoundingMode::MidpointAwayFromZero,
            expected: "1.01",
        },
        PricingCase {
            name: "pricing midpoint nearest even",
            baseline: "1.005",
            adjustments: &[],
            precision: 2,
            rounding: CalculationRoundingMode::MidpointNearestEven,
            expected: "1.00",
        },
        PricingCase {
            name: "pricing negative total is invalid",
            baseline: "1.00",
            adjustments: &[(PricingAdjustmentDirection::Deduct, "1.01")],
            precision: 2,
            rounding: CalculationRoundingMode::MidpointAwayFromZero,
            expected: "invalid",
        },
    ];
    results.extend(pricing_cases.into_iter().map(|case| {
        let actual = evaluate_pricing_amount(
            case.baseline,
            case.adjustments,
            case.precision,
            case.rounding,
        )
        .unwrap_or_else(|| "invalid".into());
        CalculationRuleTestResult {
            case_name: case.name.into(),
            expected_final_amount: case.expected.into(),
            passed: actual == case.expected,
            actual_final_amount: actual,
        }
    }));
    Ok(results)
}

fn calculation_rule_review_output_contract() -> String {
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "findings": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "code": { "minLength": 1, "maxLength": 100, "pattern": "^[a-z0-9_]+$", "type": "string" },
                        "summary": { "minLength": 1, "maxLength": 2000, "type": "string" }
                    },
                    "required": ["code", "summary"],
                    "type": "object"
                },
                "maxItems": 32,
                "type": "array"
            },
            "outcome": { "enum": ["passed", "failed"] }
        },
        "required": ["outcome", "findings"],
        "type": "object"
    }))
    .expect("static calculation rule review contract is canonical")
}

fn cost_estimator_calculation_output_contract() -> String {
    let input = json!({
        "additionalProperties": false,
        "properties": {
            "evidence": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "kind": { "const": "source_evidence" },
                        "reference": { "minLength": 1, "maxLength": 400, "type": "string" },
                        "version": { "minimum": 1, "type": "integer" }
                    },
                    "required": ["kind", "reference", "version"],
                    "type": "object"
                },
                "maxItems": 32,
                "type": "array"
            },
            "state": { "enum": ["provided", "missing", "unavailable", "ambiguous"] },
            "value": { "anyOf": [{ "maxLength": MAX_DECIMAL_BYTES, "type": "string" }, { "type": "null" }] }
        },
        "required": ["state", "value", "evidence"],
        "type": "object"
    });
    canonical_json(&json!({
        "additionalProperties": false,
        "properties": {
            "quantity": input,
            "unit_rate": input
        },
        "required": ["quantity", "unit_rate"],
        "type": "object"
    }))
    .expect("static Cost Estimator calculation contract is canonical")
}

fn tagged_calculation_evidence(
    kind: &str,
    references: &[AgentTaskInputReference],
) -> Vec<AgentTaskInputReference> {
    references
        .iter()
        .map(|reference| AgentTaskInputReference {
            kind: kind.into(),
            reference: reference.reference.clone(),
            version: reference.version,
        })
        .collect()
}

struct CostEstimatorTaskBasis<'a> {
    task_id: String,
    tender_id: &'a str,
    tender_revision: u32,
    plan_id: &'a str,
    plan_version: u32,
    description: &'a str,
    quantity_evidence: &'a [AgentTaskInputReference],
    unit_rate_evidence: &'a [AgentTaskInputReference],
    deadline: String,
    profile: &'a AgentProfileVersionView,
    scenario: &'a CalculationScenarioVersion,
}

struct CalculationAuthorityRecord {
    kind: String,
    value: String,
    description: String,
    manifest_sha256: Option<String>,
    tender_revision: u32,
    created_by: String,
    created_at: String,
}

type StoredCalculationRun = (
    String,
    String,
    String,
    u32,
    String,
    String,
    u32,
    String,
    String,
    String,
    i64,
    String,
);

fn cost_estimator_calculation_task(basis: CostEstimatorTaskBasis<'_>) -> TenderTaskView {
    let CostEstimatorTaskBasis {
        task_id,
        tender_id,
        tender_revision,
        plan_id,
        plan_version,
        description,
        quantity_evidence,
        unit_rate_evidence,
        deadline,
        profile,
        scenario,
    } = basis;
    let mut exact_inputs = vec![
        AgentTaskInputReference {
            kind: "tender_revision".into(),
            reference: tender_id.into(),
            version: tender_revision,
        },
        AgentTaskInputReference {
            kind: "calculation_scenario_version".into(),
            reference: scenario.scenario_id.clone(),
            version: scenario.version,
        },
        AgentTaskInputReference {
            kind: "work_plan_version".into(),
            reference: plan_id.into(),
            version: plan_version,
        },
        AgentTaskInputReference {
            kind: "calculation_description".into(),
            reference: description.into(),
            version: 1,
        },
    ];
    exact_inputs.extend(tagged_calculation_evidence(
        "calculation_quantity_evidence",
        quantity_evidence,
    ));
    exact_inputs.extend(tagged_calculation_evidence(
        "calculation_unit_rate_evidence",
        unit_rate_evidence,
    ));
    exact_inputs.sort_by(|a, b| {
        (&a.kind, &a.reference, a.version).cmp(&(&b.kind, &b.reference, b.version))
    });
    TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: format!(
            "Extract the exact BOQ quantity and unit rate for {description}. Return only attributable inputs and input states; do not calculate a result."
        ),
        exact_inputs,
        output_contract_json: cost_estimator_calculation_output_contract(),
        review_policy: "The Cost Estimator may propose exact inputs from only the supplied evidence. The Host Calculation Engine is the sole arithmetic authority.".into(),
        deadline,
        permissions: profile.permissions.clone(),
        resource_budget: profile.resource_budget.clone(),
    }
}

struct CostEstimatorTarget {
    tender_id: String,
    tender_revision: u32,
    scenario_id: String,
    scenario_version: u32,
    plan_id: String,
    plan_version: u32,
    description: String,
}

fn exact_cost_estimator_target(
    task: &TenderTaskView,
) -> Result<CostEstimatorTarget, TenderCommandError> {
    let tenders: Vec<&AgentTaskInputReference> = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "tender_revision")
        .collect();
    let scenarios: Vec<&AgentTaskInputReference> = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "calculation_scenario_version")
        .collect();
    let plans: Vec<&AgentTaskInputReference> = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "work_plan_version")
        .collect();
    let descriptions: Vec<&AgentTaskInputReference> = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "calculation_description")
        .collect();
    if tenders.len() != 1 || scenarios.len() != 1 || plans.len() != 1 || descriptions.len() != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(CostEstimatorTarget {
        tender_id: tenders[0].reference.clone(),
        tender_revision: tenders[0].version,
        scenario_id: scenarios[0].reference.clone(),
        scenario_version: scenarios[0].version,
        plan_id: plans[0].reference.clone(),
        plan_version: plans[0].version,
        description: descriptions[0].reference.clone(),
    })
}

struct CalculationRuleReviewTaskRequest<'a> {
    task_id: String,
    tender_id: &'a str,
    tender_revision: u32,
    plan_id: &'a str,
    plan_version: u32,
    rule: &'a CalculationRuleVersion,
    deadline: String,
    profile: &'a AgentProfileVersionView,
}

fn calculation_rule_review_task(request: CalculationRuleReviewTaskRequest<'_>) -> TenderTaskView {
    let CalculationRuleReviewTaskRequest {
        task_id,
        tender_id,
        tender_revision,
        plan_id,
        plan_version,
        rule,
        deadline,
        profile,
    } = request;
    TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: "Independently verify the exact deterministic BOQ rule, its dimensions, decimal policy, rounding behavior, and worked test vectors without editing or activating it.".into(),
        exact_inputs: vec![
            AgentTaskInputReference {
                kind: "tender_revision".into(),
                reference: tender_id.into(),
                version: tender_revision,
            },
            AgentTaskInputReference {
                kind: "calculation_rule_version".into(),
                reference: rule.rule_id.clone(),
                version: rule.version,
            },
            AgentTaskInputReference {
                kind: "work_plan_version".into(),
                reference: plan_id.into(),
                version: plan_version,
            },
        ],
        output_contract_json: calculation_rule_review_output_contract(),
        review_policy: "Pass only when every deterministic vector reproduces exactly, units are dimensionally controlled, exchange-rate direction is explicit, and both supported rounding policies are unambiguous. The reviewer cannot edit or activate the rule.".into(),
        deadline,
        permissions: profile.permissions.clone(),
        resource_budget: profile.resource_budget.clone(),
    }
}

fn exact_calculation_rule_target(
    task: &TenderTaskView,
) -> Result<(String, u32, String, u32), TenderCommandError> {
    let rules: Vec<&AgentTaskInputReference> = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "calculation_rule_version")
        .collect();
    let plans: Vec<&AgentTaskInputReference> = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "work_plan_version")
        .collect();
    if rules.len() != 1 || plans.len() != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((
        rules[0].reference.clone(),
        rules[0].version,
        plans[0].reference.clone(),
        plans[0].version,
    ))
}

#[derive(Serialize, Deserialize)]
struct CalculationRuleReviewManifest {
    schema_version: u32,
    review_id: String,
    rule_id: String,
    rule_version: u32,
    rule_manifest_sha256: String,
    reviewer_run_id: String,
    reviewer_profile_id: String,
    reviewer_profile_version: u32,
    outcome: CalculationRuleReviewOutcome,
    findings: Vec<CalculationRuleReviewFinding>,
    created_at: String,
}

impl QuantixHost {
    pub fn propose_boq_calculation_rule(
        &self,
        command: ProposeBoqCalculationRuleCommand,
    ) -> Result<CalculationRuleVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_calculation_denial(
                &tender_id,
                "propose_boq_calculation_rule",
                None,
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let result = store.propose_boq_calculation_rule(&tender_id, &command, budget);
        if result
            .as_ref()
            .is_err_and(|error| error.code == TenderErrorCode::InvalidCommand)
        {
            store.record_calculation_denial(
                &tender_id,
                "propose_boq_calculation_rule",
                None,
                "guard_denied",
            )?;
        }
        result
    }

    pub fn approve_calculation_rule(
        &self,
        command: ApproveCalculationRuleCommand,
    ) -> Result<CalculationRuleVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_calculation_denial(
                &tender_id,
                "approve_calculation_rule",
                Some(&command.rule_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let result = store.approve_calculation_rule(&tender_id, &command, budget);
        if result
            .as_ref()
            .is_err_and(|error| error.code == TenderErrorCode::InvalidCommand)
        {
            store.record_calculation_denial(
                &tender_id,
                "approve_calculation_rule",
                Some(&command.rule_id),
                "guard_denied",
            )?;
        }
        result
    }

    pub fn approve_controlled_boq_calculation_run(
        &self,
        command: ApproveControlledBoqCalculationRunCommand,
    ) -> Result<ControlledBoqCalculationRun, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_calculation_denial(
                &tender_id,
                "approve_controlled_boq_calculation_run",
                Some(&command.calculation_run_id),
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let result = store.approve_controlled_boq_calculation_run(&tender_id, &command, budget);
        if result
            .as_ref()
            .is_err_and(|error| error.code == TenderErrorCode::InvalidCommand)
        {
            store.record_calculation_denial(
                &tender_id,
                "approve_controlled_boq_calculation_run",
                Some(&command.calculation_run_id),
                "guard_denied",
            )?;
        }
        result
    }

    pub fn create_calculation_scenario(
        &self,
        command: CreateCalculationScenarioCommand,
    ) -> Result<CalculationScenarioVersion, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err() {
            store.record_calculation_denial(
                &tender_id,
                "create_calculation_scenario",
                None,
                "command_shape",
            )?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let result = store.create_calculation_scenario(&tender_id, &command, budget);
        if result
            .as_ref()
            .is_err_and(|error| error.code == TenderErrorCode::InvalidCommand)
        {
            store.record_calculation_denial(
                &tender_id,
                "create_calculation_scenario",
                None,
                "guard_denied",
            )?;
        }
        result
    }

    pub fn inspect_calculation_workspace(
        &self,
        command: InspectCalculationWorkspaceCommand,
    ) -> Result<CalculationWorkspaceInspection, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_calculation_workspace(command.scenario_offset, command.run_offset, budget);
        result
    }
}

impl TenderStore {
    pub(crate) fn record_calculation_denial(
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
        let tender_revision = current_tender_revision(&transaction)?;
        append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "calculation_command_denied",
            tender_revision,
            json!({
                "command": command,
                "reason": reason,
                "target_id": target_id,
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn prepare_cost_estimator_calculation_run(
        &mut self,
        tender_id: &TenderId,
        command: &RunCostEstimatorCalculationCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        let mut recovery_inputs = command.quantity_evidence.clone();
        recovery_inputs.extend(command.unit_rate_evidence.iter().cloned());
        if !self.active_change_allows_inputs(&recovery_inputs)? {
            self.require_change_intake_writable()?;
        }
        budget.check()?;
        validate_calculation_evidence_basket(&command.quantity_evidence)?;
        validate_calculation_evidence_basket(&command.unit_rate_evidence)?;
        let mut quantity_evidence = command.quantity_evidence.clone();
        quantity_evidence.sort_by(|left, right| {
            (&left.kind, &left.reference, left.version).cmp(&(
                &right.kind,
                &right.reference,
                right.version,
            ))
        });
        let mut unit_rate_evidence = command.unit_rate_evidence.clone();
        unit_rate_evidence.sort_by(|left, right| {
            (&left.kind, &left.reference, left.version).cmp(&(
                &right.kind,
                &right.reference,
                right.version,
            ))
        });
        let scenario = load_calculation_scenario(
            &self.connection,
            &command.scenario_id,
            command.scenario_version,
        )?;
        let run_id = random_identifier(&self.connection)?;
        let application_home = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let provider_selection =
            crate::application_settings::load_current_ai_execution_selection(application_home)?;
        let workspace = application_home
            .join("staging")
            .join(format!("agent-{}-{run_id}", tender_id.as_str()));
        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            budget.check()?;
            if !calculation_evidence_basket_is_authoritative(
                &transaction,
                &quantity_evidence,
                &mut || budget.check(),
            )? || !calculation_evidence_basket_is_authoritative(
                &transaction,
                &unit_rate_evidence,
                &mut || budget.check(),
            )? {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let active_rule_exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM calculation_rule_heads AS heads
                       JOIN calculation_rule_approvals AS approvals
                         ON approvals.rule_id = heads.rule_id
                        AND approvals.rule_version = heads.current_version
                     )",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let unresolved_indeterminate: bool = transaction
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
            if !active_rule_exists || unresolved_indeterminate {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let tender_revision = current_tender_revision(&transaction)?;
            let tender_name: String = transaction
                .query_row(
                    "SELECT name FROM tender_revisions WHERE revision = ?1",
                    [tender_revision],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let created_at = sqlite_timestamp(&transaction)?;
            let (plan_id, plan_version, profiles_json): (String, u32, String) = transaction
                .query_row(
                    "SELECT activations.plan_id, activations.plan_version, plans.profiles_json
                     FROM production_activations AS activations
                     JOIN work_plan_versions AS plans
                       ON plans.plan_id = activations.plan_id
                      AND plans.version = activations.plan_version
                     JOIN work_plan_approvals AS approvals
                       ON approvals.plan_id = activations.plan_id
                      AND approvals.plan_version = activations.plan_version
                      AND approvals.decision = 'approve'
                     WHERE activations.status = 'active'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let plan_profiles: Vec<WorkPlanProfileBinding> = parse_canonical(&profiles_json)?;
            let approved_profile = plan_profiles
                .iter()
                .map(|binding| &binding.profile)
                .find(|profile| {
                    profile
                        .capabilities
                        .iter()
                        .any(|capability| capability == COST_ESTIMATION_CAPABILITY)
                })
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let profile = load_profile(
                &transaction,
                (
                    approved_profile.profile_id.clone(),
                    approved_profile.version,
                ),
            )?;
            let (profile_is_active, profile_is_busy): (bool, bool) = transaction
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
            if !profile_is_active || profile != *approved_profile || profile_is_busy {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = cost_estimator_calculation_task(CostEstimatorTaskBasis {
                task_id: random_identifier(&transaction)?,
                tender_id: tender_id.as_str(),
                tender_revision,
                plan_id: &plan_id,
                plan_version,
                description: command.description.trim(),
                quantity_evidence: &quantity_evidence,
                unit_rate_evidence: &unit_rate_evidence,
                deadline: deadline.clone(),
                profile: &profile,
                scenario: &scenario,
            });
            insert_task(&transaction, &task, &created_at)?;
            let classification = *profile
                .permissions
                .data_classifications
                .iter()
                .max()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let payload = json!({
                "calculation_scenario": scenario,
                "data_classification": classification,
                "data_scope": profile.permissions.data_scopes.join("+"),
                "description": command.description.trim(),
                "quantity_evidence": calculation_source_evidence_view(
                    &transaction,
                    &quantity_evidence,
                    &mut || budget.check(),
                )?,
                "rules": {
                    "canonical_arithmetic_allowed": false,
                    "host_calculation_engine_is_sole_authority": true,
                    "inputs_must_cite_supplied_evidence": true
                },
                "tender": {
                    "name": tender_name,
                    "revision": tender_revision,
                    "tender_id": tender_id.as_str()
                },
                "unit_rate_evidence": calculation_source_evidence_view(
                    &transaction,
                    &unit_rate_evidence,
                    &mut || budget.check(),
                )?,
            });
            let (permission_grant, materialized_workspace) =
                derive_planned_task_grant(PlannedTaskGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home,
                    tender_id: tender_id.as_str(),
                    work_plan_version: plan_version,
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                    expires_at: &deadline,
                    payload: &payload,
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
                        append_audit_event_with_sequence(
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
            super::record_agent_run_provider_binding(
                &transaction,
                &run_id,
                &provider_selection,
                &created_at,
            )?;
            insert_event(
                &transaction,
                &run_id,
                1,
                PendingProviderEvent {
                    kind: ProviderEventKind::RunStarted,
                    summary: "Cost Estimator calculation input extraction started".into(),
                    correlation_id: None,
                    request_fingerprint: None,
                    denial_reason: None,
                    opaque_reference: None,
                },
                &created_at,
            )?;
            append_audit_event_with_sequence(
                &transaction,
                tender_id.as_str(),
                "cost_estimator_calculation_started",
                tender_revision,
                json!({
                    "profile_id": profile.profile_id,
                    "run_id": run_id,
                    "scenario_id": scenario.scenario_id,
                    "scenario_version": scenario.version.to_string(),
                    "task_id": task.task_id,
                }),
                &created_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            Ok(PreparedAgentRun {
                run_id,
                provider_selection,
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

    pub(crate) fn prepare_calculation_rule_review_run(
        &mut self,
        tender_id: &TenderId,
        rule_id: &str,
        version: u32,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        self.require_change_intake_writable()?;
        if !super::valid_identifier(rule_id) || version == 0 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let rule = self.load_calculation_rule(rule_id, version)?;
        if !rule.current || rule.review.is_some() || rule.approval.is_some() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let run_id = random_identifier(&self.connection)?;
        let application_home = self
            .root
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let provider_selection =
            crate::application_settings::load_current_ai_execution_selection(application_home)?;
        let workspace = application_home
            .join("staging")
            .join(format!("agent-{}-{run_id}", tender_id.as_str()));
        let prepared = (|| {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let target_open: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM calculation_rule_heads AS heads
                       WHERE heads.rule_id = ?1 AND heads.current_version = ?2
                         AND NOT EXISTS (
                           SELECT 1 FROM calculation_rule_reviews
                           WHERE rule_id = heads.rule_id AND rule_version = heads.current_version
                         )
                         AND NOT EXISTS (
                           SELECT 1 FROM calculation_rule_approvals
                           WHERE rule_id = heads.rule_id AND rule_version = heads.current_version
                         )
                     )",
                    params![rule_id, version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let unresolved_indeterminate: bool = transaction
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
            if !target_open || unresolved_indeterminate {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let tender_revision = current_tender_revision(&transaction)?;
            let tender_name: String = transaction
                .query_row(
                    "SELECT name FROM tender_revisions WHERE revision = ?1",
                    [tender_revision],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let created_at = sqlite_timestamp(&transaction)?;
            let approved_plan: Option<(String, u32, String)> = transaction
                .query_row(
                    "SELECT activations.plan_id, activations.plan_version, plans.profiles_json
                     FROM production_activations AS activations
                     JOIN work_plan_versions AS plans
                       ON plans.plan_id = activations.plan_id
                      AND plans.version = activations.plan_version
                     JOIN work_plan_approvals AS approvals
                       ON approvals.plan_id = activations.plan_id
                      AND approvals.plan_version = activations.plan_version
                      AND approvals.decision = 'approve'
                     WHERE activations.status = 'active'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (plan_id, plan_version, profiles_json) = approved_plan
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let plan_profiles: Vec<WorkPlanProfileBinding> = parse_canonical(&profiles_json)?;
            let approved_profile = plan_profiles
                .iter()
                .map(|binding| &binding.profile)
                .find(|profile| {
                    profile
                        .capabilities
                        .iter()
                        .any(|capability| capability == CALCULATION_RULE_REVIEW_CAPABILITY)
                })
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            let profile = load_profile(
                &transaction,
                (
                    approved_profile.profile_id.clone(),
                    approved_profile.version,
                ),
            )?;
            let profile_is_active: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_profile_heads
                       WHERE profile_id = ?1 AND current_version = ?2 AND status = 'active'
                     )",
                    params![profile.profile_id, profile.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let profile_is_busy: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs
                       WHERE profile_id = ?1 AND profile_version = ?2 AND status = 'running'
                     )",
                    params![profile.profile_id, profile.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if !profile_is_active || profile != *approved_profile || profile_is_busy {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let deadline: String = transaction
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '+1 hour')",
                    [],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let task = calculation_rule_review_task(CalculationRuleReviewTaskRequest {
                task_id: random_identifier(&transaction)?,
                tender_id: tender_id.as_str(),
                tender_revision,
                plan_id: &plan_id,
                plan_version,
                rule: &rule,
                deadline: deadline.clone(),
                profile: &profile,
            });
            insert_task(&transaction, &task, &created_at)?;
            let classification = *profile
                .permissions
                .data_classifications
                .iter()
                .max()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let payload = json!({
                "calculation_rule": rule,
                "data_classification": classification,
                "data_scope": profile.permissions.data_scopes.join("+"),
                "review_rules": {
                    "activation_allowed": false,
                    "arithmetic_must_be_replayed": true,
                    "exact_target_is_immutable": true
                },
                "tender": {
                    "name": tender_name,
                    "revision": tender_revision,
                    "tender_id": tender_id.as_str()
                },
            });
            let (permission_grant, materialized_workspace) =
                derive_planned_task_grant(PlannedTaskGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home,
                    tender_id: tender_id.as_str(),
                    work_plan_version: plan_version,
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                    expires_at: &deadline,
                    payload: &payload,
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
                        append_audit_event_with_sequence(
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
            super::record_agent_run_provider_binding(
                &transaction,
                &run_id,
                &provider_selection,
                &created_at,
            )?;
            insert_event(
                &transaction,
                &run_id,
                1,
                PendingProviderEvent {
                    kind: ProviderEventKind::RunStarted,
                    summary: "Independent Calculation Rule review started".into(),
                    correlation_id: None,
                    request_fingerprint: None,
                    denial_reason: None,
                    opaque_reference: None,
                },
                &created_at,
            )?;
            append_audit_event_with_sequence(
                &transaction,
                tender_id.as_str(),
                "calculation_rule_review_started",
                tender_revision,
                json!({
                    "reviewer_profile_id": profile.profile_id,
                    "rule_id": rule_id,
                    "rule_version": version.to_string(),
                    "run_id": run_id,
                    "task_id": task.task_id,
                }),
                &created_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            Ok(PreparedAgentRun {
                run_id,
                provider_selection,
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

    pub(crate) fn validate_calculation_rule_review_candidate(
        &self,
        task: &TenderTaskView,
        payload: &str,
    ) -> Result<CalculationRuleReviewCandidate, TenderCommandError> {
        if payload.len() > 64 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let candidate: CalculationRuleReviewCandidate = serde_json::from_str(payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        exact_calculation_rule_target(task)?;
        let mut codes = std::collections::HashSet::new();
        let findings_valid = candidate.findings.len() <= 32
            && candidate.findings.iter().all(|finding| {
                !finding.code.is_empty()
                    && finding.code.len() <= 100
                    && finding.code.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
                    && codes.insert(finding.code.clone())
                    && !finding.summary.trim().is_empty()
                    && finding.summary.len() <= 2_000
            });
        let outcome_valid = match candidate.outcome {
            CalculationRuleReviewOutcome::Passed => candidate.findings.is_empty(),
            CalculationRuleReviewOutcome::Failed => !candidate.findings.is_empty(),
        };
        if !findings_valid || !outcome_valid {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(candidate)
    }

    pub(crate) fn calculation_rule_review_target_is_current(
        &self,
        task: &TenderTaskView,
    ) -> Result<bool, TenderCommandError> {
        let (rule_id, version, _, _) = exact_calculation_rule_target(task)?;
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM calculation_rule_heads
                   WHERE rule_id = ?1 AND current_version = ?2
                 )",
                params![rule_id, version],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn validate_cost_estimator_calculation_candidate(
        &self,
        task: &TenderTaskView,
        payload: &str,
    ) -> Result<CostEstimatorCalculationCandidate, TenderCommandError> {
        if payload.len() > 64 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let candidate: CostEstimatorCalculationCandidate = serde_json::from_str(payload)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        exact_cost_estimator_target(task)?;
        if !valid_input_shape(&candidate.quantity, false)
            || !valid_input_shape(&candidate.unit_rate, false)
            || !candidate_evidence_is_within_task(
                task,
                &candidate.quantity,
                "calculation_quantity_evidence",
            )
            || !candidate_evidence_is_within_task(
                task,
                &candidate.unit_rate,
                "calculation_unit_rate_evidence",
            )
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(candidate)
    }

    pub(crate) fn cost_estimator_calculation_target_is_current(
        &self,
        task: &TenderTaskView,
    ) -> Result<bool, TenderCommandError> {
        let target = exact_cost_estimator_target(task)?;
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM calculation_scenario_versions AS scenarios
                   JOIN tender
                     ON tender.singleton = 1
                    AND tender.tender_id = ?7
                    AND tender.current_revision = ?8
                   JOIN production_activations AS activations
                     ON activations.plan_id = ?3
                    AND activations.plan_version = ?4
                    AND activations.status = 'active'
                   JOIN agent_profile_heads AS profiles
                     ON profiles.profile_id = ?5
                    AND profiles.current_version = ?6
                    AND profiles.status = 'active'
                   WHERE scenarios.scenario_id = ?1 AND scenarios.version = ?2
                 )",
                params![
                    target.scenario_id,
                    target.scenario_version,
                    target.plan_id,
                    target.plan_version,
                    task.profile_id,
                    task.profile_version,
                    target.tender_id,
                    target.tender_revision,
                ],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    fn propose_boq_calculation_rule(
        &mut self,
        tender_id: &TenderId,
        command: &ProposeBoqCalculationRuleCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<CalculationRuleVersion, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let change_rationale = command.change_rationale.trim();
        let rounding_policy = normalize_rounding_policy(&command.supported_rounding)
            .filter(|_| !change_rationale.is_empty())
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let existing: Option<(String, u32, Option<String>, bool)> = transaction
            .query_row(
                "SELECT heads.rule_id, heads.current_version, reviews.outcome,
                        EXISTS(SELECT 1 FROM calculation_rule_approvals
                               WHERE rule_id = heads.rule_id
                                 AND rule_version = heads.current_version)
                 FROM calculation_rule_heads AS heads
                 LEFT JOIN calculation_rule_reviews AS reviews
                   ON reviews.rule_id = heads.rule_id
                  AND reviews.rule_version = heads.current_version",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let (rule_id, version, is_new_rule) = match existing {
            None => (random_identifier(&transaction)?, 1, true),
            Some((rule_id, current_version, Some(outcome), false))
                if outcome == "failed" && current_version < 32 =>
            {
                let policy_json = canonical_json(&rounding_policy)?;
                let already_proposed: bool = transaction
                    .query_row(
                        "SELECT EXISTS(
                           SELECT 1 FROM calculation_rule_versions
                           WHERE rule_id = ?1 AND supported_rounding_json = ?2
                         )",
                        params![rule_id, policy_json],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if already_proposed {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                (rule_id, current_version + 1, false)
            }
            _ => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        };
        let tests = deterministic_rule_tests()?;
        if tests.iter().any(|test| !test.passed) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let manifest = CalculationRuleManifest {
            schema_version: 1,
            rule_id: rule_id.clone(),
            version,
            name: BOQ_RULE_NAME.into(),
            formula: CONTROLLED_CALCULATION_RULE_FORMULA.into(),
            engine_version: CALCULATION_ENGINE_VERSION.into(),
            supported_units: supported_units(),
            supported_currencies: supported_currencies(),
            supported_rounding: rounding_policy,
            change_rationale: change_rationale.into(),
            deterministic_tests: tests,
            created_by: "engineer_user".into(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let tender_revision = current_tender_revision(&transaction)?;
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "calculation_rule_proposed",
            tender_revision,
            json!({
                "engine_version": CALCULATION_ENGINE_VERSION,
                "manifest_sha256": manifest_sha256,
                "rule_id": rule_id,
                "rule_version": version.to_string(),
            }),
            &created_at,
        )?;
        if is_new_rule {
            transaction
                .execute(
                    "INSERT INTO calculation_rules (rule_id, created_at) VALUES (?1, ?2)",
                    params![rule_id, created_at],
                )
                .map_err(sql_error)?;
        }
        transaction
            .execute(
                "INSERT INTO calculation_rule_versions (
                   rule_id, version, name, formula, engine_version,
                   supported_units_json, supported_rounding_json,
                   deterministic_tests_json, audit_sequence, manifest_json,
                   manifest_sha256, created_by, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                           'engineer_user', ?12)",
                params![
                    rule_id,
                    version,
                    manifest.name,
                    manifest.formula,
                    manifest.engine_version,
                    canonical_json(&manifest.supported_units)?,
                    canonical_json(&manifest.supported_rounding)?,
                    canonical_json(&manifest.deterministic_tests)?,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                if is_new_rule {
                    "INSERT INTO calculation_rule_heads (rule_id, current_version) VALUES (?1, ?2)"
                } else {
                    "UPDATE calculation_rule_heads SET current_version = ?2 WHERE rule_id = ?1"
                },
                params![rule_id, version],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        budget.check()?;
        self.load_calculation_rule(&rule_id, version)
    }

    fn approve_calculation_rule(
        &mut self,
        tender_id: &TenderId,
        command: &ApproveCalculationRuleCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<CalculationRuleVersion, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let basis: Option<(String, String, String, String)> = transaction
            .query_row(
                "SELECT versions.manifest_sha256, reviews.review_id,
                        reviews.manifest_sha256, reviews.outcome
                 FROM calculation_rule_versions AS versions
                 JOIN calculation_rule_heads AS heads
                   ON heads.rule_id = versions.rule_id
                  AND heads.current_version = versions.version
                 JOIN calculation_rule_reviews AS reviews
                   ON reviews.rule_id = versions.rule_id
                  AND reviews.rule_version = versions.version
                 WHERE versions.rule_id = ?1 AND versions.version = ?2
                   AND NOT EXISTS (
                     SELECT 1 FROM calculation_rule_approvals
                     WHERE rule_id = versions.rule_id AND rule_version = versions.version
                   )",
                params![command.rule_id, command.version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((rule_manifest_sha256, review_id, review_manifest_sha256, outcome)) = basis else {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        let rationale = command.rationale.trim();
        if rule_manifest_sha256 != command.manifest_sha256
            || outcome != "passed"
            || rationale.is_empty()
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let approval_manifest = CalculationRuleApprovalManifest {
            schema_version: 1,
            approval_id: approval_id.clone(),
            rule_id: command.rule_id.clone(),
            rule_version: command.version,
            rule_manifest_sha256: rule_manifest_sha256.clone(),
            review_id: review_id.clone(),
            review_manifest_sha256: review_manifest_sha256.clone(),
            rationale: rationale.into(),
            approved_by: "engineer_user".into(),
            acting_role: "engineer_in_the_loop".into(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&approval_manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let tender_revision = current_tender_revision(&transaction)?;
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "calculation_rule_activated",
            tender_revision,
            json!({
                "approval_id": approval_id,
                "manifest_sha256": manifest_sha256,
                "review_id": review_id,
                "rule_id": command.rule_id,
                "rule_manifest_sha256": rule_manifest_sha256,
                "rule_version": command.version.to_string(),
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO calculation_rule_approvals (
                   approval_id, rule_id, rule_version, rule_manifest_sha256,
                   review_id, review_manifest_sha256, rationale, approved_by,
                   acting_role, audit_sequence, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'engineer_user',
                           'engineer_in_the_loop', ?8, ?9, ?10, ?11)",
                params![
                    approval_id,
                    command.rule_id,
                    command.version,
                    rule_manifest_sha256,
                    review_id,
                    review_manifest_sha256,
                    rationale,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        self.load_calculation_rule(&command.rule_id, command.version)
    }

    fn create_calculation_scenario(
        &mut self,
        tender_id: &TenderId,
        command: &CreateCalculationScenarioCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<CalculationScenarioVersion, TenderCommandError> {
        self.require_change_intake_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let rationale = command.rationale.trim();
        let scenario_count: u32 = transaction
            .query_row(
                "SELECT COUNT(*) FROM calculation_scenario_versions",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let quantity_unit = unit_definition(&command.quantity_unit);
        let rate_basis_unit = unit_definition(&command.rate_basis_unit);
        let same_currency = command.rate_currency == command.output_currency;
        if scenario_count >= MAX_CALCULATION_SCENARIOS
            || rationale.is_empty()
            || quantity_unit.is_none()
            || rate_basis_unit.is_none()
            || !valid_currency(&command.rate_currency)
            || !valid_currency(&command.output_currency)
            || !valid_input_shape(&command.exchange_rate, same_currency)
            || !valid_exchange_rate_governance(
                same_currency,
                &command.exchange_rate,
                command.exchange_rate_effective_date.as_deref(),
                &command.pricing_date,
                command.exchange_rate_type,
            )
            || !input_evidence_is_authoritative(&transaction, &command.exchange_rate, &mut || {
                budget.check()
            })?
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let active_rounding_policy: Option<String> = transaction
            .query_row(
                "SELECT versions.supported_rounding_json
                 FROM calculation_rule_versions AS versions
                 JOIN calculation_rule_heads AS heads
                   ON heads.rule_id = versions.rule_id
                  AND heads.current_version = versions.version
                 JOIN calculation_rule_approvals AS approvals
                   ON approvals.rule_id = versions.rule_id
                  AND approvals.rule_version = versions.version
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if active_rounding_policy
            .as_deref()
            .map(parse_canonical::<Vec<CalculationRoundingMode>>)
            .transpose()?
            .as_ref()
            .is_none_or(|policy| !policy.contains(&command.rounding_mode))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if !same_currency && command.exchange_rate.state == CalculationInputState::Provided {
            let exchange_rate = command
                .exchange_rate
                .value
                .as_deref()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            if parse_nonnegative_decimal(exchange_rate).is_err()
                || parse_nonnegative_decimal(exchange_rate).is_ok_and(|value| value.is_zero())
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let scenario_id = random_identifier(&transaction)?;
        let exchange_rate_id = random_identifier(&transaction)?;
        let rounding_policy_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let tender_revision = current_tender_revision(&transaction)?;
        let manifest = CalculationScenarioManifest {
            schema_version: 1,
            scenario_id: scenario_id.clone(),
            version: 1,
            name: command.name.trim().into(),
            quantity_unit: command.quantity_unit.clone(),
            rate_basis_unit: command.rate_basis_unit.clone(),
            rate_currency: command.rate_currency.clone(),
            exchange_rate_id: exchange_rate_id.clone(),
            exchange_rate_version: 1,
            exchange_rate: command.exchange_rate.clone(),
            exchange_rate_effective_date: command.exchange_rate_effective_date.clone(),
            pricing_date: command.pricing_date.clone(),
            exchange_rate_type: command.exchange_rate_type,
            output_currency: command.output_currency.clone(),
            rounding_policy_id: rounding_policy_id.clone(),
            rounding_policy_version: 1,
            precision: command.precision,
            rounding_mode: command.rounding_mode,
            rationale: rationale.into(),
            approved_by: "engineer_user".into(),
            acting_role: "engineer_in_the_loop".into(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "calculation_scenario_approved",
            tender_revision,
            json!({
                "exchange_rate_id": exchange_rate_id,
                "manifest_sha256": manifest_sha256,
                "rounding_policy_id": rounding_policy_id,
                "scenario_id": scenario_id,
                "scenario_version": "1",
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO calculation_scenario_versions (
                   scenario_id, version, name, quantity_unit, rate_basis_unit,
                   rate_currency, exchange_rate_id, exchange_rate_version,
                   exchange_rate_json, exchange_rate_effective_date, pricing_date,
                   exchange_rate_type, output_currency, rounding_policy_id,
                   rounding_policy_version, precision, rounding_mode, rationale,
                   approved_by, acting_role, audit_sequence, manifest_json,
                   manifest_sha256, created_at
                 ) VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10,
                           ?11, ?12, 1, ?13, ?14, ?15, 'engineer_user',
                           'engineer_in_the_loop', ?16, ?17, ?18, ?19)",
                params![
                    scenario_id,
                    manifest.name,
                    manifest.quantity_unit,
                    manifest.rate_basis_unit,
                    manifest.rate_currency,
                    exchange_rate_id,
                    canonical_json(&manifest.exchange_rate)?,
                    manifest.exchange_rate_effective_date,
                    manifest.pricing_date,
                    manifest.exchange_rate_type.map(ExchangeRateType::as_str),
                    manifest.output_currency,
                    rounding_policy_id,
                    manifest.precision,
                    manifest.rounding_mode.as_str(),
                    rationale,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        scenario_from_manifest(manifest, manifest_sha256)
    }

    fn approve_controlled_boq_calculation_run(
        &mut self,
        tender_id: &TenderId,
        command: &ApproveControlledBoqCalculationRunCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ControlledBoqCalculationRun, TenderCommandError> {
        if !self.active_change_allows_calculation_run(&command.calculation_run_id)? {
            self.require_change_intake_writable()?;
        }
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let rationale = command.rationale.trim();
        let Some(stored_run) =
            load_stored_calculation_run(&transaction, &command.calculation_run_id)?
        else {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        };
        let Some(run_manifest) =
            calculation_run_core_is_valid(&transaction, &stored_run, &mut || budget.check())?
        else {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        };
        let run_manifest_sha256 = stored_run.9.clone();
        let tender_revision = current_tender_revision(&transaction)?;
        let (approval_exists, authority_count): (bool, u32) = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM calculation_run_approvals WHERE calculation_run_id = ?1
                 ), (SELECT COUNT(*) FROM tender_record_authorities)",
                [&command.calculation_run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if rationale.is_empty()
            || run_manifest_sha256 != command.manifest_sha256
            || run_manifest.status != ControlledBoqCalculationStatus::Completed
            || run_manifest.tender_revision != tender_revision
            || approval_exists
            || authority_count >= 256
            || !calculation_run_approval_is_valid(
                &transaction,
                &run_manifest,
                &run_manifest_sha256,
                &mut || budget.check(),
            )?
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let final_amount = run_manifest
            .final_amount
            .as_deref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let approval_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let approval_manifest = CalculationRunApprovalManifest {
            schema_version: 1,
            approval_id: approval_id.clone(),
            calculation_run_id: command.calculation_run_id.clone(),
            run_manifest_sha256: run_manifest_sha256.clone(),
            rationale: rationale.into(),
            approved_by: "engineer_user".into(),
            acting_role: "engineer_in_the_loop".into(),
            tender_revision,
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&approval_manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "controlled_boq_calculation_approved",
            tender_revision,
            json!({
                "approval_id": approval_id,
                "calculation_run_id": command.calculation_run_id,
                "manifest_sha256": manifest_sha256,
                "run_manifest_sha256": run_manifest_sha256,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO calculation_run_approvals (
                   approval_id, calculation_run_id, run_manifest_sha256, rationale,
                   approved_by, acting_role, tender_revision, audit_sequence,
                   manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, 'engineer_user', 'engineer_in_the_loop',
                           ?5, ?6, ?7, ?8, ?9)",
                params![
                    approval_id,
                    command.calculation_run_id,
                    run_manifest_sha256,
                    rationale,
                    tender_revision,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO tender_record_authorities (
                   authority_id, kind, value, description, manifest_sha256,
                   tender_revision, created_by, created_at
                 ) VALUES (?1, 'calculation_run', ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    command.calculation_run_id,
                    format!("{} {}", final_amount, run_manifest.output_currency),
                    run_manifest.description,
                    run_manifest_sha256,
                    tender_revision,
                    CALCULATION_ENGINE_VERSION,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        self.load_calculation_run_by_id(&command.calculation_run_id)
    }

    pub(crate) fn inspect_calculation_workspace(
        &self,
        scenario_offset: u32,
        run_offset: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<CalculationWorkspaceInspection, TenderCommandError> {
        budget.check()?;
        let rule_basis: Option<(String, u32)> = self
            .connection
            .query_row(
                "SELECT rule_id, current_version FROM calculation_rule_heads LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let rule = rule_basis
            .map(|(rule_id, version)| self.load_calculation_rule(&rule_id, version))
            .transpose()?;
        let (total_scenario_count, total_run_count): (u32, u32) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM calculation_scenario_versions),
                        (SELECT COUNT(*) FROM calculation_runs)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let mut scenario_statement = self
            .connection
            .prepare(
                "SELECT manifest_json, manifest_sha256
                 FROM calculation_scenario_versions
                 ORDER BY rowid DESC LIMIT 8 OFFSET ?1",
            )
            .map_err(sql_error)?;
        let scenario_rows = scenario_statement
            .query_map([scenario_offset], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut recent_scenarios = Vec::new();
        for row in scenario_rows {
            budget.check()?;
            let (manifest_json, manifest_sha256) = row.map_err(sql_error)?;
            recent_scenarios.push(scenario_from_manifest(
                parse_canonical(&manifest_json)?,
                manifest_sha256,
            )?);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT manifest_json, manifest_sha256 FROM calculation_runs
                 ORDER BY rowid DESC LIMIT 8 OFFSET ?1",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([run_offset], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut recent_runs = Vec::new();
        for row in rows {
            budget.check()?;
            let (manifest_json, manifest_sha256) = row.map_err(sql_error)?;
            recent_runs.push(calculation_run_view(
                &self.connection,
                &manifest_json,
                manifest_sha256,
            )?);
        }
        Ok(CalculationWorkspaceInspection {
            rule,
            recent_scenarios,
            recent_runs,
            total_scenario_count,
            total_run_count,
            scenario_offset,
            run_offset,
            has_older_scenarios: scenario_offset.saturating_add(8) < total_scenario_count,
            has_older_runs: run_offset.saturating_add(8) < total_run_count,
        })
    }

    pub(crate) fn load_calculation_rule(
        &self,
        rule_id: &str,
        version: u32,
    ) -> Result<CalculationRuleVersion, TenderCommandError> {
        let stored: Option<StoredCalculationRuleRow> = self
            .connection
            .query_row(
                "SELECT versions.name, versions.formula, versions.engine_version,
                            versions.supported_units_json, versions.supported_rounding_json,
                            versions.deterministic_tests_json, versions.manifest_json,
                            versions.manifest_sha256, versions.created_at,
                            heads.current_version = versions.version
                     FROM calculation_rule_versions AS versions
                     JOIN calculation_rule_heads AS heads ON heads.rule_id = versions.rule_id
                     WHERE versions.rule_id = ?1 AND versions.version = ?2",
                params![rule_id, version],
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
        let Some((
            name,
            formula,
            engine_version,
            units_json,
            rounding_json,
            tests_json,
            manifest_json,
            manifest_sha256,
            created_at,
            current,
        )) = stored
        else {
            return Err(TenderCommandError::new(TenderErrorCode::NotFound));
        };
        let manifest: CalculationRuleManifest = parse_canonical(&manifest_json)?;
        if manifest.rule_id != rule_id
            || manifest.version != version
            || manifest.name != name
            || manifest.formula != formula
            || manifest.engine_version != engine_version
            || manifest.supported_units != parse_canonical::<Vec<String>>(&units_json)?
            || manifest.supported_currencies != supported_currencies()
            || manifest.supported_rounding
                != parse_canonical::<Vec<CalculationRoundingMode>>(&rounding_json)?
            || normalize_rounding_policy(&manifest.supported_rounding).as_ref()
                != Some(&manifest.supported_rounding)
            || !valid_change_rationale(&manifest.change_rationale)
            || manifest.deterministic_tests
                != parse_canonical::<Vec<CalculationRuleTestResult>>(&tests_json)?
            || sha256_hex(manifest_json.as_bytes()) != manifest_sha256
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let review = load_rule_review(&self.connection, rule_id, version)?;
        let approval = load_rule_approval(&self.connection, rule_id, version)?;
        Ok(CalculationRuleVersion {
            rule_id: rule_id.into(),
            version,
            name,
            formula,
            engine_version,
            supported_units: manifest.supported_units,
            supported_currencies: manifest.supported_currencies,
            supported_rounding: manifest.supported_rounding,
            change_rationale: manifest.change_rationale,
            deterministic_tests: manifest.deterministic_tests,
            review,
            active: approval.is_some() && current,
            approval,
            current,
            manifest_sha256,
            created_at,
        })
    }

    pub(crate) fn load_calculation_for_cost_estimator_run(
        &self,
        run_id: &str,
    ) -> Result<ControlledBoqCalculationRun, TenderCommandError> {
        let stored: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT manifest_json, manifest_sha256 FROM calculation_runs
                 WHERE cost_estimator_run_id = ?1",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((manifest_json, manifest_sha256)) = stored else {
            return Err(TenderCommandError::new(TenderErrorCode::NotFound));
        };
        calculation_run_view(&self.connection, &manifest_json, manifest_sha256)
    }

    fn load_calculation_run_by_id(
        &self,
        calculation_run_id: &str,
    ) -> Result<ControlledBoqCalculationRun, TenderCommandError> {
        let stored: Option<(String, String)> = self
            .connection
            .query_row(
                "SELECT manifest_json, manifest_sha256 FROM calculation_runs
                 WHERE calculation_run_id = ?1",
                [calculation_run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((manifest_json, manifest_sha256)) = stored else {
            return Err(TenderCommandError::new(TenderErrorCode::NotFound));
        };
        calculation_run_view(&self.connection, &manifest_json, manifest_sha256)
    }

    pub(crate) fn calculation_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let counts: (u32, u32, u32, u32, u32, u32, u32, u32, u32) = self
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM calculation_rules),
                   (SELECT COUNT(*) FROM calculation_rule_versions),
                   (SELECT COUNT(*) FROM calculation_rule_heads),
                   (SELECT COUNT(*) FROM calculation_rule_reviews),
                   (SELECT COUNT(*) FROM calculation_rule_approvals),
                   (SELECT COUNT(*) FROM calculation_runs),
                   (SELECT COUNT(*) FROM calculation_scenario_versions),
                   (SELECT COUNT(*) FROM calculation_run_approvals),
                   (SELECT COUNT(*) FROM tender_record_authorities
                    WHERE kind = 'calculation_run')",
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
                    ))
                },
            )
            .map_err(sql_error)?;
        if counts.0 > 1
            || counts.1 < counts.0
            || counts.1 > 32
            || counts.2 != counts.0
            || counts.3 > counts.1
            || counts.4 > counts.3
            || counts.5 > MAX_CALCULATION_RUNS
            || counts.6 > MAX_CALCULATION_SCENARIOS
            || counts.7 > counts.5
            || counts.8 != counts.7
        {
            return Ok(false);
        }
        if counts.0 == 0 {
            return Ok(counts.3 == 0
                && counts.4 == 0
                && counts.5 == 0
                && counts.6 == 0
                && counts.7 == 0
                && counts.8 == 0);
        }
        let expected_tests = deterministic_rule_tests()?;
        let mut version_statement = self
            .connection
            .prepare(
                "SELECT rule_id, version, name, formula, engine_version,
                        supported_units_json, supported_rounding_json,
                        deterministic_tests_json, manifest_json, manifest_sha256,
                        created_at, audit_sequence
                 FROM calculation_rule_versions ORDER BY version",
            )
            .map_err(sql_error)?;
        let version_rows = version_statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            })
            .map_err(sql_error)?;
        let mut expected_version = 1;
        let mut prior_rounding_policies = Vec::new();
        for row in version_rows {
            check()?;
            let row = row.map_err(sql_error)?;
            let tests: Vec<CalculationRuleTestResult> = parse_canonical(&row.7)?;
            let manifest: CalculationRuleManifest = parse_canonical(&row.8)?;
            let supported_unit_values: Vec<String> = parse_canonical(&row.5)?;
            let supported_rounding_values: Vec<CalculationRoundingMode> = parse_canonical(&row.6)?;
            let rounding_policy_json = canonical_json(&supported_rounding_values)?;
            let expected_change = json!({
                "engine_version": CALCULATION_ENGINE_VERSION,
                "manifest_sha256": row.9,
                "rule_id": row.0,
                "rule_version": row.1.to_string(),
            });
            if row.1 != expected_version
                || row.2 != BOQ_RULE_NAME
                || row.3 != CONTROLLED_CALCULATION_RULE_FORMULA
                || row.4 != CALCULATION_ENGINE_VERSION
                || supported_unit_values != supported_units()
                || normalize_rounding_policy(&supported_rounding_values).as_ref()
                    != Some(&supported_rounding_values)
                || prior_rounding_policies.contains(&rounding_policy_json)
                || tests != expected_tests
                || tests.iter().any(|test| !test.passed)
                || manifest.schema_version != 1
                || manifest.rule_id != row.0
                || manifest.version != row.1
                || manifest.name != row.2
                || manifest.formula != row.3
                || manifest.engine_version != row.4
                || manifest.supported_units != supported_unit_values
                || manifest.supported_currencies != supported_currencies()
                || manifest.supported_rounding != supported_rounding_values
                || !valid_change_rationale(&manifest.change_rationale)
                || manifest.deterministic_tests != tests
                || manifest.created_by != "engineer_user"
                || manifest.created_at != row.10
                || row.9 != sha256_hex(row.8.as_bytes())
                || !calculation_audit_is_exact(
                    &self.connection,
                    row.11,
                    "calculation_rule_proposed",
                    &row.10,
                    &expected_change,
                )?
            {
                return Ok(false);
            }
            prior_rounding_policies.push(rounding_policy_json);
            if row.1 < counts.1 {
                let Some(review) = load_rule_review(&self.connection, &row.0, row.1)? else {
                    return Ok(false);
                };
                let rule = CalculationRuleVersion {
                    rule_id: row.0.clone(),
                    version: row.1,
                    name: row.2.clone(),
                    formula: row.3.clone(),
                    engine_version: row.4.clone(),
                    supported_units: supported_unit_values,
                    supported_currencies: manifest.supported_currencies.clone(),
                    supported_rounding: supported_rounding_values,
                    change_rationale: manifest.change_rationale.clone(),
                    deterministic_tests: tests,
                    review: Some(review.clone()),
                    approval: None,
                    current: false,
                    active: false,
                    manifest_sha256: row.9.clone(),
                    created_at: row.10.clone(),
                };
                let stored_review: (String, String, String, i64) = self
                    .connection
                    .query_row(
                        "SELECT manifest_json, manifest_sha256,
                                rule_manifest_sha256, audit_sequence
                         FROM calculation_rule_reviews WHERE review_id = ?1",
                        [&review.review_id],
                        |stored| {
                            Ok((
                                stored.get(0)?,
                                stored.get(1)?,
                                stored.get(2)?,
                                stored.get(3)?,
                            ))
                        },
                    )
                    .map_err(sql_error)?;
                let review_manifest: CalculationRuleReviewManifest =
                    parse_canonical(&stored_review.0)?;
                let review_change = json!({
                    "manifest_sha256": review.manifest_sha256,
                    "outcome": review.outcome.as_str(),
                    "review_id": review.review_id,
                    "reviewer_profile_id": review.reviewer_profile_id,
                    "reviewer_profile_version": review.reviewer_profile_version.to_string(),
                    "reviewer_run_id": review.reviewer_run_id,
                    "rule_id": row.0,
                    "rule_manifest_sha256": row.9,
                    "rule_version": row.1.to_string(),
                });
                if review.outcome != CalculationRuleReviewOutcome::Failed
                    || load_rule_approval(&self.connection, &row.0, row.1)?.is_some()
                    || stored_review.1 != sha256_hex(stored_review.0.as_bytes())
                    || stored_review.1 != review.manifest_sha256
                    || stored_review.2 != row.9
                    || review_manifest.schema_version != 1
                    || review_manifest.review_id != review.review_id
                    || review_manifest.rule_id != row.0
                    || review_manifest.rule_version != row.1
                    || review_manifest.rule_manifest_sha256 != row.9
                    || review_manifest.reviewer_run_id != review.reviewer_run_id
                    || review_manifest.reviewer_profile_id != review.reviewer_profile_id
                    || review_manifest.reviewer_profile_version != review.reviewer_profile_version
                    || review_manifest.outcome != review.outcome
                    || review_manifest.findings != review.findings
                    || review_manifest.created_at != review.created_at
                    || !calculation_audit_is_exact(
                        &self.connection,
                        stored_review.3,
                        "calculation_rule_review_completed",
                        &review.created_at,
                        &review_change,
                    )?
                    || !calculation_rule_review_run_is_valid(
                        &self.connection,
                        &review,
                        &rule,
                        check,
                    )?
                {
                    return Ok(false);
                }
            }
            expected_version += 1;
        }
        if expected_version != counts.1 + 1 {
            return Ok(false);
        }
        let stored_rule: (
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
            i64,
        ) = self
            .connection
            .query_row(
                "SELECT versions.rule_id, versions.version, versions.name, versions.formula,
                        versions.engine_version, versions.supported_units_json,
                        versions.supported_rounding_json, versions.deterministic_tests_json,
                        versions.manifest_json, versions.manifest_sha256, versions.created_at,
                        versions.audit_sequence
                 FROM calculation_rule_versions AS versions
                 JOIN calculation_rule_heads AS heads
                   ON heads.rule_id = versions.rule_id
                  AND heads.current_version = versions.version
                 JOIN calculation_rules AS rules ON rules.rule_id = versions.rule_id",
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
                        row.get(10)?,
                        row.get(11)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        let tests: Vec<CalculationRuleTestResult> = parse_canonical(&stored_rule.7)?;
        let rule_manifest = CalculationRuleManifest {
            schema_version: 1,
            rule_id: stored_rule.0.clone(),
            version: stored_rule.1,
            name: stored_rule.2.clone(),
            formula: stored_rule.3.clone(),
            engine_version: stored_rule.4.clone(),
            supported_units: parse_canonical(&stored_rule.5)?,
            supported_currencies: supported_currencies(),
            supported_rounding: parse_canonical(&stored_rule.6)?,
            change_rationale: parse_canonical::<CalculationRuleManifest>(&stored_rule.8)?
                .change_rationale,
            deterministic_tests: tests.clone(),
            created_by: "engineer_user".into(),
            created_at: stored_rule.10.clone(),
        };
        if stored_rule.1 != counts.1
            || stored_rule.2 != BOQ_RULE_NAME
            || stored_rule.3 != CONTROLLED_CALCULATION_RULE_FORMULA
            || stored_rule.4 != CALCULATION_ENGINE_VERSION
            || rule_manifest.supported_units != supported_units()
            || rule_manifest.supported_currencies != supported_currencies()
            || normalize_rounding_policy(&rule_manifest.supported_rounding).as_ref()
                != Some(&rule_manifest.supported_rounding)
            || !valid_change_rationale(&rule_manifest.change_rationale)
            || tests != expected_tests
            || tests.iter().any(|test| !test.passed)
            || stored_rule.8 != canonical_json(&rule_manifest)?
            || stored_rule.9 != sha256_hex(stored_rule.8.as_bytes())
        {
            return Ok(false);
        }
        let proposed_change = json!({
            "engine_version": CALCULATION_ENGINE_VERSION,
            "manifest_sha256": stored_rule.9,
            "rule_id": stored_rule.0,
            "rule_version": stored_rule.1.to_string(),
        });
        if !calculation_audit_is_exact(
            &self.connection,
            stored_rule.11,
            "calculation_rule_proposed",
            &stored_rule.10,
            &proposed_change,
        )? {
            return Ok(false);
        }
        check()?;
        if let Some(review) = load_rule_review(&self.connection, &stored_rule.0, stored_rule.1)? {
            let stored_review: (String, String, i64) = self
                .connection
                .query_row(
                    "SELECT manifest_json, rule_manifest_sha256, audit_sequence
                     FROM calculation_rule_reviews WHERE review_id = ?1",
                    [&review.review_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(sql_error)?;
            let review_manifest: CalculationRuleReviewManifest = parse_canonical(&stored_review.0)?;
            let rule_version = CalculationRuleVersion {
                rule_id: stored_rule.0.clone(),
                version: stored_rule.1,
                name: stored_rule.2.clone(),
                formula: stored_rule.3.clone(),
                engine_version: stored_rule.4.clone(),
                supported_units: parse_canonical(&stored_rule.5)?,
                supported_currencies: supported_currencies(),
                supported_rounding: parse_canonical(&stored_rule.6)?,
                change_rationale: rule_manifest.change_rationale.clone(),
                deterministic_tests: tests.clone(),
                review: Some(review.clone()),
                approval: None,
                current: true,
                active: false,
                manifest_sha256: stored_rule.9.clone(),
                created_at: stored_rule.10.clone(),
            };
            let run_valid = calculation_rule_review_run_is_valid(
                &self.connection,
                &review,
                &rule_version,
                check,
            )?;
            let expected_change = json!({
                "manifest_sha256": review.manifest_sha256,
                "outcome": review.outcome.as_str(),
                "review_id": review.review_id,
                "reviewer_profile_id": review.reviewer_profile_id,
                "reviewer_profile_version": review.reviewer_profile_version.to_string(),
                "reviewer_run_id": review.reviewer_run_id,
                "rule_id": stored_rule.0,
                "rule_manifest_sha256": stored_rule.9,
                "rule_version": stored_rule.1.to_string(),
            });
            if stored_review.1 != stored_rule.9
                || review_manifest.schema_version != 1
                || review_manifest.review_id != review.review_id
                || review_manifest.rule_id != stored_rule.0
                || review_manifest.rule_version != stored_rule.1
                || review_manifest.rule_manifest_sha256 != stored_rule.9
                || review_manifest.reviewer_run_id != review.reviewer_run_id
                || review_manifest.reviewer_profile_id != review.reviewer_profile_id
                || review_manifest.reviewer_profile_version != review.reviewer_profile_version
                || review_manifest.outcome != review.outcome
                || review_manifest.findings != review.findings
                || review_manifest.created_at != review.created_at
                || review.manifest_sha256 != sha256_hex(stored_review.0.as_bytes())
                || !run_valid
                || !calculation_audit_is_exact(
                    &self.connection,
                    stored_review.2,
                    "calculation_rule_review_completed",
                    &review.created_at,
                    &expected_change,
                )?
            {
                return Ok(false);
            }
        }
        check()?;
        if let Some(approval) = load_rule_approval(&self.connection, &stored_rule.0, stored_rule.1)?
        {
            let stored_approval: (String, String, String, String, i64) = self
                .connection
                .query_row(
                    "SELECT manifest_json, rule_manifest_sha256,
                            review_id, review_manifest_sha256, audit_sequence
                     FROM calculation_rule_approvals WHERE approval_id = ?1",
                    [&approval.approval_id],
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
                .map_err(sql_error)?;
            let manifest: CalculationRuleApprovalManifest = parse_canonical(&stored_approval.0)?;
            let review = load_rule_review(&self.connection, &stored_rule.0, stored_rule.1)?;
            let expected_change = json!({
                "approval_id": approval.approval_id,
                "manifest_sha256": approval.manifest_sha256,
                "review_id": stored_approval.2,
                "rule_id": stored_rule.0,
                "rule_manifest_sha256": stored_rule.9,
                "rule_version": stored_rule.1.to_string(),
            });
            if review.as_ref().is_none_or(|review| {
                review.outcome != CalculationRuleReviewOutcome::Passed
                    || review.review_id != stored_approval.2
                    || review.manifest_sha256 != stored_approval.3
            }) || stored_approval.1 != stored_rule.9
                || manifest.schema_version != 1
                || manifest.approval_id != approval.approval_id
                || manifest.rule_id != stored_rule.0
                || manifest.rule_version != stored_rule.1
                || manifest.rule_manifest_sha256 != stored_rule.9
                || manifest.review_id != stored_approval.2
                || manifest.review_manifest_sha256 != stored_approval.3
                || manifest.rationale != approval.rationale
                || manifest.approved_by != "engineer_user"
                || manifest.acting_role != "engineer_in_the_loop"
                || manifest.created_at != approval.created_at
                || approval.manifest_sha256 != sha256_hex(stored_approval.0.as_bytes())
                || !calculation_audit_is_exact(
                    &self.connection,
                    stored_approval.4,
                    "calculation_rule_activated",
                    &approval.created_at,
                    &expected_change,
                )?
            {
                return Ok(false);
            }
        }
        let scenario_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM calculation_scenario_versions",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if scenario_count > MAX_CALCULATION_SCENARIOS {
            return Ok(false);
        }
        let mut scenario_statement = self
            .connection
            .prepare(
                "SELECT scenario_id, version, name, quantity_unit, rate_basis_unit,
                        rate_currency, exchange_rate_id, exchange_rate_version,
                        exchange_rate_json, exchange_rate_effective_date, pricing_date,
                        exchange_rate_type, output_currency, rounding_policy_id,
                        rounding_policy_version, precision, rounding_mode, rationale,
                        approved_by, acting_role, audit_sequence, manifest_json,
                        manifest_sha256, created_at
                 FROM calculation_scenario_versions ORDER BY rowid",
            )
            .map_err(sql_error)?;
        let scenario_rows = scenario_statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, u32>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, u32>(14)?,
                    row.get::<_, u32>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, String>(17)?,
                    row.get::<_, String>(18)?,
                    row.get::<_, String>(19)?,
                    row.get::<_, i64>(20)?,
                    row.get::<_, String>(21)?,
                    row.get::<_, String>(22)?,
                    row.get::<_, String>(23)?,
                ))
            })
            .map_err(sql_error)?;
        for row in scenario_rows {
            check()?;
            let row = row.map_err(sql_error)?;
            let manifest: CalculationScenarioManifest = parse_canonical(&row.21)?;
            let exchange_rate: CalculationDecimalInput = parse_canonical(&row.8)?;
            let exchange_rate_type = row.11.as_deref().map(ExchangeRateType::parse).transpose()?;
            let rounding_mode = CalculationRoundingMode::parse(&row.16)?;
            let quantity_unit = unit_definition(&row.3);
            let rate_basis_unit = unit_definition(&row.4);
            let same_currency = row.5 == row.12;
            let expected_change = json!({
                "exchange_rate_id": row.6,
                "manifest_sha256": row.22,
                "rounding_policy_id": row.13,
                "scenario_id": row.0,
                "scenario_version": row.1.to_string(),
            });
            if manifest.schema_version != 1
                || manifest.scenario_id != row.0
                || manifest.version != row.1
                || manifest.name != row.2
                || manifest.quantity_unit != row.3
                || manifest.rate_basis_unit != row.4
                || manifest.rate_currency != row.5
                || manifest.exchange_rate_id != row.6
                || manifest.exchange_rate_version != row.7
                || manifest.exchange_rate != exchange_rate
                || manifest.exchange_rate_effective_date != row.9
                || manifest.pricing_date != row.10
                || manifest.exchange_rate_type != exchange_rate_type
                || manifest.output_currency != row.12
                || manifest.rounding_policy_id != row.13
                || manifest.rounding_policy_version != row.14
                || manifest.precision != row.15
                || manifest.rounding_mode != rounding_mode
                || !rule_manifest.supported_rounding.contains(&rounding_mode)
                || manifest.rationale != row.17
                || manifest.rationale.trim() != manifest.rationale
                || manifest.approved_by != row.18
                || manifest.approved_by != "engineer_user"
                || manifest.acting_role != row.19
                || manifest.acting_role != "engineer_in_the_loop"
                || manifest.created_at != row.23
                || quantity_unit.is_none()
                || rate_basis_unit.is_none()
                || !valid_currency(&manifest.rate_currency)
                || !valid_currency(&manifest.output_currency)
                || !valid_input_shape(&exchange_rate, same_currency)
                || !valid_exchange_rate_governance(
                    same_currency,
                    &exchange_rate,
                    row.9.as_deref(),
                    &row.10,
                    exchange_rate_type,
                )
                || (!same_currency
                    && exchange_rate.state == CalculationInputState::Provided
                    && exchange_rate.value.as_deref().is_none_or(|value| {
                        parse_nonnegative_decimal(value).map_or(true, |value| value.is_zero())
                    }))
                || !input_evidence_is_authoritative(&self.connection, &exchange_rate, check)?
                || row.22 != sha256_hex(row.21.as_bytes())
                || !calculation_audit_is_exact(
                    &self.connection,
                    row.20,
                    "calculation_scenario_approved",
                    &row.23,
                    &expected_change,
                )?
            {
                return Ok(false);
            }
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT calculation_run_id, cost_estimator_run_id, rule_id, rule_version,
                        rule_approval_id, scenario_id, scenario_version, status,
                        manifest_json, manifest_sha256, audit_sequence, created_at
                 FROM calculation_runs ORDER BY rowid",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, u32>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            check()?;
            let row = row.map_err(sql_error)?;
            let Some(manifest) = calculation_run_core_is_valid(&self.connection, &row, check)?
            else {
                return Ok(false);
            };
            if !calculation_run_approval_is_valid(&self.connection, &manifest, &row.9, check)? {
                return Ok(false);
            }
        }
        check()?;
        Ok(true)
    }
}

fn valid_input_shape(input: &CalculationDecimalInput, allow_not_applicable: bool) -> bool {
    if input.evidence.len() > 32 {
        return false;
    }
    if input.evidence.iter().any(|reference| {
        reference.kind.is_empty()
            || reference.kind.len() > 64
            || reference.reference.is_empty()
            || reference.reference.len() > 400
            || reference.version == 0
    }) {
        return false;
    }
    let mut evidence = input.evidence.clone();
    evidence.sort_by(|a, b| {
        (&a.kind, &a.reference, a.version).cmp(&(&b.kind, &b.reference, b.version))
    });
    if evidence.windows(2).any(|pair| pair[0] == pair[1]) {
        return false;
    }
    match input.state {
        CalculationInputState::Provided => {
            input
                .value
                .as_deref()
                .is_some_and(|value| !value.is_empty() && value.len() <= MAX_DECIMAL_BYTES)
                && !input.evidence.is_empty()
        }
        CalculationInputState::Missing => input.value.is_none() && input.evidence.is_empty(),
        CalculationInputState::Unavailable | CalculationInputState::Ambiguous => {
            input.value.is_none()
        }
        CalculationInputState::NotApplicable => {
            allow_not_applicable && input.value.is_none() && input.evidence.is_empty()
        }
    }
}

fn valid_exchange_rate_governance(
    same_currency: bool,
    input: &CalculationDecimalInput,
    effective_date: Option<&str>,
    pricing_date: &str,
    rate_type: Option<ExchangeRateType>,
) -> bool {
    if !valid_iso_date(pricing_date) {
        return false;
    }
    if same_currency {
        return input.state == CalculationInputState::NotApplicable
            && effective_date.is_none()
            && rate_type.is_none();
    }
    if rate_type.is_none() || input.state == CalculationInputState::NotApplicable {
        return false;
    }
    match input.state {
        CalculationInputState::Provided => effective_date
            .filter(|date| valid_iso_date(date))
            .is_some_and(|date| date <= pricing_date),
        CalculationInputState::Missing
        | CalculationInputState::Unavailable
        | CalculationInputState::Ambiguous => effective_date.is_none(),
        CalculationInputState::NotApplicable => false,
    }
}

fn candidate_evidence_is_within_task(
    task: &TenderTaskView,
    input: &CalculationDecimalInput,
    tagged_kind: &str,
) -> bool {
    input.evidence.iter().all(|reference| {
        reference.kind == "source_evidence"
            && task.exact_inputs.iter().any(|exact| {
                exact.kind == tagged_kind
                    && exact.reference == reference.reference
                    && exact.version == reference.version
            })
    })
}

fn input_evidence_is_authoritative(
    transaction: &rusqlite::Connection,
    input: &CalculationDecimalInput,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    for evidence in &input.evidence {
        check()?;
        if evidence.kind != "source_evidence"
            || !query_evidence_reference_exists(transaction, evidence)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn validate_calculation_evidence_basket(
    references: &[AgentTaskInputReference],
) -> Result<(), TenderCommandError> {
    if references.is_empty()
        || references.len() > 32
        || references.iter().any(|reference| {
            reference.kind != "source_evidence"
                || reference.reference.is_empty()
                || reference.reference.len() > 400
                || reference.version == 0
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut sorted = references.to_vec();
    sorted.sort_by(|a, b| {
        (&a.kind, &a.reference, a.version).cmp(&(&b.kind, &b.reference, b.version))
    });
    if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn calculation_evidence_basket_is_authoritative(
    connection: &rusqlite::Connection,
    references: &[AgentTaskInputReference],
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    for reference in references {
        check()?;
        if reference.kind != "source_evidence"
            || !query_evidence_reference_exists(connection, reference)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn calculation_source_evidence_view(
    connection: &rusqlite::Connection,
    references: &[AgentTaskInputReference],
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<serde_json::Value>, TenderCommandError> {
    let mut result = Vec::with_capacity(references.len());
    for reference in references {
        check()?;
        let Some((artifact_id, ordinal)) = reference.reference.rsplit_once('#') else {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        };
        let ordinal = ordinal
            .parse::<u32>()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let evidence: Option<(String, String, Option<String>, String)> = connection
            .query_row(
                "SELECT artifacts.package_path, locations.kind,
                        locations.cell_range, locations.original_text
                 FROM evidence_locations AS locations
                 JOIN source_artifacts AS artifacts
                   ON artifacts.artifact_id = locations.artifact_id
                 WHERE locations.artifact_id = ?1
                   AND locations.version = ?2 AND locations.ordinal = ?3",
                params![artifact_id, reference.version, ordinal],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((document_name, location_kind, cell_range, original_text)) = evidence else {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        };
        result.push(json!({
            "document_name": document_name,
            "location_kind": location_kind,
            "cell_range": cell_range,
            "original_text": original_text,
            "reference": reference,
        }));
    }
    Ok(result)
}

pub(crate) fn calculation_evidence_view_for_estimate(
    connection: &rusqlite::Connection,
    references: &[AgentTaskInputReference],
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<serde_json::Value>, TenderCommandError> {
    calculation_source_evidence_view(connection, references, check)
}

fn evaluate_candidate(
    candidate: &CostEstimatorCalculationCandidate,
    scenario: &CalculationScenarioVersion,
) -> (
    ControlledBoqCalculationStatus,
    Option<String>,
    Option<ExactBoqResult>,
) {
    let relevant = if scenario.rate_currency == scenario.output_currency {
        vec![&candidate.quantity, &candidate.unit_rate]
    } else {
        vec![
            &candidate.quantity,
            &candidate.unit_rate,
            &scenario.exchange_rate,
        ]
    };
    if relevant
        .iter()
        .any(|input| input.state == CalculationInputState::Ambiguous)
    {
        return (
            ControlledBoqCalculationStatus::AmbiguousInput,
            Some("ambiguous_input".into()),
            None,
        );
    }
    if relevant
        .iter()
        .any(|input| input.state == CalculationInputState::Unavailable)
    {
        return (
            ControlledBoqCalculationStatus::UnavailableInput,
            Some("unavailable_input".into()),
            None,
        );
    }
    if relevant
        .iter()
        .any(|input| input.state == CalculationInputState::Missing)
    {
        return (
            ControlledBoqCalculationStatus::MissingInput,
            Some("missing_input".into()),
            None,
        );
    }
    let Some(quantity) = candidate.quantity.value.as_deref() else {
        return (
            ControlledBoqCalculationStatus::InvalidInput,
            Some("invalid_input_state".into()),
            None,
        );
    };
    let Some(unit_rate) = candidate.unit_rate.value.as_deref() else {
        return (
            ControlledBoqCalculationStatus::InvalidInput,
            Some("invalid_input_state".into()),
            None,
        );
    };
    let exchange_rate = if scenario.rate_currency == scenario.output_currency {
        "1"
    } else if let Some(value) = scenario.exchange_rate.value.as_deref() {
        value
    } else {
        return (
            ControlledBoqCalculationStatus::InvalidInput,
            Some("invalid_input_state".into()),
            None,
        );
    };
    match evaluate_boq_line(
        quantity,
        &scenario.quantity_unit,
        unit_rate,
        &scenario.rate_basis_unit,
        &scenario.rate_currency,
        exchange_rate,
        &scenario.output_currency,
        scenario.precision,
        scenario.rounding_mode,
    ) {
        Ok(result) => (
            ControlledBoqCalculationStatus::Completed,
            None,
            Some(result),
        ),
        Err(ExactBoqError::DimensionMismatch) => (
            ControlledBoqCalculationStatus::DimensionMismatch,
            Some("dimension_mismatch".into()),
            None,
        ),
        Err(error) => (
            ControlledBoqCalculationStatus::InvalidInput,
            Some(
                match error {
                    ExactBoqError::InvalidDecimal => "invalid_decimal",
                    ExactBoqError::NegativeValue => "negative_value",
                    ExactBoqError::UnknownUnit => "unknown_unit",
                    ExactBoqError::InvalidCurrency => "invalid_currency",
                    ExactBoqError::InvalidPrecision => "invalid_precision",
                    ExactBoqError::ArithmeticOverflow => "arithmetic_overflow",
                    ExactBoqError::DimensionMismatch => unreachable!(),
                }
                .into(),
            ),
            None,
        ),
    }
}

fn run_from_manifest(
    manifest: CalculationRunManifest,
    manifest_sha256: String,
) -> Result<ControlledBoqCalculationRun, TenderCommandError> {
    if manifest.schema_version != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(ControlledBoqCalculationRun {
        calculation_run_id: manifest.calculation_run_id,
        cost_estimator_run_id: manifest.cost_estimator_run_id,
        tender_revision: manifest.tender_revision,
        rule_id: manifest.rule_id,
        rule_version: manifest.rule_version,
        rule_approval_id: manifest.rule_approval_id,
        description: manifest.description,
        scenario_id: manifest.scenario_id,
        scenario_version: manifest.scenario_version,
        scenario_name: manifest.scenario_name,
        scenario_manifest_sha256: manifest.scenario_manifest_sha256,
        exchange_rate_id: manifest.exchange_rate_id,
        exchange_rate_version: manifest.exchange_rate_version,
        rounding_policy_id: manifest.rounding_policy_id,
        rounding_policy_version: manifest.rounding_policy_version,
        quantity: manifest.quantity,
        quantity_unit: manifest.quantity_unit,
        unit_rate: manifest.unit_rate,
        rate_basis_unit: manifest.rate_basis_unit,
        rate_currency: manifest.rate_currency,
        exchange_rate: manifest.exchange_rate,
        exchange_rate_effective_date: manifest.exchange_rate_effective_date,
        pricing_date: manifest.pricing_date,
        exchange_rate_type: manifest.exchange_rate_type,
        output_currency: manifest.output_currency,
        precision: manifest.precision,
        rounding_mode: manifest.rounding_mode,
        engine_version: manifest.engine_version,
        normalized_quantity: manifest.normalized_quantity,
        unrounded_source_amount: manifest.unrounded_source_amount,
        unrounded_output_amount: manifest.unrounded_output_amount,
        final_amount: manifest.final_amount,
        status: manifest.status,
        diagnostic_code: manifest.diagnostic_code,
        manifest_sha256,
        approval: None,
        created_at: manifest.created_at,
    })
}

fn scenario_from_manifest(
    manifest: CalculationScenarioManifest,
    manifest_sha256: String,
) -> Result<CalculationScenarioVersion, TenderCommandError> {
    if manifest.schema_version != 1 || manifest.version != 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(CalculationScenarioVersion {
        scenario_id: manifest.scenario_id,
        version: manifest.version,
        name: manifest.name,
        quantity_unit: manifest.quantity_unit,
        rate_basis_unit: manifest.rate_basis_unit,
        rate_currency: manifest.rate_currency,
        exchange_rate_id: manifest.exchange_rate_id,
        exchange_rate_version: manifest.exchange_rate_version,
        exchange_rate: manifest.exchange_rate,
        exchange_rate_effective_date: manifest.exchange_rate_effective_date,
        pricing_date: manifest.pricing_date,
        exchange_rate_type: manifest.exchange_rate_type,
        output_currency: manifest.output_currency,
        rounding_policy_id: manifest.rounding_policy_id,
        rounding_policy_version: manifest.rounding_policy_version,
        precision: manifest.precision,
        rounding_mode: manifest.rounding_mode,
        rationale: manifest.rationale,
        approved_by: manifest.approved_by,
        acting_role: manifest.acting_role,
        manifest_sha256,
        created_at: manifest.created_at,
    })
}

fn load_calculation_scenario(
    connection: &rusqlite::Connection,
    scenario_id: &str,
    version: u32,
) -> Result<CalculationScenarioVersion, TenderCommandError> {
    let stored: Option<(String, String)> = connection
        .query_row(
            "SELECT manifest_json, manifest_sha256
             FROM calculation_scenario_versions
             WHERE scenario_id = ?1 AND version = ?2",
            params![scenario_id, version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let Some((manifest_json, manifest_sha256)) = stored else {
        return Err(TenderCommandError::new(TenderErrorCode::NotFound));
    };
    scenario_from_manifest(parse_canonical(&manifest_json)?, manifest_sha256)
}

fn load_calculation_run_approval(
    connection: &rusqlite::Connection,
    calculation_run_id: &str,
) -> Result<Option<CalculationRunApproval>, TenderCommandError> {
    connection
        .query_row(
            "SELECT approval_id, run_manifest_sha256, rationale, approved_by,
                    acting_role, manifest_sha256, created_at
             FROM calculation_run_approvals WHERE calculation_run_id = ?1",
            [calculation_run_id],
            |row| {
                Ok(CalculationRunApproval {
                    approval_id: row.get(0)?,
                    calculation_run_id: calculation_run_id.into(),
                    run_manifest_sha256: row.get(1)?,
                    rationale: row.get(2)?,
                    approved_by: row.get(3)?,
                    acting_role: row.get(4)?,
                    manifest_sha256: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)
}

fn calculation_run_view(
    connection: &rusqlite::Connection,
    manifest_json: &str,
    manifest_sha256: String,
) -> Result<ControlledBoqCalculationRun, TenderCommandError> {
    let mut run = run_from_manifest(parse_canonical(manifest_json)?, manifest_sha256)?;
    run.approval = load_calculation_run_approval(connection, &run.calculation_run_id)?;
    Ok(run)
}

fn load_rule_review(
    connection: &rusqlite::Connection,
    rule_id: &str,
    version: u32,
) -> Result<Option<CalculationRuleReview>, TenderCommandError> {
    let stored: Option<StoredCalculationRuleReviewRow> = connection
        .query_row(
            "SELECT review_id, reviewer_run_id, reviewer_profile_id,
                    reviewer_profile_version, outcome, findings_json,
                    manifest_sha256, created_at
             FROM calculation_rule_reviews WHERE rule_id = ?1 AND rule_version = ?2",
            params![rule_id, version],
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
        .optional()
        .map_err(sql_error)?;
    stored
        .map(|stored| {
            Ok(CalculationRuleReview {
                review_id: stored.0,
                reviewer_run_id: stored.1,
                reviewer_profile_id: stored.2,
                reviewer_profile_version: stored.3,
                outcome: parse_review_outcome(&stored.4)?,
                findings: parse_canonical(&stored.5)?,
                manifest_sha256: stored.6,
                created_at: stored.7,
            })
        })
        .transpose()
}

fn load_rule_approval(
    connection: &rusqlite::Connection,
    rule_id: &str,
    version: u32,
) -> Result<Option<CalculationRuleApproval>, TenderCommandError> {
    let stored: Option<(String, String, String, String, String, String)> = connection
        .query_row(
            "SELECT approval_id, rationale, approved_by, acting_role,
                    manifest_sha256, created_at
             FROM calculation_rule_approvals WHERE rule_id = ?1 AND rule_version = ?2",
            params![rule_id, version],
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
    Ok(stored.map(|stored| CalculationRuleApproval {
        approval_id: stored.0,
        rationale: stored.1,
        approved_by: stored.2,
        acting_role: stored.3,
        manifest_sha256: stored.4,
        created_at: stored.5,
    }))
}

fn parse_review_outcome(value: &str) -> Result<CalculationRuleReviewOutcome, TenderCommandError> {
    match value {
        "passed" => Ok(CalculationRuleReviewOutcome::Passed),
        "failed" => Ok(CalculationRuleReviewOutcome::Failed),
        _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
    }
}

fn calculation_audit_is_exact(
    connection: &rusqlite::Connection,
    sequence: i64,
    event_type: &str,
    created_at: &str,
    expected_change: &serde_json::Value,
) -> Result<bool, TenderCommandError> {
    let audit: Option<(String, String, String)> = connection
        .query_row(
            "SELECT event_type, payload_json, created_at FROM audit_events WHERE sequence = ?1",
            [sequence],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    Ok(audit.is_some_and(|audit| {
        audit.0 == event_type
            && audit.2 == created_at
            && serde_json::from_str::<serde_json::Value>(&audit.1)
                .is_ok_and(|payload| payload.get("change") == Some(expected_change))
    }))
}

fn stored_calculation_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCalculationRun> {
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
}

fn load_stored_calculation_run(
    connection: &rusqlite::Connection,
    calculation_run_id: &str,
) -> Result<Option<StoredCalculationRun>, TenderCommandError> {
    connection
        .query_row(
            "SELECT calculation_run_id, cost_estimator_run_id, rule_id, rule_version,
                    rule_approval_id, scenario_id, scenario_version, status,
                    manifest_json, manifest_sha256, audit_sequence, created_at
             FROM calculation_runs WHERE calculation_run_id = ?1",
            [calculation_run_id],
            stored_calculation_run,
        )
        .optional()
        .map_err(sql_error)
}

fn calculation_run_core_is_valid(
    connection: &rusqlite::Connection,
    row: &StoredCalculationRun,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Option<CalculationRunManifest>, TenderCommandError> {
    check()?;
    if row.9 != sha256_hex(row.8.as_bytes()) {
        return Ok(None);
    }
    let manifest: CalculationRunManifest = parse_canonical(&row.8)?;
    let approval_basis: Option<(String, String)> = connection
        .query_row(
            "SELECT manifest_sha256, rule_manifest_sha256
             FROM calculation_rule_approvals
             WHERE approval_id = ?1 AND rule_id = ?2 AND rule_version = ?3",
            params![row.4, row.2, row.3],
            |basis| Ok((basis.get(0)?, basis.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let scenario = load_calculation_scenario(connection, &row.5, row.6)?;
    let candidate = CostEstimatorCalculationCandidate {
        quantity: manifest.quantity.clone(),
        unit_rate: manifest.unit_rate.clone(),
    };
    if !valid_input_shape(&candidate.quantity, false)
        || !valid_input_shape(&candidate.unit_rate, false)
        || !input_evidence_is_authoritative(connection, &manifest.quantity, check)?
        || !input_evidence_is_authoritative(connection, &manifest.unit_rate, check)?
    {
        return Ok(None);
    }
    let (status, diagnostic, exact) = evaluate_candidate(&candidate, &scenario);
    let expected_change = json!({
        "calculation_run_id": row.0,
        "cost_estimator_run_id": row.1,
        "manifest_sha256": row.9,
        "rule_approval_id": row.4,
        "rule_id": row.2,
        "rule_version": row.3.to_string(),
        "scenario_id": row.5,
        "scenario_version": row.6.to_string(),
        "status": row.7,
    });
    let valid = approval_basis.as_ref().is_some_and(|basis| {
        basis.0 == manifest.rule_approval_manifest_sha256
            && basis.1 == manifest.rule_manifest_sha256
    }) && manifest.schema_version == 1
        && manifest.calculation_run_id == row.0
        && manifest.cost_estimator_run_id == row.1
        && manifest.tender_revision > 0
        && manifest.rule_id == row.2
        && manifest.rule_version == row.3
        && manifest.rule_approval_id == row.4
        && manifest.scenario_id == row.5
        && manifest.scenario_version == row.6
        && manifest.scenario_name == scenario.name
        && manifest.scenario_manifest_sha256 == scenario.manifest_sha256
        && manifest.exchange_rate_id == scenario.exchange_rate_id
        && manifest.exchange_rate_version == scenario.exchange_rate_version
        && manifest.rounding_policy_id == scenario.rounding_policy_id
        && manifest.rounding_policy_version == scenario.rounding_policy_version
        && manifest.quantity_unit == scenario.quantity_unit
        && manifest.rate_basis_unit == scenario.rate_basis_unit
        && manifest.rate_currency == scenario.rate_currency
        && manifest.exchange_rate == scenario.exchange_rate
        && manifest.exchange_rate_effective_date == scenario.exchange_rate_effective_date
        && manifest.pricing_date == scenario.pricing_date
        && manifest.exchange_rate_type == scenario.exchange_rate_type
        && manifest.output_currency == scenario.output_currency
        && manifest.precision == scenario.precision
        && manifest.rounding_mode == scenario.rounding_mode
        && manifest.engine_version == CALCULATION_ENGINE_VERSION
        && cost_estimator_run_is_valid(connection, &manifest, check)?
        && manifest.status == status
        && manifest.status.as_str() == row.7
        && manifest.diagnostic_code == diagnostic
        && manifest.normalized_quantity
            == exact
                .as_ref()
                .map(|value| value.normalized_quantity.clone())
        && manifest.unrounded_source_amount
            == exact
                .as_ref()
                .map(|value| value.unrounded_source_amount.clone())
        && manifest.unrounded_output_amount
            == exact
                .as_ref()
                .map(|value| value.unrounded_output_amount.clone())
        && manifest.final_amount == exact.as_ref().map(|value| value.final_amount.clone())
        && manifest.created_at == row.11
        && calculation_audit_is_exact(
            connection,
            row.10,
            "controlled_boq_calculation_recorded",
            &row.11,
            &expected_change,
        )?;
    Ok(valid.then_some(manifest))
}

fn calculation_run_approval_is_valid(
    connection: &rusqlite::Connection,
    run_manifest: &CalculationRunManifest,
    run_manifest_sha256: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    type StoredApproval = (
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
    check()?;
    let approval: Option<StoredApproval> = connection
        .query_row(
            "SELECT approval_id, run_manifest_sha256, rationale, approved_by,
                    acting_role, tender_revision, audit_sequence, manifest_json,
                    manifest_sha256, created_at
             FROM calculation_run_approvals WHERE calculation_run_id = ?1",
            [&run_manifest.calculation_run_id],
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
    let authority: Option<CalculationAuthorityRecord> = connection
        .query_row(
            "SELECT kind, value, description, manifest_sha256,
                    tender_revision, created_by, created_at
             FROM tender_record_authorities WHERE authority_id = ?1",
            [&run_manifest.calculation_run_id],
            |row| {
                Ok(CalculationAuthorityRecord {
                    kind: row.get(0)?,
                    value: row.get(1)?,
                    description: row.get(2)?,
                    manifest_sha256: row.get(3)?,
                    tender_revision: row.get(4)?,
                    created_by: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some(approval) = approval else {
        return Ok(authority.is_none());
    };
    let approval_manifest: CalculationRunApprovalManifest = parse_canonical(&approval.7)?;
    let expected_change = json!({
        "approval_id": approval.0,
        "calculation_run_id": run_manifest.calculation_run_id,
        "manifest_sha256": approval.8,
        "run_manifest_sha256": run_manifest_sha256,
    });
    let final_amount = run_manifest.final_amount.as_deref().unwrap_or_default();
    Ok(
        run_manifest.status == ControlledBoqCalculationStatus::Completed
            && run_manifest.final_amount.is_some()
            && !approval.2.trim().is_empty()
            && approval.2.len() <= 4_000
            && approval_manifest.schema_version == 1
            && approval_manifest.approval_id == approval.0
            && approval_manifest.calculation_run_id == run_manifest.calculation_run_id
            && approval_manifest.run_manifest_sha256 == run_manifest_sha256
            && approval_manifest.rationale == approval.2
            && approval_manifest.approved_by == approval.3
            && approval_manifest.acting_role == approval.4
            && approval_manifest.tender_revision == approval.5
            && approval_manifest.created_at == approval.9
            && approval.1 == run_manifest_sha256
            && approval.3 == "engineer_user"
            && approval.4 == "engineer_in_the_loop"
            && approval.5 == run_manifest.tender_revision
            && approval.8 == sha256_hex(approval.7.as_bytes())
            && calculation_audit_is_exact(
                connection,
                approval.6,
                "controlled_boq_calculation_approved",
                &approval.9,
                &expected_change,
            )?
            && authority.as_ref().is_some_and(|authority| {
                authority.kind == "calculation_run"
                    && authority.value
                        == format!("{} {}", final_amount, run_manifest.output_currency)
                    && authority.description == run_manifest.description
                    && authority.manifest_sha256.as_deref() == Some(run_manifest_sha256)
                    && authority.tender_revision == approval.5
                    && authority.created_by == CALCULATION_ENGINE_VERSION
                    && authority.created_at == approval.9
            }),
    )
}

pub(crate) fn approved_calculation_run_for_estimate(
    connection: &rusqlite::Connection,
    calculation_run_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Option<ControlledBoqCalculationRun>, TenderCommandError> {
    let Some(stored) = load_stored_calculation_run(connection, calculation_run_id)? else {
        return Ok(None);
    };
    let Some(manifest) = calculation_run_core_is_valid(connection, &stored, check)? else {
        return Ok(None);
    };
    if !calculation_run_approval_is_valid(connection, &manifest, &stored.9, check)? {
        return Ok(None);
    }
    let run = calculation_run_view(connection, &stored.8, stored.9)?;
    Ok(run.approval.is_some().then_some(run))
}

pub(crate) struct RecordEstimateAggregateCalculation<'a> {
    pub aggregate_run_id: &'a str,
    pub author_run_id: &'a str,
    pub comparison_total_calculation_run_id: &'a str,
    pub tender_revision: u32,
    pub inputs: Vec<EstimateAggregateCalculationInput>,
    pub tender_id: &'a str,
    pub created_at: &'a str,
}

pub(crate) fn evaluate_estimate_aggregate(
    inputs: &[EstimateAggregateCalculationInput],
    precision: u32,
    rounding_mode: CalculationRoundingMode,
) -> Result<String, TenderCommandError> {
    let mut sum = Decimal::ZERO;
    for input in inputs {
        sum = sum
            .checked_add(
                Decimal::from_str(&input.amount)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            )
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    }
    let strategy = match rounding_mode {
        CalculationRoundingMode::MidpointAwayFromZero => RoundingStrategy::MidpointAwayFromZero,
        CalculationRoundingMode::MidpointNearestEven => RoundingStrategy::MidpointNearestEven,
    };
    let rounded = sum.round_dp_with_strategy(precision, strategy);
    Ok(format!(
        "{rounded:.precision$}",
        precision = precision as usize
    ))
}

pub(crate) fn record_estimate_aggregate_calculation(
    transaction: &Transaction<'_>,
    mut request: RecordEstimateAggregateCalculation<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<EstimateAggregateCalculationRun, TenderCommandError> {
    check()?;
    let comparison = approved_calculation_run_for_estimate(
        transaction,
        request.comparison_total_calculation_run_id,
        check,
    )?
    .filter(|run| {
        run.status == ControlledBoqCalculationStatus::Completed
            && run.tender_revision == request.tender_revision
    })
    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if request.inputs.len() > 256
        || request
            .inputs
            .iter()
            .any(|input| input.currency != comparison.output_currency)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    request.inputs.sort_by(|left, right| {
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
    let mut seen = std::collections::HashSet::new();
    for input in &request.inputs {
        check()?;
        if !seen.insert(input.calculation_run_id.clone()) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let run =
            approved_calculation_run_for_estimate(transaction, &input.calculation_run_id, check)?
                .filter(|run| {
                    run.status == ControlledBoqCalculationStatus::Completed
                        && run.tender_revision == request.tender_revision
                })
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if run.manifest_sha256 != input.calculation_manifest_sha256
            || run.final_amount.as_deref() != Some(input.amount.as_str())
            || run.output_currency != input.currency
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    let final_amount = evaluate_estimate_aggregate(
        &request.inputs,
        comparison.precision,
        comparison.rounding_mode,
    )?;
    let comparison_total_amount = comparison
        .final_amount
        .clone()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    type RuleBasis = (String, String, String);
    let rule_basis: RuleBasis = transaction
        .query_row(
            "SELECT versions.manifest_sha256, approvals.manifest_sha256,
                    scenarios.manifest_sha256
             FROM calculation_rule_versions AS versions
             JOIN calculation_rule_approvals AS approvals
               ON approvals.rule_id = versions.rule_id
              AND approvals.rule_version = versions.version
             JOIN calculation_scenario_versions AS scenarios
               ON scenarios.scenario_id = ?4 AND scenarios.version = ?5
             WHERE versions.rule_id = ?1 AND versions.version = ?2
               AND approvals.approval_id = ?3",
            params![
                comparison.rule_id,
                comparison.rule_version,
                comparison.rule_approval_id,
                comparison.scenario_id,
                comparison.scenario_version,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    let manifest = EstimateAggregateCalculationManifest {
        schema_version: 1,
        aggregate_run_id: request.aggregate_run_id.into(),
        author_run_id: request.author_run_id.into(),
        comparison_total_calculation_run_id: request.comparison_total_calculation_run_id.into(),
        comparison_total_manifest_sha256: comparison.manifest_sha256.clone(),
        comparison_total_amount,
        tender_revision: request.tender_revision,
        rule_id: comparison.rule_id.clone(),
        rule_version: comparison.rule_version,
        rule_approval_id: comparison.rule_approval_id.clone(),
        rule_manifest_sha256: rule_basis.0,
        rule_approval_manifest_sha256: rule_basis.1,
        scenario_id: comparison.scenario_id.clone(),
        scenario_version: comparison.scenario_version,
        scenario_manifest_sha256: rule_basis.2,
        precision: comparison.precision,
        rounding_mode: comparison.rounding_mode,
        engine_version: CALCULATION_ENGINE_VERSION.into(),
        inputs: request.inputs,
        final_amount: final_amount.clone(),
        currency: comparison.output_currency.clone(),
        created_at: request.created_at.into(),
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        request.tender_id,
        "estimate_aggregate_calculation_recorded",
        request.tender_revision,
        json!({
            "aggregate_run_id": request.aggregate_run_id,
            "author_run_id": request.author_run_id,
            "comparison_total_calculation_run_id": request.comparison_total_calculation_run_id,
            "manifest_sha256": manifest_sha256,
        }),
        request.created_at,
    )?;
    transaction
        .execute(
            "INSERT INTO estimate_aggregate_calculation_runs (
               aggregate_run_id, author_run_id, comparison_total_calculation_run_id,
               tender_revision, rule_id, rule_version, rule_approval_id,
               scenario_id, scenario_version, precision, rounding_mode,
               final_amount, currency, manifest_json, manifest_sha256,
               audit_sequence, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                       ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                manifest.aggregate_run_id,
                manifest.author_run_id,
                manifest.comparison_total_calculation_run_id,
                manifest.tender_revision,
                manifest.rule_id,
                manifest.rule_version,
                manifest.rule_approval_id,
                manifest.scenario_id,
                manifest.scenario_version,
                manifest.precision,
                manifest.rounding_mode.as_str(),
                manifest.final_amount,
                manifest.currency,
                manifest_json,
                manifest_sha256,
                audit_sequence,
                manifest.created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(aggregate_view(manifest, manifest_sha256, false))
}

fn aggregate_view(
    manifest: EstimateAggregateCalculationManifest,
    manifest_sha256: String,
    approved_for_reliance: bool,
) -> EstimateAggregateCalculationRun {
    EstimateAggregateCalculationRun {
        aggregate_run_id: manifest.aggregate_run_id,
        author_run_id: manifest.author_run_id,
        comparison_total_calculation_run_id: manifest.comparison_total_calculation_run_id,
        comparison_total_manifest_sha256: manifest.comparison_total_manifest_sha256,
        comparison_total_amount: manifest.comparison_total_amount,
        rule_id: manifest.rule_id,
        rule_version: manifest.rule_version,
        rule_approval_id: manifest.rule_approval_id,
        scenario_id: manifest.scenario_id,
        scenario_version: manifest.scenario_version,
        precision: manifest.precision,
        rounding_mode: manifest.rounding_mode,
        engine_version: manifest.engine_version,
        inputs: manifest.inputs,
        final_amount: manifest.final_amount,
        currency: manifest.currency,
        manifest_sha256,
        approved_for_reliance,
    }
}

pub(crate) fn load_estimate_aggregate_calculation(
    connection: &rusqlite::Connection,
    aggregate_run_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Option<EstimateAggregateCalculationRun>, TenderCommandError> {
    check()?;
    let stored: Option<(String, String, i64, String)> = connection
        .query_row(
            "SELECT manifest_json, manifest_sha256, audit_sequence, created_at
             FROM estimate_aggregate_calculation_runs WHERE aggregate_run_id = ?1",
            [aggregate_run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let Some((manifest_json, manifest_sha256, audit_sequence, created_at)) = stored else {
        return Ok(None);
    };
    let manifest: EstimateAggregateCalculationManifest = parse_canonical(&manifest_json)?;
    let expected_change = json!({
        "aggregate_run_id": manifest.aggregate_run_id,
        "author_run_id": manifest.author_run_id,
        "comparison_total_calculation_run_id": manifest.comparison_total_calculation_run_id,
        "manifest_sha256": manifest_sha256,
    });
    if manifest.schema_version != 1
        || manifest.aggregate_run_id != aggregate_run_id
        || manifest.created_at != created_at
        || manifest.engine_version != CALCULATION_ENGINE_VERSION
        || sha256_hex(manifest_json.as_bytes()) != manifest_sha256
        || !calculation_audit_is_exact(
            connection,
            audit_sequence,
            "estimate_aggregate_calculation_recorded",
            &created_at,
            &expected_change,
        )?
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    if !estimate_aggregate_manifest_is_valid(connection, &manifest, check)? {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let approval_count: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM estimate_aggregate_calculation_approvals
             WHERE aggregate_run_id = ?1",
            [aggregate_run_id],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let approval_exists =
        estimate_aggregate_approval_is_valid(connection, &manifest, &manifest_sha256)?;
    if approval_count > 1 || (approval_count == 1) != approval_exists {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(Some(aggregate_view(
        manifest,
        manifest_sha256,
        approval_exists,
    )))
}

pub(crate) struct RecordPricingCalculation<'a> {
    pub pricing_calculation_run_id: &'a str,
    pub tender_revision: u32,
    pub baseline_aggregate_run_id: &'a str,
    pub baseline_aggregate_manifest_sha256: &'a str,
    pub baseline_amount: &'a str,
    pub adjustments: Vec<PricingCalculationAdjustmentInput>,
    pub created_at: &'a str,
}

pub(crate) fn record_pricing_calculation(
    transaction: &Transaction<'_>,
    mut request: RecordPricingCalculation<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<PricingCalculationRun, TenderCommandError> {
    check()?;
    let baseline =
        load_estimate_aggregate_calculation(transaction, request.baseline_aggregate_run_id, check)?
            .filter(|run| run.approved_for_reliance)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if baseline.manifest_sha256 != request.baseline_aggregate_manifest_sha256
        || baseline.final_amount != request.baseline_amount
        || request.adjustments.len() > 64
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    request.adjustments.sort_by(|left, right| {
        (&left.adjustment_id, left.adjustment_version)
            .cmp(&(&right.adjustment_id, right.adjustment_version))
    });
    let mut seen_adjustments = std::collections::HashSet::new();
    let mut seen_runs = std::collections::HashSet::new();
    for adjustment in &request.adjustments {
        check()?;
        if !seen_adjustments.insert((
            adjustment.adjustment_id.clone(),
            adjustment.adjustment_version,
        )) || !seen_runs.insert(adjustment.calculation_run_id.clone())
            || adjustment.currency != baseline.currency
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let run = approved_calculation_run_for_estimate(
            transaction,
            &adjustment.calculation_run_id,
            check,
        )?
        .filter(|run| {
            run.status == ControlledBoqCalculationStatus::Completed
                && run.tender_revision == request.tender_revision
        })
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if run.manifest_sha256 != adjustment.calculation_manifest_sha256
            || run.final_amount.as_deref() != Some(adjustment.amount.as_str())
            || run.output_currency != adjustment.currency
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
    }
    let arithmetic_inputs = request
        .adjustments
        .iter()
        .map(|adjustment| (adjustment.direction, adjustment.amount.as_str()))
        .collect::<Vec<_>>();
    let final_amount = evaluate_pricing_amount(
        request.baseline_amount,
        &arithmetic_inputs,
        baseline.precision,
        baseline.rounding_mode,
    )
    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let manifest = PricingCalculationManifest {
        schema_version: 1,
        pricing_calculation_run_id: request.pricing_calculation_run_id.into(),
        tender_revision: request.tender_revision,
        baseline_aggregate_run_id: baseline.aggregate_run_id.clone(),
        baseline_aggregate_manifest_sha256: baseline.manifest_sha256.clone(),
        baseline_amount: baseline.final_amount.clone(),
        adjustments: request.adjustments,
        rule_id: baseline.rule_id.clone(),
        rule_version: baseline.rule_version,
        rule_approval_id: baseline.rule_approval_id.clone(),
        scenario_id: baseline.scenario_id.clone(),
        scenario_version: baseline.scenario_version,
        precision: baseline.precision,
        rounding_mode: baseline.rounding_mode,
        engine_version: CALCULATION_ENGINE_VERSION.into(),
        final_amount: final_amount.clone(),
        currency: baseline.currency.clone(),
        created_at: request.created_at.into(),
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    transaction
        .execute(
            "INSERT INTO pricing_calculation_runs (
               pricing_calculation_run_id, tender_revision, baseline_aggregate_run_id,
               baseline_aggregate_manifest_sha256, final_amount, currency,
               manifest_json, manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                manifest.pricing_calculation_run_id,
                manifest.tender_revision,
                manifest.baseline_aggregate_run_id,
                manifest.baseline_aggregate_manifest_sha256,
                manifest.final_amount,
                manifest.currency,
                manifest_json,
                manifest_sha256,
                manifest.created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(PricingCalculationRun {
        pricing_calculation_run_id: manifest.pricing_calculation_run_id,
        tender_revision: manifest.tender_revision,
        baseline_aggregate_run_id: manifest.baseline_aggregate_run_id,
        baseline_aggregate_manifest_sha256: manifest.baseline_aggregate_manifest_sha256,
        baseline_amount: manifest.baseline_amount,
        adjustments: manifest.adjustments,
        rule_id: manifest.rule_id,
        rule_version: manifest.rule_version,
        rule_approval_id: manifest.rule_approval_id,
        scenario_id: manifest.scenario_id,
        scenario_version: manifest.scenario_version,
        precision: manifest.precision,
        rounding_mode: manifest.rounding_mode,
        engine_version: manifest.engine_version,
        final_amount: manifest.final_amount,
        currency: manifest.currency,
        manifest_sha256,
        created_at: manifest.created_at,
    })
}

pub(crate) fn load_pricing_calculation(
    connection: &rusqlite::Connection,
    pricing_calculation_run_id: &str,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<PricingCalculationRun, TenderCommandError> {
    check()?;
    type Stored = (u32, String, String, String, String, String, String, String);
    let stored: Stored = connection
        .query_row(
            "SELECT tender_revision, baseline_aggregate_run_id,
                    baseline_aggregate_manifest_sha256, final_amount, currency,
                    manifest_json, manifest_sha256, created_at
             FROM pricing_calculation_runs
             WHERE pricing_calculation_run_id = ?1",
            [pricing_calculation_run_id],
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
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(stored.5.as_bytes()) != stored.6 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manifest: PricingCalculationManifest = parse_canonical(&stored.5)?;
    let baseline = load_estimate_aggregate_calculation(
        connection,
        &manifest.baseline_aggregate_run_id,
        check,
    )?
    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let rule_approval = load_rule_approval(connection, &manifest.rule_id, manifest.rule_version)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut canonical_adjustments = manifest.adjustments.clone();
    canonical_adjustments.sort_by(|left, right| {
        (&left.adjustment_id, left.adjustment_version)
            .cmp(&(&right.adjustment_id, right.adjustment_version))
    });
    let mut seen_adjustments = std::collections::HashSet::new();
    let mut seen_runs = std::collections::HashSet::new();
    for adjustment in &manifest.adjustments {
        check()?;
        if !seen_adjustments.insert((
            adjustment.adjustment_id.clone(),
            adjustment.adjustment_version,
        )) || !seen_runs.insert(adjustment.calculation_run_id.clone())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let run = approved_calculation_run_for_estimate(
            connection,
            &adjustment.calculation_run_id,
            check,
        )?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if run.manifest_sha256 != adjustment.calculation_manifest_sha256
            || run.final_amount.as_deref() != Some(adjustment.amount.as_str())
            || run.output_currency != adjustment.currency
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    let arithmetic_inputs = manifest
        .adjustments
        .iter()
        .map(|adjustment| (adjustment.direction, adjustment.amount.as_str()))
        .collect::<Vec<_>>();
    let expected = evaluate_pricing_amount(
        &manifest.baseline_amount,
        &arithmetic_inputs,
        manifest.precision,
        manifest.rounding_mode,
    )
    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if manifest.schema_version != 1
        || manifest.pricing_calculation_run_id != pricing_calculation_run_id
        || manifest.tender_revision != stored.0
        || manifest.baseline_aggregate_run_id != stored.1
        || manifest.baseline_aggregate_manifest_sha256 != stored.2
        || manifest.final_amount != stored.3
        || manifest.currency != stored.4
        || manifest.created_at != stored.7
        || manifest.engine_version != CALCULATION_ENGINE_VERSION
        || manifest.baseline_aggregate_manifest_sha256 != baseline.manifest_sha256
        || manifest.baseline_amount != baseline.final_amount
        || manifest.currency != baseline.currency
        || manifest.rule_id != baseline.rule_id
        || manifest.rule_version != baseline.rule_version
        || manifest.rule_approval_id != baseline.rule_approval_id
        || manifest.rule_approval_id != rule_approval.approval_id
        || manifest.scenario_id != baseline.scenario_id
        || manifest.scenario_version != baseline.scenario_version
        || manifest.precision != baseline.precision
        || manifest.rounding_mode != baseline.rounding_mode
        || manifest.adjustments.len() > 64
        || manifest.adjustments != canonical_adjustments
        || manifest.final_amount != expected
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(PricingCalculationRun {
        pricing_calculation_run_id: manifest.pricing_calculation_run_id,
        tender_revision: manifest.tender_revision,
        baseline_aggregate_run_id: manifest.baseline_aggregate_run_id,
        baseline_aggregate_manifest_sha256: manifest.baseline_aggregate_manifest_sha256,
        baseline_amount: manifest.baseline_amount,
        adjustments: manifest.adjustments,
        rule_id: manifest.rule_id,
        rule_version: manifest.rule_version,
        rule_approval_id: manifest.rule_approval_id,
        scenario_id: manifest.scenario_id,
        scenario_version: manifest.scenario_version,
        precision: manifest.precision,
        rounding_mode: manifest.rounding_mode,
        engine_version: manifest.engine_version,
        final_amount: manifest.final_amount,
        currency: manifest.currency,
        manifest_sha256: stored.6,
        created_at: manifest.created_at,
    })
}

fn estimate_aggregate_manifest_is_valid(
    connection: &rusqlite::Connection,
    manifest: &EstimateAggregateCalculationManifest,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<bool, TenderCommandError> {
    check()?;
    type StoredCore = (
        String,
        String,
        u32,
        String,
        u32,
        String,
        String,
        u32,
        u32,
        String,
        String,
        String,
    );
    let stored: Option<StoredCore> = connection
        .query_row(
            "SELECT author_run_id, comparison_total_calculation_run_id,
                    tender_revision, rule_id, rule_version, rule_approval_id,
                    scenario_id, scenario_version, precision, rounding_mode,
                    final_amount, currency
             FROM estimate_aggregate_calculation_runs WHERE aggregate_run_id = ?1",
            [&manifest.aggregate_run_id],
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
    let Some(stored) = stored else {
        return Ok(false);
    };
    if stored.0 != manifest.author_run_id
        || stored.1 != manifest.comparison_total_calculation_run_id
        || stored.2 != manifest.tender_revision
        || stored.3 != manifest.rule_id
        || stored.4 != manifest.rule_version
        || stored.5 != manifest.rule_approval_id
        || stored.6 != manifest.scenario_id
        || stored.7 != manifest.scenario_version
        || stored.8 != manifest.precision
        || stored.9 != manifest.rounding_mode.as_str()
        || stored.10 != manifest.final_amount
        || stored.11 != manifest.currency
        || manifest.engine_version != CALCULATION_ENGINE_VERSION
        || manifest.inputs.len() > 256
    {
        return Ok(false);
    }
    let comparison = approved_calculation_run_for_estimate(
        connection,
        &manifest.comparison_total_calculation_run_id,
        check,
    )?;
    let Some(comparison) = comparison else {
        return Ok(false);
    };
    if comparison.tender_revision != manifest.tender_revision
        || comparison.rule_id != manifest.rule_id
        || comparison.rule_version != manifest.rule_version
        || comparison.rule_approval_id != manifest.rule_approval_id
        || comparison.scenario_id != manifest.scenario_id
        || comparison.scenario_version != manifest.scenario_version
        || comparison.precision != manifest.precision
        || comparison.rounding_mode != manifest.rounding_mode
        || comparison.output_currency != manifest.currency
        || comparison.manifest_sha256 != manifest.comparison_total_manifest_sha256
        || comparison.final_amount.as_deref() != Some(manifest.comparison_total_amount.as_str())
    {
        return Ok(false);
    }
    let hashes: (String, String, String) = connection
        .query_row(
            "SELECT versions.manifest_sha256, approvals.manifest_sha256,
                    scenarios.manifest_sha256
             FROM calculation_rule_versions AS versions
             JOIN calculation_rule_approvals AS approvals
               ON approvals.rule_id = versions.rule_id
              AND approvals.rule_version = versions.version
             JOIN calculation_scenario_versions AS scenarios
               ON scenarios.scenario_id = ?4 AND scenarios.version = ?5
             WHERE versions.rule_id = ?1 AND versions.version = ?2
               AND approvals.approval_id = ?3",
            params![
                manifest.rule_id,
                manifest.rule_version,
                manifest.rule_approval_id,
                manifest.scenario_id,
                manifest.scenario_version,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(sql_error)?;
    if hashes.0 != manifest.rule_manifest_sha256
        || hashes.1 != manifest.rule_approval_manifest_sha256
        || hashes.2 != manifest.scenario_manifest_sha256
    {
        return Ok(false);
    }
    let mut seen_runs = std::collections::HashSet::new();
    for input in &manifest.inputs {
        check()?;
        if !seen_runs.insert(input.calculation_run_id.clone()) {
            return Ok(false);
        }
        let Some(run) =
            approved_calculation_run_for_estimate(connection, &input.calculation_run_id, check)?
        else {
            return Ok(false);
        };
        if run.tender_revision != manifest.tender_revision
            || run.manifest_sha256 != input.calculation_manifest_sha256
            || run.final_amount.as_deref() != Some(input.amount.as_str())
            || run.output_currency != input.currency
            || input.currency != manifest.currency
        {
            return Ok(false);
        }
    }
    Ok(
        evaluate_estimate_aggregate(&manifest.inputs, manifest.precision, manifest.rounding_mode)?
            == manifest.final_amount,
    )
}

fn estimate_aggregate_approval_is_valid(
    connection: &rusqlite::Connection,
    aggregate: &EstimateAggregateCalculationManifest,
    aggregate_manifest_sha256: &str,
) -> Result<bool, TenderCommandError> {
    type StoredApproval = (
        String,
        String,
        u32,
        String,
        String,
        String,
        i64,
        String,
        String,
        String,
    );
    let stored: Option<StoredApproval> = connection
        .query_row(
            "SELECT approval_id, basis_id, basis_version, basis_manifest_sha256,
                    rationale, manifest_json, audit_sequence, manifest_sha256,
                    approved_by, created_at
             FROM estimate_aggregate_calculation_approvals
             WHERE aggregate_run_id = ?1",
            [&aggregate.aggregate_run_id],
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
    let Some(stored) = stored else {
        return Ok(false);
    };
    let manifest: EstimateAggregateCalculationApprovalManifest = parse_canonical(&stored.5)?;
    let basis_manifest: Option<String> = connection
        .query_row(
            "SELECT manifest_sha256 FROM basis_of_estimate_versions
             WHERE basis_id = ?1 AND version = ?2",
            params![stored.1, stored.2],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let expected_change = json!({
        "aggregate_manifest_sha256": aggregate_manifest_sha256,
        "aggregate_run_id": aggregate.aggregate_run_id,
        "approval_id": stored.0,
        "basis_id": stored.1,
        "basis_manifest_sha256": stored.3,
        "basis_version": stored.2.to_string(),
        "manifest_sha256": stored.7,
    });
    Ok(manifest.schema_version == 1
        && manifest.approval_id == stored.0
        && manifest.aggregate_run_id == aggregate.aggregate_run_id
        && manifest.aggregate_manifest_sha256 == aggregate_manifest_sha256
        && manifest.basis_id == stored.1
        && manifest.basis_version == stored.2
        && manifest.basis_manifest_sha256 == stored.3
        && manifest.rationale == stored.4
        && manifest.approved_by == "engineer_user"
        && manifest.acting_role == "engineer_in_the_loop"
        && manifest.created_at == stored.9
        && stored.8 == "engineer_user"
        && sha256_hex(stored.5.as_bytes()) == stored.7
        && basis_manifest.as_deref() == Some(stored.3.as_str())
        && calculation_audit_is_exact(
            connection,
            stored.6,
            "estimate_aggregate_calculation_approved",
            &stored.9,
            &expected_change,
        )?)
}

pub(crate) struct ApproveEstimateAggregateCalculation<'a> {
    pub aggregate_run_id: &'a str,
    pub aggregate_manifest_sha256: &'a str,
    pub basis_id: &'a str,
    pub basis_version: u32,
    pub basis_manifest_sha256: &'a str,
    pub rationale: &'a str,
    pub tender_id: &'a str,
    pub tender_revision: u32,
    pub created_at: &'a str,
}

pub(crate) fn approve_estimate_aggregate_calculation(
    transaction: &Transaction<'_>,
    request: ApproveEstimateAggregateCalculation<'_>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(), TenderCommandError> {
    let _aggregate =
        load_estimate_aggregate_calculation(transaction, request.aggregate_run_id, check)?
            .filter(|run| {
                !run.approved_for_reliance
                    && run.manifest_sha256 == request.aggregate_manifest_sha256
                    && run.final_amount.len() <= MAX_DECIMAL_BYTES
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let approval_id = random_identifier(transaction)?;
    let manifest = EstimateAggregateCalculationApprovalManifest {
        schema_version: 1,
        approval_id: approval_id.clone(),
        aggregate_run_id: request.aggregate_run_id.into(),
        aggregate_manifest_sha256: request.aggregate_manifest_sha256.into(),
        basis_id: request.basis_id.into(),
        basis_version: request.basis_version,
        basis_manifest_sha256: request.basis_manifest_sha256.into(),
        rationale: request.rationale.into(),
        approved_by: "engineer_user".into(),
        acting_role: "engineer_in_the_loop".into(),
        created_at: request.created_at.into(),
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        request.tender_id,
        "estimate_aggregate_calculation_approved",
        request.tender_revision,
        json!({
            "aggregate_manifest_sha256": request.aggregate_manifest_sha256,
            "aggregate_run_id": request.aggregate_run_id,
            "approval_id": approval_id,
            "basis_id": request.basis_id,
            "basis_manifest_sha256": request.basis_manifest_sha256,
            "basis_version": request.basis_version.to_string(),
            "manifest_sha256": manifest_sha256,
        }),
        request.created_at,
    )?;
    transaction
        .execute(
            "INSERT INTO estimate_aggregate_calculation_approvals (
               approval_id, aggregate_run_id, aggregate_manifest_sha256,
               basis_id, basis_version, basis_manifest_sha256, rationale,
               approved_by, acting_role, audit_sequence, manifest_json,
               manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'engineer_user',
                       'engineer_in_the_loop', ?8, ?9, ?10, ?11)",
            params![
                approval_id,
                request.aggregate_run_id,
                request.aggregate_manifest_sha256,
                request.basis_id,
                request.basis_version,
                request.basis_manifest_sha256,
                request.rationale,
                audit_sequence,
                manifest_json,
                manifest_sha256,
                request.created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn cost_estimator_run_is_valid(
    connection: &rusqlite::Connection,
    manifest: &CalculationRunManifest,
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
    let run_basis: Option<RunBasis> = connection
        .query_row(
            "SELECT runs.status, runs.profile_id, runs.profile_version, runs.task_id,
                    runs.permission_grant_json, runs.started_at,
                    results.verification_status, results.payload_json,
                    results.data_scopes_json, results.data_classification,
                    results.created_at
             FROM agent_runs AS runs
             JOIN proposed_agent_results AS results ON results.run_id = runs.run_id
             WHERE runs.run_id = ?1",
            [&manifest.cost_estimator_run_id],
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
        payload_json,
        result_scopes_json,
        result_classification,
        result_created_at,
    )) = run_basis
    else {
        return Ok(false);
    };
    let profile = load_profile(connection, (profile_id.clone(), profile_version))?;
    let task = load_task(connection, &task_id)?;
    let permission_grant: PermissionGrant = parse_canonical(&permission_grant_json)?;
    let candidate: CostEstimatorCalculationCandidate = serde_json::from_str(&payload_json)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let result_scopes: Vec<String> = parse_canonical(&result_scopes_json)?;
    let result_classification: DataClassification =
        serde_json::from_str(&format!("\"{result_classification}\""))
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let CostEstimatorTarget {
        tender_id,
        tender_revision,
        scenario_id,
        scenario_version,
        plan_id,
        plan_version,
        description,
    } = exact_cost_estimator_target(&task)?;
    let scenario = load_calculation_scenario(connection, &scenario_id, scenario_version)?;
    let quantity_evidence = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "calculation_quantity_evidence")
        .map(|input| AgentTaskInputReference {
            kind: "source_evidence".into(),
            reference: input.reference.clone(),
            version: input.version,
        })
        .collect::<Vec<_>>();
    let unit_rate_evidence = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "calculation_unit_rate_evidence")
        .map(|input| AgentTaskInputReference {
            kind: "source_evidence".into(),
            reference: input.reference.clone(),
            version: input.version,
        })
        .collect::<Vec<_>>();
    validate_calculation_evidence_basket(&quantity_evidence)?;
    validate_calculation_evidence_basket(&unit_rate_evidence)?;
    let tender_name: String = connection
        .query_row(
            "SELECT name FROM tender_revisions
             WHERE tender_id = ?1 AND revision = ?2",
            params![tender_id, tender_revision],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let classification = *profile
        .permissions
        .data_classifications
        .iter()
        .max()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let expected_payload = json!({
        "calculation_scenario": scenario,
        "data_classification": classification,
        "data_scope": profile.permissions.data_scopes.join("+"),
        "description": description,
        "quantity_evidence": calculation_source_evidence_view(
            connection,
            &quantity_evidence,
            check,
        )?,
        "rules": {
            "canonical_arithmetic_allowed": false,
            "host_calculation_engine_is_sole_authority": true,
            "inputs_must_cite_supplied_evidence": true
        },
        "tender": {
            "name": tender_name,
            "revision": tender_revision,
            "tender_id": tender_id
        },
        "unit_rate_evidence": calculation_source_evidence_view(
            connection,
            &unit_rate_evidence,
            check,
        )?,
    });
    let expected_view_sha256 = sha256_hex(canonical_json(&expected_payload)?.as_bytes());
    let expected_task = cost_estimator_calculation_task(CostEstimatorTaskBasis {
        task_id: task.task_id.clone(),
        tender_id: &tender_id,
        tender_revision,
        plan_id: &plan_id,
        plan_version,
        description: &description,
        quantity_evidence: &quantity_evidence,
        unit_rate_evidence: &unit_rate_evidence,
        deadline: task.deadline.clone(),
        profile: &profile,
        scenario: &scenario,
    });
    let plan_profile_approved: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM work_plan_versions AS plans
               JOIN work_plan_approvals AS approvals
                 ON approvals.plan_id = plans.plan_id
                AND approvals.plan_version = plans.version
                AND approvals.decision = 'approve'
               WHERE plans.plan_id = ?1 AND plans.version = ?2
                 AND EXISTS (
                   SELECT 1 FROM json_each(plans.profiles_json) AS bindings
                   WHERE json_extract(bindings.value, '$.profile.profile_id') = ?3
                     AND json_extract(bindings.value, '$.profile.version') = ?4
                     AND EXISTS (
                       SELECT 1 FROM json_each(
                         json_extract(bindings.value, '$.profile.capabilities')
                       ) AS capabilities
                       WHERE capabilities.value = ?5
                     )
                 )
             )",
            params![
                plan_id,
                plan_version,
                profile_id,
                profile_version,
                COST_ESTIMATION_CAPABILITY,
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let started: (u32, Option<String>) = connection
        .query_row(
            "SELECT COUNT(*), MIN(payload_json) FROM audit_events
             WHERE event_type = 'cost_estimator_calculation_started'
               AND json_extract(payload_json, '$.change.run_id') = ?1",
            [&manifest.cost_estimator_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    let expected_started_change = json!({
        "profile_id": profile_id,
        "run_id": manifest.cost_estimator_run_id,
        "scenario_id": manifest.scenario_id,
        "scenario_version": manifest.scenario_version.to_string(),
        "task_id": task_id,
    });
    let started_payload: Option<serde_json::Value> = started
        .1
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let data_view = permission_grant.data_views.first();
    Ok(status == "completed"
        && result_status == "proposed"
        && result_created_at == manifest.created_at
        && manifest.tender_revision == tender_revision
        && result_scopes == permission_grant.data_scopes
        && result_classification == classification
        && candidate.quantity == manifest.quantity
        && candidate.unit_rate == manifest.unit_rate
        && candidate_evidence_is_within_task(
            &task,
            &candidate.quantity,
            "calculation_quantity_evidence",
        )
        && candidate_evidence_is_within_task(
            &task,
            &candidate.unit_rate,
            "calculation_unit_rate_evidence",
        )
        && task == expected_task
        && plan_profile_approved
        && profile
            .capabilities
            .iter()
            .any(|capability| capability == COST_ESTIMATION_CAPABILITY)
        && permission_grant.profile_id == profile_id
        && permission_grant.profile_version == profile_version
        && permission_grant.task_id == task_id
        && permission_grant.work_plan_version == plan_version
        && permission_grant.purpose == task.objective
        && permission_grant.data_scopes == profile.permissions.data_scopes
        && permission_grant.data_classifications == profile.permissions.data_classifications
        && permission_grant.allowed_actions == profile.permissions.allowed_actions
        && permission_grant.typed_tools.is_empty()
        && !permission_grant.network_allowed
        && permission_grant.workspace_write_allowed
        && permission_grant.thread_exposure == ThreadExposureSet::from_grant(&permission_grant)
        && permission_grant.workspace.workspace_id == manifest.cost_estimator_run_id
        && permission_grant.workspace.read_only_inputs == "inputs"
        && permission_grant.workspace.working_area == "working"
        && permission_grant.workspace.staged_outputs == "outputs"
        && permission_grant.access_ceiling.exact_inputs == task.exact_inputs
        && permission_grant.access_ceiling.data_scopes == profile.permissions.data_scopes
        && permission_grant.access_ceiling.data_classifications
            == profile.permissions.data_classifications
        && permission_grant.access_ceiling.allowed_actions == profile.permissions.allowed_actions
        && permission_grant.access_ceiling.allowed_tools.is_empty()
        && permission_grant.resource_budget == task.resource_budget
        && permission_grant.issued_at == run_started_at
        && permission_grant.expires_at == task.deadline
        && permission_grant.data_views.len() == 1
        && data_view.is_some_and(|view| {
            view.exact_inputs == task.exact_inputs
                && view.view_id == format!("production-task-{task_id}")
                && view.schema_version == 1
                && view.relative_path == "inputs/tender-metadata-v1.json"
                && view.sha256 == expected_view_sha256
                && view.data_scope == profile.permissions.data_scopes.join("+")
                && view.data_classification == classification
        })
        && started.0 == 1
        && started_payload
            .as_ref()
            .and_then(|payload| payload.get("change"))
            == Some(&expected_started_change))
}

fn calculation_rule_review_run_is_valid(
    connection: &rusqlite::Connection,
    review: &CalculationRuleReview,
    rule: &CalculationRuleVersion,
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
    let basis: Option<RunBasis> = connection
        .query_row(
            "SELECT runs.status, runs.profile_id, runs.profile_version, runs.task_id,
                    runs.permission_grant_json, runs.started_at,
                    results.verification_status, results.payload_json,
                    results.data_scopes_json, results.data_classification,
                    results.created_at
             FROM agent_runs AS runs
             JOIN proposed_agent_results AS results ON results.run_id = runs.run_id
             WHERE runs.run_id = ?1",
            [&review.reviewer_run_id],
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
        payload_json,
        result_scopes_json,
        result_classification,
        result_created_at,
    )) = basis
    else {
        return Ok(false);
    };
    let profile = load_profile(connection, (profile_id.clone(), profile_version))?;
    let task = load_task(connection, &task_id)?;
    let permission_grant: PermissionGrant = parse_canonical(&permission_grant_json)?;
    let candidate: CalculationRuleReviewCandidate = serde_json::from_str(&payload_json)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let result_scopes: Vec<String> = parse_canonical(&result_scopes_json)?;
    let result_classification: DataClassification =
        serde_json::from_str(&format!("\"{result_classification}\""))
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let (rule_id, rule_version, plan_id, plan_version) = exact_calculation_rule_target(&task)?;
    let tender_inputs = task
        .exact_inputs
        .iter()
        .filter(|input| input.kind == "tender_revision")
        .collect::<Vec<_>>();
    if tender_inputs.len() != 1 || rule_id != rule.rule_id || rule_version != rule.version {
        return Ok(false);
    }
    let tender_name: String = connection
        .query_row(
            "SELECT name FROM tender_revisions
             WHERE tender_id = ?1 AND revision = ?2",
            params![tender_inputs[0].reference, tender_inputs[0].version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let mut rule_basis = rule.clone();
    rule_basis.review = None;
    rule_basis.approval = None;
    rule_basis.current = true;
    rule_basis.active = false;
    let classification = *profile
        .permissions
        .data_classifications
        .iter()
        .max()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let expected_payload = json!({
        "calculation_rule": rule_basis,
        "data_classification": classification,
        "data_scope": profile.permissions.data_scopes.join("+"),
        "review_rules": {
            "activation_allowed": false,
            "arithmetic_must_be_replayed": true,
            "exact_target_is_immutable": true
        },
        "tender": {
            "name": tender_name,
            "revision": tender_inputs[0].version,
            "tender_id": tender_inputs[0].reference
        },
    });
    let expected_view_sha256 = sha256_hex(canonical_json(&expected_payload)?.as_bytes());
    let expected_task = calculation_rule_review_task(CalculationRuleReviewTaskRequest {
        task_id: task.task_id.clone(),
        tender_id: &tender_inputs[0].reference,
        tender_revision: tender_inputs[0].version,
        plan_id: &plan_id,
        plan_version,
        rule: &rule_basis,
        deadline: task.deadline.clone(),
        profile: &profile,
    });
    let plan_profile_approved: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM work_plan_versions AS plans
               JOIN work_plan_approvals AS approvals
                 ON approvals.plan_id = plans.plan_id
                AND approvals.plan_version = plans.version
                AND approvals.decision = 'approve'
               WHERE plans.plan_id = ?1 AND plans.version = ?2
                 AND EXISTS (
                   SELECT 1 FROM json_each(plans.profiles_json) AS bindings
                   WHERE json_extract(bindings.value, '$.profile.profile_id') = ?3
                     AND json_extract(bindings.value, '$.profile.version') = ?4
                     AND EXISTS (
                       SELECT 1 FROM json_each(
                         json_extract(bindings.value, '$.profile.capabilities')
                       ) AS capabilities
                       WHERE capabilities.value = ?5
                     )
                 )
             )",
            params![
                plan_id,
                plan_version,
                profile_id,
                profile_version,
                CALCULATION_RULE_REVIEW_CAPABILITY,
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let started: (u32, Option<String>) = connection
        .query_row(
            "SELECT COUNT(*), MIN(payload_json) FROM audit_events
             WHERE event_type = 'calculation_rule_review_started'
               AND json_extract(payload_json, '$.change.run_id') = ?1",
            [&review.reviewer_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(sql_error)?;
    let expected_started_change = json!({
        "reviewer_profile_id": profile_id,
        "rule_id": rule.rule_id,
        "rule_version": rule.version.to_string(),
        "run_id": review.reviewer_run_id,
        "task_id": task_id,
    });
    let started_payload: Option<serde_json::Value> = started
        .1
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut codes = std::collections::HashSet::new();
    let findings_valid = candidate.findings.len() <= 32
        && candidate.findings.iter().all(|finding| {
            !finding.code.is_empty()
                && finding.code.len() <= 100
                && finding
                    .code
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
                && codes.insert(finding.code.clone())
                && !finding.summary.trim().is_empty()
                && finding.summary.len() <= 2_000
        });
    let outcome_valid = match candidate.outcome {
        CalculationRuleReviewOutcome::Passed => candidate.findings.is_empty(),
        CalculationRuleReviewOutcome::Failed => !candidate.findings.is_empty(),
    };
    let data_view = permission_grant.data_views.first();
    check()?;
    Ok(status == "completed"
        && result_status == "proposed"
        && result_created_at == review.created_at
        && candidate.outcome == review.outcome
        && candidate.findings == review.findings
        && findings_valid
        && outcome_valid
        && task == expected_task
        && plan_profile_approved
        && profile
            .capabilities
            .iter()
            .any(|capability| capability == CALCULATION_RULE_REVIEW_CAPABILITY)
        && permission_grant.profile_id == profile_id
        && permission_grant.profile_version == profile_version
        && permission_grant.task_id == task_id
        && permission_grant.work_plan_version == plan_version
        && permission_grant.purpose == task.objective
        && permission_grant.data_scopes == profile.permissions.data_scopes
        && permission_grant.data_classifications == profile.permissions.data_classifications
        && permission_grant.allowed_actions == profile.permissions.allowed_actions
        && permission_grant.typed_tools.is_empty()
        && !permission_grant.network_allowed
        && permission_grant.workspace_write_allowed
        && permission_grant.thread_exposure == ThreadExposureSet::from_grant(&permission_grant)
        && permission_grant.workspace.workspace_id == review.reviewer_run_id
        && permission_grant.workspace.read_only_inputs == "inputs"
        && permission_grant.workspace.working_area == "working"
        && permission_grant.workspace.staged_outputs == "outputs"
        && permission_grant.access_ceiling.exact_inputs == task.exact_inputs
        && permission_grant.access_ceiling.data_scopes == profile.permissions.data_scopes
        && permission_grant.access_ceiling.data_classifications
            == profile.permissions.data_classifications
        && permission_grant.access_ceiling.allowed_actions == profile.permissions.allowed_actions
        && permission_grant.access_ceiling.allowed_tools.is_empty()
        && permission_grant.resource_budget == task.resource_budget
        && permission_grant.issued_at == run_started_at
        && permission_grant.expires_at == task.deadline
        && permission_grant.data_views.len() == 1
        && data_view.is_some_and(|view| {
            view.exact_inputs == task.exact_inputs
                && view.view_id == format!("production-task-{task_id}")
                && view.schema_version == 1
                && view.relative_path == "inputs/tender-metadata-v1.json"
                && view.sha256 == expected_view_sha256
                && view.data_scope == profile.permissions.data_scopes.join("+")
                && view.data_classification == classification
        })
        && result_scopes == permission_grant.data_scopes
        && result_classification == classification
        && started.0 == 1
        && started_payload
            .as_ref()
            .and_then(|payload| payload.get("change"))
            == Some(&expected_started_change))
}

pub(crate) fn calculation_rule_review_target_is_open(
    transaction: &Transaction<'_>,
    task: &TenderTaskView,
) -> Result<bool, TenderCommandError> {
    let (rule_id, version, plan_id, plan_version) = exact_calculation_rule_target(task)?;
    transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM calculation_rule_heads AS heads
               JOIN production_activations AS activations
                 ON activations.plan_id = ?3
                AND activations.plan_version = ?4
                AND activations.status = 'active'
               JOIN work_plan_versions AS plans
                 ON plans.plan_id = activations.plan_id
                AND plans.version = activations.plan_version
               JOIN agent_profile_heads AS profile_heads
                 ON profile_heads.profile_id = ?5
                AND profile_heads.current_version = ?6
                AND profile_heads.status = 'active'
               WHERE heads.rule_id = ?1 AND heads.current_version = ?2
                 AND EXISTS (
                   SELECT 1 FROM json_each(plans.profiles_json) AS bindings
                   WHERE json_extract(bindings.value, '$.profile.profile_id') = ?5
                     AND json_extract(bindings.value, '$.profile.version') = ?6
                     AND EXISTS (
                       SELECT 1 FROM json_each(
                         json_extract(bindings.value, '$.profile.capabilities')
                       ) AS capabilities
                       WHERE capabilities.value = ?7
                     )
                 )
                 AND NOT EXISTS (
                   SELECT 1 FROM calculation_rule_reviews
                   WHERE rule_id = heads.rule_id AND rule_version = heads.current_version
                 )
                 AND NOT EXISTS (
                   SELECT 1 FROM calculation_rule_approvals
                   WHERE rule_id = heads.rule_id AND rule_version = heads.current_version
                 )
             )",
            params![
                rule_id,
                version,
                plan_id,
                plan_version,
                task.profile_id,
                task.profile_version,
                CALCULATION_RULE_REVIEW_CAPABILITY,
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

pub(crate) fn cost_estimator_calculation_target_is_open(
    transaction: &Transaction<'_>,
    task: &TenderTaskView,
    run_id: &str,
) -> Result<bool, TenderCommandError> {
    let target = exact_cost_estimator_target(task)?;
    transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM calculation_scenario_versions AS scenarios
               JOIN tender
                 ON tender.singleton = 1
                AND tender.tender_id = ?9
                AND tender.current_revision = ?10
               JOIN production_activations AS activations
                 ON activations.plan_id = ?3
                AND activations.plan_version = ?4
                AND activations.status = 'active'
               JOIN work_plan_versions AS plans
                 ON plans.plan_id = activations.plan_id
                AND plans.version = activations.plan_version
               JOIN agent_profile_heads AS profile_heads
                 ON profile_heads.profile_id = ?5
                AND profile_heads.current_version = ?6
                AND profile_heads.status = 'active'
               WHERE scenarios.scenario_id = ?1 AND scenarios.version = ?2
                 AND EXISTS (
                   SELECT 1 FROM json_each(plans.profiles_json) AS bindings
                   WHERE json_extract(bindings.value, '$.profile.profile_id') = ?5
                     AND json_extract(bindings.value, '$.profile.version') = ?6
                     AND EXISTS (
                       SELECT 1 FROM json_each(
                         json_extract(bindings.value, '$.profile.capabilities')
                       ) AS capabilities
                       WHERE capabilities.value = ?7
                     )
                 )
                 AND EXISTS (
                   SELECT 1 FROM calculation_rule_heads AS heads
                   JOIN calculation_rule_approvals AS approvals
                     ON approvals.rule_id = heads.rule_id
                    AND approvals.rule_version = heads.current_version
                 )
                 AND NOT EXISTS (
                   SELECT 1 FROM calculation_runs WHERE cost_estimator_run_id = ?8
                 )
             )",
            params![
                target.scenario_id,
                target.scenario_version,
                target.plan_id,
                target.plan_version,
                task.profile_id,
                task.profile_version,
                COST_ESTIMATION_CAPABILITY,
                run_id,
                target.tender_id,
                target.tender_revision,
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn publish_cost_estimator_calculation(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    cost_estimator_run_id: &str,
    profile: &AgentProfileVersionView,
    task: &TenderTaskView,
    candidate: &CostEstimatorCalculationCandidate,
    created_at: &str,
) -> Result<ControlledBoqCalculationRun, TenderCommandError> {
    if !profile
        .capabilities
        .iter()
        .any(|capability| capability == COST_ESTIMATION_CAPABILITY)
        || profile.profile_id != task.profile_id
        || profile.version != task.profile_version
        || !cost_estimator_calculation_target_is_open(transaction, task, cost_estimator_run_id)?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let target = exact_cost_estimator_target(task)?;
    if target.tender_id != tender_id.as_str() || target.tender_revision != tender_revision {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let scenario =
        load_calculation_scenario(transaction, &target.scenario_id, target.scenario_version)?;
    if !input_evidence_is_authoritative(transaction, &candidate.quantity, &mut || Ok(()))?
        || !input_evidence_is_authoritative(transaction, &candidate.unit_rate, &mut || Ok(()))?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let run_count: u32 = transaction
        .query_row("SELECT COUNT(*) FROM calculation_runs", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)?;
    if run_count >= MAX_CALCULATION_RUNS {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let active_rule: (String, u32, String, String, String) = transaction
        .query_row(
            "SELECT versions.rule_id, versions.version, versions.manifest_sha256,
                    approvals.approval_id, approvals.manifest_sha256
             FROM calculation_rule_versions AS versions
             JOIN calculation_rule_heads AS heads
               ON heads.rule_id = versions.rule_id
              AND heads.current_version = versions.version
             JOIN calculation_rule_approvals AS approvals
               ON approvals.rule_id = versions.rule_id
              AND approvals.rule_version = versions.version",
            [],
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
        .map_err(sql_error)?;
    let (status, diagnostic_code, exact) = evaluate_candidate(candidate, &scenario);
    let calculation_run_id = random_identifier(transaction)?;
    let manifest = CalculationRunManifest {
        schema_version: 1,
        calculation_run_id: calculation_run_id.clone(),
        cost_estimator_run_id: cost_estimator_run_id.into(),
        tender_revision: target.tender_revision,
        rule_id: active_rule.0.clone(),
        rule_version: active_rule.1,
        rule_approval_id: active_rule.3.clone(),
        rule_manifest_sha256: active_rule.2,
        rule_approval_manifest_sha256: active_rule.4,
        description: target.description.clone(),
        scenario_id: scenario.scenario_id.clone(),
        scenario_version: scenario.version,
        scenario_name: scenario.name.clone(),
        scenario_manifest_sha256: scenario.manifest_sha256.clone(),
        exchange_rate_id: scenario.exchange_rate_id.clone(),
        exchange_rate_version: scenario.exchange_rate_version,
        rounding_policy_id: scenario.rounding_policy_id.clone(),
        rounding_policy_version: scenario.rounding_policy_version,
        quantity: candidate.quantity.clone(),
        quantity_unit: scenario.quantity_unit.clone(),
        unit_rate: candidate.unit_rate.clone(),
        rate_basis_unit: scenario.rate_basis_unit.clone(),
        rate_currency: scenario.rate_currency.clone(),
        exchange_rate: scenario.exchange_rate.clone(),
        exchange_rate_effective_date: scenario.exchange_rate_effective_date.clone(),
        pricing_date: scenario.pricing_date.clone(),
        exchange_rate_type: scenario.exchange_rate_type,
        output_currency: scenario.output_currency.clone(),
        precision: scenario.precision,
        rounding_mode: scenario.rounding_mode,
        engine_version: CALCULATION_ENGINE_VERSION.into(),
        normalized_quantity: exact
            .as_ref()
            .map(|value| value.normalized_quantity.clone()),
        unrounded_source_amount: exact
            .as_ref()
            .map(|value| value.unrounded_source_amount.clone()),
        unrounded_output_amount: exact
            .as_ref()
            .map(|value| value.unrounded_output_amount.clone()),
        final_amount: exact.as_ref().map(|value| value.final_amount.clone()),
        status,
        diagnostic_code,
        created_at: created_at.into(),
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        "controlled_boq_calculation_recorded",
        tender_revision,
        json!({
            "calculation_run_id": calculation_run_id,
            "cost_estimator_run_id": cost_estimator_run_id,
            "manifest_sha256": manifest_sha256,
            "rule_approval_id": active_rule.3,
            "rule_id": active_rule.0,
            "rule_version": active_rule.1.to_string(),
            "scenario_id": scenario.scenario_id,
            "scenario_version": scenario.version.to_string(),
            "status": status.as_str(),
        }),
        created_at,
    )?;
    transaction
        .execute(
            "INSERT INTO calculation_runs (
               calculation_run_id, cost_estimator_run_id, rule_id, rule_version,
               rule_approval_id, scenario_id, scenario_version, status,
               manifest_json, manifest_sha256, audit_sequence, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                calculation_run_id,
                cost_estimator_run_id,
                active_rule.0,
                active_rule.1,
                active_rule.3,
                scenario.scenario_id,
                scenario.version,
                status.as_str(),
                manifest_json,
                manifest_sha256,
                audit_sequence,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    run_from_manifest(manifest, manifest_sha256)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn publish_calculation_rule_review(
    transaction: &Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    reviewer_run_id: &str,
    reviewer_profile: &AgentProfileVersionView,
    task: &TenderTaskView,
    candidate: &CalculationRuleReviewCandidate,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    if !reviewer_profile
        .capabilities
        .iter()
        .any(|capability| capability == CALCULATION_RULE_REVIEW_CAPABILITY)
        || reviewer_profile.profile_id != task.profile_id
        || reviewer_profile.version != task.profile_version
        || !calculation_rule_review_target_is_open(transaction, task)?
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let (rule_id, rule_version, _, _) = exact_calculation_rule_target(task)?;
    let rule_manifest_sha256: String = transaction
        .query_row(
            "SELECT manifest_sha256 FROM calculation_rule_versions
             WHERE rule_id = ?1 AND version = ?2",
            params![rule_id, rule_version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let review_id = random_identifier(transaction)?;
    let review_manifest = CalculationRuleReviewManifest {
        schema_version: 1,
        review_id: review_id.clone(),
        rule_id: rule_id.clone(),
        rule_version,
        rule_manifest_sha256: rule_manifest_sha256.clone(),
        reviewer_run_id: reviewer_run_id.into(),
        reviewer_profile_id: reviewer_profile.profile_id.clone(),
        reviewer_profile_version: reviewer_profile.version,
        outcome: candidate.outcome,
        findings: candidate.findings.clone(),
        created_at: created_at.into(),
    };
    let manifest_json = canonical_json(&review_manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let audit_sequence = append_audit_event_with_sequence(
        transaction,
        tender_id.as_str(),
        "calculation_rule_review_completed",
        tender_revision,
        json!({
            "manifest_sha256": manifest_sha256,
            "outcome": candidate.outcome.as_str(),
            "review_id": review_id,
            "reviewer_profile_id": reviewer_profile.profile_id,
            "reviewer_profile_version": reviewer_profile.version.to_string(),
            "reviewer_run_id": reviewer_run_id,
            "rule_id": rule_id,
            "rule_manifest_sha256": rule_manifest_sha256,
            "rule_version": rule_version.to_string(),
        }),
        created_at,
    )?;
    transaction
        .execute(
            "INSERT INTO calculation_rule_reviews (
               review_id, rule_id, rule_version, rule_manifest_sha256,
               reviewer_run_id, reviewer_profile_id, reviewer_profile_version,
               outcome, findings_json, audit_sequence, manifest_json,
               manifest_sha256, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                review_id,
                rule_id,
                rule_version,
                rule_manifest_sha256,
                reviewer_run_id,
                reviewer_profile.profile_id,
                reviewer_profile.version,
                candidate.outcome.as_str(),
                canonical_json(&candidate.findings)?,
                audit_sequence,
                manifest_json,
                manifest_sha256,
                created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(())
}

fn current_tender_revision(transaction: &Transaction<'_>) -> Result<u32, TenderCommandError> {
    transaction
        .query_row(
            "SELECT current_revision FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_boq_engine_converts_units_currency_and_rounds_explicitly() {
        let result = evaluate_boq_line(
            "1250",
            "mm",
            "2.40",
            "m",
            "USD",
            "50",
            "EGP",
            2,
            CalculationRoundingMode::MidpointAwayFromZero,
        )
        .expect("valid exact BOQ inputs");

        assert_eq!(result.normalized_quantity, "1.25");
        assert_eq!(result.unrounded_source_amount, "3");
        assert_eq!(result.unrounded_output_amount, "150");
        assert_eq!(result.final_amount, "150.00");
    }

    #[test]
    fn estimate_aggregate_uses_the_governed_midpoint_rounding_policy_once() {
        let inputs = vec![EstimateAggregateCalculationInput {
            build_up_id: "b".repeat(32),
            cbs_component_id: "c".repeat(32),
            calculation_run_id: "r".repeat(32),
            calculation_manifest_sha256: "a".repeat(64),
            amount: "1.005".into(),
            currency: "USD".into(),
        }];

        assert_eq!(
            evaluate_estimate_aggregate(&inputs, 2, CalculationRoundingMode::MidpointAwayFromZero,)
                .expect("controlled aggregate"),
            "1.01"
        );
        assert_eq!(
            evaluate_estimate_aggregate(&inputs, 2, CalculationRoundingMode::MidpointNearestEven,)
                .expect("controlled aggregate"),
            "1.00"
        );
    }

    #[test]
    fn calculation_command_deserialization_is_bounded() {
        let reference = json!({
            "kind": "source_evidence",
            "reference": "artifact#1",
            "version": 1,
        });
        let too_many = vec![reference.clone(); 33];
        assert!(
            serde_json::from_value::<RunCostEstimatorCalculationCommand>(json!({
                "tender_id": "t".repeat(32),
                "scenario_id": "s".repeat(32),
                "scenario_version": 1,
                "description": "bounded",
                "quantity_evidence": too_many,
                "unit_rate_evidence": [reference.clone()],
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<RunCostEstimatorCalculationCommand>(json!({
                "tender_id": "t".repeat(32),
                "scenario_id": "s".repeat(32),
                "scenario_version": 1,
                "description": "bounded",
                "quantity_evidence": [{
                    "kind": "source_evidence",
                    "reference": "x".repeat(401),
                    "version": 1,
                }],
                "unit_rate_evidence": [reference],
            }))
            .is_err()
        );
        assert!(serde_json::from_value::<CalculationDecimalInput>(json!({
            "state": "provided",
            "value": "9".repeat(MAX_DECIMAL_BYTES + 1),
            "evidence": [],
        }))
        .is_err());
    }
}
