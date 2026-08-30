#![recursion_limit = "256"]
mod acceptance;
pub mod ai;

mod agent_runtime;
mod application_settings;
mod chatgpt_login;
mod diagnostics;
mod doctor;
mod document_parsing;
mod embedding;
mod host;
mod managed_runtime;
mod process_supervisor;
mod release_gate;
mod runtime_readiness;
mod setup;
mod tender_intake;
mod tender_store;
mod update;

pub use acceptance::{
    acceptance_evaluation_policy_sha256, acceptance_fixture_sha256, acceptance_oracle_sha256,
    measure_ocr_runtime_sha256, print_candidate_acceptance_probe,
    print_candidate_acceptance_rehearsal, AcceptanceArtifactHash, AcceptanceCheckResult,
    AcceptanceStageTiming, LiveQualificationEnvironment, LiveQualificationMetrics,
    LiveQualificationRun, PrivateQualificationRecord, ProductAcceptanceOutcome,
    ProductAcceptanceRecord, ProductAcceptanceRun, RecordLiveQualificationRunCommand,
    RunDeterministicAcceptanceCommand,
};
pub use agent_runtime::worker_lane::{
    WorkerApproval, WorkerDriverError, WorkerFailureCategory, WorkerOperation, WorkerOutcome,
    WorkerRunRequest, WorkerToolDescriptor, WorkerUsage,
};
pub use agent_runtime::{
    approve_one_run_access, AccessApproval, AccessRequest, AgentAccessRequestStatus,
    AgentAccessRequestView, AgentAccessResolution, AgentProfileStatus, AgentProfileVersionView,
    AgentRepairFeedback, AgentResourceBudget, AgentRunActivity, AgentRunHistoryItem,
    AgentRunHistoryPage, AgentRunInspection, AgentRunPermissions, AgentRunRecoveryDecision,
    AgentRunRecoveryDisposition, AgentRunState, AgentRunSummary, AgentRunWorkspaceManifest,
    AgentTaskInputReference, ApproveAgentAccessCommand, BootstrapAuthority, BootstrapRole,
    BootstrapTeamMember, CodexReadiness, DataClassification, DataViewManifest,
    InspectAgentRunCommand, InspectAgentRunHistoryCommand, InterruptAgentRunCommand,
    OneRunAccessGrant, OutputValidationIssue, PermissionCeiling, PermissionDenialReason,
    PermissionGrant, ProposedAgentResult, ProviderEvent, ProviderEventKind, ProviderFailure,
    ProviderFailureCategory, ProviderRateLimit, ProviderRateLimitState, ProviderRateLimitWindow,
    ProviderUsage, RejectedAgentOutput, RequestAgentAccessCommand, ResolveAgentAccessCommand,
    ResolveIndeterminateAgentRunCommand, RunBootstrapAgentCommand, TenderTaskView,
    ThreadExposureSet, ToolIdempotency, ToolSideEffectClass, TypedToolDefinition, TypedToolQuota,
    VerificationStatus,
};
pub use application_settings::{
    AiExecutionApproval, AiExecutionSelection, AiProviderKind, AppearancePreference,
    ApplicationDiagnostics, ApplicationSettingsView, ApplicationStorageFacts,
    ConfirmAiExecutionSelectionCommand, GeneralApplicationPreferences,
    InspectTenderAiExecutionCommand, ProviderConnectionStatus, ProviderConnectionView,
    ProviderModelOption, ProviderReasoningOption, ProviderReasoningSelection,
    TenderAiExecutionBinding, TenderAiSelectionReadiness, UpdateAiExecutionSelectionCommand,
    UpdateGeneralApplicationPreferencesCommand, UpdateTenderAiExecutionSelectionCommand,
};
pub use chatgpt_login::{
    ChatGptConnectionState, ChatGptConnectionStatus, ChatGptLoginPhase,
    StartChatGptDeviceLoginResult, StartChatGptLoginError, StartChatGptLoginResult,
    StartChatGptLoginStatus,
};
pub(crate) use diagnostics::RecordDiagnosticFact;
pub use diagnostics::{
    DeepDiagnosticsSession, DiagnosticComponent, DiagnosticCorrelation, DiagnosticEvent,
    DiagnosticHealth, DiagnosticScope, DiagnosticSeverity, DiagnosticSupportBundleResult,
    DiagnosticTimelineEvent, DiagnosticTimelineFilter, DiagnosticTimelinePage,
    DiagnosticsDeepState, DiagnosticsDeepStatus, DiagnosticsStatus, DiagnosticsStatusState,
    ExportDiagnosticsSupportBundleCommand, InspectDiagnosticTimelineCommand,
    InspectDiagnosticsStatusCommand, OpenDiagnosticLogsCommand, OpenDiagnosticLogsResult,
    RecordRendererDiagnosticCommand, RendererDiagnosticKind, StartTenderDeepDiagnosticsCommand,
    StopTenderDeepDiagnosticsCommand, DIAGNOSTIC_REDACTION_POLICY_VERSION,
};
pub use doctor::{
    compose_quantix_doctor_report, validate_quantix_doctor_repair, DoctorTenderInput,
    InspectQuantixDoctorCommand, QuantixDoctorArea, QuantixDoctorFinding,
    QuantixDoctorRepairAction, QuantixDoctorRepairCommand, QuantixDoctorRepairTarget,
    QuantixDoctorReport, QuantixDoctorSeverity,
};
pub use document_parsing::{
    DocumentParseResult, EvidenceBoundingBox, EvidenceDocument, EvidenceLanguage, EvidenceLocation,
    EvidenceLocationKind, EvidenceRegion, EvidenceSearchHit, EvidenceSearchResult,
    EvidenceSemanticSearchHit, EvidenceSemanticSearchResult, ParseExceptionCode,
    ParseSourceArtifactCommand, ParseState, SearchEvidenceCommand, SearchEvidenceSemanticCommand,
    TextDirection,
};
pub use host::QuantixHost;
pub use managed_runtime::{
    worker_python_path, ManagedCodexRuntimeState, ManagedCodexRuntimeStatus, ManagedRuntimeError,
    ManagedWorkerRuntimeState, ManagedWorkerRuntimeStatus,
};
pub use release_gate::{
    release_candidate_manifest_sha256, ChatGptProductionAssuranceEvidence,
    EvaluatePublicReleaseGateCommand, IntegrationTermsDecision, LicenseDistributionReview,
    NativePlatformQualificationEvidence, NativePlatformQualificationRecord,
    PublicReleaseGateOutcome, PublicReleaseGateRecord, RecordNativePlatformQualificationCommand,
    TechnicalRiskAcceptance,
};
pub use runtime_readiness::{
    RuntimeLayout, RuntimePreparationActivity, RuntimePreparationActivityStatus,
    RuntimePreparationProgress, RuntimePreparationStatus, RuntimePreparationStep, RuntimeReadiness,
    RuntimeReadinessIssue, RuntimeReadinessState,
};
pub use setup::{
    ensure_quantix_setup, SetupIssue, SetupOutcome, SetupPlatform, SetupState, StoragePermissions,
    MINIMUM_SETUP_FREE_SPACE_BYTES,
};
pub use tender_intake::{
    CancelPackageIntakeCommand, ChooseTenderPackageCommand, ConfirmSourceRelationshipCommand,
    DocumentRegister, DocumentRegisterEntry, ImportTenderPackageCommand, IntakeExceptionCode,
    PackageIntakeOperationKind, PackageIntakeProgress, PackageIntakeStage, RegistrationState,
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
    DeletionReceipt, DesignateBoqTableCommand, ErasedTenderCopyClass,
    EstimateAggregateCalculationInput, EstimateAggregateCalculationRun, EstimateAllowance,
    EstimateMaterialAssumption, EstimateQueryObservation, EstimateQueryReference,
    EstimateQuotation, EstimateQuotationKind, EstimateWorkspaceInspection, ExchangeRateType,
    ExportApprovedExternalRfiCommand, ExportReleaseCopyCommand, ExternalRfiApproval,
    ExternalRfiDraft, ExternalRfiEligibleQuery, ExternalRfiEligibleQueryPage,
    ExternalRfiExportRecord, ExternalRfiFindingSeverity, ExternalRfiPage,
    ExternalRfiQueryReference, ExternalRfiQuestion, ExternalRfiRecipient,
    ExternalRfiResponseCandidatePage, ExternalRfiResponseInterpretation, ExternalRfiResponseLink,
    ExternalRfiReview, ExternalRfiReviewFinding, ExternalRfiReviewOutcome, ExternalRfiReviewResult,
    FinalReviewAssignment, FinalReviewDecisionEvidence, FinalReviewDecisionEvidenceCategory,
    FinalReviewInspection, FinalReviewPlan, FinalReviewReviewer, GenerateSubmissionSectionsCommand,
    GenerationAuthoringMode, GenerationRequirement, GenerationRequirementAvailability,
    GenerationRequirementKind, GenerationRequirementRecordReference,
    ImportPortableTenderArchiveCommand, InspectBidDecisionApprovalHistoryCommand,
    InspectBidDecisionPackageRecordsCommand, InspectCalculationWorkspaceCommand,
    InspectChangeAssessmentsCommand, InspectComplianceMatrixCommand,
    InspectCoordinatedBidBaselinesCommand, InspectDecisionCockpitCommand,
    InspectEstimateWorkspaceCommand, InspectExternalRfiEligibleQueriesCommand,
    InspectExternalRfiResponseCandidatesCommand, InspectExternalRfisCommand,
    InspectManagerWorkspaceCommand, InspectPackageProductionCommand,
    InspectPricingWorkspaceCommand, InspectProductionTaskReviewCommand,
    InspectSubmissionArtifactContentCommand, InspectSubmissionPackageCommand,
    InspectSubmissionPackageItemContentCommand, InspectTenderQueriesCommand,
    InspectTenderRecordsCommand, InterpretExternalRfiResponseCommand,
    InvalidateBidDecisionApprovalCommand, MajorFindingPolicy, ManagerCapabilityDemandInput,
    ManagerConversation, ManagerIntakeStage, ManagerIntakeStatus, ManagerIntakeStatusKind,
    ManagerWorkspaceProjection, ManagerWorkspaceTender, ManagerWorkspaceTenderState,
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
    ProposeBoqCalculationRuleCommand, ProviderCleanupStatus, ProviderReferenceDiscoveryState,
    PurgeRecoveryRequiredTenderCommand, PurgeTrashedTenderCommand,
    RebindManagerIntakeProviderCommand, RecordEngineerWorkspaceMessageCommand,
    RecordPackageManualVerificationCommand, RegisterExternalRfiResponseCommand,
    RegisterTenderContentCommand, ReleaseCopyExport, ReleaseCopyItem, ReleaseReadinessBlocker,
    ReleaseReadinessBlockerCode, ReleaseReadinessCategorySummary, ReleaseReadinessReport,
    ResolveBidDecisionReturnReworkCommand, ResolveTenderRecoveryCommand, ResourceImplication,
    RetryManagerIntakeCommand, ReviewFindingSeverity, ReviseExternalRfiDraftCommand,
    ReviseTenderCommand, ReviseTenderQueryCommand, ReviseWorkPlanProposalCommand,
    RunBasisOfEstimateReviewCommand, RunBidDecisionPackageReviewCommand,
    RunCalculationRuleReviewCommand, RunCostEstimatorBasisCommand,
    RunCostEstimatorCalculationCommand, RunExternalRfiReviewCommand, RunPackageValidationCommand,
    RunPricedCostBaselineReviewCommand, RunPricingAdjustmentReviewCommand,
    RunProductionTaskCommand, RunSubmissionSectionReviewCommand, RunTenderRecordExtractionCommand,
    RunTenderRecordReviewCommand, SearchManagerWorkspaceCommand,
    SelectManagerWorkspaceTenderCommand, SelectPricingScenarioCommand, StartManagerTenderCommand,
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
    TenderDeletionSourceState, TenderErrorCode, TenderEvidenceReference, TenderInspection,
    TenderIntegrityIssue, TenderIntegrityReport, TenderIntegrityState, TenderLifecyclePhase,
    TenderOfficeMessage, TenderOfficeMessageAuthor, TenderOfficeMessageKind,
    TenderProductionInspection, TenderQuery, TenderQueryInvalidation, TenderQueryPage,
    TenderQueryResponse, TenderQueryStatus, TenderQueryTreatment, TenderQueryTreatmentProposal,
    TenderQueryTreatmentProposalInput, TenderQueryType, TenderRecordAuthority,
    TenderRecordAuthorityKind, TenderRecordAuthorityReference, TenderRecordBasisKind,
    TenderRecordContradiction, TenderRecordDecisionResult, TenderRecordEngineerDecisionKind,
    TenderRecordEvidence, TenderRecordExtractionResult, TenderRecordField,
    TenderRecordGenerationInstruction, TenderRecordInspection, TenderRecordKind, TenderRecordPage,
    TenderRecordReview, TenderRecordReviewOutcome, TenderRecordReviewResult,
    TenderRecordSourceRelationship, TenderRecordTrustClass, TenderRecordVersionReference,
    TenderRecoveryChoice, TenderRecoveryDecision, TenderRecoveryDecisionRecord,
    TenderRecoveryRecord, TenderRecoveryState, TenderRetentionDecisionCommand,
    TenderRetentionDecisionRecord, TenderRetentionState, TenderSummary,
    TrashRecoveryRequiredTenderCommand, TrashedTenderDecisionCommand, TrashedTenderRecord,
    TrashedTenderState, WorkPlanApprovalRecord, WorkPlanCapabilityGap, WorkPlanDecision,
    WorkPlanProfileBinding, WorkPlanProposalInspection, WorkPlanRevisionAction, WorkPlanTask,
    WorkPlanWorkstream, WorkspaceActionKind, WorkspaceAgentReference, WorkspaceAgentRunReference,
    WorkspaceCapabilityReadiness, WorkspaceCapabilityReadinessState, WorkspaceCurrentAction,
    WorkspaceDoctorBlockerArea, WorkspaceDoctorBlockerSummary, WorkspaceFilesSummary,
    WorkspaceMessageReference, WorkspaceMessageReferenceKind, WorkspaceOutputReference,
    WorkspaceSearchGroup, WorkspaceSearchHit, WorkspaceSearchProjection, WorkspaceSearchResultKind,
    WorkspaceTaskRow, WorkspaceTaskState, WorkspaceTeamSummary, WorkspaceTenderDocument,
    WorkspaceWorkSummary,
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

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, WindowEvent};

