use quantix_lib::{
    AcceptanceArtifactHash, AcceptanceCheckResult, AcceptanceStageTiming,
    ApproveSubmissionReleaseCommand, CodexProductionAssuranceEvidence,
    CreatePortableTenderArchiveCommand, DeletionReceipt, EvaluatePublicReleaseGateCommand,
    ExportReleaseCopyCommand, ImportPortableTenderArchiveCommand, InspectManagerWorkspaceCommand,
    IntegrationTermsDecision, LicenseDistributionReview, LiveQualificationMetrics,
    LiveQualificationRun, ManagerConversation, ManagerWorkspaceProjection, ManagerWorkspaceTender,
    NativePlatformQualificationEvidence, NativePlatformQualificationRecord,
    PortableTenderArchiveRecord, PrivateQualificationRecord, ProductAcceptanceOutcome,
    ProductAcceptanceRecord, ProductAcceptanceRun, PublicReleaseGateOutcome,
    PublicReleaseGateRecord, RecordEngineerWorkspaceMessageCommand,
    RecordLiveQualificationRunCommand, RecordNativePlatformQualificationCommand, ReleaseCopyExport,
    ReleaseCopyItem, RunDeterministicAcceptanceCommand, SelectManagerWorkspaceTenderCommand,
    StartManagerTenderCommand, SubmissionReleaseApproval, SubmissionReleaseInspection,
    SubmissionReleaseState, TechnicalRiskAcceptance, TenderOfficeMessage,
    TenderOfficeMessageAuthor, TenderOfficeMessageKind, TenderRetentionDecisionCommand,
    TenderRetentionDecisionRecord, TenderRetentionState, TrashedTenderDecisionCommand,
    TrashedTenderRecord, TrashedTenderState, WorkspaceActionKind, WorkspaceCurrentAction,
    WorkspaceFilesSummary, WorkspaceTeamSummary, WorkspaceWorkSummary,
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
    TenderOfficeMessageAuthor::export_all(&config)?;
    TenderOfficeMessageKind::export_all(&config)?;
    TenderOfficeMessage::export_all(&config)?;
    ManagerConversation::export_all(&config)?;
    ManagerWorkspaceTender::export_all(&config)?;
    WorkspaceActionKind::export_all(&config)?;
    WorkspaceCurrentAction::export_all(&config)?;
    WorkspaceWorkSummary::export_all(&config)?;
    WorkspaceFilesSummary::export_all(&config)?;
    WorkspaceTeamSummary::export_all(&config)?;
    ManagerWorkspaceProjection::export_all(&config)?;
    Ok(())
}
