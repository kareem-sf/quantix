import { invoke } from "@tauri-apps/api/core";

import type { ApproveCalculationRuleCommand } from "./bindings/ApproveCalculationRuleCommand";
import type { ApproveBasisOfEstimateCommand } from "./bindings/ApproveBasisOfEstimateCommand";
import type { ApproveCommercialStrategyCommand } from "./bindings/ApproveCommercialStrategyCommand";
import type { ApprovePricedCostBaselineCommand } from "./bindings/ApprovePricedCostBaselineCommand";
import type { ApprovePricingAdjustmentCommand } from "./bindings/ApprovePricingAdjustmentCommand";
import type { ApproveTenderPriceCommand } from "./bindings/ApproveTenderPriceCommand";
import type { BoqTableDesignation } from "./bindings/BoqTableDesignation";
import type { DesignateBoqTableCommand } from "./bindings/DesignateBoqTableCommand";
import type { BasisOfEstimateReviewResult } from "./bindings/BasisOfEstimateReviewResult";
import type { BasisOfEstimateVersion } from "./bindings/BasisOfEstimateVersion";
import type { ApproveControlledBoqCalculationRunCommand } from "./bindings/ApproveControlledBoqCalculationRunCommand";
import type { CostEstimatorBasisResult } from "./bindings/CostEstimatorBasisResult";
import type { CostEstimatorCalculationResult } from "./bindings/CostEstimatorCalculationResult";
import type { CreateCalculationScenarioCommand } from "./bindings/CreateCalculationScenarioCommand";
import type { CreateCommercialStrategyCommand } from "./bindings/CreateCommercialStrategyCommand";
import type { CreatePricedCostBaselineCommand } from "./bindings/CreatePricedCostBaselineCommand";
import type { CreatePricingAdjustmentCommand } from "./bindings/CreatePricingAdjustmentCommand";
import type { CreatePricingScenarioCommand } from "./bindings/CreatePricingScenarioCommand";
import type { CalculationRuleReviewResult } from "./bindings/CalculationRuleReviewResult";
import type { CalculationRuleVersion } from "./bindings/CalculationRuleVersion";
import type { CalculationScenarioVersion } from "./bindings/CalculationScenarioVersion";
import type { CalculationWorkspaceInspection } from "./bindings/CalculationWorkspaceInspection";
import type { ChangeAssessment } from "./bindings/ChangeAssessment";
import type { ChangeAssessmentClassification } from "./bindings/ChangeAssessmentClassification";
import type { ChangeAssessmentPage } from "./bindings/ChangeAssessmentPage";
import type { DecisionCockpit } from "./bindings/DecisionCockpit";
import type { InspectDecisionCockpitCommand } from "./bindings/InspectDecisionCockpitCommand";
import type { ControlledBoqCalculationRun } from "./bindings/ControlledBoqCalculationRun";
import type { InspectCalculationWorkspaceCommand } from "./bindings/InspectCalculationWorkspaceCommand";
import type { InspectChangeAssessmentsCommand } from "./bindings/InspectChangeAssessmentsCommand";
import type { DecideChangeAssessmentCommand } from "./bindings/DecideChangeAssessmentCommand";
import type { InspectPricingWorkspaceCommand } from "./bindings/InspectPricingWorkspaceCommand";
import type { PricingAdjustmentVersion } from "./bindings/PricingAdjustmentVersion";
import type { PricingScenarioVersion } from "./bindings/PricingScenarioVersion";
import type { PricingWorkspaceInspection } from "./bindings/PricingWorkspaceInspection";
import type { PricedCostBaselineReviewResult } from "./bindings/PricedCostBaselineReviewResult";
import type { PricedCostBaselineVersion } from "./bindings/PricedCostBaselineVersion";
import type { CommercialStrategy } from "./bindings/CommercialStrategy";
import type { EstimateWorkspaceInspection } from "./bindings/EstimateWorkspaceInspection";
import type { InspectEstimateWorkspaceCommand } from "./bindings/InspectEstimateWorkspaceCommand";
import type { ProposeBoqCalculationRuleCommand } from "./bindings/ProposeBoqCalculationRuleCommand";
import type { RunCalculationRuleReviewCommand } from "./bindings/RunCalculationRuleReviewCommand";
import type { RunPricedCostBaselineReviewCommand } from "./bindings/RunPricedCostBaselineReviewCommand";
import type { RunPricingAdjustmentReviewCommand } from "./bindings/RunPricingAdjustmentReviewCommand";
import type { PricingAdjustmentReviewResult } from "./bindings/PricingAdjustmentReviewResult";
import type { SelectPricingScenarioCommand } from "./bindings/SelectPricingScenarioCommand";
import type { RunBasisOfEstimateReviewCommand } from "./bindings/RunBasisOfEstimateReviewCommand";
import type { RunCostEstimatorBasisCommand } from "./bindings/RunCostEstimatorBasisCommand";
import type { RunCostEstimatorCalculationCommand } from "./bindings/RunCostEstimatorCalculationCommand";
import type { AgentRunInspection } from "./bindings/AgentRunInspection";
import type { AgentRunActivity } from "./bindings/AgentRunActivity";
import type { AgentRunHistoryPage } from "./bindings/AgentRunHistoryPage";
import type { InspectAgentRunCommand } from "./bindings/InspectAgentRunCommand";
import type { InspectAgentRunHistoryCommand } from "./bindings/InspectAgentRunHistoryCommand";
import type { ActivateTenderProductionCommand } from "./bindings/ActivateTenderProductionCommand";
import type { ApproveProductionFindingExceptionCommand } from "./bindings/ApproveProductionFindingExceptionCommand";
import type { ApproveExternalRfiForIssueCommand } from "./bindings/ApproveExternalRfiForIssueCommand";
import type { AgentRunRecoveryDecision } from "./bindings/AgentRunRecoveryDecision";
import type { AgentRunRecoveryDisposition } from "./bindings/AgentRunRecoveryDisposition";
import type { BidDecisionPackageInspection } from "./bindings/BidDecisionPackageInspection";
import type { BidDecisionApprovalDecision } from "./bindings/BidDecisionApprovalDecision";
import type { BidDecisionApprovalHistoryPage } from "./bindings/BidDecisionApprovalHistoryPage";
import type { BidDecisionApprovalResult } from "./bindings/BidDecisionApprovalResult";
import type { BidDecisionApprovalInvalidationResult } from "./bindings/BidDecisionApprovalInvalidationResult";
import type { BidDecisionReturnReworkResult } from "./bindings/BidDecisionReturnReworkResult";
import type { BidDecisionPackageRecordCategory } from "./bindings/BidDecisionPackageRecordCategory";
import type { BidDecisionPackageRecordPage } from "./bindings/BidDecisionPackageRecordPage";
import type { BidDecisionPackageReviewResult } from "./bindings/BidDecisionPackageReviewResult";
import type { ChooseTenderPackageCommand } from "./bindings/ChooseTenderPackageCommand";
import type { ConfirmSourceRelationshipCommand } from "./bindings/ConfirmSourceRelationshipCommand";
import type { ComplianceDispositionUpdate } from "./bindings/ComplianceDispositionUpdate";
import type { ComplianceMatrixPage } from "./bindings/ComplianceMatrixPage";
import type { ComposeTenderOfficeCommand } from "./bindings/ComposeTenderOfficeCommand";
import type { CreateBidDecisionPackageCommand } from "./bindings/CreateBidDecisionPackageCommand";
import type { DecideBidDecisionPackageCommand } from "./bindings/DecideBidDecisionPackageCommand";
import type { CreateTenderBackupCommand } from "./bindings/CreateTenderBackupCommand";
import type { CreateTenderCommand } from "./bindings/CreateTenderCommand";
import type { CreateTenderEngineerEntryCommand } from "./bindings/CreateTenderEngineerEntryCommand";
import type { CreateTenderQueryCommand } from "./bindings/CreateTenderQueryCommand";
import type { CreateExternalRfiDraftCommand } from "./bindings/CreateExternalRfiDraftCommand";
import type { DocumentRegister } from "./bindings/DocumentRegister";
import type { DocumentParseResult } from "./bindings/DocumentParseResult";
import type { DecideTenderRecordCommand } from "./bindings/DecideTenderRecordCommand";
import type { DecideTenderQueryTreatmentCommand } from "./bindings/DecideTenderQueryTreatmentCommand";
import type { DecideWorkPlanProposalCommand } from "./bindings/DecideWorkPlanProposalCommand";
import type { EvidenceDocument } from "./bindings/EvidenceDocument";
import type { EvidenceSearchResult } from "./bindings/EvidenceSearchResult";
import type { ExportApprovedExternalRfiCommand } from "./bindings/ExportApprovedExternalRfiCommand";
import type { ExternalRfiDraft } from "./bindings/ExternalRfiDraft";
import type { ExternalRfiEligibleQueryPage } from "./bindings/ExternalRfiEligibleQueryPage";
import type { ExternalRfiExportRecord } from "./bindings/ExternalRfiExportRecord";
import type { ExternalRfiPage } from "./bindings/ExternalRfiPage";
import type { ExternalRfiResponseCandidatePage } from "./bindings/ExternalRfiResponseCandidatePage";
import type { ExternalRfiReviewResult } from "./bindings/ExternalRfiReviewResult";
import type { InterruptAgentRunCommand } from "./bindings/InterruptAgentRunCommand";
import type { InspectTenderRecordsCommand } from "./bindings/InspectTenderRecordsCommand";
import type { InspectTenderQueriesCommand } from "./bindings/InspectTenderQueriesCommand";
import type { InspectExternalRfisCommand } from "./bindings/InspectExternalRfisCommand";
import type { InspectExternalRfiEligibleQueriesCommand } from "./bindings/InspectExternalRfiEligibleQueriesCommand";
import type { InspectExternalRfiResponseCandidatesCommand } from "./bindings/InspectExternalRfiResponseCandidatesCommand";
import type { InterpretExternalRfiResponseCommand } from "./bindings/InterpretExternalRfiResponseCommand";
import type { InspectBidDecisionPackageRecordsCommand } from "./bindings/InspectBidDecisionPackageRecordsCommand";
import type { InspectBidDecisionApprovalHistoryCommand } from "./bindings/InspectBidDecisionApprovalHistoryCommand";
import type { InvalidateBidDecisionApprovalCommand } from "./bindings/InvalidateBidDecisionApprovalCommand";
import type { InspectComplianceMatrixCommand } from "./bindings/InspectComplianceMatrixCommand";
import type { InspectProductionTaskReviewCommand } from "./bindings/InspectProductionTaskReviewCommand";
import type { ManagerCapabilityDemandInput } from "./bindings/ManagerCapabilityDemandInput";
import type { OpenTenderCommand } from "./bindings/OpenTenderCommand";
import type { ParseSourceArtifactCommand } from "./bindings/ParseSourceArtifactCommand";
import type { PrepareTenderRecoveryCommand } from "./bindings/PrepareTenderRecoveryCommand";
import type { ReviseTenderCommand } from "./bindings/ReviseTenderCommand";
import type { ReviseTenderQueryCommand } from "./bindings/ReviseTenderQueryCommand";
import type { ReviseExternalRfiDraftCommand } from "./bindings/ReviseExternalRfiDraftCommand";
import type { ReviseWorkPlanProposalCommand } from "./bindings/ReviseWorkPlanProposalCommand";
import type { ResolveIndeterminateAgentRunCommand } from "./bindings/ResolveIndeterminateAgentRunCommand";
import type { ResolveBidDecisionReturnReworkCommand } from "./bindings/ResolveBidDecisionReturnReworkCommand";
import type { ResolveTenderRecoveryCommand } from "./bindings/ResolveTenderRecoveryCommand";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { RunBootstrapAgentCommand } from "./bindings/RunBootstrapAgentCommand";
import type { RunProductionTaskCommand } from "./bindings/RunProductionTaskCommand";
import type { ProductionTaskRunResult } from "./bindings/ProductionTaskRunResult";
import type { ProductionTaskReviewInspection } from "./bindings/ProductionTaskReviewInspection";
import type { TenderProductionInspection } from "./bindings/TenderProductionInspection";
import type { RunBidDecisionPackageReviewCommand } from "./bindings/RunBidDecisionPackageReviewCommand";
import type { RunExternalRfiReviewCommand } from "./bindings/RunExternalRfiReviewCommand";
import type { RegisterExternalRfiResponseCommand } from "./bindings/RegisterExternalRfiResponseCommand";
import type { RunTenderRecordExtractionCommand } from "./bindings/RunTenderRecordExtractionCommand";
import type { RunTenderRecordReviewCommand } from "./bindings/RunTenderRecordReviewCommand";
import type { SearchEvidenceCommand } from "./bindings/SearchEvidenceCommand";
import type { SetupOutcome } from "./bindings/SetupOutcome";
import type { SourceRelationshipKind } from "./bindings/SourceRelationshipKind";
import type { TenderSummary } from "./bindings/TenderSummary";
import type { TenderCatalogueEntry } from "./bindings/TenderCatalogueEntry";
import type { TenderBackupRecord } from "./bindings/TenderBackupRecord";
import type { TenderIntegrityReport } from "./bindings/TenderIntegrityReport";
import type { TenderEvidenceReference } from "./bindings/TenderEvidenceReference";
import type { TenderPackageImportResult } from "./bindings/TenderPackageImportResult";
import type { TenderPackageSourceKind } from "./bindings/TenderPackageSourceKind";
import type { TenderRecoveryDecision } from "./bindings/TenderRecoveryDecision";
import type { TenderRecoveryRecord } from "./bindings/TenderRecoveryRecord";
import type { TenderRecordDecisionResult } from "./bindings/TenderRecordDecisionResult";
import type { TenderRecordAuthority } from "./bindings/TenderRecordAuthority";
import type { TenderRecordAuthorityReference } from "./bindings/TenderRecordAuthorityReference";
import type { TenderRecordEngineerDecisionKind } from "./bindings/TenderRecordEngineerDecisionKind";
import type { TenderRecordExtractionResult } from "./bindings/TenderRecordExtractionResult";
import type { TenderRecordPage } from "./bindings/TenderRecordPage";
import type { TenderRecordReviewResult } from "./bindings/TenderRecordReviewResult";
import type { TenderQuery } from "./bindings/TenderQuery";
import type { TenderQueryPage } from "./bindings/TenderQueryPage";
import type { WorkPlanDecision } from "./bindings/WorkPlanDecision";
import type { WorkPlanProposalInspection } from "./bindings/WorkPlanProposalInspection";
import type { WorkPlanRevisionAction } from "./bindings/WorkPlanRevisionAction";
import type { AssembleCoordinatedBidBaselineCommand } from "./bindings/AssembleCoordinatedBidBaselineCommand";
import type { CoordinatedBidBaseline } from "./bindings/CoordinatedBidBaseline";
import type { CoordinatedBidBaselineDecision } from "./bindings/CoordinatedBidBaselineDecision";
import type { CoordinatedBidBaselinePage } from "./bindings/CoordinatedBidBaselinePage";
import type { DecideCoordinatedBidBaselineCommand } from "./bindings/DecideCoordinatedBidBaselineCommand";
import type { InspectCoordinatedBidBaselinesCommand } from "./bindings/InspectCoordinatedBidBaselinesCommand";
import type { DecideUpdateCommand } from "./bindings/DecideUpdateCommand";
import type { InstallUpdateCommand } from "./bindings/InstallUpdateCommand";
import type { UpdateActionCommand } from "./bindings/UpdateActionCommand";
import type { UpdateDecision } from "./bindings/UpdateDecision";
import type { UpdateStatus } from "./bindings/UpdateStatus";