#[derive(Clone, Default)]
pub struct StartupSplashState {
    ready: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
    watchdog_registered_at: Arc<Mutex<Option<Instant>>>,
}

const SPLASH_WATCHDOG_HARD_DEADLINE: Duration = Duration::from_millis(7_750);

impl StartupSplashState {
    fn register_watchdog_once(&self) -> bool {
        let mut registered_at = self
            .watchdog_registered_at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if registered_at.is_some() {
            return false;
        }
        *registered_at = Some(Instant::now());
        true
    }

    fn watchdog_deadline(&self) -> Option<Instant> {
        self.watchdog_registered_at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .map(|registered_at| registered_at + SPLASH_WATCHDOG_HARD_DEADLINE)
    }
}

fn remaining_until(deadline: Instant, now: Instant) -> Duration {
    deadline.saturating_duration_since(now)
}

fn claim_startup_splash_completion(state: &StartupSplashState) -> Result<bool, String> {
    if !state.ready.load(Ordering::Acquire) {
        return Err("startup is not ready".to_string());
    }
    Ok(!state.finished.swap(true, Ordering::AcqRel))
}

fn claim_startup_splash_watchdog_completion(state: &StartupSplashState) -> bool {
    !state.finished.swap(true, Ordering::AcqRel)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupSplashPreferences {
    pub reduced_motion: bool,
}

fn finish_splash_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(splash) = app.get_webview_window("splashscreen") {
        let _ = splash.close();
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.unminimize();
        let _ = main.set_focus();
    }
}

