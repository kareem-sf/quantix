import { useDeferredValue, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowUpRight,
  DraftingCompass,
  File,
  FileSpreadsheet,
  FileText,
  FileType,
  Files as FilesIcon,
  Info,
  Network,
  Search,
} from "lucide-react";
import { tenderPath, useApi, type Schema } from "../api";
import {
  Empty,
  ErrorNotice,
  Loading,
  Status,
  statusLabel,
} from "../components/common";
import { linkUnderline } from "@/components/ui/animated-link";
import { AnimatedNumber } from "@/components/ui/animated-number";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { MicroButton } from "@/components/ui/micro-button";
import {
  FilterDisclosure,
  type FilterDisclosureItem,
} from "@/components/ui/filter-disclosure";
import { Gauge } from "@/components/ui/gauge";
import { InputGroup, InputGroupAddon } from "@/components/ui/input-group";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemTitle,
} from "@/components/ui/item";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { SmoothInput } from "@/components/ui/smooth-input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import type { SourceSelection } from "./Sources";
import { DocumentReadingMap } from "./DocumentReadingMap";
import {
  SearchPreparation,
  searchExcerpt,
  type SearchMethod,
} from "./DocumentSearch";
import { createDraftScope, useFormDraft } from "./useFormDraft";

const typeIcons: Record<string, FilterDisclosureItem["icon"]> = {
  pdf: FileText,
  spreadsheet: FileSpreadsheet,
  word: FileType,
  docx: FileType,
  cad: DraftingCompass,
};