export function ensureQuantixSetup(): Promise<SetupOutcome> {
  return invoke<SetupOutcome>("ensure_quantix_setup");
}

export function checkQuantixUpdate(): Promise<UpdateStatus> {
  return invoke<UpdateStatus>("check_quantix_update");
}

export function validateQuantixUpdateRestart(): Promise<UpdateStatus> {
  return invoke<UpdateStatus>("validate_quantix_update_restart");
}

export function decideQuantixUpdate(
  updateId: string,
  decision: UpdateDecision,
  rationale: string,
): Promise<UpdateStatus> {
  const command: DecideUpdateCommand = {
    update_id: updateId,
    decision,
    rationale,
  };
  return invoke<UpdateStatus>("decide_quantix_update", { command });
}

export function restartQuantixAfterUpdate(
  updateId: string,
): Promise<UpdateStatus> {
  const command: UpdateActionCommand = { update_id: updateId };
  return invoke<UpdateStatus>("restart_quantix_after_update", { command });
}

export function retryQuantixUpdateRepair(
  updateId: string,
): Promise<UpdateStatus> {
  const command: UpdateActionCommand = { update_id: updateId };
  return invoke<UpdateStatus>("retry_quantix_update_repair", { command });
}

export function installQuantixUpdate(updateId: string): Promise<UpdateStatus> {
  const command: InstallUpdateCommand = { update_id: updateId };
  return invoke<UpdateStatus>("install_quantix_update", { command });
}

