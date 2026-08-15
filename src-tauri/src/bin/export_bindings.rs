use quantix_lib::{
    AcceptanceArtifactHash, AcceptanceCheckResult, AcceptanceStageTiming, AgentRunInspection,
    AgentRunSummary, AiExecutionSelection, AiProviderKind, ApplicationSettingsView,
    ApproveSubmissionReleaseCommand, BootstrapRole, CodexProductionAssuranceEvidence,
    CreatePortableTenderArchiveCommand, DeletionReceipt, EvaluatePublicReleaseGateCommand,
    ExportReleaseCopyCommand, ImportPortableTenderArchiveCommand, InspectManagerWorkspaceCommand,
    IntegrationTermsDecision, LicenseDistributionReview, LiveQualificationMetrics,
    LiveQualificationRun, ManagerConversation, ManagerIntakeStage, ManagerIntakeStatus,
    ManagerIntakeStatusKind, ManagerWorkspaceProjection, ManagerWorkspaceTender,
    NativePlatformQualificationEvidence, NativePlatformQualificationRecord,
    PortableTenderArchiveRecord, PrivateQualificationRecord, ProductAcceptanceOutcome,
    ProductAcceptanceRecord, ProductAcceptanceRun, ProviderConnectionStatus,
    ProviderConnectionView, ProviderModelOption, ProviderReasoningOption,
    ProviderReasoningSelection, PublicReleaseGateOutcome, PublicReleaseGateRecord,
    RecordEngineerWorkspaceMessageCommand, RecordLiveQualificationRunCommand,
    RecordNativePlatformQualificationCommand, ReleaseCopyExport, ReleaseCopyItem,
    RetryManagerIntakeCommand, RunDeterministicAcceptanceCommand,
    SelectManagerWorkspaceTenderCommand, StartManagerTenderCommand, SubmissionReleaseApproval,
    SubmissionReleaseInspection, SubmissionReleaseState, TechnicalRiskAcceptance,
    TenderOfficeMessage, TenderOfficeMessageAuthor, TenderOfficeMessageKind,
    TenderRetentionDecisionCommand, TenderRetentionDecisionRecord, TenderRetentionState,
    TrashedTenderDecisionCommand, TrashedTenderRecord, TrashedTenderState,
    UpdateAiExecutionSelectionCommand, WorkspaceActionKind, WorkspaceCurrentAction,
    WorkspaceFilesSummary, WorkspaceMessageReference, WorkspaceMessageReferenceKind,
    WorkspaceTeamSummary, WorkspaceTenderDocument, WorkspaceWorkSummary,
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
    ImportPortableTenderArchiveCommand::export_all(&config)?;
    PortableTenderArchiveRecord::export_all(&config)?;
    TenderRetentionDecisionCommand::export_all(&config)?;
    TenderRetentionDecisionRecord::export_all(&config)?;
    TenderRetentionState::export_all(&config)?;
    TrashedTenderDecisionCommand::export_all(&config)?;
    TrashedTenderRecord::export_all(&config)?;
    TrashedTenderState::export_all(&config)?;
    AcceptanceArtifactHash::export_all(&config)?;
    AcceptanceCheckResult::export_all(&config)?;
    AcceptanceStageTiming::export_all(&config)?;
    ProductAcceptanceOutcome::export_all(&config)?;
    ProductAcceptanceRecord::export_all(&config)?;
    ProductAcceptanceRun::export_all(&config)?;
    RunDeterministicAcceptanceCommand::export_all(&config)?;
    LiveQualificationMetrics::export_all(&config)?;
    LiveQualificationRun::export_all(&config)?;
    PrivateQualificationRecord::export_all(&config)?;
    RecordLiveQualificationRunCommand::export_all(&config)?;
    CodexProductionAssuranceEvidence::export_all(&config)?;
    EvaluatePublicReleaseGateCommand::export_all(&config)?;
    IntegrationTermsDecision::export_all(&config)?;
    LicenseDistributionReview::export_all(&config)?;
    NativePlatformQualificationEvidence::export_all(&config)?;
    NativePlatformQualificationRecord::export_all(&config)?;
    RecordNativePlatformQualificationCommand::export_all(&config)?;
    PublicReleaseGateOutcome::export_all(&config)?;
    PublicReleaseGateRecord::export_all(&config)?;
    TechnicalRiskAcceptance::export_all(&config)?;
    InspectManagerWorkspaceCommand::export_all(&config)?;
    StartManagerTenderCommand::export_all(&config)?;
    SelectManagerWorkspaceTenderCommand::export_all(&config)?;
    RecordEngineerWorkspaceMessageCommand::export_all(&config)?;
    RetryManagerIntakeCommand::export_all(&config)?;
    TenderOfficeMessageAuthor::export_all(&config)?;
    TenderOfficeMessageKind::export_all(&config)?;
    TenderOfficeMessage::export_all(&config)?;
    ManagerConversation::export_all(&config)?;
    ManagerWorkspaceTender::export_all(&config)?;
    WorkspaceActionKind::export_all(&config)?;
    WorkspaceCurrentAction::export_all(&config)?;
    WorkspaceWorkSummary::export_all(&config)?;
    WorkspaceFilesSummary::export_all(&config)?;
    WorkspaceTenderDocument::export_all(&config)?;
    WorkspaceMessageReferenceKind::export_all(&config)?;
    WorkspaceMessageReference::export_all(&config)?;
    WorkspaceTeamSummary::export_all(&config)?;
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
    AiExecutionSelection::export_all(&config)?;
    ApplicationSettingsView::export_all(&config)?;
    UpdateAiExecutionSelectionCommand::export_all(&config)?;
    AgentRunInspection::export_all(&config)?;
    AgentRunSummary::export_all(&config)?;
    Ok(())
}