fn start_splash_watchdog<R: Runtime>(app: AppHandle<R>, state: StartupSplashState) {
    if !state.register_watchdog_once() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let deadline = state
            .watchdog_deadline()
            .unwrap_or_else(|| Instant::now() + SPLASH_WATCHDOG_HARD_DEADLINE);
        let delay = remaining_until(deadline, Instant::now());
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        if claim_startup_splash_watchdog_completion(&state) {
            finish_splash_window(&app);
        }
    });
}

#[cfg(test)]
mod startup_splash_tests {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use super::{
        claim_startup_splash_completion, claim_startup_splash_watchdog_completion, remaining_until,
        StartupSplashState, SPLASH_WATCHDOG_HARD_DEADLINE,
    };

    #[test]
    fn registered_watchdog_has_the_full_window_and_a_hard_cap() {
        let registered_at = Instant::now();
        let deadline = registered_at + SPLASH_WATCHDOG_HARD_DEADLINE;

        assert_eq!(
            remaining_until(deadline, registered_at),
            SPLASH_WATCHDOG_HARD_DEADLINE
        );
        assert_eq!(
            remaining_until(deadline, deadline + Duration::from_millis(1)),
            Duration::ZERO
        );
    }

    #[test]
    fn watchdog_registration_is_idempotent_and_has_an_absolute_deadline() {
        let state = StartupSplashState::default();
        assert!(state.register_watchdog_once());
        let deadline = state.watchdog_deadline().expect("watchdog deadline");
        assert!(!state.register_watchdog_once());
        assert_eq!(remaining_until(deadline, deadline), Duration::ZERO);
    }

    #[test]
    fn startup_splash_cannot_finish_before_readiness_and_finishes_once_afterward() {
        let state = StartupSplashState::default();

        assert_eq!(
            claim_startup_splash_completion(&state),
            Err("startup is not ready".to_string())
        );
        state.ready.store(true, Ordering::Release);
        assert_eq!(claim_startup_splash_completion(&state), Ok(true));
        assert_eq!(claim_startup_splash_completion(&state), Ok(false));
    }

    #[test]
    fn absolute_watchdog_can_recover_before_renderer_readiness_and_only_once() {
        let state = StartupSplashState::default();

        assert!(claim_startup_splash_watchdog_completion(&state));
        assert!(!claim_startup_splash_watchdog_completion(&state));
    }
}

mod tauri_commands {
    use std::sync::{atomic::Ordering, Mutex};
    use std::time::Instant;

    use garde::Validate;
    use tauri::Emitter;
    use tauri_plugin_opener::OpenerExt;
    use tauri_plugin_updater::UpdaterExt;

