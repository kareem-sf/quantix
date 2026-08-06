import { invoke } from "@tauri-apps/api/core";

import type { QuantixHostStatus } from "./bindings/QuantixHostStatus";

export function inspectQuantixHost(): Promise<QuantixHostStatus> {
  return invoke<QuantixHostStatus>("inspect_quantix_host");
}
