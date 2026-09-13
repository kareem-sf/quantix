import { useEffect, useId, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ChevronLeft,
  ChevronRight,
  FileText,
  History,
  Minus,
  Plus,
  Ruler,
  X,
} from "lucide-react";
import { tenderPath, useApi, type Schema } from "../api";
import { ErrorNotice, Loading, Modal } from "../components/common";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { CopyButton } from "@/components/ui/copy-button";
import { MicroButton } from "@/components/ui/micro-button";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { Separator } from "@/components/ui/separator";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { Measurements } from "./Measurements";

export type SourceSelection =
  | ({ sourceId: string } & SourceContext)
  | ({ artifactId: string } & SourceContext);

type SourceContext = {
  artifactId?: string;
  artifact?: Schema<"Artifact">;
  version?: number;
  contentHash?: string;
  page?: number;
  pageCount?: number;
  sheet?: string;
  cellRange?: string;
  origin?: string;
};

export function Citations({
  ids,
  tenderId,
  onOpen,
}: {
  ids: string[];
  tenderId: string;
  onOpen: (source: SourceSelection) => void;
}) {
  return ids.length ? (
    <div className="flex flex-wrap gap-1.5">
      {ids.map((id) => (
        <Citation key={id} id={id} tenderId={tenderId} onOpen={onOpen} />
      ))}
    </div>
  ) : null;
}

function Citation({
  id,
  tenderId,
  onOpen,
}: {
  id: string;
  tenderId: string;
  onOpen: (source: SourceSelection) => void;
}) {
  const api = useApi();
  const path = `${tenderPath(tenderId)}/evidence/${encodeURIComponent(id)}`;
  const evidence = useQuery({
    queryKey: [path],
    queryFn: () => api.get<Schema<"Evidence">>(path),
    staleTime: Infinity,
    retry: false,
  });
  return (
    <Button
      type="button"
      variant="outline"
      size="sm"
      className="h-6 max-w-full gap-1 rounded-md px-2 text-xs font-normal text-muted-foreground hover:text-foreground"
      onClick={() =>
        onOpen(
          evidence.data
            ? {
                sourceId: id,
                artifactId: evidence.data.artifact_id,
                page: evidence.data.page ?? undefined,
                sheet: evidence.data.sheet ?? undefined,
                cellRange: evidence.data.cell_range ?? undefined,
              }
            : { sourceId: id },
        )
      }
      title={evidence.data?.relative_path}
    >
      <FileText className="size-3" aria-hidden="true" />
      <span className="truncate" dir="auto">
        {evidence.data
          ? `${evidence.data.artifact_name} · ${evidence.data.locator}`
          : evidence.isError
            ? "Source unavailable"
            : "Source…"}
      </span>
    </Button>
  );
}

