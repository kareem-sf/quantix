use std::collections::{BTreeMap, BTreeSet, HashSet};

use garde::Validate;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::{
    agent_runtime::{AgentTaskInputReference, TenderTaskView},
    tender_intake::SourceRelationshipKind,
};

fn source_evidence_identity(input: &AgentTaskInputReference) -> Option<(&str, u32)> {
    let (artifact_id, ordinal) = input.reference.split_once('#')?;
    ordinal
        .parse::<u32>()
        .ok()
        .filter(|ordinal| *ordinal > 0)
        .map(|_| (artifact_id, input.version))
}

fn active_material_change_recovery_is_open(
    connection: &Connection,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM change_assessments AS assessments
               JOIN change_assessment_decisions AS decisions USING (assessment_id)
               LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
               WHERE decisions.classification = 'material'
                 AND resolutions.assessment_id IS NULL
                 AND (
                   NOT EXISTS (
                     SELECT 1 FROM change_assessment_impacts AS impacts
                     WHERE impacts.assessment_id = assessments.assessment_id
                       AND impacts.kind = 'package'
                   )
                   OR NOT EXISTS (
                     SELECT 1 FROM change_assessment_impacts AS impacts
                     WHERE impacts.assessment_id = assessments.assessment_id
                       AND impacts.kind = 'approval'
                   )
                   OR EXISTS (
                     SELECT 1
                     FROM bid_decision_package_heads AS heads
                     JOIN bid_decision_package_versions AS versions
                       ON versions.package_id = heads.package_id
                      AND versions.version = heads.current_version
                     JOIN json_each(
                       json_extract(versions.manifest_json,
                                    '$.material_change_basis.affected_areas')
                     ) AS area
                     WHERE area.value =
                       'change_assessment:' || assessments.assessment_id
                   )
                 )
             )",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn change_assessment_has_active_affected_agent_run(
    connection: &Connection,
    assessment_id: &str,
    run_id: Option<&str>,
) -> Result<bool, TenderCommandError> {
    connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1
               FROM agent_runs AS runs
               JOIN tender_tasks AS tender_tasks USING (task_id)
               WHERE runs.status = 'running'
                 AND (?2 IS NULL OR runs.run_id = ?2)
                 AND (
                   EXISTS (
                     SELECT 1 FROM change_assessment_impacts AS impacts
                     WHERE impacts.assessment_id = ?1
                       AND impacts.kind = 'agent_run'
                       AND impacts.object_id = runs.run_id
                   )
                   OR EXISTS (
                     SELECT 1
                     FROM change_assessment_impacts AS impacts
                     JOIN production_task_attempts AS attempts
                       ON attempts.production_task_id = impacts.object_id
                      AND attempts.task_id = runs.task_id
                     WHERE impacts.assessment_id = ?1
                       AND impacts.kind = 'production_task'
                   )
                   OR (
                     NOT EXISTS (
                       SELECT 1 FROM production_task_attempts AS production_attempt
                       WHERE production_attempt.task_id = runs.task_id
                     )
                     AND EXISTS (
                       SELECT 1
                       FROM json_each(tender_tasks.exact_inputs_json) AS input
                     JOIN change_assessments AS assessments
                       ON assessments.assessment_id = ?1
                     JOIN source_relationships AS relationships
                       ON relationships.relationship_id = assessments.relationship_id
                     WHERE (
                       json_extract(input.value, '$.kind') = 'source_evidence'
                       AND (
                         (json_extract(input.value, '$.reference') LIKE
                            relationships.prior_artifact_id || '#%'
                          AND CAST(json_extract(input.value, '$.version') AS INTEGER) =
                            relationships.prior_version)
                         OR (json_extract(input.value, '$.reference') LIKE
                            relationships.replacement_artifact_id || '#%'
                          AND CAST(json_extract(input.value, '$.version') AS INTEGER) =
                            relationships.replacement_version)
                       )
                     ) OR (
                       json_extract(input.value, '$.kind') = 'source_artifact'
                       AND (
                         (json_extract(input.value, '$.reference') =
                            relationships.prior_artifact_id
                          AND CAST(json_extract(input.value, '$.version') AS INTEGER) =
                            relationships.prior_version)
                         OR (json_extract(input.value, '$.reference') =
                            relationships.replacement_artifact_id
                          AND CAST(json_extract(input.value, '$.version') AS INTEGER) =
                            relationships.replacement_version)
                       )
                     )
                     )
                   )
                   OR (
                     NOT EXISTS (
                       SELECT 1 FROM production_task_attempts AS production_attempt
                       WHERE production_attempt.task_id = runs.task_id
                     )
                     AND EXISTS (
                       SELECT 1
                       FROM json_each(tender_tasks.exact_inputs_json) AS input
                     JOIN change_assessment_impacts AS impacts
                       ON impacts.assessment_id = ?1
                      AND impacts.object_id = json_extract(input.value, '$.reference')
                      AND impacts.object_version =
                        CAST(json_extract(input.value, '$.version') AS INTEGER)
                     WHERE (impacts.kind = 'package'
                              AND json_extract(input.value, '$.kind') =
                                'bid_decision_package')
                        OR (impacts.kind = 'tender_record'
                              AND json_extract(input.value, '$.kind') =
                                'tender_record_version')
                        OR (impacts.kind = 'work_plan'
                              AND json_extract(input.value, '$.kind') =
                                'work_plan_version')
                        OR (impacts.kind = 'production_artifact'
                              AND json_extract(input.value, '$.kind') =
                                'production_artifact_version')
                        OR (impacts.kind = 'tender_query'
                              AND json_extract(input.value, '$.kind') =
                                'tender_query_version')
                     )
                   )
                 )
             )",
            params![assessment_id, run_id],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

fn change_assessment_has_active_affected_execution(
    transaction: &Transaction<'_>,
    assessment_id: &str,
) -> Result<bool, TenderCommandError> {
    if change_assessment_has_active_affected_agent_run(transaction, assessment_id, None)? {
        return Ok(true);
    }
    transaction
        .query_row(
            "SELECT EXISTS(
               SELECT 1
               FROM parse_attempts AS attempts
               JOIN change_assessments AS assessments
                 ON assessments.assessment_id = ?1
               JOIN source_relationships AS relationships
                 ON relationships.relationship_id = assessments.relationship_id
               WHERE attempts.status = 'running'
                 AND (
                   (attempts.artifact_id = relationships.prior_artifact_id
                     AND attempts.version = relationships.prior_version)
                   OR (attempts.artifact_id = relationships.replacement_artifact_id
                     AND attempts.version = relationships.replacement_version)
                 )
             ) OR EXISTS(
               SELECT 1
               FROM production_tasks AS tasks
               JOIN change_assessment_impacts AS impacts
                 ON impacts.assessment_id = ?1
                AND impacts.kind = 'production_task'
                AND impacts.object_id = tasks.production_task_id
               JOIN production_task_attempts AS attempts
                 ON attempts.production_task_id = tasks.production_task_id
                AND attempts.task_id = tasks.task_id
               JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
               JOIN agent_run_recovery_dispositions AS dispositions
                 ON dispositions.run_id = runs.run_id
                AND dispositions.disposition = 'retry_task'
               WHERE tasks.status = 'indeterminate'
                 AND NOT EXISTS(
                   SELECT 1 FROM agent_runs AS retries
                   WHERE retries.retry_of_run_id = runs.run_id
                 )
             )",
            [assessment_id],
            |row| row.get(0),
        )
        .map_err(sql_error)
}

use super::agent_records::load_task;
use super::bid_decisions::invalidate_accepted_bid_decision_for_change_assessment;
use super::production_scheduler::record_version_is_relevant_to_production_task;
use super::{
    append_audit_event_with_sequence, lock_mutex_with_check, random_identifier, sha256_hex,
    sql_error, sqlite_timestamp, BidPackageOperationBudget, QuantixHost, TenderCommandError,
    TenderErrorCode, TenderEvidenceReference, TenderId, TenderLifecyclePhase, TenderStore,
    WorkPlanTask,
};

const MAX_CHANGE_ASSESSMENTS: u32 = 128;
const MAX_CHANGE_IMPACTS: usize = 4_096;
const MAX_CHANGE_ITEMS: usize = 128;
const MAX_CHANGE_PAGE_ITEMS: u32 = 4;
const MAX_CHANGE_PAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_CHANGE_SOURCE_EVIDENCE_PREVIEW: usize = 8;
const MAX_CHANGE_SOURCE_EVIDENCE_EXCERPT_BYTES: usize = 2_000;
const MAX_CHANGE_DEPENDENCIES_PER_IMPACT: usize = 512;
const MAX_CHANGE_DEPENDENCY_EDGES: usize = 16_384;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct InspectChangeAssessmentsCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(range(min = 1))]
    pub before_sequence: Option<u32>,
    #[garde(range(min = 1, max = 4))]
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct DecideChangeAssessmentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 32, max = 32))]
    pub assessment_id: String,
    #[garde(length(bytes, min = 64, max = 64))]
    pub assessment_manifest_sha256: String,
    #[garde(skip)]
    pub classification: ChangeAssessmentClassification,
    #[garde(length(bytes, min = 1, max = 4000))]
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeAssessmentClassification {
    Irrelevant,
    Material,
}

