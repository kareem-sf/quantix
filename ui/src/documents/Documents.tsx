import {
  IconFile,
  IconFileSpreadsheet,
  IconFileTypePdf,
  IconFileVector,
  IconFileWord,
  IconPhoto,
  IconX,
} from "@tabler/icons-react";
import { useState, type FormEvent } from "react";
import { useParams, useSearchParams } from "react-router";
import { AddDocuments } from "./AddDocuments";
import { useDocuments, useSearch, type TenderDocument } from "./queries";
import { Viewer } from "./Viewer";

const PROBLEM = new Set(["unreadable", "failed"]);
const ICONS: Record<string, typeof IconFile> = {
  pdf: IconFileTypePdf,
  spreadsheet: IconFileSpreadsheet,
  word: IconFileWord,
  cad: IconFileVector,
  image: IconPhoto,
};

export function Documents() {
  const { tenderId = "" } = useParams();
  const documents = useDocuments(tenderId);
  const [params, setParams] = useSearchParams();
  const [draft, setDraft] = useState(params.get("q") ?? "");
  const query = params.get("q") ?? "";
  // moving within a document replaces the address rather than adding a step to go back through
  const open = (id: string, page = 1, replace = false) =>
    setParams({ ...(query ? { q: query } : {}), doc: id, page: String(page) }, { replace });
  const page = Math.max(1, Math.floor(Number(params.get("page") ?? 1)) || 1);

  const current = (documents.data ?? []).filter((d) => d.status !== "replaced");
  const selected = current.find((d) => d.id === params.get("doc"));
  const problems = current.filter((d) => PROBLEM.has(d.status)).length;
  const reading = current.filter((d) => d.status === "waiting" || d.status === "reading").length;

  function search(event: FormEvent) {
    event.preventDefault();
    const q = draft.trim();
    setParams({ ...(q ? { q } : {}), ...(selected ? { doc: selected.id, page: String(page) } : {}) });
  }
  function clear() {
    setDraft("");
    setParams(selected ? { doc: selected.id, page: String(page) } : {});
  }

  return (
    <div className="flex h-full w-full">
      <section aria-label="Documents" className="flex w-[280px] shrink-0 xl:w-[320px] flex-col border-r border-line pt-6">
        <div className="px-4">
          <h1 className="text-[22px] font-semibold tracking-tight">Documents</h1>
          <p className="pt-1 pb-3.5 text-ink-3">
            {current.length} {current.length === 1 ? "file" : "files"}
            {reading > 0 && ` · reading ${reading}`}
            {problems > 0 && ` · ${problems} can’t be read`}
          </p>
          <form role="search" onSubmit={search} className="relative pb-2">
            <input
              aria-label="Search the documents"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && query && clear()}
              placeholder="Search every page"
              className="h-9 w-full rounded-lg border border-line-strong pr-8 pl-3 outline-none focus:border-ink"
            />
            {(draft || query) && (
              <button
                type="button"
                aria-label="Clear the search"
                onClick={clear}
                className="absolute top-[7px] right-1.5 flex size-[22px] items-center justify-center rounded text-ink-3 hover:bg-subtle hover:text-ink"
              >
                <IconX className="size-3.5" stroke={2} />
              </button>
            )}
          </form>
        </div>
        <div className="min-h-0 grow overflow-y-auto px-2.5 pb-3">
          {query ? (
            <SearchResults tenderId={tenderId} query={query} selected={selected?.id} page={page} onOpen={open} />
          ) : (
            <Groups documents={current} selected={selected?.id} onOpen={open} />
          )}
        </div>
        <div className="border-t border-line px-4 py-3.5">
          <AddDocuments tenderId={tenderId} />
        </div>
      </section>
      <section aria-label="Viewer" className="flex min-w-0 grow flex-col bg-subtle">
        {selected ? (
          <Viewer document={selected} page={page} query={query} onPage={(n) => open(selected.id, n, true)} />
        ) : (
          <p className="m-auto text-ink-3">Choose a document to see it here.</p>
        )}
      </section>
    </div>
  );
}