export function inspectRuntimeReadiness(): Promise<RuntimeReadiness> {
  return invoke<RuntimeReadiness>("inspect_runtime_readiness");
}

export function repairRuntimeReadiness(): Promise<RuntimeReadiness> {
  return invoke<RuntimeReadiness>("repair_runtime_readiness");
}

export function cancelRuntimePreparation(): Promise<boolean> {
  return invoke<boolean>("cancel_runtime_preparation");
}

export function createTender(name: string): Promise<TenderSummary> {
  const command: CreateTenderCommand = { name };
  return invoke<TenderSummary>("create_tender", { command });
}

export function listTenders(): Promise<TenderCatalogueEntry[]> {
  return invoke<TenderCatalogueEntry[]>("list_tenders");
}

export function openTender(tenderId: string): Promise<TenderSummary> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<TenderSummary>("open_tender", {
    command,
  });
}

export function inspectTenderIntegrity(
  tenderId: string,
): Promise<TenderIntegrityReport> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<TenderIntegrityReport>("inspect_tender_integrity", { command });
}

export function createTenderBackup(
  tenderId: string,
): Promise<TenderBackupRecord> {
  const command: CreateTenderBackupCommand = { tender_id: tenderId };
  return invoke<TenderBackupRecord>("create_tender_backup", { command });
}

