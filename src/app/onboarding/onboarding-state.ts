/** Cleared by a factory reset together with every other `quantix.*` key. */
export const ONBOARDING_KEY = "quantix.onboarding.v1";

export function readOnboarded() {
  try {
    return window.localStorage.getItem(ONBOARDING_KEY) !== null;
  } catch {
    // Storage can be unavailable in a locked-down WebView; skip onboarding.
    return true;
  }
}

export function markOnboarded() {
  try {
    window.localStorage.setItem(ONBOARDING_KEY, new Date().toISOString());
  } catch {
    // Without storage, onboarding shows again only while there are no tenders.
  }
}