    use super::{
        claim_startup_splash_completion, ensure_quantix_setup as ensure_setup,
        finish_splash_window, start_splash_watchdog, ActivateTenderProductionCommand,
        AgentAccessRequestView, AgentRunActivity, AgentRunHistoryPage, AgentRunInspection,
        AgentRunRecoveryDecision, ApplicationSettingsView, ApproveAgentAccessCommand,
        ApproveBasisOfEstimateCommand, ApproveCalculationRuleCommand,
        ApproveCommercialStrategyCommand, ApproveControlledBoqCalculationRunCommand,
        ApproveExternalRfiForIssueCommand, ApprovePackageFindingExceptionCommand,
        ApprovePricedCostBaselineCommand, ApprovePricingAdjustmentCommand,
        ApproveProductionFindingExceptionCommand, ApproveSubmissionReleaseCommand,
        ApproveTenderPriceCommand, AssembleCoordinatedBidBaselineCommand,
        AssembleSubmissionPackageCommand, BasisOfEstimateReviewResult, BasisOfEstimateVersion,
        BidDecisionApprovalHistoryPage, BidDecisionApprovalInvalidationResult,
        BidDecisionApprovalResult, BidDecisionPackageInspection, BidDecisionPackageRecordPage,
        BidDecisionPackageReviewResult, BidDecisionReturnReworkResult, BoqTableDesignation,
        CalculationRuleReviewResult, CalculationRuleVersion, CalculationScenarioVersion,
        CalculationWorkspaceInspection, CancelPackageIntakeCommand, ChangeAssessment,
        ChangeAssessmentPage, ChooseTenderPackageCommand, CommercialStrategy, ComplianceMatrixPage,
        ComposeTenderOfficeCommand, ConfirmAiExecutionSelectionCommand,
        ConfirmSourceRelationshipCommand, ControlledBoqCalculationRun, CoordinatedBidBaseline,
        CoordinatedBidBaselinePage, CostEstimatorBasisResult, CostEstimatorCalculationResult,
        CreateBidDecisionPackageCommand, CreateCalculationScenarioCommand,
        CreateCommercialStrategyCommand, CreateExternalRfiDraftCommand,
        CreatePortableTenderArchiveCommand, CreatePricedCostBaselineCommand,
        CreatePricingAdjustmentCommand, CreatePricingScenarioCommand, CreateTenderBackupCommand,
        CreateTenderCommand, CreateTenderEngineerEntryCommand, CreateTenderQueryCommand,
        DecideBidDecisionPackageCommand, DecideChangeAssessmentCommand,
        DecideCoordinatedBidBaselineCommand, DecideTenderQueryTreatmentCommand,
        DecideTenderRecordCommand, DecideWorkPlanProposalCommand, DecisionCockpit, DeletionReceipt,
        DesignateBoqTableCommand, DoctorTenderInput, DocumentParseResult, DocumentRegister,
        EstimateWorkspaceInspection, EvidenceDocument, EvidenceSearchResult,
        EvidenceSemanticSearchResult, ExportApprovedExternalRfiCommand, ExportReleaseCopyCommand,
        ExternalRfiDraft, ExternalRfiEligibleQueryPage, ExternalRfiExportRecord, ExternalRfiPage,
        ExternalRfiResponseCandidatePage, ExternalRfiReviewResult, FinalReviewInspection,
        GenerateSubmissionSectionsCommand, ImportPortableTenderArchiveCommand,
        ImportTenderPackageCommand, InspectAgentRunCommand, InspectAgentRunHistoryCommand,
        InspectBidDecisionApprovalHistoryCommand, InspectBidDecisionPackageRecordsCommand,
        InspectCalculationWorkspaceCommand, InspectChangeAssessmentsCommand,
        InspectComplianceMatrixCommand, InspectCoordinatedBidBaselinesCommand,
        InspectDecisionCockpitCommand, InspectEstimateWorkspaceCommand,
        InspectExternalRfiEligibleQueriesCommand, InspectExternalRfiResponseCandidatesCommand,
        InspectExternalRfisCommand, InspectManagerWorkspaceCommand,
        InspectPackageProductionCommand, InspectPricingWorkspaceCommand,
        InspectProductionTaskReviewCommand, InspectQuantixDoctorCommand,
        InspectSubmissionArtifactContentCommand, InspectSubmissionPackageCommand,
        InspectSubmissionPackageItemContentCommand, InspectTenderAiExecutionCommand,
        InspectTenderQueriesCommand, InspectTenderRecordsCommand,
        InterpretExternalRfiResponseCommand, InterruptAgentRunCommand,
        InvalidateBidDecisionApprovalCommand, LiveQualificationRun, ManagerWorkspaceProjection,
        OpenTenderCommand, PackageIntakeOperationKind, PackageIntakeProgress,
        PackageProductionGeneration, ParseSourceArtifactCommand, PortableTenderArchiveRecord,
        PrepareTenderRecoveryCommand, PricedCostBaselineReviewResult, PricedCostBaselineVersion,
        PricingAdjustmentReviewResult, PricingAdjustmentVersion, PricingScenarioVersion,
        PricingWorkspaceInspection, PrivateQualificationRecord, ProductAcceptanceRecord,
        ProductAcceptanceRun, ProductionTaskReviewInspection, ProductionTaskRunResult,
        ProposeBoqCalculationRuleCommand, PublicReleaseGateRecord,
        PurgeRecoveryRequiredTenderCommand, PurgeTrashedTenderCommand, QuantixDoctorRepairAction,
        QuantixDoctorRepairCommand, QuantixDoctorReport, QuantixHost,
        RebindManagerIntakeProviderCommand, RecordEngineerWorkspaceMessageCommand,
        RecordPackageManualVerificationCommand, RegisterExternalRfiResponseCommand,
        RequestAgentAccessCommand, ResolveAgentAccessCommand,
        ResolveBidDecisionReturnReworkCommand, ResolveIndeterminateAgentRunCommand,
        ResolveTenderRecoveryCommand, RetryManagerIntakeCommand, ReviseExternalRfiDraftCommand,
        ReviseTenderCommand, ReviseTenderQueryCommand, ReviseWorkPlanProposalCommand,
        RunBasisOfEstimateReviewCommand, RunBidDecisionPackageReviewCommand,
        RunBootstrapAgentCommand, RunCalculationRuleReviewCommand, RunCostEstimatorBasisCommand,
        RunCostEstimatorCalculationCommand, RunExternalRfiReviewCommand,
        RunPackageValidationCommand, RunPricedCostBaselineReviewCommand,
        RunPricingAdjustmentReviewCommand, RunProductionTaskCommand,
        RunSubmissionSectionReviewCommand, RunTenderRecordExtractionCommand,
        RunTenderRecordReviewCommand, RuntimePreparationProgress, RuntimeReadiness,
        SearchEvidenceCommand, SearchEvidenceSemanticCommand, SearchManagerWorkspaceCommand,
        SelectManagerWorkspaceTenderCommand, SelectPricingScenarioCommand, SetupOutcome,
        SetupState, StartChatGptDeviceLoginResult, StartChatGptLoginError, StartChatGptLoginResult,
        StartManagerTenderCommand, StartupReconciliationReport, StartupSplashPreferences,
        StartupSplashState, SubmissionArtifactContent, SubmissionItemContent,
        SubmissionPackageVersion, SubmissionReleaseInspection, SubmissionSectionReviewRunResult,
        TenderBackupRecord, TenderCatalogueEntry, TenderCommandError, TenderErrorCode,
        TenderIntegrityReport, TenderPackageImportResult, TenderPackageSourceKind,
        TenderProductionInspection, TenderQuery, TenderQueryPage, TenderRecordAuthority,
        TenderRecordDecisionResult, TenderRecordExtractionResult, TenderRecordPage,
        TenderRecordReviewResult, TenderRecoveryRecord, TenderRetentionDecisionCommand,
        TenderRetentionDecisionRecord, TenderSummary, TrashRecoveryRequiredTenderCommand,
        TrashedTenderDecisionCommand, TrashedTenderRecord, UpdateAiExecutionSelectionCommand,
        UpdateGeneralApplicationPreferencesCommand, UpdateTenderAiExecutionSelectionCommand,
        WorkPlanProposalInspection, WorkspaceSearchProjection,
    };
    use tauri_plugin_dialog::DialogExt;

    struct PendingUpdate {
        update_id: String,
        public_key: String,
        update: tauri_plugin_updater::Update,
    }

    fn retry_provider_cleanup_in_background(host: QuantixHost) {
        tauri::async_runtime::spawn(async move {
            let _ = host.retry_pending_provider_cleanup().await;
        });
    }

    pub(super) struct PendingSignedUpdate(Mutex<Option<PendingUpdate>>);

    impl PendingSignedUpdate {
        pub(super) fn new() -> Self {
            Self(Mutex::new(None))
        }
    }

