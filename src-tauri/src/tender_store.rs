use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use garde::Validate;
use rusqlite::{
    limits::Limit, params, Connection, OpenFlags, OptionalExtension, Transaction,
    TransactionBehavior,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::document_parsing::{
    DocumentParseResult, EvidenceDocument, EvidenceLanguage, EvidenceLocation,
    EvidenceLocationKind, EvidenceRegion, EvidenceSearchHit, EvidenceSearchResult,
    EvidenceSemanticSearchHit, EvidenceSemanticSearchResult, ParseExceptionCode, ParseJob,
    ParseSourceArtifactCommand, ParseState, PreparedParseOutput, SearchEvidenceCommand,
    SearchEvidenceSemanticCommand, TextDirection, MAX_EVIDENCE_LOCATIONS, MAX_MARKDOWN_BYTES,
};
use crate::tender_intake::{
    prepare_package_with_control, ConfirmSourceRelationshipCommand, DocumentRegister,
    DocumentRegisterEntry, ImportTenderPackageCommand, IntakeExceptionCode, PackageIntakeControl,
    PreparedIntake, RegistrationState, SupersessionState, TenderPackageImportResult,
};
use crate::{
    agent_runtime::{bootstrap_profile, BootstrapRole},
    application_settings::{
        AiExecutionSelection, TenderAiExecutionBinding, TenderAiSelectionReadiness,
    },
};
use crate::{host::OrdinaryWorkLease, setup::SetupState, QuantixHost};

mod agent_records;
pub(crate) mod backups;
mod bid_decisions;
mod calculations;
mod change_assessments;
mod coordinated_baselines;
mod decision_cockpit;
mod estimates;
mod external_rfis;
mod final_release;
mod manager_intake;
mod package_production;
mod package_validation;
mod pricing;
mod production_scheduler;
mod submission_packages;
mod team_composer;
mod tender_queries;
mod tender_record_proposals;
mod tender_records;
mod workspace;

pub(crate) use coordinated_baselines::exact_approved_coordinated_baseline_is_current_in_connection;

pub use backups::{
    CreatePortableTenderArchiveCommand, CreateTenderBackupCommand, DeletionReceipt,
    ErasedTenderCopyClass, ImportPortableTenderArchiveCommand, PortableTenderArchiveRecord,
    PrepareTenderRecoveryCommand, ProviderCleanupStatus, ProviderReferenceDiscoveryState,
    PurgeRecoveryRequiredTenderCommand, PurgeTrashedTenderCommand, ResolveTenderRecoveryCommand,
    TenderBackupRecord, TenderBackupState, TenderDeletionSourceState, TenderRecoveryDecision,
    TenderRecoveryDecisionRecord, TenderRecoveryRecord, TenderRecoveryState,
    TenderRetentionDecisionCommand, TenderRetentionDecisionRecord, TenderRetentionState,
    TrashRecoveryRequiredTenderCommand, TrashedTenderDecisionCommand, TrashedTenderRecord,
    TrashedTenderState,
};
pub(crate) use bid_decisions::BidPackageOperationBudget;
pub use bid_decisions::{
    BidDecisionApprovalDecision, BidDecisionApprovalHistoryPage, BidDecisionApprovalInvalidation,
    BidDecisionApprovalInvalidationResult, BidDecisionApprovalRecord, BidDecisionApprovalResult,
    BidDecisionGateBlocker, BidDecisionPackageChangeSummary, BidDecisionPackageInspection,
    BidDecisionPackageRecordBinding, BidDecisionPackageRecordCategory,
    BidDecisionPackageRecordPage, BidDecisionPackageReview, BidDecisionPackageReviewFinding,
    BidDecisionPackageReviewOutcome, BidDecisionPackageReviewResult,
    BidDecisionReturnReworkDisposition, BidDecisionReturnReworkItem, BidDecisionReturnReworkResult,
    BidRecommendation, BidRecommendationOutcome, CapabilityDemand, CapabilityDemandClassification,
    ComplianceDisposition, ComplianceDispositionUpdate, ComplianceMatrixPage, ComplianceMatrixRow,
    CreateBidDecisionPackageCommand, DecideBidDecisionPackageCommand,
    InspectBidDecisionApprovalHistoryCommand, InspectBidDecisionPackageRecordsCommand,
    InspectComplianceMatrixCommand, InvalidateBidDecisionApprovalCommand,
    ManagerCapabilityDemandInput, ResolveBidDecisionReturnReworkCommand, ResourceImplication,
    ReviewFindingSeverity, RunBidDecisionPackageReviewCommand, TenderRecordVersionReference,
};
pub use calculations::{
    ApproveCalculationRuleCommand, ApproveControlledBoqCalculationRunCommand,
    CalculationDecimalInput, CalculationInputState, CalculationRoundingMode,
    CalculationRuleApproval, CalculationRuleReview, CalculationRuleReviewFinding,
    CalculationRuleReviewOutcome, CalculationRuleReviewResult, CalculationRuleTestResult,
    CalculationRuleVersion, CalculationScenarioVersion, CalculationWorkspaceInspection,
    ControlledBoqCalculationRun, ControlledBoqCalculationStatus, CostEstimatorCalculationResult,
    CreateCalculationScenarioCommand, EstimateAggregateCalculationInput,
    EstimateAggregateCalculationRun, ExchangeRateType, InspectCalculationWorkspaceCommand,
    PricingAdjustmentDirection, PricingCalculationAdjustmentInput, PricingCalculationRun,
    ProposeBoqCalculationRuleCommand, RunCalculationRuleReviewCommand,
    RunCostEstimatorCalculationCommand,
};
pub use change_assessments::{
    ChangeAssessment, ChangeAssessmentApprovalConsequence, ChangeAssessmentClassification,
    ChangeAssessmentDecision, ChangeAssessmentDependencyKind, ChangeAssessmentDependencyReference,
    ChangeAssessmentEvidenceExcerpt, ChangeAssessmentImpact, ChangeAssessmentImpactConsequence,
    ChangeAssessmentImpactKind, ChangeAssessmentObjectKind, ChangeAssessmentPage,
    ChangeAssessmentSource, ChangeAssessmentStatus, DecideChangeAssessmentCommand,
    InspectChangeAssessmentsCommand,
};
pub use coordinated_baselines::{
    AssembleCoordinatedBidBaselineCommand, CoordinatedBidBaseline, CoordinatedBidBaselineApproval,
    CoordinatedBidBaselineBinding, CoordinatedBidBaselineBindingKind,
    CoordinatedBidBaselineBlocker, CoordinatedBidBaselineBlockerCode,
    CoordinatedBidBaselineCategory, CoordinatedBidBaselineContradiction,
    CoordinatedBidBaselineContradictionCategory, CoordinatedBidBaselineDecision,
    CoordinatedBidBaselinePage, DecideCoordinatedBidBaselineCommand,
    InspectCoordinatedBidBaselinesCommand,
};
pub use decision_cockpit::{
    DecisionAction, DecisionCockpit, DecisionDependency, DecisionDependencyStatus,
    DecisionEvidence, DecisionFact, DecisionFactKind, DecisionGroupMember, DecisionKind,
    DecisionLifecycleGate, DecisionResponsible, DecisionResponsibleKind, DecisionStatus,
    DecisionTarget, DecisionTargetKind, DecisionUrgency, InspectDecisionCockpitCommand,
    PendingDecision,
};
pub use estimates::{
    ApproveBasisOfEstimateCommand, BasisOfEstimateReview, BasisOfEstimateReviewFinding,
    BasisOfEstimateReviewOutcome, BasisOfEstimateReviewResult, BasisOfEstimateVersion,
    BoqAccountRow, BoqInventoryRow, BoqRowDisposition, BoqTableCandidate, BoqTableDesignation,
    CostBreakdownComponent, CostComponentCategory, CostEstimatorBasisResult,
    DesignateBoqTableCommand, EstimateAllowance, EstimateMaterialAssumption,
    EstimateQueryObservation, EstimateQueryReference, EstimateQuotation, EstimateQuotationKind,
    EstimateWorkspaceInspection, InspectEstimateWorkspaceCommand, RunBasisOfEstimateReviewCommand,
    RunCostEstimatorBasisCommand,
};
pub use external_rfis::{
    ApproveExternalRfiForIssueCommand, CreateExternalRfiDraftCommand,
    ExportApprovedExternalRfiCommand, ExternalRfiApproval, ExternalRfiDraft,
    ExternalRfiEligibleQuery, ExternalRfiEligibleQueryPage, ExternalRfiExportRecord,
    ExternalRfiFindingSeverity, ExternalRfiPage, ExternalRfiQueryReference, ExternalRfiQuestion,
    ExternalRfiRecipient, ExternalRfiResponseCandidatePage, ExternalRfiResponseInterpretation,
    ExternalRfiResponseLink, ExternalRfiReview, ExternalRfiReviewFinding, ExternalRfiReviewOutcome,
    ExternalRfiReviewResult, InspectExternalRfiEligibleQueriesCommand,
    InspectExternalRfiResponseCandidatesCommand, InspectExternalRfisCommand,
    InterpretExternalRfiResponseCommand, RegisterExternalRfiResponseCommand,
    ReviseExternalRfiDraftCommand, RunExternalRfiReviewCommand,
};
pub use final_release::{
    ApproveSubmissionReleaseCommand, ExportReleaseCopyCommand, ReleaseCopyExport, ReleaseCopyItem,
    SubmissionReleaseApproval, SubmissionReleaseInspection, SubmissionReleaseState,
};
pub use manager_intake::{
    ManagerIntakeStage, ManagerIntakeStatus, ManagerIntakeStatusKind, WorkspaceMessageReference,
    WorkspaceMessageReferenceKind, WorkspaceTenderDocument,
};
pub use package_production::{
    GenerateSubmissionSectionsCommand, GenerationRequirement, GenerationRequirementAvailability,
    GenerationRequirementRecordReference, InspectPackageProductionCommand,
    InspectSubmissionArtifactContentCommand, PackageProductionGeneration,
    SubmissionArtifactContent, SubmissionArtifactVersion, SubmissionGeneratedArtifactReference,
    SubmissionSourceArtifactReference,
};
pub use package_validation::{
    ApprovePackageFindingExceptionCommand, FinalReviewAssignment, FinalReviewDecisionEvidence,
    FinalReviewDecisionEvidenceCategory, FinalReviewInspection, FinalReviewPlan,
    FinalReviewReviewer, ManualVerificationResult, PackageFindingExceptionApproval,
    PackageManualVerification, PackageReviewFinding, PackageReviewResult,
    PackageValidationCheckCategory, PackageValidationOutcome, PackageValidationPolicy,
    PackageValidationResult, PackageValidationRule, PackageValidationRun,
    RecordPackageManualVerificationCommand, ReleaseReadinessBlocker, ReleaseReadinessBlockerCode,
    ReleaseReadinessCategorySummary, ReleaseReadinessReport, RunPackageValidationCommand,
    RunSubmissionSectionReviewCommand, SubmissionSectionReview, SubmissionSectionReviewRunResult,
};
pub use pricing::{
    ApproveCommercialStrategyCommand, ApprovePricedCostBaselineCommand,
    ApprovePricingAdjustmentCommand, ApproveTenderPriceCommand, ApprovedTenderPrice,
    CommercialStrategy, CommercialStrategyApproval, CreateCommercialStrategyCommand,
    CreatePricedCostBaselineCommand, CreatePricingAdjustmentCommand, CreatePricingScenarioCommand,
    InspectPricingWorkspaceCommand, PricedCostBaselineApproval, PricedCostBaselineReview,
    PricedCostBaselineReviewFinding, PricedCostBaselineReviewOutcome,
    PricedCostBaselineReviewResult, PricedCostBaselineVersion, PricingAdjustmentApproval,
    PricingAdjustmentKind, PricingAdjustmentReference, PricingAdjustmentReviewResult,
    PricingAdjustmentVersion, PricingDecisionHistoryEntry, PricingScenarioSelection,
    PricingScenarioVersion, PricingWorkspaceInspection, RunPricedCostBaselineReviewCommand,
    RunPricingAdjustmentReviewCommand, SelectPricingScenarioCommand,
};
pub use production_scheduler::{
    ActivateTenderProductionCommand, ApproveProductionFindingExceptionCommand,
    InspectProductionTaskReviewCommand, ProductionArtifactPayload, ProductionArtifactVersion,
    ProductionArtifactVersionSummary, ProductionCoordinationObservation,
    ProductionCoordinationObservationSubject, ProductionCoordinationObservationValue,
    ProductionFindingDisposition, ProductionFindingDispositionKind, ProductionFindingSeverity,
    ProductionIntegrationReadiness, ProductionQueryTreatmentApplication, ProductionRemediation,
    ProductionReview, ProductionReviewFinding, ProductionReviewResult, ProductionTaskInspection,
    ProductionTaskReviewInspection, ProductionTaskRunResult, ProductionTaskState,
    RunProductionTaskCommand, TenderProductionInspection,
};
pub use submission_packages::{
    AssembleSubmissionPackageCommand, InspectSubmissionPackageCommand,
    InspectSubmissionPackageItemContentCommand, SubmissionAuthorshipProvenance,
    SubmissionContributionKind, SubmissionCoverageBlocker, SubmissionCoverageBlockerCode,
    SubmissionCoverageDisposition, SubmissionCoverageRow, SubmissionItemContent,
    SubmissionItemSource, SubmissionPackageAssessment, SubmissionPackageCurrentnessCode,
    SubmissionPackageCurrentnessFact, SubmissionPackageDependency, SubmissionPackageDependencyKind,
    SubmissionPackageItem, SubmissionPackageSection, SubmissionPackageStatus,
    SubmissionPackageVersion, SubmissionProfileVersionReference,
    SubmissionSectionIndependenceContext, SubmissionSectionRiskContext,
    SubmissionValidationContextInput, SubmissionWorkPlanContext,
};
pub use team_composer::{
    ComposeTenderOfficeCommand, DecideWorkPlanProposalCommand, MajorFindingPolicy,
    ReviseWorkPlanProposalCommand, WorkPlanApprovalRecord, WorkPlanCapabilityGap, WorkPlanDecision,
    WorkPlanProfileBinding, WorkPlanProposalInspection, WorkPlanRevisionAction, WorkPlanTask,
    WorkPlanWorkstream,
};
pub use tender_queries::{
    AgentTenderQueryProposal, AgentTenderQueryUpdate, ApprovedQueryTreatment,
    CreateTenderQueryCommand, DecideTenderQueryTreatmentCommand, InspectTenderQueriesCommand,
    ReviseTenderQueryCommand, TenderQuery, TenderQueryInvalidation, TenderQueryPage,
    TenderQueryResponse, TenderQueryStatus, TenderQueryTreatment, TenderQueryTreatmentProposal,
    TenderQueryTreatmentProposalInput, TenderQueryType,
};
pub(crate) use tender_records::ManagerIntakeExtractionRecovery;
pub use tender_records::{
    CreateTenderEngineerEntryCommand, DecideTenderRecordCommand, GenerationAuthoringMode,
    GenerationRequirementKind, InspectTenderRecordsCommand, RunTenderRecordExtractionCommand,
    RunTenderRecordReviewCommand, TenderEvidenceReference, TenderRecordAuthority,
    TenderRecordAuthorityKind, TenderRecordAuthorityReference, TenderRecordBasisKind,
    TenderRecordContradiction, TenderRecordDecisionResult, TenderRecordEngineerDecisionKind,
    TenderRecordEvidence, TenderRecordExtractionResult, TenderRecordField,
    TenderRecordGenerationInstruction, TenderRecordInspection, TenderRecordKind, TenderRecordPage,
    TenderRecordReview, TenderRecordReviewOutcome, TenderRecordReviewResult,
    TenderRecordSourceRelationship, TenderRecordTrustClass,
};
pub use workspace::{
    InspectManagerWorkspaceCommand, ManagerConversation, ManagerWorkspaceProjection,
    ManagerWorkspaceTender, ManagerWorkspaceTenderState, RebindManagerIntakeProviderCommand,
    RecordEngineerWorkspaceMessageCommand, RetryManagerIntakeCommand,
    SearchManagerWorkspaceCommand, SelectManagerWorkspaceTenderCommand, StartManagerTenderCommand,
    TenderOfficeMessage, TenderOfficeMessageAuthor, TenderOfficeMessageKind, WorkspaceActionKind,
    WorkspaceAgentReference, WorkspaceAgentRunReference, WorkspaceCapabilityReadiness,
    WorkspaceCapabilityReadinessState, WorkspaceCurrentAction, WorkspaceDoctorBlockerArea,
    WorkspaceDoctorBlockerSummary, WorkspaceFilesSummary, WorkspaceOutputReference,
    WorkspaceSearchGroup, WorkspaceSearchHit, WorkspaceSearchProjection, WorkspaceSearchResultKind,
    WorkspaceTaskRow, WorkspaceTaskState, WorkspaceTeamSummary, WorkspaceWorkSummary,
};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
static TENDER_STORE_OPEN_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
static CONTENT_VERIFY_PASS_COUNT: AtomicUsize = AtomicUsize::new(0);

pub(crate) const TENDER_SCHEMA_VERSION: i64 = 44;

pub(crate) fn record_agent_run_provider_binding(
    transaction: &Transaction<'_>,
    run_id: &str,
    selection: &AiExecutionSelection,
    recorded_at: &str,
) -> Result<(), TenderCommandError> {
    let binding_json = serde_json_canonicalizer::to_string(selection)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
    transaction
        .execute(
            "INSERT INTO agent_run_provider_bindings (run_id, binding_json, recorded_at)
             VALUES (?1, ?2, ?3)",
            params![run_id, binding_json, recorded_at],
        )
        .map_err(sql_error)?;
    Ok(())
}
const MAX_TENDER_NAME_BYTES: usize = 200;
const MAX_CONTENT_BYTES: usize = 16 * 1024 * 1024;
const ZERO_AUDIT_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

const TENDER_SCHEMA: &str = r#"
CREATE TABLE tender (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  tender_id TEXT NOT NULL UNIQUE,
  current_revision INTEGER NOT NULL CHECK (current_revision > 0),
  lifecycle_phase TEXT NOT NULL CHECK (lifecycle_phase IN (
    'intake', 'bid_decision', 'tender_planning', 'active_production',
    'integrated_review', 'change_assessment', 'package_production', 'final_review', 'declined'
  )),
  created_at TEXT NOT NULL
);
CREATE TABLE tender_ai_execution_binding (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  revision INTEGER NOT NULL CHECK (revision > 0),
  selection_json TEXT CHECK (
    selection_json IS NULL OR json_valid(selection_json)
  ),
  readiness TEXT NOT NULL CHECK (readiness IN (
    'local_only', 'ready', 'selection_required', 'provider_unavailable',
    'catalogue_stale', 'model_unavailable', 'approval_required'
  )),
  status_summary TEXT NOT NULL CHECK (length(CAST(status_summary AS BLOB)) BETWEEN 1 AND 1000),
  updated_at TEXT NOT NULL,
  CHECK ((readiness = 'local_only') = (selection_json IS NULL))
);
CREATE TABLE tender_revisions (
  revision INTEGER PRIMARY KEY CHECK (revision > 0),
  tender_id TEXT NOT NULL,
  name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 200),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tender_id) REFERENCES tender(tender_id)
);
CREATE TABLE tender_office_conversations (
  conversation_id TEXT PRIMARY KEY CHECK (length(conversation_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE manager_workspace_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  conversation_id TEXT NOT NULL UNIQUE,
  last_activity_at TEXT NOT NULL,
  FOREIGN KEY (conversation_id) REFERENCES tender_office_conversations(conversation_id)
);
CREATE TABLE manager_intake_runs (
  intake_run_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  intake_run_id TEXT NOT NULL UNIQUE CHECK (length(intake_run_id) = 32),
  package_intake_id TEXT NOT NULL UNIQUE,
  stage TEXT NOT NULL CHECK (stage IN (
    'waiting_for_local_tools', 'waiting_for_provider_approval',
    'waiting_for_provider', 'package_registered', 'reading_documents',
    'extracting_tender_facts', 'reviewing_tender_facts',
    'preparing_first_decision', 'waiting_for_engineer',
    'bid_decision_ready', 'failed'
  )),
  provider_selection_json TEXT CHECK (
    provider_selection_json IS NULL OR json_valid(provider_selection_json)
  ),
  parseable_document_count INTEGER NOT NULL DEFAULT 0 CHECK (parseable_document_count >= 0),
  parsed_document_count INTEGER NOT NULL DEFAULT 0 CHECK (parsed_document_count >= 0),
  extraction_run_count INTEGER NOT NULL DEFAULT 0 CHECK (extraction_run_count >= 0),
  current_manager_run_id TEXT,
  blocking_agent_run_id TEXT,
  retry_not_before_epoch_seconds INTEGER CHECK (retry_not_before_epoch_seconds > 0),
  provider_retry_attempt_count INTEGER NOT NULL DEFAULT 0
    CHECK (provider_retry_attempt_count BETWEEN 0 AND 4),
  failure_summary TEXT CHECK (
    failure_summary IS NULL OR length(CAST(failure_summary AS BLOB)) BETWEEN 1 AND 2000
  ),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (package_intake_id) REFERENCES intake_runs(intake_id),
  FOREIGN KEY (current_manager_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (blocking_agent_run_id) REFERENCES agent_runs(run_id),
  CHECK (parsed_document_count <= parseable_document_count),
  CHECK (
    (stage = 'failed' AND failure_summary IS NOT NULL AND completed_at IS NOT NULL)
    OR (stage IN ('waiting_for_engineer', 'bid_decision_ready')
        AND failure_summary IS NULL AND completed_at IS NOT NULL)
    OR (stage IN (
          'waiting_for_local_tools', 'waiting_for_provider_approval',
          'waiting_for_provider', 'package_registered', 'reading_documents',
          'extracting_tender_facts', 'reviewing_tender_facts',
          'preparing_first_decision'
        ) AND failure_summary IS NULL AND completed_at IS NULL)
  ),
  CHECK (
    stage IN ('waiting_for_local_tools', 'waiting_for_provider_approval',
              'package_registered', 'reading_documents')
    OR provider_selection_json IS NOT NULL
  ),
  CHECK (
    retry_not_before_epoch_seconds IS NULL
    OR (
      stage = 'waiting_for_provider'
      AND blocking_agent_run_id IS NOT NULL
      AND provider_retry_attempt_count BETWEEN 1 AND 3
    )
  ),
  CHECK (
    provider_retry_attempt_count < 4
    OR (
      stage = 'failed'
      AND blocking_agent_run_id IS NOT NULL
      AND retry_not_before_epoch_seconds IS NULL
    )
  )
);
CREATE TABLE manager_intake_provider_rate_limit_consumptions (
  intake_run_id TEXT NOT NULL,
  source_run_id TEXT NOT NULL,
  provider_retry_attempt_count INTEGER NOT NULL
    CHECK (provider_retry_attempt_count BETWEEN 1 AND 4),
  retry_not_before_epoch_seconds INTEGER CHECK (retry_not_before_epoch_seconds > 0),
  created_at TEXT NOT NULL,
  PRIMARY KEY (intake_run_id, source_run_id),
  FOREIGN KEY (intake_run_id) REFERENCES manager_intake_runs(intake_run_id),
  FOREIGN KEY (source_run_id) REFERENCES agent_runs(run_id),
  CHECK (
    (provider_retry_attempt_count BETWEEN 1 AND 3
      AND retry_not_before_epoch_seconds IS NOT NULL)
    OR (provider_retry_attempt_count = 4
      AND retry_not_before_epoch_seconds IS NULL)
  )
);
CREATE TABLE tender_office_messages (
  message_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  message_id TEXT NOT NULL UNIQUE CHECK (length(message_id) = 32),
  conversation_id TEXT NOT NULL,
  author TEXT NOT NULL CHECK (author IN ('engineer', 'manager', 'system')),
  kind TEXT NOT NULL CHECK (kind IN (
    'routine', 'status', 'question', 'finding', 'handoff', 'blocker', 'output'
  )),
  body TEXT NOT NULL CHECK (length(CAST(body AS BLOB)) BETWEEN 1 AND 4000),
  created_at TEXT NOT NULL,
  FOREIGN KEY (conversation_id) REFERENCES tender_office_conversations(conversation_id)
);
CREATE TABLE manager_intake_outcomes (
  outcome_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  outcome_id TEXT NOT NULL UNIQUE CHECK (length(outcome_id) = 32),
  intake_run_id TEXT NOT NULL,
  manager_run_id TEXT NOT NULL UNIQUE,
  message_id TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL CHECK (kind IN ('question', 'bid_decision')),
  body TEXT NOT NULL CHECK (length(CAST(body AS BLOB)) BETWEEN 1 AND 4000),
  question TEXT CHECK (question IS NULL OR length(CAST(question AS BLOB)) BETWEEN 1 AND 4000),
  recommendation TEXT CHECK (recommendation IN ('proceed', 'hold', 'decline')),
  supporting_records_json TEXT NOT NULL CHECK (json_valid(supporting_records_json)),
  supporting_evidence_json TEXT NOT NULL CHECK (json_valid(supporting_evidence_json)),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (intake_run_id) REFERENCES manager_intake_runs(intake_run_id),
  FOREIGN KEY (manager_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (message_id) REFERENCES tender_office_messages(message_id),
  CHECK (
    (kind = 'question' AND question IS NOT NULL AND recommendation IS NULL)
    OR (kind = 'bid_decision' AND question IS NULL AND recommendation IS NOT NULL)
  )
);
CREATE TABLE manager_intake_answers (
  answer_id TEXT PRIMARY KEY CHECK (length(answer_id) = 32),
  outcome_id TEXT NOT NULL UNIQUE,
  message_id TEXT NOT NULL UNIQUE,
  authority_id TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  FOREIGN KEY (outcome_id) REFERENCES manager_intake_outcomes(outcome_id),
  FOREIGN KEY (message_id) REFERENCES tender_office_messages(message_id),
  FOREIGN KEY (authority_id) REFERENCES tender_record_authorities(authority_id)
);
CREATE TABLE manager_intake_extraction_batches (
  intake_run_id TEXT NOT NULL,
  batch_fingerprint TEXT NOT NULL CHECK (length(batch_fingerprint) = 64),
  extraction_run_id TEXT NOT NULL UNIQUE,
  evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
  completed_at TEXT NOT NULL,
  PRIMARY KEY (intake_run_id, batch_fingerprint),
  FOREIGN KEY (intake_run_id) REFERENCES manager_intake_runs(intake_run_id),
  FOREIGN KEY (extraction_run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE manager_intake_extraction_plan_batches (
  intake_run_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  batch_fingerprint TEXT NOT NULL CHECK (length(batch_fingerprint) = 64),
  canonical_inputs_json TEXT NOT NULL CHECK (json_valid(canonical_inputs_json)),
  request_context_sha256 TEXT NOT NULL CHECK (length(request_context_sha256) = 64),
  estimated_request_bytes INTEGER NOT NULL CHECK (estimated_request_bytes > 0),
  estimator_version TEXT NOT NULL CHECK (length(estimator_version) BETWEEN 1 AND 100),
  created_at TEXT NOT NULL,
  PRIMARY KEY (intake_run_id, ordinal),
  UNIQUE (intake_run_id, batch_fingerprint),
  FOREIGN KEY (intake_run_id) REFERENCES manager_intake_runs(intake_run_id)
);
CREATE TABLE manager_intake_extraction_plan_run_bindings (
  extraction_run_id TEXT PRIMARY KEY CHECK (length(extraction_run_id) = 32),
  intake_run_id TEXT NOT NULL,
  batch_fingerprint TEXT NOT NULL CHECK (length(batch_fingerprint) = 64),
  task_id TEXT NOT NULL CHECK (length(task_id) = 32),
  provider_selection_json TEXT NOT NULL CHECK (json_valid(provider_selection_json)),
  request_context_sha256 TEXT NOT NULL CHECK (length(request_context_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (extraction_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (task_id) REFERENCES tender_tasks(task_id),
  FOREIGN KEY (intake_run_id, batch_fingerprint)
    REFERENCES manager_intake_extraction_plan_batches(intake_run_id, batch_fingerprint)
);
CREATE TABLE tender_office_message_references (
  message_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  kind TEXT NOT NULL CHECK (kind IN (
    'agent_run', 'manager_intake_outcome', 'tender_record', 'source_evidence'
  )),
  reference TEXT NOT NULL CHECK (length(reference) = 32),
  version INTEGER NOT NULL CHECK (version > 0),
  evidence_ordinal INTEGER CHECK (evidence_ordinal IS NULL OR evidence_ordinal > 0),
  label TEXT NOT NULL CHECK (length(CAST(label AS BLOB)) BETWEEN 1 AND 1000),
  detail TEXT CHECK (detail IS NULL OR length(CAST(detail AS BLOB)) BETWEEN 1 AND 2000),
  PRIMARY KEY (message_id, ordinal),
  FOREIGN KEY (message_id) REFERENCES tender_office_messages(message_id),
  CHECK (
    (kind = 'source_evidence' AND evidence_ordinal IS NOT NULL)
    OR (kind != 'source_evidence' AND evidence_ordinal IS NULL)
  )
);
CREATE TRIGGER tender_office_conversations_no_update
BEFORE UPDATE ON tender_office_conversations
BEGIN
  SELECT RAISE(ABORT, 'Tender Office Conversations are immutable');
END;
CREATE TRIGGER tender_office_conversations_no_delete
BEFORE DELETE ON tender_office_conversations
BEGIN
  SELECT RAISE(ABORT, 'Tender Office Conversations are immutable');
END;
CREATE TRIGGER tender_office_messages_no_update
BEFORE UPDATE ON tender_office_messages
BEGIN
  SELECT RAISE(ABORT, 'Tender Office messages are immutable');
END;
CREATE TRIGGER tender_office_messages_no_delete
BEFORE DELETE ON tender_office_messages
BEGIN
  SELECT RAISE(ABORT, 'Tender Office messages are immutable');
END;
CREATE TRIGGER manager_intake_outcomes_no_update
BEFORE UPDATE ON manager_intake_outcomes
BEGIN
  SELECT RAISE(ABORT, 'Manager intake outcomes are immutable');
END;
CREATE TRIGGER manager_intake_outcomes_no_delete
BEFORE DELETE ON manager_intake_outcomes
BEGIN
  SELECT RAISE(ABORT, 'Manager intake outcomes are immutable');
END;
CREATE TRIGGER manager_intake_answers_no_update
BEFORE UPDATE ON manager_intake_answers
BEGIN
  SELECT RAISE(ABORT, 'Manager intake answers are immutable');
END;
CREATE TRIGGER manager_intake_answers_no_delete
BEFORE DELETE ON manager_intake_answers
BEGIN
  SELECT RAISE(ABORT, 'Manager intake answers are immutable');
END;
CREATE TRIGGER manager_intake_provider_rate_limit_consumptions_no_update
BEFORE UPDATE ON manager_intake_provider_rate_limit_consumptions
BEGIN
  SELECT RAISE(ABORT, 'Manager intake rate-limit consumptions are immutable');
END;
CREATE TRIGGER manager_intake_provider_rate_limit_consumptions_no_delete
BEFORE DELETE ON manager_intake_provider_rate_limit_consumptions
BEGIN
  SELECT RAISE(ABORT, 'Manager intake rate-limit consumptions are immutable');
END;
CREATE TRIGGER manager_intake_extraction_batches_no_update
BEFORE UPDATE ON manager_intake_extraction_batches
BEGIN
  SELECT RAISE(ABORT, 'Manager intake extraction batches are immutable');
END;
CREATE TRIGGER manager_intake_extraction_batches_no_delete
BEFORE DELETE ON manager_intake_extraction_batches
BEGIN
  SELECT RAISE(ABORT, 'Manager intake extraction batches are immutable');
END;
CREATE TRIGGER manager_intake_extraction_plan_batches_no_update
BEFORE UPDATE ON manager_intake_extraction_plan_batches
BEGIN
  SELECT RAISE(ABORT, 'Manager intake extraction plans are immutable');
END;
CREATE TRIGGER manager_intake_extraction_plan_batches_no_delete
BEFORE DELETE ON manager_intake_extraction_plan_batches
BEGIN
  SELECT RAISE(ABORT, 'Manager intake extraction plans are immutable');
END;
CREATE TRIGGER manager_intake_extraction_plan_run_bindings_no_update
BEFORE UPDATE ON manager_intake_extraction_plan_run_bindings
BEGIN
  SELECT RAISE(ABORT, 'Manager intake extraction plan bindings are immutable');
END;
CREATE TRIGGER manager_intake_extraction_plan_run_bindings_no_delete
BEFORE DELETE ON manager_intake_extraction_plan_run_bindings
BEGIN
  SELECT RAISE(ABORT, 'Manager intake extraction plan bindings are immutable');
END;
CREATE TRIGGER tender_office_message_references_no_update
BEFORE UPDATE ON tender_office_message_references
BEGIN
  SELECT RAISE(ABORT, 'Tender Office message references are immutable');
END;
CREATE TRIGGER tender_office_message_references_no_delete
BEFORE DELETE ON tender_office_message_references
BEGIN
  SELECT RAISE(ABORT, 'Tender Office message references are immutable');
END;
CREATE TABLE tender_retention (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  state TEXT NOT NULL CHECK (state IN ('active', 'archived')),
  decision_id TEXT,
  decision_manifest_sha256 TEXT CHECK (
    decision_manifest_sha256 IS NULL OR length(decision_manifest_sha256) = 64
  ),
  updated_at TEXT NOT NULL,
  CHECK (
    (state = 'active')
    OR (state = 'archived' AND length(decision_id) = 32 AND decision_manifest_sha256 IS NOT NULL)
  )
);
CREATE TABLE tender_retention_decisions (
  decision_id TEXT PRIMARY KEY CHECK (length(decision_id) = 32),
  state TEXT NOT NULL CHECK (state IN ('active', 'archived')),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_engineer'),
  decision_json TEXT NOT NULL CHECK (json_valid(decision_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  decided_at TEXT NOT NULL,
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE content_objects (
  sha256 TEXT PRIMARY KEY CHECK (length(sha256) = 64),
  integrity TEXT NOT NULL UNIQUE,
  size_bytes INTEGER NOT NULL CHECK (size_bytes > 0)
);
CREATE TABLE content_versions (
  logical_id TEXT NOT NULL CHECK (length(CAST(logical_id AS BLOB)) BETWEEN 1 AND 100),
  revision INTEGER NOT NULL CHECK (revision > 0),
  sha256 TEXT NOT NULL,
  media_type TEXT NOT NULL CHECK (length(CAST(media_type AS BLOB)) BETWEEN 1 AND 100),
  created_at TEXT NOT NULL,
  PRIMARY KEY (logical_id, revision),
  FOREIGN KEY (sha256) REFERENCES content_objects(sha256)
);
CREATE TABLE content_heads (
  logical_id TEXT PRIMARY KEY,
  current_revision INTEGER NOT NULL CHECK (current_revision > 0),
  FOREIGN KEY (logical_id, current_revision)
    REFERENCES content_versions(logical_id, revision)
);
CREATE TABLE intake_runs (
  intake_id TEXT PRIMARY KEY CHECK (length(intake_id) = 32),
  source_kind TEXT NOT NULL CHECK (source_kind IN ('directory', 'zip_archive')),
  source_path TEXT NOT NULL,
  source_name TEXT NOT NULL,
  discovered_count INTEGER NOT NULL CHECK (discovered_count >= 0),
  registered_count INTEGER NOT NULL CHECK (registered_count >= 0),
  exception_count INTEGER NOT NULL CHECK (exception_count >= 0),
  created_at TEXT NOT NULL
);
CREATE TABLE query_register (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  opened_by_intake_id TEXT NOT NULL,
  opened_at TEXT NOT NULL,
  FOREIGN KEY (opened_by_intake_id) REFERENCES intake_runs(intake_id)
);
CREATE TABLE tender_queries (
  query_id TEXT PRIMARY KEY CHECK (length(query_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE tender_query_versions (
  query_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  query_type TEXT NOT NULL CHECK (query_type IN (
    'missing_information', 'ambiguity', 'contradiction', 'responsibility_sensitive'
  )),
  question TEXT NOT NULL CHECK (length(CAST(question AS BLOB)) BETWEEN 1 AND 4000),
  ambiguity_or_gap TEXT NOT NULL CHECK (
    length(CAST(ambiguity_or_gap AS BLOB)) BETWEEN 1 AND 4000
  ),
  owner_profile_id TEXT NOT NULL CHECK (length(owner_profile_id) = 32),
  owner_profile_version INTEGER NOT NULL CHECK (owner_profile_version > 0),
  evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
  affected_records_json TEXT NOT NULL CHECK (json_valid(affected_records_json)),
  affected_task_keys_json TEXT NOT NULL CHECK (json_valid(affected_task_keys_json)),
  invalidation_targets_json TEXT NOT NULL CHECK (json_valid(invalidation_targets_json)),
  due_at TEXT NOT NULL,
  material INTEGER NOT NULL CHECK (material IN (0, 1)),
  release_blocking INTEGER NOT NULL CHECK (release_blocking IN (0, 1)),
  proposed_treatments_json TEXT NOT NULL CHECK (json_valid(proposed_treatments_json)),
  responses_json TEXT NOT NULL CHECK (json_valid(responses_json)),
  source_run_id TEXT,
  created_by TEXT NOT NULL CHECK (created_by IN ('engineer_user', 'agent_run')),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (query_id, version),
  FOREIGN KEY (query_id) REFERENCES tender_queries(query_id),
  FOREIGN KEY (owner_profile_id, owner_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (source_run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE tender_query_heads (
  query_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (query_id, current_version)
    REFERENCES tender_query_versions(query_id, version)
);
CREATE TABLE tender_query_treatment_decisions (
  decision_id TEXT PRIMARY KEY CHECK (length(decision_id) = 32),
  query_id TEXT NOT NULL,
  query_version INTEGER NOT NULL CHECK (query_version BETWEEN 1 AND 32),
  treatment TEXT NOT NULL CHECK (treatment IN (
    'internal_resolution', 'external_rfi_drafting', 'approved_assumption',
    'qualification', 'exclusion', 'allowance', 'blocked'
  )),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  treatment_details TEXT NOT NULL CHECK (
    length(CAST(treatment_details AS BLOB)) BETWEEN 1 AND 4000
  ),
  closes_query INTEGER NOT NULL CHECK (closes_query IN (0, 1)),
  decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  invalidation_targets_json TEXT NOT NULL CHECK (json_valid(invalidation_targets_json)),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (query_id, query_version),
  FOREIGN KEY (query_id, query_version)
    REFERENCES tender_query_versions(query_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE tender_query_target_invalidations (
  invalidation_id TEXT PRIMARY KEY CHECK (length(invalidation_id) = 32),
  query_id TEXT NOT NULL,
  query_version INTEGER NOT NULL CHECK (query_version BETWEEN 1 AND 32),
  target_kind TEXT NOT NULL CHECK (target_kind IN (
    'production_task', 'artifact', 'calculation', 'review', 'approval'
  )),
  target_id TEXT NOT NULL CHECK (length(CAST(target_id AS BLOB)) BETWEEN 1 AND 200),
  target_version INTEGER NOT NULL CHECK (target_version >= 0),
  reason TEXT NOT NULL CHECK (reason IN (
    'query_opened', 'evidence_changed', 'response_added', 'treatment_changed'
  )),
  created_at TEXT NOT NULL,
  UNIQUE (query_id, query_version, target_kind, target_id, target_version),
  FOREIGN KEY (query_id, query_version)
    REFERENCES tender_query_versions(query_id, version)
);
CREATE TABLE external_rfis (
  rfi_id TEXT PRIMARY KEY CHECK (length(rfi_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE external_rfi_versions (
  rfi_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  query_refs_json TEXT NOT NULL CHECK (json_valid(query_refs_json)),
  questions_json TEXT NOT NULL CHECK (json_valid(questions_json)),
  source_evidence_json TEXT NOT NULL CHECK (json_valid(source_evidence_json)),
  contractual_context TEXT NOT NULL CHECK (
    length(CAST(contractual_context AS BLOB)) BETWEEN 1 AND 8000
  ),
  response_need TEXT NOT NULL CHECK (
    length(CAST(response_need AS BLOB)) BETWEEN 1 AND 4000
  ),
  attachments_json TEXT NOT NULL CHECK (json_valid(attachments_json)),
  due_at TEXT NOT NULL,
  recipient_json TEXT NOT NULL CHECK (json_valid(recipient_json)),
  affected_task_keys_json TEXT NOT NULL CHECK (json_valid(affected_task_keys_json)),
  affected_commitments_json TEXT NOT NULL CHECK (json_valid(affected_commitments_json)),
  created_by TEXT NOT NULL CHECK (created_by = 'engineer_user'),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (rfi_id, version),
  FOREIGN KEY (rfi_id) REFERENCES external_rfis(rfi_id)
);
CREATE TABLE external_rfi_heads (
  rfi_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (rfi_id, current_version)
    REFERENCES external_rfi_versions(rfi_id, version)
);
CREATE TABLE external_rfi_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  rfi_id TEXT NOT NULL,
  rfi_version INTEGER NOT NULL CHECK (rfi_version BETWEEN 1 AND 32),
  rfi_manifest_sha256 TEXT NOT NULL CHECK (length(rfi_manifest_sha256) = 64),
  reviewer_run_id TEXT NOT NULL UNIQUE,
  reviewer_profile_id TEXT NOT NULL CHECK (length(reviewer_profile_id) = 32),
  reviewer_profile_version INTEGER NOT NULL CHECK (reviewer_profile_version > 0),
  outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
  findings_json TEXT NOT NULL CHECK (json_valid(findings_json)),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (rfi_id, rfi_version),
  FOREIGN KEY (rfi_id, rfi_version)
    REFERENCES external_rfi_versions(rfi_id, version),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (reviewer_profile_id, reviewer_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE external_rfi_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  rfi_id TEXT NOT NULL,
  rfi_version INTEGER NOT NULL CHECK (rfi_version BETWEEN 1 AND 32),
  rfi_manifest_sha256 TEXT NOT NULL CHECK (length(rfi_manifest_sha256) = 64),
  review_id TEXT NOT NULL UNIQUE,
  review_manifest_sha256 TEXT NOT NULL CHECK (length(review_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  approval_sha256 TEXT NOT NULL CHECK (length(approval_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (rfi_id, rfi_version),
  FOREIGN KEY (rfi_id, rfi_version)
    REFERENCES external_rfi_versions(rfi_id, version),
  FOREIGN KEY (review_id) REFERENCES external_rfi_reviews(review_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE external_rfi_exports (
  export_id TEXT PRIMARY KEY CHECK (length(export_id) = 32),
  approval_id TEXT NOT NULL,
  relative_path TEXT NOT NULL UNIQUE,
  bytes_sha256 TEXT NOT NULL CHECK (length(bytes_sha256) = 64),
  size_bytes INTEGER NOT NULL CHECK (size_bytes BETWEEN 1 AND 524288),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (approval_id) REFERENCES external_rfi_approvals(approval_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE external_rfi_responses (
  response_link_id TEXT PRIMARY KEY CHECK (length(response_link_id) = 32),
  rfi_id TEXT NOT NULL,
  rfi_version INTEGER NOT NULL CHECK (rfi_version BETWEEN 1 AND 32),
  approval_id TEXT NOT NULL,
  source_artifact_id TEXT NOT NULL,
  source_artifact_version INTEGER NOT NULL CHECK (source_artifact_version > 0),
  registered_by TEXT NOT NULL CHECK (registered_by = 'engineer_user'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (rfi_id, rfi_version, source_artifact_id, source_artifact_version),
  FOREIGN KEY (rfi_id, rfi_version)
    REFERENCES external_rfi_versions(rfi_id, version),
  FOREIGN KEY (approval_id) REFERENCES external_rfi_approvals(approval_id),
  FOREIGN KEY (source_artifact_id, source_artifact_version)
    REFERENCES source_artifact_versions(artifact_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE external_rfi_response_interpretations (
  interpretation_id TEXT PRIMARY KEY CHECK (length(interpretation_id) = 32),
  response_link_id TEXT NOT NULL,
  query_id TEXT NOT NULL,
  source_query_version INTEGER NOT NULL CHECK (source_query_version BETWEEN 1 AND 32),
  base_query_version INTEGER NOT NULL CHECK (base_query_version BETWEEN 1 AND 32),
  resulting_query_version INTEGER NOT NULL CHECK (resulting_query_version BETWEEN 1 AND 32),
  query_decision_id TEXT NOT NULL UNIQUE,
  material INTEGER NOT NULL CHECK (material IN (0, 1)),
  interpretation TEXT NOT NULL CHECK (length(CAST(interpretation AS BLOB)) BETWEEN 1 AND 8000),
  decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (response_link_id, query_id),
  FOREIGN KEY (response_link_id) REFERENCES external_rfi_responses(response_link_id),
  FOREIGN KEY (query_id, source_query_version)
    REFERENCES tender_query_versions(query_id, version),
  FOREIGN KEY (query_id, base_query_version)
    REFERENCES tender_query_versions(query_id, version),
  FOREIGN KEY (query_id, resulting_query_version)
    REFERENCES tender_query_versions(query_id, version),
  FOREIGN KEY (query_decision_id)
    REFERENCES tender_query_treatment_decisions(decision_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE calculation_rules (
  rule_id TEXT PRIMARY KEY CHECK (length(rule_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE calculation_rule_versions (
  rule_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 200),
  formula TEXT NOT NULL CHECK (length(CAST(formula AS BLOB)) BETWEEN 1 AND 1000),
  engine_version TEXT NOT NULL CHECK (length(CAST(engine_version AS BLOB)) BETWEEN 1 AND 64),
  supported_units_json TEXT NOT NULL CHECK (json_valid(supported_units_json)),
  supported_rounding_json TEXT NOT NULL CHECK (json_valid(supported_rounding_json)),
  deterministic_tests_json TEXT NOT NULL CHECK (json_valid(deterministic_tests_json)),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_by TEXT NOT NULL CHECK (created_by = 'engineer_user'),
  created_at TEXT NOT NULL,
  PRIMARY KEY (rule_id, version),
  FOREIGN KEY (rule_id) REFERENCES calculation_rules(rule_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE calculation_rule_heads (
  rule_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (rule_id, current_version)
    REFERENCES calculation_rule_versions(rule_id, version)
);
CREATE TABLE calculation_rule_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  rule_id TEXT NOT NULL,
  rule_version INTEGER NOT NULL CHECK (rule_version BETWEEN 1 AND 32),
  rule_manifest_sha256 TEXT NOT NULL CHECK (length(rule_manifest_sha256) = 64),
  reviewer_run_id TEXT NOT NULL UNIQUE,
  reviewer_profile_id TEXT NOT NULL CHECK (length(reviewer_profile_id) = 32),
  reviewer_profile_version INTEGER NOT NULL CHECK (reviewer_profile_version > 0),
  outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
  findings_json TEXT NOT NULL CHECK (json_valid(findings_json)),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (rule_id, rule_version),
  FOREIGN KEY (rule_id, rule_version)
    REFERENCES calculation_rule_versions(rule_id, version),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (reviewer_profile_id, reviewer_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE calculation_rule_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  rule_id TEXT NOT NULL,
  rule_version INTEGER NOT NULL CHECK (rule_version BETWEEN 1 AND 32),
  rule_manifest_sha256 TEXT NOT NULL CHECK (length(rule_manifest_sha256) = 64),
  review_id TEXT NOT NULL UNIQUE,
  review_manifest_sha256 TEXT NOT NULL CHECK (length(review_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'engineer_in_the_loop'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (rule_id, rule_version),
  FOREIGN KEY (rule_id, rule_version)
    REFERENCES calculation_rule_versions(rule_id, version),
  FOREIGN KEY (review_id) REFERENCES calculation_rule_reviews(review_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE calculation_scenario_versions (
  scenario_id TEXT NOT NULL CHECK (length(scenario_id) = 32),
  version INTEGER NOT NULL CHECK (version = 1),
  name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 100),
  quantity_unit TEXT NOT NULL CHECK (length(CAST(quantity_unit AS BLOB)) BETWEEN 1 AND 16),
  rate_basis_unit TEXT NOT NULL CHECK (length(CAST(rate_basis_unit AS BLOB)) BETWEEN 1 AND 16),
  rate_currency TEXT NOT NULL CHECK (length(rate_currency) = 3),
  exchange_rate_id TEXT NOT NULL CHECK (length(exchange_rate_id) = 32),
  exchange_rate_version INTEGER NOT NULL CHECK (exchange_rate_version = 1),
  exchange_rate_json TEXT NOT NULL CHECK (json_valid(exchange_rate_json)),
  exchange_rate_effective_date TEXT CHECK (exchange_rate_effective_date IS NULL OR length(exchange_rate_effective_date) = 10),
  pricing_date TEXT NOT NULL CHECK (length(pricing_date) = 10),
  exchange_rate_type TEXT CHECK (exchange_rate_type IS NULL OR exchange_rate_type IN ('spot', 'contract', 'budget', 'central_bank')),
  output_currency TEXT NOT NULL CHECK (length(output_currency) = 3),
  rounding_policy_id TEXT NOT NULL CHECK (length(rounding_policy_id) = 32),
  rounding_policy_version INTEGER NOT NULL CHECK (rounding_policy_version = 1),
  precision INTEGER NOT NULL CHECK (precision BETWEEN 0 AND 12),
  rounding_mode TEXT NOT NULL CHECK (rounding_mode IN ('midpoint_away_from_zero', 'midpoint_nearest_even')),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'engineer_in_the_loop'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (scenario_id, version),
  UNIQUE (exchange_rate_id, exchange_rate_version),
  UNIQUE (rounding_policy_id, rounding_policy_version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE calculation_runs (
  calculation_run_id TEXT PRIMARY KEY CHECK (length(calculation_run_id) = 32),
  cost_estimator_run_id TEXT NOT NULL UNIQUE,
  rule_id TEXT NOT NULL,
  rule_version INTEGER NOT NULL CHECK (rule_version BETWEEN 1 AND 32),
  rule_approval_id TEXT NOT NULL,
  scenario_id TEXT NOT NULL,
  scenario_version INTEGER NOT NULL CHECK (scenario_version = 1),
  status TEXT NOT NULL CHECK (status IN (
    'completed', 'missing_input', 'unavailable_input', 'ambiguous_input',
    'invalid_input', 'dimension_mismatch'
  )),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (rule_id, rule_version)
    REFERENCES calculation_rule_versions(rule_id, version),
  FOREIGN KEY (rule_approval_id) REFERENCES calculation_rule_approvals(approval_id),
  FOREIGN KEY (scenario_id, scenario_version)
    REFERENCES calculation_scenario_versions(scenario_id, version),
  FOREIGN KEY (cost_estimator_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE calculation_run_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  calculation_run_id TEXT NOT NULL UNIQUE,
  run_manifest_sha256 TEXT NOT NULL CHECK (length(run_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'engineer_in_the_loop'),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (calculation_run_id) REFERENCES calculation_runs(calculation_run_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE estimate_aggregate_calculation_runs (
  aggregate_run_id TEXT PRIMARY KEY CHECK (length(aggregate_run_id) = 32),
  author_run_id TEXT NOT NULL,
  comparison_total_calculation_run_id TEXT NOT NULL,
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  rule_id TEXT NOT NULL,
  rule_version INTEGER NOT NULL CHECK (rule_version BETWEEN 1 AND 32),
  rule_approval_id TEXT NOT NULL,
  scenario_id TEXT NOT NULL,
  scenario_version INTEGER NOT NULL CHECK (scenario_version = 1),
  precision INTEGER NOT NULL CHECK (precision BETWEEN 0 AND 12),
  rounding_mode TEXT NOT NULL CHECK (rounding_mode IN (
    'midpoint_away_from_zero', 'midpoint_nearest_even'
  )),
  final_amount TEXT NOT NULL,
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (author_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (comparison_total_calculation_run_id)
    REFERENCES calculation_runs(calculation_run_id),
  FOREIGN KEY (rule_id, rule_version)
    REFERENCES calculation_rule_versions(rule_id, version),
  FOREIGN KEY (rule_approval_id) REFERENCES calculation_rule_approvals(approval_id),
  FOREIGN KEY (scenario_id, scenario_version)
    REFERENCES calculation_scenario_versions(scenario_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE estimate_aggregate_calculation_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  aggregate_run_id TEXT NOT NULL UNIQUE,
  aggregate_manifest_sha256 TEXT NOT NULL CHECK (length(aggregate_manifest_sha256) = 64),
  basis_id TEXT NOT NULL,
  basis_version INTEGER NOT NULL CHECK (basis_version BETWEEN 1 AND 32),
  basis_manifest_sha256 TEXT NOT NULL CHECK (length(basis_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'engineer_in_the_loop'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (aggregate_run_id)
    REFERENCES estimate_aggregate_calculation_runs(aggregate_run_id),
  FOREIGN KEY (basis_id, basis_version)
    REFERENCES basis_of_estimate_versions(basis_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE boq_table_designations (
  designation_id TEXT PRIMARY KEY CHECK (length(designation_id) = 32),
  artifact_id TEXT NOT NULL,
  artifact_version INTEGER NOT NULL CHECK (artifact_version > 0),
  table_number INTEGER NOT NULL CHECK (table_number > 0),
  header_row_count INTEGER NOT NULL CHECK (header_row_count BETWEEN 0 AND 8),
  designated_by TEXT NOT NULL CHECK (designated_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'engineer_in_the_loop'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (artifact_id, artifact_version, table_number),
  FOREIGN KEY (artifact_id, artifact_version)
    REFERENCES source_artifact_versions(artifact_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE basis_of_estimates (
  basis_id TEXT PRIMARY KEY CHECK (length(basis_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE basis_of_estimate_versions (
  basis_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  author_run_id TEXT NOT NULL UNIQUE,
  author_profile_id TEXT NOT NULL CHECK (length(author_profile_id) = 32),
  author_profile_version INTEGER NOT NULL CHECK (author_profile_version > 0),
  complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
  reconciled INTEGER NOT NULL CHECK (reconciled IN (0, 1)),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (
    json_valid(manifest_json)
    AND length(CAST(manifest_json AS BLOB)) <= 4194304
  ),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (basis_id, version),
  FOREIGN KEY (basis_id) REFERENCES basis_of_estimates(basis_id),
  FOREIGN KEY (author_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (author_profile_id, author_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE basis_of_estimate_heads (
  basis_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (basis_id, current_version)
    REFERENCES basis_of_estimate_versions(basis_id, version)
);
CREATE TABLE basis_of_estimate_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  basis_id TEXT NOT NULL,
  basis_version INTEGER NOT NULL CHECK (basis_version BETWEEN 1 AND 32),
  basis_manifest_sha256 TEXT NOT NULL CHECK (length(basis_manifest_sha256) = 64),
  reviewer_run_id TEXT NOT NULL UNIQUE,
  reviewer_profile_id TEXT NOT NULL CHECK (length(reviewer_profile_id) = 32),
  reviewer_profile_version INTEGER NOT NULL CHECK (reviewer_profile_version > 0),
  outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
  findings_json TEXT NOT NULL CHECK (json_valid(findings_json)),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (basis_id, basis_version),
  FOREIGN KEY (basis_id, basis_version)
    REFERENCES basis_of_estimate_versions(basis_id, version),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (reviewer_profile_id, reviewer_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE basis_of_estimate_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  basis_id TEXT NOT NULL,
  basis_version INTEGER NOT NULL CHECK (basis_version BETWEEN 1 AND 32),
  basis_manifest_sha256 TEXT NOT NULL CHECK (length(basis_manifest_sha256) = 64),
  review_id TEXT NOT NULL UNIQUE,
  review_manifest_sha256 TEXT NOT NULL CHECK (length(review_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'engineer_in_the_loop'),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (basis_id, basis_version),
  FOREIGN KEY (basis_id, basis_version)
    REFERENCES basis_of_estimate_versions(basis_id, version),
  FOREIGN KEY (review_id) REFERENCES basis_of_estimate_reviews(review_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE priced_cost_baselines (
  baseline_id TEXT PRIMARY KEY CHECK (length(baseline_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE priced_cost_baseline_versions (
  baseline_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  basis_id TEXT NOT NULL,
  basis_version INTEGER NOT NULL CHECK (basis_version BETWEEN 1 AND 32),
  basis_manifest_sha256 TEXT NOT NULL CHECK (length(basis_manifest_sha256) = 64),
  aggregate_run_id TEXT NOT NULL,
  aggregate_manifest_sha256 TEXT NOT NULL CHECK (length(aggregate_manifest_sha256) = 64),
  amount TEXT NOT NULL CHECK (length(CAST(amount AS BLOB)) BETWEEN 1 AND 128),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (
    json_valid(manifest_json)
    AND length(CAST(manifest_json AS BLOB)) <= 1048576
  ),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (baseline_id, version),
  FOREIGN KEY (baseline_id) REFERENCES priced_cost_baselines(baseline_id),
  FOREIGN KEY (basis_id, basis_version)
    REFERENCES basis_of_estimate_versions(basis_id, version),
  FOREIGN KEY (aggregate_run_id)
    REFERENCES estimate_aggregate_calculation_runs(aggregate_run_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE priced_cost_baseline_heads (
  baseline_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (baseline_id, current_version)
    REFERENCES priced_cost_baseline_versions(baseline_id, version)
);
CREATE TABLE priced_cost_baseline_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  baseline_manifest_sha256 TEXT NOT NULL CHECK (length(baseline_manifest_sha256) = 64),
  reviewer_run_id TEXT NOT NULL UNIQUE,
  reviewer_profile_id TEXT NOT NULL CHECK (length(reviewer_profile_id) = 32),
  reviewer_profile_version INTEGER NOT NULL CHECK (reviewer_profile_version > 0),
  outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
  findings_json TEXT NOT NULL CHECK (json_valid(findings_json)),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (baseline_id, baseline_version),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES priced_cost_baseline_versions(baseline_id, version),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (reviewer_profile_id, reviewer_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE priced_cost_baseline_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  baseline_manifest_sha256 TEXT NOT NULL CHECK (length(baseline_manifest_sha256) = 64),
  review_id TEXT NOT NULL UNIQUE,
  review_manifest_sha256 TEXT NOT NULL CHECK (length(review_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'engineer_in_the_loop'),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (baseline_id, baseline_version),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES priced_cost_baseline_versions(baseline_id, version),
  FOREIGN KEY (review_id) REFERENCES priced_cost_baseline_reviews(review_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE pricing_calculation_runs (
  pricing_calculation_run_id TEXT PRIMARY KEY CHECK (length(pricing_calculation_run_id) = 32),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  baseline_aggregate_run_id TEXT NOT NULL,
  baseline_aggregate_manifest_sha256 TEXT NOT NULL CHECK (length(baseline_aggregate_manifest_sha256) = 64),
  final_amount TEXT NOT NULL CHECK (length(CAST(final_amount AS BLOB)) BETWEEN 1 AND 128),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  manifest_json TEXT NOT NULL CHECK (
    json_valid(manifest_json)
    AND length(CAST(manifest_json AS BLOB)) <= 1048576
  ),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (baseline_aggregate_run_id)
    REFERENCES estimate_aggregate_calculation_runs(aggregate_run_id)
);
CREATE TABLE pricing_adjustments (
  adjustment_id TEXT PRIMARY KEY CHECK (length(adjustment_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE pricing_adjustment_versions (
  adjustment_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  calculation_run_id TEXT NOT NULL,
  calculation_manifest_sha256 TEXT NOT NULL CHECK (length(calculation_manifest_sha256) = 64),
  kind TEXT NOT NULL CHECK (kind IN ('contingency', 'markup', 'exclusion', 'qualification', 'commercial_strategy', 'other')),
  direction TEXT NOT NULL CHECK (direction IN ('add', 'deduct')),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (adjustment_id, version),
  FOREIGN KEY (adjustment_id) REFERENCES pricing_adjustments(adjustment_id),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES priced_cost_baseline_versions(baseline_id, version),
  FOREIGN KEY (calculation_run_id) REFERENCES calculation_runs(calculation_run_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE pricing_adjustment_heads (
  adjustment_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (adjustment_id, current_version)
    REFERENCES pricing_adjustment_versions(adjustment_id, version)
);
CREATE TABLE pricing_adjustment_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  adjustment_id TEXT NOT NULL,
  adjustment_version INTEGER NOT NULL CHECK (adjustment_version BETWEEN 1 AND 32),
  adjustment_manifest_sha256 TEXT NOT NULL CHECK (length(adjustment_manifest_sha256) = 64),
  reviewer_run_id TEXT NOT NULL UNIQUE,
  reviewer_profile_id TEXT NOT NULL CHECK (length(reviewer_profile_id) = 32),
  reviewer_profile_version INTEGER NOT NULL CHECK (reviewer_profile_version > 0),
  outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
  findings_json TEXT NOT NULL CHECK (json_valid(findings_json)),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (adjustment_id, adjustment_version),
  FOREIGN KEY (adjustment_id, adjustment_version)
    REFERENCES pricing_adjustment_versions(adjustment_id, version),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE pricing_adjustment_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  adjustment_id TEXT NOT NULL,
  adjustment_version INTEGER NOT NULL CHECK (adjustment_version BETWEEN 1 AND 32),
  adjustment_manifest_sha256 TEXT NOT NULL CHECK (length(adjustment_manifest_sha256) = 64),
  review_id TEXT NOT NULL UNIQUE,
  review_manifest_sha256 TEXT NOT NULL CHECK (length(review_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (adjustment_id, adjustment_version),
  FOREIGN KEY (adjustment_id, adjustment_version)
    REFERENCES pricing_adjustment_versions(adjustment_id, version),
  FOREIGN KEY (review_id) REFERENCES pricing_adjustment_reviews(review_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE commercial_strategies (
  strategy_id TEXT PRIMARY KEY CHECK (length(strategy_id) = 32),
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES priced_cost_baseline_versions(baseline_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE commercial_strategy_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  strategy_id TEXT NOT NULL UNIQUE,
  strategy_manifest_sha256 TEXT NOT NULL CHECK (length(strategy_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (strategy_id) REFERENCES commercial_strategies(strategy_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE pricing_scenarios (
  pricing_scenario_id TEXT PRIMARY KEY CHECK (length(pricing_scenario_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE pricing_scenario_versions (
  pricing_scenario_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  strategy_id TEXT NOT NULL,
  pricing_calculation_run_id TEXT NOT NULL UNIQUE,
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (pricing_scenario_id, version),
  FOREIGN KEY (pricing_scenario_id) REFERENCES pricing_scenarios(pricing_scenario_id),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES priced_cost_baseline_versions(baseline_id, version),
  FOREIGN KEY (strategy_id) REFERENCES commercial_strategies(strategy_id),
  FOREIGN KEY (pricing_calculation_run_id)
    REFERENCES pricing_calculation_runs(pricing_calculation_run_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE pricing_scenario_selections (
  selection_id TEXT PRIMARY KEY CHECK (length(selection_id) = 32),
  pricing_scenario_id TEXT NOT NULL,
  pricing_scenario_version INTEGER NOT NULL CHECK (pricing_scenario_version BETWEEN 1 AND 32),
  scenario_manifest_sha256 TEXT NOT NULL CHECK (length(scenario_manifest_sha256) = 64),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  selected_by TEXT NOT NULL CHECK (selected_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (pricing_scenario_id, pricing_scenario_version)
    REFERENCES pricing_scenario_versions(pricing_scenario_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE pricing_selection_head (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  selection_id TEXT NOT NULL UNIQUE,
  FOREIGN KEY (selection_id) REFERENCES pricing_scenario_selections(selection_id)
);
CREATE TABLE approved_tender_prices (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  pricing_scenario_id TEXT NOT NULL,
  pricing_scenario_version INTEGER NOT NULL CHECK (pricing_scenario_version BETWEEN 1 AND 32),
  scenario_manifest_sha256 TEXT NOT NULL CHECK (length(scenario_manifest_sha256) = 64),
  selection_id TEXT NOT NULL UNIQUE,
  strategy_approval_id TEXT NOT NULL,
  pricing_calculation_run_id TEXT NOT NULL,
  calculation_manifest_sha256 TEXT NOT NULL CHECK (length(calculation_manifest_sha256) = 64),
  final_amount TEXT NOT NULL CHECK (length(CAST(final_amount AS BLOB)) BETWEEN 1 AND 128),
  currency TEXT NOT NULL CHECK (length(currency) = 3),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  approved_by TEXT NOT NULL CHECK (approved_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'engineer_in_the_loop'),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (pricing_scenario_id, pricing_scenario_version)
    REFERENCES pricing_scenario_versions(pricing_scenario_id, version),
  FOREIGN KEY (selection_id) REFERENCES pricing_scenario_selections(selection_id),
  FOREIGN KEY (strategy_approval_id) REFERENCES commercial_strategy_approvals(approval_id),
  FOREIGN KEY (pricing_calculation_run_id)
    REFERENCES pricing_calculation_runs(pricing_calculation_run_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE source_artifacts (
  artifact_id TEXT PRIMARY KEY CHECK (length(artifact_id) = 32),
  intake_id TEXT NOT NULL,
  package_path TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY (intake_id) REFERENCES intake_runs(intake_id)
);
CREATE TABLE source_artifact_versions (
  artifact_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  language TEXT NOT NULL,
  document_type TEXT NOT NULL,
  media_type TEXT,
  sha256 TEXT,
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  registration_state TEXT NOT NULL CHECK (registration_state IN ('registered', 'exception')),
  exception_code TEXT,
  created_at TEXT NOT NULL,
  PRIMARY KEY (artifact_id, version),
  FOREIGN KEY (artifact_id) REFERENCES source_artifacts(artifact_id),
  FOREIGN KEY (sha256) REFERENCES content_objects(sha256),
  CHECK (
    (registration_state = 'registered' AND sha256 IS NOT NULL AND media_type IS NOT NULL AND exception_code IS NULL)
    OR
    (registration_state = 'exception' AND sha256 IS NULL AND media_type IS NULL AND exception_code IS NOT NULL)
  )
);
CREATE TABLE source_relationships (
  relationship_id TEXT PRIMARY KEY CHECK (length(relationship_id) = 32),
  prior_artifact_id TEXT NOT NULL,
  prior_version INTEGER NOT NULL CHECK (prior_version > 0),
  replacement_artifact_id TEXT NOT NULL,
  replacement_version INTEGER NOT NULL CHECK (replacement_version > 0),
  relationship_kind TEXT NOT NULL CHECK (relationship_kind IN ('addendum', 'replacement')),
  created_at TEXT NOT NULL,
  UNIQUE (prior_artifact_id, prior_version, replacement_artifact_id, replacement_version, relationship_kind),
  CHECK (prior_artifact_id != replacement_artifact_id OR prior_version != replacement_version),
  FOREIGN KEY (prior_artifact_id, prior_version)
    REFERENCES source_artifact_versions(artifact_id, version),
  FOREIGN KEY (replacement_artifact_id, replacement_version)
    REFERENCES source_artifact_versions(artifact_id, version)
);
CREATE TABLE change_assessments (
  assessment_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  assessment_id TEXT NOT NULL UNIQUE CHECK (length(assessment_id) = 32),
  relationship_id TEXT NOT NULL UNIQUE,
  lifecycle_before TEXT NOT NULL CHECK (lifecycle_before IN (
    'intake', 'bid_decision', 'tender_planning', 'active_production',
    'integrated_review', 'package_production', 'final_review'
  )),
  baseline_id TEXT,
  baseline_version INTEGER CHECK (baseline_version IS NULL OR baseline_version BETWEEN 1 AND 32),
  baseline_manifest_sha256 TEXT CHECK (
    baseline_manifest_sha256 IS NULL OR length(baseline_manifest_sha256) = 64
  ),
  affected_commitments_json TEXT NOT NULL CHECK (json_valid(affected_commitments_json)),
  proposed_rework_json TEXT NOT NULL CHECK (json_valid(proposed_rework_json)),
  unchanged_scope_json TEXT NOT NULL CHECK (json_valid(unchanged_scope_json)),
  deadline_effect TEXT NOT NULL CHECK (length(CAST(deadline_effect AS BLOB)) BETWEEN 1 AND 2000),
  approval_consequences_json TEXT NOT NULL CHECK (json_valid(approval_consequences_json)),
  manifest_json TEXT NOT NULL CHECK (
    json_valid(manifest_json) AND length(CAST(manifest_json AS BLOB)) <= 4194304
  ),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (relationship_id) REFERENCES source_relationships(relationship_id),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES coordinated_bid_baseline_versions(baseline_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence),
  CHECK (
    (baseline_id IS NULL AND baseline_version IS NULL AND baseline_manifest_sha256 IS NULL)
    OR (baseline_id IS NOT NULL AND baseline_version IS NOT NULL
        AND baseline_manifest_sha256 IS NOT NULL)
  )
);
CREATE TABLE change_assessment_impacts (
  assessment_id TEXT NOT NULL,
  impact_sequence INTEGER NOT NULL CHECK (impact_sequence BETWEEN 1 AND 4096),
  kind TEXT NOT NULL CHECK (kind IN (
    'tender_record', 'work_plan', 'production_task', 'agent_run', 'production_artifact',
    'tender_query', 'calculation_run', 'estimate', 'pricing_decision', 'review',
    'coordinated_baseline', 'package', 'approval'
  )),
  object_id TEXT NOT NULL CHECK (length(CAST(object_id AS BLOB)) BETWEEN 1 AND 200),
  object_version INTEGER NOT NULL CHECK (object_version >= 0),
  dependencies_json TEXT NOT NULL CHECK (
    json_valid(dependencies_json) AND length(CAST(dependencies_json AS BLOB)) <= 1048576
  ),
  consequence TEXT NOT NULL CHECK (consequence IN ('stale', 'reopen', 'revoke')),
  summary TEXT NOT NULL CHECK (length(CAST(summary AS BLOB)) BETWEEN 1 AND 500),
  PRIMARY KEY (assessment_id, impact_sequence),
  UNIQUE (assessment_id, kind, object_id, object_version),
  FOREIGN KEY (assessment_id) REFERENCES change_assessments(assessment_id)
);
CREATE TABLE change_assessment_decisions (
  assessment_id TEXT PRIMARY KEY,
  classification TEXT NOT NULL CHECK (classification IN ('irrelevant', 'material')),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  lifecycle_after TEXT NOT NULL CHECK (lifecycle_after IN (
    'intake', 'bid_decision', 'tender_planning', 'active_production',
    'integrated_review', 'package_production', 'final_review'
  )),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (assessment_id) REFERENCES change_assessments(assessment_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE change_assessment_resolutions (
  assessment_id TEXT PRIMARY KEY,
  resolution TEXT NOT NULL CHECK (
    resolution IN ('irrelevant', 'source_precedence', 'successor_baseline')
  ),
  baseline_id TEXT,
  baseline_version INTEGER CHECK (baseline_version IS NULL OR baseline_version BETWEEN 1 AND 32),
  baseline_manifest_sha256 TEXT CHECK (
    baseline_manifest_sha256 IS NULL OR length(baseline_manifest_sha256) = 64
  ),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (assessment_id) REFERENCES change_assessments(assessment_id),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES coordinated_bid_baseline_versions(baseline_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence),
  CHECK (
    (resolution IN ('irrelevant', 'source_precedence')
      AND baseline_id IS NULL AND baseline_version IS NULL
      AND baseline_manifest_sha256 IS NULL)
    OR (resolution = 'successor_baseline' AND baseline_id IS NOT NULL
      AND baseline_version IS NOT NULL AND baseline_manifest_sha256 IS NOT NULL)
  )
);
CREATE TABLE parse_attempts (
  attempt_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  attempt_id TEXT NOT NULL UNIQUE CHECK (length(attempt_id) = 32),
  artifact_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  status TEXT NOT NULL CHECK (status IN ('running', 'parsed', 'failed', 'interrupted', 'quarantined')),
  exception_code TEXT,
  started_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (artifact_id, version) REFERENCES source_artifact_versions(artifact_id, version),
  CHECK (
    (status = 'running' AND exception_code IS NULL AND completed_at IS NULL)
    OR (status = 'parsed' AND exception_code IS NULL AND completed_at IS NOT NULL)
    OR (status IN ('failed', 'interrupted', 'quarantined') AND exception_code IS NOT NULL AND completed_at IS NOT NULL)
  )
);
CREATE TABLE parsed_documents (
  artifact_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  attempt_id TEXT NOT NULL UNIQUE,
  pipeline_version TEXT NOT NULL,
  markdown_sha256 TEXT NOT NULL,
  language TEXT NOT NULL CHECK (language IN ('arabic', 'english', 'mixed', 'undetermined')),
  direction TEXT NOT NULL CHECK (direction IN ('left_to_right', 'right_to_left', 'mixed', 'neutral')),
  location_count INTEGER NOT NULL CHECK (location_count > 0),
  created_at TEXT NOT NULL,
  PRIMARY KEY (artifact_id, version),
  FOREIGN KEY (artifact_id, version) REFERENCES source_artifact_versions(artifact_id, version),
  FOREIGN KEY (attempt_id) REFERENCES parse_attempts(attempt_id),
  FOREIGN KEY (markdown_sha256) REFERENCES content_objects(sha256)
);
CREATE TABLE evidence_locations (
  artifact_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  kind TEXT NOT NULL CHECK (kind IN ('section', 'paragraph', 'table', 'sheet', 'cell')),
  structural_path TEXT NOT NULL,
  provenance_json TEXT NOT NULL,
  section TEXT,
  paragraph_number INTEGER,
  table_number INTEGER,
  sheet_name TEXT,
  cell_range TEXT,
  original_text TEXT NOT NULL,
  translated_text TEXT,
  language TEXT NOT NULL CHECK (language IN ('arabic', 'english', 'mixed', 'undetermined')),
  direction TEXT NOT NULL CHECK (direction IN ('left_to_right', 'right_to_left', 'mixed', 'neutral')),
  PRIMARY KEY (artifact_id, version, ordinal),
  FOREIGN KEY (artifact_id, version) REFERENCES parsed_documents(artifact_id, version)
);
CREATE VIRTUAL TABLE evidence_fts USING fts5(
  original_text,
  artifact_id UNINDEXED,
  version UNINDEXED,
  ordinal UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 0'
);
CREATE TABLE evidence_embeddings (
  embedding_id INTEGER PRIMARY KEY AUTOINCREMENT,
  artifact_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  UNIQUE (artifact_id, version, ordinal),
  FOREIGN KEY (artifact_id, version, ordinal)
    REFERENCES evidence_locations(artifact_id, version, ordinal)
);
CREATE VIRTUAL TABLE evidence_embedding_vectors USING vec0(
  embedding float[384] distance_metric=cosine
);
CREATE TABLE agent_profiles (
  profile_id TEXT PRIMARY KEY CHECK (length(profile_id) = 32),
  stable_identity TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL
);
CREATE TABLE agent_profile_versions (
  profile_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  identity TEXT NOT NULL,
  profession TEXT NOT NULL,
  seniority TEXT NOT NULL,
  capabilities_json TEXT NOT NULL CHECK (json_valid(capabilities_json)),
  objective TEXT NOT NULL,
  behavior TEXT NOT NULL,
  skepticism TEXT NOT NULL,
  risk_tolerance TEXT NOT NULL,
  instructions TEXT NOT NULL,
  output_contract_json TEXT NOT NULL CHECK (json_valid(output_contract_json)),
  review_policy TEXT NOT NULL,
  permissions_json TEXT NOT NULL CHECK (json_valid(permissions_json)),
  prohibited_actions_json TEXT NOT NULL CHECK (json_valid(prohibited_actions_json)),
  resource_budget_json TEXT NOT NULL CHECK (json_valid(resource_budget_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY (profile_id, version),
  FOREIGN KEY (profile_id) REFERENCES agent_profiles(profile_id)
);
CREATE TABLE agent_profile_heads (
  profile_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version > 0),
  status TEXT NOT NULL CHECK (status IN ('proposed', 'active', 'suspended', 'retired')),
  FOREIGN KEY (profile_id, current_version)
    REFERENCES agent_profile_versions(profile_id, version)
);
CREATE TABLE tender_tasks (
  task_id TEXT PRIMARY KEY CHECK (length(task_id) = 32),
  profile_id TEXT NOT NULL,
  profile_version INTEGER NOT NULL CHECK (profile_version > 0),
  objective TEXT NOT NULL,
  exact_inputs_json TEXT NOT NULL CHECK (json_valid(exact_inputs_json)),
  output_contract_json TEXT NOT NULL CHECK (json_valid(output_contract_json)),
  review_policy TEXT NOT NULL,
  deadline TEXT NOT NULL,
  permissions_json TEXT NOT NULL CHECK (json_valid(permissions_json)),
  resource_budget_json TEXT NOT NULL CHECK (json_valid(resource_budget_json)),
  repair_feedback_json TEXT CHECK (
    repair_feedback_json IS NULL OR json_valid(repair_feedback_json)
  ),
  created_at TEXT NOT NULL,
  FOREIGN KEY (profile_id, profile_version)
    REFERENCES agent_profile_versions(profile_id, version)
);
CREATE TABLE provider_threads (
  profile_id TEXT NOT NULL,
  profile_version INTEGER NOT NULL CHECK (profile_version > 0),
  thread_ref TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK (status IN ('active', 'archive_pending', 'archived')),
  created_at TEXT NOT NULL,
  archived_at TEXT,
  PRIMARY KEY (thread_ref),
  FOREIGN KEY (profile_id, profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  CHECK (
    (status IN ('active', 'archive_pending') AND archived_at IS NULL)
    OR (status = 'archived' AND archived_at IS NOT NULL)
  )
);
CREATE UNIQUE INDEX provider_threads_one_active_profile_version
ON provider_threads(profile_id, profile_version)
WHERE status IN ('active', 'archive_pending');
CREATE TABLE provider_thread_exposures (
  thread_ref TEXT NOT NULL,
  run_id TEXT NOT NULL UNIQUE,
  exposure_json TEXT NOT NULL CHECK (json_valid(exposure_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY (thread_ref, run_id),
  FOREIGN KEY (thread_ref) REFERENCES provider_threads(thread_ref),
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE agent_runs (
  run_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id TEXT NOT NULL UNIQUE CHECK (length(run_id) = 32),
  task_id TEXT NOT NULL,
  profile_id TEXT NOT NULL,
  profile_version INTEGER NOT NULL CHECK (profile_version > 0),
  retry_of_run_id TEXT,
  permission_grant_json TEXT NOT NULL CHECK (json_valid(permission_grant_json)),
  status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'interrupted', 'failed', 'indeterminate')),
  provider_thread_ref TEXT,
  provider_turn_ref TEXT,
  usage_json TEXT CHECK (usage_json IS NULL OR json_valid(usage_json)),
  failure_json TEXT CHECK (failure_json IS NULL OR json_valid(failure_json)),
  started_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (task_id) REFERENCES tender_tasks(task_id),
  FOREIGN KEY (profile_id, profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (retry_of_run_id) REFERENCES agent_runs(run_id),
  CHECK (
    (status = 'running' AND completed_at IS NULL AND failure_json IS NULL)
    OR
    (status = 'completed' AND completed_at IS NOT NULL AND provider_thread_ref IS NOT NULL
      AND provider_turn_ref IS NOT NULL AND failure_json IS NULL)
    OR
    (status IN ('interrupted', 'failed', 'indeterminate') AND completed_at IS NOT NULL
      AND failure_json IS NOT NULL)
  )
);
CREATE TABLE agent_run_provider_bindings (
  run_id TEXT PRIMARY KEY,
  binding_json TEXT NOT NULL CHECK (json_valid(binding_json)),
  recorded_at TEXT NOT NULL,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
);
CREATE UNIQUE INDEX agent_runs_one_direct_retry
ON agent_runs(retry_of_run_id) WHERE retry_of_run_id IS NOT NULL;
CREATE TABLE agent_run_recovery_dispositions (
  run_id TEXT PRIMARY KEY,
  disposition TEXT NOT NULL CHECK (disposition IN ('retry_task', 'close_task')),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 500),
  decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
  decided_at TEXT NOT NULL,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE agent_access_requests (
  request_id TEXT PRIMARY KEY CHECK (length(request_id) = 32),
  run_id TEXT NOT NULL,
  request_json TEXT NOT NULL CHECK (json_valid(request_json)),
  status TEXT NOT NULL CHECK (status IN ('blocked', 'approved', 'denied', 'superseded')),
  decision_json TEXT CHECK (decision_json IS NULL OR json_valid(decision_json)),
  denial_reason TEXT CHECK (denial_reason IS NULL OR denial_reason IN (
    'default_deny', 'prohibited_action', 'grant_expired', 'secret_data',
    'outside_ceiling', 'work_plan_amendment_required', 'tool_not_granted',
    'thread_exposure_incompatible', 'engineer_denied', 'superseded'
  )),
  requested_at TEXT NOT NULL,
  decided_at TEXT,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id),
  CHECK (
    (status = 'blocked' AND decision_json IS NULL AND denial_reason IS NULL
      AND decided_at IS NULL)
    OR (status = 'approved' AND decision_json IS NOT NULL AND denial_reason IS NULL
      AND decided_at IS NOT NULL)
    OR (status = 'denied' AND decision_json IS NULL AND denial_reason IS NOT NULL
      AND decided_at IS NOT NULL)
    OR (status = 'superseded' AND decision_json IS NULL AND denial_reason = 'superseded'
      AND decided_at IS NOT NULL)
  )
);
CREATE TABLE agent_access_revocations (
  request_id TEXT PRIMARY KEY,
  reason TEXT NOT NULL CHECK (reason IN ('engineer_revoked', 'run_interrupted')),
  revoked_by TEXT NOT NULL,
  revoked_at TEXT NOT NULL,
  FOREIGN KEY (request_id) REFERENCES agent_access_requests(request_id)
);
CREATE TABLE agent_run_cancellations (
  run_id TEXT PRIMARY KEY,
  requested_by TEXT NOT NULL,
  requested_at TEXT NOT NULL,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE agent_tool_call_reservations (
  run_id TEXT NOT NULL,
  correlation_id TEXT NOT NULL,
  tool_name TEXT NOT NULL,
  approval_id TEXT NOT NULL,
  authorized_at TEXT NOT NULL,
  PRIMARY KEY (run_id, correlation_id),
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE agent_tool_call_results (
  run_id TEXT NOT NULL,
  correlation_id TEXT NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'failed')),
  completed_at TEXT NOT NULL,
  PRIMARY KEY (run_id, correlation_id),
  FOREIGN KEY (run_id, correlation_id)
    REFERENCES agent_tool_call_reservations(run_id, correlation_id)
);
CREATE TABLE provider_events (
  run_id TEXT NOT NULL,
  sequence INTEGER NOT NULL CHECK (sequence > 0),
  kind TEXT NOT NULL CHECK (kind IN (
    'run_started', 'thread_established', 'thread_resumed', 'turn_requested',
    'turn_started',
    'usage_observed', 'rate_limit_observed', 'control_request_resolved',
    'control_request_denied', 'warning', 'candidate_validated', 'candidate_rejected',
    'result_committed', 'terminal'
  )),
  summary TEXT NOT NULL,
  correlation_id TEXT,
  request_fingerprint TEXT,
  denial_reason TEXT CHECK (denial_reason IS NULL OR denial_reason IN (
    'default_deny', 'prohibited_action', 'grant_expired', 'secret_data',
    'outside_ceiling', 'work_plan_amendment_required', 'tool_not_granted',
    'thread_exposure_incompatible', 'engineer_denied', 'superseded', 'access_revoked'
  )),
  opaque_reference TEXT,
  created_at TEXT NOT NULL,
  PRIMARY KEY (run_id, sequence),
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id),
  CHECK (
    (kind = 'control_request_denied' AND correlation_id IS NOT NULL
      AND length(request_fingerprint) = 64 AND denial_reason IS NOT NULL)
    OR (kind = 'control_request_resolved' AND correlation_id IS NOT NULL
      AND request_fingerprint IS NULL AND denial_reason IS NULL)
    OR (kind NOT IN ('control_request_denied', 'control_request_resolved')
      AND correlation_id IS NULL
      AND request_fingerprint IS NULL AND denial_reason IS NULL)
  )
);
CREATE UNIQUE INDEX provider_control_requests_one_correlation
ON provider_events(run_id, correlation_id)
WHERE correlation_id IS NOT NULL;
CREATE TABLE proposed_agent_results (
  result_id TEXT PRIMARY KEY CHECK (length(result_id) = 32),
  run_id TEXT NOT NULL UNIQUE,
  verification_status TEXT NOT NULL CHECK (verification_status = 'proposed'),
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  data_scopes_json TEXT NOT NULL CHECK (json_valid(data_scopes_json)),
  data_classification TEXT NOT NULL CHECK (data_classification IN ('tender_internal', 'sensitive')),
  created_at TEXT NOT NULL,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE agent_run_rejected_outputs (
  run_id TEXT PRIMARY KEY,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  payload_sha256 TEXT NOT NULL CHECK (length(payload_sha256) = 64),
  validation_issues_json TEXT NOT NULL CHECK (json_valid(validation_issues_json)),
  created_at TEXT NOT NULL,
  FOREIGN KEY (run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE tender_record_authorities (
  authority_id TEXT PRIMARY KEY CHECK (length(authority_id) = 32),
  kind TEXT NOT NULL CHECK (kind IN ('engineer_entry', 'calculation_run')),
  value TEXT NOT NULL CHECK (length(CAST(value AS BLOB)) BETWEEN 1 AND 4000),
  description TEXT NOT NULL CHECK (length(CAST(description AS BLOB)) BETWEEN 1 AND 2000),
  manifest_sha256 TEXT,
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  created_by TEXT NOT NULL CHECK (length(CAST(created_by AS BLOB)) BETWEEN 1 AND 200),
  created_at TEXT NOT NULL,
  CHECK (
    (kind = 'engineer_entry' AND manifest_sha256 IS NULL)
    OR (kind = 'calculation_run' AND length(manifest_sha256) = 64)
  ),
  FOREIGN KEY (tender_revision) REFERENCES tender_revisions(revision)
);
CREATE TABLE tender_records (
  record_id TEXT PRIMARY KEY CHECK (length(record_id) = 32),
  stable_key TEXT NOT NULL UNIQUE CHECK (length(CAST(stable_key AS BLOB)) BETWEEN 1 AND 100),
  created_at TEXT NOT NULL
);
CREATE TABLE tender_record_versions (
  record_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  kind TEXT NOT NULL CHECK (kind IN (
    'requirement', 'evaluation_criterion', 'deliverable', 'deadline', 'form',
    'clause', 'risk', 'assumption', 'tender_query', 'project_characteristic'
  )),
  title TEXT NOT NULL CHECK (length(CAST(title AS BLOB)) BETWEEN 1 AND 500),
  generation_instruction_json TEXT CHECK (
    generation_instruction_json IS NULL OR (
      json_valid(generation_instruction_json)
      AND length(CAST(generation_instruction_json AS BLOB)) <= 65536
    )
  ),
  fields_json TEXT NOT NULL CHECK (json_valid(fields_json)),
  contradictions_json TEXT NOT NULL CHECK (json_valid(contradictions_json)),
  author_run_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (record_id, version),
  FOREIGN KEY (record_id) REFERENCES tender_records(record_id),
  FOREIGN KEY (author_run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE tender_record_heads (
  record_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version > 0),
  FOREIGN KEY (record_id, current_version)
    REFERENCES tender_record_versions(record_id, version)
);
CREATE TABLE tender_record_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  record_id TEXT NOT NULL,
  record_version INTEGER NOT NULL CHECK (record_version > 0),
  reviewer_kind TEXT NOT NULL CHECK (reviewer_kind IN ('independent_reviewer', 'engineer_user')),
  reviewer_run_id TEXT,
  outcome TEXT NOT NULL CHECK (outcome IN ('verified', 'rejected', 'approved_assumption')),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  decided_by TEXT NOT NULL CHECK (length(CAST(decided_by AS BLOB)) BETWEEN 1 AND 200),
  created_at TEXT NOT NULL,
  FOREIGN KEY (record_id, record_version)
    REFERENCES tender_record_versions(record_id, version),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id),
  CHECK (
    (reviewer_kind = 'independent_reviewer' AND reviewer_run_id IS NOT NULL
      AND outcome IN ('verified', 'rejected'))
    OR
    (reviewer_kind = 'engineer_user' AND reviewer_run_id IS NULL)
  )
);
CREATE TABLE bid_decision_packages (
  package_id TEXT PRIMARY KEY CHECK (length(package_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE bid_decision_package_versions (
  package_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  record_inventory_json TEXT NOT NULL CHECK (json_valid(record_inventory_json)),
  record_inventory_sha256 TEXT NOT NULL CHECK (length(record_inventory_sha256) = 64),
  capability_demands_json TEXT NOT NULL CHECK (json_valid(capability_demands_json)),
  resource_implications_json TEXT NOT NULL CHECK (json_valid(resource_implications_json)),
  recommendation_json TEXT NOT NULL CHECK (json_valid(recommendation_json)),
  analysis_blocker_count INTEGER NOT NULL CHECK (analysis_blocker_count >= 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  created_by TEXT NOT NULL CHECK (created_by = 'engineer_user'),
  created_at TEXT NOT NULL,
  PRIMARY KEY (package_id, version),
  FOREIGN KEY (package_id) REFERENCES bid_decision_packages(package_id),
  FOREIGN KEY (tender_revision) REFERENCES tender_revisions(revision)
);
CREATE TABLE bid_decision_package_heads (
  package_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version > 0),
  FOREIGN KEY (package_id, current_version)
    REFERENCES bid_decision_package_versions(package_id, version)
);
CREATE TABLE bid_compliance_rows (
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version > 0),
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  record_id TEXT NOT NULL,
  record_version INTEGER NOT NULL CHECK (record_version > 0),
  disposition TEXT NOT NULL CHECK (disposition IN (
    'comply', 'comply_with_qualification', 'deviation', 'not_applicable', 'unresolved'
  )),
  responsibility TEXT NOT NULL CHECK (length(CAST(responsibility AS BLOB)) BETWEEN 1 AND 200),
  planned_treatment TEXT NOT NULL CHECK (length(CAST(planned_treatment AS BLOB)) BETWEEN 1 AND 2000),
  affected_work_json TEXT NOT NULL CHECK (json_valid(affected_work_json)),
  uncertainty TEXT CHECK (uncertainty IS NULL OR length(CAST(uncertainty AS BLOB)) BETWEEN 1 AND 2000),
  related_records_json TEXT NOT NULL CHECK (json_valid(related_records_json)),
  verification_status TEXT NOT NULL CHECK (verification_status IN (
    'proposed', 'verified', 'rejected', 'stale', 'superseded'
  )),
  trust_class TEXT NOT NULL CHECK (trust_class IN (
    'ai_proposal', 'deterministic_fact', 'verified', 'engineer_verified',
    'approved_assumption', 'unresolved_gap', 'prior_decision'
  )),
  evidence_count INTEGER NOT NULL CHECK (evidence_count >= 0),
  blocker_codes_json TEXT NOT NULL CHECK (json_valid(blocker_codes_json)),
  PRIMARY KEY (package_id, package_version, ordinal),
  UNIQUE (package_id, package_version, record_id, record_version),
  FOREIGN KEY (package_id, package_version)
    REFERENCES bid_decision_package_versions(package_id, version),
  FOREIGN KEY (record_id, record_version)
    REFERENCES tender_record_versions(record_id, version)
);
CREATE TABLE bid_decision_package_record_bindings (
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version > 0),
  category TEXT NOT NULL CHECK (category IN (
    'project_fingerprint', 'risk', 'opportunity', 'assumption', 'unresolved_query'
  )),
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  record_id TEXT NOT NULL,
  record_version INTEGER NOT NULL CHECK (record_version > 0),
  PRIMARY KEY (package_id, package_version, category, ordinal),
  UNIQUE (package_id, package_version, category, record_id, record_version),
  FOREIGN KEY (package_id, package_version)
    REFERENCES bid_decision_package_versions(package_id, version),
  FOREIGN KEY (record_id, record_version)
    REFERENCES tender_record_versions(record_id, version)
);
CREATE TABLE bid_decision_package_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version > 0),
  reviewer_run_id TEXT NOT NULL UNIQUE,
  outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed')),
  findings_json TEXT NOT NULL CHECK (json_valid(findings_json)),
  created_at TEXT NOT NULL,
  UNIQUE (package_id, package_version),
  FOREIGN KEY (package_id, package_version)
    REFERENCES bid_decision_package_versions(package_id, version),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id)
);
CREATE TABLE bid_decision_approval_records (
  approval_sequence INTEGER PRIMARY KEY CHECK (approval_sequence > 0),
  approval_id TEXT NOT NULL UNIQUE CHECK (length(approval_id) = 32),
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version > 0),
  package_manifest_sha256 TEXT NOT NULL CHECK (length(package_manifest_sha256) = 64),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  decision TEXT NOT NULL CHECK (decision IN ('accept', 'return', 'reject')),
  decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  evidence_count INTEGER NOT NULL CHECK (evidence_count >= 0),
  evidence_sha256 TEXT NOT NULL CHECK (length(evidence_sha256) = 64),
  conditions_json TEXT NOT NULL CHECK (json_valid(conditions_json)),
  exceptions_json TEXT NOT NULL CHECK (json_valid(exceptions_json)),
  required_rework_json TEXT NOT NULL CHECK (json_valid(required_rework_json)),
  lifecycle_before TEXT NOT NULL CHECK (lifecycle_before = 'bid_decision'),
  lifecycle_after TEXT NOT NULL CHECK (lifecycle_after IN (
    'bid_decision', 'tender_planning', 'declined'
  )),
  consequence TEXT NOT NULL CHECK (length(CAST(consequence AS BLOB)) BETWEEN 1 AND 2000),
  preceding_approval_hash TEXT NOT NULL CHECK (length(preceding_approval_hash) = 64),
  approval_manifest_json TEXT NOT NULL CHECK (json_valid(approval_manifest_json)),
  approval_sha256 TEXT NOT NULL UNIQUE CHECK (length(approval_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (package_id, package_version),
  FOREIGN KEY (package_id, package_version)
    REFERENCES bid_decision_package_versions(package_id, version),
  FOREIGN KEY (tender_revision) REFERENCES tender_revisions(revision)
);
CREATE TABLE bid_decision_return_rework_dispositions (
  disposition_id TEXT PRIMARY KEY CHECK (length(disposition_id) = 32),
  approval_id TEXT NOT NULL UNIQUE,
  approval_sha256 TEXT NOT NULL CHECK (length(approval_sha256) = 64),
  items_json TEXT NOT NULL CHECK (json_valid(items_json)),
  resolved_by TEXT NOT NULL CHECK (resolved_by = 'engineer_user'),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (approval_id) REFERENCES bid_decision_approval_records(approval_id)
);
CREATE TABLE bid_decision_approval_invalidations (
  invalidation_id TEXT PRIMARY KEY CHECK (length(invalidation_id) = 32),
  approval_id TEXT NOT NULL UNIQUE,
  approval_sha256 TEXT NOT NULL CHECK (length(approval_sha256) = 64),
  material_change_summary TEXT NOT NULL
    CHECK (length(CAST(material_change_summary AS BLOB)) BETWEEN 1 AND 4000),
  affected_areas_json TEXT NOT NULL CHECK (json_valid(affected_areas_json)),
  changed_records_json TEXT NOT NULL CHECK (json_valid(changed_records_json)),
  invalidated_by TEXT NOT NULL CHECK (invalidated_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  lifecycle_before TEXT NOT NULL CHECK (lifecycle_before IN (
    'tender_planning', 'active_production', 'integrated_review', 'package_production',
    'final_review'
  )),
  lifecycle_after TEXT NOT NULL CHECK (lifecycle_after = 'bid_decision'),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (approval_id) REFERENCES bid_decision_approval_records(approval_id)
);
CREATE TABLE work_plans (
  plan_id TEXT PRIMARY KEY CHECK (length(plan_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE work_plan_versions (
  plan_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version > 0),
  bid_package_id TEXT NOT NULL,
  bid_package_version INTEGER NOT NULL CHECK (bid_package_version > 0),
  bid_package_manifest_sha256 TEXT NOT NULL CHECK (length(bid_package_manifest_sha256) = 64),
  capability_catalogue_version INTEGER NOT NULL CHECK (capability_catalogue_version = 1),
  permission_policy_version INTEGER NOT NULL CHECK (permission_policy_version = 1),
  profiles_json TEXT NOT NULL CHECK (json_valid(profiles_json)),
  workstreams_json TEXT NOT NULL CHECK (json_valid(workstreams_json)),
  tasks_json TEXT NOT NULL CHECK (json_valid(tasks_json)),
  query_bindings_json TEXT NOT NULL CHECK (json_valid(query_bindings_json)),
  capability_gaps_json TEXT NOT NULL CHECK (json_valid(capability_gaps_json)),
  blocker_codes_json TEXT NOT NULL CHECK (json_valid(blocker_codes_json)),
  revision_actions_json TEXT NOT NULL CHECK (json_valid(revision_actions_json)),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_by TEXT NOT NULL CHECK (created_by = 'engineer_user'),
  created_at TEXT NOT NULL,
  PRIMARY KEY (plan_id, version),
  FOREIGN KEY (plan_id) REFERENCES work_plans(plan_id),
  FOREIGN KEY (bid_package_id, bid_package_version)
    REFERENCES bid_decision_package_versions(package_id, version)
);
CREATE TABLE work_plan_heads (
  plan_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version > 0),
  FOREIGN KEY (plan_id, current_version)
    REFERENCES work_plan_versions(plan_id, version)
);
CREATE TABLE work_plan_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  plan_id TEXT NOT NULL,
  plan_version INTEGER NOT NULL CHECK (plan_version > 0),
  decision TEXT NOT NULL CHECK (decision IN ('approve', 'return', 'reject')),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  plan_manifest_sha256 TEXT NOT NULL CHECK (length(plan_manifest_sha256) = 64),
  decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  approval_manifest_json TEXT NOT NULL CHECK (json_valid(approval_manifest_json)),
  approval_sha256 TEXT NOT NULL UNIQUE CHECK (length(approval_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (plan_id, plan_version),
  FOREIGN KEY (plan_id, plan_version)
    REFERENCES work_plan_versions(plan_id, version)
);
CREATE TABLE production_activations (
  activation_id TEXT PRIMARY KEY CHECK (length(activation_id) = 32),
  plan_id TEXT NOT NULL,
  plan_version INTEGER NOT NULL CHECK (plan_version > 0),
  plan_manifest_sha256 TEXT NOT NULL CHECK (length(plan_manifest_sha256) = 64),
  status TEXT NOT NULL CHECK (status IN ('active', 'suspended', 'superseded')),
  activated_by TEXT NOT NULL CHECK (activated_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  created_at TEXT NOT NULL,
  UNIQUE (plan_id, plan_version),
  FOREIGN KEY (plan_id, plan_version)
    REFERENCES work_plan_versions(plan_id, version)
);
CREATE UNIQUE INDEX production_activations_one_active
ON production_activations(status) WHERE status = 'active';
CREATE TABLE production_tasks (
  production_task_id TEXT PRIMARY KEY CHECK (length(production_task_id) = 32),
  activation_id TEXT NOT NULL,
  task_key TEXT NOT NULL CHECK (length(CAST(task_key AS BLOB)) BETWEEN 1 AND 200),
  task_definition_json TEXT NOT NULL CHECK (json_valid(task_definition_json)),
  task_definition_sha256 TEXT NOT NULL CHECK (length(task_definition_sha256) = 64),
  task_id TEXT UNIQUE,
  status TEXT NOT NULL CHECK (status IN (
    'blocked', 'ready', 'running', 'review_ready', 'reviewing', 'remediation_ready',
    'query_blocked', 'ready_for_integration', 'attempt_limit_reached', 'failed',
    'cancelled', 'indeterminate', 'suspended'
  )),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (activation_id, task_key),
  FOREIGN KEY (activation_id) REFERENCES production_activations(activation_id),
  FOREIGN KEY (task_id) REFERENCES tender_tasks(task_id)
);
CREATE TABLE production_task_attempts (
  production_task_id TEXT NOT NULL,
  attempt_number INTEGER NOT NULL CHECK (attempt_number BETWEEN 1 AND 8),
  attempt_kind TEXT NOT NULL CHECK (
    attempt_kind IN ('author', 'review', 'remediation', 'query_control')
  ),
  task_id TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  PRIMARY KEY (production_task_id, attempt_number),
  FOREIGN KEY (production_task_id) REFERENCES production_tasks(production_task_id),
  FOREIGN KEY (task_id) REFERENCES tender_tasks(task_id)
);
CREATE TABLE production_artifact_versions (
  artifact_id TEXT NOT NULL CHECK (length(artifact_id) = 32),
  production_task_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 8),
  author_run_id TEXT NOT NULL UNIQUE,
  prior_version INTEGER,
  remediation_review_id TEXT,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  payload_sha256 TEXT NOT NULL CHECK (length(payload_sha256) = 64),
  output_validation_passed INTEGER NOT NULL CHECK (output_validation_passed = 1),
  evidence_verified INTEGER NOT NULL CHECK (evidence_verified = 1),
  data_scopes_json TEXT NOT NULL CHECK (json_valid(data_scopes_json)),
  data_classifications_json TEXT NOT NULL CHECK (json_valid(data_classifications_json)),
  created_at TEXT NOT NULL,
  PRIMARY KEY (artifact_id, version),
  UNIQUE (production_task_id, version),
  FOREIGN KEY (production_task_id) REFERENCES production_tasks(production_task_id),
  FOREIGN KEY (author_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (artifact_id, prior_version)
    REFERENCES production_artifact_versions(artifact_id, version),
  FOREIGN KEY (remediation_review_id) REFERENCES production_reviews(review_id)
);
CREATE TABLE production_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  production_task_id TEXT NOT NULL,
  target_artifact_id TEXT NOT NULL CHECK (length(target_artifact_id) = 32),
  target_version INTEGER NOT NULL CHECK (target_version BETWEEN 1 AND 8),
  target_payload_sha256 TEXT NOT NULL CHECK (length(target_payload_sha256) = 64),
  reviewer_run_id TEXT NOT NULL UNIQUE,
  reviewer_profile_id TEXT NOT NULL CHECK (length(reviewer_profile_id) = 32),
  reviewer_profile_version INTEGER NOT NULL CHECK (reviewer_profile_version > 0),
  capability TEXT NOT NULL CHECK (length(CAST(capability AS BLOB)) BETWEEN 1 AND 100),
  scope_json TEXT NOT NULL CHECK (json_valid(scope_json)),
  criteria_json TEXT NOT NULL CHECK (json_valid(criteria_json)),
  inputs_json TEXT NOT NULL CHECK (json_valid(inputs_json)),
  result TEXT NOT NULL CHECK (result IN ('satisfied', 'requires_remediation')),
  resolved_finding_ids_json TEXT NOT NULL CHECK (json_valid(resolved_finding_ids_json)),
  created_at TEXT NOT NULL,
  UNIQUE (target_artifact_id, target_version),
  FOREIGN KEY (production_task_id) REFERENCES production_tasks(production_task_id),
  FOREIGN KEY (target_artifact_id, target_version)
    REFERENCES production_artifact_versions(artifact_id, version),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (reviewer_profile_id, reviewer_profile_version)
    REFERENCES agent_profile_versions(profile_id, version)
);
CREATE TABLE production_review_findings (
  finding_id TEXT PRIMARY KEY CHECK (length(finding_id) = 32),
  review_id TEXT NOT NULL,
  finding_sequence INTEGER NOT NULL CHECK (finding_sequence BETWEEN 1 AND 32),
  severity TEXT NOT NULL CHECK (severity IN ('critical', 'major', 'minor')),
  summary TEXT NOT NULL CHECK (length(CAST(summary AS BLOB)) BETWEEN 1 AND 4000),
  evidence_references_json TEXT NOT NULL CHECK (json_valid(evidence_references_json)),
  created_at TEXT NOT NULL,
  UNIQUE (review_id, finding_sequence),
  FOREIGN KEY (review_id) REFERENCES production_reviews(review_id)
);
CREATE TABLE production_finding_dispositions (
  disposition_id TEXT PRIMARY KEY CHECK (length(disposition_id) = 32),
  finding_id TEXT NOT NULL UNIQUE,
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  disposition TEXT NOT NULL CHECK (disposition IN ('remediation_verified', 'exception_approved')),
  target_artifact_id TEXT NOT NULL CHECK (length(target_artifact_id) = 32),
  target_version INTEGER NOT NULL CHECK (target_version BETWEEN 1 AND 8),
  verifying_review_id TEXT,
  decided_by TEXT NOT NULL CHECK (decided_by IN ('host_policy', 'engineer_user')),
  acting_role TEXT NOT NULL CHECK (acting_role IN ('integration_gate', 'tendering_manager')),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  consequence TEXT NOT NULL CHECK (length(CAST(consequence AS BLOB)) BETWEEN 1 AND 4000),
  created_at TEXT NOT NULL,
  FOREIGN KEY (finding_id) REFERENCES production_review_findings(finding_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence),
  FOREIGN KEY (target_artifact_id, target_version)
    REFERENCES production_artifact_versions(artifact_id, version),
  FOREIGN KEY (verifying_review_id) REFERENCES production_reviews(review_id)
);
CREATE TABLE production_integration_readiness (
  readiness_id TEXT PRIMARY KEY CHECK (length(readiness_id) = 32),
  production_task_id TEXT NOT NULL,
  artifact_id TEXT NOT NULL CHECK (length(artifact_id) = 32),
  artifact_version INTEGER NOT NULL CHECK (artifact_version BETWEEN 1 AND 8),
  payload_sha256 TEXT NOT NULL CHECK (length(payload_sha256) = 64),
  review_id TEXT,
  output_validation_passed INTEGER NOT NULL CHECK (output_validation_passed = 1),
  evidence_verified INTEGER NOT NULL CHECK (evidence_verified = 1),
  dependencies_satisfied INTEGER NOT NULL CHECK (dependencies_satisfied = 1),
  approval_gates_json TEXT NOT NULL CHECK (json_valid(approval_gates_json)),
  finding_dispositions_sha256 TEXT NOT NULL CHECK (length(finding_dispositions_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (production_task_id, artifact_id, artifact_version),
  FOREIGN KEY (production_task_id) REFERENCES production_tasks(production_task_id),
  FOREIGN KEY (artifact_id, artifact_version)
    REFERENCES production_artifact_versions(artifact_id, version),
  FOREIGN KEY (review_id) REFERENCES production_reviews(review_id)
);
CREATE TABLE production_task_carry_forwards (
  carry_forward_id TEXT PRIMARY KEY CHECK (length(carry_forward_id) = 32),
  assessment_id TEXT NOT NULL,
  source_production_task_id TEXT NOT NULL,
  target_production_task_id TEXT NOT NULL UNIQUE,
  source_readiness_id TEXT NOT NULL,
  target_readiness_id TEXT NOT NULL UNIQUE,
  source_artifact_id TEXT NOT NULL CHECK (length(source_artifact_id) = 32),
  source_artifact_version INTEGER NOT NULL CHECK (source_artifact_version BETWEEN 1 AND 8),
  source_review_id TEXT,
  source_plan_manifest_sha256 TEXT NOT NULL CHECK (length(source_plan_manifest_sha256) = 64),
  target_plan_manifest_sha256 TEXT NOT NULL CHECK (length(target_plan_manifest_sha256) = 64),
  compatibility_sha256 TEXT NOT NULL CHECK (length(compatibility_sha256) = 64),
  manifest_json TEXT NOT NULL CHECK (
    json_valid(manifest_json) AND length(CAST(manifest_json AS BLOB)) <= 1048576
  ),
  manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  UNIQUE (assessment_id, source_production_task_id),
  FOREIGN KEY (assessment_id) REFERENCES change_assessments(assessment_id),
  FOREIGN KEY (source_production_task_id) REFERENCES production_tasks(production_task_id),
  FOREIGN KEY (target_production_task_id) REFERENCES production_tasks(production_task_id),
  FOREIGN KEY (source_readiness_id) REFERENCES production_integration_readiness(readiness_id),
  FOREIGN KEY (target_readiness_id) REFERENCES production_integration_readiness(readiness_id),
  FOREIGN KEY (source_artifact_id, source_artifact_version)
    REFERENCES production_artifact_versions(artifact_id, version),
  FOREIGN KEY (source_review_id) REFERENCES production_reviews(review_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE coordinated_bid_baselines (
  baseline_id TEXT PRIMARY KEY CHECK (length(baseline_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE coordinated_bid_baseline_versions (
  baseline_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  activation_id TEXT NOT NULL,
  plan_id TEXT NOT NULL,
  plan_version INTEGER NOT NULL CHECK (plan_version > 0),
  plan_manifest_sha256 TEXT NOT NULL CHECK (length(plan_manifest_sha256) = 64),
  coordinator_profile_id TEXT NOT NULL CHECK (length(coordinator_profile_id) = 32),
  coordinator_profile_version INTEGER NOT NULL CHECK (coordinator_profile_version > 0),
  bindings_json TEXT NOT NULL CHECK (
    json_valid(bindings_json) AND length(CAST(bindings_json AS BLOB)) <= 4194304
  ),
  contradictions_json TEXT NOT NULL CHECK (
    json_valid(contradictions_json) AND length(CAST(contradictions_json AS BLOB)) <= 1048576
  ),
  blockers_json TEXT NOT NULL CHECK (
    json_valid(blockers_json) AND length(CAST(blockers_json AS BLOB)) <= 1048576
  ),
  explanation TEXT NOT NULL CHECK (length(CAST(explanation AS BLOB)) BETWEEN 1 AND 8000),
  preceding_version_manifest_sha256 TEXT,
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (
    json_valid(manifest_json) AND length(CAST(manifest_json AS BLOB)) <= 4194304
  ),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (baseline_id, version),
  FOREIGN KEY (baseline_id) REFERENCES coordinated_bid_baselines(baseline_id),
  FOREIGN KEY (activation_id) REFERENCES production_activations(activation_id),
  FOREIGN KEY (plan_id, plan_version) REFERENCES work_plan_versions(plan_id, version),
  FOREIGN KEY (coordinator_profile_id, coordinator_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence),
  CHECK (
    (version = 1 AND preceding_version_manifest_sha256 IS NULL)
    OR (version > 1 AND length(preceding_version_manifest_sha256) = 64)
  )
);
CREATE TABLE coordinated_bid_baseline_head (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  baseline_id TEXT NOT NULL,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (baseline_id, current_version)
    REFERENCES coordinated_bid_baseline_versions(baseline_id, version)
);
CREATE TABLE coordinated_bid_baseline_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  baseline_manifest_sha256 TEXT NOT NULL CHECK (length(baseline_manifest_sha256) = 64),
  decision TEXT NOT NULL CHECK (decision IN ('approve', 'return', 'reject')),
  rationale TEXT NOT NULL CHECK (length(CAST(rationale AS BLOB)) BETWEEN 1 AND 4000),
  conditions_json TEXT NOT NULL CHECK (json_valid(conditions_json)),
  exceptions_json TEXT NOT NULL CHECK (json_valid(exceptions_json)),
  supporting_reviews_sha256 TEXT NOT NULL CHECK (length(supporting_reviews_sha256) = 64),
  decided_by TEXT NOT NULL CHECK (decided_by = 'engineer_user'),
  acting_role TEXT NOT NULL CHECK (acting_role = 'tendering_manager'),
  lifecycle_before TEXT NOT NULL CHECK (lifecycle_before = 'integrated_review'),
  lifecycle_after TEXT NOT NULL CHECK (lifecycle_after IN (
    'active_production', 'package_production'
  )),
  preceding_approval_hash TEXT NOT NULL CHECK (length(preceding_approval_hash) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  approval_sha256 TEXT NOT NULL UNIQUE CHECK (length(approval_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (baseline_id, baseline_version),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES coordinated_bid_baseline_versions(baseline_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE submission_generations (
  generation_sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  generation_id TEXT NOT NULL UNIQUE CHECK (length(generation_id) = 32),
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  baseline_manifest_sha256 TEXT NOT NULL CHECK (length(baseline_manifest_sha256) = 64),
  artifact_versions_json TEXT NOT NULL CHECK (
    json_valid(artifact_versions_json)
    AND length(CAST(artifact_versions_json AS BLOB)) <= 4194304
  ),
  requirements_json TEXT NOT NULL CHECK (
    json_valid(requirements_json)
    AND length(CAST(requirements_json AS BLOB)) <= 4194304
  ),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (
    json_valid(manifest_json) AND length(CAST(manifest_json AS BLOB)) <= 8388608
  ),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES coordinated_bid_baseline_versions(baseline_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE submission_artifacts (
  artifact_id TEXT PRIMARY KEY CHECK (length(artifact_id) = 32),
  stable_key TEXT NOT NULL UNIQUE CHECK (
    length(CAST(stable_key AS BLOB)) BETWEEN 1 AND 1000
  ),
  created_at TEXT NOT NULL
);
CREATE TABLE submission_artifact_versions (
  artifact_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  generation_id TEXT NOT NULL,
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  baseline_manifest_sha256 TEXT NOT NULL CHECK (length(baseline_manifest_sha256) = 64),
  section_key TEXT NOT NULL CHECK (length(CAST(section_key AS BLOB)) BETWEEN 1 AND 200),
  package_path TEXT NOT NULL CHECK (length(CAST(package_path AS BLOB)) BETWEEN 1 AND 1000),
  envelope_key TEXT NOT NULL CHECK (length(CAST(envelope_key AS BLOB)) BETWEEN 1 AND 200),
  language TEXT NOT NULL CHECK (length(CAST(language AS BLOB)) BETWEEN 1 AND 100),
  authoring_mode TEXT NOT NULL CHECK (authoring_mode IN ('docx', 'xlsx')),
  media_type TEXT NOT NULL CHECK (length(CAST(media_type AS BLOB)) BETWEEN 1 AND 200),
  classifications_json TEXT NOT NULL CHECK (json_valid(classifications_json)),
  scope_record_ids_json TEXT NOT NULL CHECK (json_valid(scope_record_ids_json)),
  content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
  size_bytes INTEGER NOT NULL CHECK (size_bytes BETWEEN 1 AND 16777216),
  exact_inputs_json TEXT NOT NULL CHECK (json_valid(exact_inputs_json)),
  generation_policy_id TEXT NOT NULL CHECK (
    length(CAST(generation_policy_id AS BLOB)) BETWEEN 1 AND 200
  ),
  generation_policy_version INTEGER NOT NULL CHECK (generation_policy_version > 0),
  generation_policy_sha256 TEXT NOT NULL CHECK (length(generation_policy_sha256) = 64),
  provenance_json TEXT NOT NULL CHECK (json_valid(provenance_json)),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (artifact_id, version),
  UNIQUE (generation_id, package_path),
  FOREIGN KEY (artifact_id) REFERENCES submission_artifacts(artifact_id),
  FOREIGN KEY (generation_id) REFERENCES submission_generations(generation_id),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES coordinated_bid_baseline_versions(baseline_id, version),
  FOREIGN KEY (content_sha256) REFERENCES content_objects(sha256)
);
CREATE TABLE submission_artifact_heads (
  artifact_id TEXT PRIMARY KEY,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (artifact_id, current_version)
    REFERENCES submission_artifact_versions(artifact_id, version)
);
CREATE TABLE generation_requirements (
  requirement_id TEXT PRIMARY KEY CHECK (length(requirement_id) = 64),
  generation_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 4096),
  kind TEXT NOT NULL CHECK (kind IN (
    'mandatory_requirement', 'deliverable', 'addendum_instruction', 'signature',
    'form_field', 'execution_requirement', 'required_file'
  )),
  record_id TEXT NOT NULL CHECK (length(record_id) = 32),
  record_version INTEGER NOT NULL CHECK (record_version > 0),
  record_manifest_sha256 TEXT NOT NULL CHECK (length(record_manifest_sha256) = 64),
  evidence_json TEXT NOT NULL CHECK (json_valid(evidence_json)),
  mandatory INTEGER NOT NULL CHECK (mandatory IN (0, 1)),
  section_key TEXT NOT NULL CHECK (length(CAST(section_key AS BLOB)) BETWEEN 1 AND 200),
  package_path TEXT NOT NULL CHECK (length(CAST(package_path AS BLOB)) BETWEEN 1 AND 1000),
  envelope_key TEXT NOT NULL CHECK (length(CAST(envelope_key AS BLOB)) BETWEEN 1 AND 200),
  language TEXT NOT NULL CHECK (length(CAST(language AS BLOB)) BETWEEN 1 AND 100),
  authoring_mode TEXT NOT NULL CHECK (authoring_mode IN ('docx', 'xlsx', 'unchanged_source', 'unsupported')),
  availability TEXT NOT NULL CHECK (availability IN ('available', 'missing', 'unsupported')),
  generated_artifact_id TEXT,
  generated_artifact_version INTEGER,
  source_artifact_id TEXT,
  source_artifact_version INTEGER,
  content_sha256 TEXT CHECK (content_sha256 IS NULL OR length(content_sha256) = 64),
  size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes BETWEEN 1 AND 16777216),
  calculation_references_json TEXT NOT NULL CHECK (json_valid(calculation_references_json)),
  review_references_json TEXT NOT NULL CHECK (json_valid(review_references_json)),
  decision_references_json TEXT NOT NULL CHECK (json_valid(decision_references_json)),
  authored_fields_json TEXT NOT NULL CHECK (json_valid(authored_fields_json)),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (generation_id, ordinal),
  FOREIGN KEY (generation_id) REFERENCES submission_generations(generation_id),
  FOREIGN KEY (record_id, record_version)
    REFERENCES tender_record_versions(record_id, version),
  FOREIGN KEY (generated_artifact_id, generated_artifact_version)
    REFERENCES submission_artifact_versions(artifact_id, version),
  FOREIGN KEY (source_artifact_id, source_artifact_version)
    REFERENCES source_artifact_versions(artifact_id, version),
  CHECK (
    (availability = 'available' AND content_sha256 IS NOT NULL AND size_bytes IS NOT NULL
      AND generated_artifact_id IS NOT NULL AND generated_artifact_version IS NOT NULL
      AND source_artifact_id IS NULL AND source_artifact_version IS NULL)
    OR
    (availability = 'available' AND content_sha256 IS NOT NULL AND size_bytes IS NOT NULL
      AND generated_artifact_id IS NULL AND generated_artifact_version IS NULL
      AND source_artifact_id IS NOT NULL AND source_artifact_version IS NOT NULL)
    OR
    (availability IN ('missing', 'unsupported') AND content_sha256 IS NULL AND size_bytes IS NULL
      AND generated_artifact_id IS NULL AND generated_artifact_version IS NULL
      AND source_artifact_id IS NULL AND source_artifact_version IS NULL)
  )
);
CREATE TABLE submission_packages (
  singleton INTEGER NOT NULL UNIQUE CHECK (singleton = 1),
  package_id TEXT PRIMARY KEY CHECK (length(package_id) = 32),
  created_at TEXT NOT NULL
);
CREATE TABLE submission_package_versions (
  package_id TEXT NOT NULL,
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  status TEXT NOT NULL CHECK (status = 'proposed'),
  assessment TEXT NOT NULL CHECK (assessment IN ('complete', 'blocked')),
  tender_revision INTEGER NOT NULL CHECK (tender_revision > 0),
  generation_id TEXT NOT NULL,
  generation_sequence INTEGER NOT NULL CHECK (generation_sequence BETWEEN 1 AND 32),
  generation_manifest_sha256 TEXT NOT NULL CHECK (length(generation_manifest_sha256) = 64),
  baseline_id TEXT NOT NULL,
  baseline_version INTEGER NOT NULL CHECK (baseline_version BETWEEN 1 AND 32),
  baseline_manifest_sha256 TEXT NOT NULL CHECK (length(baseline_manifest_sha256) = 64),
  baseline_approval_id TEXT NOT NULL CHECK (length(baseline_approval_id) = 32),
  work_plan_json TEXT NOT NULL CHECK (json_valid(work_plan_json)),
  calculation_references_json TEXT NOT NULL CHECK (json_valid(calculation_references_json)),
  decision_references_json TEXT NOT NULL CHECK (json_valid(decision_references_json)),
  deadline_json TEXT CHECK (deadline_json IS NULL OR json_valid(deadline_json)),
  validation_context_inputs_json TEXT NOT NULL CHECK (json_valid(validation_context_inputs_json)),
  validation_context_sha256 TEXT NOT NULL CHECK (length(validation_context_sha256) = 64),
  dependency_currentness_sha256 TEXT NOT NULL CHECK (length(dependency_currentness_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  manifest_json TEXT NOT NULL CHECK (
    json_valid(manifest_json) AND length(CAST(manifest_json AS BLOB)) <= 8388608
  ),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (package_id, version),
  FOREIGN KEY (package_id) REFERENCES submission_packages(package_id),
  FOREIGN KEY (generation_id) REFERENCES submission_generations(generation_id),
  FOREIGN KEY (baseline_id, baseline_version)
    REFERENCES coordinated_bid_baseline_versions(baseline_id, version),
  FOREIGN KEY (baseline_approval_id) REFERENCES coordinated_bid_baseline_approvals(approval_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE submission_package_items (
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 4096),
  item_id TEXT NOT NULL CHECK (length(item_id) = 64),
  package_path TEXT NOT NULL CHECK (length(CAST(package_path AS BLOB)) BETWEEN 1 AND 1000),
  section_key TEXT NOT NULL CHECK (length(CAST(section_key AS BLOB)) BETWEEN 1 AND 200),
  envelope_key TEXT NOT NULL CHECK (length(CAST(envelope_key AS BLOB)) BETWEEN 1 AND 200),
  language TEXT NOT NULL CHECK (length(CAST(language AS BLOB)) BETWEEN 1 AND 100),
  media_type TEXT NOT NULL CHECK (length(CAST(media_type AS BLOB)) BETWEEN 1 AND 200),
  source_kind TEXT NOT NULL CHECK (source_kind IN ('generated', 'unchanged_source')),
  source_id TEXT NOT NULL CHECK (length(source_id) = 32),
  source_version INTEGER NOT NULL CHECK (source_version > 0),
  source_manifest_sha256 TEXT,
  content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
  size_bytes INTEGER NOT NULL CHECK (size_bytes BETWEEN 1 AND 16777216),
  content_integrity TEXT NOT NULL CHECK (length(CAST(content_integrity AS BLOB)) BETWEEN 1 AND 1000),
  item_json TEXT NOT NULL CHECK (json_valid(item_json)),
  PRIMARY KEY (package_id, package_version, item_id),
  UNIQUE (package_id, package_version, ordinal),
  UNIQUE (package_id, package_version, package_path),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version)
);
CREATE TABLE submission_package_coverage (
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 4096),
  requirement_id TEXT NOT NULL CHECK (length(requirement_id) = 64),
  disposition TEXT NOT NULL CHECK (disposition IN ('covered', 'missing', 'unsupported')),
  item_id TEXT,
  coverage_json TEXT NOT NULL CHECK (json_valid(coverage_json)),
  PRIMARY KEY (package_id, package_version, requirement_id),
  UNIQUE (package_id, package_version, ordinal),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version),
  FOREIGN KEY (package_id, package_version, item_id)
    REFERENCES submission_package_items(package_id, package_version, item_id)
);
CREATE TABLE submission_package_sections (
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 512),
  section_key TEXT NOT NULL CHECK (length(CAST(section_key AS BLOB)) BETWEEN 1 AND 200),
  envelope_key TEXT NOT NULL CHECK (length(CAST(envelope_key AS BLOB)) BETWEEN 1 AND 200),
  language TEXT NOT NULL CHECK (length(CAST(language AS BLOB)) BETWEEN 1 AND 100),
  section_json TEXT NOT NULL CHECK (json_valid(section_json)),
  PRIMARY KEY (package_id, package_version, section_key, envelope_key, language),
  UNIQUE (package_id, package_version, ordinal),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version)
);
CREATE TABLE submission_package_uncovered_requirements (
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 4096),
  requirement_id TEXT NOT NULL CHECK (length(requirement_id) = 64),
  section_key TEXT NOT NULL CHECK (length(CAST(section_key AS BLOB)) BETWEEN 1 AND 200),
  envelope_key TEXT NOT NULL CHECK (length(CAST(envelope_key AS BLOB)) BETWEEN 1 AND 200),
  language TEXT NOT NULL CHECK (length(CAST(language AS BLOB)) BETWEEN 1 AND 100),
  requirement_json TEXT NOT NULL CHECK (json_valid(requirement_json)),
  PRIMARY KEY (package_id, package_version, requirement_id),
  UNIQUE (package_id, package_version, ordinal),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version)
);
CREATE TABLE submission_package_head (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  package_id TEXT NOT NULL,
  current_version INTEGER NOT NULL CHECK (current_version BETWEEN 1 AND 32),
  FOREIGN KEY (package_id, current_version)
    REFERENCES submission_package_versions(package_id, version)
);
CREATE TABLE package_validation_policies (
  policy_id TEXT NOT NULL CHECK (length(policy_id) = 32),
  version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 32),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  PRIMARY KEY (policy_id, version)
);
CREATE TABLE package_validation_runs (
  run_id TEXT PRIMARY KEY CHECK (length(run_id) = 32),
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  package_manifest_sha256 TEXT NOT NULL CHECK (length(package_manifest_sha256) = 64),
  policy_id TEXT NOT NULL,
  policy_version INTEGER NOT NULL CHECK (policy_version BETWEEN 1 AND 32),
  policy_manifest_sha256 TEXT NOT NULL CHECK (length(policy_manifest_sha256) = 64),
  validator_version INTEGER NOT NULL CHECK (validator_version > 0),
  renderer_version INTEGER NOT NULL CHECK (renderer_version > 0),
  context_sha256 TEXT NOT NULL CHECK (length(context_sha256) = 64),
  manifest_json TEXT NOT NULL CHECK (json_valid(manifest_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  UNIQUE (package_id, package_version),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version),
  FOREIGN KEY (policy_id, policy_version)
    REFERENCES package_validation_policies(policy_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE package_validation_item_results (
  result_id TEXT PRIMARY KEY CHECK (length(result_id) = 32),
  run_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 65536),
  item_id TEXT NOT NULL CHECK (length(item_id) = 64),
  content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
  validation_context_sha256 TEXT NOT NULL CHECK (length(validation_context_sha256) = 64),
  check_id TEXT NOT NULL CHECK (length(CAST(check_id AS BLOB)) BETWEEN 1 AND 200),
  check_version INTEGER NOT NULL CHECK (check_version > 0),
  category TEXT NOT NULL CHECK (category IN (
    'file_structure', 'rendering', 'calculation', 'cross_artifact_consistency',
    'hidden_content', 'information_boundary', 'filename', 'hash', 'package_wide'
  )),
  outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed', 'manual_verification_required')),
  policy_manifest_sha256 TEXT NOT NULL CHECK (length(policy_manifest_sha256) = 64),
  reused_from_result_id TEXT,
  result_json TEXT NOT NULL CHECK (json_valid(result_json)),
  UNIQUE (run_id, ordinal),
  UNIQUE (run_id, item_id, check_id),
  FOREIGN KEY (run_id) REFERENCES package_validation_runs(run_id),
  FOREIGN KEY (reused_from_result_id) REFERENCES package_validation_item_results(result_id)
);
CREATE TABLE package_validation_package_results (
  result_id TEXT PRIMARY KEY CHECK (length(result_id) = 32),
  run_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 4096),
  check_id TEXT NOT NULL CHECK (length(CAST(check_id AS BLOB)) BETWEEN 1 AND 200),
  check_version INTEGER NOT NULL CHECK (check_version > 0),
  category TEXT NOT NULL CHECK (category IN (
    'file_structure', 'rendering', 'calculation', 'cross_artifact_consistency',
    'hidden_content', 'information_boundary', 'filename', 'hash', 'package_wide'
  )),
  outcome TEXT NOT NULL CHECK (outcome IN ('passed', 'failed', 'manual_verification_required')),
  result_json TEXT NOT NULL CHECK (json_valid(result_json)),
  UNIQUE (run_id, ordinal),
  UNIQUE (run_id, check_id),
  FOREIGN KEY (run_id) REFERENCES package_validation_runs(run_id)
);
CREATE TABLE package_manual_verifications (
  verification_id TEXT PRIMARY KEY CHECK (length(verification_id) = 32),
  validation_result_id TEXT NOT NULL UNIQUE,
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  package_manifest_sha256 TEXT NOT NULL CHECK (length(package_manifest_sha256) = 64),
  item_id TEXT NOT NULL CHECK (length(item_id) = 64),
  content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
  capability TEXT NOT NULL CHECK (length(CAST(capability AS BLOB)) BETWEEN 1 AND 100),
  result TEXT NOT NULL CHECK (result IN ('passed', 'failed')),
  verification_json TEXT NOT NULL CHECK (json_valid(verification_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (validation_result_id) REFERENCES package_validation_item_results(result_id),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version)
);
CREATE TABLE final_review_plans (
  plan_id TEXT PRIMARY KEY CHECK (length(plan_id) = 32),
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  package_manifest_sha256 TEXT NOT NULL CHECK (length(package_manifest_sha256) = 64),
  validation_run_id TEXT NOT NULL UNIQUE,
  policy_manifest_sha256 TEXT NOT NULL CHECK (length(policy_manifest_sha256) = 64),
  plan_json TEXT NOT NULL CHECK (json_valid(plan_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  UNIQUE (package_id, package_version),
  FOREIGN KEY (validation_run_id) REFERENCES package_validation_runs(run_id),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version)
);
CREATE TABLE final_review_assignments (
  assignment_id TEXT PRIMARY KEY CHECK (length(assignment_id) = 32),
  plan_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 4096),
  section_key TEXT NOT NULL,
  required_capability TEXT NOT NULL,
  reviewer_profile_id TEXT,
  reviewer_profile_version INTEGER,
  assignment_json TEXT NOT NULL CHECK (json_valid(assignment_json)),
  UNIQUE (plan_id, ordinal),
  FOREIGN KEY (plan_id) REFERENCES final_review_plans(plan_id),
  FOREIGN KEY (reviewer_profile_id, reviewer_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  CHECK ((reviewer_profile_id IS NULL AND reviewer_profile_version IS NULL)
    OR (reviewer_profile_id IS NOT NULL AND reviewer_profile_version > 0))
);
CREATE TABLE submission_section_reviews (
  review_id TEXT PRIMARY KEY CHECK (length(review_id) = 32),
  assignment_id TEXT NOT NULL UNIQUE,
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  package_manifest_sha256 TEXT NOT NULL CHECK (length(package_manifest_sha256) = 64),
  reviewer_run_id TEXT NOT NULL UNIQUE,
  reviewer_profile_id TEXT NOT NULL,
  reviewer_profile_version INTEGER NOT NULL CHECK (reviewer_profile_version > 0),
  result TEXT NOT NULL CHECK (result IN ('satisfied', 'requires_remediation')),
  review_json TEXT NOT NULL CHECK (json_valid(review_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (assignment_id) REFERENCES final_review_assignments(assignment_id),
  FOREIGN KEY (reviewer_run_id) REFERENCES agent_runs(run_id),
  FOREIGN KEY (reviewer_profile_id, reviewer_profile_version)
    REFERENCES agent_profile_versions(profile_id, version),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version)
);
CREATE TABLE submission_section_review_findings (
  finding_id TEXT PRIMARY KEY CHECK (length(finding_id) = 32),
  review_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 1 AND 64),
  severity TEXT NOT NULL CHECK (severity IN ('critical', 'major', 'minor')),
  policy_rule_id TEXT NOT NULL,
  finding_json TEXT NOT NULL CHECK (json_valid(finding_json)),
  UNIQUE (review_id, ordinal),
  FOREIGN KEY (review_id) REFERENCES submission_section_reviews(review_id)
);
CREATE TABLE package_finding_exception_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  finding_id TEXT NOT NULL UNIQUE,
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  package_manifest_sha256 TEXT NOT NULL CHECK (length(package_manifest_sha256) = 64),
  approval_json TEXT NOT NULL CHECK (json_valid(approval_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  created_at TEXT NOT NULL,
  FOREIGN KEY (finding_id) REFERENCES submission_section_review_findings(finding_id),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version)
);
CREATE TABLE release_readiness_reports (
  report_id TEXT NOT NULL CHECK (length(report_id) = 32),
  version INTEGER NOT NULL CHECK (version > 0),
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  package_manifest_sha256 TEXT NOT NULL CHECK (length(package_manifest_sha256) = 64),
  through_event_sequence INTEGER NOT NULL CHECK (through_event_sequence > 0),
  report_json TEXT NOT NULL CHECK (json_valid(report_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  PRIMARY KEY (report_id, version),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version),
  FOREIGN KEY (through_event_sequence) REFERENCES audit_events(sequence),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE release_readiness_report_head (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  report_id TEXT NOT NULL,
  current_version INTEGER NOT NULL CHECK (current_version > 0),
  FOREIGN KEY (report_id, current_version)
    REFERENCES release_readiness_reports(report_id, version)
);
CREATE TABLE submission_release_approvals (
  approval_id TEXT PRIMARY KEY CHECK (length(approval_id) = 32),
  package_id TEXT NOT NULL,
  package_version INTEGER NOT NULL CHECK (package_version BETWEEN 1 AND 32),
  package_manifest_sha256 TEXT NOT NULL CHECK (length(package_manifest_sha256) = 64),
  canonical_manifest_root TEXT NOT NULL CHECK (length(canonical_manifest_root) = 64),
  readiness_report_id TEXT NOT NULL,
  readiness_report_version INTEGER NOT NULL CHECK (readiness_report_version > 0),
  readiness_report_manifest_sha256 TEXT NOT NULL CHECK (length(readiness_report_manifest_sha256) = 64),
  approval_json TEXT NOT NULL CHECK (json_valid(approval_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  UNIQUE (package_id, package_version),
  FOREIGN KEY (package_id, package_version)
    REFERENCES submission_package_versions(package_id, version),
  FOREIGN KEY (readiness_report_id, readiness_report_version)
    REFERENCES release_readiness_reports(report_id, version),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE submission_release_items (
  approval_id TEXT NOT NULL,
  ordinal INTEGER NOT NULL CHECK (ordinal > 0),
  package_path TEXT NOT NULL CHECK (length(CAST(package_path AS BLOB)) BETWEEN 1 AND 1000),
  content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
  size_bytes INTEGER NOT NULL CHECK (size_bytes > 0),
  content BLOB NOT NULL CHECK (length(content) = size_bytes),
  PRIMARY KEY (approval_id, ordinal),
  UNIQUE (approval_id, package_path),
  FOREIGN KEY (approval_id) REFERENCES submission_release_approvals(approval_id)
);
CREATE TABLE release_copy_exports (
  export_id TEXT PRIMARY KEY CHECK (length(export_id) = 32),
  approval_id TEXT NOT NULL,
  relative_path TEXT NOT NULL UNIQUE CHECK (length(CAST(relative_path AS BLOB)) BETWEEN 1 AND 1000),
  export_json TEXT NOT NULL CHECK (json_valid(export_json)),
  manifest_sha256 TEXT NOT NULL UNIQUE CHECK (length(manifest_sha256) = 64),
  audit_sequence INTEGER NOT NULL UNIQUE CHECK (audit_sequence > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (approval_id) REFERENCES submission_release_approvals(approval_id),
  FOREIGN KEY (audit_sequence) REFERENCES audit_events(sequence)
);
CREATE TABLE audit_events (
  sequence INTEGER PRIMARY KEY CHECK (sequence > 0),
  event_type TEXT NOT NULL,
  aggregate_revision INTEGER NOT NULL CHECK (aggregate_revision > 0),
  payload_json TEXT NOT NULL,
  preceding_hash TEXT NOT NULL CHECK (length(preceding_hash) = 64),
  current_hash TEXT NOT NULL UNIQUE CHECK (length(current_hash) = 64),
  created_at TEXT NOT NULL
);
CREATE TRIGGER tender_identity_no_update
BEFORE UPDATE OF singleton, tender_id, created_at ON tender
BEGIN
  SELECT RAISE(ABORT, 'Tender identity is immutable');
END;
CREATE TRIGGER tender_no_delete
BEFORE DELETE ON tender
BEGIN
  SELECT RAISE(ABORT, 'Tender identity is immutable');
END;
CREATE TRIGGER tender_revisions_no_update
BEFORE UPDATE ON tender_revisions
BEGIN
  SELECT RAISE(ABORT, 'Tender revisions are immutable');
END;
CREATE TRIGGER tender_revisions_no_delete
BEFORE DELETE ON tender_revisions
BEGIN
  SELECT RAISE(ABORT, 'Tender revisions are immutable');
END;
CREATE TRIGGER tender_retention_identity_no_update
BEFORE UPDATE OF singleton ON tender_retention
BEGIN
  SELECT RAISE(ABORT, 'Tender retention identity is immutable');
END;
CREATE TRIGGER tender_retention_no_delete
BEFORE DELETE ON tender_retention
BEGIN
  SELECT RAISE(ABORT, 'Tender retention state cannot be deleted');
END;
CREATE TRIGGER tender_retention_decisions_no_update
BEFORE UPDATE ON tender_retention_decisions
BEGIN
  SELECT RAISE(ABORT, 'Tender retention decisions are immutable');
END;
CREATE TRIGGER tender_retention_decisions_no_delete
BEFORE DELETE ON tender_retention_decisions
BEGIN
  SELECT RAISE(ABORT, 'Tender retention decisions are immutable');
END;
CREATE TRIGGER submission_release_approvals_no_update
BEFORE UPDATE ON submission_release_approvals
BEGIN
  SELECT RAISE(ABORT, 'Submission Release approvals are immutable');
END;
CREATE TRIGGER submission_release_approvals_no_delete
BEFORE DELETE ON submission_release_approvals
BEGIN
  SELECT RAISE(ABORT, 'Submission Release approvals are immutable');
END;
CREATE TRIGGER submission_release_items_no_update
BEFORE UPDATE ON submission_release_items
BEGIN
  SELECT RAISE(ABORT, 'Approved Submission Release bytes are immutable');
END;
CREATE TRIGGER submission_release_items_no_delete
BEFORE DELETE ON submission_release_items
BEGIN
  SELECT RAISE(ABORT, 'Approved Submission Release bytes are immutable');
END;
CREATE TRIGGER release_copy_exports_no_update
BEFORE UPDATE ON release_copy_exports
BEGIN
  SELECT RAISE(ABORT, 'Release Copy exports are immutable');
END;
CREATE TRIGGER release_copy_exports_no_delete
BEFORE DELETE ON release_copy_exports
BEGIN
  SELECT RAISE(ABORT, 'Release Copy exports are immutable');
END;
CREATE TRIGGER content_objects_no_update
BEFORE UPDATE ON content_objects
BEGIN
  SELECT RAISE(ABORT, 'Content Objects are immutable');
END;
CREATE TRIGGER content_objects_no_delete
BEFORE DELETE ON content_objects
BEGIN
  SELECT RAISE(ABORT, 'Content Objects are immutable');
END;
CREATE TRIGGER content_versions_no_update
BEFORE UPDATE ON content_versions
BEGIN
  SELECT RAISE(ABORT, 'Content versions are immutable');
END;
CREATE TRIGGER content_versions_no_delete
BEFORE DELETE ON content_versions
BEGIN
  SELECT RAISE(ABORT, 'Content versions are immutable');
END;
CREATE TRIGGER intake_runs_no_update
BEFORE UPDATE ON intake_runs
BEGIN
  SELECT RAISE(ABORT, 'Intake runs are immutable');
END;
CREATE TRIGGER intake_runs_no_delete
BEFORE DELETE ON intake_runs
BEGIN
  SELECT RAISE(ABORT, 'Intake runs are immutable');
END;
CREATE TRIGGER query_register_no_update
BEFORE UPDATE ON query_register
BEGIN
  SELECT RAISE(ABORT, 'Query Register identity is immutable');
END;
CREATE TRIGGER query_register_no_delete
BEFORE DELETE ON query_register
BEGIN
  SELECT RAISE(ABORT, 'Query Register identity is immutable');
END;
CREATE TRIGGER tender_queries_no_update
BEFORE UPDATE ON tender_queries
BEGIN
  SELECT RAISE(ABORT, 'Tender Query identities are immutable');
END;
CREATE TRIGGER tender_queries_no_delete
BEFORE DELETE ON tender_queries
BEGIN
  SELECT RAISE(ABORT, 'Tender Query identities are immutable');
END;
CREATE TRIGGER tender_query_versions_no_update
BEFORE UPDATE ON tender_query_versions
BEGIN
  SELECT RAISE(ABORT, 'Tender Query Versions are immutable');
END;
CREATE TRIGGER tender_query_versions_no_delete
BEFORE DELETE ON tender_query_versions
BEGIN
  SELECT RAISE(ABORT, 'Tender Query Versions are immutable');
END;
CREATE TRIGGER tender_query_treatment_decisions_no_update
BEFORE UPDATE ON tender_query_treatment_decisions
BEGIN
  SELECT RAISE(ABORT, 'Approved Query Treatments are immutable');
END;
CREATE TRIGGER tender_query_treatment_decisions_no_delete
BEFORE DELETE ON tender_query_treatment_decisions
BEGIN
  SELECT RAISE(ABORT, 'Approved Query Treatments are immutable');
END;
CREATE TRIGGER tender_query_target_invalidations_no_update
BEFORE UPDATE ON tender_query_target_invalidations
BEGIN
  SELECT RAISE(ABORT, 'Tender Query invalidations are immutable');
END;
CREATE TRIGGER tender_query_target_invalidations_no_delete
BEFORE DELETE ON tender_query_target_invalidations
BEGIN
  SELECT RAISE(ABORT, 'Tender Query invalidations are immutable');
END;
CREATE TRIGGER external_rfis_no_update
BEFORE UPDATE ON external_rfis
BEGIN
  SELECT RAISE(ABORT, 'External RFI identities are immutable');
END;
CREATE TRIGGER external_rfis_no_delete
BEFORE DELETE ON external_rfis
BEGIN
  SELECT RAISE(ABORT, 'External RFI identities are immutable');
END;
CREATE TRIGGER external_rfi_versions_no_update
BEFORE UPDATE ON external_rfi_versions
BEGIN
  SELECT RAISE(ABORT, 'External RFI Versions are immutable');
END;
CREATE TRIGGER external_rfi_versions_no_delete
BEFORE DELETE ON external_rfi_versions
BEGIN
  SELECT RAISE(ABORT, 'External RFI Versions are immutable');
END;
CREATE TRIGGER external_rfi_reviews_no_update
BEFORE UPDATE ON external_rfi_reviews
BEGIN
  SELECT RAISE(ABORT, 'External RFI Reviews are immutable');
END;
CREATE TRIGGER external_rfi_reviews_no_delete
BEFORE DELETE ON external_rfi_reviews
BEGIN
  SELECT RAISE(ABORT, 'External RFI Reviews are immutable');
END;
CREATE TRIGGER external_rfi_approvals_no_update
BEFORE UPDATE ON external_rfi_approvals
BEGIN
  SELECT RAISE(ABORT, 'External RFI Approvals are immutable');
END;
CREATE TRIGGER external_rfi_approvals_no_delete
BEFORE DELETE ON external_rfi_approvals
BEGIN
  SELECT RAISE(ABORT, 'External RFI Approvals are immutable');
END;
CREATE TRIGGER external_rfi_exports_no_update
BEFORE UPDATE ON external_rfi_exports
BEGIN
  SELECT RAISE(ABORT, 'External RFI Exports are immutable');
END;
CREATE TRIGGER external_rfi_exports_no_delete
BEFORE DELETE ON external_rfi_exports
BEGIN
  SELECT RAISE(ABORT, 'External RFI Exports are immutable');
END;
CREATE TRIGGER external_rfi_responses_no_update
BEFORE UPDATE ON external_rfi_responses
BEGIN
  SELECT RAISE(ABORT, 'External RFI Response links are immutable');
END;
CREATE TRIGGER external_rfi_responses_no_delete
BEFORE DELETE ON external_rfi_responses
BEGIN
  SELECT RAISE(ABORT, 'External RFI Response links are immutable');
END;
CREATE TRIGGER external_rfi_response_interpretations_no_update
BEFORE UPDATE ON external_rfi_response_interpretations
BEGIN
  SELECT RAISE(ABORT, 'External RFI Response interpretations are immutable');
END;
CREATE TRIGGER external_rfi_response_interpretations_no_delete
BEFORE DELETE ON external_rfi_response_interpretations
BEGIN
  SELECT RAISE(ABORT, 'External RFI Response interpretations are immutable');
END;
CREATE TRIGGER calculation_rules_no_update
BEFORE UPDATE ON calculation_rules
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rules are immutable');
END;
CREATE TRIGGER calculation_rules_no_delete
BEFORE DELETE ON calculation_rules
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rules are immutable');
END;
CREATE TRIGGER calculation_rule_versions_no_update
BEFORE UPDATE ON calculation_rule_versions
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rule Versions are immutable');
END;
CREATE TRIGGER calculation_rule_versions_no_delete
BEFORE DELETE ON calculation_rule_versions
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rule Versions are immutable');
END;
CREATE TRIGGER calculation_rule_heads_identity_immutable
BEFORE UPDATE ON calculation_rule_heads
WHEN NEW.rule_id != OLD.rule_id
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rule head identity is immutable');
END;
CREATE TRIGGER calculation_rule_heads_no_delete
BEFORE DELETE ON calculation_rule_heads
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rule heads cannot be deleted');
END;
CREATE TRIGGER calculation_rule_reviews_no_update
BEFORE UPDATE ON calculation_rule_reviews
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rule Reviews are immutable');
END;
CREATE TRIGGER calculation_rule_reviews_no_delete
BEFORE DELETE ON calculation_rule_reviews
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rule Reviews are immutable');
END;
CREATE TRIGGER calculation_rule_approvals_no_update
BEFORE UPDATE ON calculation_rule_approvals
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rule Approvals are immutable');
END;
CREATE TRIGGER calculation_rule_approvals_no_delete
BEFORE DELETE ON calculation_rule_approvals
BEGIN
  SELECT RAISE(ABORT, 'Calculation Rule Approvals are immutable');
END;
CREATE TRIGGER calculation_scenario_versions_no_update
BEFORE UPDATE ON calculation_scenario_versions
BEGIN
  SELECT RAISE(ABORT, 'Calculation Scenario Versions are immutable');
END;
CREATE TRIGGER calculation_scenario_versions_no_delete
BEFORE DELETE ON calculation_scenario_versions
BEGIN
  SELECT RAISE(ABORT, 'Calculation Scenario Versions are immutable');
END;
CREATE TRIGGER calculation_runs_no_update
BEFORE UPDATE ON calculation_runs
BEGIN
  SELECT RAISE(ABORT, 'Calculation Runs are immutable');
END;
CREATE TRIGGER calculation_runs_no_delete
BEFORE DELETE ON calculation_runs
BEGIN
  SELECT RAISE(ABORT, 'Calculation Runs are immutable');
END;
CREATE TRIGGER calculation_run_approvals_no_update
BEFORE UPDATE ON calculation_run_approvals
BEGIN
  SELECT RAISE(ABORT, 'Calculation Run approvals are immutable');
END;
CREATE TRIGGER calculation_run_approvals_no_delete
BEFORE DELETE ON calculation_run_approvals
BEGIN
  SELECT RAISE(ABORT, 'Calculation Run approvals are immutable');
END;
CREATE TRIGGER estimate_aggregate_calculation_runs_no_update
BEFORE UPDATE ON estimate_aggregate_calculation_runs
BEGIN
  SELECT RAISE(ABORT, 'Estimate aggregate Calculation Runs are immutable');
END;
CREATE TRIGGER estimate_aggregate_calculation_runs_no_delete
BEFORE DELETE ON estimate_aggregate_calculation_runs
BEGIN
  SELECT RAISE(ABORT, 'Estimate aggregate Calculation Runs are immutable');
END;
CREATE TRIGGER estimate_aggregate_calculation_approvals_no_update
BEFORE UPDATE ON estimate_aggregate_calculation_approvals
BEGIN
  SELECT RAISE(ABORT, 'Estimate aggregate Calculation Run approvals are immutable');
END;
CREATE TRIGGER estimate_aggregate_calculation_approvals_no_delete
BEFORE DELETE ON estimate_aggregate_calculation_approvals
BEGIN
  SELECT RAISE(ABORT, 'Estimate aggregate Calculation Run approvals are immutable');
END;
CREATE TRIGGER boq_table_designations_no_update
BEFORE UPDATE ON boq_table_designations
BEGIN
  SELECT RAISE(ABORT, 'BOQ table designations are immutable');
END;
CREATE TRIGGER boq_table_designations_no_delete
BEFORE DELETE ON boq_table_designations
BEGIN
  SELECT RAISE(ABORT, 'BOQ table designations are immutable');
END;
CREATE TRIGGER basis_of_estimates_no_update
BEFORE UPDATE ON basis_of_estimates
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate identities are immutable');
END;
CREATE TRIGGER basis_of_estimates_no_delete
BEFORE DELETE ON basis_of_estimates
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate identities are immutable');
END;
CREATE TRIGGER basis_of_estimate_versions_no_update
BEFORE UPDATE ON basis_of_estimate_versions
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate Versions are immutable');
END;
CREATE TRIGGER basis_of_estimate_versions_no_delete
BEFORE DELETE ON basis_of_estimate_versions
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate Versions are immutable');
END;
CREATE TRIGGER basis_of_estimate_heads_identity_immutable
BEFORE UPDATE ON basis_of_estimate_heads
WHEN NEW.basis_id != OLD.basis_id
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate head identity is immutable');
END;
CREATE TRIGGER basis_of_estimate_heads_no_delete
BEFORE DELETE ON basis_of_estimate_heads
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate heads cannot be deleted');
END;
CREATE TRIGGER basis_of_estimate_reviews_no_update
BEFORE UPDATE ON basis_of_estimate_reviews
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate Reviews are immutable');
END;
CREATE TRIGGER basis_of_estimate_reviews_no_delete
BEFORE DELETE ON basis_of_estimate_reviews
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate Reviews are immutable');
END;
CREATE TRIGGER basis_of_estimate_approvals_no_update
BEFORE UPDATE ON basis_of_estimate_approvals
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate Approvals are immutable');
END;
CREATE TRIGGER basis_of_estimate_approvals_no_delete
BEFORE DELETE ON basis_of_estimate_approvals
BEGIN
  SELECT RAISE(ABORT, 'Basis of Estimate Approvals are immutable');
END;
CREATE TRIGGER priced_cost_baselines_no_update
BEFORE UPDATE ON priced_cost_baselines
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline identities are immutable');
END;
CREATE TRIGGER priced_cost_baselines_no_delete
BEFORE DELETE ON priced_cost_baselines
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline identities are immutable');
END;
CREATE TRIGGER priced_cost_baseline_versions_no_update
BEFORE UPDATE ON priced_cost_baseline_versions
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline Versions are immutable');
END;
CREATE TRIGGER priced_cost_baseline_versions_no_delete
BEFORE DELETE ON priced_cost_baseline_versions
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline Versions are immutable');
END;
CREATE TRIGGER priced_cost_baseline_heads_identity_immutable
BEFORE UPDATE ON priced_cost_baseline_heads
WHEN NEW.baseline_id != OLD.baseline_id
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline head identity is immutable');
END;
CREATE TRIGGER priced_cost_baseline_heads_no_delete
BEFORE DELETE ON priced_cost_baseline_heads
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline heads cannot be deleted');
END;
CREATE TRIGGER priced_cost_baseline_reviews_no_update
BEFORE UPDATE ON priced_cost_baseline_reviews
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline Reviews are immutable');
END;
CREATE TRIGGER priced_cost_baseline_reviews_no_delete
BEFORE DELETE ON priced_cost_baseline_reviews
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline Reviews are immutable');
END;
CREATE TRIGGER priced_cost_baseline_approvals_no_update
BEFORE UPDATE ON priced_cost_baseline_approvals
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline Approvals are immutable');
END;
CREATE TRIGGER priced_cost_baseline_approvals_no_delete
BEFORE DELETE ON priced_cost_baseline_approvals
BEGIN
  SELECT RAISE(ABORT, 'Priced Cost Baseline Approvals are immutable');
END;
CREATE TRIGGER pricing_calculation_runs_no_update
BEFORE UPDATE ON pricing_calculation_runs
BEGIN
  SELECT RAISE(ABORT, 'Pricing Calculation Runs are immutable');
END;
CREATE TRIGGER pricing_calculation_runs_no_delete
BEFORE DELETE ON pricing_calculation_runs
BEGIN
  SELECT RAISE(ABORT, 'Pricing Calculation Runs are immutable');
END;
CREATE TRIGGER pricing_adjustments_no_update BEFORE UPDATE ON pricing_adjustments BEGIN SELECT RAISE(ABORT, 'Pricing Adjustments are immutable'); END;
CREATE TRIGGER pricing_adjustments_no_delete BEFORE DELETE ON pricing_adjustments BEGIN SELECT RAISE(ABORT, 'Pricing Adjustments are immutable'); END;
CREATE TRIGGER pricing_adjustment_versions_no_update BEFORE UPDATE ON pricing_adjustment_versions BEGIN SELECT RAISE(ABORT, 'Pricing Adjustment Versions are immutable'); END;
CREATE TRIGGER pricing_adjustment_versions_no_delete BEFORE DELETE ON pricing_adjustment_versions BEGIN SELECT RAISE(ABORT, 'Pricing Adjustment Versions are immutable'); END;
CREATE TRIGGER pricing_adjustment_heads_identity_immutable BEFORE UPDATE ON pricing_adjustment_heads WHEN NEW.adjustment_id != OLD.adjustment_id BEGIN SELECT RAISE(ABORT, 'Pricing Adjustment head identity is immutable'); END;
CREATE TRIGGER pricing_adjustment_heads_no_delete BEFORE DELETE ON pricing_adjustment_heads BEGIN SELECT RAISE(ABORT, 'Pricing Adjustment heads cannot be deleted'); END;
CREATE TRIGGER pricing_adjustment_reviews_no_update BEFORE UPDATE ON pricing_adjustment_reviews BEGIN SELECT RAISE(ABORT, 'Pricing Adjustment Reviews are immutable'); END;
CREATE TRIGGER pricing_adjustment_reviews_no_delete BEFORE DELETE ON pricing_adjustment_reviews BEGIN SELECT RAISE(ABORT, 'Pricing Adjustment Reviews are immutable'); END;
CREATE TRIGGER pricing_adjustment_approvals_no_update BEFORE UPDATE ON pricing_adjustment_approvals BEGIN SELECT RAISE(ABORT, 'Pricing Adjustment Approvals are immutable'); END;
CREATE TRIGGER pricing_adjustment_approvals_no_delete BEFORE DELETE ON pricing_adjustment_approvals BEGIN SELECT RAISE(ABORT, 'Pricing Adjustment Approvals are immutable'); END;
CREATE TRIGGER commercial_strategies_no_update BEFORE UPDATE ON commercial_strategies BEGIN SELECT RAISE(ABORT, 'Commercial Strategies are immutable'); END;
CREATE TRIGGER commercial_strategies_no_delete BEFORE DELETE ON commercial_strategies BEGIN SELECT RAISE(ABORT, 'Commercial Strategies are immutable'); END;
CREATE TRIGGER commercial_strategy_approvals_no_update BEFORE UPDATE ON commercial_strategy_approvals BEGIN SELECT RAISE(ABORT, 'Commercial Strategy Approvals are immutable'); END;
CREATE TRIGGER commercial_strategy_approvals_no_delete BEFORE DELETE ON commercial_strategy_approvals BEGIN SELECT RAISE(ABORT, 'Commercial Strategy Approvals are immutable'); END;
CREATE TRIGGER pricing_scenarios_no_update BEFORE UPDATE ON pricing_scenarios BEGIN SELECT RAISE(ABORT, 'Pricing Scenario identities are immutable'); END;
CREATE TRIGGER pricing_scenarios_no_delete BEFORE DELETE ON pricing_scenarios BEGIN SELECT RAISE(ABORT, 'Pricing Scenario identities are immutable'); END;
CREATE TRIGGER pricing_scenario_versions_no_update BEFORE UPDATE ON pricing_scenario_versions BEGIN SELECT RAISE(ABORT, 'Pricing Scenario Versions are immutable'); END;
CREATE TRIGGER pricing_scenario_versions_no_delete BEFORE DELETE ON pricing_scenario_versions BEGIN SELECT RAISE(ABORT, 'Pricing Scenario Versions are immutable'); END;
CREATE TRIGGER pricing_scenario_selections_no_update BEFORE UPDATE ON pricing_scenario_selections BEGIN SELECT RAISE(ABORT, 'Pricing Scenario Selections are immutable'); END;
CREATE TRIGGER pricing_scenario_selections_no_delete BEFORE DELETE ON pricing_scenario_selections BEGIN SELECT RAISE(ABORT, 'Pricing Scenario Selections are immutable'); END;
CREATE TRIGGER pricing_selection_head_identity_immutable BEFORE UPDATE ON pricing_selection_head WHEN NEW.singleton != OLD.singleton BEGIN SELECT RAISE(ABORT, 'Pricing Selection head identity is immutable'); END;
CREATE TRIGGER pricing_selection_head_no_delete BEFORE DELETE ON pricing_selection_head BEGIN SELECT RAISE(ABORT, 'Pricing Selection head cannot be deleted'); END;
CREATE TRIGGER approved_tender_prices_no_update BEFORE UPDATE ON approved_tender_prices BEGIN SELECT RAISE(ABORT, 'Approved Tender Prices are immutable'); END;
CREATE TRIGGER approved_tender_prices_no_delete BEFORE DELETE ON approved_tender_prices BEGIN SELECT RAISE(ABORT, 'Approved Tender Prices are immutable'); END;
CREATE TRIGGER submission_generations_no_update BEFORE UPDATE ON submission_generations BEGIN SELECT RAISE(ABORT, 'Submission Generations are immutable'); END;
CREATE TRIGGER submission_generations_no_delete BEFORE DELETE ON submission_generations BEGIN SELECT RAISE(ABORT, 'Submission Generations are immutable'); END;
CREATE TRIGGER submission_artifacts_no_update BEFORE UPDATE ON submission_artifacts BEGIN SELECT RAISE(ABORT, 'Submission Artifacts are immutable'); END;
CREATE TRIGGER submission_artifacts_no_delete BEFORE DELETE ON submission_artifacts BEGIN SELECT RAISE(ABORT, 'Submission Artifacts are immutable'); END;
CREATE TRIGGER submission_artifact_versions_no_update BEFORE UPDATE ON submission_artifact_versions BEGIN SELECT RAISE(ABORT, 'Submission Artifact Versions are immutable'); END;
CREATE TRIGGER submission_artifact_versions_no_delete BEFORE DELETE ON submission_artifact_versions BEGIN SELECT RAISE(ABORT, 'Submission Artifact Versions are immutable'); END;
CREATE TRIGGER submission_artifact_heads_identity_immutable BEFORE UPDATE ON submission_artifact_heads WHEN NEW.artifact_id != OLD.artifact_id BEGIN SELECT RAISE(ABORT, 'Submission Artifact head identity is immutable'); END;
CREATE TRIGGER submission_artifact_heads_no_delete BEFORE DELETE ON submission_artifact_heads BEGIN SELECT RAISE(ABORT, 'Submission Artifact heads cannot be deleted'); END;
CREATE TRIGGER generation_requirements_no_update BEFORE UPDATE ON generation_requirements BEGIN SELECT RAISE(ABORT, 'Generation Requirements are immutable'); END;
CREATE TRIGGER generation_requirements_no_delete BEFORE DELETE ON generation_requirements BEGIN SELECT RAISE(ABORT, 'Generation Requirements are immutable'); END;
CREATE TRIGGER submission_packages_no_update BEFORE UPDATE ON submission_packages BEGIN SELECT RAISE(ABORT, 'Submission Package identities are immutable'); END;
CREATE TRIGGER submission_packages_no_delete BEFORE DELETE ON submission_packages BEGIN SELECT RAISE(ABORT, 'Submission Package identities are immutable'); END;
CREATE TRIGGER submission_package_versions_no_update BEFORE UPDATE ON submission_package_versions BEGIN SELECT RAISE(ABORT, 'Submission Package Versions are immutable'); END;
CREATE TRIGGER submission_package_versions_no_delete BEFORE DELETE ON submission_package_versions BEGIN SELECT RAISE(ABORT, 'Submission Package Versions are immutable'); END;
CREATE TRIGGER submission_package_items_no_update BEFORE UPDATE ON submission_package_items BEGIN SELECT RAISE(ABORT, 'Submission Package Items are immutable'); END;
CREATE TRIGGER submission_package_items_no_delete BEFORE DELETE ON submission_package_items BEGIN SELECT RAISE(ABORT, 'Submission Package Items are immutable'); END;
CREATE TRIGGER submission_package_coverage_no_update BEFORE UPDATE ON submission_package_coverage BEGIN SELECT RAISE(ABORT, 'Submission Package Coverage is immutable'); END;
CREATE TRIGGER submission_package_coverage_no_delete BEFORE DELETE ON submission_package_coverage BEGIN SELECT RAISE(ABORT, 'Submission Package Coverage is immutable'); END;
CREATE TRIGGER submission_package_sections_no_update BEFORE UPDATE ON submission_package_sections BEGIN SELECT RAISE(ABORT, 'Submission Package Sections are immutable'); END;
CREATE TRIGGER submission_package_sections_no_delete BEFORE DELETE ON submission_package_sections BEGIN SELECT RAISE(ABORT, 'Submission Package Sections are immutable'); END;
CREATE TRIGGER submission_package_uncovered_requirements_no_update BEFORE UPDATE ON submission_package_uncovered_requirements BEGIN SELECT RAISE(ABORT, 'Submission Package Uncovered Requirements are immutable'); END;
CREATE TRIGGER submission_package_uncovered_requirements_no_delete BEFORE DELETE ON submission_package_uncovered_requirements BEGIN SELECT RAISE(ABORT, 'Submission Package Uncovered Requirements are immutable'); END;
CREATE TRIGGER submission_package_head_identity_immutable BEFORE UPDATE ON submission_package_head WHEN NEW.singleton != OLD.singleton OR NEW.package_id != OLD.package_id BEGIN SELECT RAISE(ABORT, 'Submission Package head identity is immutable'); END;
CREATE TRIGGER submission_package_head_no_delete BEFORE DELETE ON submission_package_head BEGIN SELECT RAISE(ABORT, 'Submission Package head cannot be deleted'); END;
CREATE TRIGGER package_validation_policies_no_update BEFORE UPDATE ON package_validation_policies BEGIN SELECT RAISE(ABORT, 'Package Validation Policies are immutable'); END;
CREATE TRIGGER package_validation_policies_no_delete BEFORE DELETE ON package_validation_policies BEGIN SELECT RAISE(ABORT, 'Package Validation Policies are immutable'); END;
CREATE TRIGGER package_validation_runs_no_update BEFORE UPDATE ON package_validation_runs BEGIN SELECT RAISE(ABORT, 'Package Validation Runs are immutable'); END;
CREATE TRIGGER package_validation_runs_no_delete BEFORE DELETE ON package_validation_runs BEGIN SELECT RAISE(ABORT, 'Package Validation Runs are immutable'); END;
CREATE TRIGGER package_validation_item_results_no_update BEFORE UPDATE ON package_validation_item_results BEGIN SELECT RAISE(ABORT, 'Package Validation item results are immutable'); END;
CREATE TRIGGER package_validation_item_results_no_delete BEFORE DELETE ON package_validation_item_results BEGIN SELECT RAISE(ABORT, 'Package Validation item results are immutable'); END;
CREATE TRIGGER package_validation_package_results_no_update BEFORE UPDATE ON package_validation_package_results BEGIN SELECT RAISE(ABORT, 'Package Validation package results are immutable'); END;
CREATE TRIGGER package_validation_package_results_no_delete BEFORE DELETE ON package_validation_package_results BEGIN SELECT RAISE(ABORT, 'Package Validation package results are immutable'); END;
CREATE TRIGGER package_manual_verifications_no_update BEFORE UPDATE ON package_manual_verifications BEGIN SELECT RAISE(ABORT, 'Manual Verifications are immutable'); END;
CREATE TRIGGER package_manual_verifications_no_delete BEFORE DELETE ON package_manual_verifications BEGIN SELECT RAISE(ABORT, 'Manual Verifications are immutable'); END;
CREATE TRIGGER final_review_plans_no_update BEFORE UPDATE ON final_review_plans BEGIN SELECT RAISE(ABORT, 'Final Review Plans are immutable'); END;
CREATE TRIGGER final_review_plans_no_delete BEFORE DELETE ON final_review_plans BEGIN SELECT RAISE(ABORT, 'Final Review Plans are immutable'); END;
CREATE TRIGGER final_review_assignments_no_update BEFORE UPDATE ON final_review_assignments BEGIN SELECT RAISE(ABORT, 'Final Review assignments are immutable'); END;
CREATE TRIGGER final_review_assignments_no_delete BEFORE DELETE ON final_review_assignments BEGIN SELECT RAISE(ABORT, 'Final Review assignments are immutable'); END;
CREATE TRIGGER submission_section_reviews_no_update BEFORE UPDATE ON submission_section_reviews BEGIN SELECT RAISE(ABORT, 'Submission Section Reviews are immutable'); END;
CREATE TRIGGER submission_section_reviews_no_delete BEFORE DELETE ON submission_section_reviews BEGIN SELECT RAISE(ABORT, 'Submission Section Reviews are immutable'); END;
CREATE TRIGGER submission_section_review_findings_no_update BEFORE UPDATE ON submission_section_review_findings BEGIN SELECT RAISE(ABORT, 'Submission Section Review Findings are immutable'); END;
CREATE TRIGGER submission_section_review_findings_no_delete BEFORE DELETE ON submission_section_review_findings BEGIN SELECT RAISE(ABORT, 'Submission Section Review Findings are immutable'); END;
CREATE TRIGGER package_finding_exception_approvals_no_update BEFORE UPDATE ON package_finding_exception_approvals BEGIN SELECT RAISE(ABORT, 'Package Finding Exception Approvals are immutable'); END;
CREATE TRIGGER package_finding_exception_approvals_no_delete BEFORE DELETE ON package_finding_exception_approvals BEGIN SELECT RAISE(ABORT, 'Package Finding Exception Approvals are immutable'); END;
CREATE TRIGGER release_readiness_reports_no_update BEFORE UPDATE ON release_readiness_reports BEGIN SELECT RAISE(ABORT, 'Release Readiness Reports are immutable'); END;
CREATE TRIGGER release_readiness_reports_no_delete BEFORE DELETE ON release_readiness_reports BEGIN SELECT RAISE(ABORT, 'Release Readiness Reports are immutable'); END;
CREATE TRIGGER release_readiness_report_head_identity_immutable BEFORE UPDATE ON release_readiness_report_head WHEN NEW.singleton != OLD.singleton OR NEW.report_id != OLD.report_id BEGIN SELECT RAISE(ABORT, 'Release Readiness Report head identity is immutable'); END;
CREATE TRIGGER release_readiness_report_head_no_delete BEFORE DELETE ON release_readiness_report_head BEGIN SELECT RAISE(ABORT, 'Release Readiness Report head cannot be deleted'); END;
CREATE TRIGGER source_artifacts_no_update
BEFORE UPDATE ON source_artifacts
BEGIN
  SELECT RAISE(ABORT, 'Source Artifacts are immutable');
END;
CREATE TRIGGER source_artifacts_no_delete
BEFORE DELETE ON source_artifacts
BEGIN
  SELECT RAISE(ABORT, 'Source Artifacts are immutable');
END;
CREATE TRIGGER source_artifact_versions_no_update
BEFORE UPDATE ON source_artifact_versions
BEGIN
  SELECT RAISE(ABORT, 'Source Artifact Versions are immutable');
END;
CREATE TRIGGER source_artifact_versions_no_delete
BEFORE DELETE ON source_artifact_versions
BEGIN
  SELECT RAISE(ABORT, 'Source Artifact Versions are immutable');
END;
CREATE TRIGGER source_relationships_no_update
BEFORE UPDATE ON source_relationships
BEGIN
  SELECT RAISE(ABORT, 'Source relationships are immutable');
END;
CREATE TRIGGER source_relationships_no_delete
BEFORE DELETE ON source_relationships
BEGIN
  SELECT RAISE(ABORT, 'Source relationships are immutable');
END;
CREATE TRIGGER change_assessments_no_update BEFORE UPDATE ON change_assessments BEGIN SELECT RAISE(ABORT, 'Change Assessments are immutable'); END;
CREATE TRIGGER change_assessments_no_delete BEFORE DELETE ON change_assessments BEGIN SELECT RAISE(ABORT, 'Change Assessments are immutable'); END;
CREATE TRIGGER change_assessment_impacts_no_update BEFORE UPDATE ON change_assessment_impacts BEGIN SELECT RAISE(ABORT, 'Change Assessment impacts are immutable'); END;
CREATE TRIGGER change_assessment_impacts_no_delete BEFORE DELETE ON change_assessment_impacts BEGIN SELECT RAISE(ABORT, 'Change Assessment impacts are immutable'); END;
CREATE TRIGGER change_assessment_decisions_no_update BEFORE UPDATE ON change_assessment_decisions BEGIN SELECT RAISE(ABORT, 'Change Assessment decisions are immutable'); END;
CREATE TRIGGER change_assessment_decisions_no_delete BEFORE DELETE ON change_assessment_decisions BEGIN SELECT RAISE(ABORT, 'Change Assessment decisions are immutable'); END;
CREATE TRIGGER change_assessment_resolutions_no_update BEFORE UPDATE ON change_assessment_resolutions BEGIN SELECT RAISE(ABORT, 'Change Assessment resolutions are immutable'); END;
CREATE TRIGGER change_assessment_resolutions_no_delete BEFORE DELETE ON change_assessment_resolutions BEGIN SELECT RAISE(ABORT, 'Change Assessment resolutions are immutable'); END;
CREATE TRIGGER parse_attempts_terminal_facts_no_rewrite
BEFORE UPDATE ON parse_attempts
WHEN OLD.status != 'running'
  OR NEW.status = 'running'
  OR NEW.attempt_sequence != OLD.attempt_sequence
  OR NEW.attempt_id != OLD.attempt_id
  OR NEW.artifact_id != OLD.artifact_id
  OR NEW.version != OLD.version
  OR NEW.started_at != OLD.started_at
BEGIN
  SELECT RAISE(ABORT, 'Parse attempt terminal facts are immutable');
END;
CREATE TRIGGER parse_attempts_no_delete
BEFORE DELETE ON parse_attempts
BEGIN
  SELECT RAISE(ABORT, 'Parse attempts are immutable');
END;
CREATE TRIGGER parsed_documents_no_update
BEFORE UPDATE ON parsed_documents
BEGIN
  SELECT RAISE(ABORT, 'Parsed Documents are immutable');
END;
CREATE TRIGGER parsed_documents_no_delete
BEFORE DELETE ON parsed_documents
BEGIN
  SELECT RAISE(ABORT, 'Parsed Documents are immutable');
END;
CREATE TRIGGER evidence_locations_no_update
BEFORE UPDATE ON evidence_locations
BEGIN
  SELECT RAISE(ABORT, 'Evidence locations are immutable');
END;
CREATE TRIGGER evidence_locations_no_delete
BEFORE DELETE ON evidence_locations
BEGIN
  SELECT RAISE(ABORT, 'Evidence locations are immutable');
END;
CREATE TRIGGER evidence_embeddings_no_update
BEFORE UPDATE ON evidence_embeddings
BEGIN
  SELECT RAISE(ABORT, 'Evidence embeddings are immutable');
END;
CREATE TRIGGER evidence_embeddings_no_delete
BEFORE DELETE ON evidence_embeddings
BEGIN
  SELECT RAISE(ABORT, 'Evidence embeddings are immutable');
END;
CREATE TRIGGER agent_profiles_no_update
BEFORE UPDATE ON agent_profiles
BEGIN
  SELECT RAISE(ABORT, 'Agent Profiles are immutable');
END;
CREATE TRIGGER agent_profiles_no_delete
BEFORE DELETE ON agent_profiles
BEGIN
  SELECT RAISE(ABORT, 'Agent Profiles are immutable');
END;
CREATE TRIGGER agent_profile_versions_no_update
BEFORE UPDATE ON agent_profile_versions
BEGIN
  SELECT RAISE(ABORT, 'Agent Profile Versions are immutable');
END;
CREATE TRIGGER agent_profile_versions_no_delete
BEFORE DELETE ON agent_profile_versions
BEGIN
  SELECT RAISE(ABORT, 'Agent Profile Versions are immutable');
END;
CREATE TRIGGER agent_profile_heads_identity_immutable
BEFORE UPDATE ON agent_profile_heads
WHEN NEW.profile_id != OLD.profile_id
BEGIN
  SELECT RAISE(ABORT, 'Agent Profile head identity is immutable');
END;
CREATE TRIGGER agent_profile_heads_no_delete
BEFORE DELETE ON agent_profile_heads
BEGIN
  SELECT RAISE(ABORT, 'Agent Profile heads cannot be deleted');
END;
CREATE TRIGGER tender_tasks_no_update
BEFORE UPDATE ON tender_tasks
BEGIN
  SELECT RAISE(ABORT, 'Tender Tasks are immutable');
END;
CREATE TRIGGER tender_tasks_no_delete
BEFORE DELETE ON tender_tasks
BEGIN
  SELECT RAISE(ABORT, 'Tender Tasks are immutable');
END;
CREATE TRIGGER provider_threads_only_archive
BEFORE UPDATE ON provider_threads
WHEN NEW.profile_id != OLD.profile_id
  OR NEW.profile_version != OLD.profile_version
  OR NEW.thread_ref != OLD.thread_ref
  OR NEW.created_at != OLD.created_at
  OR NOT (
    (OLD.status = 'active' AND NEW.status = 'archive_pending'
      AND NEW.archived_at IS NULL)
    OR (OLD.status = 'archive_pending' AND NEW.status = 'archived'
      AND NEW.archived_at IS NOT NULL)
  )
BEGIN
  SELECT RAISE(ABORT, 'Provider Threads are immutable except for archival');
END;
CREATE TRIGGER provider_threads_no_delete
BEFORE DELETE ON provider_threads
BEGIN
  SELECT RAISE(ABORT, 'Provider Threads are immutable');
END;
CREATE TRIGGER provider_thread_exposures_no_update
BEFORE UPDATE ON provider_thread_exposures
BEGIN
  SELECT RAISE(ABORT, 'Provider Thread exposures are immutable');
END;
CREATE TRIGGER provider_thread_exposures_no_delete
BEFORE DELETE ON provider_thread_exposures
BEGIN
  SELECT RAISE(ABORT, 'Provider Thread exposures are immutable');
END;
CREATE TRIGGER agent_runs_terminal_facts_no_rewrite
BEFORE UPDATE ON agent_runs
WHEN OLD.status != 'running'
  OR NEW.run_sequence != OLD.run_sequence
  OR NEW.run_id != OLD.run_id
  OR NEW.task_id != OLD.task_id
  OR NEW.profile_id != OLD.profile_id
  OR NEW.profile_version != OLD.profile_version
  OR NEW.retry_of_run_id IS NOT OLD.retry_of_run_id
  OR NEW.permission_grant_json != OLD.permission_grant_json
  OR NEW.started_at != OLD.started_at
  OR (
    NEW.status = 'running'
    AND (
      NEW.failure_json IS NOT NULL
      OR NEW.completed_at IS NOT NULL
      OR (OLD.provider_thread_ref IS NOT NULL
          AND NEW.provider_thread_ref IS NOT OLD.provider_thread_ref)
      OR (OLD.provider_turn_ref IS NOT NULL
          AND NEW.provider_turn_ref IS NOT OLD.provider_turn_ref)
      OR (NEW.provider_turn_ref IS NOT NULL AND NEW.provider_thread_ref IS NULL)
    )
  )
  OR (
    NEW.status != 'running'
    AND (
      (OLD.provider_thread_ref IS NOT NULL
       AND NEW.provider_thread_ref IS NOT OLD.provider_thread_ref)
      OR (OLD.provider_turn_ref IS NOT NULL
          AND NEW.provider_turn_ref IS NOT OLD.provider_turn_ref)
    )
  )
BEGIN
  SELECT RAISE(ABORT, 'Agent Run terminal facts are immutable');
END;
CREATE TRIGGER agent_runs_no_delete
BEFORE DELETE ON agent_runs
BEGIN
  SELECT RAISE(ABORT, 'Agent Runs are immutable');
END;
CREATE TRIGGER agent_run_provider_bindings_no_update
BEFORE UPDATE ON agent_run_provider_bindings
BEGIN
  SELECT RAISE(ABORT, 'Agent Run provider bindings are immutable');
END;
CREATE TRIGGER agent_run_provider_bindings_no_delete
BEFORE DELETE ON agent_run_provider_bindings
BEGIN
  SELECT RAISE(ABORT, 'Agent Run provider bindings are immutable');
END;
CREATE TRIGGER agent_run_recovery_dispositions_no_update
BEFORE UPDATE ON agent_run_recovery_dispositions
BEGIN
  SELECT RAISE(ABORT, 'Agent Run recovery dispositions are immutable');
END;
CREATE TRIGGER agent_run_recovery_dispositions_no_delete
BEFORE DELETE ON agent_run_recovery_dispositions
BEGIN
  SELECT RAISE(ABORT, 'Agent Run recovery dispositions are immutable');
END;
CREATE TRIGGER agent_access_requests_only_decide
BEFORE UPDATE ON agent_access_requests
WHEN OLD.status != 'blocked'
  OR NEW.request_id != OLD.request_id
  OR NEW.run_id != OLD.run_id
  OR NEW.request_json != OLD.request_json
  OR NEW.requested_at != OLD.requested_at
  OR NEW.status NOT IN ('approved', 'denied', 'superseded')
BEGIN
  SELECT RAISE(ABORT, 'Agent Access Requests are immutable except for one decision');
END;
CREATE TRIGGER agent_access_requests_no_delete
BEFORE DELETE ON agent_access_requests
BEGIN
  SELECT RAISE(ABORT, 'Agent Access Requests are immutable');
END;
CREATE TRIGGER agent_access_revocations_no_update
BEFORE UPDATE ON agent_access_revocations
BEGIN
  SELECT RAISE(ABORT, 'Agent Access Revocations are immutable');
END;
CREATE TRIGGER agent_access_revocations_no_delete
BEFORE DELETE ON agent_access_revocations
BEGIN
  SELECT RAISE(ABORT, 'Agent Access Revocations are immutable');
END;
CREATE TRIGGER agent_run_cancellations_no_update
BEFORE UPDATE ON agent_run_cancellations
BEGIN
  SELECT RAISE(ABORT, 'Agent Run cancellations are immutable');
END;
CREATE TRIGGER agent_run_cancellations_no_delete
BEFORE DELETE ON agent_run_cancellations
BEGIN
  SELECT RAISE(ABORT, 'Agent Run cancellations are immutable');
END;
CREATE TRIGGER agent_tool_call_reservations_no_update
BEFORE UPDATE ON agent_tool_call_reservations
BEGIN
  SELECT RAISE(ABORT, 'Agent Tool Call reservations are immutable');
END;
CREATE TRIGGER agent_tool_call_reservations_no_delete
BEFORE DELETE ON agent_tool_call_reservations
BEGIN
  SELECT RAISE(ABORT, 'Agent Tool Call reservations are immutable');
END;
CREATE TRIGGER agent_tool_call_results_no_update
BEFORE UPDATE ON agent_tool_call_results
BEGIN
  SELECT RAISE(ABORT, 'Agent Tool Call results are immutable');
END;
CREATE TRIGGER agent_tool_call_results_no_delete
BEFORE DELETE ON agent_tool_call_results
BEGIN
  SELECT RAISE(ABORT, 'Agent Tool Call results are immutable');
END;
CREATE TRIGGER provider_events_no_update
BEFORE UPDATE ON provider_events
BEGIN
  SELECT RAISE(ABORT, 'Provider Events are immutable');
END;
CREATE TRIGGER provider_events_no_delete
BEFORE DELETE ON provider_events
BEGIN
  SELECT RAISE(ABORT, 'Provider Events are immutable');
END;
CREATE TRIGGER proposed_agent_results_no_update
BEFORE UPDATE ON proposed_agent_results
BEGIN
  SELECT RAISE(ABORT, 'Proposed Agent Results are immutable');
END;
CREATE TRIGGER proposed_agent_results_no_delete
BEFORE DELETE ON proposed_agent_results
BEGIN
  SELECT RAISE(ABORT, 'Proposed Agent Results are immutable');
END;
CREATE TRIGGER agent_run_rejected_outputs_no_update
BEFORE UPDATE ON agent_run_rejected_outputs
BEGIN
  SELECT RAISE(ABORT, 'Rejected Agent Outputs are immutable');
END;
CREATE TRIGGER agent_run_rejected_outputs_no_delete
BEFORE DELETE ON agent_run_rejected_outputs
BEGIN
  SELECT RAISE(ABORT, 'Rejected Agent Outputs are immutable');
END;
CREATE TRIGGER tender_records_no_update
BEFORE UPDATE ON tender_records
BEGIN
  SELECT RAISE(ABORT, 'Tender Records are immutable');
END;
CREATE TRIGGER tender_records_no_delete
BEFORE DELETE ON tender_records
BEGIN
  SELECT RAISE(ABORT, 'Tender Records are immutable');
END;
CREATE TRIGGER tender_record_authorities_no_update
BEFORE UPDATE ON tender_record_authorities
BEGIN
  SELECT RAISE(ABORT, 'Tender Record authorities are immutable');
END;
CREATE TRIGGER tender_record_authorities_no_delete
BEFORE DELETE ON tender_record_authorities
BEGIN
  SELECT RAISE(ABORT, 'Tender Record authorities are immutable');
END;
CREATE TRIGGER tender_record_versions_no_update
BEFORE UPDATE ON tender_record_versions
BEGIN
  SELECT RAISE(ABORT, 'Tender Record Versions are immutable');
END;
CREATE TRIGGER tender_record_versions_no_delete
BEFORE DELETE ON tender_record_versions
BEGIN
  SELECT RAISE(ABORT, 'Tender Record Versions are immutable');
END;
CREATE TRIGGER tender_record_reviews_no_update
BEFORE UPDATE ON tender_record_reviews
BEGIN
  SELECT RAISE(ABORT, 'Tender Record Reviews are immutable');
END;
CREATE TRIGGER tender_record_reviews_no_delete
BEFORE DELETE ON tender_record_reviews
BEGIN
  SELECT RAISE(ABORT, 'Tender Record Reviews are immutable');
END;
CREATE TRIGGER bid_decision_packages_no_update
BEFORE UPDATE ON bid_decision_packages
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Packages are immutable');
END;
CREATE TRIGGER bid_decision_packages_no_delete
BEFORE DELETE ON bid_decision_packages
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Packages are immutable');
END;
CREATE TRIGGER bid_decision_package_versions_no_update
BEFORE UPDATE ON bid_decision_package_versions
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Package Versions are immutable');
END;
CREATE TRIGGER bid_decision_package_versions_no_delete
BEFORE DELETE ON bid_decision_package_versions
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Package Versions are immutable');
END;
CREATE TRIGGER bid_compliance_rows_no_update
BEFORE UPDATE ON bid_compliance_rows
BEGIN
  SELECT RAISE(ABORT, 'Compliance Matrix rows are immutable');
END;
CREATE TRIGGER bid_compliance_rows_no_delete
BEFORE DELETE ON bid_compliance_rows
BEGIN
  SELECT RAISE(ABORT, 'Compliance Matrix rows are immutable');
END;
CREATE TRIGGER bid_decision_package_record_bindings_no_update
BEFORE UPDATE ON bid_decision_package_record_bindings
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Package record bindings are immutable');
END;
CREATE TRIGGER bid_decision_package_record_bindings_no_delete
BEFORE DELETE ON bid_decision_package_record_bindings
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Package record bindings are immutable');
END;
CREATE TRIGGER bid_decision_package_reviews_no_update
BEFORE UPDATE ON bid_decision_package_reviews
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Package Reviews are immutable');
END;
CREATE TRIGGER bid_decision_package_reviews_no_delete
BEFORE DELETE ON bid_decision_package_reviews
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Package Reviews are immutable');
END;
CREATE TRIGGER bid_decision_approval_records_no_update
BEFORE UPDATE ON bid_decision_approval_records
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Approval Records are immutable');
END;
CREATE TRIGGER bid_decision_approval_records_no_delete
BEFORE DELETE ON bid_decision_approval_records
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Approval Records are immutable');
END;
CREATE TRIGGER bid_decision_return_rework_dispositions_no_update
BEFORE UPDATE ON bid_decision_return_rework_dispositions
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Return rework dispositions are immutable');
END;
CREATE TRIGGER bid_decision_return_rework_dispositions_no_delete
BEFORE DELETE ON bid_decision_return_rework_dispositions
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Return rework dispositions are immutable');
END;
CREATE TRIGGER bid_decision_approval_invalidations_no_update
BEFORE UPDATE ON bid_decision_approval_invalidations
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Approval invalidations are immutable');
END;
CREATE TRIGGER bid_decision_approval_invalidations_no_delete
BEFORE DELETE ON bid_decision_approval_invalidations
BEGIN
  SELECT RAISE(ABORT, 'Bid Decision Approval invalidations are immutable');
END;
CREATE TRIGGER work_plans_no_update
BEFORE UPDATE ON work_plans
BEGIN
  SELECT RAISE(ABORT, 'Work Plans are immutable');
END;
CREATE TRIGGER work_plans_no_delete
BEFORE DELETE ON work_plans
BEGIN
  SELECT RAISE(ABORT, 'Work Plans are immutable');
END;
CREATE TRIGGER work_plan_versions_no_update
BEFORE UPDATE ON work_plan_versions
BEGIN
  SELECT RAISE(ABORT, 'Work Plan Versions are immutable');
END;
CREATE TRIGGER work_plan_versions_no_delete
BEFORE DELETE ON work_plan_versions
BEGIN
  SELECT RAISE(ABORT, 'Work Plan Versions are immutable');
END;
CREATE TRIGGER work_plan_heads_identity_immutable
BEFORE UPDATE ON work_plan_heads
WHEN NEW.plan_id != OLD.plan_id
BEGIN
  SELECT RAISE(ABORT, 'Work Plan head identity is immutable');
END;
CREATE TRIGGER work_plan_heads_no_delete
BEFORE DELETE ON work_plan_heads
BEGIN
  SELECT RAISE(ABORT, 'Work Plan heads cannot be deleted');
END;
CREATE TRIGGER work_plan_approvals_no_update
BEFORE UPDATE ON work_plan_approvals
BEGIN
  SELECT RAISE(ABORT, 'Work Plan Approvals are immutable');
END;
CREATE TRIGGER work_plan_approvals_no_delete
BEFORE DELETE ON work_plan_approvals
BEGIN
  SELECT RAISE(ABORT, 'Work Plan Approvals are immutable');
END;
CREATE TRIGGER production_activations_identity_immutable
BEFORE UPDATE OF activation_id, plan_id, plan_version, plan_manifest_sha256,
                 activated_by, acting_role, created_at ON production_activations
BEGIN
  SELECT RAISE(ABORT, 'Production Activation identity is immutable');
END;
CREATE TRIGGER production_activations_no_delete
BEFORE DELETE ON production_activations
BEGIN
  SELECT RAISE(ABORT, 'Production Activations cannot be deleted');
END;
CREATE TRIGGER production_tasks_definition_immutable
BEFORE UPDATE OF production_task_id, activation_id, task_key,
                 task_definition_json, task_definition_sha256, created_at ON production_tasks
BEGIN
  SELECT RAISE(ABORT, 'Production Task definitions are immutable');
END;
CREATE TRIGGER production_tasks_no_delete
BEFORE DELETE ON production_tasks
BEGIN
  SELECT RAISE(ABORT, 'Production Tasks cannot be deleted');
END;
CREATE TRIGGER production_task_attempts_no_update
BEFORE UPDATE ON production_task_attempts
BEGIN
  SELECT RAISE(ABORT, 'Production Task Attempts are immutable');
END;
CREATE TRIGGER production_task_attempts_no_delete
BEFORE DELETE ON production_task_attempts
BEGIN
  SELECT RAISE(ABORT, 'Production Task Attempts cannot be deleted');
END;
CREATE TRIGGER production_artifact_versions_no_update
BEFORE UPDATE ON production_artifact_versions
BEGIN
  SELECT RAISE(ABORT, 'Production Artifact Versions are immutable');
END;
CREATE TRIGGER production_artifact_versions_no_delete
BEFORE DELETE ON production_artifact_versions
BEGIN
  SELECT RAISE(ABORT, 'Production Artifact Versions are immutable');
END;
CREATE TRIGGER production_reviews_no_update
BEFORE UPDATE ON production_reviews
BEGIN
  SELECT RAISE(ABORT, 'Production Reviews are immutable');
END;
CREATE TRIGGER production_reviews_no_delete
BEFORE DELETE ON production_reviews
BEGIN
  SELECT RAISE(ABORT, 'Production Reviews are immutable');
END;
CREATE TRIGGER production_review_findings_no_update
BEFORE UPDATE ON production_review_findings
BEGIN
  SELECT RAISE(ABORT, 'Production Review Findings are immutable');
END;
CREATE TRIGGER production_review_findings_no_delete
BEFORE DELETE ON production_review_findings
BEGIN
  SELECT RAISE(ABORT, 'Production Review Findings are immutable');
END;
CREATE TRIGGER production_finding_dispositions_no_update
BEFORE UPDATE ON production_finding_dispositions
BEGIN
  SELECT RAISE(ABORT, 'Production Finding Dispositions are immutable');
END;
CREATE TRIGGER production_finding_dispositions_no_delete
BEFORE DELETE ON production_finding_dispositions
BEGIN
  SELECT RAISE(ABORT, 'Production Finding Dispositions are immutable');
END;
CREATE TRIGGER production_integration_readiness_no_update
BEFORE UPDATE ON production_integration_readiness
BEGIN
  SELECT RAISE(ABORT, 'Production Integration Readiness is immutable');
END;
CREATE TRIGGER production_integration_readiness_no_delete
BEFORE DELETE ON production_integration_readiness
BEGIN
  SELECT RAISE(ABORT, 'Production Integration Readiness is immutable');
END;
CREATE TRIGGER production_task_carry_forwards_no_update
BEFORE UPDATE ON production_task_carry_forwards
BEGIN
  SELECT RAISE(ABORT, 'Production Task carry-forwards are immutable');
END;
CREATE TRIGGER production_task_carry_forwards_no_delete
BEFORE DELETE ON production_task_carry_forwards
BEGIN
  SELECT RAISE(ABORT, 'Production Task carry-forwards are immutable');
END;
CREATE TRIGGER coordinated_bid_baseline_versions_no_update
BEFORE UPDATE ON coordinated_bid_baseline_versions
BEGIN
  SELECT RAISE(ABORT, 'Coordinated Bid Baseline versions are immutable');
END;
CREATE TRIGGER coordinated_bid_baseline_versions_no_delete
BEFORE DELETE ON coordinated_bid_baseline_versions
BEGIN
  SELECT RAISE(ABORT, 'Coordinated Bid Baseline versions are immutable');
END;
CREATE TRIGGER coordinated_bid_baseline_approvals_no_update
BEFORE UPDATE ON coordinated_bid_baseline_approvals
BEGIN
  SELECT RAISE(ABORT, 'Coordinated Bid Baseline approvals are immutable');
END;
CREATE TRIGGER coordinated_bid_baseline_approvals_no_delete
BEFORE DELETE ON coordinated_bid_baseline_approvals
BEGIN
  SELECT RAISE(ABORT, 'Coordinated Bid Baseline approvals are immutable');
END;
CREATE TRIGGER audit_events_no_update
BEFORE UPDATE ON audit_events
BEGIN
  SELECT RAISE(ABORT, 'Audit Events are immutable');
END;
CREATE TRIGGER audit_events_no_delete
BEFORE DELETE ON audit_events
BEGIN
  SELECT RAISE(ABORT, 'Audit Events are immutable');
END;
CREATE INDEX audit_events_type_created_at
ON audit_events(event_type, created_at);
"#;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct CreateTenderCommand {
    #[garde(length(bytes, min = 1, max = 200))]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct ReviseTenderCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 200))]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct OpenTenderCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS, Validate)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct RegisterTenderContentCommand {
    #[garde(length(bytes, min = 32, max = 32))]
    pub tender_id: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub logical_id: String,
    #[garde(length(bytes, min = 1, max = 100))]
    pub media_type: String,
    #[garde(length(min = 1, max = 16777216))]
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderSummary {
    pub tender_id: String,
    pub name: String,
    pub revision: u32,
    pub lifecycle_phase: TenderLifecyclePhase,
    pub audit_event_count: u64,
    pub audit_chain_head: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderLifecyclePhase {
    Intake,
    BidDecision,
    TenderPlanning,
    ActiveProduction,
    IntegratedReview,
    ChangeAssessment,
    PackageProduction,
    FinalReview,
    Declined,
}

impl TenderLifecyclePhase {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Intake => "intake",
            Self::BidDecision => "bid_decision",
            Self::TenderPlanning => "tender_planning",
            Self::ActiveProduction => "active_production",
            Self::IntegratedReview => "integrated_review",
            Self::ChangeAssessment => "change_assessment",
            Self::PackageProduction => "package_production",
            Self::FinalReview => "final_review",
            Self::Declined => "declined",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        match value {
            "intake" => Ok(Self::Intake),
            "bid_decision" => Ok(Self::BidDecision),
            "tender_planning" => Ok(Self::TenderPlanning),
            "active_production" => Ok(Self::ActiveProduction),
            "integrated_review" => Ok(Self::IntegratedReview),
            "change_assessment" => Ok(Self::ChangeAssessment),
            "package_production" => Ok(Self::PackageProduction),
            "final_review" => Ok(Self::FinalReview),
            "declined" => Ok(Self::Declined),
            _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderCatalogueEntry {
    pub tender_id: String,
    pub summary: Option<TenderSummary>,
    pub integrity: TenderIntegrityReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContentVersionSummary {
    pub logical_id: String,
    pub revision: u32,
    pub sha256: String,
    pub size_bytes: u64,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderInspection {
    pub summary: TenderSummary,
    pub content_object_count: u64,
    pub content_version_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderIntegrityState {
    Ready,
    RecoveryRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderIntegrityIssue {
    AuditChainInvalid,
    DatabaseIntegrityInvalid,
    InspectionUnavailable,
    ReferencedContentMissing,
    ReferencedContentMismatch,
    ManifestInvalid,
    SchemaMismatch,
    StorageLayoutInvalid,
    TenderIdentityMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderRecoveryChoice {
    RestoreVerifiedBackup,
    PurgeTender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TenderIntegrityReport {
    pub tender_id: String,
    pub state: TenderIntegrityState,
    pub issues: Vec<TenderIntegrityIssue>,
    pub recovery_choices: Vec<TenderRecoveryChoice>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StartupReconciliationReport {
    pub removed_tender_candidates: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TenderErrorCode {
    InsufficientSpace,
    IntegrityFailed,
    InvalidCommand,
    RequestBudgetExceeded,
    NotFound,
    OauthAlreadyRunning,
    OauthPortBlocked,
    OperationTimedOut,
    RecoveryRequired,
    LocalDocumentToolsRequired,
    AiProviderRequired,
    SetupRequired,
    StoreUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TenderCommandError {
    pub code: TenderErrorCode,
}

impl TenderCommandError {
    pub(crate) fn new(code: TenderErrorCode) -> Self {
        Self { code }
    }
}

impl std::fmt::Display for TenderCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Tender command failed: {:?}", self.code)
    }
}

impl std::error::Error for TenderCommandError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TenderId(String);

impl TenderId {
    pub(crate) fn parse(value: &str) -> Result<Self, TenderCommandError> {
        if value.len() == 32
            && value
                .chars()
                .all(|character| character.is_ascii_digit() || matches!(character, 'a'..='f'))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
        }
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) struct TenderStore {
    root: std::path::PathBuf,
    connection: Connection,
    recovery_required: bool,
    archived: bool,
}

struct RawDocumentRegisterEntry {
    artifact_id: String,
    version: u32,
    package_path: String,
    language: String,
    document_type: String,
    media_type: Option<String>,
    sha256: Option<String>,
    size_bytes: i64,
    registration_state: String,
    parse_state: String,
    parse_exception_code: Option<String>,
    supersession_state: String,
    exception_code: Option<String>,
}

struct RawEvidenceLocation {
    ordinal: u32,
    kind: String,
    structural_path: String,
    provenance_json: String,
    section: Option<String>,
    paragraph_number: Option<u32>,
    table_number: Option<u32>,
    sheet_name: Option<String>,
    cell_range: Option<String>,
    original_text: String,
    translated_text: Option<String>,
    language: String,
    direction: String,
}

impl RawEvidenceLocation {
    fn read(row: &rusqlite::Row<'_>, start: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            ordinal: row.get(start)?,
            kind: row.get(start + 1)?,
            structural_path: row.get(start + 2)?,
            provenance_json: row.get(start + 3)?,
            section: row.get(start + 4)?,
            paragraph_number: row.get(start + 5)?,
            table_number: row.get(start + 6)?,
            sheet_name: row.get(start + 7)?,
            cell_range: row.get(start + 8)?,
            original_text: row.get(start + 9)?,
            translated_text: row.get(start + 10)?,
            language: row.get(start + 11)?,
            direction: row.get(start + 12)?,
        })
    }

    fn into_domain(self) -> Result<EvidenceLocation, TenderCommandError> {
        let provenance: Vec<EvidenceRegion> = serde_json::from_str(&self.provenance_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if serde_json_canonicalizer::to_string(&provenance)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            != self.provenance_json
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        if provenance.iter().any(|region| {
            region.page_number == 0
                || matches!((region.char_start, region.char_end), (Some(start), Some(end)) if end < start)
                || matches!((region.char_start, region.char_end), (Some(_), None) | (None, Some(_)))
        }) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(EvidenceLocation {
            ordinal: self.ordinal,
            kind: EvidenceLocationKind::parse(&self.kind)?,
            structural_path: self.structural_path,
            provenance,
            section: self.section,
            paragraph_number: self.paragraph_number,
            table_number: self.table_number,
            sheet_name: self.sheet_name,
            cell_range: self.cell_range,
            original_text: self.original_text,
            translated_text: self.translated_text,
            language: EvidenceLanguage::parse(&self.language)?,
            direction: TextDirection::parse(&self.direction)?,
        })
    }
}

impl TenderStore {
    /// Read the small, SQL-backed workspace projection without opening the
    /// writer store.  Workspace refreshes run frequently, so they must not
    /// perform the content-object hash walk that `open` and integrity
    /// inspection intentionally perform.
    pub(crate) fn read_workspace_tender(
        root: &Path,
        expected_tender_id: &TenderId,
    ) -> Result<ManagerWorkspaceTender, TenderCommandError> {
        let recovery = || TenderCommandError::new(TenderErrorCode::RecoveryRequired);
        validate_tender_store_layout(root).map_err(|_| recovery())?;
        let database = root.join("tender.sqlite");
        // The schema comparison below includes sqlite-vec's virtual table
        // module. Register it before opening the first connection in a fresh
        // process so that the expected schema can be constructed faithfully.
        register_sqlite_vec()?;
        let connection = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| recovery())?;
        configure_reader(&connection).map_err(|_| recovery())?;
        let schema_version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|_| recovery())?;
        if schema_version != TENDER_SCHEMA_VERSION {
            return Err(recovery());
        }
        // The workspace sidebar projection must not treat a store with an
        // altered schema as active merely because its core rows are readable.
        // In particular, a missing audit trigger is discovered only by the
        // exact schema comparison below.  Keep the projection read-only and
        // cheap, but surface the same recovery state that a full integrity
        // inspection would report so cold-start workspace refreshes can still
        // expose recovery-specific actions.
        if inspect_store_structure(&connection, expected_tender_id)
            .map_err(|_| recovery())?
            .is_some()
        {
            return Err(recovery());
        }

        type WorkspaceTenderProjectionRow =
            (String, u32, String, String, bool, Option<String>, bool);
        let projection: Result<WorkspaceTenderProjectionRow, _> = connection.query_row(
            "SELECT tender.tender_id,
                    tender.current_revision,
                    tender.lifecycle_phase,
                    tender_revisions.name,
                    tender_retention.state = 'archived',
                    manager_workspace_state.last_activity_at,
                    EXISTS(
                      SELECT 1 FROM production_tasks
                      WHERE status IN (
                        'review_ready', 'remediation_ready', 'query_blocked',
                        'attempt_limit_reached', 'indeterminate', 'failed'
                      )
                    ) OR EXISTS(
                      SELECT 1 FROM manager_intake_runs
                      WHERE stage IN (
                        'waiting_for_local_tools', 'waiting_for_provider_approval',
                        'waiting_for_provider', 'waiting_for_engineer',
                        'bid_decision_ready', 'failed'
                      )
                    )
             FROM tender
             JOIN tender_revisions
               ON tender_revisions.revision = tender.current_revision
             JOIN tender_retention
               ON tender_retention.singleton = 1
             JOIN manager_workspace_state
               ON manager_workspace_state.singleton = 1
             WHERE tender.singleton = 1",
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
                ))
            },
        );
        let (
            tender_id,
            revision,
            lifecycle_phase,
            name,
            archived,
            last_activity_at,
            needs_engineer,
        ) = projection.map_err(|_| recovery())?;
        if tender_id != expected_tender_id.as_str() {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }
        if name.is_empty() || name.len() > MAX_TENDER_NAME_BYTES {
            return Err(recovery());
        }

        Ok(ManagerWorkspaceTender {
            tender_id,
            name,
            revision,
            phase: TenderLifecyclePhase::parse(&lifecycle_phase)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::RecoveryRequired))?,
            needs_engineer,
            state: if archived {
                ManagerWorkspaceTenderState::Archived
            } else {
                ManagerWorkspaceTenderState::Active
            },
            // These boundaries are deliberately authoritative only on the
            // selected, fully-open store.  The lightweight sidebar projection
            // never grants a retention command permission.
            can_archive: false,
            can_delete: false,
            last_activity_at,
        })
    }

    fn create(root: &Path, tender_id: &TenderId, name: &str) -> Result<Self, TenderCommandError> {
        register_sqlite_vec()?;
        fs::create_dir(root).map_err(store_unavailable)?;
        for directory in ["content", "runs", "staging"] {
            fs::create_dir(root.join(directory)).map_err(store_unavailable)?;
        }

        let mut connection = Connection::open(root.join("tender.sqlite")).map_err(sql_error)?;
        configure_writer(&mut connection)?;
        connection.execute_batch(TENDER_SCHEMA).map_err(sql_error)?;
        connection
            .pragma_update(None, "user_version", TENDER_SCHEMA_VERSION)
            .map_err(sql_error)?;

        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO tender (
                   singleton, tender_id, current_revision, lifecycle_phase, created_at
                 ) VALUES (1, ?1, 1, 'intake', ?2)",
                params![tender_id.as_str(), created_at],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO tender_revisions (revision, tender_id, name, created_at) VALUES (1, ?1, ?2, ?3)",
                params![tender_id.as_str(), name, created_at],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO tender_ai_execution_binding (
                   singleton, revision, selection_json, readiness, status_summary, updated_at
                 ) VALUES (1, 1, NULL, 'local_only', ?1, ?1)",
                ["No AI provider is selected; local-only Tender work remains available."],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO tender_retention (
                   singleton, state, decision_id, decision_manifest_sha256, updated_at
                 ) VALUES (1, 'active', NULL, NULL, ?1)",
                [&created_at],
            )
            .map_err(sql_error)?;
        workspace::initialize_manager_workspace(&transaction, name, &created_at)?;
        for role in BootstrapRole::SPECIALISTS {
            let profile = bootstrap_profile(role, random_identifier(&transaction)?);
            agent_records::insert_profile(
                &transaction,
                role.stable_identity(),
                &profile,
                &created_at,
            )?;
        }
        manager_intake::initialize_manager_profile(&transaction, &created_at)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "tender_created",
            1,
            json!({ "name": name }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;

        Ok(Self {
            root: root.to_path_buf(),
            connection,
            recovery_required: false,
            archived: false,
        })
    }

    pub(crate) fn inspect_tender_ai_execution_binding(
        &self,
    ) -> Result<TenderAiExecutionBinding, TenderCommandError> {
        let row = self
            .connection
            .query_row(
                "SELECT revision, selection_json, readiness, status_summary
                 FROM tender_ai_execution_binding WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        let revision = u64::try_from(row.0)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let selection = row
            .1
            .as_deref()
            .map(serde_json::from_str::<AiExecutionSelection>)
            .transpose()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let readiness = parse_tender_ai_selection_readiness(&row.2)?;
        if (readiness == TenderAiSelectionReadiness::LocalOnly) != selection.is_none() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        Ok(TenderAiExecutionBinding {
            revision,
            selection,
            readiness,
            status_summary: row.3,
        })
    }

    pub(crate) fn seed_application_ai_execution_binding(
        &mut self,
        application_home: &Path,
        tender_id: &TenderId,
    ) -> Result<TenderAiExecutionBinding, TenderCommandError> {
        let current = self.inspect_tender_ai_execution_binding()?;
        if current.revision != 1
            || current.selection.is_some()
            || current.readiness != TenderAiSelectionReadiness::LocalOnly
        {
            return Ok(current);
        }
        let default =
            crate::application_settings::default_tender_ai_execution_binding(application_home)?;
        self.update_tender_ai_execution_binding(
            tender_id,
            current.revision,
            default.selection,
            default.readiness,
            &default.status_summary,
        )
    }

    pub(crate) fn required_tender_ai_execution_selection(
        &self,
    ) -> Result<AiExecutionSelection, TenderCommandError> {
        let binding = self.inspect_tender_ai_execution_binding()?;
        binding
            .selection
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::AiProviderRequired))
    }

    pub(crate) fn update_tender_ai_execution_binding(
        &mut self,
        tender_id: &TenderId,
        expected_revision: u64,
        selection: Option<AiExecutionSelection>,
        readiness: TenderAiSelectionReadiness,
        status_summary: &str,
    ) -> Result<TenderAiExecutionBinding, TenderCommandError> {
        self.require_storage_writable()?;
        if status_summary.trim().is_empty() || status_summary.len() > 1_000 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        if (readiness == TenderAiSelectionReadiness::LocalOnly) != selection.is_none() {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let next_revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let expected_revision_i64 = i64::try_from(expected_revision)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let next_revision_i64 = i64::try_from(next_revision)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let selection_json = selection
            .as_ref()
            .map(serde_json_canonicalizer::to_string)
            .transpose()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let updated_at = sqlite_timestamp(&transaction)?;
        let updated = transaction
            .execute(
                "UPDATE tender_ai_execution_binding
                 SET revision = ?1, selection_json = ?2, readiness = ?3,
                     status_summary = ?4, updated_at = ?5
                 WHERE singleton = 1 AND revision = ?6",
                params![
                    next_revision_i64,
                    selection_json,
                    tender_ai_selection_readiness_as_str(readiness),
                    status_summary,
                    updated_at,
                    expected_revision_i64,
                ],
            )
            .map_err(sql_error)?;
        if updated != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let tender_revision = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get::<_, u32>(0),
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "tender_ai_execution_binding_updated",
            tender_revision,
            json!({
                "binding_revision": next_revision,
                "readiness": tender_ai_selection_readiness_as_str(readiness),
                "selection_present": selection.is_some(),
            }),
            &updated_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        self.inspect_tender_ai_execution_binding()
    }

    fn reconcile_interrupted_parses(
        &mut self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let attempts = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT attempt_id, artifact_id, version
                     FROM parse_attempts
                     WHERE status = 'running'
                     ORDER BY attempt_sequence",
                )
                .map_err(sql_error)?;
            let attempts = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?,
                    ))
                })
                .map_err(sql_error)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(sql_error)?;
            attempts
        };
        if attempts.is_empty() {
            return Ok(());
        }
        for (attempt_id, _, _) in &attempts {
            if !valid_identifier(attempt_id) {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let staging_root = self
                .root
                .join("staging")
                .join(format!("parse-{attempt_id}"));
            match fs::symlink_metadata(&staging_root) {
                Ok(metadata)
                    if !metadata_is_unsafe_storage_link(&metadata) && metadata.is_dir() =>
                {
                    remove_verified_directory(&self.root.join("staging"), &staging_root)?;
                }
                Ok(_) => {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(store_unavailable(error)),
            }
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
        let completed_at = sqlite_timestamp(&transaction)?;
        for (attempt_id, artifact_id, version) in attempts {
            if transaction
                .execute(
                    "UPDATE parse_attempts
                     SET status = 'interrupted', exception_code = 'interrupted', completed_at = ?2
                     WHERE attempt_id = ?1 AND status = 'running'",
                    params![attempt_id, completed_at],
                )
                .map_err(sql_error)?
                != 1
            {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "source_artifact_parse_interrupted",
                tender_revision,
                json!({
                    "artifact_id": artifact_id,
                    "attempt_id": attempt_id,
                    "reason": "host_restart",
                    "version": version.to_string(),
                }),
                &completed_at,
            )?;
        }
        transaction.commit().map_err(sql_error)
    }

    fn reconcile_uncommitted_host_staging(&self) -> Result<(), TenderCommandError> {
        let staging_root = self.root.join("staging");
        for entry in fs::read_dir(&staging_root).map_err(store_unavailable)? {
            let entry = entry.map_err(store_unavailable)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
            if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let name = entry.file_name();
            let Some(candidate_id) = name.to_str().and_then(host_staging_candidate_id) else {
                continue;
            };
            let is_package_stage = name
                .to_str()
                .is_some_and(|name| name.starts_with("package-"));
            if (!is_package_stage && !valid_identifier(candidate_id))
                || (is_package_stage && !valid_package_operation_id(candidate_id))
            {
                continue;
            }
            remove_verified_directory(&staging_root, &entry.path())?;
        }
        Ok(())
    }

    fn open(root: &Path, expected_tender_id: &TenderId) -> Result<Self, TenderCommandError> {
        register_sqlite_vec()?;
        #[cfg(test)]
        TENDER_STORE_OPEN_COUNT.fetch_add(1, Ordering::SeqCst);
        validate_tender_store_layout(root).map_err(recovery_required_if_integrity)?;
        let database = root.join("tender.sqlite");
        let mut connection = Connection::open_with_flags(
            database,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| TenderCommandError::new(TenderErrorCode::RecoveryRequired))?;
        configure_writer(&mut connection)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::RecoveryRequired))?;
        if inspect_store_structure(&connection, expected_tender_id)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::RecoveryRequired))?
            .is_some()
        {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }
        verify_audit_chain(&connection).map_err(recovery_required_if_integrity)?;
        if inspect_referenced_content(&connection, &root.join("content"))?.is_some() {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }
        let archived: bool = connection
            .query_row(
                "SELECT state = 'archived' FROM tender_retention WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(|_| TenderCommandError::new(TenderErrorCode::RecoveryRequired))?;
        let mut store = Self {
            root: root.to_path_buf(),
            connection,
            recovery_required: false,
            archived,
        };
        if store.archived {
            return Ok(store);
        }
        store
            .reconcile_uncommitted_content(expected_tender_id)
            .map_err(recovery_required_if_integrity)?;
        if !store.semantic_manifests_are_valid()? {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }
        store
            .reconcile_interrupted_parses(expected_tender_id)
            .map_err(recovery_required_if_integrity)?;
        store
            .reconcile_uncommitted_host_staging()
            .map_err(recovery_required_if_integrity)?;
        store
            .reconcile_interrupted_agent_runs(expected_tender_id)
            .map_err(recovery_required_if_integrity)?;
        store
            .reconcile_interrupted_production_tasks(expected_tender_id)
            .map_err(recovery_required_if_integrity)?;
        verify_audit_chain(&store.connection).map_err(recovery_required_if_integrity)?;
        if inspect_referenced_content(&store.connection, &store.root.join("content"))?.is_some()
            || !store.semantic_manifests_are_valid()?
        {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }
        Ok(store)
    }

    fn reconcile_uncommitted_content(
        &mut self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let referenced = {
            let mut statement = self
                .connection
                .prepare("SELECT sha256 FROM content_objects")
                .map_err(sql_error)?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(sql_error)?
                .collect::<rusqlite::Result<HashSet<_>>>()
                .map_err(sql_error)?;
            values
        };
        let content_root = self.root.join("content");
        let content_v2 = content_root.join("content-v2");
        let mut removed_digests = Vec::new();
        if let Some(content_v2_root) =
            canonical_direct_storage_directory(&content_root, &content_v2)?
        {
            let hash_root = content_v2_root.join("sha256");
            if let Some(hash_root) =
                canonical_direct_storage_directory(&content_v2_root, &hash_root)?
            {
                for entry in walkdir::WalkDir::new(&hash_root)
                    .min_depth(1)
                    .follow_links(false)
                {
                    let entry = entry.map_err(|error| {
                        error
                            .into_io_error()
                            .map(store_unavailable)
                            .unwrap_or_else(|| {
                                TenderCommandError::new(TenderErrorCode::IntegrityFailed)
                            })
                    })?;
                    let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
                    if metadata_is_unsafe_storage_link(&metadata) {
                        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                    }
                    if !metadata.is_file() {
                        continue;
                    }
                    let relative = entry
                        .path()
                        .strip_prefix(&hash_root)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                    let components = relative
                        .components()
                        .map(|component| component.as_os_str().to_str())
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                    if components.len() != 3 {
                        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                    }
                    let digest = components.concat();
                    if digest.len() != 64
                        || !digest
                            .bytes()
                            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                    {
                        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                    }
                    if !referenced.contains(&digest) {
                        fs::remove_file(entry.path()).map_err(store_unavailable)?;
                        removed_digests.push(digest);
                    }
                }
            }
        }
        let temporary = content_root.join("tmp");
        let removed_temporary = match fs::symlink_metadata(&temporary) {
            Ok(metadata) if !metadata_is_unsafe_storage_link(&metadata) && metadata.is_dir() => {
                let count = walkdir::WalkDir::new(&temporary)
                    .min_depth(1)
                    .follow_links(false)
                    .into_iter()
                    .map(|entry| {
                        let entry = entry.map_err(walkdir_error)?;
                        let metadata =
                            fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
                        if metadata_is_unsafe_storage_link(&metadata) {
                            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                        }
                        Ok(())
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .len();
                if count > 0 {
                    remove_verified_directory(&content_root, &temporary)?;
                }
                count
            }
            Ok(_) => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(store_unavailable(error)),
        };
        if removed_digests.is_empty() && removed_temporary == 0 {
            return Ok(());
        }
        removed_digests.sort();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender
                 WHERE singleton = 1 AND tender_id = ?1",
                [tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let reconciled_at = sqlite_timestamp(&transaction)?;
        append_audit_event(
            &transaction,
            tender_id.as_str(),
            "uncommitted_content_reconciled",
            tender_revision,
            json!({
                "removed_digests": removed_digests,
                "removed_temporary_entries": removed_temporary.to_string(),
            }),
            &reconciled_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    fn inspect_integrity_with_check(
        root: &Path,
        expected_tender_id: &TenderId,
        mut check: impl FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<TenderIntegrityReport, TenderCommandError> {
        register_sqlite_vec()?;
        check()?;
        #[cfg(feature = "runtime-fixture")]
        if let Ok(failure) = std::env::var("QUANTIX_STORAGE_INSPECTION_FAIL_TENDER") {
            if failure == expected_tender_id.as_str() {
                return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
            }
            if failure == format!("not_found:{}", expected_tender_id.as_str()) {
                return Err(TenderCommandError::new(TenderErrorCode::NotFound));
            }
        }
        check()?;
        if let Err(error) = validate_tender_store_layout(root) {
            return if error.code == TenderErrorCode::NotFound {
                Err(error)
            } else {
                Ok(recovery_report(
                    expected_tender_id,
                    vec![TenderIntegrityIssue::StorageLayoutInvalid],
                ))
            };
        }
        let connection = match Connection::open_with_flags(
            root.join("tender.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(connection) => connection,
            Err(_) => {
                return Ok(recovery_report(
                    expected_tender_id,
                    vec![TenderIntegrityIssue::DatabaseIntegrityInvalid],
                ));
            }
        };
        configure_reader(&connection)?;
        match inspect_store_structure_with_check(&connection, expected_tender_id, &mut check) {
            Ok(None) => {}
            Ok(Some(issue)) => return Ok(recovery_report(expected_tender_id, vec![issue])),
            Err(error) if error.code == TenderErrorCode::OperationTimedOut => return Err(error),
            Err(_) => {
                return Ok(recovery_report(
                    expected_tender_id,
                    vec![TenderIntegrityIssue::DatabaseIntegrityInvalid],
                ));
            }
        }
        let mut issues = Vec::new();
        match verify_audit_chain_with_check(&connection, &mut check) {
            Ok(()) => {}
            Err(error) if error.code == TenderErrorCode::IntegrityFailed => {
                issues.push(TenderIntegrityIssue::AuditChainInvalid);
            }
            Err(error) => return Err(error),
        }
        if let Some(issue) =
            inspect_referenced_content_with_check(&connection, &root.join("content"), &mut check)?
        {
            issues.push(issue);
        }
        let store = Self {
            root: root.to_path_buf(),
            connection,
            recovery_required: false,
            archived: false,
        };
        if !store.semantic_manifests_are_valid_with_check(&mut check)? {
            issues.push(TenderIntegrityIssue::ManifestInvalid);
        }
        let state = if issues.is_empty() {
            TenderIntegrityState::Ready
        } else {
            TenderIntegrityState::RecoveryRequired
        };
        let recovery_choices = if issues.is_empty() {
            Vec::new()
        } else {
            vec![
                TenderRecoveryChoice::RestoreVerifiedBackup,
                TenderRecoveryChoice::PurgeTender,
            ]
        };
        Ok(TenderIntegrityReport {
            tender_id: expected_tender_id.as_str().to_owned(),
            state,
            issues,
            recovery_choices,
        })
    }

    fn semantic_manifests_are_valid(&self) -> Result<bool, TenderCommandError> {
        self.semantic_manifests_are_valid_with_check(&mut || Ok(()))
    }

    fn semantic_manifests_are_valid_with_check(
        &self,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<bool, TenderCommandError> {
        check()?;
        if !self.agent_run_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.tender_record_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.manager_intake_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.tender_query_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.external_rfi_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.calculation_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.estimate_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.pricing_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.bid_decision_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.work_plan_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.production_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.coordinated_baseline_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.change_assessment_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.package_production_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.submission_package_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        if !self.package_validation_manifests_are_valid_with_check(check)? {
            return Ok(false);
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT artifact_id, version, location_count
                 FROM parsed_documents ORDER BY artifact_id, version",
            )
            .map_err(sql_error)?;
        let parsed_documents = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, u32>(2)?,
                ))
            })
            .map_err(sql_error)?;
        for document in parsed_documents {
            check()?;
            let (artifact_id, version, location_count) = document.map_err(sql_error)?;
            let command = ParseSourceArtifactCommand {
                tender_id: String::new(),
                artifact_id,
                version,
            };
            let document = match self.evidence_document_with_check(&command, check) {
                Ok(document) => document,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error);
                }
                Err(_) => return Ok(false),
            };
            if usize::try_from(location_count).ok() != Some(document.locations.len()) {
                return Ok(false);
            }
        }
        check()?;
        Ok(true)
    }

    fn require_storage_writable(&self) -> Result<(), TenderCommandError> {
        if self.recovery_required {
            Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired))
        } else if self.archived {
            Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
        } else {
            Ok(())
        }
    }

    fn require_pre_bid_writable(&mut self) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
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
        if matches!(
            lifecycle_phase,
            TenderLifecyclePhase::Intake | TenderLifecyclePhase::BidDecision
        ) {
            Ok(())
        } else {
            let transaction = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(sql_error)?;
            let (tender_id, tender_revision): (String, u32) = transaction
                .query_row(
                    "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(sql_error)?;
            let created_at = sqlite_timestamp(&transaction)?;
            append_audit_event(
                &transaction,
                &tender_id,
                "tender_command_denied",
                tender_revision,
                json!({
                    "command": "pre_bid_mutation",
                    "lifecycle_phase": lifecycle_phase,
                    "reason": "lifecycle_closed",
                }),
                &created_at,
            )?;
            transaction.commit().map_err(sql_error)?;
            Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
        }
    }

    fn change_intake_denial_reason(
        &self,
    ) -> Result<(TenderLifecyclePhase, Option<&'static str>), TenderCommandError> {
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
        let successor_pending = lifecycle_phase == TenderLifecyclePhase::BidDecision
            && self
                .connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM bid_decision_approval_invalidations AS invalidations
                       JOIN bid_decision_approval_records AS approvals
                         ON approvals.approval_id = invalidations.approval_id
                       JOIN bid_decision_package_heads AS heads
                         ON heads.package_id = approvals.package_id
                        AND heads.current_version = approvals.package_version
                       WHERE approvals.decision = 'accept'
                     )",
                    [],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(sql_error)?;
        let recovery_retry_pending: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM production_tasks AS tasks
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
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let unresolved_change_assessment: bool = self
            .connection
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
        let denial_reason = if unresolved_change_assessment {
            Some("change_assessment_pending")
        } else if recovery_retry_pending {
            Some("production_recovery_retry_pending")
        } else if successor_pending {
            Some("material_change_successor_pending")
        } else if !matches!(
            lifecycle_phase,
            TenderLifecyclePhase::Intake
                | TenderLifecyclePhase::BidDecision
                | TenderLifecyclePhase::TenderPlanning
                | TenderLifecyclePhase::ActiveProduction
                | TenderLifecyclePhase::IntegratedReview
                | TenderLifecyclePhase::PackageProduction
                | TenderLifecyclePhase::FinalReview
        ) {
            Some("lifecycle_closed")
        } else {
            None
        };
        Ok((lifecycle_phase, denial_reason))
    }

    pub(crate) fn change_intake_is_writable(&self) -> Result<bool, TenderCommandError> {
        Ok(self.change_intake_denial_reason()?.1.is_none())
    }

    fn require_change_intake_writable(&mut self) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let (lifecycle_phase, denial_reason) = self.change_intake_denial_reason()?;
        let Some(denial_reason) = denial_reason else {
            return Ok(());
        };
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        let created_at = sqlite_timestamp(&transaction)?;
        append_audit_event(
            &transaction,
            &tender_id,
            "tender_command_denied",
            tender_revision,
            json!({
                "command": "material_change_intake",
                "lifecycle_phase": lifecycle_phase,
                "reason": denial_reason,
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }

    fn latch_recovery_required(&mut self) -> Result<(), TenderCommandError> {
        self.recovery_required = true;
        self.connection
            .pragma_update(None, "query_only", true)
            .map_err(sql_error)
    }

    fn summary(&self) -> Result<TenderSummary, TenderCommandError> {
        let (tender_id, revision, lifecycle_phase, name): (String, u32, String, String) = self
            .connection
            .query_row(
                "SELECT tender.tender_id, tender.current_revision, tender.lifecycle_phase,
                        tender_revisions.name
                 FROM tender
                 JOIN tender_revisions ON tender_revisions.revision = tender.current_revision
                 WHERE tender.singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(sql_error)?;
        let (audit_event_count, audit_chain_head): (i64, Option<String>) = self
            .connection
            .query_row(
                "SELECT COUNT(*), MAX(current_hash) FILTER (
                   WHERE sequence = (SELECT MAX(sequence) FROM audit_events)
                 ) FROM audit_events",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;

        Ok(TenderSummary {
            tender_id,
            name,
            revision,
            lifecycle_phase: TenderLifecyclePhase::parse(&lifecycle_phase)?,
            audit_event_count: audit_event_count
                .try_into()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            audit_chain_head: audit_chain_head.unwrap_or_else(|| ZERO_AUDIT_HASH.into()),
        })
    }

    fn revise(
        &mut self,
        command: &ReviseTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        self.require_pre_bid_writable()?;
        let name = command.name.trim();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (stored_tender_id, current_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if stored_tender_id != command.tender_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let next_revision = current_revision
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO tender_revisions (revision, tender_id, name, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![next_revision, stored_tender_id, name, created_at],
            )
            .map_err(sql_error)?;
        let advanced = transaction
            .execute(
                "UPDATE tender SET current_revision = ?1
                 WHERE singleton = 1 AND current_revision = ?2",
                params![next_revision, current_revision],
            )
            .map_err(sql_error)?;
        if advanced != 1 {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
        append_audit_event(
            &transaction,
            &stored_tender_id,
            "tender_revised",
            next_revision,
            json!({ "name": name }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        self.summary()
    }

    fn register_content(
        &mut self,
        command: &RegisterTenderContentCommand,
    ) -> Result<ContentVersionSummary, TenderCommandError> {
        self.require_change_intake_writable()?;
        let content_root = self.root.join("content");
        let integrity = match cacache::write_hash_sync(&content_root, &command.bytes) {
            Ok(integrity) => integrity,
            Err(error) => {
                return self.reject_content_publication(
                    command,
                    "content_store_unavailable",
                    content_store_error(error),
                );
            }
        };
        let verified = match cacache::read_hash_sync(&content_root, &integrity) {
            Ok(verified) => verified,
            Err(error) => {
                return self.reject_content_publication(
                    command,
                    "content_verification_unavailable",
                    content_store_error(error),
                );
            }
        };
        if verified != command.bytes {
            return self.reject_content_publication(
                command,
                "content_digest_mismatch",
                TenderCommandError::new(TenderErrorCode::IntegrityFailed),
            );
        }
        storage_publication_failpoint("content_after_cache_write");
        let sha256 = sha256_hex(&command.bytes);
        let size_bytes = i64::try_from(command.bytes.len())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let tender_id: String = transaction
            .query_row(
                "SELECT tender_id FROM tender WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if tender_id != command.tender_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let current_revision: Option<u32> = transaction
            .query_row(
                "SELECT current_revision FROM content_heads WHERE logical_id = ?1",
                [&command.logical_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql_error)?;
        let revision = current_revision
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO content_objects (sha256, integrity, size_bytes)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(sha256) DO NOTHING",
                params![sha256, integrity.to_string(), size_bytes],
            )
            .map_err(sql_error)?;
        let stored_object: (String, i64) = transaction
            .query_row(
                "SELECT integrity, size_bytes FROM content_objects WHERE sha256 = ?1",
                [&sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if stored_object != (integrity.to_string(), size_bytes) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        transaction
            .execute(
                "INSERT INTO content_versions (
                   logical_id, revision, sha256, media_type, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    command.logical_id,
                    revision,
                    sha256,
                    command.media_type,
                    created_at
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT INTO content_heads (logical_id, current_revision) VALUES (?1, ?2)
                 ON CONFLICT(logical_id) DO UPDATE SET current_revision = excluded.current_revision",
                params![command.logical_id, revision],
            )
            .map_err(sql_error)?;
        append_audit_event(
            &transaction,
            &tender_id,
            "content_version_registered",
            revision,
            json!({
                "logical_id": command.logical_id,
                "media_type": command.media_type,
                "sha256": sha256,
                "size_bytes": size_bytes.to_string(),
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        storage_publication_failpoint("content_after_database_commit");

        Ok(ContentVersionSummary {
            logical_id: command.logical_id.clone(),
            revision,
            sha256,
            size_bytes: size_bytes
                .try_into()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            media_type: command.media_type.clone(),
        })
    }

    fn reject_content_publication<T>(
        &mut self,
        command: &RegisterTenderContentCommand,
        reason: &str,
        error: TenderCommandError,
    ) -> Result<T, TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if tender_id != command.tender_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        append_audit_event(
            &transaction,
            &tender_id,
            "content_publication_failed",
            tender_revision,
            json!({
                "logical_id": command.logical_id,
                "reason": reason,
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Err(error)
    }

    fn import_package(
        &mut self,
        source: &Path,
    ) -> Result<TenderPackageImportResult, TenderCommandError> {
        self.import_package_with_control(source, None)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }

    pub(crate) fn import_package_with_control(
        &mut self,
        source: &Path,
        control: Option<&PackageIntakeControl>,
    ) -> Result<Option<TenderPackageImportResult>, TenderCommandError> {
        let content_root = self.root.join("content");
        self.import_package_with_control_from_content_root(source, &content_root, control)
    }

    pub(crate) fn import_package_with_control_from_content_root(
        &mut self,
        source: &Path,
        content_root: &Path,
        control: Option<&PackageIntakeControl>,
    ) -> Result<Option<TenderPackageImportResult>, TenderCommandError> {
        self.require_change_intake_writable()?;
        let Some(prepared) = prepare_package_with_control(source, content_root, control)? else {
            return Ok(None);
        };
        // Once this point is reached publication is atomic.  Cancellation is
        // intentionally disabled before opening the transaction so a request
        // can never leave half of an intake visible in the database.
        if control.is_some_and(PackageIntakeControl::is_cancelled) {
            return Ok(None);
        }
        if let Some(control) = control {
            control.set_stage(crate::tender_intake::PackageIntakeStage::RecordingDocuments);
            control.set_total(Some(
                u32::try_from(prepared.documents.len()).unwrap_or(u32::MAX),
            ));
            control.mark_finalization();
        }
        self.publish_intake(prepared, control).map(Some)
    }

    fn promote_staged_content(
        &self,
        prepared: &PreparedIntake,
        staged_content_root: &Path,
    ) -> Result<Vec<cacache::Integrity>, TenderCommandError> {
        let live_content_root = self.root.join("content");
        let mut promoted = Vec::new();
        let result = (|| -> Result<(), TenderCommandError> {
            for document in &prepared.documents {
                let Some(integrity) = document.integrity.as_deref() else {
                    continue;
                };
                let integrity = integrity
                    .parse::<cacache::Integrity>()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                let bytes = cacache::read_hash_sync(staged_content_root, &integrity)
                    .map_err(content_store_error)?;
                let existed = cacache::read_hash_sync(&live_content_root, &integrity).is_ok();
                let written_integrity = cacache::write_hash_sync(&live_content_root, &bytes)
                    .map_err(content_store_error)?;
                if written_integrity.to_string() != integrity.to_string()
                    || cacache::read_hash_sync(&live_content_root, &integrity)
                        .map_err(content_store_error)?
                        != bytes
                {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                if !existed {
                    promoted.push(integrity);
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            for integrity in &promoted {
                let _ = cacache::remove_hash_sync(&live_content_root, integrity);
            }
            return Err(error);
        }
        Ok(promoted)
    }

    pub(crate) fn begin_parse(
        &mut self,
        command: &ParseSourceArtifactCommand,
        tender_id: &TenderId,
    ) -> Result<ParseJob, TenderCommandError> {
        self.require_change_intake_writable()?;
        let (document_type, integrity, expected_sha256): (String, String, String) = self
            .connection
            .query_row(
                "SELECT sav.document_type, co.integrity, co.sha256
                 FROM source_artifact_versions sav
                 JOIN content_objects co ON co.sha256 = sav.sha256
                 WHERE sav.artifact_id = ?1 AND sav.version = ?2
                   AND sav.registration_state = 'registered'",
                params![command.artifact_id, command.version],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(sql_error)?
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let input_format = match document_type.as_str() {
            "pdf_document" => "pdf",
            "word_document" => "docx",
            "spreadsheet" => "xlsx",
            _ => return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        };
        let already_parsed: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM parsed_documents
                   WHERE artifact_id = ?1 AND version = ?2
                 ) OR EXISTS(
                   SELECT 1 FROM parse_attempts
                   WHERE artifact_id = ?1 AND version = ?2 AND status = 'running'
                 )",
                params![command.artifact_id, command.version],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        if already_parsed {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let integrity = integrity
            .parse::<cacache::Integrity>()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let bytes = cacache::read_hash_sync(self.root.join("content"), &integrity)
            .map_err(content_store_error)?;
        let actual_sha256: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if actual_sha256 != expected_sha256 {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }

        let attempt_id = random_identifier(&self.connection)?;
        let staging_root = self
            .root
            .join("staging")
            .join(format!("parse-{attempt_id}"));
        let input_directory = staging_root.join("input");
        let candidate_directory = staging_root.join("candidate");
        fs::create_dir(&staging_root).map_err(store_unavailable)?;
        let staged = (|| -> Result<PathBuf, TenderCommandError> {
            fs::create_dir(&input_directory).map_err(store_unavailable)?;
            fs::create_dir(&candidate_directory).map_err(store_unavailable)?;
            let input_path = input_directory.join(format!("source.{input_format}"));
            fs::write(&input_path, bytes).map_err(store_unavailable)?;
            Ok(input_path)
        })();
        let input_path = match staged {
            Ok(path) => path,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging_root);
                return Err(error);
            }
        };

        let recorded = (|| -> Result<(), TenderCommandError> {
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
            let started_at = sqlite_timestamp(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO parse_attempts (
                       attempt_id, artifact_id, version, status, exception_code,
                       started_at, completed_at
                     ) VALUES (?1, ?2, ?3, 'running', NULL, ?4, NULL)",
                    params![attempt_id, command.artifact_id, command.version, started_at],
                )
                .map_err(sql_error)?;
            append_audit_event(
                &transaction,
                tender_id.as_str(),
                "source_artifact_parse_started",
                tender_revision,
                json!({
                    "artifact_id": command.artifact_id,
                    "attempt_id": attempt_id,
                    "version": command.version.to_string(),
                }),
                &started_at,
            )?;
            transaction.commit().map_err(sql_error)
        })();
        if let Err(error) = recorded {
            let _ = fs::remove_dir_all(&staging_root);
            return Err(error);
        }
        Ok(ParseJob {
            attempt_id,
            tender_id: tender_id.clone(),
            artifact_id: command.artifact_id.clone(),
            version: command.version,
            input_format: input_format.into(),
            staging_root,
            input_path,
            candidate_directory,
        })
    }

    pub(crate) fn publish_parse(
        &mut self,
        job: &ParseJob,
        prepared: PreparedParseOutput,
    ) -> Result<DocumentParseResult, TenderCommandError> {
        self.require_change_intake_writable()?;
        if prepared.embeddings.len() != prepared.locations.len()
            || prepared.embeddings.iter().any(|embedding| {
                embedding.len() != crate::embedding::EMBEDDING_DIMENSIONS
                    || embedding.iter().any(|value| !value.is_finite())
            })
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let integrity =
            cacache::write_hash_sync(self.root.join("content"), &prepared.markdown_bytes)
                .map_err(content_store_error)?;
        let verified = cacache::read_hash_sync(self.root.join("content"), &integrity)
            .map_err(content_store_error)?;
        if verified != prepared.markdown_bytes {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let markdown_sha256: String = Sha256::digest(&prepared.markdown_bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let size_bytes = i64::try_from(prepared.markdown_bytes.len())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let location_count = u32::try_from(prepared.locations.len())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [job.tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let completed_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO content_objects (sha256, integrity, size_bytes)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(sha256) DO NOTHING",
                params![markdown_sha256, integrity.to_string(), size_bytes],
            )
            .map_err(sql_error)?;
        let stored: (String, i64) = transaction
            .query_row(
                "SELECT integrity, size_bytes FROM content_objects WHERE sha256 = ?1",
                [&markdown_sha256],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if stored != (integrity.to_string(), size_bytes) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        transaction
            .execute(
                "INSERT INTO parsed_documents (
                   artifact_id, version, attempt_id, pipeline_version,
                   markdown_sha256, language, direction, location_count, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    job.artifact_id,
                    job.version,
                    job.attempt_id,
                    crate::document_parsing::MARKDOWN_PIPELINE_VERSION,
                    markdown_sha256,
                    prepared.language.as_str(),
                    prepared.direction.as_str(),
                    location_count,
                    completed_at,
                ],
            )
            .map_err(sql_error)?;
        for (location, embedding) in prepared.locations.iter().zip(&prepared.embeddings) {
            let provenance_json = serde_json_canonicalizer::to_string(&location.provenance)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            transaction
                .execute(
                    "INSERT INTO evidence_locations (
                       artifact_id, version, ordinal, kind, structural_path,
                       provenance_json, section, paragraph_number, table_number,
                       sheet_name, cell_range, original_text, translated_text,
                       language, direction
                     ) VALUES (
                       ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                       ?12, ?13, ?14, ?15
                     )",
                    params![
                        job.artifact_id,
                        job.version,
                        location.ordinal,
                        location.kind.as_str(),
                        location.structural_path,
                        provenance_json,
                        location.section,
                        location.paragraph_number,
                        location.table_number,
                        location.sheet_name,
                        location.cell_range,
                        location.original_text,
                        location.translated_text,
                        location.language.as_str(),
                        location.direction.as_str(),
                    ],
                )
                .map_err(sql_error)?;
            transaction
                .execute(
                    "INSERT INTO evidence_fts (original_text, artifact_id, version, ordinal)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        location.original_text,
                        job.artifact_id,
                        job.version,
                        location.ordinal
                    ],
                )
                .map_err(sql_error)?;
            transaction
                .execute(
                    "INSERT INTO evidence_embeddings (artifact_id, version, ordinal)
                     VALUES (?1, ?2, ?3)",
                    params![job.artifact_id, job.version, location.ordinal],
                )
                .map_err(sql_error)?;
            let embedding_id = transaction.last_insert_rowid();
            let embedding_json = serde_json::to_string(embedding)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            transaction
                .execute(
                    "INSERT INTO evidence_embedding_vectors (rowid, embedding)
                     VALUES (?1, ?2)",
                    params![embedding_id, embedding_json],
                )
                .map_err(sql_error)?;
        }
        if transaction
            .execute(
                "UPDATE parse_attempts
                 SET status = 'parsed', completed_at = ?2
                 WHERE attempt_id = ?1 AND status = 'running'",
                params![job.attempt_id, completed_at],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        append_audit_event(
            &transaction,
            job.tender_id.as_str(),
            "source_artifact_parsed",
            tender_revision,
            json!({
                "artifact_id": job.artifact_id,
                "attempt_id": job.attempt_id,
                "pipeline_version": crate::document_parsing::MARKDOWN_PIPELINE_VERSION,
                "markdown_sha256": markdown_sha256,
                "location_count": location_count.to_string(),
                "version": job.version.to_string(),
            }),
            &completed_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(DocumentParseResult {
            attempt_id: job.attempt_id.clone(),
            artifact_id: job.artifact_id.clone(),
            version: job.version,
            state: ParseState::Parsed,
            exception: None,
            location_count,
            language: prepared.language,
            direction: prepared.direction,
            pipeline_version: Some(crate::document_parsing::MARKDOWN_PIPELINE_VERSION.into()),
            markdown_sha256: Some(markdown_sha256),
        })
    }

    pub(crate) fn fail_parse(
        &mut self,
        job: &ParseJob,
        state: ParseState,
        exception: ParseExceptionCode,
    ) -> Result<DocumentParseResult, TenderCommandError> {
        self.require_change_intake_writable()?;
        if !matches!(
            state,
            ParseState::Failed | ParseState::Interrupted | ParseState::Quarantined
        ) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let tender_revision: u32 = transaction
            .query_row(
                "SELECT current_revision FROM tender WHERE singleton = 1 AND tender_id = ?1",
                [job.tender_id.as_str()],
                |row| row.get(0),
            )
            .map_err(sql_error)?;
        let completed_at = sqlite_timestamp(&transaction)?;
        if transaction
            .execute(
                "UPDATE parse_attempts
                 SET status = ?2, exception_code = ?3, completed_at = ?4
                 WHERE attempt_id = ?1 AND status = 'running'",
                params![
                    job.attempt_id,
                    state.as_str(),
                    exception.as_str(),
                    completed_at
                ],
            )
            .map_err(sql_error)?
            != 1
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        append_audit_event(
            &transaction,
            job.tender_id.as_str(),
            "source_artifact_parse_failed",
            tender_revision,
            json!({
                "artifact_id": job.artifact_id,
                "attempt_id": job.attempt_id,
                "exception": exception.as_str(),
                "state": state.as_str(),
                "version": job.version.to_string(),
            }),
            &completed_at,
        )?;
        transaction.commit().map_err(sql_error)?;
        Ok(DocumentParseResult {
            attempt_id: job.attempt_id.clone(),
            artifact_id: job.artifact_id.clone(),
            version: job.version,
            state,
            exception: Some(exception),
            location_count: 0,
            language: EvidenceLanguage::Undetermined,
            direction: TextDirection::Neutral,
            pipeline_version: None,
            markdown_sha256: None,
        })
    }

    pub(crate) fn evidence_document(
        &self,
        command: &ParseSourceArtifactCommand,
    ) -> Result<EvidenceDocument, TenderCommandError> {
        self.evidence_document_with_check(command, &mut || Ok(()))
    }

    fn evidence_document_with_check(
        &self,
        command: &ParseSourceArtifactCommand,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<EvidenceDocument, TenderCommandError> {
        check()?;
        let parsed: (String, String, String, String, u32) = self
            .connection
            .query_row(
                "SELECT pd.language, pd.direction, pd.pipeline_version, pd.markdown_sha256,
                        pd.location_count
                 FROM parsed_documents pd
                 WHERE pd.artifact_id = ?1 AND pd.version = ?2",
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
        let expected_location_count = usize::try_from(parsed.4)
            .ok()
            .filter(|count| *count > 0 && *count <= MAX_EVIDENCE_LOCATIONS)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT ordinal, kind, structural_path, provenance_json, section,
                        paragraph_number, table_number, sheet_name, cell_range,
                        original_text, translated_text, language, direction,
                        length(CAST(kind AS BLOB))
                          + length(CAST(structural_path AS BLOB))
                          + length(CAST(provenance_json AS BLOB))
                          + COALESCE(length(CAST(section AS BLOB)), 0)
                          + COALESCE(length(CAST(sheet_name AS BLOB)), 0)
                          + COALESCE(length(CAST(cell_range AS BLOB)), 0)
                          + length(CAST(original_text AS BLOB))
                          + COALESCE(length(CAST(translated_text AS BLOB)), 0)
                          + length(CAST(language AS BLOB))
                          + length(CAST(direction AS BLOB))
                 FROM evidence_locations
                 WHERE artifact_id = ?1 AND version = ?2
                 ORDER BY ordinal",
            )
            .map_err(sql_error)?;
        let mut rows = statement
            .query(params![command.artifact_id, command.version])
            .map_err(sql_error)?;
        let mut locations = Vec::new();
        let mut total_bytes = 0_u64;
        while let Some(row) = rows.next().map_err(sql_error)? {
            check()?;
            if locations.len() >= expected_location_count {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let row_bytes = u64::try_from(row.get::<_, i64>(13).map_err(sql_error)?)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            total_bytes = total_bytes
                .checked_add(row_bytes)
                .filter(|total| *total <= MAX_MARKDOWN_BYTES)
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
            locations.push(
                RawEvidenceLocation::read(row, 0)
                    .map_err(sql_error)?
                    .into_domain()?,
            );
        }
        if locations.len() != expected_location_count {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        check()?;
        Ok(EvidenceDocument {
            artifact_id: command.artifact_id.clone(),
            version: command.version,
            state: ParseState::Parsed,
            exception: None,
            language: EvidenceLanguage::parse(&parsed.0)?,
            direction: TextDirection::parse(&parsed.1)?,
            pipeline_version: Some(parsed.2),
            markdown_sha256: Some(parsed.3),
            locations,
        })
    }

    pub(crate) fn search_evidence(
        &self,
        command: &SearchEvidenceCommand,
    ) -> Result<EvidenceSearchResult, TenderCommandError> {
        let query = command.query.trim();
        let phrase = format!("\"{}\"", query.replace('"', "\"\""));
        let mut statement = self
            .connection
            .prepare(
                "SELECT el.artifact_id, el.version, sa.package_path,
                        el.ordinal, el.kind, el.structural_path, el.provenance_json,
                        el.section, el.paragraph_number, el.table_number,
                        el.sheet_name, el.cell_range, el.original_text,
                        el.translated_text, el.language, el.direction
                 FROM evidence_fts
                 JOIN evidence_locations el
                   ON el.artifact_id = evidence_fts.artifact_id
                  AND el.version = evidence_fts.version
                  AND el.ordinal = evidence_fts.ordinal
                  AND el.original_text = evidence_fts.original_text
                 JOIN source_artifacts sa ON sa.artifact_id = el.artifact_id
                 WHERE evidence_fts MATCH ?1
                 ORDER BY el.artifact_id, el.version, el.ordinal
                 LIMIT 100",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map([phrase], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    RawEvidenceLocation::read(row, 3)?,
                ))
            })
            .map_err(sql_error)?;
        let mut matches = Vec::new();
        for row in rows {
            let (artifact_id, version, package_path, location) = row.map_err(sql_error)?;
            matches.push(EvidenceSearchHit {
                artifact_id,
                version,
                package_path,
                location: location.into_domain()?,
            });
        }
        Ok(EvidenceSearchResult {
            query: query.to_owned(),
            matches,
        })
    }

    pub(crate) fn search_evidence_semantic(
        &self,
        command: &SearchEvidenceSemanticCommand,
        query_embedding: &[f32],
    ) -> Result<EvidenceSemanticSearchResult, TenderCommandError> {
        if query_embedding.len() != crate::embedding::EMBEDDING_DIMENSIONS
            || query_embedding.iter().any(|value| !value.is_finite())
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let query = command.query.trim();
        let query_json = serde_json::to_string(query_embedding)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let mut statement = self
            .connection
            .prepare(
                "WITH nearest AS (
                   SELECT rowid AS embedding_id, distance
                   FROM evidence_embedding_vectors
                   WHERE embedding MATCH ?1 AND k = ?2
                 )
                 SELECT nearest.distance, el.artifact_id, el.version, sa.package_path,
                        el.ordinal, el.kind, el.structural_path, el.provenance_json,
                        el.section, el.paragraph_number, el.table_number,
                        el.sheet_name, el.cell_range, el.original_text,
                        el.translated_text, el.language, el.direction
                 FROM nearest
                 JOIN evidence_embeddings ee ON ee.embedding_id = nearest.embedding_id
                 JOIN evidence_locations el
                   ON el.artifact_id = ee.artifact_id
                  AND el.version = ee.version
                  AND el.ordinal = ee.ordinal
                 JOIN source_artifacts sa ON sa.artifact_id = el.artifact_id
                 WHERE nearest.distance <= ?3
                 ORDER BY nearest.distance, el.artifact_id, el.version, el.ordinal",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(
                params![
                    query_json,
                    i64::from(command.limit),
                    f64::from(command.distance_threshold)
                ],
                |row| {
                    Ok((
                        row.get::<_, f64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, String>(3)?,
                        RawEvidenceLocation::read(row, 4)?,
                    ))
                },
            )
            .map_err(sql_error)?;
        let mut matches = Vec::new();
        for row in rows {
            let (distance, artifact_id, version, package_path, location) =
                row.map_err(sql_error)?;
            if !distance.is_finite() || !(0.0..=2.0).contains(&distance) {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            matches.push(EvidenceSemanticSearchHit {
                distance: distance as f32,
                artifact_id,
                version,
                package_path,
                location: location.into_domain()?,
            });
        }
        Ok(EvidenceSemanticSearchResult {
            query: query.to_owned(),
            matches,
        })
    }

    fn publish_intake(
        &mut self,
        prepared: PreparedIntake,
        control: Option<&PackageIntakeControl>,
    ) -> Result<TenderPackageImportResult, TenderCommandError> {
        self.require_change_intake_writable()?;
        let discovered_count = u32::try_from(prepared.documents.len())
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let registered_count = u32::try_from(
            prepared
                .documents
                .iter()
                .filter(|document| document.exception.is_none())
                .count(),
        )
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let exception_count = discovered_count
            .checked_sub(registered_count)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let skipped_count = u32::try_from(
            prepared
                .documents
                .iter()
                .filter(|document| {
                    matches!(
                        document.exception,
                        Some(IntakeExceptionCode::Unsupported | IntakeExceptionCode::NestedArchive)
                    )
                })
                .count(),
        )
        .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let attention_count = exception_count
            .checked_sub(skipped_count)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;

        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision, lifecycle_phase): (String, u32, String) = transaction
            .query_row(
                "SELECT tender_id, current_revision, lifecycle_phase
                 FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        let intake_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO intake_runs (
                   intake_id, source_kind, source_path, source_name,
                   discovered_count, registered_count, exception_count, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    intake_id,
                    prepared.source_kind.as_str(),
                    prepared.source_path,
                    prepared.source_name,
                    discovered_count,
                    registered_count,
                    exception_count,
                    created_at
                ],
            )
            .map_err(sql_error)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO query_register (singleton, opened_by_intake_id, opened_at)
                 VALUES (1, ?1, ?2)",
                params![intake_id, created_at],
            )
            .map_err(sql_error)?;

        for document in prepared.documents {
            if let Some(control) = control {
                control.set_current_path(Some(document.package_path.clone()));
            }
            let artifact_id = random_identifier(&transaction)?;
            transaction
                .execute(
                    "INSERT INTO source_artifacts (
                       artifact_id, intake_id, package_path, created_at
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![artifact_id, intake_id, document.package_path, created_at],
                )
                .map_err(sql_error)?;
            let registration_state = if document.exception.is_some() {
                RegistrationState::Exception
            } else {
                RegistrationState::Registered
            };
            if registration_state == RegistrationState::Registered {
                let sha256 = document
                    .sha256
                    .as_ref()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                let integrity = document
                    .integrity
                    .as_ref()
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
                let size_bytes = i64::try_from(document.size_bytes)
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
                transaction
                    .execute(
                        "INSERT INTO content_objects (sha256, integrity, size_bytes)
                         VALUES (?1, ?2, ?3)
                         ON CONFLICT(sha256) DO NOTHING",
                        params![sha256, integrity, size_bytes],
                    )
                    .map_err(sql_error)?;
                let stored: (String, i64) = transaction
                    .query_row(
                        "SELECT integrity, size_bytes FROM content_objects WHERE sha256 = ?1",
                        [sha256],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(sql_error)?;
                if stored != (integrity.clone(), size_bytes) {
                    return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
                }
                if let Some(control) = control {
                    control.record_registered();
                }
            }
            transaction
                .execute(
                    "INSERT INTO source_artifact_versions (
                       artifact_id, version, language, document_type, media_type, sha256,
                       size_bytes, registration_state, exception_code, created_at
                     ) VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        artifact_id,
                        document.language,
                        document.document_type,
                        document.media_type,
                        document.sha256,
                        i64::try_from(document.size_bytes).map_err(|_| {
                            TenderCommandError::new(TenderErrorCode::InvalidCommand)
                        })?,
                        registration_state.as_str(),
                        document.exception.map(IntakeExceptionCode::as_str),
                        created_at
                    ],
                )
                .map_err(sql_error)?;
            if let Some(exception) = document.exception {
                append_audit_event(
                    &transaction,
                    &tender_id,
                    "source_artifact_registration_failed",
                    tender_revision,
                    json!({
                        "artifact_id": artifact_id,
                        "exception": exception.as_str(),
                        "package_path": document.package_path,
                    }),
                    &created_at,
                )?;
            }
        }
        let package_status = match (skipped_count, attention_count) {
            (0, 0) => format!(
                "Tender Package registered: {registered_count} supported documents available."
            ),
            (skipped, 0) => format!(
                "Tender Package registered: {registered_count} supported documents available; {skipped} other files skipped."
            ),
            (0, attention) => format!(
                "Tender Package registered: {registered_count} supported documents available; {attention} files need attention."
            ),
            (skipped, attention) => format!(
                "Tender Package registered: {registered_count} supported documents available; {skipped} other files skipped; {attention} files need attention."
            ),
        };
        let system_message_id =
            workspace::append_system_status(&transaction, &package_status, &created_at)?;
        let manager_intake_run_id = (lifecycle_phase == "intake")
            .then(|| {
                manager_intake::initialize_manager_intake_run(&transaction, &intake_id, &created_at)
            })
            .transpose()?;
        append_audit_event(
            &transaction,
            &tender_id,
            "tender_package_imported",
            tender_revision,
            json!({
                "discovered_count": discovered_count.to_string(),
                "exception_count": exception_count.to_string(),
                "intake_id": intake_id,
                "manager_intake_run_id": manager_intake_run_id,
                "system_message_id": system_message_id,
                "registered_count": registered_count.to_string(),
                "source_kind": prepared.source_kind.as_str(),
                "source_name": prepared.source_name,
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)?;

        if let Some(control) = control {
            control.opening_workspace();
        }

        Ok(TenderPackageImportResult {
            intake_id: intake_id.clone(),
            source_kind: prepared.source_kind,
            discovered_count,
            registered_count,
            exception_count,
            query_register_open: self.query_register_open()?,
            documents: self.document_register_entries(Some(&intake_id))?,
        })
    }

    fn document_register(&self) -> Result<DocumentRegister, TenderCommandError> {
        Ok(DocumentRegister {
            query_register_open: self.query_register_open()?,
            documents: self.document_register_entries(None)?,
        })
    }

    fn query_register_open(&self) -> Result<bool, TenderCommandError> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM query_register WHERE singleton = 1)",
                [],
                |row| row.get(0),
            )
            .map_err(sql_error)
    }

    fn document_register_entries(
        &self,
        intake_id: Option<&str>,
    ) -> Result<Vec<DocumentRegisterEntry>, TenderCommandError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT
                   sa.artifact_id,
                   sav.version,
                   sa.package_path,
                   COALESCE(pd.language, sav.language),
                   sav.document_type,
                   sav.media_type,
                   sav.sha256,
                   sav.size_bytes,
                   sav.registration_state,
                   CASE
                     WHEN sav.registration_state = 'exception' THEN 'unsupported'
                     ELSE COALESCE(pa.status, 'not_requested')
                   END,
                   CASE
                     WHEN sav.registration_state = 'exception' THEN 'unsupported'
                     ELSE pa.exception_code
                   END,
                   CASE
                     WHEN EXISTS (
                       SELECT 1 FROM source_relationships sr
                       WHERE sr.prior_artifact_id = sav.artifact_id
                         AND sr.prior_version = sav.version
                         AND sr.relationship_kind = 'replacement'
                     ) THEN 'superseded'
                     WHEN EXISTS (
                       SELECT 1 FROM source_relationships sr
                       WHERE (sr.replacement_artifact_id = sav.artifact_id
                              AND sr.replacement_version = sav.version)
                          OR (sr.prior_artifact_id = sav.artifact_id
                              AND sr.prior_version = sav.version
                              AND sr.relationship_kind = 'addendum')
                     ) THEN 'current'
                     ELSE 'unconfirmed'
                   END,
                   sav.exception_code
                 FROM source_artifacts sa
                 JOIN source_artifact_versions sav ON sav.artifact_id = sa.artifact_id
                 LEFT JOIN parse_attempts pa ON pa.attempt_sequence = (
                   SELECT MAX(candidate.attempt_sequence)
                   FROM parse_attempts candidate
                   WHERE candidate.artifact_id = sav.artifact_id
                     AND candidate.version = sav.version
                 )
                 LEFT JOIN parsed_documents pd
                   ON pd.artifact_id = sav.artifact_id AND pd.version = sav.version
                 WHERE (?1 IS NULL OR sa.intake_id = ?1)
                 ORDER BY sa.rowid, sav.version",
            )
            .map_err(sql_error)?;
        let rows = statement
            .query_map(params![intake_id], |row| {
                Ok(RawDocumentRegisterEntry {
                    artifact_id: row.get(0)?,
                    version: row.get(1)?,
                    package_path: row.get(2)?,
                    language: row.get(3)?,
                    document_type: row.get(4)?,
                    media_type: row.get(5)?,
                    sha256: row.get(6)?,
                    size_bytes: row.get(7)?,
                    registration_state: row.get(8)?,
                    parse_state: row.get(9)?,
                    parse_exception_code: row.get(10)?,
                    supersession_state: row.get(11)?,
                    exception_code: row.get(12)?,
                })
            })
            .map_err(sql_error)?;
        let mut documents = Vec::new();
        for row in rows {
            let row = row.map_err(sql_error)?;
            documents.push(DocumentRegisterEntry {
                artifact_id: row.artifact_id,
                version: row.version,
                package_path: row.package_path,
                language: row.language,
                document_type: row.document_type,
                media_type: row.media_type,
                sha256: row.sha256,
                size_bytes: row
                    .size_bytes
                    .try_into()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
                registration_state: RegistrationState::parse(&row.registration_state)?,
                parse_state: ParseState::parse(&row.parse_state)?,
                parse_exception: row
                    .parse_exception_code
                    .as_deref()
                    .map(ParseExceptionCode::parse)
                    .transpose()?,
                supersession_state: SupersessionState::parse(&row.supersession_state)?,
                exception: row
                    .exception_code
                    .as_deref()
                    .map(IntakeExceptionCode::parse)
                    .transpose()?,
            });
        }
        Ok(documents)
    }

    fn confirm_source_relationship(
        &mut self,
        command: &ConfirmSourceRelationshipCommand,
    ) -> Result<DocumentRegister, TenderCommandError> {
        self.require_change_intake_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision, lifecycle_phase): (String, u32, String) = transaction
            .query_row(
                "SELECT tender_id, current_revision, lifecycle_phase FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(sql_error)?;
        if tender_id != command.tender_id {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        for (artifact_id, version) in [
            (&command.prior_artifact_id, command.prior_version),
            (
                &command.replacement_artifact_id,
                command.replacement_version,
            ),
        ] {
            let has_completed_parsed_evidence: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1
                       FROM source_artifact_versions sav
                       JOIN parsed_documents pd
                         ON pd.artifact_id = sav.artifact_id AND pd.version = sav.version
                       JOIN parse_attempts pa
                         ON pa.attempt_id = pd.attempt_id
                       WHERE sav.artifact_id = ?1 AND sav.version = ?2
                         AND sav.registration_state = 'registered'
                         AND pa.status = 'parsed'
                         AND pa.completed_at IS NOT NULL
                         AND pd.location_count > 0
                     )",
                    params![artifact_id, version],
                    |row| row.get(0),
                )
                .map_err(sql_error)?;
            if !has_completed_parsed_evidence {
                return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
            }
        }
        let relationship_id = random_identifier(&transaction)?;
        let created_at = sqlite_timestamp(&transaction)?;
        transaction
            .execute(
                "INSERT INTO source_relationships (
                   relationship_id, prior_artifact_id, prior_version,
                   replacement_artifact_id, replacement_version, relationship_kind, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    relationship_id,
                    command.prior_artifact_id,
                    command.prior_version,
                    command.replacement_artifact_id,
                    command.replacement_version,
                    command.relationship_kind.as_str(),
                    created_at
                ],
            )
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        append_audit_event(
            &transaction,
            &tender_id,
            "source_relationship_confirmed",
            tender_revision,
            json!({
                "prior_artifact_id": command.prior_artifact_id,
                "prior_version": command.prior_version.to_string(),
                "relationship_kind": command.relationship_kind.as_str(),
                "replacement_artifact_id": command.replacement_artifact_id,
                "replacement_version": command.replacement_version.to_string(),
            }),
            &created_at,
        )?;
        let parsed_tender_id = TenderId::parse(&tender_id)?;
        TenderStore::open_change_assessment_in_transaction(
            &transaction,
            &parsed_tender_id,
            tender_revision,
            &relationship_id,
            TenderLifecyclePhase::parse(&lifecycle_phase)?,
            &created_at,
            BidPackageOperationBudget::for_tender(&parsed_tender_id),
        )?;
        transaction.commit().map_err(sql_error)?;
        self.document_register()
    }

    fn record_command_denied(
        &mut self,
        expected_tender_id: &TenderId,
        command_name: &str,
    ) -> Result<(), TenderCommandError> {
        self.require_storage_writable()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        let (tender_id, tender_revision): (String, u32) = transaction
            .query_row(
                "SELECT tender_id, current_revision FROM tender WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        if tender_id != expected_tender_id.as_str() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let created_at = sqlite_timestamp(&transaction)?;
        append_audit_event(
            &transaction,
            &tender_id,
            "tender_command_denied",
            tender_revision,
            json!({
                "command": command_name,
                "reason": "invalid_command",
            }),
            &created_at,
        )?;
        transaction.commit().map_err(sql_error)
    }

    fn inspection(&self) -> Result<TenderInspection, TenderCommandError> {
        let (content_object_count, content_version_count): (i64, i64) = self
            .connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM content_objects),
                   (SELECT COUNT(*) FROM content_versions)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(sql_error)?;
        Ok(TenderInspection {
            summary: self.summary()?,
            content_object_count: content_object_count
                .try_into()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
            content_version_count: content_version_count
                .try_into()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?,
        })
    }
}

pub(crate) type OpenTenderStores = Mutex<HashMap<TenderId, Arc<Mutex<TenderStore>>>>;

pub(crate) struct TenderStoreLease {
    store: Arc<Mutex<TenderStore>>,
    _ordinary_work: OrdinaryWorkLease,
}

impl Deref for TenderStoreLease {
    type Target = Arc<Mutex<TenderStore>>;

    fn deref(&self) -> &Self::Target {
        &self.store
    }
}

impl QuantixHost {
    pub fn create_tender(
        &self,
        command: CreateTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        let name = command.name.trim();
        if name.is_empty() || name.len() > MAX_TENDER_NAME_BYTES {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }

        let tender_id = self.generate_tender_id()?;
        let stage_root = self
            .application_home()
            .join("staging")
            .join(format!("tender-{}", tender_id.as_str()));
        let final_root = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let mut store = match TenderStore::create(&stage_root, &tender_id, name) {
            Ok(store) => store,
            Err(error) => {
                let _ = fs::remove_dir_all(&stage_root);
                return Err(error);
            }
        };
        if let Err(error) =
            store.seed_application_ai_execution_binding(self.application_home(), &tender_id)
        {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(error);
        }
        let summary = store.summary()?;
        if let Err(error) = self.upsert_catalogue_summary(&summary) {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(error);
        }
        drop(store);
        storage_publication_failpoint("tender_after_stage");
        if let Err(error) = fs::rename(&stage_root, &final_root) {
            let _ = fs::remove_dir_all(&stage_root);
            let _ = self.remove_catalogue_entry(&tender_id);
            return Err(store_unavailable(error));
        }
        storage_publication_failpoint("tender_after_publish");
        Ok(summary)
    }

    pub fn close_tender(&self, tender_id: &str) -> Result<(), TenderCommandError> {
        let tender_id = TenderId::parse(tender_id)?;
        let mut stores = self
            .open_tender_stores()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        if stores
            .get(&tender_id)
            .is_some_and(|store| Arc::strong_count(store) != 1)
        {
            return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
        }
        stores.remove(&tender_id);
        Ok(())
    }

    pub fn revise_tender(
        &self,
        command: ReviseTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        self.require_document_tools()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command.validate().is_err() {
            return self.reject_tender_command(&tender_id, "revise_tender");
        }
        let name = command.name.trim();
        if name.is_empty() || name.len() > MAX_TENDER_NAME_BYTES {
            return self.reject_tender_command(&tender_id, "revise_tender");
        }
        let store = self.tender_store(&tender_id)?;
        let summary = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .revise(&command)?;
        Ok(summary)
    }

    pub fn register_tender_content(
        &self,
        command: RegisterTenderContentCommand,
    ) -> Result<ContentVersionSummary, TenderCommandError> {
        self.require_document_tools()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command.validate().is_err()
            || command.bytes.len() > MAX_CONTENT_BYTES
            || !valid_logical_id(&command.logical_id)
            || !valid_media_type(&command.media_type)
        {
            return self.reject_tender_command(&tender_id, "register_tender_content");
        }
        let store = self.tender_store(&tender_id)?;
        let content = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .register_content(&command)?;
        Ok(content)
    }

    pub fn import_tender_package(
        &self,
        command: ImportTenderPackageCommand,
    ) -> Result<TenderPackageImportResult, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let source = Path::new(&command.source_path);
        if command.validate().is_err()
            || !source.is_absolute()
            || fs::symlink_metadata(source).is_err()
        {
            return self.reject_tender_command(&tender_id, "import_tender_package");
        }
        let store = self.tender_store(&tender_id)?;
        let imported = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .import_package(source)?;
        Ok(imported)
    }

    /// Imports an add-package operation through a private staging cache.  No
    /// source bytes are promoted to the live cache and no database transaction
    /// is opened until preparation has completed and cancellation is disabled.
    pub(crate) fn import_tender_package_with_control(
        &self,
        command: ImportTenderPackageCommand,
        control: &PackageIntakeControl,
    ) -> Result<Option<TenderPackageImportResult>, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        let source = Path::new(&command.source_path);
        if command.validate().is_err()
            || !source.is_absolute()
            || fs::symlink_metadata(source).is_err()
        {
            return self.reject_tender_command(&tender_id, "import_tender_package");
        }
        if control.is_cancelled() {
            return Ok(None);
        }
        let store = self.tender_store(&tender_id)?;
        let mut store = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let operation_id = control.snapshot().operation_id;
        if !valid_package_operation_id(&operation_id) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let stage_root = store
            .root
            .join("staging")
            .join(format!("package-{operation_id}"));
        let staged_content_root = stage_root.join("content");
        if let Err(error) = fs::create_dir(&stage_root) {
            return Err(store_unavailable(error));
        }
        if let Err(error) = fs::create_dir(&staged_content_root) {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(store_unavailable(error));
        }

        let outcome = (|| -> Result<Option<TenderPackageImportResult>, TenderCommandError> {
            let Some(prepared) =
                prepare_package_with_control(source, &staged_content_root, Some(control))?
            else {
                return Ok(None);
            };
            if control.is_cancelled() {
                return Ok(None);
            }
            // Promotion and the database transaction form one non-cancellable
            // finalization boundary.  The staging cache remains private until
            // this point, so cancellation cannot expose partial intake rows.
            control.set_stage(crate::tender_intake::PackageIntakeStage::RecordingDocuments);
            control.set_total(Some(
                u32::try_from(prepared.documents.len()).unwrap_or(u32::MAX),
            ));
            control.mark_finalization();
            let promoted = store.promote_staged_content(&prepared, &staged_content_root)?;
            match store.publish_intake(prepared, Some(control)) {
                Ok(result) => Ok(Some(result)),
                Err(error) => {
                    for integrity in promoted {
                        let _ = cacache::remove_hash_sync(store.root.join("content"), &integrity);
                    }
                    Err(error)
                }
            }
        })();
        drop(store);
        let _ = fs::remove_dir_all(&stage_root);
        outcome
    }

    pub fn inspect_document_register(
        &self,
        tender_id: &str,
    ) -> Result<DocumentRegister, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let register = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .document_register()?;
        Ok(register)
    }

    pub fn confirm_source_relationship(
        &self,
        command: ConfirmSourceRelationshipCommand,
    ) -> Result<DocumentRegister, TenderCommandError> {
        self.require_document_tools()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(&command.tender_id)?;
        if command.validate().is_err()
            || !valid_identifier(&command.prior_artifact_id)
            || !valid_identifier(&command.replacement_artifact_id)
            || (command.prior_artifact_id == command.replacement_artifact_id
                && command.prior_version == command.replacement_version)
        {
            return self.reject_tender_command(&tender_id, "confirm_source_relationship");
        }
        let store = self.tender_store(&tender_id)?;
        let register = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .confirm_source_relationship(&command)?;
        Ok(register)
    }

    pub fn inspect_tender(&self, tender_id: &str) -> Result<TenderInspection, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let inspection = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .inspection()?;
        Ok(inspection)
    }

    pub fn inspect_tender_integrity(
        &self,
        tender_id: &str,
    ) -> Result<TenderIntegrityReport, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        self.inspect_tender_integrity_with_check(&tender_id, || Ok(()))
    }

    pub(crate) fn inspect_tender_integrity_with_check(
        &self,
        tender_id: &TenderId,
        mut check: impl FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<TenderIntegrityReport, TenderCommandError> {
        check()?;
        let root = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let cached = self.open_tender_stores();
        let cached = lock_mutex_with_check(cached, &mut check)?
            .get(tender_id)
            .cloned();
        let report = if let Some(cached) = &cached {
            let mut guard = match lock_mutex_with_check(cached, &mut check) {
                Ok(guard) => guard,
                Err(error) if error.code == TenderErrorCode::OperationTimedOut => {
                    return Err(error);
                }
                Err(_) => {
                    self.mark_tender_recovery_required(tender_id);
                    return Ok(recovery_report(
                        tender_id,
                        vec![TenderIntegrityIssue::InspectionUnavailable],
                    ));
                }
            };
            let mut report = inspection_result_or_recovery(
                tender_id,
                TenderStore::inspect_integrity_with_check(&root, tender_id, &mut check),
                true,
            )?;
            if report.state == TenderIntegrityState::RecoveryRequired {
                if guard.latch_recovery_required().is_err()
                    && !report
                        .issues
                        .contains(&TenderIntegrityIssue::InspectionUnavailable)
                {
                    report
                        .issues
                        .push(TenderIntegrityIssue::InspectionUnavailable);
                }
                self.mark_tender_recovery_required(tender_id);
            }
            report
        } else {
            inspection_result_or_recovery(
                tender_id,
                TenderStore::inspect_integrity_with_check(&root, tender_id, &mut check),
                false,
            )?
        };
        if cached.is_none() && report.state == TenderIntegrityState::RecoveryRequired {
            self.mark_tender_recovery_required(tender_id);
        }
        check()?;
        Ok(report)
    }

    pub fn open_tender(&self, tender_id: &str) -> Result<TenderSummary, TenderCommandError> {
        require_setup(self)?;
        let tender_id = TenderId::parse(tender_id)?;
        let store = self.tender_store(&tender_id)?;
        let summary = store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .summary()?;
        Ok(summary)
    }

    pub fn list_tenders(&self) -> Result<Vec<TenderCatalogueEntry>, TenderCommandError> {
        let _ordinary_work = self.begin_ordinary_work()?;
        require_setup(self)?;
        let mut catalogue = Vec::new();
        let mut verified_summaries = Vec::new();
        let entries =
            fs::read_dir(self.application_home().join("tenders")).map_err(store_unavailable)?;
        for entry in entries {
            let entry = entry.map_err(store_unavailable)?;
            let tender_id = entry
                .file_name()
                .to_str()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                .and_then(|value| {
                    TenderId::parse(value)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                })?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
            if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
                catalogue.push(TenderCatalogueEntry {
                    tender_id: tender_id.as_str().to_owned(),
                    summary: None,
                    integrity: recovery_report(
                        &tender_id,
                        vec![TenderIntegrityIssue::StorageLayoutInvalid],
                    ),
                });
                continue;
            }
            let integrity = self.inspect_tender_integrity(tender_id.as_str())?;
            let summary = if integrity.state == TenderIntegrityState::Ready {
                let store = self.tender_store(&tender_id)?;
                let summary = store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .summary()?;
                verified_summaries.push(summary.clone());
                Some(summary)
            } else {
                None
            };
            catalogue.push(TenderCatalogueEntry {
                tender_id: tender_id.as_str().to_owned(),
                summary,
                integrity,
            });
        }
        catalogue.sort_by(|left, right| left.tender_id.cmp(&right.tender_id));
        let _ = self.upsert_catalogue_summaries(&verified_summaries);
        Ok(catalogue)
    }

    pub(crate) fn all_tender_integrity_ready_for_update(&self) -> Result<bool, TenderCommandError> {
        if !self
            .application_home()
            .join("installation.sqlite")
            .is_file()
        {
            return Err(TenderCommandError::new(TenderErrorCode::SetupRequired));
        }
        let entries =
            fs::read_dir(self.application_home().join("tenders")).map_err(store_unavailable)?;
        for entry in entries {
            let entry = entry.map_err(store_unavailable)?;
            let tender_id = entry
                .file_name()
                .to_str()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                .and_then(|value| {
                    TenderId::parse(value)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                })?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
            if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
                return Ok(false);
            }
            let integrity = self.inspect_tender_integrity_with_check(&tender_id, || Ok(()))?;
            if integrity.state != TenderIntegrityState::Ready {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(crate) fn tender_summaries_for_update(
        &self,
    ) -> Result<Vec<TenderSummary>, TenderCommandError> {
        if !self
            .application_home()
            .join("installation.sqlite")
            .is_file()
        {
            return Err(TenderCommandError::new(TenderErrorCode::SetupRequired));
        }
        let mut summaries = Vec::new();
        let entries =
            fs::read_dir(self.application_home().join("tenders")).map_err(store_unavailable)?;
        for entry in entries {
            let entry = entry.map_err(store_unavailable)?;
            let tender_id = entry
                .file_name()
                .to_str()
                .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                .and_then(|value| {
                    TenderId::parse(value)
                        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
                })?;
            let root = entry.path();
            let metadata = fs::symlink_metadata(&root).map_err(store_unavailable)?;
            if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
                return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
            }
            let integrity = self.inspect_tender_integrity_with_check(&tender_id, || Ok(()))?;
            if integrity.state != TenderIntegrityState::Ready {
                return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
            }
            let cached = self
                .open_tender_stores()
                .lock()
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                .get(&tender_id)
                .cloned();
            let summary = match cached {
                Some(store) => store
                    .lock()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
                    .summary()?,
                None => TenderStore::open(&root, &tender_id)?.summary()?,
            };
            summaries.push(summary);
        }
        summaries.sort_by(|left, right| left.tender_id.cmp(&right.tender_id));
        Ok(summaries)
    }

    pub(crate) fn tender_store(
        &self,
        tender_id: &TenderId,
    ) -> Result<TenderStoreLease, TenderCommandError> {
        self.tender_store_with_check(tender_id, &mut || Ok(()))
    }

    pub(crate) fn tender_store_with_check(
        &self,
        tender_id: &TenderId,
        check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
    ) -> Result<TenderStoreLease, TenderCommandError> {
        let ordinary_work = self.begin_ordinary_work()?;
        let recovery_required = lock_mutex_with_check(self.recovery_required_tenders(), check)?;
        if recovery_required.contains(tender_id) {
            return Err(TenderCommandError::new(TenderErrorCode::RecoveryRequired));
        }
        let mut stores = lock_mutex_with_check(self.open_tender_stores(), check)?;
        if let Some(store) = stores.get(tender_id) {
            return Ok(TenderStoreLease {
                store: Arc::clone(store),
                _ordinary_work: ordinary_work,
            });
        }

        let root = self
            .application_home()
            .join("tenders")
            .join(tender_id.as_str());
        let store = Arc::new(Mutex::new(TenderStore::open(&root, tender_id)?));
        check()?;
        stores.insert(tender_id.clone(), Arc::clone(&store));
        Ok(TenderStoreLease {
            store,
            _ordinary_work: ordinary_work,
        })
    }

    fn mark_tender_recovery_required(&self, tender_id: &TenderId) {
        let mut recovery_required = self
            .recovery_required_tenders()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        recovery_required.insert(tender_id.clone());
        let mut stores = self
            .open_tender_stores()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        stores.remove(tender_id);
    }

    fn reject_tender_command<T>(
        &self,
        tender_id: &TenderId,
        command_name: &str,
    ) -> Result<T, TenderCommandError> {
        let store = self.tender_store(tender_id)?;
        store
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
            .record_command_denied(tender_id, command_name)?;
        Err(TenderCommandError::new(TenderErrorCode::InvalidCommand))
    }

    fn upsert_catalogue_summaries(
        &self,
        summaries: &[TenderSummary],
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(sql_error)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_error)?;
        for summary in summaries {
            transaction
                .execute(
                    "INSERT INTO tender_catalogue (
                       tender_id, name, revision, audit_event_count, audit_chain_head
                     ) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(tender_id) DO UPDATE SET
                       name = excluded.name,
                       revision = excluded.revision,
                       audit_event_count = excluded.audit_event_count,
                       audit_chain_head = excluded.audit_chain_head",
                    params![
                        summary.tender_id,
                        summary.name,
                        summary.revision,
                        summary.audit_event_count as i64,
                        summary.audit_chain_head
                    ],
                )
                .map_err(sql_error)?;
        }
        transaction.commit().map_err(sql_error)?;
        storage_publication_failpoint("catalogue_after_commit");
        Ok(())
    }

    pub(crate) fn upsert_catalogue_summary(
        &self,
        summary: &TenderSummary,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let connection = Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(sql_error)?;
        connection
            .execute(
                "INSERT INTO tender_catalogue (
                   tender_id, name, revision, audit_event_count, audit_chain_head
                 ) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(tender_id) DO UPDATE SET
                   name = excluded.name,
                   revision = excluded.revision,
                   audit_event_count = excluded.audit_event_count,
                   audit_chain_head = excluded.audit_chain_head",
                params![
                    summary.tender_id,
                    summary.name,
                    summary.revision,
                    summary.audit_event_count as i64,
                    summary.audit_chain_head,
                ],
            )
            .map_err(sql_error)?;
        Ok(())
    }

    pub(crate) fn remove_catalogue_entry(
        &self,
        tender_id: &TenderId,
    ) -> Result<(), TenderCommandError> {
        let _guard = self
            .catalogue_lock()
            .lock()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        Connection::open(self.application_home().join("installation.sqlite"))
            .map_err(sql_error)?
            .execute(
                "DELETE FROM tender_catalogue WHERE tender_id = ?1",
                [tender_id.as_str()],
            )
            .map_err(sql_error)?;
        Ok(())
    }
}

pub(crate) fn require_setup(host: &QuantixHost) -> Result<(), TenderCommandError> {
    let outcome = host.ensure_setup();
    match outcome.state {
        SetupState::Ready | SetupState::Warning => host.reconcile_startup_once(),
        _ => Err(TenderCommandError::new(TenderErrorCode::SetupRequired)),
    }
}

pub(crate) fn reconcile_application_staging(
    application_home: &Path,
) -> Result<u32, TenderCommandError> {
    let staging = application_home.join("staging");
    let mut removed_tender_candidates = 0_u32;
    for entry in fs::read_dir(&staging).map_err(store_unavailable)? {
        let entry = entry.map_err(store_unavailable)?;
        let name = entry.file_name();
        let Some(tender_id) = name.to_str().and_then(|name| name.strip_prefix("tender-")) else {
            continue;
        };
        if !valid_identifier(tender_id) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
        if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        remove_verified_directory(&staging, &entry.path())?;
        Connection::open(application_home.join("installation.sqlite"))
            .map_err(sql_error)?
            .execute(
                "DELETE FROM tender_catalogue WHERE tender_id = ?1",
                [tender_id],
            )
            .map_err(sql_error)?;
        removed_tender_candidates = removed_tender_candidates
            .checked_add(1)
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
    }
    Ok(removed_tender_candidates)
}

fn valid_logical_id(logical_id: &str) -> bool {
    !logical_id.is_empty()
        && logical_id.len() <= 100
        && logical_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn valid_identifier(value: &str) -> bool {
    value.len() == 32
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, 'a'..='f'))
}

fn valid_package_operation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn host_staging_candidate_id(name: &str) -> Option<&str> {
    ["parse-", "generation-", "package-"]
        .into_iter()
        .find_map(|prefix| name.strip_prefix(prefix))
}

fn storage_publication_failpoint(name: &str) {
    #[cfg(feature = "runtime-fixture")]
    if std::env::var("QUANTIX_STORAGE_FAILPOINT").as_deref() == Ok(name) {
        std::process::abort();
    }
    #[cfg(not(feature = "runtime-fixture"))]
    let _ = name;
}

fn walkdir_error(error: walkdir::Error) -> TenderCommandError {
    error
        .into_io_error()
        .map(store_unavailable)
        .unwrap_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))
}

fn canonical_direct_storage_directory(
    parent: &Path,
    target: &Path,
) -> Result<Option<PathBuf>, TenderCommandError> {
    if target.parent() != Some(parent) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(store_unavailable(error)),
    };
    if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let canonical_parent = fs::canonicalize(parent).map_err(store_unavailable)?;
    let canonical_target = fs::canonicalize(target).map_err(store_unavailable)?;
    if canonical_target.parent() != Some(canonical_parent.as_path()) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(Some(canonical_target))
}

fn validate_content_cache_roots(content_root: &Path) -> Result<(), TenderCommandError> {
    let content_v2 = content_root.join("content-v2");
    let Some(content_v2_root) = canonical_direct_storage_directory(content_root, &content_v2)?
    else {
        return Ok(());
    };
    let hash_root = content_v2_root.join("sha256");
    canonical_direct_storage_directory(&content_v2_root, &hash_root)?;
    Ok(())
}

fn remove_verified_directory(parent: &Path, target: &Path) -> Result<(), TenderCommandError> {
    if target.parent() != Some(parent) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    let canonical_parent = fs::canonicalize(parent).map_err(store_unavailable)?;
    let canonical_target = fs::canonicalize(target).map_err(store_unavailable)?;
    if canonical_target.parent() != Some(canonical_parent.as_path()) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    for entry in walkdir::WalkDir::new(&canonical_target)
        .follow_links(false)
        .into_iter()
        .skip(1)
    {
        let entry = entry.map_err(walkdir_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
        if metadata_is_unsafe_storage_link(&metadata) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    fs::remove_dir_all(canonical_target).map_err(store_unavailable)
}

fn valid_media_type(media_type: &str) -> bool {
    !media_type.is_empty()
        && media_type.len() <= 100
        && media_type.is_ascii()
        && media_type.contains('/')
        && !media_type.chars().any(char::is_whitespace)
}

pub(crate) fn lock_mutex_with_check<'a, T>(
    mutex: &'a Mutex<T>,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<std::sync::MutexGuard<'a, T>, TenderCommandError> {
    loop {
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::WouldBlock) => {
                check()?;
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
            }
        }
    }
}

fn configure_writer(connection: &mut Connection) -> Result<(), TenderCommandError> {
    connection.set_transaction_behavior(TransactionBehavior::Immediate);
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(sql_error)?;
    connection
        .pragma_update(None, "foreign_keys", true)
        .map_err(sql_error)?;
    let journal_mode: String = connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
        .map_err(sql_error)?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable));
    }
    connection
        .pragma_update(None, "synchronous", "FULL")
        .map_err(sql_error)?;
    configure_sqlite_limits(connection)
}

/// Register sqlite-vec before any worker can open a Tender Store.
///
/// SQLite's auto-extension list is process-global.  Registration itself is
/// protected by SQLite's global mutex, but invoking it lazily from concurrent
/// store creation and workspace-refresh workers can re-enter SQLite startup
/// while another connection is being opened.  The Host initializes this once
/// during startup; the existing call sites retain the cheap OnceLock read as
/// a defensive invariant for non-Host test/fresh-process entry points.
pub(crate) fn register_sqlite_vec() -> Result<(), TenderCommandError> {
    static REGISTRATION: OnceLock<bool> = OnceLock::new();
    let registered = *REGISTRATION.get_or_init(|| {
        // sqlite-vec is statically linked; registering its entry point makes it
        // available to every subsequently opened rusqlite connection.
        let entry_point = unsafe {
            std::mem::transmute::<*const (), rusqlite::auto_extension::RawAutoExtension>(
                sqlite_vec::sqlite3_vec_init as *const (),
            )
        };
        unsafe { rusqlite::auto_extension::register_auto_extension(entry_point).is_ok() }
    });
    if registered {
        Ok(())
    } else {
        Err(TenderCommandError::new(TenderErrorCode::StoreUnavailable))
    }
}

fn configure_reader(connection: &Connection) -> Result<(), TenderCommandError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(sql_error)?;
    configure_sqlite_limits(connection)
}

fn configure_sqlite_limits(connection: &Connection) -> Result<(), TenderCommandError> {
    connection
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, 16 * 1024 * 1024)
        .map_err(sql_error)?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 1024 * 1024)
        .map_err(sql_error)?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .map_err(sql_error)?;
    Ok(())
}

fn validate_tender_store_layout(root: &Path) -> Result<(), TenderCommandError> {
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::NotFound))?;
    if metadata_is_unsafe_storage_link(&root_metadata) || !root_metadata.is_dir() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }

    let parent = root
        .parent()
        .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    let canonical_root = fs::canonicalize(root).map_err(store_unavailable)?;
    let canonical_parent = fs::canonicalize(parent).map_err(store_unavailable)?;
    if canonical_root.parent() != Some(canonical_parent.as_path()) {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }

    for entry in fs::read_dir(root).map_err(store_unavailable)? {
        let entry = entry.map_err(store_unavailable)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(store_unavailable)?;
        if metadata_is_unsafe_storage_link(&metadata) {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?
            .to_owned();
        let expected_type = match name.as_str() {
            "content" | "runs" | "staging" => Some(true),
            "tender.sqlite"
            | "tender.sqlite-journal"
            | "tender.sqlite-shm"
            | "tender.sqlite-wal" => Some(false),
            _ => None,
        };
        match expected_type {
            Some(true) if metadata.is_dir() => {}
            Some(false) if metadata.is_file() => {}
            _ => return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
        }
    }

    for directory in ["content", "runs", "staging"] {
        let metadata = fs::symlink_metadata(root.join(directory))
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        if metadata_is_unsafe_storage_link(&metadata) || !metadata.is_dir() {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
    }
    let database_metadata = fs::symlink_metadata(root.join("tender.sqlite"))
        .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
    if metadata_is_unsafe_storage_link(&database_metadata) || !database_metadata.is_file() {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    Ok(())
}

pub(crate) fn metadata_is_unsafe_storage_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }

    false
}

fn inspect_store_structure(
    connection: &Connection,
    expected_tender_id: &TenderId,
) -> Result<Option<TenderIntegrityIssue>, TenderCommandError> {
    inspect_store_structure_with_check(connection, expected_tender_id, &mut || Ok(()))
}

fn inspect_store_structure_with_check(
    connection: &Connection,
    expected_tender_id: &TenderId,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Option<TenderIntegrityIssue>, TenderCommandError> {
    check()?;
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(sql_error)?;
    if version != TENDER_SCHEMA_VERSION {
        return Ok(Some(TenderIntegrityIssue::SchemaMismatch));
    }
    let expected_schema = Connection::open_in_memory().map_err(sql_error)?;
    expected_schema
        .execute_batch(TENDER_SCHEMA)
        .map_err(sql_error)?;
    check()?;
    if tender_schema_objects(connection, check)? != tender_schema_objects(&expected_schema, check)?
    {
        return Ok(Some(TenderIntegrityIssue::SchemaMismatch));
    }
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(sql_error)?;
    check()?;
    if quick_check != "ok" {
        return Ok(Some(TenderIntegrityIssue::DatabaseIntegrityInvalid));
    }
    let foreign_key_failure: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM pragma_foreign_key_check LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    if foreign_key_failure.is_some() {
        return Ok(Some(TenderIntegrityIssue::DatabaseIntegrityInvalid));
    }
    let stored_tender_id: String = connection
        .query_row(
            "SELECT tender_id FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    check()?;
    if stored_tender_id != expected_tender_id.as_str() {
        return Ok(Some(TenderIntegrityIssue::TenderIdentityMismatch));
    }
    Ok(None)
}

fn recovery_report(
    tender_id: &TenderId,
    issues: Vec<TenderIntegrityIssue>,
) -> TenderIntegrityReport {
    TenderIntegrityReport {
        tender_id: tender_id.as_str().to_owned(),
        state: TenderIntegrityState::RecoveryRequired,
        issues,
        recovery_choices: vec![
            TenderRecoveryChoice::RestoreVerifiedBackup,
            TenderRecoveryChoice::PurgeTender,
        ],
    }
}

fn inspection_result_or_recovery(
    tender_id: &TenderId,
    result: Result<TenderIntegrityReport, TenderCommandError>,
    recover_not_found: bool,
) -> Result<TenderIntegrityReport, TenderCommandError> {
    match result {
        Ok(report) => Ok(report),
        Err(error)
            if matches!(
                error.code,
                TenderErrorCode::IntegrityFailed
                    | TenderErrorCode::RecoveryRequired
                    | TenderErrorCode::StoreUnavailable
            ) || (recover_not_found && error.code == TenderErrorCode::NotFound) =>
        {
            Ok(recovery_report(
                tender_id,
                vec![TenderIntegrityIssue::InspectionUnavailable],
            ))
        }
        Err(error) => Err(error),
    }
}

fn recovery_required_if_integrity(error: TenderCommandError) -> TenderCommandError {
    if error.code == TenderErrorCode::IntegrityFailed {
        TenderCommandError::new(TenderErrorCode::RecoveryRequired)
    } else {
        error
    }
}

type SchemaObject = (String, String, String, Option<String>);

fn tender_schema_objects(
    connection: &Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Vec<SchemaObject>, TenderCommandError> {
    let mut statement = connection
        .prepare(
            "SELECT type, name, tbl_name, sql FROM sqlite_schema
             WHERE name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(sql_error)?;
    let mut objects = Vec::new();
    for row in rows {
        check()?;
        objects.push(row.map_err(sql_error)?);
    }
    Ok(objects)
}

fn append_audit_event(
    transaction: &Transaction<'_>,
    tender_id: &str,
    event_type: &str,
    aggregate_revision: u32,
    change: serde_json::Value,
    created_at: &str,
) -> Result<(), TenderCommandError> {
    append_audit_event_with_sequence(
        transaction,
        tender_id,
        event_type,
        aggregate_revision,
        change,
        created_at,
    )
    .map(|_| ())
}

fn append_audit_event_with_sequence(
    transaction: &Transaction<'_>,
    tender_id: &str,
    event_type: &str,
    aggregate_revision: u32,
    change: serde_json::Value,
    created_at: &str,
) -> Result<i64, TenderCommandError> {
    let previous: Option<(i64, String)> = transaction
        .query_row(
            "SELECT sequence, current_hash FROM audit_events ORDER BY sequence DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_error)?;
    let sequence = previous.as_ref().map_or(1, |(sequence, _)| sequence + 1);
    let preceding_hash = previous
        .map(|(_, hash)| hash)
        .unwrap_or_else(|| ZERO_AUDIT_HASH.into());
    let payload = json!({
        "aggregate_revision": aggregate_revision.to_string(),
        "change": change,
        "created_at": created_at,
        "event_type": event_type,
        "preceding_hash": preceding_hash,
        "schema_version": "1",
        "sequence": sequence.to_string(),
        "tender_id": tender_id,
    });
    let payload_json = serde_json_canonicalizer::to_string(&payload)
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
    let current_hash = sha256_hex(payload_json.as_bytes());
    transaction
        .execute(
            "INSERT INTO audit_events (
               sequence, event_type, aggregate_revision, payload_json,
               preceding_hash, current_hash, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                sequence,
                event_type,
                aggregate_revision,
                payload_json,
                preceding_hash,
                current_hash,
                created_at
            ],
        )
        .map_err(sql_error)?;
    Ok(sequence)
}

fn verify_audit_chain(connection: &Connection) -> Result<(), TenderCommandError> {
    verify_audit_chain_with_check(connection, &mut || Ok(()))
}

fn verify_audit_chain_with_check(
    connection: &Connection,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<(), TenderCommandError> {
    check()?;
    let tender_id: String = connection
        .query_row(
            "SELECT tender_id FROM tender WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(sql_error)?;
    let mut statement = connection
        .prepare(
            "SELECT sequence, event_type, aggregate_revision, payload_json,
                    preceding_hash, current_hash, created_at
             FROM audit_events ORDER BY sequence",
        )
        .map_err(sql_error)?;
    let events = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })
        .map_err(sql_error)?;
    let mut expected_sequence = 1_i64;
    let mut expected_preceding = ZERO_AUDIT_HASH.to_owned();
    for event in events {
        check()?;
        let (
            sequence,
            event_type,
            aggregate_revision,
            payload_json,
            preceding_hash,
            current_hash,
            created_at,
        ) = event.map_err(sql_error)?;
        let parsed: serde_json::Value = serde_json::from_str(&payload_json)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let canonical = serde_json_canonicalizer::to_string(&parsed)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::IntegrityFailed))?;
        let sequence_text = sequence.to_string();
        let aggregate_revision_text = aggregate_revision.to_string();
        if sequence != expected_sequence
            || payload_json != canonical
            || preceding_hash != expected_preceding
            || sha256_hex(payload_json.as_bytes()) != current_hash
            || parsed.get("sequence").and_then(serde_json::Value::as_str)
                != Some(sequence_text.as_str())
            || parsed.get("event_type").and_then(serde_json::Value::as_str)
                != Some(event_type.as_str())
            || parsed.get("created_at").and_then(serde_json::Value::as_str)
                != Some(created_at.as_str())
            || parsed
                .get("aggregate_revision")
                .and_then(serde_json::Value::as_str)
                != Some(aggregate_revision_text.as_str())
            || parsed
                .get("preceding_hash")
                .and_then(serde_json::Value::as_str)
                != Some(preceding_hash.as_str())
            || parsed.get("tender_id").and_then(serde_json::Value::as_str)
                != Some(tender_id.as_str())
            || parsed
                .get("schema_version")
                .and_then(serde_json::Value::as_str)
                != Some("1")
        {
            return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
        }
        expected_sequence += 1;
        expected_preceding = current_hash;
    }
    if expected_sequence == 1 {
        return Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed));
    }
    check()?;
    Ok(())
}

fn inspect_referenced_content(
    connection: &Connection,
    content_root: &Path,
) -> Result<Option<TenderIntegrityIssue>, TenderCommandError> {
    inspect_referenced_content_with_check(connection, content_root, &mut || Ok(()))
}

fn inspect_referenced_content_with_check(
    connection: &Connection,
    content_root: &Path,
    check: &mut dyn FnMut() -> Result<(), TenderCommandError>,
) -> Result<Option<TenderIntegrityIssue>, TenderCommandError> {
    #[cfg(test)]
    CONTENT_VERIFY_PASS_COUNT.fetch_add(1, Ordering::SeqCst);
    check()?;
    if let Err(error) = validate_content_cache_roots(content_root) {
        return if error.code == TenderErrorCode::IntegrityFailed {
            Ok(Some(TenderIntegrityIssue::StorageLayoutInvalid))
        } else {
            Err(error)
        };
    }
    let mut statement = connection
        .prepare("SELECT sha256, integrity, size_bytes FROM content_objects ORDER BY sha256")
        .map_err(sql_error)?;
    let objects = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(sql_error)?;
    for object in objects {
        check()?;
        let (expected_sha256, integrity, expected_size) = object.map_err(sql_error)?;
        let expected_size = match u64::try_from(expected_size)
            .ok()
            .filter(|size| *size > 0 && *size <= MAX_CONTENT_BYTES as u64)
        {
            Some(size) => size,
            None => return Ok(Some(TenderIntegrityIssue::ReferencedContentMismatch)),
        };
        let integrity = match integrity.parse::<cacache::Integrity>() {
            Ok(integrity) => integrity,
            Err(_) => return Ok(Some(TenderIntegrityIssue::ReferencedContentMismatch)),
        };
        if !cacache::exists_sync(content_root, &integrity) {
            return Ok(Some(TenderIntegrityIssue::ReferencedContentMissing));
        }
        let mut reader = match cacache::SyncReader::open_hash(content_root, integrity) {
            Ok(reader) => reader,
            Err(_) => return Ok(Some(TenderIntegrityIssue::ReferencedContentMismatch)),
        };
        let mut digest = Sha256::new();
        let mut size_bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            check()?;
            let read = match reader.read(&mut buffer) {
                Ok(read) => read,
                Err(_) => return Ok(Some(TenderIntegrityIssue::ReferencedContentMismatch)),
            };
            if read == 0 {
                break;
            }
            size_bytes = match size_bytes.checked_add(read as u64) {
                Some(size_bytes) if size_bytes <= expected_size => size_bytes,
                _ => return Ok(Some(TenderIntegrityIssue::ReferencedContentMismatch)),
            };
            digest.update(&buffer[..read]);
        }
        let actual_sha256: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if size_bytes != expected_size || actual_sha256 != expected_sha256 {
            return Ok(Some(TenderIntegrityIssue::ReferencedContentMismatch));
        }
    }
    check()?;
    Ok(None)
}

fn sqlite_timestamp(transaction: &Transaction<'_>) -> Result<String, TenderCommandError> {
    transaction
        .query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |row| {
            row.get(0)
        })
        .map_err(sql_error)
}

fn tender_ai_selection_readiness_as_str(readiness: TenderAiSelectionReadiness) -> &'static str {
    match readiness {
        TenderAiSelectionReadiness::LocalOnly => "local_only",
        TenderAiSelectionReadiness::Ready => "ready",
        TenderAiSelectionReadiness::SelectionRequired => "selection_required",
        TenderAiSelectionReadiness::ProviderUnavailable => "provider_unavailable",
        TenderAiSelectionReadiness::CatalogueStale => "catalogue_stale",
        TenderAiSelectionReadiness::ModelUnavailable => "model_unavailable",
        TenderAiSelectionReadiness::ApprovalRequired => "approval_required",
    }
}

fn parse_tender_ai_selection_readiness(
    value: &str,
) -> Result<TenderAiSelectionReadiness, TenderCommandError> {
    match value {
        "local_only" => Ok(TenderAiSelectionReadiness::LocalOnly),
        "ready" => Ok(TenderAiSelectionReadiness::Ready),
        "selection_required" => Ok(TenderAiSelectionReadiness::SelectionRequired),
        "provider_unavailable" => Ok(TenderAiSelectionReadiness::ProviderUnavailable),
        "catalogue_stale" => Ok(TenderAiSelectionReadiness::CatalogueStale),
        "model_unavailable" => Ok(TenderAiSelectionReadiness::ModelUnavailable),
        "approval_required" => Ok(TenderAiSelectionReadiness::ApprovalRequired),
        _ => Err(TenderCommandError::new(TenderErrorCode::IntegrityFailed)),
    }
}

fn random_identifier(connection: &Connection) -> Result<String, TenderCommandError> {
    connection
        .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
        .map_err(sql_error)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn store_unavailable(error: std::io::Error) -> TenderCommandError {
    #[cfg(feature = "runtime-fixture")]
    eprintln!("Tender Store fixture I/O failure: {error}");
    #[cfg(not(feature = "runtime-fixture"))]
    let _ = error;
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

fn sql_error(error: rusqlite::Error) -> TenderCommandError {
    #[cfg(feature = "runtime-fixture")]
    eprintln!("Tender Store fixture SQLite failure: {error}");
    #[cfg(not(feature = "runtime-fixture"))]
    let _ = error;
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

fn content_store_error(_error: cacache::Error) -> TenderCommandError {
    TenderCommandError::new(TenderErrorCode::StoreUnavailable)
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        path::Path,
        sync::{Arc, Barrier},
    };

    use super::*;
    use crate::setup::{SetupPlatform, StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES};

    struct ReadySetupPlatform;

    impl SetupPlatform for ReadySetupPlatform {
        fn available_space(&self, _path: &Path) -> io::Result<u64> {
            Ok(MINIMUM_SETUP_FREE_SPACE_BYTES)
        }

        fn is_writable(&self, _path: &Path) -> io::Result<bool> {
            Ok(true)
        }

        fn storage_permissions(&self, _path: &Path) -> io::Result<StoragePermissions> {
            Ok(StoragePermissions::Restrictive)
        }
    }

    #[test]
    fn oauth_error_codes_serialize_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&TenderErrorCode::OauthPortBlocked).expect("serialize code"),
            r#""oauth_port_blocked""#
        );
        assert_eq!(
            serde_json::to_string(&TenderErrorCode::OauthAlreadyRunning).expect("serialize code"),
            r#""oauth_already_running""#
        );
    }

    #[test]
    fn checked_mutex_wait_honors_the_operation_deadline() {
        let mutex = Mutex::new(());
        let _held = mutex.lock().expect("hold Tender Store lock");
        let error = lock_mutex_with_check(&mutex, &mut || {
            Err(TenderCommandError::new(TenderErrorCode::OperationTimedOut))
        })
        .expect_err("expired lock wait must fail");
        assert_eq!(error.code, TenderErrorCode::OperationTimedOut);
    }

    #[test]
    fn tender_ai_binding_is_local_only_by_default_and_uses_compare_and_swap_revision() {
        let root = tempfile::tempdir().expect("Tender Store root");
        let tender_id = TenderId("0123456789abcdef0123456789abcdef".into());
        let mut store =
            TenderStore::create(&root.path().join("tender"), &tender_id, "AI binding test")
                .expect("create Tender Store");

        let initial = store
            .inspect_tender_ai_execution_binding()
            .expect("inspect default binding");
        assert_eq!(initial.revision, 1);
        assert_eq!(initial.selection, None);
        assert_eq!(initial.readiness, TenderAiSelectionReadiness::LocalOnly);

        let updated = store
            .update_tender_ai_execution_binding(
                &tender_id,
                initial.revision,
                None,
                TenderAiSelectionReadiness::LocalOnly,
                "Local-only work remains available.",
            )
            .expect("record local-only binding");
        assert_eq!(updated.revision, 2);

        let stale = store
            .update_tender_ai_execution_binding(
                &tender_id,
                initial.revision,
                None,
                TenderAiSelectionReadiness::LocalOnly,
                "stale revision must fail",
            )
            .expect_err("stale binding update");
        assert_eq!(stale.code, TenderErrorCode::InvalidCommand);
    }

    #[test]
    fn tender_ai_binding_rejects_local_only_readiness_with_a_selection() {
        let root = tempfile::tempdir().expect("Tender Store root");
        let tender_id = TenderId("fedcba9876543210fedcba9876543210".into());
        let mut store = TenderStore::create(
            &root.path().join("tender"),
            &tender_id,
            "AI binding validation",
        )
        .expect("create Tender Store");
        let selection = AiExecutionSelection {
            connection_id: "provider".into(),
            provider: crate::application_settings::AiProviderKind::Codex,
            model_id: "model".into(),
            reasoning: crate::application_settings::ProviderReasoningSelection::ProviderDefault,
            catalogue_fetched_at: "catalogue".into(),
            adapter_version: "adapter".into(),
        };
        let error = store
            .update_tender_ai_execution_binding(
                &tender_id,
                1,
                Some(selection),
                TenderAiSelectionReadiness::LocalOnly,
                "invalid local-only binding",
            )
            .expect_err("selection cannot be local-only");
        assert_eq!(error.code, TenderErrorCode::InvalidCommand);
    }

    #[test]
    fn workspace_refresh_reuses_the_validated_selected_store() {
        let user_home = tempfile::tempdir().expect("temporary user home");
        let application_home = user_home.path().join(".quantix");
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(host.ensure_setup().state, SetupState::Ready);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Workspace refresh projection".into(),
            })
            .expect("create Tender");
        let other_tender = host
            .create_tender(CreateTenderCommand {
                name: "Workspace refresh sibling".into(),
            })
            .expect("create sibling Tender");
        host.close_tender(&tender.tender_id)
            .expect("close first Tender before cold projection");
        host.close_tender(&other_tender.tender_id)
            .expect("close sibling Tender before cold projection");
        TENDER_STORE_OPEN_COUNT.store(0, Ordering::SeqCst);
        CONTENT_VERIFY_PASS_COUNT.store(0, Ordering::SeqCst);

        host.inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id.clone()),
        })
        .expect("initial workspace projection");
        let first_open_count = TENDER_STORE_OPEN_COUNT.load(Ordering::SeqCst);
        let first_verify_count = CONTENT_VERIFY_PASS_COUNT.load(Ordering::SeqCst);
        assert_eq!(first_open_count, 1);
        assert!(first_verify_count > 0);

        host.inspect_manager_workspace(InspectManagerWorkspaceCommand {
            tender_id: Some(tender.tender_id),
        })
        .expect("routine workspace refresh");
        assert_eq!(
            TENDER_STORE_OPEN_COUNT.load(Ordering::SeqCst),
            first_open_count
        );
        assert_eq!(
            CONTENT_VERIFY_PASS_COUNT.load(Ordering::SeqCst),
            first_verify_count
        );
    }

    #[test]
    fn cold_open_has_one_writer_and_close_rejects_a_borrowed_store() {
        let user_home = tempfile::tempdir().expect("temporary user home");
        let application_home = user_home.path().join(".quantix");
        let host =
            QuantixHost::with_setup_platform(&application_home, Arc::new(ReadySetupPlatform));
        assert_eq!(host.ensure_setup().state, SetupState::Ready);
        let tender = host
            .create_tender(CreateTenderCommand {
                name: "Observable writer ownership".into(),
            })
            .expect("create Tender");
        host.close_tender(&tender.tender_id).expect("close Tender");
        TENDER_STORE_OPEN_COUNT.store(0, Ordering::SeqCst);

        let barrier = Arc::new(Barrier::new(8));
        let opens = (0..8)
            .map(|_| {
                let host = host.clone();
                let tender_id = tender.tender_id.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    host.open_tender(&tender_id)
                })
            })
            .collect::<Vec<_>>();
        for open in opens {
            assert_eq!(
                open.join()
                    .expect("open thread")
                    .expect("serialized cold open"),
                tender
            );
        }
        assert_eq!(TENDER_STORE_OPEN_COUNT.load(Ordering::SeqCst), 1);

        let tender_id = TenderId::parse(&tender.tender_id).expect("valid Tender identity");
        let borrowed_store = host.tender_store(&tender_id).expect("borrow open writer");
        let close_error = host
            .close_tender(&tender.tender_id)
            .expect_err("borrowed writer cannot be closed");
        assert_eq!(close_error.code, TenderErrorCode::StoreUnavailable);
        drop(borrowed_store);
        host.close_tender(&tender.tender_id)
            .expect("close unborrowed writer");
    }
}
