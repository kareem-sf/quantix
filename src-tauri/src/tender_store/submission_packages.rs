use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Component, Path},
};

use garde::Validate;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use unicase::UniCase;
use unicode_normalization::UnicodeNormalization;

use super::tender_records::inspect_tender_record_version_in_connection;

use super::{
    append_audit_event_with_sequence, content_store_error,
    exact_approved_coordinated_baseline_is_current_in_connection, lock_mutex_with_check,
    random_identifier, sha256_hex, sql_error, sqlite_timestamp, BidPackageOperationBudget,
    CoordinatedBidBaselineBinding, CoordinatedBidBaselineBindingKind,
    CoordinatedBidBaselineCategory, GenerationRequirement, GenerationRequirementAvailability,
    GenerationRequirementKind, PackageProductionGeneration, QuantixHost, SubmissionArtifactVersion,
    TenderCommandError, TenderErrorCode, TenderId, TenderRecordKind, TenderStore,
    WorkPlanProfileBinding, WorkPlanProposalInspection, WorkPlanWorkstream,
};

const MAX_PACKAGE_VERSIONS: u32 = 32;
const MAX_PACKAGE_ITEMS: usize = 4_096;
const MAX_MANIFEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_ITEM_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct AssembleSubmissionPackageCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub generation_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub generation_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectSubmissionPackageItemContentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub package_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectSubmissionPackageCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionPackageStatus {
    Proposed,
}

impl SubmissionPackageStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "proposed" => Ok(Self::Proposed),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionPackageAssessment {
    Complete,
    Blocked,
}