export function inspectTenderBackups(
  tenderId: string,
): Promise<TenderBackupRecord[]> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<TenderBackupRecord[]>("inspect_tender_backups", { command });
}

export function prepareTenderRecovery(
  tenderId: string,
  backupId: string,
): Promise<TenderRecoveryRecord> {
  const command: PrepareTenderRecoveryCommand = {
    tender_id: tenderId,
    backup_id: backupId,
  };
  return invoke<TenderRecoveryRecord>("prepare_tender_recovery", { command });
}

export function inspectTenderRecoveries(
  tenderId: string,
): Promise<TenderRecoveryRecord[]> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<TenderRecoveryRecord[]>("inspect_tender_recoveries", {
    command,
  });
}

export function resolveTenderRecovery(
  tenderId: string,
  recoveryId: string,
  decision: TenderRecoveryDecision,
  rationale: string,
): Promise<TenderRecoveryRecord> {
  const command: ResolveTenderRecoveryCommand = {
    tender_id: tenderId,
    recovery_id: recoveryId,
    decision,
    rationale,
  };
  return invoke<TenderRecoveryRecord>("resolve_tender_recovery", { command });
}

export function reviseTender(
  tenderId: string,
  name: string,
): Promise<TenderSummary> {
  const command: ReviseTenderCommand = { tender_id: tenderId, name };
  return invoke<TenderSummary>("revise_tender", {
    command,
  });
}

export function inspectTenderQueries(
  tenderId: string,
  cursor: string | null = null,
  limit = 8,
): Promise<TenderQueryPage> {
  const command: InspectTenderQueriesCommand = {
    tender_id: tenderId,
    cursor,
    limit,
  };
  return invoke<TenderQueryPage>("inspect_tender_queries", { command });
}

export function createTenderQuery(
  command: CreateTenderQueryCommand,
): Promise<TenderQuery> {
  return invoke<TenderQuery>("create_tender_query", { command });
}

export function reviseTenderQuery(
  command: ReviseTenderQueryCommand,
): Promise<TenderQuery> {
  return invoke<TenderQuery>("revise_tender_query", { command });
}

export function decideTenderQueryTreatment(
  command: DecideTenderQueryTreatmentCommand,
): Promise<TenderQuery> {
  return invoke<TenderQuery>("decide_tender_query_treatment", { command });
}

export function inspectExternalRfis(
  tenderId: string,
  cursor: string | null = null,
  limit = 8,
): Promise<ExternalRfiPage> {
  const command: InspectExternalRfisCommand = {
    tender_id: tenderId,
    cursor,
    limit,
  };
  return invoke<ExternalRfiPage>("inspect_external_rfis", { command });
}

export function inspectExternalRfiEligibleQueries(
  tenderId: string,
  cursor: string | null = null,
  limit = 8,
): Promise<ExternalRfiEligibleQueryPage> {
  const command: InspectExternalRfiEligibleQueriesCommand = {
    tender_id: tenderId,
    cursor,
    limit,
  };
  return invoke<ExternalRfiEligibleQueryPage>(
    "inspect_external_rfi_eligible_queries",
    { command },
  );
}

export function inspectExternalRfiResponseCandidates(
  tenderId: string,
  approvalId: string,
  cursor: string | null = null,
  limit = 64,
): Promise<ExternalRfiResponseCandidatePage> {
  const command: InspectExternalRfiResponseCandidatesCommand = {
    tender_id: tenderId,
    approval_id: approvalId,
    cursor,
    limit,
  };
  return invoke<ExternalRfiResponseCandidatePage>(
    "inspect_external_rfi_response_candidates",
    { command },
  );
}

export function createExternalRfiDraft(
  command: CreateExternalRfiDraftCommand,
): Promise<ExternalRfiDraft> {
  return invoke<ExternalRfiDraft>("create_external_rfi_draft", { command });
}

export function reviseExternalRfiDraft(
  command: ReviseExternalRfiDraftCommand,
): Promise<ExternalRfiDraft> {
  return invoke<ExternalRfiDraft>("revise_external_rfi_draft", { command });
}

export function runExternalRfiReview(
  command: RunExternalRfiReviewCommand,
): Promise<ExternalRfiReviewResult> {
  return invoke<ExternalRfiReviewResult>("run_external_rfi_review", {
    command,
  });
}

export function approveExternalRfiForIssue(
  command: ApproveExternalRfiForIssueCommand,
): Promise<ExternalRfiDraft> {
  return invoke<ExternalRfiDraft>("approve_external_rfi_for_issue", {
    command,
  });
}

export function exportApprovedExternalRfi(
  command: ExportApprovedExternalRfiCommand,
): Promise<ExternalRfiExportRecord> {
  return invoke<ExternalRfiExportRecord>("export_approved_external_rfi", {
    command,
  });
}

export function registerExternalRfiResponse(
  command: RegisterExternalRfiResponseCommand,
): Promise<ExternalRfiDraft> {
  return invoke<ExternalRfiDraft>("register_external_rfi_response", {
    command,
  });
}