export function Files({
  tenderId,
  artifacts,
  onImport,
  onSource,
  meaningAvailable = false,
  activeRuns = [],
  onWork,
}: {
  tenderId: string;
  artifacts: Schema<"Artifact">[];
  onImport: () => void;
  onSource: (source: SourceSelection) => void;
  meaningAvailable?: boolean;
  activeRuns?: Schema<"Run">[];
  onWork?: () => void;
}) {
  const api = useApi();
  const searchInput = useRef<HTMLInputElement>(null);
  const viewDraft = useFormDraft(
    createDraftScope("workspace", tenderId, "documents", 1),
    {
      query: "",
      status: "all",
      area: "all",
      revision: "current",
      mode: "combined" as SearchMethod,
    },
    ["query", "status", "area", "revision", "mode"],
  );
  const { query, status, area, revision, mode } = viewDraft.value;
  const setQuery = (value: string) => viewDraft.setField("query", value);
  const setStatus = (value: string) => viewDraft.setField("status", value);
  const setArea = (value: string) => viewDraft.setField("area", value);
  const setRevision = (value: string) => viewDraft.setField("revision", value);
  const setMode = (value: SearchMethod) => viewDraft.setField("mode", value);
  const [kind, setKind] = useState("all");
  const [mapOpen, setMapOpen] = useState(false);
  const deferred = useDeferredValue(query.trim());
  // While the package is being read the map opens itself, so the first import
  // shows the Tender Manager working through the documents.
  const reading = activeRuns.some(
    (run) => run.kind === "import" || run.kind === "index",
  );
  const showMap = mapOpen || reading;
  const history = useQuery({
    queryKey: [`${tenderPath(tenderId)}/artifacts?include_history=true`],
    queryFn: () =>
      api.get<Schema<"Artifact">[]>(
        `${tenderPath(tenderId)}/artifacts?include_history=true`,
      ),
    enabled: revision === "all",
  });
  const registered = revision === "all" ? (history.data ?? []) : artifacts;
  const documentKey = artifacts
    .map((file) => `${file.id}:${file.status}`)
    .join("|");
  useEffect(() => {
    if (revision === "all") void history.refetch();
  }, [revision, documentKey, history.refetch]);
  const searchParams = new URLSearchParams({ q: deferred, mode });
  if (area !== "all") searchParams.set("area", area);
  if (status !== "all") searchParams.set("status", status);
  const search = useQuery({
    queryKey: [tenderPath(tenderId), "search", deferred, mode, area, status],
    queryFn: () =>
      api.get<Schema<"Evidence">[]>(
        `${tenderPath(tenderId)}/search?${searchParams}`,
      ),
    enabled: !!deferred,
    retry: false,
  });
  const matchesFilters = (file: Schema<"Artifact">) =>
    (status === "all" || file.status === status) &&
    (area === "all" || file.area === area) &&
    (kind === "all" || file.kind === kind);
  const visible = registered.filter(
    (file) => matchesFilters(file) && (revision === "all" || file.is_current),
  );
  const visibleIds = new Set(
    artifacts.filter(matchesFilters).map((file) => file.id),
  );
  const matchingSources = search.data?.filter((hit) =>
    visibleIds.has(hit.artifact_id),
  );
  const matchingFiles = deferred
    ? visible.filter((file) =>
        file.relative_path
          .toLocaleLowerCase()
          .includes(deferred.toLocaleLowerCase()),
      )
    : [];
  const statuses = Array.from(
    new Set(registered.map((file) => file.status)),
  ).sort();
  const areas = Array.from(new Set(registered.map((file) => file.area)))
    .filter(Boolean)
    .sort();
  const kinds = Array.from(new Set(registered.map((file) => file.kind))).sort();
  const typeItems: FilterDisclosureItem[] = [
    {
      id: "all",
      label: "All types",
      icon: FilesIcon,
      count: registered.length,
    },
    ...kinds.map((value) => ({
      id: value,
      label: documentType(value),
      icon: typeIcons[value] ?? File,
      count: registered.filter((file) => file.kind === value).length,
    })),
  ];
  const readCount = artifacts.filter(
    (file) => file.status === "extracted",
  ).length;
  const coverage = artifacts.length ? (readCount / artifacts.length) * 100 : 0;
  const sameContent = (file: Schema<"Artifact">) =>
    artifacts
      .filter(
        (other) =>
          other.id !== file.id && other.content_hash === file.content_hash,
      )
      .map((other) => other.name);

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex max-w-2xl flex-col gap-1">
          <h2 className="text-lg font-semibold tracking-tight">Documents</h2>
          <p className="text-sm text-muted-foreground">
            The imported register, source text and document revisions for this
            tender.
          </p>
        </div>
        <div className="flex items-center gap-4">
          {artifacts.length ? (
            <div className="flex items-center gap-2.5">
              <Gauge
                value={coverage}
                size={40}
                strokeWidth={12}
                showValue={false}
                aria-label={`${readCount} of ${artifacts.length} documents read`}
              />
              <div className="flex flex-col text-xs leading-tight">
                <span className="font-medium text-foreground">
                  <AnimatedNumber value={readCount} /> of {artifacts.length}{" "}
                  documents read
                </span>
                <span className="text-muted-foreground">Document reading</span>
              </div>
            </div>
          ) : null}
          <MicroButton kind="upload" onClick={onImport}>
            Import package
          </MicroButton>
        </div>
      </header>

      <div className="flex gap-2.5 rounded-xl border bg-muted/40 px-3.5 py-3 text-xs leading-relaxed text-muted-foreground">
        <Info className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
        <p>
          Quantix reads every imported copy, including scanned pages, and keeps
          the originals unchanged. Pages it cannot read reliably are flagged for
          a second reading. Reading a document is not an engineering analysis or
          review.{" "}
          {onWork ? (
            <Button
              type="button"
              variant="link"
              size="sm"
              className={cn("h-auto p-0 text-xs", linkUnderline)}
              onClick={onWork}
            >
              View findings and decisions in Work
            </Button>
          ) : (
            " Analysis findings and engineer decisions are recorded in Work."
          )}
        </p>
      </div>

      {artifacts.length ? (
        <section
          className="flex flex-col gap-3"
          aria-label="Document reading map"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex min-w-0 flex-col gap-0.5">
              <h3 className="text-sm font-medium">Document reading map</h3>
              <p className="text-xs text-muted-foreground">
                {reading
                  ? "Analyzing tender package. This map shows which documents are read."
                  : "How this package connects, and which files have extracted text."}
              </p>
            </div>
            <Button
              type="button"
              variant="outline"
              size="sm"
              aria-expanded={showMap}
              onClick={() => setMapOpen(!showMap)}
            >
              <Network data-icon="inline-start" />
              {showMap
                ? "Hide document reading map"
                : "Show document reading map"}
            </Button>
          </div>
          {showMap ? (
            <div className="quantix-reveal">
              <DocumentReadingMap
                artifacts={artifacts}
                working={reading}
                onSource={onSource}
              />
            </div>
          ) : null}
        </section>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <InputGroup className="min-w-60 flex-1">
          <InputGroupAddon>
            <Search />
          </InputGroupAddon>
          <SmoothInput
            ref={searchInput}
            data-slot="input-group-control"
            aria-label="Search source text"
            value={query}
            dir="auto"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search source text…"
            wrapperClassName="flex-1 self-stretch"
            className="h-full px-2.5 text-sm placeholder:text-muted-foreground"
          />
        </InputGroup>
        <MicroButton
          kind="search"
          active={query.length > 0}
          onClick={() => {
            if (query) setQuery("");
            searchInput.current?.focus();
          }}
        >
          {query ? "Clear search" : "Search"}
        </MicroButton>
        {kinds.length > 1 ? (
          <FilterDisclosure
            label="Filter by document type"
            items={typeItems}
            value={kinds.includes(kind) ? kind : "all"}
            onChange={setKind}
          />
        ) : null}
        {meaningAvailable ? (
          <NativeSelect
            aria-label="Search method"
            value={mode}
            onChange={(event) => setMode(event.target.value as SearchMethod)}
          >
            <NativeSelectOption value="combined">
              Meaning search
            </NativeSelectOption>
            <NativeSelectOption value="meaning">
              Meaning only
            </NativeSelectOption>
            <NativeSelectOption value="words">Exact words</NativeSelectOption>
          </NativeSelect>
        ) : null}
        <NativeSelect
          aria-label="Filter by processing status"
          value={status}
          onChange={(event) => setStatus(event.target.value)}
        >
          <NativeSelectOption value="all">
            All processing statuses
          </NativeSelectOption>
          {statuses.map((value) => (
            <NativeSelectOption key={value} value={value}>
              {statusLabel(value)}
            </NativeSelectOption>
          ))}
        </NativeSelect>
        <NativeSelect
          aria-label="Filter by area"
          value={area}
          onChange={(event) => setArea(event.target.value)}
        >
          <NativeSelectOption value="all">All areas</NativeSelectOption>
          {areas.map((value) => (
            <NativeSelectOption key={value} value={value}>
              {value}
            </NativeSelectOption>
          ))}
        </NativeSelect>
        <NativeSelect
          aria-label="Filter by revision"
          value={revision}
          onChange={(event) => setRevision(event.target.value)}
        >
          <NativeSelectOption value="current">Current files</NativeSelectOption>
          <NativeSelectOption value="all">All revisions</NativeSelectOption>
        </NativeSelect>
      </div>

      {revision === "all" ? (
        <p className="-mt-3 text-xs text-muted-foreground">
          Search uses current file versions. Earlier versions can be opened from
          the register.
        </p>
      ) : null}
      {meaningAvailable &&
      (mode !== "words" || activeRuns.some((run) => run.kind === "index")) ? (
        <SearchPreparation
          tenderId={tenderId}
          activeRuns={activeRuns}
          documentKey={documentKey}
          onWork={onWork}
        />
      ) : null}
      {revision === "all" && history.isPending ? (
        <Loading>Loading earlier file versions…</Loading>
      ) : null}
      <ErrorNotice
        error={viewDraft.error || (revision === "all" ? history.error : null)}
      />

      {deferred ? (
        <section className="flex flex-col gap-3">
          <h3 className="text-sm font-medium">Source matches</h3>
          {search.isPending ? <Loading>Searching documents…</Loading> : null}
          <ErrorNotice error={search.error} />
          {matchingFiles.length ? (
            <DocumentRegister
              files={matchingFiles}
              sameContent={sameContent}
              onSource={onSource}
            />
          ) : null}
          {matchingSources?.length === 0 && matchingFiles.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No source matches these search and document filters.
            </p>
          ) : null}
          {matchingSources?.length ? (
            <div className="flex flex-col gap-2">
              {matchingSources.map((hit) => (
                <Item
                  key={hit.id}
                  variant="outline"
                  className="bg-card text-start"
                  render={
                    <button
                      type="button"
                      onClick={() => {
                        const artifact = artifacts.find(
                          (item) => item.id === hit.artifact_id,
                        );
                        onSource({
                          sourceId: hit.id,
                          artifactId: hit.artifact_id,
                          ...(artifact
                            ? {
                                artifact,
                                version: artifact.version,
                                contentHash: artifact.content_hash,
                              }
                            : {}),
                          ...(hit.page ? { page: hit.page } : {}),
                          ...(hit.sheet ? { sheet: hit.sheet } : {}),
                          ...(hit.cell_range
                            ? { cellRange: hit.cell_range }
                            : {}),
                        });
                      }}
                    />
                  }
                >
                  <ItemContent>
                    <ItemTitle>
                      <span dir="auto">{hit.artifact_name}</span>
                      <Badge variant="secondary" className="font-normal">
                        {hit.locator}
                      </Badge>
                    </ItemTitle>
                    <ItemDescription className="line-clamp-3" dir="auto">
                      {searchExcerpt(hit)}
                    </ItemDescription>
                  </ItemContent>
                  <ItemActions>
                    <ArrowUpRight className="size-4 text-muted-foreground rtl:-scale-x-100" />
                  </ItemActions>
                </Item>
              ))}
            </div>
          ) : null}
        </section>
      ) : registered.length ? (
        <DocumentRegister
          files={visible}
          sameContent={sameContent}
          onSource={onSource}
        />
      ) : revision === "all" && (history.isPending || history.error) ? null : (
        <Empty
          title="Add the tender documents"
          action={
            <MicroButton kind="upload" onClick={onImport}>
              Import package
            </MicroButton>
          }
        >
          Import a folder or ZIP file to build the document register and inspect
          the source evidence.
        </Empty>
      )}
    </div>
  );
}

