import {
  useCallback,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";

/** JSON data is deliberately narrower than an arbitrary API response. */
export type DraftJson =
  string | number | boolean | null | DraftJson[] | { [key: string]: DraftJson };

export type DraftScope = {
  namespace: string;
  tenderId: string;
  form: string;
  version: string;
};

export type DraftFields<T extends Record<string, unknown>> = readonly (keyof T &
  string)[];

type DraftEnvelope = {
  schema: 1;
  namespace: string;
  tender_id: string;
  form: string;
  version: string;
  revision: string;
  updated_at: string;
  fields: Record<string, DraftJson>;
};

export type DraftResult<T extends Record<string, unknown>> = {
  value: Partial<T> | null;
  revision: string | null;
  error: Error | null;
};

export type DraftWriteResult = {
  revision: string | null;
  error: Error | null;
};

export type DraftRemoveResult = {
  removed: boolean;
  revision: string | null;
  error: Error | null;
};

export type FormDraftOptions = {
  storage?: Storage;
  /** Skip one initial/rebind read when an explicit server reload must win. */
  hydrate?: boolean;
};

const STORAGE_PREFIX = "quantix.form-draft.v1";
const SECRET_FIELD =
  /(?:password|passphrase|api[_-]?key|token|secret|credential|authorization|private[_-]?key|access[_-]?key)/i;
let revisionCounter = 0;

export function createDraftScope(
  namespace: string,
  tenderId: string,
  form: string,
  version: string | number,
): DraftScope {
  return {
    namespace: normalizePart(namespace),
    tenderId: normalizePart(tenderId),
    form: normalizePart(form),
    version: normalizePart(String(version)),
  };
}

export function draftStorageKey(scope: DraftScope) {
  return [
    STORAGE_PREFIX,
    scope.namespace,
    scope.tenderId,
    scope.form,
    scope.version,
  ]
    .map((part) => encodeURIComponent(part))
    .join(":");
}

export function readFormDraft<T extends Record<string, unknown>>(
  scope: DraftScope,
  fields: DraftFields<T>,
  storage: Storage | undefined = defaultStorage(),
): DraftResult<T> {
  if (!storage) {
    return {
      value: null,
      revision: null,
      error: storageUnavailableError(),
    };
  }
  try {
    const raw = storage.getItem(draftStorageKey(scope));
    if (!raw) return { value: null, revision: null, error: null };
    const parsed: unknown = JSON.parse(raw);
    if (!isEnvelope(parsed) || !matchesScope(parsed, scope)) {
      return { value: null, revision: null, error: null };
    }
    const value = pickFields<T>(parsed.fields, fields);
    return { value, revision: parsed.revision, error: null };
  } catch {
    return {
      value: null,
      revision: null,
      error: draftStorageError("read"),
    };
  }
}

export function writeFormDraft<T extends Record<string, unknown>>(
  scope: DraftScope,
  value: T,
  fields: DraftFields<T>,
  storage: Storage | undefined = defaultStorage(),
): DraftWriteResult {
  if (!storage) return { revision: null, error: storageUnavailableError() };
  const picked = pickFields<T>(value, fields);
  const secretField = fields.find((field) => SECRET_FIELD.test(field));
  if (secretField) {
    return {
      revision: null,
      error: new Error(
        `Sensitive field “${secretField}” was excluded from this saved draft. Keep credentials in Settings.`,
      ),
    };
  }
  const revision = newRevision();
  const envelope: DraftEnvelope = {
    schema: 1,
    namespace: scope.namespace,
    tender_id: scope.tenderId,
    form: scope.form,
    version: scope.version,
    revision,
    updated_at: new Date().toISOString(),
    fields: picked as Record<string, DraftJson>,
  };
  try {
    storage.setItem(draftStorageKey(scope), JSON.stringify(envelope));
    return { revision, error: null };
  } catch {
    return { revision: null, error: draftStorageError("save") };
  }
}

export function removeFormDraft(
  scope: DraftScope,
  expectedRevision?: string | null,
  storage: Storage | undefined = defaultStorage(),
): DraftRemoveResult {
  if (!storage) {
    return {
      removed: false,
      revision: null,
      error: storageUnavailableError(),
    };
  }
  try {
    const key = draftStorageKey(scope);
    const raw = storage.getItem(key);
    if (!raw) return { removed: false, revision: null, error: null };
    const parsed: unknown = JSON.parse(raw);
    if (!isEnvelope(parsed) || !matchesScope(parsed, scope)) {
      return { removed: false, revision: null, error: null };
    }
    if (expectedRevision && parsed.revision !== expectedRevision) {
      return { removed: false, revision: parsed.revision, error: null };
    }
    storage.removeItem(key);
    return { removed: true, revision: parsed.revision, error: null };
  } catch {
    return {
      removed: false,
      revision: null,
      error: draftStorageError("remove"),
    };
  }
}

/**
 * Hydrates once on mount and persists only values changed through this hook.
 * Call `markAccepted(revision)` after a successful save; it will leave newer
 * edits intact when the accepted request was still in flight.
 */
export function useFormDraft<T extends Record<string, unknown>>(
  scope: DraftScope,
  initialValue: T,
  fields: DraftFields<T>,
  options: FormDraftOptions = {},
): {
  value: T;
  setValue: Dispatch<SetStateAction<T>>;
  setField: <K extends keyof T>(field: K, value: T[K]) => void;
  error: Error | null;
  revision: string | null;
  markAccepted: (acceptedRevision?: string | null) => DraftRemoveResult;
  clear: () => DraftRemoveResult;
} {
  const storage = options.storage ?? defaultStorage();
  const shouldHydrate = options.hydrate !== false;
  const dirty = useRef(false);
  const revisionRef = useRef<string | null>(null);
  const revisionInitialized = useRef(false);
  const scopeKey = draftStorageKey(scope);
  const bindingKeyRef = useRef(scopeKey);
  const [hydrated] = useState<DraftResult<T>>(() =>
    shouldHydrate ? readFormDraft(scope, fields, storage) : emptyDraft(),
  );
  if (!revisionInitialized.current) {
    revisionRef.current = hydrated.revision;
    revisionInitialized.current = true;
  }
  const [value, setState] = useState<T>(() =>
    hydrated.value ? { ...initialValue, ...hydrated.value } : initialValue,
  );
  const valueRef = useRef(value);
  valueRef.current = value;
  const [error, setError] = useState<Error | null>(hydrated.error);

  // A form can be reused for a different record/version without a keyed
  // boundary. Rebind synchronously before returning so geometry and text from
  // the previous scope never appear under the new scope.
  if (bindingKeyRef.current !== scopeKey) {
    const loaded = shouldHydrate
      ? readFormDraft(scope, fields, storage)
      : emptyDraft<T>();
    bindingKeyRef.current = scopeKey;
    revisionRef.current = loaded.revision;
    dirty.current = false;
    const rebound = loaded.value
      ? ({ ...initialValue, ...loaded.value } as T)
      : initialValue;
    valueRef.current = rebound;
    setState(rebound);
    setError(loaded.error);
  }

  const setValue = useCallback<Dispatch<SetStateAction<T>>>(
    (next) => {
      const current = valueRef.current;
      const resolved = typeof next === "function" ? next(current) : next;
      dirty.current = true;
      valueRef.current = resolved;
      const result = writeFormDraft(scope, resolved, fields, storage);
      if (result.revision) revisionRef.current = result.revision;
      setError(result.error);
      setState(resolved);
    },
    [fields, scope, storage],
  );

  const setField = useCallback(
    <K extends keyof T>(field: K, fieldValue: T[K]) => {
      setValue((current) => ({ ...current, [field]: fieldValue }));
    },
    [setValue],
  );

  const clear = useCallback(() => {
    const result = removeFormDraft(scope, revisionRef.current, storage);
    setError(result.error);
    if (result.removed) {
      revisionRef.current = null;
      dirty.current = false;
    }
    return result;
  }, [scope, storage]);

  const markAccepted = useCallback(
    (acceptedRevision?: string | null) => {
      if (
        acceptedRevision !== undefined &&
        revisionRef.current !== acceptedRevision
      ) {
        return { removed: false, revision: revisionRef.current, error: null };
      }
      return clear();
    },
    [clear],
  );

  return {
    value,
    setValue,
    setField,
    error,
    revision: revisionRef.current,
    markAccepted,
    clear,
  };
}

function emptyDraft<T extends Record<string, unknown>>(): DraftResult<T> {
  return { value: null, revision: null, error: null };
}

function pickFields<T extends Record<string, unknown>>(
  value: Record<string, unknown>,
  fields: DraftFields<T>,
): Partial<T> {
  const picked: Record<string, DraftJson> = {};
  for (const field of fields) {
    if (SECRET_FIELD.test(field)) continue;
    const input = value[field];
    if (input === undefined) continue;
    const json = toDraftJson(input);
    if (json !== undefined) picked[field] = json;
  }
  return picked as Partial<T>;
}

function toDraftJson(value: unknown): DraftJson | undefined {
  if (value === null || typeof value === "string" || typeof value === "boolean")
    return value;
  if (typeof value === "number")
    return Number.isFinite(value) ? value : undefined;
  if (Array.isArray(value)) {
    const output = value.map(toDraftJson);
    return output.every((item): item is DraftJson => item !== undefined)
      ? output
      : undefined;
  }
  if (typeof value === "object") {
    const output: Record<string, DraftJson> = {};
    for (const [key, item] of Object.entries(value)) {
      if (SECRET_FIELD.test(key)) continue;
      const json = toDraftJson(item);
      if (json !== undefined) output[key] = json;
    }
    return output;
  }
  return undefined;
}

function isEnvelope(value: unknown): value is DraftEnvelope {
  return (
    typeof value === "object" &&
    value !== null &&
    (value as DraftEnvelope).schema === 1 &&
    typeof (value as DraftEnvelope).namespace === "string" &&
    typeof (value as DraftEnvelope).tender_id === "string" &&
    typeof (value as DraftEnvelope).form === "string" &&
    typeof (value as DraftEnvelope).version === "string" &&
    typeof (value as DraftEnvelope).revision === "string" &&
    typeof (value as DraftEnvelope).fields === "object" &&
    (value as DraftEnvelope).fields !== null
  );
}

function matchesScope(value: DraftEnvelope, scope: DraftScope) {
  return (
    value.namespace === scope.namespace &&
    value.tender_id === scope.tenderId &&
    value.form === scope.form &&
    value.version === scope.version
  );
}

function defaultStorage(): Storage | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage;
  } catch {
    return undefined;
  }
}

function storageUnavailableError() {
  return new Error(
    "Saved drafts are unavailable on this device. Keep a copy before closing.",
  );
}

function normalizePart(value: string) {
  return value.trim();
}

function newRevision() {
  revisionCounter += 1;
  return `${Date.now().toString(36)}-${revisionCounter.toString(36)}`;
}

function draftStorageError(action: "read" | "save" | "remove") {
  return new Error(
    `Saved draft could not be ${action === "read" ? "read" : action === "save" ? "saved" : "removed"} on this device. Keep a copy before closing.`,
  );
}
