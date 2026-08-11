#![recursion_limit = "256"]

mod agent_runtime;
mod document_parsing;
mod host;
mod process_supervisor;
mod runtime_readiness;
mod setup;
mod tender_intake;
mod tender_store;

pub use agent_runtime::{
    approve_one_run_access, AccessApproval, AccessRequest, AgentAccessRequestStatus,
    AgentAccessRequestView, AgentAccessResolution, AgentProfileStatus, AgentProfileVersionView,
    AgentResourceBudget, AgentRunActivity, AgentRunHistoryItem, AgentRunHistoryPage,
    AgentRunInspection, AgentRunPermissions, AgentRunRecoveryDecision, AgentRunRecoveryDisposition,
    AgentRunState, AgentRunSummary, AgentRunWorkspaceManifest, AgentTaskInputReference,
    ApproveAgentAccessCommand, BootstrapAuthority, BootstrapRole, BootstrapTeamMember,
    DataClassification, DataViewManifest, InspectAgentRunCommand, InspectAgentRunHistoryCommand,
    InterruptAgentRunCommand, OneRunAccessGrant, PermissionCeiling, PermissionDenialReason,
    PermissionGrant, ProposedAgentResult, ProviderEvent, ProviderEventKind, ProviderFailure,
    ProviderFailureCategory, ProviderRateLimit, ProviderRateLimitState, ProviderRateLimitWindow,
    ProviderUsage, RequestAgentAccessCommand, ResolveAgentAccessCommand,
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
    ApprovePricedCostBaselineCommand, ApprovePricingAdjustmentCommand,
    ApproveProductionFindingExceptionCommand, ApproveTenderPriceCommand, ApprovedQueryTreatment,
    ApprovedTenderPrice, BasisOfEstimateReview, BasisOfEstimateReviewFinding,
    BasisOfEstimateReviewOutcome, BasisOfEstimateReviewResult, BasisOfEstimateVersion,
    BidDecisionApprovalDecision, BidDecisionApprovalHistoryPage, BidDecisionApprovalInvalidation,
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
    CommercialStrategy, CommercialStrategyApproval, ComplianceDisposition,
    ComplianceDispositionUpdate, ComplianceMatrixPage, ComplianceMatrixRow,
    ComposeTenderOfficeCommand, ContentVersionSummary, ControlledBoqCalculationRun,
    ControlledBoqCalculationStatus, CostBreakdownComponent, CostComponentCategory,
    CostEstimatorBasisResult, CostEstimatorCalculationResult, CreateBidDecisionPackageCommand,
    CreateCalculationScenarioCommand, CreateCommercialStrategyCommand,
    CreateExternalRfiDraftCommand, CreatePricedCostBaselineCommand, CreatePricingAdjustmentCommand,
    CreatePricingScenarioCommand, CreateTenderBackupCommand, CreateTenderCommand,
    CreateTenderEngineerEntryCommand, CreateTenderQueryCommand, DecideBidDecisionPackageCommand,
    DecideTenderQueryTreatmentCommand, DecideTenderRecordCommand, DecideWorkPlanProposalCommand,
    DesignateBoqTableCommand, EstimateAggregateCalculationInput, EstimateAggregateCalculationRun,
    EstimateAllowance, EstimateMaterialAssumption, EstimateQueryObservation,
    EstimateQueryReference, EstimateQuotation, EstimateQuotationKind, EstimateWorkspaceInspection,
    ExchangeRateType, ExportApprovedExternalRfiCommand, ExternalRfiApproval, ExternalRfiDraft,
    ExternalRfiEligibleQuery, ExternalRfiEligibleQueryPage, ExternalRfiExportRecord,
    ExternalRfiFindingSeverity, ExternalRfiPage, ExternalRfiQueryReference, ExternalRfiQuestion,
    ExternalRfiRecipient, ExternalRfiResponseCandidatePage, ExternalRfiResponseInterpretation,
    ExternalRfiResponseLink, ExternalRfiReview, ExternalRfiReviewFinding, ExternalRfiReviewOutcome,
    ExternalRfiReviewResult, InspectBidDecisionApprovalHistoryCommand,
    InspectBidDecisionPackageRecordsCommand, InspectCalculationWorkspaceCommand,
    InspectComplianceMatrixCommand, InspectEstimateWorkspaceCommand,
    InspectExternalRfiEligibleQueriesCommand, InspectExternalRfiResponseCandidatesCommand,
    InspectExternalRfisCommand, InspectPricingWorkspaceCommand, InspectProductionTaskReviewCommand,
    InspectTenderQueriesCommand, InspectTenderRecordsCommand, InterpretExternalRfiResponseCommand,
    InvalidateBidDecisionApprovalCommand, MajorFindingPolicy, ManagerCapabilityDemandInput,
    OpenTenderCommand, PrepareTenderRecoveryCommand, PricedCostBaselineApproval,
    PricedCostBaselineReview, PricedCostBaselineReviewFinding, PricedCostBaselineReviewOutcome,
    PricedCostBaselineReviewResult, PricedCostBaselineVersion, PricingAdjustmentApproval,
    PricingAdjustmentDirection, PricingAdjustmentKind, PricingAdjustmentReference,
    PricingAdjustmentReviewResult, PricingAdjustmentVersion, PricingCalculationAdjustmentInput,
    PricingCalculationRun, PricingDecisionHistoryEntry, PricingScenarioSelection,
    PricingScenarioVersion, PricingWorkspaceInspection, ProductionArtifactPayload,
    ProductionArtifactVersion, ProductionArtifactVersionSummary, ProductionFindingDisposition,
    ProductionFindingDispositionKind, ProductionFindingSeverity, ProductionIntegrationReadiness,
    ProductionQueryTreatmentApplication, ProductionRemediation, ProductionReview,
    ProductionReviewFinding, ProductionReviewResult, ProductionTaskInspection,
    ProductionTaskReviewInspection, ProductionTaskRunResult, ProductionTaskState,
    ProposeBoqCalculationRuleCommand, RegisterExternalRfiResponseCommand,
    RegisterTenderContentCommand, ResolveBidDecisionReturnReworkCommand,
    ResolveTenderRecoveryCommand, ResourceImplication, ReviewFindingSeverity,
    ReviseExternalRfiDraftCommand, ReviseTenderCommand, ReviseTenderQueryCommand,
    ReviseWorkPlanProposalCommand, RunBasisOfEstimateReviewCommand,
    RunBidDecisionPackageReviewCommand, RunCalculationRuleReviewCommand,
    RunCostEstimatorBasisCommand, RunCostEstimatorCalculationCommand, RunExternalRfiReviewCommand,
    RunPricedCostBaselineReviewCommand, RunPricingAdjustmentReviewCommand,
    RunProductionTaskCommand, RunTenderRecordExtractionCommand, RunTenderRecordReviewCommand,
    SelectPricingScenarioCommand, StartupReconciliationReport, TenderBackupRecord,
    TenderBackupState, TenderCatalogueEntry, TenderCommandError, TenderErrorCode,
    TenderEvidenceReference, TenderInspection, TenderIntegrityIssue, TenderIntegrityReport,
    TenderIntegrityState, TenderLifecyclePhase, TenderProductionInspection, TenderQuery,
    TenderQueryInvalidation, TenderQueryPage, TenderQueryResponse, TenderQueryStatus,
    TenderQueryTreatment, TenderQueryTreatmentProposal, TenderQueryTreatmentProposalInput,
    TenderQueryType, TenderRecordAuthority, TenderRecordAuthorityKind,
    TenderRecordAuthorityReference, TenderRecordBasisKind, TenderRecordContradiction,
    TenderRecordDecisionResult, TenderRecordEngineerDecisionKind, TenderRecordEvidence,
    TenderRecordExtractionResult, TenderRecordField, TenderRecordInspection, TenderRecordKind,
    TenderRecordPage, TenderRecordReview, TenderRecordReviewOutcome, TenderRecordReviewResult,
    TenderRecordSourceRelationship, TenderRecordTrustClass, TenderRecordVersionReference,
    TenderRecoveryChoice, TenderRecoveryDecision, TenderRecoveryDecisionRecord,
    TenderRecoveryRecord, TenderRecoveryState, TenderSummary, WorkPlanApprovalRecord,
    WorkPlanCapabilityGap, WorkPlanDecision, WorkPlanProfileBinding, WorkPlanProposalInspection,
    WorkPlanRevisionAction, WorkPlanTask, WorkPlanWorkstream,
};