export function interpretExternalRfiResponse(
  command: InterpretExternalRfiResponseCommand,
): Promise<ExternalRfiDraft> {
  return invoke<ExternalRfiDraft>("interpret_external_rfi_response", {
    command,
  });
}

export function proposeBoqCalculationRule(
  command: ProposeBoqCalculationRuleCommand,
): Promise<CalculationRuleVersion> {
  return invoke<CalculationRuleVersion>("propose_boq_calculation_rule", {
    command,
  });
}

export function runCalculationRuleReview(
  command: RunCalculationRuleReviewCommand,
): Promise<CalculationRuleReviewResult> {
  return invoke<CalculationRuleReviewResult>("run_calculation_rule_review", {
    command,
  });
}

export function approveCalculationRule(
  command: ApproveCalculationRuleCommand,
): Promise<CalculationRuleVersion> {
  return invoke<CalculationRuleVersion>("approve_calculation_rule", {
    command,
  });
}

export function createCalculationScenario(
  command: CreateCalculationScenarioCommand,
): Promise<CalculationScenarioVersion> {
  return invoke<CalculationScenarioVersion>("create_calculation_scenario", {
    command,
  });
}

export function runCostEstimatorCalculation(
  command: RunCostEstimatorCalculationCommand,
): Promise<CostEstimatorCalculationResult> {
  return invoke<CostEstimatorCalculationResult>(
    "run_cost_estimator_calculation",
    {
      command,
    },
  );
}

export function approveControlledBoqCalculationRun(
  command: ApproveControlledBoqCalculationRunCommand,
): Promise<ControlledBoqCalculationRun> {
  return invoke<ControlledBoqCalculationRun>(
    "approve_controlled_boq_calculation_run",
    { command },
  );
}

export function inspectCalculationWorkspace(
  tenderId: string,
  scenarioOffset = 0,
  runOffset = 0,
): Promise<CalculationWorkspaceInspection> {
  const command: InspectCalculationWorkspaceCommand = {
    tender_id: tenderId,
    scenario_offset: scenarioOffset,
    run_offset: runOffset,
  };
  return invoke<CalculationWorkspaceInspection>(
    "inspect_calculation_workspace",
    {
      command,
    },
  );
}

export function runCostEstimatorBasis(
  command: RunCostEstimatorBasisCommand,
): Promise<CostEstimatorBasisResult> {
  return invoke<CostEstimatorBasisResult>("run_cost_estimator_basis", {
    command,
  });
}

export function designateBoqTable(
  command: DesignateBoqTableCommand,
): Promise<BoqTableDesignation> {
  return invoke<BoqTableDesignation>("designate_boq_table", { command });
}

export function runBasisOfEstimateReview(
  command: RunBasisOfEstimateReviewCommand,
): Promise<BasisOfEstimateReviewResult> {
  return invoke<BasisOfEstimateReviewResult>("run_basis_of_estimate_review", {
    command,
  });
}

export function approveBasisOfEstimate(
  command: ApproveBasisOfEstimateCommand,
): Promise<BasisOfEstimateVersion> {
  return invoke<BasisOfEstimateVersion>("approve_basis_of_estimate", {
    command,
  });
}

export function inspectEstimateWorkspace(
  tenderId: string,
  basisOffset = 0,
  boqCandidateCursor: string | null = null,
): Promise<EstimateWorkspaceInspection> {
  const command: InspectEstimateWorkspaceCommand = {
    tender_id: tenderId,
    basis_offset: basisOffset,
    boq_candidate_cursor: boqCandidateCursor,
  };
  return invoke<EstimateWorkspaceInspection>("inspect_estimate_workspace", {
    command,
  });
}

export function createPricedCostBaseline(
  command: CreatePricedCostBaselineCommand,
): Promise<PricedCostBaselineVersion> {
  return invoke<PricedCostBaselineVersion>("create_priced_cost_baseline", {
    command,
  });
}

export function runPricedCostBaselineReview(
  command: RunPricedCostBaselineReviewCommand,
): Promise<PricedCostBaselineReviewResult> {
  return invoke<PricedCostBaselineReviewResult>(
    "run_priced_cost_baseline_review",
    { command },
  );
}

export function approvePricedCostBaseline(
  command: ApprovePricedCostBaselineCommand,
): Promise<PricedCostBaselineVersion> {
  return invoke<PricedCostBaselineVersion>("approve_priced_cost_baseline", {
    command,
  });
}

export function createPricingAdjustment(
  command: CreatePricingAdjustmentCommand,
): Promise<PricingAdjustmentVersion> {
  return invoke<PricingAdjustmentVersion>("create_pricing_adjustment", {
    command,
  });
}

export function runPricingAdjustmentReview(
  command: RunPricingAdjustmentReviewCommand,
): Promise<PricingAdjustmentReviewResult> {
  return invoke<PricingAdjustmentReviewResult>(
    "run_pricing_adjustment_review",
    { command },
  );
}

export function approvePricingAdjustment(
  command: ApprovePricingAdjustmentCommand,
): Promise<PricingAdjustmentVersion> {
  return invoke<PricingAdjustmentVersion>("approve_pricing_adjustment", {
    command,
  });
}

export function createCommercialStrategy(
  command: CreateCommercialStrategyCommand,
): Promise<CommercialStrategy> {
  return invoke<CommercialStrategy>("create_commercial_strategy", { command });
}

export function approveCommercialStrategy(
  command: ApproveCommercialStrategyCommand,
): Promise<CommercialStrategy> {
  return invoke<CommercialStrategy>("approve_commercial_strategy", {
    command,
  });
}

export function createPricingScenario(
  command: CreatePricingScenarioCommand,
): Promise<PricingScenarioVersion> {
  return invoke<PricingScenarioVersion>("create_pricing_scenario", { command });
}

export function selectPricingScenario(
  command: SelectPricingScenarioCommand,
): Promise<PricingScenarioVersion> {
  return invoke<PricingScenarioVersion>("select_pricing_scenario", { command });
}

