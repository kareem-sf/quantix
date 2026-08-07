import { invoke } from "@tauri-apps/api/core";

import type { SetupOutcome } from "./bindings/SetupOutcome";

export function ensureQuantixSetup(): Promise<SetupOutcome> {
  return invoke<SetupOutcome>("ensure_quantix_setup");
}