export function SourceDrawer({
  tenderId,
  selection: externalSelection,
  artifacts,
  presentation = "modal",
  onClose,
  onSelectionChange,
}: {
  tenderId: string;
  selection: SourceSelection;
  artifacts: Schema<"Artifact">[];
  presentation?: "modal" | "inline";
  onClose: () => void;
  onSelectionChange?: (selection: SourceSelection) => void;
}) {
  const api = useApi();
  const base = tenderPath(tenderId);
  const externalKey = JSON.stringify([
    tenderId,
    "sourceId" in externalSelection ? externalSelection.sourceId : null,
    externalSelection.artifactId,
    externalSelection.version,
    externalSelection.contentHash,
    externalSelection.page,
    externalSelection.sheet,
    externalSelection.cellRange,
    externalSelection.origin,
  ]);
  const [localSelection, setLocalSelection] = useState<{
    basis: string;
    value: SourceSelection;
  } | null>(null);
  const selection =
    localSelection?.basis === externalKey
      ? localSelection.value
      : externalSelection;
  useEffect(() => setLocalSelection(null), [externalKey]);
  const sourceId = "sourceId" in selection ? selection.sourceId : null;
  const source = useQuery({
    queryKey: [base, "source", sourceId],
    queryFn: () => api.get<Schema<"Evidence">>(`${base}/evidence/${sourceId}`),
    enabled: !!sourceId,
    retry: false,
  });
  const artifactId =
    ("artifactId" in selection ? selection.artifactId : undefined) ??
    source.data?.artifact_id;
  const knownArtifact =
    selection.artifact ?? artifacts.find((item) => item.id === artifactId);
  const savedArtifact = useQuery({
    queryKey: [base, "artifact", artifactId],
    queryFn: () =>
      api.get<Schema<"Artifact">>(`${base}/artifacts/${artifactId}`),
    enabled: !!artifactId && !knownArtifact,
    retry: false,
  });
  const artifact = knownArtifact ?? savedArtifact.data;
  const activeSheet = selection.sheet ?? source.data?.sheet ?? undefined;
  const activeCellRange =
    selection.cellRange ?? source.data?.cell_range ?? undefined;
  const evidenceLocation = JSON.stringify([
    artifactId,
    sourceId,
    activeSheet,
    activeCellRange,
  ]);
  const [evidencePage, setEvidencePage] = useState({
    location: evidenceLocation,
    offset: 0,
  });
  const offset =
    evidencePage.location === evidenceLocation ? evidencePage.offset : 0;
  function setOffset(next: number) {
    setEvidencePage({ location: evidenceLocation, offset: next });
  }
  useEffect(() => {
    setEvidencePage({ location: evidenceLocation, offset: 0 });
  }, [evidenceLocation]);
  const evidenceQuery = new URLSearchParams({
    offset: String(offset),
    limit: "30",
  });
  if (activeSheet) evidenceQuery.set("sheet", activeSheet);
  if (activeCellRange) evidenceQuery.set("cell_range", activeCellRange);
  const evidence = useQuery({
    queryKey: [
      base,
      artifactId,
      "evidence",
      offset,
      activeSheet,
      activeCellRange,
    ],
    queryFn: () =>
      api.get<Schema<"Evidence">[]>(
        `${base}/artifacts/${artifactId}/evidence?${evidenceQuery}`,
      ),
    enabled: !!artifactId && !sourceId,
    retry: false,
  });
  const [page, setPage] = useState<number | null>(null);
  const [pageInput, setPageInput] = useState("");
  const [pageInputError, setPageInputError] = useState<Error | null>(null);
  const [measuring, setMeasuring] = useState(false);
  const [zoom, setZoom] = useState(100);
  const [hiddenPreviewFor, setHiddenPreviewFor] = useState<string | null>(null);
  const previewVisible = hiddenPreviewFor !== artifactId;
  const previewId = useId();
  const [downloading, setDownloading] = useState(false);
  const [downloadError, setDownloadError] = useState<unknown>(null);
  const isPdf =
    artifact?.kind === "pdf" || artifact?.name.toLowerCase().endsWith(".pdf");
  const isSpreadsheet = artifact?.kind === "spreadsheet";
  const metadataPageCount = positiveNumber(artifact?.metadata?.page_count);
  const pageInfo = useQuery({
    queryKey: [base, artifactId, "measurement-page-info"],
    queryFn: () =>
      api.get<Schema<"MeasurementPage">>(
        `${base}/artifacts/${artifactId}/measurement-page?page=1`,
      ),
    enabled: !!artifactId && !!isPdf && metadataPageCount === undefined,
    staleTime: Infinity,
    retry: false,
  });
  const pageCount =
    metadataPageCount ?? positiveNumber(pageInfo.data?.page_count);
  const requestedPage =
    page ??
    ("page" in selection ? selection.page : undefined) ??
    source.data?.page ??
    1;
  const activePage = pageCount
    ? clamp(requestedPage, 1, pageCount)
    : Math.max(1, requestedPage);
  const invalidRequestedPage =
    pageCount !== undefined && requestedPage !== activePage && page === null;
  const context: SourceContext = {
    artifactId,
    version:
      selection.version ??
      artifact?.version ??
      (source.data?.metadata?.version as number | undefined),
    contentHash: selection.contentHash ?? artifact?.content_hash,
    page: activePage,
    pageCount,
    sheet: activeSheet,
    cellRange: activeCellRange,
    origin: selection.origin,
  };
  const preview = useQuery({
    queryKey: [base, artifactId, "preview", activePage],
    queryFn: () =>
      api.blob(`${base}/artifacts/${artifactId}/preview?page=${activePage}`),
    enabled: !!artifactId && !!isPdf,
    retry: false,
  });
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  useEffect(() => {
    // Browser back/forward can change the source locator without remounting.
    setPage(null);
    setPageInputError(null);
  }, [selection.page, artifactId, sourceId]);
  useEffect(() => {
    setPageInput(String(activePage));
  }, [activePage]);
  useEffect(() => {
    if (!preview.data) {
      setImageUrl(null);
      return;
    }
    const url = URL.createObjectURL(preview.data);
    setImageUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [preview.data]);
  function changePage(next: number) {
    const bounded = pageCount ? clamp(next, 1, pageCount) : Math.max(1, next);
    setPage(bounded);
    setPageInput(String(bounded));
    onSelectionChange?.({
      ...selection,
      ...(artifactId ? { artifactId } : {}),
      ...(artifact?.version !== undefined
        ? { version: artifact.version }
        : selection.version !== undefined
          ? { version: selection.version }
          : {}),
      page: bounded,
      ...(artifact?.content_hash
        ? { contentHash: artifact.content_hash }
        : selection.contentHash
          ? { contentHash: selection.contentHash }
          : {}),
      ...(selection.sheet || source.data?.sheet
        ? { sheet: selection.sheet ?? source.data?.sheet ?? undefined }
        : {}),
      ...(selection.cellRange || source.data?.cell_range
        ? {
            cellRange:
              selection.cellRange ?? source.data?.cell_range ?? undefined,
          }
        : {}),
    } as SourceSelection);
  }
  function submitPage(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const next = Number(pageInput);
    if (
      !Number.isInteger(next) ||
      next < 1 ||
      (pageCount !== undefined && next > pageCount)
    ) {
      setPageInputError(
        new Error(
          pageCount
            ? `Page ${pageInput || "value"} is outside the available range 1–${pageCount}.`
            : "Enter a whole page number greater than zero.",
        ),
      );
      return;
    }
    setPageInputError(null);
    changePage(next);
  }
  function changeWorksheet(sheet: string, cellRange: string) {
    if (!artifactId) return;
    // Browsing a worksheet leaves the single-citation selection, but keeps its
    // immutable artifact and the route the engineer should return to.
    const next: SourceSelection = {
      artifactId,
      artifact,
      version: artifact?.version ?? selection.version,
      contentHash: artifact?.content_hash ?? selection.contentHash,
      page: selection.page,
      pageCount: selection.pageCount,
      sheet: sheet || undefined,
      cellRange: cellRange.trim() || undefined,
      origin: selection.origin,
    };
    setLocalSelection({ basis: externalKey, value: next });
    setOffset(0);
    onSelectionChange?.(next);
  }
  async function downloadOriginal(file: Schema<"Artifact">) {
    setDownloading(true);
    setDownloadError(null);
    try {
      const blob = await api.blob(`${base}/artifacts/${file.id}/original`),
        url = URL.createObjectURL(blob),
        link = document.createElement("a");
      link.href = url;
      link.download = file.name;
      document.body.append(link);
      link.click();
      link.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (failure) {
      setDownloadError(failure);
    } finally {
      setDownloading(false);
    }
  }
  const title =
    artifact?.name ?? source.data?.artifact_name ?? "Source document";
  const sourcePath = artifact?.relative_path ?? source.data?.relative_path;
  const sourceBody = (
    <div className="flex flex-col gap-4">
      {measuring && artifactId ? (
        <div className="legacy-screen">
          <Measurements
            key={`${artifactId}:${activePage}:${artifact?.version ?? selection.version ?? ""}`}
            tenderId={tenderId}
            artifactId={artifactId}
            initialPage={activePage}
            artifactVersion={artifact?.version ?? selection.version}
            contentHash={artifact?.content_hash ?? selection.contentHash}
            onClose={() => setMeasuring(false)}
          />
        </div>
      ) : (
        <>
          {sourcePath ? (
            <p className="text-xs break-all text-muted-foreground" dir="auto">
              {sourcePath}
            </p>
          ) : null}
          {artifact ? (
            <div
              className="flex flex-wrap items-center gap-1.5"
              aria-label="Source context"
            >
              <Badge variant="secondary" className="font-normal">
                Version {artifact.version}
              </Badge>
              {artifact.area ? (
                <Badge variant="secondary" className="font-normal">
                  {artifact.area}
                </Badge>
              ) : null}
              {context.sheet ? (
                <Badge variant="secondary" className="font-normal">
                  {context.sheet}
                </Badge>
              ) : null}
              {context.cellRange ? (
                <Badge variant="secondary" className="font-mono font-normal">
                  {context.cellRange}
                </Badge>
              ) : null}
              {context.contentHash ? (
                <>
                  <Badge
                    variant="outline"
                    className="font-mono font-normal text-muted-foreground"
                    title={context.contentHash}
                  >
                    Hash {context.contentHash.slice(0, 12)}…
                  </Badge>
                  <CopyButton value={context.contentHash} />
                </>
              ) : null}
            </div>
          ) : null}
          {(isPdf && artifactId) || artifact ? (
            <div className="flex flex-wrap gap-2">
              {isPdf && artifactId ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setMeasuring(true)}
                >
                  <Ruler data-icon="inline-start" />
                  Measure drawing
                </Button>
              ) : null}
              {artifact ? (
                <MicroButton
                  kind="download"
                  type="button"
                  disabled={downloading}
                  onClick={() => void downloadOriginal(artifact)}
                >
                  {downloading ? "Downloading…" : "Download original"}
                </MicroButton>
              ) : null}
            </div>
          ) : null}
          {artifact && !artifact.is_current ? (
            <p className="flex items-start gap-1.5 text-xs text-amber-700 dark:text-amber-400">
              <History
                className="mt-0.5 size-3.5 shrink-0"
                aria-hidden="true"
              />
              <span>
                Earlier file revision · Version {artifact.version}. The original
                and source text remain available for review.
              </span>
            </p>
          ) : null}
          <ErrorNotice
            error={
              pageInputError ||
              (invalidRequestedPage
                ? new Error(
                    `Page ${requestedPage} is outside the available range 1–${pageCount}. Showing page ${activePage}.`,
                  )
                : null) ||
              source.error ||
              (!isSpreadsheet && evidence.error) ||
              savedArtifact.error ||
              downloadError
            }
          />
          {sourceId && source.isPending ? (
            <Loading>Loading source…</Loading>
          ) : null}
          {isSpreadsheet && artifactId ? (
            <WorksheetNavigation
              key={JSON.stringify([artifactId, activeSheet, activeCellRange])}
              sheets={recordedSheets(artifact?.metadata?.sheets)}
              sheet={activeSheet ?? ""}
              cellRange={activeCellRange ?? ""}
              error={evidence.error}
              disabled={!!sourceId && source.isPending}
              onApply={changeWorksheet}
            />
          ) : null}
          {isPdf ? (
            <>
              <MicroButton
                kind="preview"
                active={previewVisible}
                className="self-start"
                aria-expanded={previewVisible}
                aria-controls={previewId}
                onClick={() =>
                  setHiddenPreviewFor(
                    previewVisible ? (artifactId ?? null) : null,
                  )
                }
              >
                {previewVisible ? "Hide preview" : "Preview"}
              </MicroButton>
              <div id={previewId} hidden={!previewVisible}>
                <div className="quantix-reveal flex flex-col gap-3">
                  <div className="flex flex-wrap items-center gap-1 rounded-lg border bg-card p-1">
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      disabled={activePage <= 1}
                      onClick={() => changePage(activePage - 1)}
                      aria-label="Previous page"
                    >
                      <ChevronLeft className="rtl:-scale-x-100" />
                    </Button>
                    <form
                      className="flex items-center gap-1"
                      onSubmit={submitPage}
                    >
                      <Input
                        aria-label="Page number"
                        inputMode="numeric"
                        className="h-7 w-14 text-center tabular-nums"
                        value={pageInput}
                        onChange={(event) => setPageInput(event.target.value)}
                      />
                      <Button
                        type="submit"
                        variant="ghost"
                        size="sm"
                        disabled={!pageCount}
                      >
                        Go to page
                      </Button>
                    </form>
                    <span
                      className="px-1 text-xs text-muted-foreground tabular-nums"
                      aria-live="polite"
                    >
                      {pageCount
                        ? `Page ${activePage} of ${pageCount}`
                        : `Page ${activePage}`}
                    </span>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      disabled={
                        pageCount === undefined || activePage >= pageCount
                      }
                      onClick={() => changePage(activePage + 1)}
                      aria-label="Next page"
                    >
                      <ChevronRight className="rtl:-scale-x-100" />
                    </Button>
                    <Separator orientation="vertical" className="mx-1 h-5" />
                    <div
                      role="group"
                      aria-label="Preview controls"
                      className="flex items-center gap-1"
                    >
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => setZoom(100)}
                        aria-pressed={zoom === 100}
                      >
                        Fit width
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        disabled={zoom <= 50}
                        aria-label="Zoom out"
                        onClick={() =>
                          setZoom((current) => Math.max(50, current - 25))
                        }
                      >
                        <Minus />
                      </Button>
                      <span
                        className="w-10 text-center text-xs text-muted-foreground tabular-nums"
                        aria-live="polite"
                      >
                        {zoom}%
                      </span>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-sm"
                        disabled={zoom >= 200}
                        aria-label="Zoom in"
                        onClick={() =>
                          setZoom((current) => Math.min(200, current + 25))
                        }
                      >
                        <Plus />
                      </Button>
                    </div>
                  </div>
                  {pageInfo.isPending && metadataPageCount === undefined ? (
                    <p className="text-xs text-muted-foreground">
                      Checking the PDF page count…
                    </p>
                  ) : null}
                  {pageInfo.error && metadataPageCount === undefined ? (
                    <p className="text-xs text-muted-foreground">
                      The total page count is unavailable. Use the source text
                      or original download while the page index is unavailable.
                    </p>
                  ) : null}
                  {preview.isPending ? <Loading>Loading page…</Loading> : null}
                  <ErrorNotice error={preview.error} />
                  {imageUrl ? (
                    <div className="max-h-[70vh] overflow-auto rounded-lg border bg-muted/40 p-2">
                      <img
                        className="mx-auto block rounded-sm bg-white shadow-sm"
                        style={{ width: `${zoom}%`, maxWidth: "none" }}
                        src={imageUrl}
                        alt={`${artifact?.name ?? "Document"}, page ${activePage}`}
                      />
                    </div>
                  ) : null}
                </div>
              </div>
            </>
          ) : null}
          {sourceId && source.data ? (
            <SourceText evidence={source.data} highlighted />
          ) : null}
          {!sourceId ? (
            <div className="flex flex-col gap-2">
              <h3 className="text-sm font-medium">Source text</h3>
              {evidence.isPending ? (
                <Loading>Loading source text…</Loading>
              ) : null}
              {evidence.data?.map((item) => (
                <SourceText key={item.id} evidence={item} />
              ))}
              {evidence.data?.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  {isSpreadsheet && (activeSheet || activeCellRange)
                    ? "No extracted rows match this selection. Clear the range, choose another sheet or download the original to inspect it."
                    : "This file has no extracted text. View the page preview or download the original to inspect it."}
                </p>
              ) : null}
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={offset === 0}
                  onClick={() => setOffset(Math.max(0, offset - 30))}
                >
                  Previous sources
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={!evidence.data || evidence.data.length < 30}
                  onClick={() => setOffset(offset + 30)}
                >
                  Next sources
                </Button>
              </div>
            </div>
          ) : null}
        </>
      )}
    </div>
  );
  if (presentation === "inline") {
    return (
      <section className="flex flex-col gap-4" aria-label="Source document">
        <div className="flex items-start justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <span
              className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground"
              aria-hidden="true"
            >
              <FileText className="size-4" />
            </span>
            <h2 className="truncate text-sm font-semibold" dir="auto">
              {title}
            </h2>
          </div>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={onClose}
            aria-label="Close source viewer"
          >
            <X />
          </Button>
        </div>
        {sourceBody}
      </section>
    );
  }
  return (
    <Modal drawer legacy={false} title={title} onClose={onClose}>
      {sourceBody}
    </Modal>
  );
}

