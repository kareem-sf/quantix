#![recursion_limit = "256"]

mod acceptance;
mod agent_runtime;
mod document_parsing;
mod host;
mod process_supervisor;
mod release_gate;
mod runtime_readiness;
mod setup;
mod tender_intake;
mod tender_store;
mod update;

pub use acceptance::{
    acceptance_fixture_sha256, acceptance_oracle_sha256, print_candidate_acceptance_probe,
    AcceptanceArtifactHash, AcceptanceCheckResult, AcceptanceStageTiming, LiveQualificationMetrics,
    LiveQualificationRun, PrivateQualificationRecord, ProductAcceptanceOutcome,
    ProductAcceptanceRecord, ProductAcceptanceRun, RecordLiveQualificationRunCommand,
    RunDeterministicAcceptanceCommand,
};
pub use agent_runtime::{
    approve_one_run_access, AccessApproval, AccessRequest, AgentAccessRequestStatus,
    AgentAccessRequestView, AgentAccessResolution, AgentProfileStatus, AgentProfileVersionView,
    AgentResourceBudget, AgentRunActivity, AgentRunHistoryItem, AgentRunHistoryPage,
    AgentRunInspection, AgentRunPermissions, AgentRunRecoveryDecision, AgentRunRecoveryDisposition,
    AgentRunState, AgentRunSummary, AgentRunWorkspaceManifest, AgentTaskInputReference,
    ApproveAgentAccessCommand, BootstrapAuthority, BootstrapRole, BootstrapTeamMember,
    CodexReadiness, DataClassification, DataViewManifest, InspectAgentRunCommand,
    InspectAgentRunHistoryCommand, InterruptAgentRunCommand, OneRunAccessGrant, PermissionCeiling,
    PermissionDenialReason, PermissionGrant, ProposedAgentResult, ProviderEvent, ProviderEventKind,
    ProviderFailure, ProviderFailureCategory, ProviderRateLimit, ProviderRateLimitState,
    ProviderRateLimitWindow, ProviderUsage, RequestAgentAccessCommand, ResolveAgentAccessCommand,
    ResolveIndeterminateAgentRunCommand, RunBootstrapAgentCommand, TenderTaskView,
    ThreadExposureSet, ToolIdempotency, ToolSideEffectClass, TypedToolDefinition, TypedToolQuota,
    VerificationStatus,
};
pub use document_parsing::{
    DocumentParseResult, EvidenceBoundingBox, EvidenceDocument, EvidenceLanguage, EvidenceLocation,
    EvidenceLocationKind, EvidenceRegion, EvidenceSearchHit, EvidenceSearchResult,
    ParseExceptionCode, ParseSourceArtifactCommand, ParseState, SearchEvidenceCommand,
    TextDirection,
};
pub use host::QuantixHost;
pub use release_gate::{
    release_candidate_manifest_sha256, CodexProductionAssuranceEvidence,
    EvaluatePublicReleaseGateCommand, IntegrationTermsDecision, LicenseDistributionReview,
    NativePlatformQualificationEvidence, NativePlatformQualificationRecord,
    PublicReleaseGateOutcome, PublicReleaseGateRecord, RecordNativePlatformQualificationCommand,
    TechnicalRiskAcceptance,
};
pub use runtime_readiness::{
    RuntimeLayout, RuntimeReadiness, RuntimeReadinessIssue, RuntimeReadinessState,
};
pub use setup::{
    ensure_quantix_setup, DeviceProtection, SetupIssue, SetupOutcome, SetupPlatform, SetupState,
    StoragePermissions, MINIMUM_SETUP_FREE_SPACE_BYTES,
};
pub use tender_intake::{
    ChooseTenderPackageCommand, ConfirmSourceRelationshipCommand, DocumentRegister,
    DocumentRegisterEntry, ImportTenderPackageCommand, IntakeExceptionCode, RegistrationState,
    SourceRelationshipKind, SupersessionState, TenderPackageImportResult, TenderPackageSourceKind,
};
pub use tender_store::{
    ActivateTenderProductionCommand, AgentTenderQueryProposal, AgentTenderQueryUpdate,
    ApproveBasisOfEstimateCommand, ApproveCalculationRuleCommand, ApproveCommercialStrategyCommand,
    ApproveControlledBoqCalculationRunCommand, ApproveExternalRfiForIssueCommand,
    ApprovePackageFindingExceptionCommand, ApprovePricedCostBaselineCommand,
    ApprovePricingAdjustmentCommand, ApproveProductionFindingExceptionCommand,
    ApproveSubmissionReleaseCommand, ApproveTenderPriceCommand, ApprovedQueryTreatment,
    ApprovedTenderPrice, AssembleCoordinatedBidBaselineCommand, AssembleSubmissionPackageCommand,
    BasisOfEstimateReview, BasisOfEstimateReviewFinding, BasisOfEstimateReviewOutcome,
    BasisOfEstimateReviewResult, BasisOfEstimateVersion, BidDecisionApprovalDecision,
    BidDecisionApprovalHistoryPage, BidDecisionApprovalInvalidation,
    BidDecisionApprovalInvalidationResult, BidDecisionApprovalRecord, BidDecisionApprovalResult,
    BidDecisionGateBlocker, BidDecisionPackageChangeSummary, BidDecisionPackageInspection,
    BidDecisionPackageRecordBinding, BidDecisionPackageRecordCategory,
    BidDecisionPackageRecordPage, BidDecisionPackageReview, BidDecisionPackageReviewFinding,
    BidDecisionPackageReviewOutcome, BidDecisionPackageReviewResult,
    BidDecisionReturnReworkDisposition, BidDecisionReturnReworkItem, BidDecisionReturnReworkResult,
    BidRecommendation, BidRecommendationOutcome, BoqAccountRow, BoqInventoryRow, BoqRowDisposition,
    BoqTableCandidate, BoqTableDesignation, CalculationDecimalInput, CalculationInputState,
    CalculationRoundingMode, CalculationRuleApproval, CalculationRuleReview,
    CalculationRuleReviewFinding, CalculationRuleReviewOutcome, CalculationRuleReviewResult,
    CalculationRuleTestResult, CalculationRuleVersion, CalculationScenarioVersion,
    CalculationWorkspaceInspection, CapabilityDemand, CapabilityDemandClassification,
    ChangeAssessment, ChangeAssessmentApprovalConsequence, ChangeAssessmentClassification,
    ChangeAssessmentDecision, ChangeAssessmentDependencyKind, ChangeAssessmentDependencyReference,
    ChangeAssessmentEvidenceExcerpt, ChangeAssessmentImpact, ChangeAssessmentImpactConsequence,
    ChangeAssessmentImpactKind, ChangeAssessmentObjectKind, ChangeAssessmentPage,
    ChangeAssessmentSource, ChangeAssessmentStatus, CommercialStrategy, CommercialStrategyApproval,
    ComplianceDisposition, ComplianceDispositionUpdate, ComplianceMatrixPage, ComplianceMatrixRow,
    ComposeTenderOfficeCommand, ContentVersionSummary, ControlledBoqCalculationRun,
    ControlledBoqCalculationStatus, CoordinatedBidBaseline, CoordinatedBidBaselineApproval,
    CoordinatedBidBaselineBinding, CoordinatedBidBaselineBindingKind,
    CoordinatedBidBaselineBlocker, CoordinatedBidBaselineBlockerCode,
    CoordinatedBidBaselineCategory, CoordinatedBidBaselineContradiction,
    CoordinatedBidBaselineContradictionCategory, CoordinatedBidBaselineDecision,
    CoordinatedBidBaselinePage, CostBreakdownComponent, CostComponentCategory,
    CostEstimatorBasisResult, CostEstimatorCalculationResult, CreateBidDecisionPackageCommand,
    CreateCalculationScenarioCommand, CreateCommercialStrategyCommand,
    CreateExternalRfiDraftCommand, CreatePortableTenderArchiveCommand,
    CreatePricedCostBaselineCommand, CreatePricingAdjustmentCommand, CreatePricingScenarioCommand,
    CreateTenderBackupCommand, CreateTenderCommand, CreateTenderEngineerEntryCommand,
    CreateTenderQueryCommand, DecideBidDecisionPackageCommand, DecideChangeAssessmentCommand,
    DecideCoordinatedBidBaselineCommand, DecideTenderQueryTreatmentCommand,
    DecideTenderRecordCommand, DecideWorkPlanProposalCommand, DecisionAction, DecisionCockpit,
    DecisionDependency, DecisionDependencyStatus, DecisionEvidence, DecisionFact, DecisionFactKind,
    DecisionGroupMember, DecisionKind, DecisionLifecycleGate, DecisionResponsible,
    DecisionResponsibleKind, DecisionStatus, DecisionTarget, DecisionTargetKind, DecisionUrgency,
    DeletionReceipt, DesignateBoqTableCommand, EstimateAggregateCalculationInput,
    EstimateAggregateCalculationRun, EstimateAllowance, EstimateMaterialAssumption,
    EstimateQueryObservation, EstimateQueryReference, EstimateQuotation, EstimateQuotationKind,
    EstimateWorkspaceInspection, ExchangeRateType, ExportApprovedExternalRfiCommand,
    ExportReleaseCopyCommand, ExternalRfiApproval, ExternalRfiDraft, ExternalRfiEligibleQuery,
    ExternalRfiEligibleQueryPage, ExternalRfiExportRecord, ExternalRfiFindingSeverity,
    ExternalRfiPage, ExternalRfiQueryReference, ExternalRfiQuestion, ExternalRfiRecipient,
    ExternalRfiResponseCandidatePage, ExternalRfiResponseInterpretation, ExternalRfiResponseLink,
    ExternalRfiReview, ExternalRfiReviewFinding, ExternalRfiReviewOutcome, ExternalRfiReviewResult,
    FinalReviewAssignment, FinalReviewInspection, FinalReviewPlan, FinalReviewReviewer,
    GenerateSubmissionSectionsCommand, GenerationAuthoringMode, GenerationRequirement,
    GenerationRequirementAvailability, GenerationRequirementKind,
    GenerationRequirementRecordReference, ImportPortableTenderArchiveCommand,
    InspectBidDecisionApprovalHistoryCommand, InspectBidDecisionPackageRecordsCommand,
    InspectCalculationWorkspaceCommand, InspectChangeAssessmentsCommand,
    InspectComplianceMatrixCommand, InspectCoordinatedBidBaselinesCommand,
    InspectDecisionCockpitCommand, InspectEstimateWorkspaceCommand,
    InspectExternalRfiEligibleQueriesCommand, InspectExternalRfiResponseCandidatesCommand,
    InspectExternalRfisCommand, InspectPackageProductionCommand, InspectPricingWorkspaceCommand,
    InspectProductionTaskReviewCommand, InspectSubmissionArtifactContentCommand,
    InspectSubmissionPackageCommand, InspectSubmissionPackageItemContentCommand,
    InspectTenderQueriesCommand, InspectTenderRecordsCommand, InterpretExternalRfiResponseCommand,
    InvalidateBidDecisionApprovalCommand, MajorFindingPolicy, ManagerCapabilityDemandInput,
    ManualVerificationResult, OpenTenderCommand, PackageFindingExceptionApproval,
    PackageManualVerification, PackageProductionGeneration, PackageReviewFinding,
    PackageReviewResult, PackageValidationCheckCategory, PackageValidationOutcome,
    PackageValidationPolicy, PackageValidationResult, PackageValidationRule, PackageValidationRun,
    PendingDecision, PortableTenderArchiveRecord, PrepareTenderRecoveryCommand,
    PricedCostBaselineApproval, PricedCostBaselineReview, PricedCostBaselineReviewFinding,
    PricedCostBaselineReviewOutcome, PricedCostBaselineReviewResult, PricedCostBaselineVersion,
    PricingAdjustmentApproval, PricingAdjustmentDirection, PricingAdjustmentKind,
    PricingAdjustmentReference, PricingAdjustmentReviewResult, PricingAdjustmentVersion,
    PricingCalculationAdjustmentInput, PricingCalculationRun, PricingDecisionHistoryEntry,
    PricingScenarioSelection, PricingScenarioVersion, PricingWorkspaceInspection,
    ProductionArtifactPayload, ProductionArtifactVersion, ProductionArtifactVersionSummary,
    ProductionCoordinationObservation, ProductionCoordinationObservationSubject,
    ProductionCoordinationObservationValue, ProductionFindingDisposition,
    ProductionFindingDispositionKind, ProductionFindingSeverity, ProductionIntegrationReadiness,
    ProductionQueryTreatmentApplication, ProductionRemediation, ProductionReview,
    ProductionReviewFinding, ProductionReviewResult, ProductionTaskInspection,
    ProductionTaskReviewInspection, ProductionTaskRunResult, ProductionTaskState,
    ProposeBoqCalculationRuleCommand, RecordPackageManualVerificationCommand,
    RegisterExternalRfiResponseCommand, RegisterTenderContentCommand, ReleaseCopyExport,
    ReleaseCopyItem, ReleaseReadinessBlocker, ReleaseReadinessBlockerCode,
    ReleaseReadinessCategorySummary, ReleaseReadinessReport, ResolveBidDecisionReturnReworkCommand,
    ResolveTenderRecoveryCommand, ResourceImplication, ReviewFindingSeverity,
    ReviseExternalRfiDraftCommand, ReviseTenderCommand, ReviseTenderQueryCommand,
    ReviseWorkPlanProposalCommand, RunBasisOfEstimateReviewCommand,
    RunBidDecisionPackageReviewCommand, RunCalculationRuleReviewCommand,
    RunCostEstimatorBasisCommand, RunCostEstimatorCalculationCommand, RunExternalRfiReviewCommand,
    RunPackageValidationCommand, RunPricedCostBaselineReviewCommand,
    RunPricingAdjustmentReviewCommand, RunProductionTaskCommand, RunSubmissionSectionReviewCommand,
    RunTenderRecordExtractionCommand, RunTenderRecordReviewCommand, SelectPricingScenarioCommand,
    StartupReconciliationReport, SubmissionArtifactContent, SubmissionArtifactVersion,
    SubmissionAuthorshipProvenance, SubmissionContributionKind, SubmissionCoverageBlocker,
    SubmissionCoverageBlockerCode, SubmissionCoverageDisposition, SubmissionCoverageRow,
    SubmissionGeneratedArtifactReference, SubmissionItemContent, SubmissionItemSource,
    SubmissionPackageAssessment, SubmissionPackageCurrentnessCode,
    SubmissionPackageCurrentnessFact, SubmissionPackageDependency, SubmissionPackageDependencyKind,
    SubmissionPackageItem, SubmissionPackageSection, SubmissionPackageStatus,
    SubmissionPackageVersion, SubmissionProfileVersionReference, SubmissionReleaseApproval,
    SubmissionReleaseInspection, SubmissionReleaseState, SubmissionSectionIndependenceContext,
    SubmissionSectionReview, SubmissionSectionReviewRunResult, SubmissionSectionRiskContext,
    SubmissionSourceArtifactReference, SubmissionValidationContextInput, SubmissionWorkPlanContext,
    TenderBackupRecord, TenderBackupState, TenderCatalogueEntry, TenderCommandError,
    TenderErrorCode, TenderEvidenceReference, TenderInspection, TenderIntegrityIssue,
    TenderIntegrityReport, TenderIntegrityState, TenderLifecyclePhase, TenderProductionInspection,
    TenderQuery, TenderQueryInvalidation, TenderQueryPage, TenderQueryResponse, TenderQueryStatus,
    TenderQueryTreatment, TenderQueryTreatmentProposal, TenderQueryTreatmentProposalInput,
    TenderQueryType, TenderRecordAuthority, TenderRecordAuthorityKind,
    TenderRecordAuthorityReference, TenderRecordBasisKind, TenderRecordContradiction,
    TenderRecordDecisionResult, TenderRecordEngineerDecisionKind, TenderRecordEvidence,
    TenderRecordExtractionResult, TenderRecordField, TenderRecordGenerationInstruction,
    TenderRecordInspection, TenderRecordKind, TenderRecordPage, TenderRecordReview,
    TenderRecordReviewOutcome, TenderRecordReviewResult, TenderRecordSourceRelationship,
    TenderRecordTrustClass, TenderRecordVersionReference, TenderRecoveryChoice,
    TenderRecoveryDecision, TenderRecoveryDecisionRecord, TenderRecoveryRecord,
    TenderRecoveryState, TenderRetentionDecisionCommand, TenderRetentionDecisionRecord,
    TenderRetentionState, TenderSummary, TrashedTenderDecisionCommand, TrashedTenderRecord,
    TrashedTenderState, WorkPlanApprovalRecord, WorkPlanCapabilityGap, WorkPlanDecision,
    WorkPlanProfileBinding, WorkPlanProposalInspection, WorkPlanRevisionAction, WorkPlanTask,
    WorkPlanWorkstream,
};
pub use update::{
    current_application_artifact_is_restorable, current_update_platform,
    run_update_rollback_helper, run_update_rollback_helper_from_args,
    run_update_rollback_helper_with_launcher, update_platform_from_target,
    verify_signed_update_artifact, verify_signed_update_candidate, ApplicationRollbackPlan,
    DecideUpdateCommand, InstallUpdateCommand, InstalledApplicationArtifactKind,
    InstalledApplicationArtifactSet, SignedArtifactIdentity, UpdateActionCommand, UpdateCandidate,
    UpdateCommandError, UpdateCompatibilityManifest, UpdateDecision, UpdateDecisionRecord,
    UpdateDiagnostic, UpdateImpact, UpdateOffer, UpdatePlatform, UpdateReleaseInformation,
    UpdateState, UpdateStatus,
};