impl SubmissionPackageAssessment {
    fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Blocked => "blocked",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "complete" => Ok(Self::Complete),
            "blocked" => Ok(Self::Blocked),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionCoverageDisposition {
    Covered,
    Missing,
    Unsupported,
}

impl SubmissionCoverageDisposition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Covered => "covered",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionCoverageBlockerCode {
    MissingItem,
    UnsupportedAuthoring,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionCoverageBlocker {
    pub code: SubmissionCoverageBlockerCode,
    pub requirement_id: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionPackageDependencyKind {
    Baseline,
    BaselineApproval,
    WorkPlan,
    WorkPlanApproval,
    ProductionActivation,
    CalculationManifest,
    Decision,
    TenderDeadline,
    ChangeState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionPackageDependency {
    pub kind: SubmissionPackageDependencyKind,
    pub reference_id: String,
    pub version: u32,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionDecisionReferenceKind {
    Approval,
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionDecisionReference {
    pub kind: SubmissionDecisionReferenceKind,
    pub decision_id: String,
    pub subject_kind: CoordinatedBidBaselineBindingKind,
    pub subject_reference_id: String,
    pub subject_version: u32,
    pub subject_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionProfileVersionReference {
    pub profile_id: String,
    pub profile_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionWorkPlanContext {
    pub plan_id: String,
    pub plan_version: u32,
    pub plan_manifest_sha256: String,
    pub plan_approval_id: String,
    pub plan_approval_sha256: String,
    pub activation_id: String,
    pub authorized_profile_versions: Vec<SubmissionProfileVersionReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionContributionKind {
    MaterialInstructionAuthor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionAuthorshipProvenance {
    AgentContribution {
        run_id: String,
        profile_id: String,
        profile_version: u32,
        contribution_kind: SubmissionContributionKind,
        exact_source: SubmissionValidationContextInput,
    },
    ExternalSource {
        artifact_id: String,
        version: u32,
        content_sha256: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionItemSource {
    Generated {
        artifact_id: String,
        version: u32,
        manifest_sha256: String,
        content_sha256: String,
        size_bytes: u64,
    },
    UnchangedSource {
        artifact_id: String,
        version: u32,
        content_sha256: String,
        size_bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionValidationContextInput {
    pub input_kind: String,
    pub reference_id: String,
    pub version: u32,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionPackageItem {
    pub item_id: String,
    pub package_path: String,
    pub section_key: String,
    pub envelope_key: String,
    pub language: String,
    pub media_type: String,
    pub classifications: Vec<GenerationRequirementKind>,
    pub scope_record_ids: Vec<String>,
    pub content_sha256: String,
    pub size_bytes: u64,
    pub source: SubmissionItemSource,
    pub requirement_ids: Vec<String>,
    pub evidence: Vec<super::TenderRecordEvidence>,
    pub provenance: Vec<String>,
    pub calculation_references: Vec<String>,
    pub review_references: Vec<String>,
    pub decision_references: Vec<String>,
    pub authorship: Vec<SubmissionAuthorshipProvenance>,
    pub validation_context_inputs: Vec<SubmissionValidationContextInput>,
    pub validation_context_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionCoverageRow {
    pub requirement: GenerationRequirement,
    pub disposition: SubmissionCoverageDisposition,
    pub item_id: Option<String>,
    pub blockers: Vec<SubmissionCoverageBlocker>,
    pub required_capabilities: Vec<String>,
    pub risk_references: Vec<SubmissionPackageDependency>,
    pub manual_validation_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionSectionRiskContext {
    pub baseline_manifest_sha256: String,
    pub risk_references: Vec<SubmissionPackageDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionSectionIndependenceContext {
    pub independent_review_required: bool,
    pub author_profile_versions: Vec<SubmissionProfileVersionReference>,
    pub authorized_profile_versions: Vec<SubmissionProfileVersionReference>,
    pub derivation_inputs: Vec<SubmissionValidationContextInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionPackageSection {
    pub section_key: String,
    pub envelope_key: String,
    pub language: String,
    pub item_ids: Vec<String>,
    pub requirement_ids: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub risk_context: SubmissionSectionRiskContext,
    pub independence_context: SubmissionSectionIndependenceContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionPackageVersion {
    pub package_id: String,
    pub version: u32,
    pub tender_revision: u32,
    pub status: SubmissionPackageStatus,
    pub assessment: SubmissionPackageAssessment,
    pub current: bool,
    pub currentness_facts: Vec<SubmissionPackageCurrentnessFact>,
    pub generation_id: String,
    pub generation_sequence: u32,
    pub generation_manifest_sha256: String,
    pub baseline_id: String,
    pub baseline_version: u32,
    pub baseline_manifest_sha256: String,
    pub baseline_approval_id: String,
    pub work_plan: SubmissionWorkPlanContext,
    pub calculation_manifest_references: Vec<SubmissionPackageDependency>,
    pub current_decision_references: Vec<SubmissionDecisionReference>,
    pub submission_deadline: Option<SubmissionPackageDependency>,
    pub sections: Vec<SubmissionPackageSection>,
    pub items: Vec<SubmissionPackageItem>,
    pub coverage: Vec<SubmissionCoverageRow>,
    pub validation_context_inputs: Vec<SubmissionValidationContextInput>,
    pub validation_context_sha256: String,
    pub dependency_currentness_sha256: String,
    pub manifest_bytes: Vec<u8>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionItemContent {
    pub package_id: String,
    pub package_version: u32,
    pub package_manifest_sha256: String,
    pub item_id: String,
    pub package_path: String,
    pub media_type: String,
    pub content_sha256: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SubmissionPackageCurrentnessCode {
    HeadAdvanced,
    GenerationAdvanced,
    BaselineChanged,
    WorkPlanChanged,
    ChangePending,
    SourceChanged,
    DependencyDigestChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionPackageCurrentnessFact {
    pub code: SubmissionPackageCurrentnessCode,
    pub current: bool,
    pub reference_id: String,
    pub expected_value: String,
    pub actual_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SubmissionPackageManifest {
    schema_version: u32,
    package_id: String,
    version: u32,
    tender_revision: u32,
    status: SubmissionPackageStatus,
    assessment: SubmissionPackageAssessment,
    generation_id: String,
    generation_sequence: u32,
    generation_manifest_sha256: String,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    baseline_approval_id: String,
    work_plan: SubmissionWorkPlanContext,
    calculation_manifest_references: Vec<SubmissionPackageDependency>,
    current_decision_references: Vec<SubmissionDecisionReference>,
    submission_deadline: Option<SubmissionPackageDependency>,
    sections: Vec<SubmissionPackageSection>,
    items: Vec<SubmissionPackageItem>,
    coverage: Vec<SubmissionCoverageRow>,
    validation_context_inputs: Vec<SubmissionValidationContextInput>,
    validation_context_sha256: String,
    dependency_currentness_sha256: String,
    created_at: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ExactSubmissionPackageItem {
    pub(crate) item: SubmissionPackageItem,
    pub(crate) content_integrity: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ExactSubmissionPackage {
    pub(crate) package: SubmissionPackageVersion,
    pub(crate) items: Vec<ExactSubmissionPackageItem>,
}

#[derive(Debug, Clone)]
struct ExactBaselineContext {
    tender_revision: u32,
    approval_id: String,
    approval_sha256: String,
    activation_id: String,
    plan_id: String,
    plan_version: u32,
    plan_manifest_sha256: String,
    bindings: Vec<CoordinatedBidBaselineBinding>,
}

#[derive(Debug, Clone)]
struct PlanInputs {
    context: SubmissionWorkPlanContext,
    workstreams: Vec<WorkPlanWorkstream>,
}

#[derive(Debug)]
struct BuiltItem {
    view: SubmissionPackageItem,
    content_integrity: String,
}

impl QuantixHost {
    pub fn assemble_submission_package(
        &self,
        command: AssembleSubmissionPackageCommand,
    ) -> Result<SubmissionPackageVersion, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .assemble_submission_package(&tender_id, &command, budget);
        result
    }

    pub fn inspect_current_submission_package(
        &self,
        tender_id: &str,
    ) -> Result<Option<SubmissionPackageVersion>, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_current_submission_package(budget);
        result
    }

    pub fn inspect_submission_package_item_content(
        &self,
        command: InspectSubmissionPackageItemContentCommand,
    ) -> Result<SubmissionItemContent, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_submission_package_item_content(&command, budget);
        result
    }
}

impl TenderStore {
    fn assemble_submission_package(
        &mut self,
        tender_id: &TenderId,
        command: &AssembleSubmissionPackageCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<SubmissionPackageVersion, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let content_root = self.root.join("content");
        let generation = self.load_exact_current_package_production_generation(
            &command.generation_id,
            &command.generation_manifest_sha256,
            budget,
        )?;
        let baseline_view = self.load_exact_current_approved_coordinated_baseline(
            &generation.baseline_id,
            generation.baseline_version,
            &generation.baseline_manifest_sha256,
            budget,
        )?;
        let baseline = exact_baseline_context_from_view(&baseline_view)?;
        let plan_view = self.load_exact_approved_active_work_plan_for_submission(
            &baseline.plan_id,
            baseline.plan_version,
            &baseline.plan_manifest_sha256,
            &baseline.activation_id,
            budget,
        )?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        TenderStore::load_exact_current_package_production_generation_in_transaction(
            &transaction,
            &generation,
        )?;
        revalidate_exact_dependency_snapshot(&transaction, &generation, &baseline, &plan_view)?;
        if let Some((package_id, version, manifest_sha256)) = transaction
            .query_row(
                "SELECT versions.package_id, versions.version, versions.manifest_sha256
                 FROM submission_package_head AS head
                 JOIN submission_package_versions AS versions
                   ON versions.package_id = head.package_id
                  AND versions.version = head.current_version
                 WHERE versions.generation_id = ?1
                   AND versions.generation_manifest_sha256 = ?2",
                params![command.generation_id, command.generation_manifest_sha256],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?
        {
            let exact = load_exact_submission_package_for_transaction(
                &transaction,
                &package_id,
                version,
                &manifest_sha256,
                budget,
            )?;
            transaction.commit().map_err(sql_error)?;
            return Ok(exact.package);
        }

        require_package_production_lifecycle(&transaction)?;

        let plan = load_exact_work_plan(&transaction, &baseline, &plan_view)?;
        let package_id: String = match transaction
            .query_row(
                "SELECT package_id FROM submission_packages LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?
        {
            Some(package_id) => package_id,
            None => random_identifier(&transaction)?,
        };
        let version: u32 = transaction
            .query_row(
                "SELECT COALESCE(MAX(version), 0) + 1 FROM submission_package_versions",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if version > MAX_PACKAGE_VERSIONS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let risk_references = baseline_risk_references(&baseline.bindings);
        let mut items = build_items(
            &transaction,
            &content_root,
            &package_id,
            &generation,
            budget,
        )?;
        if items.len() > MAX_PACKAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        items.sort_by(|left, right| left.view.package_path.cmp(&right.view.package_path));
        let item_by_requirement = items
            .iter()
            .flat_map(|item| {
                item.view
                    .requirement_ids
                    .iter()
                    .map(move |requirement_id| (requirement_id.clone(), item.view.item_id.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        let coverage = generation
            .requirements
            .iter()
            .map(|requirement| {
                let item_id = item_by_requirement.get(&requirement.requirement_id).cloned();
                let item = item_id
                    .as_ref()
                    .and_then(|item_id| items.iter().find(|item| &item.view.item_id == item_id));
                let (disposition, blockers) = match (requirement.availability, item) {
                    (GenerationRequirementAvailability::Missing, None) => (
                        SubmissionCoverageDisposition::Missing,
                        vec![SubmissionCoverageBlocker {
                            code: SubmissionCoverageBlockerCode::MissingItem,
                            requirement_id: requirement.requirement_id.clone(),
                            detail: "No exact immutable package item covers this requirement."
                                .into(),
                        }],
                    ),
                    (GenerationRequirementAvailability::Unsupported, None) => (
                        SubmissionCoverageDisposition::Unsupported,
                        vec![SubmissionCoverageBlocker {
                            code: SubmissionCoverageBlockerCode::UnsupportedAuthoring,
                            requirement_id: requirement.requirement_id.clone(),
                            detail: "The verified requirement names an unsupported submission authoring format."
                                .into(),
                        }],
                    ),
                    (GenerationRequirementAvailability::Available, Some(item))
                        if supported_package_media_type(&item.view.media_type) => {
                        (SubmissionCoverageDisposition::Covered, Vec::new())
                    }
                    _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
                };
                Ok(SubmissionCoverageRow {
                    requirement: requirement.clone(),
                    disposition,
                    item_id,
                    blockers,
                    required_capabilities: capabilities_for_section(
                        &plan,
                        &requirement.section_key,
                    ),
                    risk_references: risks_for_requirement(&risk_references, requirement),
                    manual_validation_required: requirement.unchanged_source_artifact.is_some(),
                })
            })
            .collect::<Result<Vec<_>, TenderCommandError>>()?;
        let sections = build_sections(
            &items,
            &coverage,
            &risk_references,
            &plan,
            &plan.context,
            &generation,
        )?;
        let calculation_manifest_references = baseline
            .bindings
            .iter()
            .filter(|binding| {
                binding.kind == CoordinatedBidBaselineBindingKind::CalculationManifest
            })
            .map(dependency_from_binding)
            .collect::<Vec<_>>();
        let current_decision_references = baseline
            .bindings
            .iter()
            .flat_map(decision_references_from_binding)
            .collect::<Vec<_>>();
        let submission_deadline =
            load_submission_deadline(&transaction, &generation, &baseline.bindings)?;
        let validation_context_inputs =
            package_validation_inputs(&generation, &baseline, &plan.context, &items);
        let validation_context_sha256 = canonical_sha256(&validation_context_inputs)?;
        let dependency_currentness_sha256 = canonical_sha256(&json!({
            "inputs": validation_context_inputs,
            "item_validation_roots": items.iter().map(|item| item.view.validation_context_sha256.clone()).collect::<Vec<_>>(),
        }))?;
        let assessment = if coverage.iter().all(|row| {
            row.disposition == SubmissionCoverageDisposition::Covered && row.blockers.is_empty()
        }) {
            SubmissionPackageAssessment::Complete
        } else {
            SubmissionPackageAssessment::Blocked
        };
        let status = SubmissionPackageStatus::Proposed;
        let manifest = SubmissionPackageManifest {
            schema_version: 1,
            package_id: package_id.clone(),
            version,
            tender_revision: baseline.tender_revision,
            status,
            assessment,
            generation_id: generation.generation_id.clone(),
            generation_sequence: generation.sequence,
            generation_manifest_sha256: generation.manifest_sha256.clone(),
            baseline_id: generation.baseline_id.clone(),
            baseline_version: generation.baseline_version,
            baseline_manifest_sha256: generation.baseline_manifest_sha256.clone(),
            baseline_approval_id: baseline.approval_id.clone(),
            work_plan: plan.context.clone(),
            calculation_manifest_references: calculation_manifest_references.clone(),
            current_decision_references: current_decision_references.clone(),
            submission_deadline: submission_deadline.clone(),
            sections: sections.clone(),
            items: items.iter().map(|item| item.view.clone()).collect(),
            coverage: coverage.clone(),
            validation_context_inputs: validation_context_inputs.clone(),
            validation_context_sha256: validation_context_sha256.clone(),
            dependency_currentness_sha256: dependency_currentness_sha256.clone(),
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        if manifest_json.len() > MAX_MANIFEST_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());

        transaction
            .execute(
                "INSERT OR IGNORE INTO submission_packages (singleton, package_id, created_at)
                 VALUES (1, ?1, ?2)",
                params![package_id, created_at],
            )
            .map_err(sql_error)?;
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "submission_package_assembled",
            baseline.tender_revision,
            json!({
                "package_id": package_id,
                "version": version.to_string(),
                "manifest_sha256": manifest_sha256,
                "generation_id": generation.generation_id,
                "generation_manifest_sha256": generation.manifest_sha256,
                "item_count": items.len().to_string(),
                "coverage_count": coverage.len().to_string(),
                "status": status.as_str(),
                "assessment": assessment.as_str(),
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO submission_package_versions (
                   package_id, version, status, assessment, generation_id, generation_sequence,
                   generation_manifest_sha256, baseline_id, baseline_version,
                   baseline_manifest_sha256, baseline_approval_id, work_plan_json,
                   calculation_references_json, decision_references_json, deadline_json,
                   validation_context_inputs_json, validation_context_sha256,
                   dependency_currentness_sha256, audit_sequence, manifest_json,
                   manifest_sha256, created_at, tender_revision
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                   ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
                 )",
                params![
                    package_id,
                    version,
                    status.as_str(),
                    assessment.as_str(),
                    generation.generation_id,
                    generation.sequence,
                    generation.manifest_sha256,
                    generation.baseline_id,
                    generation.baseline_version,
                    generation.baseline_manifest_sha256,
                    baseline.approval_id,
                    canonical_json(&plan.context)?,
                    canonical_json(&calculation_manifest_references)?,
                    canonical_json(&current_decision_references)?,
                    submission_deadline
                        .as_ref()
                        .map(canonical_json)
                        .transpose()?,
                    canonical_json(&validation_context_inputs)?,
                    validation_context_sha256,
                    dependency_currentness_sha256,
                    audit_sequence,
                    manifest_json,
                    manifest_sha256,
                    created_at,
                    baseline.tender_revision,
                ],
            )
            .map_err(sql_error)?;
        persist_relational_package(
            &transaction,
            &package_id,
            version,
            &items,
            &coverage,
            &sections,
            budget,
        )?;
        advance_submission_package_head(&transaction, &package_id, version)?;
        if assessment == SubmissionPackageAssessment::Complete {
            transaction
                .execute(
                    "UPDATE tender SET lifecycle_phase = 'final_review'
                     WHERE singleton = 1 AND lifecycle_phase = 'package_production'",
                    [],
                )
                .map_err(sql_error)?;
        }
        let exact = load_exact_submission_package_for_transaction(
            &transaction,
            &package_id,
            version,
            &manifest_sha256,
            budget,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(exact.package)
    }

    fn inspect_current_submission_package(
        &mut self,
        budget: BidPackageOperationBudget,
    ) -> Result<Option<SubmissionPackageVersion>, TenderCommandError> {
        budget.check()?;
        let content_root = self.root.join("content");
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(sql_error)?;
        let head: Option<(String, u32, String)> = transaction
            .query_row(
                "SELECT versions.package_id, versions.version, versions.manifest_sha256
                 FROM submission_package_head AS head
                 JOIN submission_package_versions AS versions
                   ON versions.package_id = head.package_id
                  AND versions.version = head.current_version",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let package = head
            .map(|(package_id, version, manifest_sha256)| {
                let exact = load_stored_package(
                    &transaction,
                    &package_id,
                    version,
                    &manifest_sha256,
                    false,
                    budget,
                )?;
                for item in &exact.items {
                    budget.check()?;
                    verify_content_streaming(
                        &content_root,
                        &item.content_integrity,
                        &item.item.content_sha256,
                        item.item.size_bytes,
                    )?;
                }
                Ok(exact.package)
            })
            .transpose()?;
        transaction.commit().map_err(sql_error)?;
        Ok(package)
    }

    fn inspect_submission_package_item_content(
        &mut self,
        command: &InspectSubmissionPackageItemContentCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<SubmissionItemContent, TenderCommandError> {
        budget.check()?;
        let content_root = self.root.join("content");
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(sql_error)?;
        let exact = load_stored_package(
            &transaction,
            &command.package_id,
            command.version,
            &command.manifest_sha256,
            false,
            budget,
        )?;
        let item = exact
            .items
            .into_iter()
            .find(|item| item.item.item_id == command.item_id)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        let bytes = load_exact_submission_package_item_bytes_for_transaction(&content_root, &item)?;
        let result = SubmissionItemContent {
            package_id: command.package_id.clone(),
            package_version: command.version,
            package_manifest_sha256: command.manifest_sha256.clone(),
            item_id: item.item.item_id,
            package_path: item.item.package_path,
            media_type: item.item.media_type,
            content_sha256: item.item.content_sha256,
            bytes,
        };
        transaction.commit().map_err(sql_error)?;
        Ok(result)
    }

    pub(crate) fn submission_package_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let (identities, heads, versions, lifecycle): (i64, i64, i64, String) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM submission_packages),
                        (SELECT COUNT(*) FROM submission_package_head),
                        (SELECT COUNT(*) FROM submission_package_versions),
                        (SELECT lifecycle_phase FROM tender WHERE singleton = 1)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(sql_error)?;
        if identities > 1
            || heads != identities
            || (versions == 0 && identities != 0)
            || versions > MAX_PACKAGE_VERSIONS as i64
        {
            return Ok(false);
        }
        let budget = BidPackageOperationBudget::from_connection(&self.connection)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT package_id, version, manifest_sha256
                 FROM submission_package_versions ORDER BY package_id, version",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            check()?;
            let (package_id, version, manifest_sha256) = row.map_err(sql_error)?;
            if load_stored_package(
                &self.connection,
                &package_id,
                version,
                &manifest_sha256,
                false,
                budget,
            )
            .is_err()
            {
                return Ok(false);
            }
        }
        if identities == 1 {
            let (package_id, head_version, max_version): (String, u32, u32) = self
                .connection
                .query_row(
                    "SELECT head.package_id, head.current_version, MAX(versions.version)
                     FROM submission_package_head AS head
                     JOIN submission_package_versions AS versions
                       ON versions.package_id = head.package_id
                     GROUP BY head.package_id, head.current_version",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(sql_error)?;
            let contiguous: bool = self
                .connection
                .query_row(
                    "SELECT COUNT(*) = MAX(version) AND MIN(version) = 1
                     FROM submission_package_versions WHERE package_id = ?1",
                    [&package_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if head_version != max_version || !contiguous {
                return Ok(false);
            }
            let head_manifest: String = self
                .connection
                .query_row(
                    "SELECT manifest_sha256 FROM submission_package_versions
                     WHERE package_id = ?1 AND version = ?2",
                    params![package_id, head_version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let head = load_stored_package(
                &self.connection,
                &package_id,
                head_version,
                &head_manifest,
                false,
                budget,
            )?;
            if lifecycle == "final_review"
                && (head.package.assessment != SubmissionPackageAssessment::Complete
                    || !head.package.current)
            {
                return Ok(false);
            }
            if lifecycle == "package_production"
                && head.package.assessment == SubmissionPackageAssessment::Complete
                && head.package.current
            {
                return Ok(false);
            }
        } else if lifecycle == "final_review" {
            return Ok(false);
        }
        Ok(true)
    }
}

fn require_package_production_lifecycle(
    transaction: &Transaction<'_>,
) -> Result<(), TenderCommandError> {
    let lifecycle: String = transaction
        .query_row(
            "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let unresolved_change: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM change_assessments AS assessments
               LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
               WHERE resolutions.assessment_id IS NULL
             )",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if lifecycle != "package_production" || unresolved_change {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn exact_baseline_context_from_view(
    baseline: &super::CoordinatedBidBaseline,
) -> Result<ExactBaselineContext, TenderCommandError> {
    let approval = baseline
        .approval
        .as_ref()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(ExactBaselineContext {
        tender_revision: baseline.tender_revision,
        approval_id: approval.approval_id.clone(),
        approval_sha256: approval.approval_sha256.clone(),
        activation_id: baseline.activation_id.clone(),
        plan_id: baseline.plan_id.clone(),
        plan_version: baseline.plan_version,
        plan_manifest_sha256: baseline.plan_manifest_sha256.clone(),
        bindings: baseline.bindings.clone(),
    })
}

fn load_exact_work_plan(
    connection: &Connection,
    baseline: &ExactBaselineContext,
    exact: &WorkPlanProposalInspection,
) -> Result<PlanInputs, TenderCommandError> {
    let row: (String, String, String, String, String) = connection
        .query_row(
            "SELECT approvals.approval_id, approvals.approval_sha256,
                    versions.profiles_json, versions.workstreams_json,
                    activations.activation_id
             FROM work_plan_versions AS versions
             JOIN work_plan_approvals AS approvals
               ON approvals.plan_id = versions.plan_id
              AND approvals.plan_version = versions.version
             JOIN production_activations AS activations
               ON activations.plan_id = versions.plan_id
              AND activations.plan_version = versions.version
             WHERE versions.plan_id = ?1 AND versions.version = ?2
               AND versions.manifest_sha256 = ?3
               AND approvals.decision = 'approve'
               AND activations.activation_id = ?4 AND activations.status = 'active'",
            params![
                baseline.plan_id,
                baseline.plan_version,
                baseline.plan_manifest_sha256,
                baseline.activation_id
            ],
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
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let profiles: Vec<WorkPlanProfileBinding> = parse_canonical(&row.2)?;
    let workstreams: Vec<WorkPlanWorkstream> = parse_canonical(&row.3)?;
    let exact_approval = exact
        .approval
        .as_ref()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if exact.plan_id != baseline.plan_id
        || exact.version != baseline.plan_version
        || exact.manifest_sha256 != baseline.plan_manifest_sha256
        || exact.profiles != profiles
        || exact.workstreams != workstreams
        || exact_approval.approval_id != row.0
        || exact_approval.approval_sha256 != row.1
        || exact_approval.plan_manifest_sha256 != baseline.plan_manifest_sha256
        || row.4 != baseline.activation_id
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let mut authorized_profile_versions = profiles
        .iter()
        .map(|binding| SubmissionProfileVersionReference {
            profile_id: binding.profile.profile_id.clone(),
            profile_version: binding.profile.version,
        })
        .collect::<Vec<_>>();
    authorized_profile_versions.sort_by(|left, right| {
        (&left.profile_id, left.profile_version).cmp(&(&right.profile_id, right.profile_version))
    });
    authorized_profile_versions.dedup();
    if authorized_profile_versions.is_empty() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(PlanInputs {
        context: SubmissionWorkPlanContext {
            plan_id: baseline.plan_id.clone(),
            plan_version: baseline.plan_version,
            plan_manifest_sha256: baseline.plan_manifest_sha256.clone(),
            plan_approval_id: row.0,
            plan_approval_sha256: row.1,
            activation_id: row.4,
            authorized_profile_versions,
        },
        workstreams,
    })
}

fn revalidate_exact_dependency_snapshot(
    transaction: &Transaction<'_>,
    generation: &PackageProductionGeneration,
    baseline: &ExactBaselineContext,
    plan: &WorkPlanProposalInspection,
) -> Result<(), TenderCommandError> {
    let exact_baseline_and_plan: bool = transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM coordinated_bid_baseline_head AS baseline_head
               JOIN coordinated_bid_baseline_versions AS baseline_versions
                 ON baseline_versions.baseline_id = baseline_head.baseline_id
                AND baseline_versions.version = baseline_head.current_version
               JOIN coordinated_bid_baseline_approvals AS baseline_approvals
                 ON baseline_approvals.baseline_id = baseline_versions.baseline_id
                AND baseline_approvals.baseline_version = baseline_versions.version
               JOIN work_plan_heads AS plan_head
                 ON plan_head.plan_id = baseline_versions.plan_id
                AND plan_head.current_version = baseline_versions.plan_version
               JOIN work_plan_versions AS plan_versions
                 ON plan_versions.plan_id = plan_head.plan_id
                AND plan_versions.version = plan_head.current_version
               JOIN work_plan_approvals AS plan_approvals
                 ON plan_approvals.plan_id = plan_versions.plan_id
                AND plan_approvals.plan_version = plan_versions.version
               JOIN production_activations AS activations
                 ON activations.plan_id = plan_versions.plan_id
                AND activations.plan_version = plan_versions.version
               WHERE baseline_versions.baseline_id = ?1
                 AND baseline_versions.version = ?2
                 AND baseline_versions.manifest_sha256 = ?3
                 AND baseline_approvals.approval_id = ?4
                 AND baseline_approvals.approval_sha256 = ?5
                 AND baseline_approvals.baseline_manifest_sha256 = ?3
                 AND baseline_approvals.decision = 'approve'
                 AND baseline_versions.plan_id = ?6
                 AND baseline_versions.plan_version = ?7
                 AND baseline_versions.plan_manifest_sha256 = ?8
                 AND baseline_versions.activation_id = ?9
                 AND plan_versions.manifest_sha256 = ?8
                 AND plan_approvals.approval_id = ?10
                 AND plan_approvals.approval_sha256 = ?11
                 AND plan_approvals.plan_manifest_sha256 = ?8
                 AND plan_approvals.decision = 'approve'
                 AND activations.activation_id = ?9
                 AND activations.status = 'active'
             )",
            params![
                generation.baseline_id,
                generation.baseline_version,
                generation.baseline_manifest_sha256,
                baseline.approval_id,
                baseline.approval_sha256,
                baseline.plan_id,
                baseline.plan_version,
                baseline.plan_manifest_sha256,
                baseline.activation_id,
                plan.approval
                    .as_ref()
                    .map(|approval| approval.approval_id.as_str()),
                plan.approval
                    .as_ref()
                    .map(|approval| approval.approval_sha256.as_str()),
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if !exact_baseline_and_plan {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn build_items(
    connection: &Connection,
    content_root: &Path,
    package_id: &str,
    generation: &PackageProductionGeneration,
    budget: BidPackageOperationBudget,
) -> Result<Vec<BuiltItem>, TenderCommandError> {
    let artifacts = generation
        .artifact_versions
        .iter()
        .map(|artifact| ((artifact.artifact_id.clone(), artifact.version), artifact))
        .collect::<BTreeMap<_, _>>();
    let mut grouped: BTreeMap<String, Vec<&GenerationRequirement>> = BTreeMap::new();
    for requirement in &generation.requirements {
        budget.check()?;
        validate_package_path(&requirement.package_path)?;
        if requirement.availability != GenerationRequirementAvailability::Available {
            continue;
        }
        grouped
            .entry(package_collision_key(&requirement.package_path))
            .or_default()
            .push(requirement);
    }
    let mut items = Vec::with_capacity(grouped.len());
    for (collision_key, requirements) in grouped {
        budget.check()?;
        let first = requirements[0];
        if requirements.iter().any(|requirement| {
            requirement.package_path != first.package_path
                || requirement.section_key != first.section_key
                || requirement.envelope_key != first.envelope_key
                || requirement.language != first.language
                || requirement.content_sha256 != first.content_sha256
                || requirement.size_bytes != first.size_bytes
                || requirement.generated_artifact != first.generated_artifact
                || requirement.unchanged_source_artifact != first.unchanged_source_artifact
        }) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let exact_content_sha256 = first
            .content_sha256
            .as_ref()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let exact_size_bytes = first
            .size_bytes
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let (source, media_type, classifications, scope_record_ids, provenance, integrity) = match (
            &first.generated_artifact,
            &first.unchanged_source_artifact,
        ) {
            (Some(reference), None) => {
                let artifact = artifacts
                    .get(&(reference.artifact_id.clone(), reference.version))
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                validate_generated_source(connection, content_root, artifact)?;
                let integrity: String = connection
                        .query_row(
                            "SELECT objects.integrity
                             FROM submission_artifact_versions AS versions
                             JOIN content_objects AS objects ON objects.sha256 = versions.content_sha256
                             WHERE versions.artifact_id = ?1 AND versions.version = ?2",
                            params![reference.artifact_id, reference.version],
                            |row| row.get(0),
                        )
                        .map_err(sql_error)?;
                (
                    SubmissionItemSource::Generated {
                        artifact_id: artifact.artifact_id.clone(),
                        version: artifact.version,
                        manifest_sha256: artifact.manifest_sha256.clone(),
                        content_sha256: artifact.content_sha256.clone(),
                        size_bytes: artifact.size_bytes,
                    },
                    artifact.media_type.clone(),
                    artifact.classifications.clone(),
                    artifact.scope_record_ids.clone(),
                    artifact.provenance.clone(),
                    integrity,
                )
            }
            (None, Some(reference)) => {
                let (media_type, content_sha256, size_bytes, integrity): (String, String, i64, String) = connection
                        .query_row(
                            "SELECT versions.media_type, versions.sha256, versions.size_bytes, objects.integrity
                             FROM source_artifact_versions AS versions
                             JOIN content_objects AS objects ON objects.sha256 = versions.sha256
                             WHERE versions.artifact_id = ?1 AND versions.version = ?2
                               AND versions.registration_state = 'registered'",
                            params![reference.artifact_id, reference.version],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                        )
                        .optional()
                        .map_err(sql_error)?
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                let size_bytes = u64::try_from(size_bytes)
                    .ok()
                    .filter(|size| *size > 0 && *size <= MAX_ITEM_BYTES)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                if content_sha256 != *exact_content_sha256 || size_bytes != exact_size_bytes {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                read_and_verify_content(content_root, &integrity, &content_sha256, size_bytes)?;
                (
                    SubmissionItemSource::UnchangedSource {
                        artifact_id: reference.artifact_id.clone(),
                        version: reference.version,
                        content_sha256: content_sha256.clone(),
                        size_bytes,
                    },
                    media_type,
                    requirements
                        .iter()
                        .map(|requirement| requirement.kind)
                        .collect(),
                    requirements
                        .iter()
                        .map(|requirement| requirement.record.record_id.clone())
                        .collect(),
                    vec![format!(
                        "source:{}:{}",
                        reference.artifact_id, reference.version
                    )],
                    integrity,
                )
            }
            _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        };
        let mut requirement_ids = requirements
            .iter()
            .map(|requirement| requirement.requirement_id.clone())
            .collect::<Vec<_>>();
        requirement_ids.sort();
        let mut evidence = requirements
            .iter()
            .flat_map(|requirement| requirement.evidence.clone())
            .collect::<Vec<_>>();
        evidence.sort_by(|left, right| {
            (
                &left.reference.artifact_id,
                left.reference.version,
                left.reference.ordinal,
            )
                .cmp(&(
                    &right.reference.artifact_id,
                    right.reference.version,
                    right.reference.ordinal,
                ))
        });
        evidence.dedup();
        let mut calculation_references = union_strings(
            requirements
                .iter()
                .flat_map(|r| r.calculation_references.iter()),
        );
        let mut review_references =
            union_strings(requirements.iter().flat_map(|r| r.review_references.iter()));
        let mut decision_references = union_strings(
            requirements
                .iter()
                .flat_map(|r| r.decision_references.iter()),
        );
        calculation_references.sort();
        review_references.sort();
        decision_references.sort();
        let authorship = load_authorship(connection, &requirements, &source)?;
        let item_id = submission_item_id(
            &collision_key,
            &source,
            exact_content_sha256,
            exact_size_bytes,
        )?;
        let mut validation_context_inputs = requirements
            .iter()
            .map(|requirement| SubmissionValidationContextInput {
                input_kind: "generation_requirement".into(),
                reference_id: requirement.requirement_id.clone(),
                version: 1,
                sha256: requirement.manifest_sha256.clone(),
            })
            .collect::<Vec<_>>();
        validation_context_inputs.push(source_validation_input(&source));
        validation_context_inputs.sort_by(|left, right| {
            (
                &left.input_kind,
                &left.reference_id,
                left.version,
                &left.sha256,
            )
                .cmp(&(
                    &right.input_kind,
                    &right.reference_id,
                    right.version,
                    &right.sha256,
                ))
        });
        let validation_context_sha256 = canonical_sha256(&json!({
            "package_id": package_id,
            "item_id": item_id,
            "package_path": first.package_path,
            "media_type": media_type,
            "inputs": validation_context_inputs,
            "calculation_references": calculation_references,
            "review_references": review_references,
            "decision_references": decision_references,
        }))?;
        let mut classifications = classifications;
        classifications.sort_by_key(|kind| kind.as_str());
        classifications.dedup();
        let mut scope_record_ids = scope_record_ids;
        scope_record_ids.sort();
        scope_record_ids.dedup();
        items.push(BuiltItem {
            view: SubmissionPackageItem {
                item_id,
                package_path: first.package_path.clone(),
                section_key: first.section_key.clone(),
                envelope_key: first.envelope_key.clone(),
                language: first.language.clone(),
                media_type,
                classifications,
                scope_record_ids,
                content_sha256: exact_content_sha256.clone(),
                size_bytes: exact_size_bytes,
                source,
                requirement_ids,
                evidence,
                provenance,
                calculation_references,
                review_references,
                decision_references,
                authorship,
                validation_context_inputs,
                validation_context_sha256,
            },
            content_integrity: integrity,
        });
    }
    Ok(items)
}

fn validate_generated_source(
    connection: &Connection,
    content_root: &Path,
    artifact: &SubmissionArtifactVersion,
) -> Result<(), TenderCommandError> {
    let row: (String, i64, String, String) = connection
        .query_row(
            "SELECT versions.content_sha256, versions.size_bytes,
                    versions.manifest_sha256, objects.integrity
             FROM submission_artifact_versions AS versions
             JOIN content_objects AS objects ON objects.sha256 = versions.content_sha256
             WHERE versions.artifact_id = ?1 AND versions.version = ?2",
            params![artifact.artifact_id, artifact.version],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let size = u64::try_from(row.1)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if row.0 != artifact.content_sha256
        || size != artifact.size_bytes
        || row.2 != artifact.manifest_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    read_and_verify_content(content_root, &row.3, &row.0, size).map(|_| ())
}

fn load_authorship(
    connection: &Connection,
    requirements: &[&GenerationRequirement],
    source: &SubmissionItemSource,
) -> Result<Vec<SubmissionAuthorshipProvenance>, TenderCommandError> {
    let mut authorship = Vec::new();
    for requirement in requirements {
        let row: (String, String, u32) = connection
            .query_row(
                "SELECT versions.author_run_id, runs.profile_id, runs.profile_version
                 FROM tender_record_versions AS versions
                 JOIN agent_runs AS runs ON runs.run_id = versions.author_run_id
                 JOIN generation_requirements AS generated
                   ON generated.record_id = versions.record_id
                  AND generated.record_version = versions.version
                 WHERE versions.record_id = ?1 AND versions.version = ?2
                   AND generated.requirement_id = ?3
                   AND generated.record_manifest_sha256 = ?4",
                params![
                    requirement.record.record_id,
                    requirement.record.version,
                    requirement.requirement_id,
                    requirement.record.manifest_sha256
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        authorship.push(SubmissionAuthorshipProvenance::AgentContribution {
            run_id: row.0,
            profile_id: row.1,
            profile_version: row.2,
            contribution_kind: SubmissionContributionKind::MaterialInstructionAuthor,
            exact_source: SubmissionValidationContextInput {
                input_kind: "tender_record_version".into(),
                reference_id: requirement.record.record_id.clone(),
                version: requirement.record.version,
                sha256: requirement.record.manifest_sha256.clone(),
            },
        });
    }
    if let SubmissionItemSource::UnchangedSource {
        artifact_id,
        version,
        content_sha256,
        ..
    } = source
    {
        authorship.push(SubmissionAuthorshipProvenance::ExternalSource {
            artifact_id: artifact_id.clone(),
            version: *version,
            content_sha256: content_sha256.clone(),
        });
    }
    let mut keyed = authorship
        .into_iter()
        .map(|author| Ok((canonical_json(&author)?, author)))
        .collect::<Result<Vec<_>, TenderCommandError>>()?;
    keyed.sort_by(|left, right| left.0.cmp(&right.0));
    keyed.dedup_by(|left, right| left.0 == right.0);
    Ok(keyed.into_iter().map(|(_, author)| author).collect())
}

fn build_sections(
    items: &[BuiltItem],
    coverage: &[SubmissionCoverageRow],
    risks: &[SubmissionPackageDependency],
    plan: &PlanInputs,
    work_plan: &SubmissionWorkPlanContext,
    generation: &PackageProductionGeneration,
) -> Result<Vec<SubmissionPackageSection>, TenderCommandError> {
    type SectionGroup = (BTreeSet<String>, BTreeSet<String>, BTreeSet<(String, u32)>);
    let mut groups: BTreeMap<(String, String, String), SectionGroup> = BTreeMap::new();
    for item in items {
        let group = groups
            .entry((
                item.view.section_key.clone(),
                item.view.envelope_key.clone(),
                item.view.language.clone(),
            ))
            .or_default();
        group.0.insert(item.view.item_id.clone());
        group.1.extend(item.view.requirement_ids.iter().cloned());
        for author in &item.view.authorship {
            if let SubmissionAuthorshipProvenance::AgentContribution {
                profile_id,
                profile_version,
                ..
            } = author
            {
                group.2.insert((profile_id.clone(), *profile_version));
            }
        }
    }
    for row in coverage.iter().filter(|row| row.item_id.is_none()) {
        groups
            .entry((
                row.requirement.section_key.clone(),
                row.requirement.envelope_key.clone(),
                row.requirement.language.clone(),
            ))
            .or_default()
            .1
            .insert(row.requirement.requirement_id.clone());
    }
    groups
        .into_iter()
        .map(
            |((section_key, envelope_key, language), (item_ids, requirement_ids, authors))| {
                let author_profile_versions = authors
                    .into_iter()
                    .map(
                        |(profile_id, profile_version)| SubmissionProfileVersionReference {
                            profile_id,
                            profile_version,
                        },
                    )
                    .collect::<Vec<_>>();
                let derivation_inputs = vec![
                    SubmissionValidationContextInput {
                        input_kind: "work_plan".into(),
                        reference_id: work_plan.plan_id.clone(),
                        version: work_plan.plan_version,
                        sha256: work_plan.plan_manifest_sha256.clone(),
                    },
                    SubmissionValidationContextInput {
                        input_kind: "generation".into(),
                        reference_id: generation.generation_id.clone(),
                        version: generation.sequence,
                        sha256: generation.manifest_sha256.clone(),
                    },
                ];
                Ok(SubmissionPackageSection {
                    section_key: section_key.clone(),
                    envelope_key,
                    language,
                    item_ids: item_ids.into_iter().collect(),
                    requirement_ids: requirement_ids.into_iter().collect(),
                    required_capabilities: capabilities_for_section(plan, &section_key),
                    risk_context: SubmissionSectionRiskContext {
                        baseline_manifest_sha256: generation.baseline_manifest_sha256.clone(),
                        risk_references: risks
                            .iter()
                            .filter(|risk| {
                                coverage.iter().any(|row| {
                                    row.requirement.section_key == section_key
                                        && row.risk_references.contains(risk)
                                })
                            })
                            .cloned()
                            .collect(),
                    },
                    independence_context: SubmissionSectionIndependenceContext {
                        independent_review_required: true,
                        author_profile_versions,
                        authorized_profile_versions: work_plan.authorized_profile_versions.clone(),
                        derivation_inputs,
                    },
                })
            },
        )
        .collect::<Result<Vec<_>, TenderCommandError>>()
        .and_then(|sections| {
            let expected = coverage
                .iter()
                .map(|row| row.requirement.requirement_id.clone())
                .collect::<BTreeSet<_>>();
            let sectioned = sections
                .iter()
                .flat_map(|section| section.requirement_ids.iter().cloned())
                .collect::<BTreeSet<_>>();
            if expected != sectioned {
                Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed))
            } else {
                Ok(sections)
            }
        })
}

fn baseline_risk_references(
    bindings: &[CoordinatedBidBaselineBinding],
) -> Vec<SubmissionPackageDependency> {
    bindings
        .iter()
        .filter(|binding| binding.category == CoordinatedBidBaselineCategory::Risk)
        .map(dependency_from_binding)
        .collect()
}

fn capabilities_for_section(plan: &PlanInputs, section_key: &str) -> Vec<String> {
    let preferred = match section_key {
        "commercial" => "cost_estimation",
        "forms" => "document_control",
        "technical" => "tender_analysis",
        other => other,
    };
    let mut capabilities = plan
        .workstreams
        .iter()
        .filter(|workstream| {
            workstream.workstream_key == preferred || workstream.capability == preferred
        })
        .map(|workstream| workstream.capability.clone())
        .collect::<Vec<_>>();
    capabilities.sort();
    capabilities.dedup();
    capabilities
}

fn risks_for_requirement(
    risks: &[SubmissionPackageDependency],
    requirement: &GenerationRequirement,
) -> Vec<SubmissionPackageDependency> {
    risks
        .iter()
        .filter(|risk| {
            risk.reference_id == requirement.record.record_id
                || requirement
                    .decision_references
                    .iter()
                    .any(|reference| reference.contains(&risk.reference_id))
                || requirement
                    .review_references
                    .iter()
                    .any(|reference| reference.contains(&risk.reference_id))
        })
        .cloned()
        .collect()
}

fn supported_package_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/pdf"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    )
}

fn dependency_from_binding(binding: &CoordinatedBidBaselineBinding) -> SubmissionPackageDependency {
    SubmissionPackageDependency {
        kind: if binding.kind == CoordinatedBidBaselineBindingKind::CalculationManifest {
            SubmissionPackageDependencyKind::CalculationManifest
        } else {
            SubmissionPackageDependencyKind::Decision
        },
        reference_id: binding.reference_id.clone(),
        version: binding.version,
        manifest_sha256: binding.manifest_sha256.clone(),
    }
}

fn decision_references_from_binding(
    binding: &CoordinatedBidBaselineBinding,
) -> Vec<SubmissionDecisionReference> {
    let mut references = Vec::with_capacity(2);
    if let Some(decision_id) = &binding.approval_id {
        references.push(SubmissionDecisionReference {
            kind: SubmissionDecisionReferenceKind::Approval,
            decision_id: decision_id.clone(),
            subject_kind: binding.kind,
            subject_reference_id: binding.reference_id.clone(),
            subject_version: binding.version,
            subject_manifest_sha256: binding.manifest_sha256.clone(),
        });
    }
    if let Some(decision_id) = &binding.supporting_review_id {
        references.push(SubmissionDecisionReference {
            kind: SubmissionDecisionReferenceKind::Review,
            decision_id: decision_id.clone(),
            subject_kind: binding.kind,
            subject_reference_id: binding.reference_id.clone(),
            subject_version: binding.version,
            subject_manifest_sha256: binding.manifest_sha256.clone(),
        });
    }
    references
}

fn load_submission_deadline(
    connection: &Connection,
    generation: &PackageProductionGeneration,
    bindings: &[CoordinatedBidBaselineBinding],
) -> Result<Option<SubmissionPackageDependency>, TenderCommandError> {
    let candidates = generation
        .requirements
        .iter()
        .map(|requirement| {
            (
                requirement.record.record_id.clone(),
                requirement.record.version,
                requirement.record.manifest_sha256.clone(),
            )
        })
        .chain(
            bindings
                .iter()
                .filter(|binding| {
                    binding.kind == CoordinatedBidBaselineBindingKind::TenderRecordVersion
                })
                .map(|binding| {
                    (
                        binding.reference_id.clone(),
                        binding.version,
                        binding.manifest_sha256.clone(),
                    )
                }),
        );
    for (record_id, version, manifest_sha256) in candidates {
        let record = inspect_tender_record_version_in_connection(connection, &record_id, version)?;
        let supporting_review_id = record.reviews.last().map(|review| review.review_id.clone());
        let immutable_record = json!({
            "record_id": record.record_id,
            "stable_key": record.stable_key,
            "version": record.version,
            "kind": record.kind,
            "title": record.title,
            "generation_instruction": record.generation_instruction,
            "fields": record.fields,
            "contradictions": record.contradictions,
            "author_run_id": record.author_run_id,
            "author_profile_id": record.author_profile_id,
            "supporting_review_id": supporting_review_id,
            "trust_class": record.trust_class,
        });
        if sha256_hex(canonical_json(&immutable_record)?.as_bytes()) != manifest_sha256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        if record.kind == TenderRecordKind::Deadline {
            return Ok(Some(SubmissionPackageDependency {
                kind: SubmissionPackageDependencyKind::TenderDeadline,
                reference_id: record_id,
                version,
                manifest_sha256,
            }));
        }
    }
    Ok(None)
}

fn package_validation_inputs(
    generation: &PackageProductionGeneration,
    baseline: &ExactBaselineContext,
    work_plan: &SubmissionWorkPlanContext,
    items: &[BuiltItem],
) -> Vec<SubmissionValidationContextInput> {
    let mut inputs = vec![
        SubmissionValidationContextInput {
            input_kind: "generation".into(),
            reference_id: generation.generation_id.clone(),
            version: generation.sequence,
            sha256: generation.manifest_sha256.clone(),
        },
        SubmissionValidationContextInput {
            input_kind: "baseline".into(),
            reference_id: generation.baseline_id.clone(),
            version: generation.baseline_version,
            sha256: generation.baseline_manifest_sha256.clone(),
        },
        SubmissionValidationContextInput {
            input_kind: "baseline_approval".into(),
            reference_id: baseline.approval_id.clone(),
            version: generation.baseline_version,
            sha256: baseline.approval_sha256.clone(),
        },
        SubmissionValidationContextInput {
            input_kind: "work_plan".into(),
            reference_id: work_plan.plan_id.clone(),
            version: work_plan.plan_version,
            sha256: work_plan.plan_manifest_sha256.clone(),
        },
        SubmissionValidationContextInput {
            input_kind: "work_plan_approval".into(),
            reference_id: work_plan.plan_approval_id.clone(),
            version: work_plan.plan_version,
            sha256: work_plan.plan_approval_sha256.clone(),
        },
        SubmissionValidationContextInput {
            input_kind: "production_activation".into(),
            reference_id: work_plan.activation_id.clone(),
            version: 1,
            sha256: work_plan.plan_manifest_sha256.clone(),
        },
    ];
    inputs.extend(items.iter().map(|item| SubmissionValidationContextInput {
        input_kind: "submission_item".into(),
        reference_id: item.view.item_id.clone(),
        version: 1,
        sha256: item.view.validation_context_sha256.clone(),
    }));
    inputs.extend(
        baseline
            .bindings
            .iter()
            .map(|binding| SubmissionValidationContextInput {
                input_kind: format!(
                    "baseline_binding:{}",
                    baseline_binding_kind_name(binding.kind)
                ),
                reference_id: binding.reference_id.clone(),
                version: binding.version,
                sha256: binding.manifest_sha256.clone(),
            }),
    );
    inputs.sort_by(|left, right| {
        (&left.input_kind, &left.reference_id, left.version).cmp(&(
            &right.input_kind,
            &right.reference_id,
            right.version,
        ))
    });
    inputs
}

fn baseline_binding_kind_name(kind: CoordinatedBidBaselineBindingKind) -> &'static str {
    match kind {
        CoordinatedBidBaselineBindingKind::ProductionArtifactVersion => "production_artifact",
        CoordinatedBidBaselineBindingKind::TenderRecordVersion => "tender_record",
        CoordinatedBidBaselineBindingKind::TenderQueryVersion => "tender_query",
        CoordinatedBidBaselineBindingKind::ExternalRfiVersion => "external_rfi",
        CoordinatedBidBaselineBindingKind::PricedCostBaseline => "priced_cost_baseline",
        CoordinatedBidBaselineBindingKind::ApprovedTenderPrice => "approved_tender_price",
        CoordinatedBidBaselineBindingKind::CalculationManifest => "calculation_manifest",
        CoordinatedBidBaselineBindingKind::CommercialStrategy => "commercial_strategy",
    }
}

fn persist_relational_package(
    transaction: &Transaction<'_>,
    package_id: &str,
    version: u32,
    items: &[BuiltItem],
    coverage: &[SubmissionCoverageRow],
    sections: &[SubmissionPackageSection],
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    for (index, item) in items.iter().enumerate() {
        budget.check()?;
        let (source_kind, source_id, source_version, source_manifest) = match &item.view.source {
            SubmissionItemSource::Generated {
                artifact_id,
                version,
                manifest_sha256,
                ..
            } => (
                "generated",
                artifact_id,
                *version,
                Some(manifest_sha256.as_str()),
            ),
            SubmissionItemSource::UnchangedSource {
                artifact_id,
                version,
                ..
            } => ("unchanged_source", artifact_id, *version, None),
        };
        transaction
            .execute(
                "INSERT INTO submission_package_items (
               package_id, package_version, ordinal, item_id, package_path, section_key,
               envelope_key, language, media_type, source_kind, source_id, source_version,
               source_manifest_sha256, content_sha256, size_bytes, content_integrity, item_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params![
                    package_id,
                    version,
                    u32::try_from(index + 1)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    item.view.item_id,
                    item.view.package_path,
                    item.view.section_key,
                    item.view.envelope_key,
                    item.view.language,
                    item.view.media_type,
                    source_kind,
                    source_id,
                    source_version,
                    source_manifest,
                    item.view.content_sha256,
                    i64::try_from(item.view.size_bytes)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    item.content_integrity,
                    canonical_json(&item.view)?
                ],
            )
            .map_err(sql_error)?;
    }
    for (index, row) in coverage.iter().enumerate() {
        budget.check()?;
        transaction.execute(
            "INSERT INTO submission_package_coverage (
               package_id, package_version, ordinal, requirement_id, disposition, item_id, coverage_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![package_id, version, u32::try_from(index + 1).map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?, row.requirement.requirement_id, row.disposition.as_str(), row.item_id, canonical_json(row)?],
        ).map_err(sql_error)?;
    }
    for (index, row) in coverage
        .iter()
        .filter(|row| row.item_id.is_none())
        .enumerate()
    {
        budget.check()?;
        transaction
            .execute(
                "INSERT INTO submission_package_uncovered_requirements (
                   package_id, package_version, ordinal, requirement_id, section_key,
                   envelope_key, language, requirement_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    package_id,
                    version,
                    u32::try_from(index + 1)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    row.requirement.requirement_id,
                    row.requirement.section_key,
                    row.requirement.envelope_key,
                    row.requirement.language,
                    canonical_json(&row.requirement)?,
                ],
            )
            .map_err(sql_error)?;
    }
    for (index, section) in sections.iter().enumerate() {
        budget.check()?;
        transaction.execute(
            "INSERT INTO submission_package_sections (
               package_id, package_version, ordinal, section_key, envelope_key, language, section_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![package_id, version, u32::try_from(index + 1).map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?, section.section_key, section.envelope_key, section.language, canonical_json(section)?],
        ).map_err(sql_error)?;
    }
    Ok(())
}

pub(crate) fn advance_submission_package_head(
    transaction: &Transaction<'_>,
    package_id: &str,
    version: u32,
) -> Result<(), TenderCommandError> {
    transaction
        .execute(
            "INSERT INTO submission_package_head (singleton, package_id, current_version)
             VALUES (1, ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET current_version = excluded.current_version",
            params![package_id, version],
        )
        .map_err(sql_error)?;
    Ok(())
}

pub(crate) fn load_exact_submission_package_for_transaction(
    transaction: &Transaction<'_>,
    package_id: &str,
    version: u32,
    manifest_sha256: &str,
    budget: BidPackageOperationBudget,
) -> Result<ExactSubmissionPackage, TenderCommandError> {
    load_stored_package(
        transaction,
        package_id,
        version,
        manifest_sha256,
        true,
        budget,
    )
}

pub(crate) fn load_submission_package_for_review_transaction(
    transaction: &Transaction<'_>,
    package_id: &str,
    version: u32,
    manifest_sha256: &str,
    require_current: bool,
    budget: BidPackageOperationBudget,
) -> Result<ExactSubmissionPackage, TenderCommandError> {
    load_stored_package(
        transaction,
        package_id,
        version,
        manifest_sha256,
        require_current,
        budget,
    )
}

pub(crate) fn load_submission_package_snapshot_for_integrity(
    connection: &Connection,
    package_id: &str,
    version: u32,
    manifest_sha256: &str,
) -> Result<SubmissionPackageVersion, TenderCommandError> {
    load_stored_package(
        connection,
        package_id,
        version,
        manifest_sha256,
        false,
        BidPackageOperationBudget::from_connection(connection)?,
    )
    .map(|exact| exact.package)
}

pub(crate) fn load_exact_submission_package_item_bytes_for_transaction(
    content_root: &Path,
    item: &ExactSubmissionPackageItem,
) -> Result<Vec<u8>, TenderCommandError> {
    read_and_verify_content(
        content_root,
        &item.content_integrity,
        &item.item.content_sha256,
        item.item.size_bytes,
    )
}

fn load_stored_package(
    connection: &Connection,
    package_id: &str,
    version: u32,
    manifest_sha256: &str,
    require_current: bool,
    budget: BidPackageOperationBudget,
) -> Result<ExactSubmissionPackage, TenderCommandError> {
    type StoredPackageRow = (
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
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        i64,
        String,
        u32,
    );
    let row: StoredPackageRow = connection
        .query_row(
            "SELECT status, assessment, generation_id, generation_sequence,
                    generation_manifest_sha256, baseline_id, baseline_version,
                    baseline_manifest_sha256, baseline_approval_id, work_plan_json,
                    calculation_references_json, decision_references_json, deadline_json,
                    validation_context_inputs_json, validation_context_sha256,
                    dependency_currentness_sha256, manifest_json, audit_sequence, created_at,
                    tender_revision
             FROM submission_package_versions
             WHERE package_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
            params![package_id, version, manifest_sha256],
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
                ))
            },
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if sha256_hex(row.16.as_bytes()) != manifest_sha256 || row.16.len() > MAX_MANIFEST_BYTES {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let manifest: SubmissionPackageManifest = parse_canonical(&row.16)?;
    let generation_requirements_json: String = connection
        .query_row(
            "SELECT requirements_json FROM submission_generations
             WHERE generation_id = ?1 AND generation_sequence = ?2 AND manifest_sha256 = ?3",
            params![
                manifest.generation_id,
                manifest.generation_sequence,
                manifest.generation_manifest_sha256
            ],
            |generation| generation.get(0),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let generation_requirements: Vec<GenerationRequirement> =
        parse_canonical(&generation_requirements_json)?;
    let item_rows = load_item_rows(connection, package_id, version)?;
    let coverage = load_coverage_rows(connection, package_id, version, &item_rows)?;
    let uncovered = load_uncovered_requirement_rows(connection, package_id, version)?;
    let sections = load_section_rows(connection, package_id, version)?;
    let expected_requirement_ids = generation_requirements
        .iter()
        .map(|requirement| requirement.requirement_id.as_str())
        .collect::<BTreeSet<_>>();
    let actual_requirement_ids = coverage
        .iter()
        .map(|coverage| coverage.requirement.requirement_id.as_str())
        .collect::<BTreeSet<_>>();
    let derived_assessment = if coverage.iter().all(|coverage| {
        coverage.disposition == SubmissionCoverageDisposition::Covered
            && coverage.item_id.is_some()
            && coverage.blockers.is_empty()
    }) {
        SubmissionPackageAssessment::Complete
    } else {
        SubmissionPackageAssessment::Blocked
    };
    if manifest.schema_version != 1
        || manifest.package_id != package_id
        || manifest.version != version
        || manifest.tender_revision != row.19
        || manifest.status != SubmissionPackageStatus::parse(&row.0)?
        || manifest.assessment != SubmissionPackageAssessment::parse(&row.1)?
        || manifest.generation_id != row.2
        || manifest.generation_sequence != row.3
        || manifest.generation_manifest_sha256 != row.4
        || manifest.baseline_id != row.5
        || manifest.baseline_version != row.6
        || manifest.baseline_manifest_sha256 != row.7
        || manifest.baseline_approval_id != row.8
        || manifest.work_plan != parse_canonical(&row.9)?
        || manifest.calculation_manifest_references
            != parse_canonical::<Vec<SubmissionPackageDependency>>(&row.10)?
        || manifest.current_decision_references
            != parse_canonical::<Vec<SubmissionDecisionReference>>(&row.11)?
        || manifest.submission_deadline != row.12.as_deref().map(parse_canonical).transpose()?
        || manifest.validation_context_inputs
            != parse_canonical::<Vec<SubmissionValidationContextInput>>(&row.13)?
        || manifest.validation_context_sha256 != row.14
        || manifest.dependency_currentness_sha256 != row.15
        || manifest.created_at != row.18
        || manifest.items
            != item_rows
                .iter()
                .map(|item| item.item.clone())
                .collect::<Vec<_>>()
        || manifest.coverage != coverage
        || expected_requirement_ids.len() != generation_requirements.len()
        || actual_requirement_ids.len() != coverage.len()
        || expected_requirement_ids != actual_requirement_ids
        || coverage.iter().any(|coverage| {
            generation_requirements.iter().find(|requirement| {
                requirement.requirement_id == coverage.requirement.requirement_id
            }) != Some(&coverage.requirement)
                || coverage.item_id.as_ref().is_some_and(|item_id| {
                    item_rows.iter().all(|item| {
                        item.item.item_id != *item_id
                            || !item
                                .item
                                .requirement_ids
                                .contains(&coverage.requirement.requirement_id)
                    })
                })
                || (coverage.disposition == SubmissionCoverageDisposition::Covered
                    && coverage.item_id.is_none())
        })
        || manifest.assessment != derived_assessment
        || uncovered
            != manifest
                .coverage
                .iter()
                .filter(|row| row.item_id.is_none())
                .map(|row| row.requirement.clone())
                .collect::<Vec<_>>()
        || manifest.sections != sections
        || canonical_sha256(&manifest.validation_context_inputs)?
            != manifest.validation_context_sha256
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let audit_valid: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM audit_events
               WHERE sequence = ?1 AND event_type = 'submission_package_assembled'
                 AND aggregate_revision = ?6
                 AND created_at = ?2
                 AND json_extract(payload_json, '$.change.package_id') = ?3
                 AND json_extract(payload_json, '$.change.version') = ?4
                 AND json_extract(payload_json, '$.change.manifest_sha256') = ?5
                 AND json_extract(payload_json, '$.change.assessment') = ?7
             )",
            params![
                row.17,
                manifest.created_at,
                package_id,
                version.to_string(),
                manifest_sha256,
                manifest.tender_revision,
                manifest.assessment.as_str(),
            ],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if !audit_valid {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let currentness_facts = package_currentness_facts(connection, &manifest, &item_rows, budget)?;
    let current = currentness_facts.iter().all(|fact| fact.current);
    if require_current && !current {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(ExactSubmissionPackage {
        package: SubmissionPackageVersion {
            package_id: manifest.package_id,
            version: manifest.version,
            tender_revision: manifest.tender_revision,
            status: manifest.status,
            assessment: manifest.assessment,
            current,
            currentness_facts,
            generation_id: manifest.generation_id,
            generation_sequence: manifest.generation_sequence,
            generation_manifest_sha256: manifest.generation_manifest_sha256,
            baseline_id: manifest.baseline_id,
            baseline_version: manifest.baseline_version,
            baseline_manifest_sha256: manifest.baseline_manifest_sha256,
            baseline_approval_id: manifest.baseline_approval_id,
            work_plan: manifest.work_plan,
            calculation_manifest_references: manifest.calculation_manifest_references,
            current_decision_references: manifest.current_decision_references,
            submission_deadline: manifest.submission_deadline,
            sections: manifest.sections,
            items: manifest.items,
            coverage: manifest.coverage,
            validation_context_inputs: manifest.validation_context_inputs,
            validation_context_sha256: manifest.validation_context_sha256,
            dependency_currentness_sha256: manifest.dependency_currentness_sha256,
            manifest_bytes: row.16.into_bytes(),
            manifest_sha256: manifest_sha256.to_owned(),
            created_at: manifest.created_at,
        },
        items: item_rows,
    })
}

fn load_item_rows(
    connection: &Connection,
    package_id: &str,
    version: u32,
) -> Result<Vec<ExactSubmissionPackageItem>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT item_json, content_integrity, content_sha256, size_bytes,
                    item_id, package_path, section_key, envelope_key, language, media_type,
                    source_kind, source_id, source_version, source_manifest_sha256
             FROM submission_package_items
             WHERE package_id = ?1 AND package_version = ?2 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![package_id, version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, u32>(12)?,
                row.get::<_, Option<String>>(13)?,
            ))
        })
        .map_err(sql_error)?;
    let mut items = Vec::new();
    for row in rows {
        let row = row.map_err(sql_error)?;
        let item: SubmissionPackageItem = parse_canonical(&row.0)?;
        let expected_source = match row.10.as_str() {
            "generated" => SubmissionItemSource::Generated {
                artifact_id: row.11,
                version: row.12,
                manifest_sha256: row
                    .13
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                content_sha256: row.2.clone(),
                size_bytes: u64::try_from(row.3)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            },
            "unchanged_source" if row.13.is_none() => SubmissionItemSource::UnchangedSource {
                artifact_id: row.11,
                version: row.12,
                content_sha256: row.2.clone(),
                size_bytes: u64::try_from(row.3)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            },
            _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        };
        if item.item_id != row.4
            || item.package_path != row.5
            || item.section_key != row.6
            || item.envelope_key != row.7
            || item.language != row.8
            || item.media_type != row.9
            || item.content_sha256 != row.2
            || item.source != expected_source
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        validate_package_path(&item.package_path)?;
        items.push(ExactSubmissionPackageItem {
            item,
            content_integrity: row.1,
        });
    }
    Ok(items)
}

fn load_coverage_rows(
    connection: &Connection,
    package_id: &str,
    version: u32,
    item_rows: &[ExactSubmissionPackageItem],
) -> Result<Vec<SubmissionCoverageRow>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT requirement_id, disposition, item_id, coverage_json
         FROM submission_package_coverage
         WHERE package_id = ?1 AND package_version = ?2 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![package_id, version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(sql_error)?;
    let mut coverage = Vec::new();
    for row in rows {
        let (requirement_id, disposition, item_id, json) = row.map_err(sql_error)?;
        let value: SubmissionCoverageRow = parse_canonical(&json)?;
        let blocker_shape_is_exact = value.blockers.iter().all(|blocker| {
            blocker.requirement_id == value.requirement.requirement_id
                && match value.disposition {
                    SubmissionCoverageDisposition::Covered => false,
                    SubmissionCoverageDisposition::Missing => {
                        blocker.code == SubmissionCoverageBlockerCode::MissingItem
                    }
                    SubmissionCoverageDisposition::Unsupported => matches!(
                        blocker.code,
                        SubmissionCoverageBlockerCode::UnsupportedAuthoring
                    ),
                }
        });
        let disposition_shape_is_exact = match value.disposition {
            SubmissionCoverageDisposition::Covered => {
                value.requirement.availability == GenerationRequirementAvailability::Available
                    && value.item_id.is_some()
                    && value.blockers.is_empty()
            }
            SubmissionCoverageDisposition::Missing => {
                value.requirement.availability == GenerationRequirementAvailability::Missing
                    && value.item_id.is_none()
                    && value.blockers.len() == 1
                    && blocker_shape_is_exact
            }
            SubmissionCoverageDisposition::Unsupported => {
                value.requirement.availability == GenerationRequirementAvailability::Unsupported
                    && value.item_id.is_none()
                    && value.blockers.len() == 1
                    && blocker_shape_is_exact
            }
        };
        let expected_manual_validation = value
            .item_id
            .as_ref()
            .and_then(|item_id| item_rows.iter().find(|item| item.item.item_id == *item_id))
            .is_some_and(|item| {
                matches!(
                    &item.item.source,
                    SubmissionItemSource::UnchangedSource { .. }
                )
            });
        if value.requirement.requirement_id != requirement_id
            || value.disposition.as_str() != disposition
            || value.item_id != item_id
            || !disposition_shape_is_exact
            || value.manual_validation_required != expected_manual_validation
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        coverage.push(value);
    }
    Ok(coverage)
}

fn load_section_rows(
    connection: &Connection,
    package_id: &str,
    version: u32,
) -> Result<Vec<SubmissionPackageSection>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT section_key, envelope_key, language, section_json
         FROM submission_package_sections
         WHERE package_id = ?1 AND package_version = ?2 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![package_id, version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(sql_error)?;
    let mut sections = Vec::new();
    for row in rows {
        let (section_key, envelope_key, language, json) = row.map_err(sql_error)?;
        let value: SubmissionPackageSection = parse_canonical(&json)?;
        if value.section_key != section_key
            || value.envelope_key != envelope_key
            || value.language != language
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        sections.push(value);
    }
    Ok(sections)
}

fn load_uncovered_requirement_rows(
    connection: &Connection,
    package_id: &str,
    version: u32,
) -> Result<Vec<GenerationRequirement>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT requirement_id, section_key, envelope_key, language, requirement_json
             FROM submission_package_uncovered_requirements
             WHERE package_id = ?1 AND package_version = ?2 ORDER BY ordinal",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![package_id, version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(sql_error)?;
    let mut requirements = Vec::new();
    for row in rows {
        let (requirement_id, section_key, envelope_key, language, json) = row.map_err(sql_error)?;
        let requirement: GenerationRequirement = parse_canonical(&json)?;
        if requirement.requirement_id != requirement_id
            || requirement.section_key != section_key
            || requirement.envelope_key != envelope_key
            || requirement.language != language
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        requirements.push(requirement);
    }
    Ok(requirements)
}

fn package_currentness_facts(
    connection: &Connection,
    manifest: &SubmissionPackageManifest,
    items: &[ExactSubmissionPackageItem],
    budget: BidPackageOperationBudget,
) -> Result<Vec<SubmissionPackageCurrentnessFact>, TenderCommandError> {
    let head_matches: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM submission_package_head WHERE singleton = 1 AND package_id = ?1 AND current_version = ?2)",
        params![manifest.package_id, manifest.version],
        |row| row.get(0),
    ).map_err(sql_error)?;
    let generation_matches: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM submission_generations WHERE generation_id = ?1 AND generation_sequence = ?2 AND manifest_sha256 = ?3 AND generation_sequence = (SELECT MAX(generation_sequence) FROM submission_generations))",
        params![manifest.generation_id, manifest.generation_sequence, manifest.generation_manifest_sha256],
        |row| row.get(0),
    ).map_err(sql_error)?;
    let baseline_matches = exact_approved_coordinated_baseline_is_current_in_connection(
        connection,
        &manifest.baseline_id,
        manifest.baseline_version,
        &manifest.baseline_manifest_sha256,
        &manifest.baseline_approval_id,
        budget,
    )?;
    let plan_matches: bool = connection.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM work_plan_heads AS head
           JOIN work_plan_versions AS versions ON versions.plan_id = head.plan_id AND versions.version = head.current_version
           JOIN work_plan_approvals AS approvals ON approvals.plan_id = versions.plan_id AND approvals.plan_version = versions.version
           JOIN production_activations AS activations ON activations.activation_id = ?6
           WHERE versions.plan_id = ?1 AND versions.version = ?2 AND versions.manifest_sha256 = ?3
             AND approvals.approval_id = ?4 AND approvals.approval_sha256 = ?5
             AND approvals.decision = 'approve' AND activations.plan_id = versions.plan_id
             AND activations.plan_version = versions.version AND activations.status = 'active')",
        params![manifest.work_plan.plan_id, manifest.work_plan.plan_version, manifest.work_plan.plan_manifest_sha256, manifest.work_plan.plan_approval_id, manifest.work_plan.plan_approval_sha256, manifest.work_plan.activation_id],
        |row| row.get(0),
    ).map_err(sql_error)?;
    let affected_change: bool = connection
        .query_row(
            "SELECT EXISTS(
           SELECT 1 FROM change_assessments AS assessments
           LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
           WHERE resolutions.assessment_id IS NULL
             AND (
               (assessments.baseline_id = ?1 AND assessments.baseline_version = ?2)
               OR EXISTS(
                 SELECT 1 FROM change_assessment_impacts AS impacts
                 WHERE impacts.assessment_id = assessments.assessment_id
                   AND impacts.kind = 'coordinated_baseline'
                   AND impacts.object_id = ?1 AND impacts.object_version = ?2
               )
             )
         )",
            params![manifest.baseline_id, manifest.baseline_version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let mut source_rows_match = true;
    for item in items {
        if !source_row_matches(connection, item)? {
            source_rows_match = false;
        }
    }
    let currentness_sha256 = canonical_sha256(&json!({
        "inputs": manifest.validation_context_inputs,
        "item_validation_roots": items.iter().map(|item| item.item.validation_context_sha256.clone()).collect::<Vec<_>>(),
    }))?;
    Ok(vec![
        SubmissionPackageCurrentnessFact {
            code: SubmissionPackageCurrentnessCode::HeadAdvanced,
            current: head_matches,
            reference_id: manifest.package_id.clone(),
            expected_value: format!("{}:{}", manifest.package_id, manifest.version),
            actual_value: head_matches
                .then(|| format!("{}:{}", manifest.package_id, manifest.version)),
        },
        SubmissionPackageCurrentnessFact {
            code: SubmissionPackageCurrentnessCode::GenerationAdvanced,
            current: generation_matches,
            reference_id: manifest.generation_id.clone(),
            expected_value: manifest.generation_manifest_sha256.clone(),
            actual_value: generation_matches.then(|| manifest.generation_manifest_sha256.clone()),
        },
        SubmissionPackageCurrentnessFact {
            code: SubmissionPackageCurrentnessCode::BaselineChanged,
            current: baseline_matches,
            reference_id: manifest.baseline_id.clone(),
            expected_value: manifest.baseline_manifest_sha256.clone(),
            actual_value: baseline_matches.then(|| manifest.baseline_manifest_sha256.clone()),
        },
        SubmissionPackageCurrentnessFact {
            code: SubmissionPackageCurrentnessCode::WorkPlanChanged,
            current: plan_matches,
            reference_id: manifest.work_plan.plan_id.clone(),
            expected_value: manifest.work_plan.plan_manifest_sha256.clone(),
            actual_value: plan_matches.then(|| manifest.work_plan.plan_manifest_sha256.clone()),
        },
        SubmissionPackageCurrentnessFact {
            code: SubmissionPackageCurrentnessCode::ChangePending,
            current: !affected_change,
            reference_id: manifest.package_id.clone(),
            expected_value: "unaffected".into(),
            actual_value: (!affected_change).then(|| "unaffected".into()),
        },
        SubmissionPackageCurrentnessFact {
            code: SubmissionPackageCurrentnessCode::SourceChanged,
            current: source_rows_match,
            reference_id: manifest.package_id.clone(),
            expected_value: manifest.validation_context_sha256.clone(),
            actual_value: source_rows_match.then(|| manifest.validation_context_sha256.clone()),
        },
        SubmissionPackageCurrentnessFact {
            code: SubmissionPackageCurrentnessCode::DependencyDigestChanged,
            current: currentness_sha256 == manifest.dependency_currentness_sha256,
            reference_id: manifest.package_id.clone(),
            expected_value: manifest.dependency_currentness_sha256.clone(),
            actual_value: Some(currentness_sha256),
        },
    ])
}

fn source_row_matches(
    connection: &Connection,
    item: &ExactSubmissionPackageItem,
) -> Result<bool, TenderCommandError> {
    match &item.item.source {
        SubmissionItemSource::Generated {
            artifact_id,
            version,
            manifest_sha256,
            content_sha256,
            size_bytes,
        } => connection
            .query_row(
                "SELECT EXISTS(
               SELECT 1 FROM submission_artifact_versions AS versions
               JOIN submission_artifact_heads AS heads
                 ON heads.artifact_id = versions.artifact_id
                AND heads.current_version = versions.version
               JOIN content_objects AS objects ON objects.sha256 = versions.content_sha256
               WHERE versions.artifact_id = ?1 AND versions.version = ?2
                 AND versions.manifest_sha256 = ?3 AND versions.content_sha256 = ?4
                 AND versions.size_bytes = ?5 AND objects.integrity = ?6)",
                params![
                    artifact_id,
                    version,
                    manifest_sha256,
                    content_sha256,
                    i64::try_from(*size_bytes)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    item.content_integrity
                ],
                |row| row.get(0),
            )
            .map_err(sql_error),
        SubmissionItemSource::UnchangedSource {
            artifact_id,
            version,
            content_sha256,
            size_bytes,
        } => connection
            .query_row(
                "SELECT EXISTS(
               SELECT 1 FROM source_artifact_versions AS versions
               JOIN content_objects AS objects ON objects.sha256 = versions.sha256
               WHERE versions.artifact_id = ?1 AND versions.version = ?2
                 AND versions.sha256 = ?3 AND versions.size_bytes = ?4
                 AND versions.registration_state = 'registered' AND objects.integrity = ?5
                 AND NOT EXISTS(
                   SELECT 1 FROM source_relationships AS relationships
                   WHERE relationships.prior_artifact_id = versions.artifact_id
                     AND relationships.prior_version = versions.version
                     AND relationships.relationship_kind = 'replacement'
                 ))",
                params![
                    artifact_id,
                    version,
                    content_sha256,
                    i64::try_from(*size_bytes)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                    item.content_integrity
                ],
                |row| row.get(0),
            )
            .map_err(sql_error),
    }
}

fn source_validation_input(source: &SubmissionItemSource) -> SubmissionValidationContextInput {
    match source {
        SubmissionItemSource::Generated {
            artifact_id,
            version,
            manifest_sha256,
            ..
        } => SubmissionValidationContextInput {
            input_kind: "generated_artifact".into(),
            reference_id: artifact_id.clone(),
            version: *version,
            sha256: manifest_sha256.clone(),
        },
        SubmissionItemSource::UnchangedSource {
            artifact_id,
            version,
            content_sha256,
            ..
        } => SubmissionValidationContextInput {
            input_kind: "unchanged_source_artifact".into(),
            reference_id: artifact_id.clone(),
            version: *version,
            sha256: content_sha256.clone(),
        },
    }
}

fn read_and_verify_content(
    content_root: &Path,
    integrity: &str,
    expected_sha256: &str,
    expected_size: u64,
) -> Result<Vec<u8>, TenderCommandError> {
    if expected_size == 0 || expected_size > MAX_ITEM_BYTES {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let integrity = integrity
        .parse::<cacache::Integrity>()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let reader =
        cacache::SyncReader::open_hash(content_root, integrity).map_err(content_store_error)?;
    let mut bytes = Vec::with_capacity(expected_size as usize);
    reader
        .take(expected_size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if bytes.len() as u64 != expected_size || sha256_hex(&bytes) != expected_sha256 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(bytes)
}

fn verify_content_streaming(
    content_root: &Path,
    integrity: &str,
    expected_sha256: &str,
    expected_size: u64,
) -> Result<(), TenderCommandError> {
    if expected_size == 0 || expected_size > MAX_ITEM_BYTES {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let integrity = integrity
        .parse::<cacache::Integrity>()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut reader =
        cacache::SyncReader::open_hash(content_root, integrity).map_err(content_store_error)?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .filter(|size| *size <= expected_size)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        digest.update(&buffer[..read]);
    }
    let actual = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if size != expected_size || actual != expected_sha256 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn validate_package_path(package_path: &str) -> Result<(), TenderCommandError> {
    let components = Path::new(package_path).components().collect::<Vec<_>>();
    if package_path.is_empty()
        || package_path.len() > 1_000
        || package_path.contains('\\')
        || Path::new(package_path).is_absolute()
        || components.len() != 2
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

fn package_collision_key(package_path: &str) -> String {
    let compatibility_normalized = package_path.nfkc().collect::<String>();
    UniCase::unicode(compatibility_normalized)
        .to_folded_case()
        .nfkc()
        .collect()
}

fn submission_item_id(
    normalized_path: &str,
    source: &SubmissionItemSource,
    content_sha256: &str,
    size_bytes: u64,
) -> Result<String, TenderCommandError> {
    canonical_sha256(&json!({
        "kind": "submission_package_item",
        "normalized_path": normalized_path,
        "source": source,
        "content_sha256": content_sha256,
        "size_bytes": size_bytes,
    }))
}

fn union_strings<'a>(values: impl Iterator<Item = &'a String>) -> Vec<String> {
    values
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn canonical_sha256<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    Ok(sha256_hex(canonical_json(value)?.as_bytes()))
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn parse_canonical<T: for<'de> Deserialize<'de>>(value: &str) -> Result<T, TenderCommandError> {
    let parsed: Value = serde_json::from_str(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&parsed)? != value {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    serde_json::from_value(parsed)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_paths_use_nfkc_and_full_unicode_case_folding() {
        assert_eq!(
            package_collision_key("01-Technical/Ｍaße.docx"),
            package_collision_key("01-technical/masse.docx")
        );
    }

    #[test]
    fn package_paths_reject_traversal_and_platform_separators() {
        assert!(validate_package_path("../escape.docx").is_err());
        assert!(validate_package_path("folder\\escape.docx").is_err());
        assert!(validate_package_path("folder/nested/escape.docx").is_err());
        assert!(validate_package_path("01-Technical/offer.docx").is_ok());
    }

    #[test]
    fn changed_exact_bytes_change_the_content_addressed_item_id() {
        let source = |content_sha256: &str| SubmissionItemSource::Generated {
            artifact_id: "artifact-1".into(),
            version: 1,
            manifest_sha256: "c".repeat(64),
            content_sha256: content_sha256.into(),
            size_bytes: 1,
        };
        let first_sha256 = "a".repeat(64);
        let changed_sha256 = "b".repeat(64);

        assert_ne!(
            submission_item_id(
                "01-Technical/offer.docx",
                &source(&first_sha256),
                &first_sha256,
                1,
            )
            .expect("first content-addressed item ID"),
            submission_item_id(
                "01-Technical/offer.docx",
                &source(&changed_sha256),
                &changed_sha256,
                1,
            )
            .expect("changed content-addressed item ID")
        );
    }
}
