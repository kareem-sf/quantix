import { invoke } from "@tauri-apps/api/core";

import type { CreateTenderCommand } from "./bindings/CreateTenderCommand";
import type { OpenTenderCommand } from "./bindings/OpenTenderCommand";
import type { ReviseTenderCommand } from "./bindings/ReviseTenderCommand";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { SetupOutcome } from "./bindings/SetupOutcome";
import type { TenderSummary } from "./bindings/TenderSummary";

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
