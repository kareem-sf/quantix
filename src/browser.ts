import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Schema } from "./api";

// These original clients can open a browser themselves. Start the normal
// desktop browser first, outside the AI process owner's cleanup boundary.
const signInPages: Partial<Record<Schema<"ConnectionRecord">["protocol"], string>> = {
  grok_build: "https://grok.com/",
};

export async function prepareSignInBrowser(protocol: Schema<"ConnectionRecord">["protocol"]) {
  const url = signInPages[protocol];
  if (!url || !isTauri()) return;
  try {
    await invoke("prepare_sign_in_browser", { url });
  } catch (error) {
    throw new Error(typeof error === "string" ? error : "Your default browser could not open. Reopen Quantix to finish updating, then try sign-in again.");
  }
}
