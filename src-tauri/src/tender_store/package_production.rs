use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Cursor,
    path::{Component, Path},
};

use docx_rs::{AlignmentType, Docx, Paragraph, Run, RunFonts};
use garde::Validate;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use rust_xlsxwriter::{DocProperties, ExcelDateTime, Format, Workbook};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;
use unicase::UniCase;
use unicode_normalization::UnicodeNormalization;

use crate::agent_runtime::VerificationStatus;

use super::tender_records::{
    GenerationAuthoringMode, GenerationRequirementKind, TenderRecordGenerationInstruction,
};

use super::{
    append_audit_event_with_sequence, content_store_error, lock_mutex_with_check,
    random_identifier, sha256_hex, sql_error, sqlite_timestamp, store_unavailable,
    BidPackageOperationBudget, CoordinatedBidBaselineBinding, CoordinatedBidBaselineBindingKind,
    CoordinatedBidBaselineCategory, CoordinatedBidBaselineDecision, QuantixHost,
    TenderCommandError, TenderErrorCode, TenderId, TenderRecordAuthorityKind,
    TenderRecordBasisKind, TenderRecordEvidence, TenderRecordField, TenderRecordTrustClass,
    TenderStore,
};

const GENERATION_POLICY_ID: &str = "quantix-controlled-office-open-xml";
const GENERATION_POLICY_VERSION: u32 = 1;
const MAX_ARTIFACTS: usize = 128;
const MAX_REQUIREMENTS: usize = 4_096;
const MAX_GENERATIONS: u32 = 32;
const MAX_GENERATED_BYTES: usize = 16 * 1024 * 1024;
const MAX_TOTAL_GENERATED_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct GenerateSubmissionSectionsCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub baseline_id: String,
    #[garde(range(min = 1, max = 32))]
    pub baseline_version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub baseline_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectPackageProductionCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectSubmissionArtifactContentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub artifact_id: String,
    #[garde(range(min = 1, max = 32))]
    pub version: u32,
    #[garde(length(bytes, min = 64, max = 64))]
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionArtifactContent {
    pub artifact_id: String,
    pub version: u32,
    pub media_type: String,
    pub content_sha256: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GenerationRequirementRecordReference {
    pub record_id: String,
    pub version: u32,
    pub manifest_sha256: String,
    pub stable_key: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionGeneratedArtifactReference {
    pub artifact_id: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionSourceArtifactReference {
    pub artifact_id: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GenerationRequirement {
    pub requirement_id: String,
    pub kind: GenerationRequirementKind,
    pub record: GenerationRequirementRecordReference,
    pub evidence: Vec<TenderRecordEvidence>,
    pub authored_fields: Vec<TenderRecordField>,
    pub mandatory: bool,
    pub section_key: String,
    pub package_path: String,
    pub envelope_key: String,
    pub language: String,
    pub authoring_mode: GenerationAuthoringMode,
    pub availability: GenerationRequirementAvailability,
    pub generated_artifact: Option<SubmissionGeneratedArtifactReference>,
    pub unchanged_source_artifact: Option<SubmissionSourceArtifactReference>,
    pub content_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub calculation_references: Vec<String>,
    pub review_references: Vec<String>,
    pub decision_references: Vec<String>,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GenerationRequirementAvailability {
    Available,
    Missing,
    Unsupported,
}

impl GenerationRequirementAvailability {
    fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Missing => "missing",
            Self::Unsupported => "unsupported",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "available" => Ok(Self::Available),
            "missing" => Ok(Self::Missing),
            "unsupported" => Ok(Self::Unsupported),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubmissionArtifactVersion {
    pub artifact_id: String,
    pub version: u32,
    pub baseline_id: String,
    pub baseline_version: u32,
    pub baseline_manifest_sha256: String,
    pub section_key: String,
    pub package_path: String,
    pub envelope_key: String,
    pub language: String,
    pub authoring_mode: GenerationAuthoringMode,
    pub media_type: String,
    pub classifications: Vec<GenerationRequirementKind>,
    pub scope_record_ids: Vec<String>,
    pub content_sha256: String,
    pub size_bytes: u64,
    pub exact_inputs: Vec<String>,
    pub generation_policy_id: String,
    pub generation_policy_version: u32,
    pub generation_policy_sha256: String,
    pub provenance: Vec<String>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageProductionGeneration {
    pub generation_id: String,
    pub sequence: u32,
    pub baseline_id: String,
    pub baseline_version: u32,
    pub baseline_manifest_sha256: String,
    pub artifact_versions: Vec<SubmissionArtifactVersion>,
    pub requirements: Vec<GenerationRequirement>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArtifactManifest {
    schema_version: u32,
    artifact_id: String,
    version: u32,
    generation_id: String,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    section_key: String,
    package_path: String,
    envelope_key: String,
    language: String,
    authoring_mode: GenerationAuthoringMode,
    media_type: String,
    classifications: Vec<GenerationRequirementKind>,
    scope_record_ids: Vec<String>,
    content_sha256: String,
    size_bytes: u64,
    exact_inputs: Vec<String>,
    generation_policy_id: String,
    generation_policy_version: u32,
    generation_policy_sha256: String,
    provenance: Vec<String>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RequirementManifest {
    schema_version: u32,
    requirement_id: String,
    kind: GenerationRequirementKind,
    record: GenerationRequirementRecordReference,
    evidence: Vec<TenderRecordEvidence>,
    authored_fields: Vec<TenderRecordField>,
    mandatory: bool,
    section_key: String,
    package_path: String,
    envelope_key: String,
    language: String,
    authoring_mode: GenerationAuthoringMode,
    availability: GenerationRequirementAvailability,
    generated_artifact: Option<SubmissionGeneratedArtifactReference>,
    unchanged_source_artifact: Option<SubmissionSourceArtifactReference>,
    content_sha256: Option<String>,
    size_bytes: Option<u64>,
    calculation_references: Vec<String>,
    review_references: Vec<String>,
    decision_references: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GenerationManifest {
    schema_version: u32,
    generation_id: String,
    sequence: u32,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    artifact_versions: Vec<SubmissionArtifactVersion>,
    requirements: Vec<GenerationRequirement>,
    created_at: String,
}

#[derive(Debug, Clone)]
struct BaselineInput {
    bindings: Vec<CoordinatedBidBaselineBinding>,
    approval_id: String,
}

#[derive(Debug, Clone)]
struct ApprovedPriceInput {
    amount: String,
    currency: String,
    calculation_run_id: String,
    calculation_manifest_sha256: String,
}

#[derive(Debug, Clone)]
struct RecordRequirementDraft {
    kind: GenerationRequirementKind,
    record: GenerationRequirementRecordReference,
    fields: Vec<TenderRecordField>,
    evidence: Vec<TenderRecordEvidence>,
    mandatory: bool,
    section_key: String,
    package_path: String,
    envelope_key: String,
    language: String,
    authoring_mode: GenerationAuthoringMode,
}

#[derive(Debug)]
struct ArtifactCandidate {
    section_key: String,
    package_path: String,
    envelope_key: String,
    language: String,
    authoring_mode: GenerationAuthoringMode,
    media_type: String,
    classifications: Vec<GenerationRequirementKind>,
    scope_record_ids: Vec<String>,
    bytes: Vec<u8>,
    content_sha256: String,
    integrity: String,
    exact_inputs: Vec<String>,
    provenance: Vec<String>,
}

struct StoredGenerationRow {
    sequence: u32,
    generation_id: String,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    artifact_versions_json: String,
    requirements_json: String,
    audit_sequence: i64,
    manifest_json: String,
    manifest_sha256: String,
    created_at: String,
}

struct StoredArtifactRow {
    artifact_id: String,
    version: u32,
    baseline_id: String,
    baseline_version: u32,
    baseline_manifest_sha256: String,
    section_key: String,
    package_path: String,
    envelope_key: String,
    language: String,
    authoring_mode: String,
    media_type: String,
    classifications_json: String,
    scope_record_ids_json: String,
    content_sha256: String,
    size_bytes: i64,
    exact_inputs_json: String,
    generation_policy_id: String,
    generation_policy_version: u32,
    generation_policy_sha256: String,
    provenance_json: String,
    manifest_json: String,
    manifest_sha256: String,
    created_at: String,
    stable_key: String,
}

struct StoredRequirementRow {
    ordinal: u32,
    kind: String,
    record_id: String,
    record_version: u32,
    record_manifest_sha256: String,
    record_stable_key: String,
    record_title: String,
    evidence_json: String,
    mandatory: bool,
    section_key: String,
    package_path: String,
    envelope_key: String,
    language: String,
    authoring_mode: String,
    availability: String,
    generated_artifact_id: Option<String>,
    generated_artifact_version: Option<u32>,
    source_artifact_id: Option<String>,
    source_artifact_version: Option<u32>,
    content_sha256: Option<String>,
    size_bytes: Option<i64>,
    calculation_references_json: String,
    review_references_json: String,
    decision_references_json: String,
    authored_fields_json: String,
    manifest_json: String,
    manifest_sha256: String,
    created_at: String,
}

struct StagingDirectory(std::path::PathBuf);

impl StagingDirectory {
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl QuantixHost {
    pub fn generate_submission_sections(
        &self,
        command: GenerateSubmissionSectionsCommand,
    ) -> Result<PackageProductionGeneration, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .generate_submission_sections(&tender_id, &command, budget);
        result
    }

    pub fn inspect_package_production(
        &self,
        command: InspectPackageProductionCommand,
    ) -> Result<Option<PackageProductionGeneration>, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_package_production(budget);
        result
    }

    pub fn inspect_submission_artifact_content(
        &self,
        command: InspectSubmissionArtifactContentCommand,
    ) -> Result<SubmissionArtifactContent, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_submission_artifact_content(&command, budget);
        result
    }
}

impl TenderStore {
    pub(crate) fn load_exact_current_package_production_generation_in_transaction(
        transaction: &Transaction<'_>,
        generation: &PackageProductionGeneration,
    ) -> Result<(), TenderCommandError> {
        let row: (u32, String, u32, String, String, String, String) = transaction
            .query_row(
                "SELECT generation_sequence, baseline_id, baseline_version,
                        baseline_manifest_sha256, artifact_versions_json, requirements_json,
                        manifest_json
                 FROM submission_generations
                 WHERE generation_id = ?1 AND manifest_sha256 = ?2
                   AND generation_sequence = (SELECT MAX(generation_sequence) FROM submission_generations)",
                params![generation.generation_id, generation.manifest_sha256],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let manifest: GenerationManifest = parse_canonical(&row.6)?;
        if row.0 != generation.sequence
            || row.1 != generation.baseline_id
            || row.2 != generation.baseline_version
            || row.3 != generation.baseline_manifest_sha256
            || parse_canonical::<Vec<SubmissionArtifactVersion>>(&row.4)?
                != generation.artifact_versions
            || parse_canonical::<Vec<GenerationRequirement>>(&row.5)? != generation.requirements
            || manifest.generation_id != generation.generation_id
            || manifest.sequence != generation.sequence
            || manifest.artifact_versions != generation.artifact_versions
            || manifest.requirements != generation.requirements
            || sha256_hex(row.6.as_bytes()) != generation.manifest_sha256
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(())
    }

    pub(crate) fn load_exact_current_package_production_generation(
        &self,
        generation_id: &str,
        manifest_sha256: &str,
        budget: BidPackageOperationBudget,
    ) -> Result<PackageProductionGeneration, TenderCommandError> {
        budget.check()?;
        if !self.package_production_manifests_are_valid_with_check(&mut || budget.check())? {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let generation = self
            .inspect_package_production(budget)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if generation.generation_id != generation_id
            || generation.manifest_sha256 != manifest_sha256
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(generation)
    }

    fn generate_submission_sections(
        &mut self,
        tender_id: &TenderId,
        command: &GenerateSubmissionSectionsCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<PackageProductionGeneration, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let baseline = load_current_approved_baseline(self, command, budget)?;
        let generation_count: u32 = self
            .connection
            .query_row("SELECT COUNT(*) FROM submission_generations", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        if generation_count >= MAX_GENERATIONS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let generation_id = random_identifier(&self.connection)?;
        let staging = self
            .root
            .join("staging")
            .join(format!("generation-{generation_id}"));
        fs::create_dir(&staging).map_err(store_unavailable)?;
        let staging = StagingDirectory(staging);

        let requirement_drafts =
            load_generation_requirement_drafts(self, &baseline.bindings, budget)?;
        if requirement_drafts.is_empty() || requirement_drafts.len() > MAX_REQUIREMENTS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut candidates = build_artifact_candidates(
            &self.connection,
            staging.path(),
            &baseline.bindings,
            &requirement_drafts,
            budget,
        )?;
        if candidates.len() > MAX_ARTIFACTS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let total_generated_bytes = candidates
            .iter()
            .try_fold(0_usize, |total, candidate| {
                total.checked_add(candidate.bytes.len())
            })
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if total_generated_bytes > MAX_TOTAL_GENERATED_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let policy_sha256 = generation_policy_sha256()?;
        for candidate in &mut candidates {
            budget.check()?;
            if candidate.bytes.is_empty() || candidate.bytes.len() > MAX_GENERATED_BYTES {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            validate_office_open_xml(candidate.authoring_mode, &candidate.bytes)?;
            let integrity = cacache::write_hash_sync(self.root.join("content"), &candidate.bytes)
                .map_err(content_store_error)?;
            let verified = cacache::read_hash_sync(self.root.join("content"), &integrity)
                .map_err(content_store_error)?;
            if verified != candidate.bytes {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            candidate.integrity = integrity.to_string();
        }

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let baseline = recheck_exact_approved_baseline(&transaction, command)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        let sequence = generation_count + 1;
        let mut artifact_versions = Vec::with_capacity(candidates.len());
        for candidate in &candidates {
            budget.check()?;
            let stable_identity = canonical_json(&json!({
                "envelope_key": candidate.envelope_key,
                "package_path": candidate.package_path,
                "section_key": candidate.section_key,
            }))?;
            let stable_key = format!(
                "submission-artifact:{}",
                sha256_hex(stable_identity.as_bytes())
            );
            let existing: Option<(String, u32)> = transaction
                .query_row(
                    "SELECT artifacts.artifact_id, heads.current_version
                     FROM submission_artifacts AS artifacts
                     JOIN submission_artifact_heads AS heads USING (artifact_id)
                     WHERE artifacts.stable_key = ?1",
                    [&stable_key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (artifact_id, version) = match existing {
                Some((artifact_id, current_version)) if current_version < 32 => {
                    (artifact_id, current_version + 1)
                }
                Some(_) => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
                None => {
                    let artifact_id = random_identifier(&transaction)?;
                    transaction
                        .execute(
                            "INSERT INTO submission_artifacts (artifact_id, stable_key, created_at)
                             VALUES (?1, ?2, ?3)",
                            params![artifact_id, stable_key, created_at],
                        )
                        .map_err(sql_error)?;
                    (artifact_id, 1)
                }
            };
            let manifest = ArtifactManifest {
                schema_version: 1,
                artifact_id: artifact_id.clone(),
                version,
                generation_id: generation_id.clone(),
                baseline_id: command.baseline_id.clone(),
                baseline_version: command.baseline_version,
                baseline_manifest_sha256: command.baseline_manifest_sha256.clone(),
                section_key: candidate.section_key.clone(),
                package_path: candidate.package_path.clone(),
                envelope_key: candidate.envelope_key.clone(),
                language: candidate.language.clone(),
                authoring_mode: candidate.authoring_mode,
                media_type: candidate.media_type.clone(),
                classifications: candidate.classifications.clone(),
                scope_record_ids: candidate.scope_record_ids.clone(),
                content_sha256: candidate.content_sha256.clone(),
                size_bytes: candidate.bytes.len() as u64,
                exact_inputs: candidate.exact_inputs.clone(),
                generation_policy_id: GENERATION_POLICY_ID.into(),
                generation_policy_version: GENERATION_POLICY_VERSION,
                generation_policy_sha256: policy_sha256.clone(),
                provenance: candidate.provenance.clone(),
                created_at: created_at.clone(),
            };
            let manifest_json = canonical_json(&manifest)?;
            artifact_versions.push(SubmissionArtifactVersion {
                artifact_id,
                version,
                baseline_id: command.baseline_id.clone(),
                baseline_version: command.baseline_version,
                baseline_manifest_sha256: command.baseline_manifest_sha256.clone(),
                section_key: candidate.section_key.clone(),
                package_path: candidate.package_path.clone(),
                envelope_key: candidate.envelope_key.clone(),
                language: candidate.language.clone(),
                authoring_mode: candidate.authoring_mode,
                media_type: candidate.media_type.clone(),
                classifications: candidate.classifications.clone(),
                scope_record_ids: candidate.scope_record_ids.clone(),
                content_sha256: candidate.content_sha256.clone(),
                size_bytes: candidate.bytes.len() as u64,
                exact_inputs: candidate.exact_inputs.clone(),
                generation_policy_id: GENERATION_POLICY_ID.into(),
                generation_policy_version: GENERATION_POLICY_VERSION,
                generation_policy_sha256: policy_sha256.clone(),
                provenance: candidate.provenance.clone(),
                manifest_sha256: sha256_hex(manifest_json.as_bytes()),
                created_at: created_at.clone(),
            });
        }

        let calculations = baseline
            .bindings
            .iter()
            .filter(|binding| {
                binding.kind == CoordinatedBidBaselineBindingKind::CalculationManifest
            })
            .map(|binding| {
                format!(
                    "{}:{}:{}",
                    binding.reference_id, binding.version, binding.manifest_sha256
                )
            })
            .collect::<Vec<_>>();
        let reviews = baseline
            .bindings
            .iter()
            .filter_map(|binding| binding.supporting_review_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut decisions = baseline
            .bindings
            .iter()
            .filter_map(|binding| binding.approval_id.clone())
            .collect::<BTreeSet<_>>();
        decisions.insert(baseline.approval_id);
        let decisions = decisions.into_iter().collect::<Vec<_>>();
        let mut requirements = build_requirements(
            &transaction,
            &generation_id,
            &requirement_drafts,
            &artifact_versions,
            calculations,
            reviews,
            decisions,
        )?;
        requirements.sort_by(|left, right| {
            (left.kind, &left.package_path, &left.requirement_id).cmp(&(
                right.kind,
                &right.package_path,
                &right.requirement_id,
            ))
        });
        let generation_manifest = GenerationManifest {
            schema_version: 1,
            generation_id: generation_id.clone(),
            sequence,
            baseline_id: command.baseline_id.clone(),
            baseline_version: command.baseline_version,
            baseline_manifest_sha256: command.baseline_manifest_sha256.clone(),
            artifact_versions: artifact_versions.clone(),
            requirements: requirements.clone(),
            created_at: created_at.clone(),
        };
        let generation_manifest_json = canonical_json(&generation_manifest)?;
        let generation_manifest_sha256 = sha256_hex(generation_manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "submission_sections_generated",
            tender_revision,
            json!({
                "artifact_count": artifact_versions.len().to_string(),
                "baseline_id": command.baseline_id,
                "baseline_manifest_sha256": command.baseline_manifest_sha256,
                "baseline_version": command.baseline_version.to_string(),
                "generation_id": generation_id,
                "generation_manifest_sha256": generation_manifest_sha256,
                "requirement_count": requirements.len().to_string(),
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO submission_generations (
                   generation_sequence, generation_id, baseline_id, baseline_version,
                   baseline_manifest_sha256, artifact_versions_json, requirements_json,
                   audit_sequence, manifest_json, manifest_sha256, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    sequence,
                    generation_id,
                    command.baseline_id,
                    command.baseline_version,
                    command.baseline_manifest_sha256,
                    canonical_json(&artifact_versions)?,
                    canonical_json(&requirements)?,
                    audit_sequence,
                    generation_manifest_json,
                    generation_manifest_sha256,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        for (candidate, artifact) in candidates.iter().zip(&artifact_versions) {
            let size_bytes = i64::try_from(artifact.size_bytes)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            transaction
                .execute(
                    "INSERT INTO content_objects (sha256, integrity, size_bytes)
                     VALUES (?1, ?2, ?3) ON CONFLICT(sha256) DO NOTHING",
                    params![candidate.content_sha256, candidate.integrity, size_bytes],
                )
                .map_err(sql_error)?;
            let manifest = artifact_manifest_from_view(artifact, &generation_id);
            let manifest_json = canonical_json(&manifest)?;
            transaction
                .execute(
                    "INSERT INTO submission_artifact_versions (
                       artifact_id, version, generation_id, baseline_id, baseline_version,
                       baseline_manifest_sha256, section_key, package_path, envelope_key, language,
                       authoring_mode, media_type, classifications_json, scope_record_ids_json,
                       content_sha256, size_bytes,
                       exact_inputs_json, generation_policy_id, generation_policy_version,
                       generation_policy_sha256, provenance_json, manifest_json,
                       manifest_sha256, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                               ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23,
                               ?24)",
                    params![
                        artifact.artifact_id,
                        artifact.version,
                        generation_id,
                        artifact.baseline_id,
                        artifact.baseline_version,
                        artifact.baseline_manifest_sha256,
                        artifact.section_key,
                        artifact.package_path,
                        artifact.envelope_key,
                        artifact.language,
                        artifact.authoring_mode.as_str(),
                        artifact.media_type,
                        canonical_json(&artifact.classifications)?,
                        canonical_json(&artifact.scope_record_ids)?,
                        artifact.content_sha256,
                        size_bytes,
                        canonical_json(&artifact.exact_inputs)?,
                        artifact.generation_policy_id,
                        artifact.generation_policy_version,
                        artifact.generation_policy_sha256,
                        canonical_json(&artifact.provenance)?,
                        manifest_json,
                        artifact.manifest_sha256,
                        artifact.created_at,
                    ],
                )
                .map_err(sql_error)?;
            transaction
                .execute(
                    "INSERT INTO submission_artifact_heads (artifact_id, current_version)
                     VALUES (?1, ?2)
                     ON CONFLICT(artifact_id) DO UPDATE SET current_version = excluded.current_version",
                    params![artifact.artifact_id, artifact.version],
                )
                .map_err(sql_error)?;
        }
        for (index, requirement) in requirements.iter().enumerate() {
            let manifest = requirement_manifest_from_view(requirement);
            let manifest_json = canonical_json(&manifest)?;
            let ordinal = u32::try_from(index + 1)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let size_bytes = requirement
                .size_bytes
                .map(i64::try_from)
                .transpose()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            transaction
                .execute(
                    "INSERT INTO generation_requirements (
                       requirement_id, generation_id, ordinal, kind, record_id, record_version,
                       record_manifest_sha256, evidence_json, mandatory, section_key, package_path,
                       envelope_key, language, authoring_mode, availability, generated_artifact_id,
                       generated_artifact_version,
                       source_artifact_id, source_artifact_version, content_sha256, size_bytes,
                       calculation_references_json, review_references_json,
                       decision_references_json, authored_fields_json,
                       manifest_json, manifest_sha256, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                               ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23,
                               ?24, ?25, ?26, ?27, ?28)",
                    params![
                        requirement.requirement_id,
                        generation_id,
                        ordinal,
                        requirement.kind.as_str(),
                        requirement.record.record_id,
                        requirement.record.version,
                        requirement.record.manifest_sha256,
                        canonical_json(&requirement.evidence)?,
                        requirement.mandatory,
                        requirement.section_key,
                        requirement.package_path,
                        requirement.envelope_key,
                        requirement.language,
                        requirement.authoring_mode.as_str(),
                        requirement.availability.as_str(),
                        requirement
                            .generated_artifact
                            .as_ref()
                            .map(|item| &item.artifact_id),
                        requirement
                            .generated_artifact
                            .as_ref()
                            .map(|item| item.version),
                        requirement
                            .unchanged_source_artifact
                            .as_ref()
                            .map(|item| &item.artifact_id),
                        requirement
                            .unchanged_source_artifact
                            .as_ref()
                            .map(|item| item.version),
                        requirement.content_sha256,
                        size_bytes,
                        canonical_json(&requirement.calculation_references)?,
                        canonical_json(&requirement.review_references)?,
                        canonical_json(&requirement.decision_references)?,
                        canonical_json(&requirement.authored_fields)?,
                        manifest_json,
                        requirement.manifest_sha256,
                        created_at,
                    ],
                )
                .map_err(sql_error)?;
        }
        transaction.commit().map_err(sql_error)?;
        Ok(PackageProductionGeneration {
            generation_id,
            sequence,
            baseline_id: command.baseline_id.clone(),
            baseline_version: command.baseline_version,
            baseline_manifest_sha256: command.baseline_manifest_sha256.clone(),
            artifact_versions,
            requirements,
            manifest_sha256: generation_manifest_sha256,
            created_at,
        })
    }

    fn inspect_package_production(
        &self,
        budget: BidPackageOperationBudget,
    ) -> Result<Option<PackageProductionGeneration>, TenderCommandError> {
        budget.check()?;
        type StoredGenerationRow = (
            u32,
            String,
            String,
            u32,
            String,
            String,
            String,
            String,
            String,
        );
        let row: Option<StoredGenerationRow> = self
            .connection
            .query_row(
                "SELECT generation_sequence, generation_id, baseline_id, baseline_version,
                        baseline_manifest_sha256, artifact_versions_json, requirements_json,
                        manifest_sha256, created_at
                 FROM submission_generations ORDER BY generation_sequence DESC LIMIT 1",
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
            .optional()
            .map_err(sql_error)?;
        row.map(|row| {
            Ok(PackageProductionGeneration {
                sequence: row.0,
                generation_id: row.1,
                baseline_id: row.2,
                baseline_version: row.3,
                baseline_manifest_sha256: row.4,
                artifact_versions: parse_canonical(&row.5)?,
                requirements: parse_canonical(&row.6)?,
                manifest_sha256: row.7,
                created_at: row.8,
            })
        })
        .transpose()
    }

    fn inspect_submission_artifact_content(
        &self,
        command: &InspectSubmissionArtifactContentCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<SubmissionArtifactContent, TenderCommandError> {
        budget.check()?;
        let row: (String, String, i64, String, String) = self
            .connection
            .query_row(
                "SELECT versions.media_type, versions.content_sha256, versions.size_bytes,
                        objects.integrity, versions.manifest_sha256
                 FROM submission_artifact_versions AS versions
                 JOIN content_objects AS objects ON objects.sha256 = versions.content_sha256
                 WHERE versions.artifact_id = ?1 AND versions.version = ?2",
                params![command.artifact_id, command.version],
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
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        let size_bytes = u64::try_from(row.2)
            .ok()
            .filter(|size| *size > 0 && *size <= MAX_GENERATED_BYTES as u64)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if row.4 != command.manifest_sha256 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        budget.check()?;
        let integrity = row
            .3
            .parse::<cacache::Integrity>()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let bytes = cacache::read_hash_sync(self.root.join("content"), &integrity)
            .map_err(content_store_error)?;
        if bytes.len() as u64 != size_bytes || sha256_hex(&bytes) != row.1 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(SubmissionArtifactContent {
            artifact_id: command.artifact_id.clone(),
            version: command.version,
            media_type: row.0,
            content_sha256: row.1,
            bytes,
        })
    }

    pub(crate) fn package_production_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        let generations: Vec<StoredGenerationRow> = self
            .connection
            .prepare(
                "SELECT generation_sequence, generation_id, baseline_id, baseline_version,
                        baseline_manifest_sha256, artifact_versions_json, requirements_json,
                        audit_sequence, manifest_json, manifest_sha256, created_at
                 FROM submission_generations ORDER BY generation_sequence",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| {
                        Ok(StoredGenerationRow {
                            sequence: row.get(0)?,
                            generation_id: row.get(1)?,
                            baseline_id: row.get(2)?,
                            baseline_version: row.get(3)?,
                            baseline_manifest_sha256: row.get(4)?,
                            artifact_versions_json: row.get(5)?,
                            requirements_json: row.get(6)?,
                            audit_sequence: row.get(7)?,
                            manifest_json: row.get(8)?,
                            manifest_sha256: row.get(9)?,
                            created_at: row.get(10)?,
                        })
                    })?
                    .collect()
            })
            .map_err(sql_error)?;
        for (index, generation) in generations.into_iter().enumerate() {
            check()?;
            let manifest: GenerationManifest = match parse_canonical(&generation.manifest_json) {
                Ok(manifest) => manifest,
                Err(_) => {
                    return Ok(false);
                }
            };
            let expected_sequence = u32::try_from(index + 1)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if generation.sequence != expected_sequence
                || manifest.sequence != generation.sequence
                || manifest.generation_id != generation.generation_id
                || manifest.baseline_id != generation.baseline_id
                || manifest.baseline_version != generation.baseline_version
                || manifest.baseline_manifest_sha256 != generation.baseline_manifest_sha256
                || manifest.created_at != generation.created_at
                || sha256_hex(generation.manifest_json.as_bytes()) != generation.manifest_sha256
                || canonical_json(&manifest.artifact_versions)? != generation.artifact_versions_json
                || canonical_json(&manifest.requirements)? != generation.requirements_json
            {
                return Ok(false);
            }
            let audit: Option<(String, String, String)> = self
                .connection
                .query_row(
                    "SELECT event_type, payload_json, created_at
                     FROM audit_events WHERE sequence = ?1",
                    [generation.audit_sequence],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let Some((event_type, payload_json, audit_created_at)) = audit else {
                return Ok(false);
            };
            let audit_payload: Value = match parse_canonical(&payload_json) {
                Ok(payload) => payload,
                Err(_) => {
                    return Ok(false);
                }
            };
            let baseline_version = generation.baseline_version.to_string();
            let artifact_count = manifest.artifact_versions.len().to_string();
            let requirement_count = manifest.requirements.len().to_string();
            if event_type != "submission_sections_generated"
                || audit_created_at != generation.created_at
                || audit_payload
                    .pointer("/change/generation_id")
                    .and_then(Value::as_str)
                    != Some(generation.generation_id.as_str())
                || audit_payload
                    .pointer("/change/generation_manifest_sha256")
                    .and_then(Value::as_str)
                    != Some(generation.manifest_sha256.as_str())
                || audit_payload
                    .pointer("/change/baseline_id")
                    .and_then(Value::as_str)
                    != Some(generation.baseline_id.as_str())
                || audit_payload
                    .pointer("/change/baseline_version")
                    .and_then(Value::as_str)
                    != Some(baseline_version.as_str())
                || audit_payload
                    .pointer("/change/baseline_manifest_sha256")
                    .and_then(Value::as_str)
                    != Some(generation.baseline_manifest_sha256.as_str())
                || audit_payload
                    .pointer("/change/artifact_count")
                    .and_then(Value::as_str)
                    != Some(artifact_count.as_str())
                || audit_payload
                    .pointer("/change/requirement_count")
                    .and_then(Value::as_str)
                    != Some(requirement_count.as_str())
            {
                return Ok(false);
            }
            let artifact_count: u32 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM submission_artifact_versions WHERE generation_id = ?1",
                    [&generation.generation_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let requirement_count: u32 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM generation_requirements WHERE generation_id = ?1",
                    [&generation.generation_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if usize::try_from(artifact_count).ok() != Some(manifest.artifact_versions.len())
                || usize::try_from(requirement_count).ok() != Some(manifest.requirements.len())
            {
                return Ok(false);
            }
            for artifact in &manifest.artifact_versions {
                check()?;
                let stored: Option<StoredArtifactRow> = self
                    .connection
                    .query_row(
                        "SELECT versions.artifact_id, versions.version, versions.baseline_id,
                                versions.baseline_version, versions.baseline_manifest_sha256,
                                versions.section_key, versions.package_path, versions.envelope_key,
                                versions.language, versions.authoring_mode, versions.media_type,
                                versions.classifications_json, versions.scope_record_ids_json,
                                versions.content_sha256, versions.size_bytes,
                                versions.exact_inputs_json, versions.generation_policy_id,
                                versions.generation_policy_version,
                                versions.generation_policy_sha256, versions.provenance_json,
                                versions.manifest_json, versions.manifest_sha256, versions.created_at,
                                artifacts.stable_key
                         FROM submission_artifact_versions AS versions
                         JOIN submission_artifacts AS artifacts USING (artifact_id)
                         WHERE versions.artifact_id = ?1 AND versions.version = ?2
                           AND versions.generation_id = ?3",
                        params![artifact.artifact_id, artifact.version, generation.generation_id],
                        |row| {
                            Ok(StoredArtifactRow {
                                artifact_id: row.get(0)?,
                                version: row.get(1)?,
                                baseline_id: row.get(2)?,
                                baseline_version: row.get(3)?,
                                baseline_manifest_sha256: row.get(4)?,
                                section_key: row.get(5)?,
                                package_path: row.get(6)?,
                                envelope_key: row.get(7)?,
                                language: row.get(8)?,
                                authoring_mode: row.get(9)?,
                                media_type: row.get(10)?,
                                classifications_json: row.get(11)?,
                                scope_record_ids_json: row.get(12)?,
                                content_sha256: row.get(13)?,
                                size_bytes: row.get(14)?,
                                exact_inputs_json: row.get(15)?,
                                generation_policy_id: row.get(16)?,
                                generation_policy_version: row.get(17)?,
                                generation_policy_sha256: row.get(18)?,
                                provenance_json: row.get(19)?,
                                manifest_json: row.get(20)?,
                                manifest_sha256: row.get(21)?,
                                created_at: row.get(22)?,
                                stable_key: row.get(23)?,
                            })
                        },
                    )
                    .optional()
                    .map_err(sql_error)?;
                let Some(stored) = stored else {
                    return Ok(false);
                };
                let Ok(authoring_mode) = GenerationAuthoringMode::parse(&stored.authoring_mode)
                else {
                    return Ok(false);
                };
                let Ok(classifications) = parse_canonical(&stored.classifications_json) else {
                    return Ok(false);
                };
                let Ok(scope_record_ids) = parse_canonical(&stored.scope_record_ids_json) else {
                    return Ok(false);
                };
                let Ok(exact_inputs) = parse_canonical(&stored.exact_inputs_json) else {
                    return Ok(false);
                };
                let Ok(provenance) = parse_canonical(&stored.provenance_json) else {
                    return Ok(false);
                };
                let reconstructed = SubmissionArtifactVersion {
                    artifact_id: stored.artifact_id,
                    version: stored.version,
                    baseline_id: stored.baseline_id,
                    baseline_version: stored.baseline_version,
                    baseline_manifest_sha256: stored.baseline_manifest_sha256,
                    section_key: stored.section_key,
                    package_path: stored.package_path,
                    envelope_key: stored.envelope_key,
                    language: stored.language,
                    authoring_mode,
                    media_type: stored.media_type,
                    classifications,
                    scope_record_ids,
                    content_sha256: stored.content_sha256,
                    size_bytes: match u64::try_from(stored.size_bytes) {
                        Ok(size_bytes) => size_bytes,
                        Err(_) => {
                            return Ok(false);
                        }
                    },
                    exact_inputs,
                    generation_policy_id: stored.generation_policy_id,
                    generation_policy_version: stored.generation_policy_version,
                    generation_policy_sha256: stored.generation_policy_sha256,
                    provenance,
                    manifest_sha256: stored.manifest_sha256,
                    created_at: stored.created_at,
                };
                let stable_identity = canonical_json(&json!({
                    "envelope_key": &reconstructed.envelope_key,
                    "package_path": &reconstructed.package_path,
                    "section_key": &reconstructed.section_key,
                }))?;
                let expected_stable_key = format!(
                    "submission-artifact:{}",
                    sha256_hex(stable_identity.as_bytes())
                );
                let expected_manifest = canonical_json(&artifact_manifest_from_view(
                    &reconstructed,
                    &generation.generation_id,
                ))?;
                let head: Option<(u32, u32)> = self
                    .connection
                    .query_row(
                        "SELECT heads.current_version, MAX(versions.version)
                         FROM submission_artifact_heads AS heads
                         JOIN submission_artifact_versions AS versions USING (artifact_id)
                         WHERE heads.artifact_id = ?1 GROUP BY heads.artifact_id",
                        [&reconstructed.artifact_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(sql_error)?;
                if &reconstructed != artifact
                    || stored.stable_key != expected_stable_key
                    || stored.manifest_json != expected_manifest
                    || sha256_hex(stored.manifest_json.as_bytes()) != reconstructed.manifest_sha256
                    || head.is_none_or(|(current, latest)| current != latest)
                {
                    return Ok(false);
                }
            }
            for (index, requirement) in manifest.requirements.iter().enumerate() {
                check()?;
                let stored: Option<StoredRequirementRow> = self
                    .connection
                    .query_row(
                        "SELECT requirements.ordinal, requirements.kind, requirements.record_id,
                                requirements.record_version, requirements.record_manifest_sha256,
                                records.stable_key, versions.title, requirements.evidence_json,
                                requirements.mandatory, requirements.section_key,
                                requirements.package_path, requirements.envelope_key,
                                requirements.language, requirements.authoring_mode,
                                requirements.availability,
                                requirements.generated_artifact_id,
                                requirements.generated_artifact_version,
                                requirements.source_artifact_id,
                                requirements.source_artifact_version,
                                requirements.content_sha256, requirements.size_bytes,
                                requirements.calculation_references_json,
                                requirements.review_references_json,
                                requirements.decision_references_json,
                                requirements.authored_fields_json, requirements.manifest_json,
                                requirements.manifest_sha256, requirements.created_at
                         FROM generation_requirements AS requirements
                         JOIN tender_records AS records USING (record_id)
                         JOIN tender_record_versions AS versions
                           ON versions.record_id = requirements.record_id
                          AND versions.version = requirements.record_version
                         WHERE requirement_id = ?1 AND generation_id = ?2",
                        params![requirement.requirement_id, generation.generation_id],
                        |row| {
                            Ok(StoredRequirementRow {
                                ordinal: row.get(0)?,
                                kind: row.get(1)?,
                                record_id: row.get(2)?,
                                record_version: row.get(3)?,
                                record_manifest_sha256: row.get(4)?,
                                record_stable_key: row.get(5)?,
                                record_title: row.get(6)?,
                                evidence_json: row.get(7)?,
                                mandatory: row.get(8)?,
                                section_key: row.get(9)?,
                                package_path: row.get(10)?,
                                envelope_key: row.get(11)?,
                                language: row.get(12)?,
                                authoring_mode: row.get(13)?,
                                availability: row.get(14)?,
                                generated_artifact_id: row.get(15)?,
                                generated_artifact_version: row.get(16)?,
                                source_artifact_id: row.get(17)?,
                                source_artifact_version: row.get(18)?,
                                content_sha256: row.get(19)?,
                                size_bytes: row.get(20)?,
                                calculation_references_json: row.get(21)?,
                                review_references_json: row.get(22)?,
                                decision_references_json: row.get(23)?,
                                authored_fields_json: row.get(24)?,
                                manifest_json: row.get(25)?,
                                manifest_sha256: row.get(26)?,
                                created_at: row.get(27)?,
                            })
                        },
                    )
                    .optional()
                    .map_err(sql_error)?;
                let Some(stored) = stored else {
                    return Ok(false);
                };
                let Ok(kind) = GenerationRequirementKind::parse(&stored.kind) else {
                    return Ok(false);
                };
                let Ok(authoring_mode) = GenerationAuthoringMode::parse(&stored.authoring_mode)
                else {
                    return Ok(false);
                };
                let Ok(availability) =
                    GenerationRequirementAvailability::parse(&stored.availability)
                else {
                    return Ok(false);
                };
                let Ok(evidence) = parse_canonical(&stored.evidence_json) else {
                    return Ok(false);
                };
                let Ok(authored_fields) = parse_canonical(&stored.authored_fields_json) else {
                    return Ok(false);
                };
                let Ok(calculation_references) =
                    parse_canonical(&stored.calculation_references_json)
                else {
                    return Ok(false);
                };
                let Ok(review_references) = parse_canonical(&stored.review_references_json) else {
                    return Ok(false);
                };
                let Ok(decision_references) = parse_canonical(&stored.decision_references_json)
                else {
                    return Ok(false);
                };
                let reconstructed = GenerationRequirement {
                    requirement_id: requirement.requirement_id.clone(),
                    kind,
                    record: GenerationRequirementRecordReference {
                        record_id: stored.record_id,
                        version: stored.record_version,
                        manifest_sha256: stored.record_manifest_sha256,
                        stable_key: stored.record_stable_key,
                        title: stored.record_title,
                    },
                    evidence,
                    authored_fields,
                    mandatory: stored.mandatory,
                    section_key: stored.section_key,
                    package_path: stored.package_path,
                    envelope_key: stored.envelope_key,
                    language: stored.language,
                    authoring_mode,
                    availability,
                    generated_artifact: match (
                        stored.generated_artifact_id,
                        stored.generated_artifact_version,
                    ) {
                        (Some(artifact_id), Some(version)) => {
                            Some(SubmissionGeneratedArtifactReference {
                                artifact_id,
                                version,
                            })
                        }
                        (None, None) => None,
                        _ => {
                            return Ok(false);
                        }
                    },
                    unchanged_source_artifact: match (
                        stored.source_artifact_id,
                        stored.source_artifact_version,
                    ) {
                        (Some(artifact_id), Some(version)) => {
                            Some(SubmissionSourceArtifactReference {
                                artifact_id,
                                version,
                            })
                        }
                        (None, None) => None,
                        _ => {
                            return Ok(false);
                        }
                    },
                    content_sha256: stored.content_sha256,
                    size_bytes: match stored.size_bytes.map(u64::try_from).transpose() {
                        Ok(size_bytes) => size_bytes,
                        Err(_) => return Ok(false),
                    },
                    calculation_references,
                    review_references,
                    decision_references,
                    manifest_sha256: stored.manifest_sha256,
                };
                let expected_requirement_id = sha256_hex(
                    format!(
                        "{}:{}:{}:{}",
                        generation.generation_id,
                        reconstructed.record.record_id,
                        reconstructed.record.version,
                        reconstructed.kind.as_str()
                    )
                    .as_bytes(),
                );
                let expected = canonical_json(&requirement_manifest_from_view(&reconstructed))?;
                let expected_ordinal = u32::try_from(index + 1)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                if &reconstructed != requirement
                    || reconstructed.requirement_id != expected_requirement_id
                    || stored.ordinal != expected_ordinal
                    || stored.created_at != generation.created_at
                    || stored.manifest_json != expected
                    || sha256_hex(stored.manifest_json.as_bytes()) != reconstructed.manifest_sha256
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

fn load_current_approved_baseline(
    store: &TenderStore,
    command: &GenerateSubmissionSectionsCommand,
    budget: BidPackageOperationBudget,
) -> Result<BaselineInput, TenderCommandError> {
    let baseline = store.load_coordinated_bid_baseline(
        &command.baseline_id,
        command.baseline_version,
        budget,
    )?;
    let approval = baseline
        .approval
        .filter(|approval| approval.decision == CoordinatedBidBaselineDecision::Approve)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if !baseline.current || baseline.manifest_sha256 != command.baseline_manifest_sha256 {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(BaselineInput {
        bindings: baseline.bindings,
        approval_id: approval.approval_id,
    })
}

fn recheck_exact_approved_baseline(
    connection: &rusqlite::Connection,
    command: &GenerateSubmissionSectionsCommand,
) -> Result<BaselineInput, TenderCommandError> {
    let lifecycle: String = connection
        .query_row(
            "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if lifecycle != "package_production" {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let unresolved_change: bool = connection
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
    if unresolved_change {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let row: Option<(String, String, String)> = connection
        .query_row(
            "SELECT versions.manifest_sha256, versions.bindings_json, approvals.approval_id
             FROM coordinated_bid_baseline_head AS head
             JOIN coordinated_bid_baseline_versions AS versions
               ON versions.baseline_id = head.baseline_id
              AND versions.version = head.current_version
             JOIN coordinated_bid_baseline_approvals AS approvals
               ON approvals.baseline_id = versions.baseline_id
              AND approvals.baseline_version = versions.version
             WHERE versions.baseline_id = ?1 AND versions.version = ?2
               AND approvals.decision = 'approve'",
            params![command.baseline_id, command.baseline_version],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let Some((manifest_sha256, bindings_json, approval_id)) = row else {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    };
    if manifest_sha256 != command.baseline_manifest_sha256 {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(BaselineInput {
        bindings: parse_canonical(&bindings_json)?,
        approval_id,
    })
}

fn load_generation_requirement_drafts(
    store: &TenderStore,
    bindings: &[CoordinatedBidBaselineBinding],
    budget: BidPackageOperationBudget,
) -> Result<Vec<RecordRequirementDraft>, TenderCommandError> {
    let mut drafts = Vec::new();
    for binding in bindings
        .iter()
        .filter(|binding| binding.kind == CoordinatedBidBaselineBindingKind::TenderRecordVersion)
    {
        budget.check()?;
        let record = store.inspect_tender_record_version(&binding.reference_id, binding.version)?;
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
        if binding.manifest_sha256 != sha256_hex(canonical_json(&immutable_record)?.as_bytes())
            || binding.source != record.stable_key
            || binding.summary != record.title
            || binding.supporting_review_id != supporting_review_id
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let fields = record.fields;
        let Some(instruction) = record.generation_instruction else {
            continue;
        };
        if record.verification_status != VerificationStatus::Verified
            || !matches!(
                record.trust_class,
                TenderRecordTrustClass::Verified | TenderRecordTrustClass::EngineerVerified
            )
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let TenderRecordGenerationInstruction {
            kind,
            mandatory,
            section_key,
            package_path,
            envelope_key,
            language,
            authoring_mode,
            requested_authoring_format: _,
            evidence,
        } = instruction;
        validate_package_path(&package_path)?;
        if section_key.len() > 200 || envelope_key.len() > 200 || language.len() > 100 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if evidence.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if fields
            .iter()
            .any(|field| !material_generation_field_is_valid(field, bindings))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        drafts.push(RecordRequirementDraft {
            kind,
            record: GenerationRequirementRecordReference {
                record_id: binding.reference_id.clone(),
                version: binding.version,
                manifest_sha256: binding.manifest_sha256.clone(),
                stable_key: record.stable_key,
                title: record.title,
            },
            fields,
            evidence,
            mandatory,
            section_key,
            package_path,
            envelope_key,
            language,
            authoring_mode,
        });
    }
    Ok(drafts)
}

fn material_generation_field_is_valid(
    field: &TenderRecordField,
    bindings: &[CoordinatedBidBaselineBinding],
) -> bool {
    if field.value.is_none() && field.normalized_value.is_none() {
        return false;
    }
    match field.basis_kind {
        TenderRecordBasisKind::Evidence => !field.evidence.is_empty(),
        TenderRecordBasisKind::CalculationRun => {
            let Some(reference_id) = field.basis_reference.as_deref() else {
                return false;
            };
            let Some(authority) = field.basis_authority.as_ref() else {
                return false;
            };
            authority.kind == TenderRecordAuthorityKind::CalculationRun
                && authority.authority_id.as_str() == reference_id
                && authority.manifest_sha256.as_ref().is_some_and(|manifest| {
                    bindings.iter().any(|binding| {
                        binding.kind == CoordinatedBidBaselineBindingKind::CalculationManifest
                            && binding.reference_id.as_str() == reference_id
                            && binding.manifest_sha256.as_str() == manifest.as_str()
                    })
                })
        }
        TenderRecordBasisKind::Assumption
        | TenderRecordBasisKind::TenderQuery
        | TenderRecordBasisKind::EngineerEntry => false,
    }
}

fn build_artifact_candidates(
    connection: &rusqlite::Connection,
    staging: &Path,
    bindings: &[CoordinatedBidBaselineBinding],
    drafts: &[RecordRequirementDraft],
    budget: BidPackageOperationBudget,
) -> Result<Vec<ArtifactCandidate>, TenderCommandError> {
    let mut groups: BTreeMap<
        (String, String, String, String, GenerationAuthoringMode),
        Vec<&RecordRequirementDraft>,
    > = BTreeMap::new();
    for draft in drafts {
        if matches!(
            draft.authoring_mode,
            GenerationAuthoringMode::Docx | GenerationAuthoringMode::Xlsx
        ) {
            groups
                .entry((
                    draft.section_key.clone(),
                    draft.package_path.clone(),
                    draft.envelope_key.clone(),
                    draft.language.clone(),
                    draft.authoring_mode,
                ))
                .or_default()
                .push(draft);
        }
    }
    validate_unique_package_paths(connection, drafts)?;
    let approved_price = load_exact_bound_approved_price(connection, bindings)?;
    let mut candidates = Vec::new();
    for ((section_key, package_path, envelope_key, language, mode), mut requirements) in groups {
        budget.check()?;
        validate_package_path(&package_path)?;
        requirements.sort_by(|left, right| {
            (&left.record.record_id, left.record.version, left.kind).cmp(&(
                &right.record.record_id,
                right.record.version,
                right.kind,
            ))
        });
        let bytes = match mode {
            GenerationAuthoringMode::Docx => render_docx(&requirements, budget)?,
            GenerationAuthoringMode::Xlsx => {
                render_xlsx(&requirements, approved_price.as_ref(), budget)?
            }
            GenerationAuthoringMode::UnchangedSource => unreachable!(),
            GenerationAuthoringMode::Unsupported => continue,
        };
        let staged_path = staging.join(sha256_hex(package_path.as_bytes()));
        fs::write(&staged_path, &bytes).map_err(store_unavailable)?;
        let staged = fs::read(&staged_path).map_err(store_unavailable)?;
        if staged != bytes {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let media_type = match mode {
            GenerationAuthoringMode::Docx => {
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            }
            GenerationAuthoringMode::Xlsx => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
            GenerationAuthoringMode::UnchangedSource => unreachable!(),
            GenerationAuthoringMode::Unsupported => continue,
        };
        let mut exact_inputs = bindings
            .iter()
            .map(|binding| {
                format!(
                    "{}:{}:{}:{}",
                    baseline_binding_kind_str(binding.kind),
                    binding.reference_id,
                    binding.version,
                    binding.manifest_sha256
                )
            })
            .collect::<Vec<_>>();
        exact_inputs.sort();
        let provenance = requirements
            .iter()
            .map(|requirement| {
                format!(
                    "tender_record:{}:{}:{}",
                    requirement.record.record_id,
                    requirement.record.version,
                    requirement.record.manifest_sha256
                )
            })
            .collect::<Vec<_>>();
        let classifications = requirements
            .iter()
            .map(|requirement| requirement.kind)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let scope_record_ids = requirements
            .iter()
            .map(|requirement| requirement.record.record_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        candidates.push(ArtifactCandidate {
            section_key,
            package_path,
            envelope_key,
            language,
            authoring_mode: mode,
            media_type: media_type.into(),
            classifications,
            scope_record_ids,
            content_sha256: sha256_hex(&bytes),
            bytes,
            integrity: String::new(),
            exact_inputs,
            provenance,
        });
    }
    Ok(candidates)
}

fn load_exact_bound_approved_price(
    connection: &rusqlite::Connection,
    bindings: &[CoordinatedBidBaselineBinding],
) -> Result<Option<ApprovedPriceInput>, TenderCommandError> {
    let prices = bindings
        .iter()
        .filter(|binding| binding.kind == CoordinatedBidBaselineBindingKind::ApprovedTenderPrice)
        .collect::<Vec<_>>();
    if prices.len() > 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let Some(binding) = prices.first() else {
        return Ok(None);
    };
    let price = connection
        .query_row(
            "SELECT final_amount, currency, pricing_calculation_run_id,
                    calculation_manifest_sha256, manifest_sha256
             FROM approved_tender_prices
             WHERE pricing_scenario_id = ?1 AND pricing_scenario_version = ?2",
            params![binding.reference_id, binding.version],
            |row| {
                Ok((
                    ApprovedPriceInput {
                        amount: row.get(0)?,
                        currency: row.get(1)?,
                        calculation_run_id: row.get(2)?,
                        calculation_manifest_sha256: row.get(3)?,
                    },
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let Some((price, price_manifest_sha256)) = price else {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    };
    if binding.category != CoordinatedBidBaselineCategory::Commercial
        || price_manifest_sha256 != binding.manifest_sha256
        || !bindings.iter().any(|calculation| {
            calculation.kind == CoordinatedBidBaselineBindingKind::CalculationManifest
                && calculation.reference_id == price.calculation_run_id
                && calculation.manifest_sha256 == price.calculation_manifest_sha256
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(Some(price))
}

fn validate_unique_package_paths(
    connection: &rusqlite::Connection,
    drafts: &[RecordRequirementDraft],
) -> Result<(), TenderCommandError> {
    let mut paths =
        BTreeMap::<String, (String, String, String, String, GenerationAuthoringMode)>::new();
    let existing = connection
        .prepare(
            "SELECT versions.package_path, versions.section_key, versions.envelope_key,
                    versions.language, versions.authoring_mode
             FROM submission_artifact_heads AS heads
             JOIN submission_artifact_versions AS versions
               ON versions.artifact_id = heads.artifact_id
              AND versions.version = heads.current_version",
        )
        .and_then(|mut statement| {
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(sql_error)?;
    for (package_path, section_key, envelope_key, language, mode) in existing {
        let mode = GenerationAuthoringMode::parse(&mode)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        paths.insert(
            package_collision_key(&package_path),
            (package_path, section_key, envelope_key, language, mode),
        );
    }
    for draft in drafts {
        if draft.authoring_mode == GenerationAuthoringMode::Unsupported {
            continue;
        }
        let expected_extension = match draft.authoring_mode {
            GenerationAuthoringMode::Docx => Some("docx"),
            GenerationAuthoringMode::Xlsx => Some("xlsx"),
            GenerationAuthoringMode::UnchangedSource => None,
            GenerationAuthoringMode::Unsupported => None,
        };
        if expected_extension.is_some_and(|extension| {
            !draft
                .package_path
                .rsplit_once('.')
                .is_some_and(|(_, actual)| actual.eq_ignore_ascii_case(extension))
        }) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let collision_key = package_collision_key(&draft.package_path);
        let identity = (
            draft.package_path.clone(),
            draft.section_key.clone(),
            draft.envelope_key.clone(),
            draft.language.clone(),
            draft.authoring_mode,
        );
        if let Some(prior) = paths.insert(collision_key, identity.clone()) {
            if prior != identity {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
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

fn build_requirements(
    transaction: &Transaction<'_>,
    generation_id: &str,
    drafts: &[RecordRequirementDraft],
    artifacts: &[SubmissionArtifactVersion],
    calculation_references: Vec<String>,
    review_references: Vec<String>,
    decision_references: Vec<String>,
) -> Result<Vec<GenerationRequirement>, TenderCommandError> {
    let mut requirements = Vec::new();
    for draft in drafts {
        let (
            availability,
            generated_artifact,
            unchanged_source_artifact,
            content_sha256,
            size_bytes,
        ) = if draft.authoring_mode == GenerationAuthoringMode::Unsupported {
            (
                GenerationRequirementAvailability::Unsupported,
                None,
                None,
                None,
                None,
            )
        } else if draft.authoring_mode == GenerationAuthoringMode::UnchangedSource {
            let source_references = draft
                .evidence
                .iter()
                .map(|evidence| {
                    (
                        evidence.reference.artifact_id.clone(),
                        evidence.reference.version,
                    )
                })
                .collect::<BTreeSet<_>>();
            if source_references.len() != 1 {
                (
                    GenerationRequirementAvailability::Missing,
                    None,
                    None,
                    None,
                    None,
                )
            } else {
                let (source_artifact_id, source_artifact_version) = source_references
                    .into_iter()
                    .next()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                let source: (String, i64) = transaction
                        .query_row(
                            "SELECT sha256, size_bytes FROM source_artifact_versions
                             WHERE artifact_id = ?1 AND version = ?2 AND registration_state = 'registered'",
                            params![source_artifact_id, source_artifact_version],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .map_err(sql_error)?;
                (
                    GenerationRequirementAvailability::Available,
                    None,
                    Some(SubmissionSourceArtifactReference {
                        artifact_id: source_artifact_id,
                        version: source_artifact_version,
                    }),
                    Some(source.0),
                    Some(
                        u64::try_from(source.1).map_err(|_| {
                            TenderCommandError::new(TenderErrorCode::IntegrityFailed)
                        })?,
                    ),
                )
            }
        } else {
            let artifact = artifacts
                .iter()
                .find(|artifact| artifact.package_path == draft.package_path)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            (
                GenerationRequirementAvailability::Available,
                Some(SubmissionGeneratedArtifactReference {
                    artifact_id: artifact.artifact_id.clone(),
                    version: artifact.version,
                }),
                None,
                Some(artifact.content_sha256.clone()),
                Some(artifact.size_bytes),
            )
        };
        let requirement_id = sha256_hex(
            format!(
                "{generation_id}:{}:{}:{}",
                draft.record.record_id,
                draft.record.version,
                draft.kind.as_str()
            )
            .as_bytes(),
        );
        let manifest = RequirementManifest {
            schema_version: 1,
            requirement_id: requirement_id.clone(),
            kind: draft.kind,
            record: draft.record.clone(),
            evidence: draft.evidence.clone(),
            authored_fields: draft.fields.clone(),
            mandatory: draft.mandatory,
            section_key: draft.section_key.clone(),
            package_path: draft.package_path.clone(),
            envelope_key: draft.envelope_key.clone(),
            language: draft.language.clone(),
            authoring_mode: draft.authoring_mode,
            availability,
            generated_artifact: generated_artifact.clone(),
            unchanged_source_artifact: unchanged_source_artifact.clone(),
            content_sha256: content_sha256.clone(),
            size_bytes,
            calculation_references: calculation_references.clone(),
            review_references: review_references.clone(),
            decision_references: decision_references.clone(),
        };
        requirements.push(GenerationRequirement {
            requirement_id,
            kind: draft.kind,
            record: draft.record.clone(),
            evidence: draft.evidence.clone(),
            authored_fields: draft.fields.clone(),
            mandatory: draft.mandatory,
            section_key: draft.section_key.clone(),
            package_path: draft.package_path.clone(),
            envelope_key: draft.envelope_key.clone(),
            language: draft.language.clone(),
            authoring_mode: draft.authoring_mode,
            availability,
            generated_artifact,
            unchanged_source_artifact,
            content_sha256,
            size_bytes,
            calculation_references: calculation_references.clone(),
            review_references: review_references.clone(),
            decision_references: decision_references.clone(),
            manifest_sha256: sha256_hex(canonical_json(&manifest)?.as_bytes()),
        });
    }
    Ok(requirements)
}

fn render_docx(
    requirements: &[&RecordRequirementDraft],
    budget: BidPackageOperationBudget,
) -> Result<Vec<u8>, TenderCommandError> {
    if requirements.is_empty() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut paragraph_index = 1_u32;
    let mut document = Docx::new()
        .created_at("1970-01-01T00:00:00Z")
        .updated_at("1970-01-01T00:00:00Z")
        .add_paragraph(deterministic_paragraph(
            &mut paragraph_index,
            "Controlled Tender Submission / عرض العطاء الخاضع للرقابة",
            true,
        ));
    for requirement in requirements {
        budget.check()?;
        document = document.add_paragraph(deterministic_paragraph(
            &mut paragraph_index,
            &requirement.record.title,
            true,
        ));
        for field in &requirement.fields {
            budget.check()?;
            if let Some(value) = field.value.as_deref().or(field.normalized_value.as_deref()) {
                document = document.add_paragraph(deterministic_paragraph(
                    &mut paragraph_index,
                    &format!("{}: {value}", field.name),
                    false,
                ));
            }
        }
    }
    let mut output = Cursor::new(Vec::new());
    document
        .build()
        .pack(&mut output)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
    Ok(output.into_inner())
}

fn deterministic_paragraph(index: &mut u32, text: &str, bold: bool) -> Paragraph {
    let fonts = RunFonts::new().ascii("Arial").hi_ansi("Arial").cs("Arial");
    let mut run = Run::new().fonts(fonts).add_text(text);
    if bold {
        run = run.bold();
    }
    let mut paragraph = Paragraph::new().id(format!("{:08x}", *index)).add_run(run);
    *index += 1;
    if contains_arabic(text) {
        paragraph.property = paragraph.property.bidi(true);
        paragraph = paragraph.align(AlignmentType::Right);
    }
    paragraph
}

fn render_xlsx(
    requirements: &[&RecordRequirementDraft],
    approved_price: Option<&ApprovedPriceInput>,
    budget: BidPackageOperationBudget,
) -> Result<Vec<u8>, TenderCommandError> {
    if requirements.is_empty() {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut workbook = Workbook::new();
    let fixed_created = ExcelDateTime::from_ymd(1970, 1, 1)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    workbook.set_properties(
        &DocProperties::new()
            .set_author("Quantix")
            .set_creation_datetime(&fixed_created),
    );
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name("Commercial Offer")
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if requirements.iter().any(|requirement| {
        contains_arabic(&requirement.language)
            || requirement
                .fields
                .iter()
                .filter_map(|field| field.value.as_deref())
                .any(contains_arabic)
    }) {
        worksheet.set_right_to_left(true);
    }
    let heading = Format::new().set_bold().set_reading_direction(2);
    worksheet
        .write_with_format(
            0,
            0,
            "Controlled Commercial Offer / العرض التجاري",
            &heading,
        )
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
    let mut row = 2;
    if let Some(price) = approved_price {
        let exact_amount = price
            .amount
            .parse::<Decimal>()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let amount = exact_amount
            .to_f64()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if Decimal::from_f64_retain(amount)
            .map(|value| value.round_dp(exact_amount.scale()).normalize())
            != Some(exact_amount.normalize())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let amount_format_code = if exact_amount.scale() == 0 {
            "#,##0".to_owned()
        } else {
            format!("#,##0.{}", "0".repeat(exact_amount.scale() as usize))
        };
        let amount_format = Format::new().set_num_format(amount_format_code);
        worksheet
            .write(row, 0, "Approved Tender Price")
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        worksheet
            .write_with_format(row, 1, amount, &amount_format)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        worksheet
            .write(row, 2, &price.currency)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        worksheet
            .write(row, 3, &price.calculation_run_id)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        worksheet
            .write(row, 4, &price.calculation_manifest_sha256)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        worksheet
            .write(row, 5, &price.amount)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        row += 2;
    }
    for requirement in requirements {
        budget.check()?;
        worksheet
            .write(row, 0, &requirement.record.title)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        for field in &requirement.fields {
            budget.check()?;
            if let Some(value) = field.value.as_deref().or(field.normalized_value.as_deref()) {
                let value_format =
                    Format::new().set_reading_direction(if contains_arabic(value) { 2 } else { 1 });
                worksheet
                    .write(row, 1, &field.name)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
                worksheet
                    .write_with_format(row, 2, value, &value_format)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
                row += 1;
            }
        }
        row += 1;
    }
    workbook
        .save_to_buffer()
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
}

fn contains_arabic(value: &str) -> bool {
    value.chars().any(
        |character| matches!(character as u32, 0x0600..=0x06ff | 0x0750..=0x077f | 0x08a0..=0x08ff),
    )
}

fn validate_office_open_xml(
    mode: GenerationAuthoringMode,
    bytes: &[u8],
) -> Result<(), TenderCommandError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let required = match mode {
        GenerationAuthoringMode::Docx => "word/document.xml",
        GenerationAuthoringMode::Xlsx => "xl/workbook.xml",
        GenerationAuthoringMode::UnchangedSource => {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
        }
        GenerationAuthoringMode::Unsupported => {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
        }
    };
    archive
        .by_name(required)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(())
}

fn validate_package_path(path: &str) -> Result<(), TenderCommandError> {
    if path.is_empty()
        || path.len() > 1000
        || path
            .chars()
            .any(|character| matches!(character, '\\' | '\0' | ':'))
        || path.split('/').any(|component| {
            component.is_empty()
                || component == "."
                || component.ends_with(' ')
                || component.ends_with('.')
                || component.chars().any(char::is_control)
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(())
}

fn generation_policy_sha256() -> Result<String, TenderCommandError> {
    Ok(sha256_hex(
        canonical_json(&json!({
            "id": GENERATION_POLICY_ID,
            "version": GENERATION_POLICY_VERSION,
            "formats": ["docx", "xlsx"],
            "calculation_authority": "approved_calculation_run",
            "meaning_change": "forbidden",
        }))?
        .as_bytes(),
    ))
}

fn baseline_binding_kind_str(kind: CoordinatedBidBaselineBindingKind) -> &'static str {
    match kind {
        CoordinatedBidBaselineBindingKind::ProductionArtifactVersion => {
            "production_artifact_version"
        }
        CoordinatedBidBaselineBindingKind::TenderRecordVersion => "tender_record_version",
        CoordinatedBidBaselineBindingKind::TenderQueryVersion => "tender_query_version",
        CoordinatedBidBaselineBindingKind::ExternalRfiVersion => "external_rfi_version",
        CoordinatedBidBaselineBindingKind::PricedCostBaseline => "priced_cost_baseline",
        CoordinatedBidBaselineBindingKind::ApprovedTenderPrice => "approved_tender_price",
        CoordinatedBidBaselineBindingKind::CalculationManifest => "calculation_manifest",
        CoordinatedBidBaselineBindingKind::CommercialStrategy => "commercial_strategy",
    }
}

fn artifact_manifest_from_view(
    artifact: &SubmissionArtifactVersion,
    generation_id: &str,
) -> ArtifactManifest {
    ArtifactManifest {
        schema_version: 1,
        artifact_id: artifact.artifact_id.clone(),
        version: artifact.version,
        generation_id: generation_id.into(),
        baseline_id: artifact.baseline_id.clone(),
        baseline_version: artifact.baseline_version,
        baseline_manifest_sha256: artifact.baseline_manifest_sha256.clone(),
        section_key: artifact.section_key.clone(),
        package_path: artifact.package_path.clone(),
        envelope_key: artifact.envelope_key.clone(),
        language: artifact.language.clone(),
        authoring_mode: artifact.authoring_mode,
        media_type: artifact.media_type.clone(),
        classifications: artifact.classifications.clone(),
        scope_record_ids: artifact.scope_record_ids.clone(),
        content_sha256: artifact.content_sha256.clone(),
        size_bytes: artifact.size_bytes,
        exact_inputs: artifact.exact_inputs.clone(),
        generation_policy_id: artifact.generation_policy_id.clone(),
        generation_policy_version: artifact.generation_policy_version,
        generation_policy_sha256: artifact.generation_policy_sha256.clone(),
        provenance: artifact.provenance.clone(),
        created_at: artifact.created_at.clone(),
    }
}

fn requirement_manifest_from_view(requirement: &GenerationRequirement) -> RequirementManifest {
    RequirementManifest {
        schema_version: 1,
        requirement_id: requirement.requirement_id.clone(),
        kind: requirement.kind,
        record: requirement.record.clone(),
        evidence: requirement.evidence.clone(),
        authored_fields: requirement.authored_fields.clone(),
        mandatory: requirement.mandatory,
        section_key: requirement.section_key.clone(),
        package_path: requirement.package_path.clone(),
        envelope_key: requirement.envelope_key.clone(),
        language: requirement.language.clone(),
        authoring_mode: requirement.authoring_mode,
        availability: requirement.availability,
        generated_artifact: requirement.generated_artifact.clone(),
        unchanged_source_artifact: requirement.unchanged_source_artifact.clone(),
        content_sha256: requirement.content_sha256.clone(),
        size_bytes: requirement.size_bytes,
        calculation_references: requirement.calculation_references.clone(),
        review_references: requirement.review_references.clone(),
        decision_references: requirement.decision_references.clone(),
    }
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
    use crate::tender_store::TenderRecordAuthority;

    #[test]
    fn package_paths_use_nfkc_and_full_unicode_case_folding() {
        assert_eq!(
            package_collision_key("01-Technical/Ｍaße.docx"),
            package_collision_key("01-technical/masse.docx")
        );
    }

    #[test]
    fn material_fields_require_evidence_or_the_exact_bound_calculation_manifest() {
        let run_id = "a".repeat(32);
        let manifest_sha256 = "b".repeat(64);
        let field = TenderRecordField {
            name: "approved_calculated_total".into(),
            value: Some("1250.00".into()),
            basis_kind: TenderRecordBasisKind::CalculationRun,
            basis_reference: Some(run_id.clone()),
            basis_description: Some("Exact approved total".into()),
            basis_authority: Some(TenderRecordAuthority {
                authority_id: run_id.clone(),
                kind: TenderRecordAuthorityKind::CalculationRun,
                value: "1250.00".into(),
                description: "Exact approved total".into(),
                manifest_sha256: Some(manifest_sha256.clone()),
                tender_revision: 1,
                created_by: "quantix-host".into(),
                created_at: "2026-08-12T00:00:00Z".into(),
            }),
            original_expression: None,
            normalized_value: None,
            timezone: None,
            uncertainty: None,
            evidence: Vec::new(),
        };
        let exact_binding = CoordinatedBidBaselineBinding {
            category: CoordinatedBidBaselineCategory::Commercial,
            kind: CoordinatedBidBaselineBindingKind::CalculationManifest,
            reference_id: run_id,
            version: 1,
            manifest_sha256,
            source: "approved Calculation Run".into(),
            summary: "Exact manifest".into(),
            supporting_review_id: None,
            approval_id: None,
        };
        assert!(material_generation_field_is_valid(
            &field,
            std::slice::from_ref(&exact_binding)
        ));

        let mut empty_material = field.clone();
        empty_material.value = None;
        empty_material.normalized_value = None;
        assert!(!material_generation_field_is_valid(
            &empty_material,
            std::slice::from_ref(&exact_binding)
        ));

        let mut wrong_binding = exact_binding;
        wrong_binding.manifest_sha256 = "c".repeat(64);
        assert!(!material_generation_field_is_valid(
            &field,
            &[wrong_binding]
        ));

        let normalized_without_evidence = TenderRecordField {
            name: "submission_deadline".into(),
            value: None,
            basis_kind: TenderRecordBasisKind::Evidence,
            basis_reference: None,
            basis_description: None,
            basis_authority: None,
            original_expression: Some("1 June 2026".into()),
            normalized_value: Some("2026-06-01".into()),
            timezone: Some("Africa/Cairo".into()),
            uncertainty: None,
            evidence: Vec::new(),
        };
        assert!(!material_generation_field_is_valid(
            &normalized_without_evidence,
            &[]
        ));
    }
}
