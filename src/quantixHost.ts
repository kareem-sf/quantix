import { invoke } from "@tauri-apps/api/core";

import type { TenderOfficeReadiness } from "./bindings/TenderOfficeReadiness";

export function inspectTenderOfficeReadiness(): Promise<TenderOfficeReadiness> {
  return invoke<TenderOfficeReadiness>("inspect_tender_office_readiness");
}