function Groups(props: { documents: TenderDocument[]; selected?: string; onOpen: (id: string) => void }) {
  const groups = new Map<string, TenderDocument[]>();
  for (const d of props.documents) {
    const name = d.group_name ?? folderGroup(d.path, props.documents);
    groups.set(name, [...(groups.get(name) ?? []), d]);
  }
  return [...groups].map(([name, items]) => (
    <div key={name} className="flex flex-col py-1.5">
      <span className="flex justify-between px-1.5 py-1.5 text-xs font-semibold text-ink-2">
        <span dir="auto">{name}</span>
        <span className="font-normal">{items.length}</span>
      </span>
      {items.map((d) => {
        const Icon = ICONS[d.kind] ?? IconFile;
        return (
          <button
            key={d.id}
            onClick={() => props.onOpen(d.id)}
            aria-current={d.id === props.selected ? "true" : undefined}
            className={`flex gap-2 rounded-md px-1.5 py-[7px] text-left ${d.id === props.selected ? "bg-selected" : "hover:bg-rail"}`}
          >
            <Icon className="mt-px size-4 shrink-0 text-ink-3" stroke={1.5} aria-hidden />
            <span className="flex min-w-0 grow flex-col gap-0.5">
              <span className="flex w-full justify-between gap-2.5">
                <span className="line-clamp-2 min-w-0 font-medium [overflow-wrap:anywhere]" dir="auto" title={d.name}>
                  {d.name}
                </span>
                <span className="shrink-0 text-ink-3">{d.kind === "pdf" && d.page_count ? `${d.page_count} pp` : ""}</span>
              </span>
              {subfolder(d.path, props.documents) && (
                <span className="text-xs text-ink-3" dir="auto">
                  {subfolder(d.path, props.documents)}
                </span>
              )}
              {d.description && <span className="text-ink-3">{d.description}</span>}
              {coverage(d) && <span className="text-xs text-ink-3">{coverage(d)}</span>}
              {PROBLEM.has(d.status) && <span className="text-attention">{d.note}</span>}
              {d.status === "waiting" && <span className="text-ink-3">Waiting to be read</span>}
              {d.status === "reading" && <span className="text-ink-3">Reading…</span>}
            </span>
          </button>
        );
      })}
    </div>
  ));
}

/** How much of the document was read, kept apart: scans still to read, pages the office opened, pages it cites. */
function coverage(d: TenderDocument) {
  const pages = (n: number) => `${n} ${n === 1 ? "page" : "pages"}`;
  return [
    d.scans_to_read ? `${pages(d.scans_to_read)} still to read by OCR` : "",
    d.opened ? `${pages(d.opened)} opened by the office` : "",
    d.cited ? `${d.cited} cited` : "",
  ]
    .filter(Boolean)
    .join(" · ");
}

/** Until the office groups the package, group by the folders inside it (ignoring one folder that holds everything). */
export function folderGroup(path: string, all: TenderDocument[]) {
  const top = path.split("/")[0];
  const shared = all.length > 1 && all.every((d) => d.path.startsWith(`${top}/`));
  const folders = path.split("/").slice(shared ? 1 : 0, -1);
  return folders[0] ?? "Documents";
}

/** The folders below the group, e.g. "Rev01 (Update)", so revisions of the same file can be told apart. */
export function subfolder(path: string, all: TenderDocument[]) {
  const top = path.split("/")[0];
  const shared = all.length > 1 && all.every((d) => d.path.startsWith(`${top}/`));
  return path.split("/").slice(shared ? 2 : 1, -1).join(" / ");
}

function SearchResults(props: {
  tenderId: string;
  query: string;
  selected?: string;
  page: number;
  onOpen: (id: string, page: number) => void;
}) {
  const hits = useSearch(props.tenderId, props.query);
  if (hits.isPending) return <p className="px-1.5 py-2 text-ink-3">Searching…</p>;
  if (hits.isError) return <p className="px-1.5 py-2 text-attention">{hits.error.message}</p>;
  if (!hits.data?.length) return <p className="px-1.5 py-2 text-ink-2">Nothing found for “{props.query}”.</p>;
  return (
    <>
      <p className="px-1.5 py-2 text-xs text-ink-3">
        {hits.data.length} {hits.data.length === 1 ? "page" : "pages"} found
      </p>
      {hits.data.map((hit) => {
        const chosen = hit.document_id === props.selected && hit.page === props.page;
        return (
          <button
            key={`${hit.document_id}-${hit.page}`}
            onClick={() => props.onOpen(hit.document_id, hit.page)}
            aria-current={chosen ? "true" : undefined}
            className={`flex w-full flex-col gap-1 rounded-md px-1.5 py-2 text-left ${chosen ? "bg-selected" : "hover:bg-rail"}`}
          >
            <span className="font-medium" dir="auto">
              {hit.name} <span className="font-normal text-ink-3">· page {hit.page}</span>
            </span>
            <span className="leading-normal text-ink-2" dir="auto">
              {hit.snippet.split(/(\[[^\]]*\])/).map((part, i) =>
                part.startsWith("[") ? (
                  <mark key={i} className="rounded-sm bg-attention/15 text-ink">
                    {part.slice(1, -1)}
                  </mark>
                ) : (
                  part
                ),
              )}
            </span>
          </button>
        );
      })}
    </>
  );
}
