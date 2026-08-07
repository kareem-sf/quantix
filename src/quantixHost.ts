import { invoke } from "@tauri-apps/api/core";

import type { ChooseTenderPackageCommand } from "./bindings/ChooseTenderPackageCommand";
import type { ConfirmSourceRelationshipCommand } from "./bindings/ConfirmSourceRelationshipCommand";
import type { CreateTenderCommand } from "./bindings/CreateTenderCommand";
import type { DocumentRegister } from "./bindings/DocumentRegister";
import type { OpenTenderCommand } from "./bindings/OpenTenderCommand";
import type { ReviseTenderCommand } from "./bindings/ReviseTenderCommand";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
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
