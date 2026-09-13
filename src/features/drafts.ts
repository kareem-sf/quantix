import {
  createDraftScope,
  readFormDraft,
  removeFormDraft,
  writeFormDraft,
} from "./useFormDraft";

// Kept as a read-only migration path for the manager composer. New drafts use
// the shared scoped envelope so other forms cannot accidentally share text.
const legacyDraftKey = (tenderId: string) =>
  `quantix.manager-draft.v1:${tenderId}`;
const fields = ["text"] as const;

function scope(tenderId: string) {
  return createDraftScope("manager", tenderId, "instruction", 1);
}

export function readDraft(
  tenderId: string | undefined,
  fallback: string,
): { text: string; error: Error | null } {
  if (!tenderId) return { text: fallback, error: null };
  const stored = readFormDraft<{ text: string }>(scope(tenderId), fields);
  if (stored.value?.text !== undefined) {
    return { text: stored.value.text, error: stored.error };
  }
  if (stored.error) return { text: fallback, error: stored.error };
  try {
    return {
      text: window.localStorage.getItem(legacyDraftKey(tenderId)) ?? fallback,
      error: null,
    };
  } catch {
    return {
      text: fallback,
      error: new Error(
        "Saved drafts could not be read on this device. Keep a copy of your instruction before closing.",
      ),
    };
  }
}

export function saveDraft(
  tenderId: string | undefined,
  text: string,
): Error | null {
  if (!tenderId) return null;
  const result = writeFormDraft(scope(tenderId), { text }, fields);
  if (!result.error && !text) {
    const current = readFormDraft<{ text: string }>(scope(tenderId), fields);
    removeFormDraft(scope(tenderId), current.revision);
  }
  try {
    window.localStorage.removeItem(legacyDraftKey(tenderId));
  } catch {
    return (
      result.error ??
      new Error(
        "Saved drafts could not be removed on this device. Keep a copy before closing.",
      )
    );
  }
  return result.error;
}