function WorksheetNavigation({
  sheets,
  sheet,
  cellRange,
  error,
  disabled,
  onApply,
}: {
  sheets: string[];
  sheet: string;
  cellRange: string;
  error: unknown;
  disabled: boolean;
  onApply: (sheet: string, cellRange: string) => void;
}) {
  const [sheetInput, setSheetInput] = useState(sheet);
  const [rangeInput, setRangeInput] = useState(cellRange);
  const sheetId = useId();
  const rangeId = useId();
  const helpId = useId();
  const errorId = useId();
  return (
    <form
      className="flex flex-col gap-3 rounded-lg border bg-card p-3"
      aria-label="Worksheet navigation"
      onSubmit={(event) => {
        event.preventDefault();
        onApply(sheetInput, rangeInput);
      }}
    >
      <div className="flex flex-wrap items-end gap-2">
        <Field className="w-auto min-w-40 flex-1 gap-1.5">
          <FieldLabel htmlFor={sheetId}>Sheet</FieldLabel>
          <NativeSelect
            id={sheetId}
            className="w-full"
            value={sheetInput}
            onChange={(event) => setSheetInput(event.target.value)}
            disabled={disabled}
          >
            <NativeSelectOption value="">All sheets</NativeSelectOption>
            {sheet && !sheets.includes(sheet) ? (
              <NativeSelectOption value={sheet}>
                {sheet} (not listed)
              </NativeSelectOption>
            ) : null}
            {sheets.map((name) => (
              <NativeSelectOption key={name} value={name}>
                {name}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </Field>
        <Field className="w-auto min-w-32 flex-1 gap-1.5">
          <FieldLabel htmlFor={rangeId}>Cell range</FieldLabel>
          <Input
            id={rangeId}
            className="font-mono"
            value={rangeInput}
            onChange={(event) => setRangeInput(event.target.value)}
            placeholder="A1:D20"
            aria-describedby={`${helpId}${error ? ` ${errorId}` : ""}`}
            aria-invalid={!!error}
            disabled={disabled}
            dir="ltr"
          />
        </Field>
        <Button type="submit" disabled={disabled}>
          Apply
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={disabled || (!cellRange && !rangeInput)}
          onClick={() => {
            setRangeInput("");
            onApply(sheetInput, "");
          }}
        >
          Clear range
        </Button>
      </div>
      <FieldDescription id={helpId} className="text-xs">
        Leave the range blank to browse the sheet. A range shows the original
        rows that overlap those cells.
      </FieldDescription>
      <div id={errorId}>
        <ErrorNotice error={error} />
      </div>
    </form>
  );
}

function recordedSheets(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const names = value.flatMap((sheet: unknown) => {
    if (typeof sheet !== "object" || sheet === null || !("name" in sheet))
      return [];
    return typeof sheet.name === "string" && sheet.name.trim()
      ? [sheet.name]
      : [];
  });
  return [...new Set(names)];
}

function SourceText({
  evidence,
  highlighted = false,
}: {
  evidence: Schema<"Evidence">;
  highlighted?: boolean;
}) {
  const cells = evidence.metadata?.cells;
  const location = [evidence.locator, evidence.sheet, evidence.cell_range]
    .filter(Boolean)
    .filter((part, index, all) => all.indexOf(part) === index)
    .join(" · ");
  return (
    <article
      className={cn(
        "flex flex-col gap-2 rounded-lg border bg-card p-3",
        highlighted && "border-primary/40 ring-3 ring-primary/10",
      )}
    >
      <h4 className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
        <FileText className="size-3.5" aria-hidden="true" />
        {location || "Source passage"}
      </h4>
      <pre
        className="font-sans text-sm leading-relaxed break-words whitespace-pre-wrap"
        dir="auto"
      >
        {evidence.text || "No extracted text is available for this passage."}
      </pre>
      {Array.isArray(cells) && cells.length ? (
        <details className="text-xs">
          <summary className="w-fit cursor-pointer text-muted-foreground hover:text-foreground">
            Cell values and formulas
          </summary>
          <div className="mt-2 overflow-hidden rounded-md border">
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead>Cell</TableHead>
                  <TableHead>Value</TableHead>
                  <TableHead>Formula</TableHead>
                  <TableHead>Cached value</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {cells.map((cell: Record<string, unknown>, index: number) => (
                  <TableRow key={index}>
                    <TableCell className="font-mono">
                      {String(cell.coordinate ?? "")}
                    </TableCell>
                    <TableCell>{String(cell.value ?? "")}</TableCell>
                    <TableCell className="font-mono">
                      {String(cell.formula ?? "")}
                    </TableCell>
                    <TableCell>{String(cell.cached_value ?? "")}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </details>
      ) : null}
    </article>
  );
}

function positiveNumber(value: unknown) {
  return typeof value === "number" && Number.isInteger(value) && value > 0
    ? value
    : undefined;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}