function DocumentRegister({
  files,
  sameContent,
  onSource,
}: {
  files: Schema<"Artifact">[];
  sameContent: (file: Schema<"Artifact">) => string[];
  onSource: (selection: SourceSelection) => void;
}) {
  return (
    <div className="overflow-hidden rounded-xl border bg-card">
      <Table aria-label="Document register">
        <TableHeader className="bg-muted/40">
          <TableRow className="hover:bg-transparent">
            <TableHead className="ps-4">Document</TableHead>
            <TableHead>Area</TableHead>
            <TableHead>Type</TableHead>
            <TableHead>Revision</TableHead>
            <TableHead className="pe-4">Reading status</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {files.map((file) => (
            <TableRow key={file.id}>
              <TableCell className="min-w-72 py-3 ps-4 whitespace-normal">
                <div className="flex items-start gap-3">
                  <span
                    className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground"
                    aria-hidden="true"
                  >
                    <FileText className="size-4" />
                  </span>
                  <div className="flex min-w-0 flex-col gap-0.5">
                    <Button
                      type="button"
                      variant="link"
                      className="h-auto justify-start p-0 text-start font-medium whitespace-normal text-foreground"
                      dir="auto"
                      onClick={() =>
                        onSource({
                          artifactId: file.id,
                          artifact: file,
                          version: file.version,
                          contentHash: file.content_hash,
                        })
                      }
                    >
                      {file.name}
                    </Button>
                    <p className="text-xs text-muted-foreground">
                      {file.relative_path !== file.name ? (
                        <>
                          <span dir="auto">{file.relative_path}</span> ·{" "}
                        </>
                      ) : null}
                      {formatSize(file.size)}
                    </p>
                    {sameContent(file).length ? (
                      <p className="text-xs text-muted-foreground">
                        Same file as{" "}
                        <span dir="auto">{sameContent(file).join(", ")}</span>
                      </p>
                    ) : null}
                    {file.warnings?.length ? (
                      <details className="text-xs">
                        <summary className="w-fit cursor-pointer text-amber-700 dark:text-amber-400">
                          {file.warnings.length} extraction{" "}
                          {file.warnings.length === 1 ? "issue" : "issues"}
                        </summary>
                        <ul className="mt-1 flex flex-col gap-0.5 text-muted-foreground">
                          {file.warnings.map((warning, index) => (
                            <li key={index}>
                              {String(
                                warning.message ??
                                  warning.code ??
                                  "Document requires attention",
                              )}
                              {warning.locator
                                ? ` · ${String(warning.locator)}`
                                : ""}
                            </li>
                          ))}
                        </ul>
                      </details>
                    ) : null}
                  </div>
                </div>
              </TableCell>
              <TableCell className="text-muted-foreground">
                {file.area || "General"}
              </TableCell>
              <TableCell className="text-muted-foreground">
                {documentType(file.kind)}
              </TableCell>
              <TableCell>
                <div className="flex flex-col">
                  <span>Version {file.version}</span>
                  <span className="text-xs text-muted-foreground">
                    {file.is_current ? "Current" : "Previous revision"}
                  </span>
                </div>
              </TableCell>
              <TableCell className="pe-4">
                <Status value={file.status} />
              </TableCell>
            </TableRow>
          ))}
          {files.length === 0 ? (
            <TableRow className="hover:bg-transparent">
              <TableCell
                colSpan={5}
                className="h-24 text-center text-muted-foreground"
              >
                No documents match these filters.
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </div>
  );
}

function documentType(kind: string) {
  return (
    (
      {
        pdf: "PDF",
        spreadsheet: "Spreadsheet",
        word: "Word",
        docx: "Word",
        cad: "CAD",
      } as Record<string, string>
    )[kind] ?? kind.toUpperCase()
  );
}

function formatSize(bytes: number) {
  return bytes >= 1_048_576
    ? `${(bytes / 1_048_576).toFixed(1)} MB`
    : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
