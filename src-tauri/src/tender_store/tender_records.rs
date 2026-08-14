use std::{collections::HashSet, fs, path::Path};

use garde::Validate;
use jiff::Timestamp;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::{
    agent_runtime::{
        permissions::{derive_pre_bid_data_grant, permission_duration, PreBidDataGrantRequest},
        AgentProfileVersionView, AgentResourceBudget, AgentRunInspection, AgentRunPermissions,
        AgentTaskInputReference, BootstrapRole, DataClassification, PendingProviderEvent,
        PreparedAgentRun, ProviderEventKind, TenderTaskView, VerificationStatus,
    },
    document_parsing::EvidenceLocation,
    tender_intake::SourceRelationshipKind,
};

use super::{
    agent_records::{
        ensure_agent_run_capacity, insert_event, insert_profile_version, insert_task, load_profile,
        load_task, load_thread_exposure, update_profile_head,
    },
    append_audit_event, random_identifier, sql_error, sqlite_timestamp, valid_identifier,
    RawEvidenceLocation, TenderCommandError, TenderErrorCode, TenderId, TenderStore,
};

pub(crate) const MAX_RECORD_EVIDENCE_INPUTS: usize = 256;
pub(crate) const MAX_RECORDS_PER_RESULT: usize = 256;
pub(crate) const MAX_DECISION_RECORD_INVENTORY: usize = MAX_RECORDS_PER_RESULT;
pub(crate) const MAX_RECORD_FIELDS: usize = 64;
pub(crate) const MAX_RECORD_CONTRADICTIONS: usize = 32;
pub(crate) const RECORD_EXTRACTION_CAPABILITY: &str = "extract_evidence_backed_tender_records";
pub(crate) const RECORD_EXTRACTION_SCOPE: &str = "tender_sources";
pub(crate) const RECORD_EXTRACTION_ACTION: &str = "propose_tender_records";
pub(crate) const RECORD_REVIEW_CAPABILITY: &str = "independently_review_tender_record";
pub(crate) const RECORD_REVIEW_SCOPE: &str = "tender_record";
pub(crate) const RECORD_REVIEW_ACTION: &str = "review_exact_tender_record";
const MAX_RECORD_SOURCE_RELATIONSHIPS: usize = 512;
const MAX_RECORD_VERSIONS: usize = 1_000;
const MAX_RECORD_REVIEWS: usize = 64;
const MAX_EXPANDED_RECORD_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RECORD_PAGE_ITEMS: u32 = 4;
const MAX_RECORD_PAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_RECORD_AUTHORITIES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct TenderEvidenceReference {
    #[garde(length(bytes, min = 32, max = 32))]
    pub artifact_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
    #[garde(range(min = 1))]
    pub ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunTenderRecordExtractionCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(min = 1, max = 256), dive)]
    pub evidence: Vec<TenderEvidenceReference>,
    #[garde(length(max = 256), dive)]
    pub authorities: Vec<TenderRecordAuthorityReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct TenderRecordAuthorityReference {
    #[garde(length(bytes, min = 32, max = 32))]
    pub authority_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateTenderEngineerEntryCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub value: String,
    #[garde(length(bytes, min = 1, max = 2000))]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RunTenderRecordReviewCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub record_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectTenderRecordsCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, max = 128))]
    pub cursor: Option<String>,
    #[garde(range(min = 1, max = 4))]
    pub limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecordReviewOutcome {
    Verified,
    Rejected,
    ApprovedAssumption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GenerationRequirementKind {
    MandatoryRequirement,
    Deliverable,
    AddendumInstruction,
    Signature,
    FormField,
    ExecutionRequirement,
    RequiredFile,
}

impl GenerationRequirementKind {
    pub(super) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "mandatory_requirement" => Ok(Self::MandatoryRequirement),
            "deliverable" => Ok(Self::Deliverable),
            "addendum_instruction" => Ok(Self::AddendumInstruction),
            "signature" => Ok(Self::Signature),
            "form_field" => Ok(Self::FormField),
            "execution_requirement" => Ok(Self::ExecutionRequirement),
            "required_file" => Ok(Self::RequiredFile),
            _ => Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::MandatoryRequirement => "mandatory_requirement",
            Self::Deliverable => "deliverable",
            Self::AddendumInstruction => "addendum_instruction",
            Self::Signature => "signature",
            Self::FormField => "form_field",
            Self::ExecutionRequirement => "execution_requirement",
            Self::RequiredFile => "required_file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum GenerationAuthoringMode {
    Docx,
    Xlsx,
    UnchangedSource,
    Unsupported,
}

impl GenerationAuthoringMode {
    pub(super) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "docx" => Ok(Self::Docx),
            "xlsx" => Ok(Self::Xlsx),
            "unchanged_source" => Ok(Self::UnchangedSource),
            "unsupported" => Ok(Self::Unsupported),
            _ => Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::UnchangedSource => "unchanged_source",
            Self::Unsupported => "unsupported",
        }
    }
}