use tauri::Manager;

mod tauri_commands {
    use super::{
        ensure_quantix_setup as ensure_setup, ActivateTenderProductionCommand,
        AgentAccessRequestView, AgentRunActivity, AgentRunHistoryPage, AgentRunInspection,
        AgentRunRecoveryDecision, ApproveAgentAccessCommand, ApproveBasisOfEstimateCommand,
        ApproveCalculationRuleCommand, ApproveCommercialStrategyCommand,
        ApproveControlledBoqCalculationRunCommand, ApproveExternalRfiForIssueCommand,
        ApprovePricedCostBaselineCommand, ApprovePricingAdjustmentCommand,
        ApproveProductionFindingExceptionCommand, ApproveTenderPriceCommand,
        BasisOfEstimateReviewResult, BasisOfEstimateVersion, BidDecisionApprovalHistoryPage,
        BidDecisionApprovalInvalidationResult, BidDecisionApprovalResult,
        BidDecisionPackageInspection, BidDecisionPackageRecordPage, BidDecisionPackageReviewResult,
        BidDecisionReturnReworkResult, BoqTableDesignation, CalculationRuleReviewResult,
        CalculationRuleVersion, CalculationScenarioVersion, CalculationWorkspaceInspection,
        ChooseTenderPackageCommand, CommercialStrategy, ComplianceMatrixPage,
        ComposeTenderOfficeCommand, ConfirmSourceRelationshipCommand, ControlledBoqCalculationRun,
        CostEstimatorBasisResult, CostEstimatorCalculationResult, CreateBidDecisionPackageCommand,
        CreateCalculationScenarioCommand, CreateCommercialStrategyCommand,
        CreateExternalRfiDraftCommand, CreatePricedCostBaselineCommand,
        CreatePricingAdjustmentCommand, CreatePricingScenarioCommand, CreateTenderBackupCommand,
        CreateTenderCommand, CreateTenderEngineerEntryCommand, CreateTenderQueryCommand,
        DecideBidDecisionPackageCommand, DecideTenderQueryTreatmentCommand,
        DecideTenderRecordCommand, DecideWorkPlanProposalCommand, DesignateBoqTableCommand,
        DocumentParseResult, DocumentRegister, EstimateWorkspaceInspection, EvidenceDocument,
        EvidenceSearchResult, ExportApprovedExternalRfiCommand, ExternalRfiDraft,
        ExternalRfiEligibleQueryPage, ExternalRfiExportRecord, ExternalRfiPage,
        ExternalRfiResponseCandidatePage, ExternalRfiReviewResult, ImportTenderPackageCommand,
        InspectAgentRunCommand, InspectAgentRunHistoryCommand,
        InspectBidDecisionApprovalHistoryCommand, InspectBidDecisionPackageRecordsCommand,
        InspectCalculationWorkspaceCommand, InspectComplianceMatrixCommand,
        InspectEstimateWorkspaceCommand, InspectExternalRfiEligibleQueriesCommand,
        InspectExternalRfiResponseCandidatesCommand, InspectExternalRfisCommand,
        InspectPricingWorkspaceCommand, InspectProductionTaskReviewCommand,
        InspectTenderQueriesCommand, InspectTenderRecordsCommand,
        InterpretExternalRfiResponseCommand, InterruptAgentRunCommand,
        InvalidateBidDecisionApprovalCommand, OpenTenderCommand, ParseSourceArtifactCommand,
        PrepareTenderRecoveryCommand, PricedCostBaselineReviewResult, PricedCostBaselineVersion,
        PricingAdjustmentReviewResult, PricingAdjustmentVersion, PricingScenarioVersion,
        PricingWorkspaceInspection, ProductionTaskReviewInspection, ProductionTaskRunResult,
        ProposeBoqCalculationRuleCommand, QuantixHost, RegisterExternalRfiResponseCommand,
        RequestAgentAccessCommand, ResolveAgentAccessCommand,
        ResolveBidDecisionReturnReworkCommand, ResolveIndeterminateAgentRunCommand,
        ResolveTenderRecoveryCommand, ReviseExternalRfiDraftCommand, ReviseTenderCommand,
        ReviseTenderQueryCommand, ReviseWorkPlanProposalCommand, RunBasisOfEstimateReviewCommand,
        RunBidDecisionPackageReviewCommand, RunBootstrapAgentCommand,
        RunCalculationRuleReviewCommand, RunCostEstimatorBasisCommand,
        RunCostEstimatorCalculationCommand, RunExternalRfiReviewCommand,
        RunPricedCostBaselineReviewCommand, RunPricingAdjustmentReviewCommand,
        RunProductionTaskCommand, RunTenderRecordExtractionCommand, RunTenderRecordReviewCommand,
        RuntimeReadiness, SearchEvidenceCommand, SelectPricingScenarioCommand, SetupOutcome,
        TenderBackupRecord, TenderCatalogueEntry, TenderCommandError, TenderErrorCode,
        TenderIntegrityReport, TenderPackageImportResult, TenderPackageSourceKind,
        TenderProductionInspection, TenderQuery, TenderQueryPage, TenderRecordAuthority,
        TenderRecordDecisionResult, TenderRecordExtractionResult, TenderRecordPage,
        TenderRecordReviewResult, TenderRecoveryRecord, TenderSummary, WorkPlanProposalInspection,
    };
    use tauri_plugin_dialog::DialogExt;

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

pub fn configure_tauri_builder<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        tauri_commands::ensure_quantix_setup,
        tauri_commands::create_tender,
        tauri_commands::list_tenders,
        tauri_commands::open_tender,
        tauri_commands::inspect_tender_integrity,
        tauri_commands::create_tender_backup,
        tauri_commands::inspect_tender_backups,
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