    #[tauri::command]
    pub(super) fn report_startup_splash_preferences<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        preferences: StartupSplashPreferences,
    ) -> Result<(), String> {
        app.emit_to("splashscreen", "quantix-startup-preferences", preferences)
            .map_err(|error| error.to_string())
    }

    #[tauri::command]
    pub(super) fn notify_startup_display_ready<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        state: tauri::State<'_, StartupSplashState>,
    ) -> Result<(), String> {
        let state = state.inner().clone();
        if state.ready.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let _ = app.emit_to("splashscreen", "quantix-startup-ready", ());
        start_splash_watchdog(app, state);
        Ok(())
    }

    #[tauri::command]
    pub(super) fn inspect_startup_display_ready(
        state: tauri::State<'_, StartupSplashState>,
    ) -> bool {
        state.ready.load(Ordering::Acquire)
    }

    #[tauri::command]
    pub(super) fn finish_startup_splash<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        state: tauri::State<'_, StartupSplashState>,
    ) -> Result<(), String> {
        if !claim_startup_splash_completion(state.inner())? {
            return Ok(());
        }
        finish_splash_window(&app);
        Ok(())
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
        let setup_host = host.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || ensure_setup(&setup_host))
            .await
            .map_err(|_| "Quantix Setup stopped unexpectedly")?;
        if matches!(outcome.state, SetupState::Ready | SetupState::Warning) {
            retry_provider_cleanup_in_background(host);
        }
        Ok(outcome)
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
    pub(super) async fn refresh_application_settings(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        let host = host.inner().clone();
        let view = host.refresh_application_settings().await?;
        if host.document_tools_are_verified() {
            retry_provider_cleanup_in_background(host);
        }
        Ok(view)
    }

    #[tauri::command]
    pub(super) fn inspect_application_settings(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        host.inner().inspect_application_settings()
    }

    #[tauri::command]
    pub(super) async fn update_ai_execution_selection(
        host: tauri::State<'_, QuantixHost>,
        command: UpdateAiExecutionSelectionCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        host.inner().update_ai_execution_selection(command).await
    }

    #[tauri::command]
    pub(super) fn inspect_tender_ai_execution(
        host: tauri::State<'_, QuantixHost>,
        command: InspectTenderAiExecutionCommand,
    ) -> Result<crate::TenderAiExecutionBinding, TenderCommandError> {
        host.inner().inspect_tender_ai_execution(command)
    }

    #[tauri::command]
    pub(super) fn update_tender_ai_execution(
        host: tauri::State<'_, QuantixHost>,
        command: UpdateTenderAiExecutionSelectionCommand,
    ) -> Result<crate::TenderAiExecutionBinding, TenderCommandError> {
        host.inner().update_tender_ai_execution(command)
    }

    async fn compose_doctor_report(
        host: &QuantixHost,
        command: &InspectQuantixDoctorCommand,
    ) -> Result<QuantixDoctorReport, TenderCommandError> {
        let setup = host.ensure_setup();
        let runtime = host.inspect_runtime_readiness().await;
        let settings = host.inspect_application_settings()?;
        let update = host.inspect_update_status();
        let diagnostics = host.diagnostics().inspect_health();
        let tender = command
            .tender_id
            .as_ref()
            .map(|tender_id| {
                let integrity = host.inspect_tender_integrity(tender_id)?;
                let ai_execution =
                    host.inspect_tender_ai_execution(InspectTenderAiExecutionCommand {
                        tender_id: tender_id.clone(),
                    })?;
                Ok::<_, TenderCommandError>(DoctorTenderInput {
                    tender_id: tender_id.clone(),
                    integrity,
                    ai_execution,
                })
            })
            .transpose()?;
        Ok(crate::compose_quantix_doctor_report(
            &setup,
            &runtime,
            &settings,
            update.as_ref().map_err(|error| error.diagnostic),
            &diagnostics,
            tender.as_ref(),
        ))
    }

    #[tauri::command]
    pub(super) async fn inspect_quantix_doctor(
        host: tauri::State<'_, QuantixHost>,
        command: InspectQuantixDoctorCommand,
    ) -> Result<QuantixDoctorReport, TenderCommandError> {
        compose_doctor_report(host.inner(), &command).await
    }

    #[tauri::command]
    pub(super) async fn repair_quantix_doctor(
        host: tauri::State<'_, QuantixHost>,
        command: QuantixDoctorRepairCommand,
    ) -> Result<QuantixDoctorReport, TenderCommandError> {
        let inspect = InspectQuantixDoctorCommand {
            tender_id: command.tender_id.clone(),
        };
        let current = compose_doctor_report(host.inner(), &inspect).await?;
        if !crate::validate_quantix_doctor_repair(&current, &command) {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        match command.action {
            QuantixDoctorRepairAction::PrepareDocumentTools
            | QuantixDoctorRepairAction::RetryDocumentTools => {
                host.inner().repair_runtime_readiness().await;
            }
            QuantixDoctorRepairAction::RefreshAiProvider => {
                host.inner().refresh_application_settings().await?;
            }
            QuantixDoctorRepairAction::RetryDiagnostics => {
                host.inner()
                    .diagnostics()
                    .retry()
                    .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
            }
            QuantixDoctorRepairAction::InspectTenderIntegrity
            | QuantixDoctorRepairAction::RebindTenderAiSelection
            | QuantixDoctorRepairAction::RetryUpdateInspection => {}
        }
        compose_doctor_report(host.inner(), &inspect).await
    }

    fn diagnostic_target(
        scope: crate::DiagnosticScope,
        tender_id: Option<String>,
    ) -> Result<Option<String>, TenderCommandError> {
        match (scope, tender_id) {
            (crate::DiagnosticScope::Application, None) => Ok(None),
            (crate::DiagnosticScope::Tender, Some(tender_id)) => {
                crate::tender_store::TenderId::parse(&tender_id)?;
                Ok(Some(tender_id))
            }
            _ => Err(TenderCommandError::new(TenderErrorCode::InvalidCommand)),
        }
    }

    #[tauri::command]
    pub(super) async fn inspect_diagnostics_status(
        host: tauri::State<'_, QuantixHost>,
        command: crate::InspectDiagnosticsStatusCommand,
    ) -> Result<crate::DiagnosticsStatus, TenderCommandError> {
        let tender_id = diagnostic_target(command.scope, command.tender_id)?;
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.diagnostics()
                .inspect_status(command.scope, tender_id.as_deref())
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    #[tauri::command]
    pub(super) async fn inspect_diagnostic_timeline(
        host: tauri::State<'_, QuantixHost>,
        command: crate::InspectDiagnosticTimelineCommand,
    ) -> Result<crate::DiagnosticTimelinePage, TenderCommandError> {
        let tender_id = diagnostic_target(command.scope, command.tender_id)?;
        let component = match command.component.as_deref() {
            Some(component) => Some(
                crate::DiagnosticComponent::parse(component)
                    .ok_or_else(|| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
            ),
            None => None,
        };
        let limit = command.limit.unwrap_or(50).clamp(1, 200);
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            Ok(host.diagnostics().inspect_timeline(
                crate::DiagnosticTimelineFilter {
                    scope: Some(command.scope),
                    severity: command.severity,
                    component,
                    tender_id,
                },
                command.cursor.as_deref(),
                limit,
            ))
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    #[tauri::command]
    pub(super) fn start_tender_deep_diagnostics(
        host: tauri::State<'_, QuantixHost>,
        command: crate::StartTenderDeepDiagnosticsCommand,
    ) -> Result<crate::DeepDiagnosticsSession, TenderCommandError> {
        if command.policy_revision != crate::DIAGNOSTIC_REDACTION_POLICY_VERSION {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        host.inner().inspect_tender_integrity(&command.tender_id)?;
        let session = host
            .inner()
            .diagnostics()
            .start_deep(&command.tender_id)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        let mut fact = crate::RecordDiagnosticFact::new(
            crate::DiagnosticSeverity::Info,
            crate::DiagnosticComponent::Diagnostics,
            "deep_diagnostics_started",
            "Redacted deep diagnostics started",
        );
        fact.outcome = Some("started".into());
        fact.initiated_by = Some("engineer_user".into());
        fact.success = Some(true);
        host.inner()
            .diagnostics()
            .record_tender(&command.tender_id, fact);
        Ok(session)
    }

    #[tauri::command]
    pub(super) fn stop_tender_deep_diagnostics(
        host: tauri::State<'_, QuantixHost>,
        command: crate::StopTenderDeepDiagnosticsCommand,
    ) -> Result<bool, TenderCommandError> {
        if !host
            .inner()
            .diagnostics()
            .stop_deep(&command.tender_id, &command.session_id)
        {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let mut fact = crate::RecordDiagnosticFact::new(
            crate::DiagnosticSeverity::Info,
            crate::DiagnosticComponent::Diagnostics,
            "deep_diagnostics_stopped",
            "Redacted deep diagnostics stopped",
        );
        fact.outcome = Some("stopped".into());
        fact.initiated_by = Some("engineer_user".into());
        fact.success = Some(true);
        host.inner()
            .diagnostics()
            .record_tender(&command.tender_id, fact);
        Ok(true)
    }

    #[tauri::command]
    pub(super) fn open_diagnostic_logs<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
        command: crate::OpenDiagnosticLogsCommand,
    ) -> Result<crate::OpenDiagnosticLogsResult, TenderCommandError> {
        let tender_id = diagnostic_target(command.scope, command.tender_id)?;
        let directory = match tender_id.as_deref() {
            Some(tender_id) => host
                .inner()
                .diagnostics()
                .tender_directory(tender_id)
                .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?,
            None => host.inner().diagnostics().application_directory(),
        };
        std::fs::create_dir_all(&directory)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        app.opener()
            .open_path(directory.to_string_lossy(), None::<&str>)
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?;
        Ok(crate::OpenDiagnosticLogsResult {
            directory: directory.to_string_lossy().into_owned(),
        })
    }

    #[tauri::command]
    pub(super) async fn export_diagnostics_support_bundle(
        host: tauri::State<'_, QuantixHost>,
        command: crate::ExportDiagnosticsSupportBundleCommand,
    ) -> Result<crate::DiagnosticSupportBundleResult, TenderCommandError> {
        if command.policy_revision != crate::DIAGNOSTIC_REDACTION_POLICY_VERSION {
            return Err(TenderCommandError::new(TenderErrorCode::InvalidCommand));
        }
        let tender_id = diagnostic_target(command.scope, command.tender_id)?;
        let doctor_report = compose_doctor_report(
            host.inner(),
            &InspectQuantixDoctorCommand {
                tender_id: tender_id.clone(),
            },
        )
        .await?;
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.diagnostics()
                .export_support_bundle(
                    tender_id.as_deref(),
                    7,
                    command.include_deep,
                    Some(&doctor_report),
                )
                .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))
        })
        .await
        .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    #[tauri::command]
    pub(super) fn record_renderer_diagnostic(
        host: tauri::State<'_, QuantixHost>,
        command: crate::RecordRendererDiagnosticCommand,
    ) -> bool {
        if !host.inner().allow_renderer_diagnostic() {
            return false;
        }
        let (severity, event_name, summary, outcome) = match command.kind {
            crate::RendererDiagnosticKind::SurfaceUnavailable => (
                crate::DiagnosticSeverity::Error,
                "renderer_surface_unavailable",
                "A renderer surface became unavailable",
                "failed",
            ),
            crate::RendererDiagnosticKind::InteractionFailed => (
                crate::DiagnosticSeverity::Warning,
                "renderer_interaction_failed",
                "A renderer interaction failed",
                "failed",
            ),
            crate::RendererDiagnosticKind::StateRecovered => (
                crate::DiagnosticSeverity::Info,
                "renderer_state_recovered",
                "The renderer recovered its application state",
                "recovered",
            ),
        };
        let mut fact = crate::RecordDiagnosticFact::new(
            severity,
            crate::DiagnosticComponent::Renderer,
            event_name,
            summary,
        );
        fact.outcome = Some(outcome.into());
        host.inner().diagnostics().record_application(fact)
    }

    #[tauri::command]
    pub(super) async fn confirm_ai_execution_selection(
        host: tauri::State<'_, QuantixHost>,
        command: ConfirmAiExecutionSelectionCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        host.inner().confirm_ai_execution_selection(command).await
    }

    #[tauri::command]
    pub(super) fn clear_ai_execution_selection(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        host.inner().clear_ai_execution_selection()
    }

    #[tauri::command]
    pub(super) async fn update_general_application_preferences(
        host: tauri::State<'_, QuantixHost>,
        command: UpdateGeneralApplicationPreferencesCommand,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        host.inner()
            .update_general_application_preferences(command)
            .await
    }

    #[tauri::command]
    pub(super) async fn start_chatgpt_login(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<StartChatGptLoginResult, StartChatGptLoginError> {
        host.inner().start_chatgpt_login().await
    }

    #[tauri::command]
    pub(super) async fn start_chatgpt_device_login(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<StartChatGptDeviceLoginResult, StartChatGptLoginError> {
        host.inner().start_chatgpt_device_login().await
    }

    #[tauri::command]
    pub(super) async fn open_chatgpt_device_login_page(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<(), TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.open_chatgpt_device_login_page())
            .await
            .map_err(|_| TenderCommandError::new(TenderErrorCode::StoreUnavailable))?
    }

    #[tauri::command]
    pub(super) async fn cancel_chatgpt_login(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<(), TenderCommandError> {
        host.inner().cancel_chatgpt_login().await;
        Ok(())
    }

    #[tauri::command]
    pub(super) async fn disconnect_chatgpt(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<ApplicationSettingsView, TenderCommandError> {
        host.inner().disconnect_chatgpt().await
    }

    #[tauri::command]
    pub(super) async fn inspect_manager_workspace(
        host: tauri::State<'_, QuantixHost>,
        command: InspectManagerWorkspaceCommand,
    ) -> Result<ManagerWorkspaceProjection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.inspect_manager_workspace(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn search_manager_workspace(
        host: tauri::State<'_, QuantixHost>,
        command: SearchManagerWorkspaceCommand,
    ) -> Result<WorkspaceSearchProjection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.search_manager_workspace(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) fn inspect_package_intake_progress(
        host: tauri::State<'_, QuantixHost>,
    ) -> Option<PackageIntakeProgress> {
        host.inner().inspect_package_intake_progress()
    }

    #[tauri::command]
    pub(super) fn cancel_package_intake(
        host: tauri::State<'_, QuantixHost>,
        command: CancelPackageIntakeCommand,
    ) -> bool {
        host.inner().cancel_package_intake(command)
    }

    #[tauri::command]
    pub(super) async fn start_manager_tender<R: tauri::Runtime>(
        app: tauri::AppHandle<R>,
        host: tauri::State<'_, QuantixHost>,
        command: StartManagerTenderCommand,
    ) -> Result<Option<ManagerWorkspaceProjection>, TenderCommandError> {
        let source_kind = command.source_kind;
        let local_only = command.local_only;
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
        let source_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Tender Package")
            .to_owned();
        let control = host.begin_package_intake(
            PackageIntakeOperationKind::StartTender,
            source_kind,
            source_name,
        )?;
        let operation_id = control.snapshot().operation_id.clone();
        let intake_started = Instant::now();
        let start_host = host.clone();
        let projection = tauri::async_runtime::spawn_blocking(move || {
            start_host.start_manager_tender_from_package_with_control(
                &source_path,
                Some(&control),
                local_only,
            )
        })
        .await;
        let elapsed_ms = u64::try_from(intake_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match &projection {
            Ok(Ok(Some(projection))) => {
                if let Some(tender) = &projection.selected_tender {
                    host.record_tender_diagnostic(
                        &tender.tender_id,
                        crate::DiagnosticSeverity::Info,
                        crate::DiagnosticComponent::Package,
                        "package_intake_completed",
                        "The Tender package was published",
                        Some(operation_id.clone()),
                        Some(elapsed_ms),
                        Some("completed"),
                        None,
                    );
                }
            }
            Ok(Err(error)) => host.record_application_diagnostic(
                crate::DiagnosticSeverity::Error,
                crate::DiagnosticComponent::Package,
                "package_intake_failed",
                "The governed package intake operation failed",
                Some(operation_id.clone()),
                Some(elapsed_ms),
                Some("failed"),
                Some(format!("{:?}", error.code)),
            ),
            Err(_) => host.record_application_diagnostic(
                crate::DiagnosticSeverity::Error,
                crate::DiagnosticComponent::Package,
                "package_intake_stopped",
                "The package intake worker stopped unexpectedly",
                Some(operation_id.clone()),
                Some(elapsed_ms),
                Some("failed"),
                Some("WORKER_STOPPED".into()),
            ),
            Ok(Ok(None)) => {}
        }
        host.finish_package_intake(&operation_id);
        let projection = projection.map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        if let Some(projection) = &projection {
            if let Some(tender) = &projection.selected_tender {
                // The Tender is already atomically published. A transient
                // scheduling failure must not turn that successful import
                // into a misleading package failure in the renderer; active
                // intake records are resumed by the normal startup/refresh
                // path.
                let _ = host.start_manager_intake_background(tender.tender_id.clone());
            }
        }
        Ok(projection)
    }

    #[tauri::command]
    pub(super) async fn resume_manager_intakes(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<(), TenderCommandError> {
        host.inner().resume_manager_intakes()
    }

    #[tauri::command]
    pub(super) async fn retry_manager_intake(
        host: tauri::State<'_, QuantixHost>,
        command: RetryManagerIntakeCommand,
    ) -> Result<(), TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        host.inner().retry_manager_intake(&command.tender_id)
    }

    #[tauri::command]
    pub(super) async fn rebind_manager_intake_provider(
        host: tauri::State<'_, QuantixHost>,
        command: RebindManagerIntakeProviderCommand,
    ) -> Result<(), TenderCommandError> {
        command
            .validate()
            .map_err(|_| TenderCommandError::new(TenderErrorCode::InvalidCommand))?;
        host.inner()
            .rebind_manager_intake_provider(&command.tender_id)
            .await
    }

    #[tauri::command]
    pub(super) async fn select_manager_workspace_tender(
        host: tauri::State<'_, QuantixHost>,
        command: SelectManagerWorkspaceTenderCommand,
    ) -> Result<ManagerWorkspaceProjection, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.select_manager_workspace_tender(command))
            .await
            .map_err(|_| TenderCommandError {
                code: TenderErrorCode::StoreUnavailable,
            })?
    }

    #[tauri::command]
    pub(super) async fn record_engineer_workspace_message(
        host: tauri::State<'_, QuantixHost>,
        command: RecordEngineerWorkspaceMessageCommand,
    ) -> Result<ManagerWorkspaceProjection, TenderCommandError> {
        let host = host.inner().clone();
        let message_host = host.clone();
        let projection = tauri::async_runtime::spawn_blocking(move || {
            message_host.record_engineer_workspace_message(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        if projection
            .intake
            .as_ref()
            .is_some_and(|intake| intake.stage.is_active())
            && host.document_tools_are_verified()
        {
            if let Some(tender) = &projection.selected_tender {
                host.start_manager_intake_background(tender.tender_id.clone())?;
            }
        }
        Ok(projection)
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
        release_candidate_manifest_sha256: String,
    ) -> Result<Option<PublicReleaseGateRecord>, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || {
            host.inspect_current_public_release_gate(&release_candidate_manifest_sha256)
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
    pub(super) async fn trash_recovery_required_tender(
        host: tauri::State<'_, QuantixHost>,
        command: TrashRecoveryRequiredTenderCommand,
    ) -> Result<TrashedTenderRecord, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || host.trash_recovery_required_tender(command))
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
    pub(super) async fn inspect_startup_reconciliation(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<StartupReconciliationReport, TenderCommandError> {
        let host = host.inner().clone();
        tauri::async_runtime::spawn_blocking(move || Ok(host.inspect_startup_reconciliation()))
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
        command: PurgeTrashedTenderCommand,
    ) -> Result<DeletionReceipt, TenderCommandError> {
        let host = host.inner().clone();
        let deletion_host = host.clone();
        let receipt = tauri::async_runtime::spawn_blocking(move || {
            deletion_host.purge_trashed_tender(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        retry_provider_cleanup_in_background(host);
        Ok(receipt)
    }

    #[tauri::command]
    pub(super) async fn purge_recovery_required_tender(
        host: tauri::State<'_, QuantixHost>,
        command: PurgeRecoveryRequiredTenderCommand,
    ) -> Result<DeletionReceipt, TenderCommandError> {
        let host = host.inner().clone();
        let deletion_host = host.clone();
        let receipt = tauri::async_runtime::spawn_blocking(move || {
            deletion_host.purge_recovery_required_tender(command)
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        retry_provider_cleanup_in_background(host);
        Ok(receipt)
    }

    #[tauri::command]
    pub(super) async fn inspect_deletion_receipts(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<Vec<DeletionReceipt>, TenderCommandError> {
        let host = host.inner().clone();
        let inspection_host = host.clone();
        let receipts = tauri::async_runtime::spawn_blocking(move || {
            inspection_host.inspect_deletion_receipts()
        })
        .await
        .map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        retry_provider_cleanup_in_background(host);
        Ok(receipts)
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
        let tender_id = command.tender_id;
        let intake_tender_id = tender_id.clone();
        let host = host.inner().clone();
        let source_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Tender Package")
            .to_owned();
        let control = host.begin_package_intake(
            PackageIntakeOperationKind::AddPackage,
            source_kind,
            source_name,
        )?;
        let operation_id = control.snapshot().operation_id.clone();
        let intake_started = Instant::now();
        let import_host = host.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            import_host.import_tender_package_with_control(
                ImportTenderPackageCommand {
                    tender_id,
                    source_path: source_path.to_string_lossy().into_owned(),
                },
                &control,
            )
        })
        .await;
        let elapsed_ms = u64::try_from(intake_started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match &result {
            Ok(Ok(Some(_))) => host.record_tender_diagnostic(
                &intake_tender_id,
                crate::DiagnosticSeverity::Info,
                crate::DiagnosticComponent::Package,
                "package_add_completed",
                "An additional Tender package was published",
                Some(operation_id.clone()),
                Some(elapsed_ms),
                Some("completed"),
                None,
            ),
            Ok(Err(error)) => host.record_tender_diagnostic(
                &intake_tender_id,
                crate::DiagnosticSeverity::Error,
                crate::DiagnosticComponent::Package,
                "package_add_failed",
                "The additional package intake operation failed",
                Some(operation_id.clone()),
                Some(elapsed_ms),
                Some("failed"),
                Some(format!("{:?}", error.code)),
            ),
            Err(_) => host.record_tender_diagnostic(
                &intake_tender_id,
                crate::DiagnosticSeverity::Error,
                crate::DiagnosticComponent::Package,
                "package_add_stopped",
                "The additional package worker stopped unexpectedly",
                Some(operation_id.clone()),
                Some(elapsed_ms),
                Some("failed"),
                Some("WORKER_STOPPED".into()),
            ),
            Ok(Ok(None)) => {}
        }
        host.finish_package_intake(&operation_id);
        let result = result.map_err(|_| TenderCommandError {
            code: TenderErrorCode::StoreUnavailable,
        })??;
        if result.is_some() {
            // Package publication is already committed at this boundary.
            // Keep scheduling failures separate from the import result so the
            // UI never asks the Engineer to repeat a successful package add.
            let _ = host.start_manager_intake_background(intake_tender_id);
        }
        Ok(result)
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
    pub(super) async fn search_evidence_semantic(
        host: tauri::State<'_, QuantixHost>,
        command: SearchEvidenceSemanticCommand,
    ) -> Result<EvidenceSemanticSearchResult, TenderCommandError> {
        host.inner().search_evidence_semantic(command).await
    }

    #[tauri::command]
    pub(super) async fn inspect_document_tool_readiness(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<RuntimeReadiness, &'static str> {
        let host = host.inner().clone();
        Ok(host.inspect_runtime_readiness().await)
    }

    #[tauri::command]
    pub(super) async fn prepare_document_tools(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<RuntimeReadiness, &'static str> {
        let host = host.inner().clone();
        Ok(host.repair_runtime_readiness().await)
    }

    #[tauri::command]
    pub(super) async fn repair_document_tools(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<RuntimeReadiness, &'static str> {
        let host = host.inner().clone();
        Ok(host.repair_runtime_readiness().await)
    }

    #[tauri::command]
    pub(super) fn inspect_document_tool_preparation_progress(
        host: tauri::State<'_, QuantixHost>,
    ) -> RuntimePreparationProgress {
        host.inner().inspect_runtime_preparation_progress()
    }

    #[tauri::command]
    pub(super) fn cancel_document_tool_preparation(host: tauri::State<'_, QuantixHost>) -> bool {
        host.inner().cancel_runtime_preparation()
    }

    #[tauri::command]
    pub(super) async fn inspect_codex_runtime(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<crate::managed_runtime::ManagedCodexRuntimeStatus, &'static str> {
        Ok(crate::managed_runtime::inspect_managed_codex_runtime(
            host.inner().application_home(),
        ))
    }

    #[tauri::command]
    pub(super) async fn prepare_codex_runtime(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<crate::managed_runtime::ManagedCodexRuntimeStatus, &'static str> {
        crate::managed_runtime::prepare_managed_codex_runtime(
            host.inner().application_home(),
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .map_err(|_| "codex_runtime_prepare_failed")
    }

    #[tauri::command]
    pub(super) async fn inspect_worker_runtime(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<crate::managed_runtime::ManagedWorkerRuntimeStatus, &'static str> {
        Ok(host.inner().inspect_worker_runtime())
    }

    #[tauri::command]
    pub(super) async fn prepare_worker_runtime(
        host: tauri::State<'_, QuantixHost>,
    ) -> Result<crate::managed_runtime::ManagedWorkerRuntimeStatus, &'static str> {
        host.inner()
            .prepare_worker_runtime(tokio_util::sync::CancellationToken::new())
            .await
            .map_err(|_| "worker_runtime_prepare_failed")
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
        .manage(StartupSplashState::default())
        .invoke_handler(tauri::generate_handler![
            tauri_commands::report_startup_splash_preferences,
            tauri_commands::notify_startup_display_ready,
            tauri_commands::inspect_startup_display_ready,
            tauri_commands::finish_startup_splash,
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
            tauri_commands::refresh_application_settings,
            tauri_commands::inspect_application_settings,
            tauri_commands::update_general_application_preferences,
            tauri_commands::update_ai_execution_selection,
            tauri_commands::inspect_tender_ai_execution,
            tauri_commands::update_tender_ai_execution,
            tauri_commands::confirm_ai_execution_selection,
            tauri_commands::clear_ai_execution_selection,
            tauri_commands::inspect_quantix_doctor,
            tauri_commands::repair_quantix_doctor,
            tauri_commands::inspect_diagnostics_status,
            tauri_commands::inspect_diagnostic_timeline,
            tauri_commands::start_tender_deep_diagnostics,
            tauri_commands::stop_tender_deep_diagnostics,
            tauri_commands::open_diagnostic_logs,
            tauri_commands::export_diagnostics_support_bundle,
            tauri_commands::record_renderer_diagnostic,
            tauri_commands::start_chatgpt_login,
            tauri_commands::start_chatgpt_device_login,
            tauri_commands::open_chatgpt_device_login_page,
            tauri_commands::cancel_chatgpt_login,
            tauri_commands::disconnect_chatgpt,
            tauri_commands::inspect_manager_workspace,
            tauri_commands::search_manager_workspace,
            tauri_commands::inspect_package_intake_progress,
            tauri_commands::cancel_package_intake,
            tauri_commands::start_manager_tender,
            tauri_commands::resume_manager_intakes,
            tauri_commands::retry_manager_intake,
            tauri_commands::rebind_manager_intake_provider,
            tauri_commands::select_manager_workspace_tender,
            tauri_commands::record_engineer_workspace_message,
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
            tauri_commands::trash_recovery_required_tender,
            tauri_commands::inspect_trashed_tenders,
            tauri_commands::inspect_startup_reconciliation,
            tauri_commands::restore_trashed_tender,
            tauri_commands::purge_trashed_tender,
            tauri_commands::purge_recovery_required_tender,
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
            tauri_commands::search_evidence_semantic,
            tauri_commands::inspect_document_tool_readiness,
            tauri_commands::prepare_document_tools,
            tauri_commands::inspect_codex_runtime,
            tauri_commands::prepare_codex_runtime,
            tauri_commands::inspect_worker_runtime,
            tauri_commands::prepare_worker_runtime,
            tauri_commands::repair_document_tools,
            tauri_commands::inspect_document_tool_preparation_progress,
            tauri_commands::cancel_document_tool_preparation,
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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(splash) = app.get_webview_window("splashscreen") {
                if splash.is_visible().unwrap_or(false) {
                    let _ = splash.set_focus();
                    return;
                }
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .on_window_event(|window, event| {
            if window.label() == "splashscreen"
                && matches!(event, WindowEvent::CloseRequested { .. })
            {
                let state = window.app_handle().state::<StartupSplashState>();
                if !state.finished.load(Ordering::Acquire) {
                    window.app_handle().exit(0);
                }
            }
        })
        .setup(|app| {
            let splash_state = app.state::<StartupSplashState>().inner().clone();
            start_splash_watchdog(app.handle().clone(), splash_state);
            let default_application_home = app.path().home_dir()?.join(".quantix");
            #[cfg(debug_assertions)]
            let application_home = std::env::var_os("QUANTIX_APPLICATION_HOME")
                .map(std::path::PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or(default_application_home);
            #[cfg(not(debug_assertions))]
            let application_home = default_application_home;
            let resource_directory = app.path().resource_dir()?;
            #[cfg(target_os = "windows")]
            if let Some(window) = app.get_webview_window("main") {
                window.set_decorations(false)?;
            }
            app.manage(QuantixHost::new(application_home, resource_directory));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