use tauri::Manager;

mod tauri_commands {
    use std::sync::Mutex;

    use garde::Validate;
    use tauri_plugin_updater::UpdaterExt;

    use super::{
        ensure_quantix_setup as ensure_setup, ActivateTenderProductionCommand,
        AgentAccessRequestView, AgentRunActivity, AgentRunHistoryPage, AgentRunInspection,
        AgentRunRecoveryDecision, ApproveAgentAccessCommand, ApproveBasisOfEstimateCommand,
        ApproveCalculationRuleCommand, ApproveCommercialStrategyCommand,
        ApproveControlledBoqCalculationRunCommand, ApproveExternalRfiForIssueCommand,
        ApprovePackageFindingExceptionCommand, ApprovePricedCostBaselineCommand,
        ApprovePricingAdjustmentCommand, ApproveProductionFindingExceptionCommand,
        ApproveSubmissionReleaseCommand, ApproveTenderPriceCommand,
        AssembleCoordinatedBidBaselineCommand, AssembleSubmissionPackageCommand,
        BasisOfEstimateReviewResult, BasisOfEstimateVersion, BidDecisionApprovalHistoryPage,
        BidDecisionApprovalInvalidationResult, BidDecisionApprovalResult,
        BidDecisionPackageInspection, BidDecisionPackageRecordPage, BidDecisionPackageReviewResult,
        BidDecisionReturnReworkResult, BoqTableDesignation, CalculationRuleReviewResult,
        CalculationRuleVersion, CalculationScenarioVersion, CalculationWorkspaceInspection,
        ChangeAssessment, ChangeAssessmentPage, ChooseTenderPackageCommand, CommercialStrategy,
        ComplianceMatrixPage, ComposeTenderOfficeCommand, ConfirmSourceRelationshipCommand,
        ControlledBoqCalculationRun, CoordinatedBidBaseline, CoordinatedBidBaselinePage,
        CostEstimatorBasisResult, CostEstimatorCalculationResult, CreateBidDecisionPackageCommand,
        CreateCalculationScenarioCommand, CreateCommercialStrategyCommand,
        CreateExternalRfiDraftCommand, CreatePortableTenderArchiveCommand,
        CreatePricedCostBaselineCommand, CreatePricingAdjustmentCommand,
        CreatePricingScenarioCommand, CreateTenderBackupCommand, CreateTenderCommand,
        CreateTenderEngineerEntryCommand, CreateTenderQueryCommand,
        DecideBidDecisionPackageCommand, DecideChangeAssessmentCommand,
        DecideCoordinatedBidBaselineCommand, DecideTenderQueryTreatmentCommand,
        DecideTenderRecordCommand, DecideWorkPlanProposalCommand, DecisionCockpit, DeletionReceipt,
        DesignateBoqTableCommand, DocumentParseResult, DocumentRegister,
        EstimateWorkspaceInspection, EvidenceDocument, EvidenceSearchResult,
        ExportApprovedExternalRfiCommand, ExportReleaseCopyCommand, ExternalRfiDraft,
        ExternalRfiEligibleQueryPage, ExternalRfiExportRecord, ExternalRfiPage,
        ExternalRfiResponseCandidatePage, ExternalRfiReviewResult, FinalReviewInspection,
        GenerateSubmissionSectionsCommand, ImportPortableTenderArchiveCommand,
        ImportTenderPackageCommand, InspectAgentRunCommand, InspectAgentRunHistoryCommand,
        InspectBidDecisionApprovalHistoryCommand, InspectBidDecisionPackageRecordsCommand,
        InspectCalculationWorkspaceCommand, InspectChangeAssessmentsCommand,
        InspectComplianceMatrixCommand, InspectCoordinatedBidBaselinesCommand,
        InspectDecisionCockpitCommand, InspectEstimateWorkspaceCommand,
        InspectExternalRfiEligibleQueriesCommand, InspectExternalRfiResponseCandidatesCommand,
        InspectExternalRfisCommand, InspectPackageProductionCommand,
        InspectPricingWorkspaceCommand, InspectProductionTaskReviewCommand,
        InspectSubmissionArtifactContentCommand, InspectSubmissionPackageCommand,
        InspectSubmissionPackageItemContentCommand, InspectTenderQueriesCommand,
        InspectTenderRecordsCommand, InterpretExternalRfiResponseCommand, InterruptAgentRunCommand,
        InvalidateBidDecisionApprovalCommand, LiveQualificationRun, OpenTenderCommand,
        PackageProductionGeneration, ParseSourceArtifactCommand, PortableTenderArchiveRecord,
        PrepareTenderRecoveryCommand, PricedCostBaselineReviewResult, PricedCostBaselineVersion,
        PricingAdjustmentReviewResult, PricingAdjustmentVersion, PricingScenarioVersion,
        PricingWorkspaceInspection, PrivateQualificationRecord, ProductAcceptanceRecord,
        ProductAcceptanceRun, ProductionTaskReviewInspection, ProductionTaskRunResult,
        ProposeBoqCalculationRuleCommand, PublicReleaseGateRecord, QuantixHost,
        RecordPackageManualVerificationCommand, RegisterExternalRfiResponseCommand,
        RequestAgentAccessCommand, ResolveAgentAccessCommand,
        ResolveBidDecisionReturnReworkCommand, ResolveIndeterminateAgentRunCommand,
        ResolveTenderRecoveryCommand, ReviseExternalRfiDraftCommand, ReviseTenderCommand,
        ReviseTenderQueryCommand, ReviseWorkPlanProposalCommand, RunBasisOfEstimateReviewCommand,
        RunBidDecisionPackageReviewCommand, RunBootstrapAgentCommand,
        RunCalculationRuleReviewCommand, RunCostEstimatorBasisCommand,
        RunCostEstimatorCalculationCommand, RunExternalRfiReviewCommand,
        RunPackageValidationCommand, RunPricedCostBaselineReviewCommand,
        RunPricingAdjustmentReviewCommand, RunProductionTaskCommand,
        RunSubmissionSectionReviewCommand, RunTenderRecordExtractionCommand,
        RunTenderRecordReviewCommand, RuntimeReadiness, SearchEvidenceCommand,
        SelectPricingScenarioCommand, SetupOutcome, SubmissionArtifactContent,
        SubmissionItemContent, SubmissionPackageVersion, SubmissionReleaseInspection,
        SubmissionSectionReviewRunResult, TenderBackupRecord, TenderCatalogueEntry,
        TenderCommandError, TenderErrorCode, TenderIntegrityReport, TenderPackageImportResult,
        TenderPackageSourceKind, TenderProductionInspection, TenderQuery, TenderQueryPage,
        TenderRecordAuthority, TenderRecordDecisionResult, TenderRecordExtractionResult,
        TenderRecordPage, TenderRecordReviewResult, TenderRecoveryRecord,
        TenderRetentionDecisionCommand, TenderRetentionDecisionRecord, TenderSummary,
        TrashedTenderDecisionCommand, TrashedTenderRecord, WorkPlanProposalInspection,
    };
    use tauri_plugin_dialog::DialogExt;

