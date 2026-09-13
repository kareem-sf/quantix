export const workspaceSections = [
  "manager",
  "documents",
  "work",
  "estimate",
  "submission",
  "recents",
] as const;

export type WorkspaceSection = (typeof workspaceSections)[number];

export type SourceRouteOptions = {
  sourceId?: string;
  artifactId?: string;
  version?: number;
  contentHash?: string;
  page?: number;
  sheet?: string;
  cellRange?: string;
  pageCount?: number;
  origin?: string;
  view?: string;
  recordId?: string;
  sel?: string;
};

export type RecordRouteOptions = {
  view: string;
  recordId: string;
  origin?: string;
};

export type RouteContext =
  | {
      kind: "tender";
      tenderId: string;
      section: WorkspaceSection;
      recordId?: string;
      sourceId?: string;
      artifactId?: string;
      version?: number;
      contentHash?: string;
      page?: number;
      sheet?: string;
      cellRange?: string;
      pageCount?: number;
      origin?: string;
      view?: string;
    }
  | { kind: "settings"; origin?: string };

const sectionSet = new Set<string>(workspaceSections);

export function tenderRoute(
  tenderId: string,
  section: WorkspaceSection = "manager",
  recordId?: string,
) {
  const path = `/tenders/${encodeURIComponent(tenderId)}/${section}`;
  return recordId ? `${path}/${encodeURIComponent(recordId)}` : path;
}

export function sourceRoute(
  tenderId: string,
  section: WorkspaceSection,
  options: SourceRouteOptions,
) {
  const query = new URLSearchParams();
  if (options.sourceId) query.set("source_id", options.sourceId);
  if (options.artifactId) query.set("artifact_id", options.artifactId);
  if (isPositiveInteger(options.version))
    query.set("version", String(options.version));
  if (options.contentHash) query.set("content_hash", options.contentHash);
  if (isPositiveInteger(options.page)) query.set("page", String(options.page));
  if (options.sheet) query.set("sheet", options.sheet);
  if (options.cellRange) query.set("cell_range", options.cellRange);
  if (isPositiveInteger(options.pageCount))
    query.set("page_count", String(options.pageCount));
  if (options.origin) query.set("origin", options.origin);
  if (options.view) query.set("view", options.view);
  if (options.recordId) query.set("record", options.recordId);
  if (options.sel) query.set("sel", options.sel);
  const suffix = query.toString();
  return `${tenderRoute(tenderId, section)}${suffix ? `?${suffix}` : ""}`;
}

export function stripSourceQuery(input: string) {
  const url = new URL(input, "http://quantix.local");
  [
    "source_id",
    "artifact_id",
    "version",
    "content_hash",
    "page",
    "sheet",
    "cell_range",
    "page_count",
    "origin",
  ].forEach((key) => url.searchParams.delete(key));
  const query = url.searchParams.toString();
  return `${url.pathname}${query ? `?${query}` : ""}`;
}

export function recordRoute(
  tenderId: string,
  section: WorkspaceSection,
  options: RecordRouteOptions,
) {
  const query = new URLSearchParams({
    view: options.view,
    record: options.recordId,
  });
  if (options.origin) query.set("origin", options.origin);
  return `${tenderRoute(tenderId, section)}?${query.toString()}`;
}

export function settingsRoute(origin?: string) {
  if (!origin) return "/settings";
  return `/settings?${new URLSearchParams({ return: origin }).toString()}`;
}

export function parseRouteContext(input: string): RouteContext {
  const url = new URL(input, "http://quantix.local");
  const tenderMatch = url.pathname.match(
    /^\/tenders\/([^/]+)(?:\/([^/]+))?(?:\/([^/]+))?$/,
  );
  if (tenderMatch) {
    const tenderId = safeDecode(tenderMatch[1]);
    const pathRecordId = tenderMatch[3]
      ? safeDecode(tenderMatch[3])
      : undefined;
    const queryRecord = url.searchParams.get("record");
    const recordId =
      pathRecordId ?? (queryRecord ? safeDecode(queryRecord) : undefined);
    if (
      !tenderId ||
      (tenderMatch[3] && !pathRecordId) ||
      (queryRecord && !recordId)
    ) {
      return { kind: "settings" };
    }
    const section = sectionSet.has(tenderMatch[2] ?? "")
      ? (tenderMatch[2] as WorkspaceSection)
      : "manager";
    const version = positiveInteger(url.searchParams.get("version"));
    const pageCount = positiveInteger(url.searchParams.get("page_count"));
    const page = positiveInteger(url.searchParams.get("page"));
    return {
      kind: "tender",
      tenderId,
      section,
      ...(recordId ? { recordId } : {}),
      ...(url.searchParams.get("source_id")
        ? { sourceId: url.searchParams.get("source_id")! }
        : {}),
      ...(url.searchParams.get("artifact_id")
        ? { artifactId: url.searchParams.get("artifact_id")! }
        : {}),
      ...(version !== undefined ? { version } : {}),
      ...(url.searchParams.get("content_hash")
        ? { contentHash: url.searchParams.get("content_hash")! }
        : {}),
      ...(page !== undefined ? { page } : {}),
      ...(url.searchParams.get("sheet")
        ? { sheet: url.searchParams.get("sheet")! }
        : {}),
      ...(url.searchParams.get("cell_range")
        ? { cellRange: url.searchParams.get("cell_range")! }
        : {}),
      ...(pageCount !== undefined ? { pageCount } : {}),
      ...(url.searchParams.get("origin")
        ? { origin: url.searchParams.get("origin")! }
        : {}),
      ...(url.searchParams.get("view")
        ? { view: url.searchParams.get("view")! }
        : {}),
      ...(url.searchParams.get("sel")
        ? { sel: url.searchParams.get("sel")! }
        : {}),
    };
  }
  return {
    kind: "settings",
    ...(url.searchParams.get("return") || url.searchParams.get("origin")
      ? {
          origin:
            url.searchParams.get("return") ?? url.searchParams.get("origin")!,
        }
      : {}),
  };
}

export function isWorkspaceSection(
  value: string | undefined,
): value is WorkspaceSection {
  return !!value && sectionSet.has(value);
}

export function safeLocalOrigin(origin: string | undefined) {
  if (!origin || !origin.startsWith("/")) return undefined;
  try {
    const url = new URL(origin, "http://quantix.local");
    if (
      url.origin !== "http://quantix.local" ||
      url.pathname.startsWith("//")
    ) {
      return undefined;
    }
    const context = parseRouteContext(`${url.pathname}${url.search}`);
    if (!url.pathname.startsWith("/tenders/") && url.pathname !== "/settings") {
      return undefined;
    }
    if (context.kind !== "tender" && context.kind !== "settings")
      return undefined;
    return `${url.pathname}${url.search}`;
  } catch {
    return undefined;
  }
}

function safeDecode(value: string) {
  try {
    return decodeURIComponent(value);
  } catch {
    return undefined;
  }
}

function positiveInteger(raw: string | null) {
  if (!raw || !/^\d+$/.test(raw)) return undefined;
  const parsed = Number(raw);
  return Number.isSafeInteger(parsed) && parsed > 0 ? parsed : undefined;
}

function isPositiveInteger(value: number | undefined): value is number {
  return value !== undefined && Number.isSafeInteger(value) && value > 0;
}