export function approveTenderPrice(
  command: ApproveTenderPriceCommand,
): Promise<PricingScenarioVersion> {
  return invoke<PricingScenarioVersion>("approve_tender_price", { command });
}

export function inspectPricingWorkspace(
  tenderId: string,
): Promise<PricingWorkspaceInspection> {
  const command: InspectPricingWorkspaceCommand = { tender_id: tenderId };
  return invoke<PricingWorkspaceInspection>("inspect_pricing_workspace", {
    command,
  });
}

export function chooseAndImportTenderPackage(
  tenderId: string,
  sourceKind: TenderPackageSourceKind,
): Promise<TenderPackageImportResult | null> {
  const command: ChooseTenderPackageCommand = {
    tender_id: tenderId,
    source_kind: sourceKind,
  };
  return invoke<TenderPackageImportResult | null>(
    "choose_and_import_tender_package",
    { command },
  );
}

export function inspectDocumentRegister(
  tenderId: string,
): Promise<DocumentRegister> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<DocumentRegister>("inspect_document_register", { command });
}

export function confirmSourceRelationship(
  tenderId: string,
  priorArtifactId: string,
  priorVersion: number,
  replacementArtifactId: string,
  replacementVersion: number,
  relationshipKind: SourceRelationshipKind,
): Promise<DocumentRegister> {
  const command: ConfirmSourceRelationshipCommand = {
    tender_id: tenderId,
    prior_artifact_id: priorArtifactId,
    prior_version: priorVersion,
    replacement_artifact_id: replacementArtifactId,
    replacement_version: replacementVersion,
    relationship_kind: relationshipKind,
  };
  return invoke<DocumentRegister>("confirm_source_relationship", { command });
}

function parseTarget(
  tenderId: string,
  artifactId: string,
  version: number,
): ParseSourceArtifactCommand {
  return {
    tender_id: tenderId,
    artifact_id: artifactId,
    version,
  };
}

export function parseSourceArtifact(
  tenderId: string,
  artifactId: string,
  version: number,
): Promise<DocumentParseResult> {
  return invoke<DocumentParseResult>("parse_source_artifact", {
    command: parseTarget(tenderId, artifactId, version),
  });
}

export function cancelSourceArtifactParse(
  tenderId: string,
  artifactId: string,
  version: number,
): Promise<boolean> {
  return invoke<boolean>("cancel_source_artifact_parse", {
    command: parseTarget(tenderId, artifactId, version),
  });
}

export function inspectEvidence(
  tenderId: string,
  artifactId: string,
  version: number,
): Promise<EvidenceDocument> {
  return invoke<EvidenceDocument>("inspect_evidence", {
    command: parseTarget(tenderId, artifactId, version),
  });
}

export function inspectDecisionCockpit(
  tenderId: string,
): Promise<DecisionCockpit> {
  const command: InspectDecisionCockpitCommand = { tender_id: tenderId };
  return invoke<DecisionCockpit>("inspect_decision_cockpit", { command });
}

export function searchEvidence(
  tenderId: string,
  query: string,
): Promise<EvidenceSearchResult> {
  const command: SearchEvidenceCommand = { tender_id: tenderId, query };
  return invoke<EvidenceSearchResult>("search_evidence", { command });
}

export function runBootstrapAgent(
  tenderId: string,
  retryOfRunId: string | null = null,
): Promise<AgentRunInspection> {
  const command: RunBootstrapAgentCommand = {
    tender_id: tenderId,
    retry_of_run_id: retryOfRunId,
  };
  return invoke<AgentRunInspection>("run_bootstrap_agent", { command });
}

export function runTenderRecordExtraction(
  tenderId: string,
  evidence: TenderEvidenceReference[],
  authorities: TenderRecordAuthorityReference[],
): Promise<TenderRecordExtractionResult> {
  const command: RunTenderRecordExtractionCommand = {
    tender_id: tenderId,
    evidence,
    authorities,
  };
  return invoke<TenderRecordExtractionResult>("run_tender_record_extraction", {
    command,
  });
}

export function createTenderEngineerEntry(
  tenderId: string,
  value: string,
  description: string,
): Promise<TenderRecordAuthority> {
  const command: CreateTenderEngineerEntryCommand = {
    tender_id: tenderId,
    value,
    description,
  };
  return invoke<TenderRecordAuthority>("create_tender_engineer_entry", {
    command,
  });
}

export function inspectTenderRecordAuthorities(
  tenderId: string,
): Promise<TenderRecordAuthority[]> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<TenderRecordAuthority[]>("inspect_tender_record_authorities", {
    command,
  });
}

export function runTenderRecordReview(
  tenderId: string,
  recordId: string,
  version: number,
): Promise<TenderRecordReviewResult> {
  const command: RunTenderRecordReviewCommand = {
    tender_id: tenderId,
    record_id: recordId,
    version,
  };
  return invoke<TenderRecordReviewResult>("run_tender_record_review", {
    command,
  });
}

export function inspectTenderRecords(
  tenderId: string,
  cursor: string | null,
  limit: number,
): Promise<TenderRecordPage> {
  const command: InspectTenderRecordsCommand = {
    tender_id: tenderId,
    cursor,
    limit,
  };
  return invoke<TenderRecordPage>("inspect_tender_records", {
    command,
  });
}

export function decideTenderRecord(
  tenderId: string,
  recordId: string,
  version: number,
  decision: TenderRecordEngineerDecisionKind,
  rationale: string,
): Promise<TenderRecordDecisionResult> {
  const command: DecideTenderRecordCommand = {
    tender_id: tenderId,
    record_id: recordId,
    version,
    decision,
    rationale,
  };
  return invoke<TenderRecordDecisionResult>("decide_tender_record", {
    command,
  });
}