impl TenderRecordReviewOutcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Rejected => "rejected",
            Self::ApprovedAssumption => "approved_assumption",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "verified" => Ok(Self::Verified),
            "rejected" => Ok(Self::Rejected),
            "approved_assumption" => Ok(Self::ApprovedAssumption),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecordEngineerDecisionKind {
    Verify,
    Reject,
    ApproveAssumption,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DecideTenderRecordCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub record_id: String,
    #[garde(range(min = 1))]
    pub version: u32,
    #[garde(skip)]
    pub decision: TenderRecordEngineerDecisionKind,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecordKind {
    Requirement,
    EvaluationCriterion,
    Deliverable,
    Deadline,
    Form,
    Clause,
    Risk,
    Assumption,
    TenderQuery,
    ProjectCharacteristic,
}

impl TenderRecordKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Requirement => "requirement",
            Self::EvaluationCriterion => "evaluation_criterion",
            Self::Deliverable => "deliverable",
            Self::Deadline => "deadline",
            Self::Form => "form",
            Self::Clause => "clause",
            Self::Risk => "risk",
            Self::Assumption => "assumption",
            Self::TenderQuery => "tender_query",
            Self::ProjectCharacteristic => "project_characteristic",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "requirement" => Ok(Self::Requirement),
            "evaluation_criterion" => Ok(Self::EvaluationCriterion),
            "deliverable" => Ok(Self::Deliverable),
            "deadline" => Ok(Self::Deadline),
            "form" => Ok(Self::Form),
            "clause" => Ok(Self::Clause),
            "risk" => Ok(Self::Risk),
            "assumption" => Ok(Self::Assumption),
            "tender_query" => Ok(Self::TenderQuery),
            "project_characteristic" => Ok(Self::ProjectCharacteristic),
            _ => Err(super::TenderCommandError::new(
                super::TenderErrorCode::IntegrityFailed,
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecordBasisKind {
    Evidence,
    Assumption,
    TenderQuery,
    CalculationRun,
    EngineerEntry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecordAuthorityKind {
    EngineerEntry,
    CalculationRun,
}

impl TenderRecordAuthorityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::EngineerEntry => "engineer_entry",
            Self::CalculationRun => "calculation_run",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "engineer_entry" => Ok(Self::EngineerEntry),
            "calculation_run" => Ok(Self::CalculationRun),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordAuthority {
    pub authority_id: String,
    pub kind: TenderRecordAuthorityKind,
    pub value: String,
    pub description: String,
    pub manifest_sha256: Option<String>,
    pub tender_revision: u32,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecordTrustClass {
    AiProposal,
    DeterministicFact,
    Verified,
    EngineerVerified,
    ApprovedAssumption,
    UnresolvedGap,
    PriorDecision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordEvidence {
    pub reference: TenderEvidenceReference,
    pub package_path: String,
    pub location: EvidenceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordSourceRelationship {
    pub relationship_id: String,
    pub prior_artifact_id: String,
    pub prior_version: u32,
    pub replacement_artifact_id: String,
    pub replacement_version: u32,
    pub relationship_kind: SourceRelationshipKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordReview {
    pub review_id: String,
    pub outcome: TenderRecordReviewOutcome,
    pub rationale: String,
    pub reviewer_kind: String,
    pub reviewer_run_id: Option<String>,
    pub decided_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordField {
    pub name: String,
    pub value: Option<String>,
    pub basis_kind: TenderRecordBasisKind,
    pub basis_reference: Option<String>,
    pub basis_description: Option<String>,
    pub basis_authority: Option<TenderRecordAuthority>,
    pub original_expression: Option<String>,
    pub normalized_value: Option<String>,
    pub timezone: Option<String>,
    pub uncertainty: Option<String>,
    pub evidence: Vec<TenderRecordEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordGenerationInstruction {
    pub kind: GenerationRequirementKind,
    pub mandatory: bool,
    pub section_key: String,
    pub package_path: String,
    pub envelope_key: String,
    pub language: String,
    pub authoring_mode: GenerationAuthoringMode,
    pub requested_authoring_format: Option<String>,
    pub evidence: Vec<TenderRecordEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordContradiction {
    pub field_name: String,
    pub summary: String,
    pub evidence: Vec<TenderRecordEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordInspection {
    pub record_id: String,
    pub stable_key: String,
    pub version: u32,
    pub kind: TenderRecordKind,
    pub title: String,
    pub verification_status: VerificationStatus,
    pub trust_class: TenderRecordTrustClass,
    pub fields: Vec<TenderRecordField>,
    pub generation_instruction: Option<TenderRecordGenerationInstruction>,
    pub contradictions: Vec<TenderRecordContradiction>,
    pub source_relationships: Vec<TenderRecordSourceRelationship>,
    pub reviews: Vec<TenderRecordReview>,
    pub author_run_id: String,
    pub author_profile_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordExtractionResult {
    pub run: AgentRunInspection,
    pub published_record_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordReviewResult {
    pub run: AgentRunInspection,
    pub record: TenderRecordInspection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordDecisionResult {
    pub record: TenderRecordInspection,
    pub review: TenderRecordReview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderRecordPage {
    pub records: Vec<TenderRecordInspection>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordCandidateBatch {
    pub records: Vec<TenderRecordCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordCandidate {
    pub stable_key: String,
    pub kind: TenderRecordKind,
    pub title: String,
    pub generation_instruction: Option<TenderRecordGenerationInstructionCandidate>,
    pub fields: Vec<TenderRecordFieldCandidate>,
    pub contradictions: Vec<TenderRecordContradictionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordGenerationInstructionCandidate {
    pub kind: GenerationRequirementKind,
    pub mandatory: bool,
    pub section_key: String,
    pub package_path: String,
    pub envelope_key: String,
    pub language: String,
    pub authoring_mode: GenerationAuthoringMode,
    pub requested_authoring_format: Option<String>,
    pub evidence: Vec<TenderEvidenceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordFieldCandidate {
    pub name: String,
    pub value: Option<String>,
    pub basis_kind: TenderRecordBasisKind,
    pub basis_reference: Option<String>,
    pub basis_description: Option<String>,
    pub original_expression: Option<String>,
    pub normalized_value: Option<String>,
    pub timezone: Option<String>,
    pub uncertainty: Option<String>,
    pub evidence: Vec<TenderEvidenceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordContradictionCandidate {
    pub field_name: String,
    pub summary: String,
    pub evidence: Vec<TenderEvidenceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TenderRecordReviewCandidate {
    pub outcome: TenderRecordReviewOutcome,
    pub rationale: String,
}

pub(crate) fn record_extraction_profile(profile_id: String) -> AgentProfileVersionView {
    AgentProfileVersionView {
        profile_id,
        version: 3,
        identity: "Tender Analyst".into(),
        profession: "Tender Engineer".into(),
        seniority: "Senior".into(),
        capabilities: vec![RECORD_EXTRACTION_CAPABILITY.into()],
        objective: "Extract structured Tender Records and any Tender-specific submission instruction from exact Evidence without filling gaps.".into(),
        behavior: "Separate source wording, interpretation, contradictions, uncertainty, and missing information.".into(),
        skepticism: "Treat every material claim as unsupported until exact provenance is supplied.".into(),
        risk_tolerance: "Low tolerance for invented or weakly attributed Tender facts.".into(),
        instructions: "Extract only structured pre-bid Tender records supported by the supplied exact Evidence. When the Evidence explicitly controls submission generation, publish one typed generation_instruction containing its requirement kind, mandatory status, section, exact package path, envelope, language, supported authoring mode, and exact Evidence; otherwise omit it. Preserve original-language authority, label translations as derived, represent absence as an Assumption or Tender Query, surface contradictions, and make no approval decision.".into(),
        output_contract_json: record_extraction_output_contract(),
        review_policy: "Every proposed record requires independent review or exact Engineer User verification. Missing provenance blocks verification.".into(),
        permissions: record_extraction_permissions(),
        prohibited_actions: standard_prohibited_actions(),
        resource_budget: record_extraction_budget(),
    }
}

pub(crate) fn record_review_profile(profile_id: String) -> AgentProfileVersionView {
    AgentProfileVersionView {
        profile_id,
        version: 3,
        identity: "Independent Reviewer".into(),
        profession: "Tender Assurance Engineer".into(),
        seniority: "Senior".into(),
        capabilities: vec![RECORD_REVIEW_CAPABILITY.into()],
        objective: "Independently review one exact proposed Tender Record version.".into(),
        behavior: "Review without editing the target and report only attributable findings.".into(),
        skepticism: "Challenge provenance, contradictions, assumptions, and unsupported certainty.".into(),
        risk_tolerance: "Very low tolerance for unverified material Tender facts.".into(),
        instructions: "Review the supplied immutable Tender Record version, including any typed submission generation instruction, against its exact authoritative Evidence. Return only an attributable verification or rejection outcome. Do not rewrite the author target and never fill missing provenance with plausible content.".into(),
        output_contract_json: record_review_output_contract(),
        review_policy: "Verification is allowed only when every material field has an eligible exact provenance basis. Assumptions and unresolved Tender Queries require an Engineer decision, not independent verification.".into(),
        permissions: record_review_permissions(),
        prohibited_actions: standard_prohibited_actions(),
        resource_budget: record_review_budget(),
    }
}

fn standard_prohibited_actions() -> Vec<String> {
    vec![
        "approve_tender_decision".into(),
        "mutate_tender_store_directly".into(),
        "perform_external_action".into(),
        "access_secret_data".into(),
    ]
}

fn record_review_task(
    task_id: String,
    record_id: &str,
    version: u32,
    deadline: String,
    profile: &AgentProfileVersionView,
) -> TenderTaskView {
    TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: "Independently verify or reject the supplied exact immutable Tender Record version against its authoritative Evidence without editing it.".into(),
        exact_inputs: vec![AgentTaskInputReference {
            kind: "tender_record_version".into(),
            reference: record_id.into(),
            version,
        }],
        output_contract_json: profile.output_contract_json.clone(),
        review_policy: profile.review_policy.clone(),
        deadline,
        permissions: record_review_permissions(),
        resource_budget: profile.resource_budget.clone(),
    }
}

pub(crate) fn record_extraction_task(
    task_id: String,
    tender_id: &str,
    tender_revision: u32,
    evidence: &[TenderEvidenceReference],
    authorities: &[TenderRecordAuthority],
    deadline: String,
    profile: &AgentProfileVersionView,
) -> TenderTaskView {
    let mut exact_inputs = vec![AgentTaskInputReference {
        kind: "tender_revision".into(),
        reference: tender_id.into(),
        version: tender_revision,
    }];
    exact_inputs.extend(evidence.iter().map(|reference| AgentTaskInputReference {
        kind: "source_evidence".into(),
        reference: format!("{}#{}", reference.artifact_id, reference.ordinal),
        version: reference.version,
    }));
    exact_inputs.extend(authorities.iter().map(|authority| {
        AgentTaskInputReference {
            kind: match authority.kind {
                TenderRecordAuthorityKind::EngineerEntry => "engineer_entry",
                TenderRecordAuthorityKind::CalculationRun => "approved_calculation_run",
            }
            .into(),
            reference: authority.authority_id.clone(),
            version: authority.tender_revision,
        }
    }));
    TenderTaskView {
        task_id,
        profile_id: profile.profile_id.clone(),
        profile_version: profile.version,
        objective: "Propose evidence-backed requirements, evaluation criteria, deliverables, deadlines, forms, clauses, risks, assumptions, Tender Queries, and project characteristics from the supplied exact Evidence.".into(),
        exact_inputs,
        output_contract_json: profile.output_contract_json.clone(),
        review_policy: profile.review_policy.clone(),
        deadline,
        permissions: record_extraction_permissions(),
        resource_budget: profile.resource_budget.clone(),
    }
}

fn record_extraction_permissions() -> AgentRunPermissions {
    AgentRunPermissions {
        data_scopes: vec![RECORD_EXTRACTION_SCOPE.into()],
        data_classifications: vec![DataClassification::TenderInternal],
        allowed_actions: vec![RECORD_EXTRACTION_ACTION.into()],
        allowed_tools: Vec::new(),
        network_allowed: false,
        workspace_write_allowed: true,
    }
}

fn record_extraction_budget() -> AgentResourceBudget {
    #[cfg(feature = "runtime-fixture")]
    let duration_seconds = 8;
    #[cfg(not(feature = "runtime-fixture"))]
    let duration_seconds = 120;
    AgentResourceBudget {
        provider_turns: 1,
        duration_seconds,
        output_bytes: 256 * 1024,
    }
}

fn record_review_permissions() -> AgentRunPermissions {
    AgentRunPermissions {
        data_scopes: vec![RECORD_REVIEW_SCOPE.into()],
        data_classifications: vec![DataClassification::TenderInternal],
        allowed_actions: vec![RECORD_REVIEW_ACTION.into()],
        allowed_tools: Vec::new(),
        network_allowed: false,
        workspace_write_allowed: true,
    }
}

fn record_review_budget() -> AgentResourceBudget {
    #[cfg(feature = "runtime-fixture")]
    let duration_seconds = 8;
    #[cfg(not(feature = "runtime-fixture"))]
    let duration_seconds = 120;
    AgentResourceBudget {
        provider_turns: 1,
        duration_seconds,
        output_bytes: 16 * 1024,
    }
}

fn record_review_output_contract() -> String {
    serde_json_canonicalizer::to_string(&json!({
        "additionalProperties": false,
        "properties": {
            "outcome": { "enum": ["verified", "rejected"] },
            "rationale": { "type": "string", "minLength": 1, "maxLength": 4000 }
        },
        "required": ["outcome", "rationale"],
        "type": "object"
    }))
    .expect("static Tender Record review output contract is canonical JSON")
}

fn record_extraction_output_contract() -> String {
    serde_json_canonicalizer::to_string(&serde_json::json!({
        "additionalProperties": false,
        "properties": {
            "records": {
                "items": {
                    "additionalProperties": false,
                    "properties": {
                        "contradictions": {
                            "items": {
                                "additionalProperties": false,
                                "properties": {
                                    "evidence": { "$ref": "#/$defs/evidence_list" },
                                    "field_name": { "$ref": "#/$defs/name" },
                                    "summary": { "maxLength": 2000, "minLength": 1, "type": "string" }
                                },
                                "required": ["field_name", "summary", "evidence"],
                                "type": "object"
                            },
                            "maxItems": MAX_RECORD_CONTRADICTIONS,
                            "type": "array"
                        },
                        "fields": {
                            "items": {
                                "additionalProperties": false,
                                "properties": {
                                    "basis_description": { "type": ["string", "null"], "maxLength": 2000 },
                                    "basis_kind": { "enum": ["evidence", "assumption", "tender_query", "calculation_run", "engineer_entry"] },
                                    "basis_reference": { "type": ["string", "null"], "maxLength": 100 },
                                    "evidence": { "$ref": "#/$defs/evidence_list" },
                                    "name": { "$ref": "#/$defs/name" },
                                    "normalized_value": { "type": ["string", "null"], "maxLength": 2000 },
                                    "original_expression": { "type": ["string", "null"], "maxLength": 2000 },
                                    "timezone": { "type": ["string", "null"], "maxLength": 100 },
                                    "uncertainty": { "type": ["string", "null"], "maxLength": 2000 },
                                    "value": { "type": ["string", "null"], "maxLength": 4000 }
                                },
                                "required": ["name", "value", "basis_kind", "basis_reference", "basis_description", "original_expression", "normalized_value", "timezone", "uncertainty", "evidence"],
                                "type": "object"
                            },
                            "maxItems": MAX_RECORD_FIELDS,
                            "minItems": 1,
                            "type": "array"
                        },
                        "generation_instruction": {
                            "oneOf": [
                                { "type": "null" },
                                {
                                    "additionalProperties": false,
                                    "properties": {
                                        "authoring_mode": { "enum": ["docx", "xlsx", "unchanged_source", "unsupported"] },
                                        "envelope_key": { "maxLength": 200, "minLength": 1, "type": "string" },
                                        "evidence": { "$ref": "#/$defs/evidence_list" },
                                        "kind": { "enum": ["mandatory_requirement", "deliverable", "addendum_instruction", "signature", "form_field", "execution_requirement", "required_file"] },
                                        "language": { "maxLength": 100, "minLength": 1, "type": "string" },
                                        "mandatory": { "type": "boolean" },
                                        "package_path": { "maxLength": 1000, "minLength": 1, "type": "string" },
                                        "requested_authoring_format": { "type": ["string", "null"], "maxLength": 200 },
                                        "section_key": { "maxLength": 200, "minLength": 1, "type": "string" }
                                    },
                                    "required": ["kind", "mandatory", "section_key", "package_path", "envelope_key", "language", "authoring_mode", "requested_authoring_format", "evidence"],
                                    "type": "object"
                                }
                            ]
                        },
                        "kind": { "enum": ["requirement", "evaluation_criterion", "deliverable", "deadline", "form", "clause", "risk", "assumption", "tender_query", "project_characteristic"] },
                        "stable_key": { "maxLength": 100, "minLength": 1, "pattern": "^[a-z0-9][a-z0-9_-]*$", "type": "string" },
                        "title": { "maxLength": 500, "minLength": 1, "type": "string" }
                    },
                    "required": ["stable_key", "kind", "title", "generation_instruction", "fields", "contradictions"],
                    "type": "object"
                },
                "maxItems": MAX_RECORDS_PER_RESULT,
                "minItems": 1,
                "type": "array"
            }
        },
        "required": ["records"],
        "type": "object",
        "$defs": {
            "evidence": {
                "additionalProperties": false,
                "properties": {
                    "artifact_id": { "maxLength": 32, "minLength": 32, "type": "string" },
                    "ordinal": { "minimum": 1, "type": "integer" },
                    "version": { "minimum": 1, "type": "integer" }
                },
                "required": ["artifact_id", "version", "ordinal"],
                "type": "object"
            },
            "evidence_list": { "items": { "$ref": "#/$defs/evidence" }, "maxItems": 32, "type": "array" },
            "name": { "maxLength": 100, "minLength": 1, "pattern": "^[a-z0-9][a-z0-9_-]*$", "type": "string" }
        }
    }))
    .expect("static Tender Record output contract is canonical JSON")
}

pub(super) fn insert_engineer_entry(
    transaction: &Transaction<'_>,
    tender_revision: u32,
    value: &str,
    description: &str,
    created_at: &str,
) -> Result<TenderRecordAuthority, TenderCommandError> {
    let value = value.trim();
    let description = description.trim();
    if value.is_empty()
        || value.len() > 4_000
        || description.is_empty()
        || description.len() > 2_000
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let authority_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM tender_record_authorities",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if usize::try_from(authority_count)
        .ok()
        .is_none_or(|count| count >= MAX_RECORD_AUTHORITIES)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let authority = TenderRecordAuthority {
        authority_id: random_identifier(transaction)?,
        kind: TenderRecordAuthorityKind::EngineerEntry,
        value: value.into(),
        description: description.into(),
        manifest_sha256: None,
        tender_revision,
        created_by: "engineer_user".into(),
        created_at: created_at.into(),
    };
    transaction
        .execute(
            "INSERT INTO tender_record_authorities (
               authority_id, kind, value, description, manifest_sha256,
               tender_revision, created_by, created_at
             ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)",
            params![
                authority.authority_id,
                authority.kind.as_str(),
                authority.value,
                authority.description,
                authority.tender_revision,
                authority.created_by,
                authority.created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(authority)
}

impl TenderStore {
    pub(crate) fn prepare_tender_record_extraction_run(
        &mut self,
        tender_id: &TenderId,
        evidence: &[TenderEvidenceReference],
        authority_references: &[TenderRecordAuthorityReference],
        manager_intake_run_id: Option<&str>,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        if !self.active_change_allows_record_extraction(evidence)? {
            self.require_change_intake_writable()?;
        }
        if evidence.is_empty() || evidence.len() > MAX_RECORD_EVIDENCE_INPUTS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut unique = HashSet::new();
        if evidence.iter().any(|reference| {
            !valid_identifier(&reference.artifact_id)
                || reference.version == 0
                || reference.ordinal == 0
                || !unique.insert((
                    reference.artifact_id.clone(),
                    reference.version,
                    reference.ordinal,
                ))
        }) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let authority_ids = authority_references
            .iter()
            .map(|reference| reference.authority_id.as_str())
            .collect::<HashSet<_>>();
        if authority_references.len() > MAX_RECORD_AUTHORITIES
            || authority_ids.len() != authority_references.len()
            || authority_references
                .iter()
                .any(|reference| !valid_identifier(&reference.authority_id))
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
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
            let tender_revision: u32 = transaction
                .query_row(
                    "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                    [tender_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let created_at = sqlite_timestamp(&transaction)?;
            let authorities =
                load_record_authorities_by_references(&transaction, authority_references)?;
            let has_unresolved_indeterminate: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs
                       JOIN tender_tasks ON tender_tasks.task_id = agent_runs.task_id
                       WHERE status = 'indeterminate'
                         AND (?1 IS NULL OR EXISTS (
                           SELECT 1 FROM json_each(tender_tasks.exact_inputs_json)
                           WHERE json_extract(value, '$.kind') = 'manager_intake_run'
                             AND json_extract(value, '$.reference') = ?1
                             AND json_extract(value, '$.version') = 1
                         ))
                         AND NOT EXISTS (
                           SELECT 1 FROM agent_run_recovery_dispositions
                           WHERE agent_run_recovery_dispositions.run_id = agent_runs.run_id
                         )
                     )",
                    [manager_intake_run_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if has_unresolved_indeterminate {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let profile_id: String = transaction
                .query_row(
                    "SELECT profile_id FROM agent_profiles WHERE stable_identity = ?1",
                    [BootstrapRole::TenderAnalyst.stable_identity()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let profile = record_extraction_profile(profile_id.clone());
            let profile_version_exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_profile_versions
                       WHERE profile_id = ?1 AND version = ?2
                     )",
                    params![profile_id, profile.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if profile_version_exists {
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
            let change_recovery = TenderStore::active_change_record_recovery_context(&transaction)?;
            let mut task = record_extraction_task(
                random_identifier(&transaction)?,
                tender_id.as_str(),
                tender_revision,
                evidence,
                &authorities,
                deadline,
                &profile,
            );
            if let Some(change_recovery) = &change_recovery {
                task.exact_inputs.push(AgentTaskInputReference {
                    kind: "change_assessment".into(),
                    reference: change_recovery.assessment_id.clone(),
                    version: 1,
                });
            }
            if let Some(intake_run_id) = manager_intake_run_id {
                let current: bool = transaction
                    .query_row(
                        "SELECT EXISTS(
                           SELECT 1 FROM manager_intake_runs
                           WHERE intake_run_id = ?1 AND stage = 'extracting_tender_facts'
                         )",
                        [intake_run_id],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if !current {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                task.exact_inputs.push(AgentTaskInputReference {
                    kind: "manager_intake_run".into(),
                    reference: intake_run_id.to_owned(),
                    version: 1,
                });
            }
            insert_task(&transaction, &task, &created_at)?;
            let mut payload = record_extraction_data_view(
                &transaction,
                tender_id,
                tender_revision,
                evidence,
                &authorities,
            )?;
            if let Some(change_recovery) = &change_recovery {
                payload["change_assessment"] = json!({
                    "assessment_id": change_recovery.assessment_id,
                    "allowed_stable_keys": change_recovery.allowed_stable_keys,
                    "prior_records": change_recovery.prior_records,
                    "instruction": "Publish successors only for these exact impacted stable keys using the supplied replacement Evidence.",
                });
            }
            let (permission_grant, materialized_workspace) =
                derive_pre_bid_data_grant(PreBidDataGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home,
                    tender_id: tender_id.as_str(),
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                    data_scope: RECORD_EXTRACTION_SCOPE,
                    allowed_action: RECORD_EXTRACTION_ACTION,
                    relative_path: "tender-evidence-v1.json",
                    view_id: "tender-evidence-v1",
                    payload: &payload,
                })?;
            if materialized_workspace != workspace
                || permission_duration(&permission_grant, jiff::Timestamp::now())
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
                    .is_zero()
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }

            let existing_provider_thread: Option<(String, String)> = transaction
                .query_row(
                    "SELECT thread_ref, status FROM provider_threads
                     WHERE profile_id = ?1 AND profile_version = ?2
                       AND status IN ('active', 'archive_pending')",
                    params![profile.profile_id, profile.version],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (provider_thread_ref, provider_thread_to_archive) = match existing_provider_thread {
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
                Some(_) => {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
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
                    summary: "Agent Run started".into(),
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
                "agent_run_started",
                tender_revision,
                json!({
                    "profile_id": profile.profile_id,
                    "profile_version": profile.version.to_string(),
                    "retry_of_run_id": Value::Null,
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

    pub(crate) fn prepare_tender_record_review_run(
        &mut self,
        tender_id: &TenderId,
        record_id: &str,
        version: u32,
        manager_intake_run_id: Option<&str>,
    ) -> Result<PreparedAgentRun, TenderCommandError> {
        if !self.active_change_allows_record_governance(record_id)? {
            self.require_change_intake_writable()?;
        }
        if !valid_identifier(record_id) || version == 0 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
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
            let tender_revision: u32 = transaction
                .query_row(
                    "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                    [tender_id.as_str()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            let (author_profile_id, current_version, review_count): (String, u32, i64) = transaction
                .query_row(
                    "SELECT agent_runs.profile_id, tender_record_heads.current_version,
                            (SELECT COUNT(*) FROM tender_record_reviews
                             WHERE tender_record_reviews.record_id = tender_record_versions.record_id
                               AND tender_record_reviews.record_version = tender_record_versions.version)
                     FROM tender_record_versions
                     JOIN tender_record_heads USING (record_id)
                     JOIN agent_runs ON agent_runs.run_id = tender_record_versions.author_run_id
                     WHERE tender_record_versions.record_id = ?1
                       AND tender_record_versions.version = ?2",
                    params![record_id, version],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(sql_error)?
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
            if current_version != version || review_count != 0 {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let created_at = sqlite_timestamp(&transaction)?;
            let has_unresolved_indeterminate: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_runs
                       JOIN tender_tasks ON tender_tasks.task_id = agent_runs.task_id
                       WHERE status = 'indeterminate'
                         AND (?1 IS NULL OR EXISTS (
                           SELECT 1 FROM json_each(tender_tasks.exact_inputs_json)
                           WHERE json_extract(value, '$.kind') = 'manager_intake_run'
                             AND json_extract(value, '$.reference') = ?1
                             AND json_extract(value, '$.version') = 1
                         ))
                         AND NOT EXISTS (
                           SELECT 1 FROM agent_run_recovery_dispositions
                           WHERE agent_run_recovery_dispositions.run_id = agent_runs.run_id
                         )
                     )",
                    [manager_intake_run_id],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if has_unresolved_indeterminate {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let profile_id: String = transaction
                .query_row(
                    "SELECT profile_id FROM agent_profiles WHERE stable_identity = ?1",
                    [BootstrapRole::IndependentReviewer.stable_identity()],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if profile_id == author_profile_id {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let profile = record_review_profile(profile_id.clone());
            let profile_version_exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM agent_profile_versions
                       WHERE profile_id = ?1 AND version = ?2
                     )",
                    params![profile_id, profile.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if profile_version_exists {
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
            let mut task = record_review_task(
                random_identifier(&transaction)?,
                record_id,
                version,
                deadline,
                &profile,
            );
            if let Some(intake_run_id) = manager_intake_run_id {
                let current: bool = transaction
                    .query_row(
                        "SELECT EXISTS(
                           SELECT 1 FROM manager_intake_runs
                           WHERE intake_run_id = ?1 AND stage = 'reviewing_tender_facts'
                         )",
                        [intake_run_id],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if !current {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                task.exact_inputs.push(AgentTaskInputReference {
                    kind: "manager_intake_run".into(),
                    reference: intake_run_id.to_owned(),
                    version: 1,
                });
            }
            insert_task(&transaction, &task, &created_at)?;
            let payload = record_review_data_view(&transaction, record_id, version)?;
            let (permission_grant, materialized_workspace) =
                derive_pre_bid_data_grant(PreBidDataGrantRequest {
                    run_id: &run_id,
                    grant_id: random_identifier(&transaction)?,
                    application_home,
                    tender_id: tender_id.as_str(),
                    profile: &profile,
                    task: &task,
                    issued_at: &created_at,
                    data_scope: RECORD_REVIEW_SCOPE,
                    allowed_action: RECORD_REVIEW_ACTION,
                    relative_path: "tender-record-review-v1.json",
                    view_id: "tender-record-review-v1",
                    payload: &payload,
                })?;
            if materialized_workspace != workspace
                || permission_duration(&permission_grant, jiff::Timestamp::now())
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?
                    .is_zero()
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let existing_provider_thread: Option<(String, String)> = transaction
                .query_row(
                    "SELECT thread_ref, status FROM provider_threads
                     WHERE profile_id = ?1 AND profile_version = ?2
                       AND status IN ('active', 'archive_pending')",
                    params![profile.profile_id, profile.version],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sql_error)?;
            let (provider_thread_ref, provider_thread_to_archive) = match existing_provider_thread {
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
                    summary: "Independent Tender Record review started".into(),
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
                "tender_record_review_started",
                tender_revision,
                json!({
                    "record_id": record_id,
                    "record_version": version.to_string(),
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

    pub(crate) fn validate_tender_record_candidate(
        &self,
        task: &TenderTaskView,
        payload_json: &str,
    ) -> Result<TenderRecordCandidateBatch, TenderCommandError> {
        let candidate: TenderRecordCandidateBatch = serde_json::from_str(payload_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if canonical_json(&candidate)? != payload_json
            || candidate.records.is_empty()
            || candidate.records.len() > MAX_RECORDS_PER_RESULT
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let allowed_evidence = task
            .exact_inputs
            .iter()
            .filter(|input| input.kind == "source_evidence")
            .map(|input| {
                let (artifact_id, ordinal) = input
                    .reference
                    .split_once('#')
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                Ok(TenderEvidenceReference {
                    artifact_id: artifact_id.into(),
                    version: input.version,
                    ordinal: ordinal
                        .parse()
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                })
            })
            .collect::<Result<HashSet<_>, TenderCommandError>>()?;
        let allowed_authorities = task
            .exact_inputs
            .iter()
            .filter_map(|input| {
                let kind = match input.kind.as_str() {
                    "engineer_entry" => TenderRecordAuthorityKind::EngineerEntry,
                    "approved_calculation_run" => TenderRecordAuthorityKind::CalculationRun,
                    _ => return None,
                };
                Some((input.reference.clone(), kind, input.version))
            })
            .collect::<HashSet<_>>();
        if allowed_evidence.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut stable_keys = HashSet::new();
        for record in &candidate.records {
            if !valid_record_key(&record.stable_key)
                || !stable_keys.insert(record.stable_key.as_str())
                || record.title.trim().is_empty()
                || record.title.len() > 500
                || record.fields.is_empty()
                || record.fields.len() > MAX_RECORD_FIELDS
                || record.contradictions.len() > MAX_RECORD_CONTRADICTIONS
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            if let Some(instruction) = &record.generation_instruction {
                let unique_instruction_evidence =
                    instruction.evidence.iter().collect::<HashSet<_>>();
                if instruction.section_key.trim().is_empty()
                    || instruction.section_key.len() > 200
                    || instruction.package_path.trim().is_empty()
                    || instruction.package_path.len() > 1_000
                    || instruction.envelope_key.trim().is_empty()
                    || instruction.envelope_key.len() > 200
                    || instruction.language.trim().is_empty()
                    || instruction.language.len() > 100
                    || match instruction.authoring_mode {
                        GenerationAuthoringMode::Unsupported => instruction
                            .requested_authoring_format
                            .as_deref()
                            .is_none_or(|format| format.trim().is_empty() || format.len() > 200),
                        _ => instruction.requested_authoring_format.is_some(),
                    }
                    || instruction.evidence.is_empty()
                    || instruction.evidence.len() > 32
                    || unique_instruction_evidence.len() != instruction.evidence.len()
                    || instruction
                        .evidence
                        .iter()
                        .any(|reference| !allowed_evidence.contains(reference))
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            let mut field_names = HashSet::new();
            for field in &record.fields {
                let unique_field_evidence = field.evidence.iter().collect::<HashSet<_>>();
                if !valid_record_key(&field.name)
                    || !field_names.insert(field.name.as_str())
                    || field
                        .value
                        .as_ref()
                        .is_some_and(|value| value.len() > 4_000)
                    || field
                        .basis_description
                        .as_ref()
                        .is_some_and(|value| value.len() > 2_000)
                    || field
                        .original_expression
                        .as_ref()
                        .is_some_and(|value| value.len() > 2_000)
                    || field
                        .normalized_value
                        .as_ref()
                        .is_some_and(|value| value.len() > 2_000)
                    || field
                        .timezone
                        .as_ref()
                        .is_some_and(|value| value.len() > 100)
                    || field
                        .uncertainty
                        .as_ref()
                        .is_some_and(|value| value.len() > 2_000)
                    || field.evidence.len() > 32
                    || unique_field_evidence.len() != field.evidence.len()
                    || field
                        .evidence
                        .iter()
                        .any(|reference| !allowed_evidence.contains(reference))
                    || (record.generation_instruction.is_some()
                        && field.basis_kind == TenderRecordBasisKind::Evidence
                        && field.value.is_none()
                        && field.normalized_value.is_none())
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
                match field.basis_kind {
                    TenderRecordBasisKind::Evidence
                        if !field.evidence.is_empty()
                            && field.basis_reference.is_none()
                            && field.basis_description.is_none() => {}
                    TenderRecordBasisKind::Assumption
                        if record.kind == TenderRecordKind::Assumption
                            && field.evidence.is_empty()
                            && field.basis_reference.as_deref()
                                == Some(record.stable_key.as_str())
                            && field
                                .basis_description
                                .as_deref()
                                .is_some_and(|value| !value.trim().is_empty()) => {}
                    TenderRecordBasisKind::TenderQuery
                        if record.kind == TenderRecordKind::TenderQuery
                            && field.basis_reference.as_deref()
                                == Some(record.stable_key.as_str())
                            && field
                                .basis_description
                                .as_deref()
                                .is_some_and(|value| !value.trim().is_empty()) => {}
                    TenderRecordBasisKind::EngineerEntry
                        if record_authority_matches_field(
                            &self.connection,
                            field,
                            TenderRecordAuthorityKind::EngineerEntry,
                            &allowed_authorities,
                        )? => {}
                    TenderRecordBasisKind::CalculationRun
                        if record_authority_matches_field(
                            &self.connection,
                            field,
                            TenderRecordAuthorityKind::CalculationRun,
                            &allowed_authorities,
                        )? => {}
                    _ => {
                        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                    }
                }
                if record.kind == TenderRecordKind::Deadline
                    && (field.original_expression.is_none()
                        || field.timezone.is_none()
                        || (field.normalized_value.is_none() && field.uncertainty.is_none())
                        || field
                            .normalized_value
                            .as_deref()
                            .is_some_and(|value| value.parse::<Timestamp>().is_err()))
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            for contradiction in &record.contradictions {
                let unique_contradiction_evidence =
                    contradiction.evidence.iter().collect::<HashSet<_>>();
                if !field_names.contains(contradiction.field_name.as_str())
                    || contradiction.summary.trim().is_empty()
                    || contradiction.summary.len() > 2_000
                    || contradiction.evidence.len() < 2
                    || contradiction.evidence.len() > 32
                    || unique_contradiction_evidence.len() != contradiction.evidence.len()
                    || contradiction
                        .evidence
                        .iter()
                        .any(|reference| !allowed_evidence.contains(reference))
                {
                    return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
                }
            }
            if expanded_record_candidate_bytes(&self.connection, record)?
                > MAX_EXPANDED_RECORD_BYTES
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        Ok(candidate)
    }

    pub(crate) fn validate_tender_record_review_candidate(
        &self,
        task: &TenderTaskView,
        reviewer_profile_id: &str,
        payload_json: &str,
    ) -> Result<TenderRecordReviewCandidate, TenderCommandError> {
        let candidate: TenderRecordReviewCandidate = serde_json::from_str(payload_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if canonical_json(&candidate)? != payload_json
            || candidate.rationale.trim().is_empty()
            || candidate.rationale.len() > 4_000
            || candidate.outcome == TenderRecordReviewOutcome::ApprovedAssumption
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let input = task
            .exact_inputs
            .iter()
            .find(|input| input.kind == "tender_record_version")
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let (author_profile_id, fields_json): (String, String) = self
            .connection
            .query_row(
                "SELECT agent_runs.profile_id, tender_record_versions.fields_json
                 FROM tender_record_versions
                 JOIN agent_runs ON agent_runs.run_id = tender_record_versions.author_run_id
                 WHERE tender_record_versions.record_id = ?1
                   AND tender_record_versions.version = ?2",
                params![input.reference, input.version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if author_profile_id == reviewer_profile_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let fields: Vec<TenderRecordFieldCandidate> = parse_canonical_json(&fields_json)?;
        if candidate.outcome == TenderRecordReviewOutcome::Verified
            && !record_fields_are_verifiable(&self.connection, &fields)?
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        Ok(candidate)
    }

    pub(crate) fn decide_tender_record(
        &mut self,
        tender_id: &TenderId,
        command: &DecideTenderRecordCommand,
    ) -> Result<TenderRecordDecisionResult, TenderCommandError> {
        if !self.active_change_allows_record_governance(&command.record_id)? {
            self.require_change_intake_writable()?;
        }
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
        let (kind, fields_json, current_version, engineer_decision_count): (
            String,
            String,
            u32,
            i64,
        ) = transaction
            .query_row(
                "SELECT tender_record_versions.kind, tender_record_versions.fields_json,
                        tender_record_heads.current_version,
                        (SELECT COUNT(*) FROM tender_record_reviews
                         WHERE tender_record_reviews.record_id = tender_record_versions.record_id
                           AND tender_record_reviews.record_version = tender_record_versions.version
                           AND reviewer_kind = 'engineer_user')
                 FROM tender_record_versions
                 JOIN tender_record_heads USING (record_id)
                 WHERE tender_record_versions.record_id = ?1
                   AND tender_record_versions.version = ?2",
                params![command.record_id, command.version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))?;
        if current_version != command.version || engineer_decision_count != 0 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let kind = TenderRecordKind::parse(&kind)?;
        let fields: Vec<TenderRecordFieldCandidate> = parse_canonical_json(&fields_json)?;
        let outcome = match command.decision {
            TenderRecordEngineerDecisionKind::Verify
                if record_fields_are_verifiable(&transaction, &fields)? =>
            {
                TenderRecordReviewOutcome::Verified
            }
            TenderRecordEngineerDecisionKind::Reject => TenderRecordReviewOutcome::Rejected,
            TenderRecordEngineerDecisionKind::ApproveAssumption
                if kind == TenderRecordKind::Assumption
                    && fields.iter().all(|field| {
                        field.basis_kind == TenderRecordBasisKind::Assumption
                            && field.evidence.is_empty()
                            && field
                                .basis_description
                                .as_deref()
                                .is_some_and(|value| !value.trim().is_empty())
                    }) =>
            {
                TenderRecordReviewOutcome::ApprovedAssumption
            }
            _ => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        };
        let created_at = sqlite_timestamp(&transaction)?;
        let review = insert_record_review(
            &transaction,
            RecordReviewInsert {
                record_id: &command.record_id,
                version: command.version,
                reviewer_kind: "engineer_user",
                reviewer_run_id: None,
                outcome,
                rationale: &command.rationale,
                decided_by: "engineer_user",
                created_at: &created_at,
            },
        )?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "tender_record_engineer_decided",
            tender_revision,
            json!({
                "outcome": outcome.as_str(),
                "record_id": command.record_id,
                "record_version": command.version.to_string(),
                "review_id": review.review_id,
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(TenderRecordDecisionResult {
            record: self.inspect_tender_record_version(&command.record_id, command.version)?,
            review,
        })
    }

    pub(crate) fn create_tender_engineer_entry(
        &mut self,
        tender_id: &TenderId,
        command: &CreateTenderEngineerEntryCommand,
    ) -> Result<TenderRecordAuthority, TenderCommandError> {
        self.require_change_intake_writable()?;
        if command.value.trim().is_empty() || command.description.trim().is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
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
        let created_at = sqlite_timestamp(&transaction)?;
        let authority = insert_engineer_entry(
            &transaction,
            tender_revision,
            &command.value,
            &command.description,
            &created_at,
        )?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "tender_record_engineer_entry_created",
            tender_revision,
            json!({
                "authority_id": authority.authority_id,
                "created_by": authority.created_by,
            }),
            &authority.created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(authority)
    }

    pub(crate) fn inspect_tender_record_authorities(
        &self,
    ) -> Result<Vec<TenderRecordAuthority>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT authority_id, kind, value, description, manifest_sha256,
                        tender_revision, created_by, created_at
                 FROM tender_record_authorities ORDER BY rowid",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        if rows.len() > MAX_RECORD_AUTHORITIES {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        rows.into_iter()
            .map(
                |(
                    authority_id,
                    kind,
                    value,
                    description,
                    manifest_sha256,
                    tender_revision,
                    created_by,
                    created_at,
                )| {
                    validate_record_authority(TenderRecordAuthority {
                        authority_id,
                        kind: TenderRecordAuthorityKind::parse(&kind)?,
                        value,
                        description,
                        manifest_sha256,
                        tender_revision,
                        created_by,
                        created_at,
                    })
                },
            )
            .collect()
    }

    pub(crate) fn tender_record_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        if self.inspect_tender_record_authorities().is_err() {
            return Ok(false);
        }
        let (record_rows, head_rows): (i64, i64) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM tender_records),
                        (SELECT COUNT(*) FROM tender_record_heads)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if record_rows != head_rows
            || usize::try_from(record_rows)
                .ok()
                .is_none_or(|count| count > MAX_DECISION_RECORD_INVENTORY)
        {
            return Ok(false);
        }
        let mut record_statement = self
            .connection
            .prepare(
                "SELECT tender_records.record_id, tender_records.stable_key,
                        tender_record_heads.current_version
                 FROM tender_records
                 JOIN tender_record_heads USING (record_id)
                 ORDER BY tender_records.rowid",
            )
            .map_err(sql_error)?;
        let mut record_rows = record_statement.query([]).map_err(sql_error)?;
        let mut record_count = 0_usize;
        while let Some(record_row) = record_rows.next().map_err(sql_error)? {
            check()?;
            record_count = record_count
                .checked_add(1)
                .filter(|count| *count <= MAX_DECISION_RECORD_INVENTORY)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let record_id = record_row.get::<_, String>(0).map_err(sql_error)?;
            let stable_key = record_row.get::<_, String>(1).map_err(sql_error)?;
            let current_version = record_row.get::<_, u32>(2).map_err(sql_error)?;
            if !valid_record_key(&stable_key) {
                return Ok(false);
            }
            let mut version_statement = self
                .connection
                .prepare(
                    "SELECT version, kind, title, generation_instruction_json,
                            fields_json, contradictions_json,
                            agent_runs.task_id
                     FROM tender_record_versions
                     JOIN agent_runs ON agent_runs.run_id = tender_record_versions.author_run_id
                     WHERE tender_record_versions.record_id = ?1
                     ORDER BY version",
                )
                .map_err(sql_error)?;
            let mut version_rows = version_statement.query([&record_id]).map_err(sql_error)?;
            let mut expected_version = 1_u32;
            while let Some(version_row) = version_rows.next().map_err(sql_error)? {
                check()?;
                if usize::try_from(expected_version)
                    .ok()
                    .is_none_or(|version| version > MAX_RECORD_VERSIONS)
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let version = version_row.get::<_, u32>(0).map_err(sql_error)?;
                let kind =
                    TenderRecordKind::parse(&version_row.get::<_, String>(1).map_err(sql_error)?)?;
                let title = version_row.get::<_, String>(2).map_err(sql_error)?;
                let generation_instruction_json =
                    version_row.get::<_, Option<String>>(3).map_err(sql_error)?;
                let fields_json = version_row.get::<_, String>(4).map_err(sql_error)?;
                let contradictions_json = version_row.get::<_, String>(5).map_err(sql_error)?;
                let task_id = version_row.get::<_, String>(6).map_err(sql_error)?;
                if version != expected_version {
                    return Ok(false);
                }
                let fields: Vec<TenderRecordFieldCandidate> = parse_canonical_json(&fields_json)?;
                let contradictions: Vec<TenderRecordContradictionCandidate> =
                    parse_canonical_json(&contradictions_json)?;
                let candidate = TenderRecordCandidateBatch {
                    records: vec![TenderRecordCandidate {
                        stable_key: stable_key.clone(),
                        kind,
                        title,
                        generation_instruction: generation_instruction_json
                            .as_deref()
                            .map(parse_canonical_json)
                            .transpose()?,
                        fields: fields.clone(),
                        contradictions,
                    }],
                };
                let task = load_task(&self.connection, &task_id)?;
                let payload = canonical_json(&candidate)?;
                if self
                    .validate_tender_record_candidate(&task, &payload)
                    .is_err()
                {
                    return Ok(false);
                }
                let reviews = load_record_reviews(&self.connection, &record_id, version)?;
                for review in reviews {
                    check()?;
                    match review.reviewer_kind.as_str() {
                        "independent_reviewer" => {
                            let Some(run_id) = review.reviewer_run_id else {
                                return Ok(false);
                            };
                            let (status, profile_id, profile_version, review_task_id): (
                                String,
                                String,
                                u32,
                                String,
                            ) = self
                                .connection
                                .query_row(
                                    "SELECT status, profile_id, profile_version, task_id
                                     FROM agent_runs WHERE run_id = ?1",
                                    [&run_id],
                                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                                )
                                .map_err(sql_error)?;
                            let profile =
                                load_profile(&self.connection, (profile_id, profile_version))?;
                            let review_task = load_task(&self.connection, &review_task_id)?;
                            if status != "completed"
                                || !profile
                                    .capabilities
                                    .iter()
                                    .any(|capability| capability == RECORD_REVIEW_CAPABILITY)
                                || review_task.exact_inputs
                                    != vec![AgentTaskInputReference {
                                        kind: "tender_record_version".into(),
                                        reference: record_id.clone(),
                                        version,
                                    }]
                                || review.outcome == TenderRecordReviewOutcome::ApprovedAssumption
                                || (review.outcome == TenderRecordReviewOutcome::Verified
                                    && !record_fields_are_verifiable(&self.connection, &fields)?)
                            {
                                return Ok(false);
                            }
                        }
                        "engineer_user" => {
                            if review.reviewer_run_id.is_some()
                                || (review.outcome == TenderRecordReviewOutcome::Verified
                                    && !record_fields_are_verifiable(&self.connection, &fields)?)
                                || (review.outcome == TenderRecordReviewOutcome::ApprovedAssumption
                                    && (kind != TenderRecordKind::Assumption
                                        || fields.iter().any(|field| {
                                            field.basis_kind != TenderRecordBasisKind::Assumption
                                                || !field.evidence.is_empty()
                                        })))
                            {
                                return Ok(false);
                            }
                        }
                        _ => return Ok(false),
                    }
                }
                expected_version = expected_version
                    .checked_add(1)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            }
            if current_version != expected_version.saturating_sub(1) {
                return Ok(false);
            }
        }
        check()?;
        Ok(true)
    }

    pub(crate) fn count_tender_records_by_run(
        &self,
        run_id: &str,
    ) -> Result<u32, TenderCommandError> {
        self.connection
            .query_row(
                "SELECT COUNT(*) FROM tender_record_versions WHERE author_run_id = ?1",
                [run_id],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn inspect_tender_record_page(
        &self,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<TenderRecordPage, TenderCommandError> {
        if limit == 0 || limit > MAX_RECORD_PAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (after_key, after_version) = cursor
            .map(parse_record_cursor)
            .transpose()?
            .unwrap_or_else(|| (String::new(), 0));
        let sql_limit = limit
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT tender_record_versions.record_id, tender_records.stable_key,
                        tender_record_versions.version
                 FROM tender_record_versions
                 JOIN tender_records USING (record_id)
                 WHERE tender_records.stable_key > ?1
                    OR (tender_records.stable_key = ?1
                        AND tender_record_versions.version > ?2)
                 ORDER BY tender_records.stable_key, tender_record_versions.version
                 LIMIT ?3",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![after_key, after_version, sql_limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                ))
            })
            .map_err(sql_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(sql_error)?;
        let mut records = Vec::new();
        let mut last_cursor = None;
        for (record_id, stable_key, version) in rows.iter().take(limit as usize) {
            records.push(self.inspect_tender_record_version(record_id, *version)?);
            if canonical_json(&records)?.len() > MAX_RECORD_PAGE_BYTES {
                records.pop();
                break;
            }
            last_cursor = Some(format_record_cursor(stable_key, *version));
        }
        if records.is_empty() && !rows.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let has_more = rows.len() > records.len();
        Ok(TenderRecordPage {
            records,
            next_cursor: has_more.then_some(last_cursor).flatten(),
        })
    }

    pub(crate) fn inspect_tender_record_version(
        &self,
        record_id: &str,
        version: u32,
    ) -> Result<TenderRecordInspection, TenderCommandError> {
        inspect_tender_record_version_in_connection(&self.connection, record_id, version)
    }
}

pub(crate) fn inspect_tender_record_version_in_connection(
    connection: &rusqlite::Connection,
    record_id: &str,
    version: u32,
) -> Result<TenderRecordInspection, TenderCommandError> {
    inspect_tender_records_where_in_connection(
        connection,
        "tender_record_versions.record_id = ?1 AND tender_record_versions.version = ?2",
        params![record_id, version],
    )?
    .into_iter()
    .next()
    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::NotFound))
}

fn inspect_tender_records_where_in_connection<P: rusqlite::Params>(
    connection: &rusqlite::Connection,
    predicate: &str,
    params: P,
) -> Result<Vec<TenderRecordInspection>, TenderCommandError> {
    let sql = format!(
        "SELECT tender_record_versions.record_id, tender_records.stable_key,
                    tender_record_versions.version, tender_record_versions.kind,
                    tender_record_versions.title,
                    tender_record_versions.generation_instruction_json,
                    tender_record_versions.fields_json, tender_record_versions.contradictions_json,
                    tender_record_versions.author_run_id, agent_runs.profile_id,
                    tender_record_versions.created_at
             FROM tender_record_versions
             JOIN tender_records USING (record_id)
             JOIN tender_record_heads USING (record_id)
             JOIN agent_runs ON agent_runs.run_id = tender_record_versions.author_run_id
             WHERE {predicate}
             ORDER BY tender_records.stable_key, tender_record_versions.version"
    );
    let mut statement = connection.prepare(&sql).map_err(sql_error)?;
    let rows = statement
        .query_map(params, |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(sql_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(sql_error)?;
    rows.into_iter()
        .map(
            |(
                record_id,
                stable_key,
                version,
                kind,
                title,
                generation_instruction_json,
                fields_json,
                contradictions_json,
                author_run_id,
                author_profile_id,
                created_at,
            )| {
                let kind = TenderRecordKind::parse(&kind)?;
                let generation_instruction_candidate = generation_instruction_json
                    .as_deref()
                    .map(parse_canonical_json::<TenderRecordGenerationInstructionCandidate>)
                    .transpose()?;
                let field_candidates: Vec<TenderRecordFieldCandidate> =
                    parse_canonical_json(&fields_json)?;
                let contradiction_candidates: Vec<TenderRecordContradictionCandidate> =
                    parse_canonical_json(&contradictions_json)?;
                let references = field_candidates
                    .iter()
                    .flat_map(|field| field.evidence.iter().cloned())
                    .chain(
                        contradiction_candidates
                            .iter()
                            .flat_map(|contradiction| contradiction.evidence.iter().cloned()),
                    )
                    .chain(
                        generation_instruction_candidate
                            .iter()
                            .flat_map(|instruction| instruction.evidence.iter().cloned()),
                    )
                    .collect::<HashSet<_>>();
                let generation_instruction = generation_instruction_candidate
                    .map(|instruction| {
                        resolve_generation_instruction_in_connection(connection, instruction)
                    })
                    .transpose()?;
                let fields = field_candidates
                    .into_iter()
                    .map(|field| resolve_record_field_in_connection(connection, field))
                    .collect::<Result<Vec<_>, _>>()?;
                let contradictions = contradiction_candidates
                    .into_iter()
                    .map(|contradiction| {
                        resolve_contradiction_in_connection(connection, contradiction)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let reviews = load_record_reviews(connection, &record_id, version)?;
                let effective_review = reviews
                    .iter()
                    .rev()
                    .find(|review| review.reviewer_kind == "engineer_user")
                    .or_else(|| reviews.last());
                let current_version: u32 = connection
                    .query_row(
                        "SELECT current_version FROM tender_record_heads WHERE record_id = ?1",
                        [&record_id],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                let source_relationships = load_source_relationships(connection, &references)?;
                let base_trust_class = if matches!(
                    kind,
                    TenderRecordKind::Assumption | TenderRecordKind::TenderQuery
                ) {
                    TenderRecordTrustClass::UnresolvedGap
                } else if record_is_deterministic_fact(&fields, &contradictions) {
                    TenderRecordTrustClass::DeterministicFact
                } else {
                    TenderRecordTrustClass::AiProposal
                };
                let source_invalidated = source_relationships.iter().any(|relationship| {
                    references.iter().any(|reference| {
                        reference.artifact_id == relationship.prior_artifact_id
                            && reference.version == relationship.prior_version
                    }) && !references.iter().any(|reference| {
                        reference.artifact_id == relationship.replacement_artifact_id
                            && reference.version == relationship.replacement_version
                    })
                });
                let (verification_status, trust_class) = if version < current_version {
                    (
                        VerificationStatus::Superseded,
                        if reviews.is_empty() {
                            base_trust_class
                        } else {
                            TenderRecordTrustClass::PriorDecision
                        },
                    )
                } else if source_invalidated {
                    (
                        VerificationStatus::Stale,
                        if reviews.is_empty() {
                            base_trust_class
                        } else {
                            TenderRecordTrustClass::PriorDecision
                        },
                    )
                } else {
                    match effective_review {
                        Some(review) if review.outcome == TenderRecordReviewOutcome::Verified => (
                            VerificationStatus::Verified,
                            if review.reviewer_kind == "engineer_user" {
                                TenderRecordTrustClass::EngineerVerified
                            } else {
                                TenderRecordTrustClass::Verified
                            },
                        ),
                        Some(review) if review.outcome == TenderRecordReviewOutcome::Rejected => {
                            (VerificationStatus::Rejected, base_trust_class)
                        }
                        Some(review)
                            if review.outcome == TenderRecordReviewOutcome::ApprovedAssumption =>
                        {
                            (
                                VerificationStatus::Verified,
                                TenderRecordTrustClass::ApprovedAssumption,
                            )
                        }
                        Some(_) => {
                            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                        }
                        None => (VerificationStatus::Proposed, base_trust_class),
                    }
                };
                Ok(TenderRecordInspection {
                    record_id,
                    stable_key,
                    version,
                    kind,
                    title,
                    verification_status,
                    trust_class,
                    fields,
                    generation_instruction,
                    contradictions,
                    source_relationships,
                    reviews,
                    author_run_id,
                    author_profile_id,
                    created_at,
                })
            },
        )
        .collect()
}

fn resolve_generation_instruction_in_connection(
    connection: &rusqlite::Connection,
    instruction: TenderRecordGenerationInstructionCandidate,
) -> Result<TenderRecordGenerationInstruction, TenderCommandError> {
    Ok(TenderRecordGenerationInstruction {
        kind: instruction.kind,
        mandatory: instruction.mandatory,
        section_key: instruction.section_key,
        package_path: instruction.package_path,
        envelope_key: instruction.envelope_key,
        language: instruction.language,
        authoring_mode: instruction.authoring_mode,
        requested_authoring_format: instruction.requested_authoring_format,
        evidence: instruction
            .evidence
            .into_iter()
            .map(|reference| resolve_record_evidence_in_connection(connection, reference))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn resolve_record_field_in_connection(
    connection: &rusqlite::Connection,
    field: TenderRecordFieldCandidate,
) -> Result<TenderRecordField, TenderCommandError> {
    let basis_authority = match field.basis_kind {
        TenderRecordBasisKind::EngineerEntry | TenderRecordBasisKind::CalculationRun => {
            Some(load_record_authority(
                connection,
                field
                    .basis_reference
                    .as_deref()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            )?)
        }
        _ => None,
    };
    Ok(TenderRecordField {
        name: field.name,
        value: field.value,
        basis_kind: field.basis_kind,
        basis_reference: field.basis_reference,
        basis_description: field.basis_description,
        basis_authority,
        original_expression: field.original_expression,
        normalized_value: field.normalized_value,
        timezone: field.timezone,
        uncertainty: field.uncertainty,
        evidence: field
            .evidence
            .into_iter()
            .map(|reference| resolve_record_evidence_in_connection(connection, reference))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn resolve_contradiction_in_connection(
    connection: &rusqlite::Connection,
    contradiction: TenderRecordContradictionCandidate,
) -> Result<TenderRecordContradiction, TenderCommandError> {
    Ok(TenderRecordContradiction {
        field_name: contradiction.field_name,
        summary: contradiction.summary,
        evidence: contradiction
            .evidence
            .into_iter()
            .map(|reference| resolve_record_evidence_in_connection(connection, reference))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn resolve_record_evidence_in_connection(
    connection: &rusqlite::Connection,
    reference: TenderEvidenceReference,
) -> Result<TenderRecordEvidence, TenderCommandError> {
    let (package_path, location): (String, RawEvidenceLocation) = connection
        .query_row(
            "SELECT source_artifacts.package_path,
                        evidence_locations.ordinal, evidence_locations.kind,
                        evidence_locations.structural_path, evidence_locations.provenance_json,
                        evidence_locations.section, evidence_locations.paragraph_number,
                        evidence_locations.table_number, evidence_locations.sheet_name,
                        evidence_locations.cell_range, evidence_locations.original_text,
                        evidence_locations.translated_text, evidence_locations.language,
                        evidence_locations.direction
                 FROM evidence_locations
                 JOIN source_artifacts USING (artifact_id)
                 WHERE evidence_locations.artifact_id = ?1
                   AND evidence_locations.version = ?2
                   AND evidence_locations.ordinal = ?3",
            params![reference.artifact_id, reference.version, reference.ordinal],
            |row| Ok((row.get(0)?, RawEvidenceLocation::read(row, 1)?)),
        )
        .optional()
        .map_err(sql_error)?
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    Ok(TenderRecordEvidence {
        reference,
        package_path,
        location: location.into_domain()?,
    })
}

fn expanded_record_candidate_bytes(
    connection: &rusqlite::Connection,
    record: &TenderRecordCandidate,
) -> Result<u64, TenderCommandError> {
    let mut total = u64::try_from(canonical_json(record)?.len())
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    for reference in record
        .fields
        .iter()
        .flat_map(|field| field.evidence.iter())
        .chain(
            record
                .contradictions
                .iter()
                .flat_map(|contradiction| contradiction.evidence.iter()),
        )
        .chain(
            record
                .generation_instruction
                .iter()
                .flat_map(|instruction| instruction.evidence.iter()),
        )
    {
        let expanded_bytes: i64 = connection
            .query_row(
                "SELECT length(CAST(source_artifacts.package_path AS BLOB))
                        + length(CAST(evidence_locations.kind AS BLOB))
                        + length(CAST(evidence_locations.structural_path AS BLOB))
                        + length(CAST(evidence_locations.provenance_json AS BLOB))
                        + COALESCE(length(CAST(evidence_locations.section AS BLOB)), 0)
                        + COALESCE(length(CAST(evidence_locations.sheet_name AS BLOB)), 0)
                        + COALESCE(length(CAST(evidence_locations.cell_range AS BLOB)), 0)
                        + length(CAST(evidence_locations.original_text AS BLOB))
                        + COALESCE(length(CAST(evidence_locations.translated_text AS BLOB)), 0)
                        + length(CAST(evidence_locations.language AS BLOB))
                        + length(CAST(evidence_locations.direction AS BLOB))
                 FROM evidence_locations
                 JOIN source_artifacts USING (artifact_id)
                 WHERE evidence_locations.artifact_id = ?1
                   AND evidence_locations.version = ?2
                   AND evidence_locations.ordinal = ?3",
                params![reference.artifact_id, reference.version, reference.ordinal],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        total = total
            .checked_add(
                u64::try_from(expanded_bytes)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            )
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if total > MAX_EXPANDED_RECORD_BYTES {
            break;
        }
    }
    Ok(total)
}

fn validate_record_authority(
    authority: TenderRecordAuthority,
) -> Result<TenderRecordAuthority, TenderCommandError> {
    let manifest_valid = match authority.kind {
        TenderRecordAuthorityKind::EngineerEntry => authority.manifest_sha256.is_none(),
        TenderRecordAuthorityKind::CalculationRun => {
            authority.manifest_sha256.as_deref().is_some_and(|hash| {
                hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        }
    };
    if !valid_identifier(&authority.authority_id)
        || authority.value.trim().is_empty()
        || authority.value.len() > 4_000
        || authority.description.trim().is_empty()
        || authority.description.len() > 2_000
        || authority.tender_revision == 0
        || authority.created_by.trim().is_empty()
        || authority.created_by.len() > 200
        || !manifest_valid
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(authority)
}

fn load_record_authority(
    connection: &rusqlite::Connection,
    authority_id: &str,
) -> Result<TenderRecordAuthority, TenderCommandError> {
    let raw: (
        String,
        String,
        String,
        String,
        Option<String>,
        u32,
        String,
        String,
    ) = connection
        .query_row(
            "SELECT authority_id, kind, value, description, manifest_sha256,
                    tender_revision, created_by, created_at
             FROM tender_record_authorities WHERE authority_id = ?1",
            [authority_id],
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
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    validate_record_authority(TenderRecordAuthority {
        authority_id: raw.0,
        kind: TenderRecordAuthorityKind::parse(&raw.1)?,
        value: raw.2,
        description: raw.3,
        manifest_sha256: raw.4,
        tender_revision: raw.5,
        created_by: raw.6,
        created_at: raw.7,
    })
}

fn load_record_authorities_by_references(
    connection: &rusqlite::Connection,
    references: &[TenderRecordAuthorityReference],
) -> Result<Vec<TenderRecordAuthority>, TenderCommandError> {
    references
        .iter()
        .map(|reference| load_record_authority(connection, &reference.authority_id))
        .collect()
}

fn record_authority_matches_field(
    connection: &rusqlite::Connection,
    field: &TenderRecordFieldCandidate,
    expected_kind: TenderRecordAuthorityKind,
    allowed_authorities: &HashSet<(String, TenderRecordAuthorityKind, u32)>,
) -> Result<bool, TenderCommandError> {
    let Some(authority_id) = field.basis_reference.as_deref() else {
        return Ok(false);
    };
    let authority = load_record_authority(connection, authority_id)?;
    Ok(authority.kind == expected_kind
        && allowed_authorities.contains(&(
            authority.authority_id.clone(),
            authority.kind,
            authority.tender_revision,
        ))
        && field.evidence.is_empty()
        && field.value.as_deref() == Some(authority.value.as_str())
        && field.basis_description.as_deref() == Some(authority.description.as_str()))
}

fn record_fields_are_verifiable(
    connection: &rusqlite::Connection,
    fields: &[TenderRecordFieldCandidate],
) -> Result<bool, TenderCommandError> {
    if fields.is_empty() {
        return Ok(false);
    }
    for field in fields {
        match field.basis_kind {
            TenderRecordBasisKind::Evidence if !field.evidence.is_empty() => {
                for reference in &field.evidence {
                    let exists: bool = connection
                        .query_row(
                            "SELECT EXISTS(
                               SELECT 1 FROM evidence_locations
                               WHERE artifact_id = ?1 AND version = ?2 AND ordinal = ?3
                             )",
                            params![reference.artifact_id, reference.version, reference.ordinal],
                            |row| row.get(0),
                        )
                        .map_err(sql_error)?;
                    if !exists {
                        return Ok(false);
                    }
                }
            }
            TenderRecordBasisKind::EngineerEntry | TenderRecordBasisKind::CalculationRun => {
                let expected_kind = if field.basis_kind == TenderRecordBasisKind::EngineerEntry {
                    TenderRecordAuthorityKind::EngineerEntry
                } else {
                    TenderRecordAuthorityKind::CalculationRun
                };
                let Some(authority_id) = field.basis_reference.as_deref() else {
                    return Ok(false);
                };
                let authority = match load_record_authority(connection, authority_id) {
                    Ok(authority) => authority,
                    Err(error) if error.code == TenderErrorCode::InvalidCommand => {
                        return Ok(false)
                    }
                    Err(error) => return Err(error),
                };
                if authority.kind != expected_kind
                    || !field.evidence.is_empty()
                    || field.value.as_deref() != Some(authority.value.as_str())
                    || field.basis_description.as_deref() != Some(authority.description.as_str())
                {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn record_is_deterministic_fact(
    fields: &[TenderRecordField],
    contradictions: &[TenderRecordContradiction],
) -> bool {
    contradictions.is_empty()
        && !fields.is_empty()
        && fields.iter().all(|field| {
            field.basis_kind == TenderRecordBasisKind::CalculationRun
                && field
                    .basis_authority
                    .as_ref()
                    .is_some_and(|authority| authority.manifest_sha256.is_some())
        })
}

struct RecordReviewInsert<'a> {
    record_id: &'a str,
    version: u32,
    reviewer_kind: &'a str,
    reviewer_run_id: Option<&'a str>,
    outcome: TenderRecordReviewOutcome,
    rationale: &'a str,
    decided_by: &'a str,
    created_at: &'a str,
}

fn insert_record_review(
    transaction: &rusqlite::Transaction<'_>,
    input: RecordReviewInsert<'_>,
) -> Result<TenderRecordReview, TenderCommandError> {
    let review_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM tender_record_reviews
             WHERE record_id = ?1 AND record_version = ?2",
            params![input.record_id, input.version],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    if usize::try_from(review_count)
        .ok()
        .is_none_or(|count| count >= MAX_RECORD_REVIEWS)
    {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let review = TenderRecordReview {
        review_id: random_identifier(transaction)?,
        outcome: input.outcome,
        rationale: input.rationale.into(),
        reviewer_kind: input.reviewer_kind.into(),
        reviewer_run_id: input.reviewer_run_id.map(str::to_owned),
        decided_by: input.decided_by.into(),
        created_at: input.created_at.into(),
    };
    transaction
        .execute(
            "INSERT INTO tender_record_reviews (
               review_id, record_id, record_version, reviewer_kind, reviewer_run_id,
               outcome, rationale, decided_by, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                review.review_id,
                input.record_id,
                input.version,
                review.reviewer_kind,
                review.reviewer_run_id,
                review.outcome.as_str(),
                review.rationale,
                review.decided_by,
                review.created_at,
            ],
        )
        .map_err(sql_error)?;
    Ok(review)
}

fn load_record_reviews(
    connection: &rusqlite::Connection,
    record_id: &str,
    version: u32,
) -> Result<Vec<TenderRecordReview>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT review_id, outcome, rationale, reviewer_kind, reviewer_run_id,
                    decided_by, created_at
             FROM tender_record_reviews
             WHERE record_id = ?1 AND record_version = ?2
             ORDER BY rowid",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![record_id, version], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(sql_error)?;
    let mut reviews = Vec::new();
    for row in rows {
        if reviews.len() == MAX_RECORD_REVIEWS {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let (review_id, outcome, rationale, reviewer_kind, reviewer_run_id, decided_by, created_at) =
            row.map_err(sql_error)?;
        reviews.push(TenderRecordReview {
            review_id,
            outcome: TenderRecordReviewOutcome::parse(&outcome)?,
            rationale,
            reviewer_kind,
            reviewer_run_id,
            decided_by,
            created_at,
        });
    }
    Ok(reviews)
}

fn load_source_relationships(
    connection: &rusqlite::Connection,
    references: &HashSet<TenderEvidenceReference>,
) -> Result<Vec<TenderRecordSourceRelationship>, TenderCommandError> {
    let artifact_versions = references
        .iter()
        .map(|reference| (reference.artifact_id.as_str(), reference.version))
        .collect::<HashSet<_>>();
    let mut relationships = Vec::new();
    let mut seen = HashSet::new();
    for (artifact_id, version) in artifact_versions {
        let mut statement = connection
            .prepare(
                "SELECT relationship_id, prior_artifact_id, prior_version,
                        replacement_artifact_id, replacement_version, relationship_kind
                 FROM source_relationships
                 WHERE ((prior_artifact_id = ?1 AND prior_version = ?2)
                    OR (replacement_artifact_id = ?1 AND replacement_version = ?2))
                   AND EXISTS(
                     SELECT 1
                     FROM change_assessments AS assessments
                     JOIN change_assessment_decisions AS decisions USING (assessment_id)
                     WHERE assessments.relationship_id = source_relationships.relationship_id
                       AND decisions.classification = 'material'
                   )
                 ORDER BY rowid",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![artifact_id, version], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(sql_error)?;
        for row in rows {
            let (
                relationship_id,
                prior_artifact_id,
                prior_version,
                replacement_artifact_id,
                replacement_version,
                relationship_kind,
            ) = row.map_err(sql_error)?;
            if !seen.insert(relationship_id.clone()) {
                continue;
            }
            if relationships.len() == MAX_RECORD_SOURCE_RELATIONSHIPS {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            relationships.push(TenderRecordSourceRelationship {
                relationship_id,
                prior_artifact_id,
                prior_version,
                replacement_artifact_id,
                replacement_version,
                relationship_kind: SourceRelationshipKind::parse(&relationship_kind)?,
            });
        }
    }
    relationships.sort_by(|left, right| left.relationship_id.cmp(&right.relationship_id));
    Ok(relationships)
}

pub(super) fn publish_tender_record_review(
    transaction: &rusqlite::Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    reviewer_run_id: &str,
    task: &TenderTaskView,
    candidate: &TenderRecordReviewCandidate,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    if !tender_record_review_target_is_open(transaction, task)? {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let input = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "tender_record_version")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let review = insert_record_review(
        transaction,
        RecordReviewInsert {
            record_id: &input.reference,
            version: input.version,
            reviewer_kind: "independent_reviewer",
            reviewer_run_id: Some(reviewer_run_id),
            outcome: candidate.outcome,
            rationale: &candidate.rationale,
            decided_by: BootstrapRole::IndependentReviewer.stable_identity(),
            created_at,
        },
    )?;
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "tender_record_independently_reviewed",
        tender_revision,
        json!({
            "outcome": candidate.outcome.as_str(),
            "record_id": input.reference,
            "record_version": input.version.to_string(),
            "review_id": review.review_id,
            "reviewer_run_id": reviewer_run_id,
        }),
        created_at,
    )
}

pub(super) fn tender_record_review_target_is_open(
    connection: &rusqlite::Connection,
    task: &TenderTaskView,
) -> Result<bool, TenderCommandError> {
    let input = task
        .exact_inputs
        .iter()
        .find(|input| input.kind == "tender_record_version")
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM tender_record_heads
               WHERE record_id = ?1 AND current_version = ?2
                 AND NOT EXISTS (
                   SELECT 1 FROM tender_record_reviews
                   WHERE record_id = ?1 AND record_version = ?2
                 )
             )",
            params![input.reference, input.version],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

pub(super) fn publish_tender_record_candidates(
    transaction: &rusqlite::Transaction<'_>,
    tender_id: &TenderId,
    tender_revision: u32,
    run_id: &str,
    candidate: &TenderRecordCandidateBatch,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    let stable_keys = candidate
        .records
        .iter()
        .map(|record| record.stable_key.clone())
        .collect::<Vec<_>>();
    if !TenderStore::active_change_record_candidate_keys_are_allowed(transaction, &stable_keys)? {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    if !tender_record_candidates_fit_decision_inventory(transaction, candidate)? {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let mut published = Vec::with_capacity(candidate.records.len());
    for record in &candidate.records {
        let existing: Option<(String, u32)> = transaction
            .query_row(
                "SELECT tender_records.record_id, tender_record_heads.current_version
                 FROM tender_records
                 JOIN tender_record_heads USING (record_id)
                 WHERE tender_records.stable_key = ?1",
                [&record.stable_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let (record_id, version) = if let Some((record_id, current_version)) = existing {
            if usize::try_from(current_version)
                .ok()
                .is_none_or(|version| version >= MAX_RECORD_VERSIONS)
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            (
                record_id,
                current_version
                    .checked_add(1)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            )
        } else {
            let record_count: i64 = transaction
                .query_row("SELECT COUNT(*) FROM tender_records", [], |row| row.get(0))
                .map_err(sql_error)?;
            if usize::try_from(record_count)
                .ok()
                .is_none_or(|count| count >= MAX_DECISION_RECORD_INVENTORY)
            {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            let record_id = random_identifier(transaction)?;
            transaction
                .execute(
                    "INSERT INTO tender_records (record_id, stable_key, created_at)
                     VALUES (?1, ?2, ?3)",
                    params![record_id, record.stable_key, created_at],
                )
                .map_err(sql_error)?;
            (record_id, 1)
        };
        transaction
            .execute(
                "INSERT INTO tender_record_versions (
                   record_id, version, kind, title, generation_instruction_json, fields_json,
                   contradictions_json, author_run_id, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    record_id,
                    version,
                    record.kind.as_str(),
                    record.title,
                    record
                        .generation_instruction
                        .as_ref()
                        .map(canonical_json)
                        .transpose()?,
                    canonical_json(&record.fields)?,
                    canonical_json(&record.contradictions)?,
                    run_id,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO tender_record_heads (record_id, current_version)
                 VALUES (?1, ?2)
                 ON CONFLICT(record_id) DO UPDATE SET current_version = excluded.current_version",
                params![record_id, version],
            )
            .map_err(sql_error)?;
        published.push(json!({
            "record_id": record_id,
            "stable_key": record.stable_key,
            "version": version.to_string(),
        }));
    }
    append_audit_event(
        transaction,
        tender_id.as_str(),
        "tender_records_proposed",
        tender_revision,
        json!({ "records": published, "run_id": run_id }),
        created_at,
    )
}

pub(super) fn tender_record_candidates_fit_decision_inventory(
    transaction: &rusqlite::Transaction<'_>,
    candidate: &TenderRecordCandidateBatch,
) -> Result<bool, TenderCommandError> {
    let current_count: i64 = transaction
        .query_row("SELECT COUNT(*) FROM tender_record_heads", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)?;
    let Some(remaining) = usize::try_from(current_count)
        .ok()
        .and_then(|count| MAX_DECISION_RECORD_INVENTORY.checked_sub(count))
    else {
        return Ok(false);
    };
    let mut new_records = 0_usize;
    for record in &candidate.records {
        let exists: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tender_records WHERE stable_key = ?1)",
                [&record.stable_key],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !exists {
            new_records = new_records
                .checked_add(1)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            if new_records > remaining {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn record_extraction_data_view(
    connection: &rusqlite::Connection,
    tender_id: &TenderId,
    tender_revision: u32,
    evidence: &[TenderEvidenceReference],
    authorities: &[TenderRecordAuthority],
) -> Result<Value, TenderCommandError> {
    let tender_name: String = connection
        .query_row(
            "SELECT name FROM tender_revisions WHERE tender_id = ?1 AND revision = ?2",
            params![tender_id.as_str(), tender_revision],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let mut resolved = Vec::with_capacity(evidence.len());
    for reference in evidence {
        let (package_path, location): (String, RawEvidenceLocation) = connection
            .query_row(
                "SELECT source_artifacts.package_path,
                        evidence_locations.ordinal, evidence_locations.kind,
                        evidence_locations.structural_path, evidence_locations.provenance_json,
                        evidence_locations.section, evidence_locations.paragraph_number,
                        evidence_locations.table_number, evidence_locations.sheet_name,
                        evidence_locations.cell_range, evidence_locations.original_text,
                        evidence_locations.translated_text, evidence_locations.language,
                        evidence_locations.direction
                 FROM evidence_locations
                 JOIN source_artifacts USING (artifact_id)
                 WHERE evidence_locations.artifact_id = ?1
                   AND evidence_locations.version = ?2
                   AND evidence_locations.ordinal = ?3",
                params![reference.artifact_id, reference.version, reference.ordinal],
                |row| Ok((row.get(0)?, RawEvidenceLocation::read(row, 1)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        resolved.push(json!({
            "location": location.into_domain()?,
            "package_path": package_path,
            "reference": reference,
        }));
    }
    let references = evidence.iter().cloned().collect::<HashSet<_>>();
    let source_relationships = load_source_relationships(connection, &references)?;
    Ok(json!({
        "data_classification": DataClassification::TenderInternal,
        "data_scope": RECORD_EXTRACTION_SCOPE,
        "authorities": authorities,
        "evidence": resolved,
        "schema_version": 1,
        "source_relationships": source_relationships,
        "tender": {
            "tender_id": tender_id.as_str(),
            "name": tender_name,
            "revision": tender_revision,
        }
    }))
}

fn record_review_data_view(
    connection: &rusqlite::Connection,
    record_id: &str,
    version: u32,
) -> Result<Value, TenderCommandError> {
    let (
        stable_key,
        kind,
        title,
        generation_instruction_json,
        fields_json,
        contradictions_json,
        author_run_id,
    ): (
        String,
        String,
        String,
        Option<String>,
        String,
        String,
        String,
    ) = connection
        .query_row(
            "SELECT tender_records.stable_key, tender_record_versions.kind,
                    tender_record_versions.title,
                    tender_record_versions.generation_instruction_json,
                    tender_record_versions.fields_json, tender_record_versions.contradictions_json,
                    tender_record_versions.author_run_id
             FROM tender_record_versions
             JOIN tender_records USING (record_id)
             WHERE tender_record_versions.record_id = ?1
               AND tender_record_versions.version = ?2",
            params![record_id, version],
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
        .map_err(sql_error)?;
    let fields: Vec<TenderRecordFieldCandidate> = parse_canonical_json(&fields_json)?;
    let generation_instruction = generation_instruction_json
        .as_deref()
        .map(parse_canonical_json::<TenderRecordGenerationInstructionCandidate>)
        .transpose()?;
    let contradictions: Vec<TenderRecordContradictionCandidate> =
        parse_canonical_json(&contradictions_json)?;
    let references = fields
        .iter()
        .flat_map(|field| field.evidence.iter().cloned())
        .chain(
            contradictions
                .iter()
                .flat_map(|contradiction| contradiction.evidence.iter().cloned()),
        )
        .chain(
            generation_instruction
                .iter()
                .flat_map(|instruction| instruction.evidence.iter().cloned()),
        )
        .collect::<HashSet<_>>();
    let mut evidence = Vec::with_capacity(references.len());
    for reference in &references {
        let (package_path, location): (String, RawEvidenceLocation) = connection
            .query_row(
                "SELECT source_artifacts.package_path,
                        evidence_locations.ordinal, evidence_locations.kind,
                        evidence_locations.structural_path, evidence_locations.provenance_json,
                        evidence_locations.section, evidence_locations.paragraph_number,
                        evidence_locations.table_number, evidence_locations.sheet_name,
                        evidence_locations.cell_range, evidence_locations.original_text,
                        evidence_locations.translated_text, evidence_locations.language,
                        evidence_locations.direction
                 FROM evidence_locations
                 JOIN source_artifacts USING (artifact_id)
                 WHERE evidence_locations.artifact_id = ?1
                   AND evidence_locations.version = ?2
                   AND evidence_locations.ordinal = ?3",
                params![reference.artifact_id, reference.version, reference.ordinal],
                |row| Ok((row.get(0)?, RawEvidenceLocation::read(row, 1)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        evidence.push(json!({
            "location": location.into_domain()?,
            "package_path": package_path,
            "reference": reference,
        }));
    }
    evidence.sort_by(|left, right| {
        left.pointer("/reference/artifact_id")
            .and_then(Value::as_str)
            .cmp(
                &right
                    .pointer("/reference/artifact_id")
                    .and_then(Value::as_str),
            )
            .then_with(|| {
                left.pointer("/reference/ordinal")
                    .and_then(Value::as_u64)
                    .cmp(&right.pointer("/reference/ordinal").and_then(Value::as_u64))
            })
    });
    let verification_eligible = record_fields_are_verifiable(connection, &fields)?
        && generation_instruction.as_ref().is_none_or(|instruction| {
            !instruction.evidence.is_empty()
                && instruction
                    .evidence
                    .iter()
                    .all(|reference| references.contains(reference))
        });
    let source_relationships = load_source_relationships(connection, &references)?;
    Ok(json!({
        "data_classification": DataClassification::TenderInternal,
        "data_scope": RECORD_REVIEW_SCOPE,
        "evidence": evidence,
        "record": {
            "author_run_id": author_run_id,
            "contradictions": contradictions,
            "fields": fields,
            "generation_instruction": generation_instruction,
            "kind": kind,
            "record_id": record_id,
            "stable_key": stable_key,
            "title": title,
            "version": version,
        },
        "schema_version": 1,
        "source_relationships": source_relationships,
        "verification_eligible": verification_eligible,
    }))
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

fn valid_record_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'_' | b'-'))
        })
}

fn format_record_cursor(stable_key: &str, version: u32) -> String {
    format!("{stable_key}:{version}")
}

fn parse_record_cursor(value: &str) -> Result<(String, u32), TenderCommandError> {
    let (stable_key, version) = value
        .rsplit_once(':')
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    let version = version
        .parse::<u32>()
        .ok()
        .filter(|version| *version > 0)
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
    if !valid_record_key(stable_key) {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok((stable_key.into(), version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(stable_key: &str) -> TenderRecordCandidate {
        TenderRecordCandidate {
            stable_key: stable_key.into(),
            kind: TenderRecordKind::Risk,
            title: stable_key.into(),
            generation_instruction: None,
            fields: Vec::new(),
            contradictions: Vec::new(),
        }
    }

    #[test]
    fn bid_decision_reentry_rejects_inventory_overflow_before_any_record_write() {
        let mut connection = rusqlite::Connection::open_in_memory().expect("open SQLite");
        connection
            .execute_batch(
                "CREATE TABLE tender (
                   singleton INTEGER PRIMARY KEY,
                   lifecycle_phase TEXT NOT NULL
                 );
                 CREATE TABLE tender_records (
                   record_id TEXT PRIMARY KEY,
                   stable_key TEXT NOT NULL UNIQUE
                 );
                 CREATE TABLE tender_record_heads (
                   record_id TEXT PRIMARY KEY,
                   current_version INTEGER NOT NULL
                 );
                 CREATE TABLE change_assessments (
                   assessment_id TEXT PRIMARY KEY
                 );
                 CREATE TABLE change_assessment_decisions (
                   assessment_id TEXT PRIMARY KEY,
                   classification TEXT NOT NULL
                 );
                 CREATE TABLE change_assessment_resolutions (
                   assessment_id TEXT PRIMARY KEY
                 );
                 INSERT INTO tender (singleton, lifecycle_phase)
                 VALUES (1, 'bid_decision');
                 WITH RECURSIVE counter(value) AS (
                   VALUES (1)
                   UNION ALL
                   SELECT value + 1 FROM counter WHERE value < 255
                 )
                 INSERT INTO tender_records (record_id, stable_key)
                 SELECT printf('%032x', value), printf('existing_%03d', value)
                 FROM counter;
                 INSERT INTO tender_record_heads (record_id, current_version)
                 SELECT record_id, 1 FROM tender_records;",
            )
            .expect("fill the accepted decision inventory boundary");
        let transaction = connection.transaction().expect("start transaction");
        let batch = TenderRecordCandidateBatch {
            records: vec![candidate("new_record_a"), candidate("new_record_b")],
        };

        let error = publish_tender_record_candidates(
            &transaction,
            &TenderId::parse("00000000000000000000000000000001").expect("Tender identity"),
            1,
            "00000000000000000000000000000002",
            &batch,
            "2026-08-09T00:00:00Z",
        )
        .expect_err("the decision inventory cap must reject the entire publication");

        assert_eq!(error.code, TenderErrorCode::InvalidCommand);
        assert_eq!(
            transaction
                .query_row("SELECT COUNT(*) FROM tender_records", [], |row| {
                    row.get::<_, u32>(0)
                })
                .expect("count retained records"),
            255
        );
        assert_eq!(
            transaction
                .query_row(
                    "SELECT COUNT(*) FROM tender_records WHERE stable_key LIKE 'new_record_%'",
                    [],
                    |row| row.get::<_, u32>(0),
                )
                .expect("count rejected records"),
            0
        );
    }
}
