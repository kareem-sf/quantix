use quantix_lib::{
    AcceptanceArtifactHash, AcceptanceCheckResult, AcceptanceStageTiming, AgentRepairFeedback,
    AgentRunInspection, AgentRunSummary, AiExecutionApproval, AiExecutionSelection, AiProviderKind,
    AppearancePreference, ApplicationDiagnostics, ApplicationSettingsView, ApplicationStorageFacts,
    ApproveSubmissionReleaseCommand, BootstrapRole, CancelPackageIntakeCommand,
    ChatGptConnectionState, ChatGptConnectionStatus, ChatGptProductionAssuranceEvidence,
    ConfirmAiExecutionSelectionCommand, CreatePortableTenderArchiveCommand, DeepDiagnosticsSession,
    DeletionReceipt, DiagnosticComponent, DiagnosticCorrelation, DiagnosticEvent, DiagnosticHealth,
    DiagnosticScope, DiagnosticSeverity, DiagnosticSupportBundleResult, DiagnosticTimelineEvent,
    DiagnosticTimelineFilter, DiagnosticTimelinePage, DiagnosticsDeepState, DiagnosticsDeepStatus,
    DiagnosticsStatus, DiagnosticsStatusState, ErasedTenderCopyClass,
    EvaluatePublicReleaseGateCommand, ExportDiagnosticsSupportBundleCommand,
    ExportReleaseCopyCommand, GeneralApplicationPreferences, ImportPortableTenderArchiveCommand,
    InspectDiagnosticTimelineCommand, InspectDiagnosticsStatusCommand,
    InspectManagerWorkspaceCommand, InspectQuantixDoctorCommand, InspectTenderAiExecutionCommand,
    IntegrationTermsDecision, LicenseDistributionReview, LiveQualificationMetrics,
    LiveQualificationRun, ManagedCodexRuntimeState, ManagedCodexRuntimeStatus,
    ManagedWorkerRuntimeState, ManagedWorkerRuntimeStatus, ManagerConversation, ManagerIntakeStage,
    ManagerIntakeStatus, ManagerIntakeStatusKind, ManagerWorkspaceProjection,
    ManagerWorkspaceTender, ManagerWorkspaceTenderState, NativePlatformQualificationEvidence,
    NativePlatformQualificationRecord, OpenDiagnosticLogsCommand, OpenDiagnosticLogsResult,
    PackageIntakeOperationKind, PackageIntakeProgress, PackageIntakeStage,
    PortableTenderArchiveRecord, PrivateQualificationRecord, ProductAcceptanceOutcome,
    ProductAcceptanceRecord, ProductAcceptanceRun, ProviderCleanupStatus, ProviderConnectionStatus,
    ProviderConnectionView, ProviderModelOption, ProviderReasoningOption,
    ProviderReasoningSelection, ProviderReferenceDiscoveryState, PublicReleaseGateOutcome,
    PublicReleaseGateRecord, PurgeRecoveryRequiredTenderCommand, PurgeTrashedTenderCommand,
    QuantixDoctorArea, QuantixDoctorFinding, QuantixDoctorRepairAction, QuantixDoctorRepairCommand,
    QuantixDoctorRepairTarget, QuantixDoctorReport, QuantixDoctorSeverity,
    RebindManagerIntakeProviderCommand, RecordEngineerWorkspaceMessageCommand,
    RecordLiveQualificationRunCommand, RecordNativePlatformQualificationCommand,
    RecordRendererDiagnosticCommand, ReleaseCopyExport, ReleaseCopyItem, RendererDiagnosticKind,
    RetryManagerIntakeCommand, RunDeterministicAcceptanceCommand, RuntimePreparationActivity,
    RuntimePreparationActivityStatus, RuntimePreparationProgress, RuntimePreparationStatus,
    RuntimePreparationStep, RuntimeReadiness, RuntimeReadinessIssue, RuntimeReadinessState,
    SearchManagerWorkspaceCommand, SelectManagerWorkspaceTenderCommand, SetupIssue,
    StartChatGptDeviceLoginResult, StartChatGptLoginError, StartChatGptLoginResult,
    StartChatGptLoginStatus, StartManagerTenderCommand, StartTenderDeepDiagnosticsCommand,
    StopTenderDeepDiagnosticsCommand, SubmissionReleaseApproval, SubmissionReleaseInspection,
    SubmissionReleaseState, TechnicalRiskAcceptance, TenderAiExecutionBinding,
    TenderAiSelectionReadiness, TenderDeletionSourceState, TenderErrorCode, TenderOfficeMessage,
    TenderOfficeMessageAuthor, TenderOfficeMessageKind, TenderRetentionDecisionCommand,
    TenderRetentionDecisionRecord, TenderRetentionState, TrashRecoveryRequiredTenderCommand,
    TrashedTenderDecisionCommand, TrashedTenderRecord, TrashedTenderState,
    UpdateAiExecutionSelectionCommand, UpdateCompatibilityManifest, UpdateDiagnostic,
    UpdateGeneralApplicationPreferencesCommand, UpdateTenderAiExecutionSelectionCommand,
    WorkspaceActionKind, WorkspaceAgentReference, WorkspaceAgentRunReference,
    WorkspaceCapabilityReadiness, WorkspaceCapabilityReadinessState, WorkspaceCurrentAction,
    WorkspaceDoctorBlockerArea, WorkspaceDoctorBlockerSummary, WorkspaceFilesSummary,
    WorkspaceMessageReference, WorkspaceMessageReferenceKind, WorkspaceOutputReference,
    WorkspaceSearchGroup, WorkspaceSearchHit, WorkspaceSearchProjection, WorkspaceSearchResultKind,
    WorkspaceTaskRow, WorkspaceTaskState, WorkspaceTeamSummary, WorkspaceTenderDocument,
    WorkspaceWorkSummary,
};
use ts_rs::{Config, TS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings");
    let config = Config::new().with_out_dir(output_directory);
    ApproveSubmissionReleaseCommand::export_all(&config)?;
    ExportReleaseCopyCommand::export_all(&config)?;
    ReleaseCopyExport::export_all(&config)?;
    ReleaseCopyItem::export_all(&config)?;
    SubmissionReleaseApproval::export_all(&config)?;
    SubmissionReleaseInspection::export_all(&config)?;
    SubmissionReleaseState::export_all(&config)?;
    CreatePortableTenderArchiveCommand::export_all(&config)?;
    DeletionReceipt::export_all(&config)?;
    ErasedTenderCopyClass::export_all(&config)?;
    ProviderCleanupStatus::export_all(&config)?;
    ProviderReferenceDiscoveryState::export_all(&config)?;
    ImportPortableTenderArchiveCommand::export_all(&config)?;
    PortableTenderArchiveRecord::export_all(&config)?;
    TenderRetentionDecisionCommand::export_all(&config)?;
    TenderRetentionDecisionRecord::export_all(&config)?;
    TenderRetentionState::export_all(&config)?;
    TrashedTenderDecisionCommand::export_all(&config)?;
    TrashRecoveryRequiredTenderCommand::export_all(&config)?;
    PurgeRecoveryRequiredTenderCommand::export_all(&config)?;
    PurgeTrashedTenderCommand::export_all(&config)?;
    TrashedTenderRecord::export_all(&config)?;
    TrashedTenderState::export_all(&config)?;
    TenderDeletionSourceState::export_all(&config)?;
    AcceptanceArtifactHash::export_all(&config)?;
    AcceptanceCheckResult::export_all(&config)?;
    AcceptanceStageTiming::export_all(&config)?;
    ProductAcceptanceOutcome::export_all(&config)?;
    ProductAcceptanceRecord::export_all(&config)?;
    ProductAcceptanceRun::export_all(&config)?;
    RunDeterministicAcceptanceCommand::export_all(&config)?;
    LiveQualificationMetrics::export_all(&config)?;
    LiveQualificationRun::export_all(&config)?;
    ManagedCodexRuntimeState::export_all(&config)?;
    ManagedCodexRuntimeStatus::export_all(&config)?;
    ManagedWorkerRuntimeState::export_all(&config)?;
    ManagedWorkerRuntimeStatus::export_all(&config)?;
    ManagedWorkerRuntimeState::export_all(&config)?;
    ManagedWorkerRuntimeStatus::export_all(&config)?;
    PrivateQualificationRecord::export_all(&config)?;
    RecordLiveQualificationRunCommand::export_all(&config)?;
    ChatGptProductionAssuranceEvidence::export_all(&config)?;
    EvaluatePublicReleaseGateCommand::export_all(&config)?;
    IntegrationTermsDecision::export_all(&config)?;
    LicenseDistributionReview::export_all(&config)?;
    NativePlatformQualificationEvidence::export_all(&config)?;
    NativePlatformQualificationRecord::export_all(&config)?;
    RecordNativePlatformQualificationCommand::export_all(&config)?;
    PublicReleaseGateOutcome::export_all(&config)?;
    PublicReleaseGateRecord::export_all(&config)?;
    TechnicalRiskAcceptance::export_all(&config)?;
    UpdateCompatibilityManifest::export_all(&config)?;
    UpdateDiagnostic::export_all(&config)?;
    InspectManagerWorkspaceCommand::export_all(&config)?;
    SearchManagerWorkspaceCommand::export_all(&config)?;
    CancelPackageIntakeCommand::export_all(&config)?;
    PackageIntakeOperationKind::export_all(&config)?;
    PackageIntakeProgress::export_all(&config)?;
    PackageIntakeStage::export_all(&config)?;
    StartManagerTenderCommand::export_all(&config)?;
    SelectManagerWorkspaceTenderCommand::export_all(&config)?;
    RecordEngineerWorkspaceMessageCommand::export_all(&config)?;
    RetryManagerIntakeCommand::export_all(&config)?;
    RebindManagerIntakeProviderCommand::export_all(&config)?;
    TenderOfficeMessageAuthor::export_all(&config)?;
    TenderOfficeMessageKind::export_all(&config)?;
    TenderOfficeMessage::export_all(&config)?;
    ManagerConversation::export_all(&config)?;
    ManagerWorkspaceTender::export_all(&config)?;
    ManagerWorkspaceTenderState::export_all(&config)?;
    WorkspaceActionKind::export_all(&config)?;
    WorkspaceCurrentAction::export_all(&config)?;
    WorkspaceCapabilityReadinessState::export_all(&config)?;
    WorkspaceCapabilityReadiness::export_all(&config)?;
    WorkspaceDoctorBlockerArea::export_all(&config)?;
    WorkspaceDoctorBlockerSummary::export_all(&config)?;
    WorkspaceWorkSummary::export_all(&config)?;
    WorkspaceTaskState::export_all(&config)?;
    WorkspaceTaskRow::export_all(&config)?;
    WorkspaceAgentReference::export_all(&config)?;
    WorkspaceAgentRunReference::export_all(&config)?;
    WorkspaceFilesSummary::export_all(&config)?;
    WorkspaceOutputReference::export_all(&config)?;
    WorkspaceTenderDocument::export_all(&config)?;
    WorkspaceMessageReferenceKind::export_all(&config)?;
    WorkspaceMessageReference::export_all(&config)?;
    WorkspaceTeamSummary::export_all(&config)?;
    WorkspaceSearchResultKind::export_all(&config)?;
    WorkspaceSearchHit::export_all(&config)?;
    WorkspaceSearchGroup::export_all(&config)?;
    WorkspaceSearchProjection::export_all(&config)?;
    ManagerIntakeStage::export_all(&config)?;
    ManagerIntakeStatusKind::export_all(&config)?;
    ManagerIntakeStatus::export_all(&config)?;
    BootstrapRole::export_all(&config)?;
    ManagerWorkspaceProjection::export_all(&config)?;
    AiProviderKind::export_all(&config)?;
    ProviderConnectionStatus::export_all(&config)?;
    ProviderReasoningSelection::export_all(&config)?;
    ProviderReasoningOption::export_all(&config)?;
    ProviderModelOption::export_all(&config)?;
    ProviderConnectionView::export_all(&config)?;
    AiExecutionApproval::export_all(&config)?;
    AiExecutionSelection::export_all(&config)?;
    TenderAiSelectionReadiness::export_all(&config)?;
    TenderAiExecutionBinding::export_all(&config)?;
    ApplicationSettingsView::export_all(&config)?;
    AppearancePreference::export_all(&config)?;
    GeneralApplicationPreferences::export_all(&config)?;
    ApplicationStorageFacts::export_all(&config)?;
    ApplicationDiagnostics::export_all(&config)?;
    UpdateGeneralApplicationPreferencesCommand::export_all(&config)?;
    UpdateAiExecutionSelectionCommand::export_all(&config)?;
    InspectTenderAiExecutionCommand::export_all(&config)?;
    UpdateTenderAiExecutionSelectionCommand::export_all(&config)?;
    ConfirmAiExecutionSelectionCommand::export_all(&config)?;
    InspectQuantixDoctorCommand::export_all(&config)?;
    QuantixDoctorSeverity::export_all(&config)?;
    QuantixDoctorArea::export_all(&config)?;
    QuantixDoctorRepairAction::export_all(&config)?;
    QuantixDoctorRepairTarget::export_all(&config)?;
    QuantixDoctorFinding::export_all(&config)?;
    QuantixDoctorReport::export_all(&config)?;
    QuantixDoctorRepairCommand::export_all(&config)?;
    DiagnosticSeverity::export_all(&config)?;
    DiagnosticScope::export_all(&config)?;
    DiagnosticComponent::export_all(&config)?;
    DiagnosticCorrelation::export_all(&config)?;
    DiagnosticEvent::export_all(&config)?;
    DiagnosticHealth::export_all(&config)?;
    DiagnosticTimelineFilter::export_all(&config)?;
    DiagnosticTimelineEvent::export_all(&config)?;
    DiagnosticTimelinePage::export_all(&config)?;
    DiagnosticsStatusState::export_all(&config)?;
    DiagnosticsDeepState::export_all(&config)?;
    DiagnosticsDeepStatus::export_all(&config)?;
    DiagnosticsStatus::export_all(&config)?;
    DeepDiagnosticsSession::export_all(&config)?;
    DiagnosticSupportBundleResult::export_all(&config)?;
    OpenDiagnosticLogsResult::export_all(&config)?;
    InspectDiagnosticsStatusCommand::export_all(&config)?;
    InspectDiagnosticTimelineCommand::export_all(&config)?;
    StartTenderDeepDiagnosticsCommand::export_all(&config)?;
    StopTenderDeepDiagnosticsCommand::export_all(&config)?;
    OpenDiagnosticLogsCommand::export_all(&config)?;
    ExportDiagnosticsSupportBundleCommand::export_all(&config)?;
    RendererDiagnosticKind::export_all(&config)?;
    RecordRendererDiagnosticCommand::export_all(&config)?;
    ChatGptConnectionState::export_all(&config)?;
    ChatGptConnectionStatus::export_all(&config)?;
    StartChatGptDeviceLoginResult::export_all(&config)?;
    StartChatGptLoginError::export_all(&config)?;
    StartChatGptLoginResult::export_all(&config)?;
    StartChatGptLoginStatus::export_all(&config)?;
    AgentRepairFeedback::export_all(&config)?;
    AgentRunInspection::export_all(&config)?;
    AgentRunSummary::export_all(&config)?;
    RuntimePreparationActivity::export_all(&config)?;
    RuntimePreparationActivityStatus::export_all(&config)?;
    RuntimePreparationProgress::export_all(&config)?;
    RuntimePreparationStatus::export_all(&config)?;
    RuntimePreparationStep::export_all(&config)?;
    RuntimeReadiness::export_all(&config)?;
    RuntimeReadinessIssue::export_all(&config)?;
    RuntimeReadinessState::export_all(&config)?;
    SetupIssue::export_all(&config)?;
    TenderErrorCode::export_all(&config)?;
    Ok(())
}