export function createBidDecisionPackage(
  tenderId: string,
  baseVersion: number | null,
  dispositionUpdates: ComplianceDispositionUpdate[],
  managerCapabilityDemands: ManagerCapabilityDemandInput[],
): Promise<BidDecisionPackageInspection> {
  const command: CreateBidDecisionPackageCommand = {
    tender_id: tenderId,
    base_version: baseVersion,
    disposition_updates: dispositionUpdates,
    manager_capability_demands: managerCapabilityDemands,
  };
  return invoke<BidDecisionPackageInspection>("create_bid_decision_package", {
    command,
  });
}

export function inspectCurrentBidDecisionPackage(
  tenderId: string,
): Promise<BidDecisionPackageInspection | null> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<BidDecisionPackageInspection | null>(
    "inspect_current_bid_decision_package",
    { command },
  );
}

export function composeTenderOffice(
  tenderId: string,
): Promise<WorkPlanProposalInspection> {
  const command: ComposeTenderOfficeCommand = { tender_id: tenderId };
  return invoke<WorkPlanProposalInspection>("compose_tender_office", {
    command,
  });
}

export function inspectCurrentWorkPlan(
  tenderId: string,
): Promise<WorkPlanProposalInspection | null> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<WorkPlanProposalInspection | null>(
    "inspect_current_work_plan",
    {
      command,
    },
  );
}

export function reviseWorkPlanProposal(
  tenderId: string,
  planId: string,
  baseVersion: number,
  actions: WorkPlanRevisionAction[],
): Promise<WorkPlanProposalInspection> {
  const command: ReviseWorkPlanProposalCommand = {
    tender_id: tenderId,
    plan_id: planId,
    base_version: baseVersion,
    actions,
  };
  return invoke<WorkPlanProposalInspection>("revise_work_plan_proposal", {
    command,
  });
}

export function decideWorkPlanProposal(
  tenderId: string,
  planId: string,
  version: number,
  decision: WorkPlanDecision,
  rationale: string,
): Promise<WorkPlanProposalInspection> {
  const command: DecideWorkPlanProposalCommand = {
    tender_id: tenderId,
    plan_id: planId,
    version,
    decision,
    rationale,
  };
  return invoke<WorkPlanProposalInspection>("decide_work_plan_proposal", {
    command,
  });
}

export function activateTenderProduction(
  tenderId: string,
  planId: string,
  planVersion: number,
  planManifestSha256: string,
): Promise<TenderProductionInspection> {
  const command: ActivateTenderProductionCommand = {
    tender_id: tenderId,
    plan_id: planId,
    plan_version: planVersion,
    plan_manifest_sha256: planManifestSha256,
  };
  return invoke<TenderProductionInspection>("activate_tender_production", {
    command,
  });
}

export function inspectTenderProduction(
  tenderId: string,
): Promise<TenderProductionInspection | null> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<TenderProductionInspection | null>(
    "inspect_tender_production",
    {
      command,
    },
  );
}

export function runProductionTask(
  tenderId: string,
  productionTaskId: string,
): Promise<ProductionTaskRunResult> {
  const command: RunProductionTaskCommand = {
    tender_id: tenderId,
    production_task_id: productionTaskId,
  };
  return invoke<ProductionTaskRunResult>("run_production_task", { command });
}

export function inspectProductionTaskReview(
  tenderId: string,
  productionTaskId: string,
): Promise<ProductionTaskReviewInspection> {
  const command: InspectProductionTaskReviewCommand = {
    tender_id: tenderId,
    production_task_id: productionTaskId,
  };
  return invoke<ProductionTaskReviewInspection>(
    "inspect_production_task_review",
    { command },
  );
}

export function approveProductionFindingException(
  tenderId: string,
  productionTaskId: string,
  findingId: string,
  reviewId: string,
  artifactId: string,
  artifactVersion: number,
  payloadSha256: string,
  rationale: string,
  consequence: string,
): Promise<ProductionTaskReviewInspection> {
  const command: ApproveProductionFindingExceptionCommand = {
    tender_id: tenderId,
    production_task_id: productionTaskId,
    finding_id: findingId,
    review_id: reviewId,
    artifact_id: artifactId,
    artifact_version: artifactVersion,
    payload_sha256: payloadSha256,
    rationale,
    consequence,
  };
  return invoke<ProductionTaskReviewInspection>(
    "approve_production_finding_exception",
    { command },
  );
}

export function decideBidDecisionPackage(
  tenderId: string,
  packageId: string,
  version: number,
  manifestSha256: string,
  decision: BidDecisionApprovalDecision,
  rationale: string,
  conditions: string[],
  exceptions: string[],
  requiredRework: string[],
): Promise<BidDecisionApprovalResult> {
  const command: DecideBidDecisionPackageCommand = {
    tender_id: tenderId,
    package_id: packageId,
    version,
    manifest_sha256: manifestSha256,
    decision,
    rationale,
    conditions,
    exceptions,
    required_rework: requiredRework,
  };
  return invoke<BidDecisionApprovalResult>("decide_bid_decision_package", {
    command,
  });
}

export function inspectBidDecisionApprovalHistory(
  tenderId: string,
  beforeSequence: number | null,
  limit: number,
): Promise<BidDecisionApprovalHistoryPage> {
  const command: InspectBidDecisionApprovalHistoryCommand = {
    tender_id: tenderId,
    before_sequence: beforeSequence,
    limit,
  };
  return invoke<BidDecisionApprovalHistoryPage>(
    "inspect_bid_decision_approval_history",
    { command },
  );
}

export function resolveBidDecisionReturnRework(
  tenderId: string,
  approvalId: string,
  resolutions: string[],
): Promise<BidDecisionReturnReworkResult> {
  const command: ResolveBidDecisionReturnReworkCommand = {
    tender_id: tenderId,
    approval_id: approvalId,
    resolutions,
  };
  return invoke<BidDecisionReturnReworkResult>(
    "resolve_bid_decision_return_rework",
    { command },
  );
}

