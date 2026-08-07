import { invoke } from "@tauri-apps/api/core";

import type { AgentRunInspection } from "./bindings/AgentRunInspection";
import type { ChooseTenderPackageCommand } from "./bindings/ChooseTenderPackageCommand";
import type { ConfirmSourceRelationshipCommand } from "./bindings/ConfirmSourceRelationshipCommand";
import type { CreateTenderCommand } from "./bindings/CreateTenderCommand";
import type { DocumentRegister } from "./bindings/DocumentRegister";
import type { DocumentParseResult } from "./bindings/DocumentParseResult";
import type { EvidenceDocument } from "./bindings/EvidenceDocument";
import type { EvidenceSearchResult } from "./bindings/EvidenceSearchResult";
import type { InterruptAgentRunCommand } from "./bindings/InterruptAgentRunCommand";
import type { OpenTenderCommand } from "./bindings/OpenTenderCommand";
import type { ParseSourceArtifactCommand } from "./bindings/ParseSourceArtifactCommand";
import type { ReviseTenderCommand } from "./bindings/ReviseTenderCommand";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { RunBootstrapAgentCommand } from "./bindings/RunBootstrapAgentCommand";
import type { SearchEvidenceCommand } from "./bindings/SearchEvidenceCommand";
import type { SetupOutcome } from "./bindings/SetupOutcome";
import type { SourceRelationshipKind } from "./bindings/SourceRelationshipKind";
import type { TenderSummary } from "./bindings/TenderSummary";
import type { TenderPackageImportResult } from "./bindings/TenderPackageImportResult";
import type { TenderPackageSourceKind } from "./bindings/TenderPackageSourceKind";

export function ensureQuantixSetup(): Promise<SetupOutcome> {
  return invoke<SetupOutcome>("ensure_quantix_setup");
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

export function listTenders(): Promise<TenderSummary[]> {
  return invoke<TenderSummary[]>("list_tenders");
}

export function openTender(tenderId: string): Promise<TenderSummary> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<TenderSummary>("open_tender", {
    command,
  });
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

export function inspectAgentRuns(
  tenderId: string,
): Promise<AgentRunInspection[]> {
  const command: OpenTenderCommand = { tender_id: tenderId };
  return invoke<AgentRunInspection[]>("inspect_agent_runs", { command });
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