impl ChangeAssessmentClassification {
    fn as_str(self) -> &'static str {
        match self {
            Self::Irrelevant => "irrelevant",
            Self::Material => "material",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "irrelevant" => Ok(Self::Irrelevant),
            "material" => Ok(Self::Material),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeAssessmentStatus {
    Pending,
    ReworkRequired,
    Resolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeAssessmentImpactKind {
    TenderRecord,
    WorkPlan,
    ProductionTask,
    AgentRun,
    ProductionArtifact,
    TenderQuery,
    CalculationRun,
    Estimate,
    PricingDecision,
    Review,
    CoordinatedBaseline,
    Package,
    Approval,
}

impl ChangeAssessmentImpactKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::TenderRecord => "tender_record",
            Self::WorkPlan => "work_plan",
            Self::ProductionTask => "production_task",
            Self::AgentRun => "agent_run",
            Self::ProductionArtifact => "production_artifact",
            Self::TenderQuery => "tender_query",
            Self::CalculationRun => "calculation_run",
            Self::Estimate => "estimate",
            Self::PricingDecision => "pricing_decision",
            Self::Review => "review",
            Self::CoordinatedBaseline => "coordinated_baseline",
            Self::Package => "package",
            Self::Approval => "approval",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "tender_record" => Ok(Self::TenderRecord),
            "work_plan" => Ok(Self::WorkPlan),
            "production_task" => Ok(Self::ProductionTask),
            "agent_run" => Ok(Self::AgentRun),
            "production_artifact" => Ok(Self::ProductionArtifact),
            "tender_query" => Ok(Self::TenderQuery),
            "calculation_run" => Ok(Self::CalculationRun),
            "estimate" => Ok(Self::Estimate),
            "pricing_decision" => Ok(Self::PricingDecision),
            "review" => Ok(Self::Review),
            "coordinated_baseline" => Ok(Self::CoordinatedBaseline),
            "package" => Ok(Self::Package),
            "approval" => Ok(Self::Approval),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeAssessmentObjectKind {
    SourceArtifact,
    TenderRecord,
    WorkPlan,
    ProductionTask,
    AgentRun,
    ProductionArtifact,
    TenderQuery,
    CalculationRun,
    Estimate,
    PricingDecision,
    Review,
    CoordinatedBaseline,
    Package,
    Approval,
}

impl From<ChangeAssessmentImpactKind> for ChangeAssessmentObjectKind {
    fn from(value: ChangeAssessmentImpactKind) -> Self {
        match value {
            ChangeAssessmentImpactKind::TenderRecord => Self::TenderRecord,
            ChangeAssessmentImpactKind::WorkPlan => Self::WorkPlan,
            ChangeAssessmentImpactKind::ProductionTask => Self::ProductionTask,
            ChangeAssessmentImpactKind::AgentRun => Self::AgentRun,
            ChangeAssessmentImpactKind::ProductionArtifact => Self::ProductionArtifact,
            ChangeAssessmentImpactKind::TenderQuery => Self::TenderQuery,
            ChangeAssessmentImpactKind::CalculationRun => Self::CalculationRun,
            ChangeAssessmentImpactKind::Estimate => Self::Estimate,
            ChangeAssessmentImpactKind::PricingDecision => Self::PricingDecision,
            ChangeAssessmentImpactKind::Review => Self::Review,
            ChangeAssessmentImpactKind::CoordinatedBaseline => Self::CoordinatedBaseline,
            ChangeAssessmentImpactKind::Package => Self::Package,
            ChangeAssessmentImpactKind::Approval => Self::Approval,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeAssessmentDependencyKind {
    SourceEvidence,
    RecordMembership,
    TaskInput,
    RunExecution,
    ArtifactOutput,
    QueryEvidence,
    CalculationInput,
    ReviewTarget,
    BaselineBinding,
    PackageBinding,
    ApprovalTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeAssessmentDependencyReference {
    pub kind: ChangeAssessmentObjectKind,
    pub object_id: String,
    pub object_version: u32,
    pub dependency_kind: ChangeAssessmentDependencyKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeAssessmentImpactConsequence {
    Stale,
    Reopen,
    Revoke,
}

impl ChangeAssessmentImpactConsequence {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stale => "stale",
            Self::Reopen => "reopen",
            Self::Revoke => "revoke",
        }
    }

    fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "stale" => Ok(Self::Stale),
            "reopen" => Ok(Self::Reopen),
            "revoke" => Ok(Self::Revoke),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeAssessmentEvidenceExcerpt {
    pub ordinal: u32,
    pub kind: String,
    pub structural_path: String,
    pub original_text: String,
    pub translated_text: Option<String>,
    pub language: String,
    pub text_sha256: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeAssessmentSource {
    pub artifact_id: String,
    pub version: u32,
    pub package_path: String,
    pub document_type: String,
    pub sha256: String,
    pub evidence_count: u32,
    pub evidence_preview: Vec<ChangeAssessmentEvidenceExcerpt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeAssessmentImpact {
    pub kind: ChangeAssessmentImpactKind,
    pub object_id: String,
    pub object_version: u32,
    pub dependencies: Vec<ChangeAssessmentDependencyReference>,
    pub consequence: ChangeAssessmentImpactConsequence,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeAssessmentApprovalConsequence {
    pub reference: String,
    pub consequence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeAssessmentDecision {
    pub classification: ChangeAssessmentClassification,
    pub rationale: String,
    pub decided_by: String,
    pub acting_role: String,
    pub lifecycle_after: TenderLifecyclePhase,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeAssessment {
    pub assessment_sequence: u32,
    pub assessment_id: String,
    pub relationship_id: String,
    pub relationship_kind: SourceRelationshipKind,
    pub prior_source: ChangeAssessmentSource,
    pub replacement_source: ChangeAssessmentSource,
    pub lifecycle_before: TenderLifecyclePhase,
    pub status: ChangeAssessmentStatus,
    pub baseline_id: Option<String>,
    pub baseline_version: Option<u32>,
    pub baseline_manifest_sha256: Option<String>,
    pub impacts: Vec<ChangeAssessmentImpact>,
    pub affected_commitments: Vec<String>,
    pub proposed_rework: Vec<String>,
    pub unchanged_scope: Vec<String>,
    pub deadline_effect: String,
    pub approval_consequences: Vec<ChangeAssessmentApprovalConsequence>,
    pub decision: Option<ChangeAssessmentDecision>,
    pub resolution_baseline_id: Option<String>,
    pub resolution_baseline_version: Option<u32>,
    pub manifest_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChangeAssessmentPage {
    pub active: Option<ChangeAssessment>,
    pub items: Vec<ChangeAssessment>,
    pub next_before_sequence: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ChangeAssessmentManifest {
    schema_version: u32,
    assessment_id: String,
    relationship_id: String,
    relationship_kind: SourceRelationshipKind,
    prior_source: ChangeAssessmentSource,
    replacement_source: ChangeAssessmentSource,
    lifecycle_before: TenderLifecyclePhase,
    baseline_id: Option<String>,
    baseline_version: Option<u32>,
    baseline_manifest_sha256: Option<String>,
    impacts: Vec<ChangeAssessmentImpact>,
    affected_commitments: Vec<String>,
    proposed_rework: Vec<String>,
    unchanged_scope: Vec<String>,
    deadline_effect: String,
    approval_consequences: Vec<ChangeAssessmentApprovalConsequence>,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ChangeAssessmentDecisionManifest {
    schema_version: u32,
    assessment_id: String,
    assessment_manifest_sha256: String,
    classification: ChangeAssessmentClassification,
    rationale: String,
    decided_by: String,
    acting_role: String,
    lifecycle_before: TenderLifecyclePhase,
    lifecycle_after: TenderLifecyclePhase,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ImpactKey(ChangeAssessmentImpactKind, String, u32);

pub(crate) struct ActiveChangeRecordRecoveryContext {
    pub(crate) assessment_id: String,
    pub(crate) allowed_stable_keys: Vec<String>,
    pub(crate) prior_records: Vec<Value>,
}

impl QuantixHost {
    pub fn inspect_change_assessments(
        &self,
        command: InspectChangeAssessmentsCommand,
    ) -> Result<ChangeAssessmentPage, TenderCommandError> {
        super::require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        if command.limit > MAX_CHANGE_PAGE_ITEMS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let result = lock_mutex_with_check(&store, &mut || budget.check())?
            .inspect_change_assessments(command.before_sequence, command.limit, budget);
        result
    }

    pub fn decide_change_assessment(
        &self,
        mut command: DecideChangeAssessmentCommand,
    ) -> Result<ChangeAssessment, TenderCommandError> {
        super::require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let budget = BidPackageOperationBudget::for_tender(&tender_id);
        let store = self.tender_store_with_check(&tender_id, &mut || budget.check())?;
        let mut store = lock_mutex_with_check(&store, &mut || budget.check())?;
        if command.validate().is_err()
            || command.rationale.trim().is_empty()
            || !valid_hash(&command.assessment_manifest_sha256)
        {
            store.record_change_assessment_denial(&tender_id, "command_shape")?;
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        command.rationale = command.rationale.trim().to_owned();
        store.decide_change_assessment(&tender_id, &command, budget)
    }
}

impl TenderStore {
    pub(crate) fn active_change_record_recovery_context(
        transaction: &Transaction<'_>,
    ) -> Result<Option<ActiveChangeRecordRecoveryContext>, TenderCommandError> {
        if !active_material_change_recovery_is_open(transaction)? {
            return Ok(None);
        }
        let assessment_id: Option<String> = transaction
            .query_row(
                "SELECT assessments.assessment_id
                 FROM change_assessments AS assessments
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 WHERE decisions.classification = 'material'
                   AND resolutions.assessment_id IS NULL
                 ORDER BY assessments.assessment_sequence DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(assessment_id) = assessment_id else {
            return Ok(None);
        };
        let mut statement = transaction
            .prepare(
                "SELECT records.stable_key, versions.kind, versions.title,
                        versions.fields_json, versions.contradictions_json
                 FROM change_assessment_impacts AS impacts
                 JOIN tender_records AS records ON records.record_id = impacts.object_id
                 JOIN tender_record_versions AS versions
                   ON versions.record_id = impacts.object_id
                  AND versions.version = impacts.object_version
                 WHERE impacts.assessment_id = ?1 AND impacts.kind = 'tender_record'
                 ORDER BY impacts.impact_sequence LIMIT 257",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([&assessment_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(sql_error)?;
        let mut stable_keys = Vec::new();
        let mut prior_records = Vec::new();
        for row in rows {
            if stable_keys.len() == 256 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (stable_key, kind, title, fields_json, contradictions_json) =
                row.map_err(sql_error)?;
            stable_keys.push(stable_key.clone());
            prior_records.push(json!({
                "stable_key": stable_key,
                "kind": kind,
                "title": title,
                "fields": parse_canonical::<Value>(&fields_json)?,
                "contradictions": parse_canonical::<Value>(&contradictions_json)?,
            }));
        }
        if stable_keys.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(Some(ActiveChangeRecordRecoveryContext {
            assessment_id,
            allowed_stable_keys: stable_keys,
            prior_records,
        }))
    }

    fn active_change_recovery_intake_is_open(&self) -> Result<bool, TenderCommandError> {
        active_material_change_recovery_is_open(&self.connection)
    }

    pub(crate) fn active_change_replacement_is(
        &self,
        artifact_id: &str,
        version: u32,
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessments AS assessments
                   JOIN change_assessment_decisions AS decisions USING (assessment_id)
                   JOIN source_relationships AS relationships USING (relationship_id)
                   LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                   WHERE relationships.replacement_artifact_id = ?1
                     AND relationships.replacement_version = ?2
                     AND decisions.classification = 'material'
                     AND resolutions.assessment_id IS NULL
                 )",
                params![artifact_id, version],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn active_change_inputs_include_replacement(
        &self,
        inputs: &[AgentTaskInputReference],
    ) -> Result<bool, TenderCommandError> {
        for input in inputs {
            let Some((artifact_id, version)) = source_evidence_identity(input) else {
                continue;
            };
            if self.active_change_replacement_is(artifact_id, version)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn active_change_allows_object(
        &self,
        kind: ChangeAssessmentImpactKind,
        object_id: &str,
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessment_impacts AS impacts
                   JOIN change_assessment_decisions AS decisions USING (assessment_id)
                   LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                   WHERE impacts.kind = ?1 AND impacts.object_id = ?2
                     AND decisions.classification = 'material'
                     AND resolutions.assessment_id IS NULL
                 )",
                params![kind.as_str(), object_id],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn active_change_allows_calculation_run(
        &self,
        calculation_run_id: &str,
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        let row: Option<(String, String, String, u32)> = self
            .connection
            .query_row(
                "SELECT runs.manifest_json, assessments.assessment_id,
                        relationships.replacement_artifact_id,
                        relationships.replacement_version
                 FROM calculation_runs AS runs
                 JOIN change_assessments AS assessments
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 JOIN source_relationships AS relationships USING (relationship_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 WHERE runs.calculation_run_id = ?1
                   AND runs.created_at >= decisions.created_at
                   AND decisions.classification = 'material'
                   AND resolutions.assessment_id IS NULL
                 ORDER BY assessments.assessment_sequence DESC LIMIT 1",
                [calculation_run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((manifest_json, assessment_id, replacement_artifact_id, replacement_version)) =
            row
        else {
            return Ok(false);
        };
        if json_text_references_source(
            &manifest_json,
            &replacement_artifact_id,
            replacement_version,
        )? {
            return Ok(true);
        }
        let manifest: Value = parse_canonical(&manifest_json)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT object_id, object_version FROM change_assessment_impacts
                 WHERE assessment_id = ?1 AND kind IN (
                   'tender_record', 'tender_query', 'calculation_run'
                 ) ORDER BY impact_sequence LIMIT 1025",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([assessment_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?;
        let mut count = 0usize;
        for row in rows {
            count += 1;
            if count > 1_024 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (object_id, object_version) = row.map_err(sql_error)?;
            if json_contains_object_version(&manifest, &object_id, object_version) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn active_change_allows_estimate(
        &self,
        basis_id: &str,
        version: u32,
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1
                   FROM basis_of_estimate_versions AS versions
                   JOIN change_assessment_decisions AS decisions
                   JOIN change_assessments AS assessments USING (assessment_id)
                   LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                   WHERE versions.basis_id = ?1 AND versions.version = ?2
                     AND versions.created_at >= decisions.created_at
                     AND decisions.classification = 'material'
                     AND resolutions.assessment_id IS NULL
                     AND EXISTS(
                       SELECT 1 FROM change_assessment_impacts AS impacts
                       WHERE impacts.assessment_id = assessments.assessment_id
                         AND impacts.kind = 'estimate'
                     )
                 )",
                params![basis_id, version],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn active_change_allows_pricing_object(
        &self,
        object_id: &str,
        version: u32,
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1
                   FROM (
                     SELECT baseline_id AS object_id, version, created_at
                     FROM priced_cost_baseline_versions
                     UNION ALL
                     SELECT adjustment_id, version, created_at FROM pricing_adjustment_versions
                     UNION ALL
                     SELECT strategy_id, 1, created_at FROM commercial_strategies
                     UNION ALL
                     SELECT pricing_scenario_id, version, created_at
                     FROM pricing_scenario_versions
                   ) AS objects
                   JOIN change_assessment_decisions AS decisions
                   JOIN change_assessments AS assessments USING (assessment_id)
                   LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                   WHERE objects.object_id = ?1 AND objects.version = ?2
                     AND objects.created_at >= decisions.created_at
                     AND decisions.classification = 'material'
                     AND resolutions.assessment_id IS NULL
                     AND EXISTS(
                       SELECT 1 FROM change_assessment_impacts AS impacts
                       WHERE impacts.assessment_id = assessments.assessment_id
                         AND impacts.kind = 'pricing_decision'
                     )
                 )",
                params![object_id, version],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn active_change_allows_inputs(
        &self,
        inputs: &[AgentTaskInputReference],
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        let active: Option<(String, String, u32, String, u32)> = self
            .connection
            .query_row(
                "SELECT assessments.assessment_id,
                        relationships.replacement_artifact_id,
                        relationships.replacement_version,
                        relationships.prior_artifact_id, relationships.prior_version
                 FROM change_assessments AS assessments
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 JOIN source_relationships AS relationships USING (relationship_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 JOIN tender ON tender.singleton = 1
                 WHERE decisions.classification = 'material'
                   AND resolutions.assessment_id IS NULL
                   AND tender.lifecycle_phase IN (
                     'bid_decision', 'tender_planning', 'active_production'
                   )
                 ORDER BY assessments.assessment_sequence DESC LIMIT 1",
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
            .optional()
            .map_err(sql_error)?;
        let Some((
            assessment_id,
            replacement_artifact_id,
            replacement_version,
            _prior_artifact_id,
            _prior_version,
        )) = active
        else {
            return Ok(false);
        };
        if inputs.iter().any(|input| {
            input.kind == "change_assessment"
                && input.reference == assessment_id
                && input.version == 1
        }) {
            return Ok(true);
        }
        for input in inputs {
            if input.kind == "bid_decision_package" {
                let bound_successor: bool = self
                    .connection
                    .query_row(
                        "SELECT EXISTS(
                           SELECT 1 FROM bid_decision_package_versions AS versions
                           JOIN json_each(
                             json_extract(versions.manifest_json,
                                          '$.material_change_basis.affected_areas')
                           ) AS area
                           WHERE versions.package_id = ?1 AND versions.version = ?2
                             AND area.value = 'change_assessment:' || ?3
                         )",
                        params![input.reference, input.version, assessment_id],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if bound_successor {
                    return Ok(true);
                }
            }
        }
        let source_inputs = inputs
            .iter()
            .filter(|input| {
                matches!(
                    input.kind.as_str(),
                    "source_evidence"
                        | "calculation_quantity_evidence"
                        | "calculation_unit_rate_evidence"
                )
            })
            .collect::<Vec<_>>();
        if !source_inputs.is_empty()
            && source_inputs.iter().any(|input| {
                source_evidence_identity(input)
                    == Some((replacement_artifact_id.as_str(), replacement_version))
            })
        {
            return Ok(true);
        }
        for input in inputs {
            let successor_is_allowed = match input.kind.as_str() {
                "approved_calculation_run" | "calculation_run" => {
                    self.active_change_allows_calculation_run(&input.reference)?
                }
                "basis_of_estimate_version" => {
                    self.active_change_allows_estimate(&input.reference, input.version)?
                }
                "priced_cost_baseline_version"
                | "pricing_adjustment_version"
                | "commercial_strategy"
                | "pricing_scenario_version" => {
                    self.active_change_allows_pricing_object(&input.reference, input.version)?
                }
                _ => false,
            };
            if successor_is_allowed {
                return Ok(true);
            }
            let kind = match input.kind.as_str() {
                "tender_record_version" => Some(ChangeAssessmentImpactKind::TenderRecord),
                "tender_query_version" => Some(ChangeAssessmentImpactKind::TenderQuery),
                "calculation_run" => Some(ChangeAssessmentImpactKind::CalculationRun),
                "bid_decision_package" => Some(ChangeAssessmentImpactKind::Package),
                "work_plan" => Some(ChangeAssessmentImpactKind::WorkPlan),
                "production_artifact_version" => {
                    Some(ChangeAssessmentImpactKind::ProductionArtifact)
                }
                "coordinated_bid_baseline" => Some(ChangeAssessmentImpactKind::CoordinatedBaseline),
                _ => None,
            };
            let Some(kind) = kind else { continue };
            let impacted: bool = self
                .connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM change_assessment_impacts
                       WHERE assessment_id = ?1 AND kind = ?2 AND object_id = ?3
                         AND (object_version = ?4 OR kind IN ('tender_record', 'tender_query'))
                     )",
                    params![assessment_id, kind.as_str(), input.reference, input.version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if impacted {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn active_change_allows_run(
        &self,
        run_id: &str,
    ) -> Result<bool, TenderCommandError> {
        let task_id: Option<String> = self
            .connection
            .query_row(
                "SELECT task_id FROM agent_runs WHERE run_id = ?1",
                [run_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(task_id) = task_id else {
            return Ok(false);
        };
        let task = load_task(&self.connection, &task_id)?;
        self.active_change_allows_agent_task(&task)
    }

    pub(crate) fn pending_change_has_active_affected_run(
        &self,
        run_id: &str,
    ) -> Result<bool, TenderCommandError> {
        let assessment_ids = self
            .connection
            .prepare(
                "SELECT assessments.assessment_id
                 FROM change_assessments AS assessments
                 LEFT JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 WHERE decisions.assessment_id IS NULL
                   AND resolutions.assessment_id IS NULL
                 ORDER BY assessments.audit_sequence",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(sql_error)?;
        for assessment_id in assessment_ids {
            if change_assessment_has_active_affected_agent_run(
                &self.connection,
                &assessment_id,
                Some(run_id),
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn unresolved_change_allows_unaffected_run(
        &self,
        run_id: &str,
    ) -> Result<bool, TenderCommandError> {
        let running = self
            .connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM agent_runs
                   WHERE run_id = ?1 AND status = 'running'
                 )",
                [run_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(sql_error)?;
        if !running {
            return Ok(false);
        }
        let assessment_ids = self
            .connection
            .prepare(
                "SELECT assessments.assessment_id
                 FROM change_assessments AS assessments
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 WHERE resolutions.assessment_id IS NULL
                 ORDER BY assessments.audit_sequence",
            )
            .and_then(|mut statement| {
                statement
                    .query_map([], |row| row.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(sql_error)?;
        if assessment_ids.is_empty() {
            return Ok(false);
        }
        for assessment_id in assessment_ids {
            if change_assessment_has_active_affected_agent_run(
                &self.connection,
                &assessment_id,
                Some(run_id),
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn active_change_allows_baseline_successor(
        &self,
        baseline_id: &str,
        baseline_version: u32,
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1
                   FROM change_assessment_impacts AS impacts
                   JOIN change_assessment_decisions AS decisions USING (assessment_id)
                   LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                   WHERE impacts.kind = 'coordinated_baseline'
                     AND impacts.object_id = ?1 AND impacts.object_version = ?2
                     AND decisions.classification = 'material'
                     AND resolutions.assessment_id IS NULL
                 )",
                params![baseline_id, baseline_version],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn active_change_allows_agent_task(
        &self,
        task: &TenderTaskView,
    ) -> Result<bool, TenderCommandError> {
        self.active_change_allows_inputs(&task.exact_inputs)
    }

    pub(super) fn open_change_assessment_in_transaction(
        transaction: &Transaction<'_>,
        tender_id: &TenderId,
        tender_revision: u32,
        relationship_id: &str,
        lifecycle_before: TenderLifecyclePhase,
        created_at: &str,
        budget: BidPackageOperationBudget,
    ) -> Result<String, TenderCommandError> {
        budget.check()?;
        let unresolved: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessments AS assessments
                   LEFT JOIN change_assessment_resolutions AS resolutions
                     USING (assessment_id)
                   WHERE resolutions.assessment_id IS NULL
                 )",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if unresolved || lifecycle_before == TenderLifecyclePhase::Declined {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let assessment_count: u32 = transaction
            .query_row("SELECT COUNT(*) FROM change_assessments", [], |row| {
                row.get(0)
            })
            .map_err(sql_error)?;
        if assessment_count >= MAX_CHANGE_ASSESSMENTS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let (relationship_kind, prior_source, replacement_source) =
            load_relationship_sources(transaction, relationship_id)?;
        let mut impacts = derive_impacts(
            transaction,
            &prior_source.artifact_id,
            prior_source.version,
            budget,
        )?;
        impacts.sort_by(|left, right| {
            (left.kind, &left.object_id, left.object_version).cmp(&(
                right.kind,
                &right.object_id,
                right.object_version,
            ))
        });
        if impacts.len() > MAX_CHANGE_IMPACTS {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let baseline: Option<(String, u32, String)> = transaction
            .query_row(
                "SELECT versions.baseline_id, versions.version, versions.manifest_sha256
                 FROM coordinated_bid_baseline_head AS head
                 JOIN coordinated_bid_baseline_versions AS versions
                   ON versions.baseline_id = head.baseline_id
                  AND versions.version = head.current_version
                 LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let affected_commitments = affected_commitments(&impacts);
        let proposed_rework = proposed_rework(&impacts);
        let unchanged_scope = unchanged_scope(transaction, &impacts)?;
        let deadline_effect = if impacts.iter().any(|impact| {
            impact.kind == ChangeAssessmentImpactKind::TenderRecord
                && impact.summary.to_ascii_lowercase().contains("deadline")
        }) {
            "One or more exact deadline commitments depend on the prior Source Artifact Version and must be revalidated before baseline approval."
                .to_owned()
        } else if impacts
            .iter()
            .any(|impact| impact.kind == ChangeAssessmentImpactKind::ProductionTask)
        {
            "Affected Work Plan task deadlines remain visible and must be reconfirmed through targeted rework."
                .to_owned()
        } else {
            "No current deadline dependency was identified; existing deadline commitments remain unchanged."
                .to_owned()
        };
        let approval_consequences = impacts
            .iter()
            .filter(|impact| impact.kind == ChangeAssessmentImpactKind::Approval)
            .take(MAX_CHANGE_ITEMS)
            .map(|impact| ChangeAssessmentApprovalConsequence {
                reference: impact.object_id.clone(),
                consequence: impact.summary.clone(),
            })
            .collect::<Vec<_>>();
        let assessment_id = random_identifier(transaction)?;
        let manifest = ChangeAssessmentManifest {
            schema_version: 1,
            assessment_id: assessment_id.clone(),
            relationship_id: relationship_id.to_owned(),
            relationship_kind,
            prior_source,
            replacement_source,
            lifecycle_before,
            baseline_id: baseline.as_ref().map(|value| value.0.clone()),
            baseline_version: baseline.as_ref().map(|value| value.1),
            baseline_manifest_sha256: baseline.as_ref().map(|value| value.2.clone()),
            impacts: impacts.clone(),
            affected_commitments: affected_commitments.clone(),
            proposed_rework: proposed_rework.clone(),
            unchanged_scope: unchanged_scope.clone(),
            deadline_effect: deadline_effect.clone(),
            approval_consequences: approval_consequences.clone(),
            created_at: created_at.to_owned(),
        };
        let manifest_json = canonical_json(&manifest)?;
        if manifest_json.len() > 4 * 1024 * 1024 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            transaction,
            tender_id.as_str(),
            "change_assessment_opened",
            tender_revision,
            json!({
                "assessment_id": assessment_id,
                "impact_count": impacts.len().to_string(),
                "lifecycle_after": TenderLifecyclePhase::ChangeAssessment,
                "lifecycle_before": lifecycle_before,
                "manifest_sha256": manifest_sha256,
                "relationship_id": relationship_id,
            }),
            created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO change_assessments (
                   assessment_id, relationship_id, lifecycle_before, baseline_id,
                   baseline_version, baseline_manifest_sha256, affected_commitments_json,
                   proposed_rework_json, unchanged_scope_json, deadline_effect,
                   approval_consequences_json, manifest_json, manifest_sha256,
                   audit_sequence, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    assessment_id,
                    relationship_id,
                    lifecycle_before.as_str(),
                    baseline.as_ref().map(|value| value.0.as_str()),
                    baseline.as_ref().map(|value| value.1),
                    baseline.as_ref().map(|value| value.2.as_str()),
                    canonical_json(&affected_commitments)?,
                    canonical_json(&proposed_rework)?,
                    canonical_json(&unchanged_scope)?,
                    deadline_effect,
                    canonical_json(&approval_consequences)?,
                    manifest_json,
                    manifest_sha256,
                    audit_sequence,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        for (index, impact) in impacts.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO change_assessment_impacts (
                       assessment_id, impact_sequence, kind, object_id, object_version,
                       dependencies_json, consequence, summary
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        assessment_id,
                        u32::try_from(index + 1).map_err(|_| {
                            TenderCommandError::new(TenderErrorCode::InvalidCommand)
                        })?,
                        impact.kind.as_str(),
                        impact.object_id,
                        impact.object_version,
                        canonical_json(&impact.dependencies)?,
                        impact.consequence.as_str(),
                        impact.summary,
                    ],
                )
                .map_err(sql_error)?;
        }
        transaction
            .execute(
                "UPDATE tender SET lifecycle_phase = 'change_assessment' WHERE singleton = 1",
                [],
            )
            .map_err(sql_error)?;
        Ok(assessment_id)
    }

    fn decide_change_assessment(
        &mut self,
        tender_id: &TenderId,
        command: &DecideChangeAssessmentCommand,
        budget: BidPackageOperationBudget,
    ) -> Result<ChangeAssessment, TenderCommandError> {
        self.require_storage_writable()?;
        budget.check()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (revision, lifecycle): (u32, String) = transaction
            .query_row(
                "SELECT current_revision, lifecycle_phase FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if TenderLifecyclePhase::parse(&lifecycle)? != TenderLifecyclePhase::ChangeAssessment {
            return deny_change_assessment_transaction(
                transaction,
                tender_id,
                revision,
                "lifecycle_not_change_assessment",
            );
        }
        let row: Option<(String, String, u32)> = transaction
            .query_row(
                "SELECT assessments.manifest_sha256, assessments.lifecycle_before,
                        (SELECT COUNT(*) FROM change_assessment_impacts
                         WHERE assessment_id = assessments.assessment_id)
                 FROM change_assessments AS assessments
                 LEFT JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 WHERE assessments.assessment_id = ?1 AND decisions.assessment_id IS NULL",
                [&command.assessment_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((stored_manifest_sha256, lifecycle_before, impact_count)) = row else {
            return deny_change_assessment_transaction(
                transaction,
                tender_id,
                revision,
                "assessment_not_pending",
            );
        };
        if stored_manifest_sha256 != command.assessment_manifest_sha256 {
            return deny_change_assessment_transaction(
                transaction,
                tender_id,
                revision,
                "assessment_basis_mismatch",
            );
        }
        let lifecycle_before = TenderLifecyclePhase::parse(&lifecycle_before)?;
        if command.classification == ChangeAssessmentClassification::Material && impact_count > 0 {
            let active_execution = change_assessment_has_active_affected_execution(
                &transaction,
                &command.assessment_id,
            )?;
            if active_execution {
                return deny_change_assessment_transaction(
                    transaction,
                    tender_id,
                    revision,
                    "affected_execution_active",
                );
            }
        }
        let created_at = sqlite_timestamp(&transaction)?;
        let package_reentry = command.classification == ChangeAssessmentClassification::Material
            && transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM change_assessment_impacts
                       WHERE assessment_id = ?1 AND kind = 'package'
                     )",
                    [&command.assessment_id],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?;
        let lifecycle_after = if package_reentry {
            TenderLifecyclePhase::BidDecision
        } else {
            lifecycle_before
        };
        let manifest = ChangeAssessmentDecisionManifest {
            schema_version: 1,
            assessment_id: command.assessment_id.clone(),
            assessment_manifest_sha256: command.assessment_manifest_sha256.clone(),
            classification: command.classification,
            rationale: command.rationale.clone(),
            decided_by: "engineer_user".into(),
            acting_role: "tendering_manager".into(),
            lifecycle_before: TenderLifecyclePhase::ChangeAssessment,
            lifecycle_after,
            created_at: created_at.clone(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let audit_sequence = append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "change_assessment_decided",
            revision,
            json!({
                "assessment_id": command.assessment_id,
                "assessment_manifest_sha256": command.assessment_manifest_sha256,
                "classification": command.classification,
                "lifecycle_after": lifecycle_after,
                "manifest_sha256": manifest_sha256,
                "rationale": command.rationale,
            }),
            &created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO change_assessment_decisions (
                   assessment_id, classification, rationale, decided_by, acting_role,
                   lifecycle_after, manifest_json, manifest_sha256, audit_sequence, created_at
                 ) VALUES (?1, ?2, ?3, 'engineer_user', 'tendering_manager', ?4, ?5, ?6, ?7, ?8)",
                params![
                    command.assessment_id,
                    command.classification.as_str(),
                    command.rationale,
                    lifecycle_after.as_str(),
                    manifest_json,
                    manifest_sha256,
                    audit_sequence,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        if command.classification == ChangeAssessmentClassification::Material {
            if impact_count == 0 {
                let resolution_audit = append_audit_event_with_sequence(
                    &transaction,
                    tender_id.as_str(),
                    "change_assessment_resolved",
                    revision,
                    json!({
                        "assessment_id": command.assessment_id,
                        "resolution": "source_precedence",
                    }),
                    &created_at,
                )?;
                transaction
                    .execute(
                        "INSERT INTO change_assessment_resolutions (
                           assessment_id, resolution, audit_sequence, created_at
                         ) VALUES (?1, 'source_precedence', ?2, ?3)",
                        params![command.assessment_id, resolution_audit, created_at],
                    )
                    .map_err(sql_error)?;
            } else if package_reentry {
                let approval_impact_exists = transaction
                    .query_row(
                        "SELECT EXISTS(
                           SELECT 1 FROM change_assessment_impacts
                           WHERE assessment_id = ?1 AND kind = 'approval'
                         )",
                        [&command.assessment_id],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(sql_error)?;
                let approval_invalidated = invalidate_accepted_bid_decision_for_change_assessment(
                    &transaction,
                    tender_id,
                    &command.assessment_id,
                    lifecycle_before,
                    &command.rationale,
                    revision,
                    &created_at,
                )?;
                if approval_impact_exists && !approval_invalidated {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                transaction
                    .execute(
                        "UPDATE production_tasks
                         SET status = 'suspended', updated_at = ?2
                         WHERE production_task_id IN (
                           SELECT object_id FROM change_assessment_impacts
                           WHERE assessment_id = ?1 AND kind = 'production_task'
                         ) AND status = 'ready_for_integration'",
                        params![command.assessment_id, created_at],
                    )
                    .map_err(sql_error)?;
            } else {
                transaction
                    .execute(
                        "UPDATE production_tasks
                         SET status = CASE
                             WHEN status IN ('ready_for_integration', 'review_ready')
                               THEN 'remediation_ready'
                             ELSE status END,
                             updated_at = ?2
                         WHERE production_task_id IN (
                           SELECT object_id FROM change_assessment_impacts
                           WHERE assessment_id = ?1 AND kind = 'production_task'
                         )",
                        params![command.assessment_id, created_at],
                    )
                    .map_err(sql_error)?;
            }
        } else {
            let resolution_audit = append_audit_event_with_sequence(
                &transaction,
                tender_id.as_str(),
                "change_assessment_resolved",
                revision,
                json!({
                    "assessment_id": command.assessment_id,
                    "resolution": "irrelevant",
                }),
                &created_at,
            )?;
            transaction
                .execute(
                    "INSERT INTO change_assessment_resolutions (
                       assessment_id, resolution, audit_sequence, created_at
                     ) VALUES (?1, 'irrelevant', ?2, ?3)",
                    params![command.assessment_id, resolution_audit, created_at],
                )
                .map_err(sql_error)?;
        }
        transaction
            .execute(
                "UPDATE tender SET lifecycle_phase = ?1 WHERE singleton = 1",
                [lifecycle_after.as_str()],
            )
            .map_err(sql_error)?;
        transaction.commit().map_err(sql_error)?;
        self.load_change_assessment(&command.assessment_id, budget)
    }

    fn inspect_change_assessments(
        &self,
        before_sequence: Option<u32>,
        limit: u32,
        budget: BidPackageOperationBudget,
    ) -> Result<ChangeAssessmentPage, TenderCommandError> {
        budget.check()?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT assessment_sequence, assessment_id FROM change_assessments
                 WHERE (?1 IS NULL OR assessment_sequence < ?1)
                 ORDER BY assessment_sequence DESC LIMIT ?2",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![before_sequence, limit.saturating_add(1)], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut ids = Vec::new();
        for row in rows {
            budget.check()?;
            ids.push(row.map_err(sql_error)?);
        }
        let limit = usize::try_from(limit)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let has_more = ids.len() > limit;
        ids.truncate(limit);
        let mut items = Vec::new();
        let mut response_bytes = 0usize;
        for (_, assessment_id) in &ids {
            let assessment = self.load_change_assessment(assessment_id, budget)?;
            response_bytes = response_bytes
                .checked_add(canonical_json(&assessment)?.len())
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
            if response_bytes > MAX_CHANGE_PAGE_BYTES {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            items.push(assessment);
        }
        let active = self
            .connection
            .query_row(
                "SELECT assessments.assessment_id
                 FROM change_assessments AS assessments
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 WHERE resolutions.assessment_id IS NULL
                 ORDER BY assessments.assessment_sequence DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(sql_error)?
            .map(|assessment_id| self.load_change_assessment(&assessment_id, budget))
            .transpose()?;
        Ok(ChangeAssessmentPage {
            active,
            next_before_sequence: has_more.then(|| ids.last().map(|item| item.0)).flatten(),
            items,
        })
    }

    fn load_change_assessment(
        &self,
        assessment_id: &str,
        budget: BidPackageOperationBudget,
    ) -> Result<ChangeAssessment, TenderCommandError> {
        budget.check()?;
        type AssessmentRow = (
            u32,
            String,
            String,
            Option<String>,
            Option<u32>,
            Option<String>,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        );
        let row: AssessmentRow = self
            .connection
            .query_row(
                "SELECT assessment_sequence, relationship_id, lifecycle_before,
                        baseline_id, baseline_version, baseline_manifest_sha256,
                        affected_commitments_json, proposed_rework_json,
                        unchanged_scope_json, deadline_effect, approval_consequences_json,
                        manifest_json, manifest_sha256, created_at
                 FROM change_assessments WHERE assessment_id = ?1",
                [assessment_id],
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
                    ))
                },
            )
            .map_err(sql_error)?;
        let (relationship_kind, prior_source, replacement_source) =
            load_relationship_sources(&self.connection, &row.1)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT kind, object_id, object_version, dependencies_json, consequence, summary
                 FROM change_assessment_impacts WHERE assessment_id = ?1
                 ORDER BY impact_sequence",
            )
            .map_err(sql_error)?;
        let impact_rows = statement
            .query_map([assessment_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(sql_error)?;
        let mut impacts = Vec::new();
        for (index, impact) in impact_rows.enumerate() {
            budget.check()?;
            let impact = impact.map_err(sql_error)?;
            let expected_sequence = u32::try_from(index + 1)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            let stored_sequence: u32 = self
                .connection
                .query_row(
                    "SELECT impact_sequence FROM change_assessment_impacts
                     WHERE assessment_id = ?1 AND kind = ?2 AND object_id = ?3
                       AND object_version = ?4",
                    params![assessment_id, impact.0, impact.1, impact.2],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if stored_sequence != expected_sequence {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let dependencies: Vec<ChangeAssessmentDependencyReference> =
                parse_canonical(&impact.3)?;
            if dependencies.is_empty()
                || dependencies.len() > MAX_CHANGE_DEPENDENCIES_PER_IMPACT
                || dependencies.windows(2).any(|items| items[0] >= items[1])
                || dependencies.iter().any(|dependency| {
                    dependency.object_id.is_empty() || dependency.object_id.len() > 200
                })
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            impacts.push(ChangeAssessmentImpact {
                kind: ChangeAssessmentImpactKind::parse(&impact.0)?,
                object_id: impact.1,
                object_version: impact.2,
                dependencies,
                consequence: ChangeAssessmentImpactConsequence::parse(&impact.4)?,
                summary: impact.5,
            });
        }
        if impacts.len() > MAX_CHANGE_IMPACTS {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let dependency_count = impacts.iter().try_fold(0usize, |count, impact| {
            count
                .checked_add(impact.dependencies.len())
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
        })?;
        if dependency_count > MAX_CHANGE_DEPENDENCY_EDGES {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let affected_commitments: Vec<String> = parse_canonical(&row.6)?;
        let proposed_rework: Vec<String> = parse_canonical(&row.7)?;
        let unchanged_scope: Vec<String> = parse_canonical(&row.8)?;
        let approval_consequences: Vec<ChangeAssessmentApprovalConsequence> =
            parse_canonical(&row.10)?;
        let manifest = ChangeAssessmentManifest {
            schema_version: 1,
            assessment_id: assessment_id.to_owned(),
            relationship_id: row.1.clone(),
            relationship_kind,
            prior_source: prior_source.clone(),
            replacement_source: replacement_source.clone(),
            lifecycle_before: TenderLifecyclePhase::parse(&row.2)?,
            baseline_id: row.3.clone(),
            baseline_version: row.4,
            baseline_manifest_sha256: row.5.clone(),
            impacts: impacts.clone(),
            affected_commitments: affected_commitments.clone(),
            proposed_rework: proposed_rework.clone(),
            unchanged_scope: unchanged_scope.clone(),
            deadline_effect: row.9.clone(),
            approval_consequences: approval_consequences.clone(),
            created_at: row.13.clone(),
        };
        if canonical_json(&manifest)? != row.11 || sha256_hex(row.11.as_bytes()) != row.12 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let assessment_audit_count: u32 = self
            .connection
            .query_row(
                "SELECT COUNT(*)
                 FROM change_assessments AS assessments
                 JOIN audit_events AS audit ON audit.sequence = assessments.audit_sequence
                 WHERE assessments.assessment_id = ?1
                   AND audit.event_type = 'change_assessment_opened'
                   AND audit.created_at = assessments.created_at
                   AND json_extract(audit.payload_json, '$.change.assessment_id') = assessments.assessment_id
                   AND json_extract(audit.payload_json, '$.change.relationship_id') = assessments.relationship_id
                   AND json_extract(audit.payload_json, '$.change.manifest_sha256') = assessments.manifest_sha256
                   AND json_extract(audit.payload_json, '$.change.impact_count') =
                       CAST((SELECT COUNT(*) FROM change_assessment_impacts AS impacts
                             WHERE impacts.assessment_id = assessments.assessment_id) AS TEXT)
                   AND json_extract(audit.payload_json, '$.change.lifecycle_before') = assessments.lifecycle_before
                   AND json_extract(audit.payload_json, '$.change.lifecycle_after') = 'change_assessment'",
                [assessment_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if assessment_audit_count != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let decision = self
            .connection
            .query_row(
                "SELECT classification, rationale, decided_by, acting_role, lifecycle_after,
                        manifest_json, manifest_sha256, audit_sequence, created_at
                 FROM change_assessment_decisions WHERE assessment_id = ?1",
                [assessment_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(sql_error)?
            .map(|decision| {
                let classification = ChangeAssessmentClassification::parse(&decision.0)?;
                let lifecycle_after = TenderLifecyclePhase::parse(&decision.4)?;
                let expected = ChangeAssessmentDecisionManifest {
                    schema_version: 1,
                    assessment_id: assessment_id.to_owned(),
                    assessment_manifest_sha256: row.12.clone(),
                    classification,
                    rationale: decision.1.clone(),
                    decided_by: decision.2.clone(),
                    acting_role: decision.3.clone(),
                    lifecycle_before: TenderLifecyclePhase::ChangeAssessment,
                    lifecycle_after,
                    created_at: decision.8.clone(),
                };
                if canonical_json(&expected)? != decision.5
                    || sha256_hex(decision.5.as_bytes()) != decision.6
                    || decision.2 != "engineer_user"
                    || decision.3 != "tendering_manager"
                    || decision.1.trim().is_empty()
                    || decision.1.len() > 4_000
                    || decision.4
                        != if classification == ChangeAssessmentClassification::Material
                            && impacts
                                .iter()
                                .any(|impact| impact.kind == ChangeAssessmentImpactKind::Package)
                        {
                            TenderLifecyclePhase::BidDecision.as_str()
                        } else {
                            row.2.as_str()
                        }
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                let decision_audit_count: u32 = self
                    .connection
                    .query_row(
                        "SELECT COUNT(*) FROM audit_events
                         WHERE sequence = ?1 AND event_type = 'change_assessment_decided'
                           AND created_at = ?2
                           AND json_extract(payload_json, '$.change.assessment_id') = ?3
                           AND json_extract(payload_json, '$.change.assessment_manifest_sha256') = ?4
                           AND json_extract(payload_json, '$.change.classification') = ?5
                           AND json_extract(payload_json, '$.change.lifecycle_after') = ?6
                           AND json_extract(payload_json, '$.change.manifest_sha256') = ?7
                           AND json_extract(payload_json, '$.change.rationale') = ?8",
                        params![
                            decision.7,
                            decision.8,
                            assessment_id,
                            row.12,
                            decision.0,
                            decision.4,
                            decision.6,
                            decision.1,
                        ],
                        |row| row.get(0),
                    )
                    .map_err(sql_error)?;
                if decision_audit_count != 1 {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Ok(ChangeAssessmentDecision {
                    classification,
                    rationale: decision.1,
                    decided_by: decision.2,
                    acting_role: decision.3,
                    lifecycle_after,
                    manifest_sha256: decision.6,
                    created_at: decision.8,
                })
            })
            .transpose()?;
        type ResolutionRow = (
            String,
            Option<String>,
            Option<u32>,
            Option<String>,
            i64,
            String,
        );
        let resolution: Option<ResolutionRow> = self
            .connection
            .query_row(
                "SELECT resolution, baseline_id, baseline_version,
                        baseline_manifest_sha256, audit_sequence, created_at
                 FROM change_assessment_resolutions WHERE assessment_id = ?1",
                [assessment_id],
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
        if let Some(resolution) = &resolution {
            let resolution_valid = match resolution.0.as_str() {
                "irrelevant" => {
                    decision.as_ref().is_some_and(|decision| {
                        decision.classification == ChangeAssessmentClassification::Irrelevant
                    }) && resolution.1.is_none()
                        && resolution.2.is_none()
                        && resolution.3.is_none()
                }
                "source_precedence" => {
                    decision.as_ref().is_some_and(|decision| {
                        decision.classification == ChangeAssessmentClassification::Material
                    }) && impacts.is_empty()
                        && resolution.1.is_none()
                        && resolution.2.is_none()
                        && resolution.3.is_none()
                }
                "successor_baseline" => {
                    decision.as_ref().is_some_and(|decision| {
                        decision.classification == ChangeAssessmentClassification::Material
                    }) && match (&resolution.1, resolution.2, &resolution.3) {
                        (Some(baseline_id), Some(version), Some(manifest_sha256)) => self
                            .connection
                            .query_row(
                                "SELECT EXISTS(
                                   SELECT 1 FROM coordinated_bid_baseline_versions
                                   WHERE baseline_id = ?1 AND version = ?2
                                     AND manifest_sha256 = ?3
                                 )",
                                params![baseline_id, version, manifest_sha256],
                                |row| row.get::<_, bool>(0),
                            )
                            .map_err(sql_error)?,
                        _ => false,
                    }
                }
                _ => false,
            };
            if !resolution_valid {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let resolution_audit_count: u32 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events
                     WHERE sequence = ?1 AND event_type = 'change_assessment_resolved'
                       AND created_at = ?2
                       AND json_extract(payload_json, '$.change.assessment_id') = ?3
                       AND json_extract(payload_json, '$.change.resolution') = ?4
                       AND (?5 IS NULL OR json_extract(payload_json, '$.change.baseline_id') = ?5)
                       AND (?6 IS NULL OR json_extract(payload_json, '$.change.baseline_version') = CAST(?6 AS TEXT))
                       AND (?7 IS NULL OR json_extract(payload_json, '$.change.baseline_manifest_sha256') = ?7)",
                    params![
                        resolution.4,
                        resolution.5,
                        assessment_id,
                        resolution.0,
                        resolution.1,
                        resolution.2,
                        resolution.3,
                    ],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if resolution_audit_count != 1 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
        }
        let status = match (&decision, &resolution) {
            (None, None) => ChangeAssessmentStatus::Pending,
            (Some(decision), None)
                if decision.classification == ChangeAssessmentClassification::Material =>
            {
                ChangeAssessmentStatus::ReworkRequired
            }
            (Some(_), Some(_)) => ChangeAssessmentStatus::Resolved,
            _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        };
        Ok(ChangeAssessment {
            assessment_sequence: row.0,
            assessment_id: assessment_id.to_owned(),
            relationship_id: row.1,
            relationship_kind,
            prior_source,
            replacement_source,
            lifecycle_before: TenderLifecyclePhase::parse(&row.2)?,
            status,
            baseline_id: row.3,
            baseline_version: row.4,
            baseline_manifest_sha256: row.5,
            impacts,
            affected_commitments,
            proposed_rework,
            unchanged_scope,
            deadline_effect: row.9,
            approval_consequences,
            decision,
            resolution_baseline_id: resolution.as_ref().and_then(|value| value.1.clone()),
            resolution_baseline_version: resolution.and_then(|value| value.2),
            manifest_sha256: row.12,
            created_at: row.13,
        })
    }

    fn record_change_assessment_denial(
        &mut self,
        tender_id: &TenderId,
        reason: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        append_audit_event_with_sequence(
            &transaction,
            tender_id.as_str(),
            "change_assessment_denied",
            revision,
            json!({"reason": reason}),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    pub(crate) fn change_assessment_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        let (
            relationship_count,
            assessment_count,
            impact_count,
            decision_count,
            resolution_count,
        ): (
            u32,
            u32,
            u32,
            u32,
            u32,
        ) = self
            .connection
            .query_row(
                "SELECT (SELECT COUNT(*) FROM source_relationships),
                        (SELECT COUNT(*) FROM change_assessments),
                        (SELECT COUNT(*) FROM change_assessment_impacts),
                        (SELECT COUNT(*) FROM change_assessment_decisions),
                        (SELECT COUNT(*) FROM change_assessment_resolutions)",
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
        if relationship_count != assessment_count
            || assessment_count > MAX_CHANGE_ASSESSMENTS
            || impact_count > assessment_count.saturating_mul(MAX_CHANGE_IMPACTS as u32)
            || decision_count > assessment_count
            || resolution_count > decision_count
        {
            return Ok(false);
        }
        let maximum_sequence: Option<u32> = self
            .connection
            .query_row(
                "SELECT MAX(assessment_sequence) FROM change_assessments",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if (assessment_count == 0 && maximum_sequence.is_some())
            || (assessment_count > 0 && maximum_sequence != Some(assessment_count))
        {
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
        let mut statement = self
            .connection
            .prepare("SELECT assessment_id FROM change_assessments ORDER BY assessment_sequence")
            .map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(sql_error)?;
        let mut unresolved = Vec::new();
        for assessment_id in rows {
            check()?;
            let assessment_id = assessment_id.map_err(sql_error)?;
            let assessment = match self.load_change_assessment(&assessment_id, budget) {
                Ok(assessment) => assessment,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error)
                }
                Err(_) => return Ok(false),
            };
            if assessment.status != ChangeAssessmentStatus::Resolved {
                unresolved.push(assessment);
            }
        }
        if unresolved.len() > 1 {
            return Ok(false);
        }
        let lifecycle = TenderLifecyclePhase::parse(
            &self
                .connection
                .query_row(
                    "SELECT lifecycle_phase FROM tender WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sql_error)?,
        )?;
        if assessment_count == 0 && lifecycle == TenderLifecyclePhase::ChangeAssessment {
            return Ok(false);
        }
        if let Some(assessment) = unresolved.first() {
            match assessment.status {
                ChangeAssessmentStatus::Pending
                    if lifecycle != TenderLifecyclePhase::ChangeAssessment =>
                {
                    return Ok(false)
                }
                ChangeAssessmentStatus::ReworkRequired
                    if !matches!(
                        lifecycle,
                        TenderLifecyclePhase::BidDecision
                            | TenderLifecyclePhase::TenderPlanning
                            | TenderLifecyclePhase::ActiveProduction
                            | TenderLifecyclePhase::IntegratedReview
                    ) =>
                {
                    return Ok(false)
                }
                _ => {}
            }
        }
        Ok(true)
    }

    pub(crate) fn unresolved_change_assessment(
        &self,
    ) -> Result<Option<(String, ChangeAssessmentStatus)>, TenderCommandError> {
        let row: Option<(String, Option<String>)> = self
            .connection
            .query_row(
                "SELECT assessments.assessment_id, decisions.classification
                 FROM change_assessments AS assessments
                 LEFT JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 WHERE resolutions.assessment_id IS NULL
                 ORDER BY assessments.assessment_sequence DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        row.map(|(id, decision)| {
            Ok((
                id,
                if decision.as_deref() == Some("material") {
                    ChangeAssessmentStatus::ReworkRequired
                } else {
                    ChangeAssessmentStatus::Pending
                },
            ))
        })
        .transpose()
    }

    pub(crate) fn active_change_assessment_inputs_for_task(
        transaction: &Transaction<'_>,
        production_task_id: &str,
    ) -> Result<(Vec<AgentTaskInputReference>, Option<Value>), TenderCommandError> {
        let assessment: Option<(String, String, u32)> = transaction
            .query_row(
                "SELECT assessments.assessment_id, relationships.replacement_artifact_id,
                        relationships.replacement_version
                 FROM change_assessments AS assessments
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 JOIN source_relationships AS relationships USING (relationship_id)
                 JOIN change_assessment_impacts AS impacts USING (assessment_id)
                 JOIN production_tasks AS impacted_task
                   ON impacted_task.production_task_id = impacts.object_id
                 JOIN production_tasks AS current_task
                   ON current_task.production_task_id = ?1
                  AND current_task.task_key = impacted_task.task_key
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 WHERE decisions.classification = 'material'
                   AND resolutions.assessment_id IS NULL
                   AND impacts.kind = 'production_task'
                 LIMIT 1",
                [production_task_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        let Some((assessment_id, artifact_id, version)) = assessment else {
            return Ok((Vec::new(), None));
        };
        let mut inputs = vec![AgentTaskInputReference {
            kind: "change_assessment".into(),
            reference: assessment_id.clone(),
            version: 1,
        }];
        let mut statement = transaction
            .prepare(
                "SELECT ordinal FROM evidence_locations
                 WHERE artifact_id = ?1 AND version = ?2 ORDER BY ordinal LIMIT 256",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![artifact_id, version], |row| row.get::<_, u32>(0))
            .map_err(sql_error)?;
        for ordinal in rows {
            inputs.push(AgentTaskInputReference {
                kind: "source_evidence".into(),
                reference: format!("{artifact_id}#{}", ordinal.map_err(sql_error)?),
                version,
            });
        }
        Ok((
            inputs,
            Some(json!({
                "assessment_id": assessment_id,
                "replacement_artifact_id": artifact_id,
                "replacement_artifact_version": version,
            })),
        ))
    }

    pub(crate) fn task_has_active_change_rework(
        connection: &rusqlite::Connection,
        production_task_id: &str,
    ) -> Result<bool, TenderCommandError> {
        connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessment_impacts AS impacts
                   JOIN change_assessment_decisions AS decisions USING (assessment_id)
                   JOIN production_tasks AS impacted_task
                     ON impacted_task.production_task_id = impacts.object_id
                   JOIN production_tasks AS current_task
                     ON current_task.production_task_id = ?1
                    AND current_task.task_key = impacted_task.task_key
                   LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                   WHERE impacts.kind = 'production_task'
                     AND decisions.classification = 'material'
                     AND resolutions.assessment_id IS NULL
                 )",
                [production_task_id],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn active_change_allows_record_extraction(
        &self,
        evidence: &[TenderEvidenceReference],
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        if evidence.is_empty() {
            return Ok(false);
        }
        let target: Option<(String, u32)> = self
            .connection
            .query_row(
                "SELECT relationships.replacement_artifact_id,
                        relationships.replacement_version
                 FROM change_assessments AS assessments
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 JOIN source_relationships AS relationships USING (relationship_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 JOIN tender ON tender.singleton = 1
                 WHERE decisions.classification = 'material'
                   AND resolutions.assessment_id IS NULL
                   AND tender.lifecycle_phase IN (
                     'bid_decision', 'tender_planning', 'active_production'
                   )
                 LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        Ok(target.is_some_and(|(artifact_id, version)| {
            evidence
                .iter()
                .all(|item| item.artifact_id == artifact_id && item.version == version)
        }))
    }

    pub(crate) fn active_change_allows_record_governance(
        &self,
        record_id: &str,
    ) -> Result<bool, TenderCommandError> {
        if !self.active_change_recovery_intake_is_open()? {
            return Ok(false);
        }
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessment_impacts AS impacts
                   JOIN change_assessment_decisions AS decisions USING (assessment_id)
                   LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                   JOIN tender ON tender.singleton = 1
                   WHERE impacts.kind = 'tender_record' AND impacts.object_id = ?1
                     AND decisions.classification = 'material'
                     AND resolutions.assessment_id IS NULL
                     AND tender.lifecycle_phase IN (
                       'bid_decision', 'tender_planning', 'active_production'
                     )
                 )",
                [record_id],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn active_change_record_candidate_keys_are_allowed(
        transaction: &Transaction<'_>,
        stable_keys: &[String],
    ) -> Result<bool, TenderCommandError> {
        let active_material: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessments AS assessments
                   JOIN change_assessment_decisions AS decisions USING (assessment_id)
                   LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                   WHERE decisions.classification = 'material'
                     AND resolutions.assessment_id IS NULL
                 )",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if !active_material {
            return Ok(true);
        }
        let recovery_open = active_material_change_recovery_is_open(transaction)?;
        if !recovery_open {
            return Ok(false);
        }
        let assessment: Option<String> = transaction
            .query_row(
                "SELECT assessments.assessment_id
                 FROM change_assessments AS assessments
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 JOIN tender ON tender.singleton = 1
                 WHERE decisions.classification = 'material'
                   AND resolutions.assessment_id IS NULL
                   AND tender.lifecycle_phase IN (
                     'bid_decision', 'tender_planning', 'active_production'
                   )
                 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let Some(assessment_id) = assessment else {
            return Ok(false);
        };
        if stable_keys.is_empty() {
            return Ok(false);
        }
        for stable_key in stable_keys {
            let allowed: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM change_assessment_impacts AS impacts
                       JOIN tender_records AS records ON records.record_id = impacts.object_id
                       WHERE impacts.assessment_id = ?1 AND impacts.kind = 'tender_record'
                         AND records.stable_key = ?2
                     )",
                    params![assessment_id, stable_key],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if !allowed {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn object_has_material_change_impact(
        &self,
        kind: ChangeAssessmentImpactKind,
        object_id: &str,
        object_version: u32,
    ) -> Result<bool, TenderCommandError> {
        self.connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessment_impacts AS impacts
                   JOIN change_assessment_decisions AS decisions USING (assessment_id)
                   WHERE impacts.kind = ?1 AND impacts.object_id = ?2
                     AND impacts.object_version = ?3
                     AND decisions.classification = 'material'
                 )",
                params![kind.as_str(), object_id, object_version],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    pub(crate) fn resolve_change_assessment_for_baseline(
        transaction: &Transaction<'_>,
        tender_id: &TenderId,
        revision: u32,
        baseline_id: &str,
        baseline_version: u32,
        baseline_manifest_sha256: &str,
        created_at: &str,
    ) -> Result<(), TenderCommandError> {
        let assessment: Option<(String, Option<u32>, String, String, u32)> = transaction
            .query_row(
                "SELECT assessments.assessment_id, assessments.baseline_version,
                        decisions.created_at, relationships.replacement_artifact_id,
                        relationships.replacement_version
                 FROM change_assessments AS assessments
                 JOIN change_assessment_decisions AS decisions USING (assessment_id)
                 JOIN source_relationships AS relationships USING (relationship_id)
                 LEFT JOIN change_assessment_resolutions AS resolutions USING (assessment_id)
                 WHERE decisions.classification = 'material'
                   AND resolutions.assessment_id IS NULL
                 ORDER BY assessments.assessment_sequence DESC LIMIT 1",
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
            .optional()
            .map_err(sql_error)?;
        let Some((
            assessment_id,
            prior_version,
            decision_created_at,
            replacement_artifact_id,
            replacement_version,
        )) = assessment
        else {
            return Ok(());
        };
        if prior_version.is_some_and(|prior| baseline_version <= prior) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let unresolved_tasks: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessment_impacts AS impacts
                   JOIN production_tasks AS prior_tasks
                     ON prior_tasks.production_task_id = impacts.object_id
                   LEFT JOIN production_activations AS active_activation
                     ON active_activation.status = 'active'
                   LEFT JOIN production_tasks AS tasks
                     ON tasks.activation_id = active_activation.activation_id
                    AND tasks.task_key = prior_tasks.task_key
                   WHERE impacts.assessment_id = ?1 AND impacts.kind = 'production_task'
                     AND (tasks.production_task_id IS NULL
                          OR tasks.status != 'ready_for_integration' OR NOT EXISTS(
                       SELECT 1
                       FROM production_task_attempts AS attempts
                       JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
                       JOIN tender_tasks AS task_views ON task_views.task_id = attempts.task_id
                       WHERE attempts.production_task_id = tasks.production_task_id
                         AND runs.status = 'completed' AND runs.started_at >= ?2
                         AND EXISTS(
                           SELECT 1 FROM json_each(task_views.exact_inputs_json) AS input
                           WHERE json_extract(input.value, '$.kind') = 'change_assessment'
                             AND json_extract(input.value, '$.reference') = ?1
                             AND json_extract(input.value, '$.version') = 1
                         )
                     ))
                 )",
                params![assessment_id, decision_created_at],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if unresolved_tasks {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut record_statement = transaction
            .prepare(
                "SELECT impacts.object_id, impacts.object_version, heads.current_version,
                        versions.fields_json, versions.contradictions_json
                 FROM change_assessment_impacts AS impacts
                 LEFT JOIN tender_record_heads AS heads ON heads.record_id = impacts.object_id
                 LEFT JOIN tender_record_versions AS versions
                   ON versions.record_id = heads.record_id AND versions.version = heads.current_version
                 WHERE impacts.assessment_id = ?1 AND impacts.kind = 'tender_record'
                 ORDER BY impacts.impact_sequence LIMIT 257",
            )
            .map_err(sql_error)?;
        let records = record_statement
            .query_map([&assessment_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, Option<u32>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(sql_error)?;
        let mut record_count = 0usize;
        for record in records {
            record_count += 1;
            if record_count > 256 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (
                record_id,
                prior_record_version,
                current_version,
                fields_json,
                contradictions_json,
            ) = record.map_err(sql_error)?;
            let Some(current_version) =
                current_version.filter(|value| *value > prior_record_version)
            else {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            };
            let replacement_bound = fields_json
                .as_deref()
                .map(|value| {
                    json_text_references_source(
                        value,
                        &replacement_artifact_id,
                        replacement_version,
                    )
                })
                .transpose()?
                .unwrap_or(false)
                || contradictions_json
                    .as_deref()
                    .map(|value| {
                        json_text_references_source(
                            value,
                            &replacement_artifact_id,
                            replacement_version,
                        )
                    })
                    .transpose()?
                    .unwrap_or(false);
            let admitted: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM tender_record_reviews
                       WHERE record_id = ?1 AND record_version = ?2
                         AND outcome IN ('verified', 'approved_assumption')
                     )",
                    params![record_id, current_version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if !replacement_bound || !admitted {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        drop(record_statement);
        let unresolved_queries: bool = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM change_assessment_impacts AS impacts
                   LEFT JOIN tender_query_heads AS heads ON heads.query_id = impacts.object_id
                   WHERE impacts.assessment_id = ?1 AND impacts.kind = 'tender_query'
                     AND (heads.current_version IS NULL
                          OR heads.current_version <= impacts.object_version)
                 )",
                [&assessment_id],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if unresolved_queries {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let baseline_manifest_json: String = transaction
            .query_row(
                "SELECT manifest_json FROM coordinated_bid_baseline_versions
                 WHERE baseline_id = ?1 AND version = ?2 AND manifest_sha256 = ?3",
                params![baseline_id, baseline_version, baseline_manifest_sha256],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let baseline_manifest: Value = parse_canonical(&baseline_manifest_json)?;
        let mut stale_statement = transaction
            .prepare(
                "SELECT object_id, object_version FROM change_assessment_impacts
                 WHERE assessment_id = ?1 AND kind IN (
                   'calculation_run', 'estimate', 'pricing_decision',
                   'production_artifact', 'tender_query'
                 ) ORDER BY impact_sequence LIMIT 1025",
            )
            .map_err(sql_error)?;
        let stale = stale_statement
            .query_map([&assessment_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?;
        let mut stale_count = 0usize;
        for stale in stale {
            stale_count += 1;
            if stale_count > 1_024 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (object_id, object_version) = stale.map_err(sql_error)?;
            if json_contains_object_version(&baseline_manifest, &object_id, object_version) {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let audit_sequence = append_audit_event_with_sequence(
            transaction,
            tender_id.as_str(),
            "change_assessment_resolved",
            revision,
            json!({
                "assessment_id": assessment_id,
                "baseline_id": baseline_id,
                "baseline_manifest_sha256": baseline_manifest_sha256,
                "baseline_version": baseline_version.to_string(),
                "resolution": "successor_baseline",
            }),
            created_at,
        )?;
        transaction
            .execute(
                "INSERT INTO change_assessment_resolutions (
                   assessment_id, resolution, baseline_id, baseline_version,
                   baseline_manifest_sha256, audit_sequence, created_at
                 ) VALUES (?1, 'successor_baseline', ?2, ?3, ?4, ?5, ?6)",
                params![
                    assessment_id,
                    baseline_id,
                    baseline_version,
                    baseline_manifest_sha256,
                    audit_sequence,
                    created_at,
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }
}

fn deny_change_assessment_transaction<T>(
    transaction: Transaction<'_>,
    tender_id: &TenderId,
    revision: u32,
    reason: &str,
) -> Result<T, TenderCommandError> {
    let created_at = sqlite_timestamp(&transaction)?;
    append_audit_event_with_sequence(
        &transaction,
        tender_id.as_str(),
        "change_assessment_denied",
        revision,
        json!({"reason": reason}),
        &created_at,
    )?;
    transaction.commit().map_err(sql_error)?;
    Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
}

fn load_relationship_sources(
    connection: &rusqlite::Connection,
    relationship_id: &str,
) -> Result<
    (
        SourceRelationshipKind,
        ChangeAssessmentSource,
        ChangeAssessmentSource,
    ),
    TenderCommandError,
> {
    type Row = (
        String,
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
    );
    let row: Row = connection
        .query_row(
            "SELECT relationships.relationship_kind,
                    relationships.prior_artifact_id, relationships.prior_version,
                    prior_artifacts.package_path, prior_versions.document_type, prior_versions.sha256,
                    relationships.replacement_artifact_id, relationships.replacement_version,
                    replacement_artifacts.package_path, replacement_versions.document_type,
                    replacement_versions.sha256
             FROM source_relationships AS relationships
             JOIN source_artifacts AS prior_artifacts
               ON prior_artifacts.artifact_id = relationships.prior_artifact_id
             JOIN source_artifact_versions AS prior_versions
               ON prior_versions.artifact_id = relationships.prior_artifact_id
              AND prior_versions.version = relationships.prior_version
             JOIN source_artifacts AS replacement_artifacts
               ON replacement_artifacts.artifact_id = relationships.replacement_artifact_id
             JOIN source_artifact_versions AS replacement_versions
               ON replacement_versions.artifact_id = relationships.replacement_artifact_id
              AND replacement_versions.version = relationships.replacement_version
             WHERE relationships.relationship_id = ?1",
            [relationship_id],
            |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                    row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                    row.get(10)?,
                ))
            },
        )
        .map_err(sql_error)?;
    let prior_evidence = load_change_source_evidence(connection, &row.1, row.2)?;
    let replacement_evidence = load_change_source_evidence(connection, &row.6, row.7)?;
    Ok((
        SourceRelationshipKind::parse(&row.0)?,
        ChangeAssessmentSource {
            artifact_id: row.1,
            version: row.2,
            package_path: row.3,
            document_type: row.4,
            sha256: row.5,
            evidence_count: prior_evidence.0,
            evidence_preview: prior_evidence.1,
        },
        ChangeAssessmentSource {
            artifact_id: row.6,
            version: row.7,
            package_path: row.8,
            document_type: row.9,
            sha256: row.10,
            evidence_count: replacement_evidence.0,
            evidence_preview: replacement_evidence.1,
        },
    ))
}

fn load_change_source_evidence(
    connection: &rusqlite::Connection,
    artifact_id: &str,
    version: u32,
) -> Result<(u32, Vec<ChangeAssessmentEvidenceExcerpt>), TenderCommandError> {
    let count = connection
        .query_row(
            "SELECT COUNT(*) FROM evidence_locations WHERE artifact_id = ?1 AND version = ?2",
            params![artifact_id, version],
            |row| row.get::<_, u32>(0),
        )
        .map_err(sql_error)?;
    let preview_limit = u32::try_from(MAX_CHANGE_SOURCE_EVIDENCE_PREVIEW)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let mut statement = connection
        .prepare(
            "SELECT ordinal, kind, structural_path, original_text, translated_text, language
             FROM evidence_locations WHERE artifact_id = ?1 AND version = ?2
             ORDER BY ordinal LIMIT ?3",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map(params![artifact_id, version, preview_limit], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(sql_error)?;
    let mut preview = Vec::new();
    for row in rows {
        let (ordinal, kind, structural_path, original_text, translated_text, language) =
            row.map_err(sql_error)?;
        let text_sha256 = sha256_hex(original_text.as_bytes());
        let (original_text, original_truncated) =
            bounded_utf8_excerpt(&original_text, MAX_CHANGE_SOURCE_EVIDENCE_EXCERPT_BYTES);
        let (translated_text, translated_truncated) = match translated_text {
            Some(value) => {
                let (value, truncated) =
                    bounded_utf8_excerpt(&value, MAX_CHANGE_SOURCE_EVIDENCE_EXCERPT_BYTES);
                (Some(value), truncated)
            }
            None => (None, false),
        };
        preview.push(ChangeAssessmentEvidenceExcerpt {
            ordinal,
            kind,
            structural_path,
            original_text,
            translated_text,
            language,
            text_sha256,
            truncated: original_truncated || translated_truncated,
        });
    }
    if preview.len() > MAX_CHANGE_SOURCE_EVIDENCE_PREVIEW
        || usize::try_from(count)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            < preview.len()
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok((count, preview))
}

fn bounded_utf8_excerpt(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_owned(), true)
}

fn derive_impacts(
    connection: &rusqlite::Connection,
    artifact_id: &str,
    version: u32,
    budget: BidPackageOperationBudget,
) -> Result<Vec<ChangeAssessmentImpact>, TenderCommandError> {
    let mut impacts = BTreeMap::<ImpactKey, ChangeAssessmentImpact>::new();
    let mut impacted_records = BTreeSet::new();
    let mut record_authors = BTreeSet::new();
    let mut statement = connection
        .prepare(
            "SELECT versions.record_id, versions.version, versions.kind, versions.title,
                    versions.fields_json, versions.contradictions_json, versions.author_run_id
             FROM tender_record_heads AS heads
             JOIN tender_record_versions AS versions
               ON versions.record_id = heads.record_id AND versions.version = heads.current_version
             ORDER BY versions.record_id LIMIT 257",
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
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(sql_error)?;
    let mut count = 0usize;
    for row in rows {
        budget.check()?;
        count += 1;
        if count > 256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let row = row.map_err(sql_error)?;
        if json_text_references_source(&row.4, artifact_id, version)?
            || json_text_references_source(&row.5, artifact_id, version)?
        {
            impacted_records.insert((row.0.clone(), row.1));
            record_authors.insert(row.6);
            add_impact(
                &mut impacts,
                ChangeAssessmentImpactKind::TenderRecord,
                row.0,
                row.1,
                dependency(
                    ChangeAssessmentObjectKind::SourceArtifact,
                    artifact_id,
                    version,
                    ChangeAssessmentDependencyKind::SourceEvidence,
                ),
                ChangeAssessmentImpactConsequence::Stale,
                format!(
                    "Current {} record '{}' cites the prior source.",
                    row.2, row.3
                ),
            )?;
        }
    }

    let mut impacted_packages = BTreeSet::new();
    let mut package_statement = connection
        .prepare(
            "SELECT heads.package_id, heads.current_version
             FROM bid_decision_package_heads AS heads ORDER BY heads.package_id LIMIT 33",
        )
        .map_err(sql_error)?;
    let packages = package_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(sql_error)?;
    for package in packages {
        budget.check()?;
        let (package_id, package_version) = package.map_err(sql_error)?;
        let mut bound_records = Vec::new();
        for (record_id, record_version) in &impacted_records {
            if connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM bid_compliance_rows
                       WHERE package_id = ?1 AND package_version = ?2
                         AND record_id = ?3 AND record_version = ?4
                       UNION ALL
                       SELECT 1 FROM bid_decision_package_record_bindings
                       WHERE package_id = ?1 AND package_version = ?2
                         AND record_id = ?3 AND record_version = ?4
                     )",
                    params![package_id, package_version, record_id, record_version],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?
            {
                bound_records.push((record_id.clone(), *record_version));
            }
        }
        if !bound_records.is_empty() {
            impacted_packages.insert((package_id.clone(), package_version));
            for (record_id, record_version) in bound_records {
                add_impact(
                    &mut impacts,
                    ChangeAssessmentImpactKind::Package,
                    package_id.clone(),
                    package_version,
                    dependency(
                        ChangeAssessmentObjectKind::TenderRecord,
                        record_id,
                        record_version,
                        ChangeAssessmentDependencyKind::RecordMembership,
                    ),
                    ChangeAssessmentImpactConsequence::Stale,
                    "The current Bid Decision Package binds an affected Tender Record.".into(),
                )?;
            }
        }
    }

    let mut impacted_tasks = BTreeSet::new();
    let mut task_statement = connection
        .prepare(
            "SELECT tasks.production_task_id, tasks.task_key, tasks.task_definition_json,
                    tasks.task_id
             FROM production_tasks AS tasks
             JOIN production_activations AS activations USING (activation_id)
             WHERE activations.status = 'active' ORDER BY tasks.production_task_id LIMIT 257",
        )
        .map_err(sql_error)?;
    let tasks = task_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(sql_error)?;
    let mut task_count = 0usize;
    for task in tasks {
        budget.check()?;
        task_count += 1;
        if task_count > 256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let task = task.map_err(sql_error)?;
        let definition: WorkPlanTask = parse_canonical(&task.2)?;
        let mut bound_records = Vec::new();
        for (record_id, record_version) in &impacted_records {
            if record_version_is_relevant_to_production_task(
                connection,
                record_id,
                *record_version,
                &definition,
            )? {
                bound_records.push((record_id.clone(), *record_version));
            }
        }
        let direct = json_text_references_source(&task.2, artifact_id, version)?;
        if !bound_records.is_empty() || direct {
            impacted_tasks.insert(task.0.clone());
            if direct {
                add_impact(
                    &mut impacts,
                    ChangeAssessmentImpactKind::ProductionTask,
                    task.0.clone(),
                    0,
                    dependency(
                        ChangeAssessmentObjectKind::SourceArtifact,
                        artifact_id,
                        version,
                        ChangeAssessmentDependencyKind::TaskInput,
                    ),
                    ChangeAssessmentImpactConsequence::Reopen,
                    format!(
                        "Production task '{}' consumes the affected exact package/input.",
                        task.1
                    ),
                )?;
            }
            for (record_id, record_version) in bound_records {
                add_impact(
                    &mut impacts,
                    ChangeAssessmentImpactKind::ProductionTask,
                    task.0.clone(),
                    0,
                    dependency(
                        ChangeAssessmentObjectKind::TenderRecord,
                        record_id,
                        record_version,
                        ChangeAssessmentDependencyKind::TaskInput,
                    ),
                    ChangeAssessmentImpactConsequence::Reopen,
                    format!(
                        "Production task '{}' consumes the affected exact package/input.",
                        task.1
                    ),
                )?;
            }
        }
    }
    let direct_tasks = impacted_tasks.iter().cloned().collect::<Vec<_>>();
    for production_task_id in direct_tasks {
        budget.check()?;
        let mut dependent_statement = connection
            .prepare(
                "WITH RECURSIVE affected(production_task_id, activation_id, task_key) AS (
                   SELECT production_task_id, activation_id, task_key
                   FROM production_tasks WHERE production_task_id = ?1
                   UNION
                   SELECT dependent.production_task_id, dependent.activation_id,
                          dependent.task_key
                   FROM production_tasks AS dependent
                   JOIN affected AS prerequisite
                     ON prerequisite.activation_id = dependent.activation_id
                   JOIN json_each(dependent.task_definition_json, '$.dependencies') AS dependency
                     ON dependency.value = prerequisite.task_key
                 )
                 SELECT production_task_id, task_key FROM affected
                 ORDER BY production_task_id LIMIT 257",
            )
            .map_err(sql_error)?;
        let dependents = dependent_statement
            .query_map([&production_task_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut dependent_count = 0usize;
        for dependent in dependents {
            budget.check()?;
            dependent_count += 1;
            if dependent_count > 256 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (dependent_id, task_key) = dependent.map_err(sql_error)?;
            if impacted_tasks.insert(dependent_id.clone()) {
                add_impact(
                    &mut impacts,
                    ChangeAssessmentImpactKind::ProductionTask,
                    dependent_id,
                    0,
                    dependency(
                        ChangeAssessmentObjectKind::ProductionTask,
                        production_task_id.clone(),
                        0,
                        ChangeAssessmentDependencyKind::TaskInput,
                    ),
                    ChangeAssessmentImpactConsequence::Reopen,
                    format!("Production task '{task_key}' transitively depends on affected work."),
                )?;
            }
        }
    }

    for run_id in record_authors {
        add_impact(
            &mut impacts,
            ChangeAssessmentImpactKind::AgentRun,
            run_id,
            0,
            dependency(
                ChangeAssessmentObjectKind::SourceArtifact,
                artifact_id,
                version,
                ChangeAssessmentDependencyKind::RunExecution,
            ),
            ChangeAssessmentImpactConsequence::Stale,
            "Agent Run authored an affected current Tender Record.".into(),
        )?;
    }
    for production_task_id in &impacted_tasks {
        add_production_task_dependents(connection, production_task_id, &mut impacts, budget)?;
    }
    add_query_impacts(
        connection,
        artifact_id,
        version,
        &impacted_records,
        &impacted_tasks,
        &mut impacts,
        budget,
    )?;
    let impacted_calculations =
        add_calculation_impacts(connection, artifact_id, version, &mut impacts, budget)?;
    add_baseline_and_approval_impacts(
        connection,
        &impacted_records,
        &impacted_packages,
        &impacted_tasks,
        &impacted_calculations,
        &mut impacts,
        budget,
    )?;
    if impacts.len() > MAX_CHANGE_IMPACTS {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    let dependency_count = impacts.values().try_fold(0usize, |count, impact| {
        count
            .checked_add(impact.dependencies.len())
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
    })?;
    if dependency_count > MAX_CHANGE_DEPENDENCY_EDGES {
        return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
    }
    Ok(impacts.into_values().collect())
}

fn add_production_task_dependents(
    connection: &rusqlite::Connection,
    production_task_id: &str,
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT attempts.task_id, attempts.attempt_number, runs.run_id
             FROM production_task_attempts AS attempts
             JOIN agent_runs AS runs ON runs.task_id = attempts.task_id
             WHERE attempts.production_task_id = ?1 ORDER BY attempts.attempt_number LIMIT 9",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([production_task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_error)?;
    for row in rows {
        budget.check()?;
        let (_, attempt, run_id) = row.map_err(sql_error)?;
        add_impact(
            impacts,
            ChangeAssessmentImpactKind::AgentRun,
            run_id,
            attempt,
            dependency(
                ChangeAssessmentObjectKind::ProductionTask,
                production_task_id,
                0,
                ChangeAssessmentDependencyKind::RunExecution,
            ),
            ChangeAssessmentImpactConsequence::Stale,
            "Agent Run executed an affected production task.".into(),
        )?;
    }
    let artifact: Option<(String, u32, String)> = connection
        .query_row(
            "SELECT artifact_id, version, payload_json
             FROM production_artifact_versions WHERE production_task_id = ?1
             ORDER BY version DESC LIMIT 1",
            [production_task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    if let Some((artifact_id, artifact_version, _)) = artifact {
        add_impact(
            impacts,
            ChangeAssessmentImpactKind::ProductionArtifact,
            artifact_id.clone(),
            artifact_version,
            dependency(
                ChangeAssessmentObjectKind::ProductionTask,
                production_task_id,
                0,
                ChangeAssessmentDependencyKind::ArtifactOutput,
            ),
            ChangeAssessmentImpactConsequence::Stale,
            "The current production Artifact was produced from an affected task input.".into(),
        )?;
        let review: Option<(String, String)> = connection
            .query_row(
                "SELECT review_id, reviewer_run_id FROM production_reviews
                 WHERE target_artifact_id = ?1 AND target_version = ?2",
                params![artifact_id, artifact_version],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some((review_id, reviewer_run_id)) = review {
            add_impact(
                impacts,
                ChangeAssessmentImpactKind::Review,
                review_id,
                artifact_version,
                dependency(
                    ChangeAssessmentObjectKind::ProductionArtifact,
                    artifact_id.clone(),
                    artifact_version,
                    ChangeAssessmentDependencyKind::ReviewTarget,
                ),
                ChangeAssessmentImpactConsequence::Stale,
                "Independent Review targets the affected Artifact version.".into(),
            )?;
            add_impact(
                impacts,
                ChangeAssessmentImpactKind::AgentRun,
                reviewer_run_id,
                artifact_version,
                dependency(
                    ChangeAssessmentObjectKind::ProductionArtifact,
                    artifact_id,
                    artifact_version,
                    ChangeAssessmentDependencyKind::RunExecution,
                ),
                ChangeAssessmentImpactConsequence::Stale,
                "Reviewer Agent Run evaluated the affected Artifact version.".into(),
            )?;
        }
    }
    Ok(())
}

fn add_query_impacts(
    connection: &rusqlite::Connection,
    artifact_id: &str,
    version: u32,
    impacted_records: &BTreeSet<(String, u32)>,
    impacted_tasks: &BTreeSet<String>,
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT versions.query_id, versions.version, versions.evidence_json,
                    versions.affected_records_json, versions.affected_task_keys_json,
                    versions.source_run_id
             FROM tender_query_heads AS heads
             JOIN tender_query_versions AS versions
               ON versions.query_id = heads.query_id AND versions.version = heads.current_version
             ORDER BY versions.query_id LIMIT 257",
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
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(sql_error)?;
    let mut count = 0usize;
    for row in rows {
        budget.check()?;
        count += 1;
        if count > 256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let row = row.map_err(sql_error)?;
        let records: Value = parse_canonical(&row.3)?;
        let tasks: Vec<String> = parse_canonical(&row.4)?;
        let mut query_dependencies = Vec::new();
        if json_text_references_source(&row.2, artifact_id, version)? {
            query_dependencies.push(dependency(
                ChangeAssessmentObjectKind::SourceArtifact,
                artifact_id,
                version,
                ChangeAssessmentDependencyKind::QueryEvidence,
            ));
        }
        for (record_id, record_version) in impacted_records {
            if json_contains_object_version(&records, record_id, *record_version) {
                query_dependencies.push(dependency(
                    ChangeAssessmentObjectKind::TenderRecord,
                    record_id,
                    *record_version,
                    ChangeAssessmentDependencyKind::QueryEvidence,
                ));
            }
        }
        for task_key in &tasks {
            for production_task_id in impacted_tasks {
                if connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM production_tasks
                         WHERE production_task_id = ?1 AND task_key = ?2)",
                        params![production_task_id, task_key],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(sql_error)?
                {
                    query_dependencies.push(dependency(
                        ChangeAssessmentObjectKind::ProductionTask,
                        production_task_id,
                        0,
                        ChangeAssessmentDependencyKind::QueryEvidence,
                    ));
                }
            }
        }
        query_dependencies.sort();
        query_dependencies.dedup();
        if !query_dependencies.is_empty() {
            for query_dependency in query_dependencies {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::TenderQuery,
                    row.0.clone(),
                    row.1,
                    query_dependency,
                    ChangeAssessmentImpactConsequence::Reopen,
                    "Current Tender Query is linked to affected evidence, records, or work.".into(),
                )?;
            }
            if let Some(run_id) = row.5 {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::AgentRun,
                    run_id,
                    row.1,
                    dependency(
                        ChangeAssessmentObjectKind::TenderQuery,
                        row.0.clone(),
                        row.1,
                        ChangeAssessmentDependencyKind::RunExecution,
                    ),
                    ChangeAssessmentImpactConsequence::Stale,
                    "Agent Run proposed an affected current Tender Query.".into(),
                )?;
            }
            let treatment_decision: Option<String> = connection
                .query_row(
                    "SELECT decision_id FROM tender_query_treatment_decisions
                     WHERE query_id = ?1 AND query_version = ?2",
                    params![row.0, row.1],
                    |row| row.get(0),
                )
                .optional()
                .map_err(sql_error)?;
            if let Some(decision_id) = treatment_decision {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::Approval,
                    decision_id,
                    row.1,
                    dependency(
                        ChangeAssessmentObjectKind::TenderQuery,
                        row.0.clone(),
                        row.1,
                        ChangeAssessmentDependencyKind::ApprovalTarget,
                    ),
                    ChangeAssessmentImpactConsequence::Revoke,
                    "Tender Query treatment decision is revoked with its affected Query version."
                        .into(),
                )?;
            }
            add_query_downstream_impacts(connection, &row.0, row.1, impacts, budget)?;
        }
    }
    Ok(())
}

fn add_query_downstream_impacts(
    connection: &rusqlite::Connection,
    query_id: &str,
    query_version: u32,
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut basis_statement = connection
        .prepare(
            "SELECT versions.basis_id, versions.version, versions.manifest_json
             FROM basis_of_estimate_heads AS heads
             JOIN basis_of_estimate_versions AS versions
               ON versions.basis_id = heads.basis_id AND versions.version = heads.current_version
             ORDER BY versions.basis_id LIMIT 33",
        )
        .map_err(sql_error)?;
    let basis_rows = basis_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(sql_error)?;
    let mut affected_bases = Vec::new();
    for row in basis_rows {
        budget.check()?;
        let (basis_id, basis_version, manifest_json) = row.map_err(sql_error)?;
        let manifest: Value = parse_canonical(&manifest_json)?;
        if !json_contains_object_version(&manifest, query_id, query_version) {
            continue;
        }
        add_impact(
            impacts,
            ChangeAssessmentImpactKind::Estimate,
            basis_id.clone(),
            basis_version,
            dependency(
                ChangeAssessmentObjectKind::TenderQuery,
                query_id,
                query_version,
                ChangeAssessmentDependencyKind::QueryEvidence,
            ),
            ChangeAssessmentImpactConsequence::Stale,
            "Basis of Estimate binds the affected Query treatment.".into(),
        )?;
        add_version_review_and_approval_impacts(
            connection,
            "basis_of_estimate_reviews",
            "basis_of_estimate_approvals",
            "basis_id",
            "basis_version",
            &basis_id,
            basis_version,
            ChangeAssessmentObjectKind::Estimate,
            impacts,
            budget,
        )?;
        affected_bases.push((basis_id, basis_version));
    }
    drop(basis_statement);

    for (basis_id, basis_version) in affected_bases {
        let mut baseline_statement = connection
            .prepare(
                "SELECT versions.baseline_id, versions.version
                 FROM priced_cost_baseline_heads AS heads
                 JOIN priced_cost_baseline_versions AS versions
                   ON versions.baseline_id = heads.baseline_id
                  AND versions.version = heads.current_version
                 WHERE versions.basis_id = ?1 AND versions.basis_version = ?2
                 ORDER BY versions.baseline_id LIMIT 33",
            )
            .map_err(sql_error)?;
        let baseline_rows = baseline_statement
            .query_map(params![basis_id, basis_version], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
            })
            .map_err(sql_error)?;
        let mut affected_baselines = Vec::new();
        for row in baseline_rows {
            budget.check()?;
            let (baseline_id, baseline_version) = row.map_err(sql_error)?;
            add_impact(
                impacts,
                ChangeAssessmentImpactKind::PricingDecision,
                baseline_id.clone(),
                baseline_version,
                dependency(
                    ChangeAssessmentObjectKind::Estimate,
                    basis_id.clone(),
                    basis_version,
                    ChangeAssessmentDependencyKind::CalculationInput,
                ),
                ChangeAssessmentImpactConsequence::Stale,
                "Priced Cost Baseline depends on the affected Basis of Estimate.".into(),
            )?;
            add_version_review_and_approval_impacts(
                connection,
                "priced_cost_baseline_reviews",
                "priced_cost_baseline_approvals",
                "baseline_id",
                "baseline_version",
                &baseline_id,
                baseline_version,
                ChangeAssessmentObjectKind::PricingDecision,
                impacts,
                budget,
            )?;
            affected_baselines.push((baseline_id, baseline_version));
        }
        drop(baseline_statement);
        for (baseline_id, baseline_version) in affected_baselines {
            add_pricing_chain_impacts(connection, &baseline_id, baseline_version, impacts, budget)?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_version_review_and_approval_impacts(
    connection: &rusqlite::Connection,
    review_table: &str,
    approval_table: &str,
    id_column: &str,
    version_column: &str,
    object_id: &str,
    object_version: u32,
    object_kind: ChangeAssessmentObjectKind,
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    budget.check()?;
    let review_sql = format!(
        "SELECT review_id FROM {review_table}
         WHERE {id_column} = ?1 AND {version_column} = ?2 LIMIT 2"
    );
    let reviews = connection
        .prepare(&review_sql)
        .and_then(|mut statement| {
            statement
                .query_map(params![object_id, object_version], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(sql_error)?;
    if reviews.len() > 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for review_id in reviews {
        add_impact(
            impacts,
            ChangeAssessmentImpactKind::Review,
            review_id,
            object_version,
            dependency(
                object_kind,
                object_id,
                object_version,
                ChangeAssessmentDependencyKind::ReviewTarget,
            ),
            ChangeAssessmentImpactConsequence::Stale,
            "Independent Review targets an affected exact version.".into(),
        )?;
    }
    let approval_sql = format!(
        "SELECT approval_id FROM {approval_table}
         WHERE {id_column} = ?1 AND {version_column} = ?2 LIMIT 2"
    );
    let approvals = connection
        .prepare(&approval_sql)
        .and_then(|mut statement| {
            statement
                .query_map(params![object_id, object_version], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(sql_error)?;
    if approvals.len() > 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for approval_id in approvals {
        add_impact(
            impacts,
            ChangeAssessmentImpactKind::Approval,
            approval_id,
            object_version,
            dependency(
                object_kind,
                object_id,
                object_version,
                ChangeAssessmentDependencyKind::ApprovalTarget,
            ),
            ChangeAssessmentImpactConsequence::Revoke,
            "Exact approval is revoked because its target is affected.".into(),
        )?;
    }
    Ok(())
}

fn add_pricing_chain_impacts(
    connection: &rusqlite::Connection,
    baseline_id: &str,
    baseline_version: u32,
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut adjustment_statement = connection
        .prepare(
            "SELECT versions.adjustment_id, versions.version
             FROM pricing_adjustment_heads AS heads
             JOIN pricing_adjustment_versions AS versions
               ON versions.adjustment_id = heads.adjustment_id
              AND versions.version = heads.current_version
             WHERE versions.baseline_id = ?1 AND versions.baseline_version = ?2
             ORDER BY versions.adjustment_id LIMIT 65",
        )
        .map_err(sql_error)?;
    let adjustments = adjustment_statement
        .query_map(params![baseline_id, baseline_version], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(sql_error)?;
    let mut adjustment_count = 0usize;
    for row in adjustments {
        budget.check()?;
        adjustment_count += 1;
        if adjustment_count > 64 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let (adjustment_id, adjustment_version) = row.map_err(sql_error)?;
        add_impact(
            impacts,
            ChangeAssessmentImpactKind::PricingDecision,
            adjustment_id.clone(),
            adjustment_version,
            dependency(
                ChangeAssessmentObjectKind::PricingDecision,
                baseline_id,
                baseline_version,
                ChangeAssessmentDependencyKind::CalculationInput,
            ),
            ChangeAssessmentImpactConsequence::Stale,
            "Pricing adjustment depends on the affected Priced Cost Baseline.".into(),
        )?;
        add_version_review_and_approval_impacts(
            connection,
            "pricing_adjustment_reviews",
            "pricing_adjustment_approvals",
            "adjustment_id",
            "adjustment_version",
            &adjustment_id,
            adjustment_version,
            ChangeAssessmentObjectKind::PricingDecision,
            impacts,
            budget,
        )?;
    }
    drop(adjustment_statement);

    let selected: Option<(String, u32, String, String, String)> = connection
        .query_row(
            "SELECT versions.pricing_scenario_id, versions.version, versions.strategy_id,
                    versions.pricing_calculation_run_id, selections.selection_id
             FROM pricing_selection_head AS head
             JOIN pricing_scenario_selections AS selections USING (selection_id)
             JOIN pricing_scenario_versions AS versions
               ON versions.pricing_scenario_id = selections.pricing_scenario_id
              AND versions.version = selections.pricing_scenario_version
             WHERE versions.baseline_id = ?1 AND versions.baseline_version = ?2",
            params![baseline_id, baseline_version],
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
    let Some((scenario_id, scenario_version, strategy_id, calculation_run_id, selection_id)) =
        selected
    else {
        return Ok(());
    };
    add_impact(
        impacts,
        ChangeAssessmentImpactKind::PricingDecision,
        strategy_id.clone(),
        1,
        dependency(
            ChangeAssessmentObjectKind::PricingDecision,
            baseline_id,
            baseline_version,
            ChangeAssessmentDependencyKind::BaselineBinding,
        ),
        ChangeAssessmentImpactConsequence::Stale,
        "Commercial Strategy depends on the affected Priced Cost Baseline.".into(),
    )?;
    let strategy_approval: Option<String> = connection
        .query_row(
            "SELECT approval_id FROM commercial_strategy_approvals WHERE strategy_id = ?1",
            [&strategy_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if let Some(approval_id) = strategy_approval {
        add_impact(
            impacts,
            ChangeAssessmentImpactKind::Approval,
            approval_id,
            1,
            dependency(
                ChangeAssessmentObjectKind::PricingDecision,
                strategy_id.clone(),
                1,
                ChangeAssessmentDependencyKind::ApprovalTarget,
            ),
            ChangeAssessmentImpactConsequence::Revoke,
            "Commercial Strategy approval is revoked with its affected target.".into(),
        )?;
    }
    add_impact(
        impacts,
        ChangeAssessmentImpactKind::PricingDecision,
        scenario_id.clone(),
        scenario_version,
        dependency(
            ChangeAssessmentObjectKind::PricingDecision,
            baseline_id,
            baseline_version,
            ChangeAssessmentDependencyKind::BaselineBinding,
        ),
        ChangeAssessmentImpactConsequence::Stale,
        "Selected Pricing Scenario depends on the affected Priced Cost Baseline.".into(),
    )?;
    add_impact(
        impacts,
        ChangeAssessmentImpactKind::CalculationRun,
        calculation_run_id,
        1,
        dependency(
            ChangeAssessmentObjectKind::PricingDecision,
            scenario_id.clone(),
            scenario_version,
            ChangeAssessmentDependencyKind::CalculationInput,
        ),
        ChangeAssessmentImpactConsequence::Reopen,
        "Final Price calculation depends on the affected Pricing Scenario.".into(),
    )?;
    add_impact(
        impacts,
        ChangeAssessmentImpactKind::Approval,
        selection_id.clone(),
        scenario_version,
        dependency(
            ChangeAssessmentObjectKind::PricingDecision,
            scenario_id.clone(),
            scenario_version,
            ChangeAssessmentDependencyKind::ApprovalTarget,
        ),
        ChangeAssessmentImpactConsequence::Revoke,
        "Pricing Scenario selection is revoked with its affected target.".into(),
    )?;
    let tender_price: Option<String> = connection
        .query_row(
            "SELECT approval_id FROM approved_tender_prices WHERE selection_id = ?1",
            [&selection_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if let Some(approval_id) = tender_price {
        add_impact(
            impacts,
            ChangeAssessmentImpactKind::Approval,
            approval_id,
            scenario_version,
            dependency(
                ChangeAssessmentObjectKind::PricingDecision,
                scenario_id,
                scenario_version,
                ChangeAssessmentDependencyKind::ApprovalTarget,
            ),
            ChangeAssessmentImpactConsequence::Revoke,
            "Approved Tender Price is revoked with its affected Pricing Scenario.".into(),
        )?;
    }
    Ok(())
}

fn add_calculation_impacts(
    connection: &rusqlite::Connection,
    artifact_id: &str,
    version: u32,
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    budget: BidPackageOperationBudget,
) -> Result<BTreeSet<String>, TenderCommandError> {
    let tables = [
        ("calculation_runs", "calculation_run_id"),
        ("estimate_aggregate_calculation_runs", "aggregate_run_id"),
        ("pricing_calculation_runs", "pricing_calculation_run_id"),
    ];
    let mut candidates = Vec::new();
    for (table, id_column) in tables {
        let query =
            format!("SELECT {id_column}, manifest_json FROM {table} ORDER BY created_at LIMIT 513");
        let mut statement = connection.prepare(&query).map_err(sql_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(sql_error)?;
        let mut count = 0usize;
        for row in rows {
            budget.check()?;
            count += 1;
            if count > 512 {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let (run_id, manifest_json) = row.map_err(sql_error)?;
            candidates.push((table, run_id, manifest_json));
        }
    }
    let mut affected: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut advanced = false;
        for (_, run_id, manifest_json) in &candidates {
            if affected.contains(run_id) {
                continue;
            }
            let direct = json_text_references_source(manifest_json, artifact_id, version)?;
            let mut transitive = false;
            for dependency in &affected {
                if json_text_contains_string(manifest_json, dependency)? {
                    transitive = true;
                    break;
                }
            }
            if direct || transitive {
                affected.insert(run_id.clone());
                advanced = true;
            }
        }
        if !advanced {
            break;
        }
    }
    for (table, run_id, manifest_json) in &candidates {
        if !affected.contains(run_id) {
            continue;
        }
        let mut dependencies = Vec::new();
        if json_text_references_source(manifest_json, artifact_id, version)? {
            dependencies.push(dependency(
                ChangeAssessmentObjectKind::SourceArtifact,
                artifact_id,
                version,
                ChangeAssessmentDependencyKind::CalculationInput,
            ));
        }
        for affected_run_id in &affected {
            if affected_run_id != run_id
                && json_text_contains_string(manifest_json, affected_run_id)?
            {
                dependencies.push(dependency(
                    ChangeAssessmentObjectKind::CalculationRun,
                    affected_run_id,
                    1,
                    ChangeAssessmentDependencyKind::CalculationInput,
                ));
            }
        }
        dependencies.sort();
        dependencies.dedup();
        if dependencies.is_empty() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for calculation_dependency in dependencies {
            add_impact(
                impacts,
                ChangeAssessmentImpactKind::CalculationRun,
                run_id.clone(),
                1,
                calculation_dependency,
                ChangeAssessmentImpactConsequence::Reopen,
                if *table == "calculation_runs" {
                    "Controlled Calculation Run consumes affected source Evidence.".into()
                } else {
                    "Controlled aggregate or pricing calculation transitively depends on an affected Calculation Run.".into()
                },
            )?;
        }
        add_calculation_decision_impacts(connection, table, run_id, impacts, budget)?;
    }
    Ok(affected)
}

fn add_calculation_decision_impacts(
    connection: &rusqlite::Connection,
    _table: &str,
    run_id: &str,
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT kind, object_id, object_version, summary FROM (
               SELECT 'approval' AS kind, approval_id AS object_id, 1 AS object_version,
                      'Calculation approval depends on the affected run.' AS summary
               FROM calculation_run_approvals WHERE calculation_run_id = ?1
               UNION ALL
               SELECT 'approval', approval_id, basis_version,
                      'Estimate aggregate approval depends on the affected run.'
               FROM estimate_aggregate_calculation_approvals WHERE aggregate_run_id = ?1
               UNION ALL
               SELECT 'estimate', versions.basis_id, versions.version,
                      'Basis of Estimate depends on the affected calculation.'
               FROM basis_of_estimate_versions AS versions
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'review', reviews.review_id, reviews.basis_version,
                      'Basis of Estimate review depends on the affected calculation.'
               FROM basis_of_estimate_reviews AS reviews
               JOIN basis_of_estimate_versions AS versions
                 ON versions.basis_id = reviews.basis_id
                AND versions.version = reviews.basis_version
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'approval', approvals.approval_id, approvals.basis_version,
                      'Basis of Estimate approval depends on the affected calculation.'
               FROM basis_of_estimate_approvals AS approvals
               JOIN basis_of_estimate_versions AS versions
                 ON versions.basis_id = approvals.basis_id
                AND versions.version = approvals.basis_version
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'pricing', versions.baseline_id, versions.version,
                      'Priced Cost Baseline depends on the affected calculation.'
               FROM priced_cost_baseline_versions AS versions
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'review', reviews.review_id, reviews.baseline_version,
                      'Priced Cost Baseline review depends on the affected calculation.'
               FROM priced_cost_baseline_reviews AS reviews
               JOIN priced_cost_baseline_versions AS versions
                 ON versions.baseline_id = reviews.baseline_id
                AND versions.version = reviews.baseline_version
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'approval', approvals.approval_id, approvals.baseline_version,
                      'Priced Cost Baseline approval depends on the affected calculation.'
               FROM priced_cost_baseline_approvals AS approvals
               JOIN priced_cost_baseline_versions AS versions
                 ON versions.baseline_id = approvals.baseline_id
                AND versions.version = approvals.baseline_version
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'pricing', versions.adjustment_id, versions.version,
                      'Pricing adjustment depends on the affected calculation.'
               FROM pricing_adjustment_versions AS versions
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'review', reviews.review_id, reviews.adjustment_version,
                      'Pricing adjustment review depends on the affected calculation.'
               FROM pricing_adjustment_reviews AS reviews
               JOIN pricing_adjustment_versions AS versions
                 ON versions.adjustment_id = reviews.adjustment_id
                AND versions.version = reviews.adjustment_version
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'approval', approvals.approval_id, approvals.adjustment_version,
                      'Pricing adjustment approval depends on the affected calculation.'
               FROM pricing_adjustment_approvals AS approvals
               JOIN pricing_adjustment_versions AS versions
                 ON versions.adjustment_id = approvals.adjustment_id
                AND versions.version = approvals.adjustment_version
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'pricing', strategies.strategy_id, 1,
                      'Commercial strategy depends on the affected calculation.'
               FROM commercial_strategies AS strategies
               WHERE EXISTS(SELECT 1 FROM json_tree(strategies.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'approval', approvals.approval_id, 1,
                      'Commercial strategy approval depends on the affected calculation.'
               FROM commercial_strategy_approvals AS approvals
               JOIN commercial_strategies AS strategies USING (strategy_id)
               WHERE EXISTS(SELECT 1 FROM json_tree(strategies.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'pricing', versions.pricing_scenario_id, versions.version,
                      'Pricing scenario depends on the affected calculation.'
               FROM pricing_scenario_versions AS versions
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'approval', selections.selection_id,
                      selections.pricing_scenario_version,
                      'Pricing scenario selection depends on the affected calculation.'
               FROM pricing_scenario_selections AS selections
               JOIN pricing_scenario_versions AS versions
                 ON versions.pricing_scenario_id = selections.pricing_scenario_id
                AND versions.version = selections.pricing_scenario_version
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
               UNION ALL
               SELECT 'approval', prices.approval_id, prices.pricing_scenario_version,
                      'Approved Tender Price depends on the affected calculation.'
               FROM approved_tender_prices AS prices
               JOIN pricing_scenario_versions AS versions
                 ON versions.pricing_scenario_id = prices.pricing_scenario_id
                AND versions.version = prices.pricing_scenario_version
               WHERE EXISTS(SELECT 1 FROM json_tree(versions.manifest_json) WHERE value = ?1)
             ) ORDER BY kind, object_id LIMIT 257",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(sql_error)?;
    let mut count = 0usize;
    for row in rows {
        budget.check()?;
        count += 1;
        if count > 256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let (kind, object_id, object_version, summary) = row.map_err(sql_error)?;
        add_impact(
            impacts,
            match kind.as_str() {
                "review" => ChangeAssessmentImpactKind::Review,
                "approval" => ChangeAssessmentImpactKind::Approval,
                "estimate" => ChangeAssessmentImpactKind::Estimate,
                "pricing" => ChangeAssessmentImpactKind::PricingDecision,
                _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
            },
            object_id,
            object_version,
            dependency(
                ChangeAssessmentObjectKind::CalculationRun,
                run_id,
                1,
                match kind.as_str() {
                    "review" => ChangeAssessmentDependencyKind::ReviewTarget,
                    "approval" => ChangeAssessmentDependencyKind::ApprovalTarget,
                    "estimate" | "pricing" => ChangeAssessmentDependencyKind::CalculationInput,
                    _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
                },
            ),
            if kind == "approval" {
                ChangeAssessmentImpactConsequence::Revoke
            } else {
                ChangeAssessmentImpactConsequence::Stale
            },
            summary,
        )?;
    }
    Ok(())
}

fn add_baseline_and_approval_impacts(
    connection: &rusqlite::Connection,
    impacted_records: &BTreeSet<(String, u32)>,
    impacted_packages: &BTreeSet<(String, u32)>,
    impacted_tasks: &BTreeSet<String>,
    impacted_calculations: &BTreeSet<String>,
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    budget: BidPackageOperationBudget,
) -> Result<(), TenderCommandError> {
    for (package_id, package_version) in impacted_packages {
        budget.check()?;
        let approval: Option<String> = connection
            .query_row(
                "SELECT approval_id FROM bid_decision_approval_records
                 WHERE package_id = ?1 AND package_version = ?2 AND decision = 'accept'",
                params![package_id, package_version],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some(approval_id) = approval {
            add_impact(
                impacts,
                ChangeAssessmentImpactKind::Approval,
                approval_id,
                *package_version,
                dependency(
                    ChangeAssessmentObjectKind::Package,
                    package_id,
                    *package_version,
                    ChangeAssessmentDependencyKind::ApprovalTarget,
                ),
                ChangeAssessmentImpactConsequence::Revoke,
                "Proceed approval is incompatible with the affected package basis.".into(),
            )?;
        }
        let plan: Option<(String, u32, Option<String>)> = connection
            .query_row(
                "SELECT heads.plan_id, heads.current_version, approvals.approval_id
                 FROM work_plan_heads AS heads
                 JOIN work_plan_versions AS versions
                   ON versions.plan_id = heads.plan_id AND versions.version = heads.current_version
                 LEFT JOIN work_plan_approvals AS approvals
                   ON approvals.plan_id = versions.plan_id AND approvals.plan_version = versions.version
                 WHERE versions.bid_package_id = ?1 AND versions.bid_package_version = ?2",
                params![package_id, package_version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?;
        if let Some((plan_id, plan_version, approval_id)) = plan {
            add_impact(
                impacts,
                ChangeAssessmentImpactKind::WorkPlan,
                plan_id.clone(),
                plan_version,
                dependency(
                    ChangeAssessmentObjectKind::Package,
                    package_id,
                    *package_version,
                    ChangeAssessmentDependencyKind::PackageBinding,
                ),
                ChangeAssessmentImpactConsequence::Stale,
                "The current Work Plan binds the affected Bid Decision Package.".into(),
            )?;
            if let Some(approval_id) = approval_id {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::Approval,
                    approval_id,
                    plan_version,
                    dependency(
                        ChangeAssessmentObjectKind::WorkPlan,
                        plan_id,
                        plan_version,
                        ChangeAssessmentDependencyKind::ApprovalTarget,
                    ),
                    ChangeAssessmentImpactConsequence::Revoke,
                    "Work Plan approval is bound to the affected package.".into(),
                )?;
            }
        }
    }
    for (record_id, record_version) in impacted_records {
        let reviews = connection
            .prepare(
                "SELECT review_id FROM tender_record_reviews
                 WHERE record_id = ?1 AND record_version = ?2 ORDER BY created_at LIMIT 33",
            )
            .and_then(|mut statement| {
                statement
                    .query_map(params![record_id, record_version], |row| {
                        row.get::<_, String>(0)
                    })?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(sql_error)?;
        for review_id in reviews {
            add_impact(
                impacts,
                ChangeAssessmentImpactKind::Review,
                review_id,
                *record_version,
                dependency(
                    ChangeAssessmentObjectKind::TenderRecord,
                    record_id,
                    *record_version,
                    ChangeAssessmentDependencyKind::ReviewTarget,
                ),
                ChangeAssessmentImpactConsequence::Stale,
                "Tender Record review targets affected source evidence.".into(),
            )?;
        }
    }
    let baseline: Option<(String, u32, Option<String>)> = connection
        .query_row(
            "SELECT head.baseline_id, head.current_version, approvals.approval_id
             FROM coordinated_bid_baseline_head AS head
             LEFT JOIN coordinated_bid_baseline_approvals AS approvals
               ON approvals.baseline_id = head.baseline_id
              AND approvals.baseline_version = head.current_version",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(sql_error)?;
    if let Some((baseline_id, baseline_version, approval)) = baseline {
        if !impacted_records.is_empty()
            || !impacted_tasks.is_empty()
            || !impacted_packages.is_empty()
            || !impacted_calculations.is_empty()
        {
            for (record_id, record_version) in impacted_records {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::CoordinatedBaseline,
                    baseline_id.clone(),
                    baseline_version,
                    dependency(
                        ChangeAssessmentObjectKind::TenderRecord,
                        record_id,
                        *record_version,
                        ChangeAssessmentDependencyKind::BaselineBinding,
                    ),
                    ChangeAssessmentImpactConsequence::Stale,
                    "Current Coordinated Bid Baseline binds affected work or records.".into(),
                )?;
            }
            for production_task_id in impacted_tasks {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::CoordinatedBaseline,
                    baseline_id.clone(),
                    baseline_version,
                    dependency(
                        ChangeAssessmentObjectKind::ProductionTask,
                        production_task_id,
                        0,
                        ChangeAssessmentDependencyKind::BaselineBinding,
                    ),
                    ChangeAssessmentImpactConsequence::Stale,
                    "Current Coordinated Bid Baseline binds affected work or records.".into(),
                )?;
            }
            for (package_id, package_version) in impacted_packages {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::CoordinatedBaseline,
                    baseline_id.clone(),
                    baseline_version,
                    dependency(
                        ChangeAssessmentObjectKind::Package,
                        package_id,
                        *package_version,
                        ChangeAssessmentDependencyKind::BaselineBinding,
                    ),
                    ChangeAssessmentImpactConsequence::Stale,
                    "Current Coordinated Bid Baseline binds affected work or records.".into(),
                )?;
            }
            for run_id in impacted_calculations {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::CoordinatedBaseline,
                    baseline_id.clone(),
                    baseline_version,
                    dependency(
                        ChangeAssessmentObjectKind::CalculationRun,
                        run_id,
                        1,
                        ChangeAssessmentDependencyKind::BaselineBinding,
                    ),
                    ChangeAssessmentImpactConsequence::Stale,
                    "Current Coordinated Bid Baseline binds affected work or records.".into(),
                )?;
            }
            if let Some(approval_id) = approval {
                add_impact(
                    impacts,
                    ChangeAssessmentImpactKind::Approval,
                    approval_id,
                    baseline_version,
                    dependency(
                        ChangeAssessmentObjectKind::CoordinatedBaseline,
                        baseline_id,
                        baseline_version,
                        ChangeAssessmentDependencyKind::ApprovalTarget,
                    ),
                    ChangeAssessmentImpactConsequence::Revoke,
                    "Baseline Approval is revoked until a successor baseline is approved.".into(),
                )?;
            }
        }
    }
    Ok(())
}

fn add_impact(
    impacts: &mut BTreeMap<ImpactKey, ChangeAssessmentImpact>,
    kind: ChangeAssessmentImpactKind,
    object_id: String,
    object_version: u32,
    dependency: ChangeAssessmentDependencyReference,
    consequence: ChangeAssessmentImpactConsequence,
    summary: String,
) -> Result<(), TenderCommandError> {
    if object_id.is_empty()
        || object_id.len() > 200
        || dependency.object_id.is_empty()
        || dependency.object_id.len() > 200
        || summary.trim().is_empty()
        || summary.len() > 500
    {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let impact = impacts
        .entry(ImpactKey(kind, object_id.clone(), object_version))
        .or_insert(ChangeAssessmentImpact {
            kind,
            object_id,
            object_version,
            dependencies: Vec::new(),
            consequence,
            summary,
        });
    if impact.consequence != consequence {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    match impact.dependencies.binary_search(&dependency) {
        Ok(_) => {}
        Err(index) => {
            if impact.dependencies.len() >= MAX_CHANGE_DEPENDENCIES_PER_IMPACT {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
            impact.dependencies.insert(index, dependency);
        }
    }
    Ok(())
}

fn dependency(
    kind: ChangeAssessmentObjectKind,
    object_id: impl Into<String>,
    object_version: u32,
    dependency_kind: ChangeAssessmentDependencyKind,
) -> ChangeAssessmentDependencyReference {
    ChangeAssessmentDependencyReference {
        kind,
        object_id: object_id.into(),
        object_version,
        dependency_kind,
    }
}

fn affected_commitments(impacts: &[ChangeAssessmentImpact]) -> Vec<String> {
    impacts
        .iter()
        .filter(|impact| {
            matches!(
                impact.kind,
                ChangeAssessmentImpactKind::TenderRecord
                    | ChangeAssessmentImpactKind::ProductionTask
                    | ChangeAssessmentImpactKind::CalculationRun
            )
        })
        .take(MAX_CHANGE_ITEMS)
        .map(|impact| impact.summary.clone())
        .collect()
}

fn proposed_rework(impacts: &[ChangeAssessmentImpact]) -> Vec<String> {
    let kinds = impacts
        .iter()
        .map(|impact| impact.kind)
        .collect::<HashSet<_>>();
    let mut result = Vec::new();
    if kinds.contains(&ChangeAssessmentImpactKind::TenderRecord) {
        result.push(
            "Re-extract and re-verify only Tender Records bound to the superseded source Evidence."
                .into(),
        );
    }
    if kinds.contains(&ChangeAssessmentImpactKind::TenderQuery) {
        result.push(
            "Reconcile affected Tender Queries and obtain exact Manager treatment decisions."
                .into(),
        );
    }
    if kinds.contains(&ChangeAssessmentImpactKind::CalculationRun) {
        result.push(
            "Re-run and approve only controlled calculations whose exact inputs changed.".into(),
        );
    }
    if kinds.contains(&ChangeAssessmentImpactKind::ProductionTask) {
        result.push(
            "Remediate and independently review only affected production task Artifacts.".into(),
        );
    }
    if kinds.contains(&ChangeAssessmentImpactKind::CoordinatedBaseline) {
        result.push("Assemble and approve an immutable successor Coordinated Bid Baseline.".into());
    }
    if result.is_empty() {
        result.push("No canonical dependency rework is proposed; classify the relationship as irrelevant after evidence review.".into());
    }
    result
}

fn unchanged_scope(
    connection: &rusqlite::Connection,
    impacts: &[ChangeAssessmentImpact],
) -> Result<Vec<String>, TenderCommandError> {
    let impacted_records = impacts
        .iter()
        .filter(|impact| impact.kind == ChangeAssessmentImpactKind::TenderRecord)
        .count();
    let impacted_tasks = impacts
        .iter()
        .filter(|impact| impact.kind == ChangeAssessmentImpactKind::ProductionTask)
        .count();
    let current_records: u32 = connection
        .query_row("SELECT COUNT(*) FROM tender_record_heads", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)?;
    let active_tasks: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM production_tasks AS tasks
             JOIN production_activations AS activations USING (activation_id)
             WHERE activations.status = 'active'",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    Ok(vec![
        format!(
            "{} current Tender Records have no typed dependency on the prior source.",
            usize::try_from(current_records)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
                .saturating_sub(impacted_records)
        ),
        format!(
            "{} active production tasks remain unchanged.",
            usize::try_from(active_tasks)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
                .saturating_sub(impacted_tasks)
        ),
    ])
}

fn json_text_references_source(
    text: &str,
    artifact_id: &str,
    version: u32,
) -> Result<bool, TenderCommandError> {
    let value: Value = parse_canonical(text)?;
    Ok(json_references_source(&value, artifact_id, version))
}

fn json_references_source(value: &Value, artifact_id: &str, version: u32) -> bool {
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| json_references_source(value, artifact_id, version)),
        Value::Object(values) => {
            let direct = values
                .get("artifact_id")
                .or_else(|| values.get("source_artifact_id"))
                .and_then(Value::as_str)
                == Some(artifact_id)
                && values
                    .get("version")
                    .or_else(|| values.get("source_artifact_version"))
                    .and_then(Value::as_u64)
                    == Some(u64::from(version));
            let input = values.get("kind").and_then(Value::as_str) == Some("source_evidence")
                && values.get("version").and_then(Value::as_u64) == Some(u64::from(version))
                && values
                    .get("reference")
                    .and_then(Value::as_str)
                    .is_some_and(|reference| {
                        reference == artifact_id
                            || reference.starts_with(&format!("{artifact_id}#"))
                    });
            direct
                || input
                || values
                    .values()
                    .any(|value| json_references_source(value, artifact_id, version))
        }
        _ => false,
    }
}

fn json_text_contains_string(text: &str, expected: &str) -> Result<bool, TenderCommandError> {
    let value: Value = parse_canonical(text)?;
    Ok(json_contains_string(&value, expected))
}

fn json_contains_string(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(value) => value == expected,
        Value::Array(values) => values
            .iter()
            .any(|value| json_contains_string(value, expected)),
        Value::Object(values) => values
            .values()
            .any(|value| json_contains_string(value, expected)),
        _ => false,
    }
}

fn json_contains_object_version(value: &Value, object_id: &str, version: u32) -> bool {
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| json_contains_object_version(value, object_id, version)),
        Value::Object(values) => {
            let exact_pair = values.iter().any(|(key, value)| {
                if value.as_str() != Some(object_id) {
                    return false;
                }
                let version_key = key
                    .strip_suffix("_id")
                    .map(|prefix| format!("{prefix}_version"));
                version_key
                    .as_ref()
                    .and_then(|key| values.get(key))
                    .or_else(|| values.get("version"))
                    .and_then(Value::as_u64)
                    == Some(u64::from(version))
            });
            exact_pair
                || values
                    .values()
                    .any(|value| json_contains_object_version(value, object_id, version))
        }
        _ => false,
    }
}

fn canonical_json(value: &impl Serialize) -> Result<String, TenderCommandError> {
    serde_json_canonicalizer::to_string(value)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn parse_canonical<T>(text: &str) -> Result<T, TenderCommandError>
where
    T: for<'de> Deserialize<'de> + Serialize,
{
    let value = serde_json::from_str(text)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if canonical_json(&value)? != text {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(value)
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