export function invalidateBidDecisionApproval(
  tenderId: string,
  approvalId: string,
  approvalSha256: string,
  materialChangeSummary: string,
  affectedAreas: string[],
): Promise<BidDecisionApprovalInvalidationResult> {
  const command: InvalidateBidDecisionApprovalCommand = {
    tender_id: tenderId,
    approval_id: approvalId,
    approval_sha256: approvalSha256,
    material_change_summary: materialChangeSummary,
    affected_areas: affectedAreas,
  };
  return invoke<BidDecisionApprovalInvalidationResult>(
    "invalidate_bid_decision_approval",
    { command },
  );
}

export function inspectComplianceMatrix(
  tenderId: string,
  packageId: string,
  version: number,
  afterOrdinal: number | null,
  limit: number,
): Promise<ComplianceMatrixPage> {
  const command: InspectComplianceMatrixCommand = {
    tender_id: tenderId,
    package_id: packageId,
    version,
    after_ordinal: afterOrdinal,
    limit,
  };
  return invoke<ComplianceMatrixPage>("inspect_compliance_matrix", { command });
}

export function inspectBidDecisionPackageRecords(
  tenderId: string,
  packageId: string,
  version: number,
  category: BidDecisionPackageRecordCategory,
  afterOrdinal: number | null,
  limit: number,
): Promise<BidDecisionPackageRecordPage> {
  const command: InspectBidDecisionPackageRecordsCommand = {
    tender_id: tenderId,
    package_id: packageId,
    version,
    category,
    after_ordinal: afterOrdinal,
    limit,
  };
  return invoke<BidDecisionPackageRecordPage>(
    "inspect_bid_decision_package_records",
    { command },
  );
}

export function runBidDecisionPackageReview(
  tenderId: string,
  packageId: string,
  version: number,
): Promise<BidDecisionPackageReviewResult> {
  const command: RunBidDecisionPackageReviewCommand = {
    tender_id: tenderId,
    package_id: packageId,
    version,
  };
  return invoke<BidDecisionPackageReviewResult>(
    "run_bid_decision_package_review",
    { command },
  );
}

export function inspectAgentRunHistory(
  tenderId: string,
  beforeSequence: bigint | null,
  limit: number,
): Promise<AgentRunHistoryPage> {
  const command: InspectAgentRunHistoryCommand = {
    tender_id: tenderId,
    before_sequence: beforeSequence,
    limit,
  };
  return invoke<AgentRunHistoryPage>("inspect_agent_run_history", { command });
}

export function inspectAgentRun(
  tenderId: string,
  runId: string,
): Promise<AgentRunInspection> {
  const command: InspectAgentRunCommand = {
    tender_id: tenderId,
    run_id: runId,
  };
  return invoke<AgentRunInspection>("inspect_agent_run", { command });
}

export function inspectAgentRunActivity(
  tenderId: string,
): Promise<AgentRunActivity> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<AgentRunActivity>("inspect_agent_run_activity", { command });
}

export function resolveIndeterminateAgentRun(
  tenderId: string,
  runId: string,
  disposition: AgentRunRecoveryDisposition,
  rationale: string,
): Promise<AgentRunRecoveryDecision> {
  const command: ResolveIndeterminateAgentRunCommand = {
    tender_id: tenderId,
    run_id: runId,
    disposition,
    rationale,
  };
  return invoke<AgentRunRecoveryDecision>("resolve_indeterminate_agent_run", {
    command,
  });
}

export function interruptAgentRun(
  tenderId: string,
  runId: string,
): Promise<boolean> {
  const command: InterruptAgentRunCommand = {
    tender_id: tenderId,
    run_id: runId,
  };
  return invoke<boolean>("interrupt_agent_run", { command });
}

export function assembleCoordinatedBidBaseline(
  tenderId: string,
  baseVersion: number | null,
): Promise<CoordinatedBidBaseline> {
  const command: AssembleCoordinatedBidBaselineCommand = {
    tender_id: tenderId,
    base_version: baseVersion,
  };
  return invoke<CoordinatedBidBaseline>("assemble_coordinated_bid_baseline", {
    command,
  });
}

export function inspectCoordinatedBidBaselines(
  tenderId: string,
  beforeVersion: number | null,
  limit: number,
): Promise<CoordinatedBidBaselinePage> {
  const command: InspectCoordinatedBidBaselinesCommand = {
    tender_id: tenderId,
    before_version: beforeVersion,
    limit,
  };
  return invoke<CoordinatedBidBaselinePage>(
    "inspect_coordinated_bid_baselines",
    { command },
  );
}

export function inspectChangeAssessments(
  tenderId: string,
  beforeSequence: number | null,
  limit: number,
): Promise<ChangeAssessmentPage> {
  const command: InspectChangeAssessmentsCommand = {
    tender_id: tenderId,
    before_sequence: beforeSequence,
    limit,
  };
  return invoke<ChangeAssessmentPage>("inspect_change_assessments", {
    command,
  });
}

export function decideChangeAssessment(
  tenderId: string,
  assessmentId: string,
  assessmentManifestSha256: string,
  classification: ChangeAssessmentClassification,
  rationale: string,
): Promise<ChangeAssessment> {
  const command: DecideChangeAssessmentCommand = {
    tender_id: tenderId,
    assessment_id: assessmentId,
    assessment_manifest_sha256: assessmentManifestSha256,
    classification,
    rationale,
  };
  return invoke<ChangeAssessment>("decide_change_assessment", { command });
}

export function decideCoordinatedBidBaseline(
  tenderId: string,
  baselineId: string,
  version: number,
  manifestSha256: string,
  decision: CoordinatedBidBaselineDecision,
  rationale: string,
  conditions: string[],
  exceptions: string[],
): Promise<CoordinatedBidBaseline> {
  const command: DecideCoordinatedBidBaselineCommand = {
    tender_id: tenderId,
    baseline_id: baselineId,
    version,
    manifest_sha256: manifestSha256,
    decision,
    rationale,
    conditions,
    exceptions,
  };
  return invoke<CoordinatedBidBaseline>("decide_coordinated_bid_baseline", {
    command,
  });
}