    struct PendingUpdate {
        update_id: String,
        public_key: String,
        update: tauri_plugin_updater::Update,
    }

    pub(super) struct PendingSignedUpdate(Mutex<Option<PendingUpdate>>);

    impl PendingSignedUpdate {
        pub(super) fn new() -> Self {
            Self(Mutex::new(None))
        }
    }

    #[tauri::command]
    pub(super) async fn check_quantix_update<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
        pending: tauri::State<'_, PendingSignedUpdate>,
    ) -> Result<crate::UpdateStatus, crate::UpdateCommandError> {
        host.require_update_ready_setup()?;
        let release = crate::update::update_release_configuration()?;
        let update = app
            .updater_builder()
            .endpoints(vec![release.endpoint])
            .map_err(|_| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
            })?
            .pubkey(release.public_key.clone())
            .build()
            .map_err(|_| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
            })?
            .check()
            .await
            .map_err(|_| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
            })?;
        let Some(update) = update else {
            return host.inspect_update_status();
        };
        let (candidate, manifest_signature) = crate::update::candidate_from_tauri_update(&update)?;
        crate::verify_signed_update_candidate(
            &candidate,
            &manifest_signature,
            &release.public_key,
        )?;
        let status = host.present_update(candidate)?;
        let update_id = status
            .offer
            .as_ref()
            .map(|offer| offer.update_id.clone())
            .ok_or_else(|| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::InvalidManifest)
            })?;
        *pending.0.lock().map_err(|_| {
            crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
        })? = Some(PendingUpdate {
            update_id,
            public_key: release.public_key,
            update,
        });
        Ok(status)
    }

    #[tauri::command]
    pub(super) async fn validate_quantix_update_restart<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<crate::UpdateStatus, crate::UpdateCommandError> {
        let application_version = app.package_info().version.to_string();
        let status = host
            .validate_update_after_restart(&application_version)
            .await?;
        Ok(status)
    }

    #[tauri::command]
    pub(super) async fn decide_quantix_update(
        host: tauri::State<'_, QuantixHost>,
        command: crate::DecideUpdateCommand,
    ) -> Result<crate::UpdateStatus, crate::UpdateCommandError> {
        command.validate().map_err(|_| {
            crate::UpdateCommandError::new(crate::UpdateDiagnostic::InvalidManifest)
        })?;
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.decide_update(command.update_id, command.decision, command.rationale)
        })
        .await
        .map_err(|_| crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable))?
    }

    #[tauri::command]
    pub(super) async fn install_quantix_update(
        host: tauri::State<'_, QuantixHost>,
        pending: tauri::State<'_, PendingSignedUpdate>,
        command: crate::InstallUpdateCommand,
    ) -> Result<crate::UpdateStatus, crate::UpdateCommandError> {
        command.validate().map_err(|_| {
            crate::UpdateCommandError::new(crate::UpdateDiagnostic::InvalidManifest)
        })?;
        let pending_update = {
            let mut slot = pending.0.lock().map_err(|_| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
            })?;
            if slot
                .as_ref()
                .is_none_or(|candidate| candidate.update_id != command.update_id)
            {
                return Err(crate::UpdateCommandError::new(
                    crate::UpdateDiagnostic::UpdaterUnavailable,
                ));
            }
            slot.take().expect("pending update was checked above")
        };
        let installing = match host.authorize_update_installation(&command.update_id) {
            Ok(status) => status,
            Err(error) => {
                *pending.0.lock().map_err(|_| {
                    crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
                })? = Some(pending_update);
                return Err(error);
            }
        };
        if !host.quiesce_agent_provider_for_update().await {
            let _ = host.cancel_update_installation_authorization(&command.update_id);
            *pending.0.lock().map_err(|_| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
            })? = Some(pending_update);
            return Err(crate::UpdateCommandError::new(
                crate::UpdateDiagnostic::ActiveWork,
            ));
        }
        let current_version = installing
            .offer
            .as_ref()
            .map(|offer| offer.current_version.clone())
            .ok_or_else(|| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::InvalidManifest)
            })?;
        let artifact_set = match crate::update::installed_application_artifact_for_recovery() {
            Ok(artifact_set) => artifact_set,
            Err(error) => {
                let _ = host.cancel_update_installation_authorization(&command.update_id);
                *pending.0.lock().map_err(|_| {
                    crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
                })? = Some(pending_update);
                return Err(error);
            }
        };
        let expected_sha256 = installing
            .offer
            .as_ref()
            .map(|offer| offer.artifact.sha256.clone())
            .ok_or_else(|| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::InvalidManifest)
            })?;
        let bytes = match pending_update.update.download(|_, _| {}, || {}).await {
            Ok(bytes) => bytes,
            Err(error) => {
                let diagnostic = crate::update::updater_download_diagnostic(&error);
                if diagnostic == crate::UpdateDiagnostic::DownloadFailed {
                    let _ = host.cancel_update_installation_authorization(&command.update_id);
                    *pending.0.lock().map_err(|_| {
                        crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
                    })? = Some(pending_update);
                    return Err(crate::UpdateCommandError::new(diagnostic));
                }
                return host.record_update_rejection_before_install(&command.update_id, diagnostic);
            }
        };
        if let Err(error) = crate::verify_signed_update_artifact(
            &bytes,
            &pending_update.update.signature,
            &pending_update.public_key,
            &expected_sha256,
        ) {
            return host
                .record_update_rejection_before_install(&command.update_id, error.diagnostic);
        }
        if host
            .stage_application_recovery_point(&command.update_id, &current_version, &artifact_set)
            .is_err()
        {
            let _ = host.cancel_update_installation_authorization(&command.update_id);
            *pending.0.lock().map_err(|_| {
                crate::UpdateCommandError::new(crate::UpdateDiagnostic::UpdaterUnavailable)
            })? = Some(pending_update);
            return Err(crate::UpdateCommandError::new(
                crate::UpdateDiagnostic::InstallationFailed,
            ));
        }
        if let Err(error) = host.begin_update_installation_after_recovery(&command.update_id) {
            let _ = host.cancel_update_installation_authorization(&command.update_id);
            return Err(error);
        }
        if pending_update.update.install(bytes).is_err() {
            let repair = host.record_update_failure(
                &command.update_id,
                crate::UpdateDiagnostic::InstallationFailed,
            )?;
            let _ = host.restore_application_recovery_point(&command.update_id);
            return Ok(repair);
        }
        // The pinned Tauri updater exits the Windows process during installer handoff.
        // On platforms where install returns, persist the explicit renderer-driven restart gate.
        host.record_update_installed(&command.update_id)
    }

    #[tauri::command]
    pub(super) async fn restart_quantix_after_update<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
        command: crate::UpdateActionCommand,
    ) -> Result<crate::UpdateStatus, crate::UpdateCommandError> {
        command.validate().map_err(|_| {
            crate::UpdateCommandError::new(crate::UpdateDiagnostic::InvalidManifest)
        })?;
        let status = host.authorize_update_restart(&command.update_id)?;
        // request_restart delivers Tauri exit/restart events before replacing this process.
        Ok(super::perform_authorized_update_restart(status, || {
            app.request_restart()
        }))
    }

    #[tauri::command]
    pub(super) async fn retry_quantix_update_repair<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
        command: crate::UpdateActionCommand,
    ) -> Result<crate::UpdateStatus, crate::UpdateCommandError> {
        command.validate().map_err(|_| {
            crate::UpdateCommandError::new(crate::UpdateDiagnostic::InvalidManifest)
        })?;
        let status = host.inspect_update_status()?;
        if status
            .offer
            .as_ref()
            .is_none_or(|offer| offer.update_id != command.update_id)
        {
            return Err(crate::UpdateCommandError::new(
                crate::UpdateDiagnostic::InstallationFailed,
            ));
        }
        host.schedule_application_rollback(&command.update_id)?;
        app.exit(0);
        Ok(status)
    }

    #[tauri::command]
    pub(super) async fn ensure_quantix_setup(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<SetupOutcome, &'static str> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || ensure_setup(&host))
            .await
            .map_err(|_| "Quantix Setup stopped unexpectedly")
    }

    #[tauri::command]
    pub(super) async fn create_tender(
        host: tauri::State<'_, QuantixHost>,
        command: CreateTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn list_tenders(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<Vec<TenderCatalogueEntry>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.list_tenders())
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_product_acceptance_runs(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<Vec<ProductAcceptanceRun>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_product_acceptance_runs())
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn aggregate_product_acceptance(
        host: tauri::State<'_, QuantixHost>,
        source_revision: String,
    ) -> Result<ProductAcceptanceRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.aggregate_product_acceptance(&source_revision)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_live_qualification_runs(
        host: tauri::State<'_, QuantixHost>,
        release_candidate_sha256: String,
    ) -> Result<Vec<LiveQualificationRun>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_live_qualification_runs(&release_candidate_sha256)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn qualify_private_v0(
        host: tauri::State<'_, QuantixHost>,
        release_candidate_sha256: String,
    ) -> Result<PrivateQualificationRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.qualify_private_v0(&release_candidate_sha256)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_current_public_release_gate(
        host: tauri::State<'_, QuantixHost>,
        release_candidate_sha256: String,
    ) -> Result<Option<PublicReleaseGateRecord>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_current_public_release_gate(&release_candidate_sha256)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn open_tender(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.open_tender(&command.tender_id))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_tender_integrity(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<TenderIntegrityReport, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_tender_integrity(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn create_tender_backup(
        host: tauri::State<'_, QuantixHost>,
        command: CreateTenderBackupCommand,
    ) -> Result<TenderBackupRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_tender_backup(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_tender_backups(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Vec<TenderBackupRecord>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_tender_backups(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn create_portable_tender_archive(
        host: tauri::State<'_, QuantixHost>,
        command: CreatePortableTenderArchiveCommand,
    ) -> Result<PortableTenderArchiveRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_portable_tender_archive(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_portable_tender_archives(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Vec<PortableTenderArchiveRecord>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_portable_tender_archives(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn import_portable_tender_archive(
        host: tauri::State<'_, QuantixHost>,
        command: ImportPortableTenderArchiveCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.import_portable_tender_archive(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn choose_and_import_portable_tender_archive<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<Option<TenderSummary>, TenderCommandError> {
        let source = app
            .dialog()
            .file()
            .add_filter("Quantix Portable Tender Archive", &["qtarchive"])
            .blocking_pick_file()
            .and_then(|path| path.into_path().ok());
        let Some(source) = source else {
            return Ok(None);
        };
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.import_portable_tender_archive(ImportPortableTenderArchiveCommand {
                source_path: source.to_string_lossy().into_owned(),
            })
            .map(Some)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn archive_tender(
        host: tauri::State<'_, QuantixHost>,
        command: TenderRetentionDecisionCommand,
    ) -> Result<TenderRetentionDecisionRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.archive_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn restore_archived_tender(
        host: tauri::State<'_, QuantixHost>,
        command: TenderRetentionDecisionCommand,
    ) -> Result<TenderRetentionDecisionRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.restore_archived_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_tender_retention(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<crate::TenderRetentionState, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_tender_retention(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn trash_tender(
        host: tauri::State<'_, QuantixHost>,
        command: TenderRetentionDecisionCommand,
    ) -> Result<TrashedTenderRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.trash_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_trashed_tenders(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<Vec<TrashedTenderRecord>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_trashed_tenders())
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn restore_trashed_tender(
        host: tauri::State<'_, QuantixHost>,
        command: TrashedTenderDecisionCommand,
    ) -> Result<TrashedTenderRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.restore_trashed_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn purge_trashed_tender(
        host: tauri::State<'_, QuantixHost>,
        command: TrashedTenderDecisionCommand,
    ) -> Result<DeletionReceipt, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.purge_trashed_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_deletion_receipts(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<Vec<DeletionReceipt>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_deletion_receipts())
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn prepare_tender_recovery(
        host: tauri::State<'_, QuantixHost>,
        command: PrepareTenderRecoveryCommand,
    ) -> Result<TenderRecoveryRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.prepare_tender_recovery(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_tender_recoveries(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Vec<TenderRecoveryRecord>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_tender_recoveries(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn resolve_tender_recovery(
        host: tauri::State<'_, QuantixHost>,
        command: ResolveTenderRecoveryCommand,
    ) -> Result<TenderRecoveryRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.resolve_tender_recovery(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn revise_tender(
        host: tauri::State<'_, QuantixHost>,
        command: ReviseTenderCommand,
    ) -> Result<TenderSummary, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.revise_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn choose_and_import_tender_package<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
        command: ChooseTenderPackageCommand,
    ) -> Result<Option<TenderPackageImportResult>, TenderCommandError> {
        let source_kind = command.source_kind;
        let selected = tauri::async_runtime::spawn_blocking(move || {
            let picker = app.dialog().file();
            match source_kind {
                TenderPackageSourceKind::Directory => picker.blocking_pick_folder(),
                TenderPackageSourceKind::ZipArchive => picker
                    .add_filter("ZIP archive", &["zip"])
                    .blocking_pick_file(),
            }
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?;
        let Some(selected) = selected else {
            return Ok(None);
        };
        let source_path = selected.into_path().map_err(|_| TenderCommandError {
            code: TenderErrorCode::InvalidCommand,
        })?;
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.import_tender_package(ImportTenderPackageCommand {
                tender_id: command.tender_id,
                source_path: source_path.to_string_lossy().into_owned(),
            })
            .map(Some)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_document_register(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<DocumentRegister, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_document_register(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn confirm_source_relationship(
        host: tauri::State<'_, QuantixHost>,
        command: ConfirmSourceRelationshipCommand,
    ) -> Result<DocumentRegister, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.confirm_source_relationship(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn parse_source_artifact(
        host: tauri::State<'_, QuantixHost>,
        command: ParseSourceArtifactCommand,
    ) -> Result<DocumentParseResult, TenderCommandError> {
        host.inner().parse_source_artifact(command).await
    }

    #[tauri::command]
    pub(super) fn cancel_source_artifact_parse(
        host: tauri::State<'_, QuantixHost>,
        command: ParseSourceArtifactCommand,
    ) -> Result<bool, TenderCommandError> {
        host.inner().cancel_source_artifact_parse(command)
    }

    #[tauri::command]
    pub(super) async fn inspect_evidence(
        host: tauri::State<'_, QuantixHost>,
        command: ParseSourceArtifactCommand,
    ) -> Result<EvidenceDocument, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_evidence(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn search_evidence(
        host: tauri::State<'_, QuantixHost>,
        command: SearchEvidenceCommand,
    ) -> Result<EvidenceSearchResult, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.search_evidence(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_runtime_readiness(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<RuntimeReadiness, &'static str> {
        Ok(host.inner().inspect_runtime_readiness().await)
    }

    #[tauri::command]
    pub(super) async fn repair_runtime_readiness(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<RuntimeReadiness, &'static str> {
        Ok(host.inner().repair_runtime_readiness().await)
    }

    #[tauri::command]
    pub(super) fn cancel_runtime_preparation(host: tauri::State<'_, QuantixHost>) -> bool {
        host.inner().cancel_runtime_preparation()
    }

    #[tauri::command]
    pub(super) async fn run_bootstrap_agent(
        host: tauri::State<'_, QuantixHost>,
        command: RunBootstrapAgentCommand,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        host.inner().run_bootstrap_agent(command).await
    }

    #[tauri::command]
    pub(super) async fn run_tender_record_extraction(
        host: tauri::State<'_, QuantixHost>,
        command: RunTenderRecordExtractionCommand,
    ) -> Result<TenderRecordExtractionResult, TenderCommandError> {
        host.inner().run_tender_record_extraction(command).await
    }

    #[tauri::command]
    pub(super) async fn run_tender_record_review(
        host: tauri::State<'_, QuantixHost>,
        command: RunTenderRecordReviewCommand,
    ) -> Result<TenderRecordReviewResult, TenderCommandError> {
        host.inner().run_tender_record_review(command).await
    }

    #[tauri::command]
    pub(super) async fn inspect_tender_records(
        host: tauri::State<'_, QuantixHost>,
        command: InspectTenderRecordsCommand,
    ) -> Result<TenderRecordPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_tender_record_page(
                &command.tender_id,
                command.cursor.as_deref(),
                command.limit,
            )
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn create_tender_engineer_entry(
        host: tauri::State<'_, QuantixHost>,
        command: CreateTenderEngineerEntryCommand,
    ) -> Result<TenderRecordAuthority, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_tender_engineer_entry(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_tender_record_authorities(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Vec<TenderRecordAuthority>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_tender_record_authorities(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn decide_tender_record(
        host: tauri::State<'_, QuantixHost>,
        command: DecideTenderRecordCommand,
    ) -> Result<TenderRecordDecisionResult, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.decide_tender_record(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn create_tender_query(
        host: tauri::State<'_, QuantixHost>,
        command: CreateTenderQueryCommand,
    ) -> Result<TenderQuery, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_tender_query(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn revise_tender_query(
        host: tauri::State<'_, QuantixHost>,
        command: ReviseTenderQueryCommand,
    ) -> Result<TenderQuery, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.revise_tender_query(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn decide_tender_query_treatment(
        host: tauri::State<'_, QuantixHost>,
        command: DecideTenderQueryTreatmentCommand,
    ) -> Result<TenderQuery, TenderCommandError> {
        let host = host.inner().clone();
        let scheduler_host = host.clone();
        let tender_id = command.tender_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            host.decide_tender_query_treatment(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        scheduler_host.start_production_scheduler(tender_id);
        Ok(result)
    }

    #[tauri::command]
    pub(super) async fn inspect_tender_queries(
        host: tauri::State<'_, QuantixHost>,
        command: InspectTenderQueriesCommand,
    ) -> Result<TenderQueryPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_tender_queries(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn create_external_rfi_draft(
        host: tauri::State<'_, QuantixHost>,
        command: CreateExternalRfiDraftCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_external_rfi_draft(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn revise_external_rfi_draft(
        host: tauri::State<'_, QuantixHost>,
        command: ReviseExternalRfiDraftCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.revise_external_rfi_draft(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_external_rfis(
        host: tauri::State<'_, QuantixHost>,
        command: InspectExternalRfisCommand,
    ) -> Result<ExternalRfiPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_external_rfis(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_external_rfi_eligible_queries(
        host: tauri::State<'_, QuantixHost>,
        command: InspectExternalRfiEligibleQueriesCommand,
    ) -> Result<ExternalRfiEligibleQueryPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_external_rfi_eligible_queries(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_external_rfi_response_candidates(
        host: tauri::State<'_, QuantixHost>,
        command: InspectExternalRfiResponseCandidatesCommand,
    ) -> Result<ExternalRfiResponseCandidatePage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_external_rfi_response_candidates(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn run_external_rfi_review(
        host: tauri::State<'_, QuantixHost>,
        command: RunExternalRfiReviewCommand,
    ) -> Result<ExternalRfiReviewResult, TenderCommandError> {
        host.inner().run_external_rfi_review(command).await
    }

    #[tauri::command]
    pub(super) async fn approve_external_rfi_for_issue(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveExternalRfiForIssueCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_external_rfi_for_issue(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn export_approved_external_rfi(
        host: tauri::State<'_, QuantixHost>,
        command: ExportApprovedExternalRfiCommand,
    ) -> Result<ExternalRfiExportRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.export_approved_external_rfi(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn register_external_rfi_response(
        host: tauri::State<'_, QuantixHost>,
        command: RegisterExternalRfiResponseCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.register_external_rfi_response(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn interpret_external_rfi_response(
        host: tauri::State<'_, QuantixHost>,
        command: InterpretExternalRfiResponseCommand,
    ) -> Result<ExternalRfiDraft, TenderCommandError> {
        let host = host.inner().clone();
        let scheduler_host = host.clone();
        let tender_id = command.tender_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            host.interpret_external_rfi_response(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        scheduler_host.start_production_scheduler(tender_id);
        Ok(result)
    }

    #[tauri::command]
    pub(super) async fn propose_boq_calculation_rule(
        host: tauri::State<'_, QuantixHost>,
        command: ProposeBoqCalculationRuleCommand,
    ) -> Result<CalculationRuleVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.propose_boq_calculation_rule(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn run_calculation_rule_review(
        host: tauri::State<'_, QuantixHost>,
        command: RunCalculationRuleReviewCommand,
    ) -> Result<CalculationRuleReviewResult, TenderCommandError> {
        host.inner().run_calculation_rule_review(command).await
    }

    #[tauri::command]
    pub(super) async fn approve_calculation_rule(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveCalculationRuleCommand,
    ) -> Result<CalculationRuleVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_calculation_rule(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn create_calculation_scenario(
        host: tauri::State<'_, QuantixHost>,
        command: CreateCalculationScenarioCommand,
    ) -> Result<CalculationScenarioVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_calculation_scenario(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn run_cost_estimator_calculation(
        host: tauri::State<'_, QuantixHost>,
        command: RunCostEstimatorCalculationCommand,
    ) -> Result<CostEstimatorCalculationResult, TenderCommandError> {
        host.inner().run_cost_estimator_calculation(command).await
    }

    #[tauri::command]
    pub(super) async fn approve_controlled_boq_calculation_run(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveControlledBoqCalculationRunCommand,
    ) -> Result<ControlledBoqCalculationRun, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.approve_controlled_boq_calculation_run(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_calculation_workspace(
        host: tauri::State<'_, QuantixHost>,
        command: InspectCalculationWorkspaceCommand,
    ) -> Result<CalculationWorkspaceInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_calculation_workspace(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn designate_boq_table(
        host: tauri::State<'_, QuantixHost>,
        command: DesignateBoqTableCommand,
    ) -> Result<BoqTableDesignation, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.designate_boq_table(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn run_cost_estimator_basis(
        host: tauri::State<'_, QuantixHost>,
        command: RunCostEstimatorBasisCommand,
    ) -> Result<CostEstimatorBasisResult, TenderCommandError> {
        host.inner().run_cost_estimator_basis(command).await
    }

    #[tauri::command]
    pub(super) async fn run_basis_of_estimate_review(
        host: tauri::State<'_, QuantixHost>,
        command: RunBasisOfEstimateReviewCommand,
    ) -> Result<BasisOfEstimateReviewResult, TenderCommandError> {
        host.inner().run_basis_of_estimate_review(command).await
    }

    #[tauri::command]
    pub(super) async fn approve_basis_of_estimate(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveBasisOfEstimateCommand,
    ) -> Result<BasisOfEstimateVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_basis_of_estimate(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_estimate_workspace(
        host: tauri::State<'_, QuantixHost>,
        command: InspectEstimateWorkspaceCommand,
    ) -> Result<EstimateWorkspaceInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_estimate_workspace(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn create_priced_cost_baseline(
        host: tauri::State<'_, QuantixHost>,
        command: CreatePricedCostBaselineCommand,
    ) -> Result<PricedCostBaselineVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_priced_cost_baseline(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn run_priced_cost_baseline_review(
        host: tauri::State<'_, QuantixHost>,
        command: RunPricedCostBaselineReviewCommand,
    ) -> Result<PricedCostBaselineReviewResult, TenderCommandError> {
        host.inner().run_priced_cost_baseline_review(command).await
    }

    #[tauri::command]
    pub(super) async fn approve_priced_cost_baseline(
        host: tauri::State<'_, QuantixHost>,
        command: ApprovePricedCostBaselineCommand,
    ) -> Result<PricedCostBaselineVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_priced_cost_baseline(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn create_pricing_adjustment(
        host: tauri::State<'_, QuantixHost>,
        command: CreatePricingAdjustmentCommand,
    ) -> Result<PricingAdjustmentVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_pricing_adjustment(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn run_pricing_adjustment_review(
        host: tauri::State<'_, QuantixHost>,
        command: RunPricingAdjustmentReviewCommand,
    ) -> Result<PricingAdjustmentReviewResult, TenderCommandError> {
        host.inner().run_pricing_adjustment_review(command).await
    }

    #[tauri::command]
    pub(super) async fn approve_pricing_adjustment(
        host: tauri::State<'_, QuantixHost>,
        command: ApprovePricingAdjustmentCommand,
    ) -> Result<PricingAdjustmentVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_pricing_adjustment(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn create_commercial_strategy(
        host: tauri::State<'_, QuantixHost>,
        command: CreateCommercialStrategyCommand,
    ) -> Result<CommercialStrategy, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_commercial_strategy(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn approve_commercial_strategy(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveCommercialStrategyCommand,
    ) -> Result<CommercialStrategy, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_commercial_strategy(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn create_pricing_scenario(
        host: tauri::State<'_, QuantixHost>,
        command: CreatePricingScenarioCommand,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_pricing_scenario(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn select_pricing_scenario(
        host: tauri::State<'_, QuantixHost>,
        command: SelectPricingScenarioCommand,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.select_pricing_scenario(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn approve_tender_price(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveTenderPriceCommand,
    ) -> Result<PricingScenarioVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_tender_price(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_pricing_workspace(
        host: tauri::State<'_, QuantixHost>,
        command: InspectPricingWorkspaceCommand,
    ) -> Result<PricingWorkspaceInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_pricing_workspace(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn assemble_coordinated_bid_baseline(
        host: tauri::State<'_, QuantixHost>,
        command: AssembleCoordinatedBidBaselineCommand,
    ) -> Result<CoordinatedBidBaseline, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.assemble_coordinated_bid_baseline(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn decide_coordinated_bid_baseline(
        host: tauri::State<'_, QuantixHost>,
        command: DecideCoordinatedBidBaselineCommand,
    ) -> Result<CoordinatedBidBaseline, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.decide_coordinated_bid_baseline(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_coordinated_bid_baselines(
        host: tauri::State<'_, QuantixHost>,
        command: InspectCoordinatedBidBaselinesCommand,
    ) -> Result<CoordinatedBidBaselinePage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_coordinated_bid_baselines(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn generate_submission_sections(
        host: tauri::State<'_, QuantixHost>,
        command: GenerateSubmissionSectionsCommand,
    ) -> Result<PackageProductionGeneration, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.generate_submission_sections(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_package_production(
        host: tauri::State<'_, QuantixHost>,
        command: InspectPackageProductionCommand,
    ) -> Result<Option<PackageProductionGeneration>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_package_production(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_submission_artifact_content(
        host: tauri::State<'_, QuantixHost>,
        command: InspectSubmissionArtifactContentCommand,
    ) -> Result<SubmissionArtifactContent, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_submission_artifact_content(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn assemble_submission_package(
        host: tauri::State<'_, QuantixHost>,
        command: AssembleSubmissionPackageCommand,
    ) -> Result<SubmissionPackageVersion, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.assemble_submission_package(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_current_submission_package(
        host: tauri::State<'_, QuantixHost>,
        command: InspectSubmissionPackageCommand,
    ) -> Result<Option<SubmissionPackageVersion>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_current_submission_package(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_submission_package_item_content(
        host: tauri::State<'_, QuantixHost>,
        command: InspectSubmissionPackageItemContentCommand,
    ) -> Result<SubmissionItemContent, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_submission_package_item_content(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn run_package_validation(
        host: tauri::State<'_, QuantixHost>,
        command: RunPackageValidationCommand,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.run_package_validation(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_final_review(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Option<FinalReviewInspection>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_final_review(&command.tender_id))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn record_package_manual_verification(
        host: tauri::State<'_, QuantixHost>,
        command: RecordPackageManualVerificationCommand,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.record_package_manual_verification(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn run_submission_section_review(
        host: tauri::State<'_, QuantixHost>,
        command: RunSubmissionSectionReviewCommand,
    ) -> Result<SubmissionSectionReviewRunResult, TenderCommandError> {
        host.inner().run_submission_section_review(command).await
    }

    #[tauri::command]
    pub(super) async fn approve_package_finding_exception(
        host: tauri::State<'_, QuantixHost>,
        command: ApprovePackageFindingExceptionCommand,
    ) -> Result<FinalReviewInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.approve_package_finding_exception(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn approve_submission_release(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveSubmissionReleaseCommand,
    ) -> Result<SubmissionReleaseInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_submission_release(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_submission_release(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Option<SubmissionReleaseInspection>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_submission_release(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn export_release_copy(
        host: tauri::State<'_, QuantixHost>,
        command: ExportReleaseCopyCommand,
    ) -> Result<SubmissionReleaseInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.export_release_copy(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_change_assessments(
        host: tauri::State<'_, QuantixHost>,
        command: InspectChangeAssessmentsCommand,
    ) -> Result<ChangeAssessmentPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_change_assessments(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn decide_change_assessment(
        host: tauri::State<'_, QuantixHost>,
        command: DecideChangeAssessmentCommand,
    ) -> Result<ChangeAssessment, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.decide_change_assessment(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn create_bid_decision_package(
        host: tauri::State<'_, QuantixHost>,
        command: CreateBidDecisionPackageCommand,
    ) -> Result<BidDecisionPackageInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.create_bid_decision_package(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_current_bid_decision_package(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Option<BidDecisionPackageInspection>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_current_bid_decision_package(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn compose_tender_office(
        host: tauri::State<'_, QuantixHost>,
        command: ComposeTenderOfficeCommand,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.compose_tender_office(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_current_work_plan(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Option<WorkPlanProposalInspection>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_current_work_plan(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn revise_work_plan_proposal(
        host: tauri::State<'_, QuantixHost>,
        command: ReviseWorkPlanProposalCommand,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.revise_work_plan_proposal(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn decide_work_plan_proposal(
        host: tauri::State<'_, QuantixHost>,
        command: DecideWorkPlanProposalCommand,
    ) -> Result<WorkPlanProposalInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.decide_work_plan_proposal(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn activate_tender_production(
        host: tauri::State<'_, QuantixHost>,
        command: ActivateTenderProductionCommand,
    ) -> Result<TenderProductionInspection, TenderCommandError> {
        let host = host.inner().clone();
        let scheduler_host = host.clone();
        let tender_id = command.tender_id.clone();
        let inspection =
            tauri::async_runtime::spawn_blocking(move || host.activate_tender_production(command))
                .await
                .map_err(|_| TenderCommandError {
                    code: TenderErrorCode::StoreUnavailable,
                })??;
        scheduler_host.start_production_scheduler(tender_id);
        Ok(inspection)
    }

    #[tauri::command]
    pub(super) async fn inspect_tender_production(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<Option<TenderProductionInspection>, TenderCommandError> {
        let host = host.inner().clone();
        let scheduler_host = host.clone();
        let tender_id = command.tender_id.clone();
        let inspection = tauri::async_runtime::spawn_blocking(move || {
            host.inspect_tender_production(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        if inspection
            .as_ref()
            .is_some_and(|production| production.active)
        {
            scheduler_host.start_production_scheduler(tender_id);
        }
        Ok(inspection)
    }

    #[tauri::command]
    pub(super) async fn run_production_task(
        host: tauri::State<'_, QuantixHost>,
        command: RunProductionTaskCommand,
    ) -> Result<ProductionTaskRunResult, TenderCommandError> {
        host.inner().run_production_task(command).await
    }

    #[tauri::command]
    pub(super) async fn inspect_production_task_review(
        host: tauri::State<'_, QuantixHost>,
        command: InspectProductionTaskReviewCommand,
    ) -> Result<ProductionTaskReviewInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_production_task_review(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn approve_production_finding_exception(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveProductionFindingExceptionCommand,
    ) -> Result<ProductionTaskReviewInspection, TenderCommandError> {
        let host = host.inner().clone();
        let scheduler_host = host.clone();
        let tender_id = command.tender_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            host.approve_production_finding_exception(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        scheduler_host.start_production_scheduler(tender_id);
        Ok(result)
    }

    #[tauri::command]
    pub(super) async fn decide_bid_decision_package(
        host: tauri::State<'_, QuantixHost>,
        command: DecideBidDecisionPackageCommand,
    ) -> Result<BidDecisionApprovalResult, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.decide_bid_decision_package(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_bid_decision_approval_history(
        host: tauri::State<'_, QuantixHost>,
        command: InspectBidDecisionApprovalHistoryCommand,
    ) -> Result<BidDecisionApprovalHistoryPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_bid_decision_approval_history(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn resolve_bid_decision_return_rework(
        host: tauri::State<'_, QuantixHost>,
        command: ResolveBidDecisionReturnReworkCommand,
    ) -> Result<BidDecisionReturnReworkResult, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.resolve_bid_decision_return_rework(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn invalidate_bid_decision_approval(
        host: tauri::State<'_, QuantixHost>,
        command: InvalidateBidDecisionApprovalCommand,
    ) -> Result<BidDecisionApprovalInvalidationResult, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.invalidate_bid_decision_approval(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_compliance_matrix(
        host: tauri::State<'_, QuantixHost>,
        command: InspectComplianceMatrixCommand,
    ) -> Result<ComplianceMatrixPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_compliance_matrix_page(
                &command.tender_id,
                &command.package_id,
                command.version,
                command.after_ordinal,
                command.limit,
            )
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_bid_decision_package_records(
        host: tauri::State<'_, QuantixHost>,
        command: InspectBidDecisionPackageRecordsCommand,
    ) -> Result<BidDecisionPackageRecordPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_bid_decision_package_record_page(
                &command.tender_id,
                &command.package_id,
                command.version,
                command.category,
                command.after_ordinal,
                command.limit,
            )
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn run_bid_decision_package_review(
        host: tauri::State<'_, QuantixHost>,
        command: RunBidDecisionPackageReviewCommand,
    ) -> Result<BidDecisionPackageReviewResult, TenderCommandError> {
        host.inner().run_bid_decision_package_review(command).await
    }

    #[tauri::command]
    pub(super) async fn inspect_agent_run_history(
        host: tauri::State<'_, QuantixHost>,
        command: InspectAgentRunHistoryCommand,
    ) -> Result<AgentRunHistoryPage, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_agent_run_history(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_agent_run(
        host: tauri::State<'_, QuantixHost>,
        command: InspectAgentRunCommand,
    ) -> Result<AgentRunInspection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_agent_run(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn inspect_agent_run_activity(
        host: tauri::State<'_, QuantixHost>,
        command: OpenTenderCommand,
    ) -> Result<AgentRunActivity, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_agent_run_activity(&command.tender_id)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })?
    }

    #[tauri::command]
    pub(super) async fn inspect_decision_cockpit(
        host: tauri::State<'_, QuantixHost>,
        command: InspectDecisionCockpitCommand,
    ) -> Result<DecisionCockpit, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_decision_cockpit(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn request_agent_access(
        host: tauri::State<'_, QuantixHost>,
        command: RequestAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.request_agent_access(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn approve_agent_access(
        host: tauri::State<'_, QuantixHost>,
        command: ApproveAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.approve_agent_access(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn resolve_agent_access(
        host: tauri::State<'_, QuantixHost>,
        command: ResolveAgentAccessCommand,
    ) -> Result<AgentAccessRequestView, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.resolve_agent_access(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn resolve_indeterminate_agent_run(
        host: tauri::State<'_, QuantixHost>,
        command: ResolveIndeterminateAgentRunCommand,
    ) -> Result<AgentRunRecoveryDecision, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.resolve_indeterminate_agent_run(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) fn interrupt_agent_run(
        host: tauri::State<'_, QuantixHost>,
        command: InterruptAgentRunCommand,
    ) -> Result<bool, TenderCommandError> {
        host.inner().interrupt_agent_run(command)
    }
}

pub fn perform_authorized_update_restart(
    status: UpdateStatus,
    request_restart: impl FnOnce(),
) -> UpdateStatus {
    request_restart();
    status
}

pub fn configure_tauri_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .manage(tauri_commands::PendingSignedUpdate::new())
        .invoke_handler(tauri::generate_handler![
            tauri_commands::ensure_quantix_setup,
            tauri_commands::inspect_product_acceptance_runs,
            tauri_commands::aggregate_product_acceptance,
            tauri_commands::inspect_live_qualification_runs,
            tauri_commands::qualify_private_v0,
            tauri_commands::inspect_current_public_release_gate,
            tauri_commands::check_quantix_update,
            tauri_commands::validate_quantix_update_restart,
            tauri_commands::decide_quantix_update,
            tauri_commands::install_quantix_update,
            tauri_commands::restart_quantix_after_update,
            tauri_commands::retry_quantix_update_repair,
            tauri_commands::create_tender,
            tauri_commands::list_tenders,
            tauri_commands::open_tender,
            tauri_commands::inspect_tender_integrity,
            tauri_commands::create_tender_backup,
            tauri_commands::inspect_tender_backups,
            tauri_commands::create_portable_tender_archive,
            tauri_commands::inspect_portable_tender_archives,
            tauri_commands::import_portable_tender_archive,
            tauri_commands::choose_and_import_portable_tender_archive,
            tauri_commands::archive_tender,
            tauri_commands::restore_archived_tender,
            tauri_commands::inspect_tender_retention,
            tauri_commands::trash_tender,
            tauri_commands::inspect_trashed_tenders,
            tauri_commands::restore_trashed_tender,
            tauri_commands::purge_trashed_tender,
            tauri_commands::inspect_deletion_receipts,
            tauri_commands::prepare_tender_recovery,
            tauri_commands::inspect_tender_recoveries,
            tauri_commands::resolve_tender_recovery,
            tauri_commands::revise_tender,
            tauri_commands::choose_and_import_tender_package,
            tauri_commands::inspect_document_register,
            tauri_commands::confirm_source_relationship,
            tauri_commands::parse_source_artifact,
            tauri_commands::cancel_source_artifact_parse,
            tauri_commands::inspect_evidence,
            tauri_commands::search_evidence,
            tauri_commands::inspect_runtime_readiness,
            tauri_commands::repair_runtime_readiness,
            tauri_commands::cancel_runtime_preparation,
            tauri_commands::run_bootstrap_agent,
            tauri_commands::run_tender_record_extraction,
            tauri_commands::run_tender_record_review,
            tauri_commands::inspect_tender_records,
            tauri_commands::create_tender_engineer_entry,
            tauri_commands::inspect_tender_record_authorities,
            tauri_commands::decide_tender_record,
            tauri_commands::create_tender_query,
            tauri_commands::revise_tender_query,
            tauri_commands::decide_tender_query_treatment,
            tauri_commands::inspect_tender_queries,
            tauri_commands::create_external_rfi_draft,
            tauri_commands::revise_external_rfi_draft,
            tauri_commands::inspect_external_rfis,
            tauri_commands::inspect_external_rfi_eligible_queries,
            tauri_commands::inspect_external_rfi_response_candidates,
            tauri_commands::run_external_rfi_review,
            tauri_commands::approve_external_rfi_for_issue,
            tauri_commands::export_approved_external_rfi,
            tauri_commands::register_external_rfi_response,
            tauri_commands::interpret_external_rfi_response,
            tauri_commands::propose_boq_calculation_rule,
            tauri_commands::run_calculation_rule_review,
            tauri_commands::approve_calculation_rule,
            tauri_commands::create_calculation_scenario,
            tauri_commands::run_cost_estimator_calculation,
            tauri_commands::approve_controlled_boq_calculation_run,
            tauri_commands::inspect_calculation_workspace,
            tauri_commands::designate_boq_table,
            tauri_commands::run_cost_estimator_basis,
            tauri_commands::run_basis_of_estimate_review,
            tauri_commands::approve_basis_of_estimate,
            tauri_commands::inspect_estimate_workspace,
            tauri_commands::create_priced_cost_baseline,
            tauri_commands::run_priced_cost_baseline_review,
            tauri_commands::approve_priced_cost_baseline,
            tauri_commands::create_pricing_adjustment,
            tauri_commands::run_pricing_adjustment_review,
            tauri_commands::approve_pricing_adjustment,
            tauri_commands::create_commercial_strategy,
            tauri_commands::approve_commercial_strategy,
            tauri_commands::create_pricing_scenario,
            tauri_commands::select_pricing_scenario,
            tauri_commands::approve_tender_price,
            tauri_commands::inspect_pricing_workspace,
            tauri_commands::assemble_coordinated_bid_baseline,
            tauri_commands::decide_coordinated_bid_baseline,
            tauri_commands::inspect_coordinated_bid_baselines,
            tauri_commands::generate_submission_sections,
            tauri_commands::inspect_package_production,
            tauri_commands::inspect_submission_artifact_content,
            tauri_commands::assemble_submission_package,
            tauri_commands::inspect_current_submission_package,
            tauri_commands::inspect_submission_package_item_content,
            tauri_commands::run_package_validation,
            tauri_commands::inspect_final_review,
            tauri_commands::record_package_manual_verification,
            tauri_commands::run_submission_section_review,
            tauri_commands::approve_package_finding_exception,
            tauri_commands::approve_submission_release,
            tauri_commands::inspect_submission_release,
            tauri_commands::export_release_copy,
            tauri_commands::inspect_change_assessments,
            tauri_commands::decide_change_assessment,
            tauri_commands::create_bid_decision_package,
            tauri_commands::inspect_current_bid_decision_package,
            tauri_commands::compose_tender_office,
            tauri_commands::inspect_current_work_plan,
            tauri_commands::revise_work_plan_proposal,
            tauri_commands::decide_work_plan_proposal,
            tauri_commands::activate_tender_production,
            tauri_commands::inspect_tender_production,
            tauri_commands::run_production_task,
            tauri_commands::inspect_production_task_review,
            tauri_commands::approve_production_finding_exception,
            tauri_commands::decide_bid_decision_package,
            tauri_commands::inspect_bid_decision_approval_history,
            tauri_commands::resolve_bid_decision_return_rework,
            tauri_commands::invalidate_bid_decision_approval,
            tauri_commands::inspect_compliance_matrix,
            tauri_commands::inspect_bid_decision_package_records,
            tauri_commands::run_bid_decision_package_review,
            tauri_commands::inspect_agent_run_history,
            tauri_commands::inspect_agent_run,
            tauri_commands::inspect_agent_run_activity,
            tauri_commands::inspect_decision_cockpit,
            tauri_commands::request_agent_access,
            tauri_commands::approve_agent_access,
            tauri_commands::resolve_agent_access,
            tauri_commands::resolve_indeterminate_agent_run,
            tauri_commands::interrupt_agent_run
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_tauri_builder(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let application_home = app.path().home_dir()?.join(".quantix");
            let resource_directory = app.path().resource_dir()?;
            app.manage(QuantixHost::new(application_home, resource_directory));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
