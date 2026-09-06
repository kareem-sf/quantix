const draftKey = (tenderId: string) => `quantix.manager-draft.v1:${tenderId}`;

export function readDraft(
  tenderId: string | undefined,
  fallback: string,
): { text: string; error: Error | null } {
  if (!tenderId) return { text: fallback, error: null };
  try {
    return {
      text: window.localStorage.getItem(draftKey(tenderId)) ?? fallback,
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
  try {
    if (text) window.localStorage.setItem(draftKey(tenderId), text);
    else window.localStorage.removeItem(draftKey(tenderId));
    return null;
  } catch {
    return new Error(
      "This draft could not be saved on this device. Keep a copy before closing.",
    );
  }
}
