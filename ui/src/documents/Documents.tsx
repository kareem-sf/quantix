import { useState, type FormEvent } from "react";
import { useParams, useSearchParams } from "react-router";
import { AddDocuments } from "./AddDocuments";
import {
  hasImages,
  originalFile,
  pageImage,
  useDocuments,
  usePage,
  useSearch,
  type TenderDocument,
} from "./queries";

const PROBLEM = new Set(["unreadable", "failed"]);

export function Documents() {
  const { tenderId = "" } = useParams();
  const documents = useDocuments(tenderId);
  const [params, setParams] = useSearchParams();
  const [draft, setDraft] = useState(params.get("q") ?? "");
  const query = params.get("q") ?? "";
  const open = (id: string, page = 1) => setParams({ ...(query ? { q: query } : {}), doc: id, page: String(page) });

  const current = (documents.data ?? []).filter((d) => d.status !== "replaced");
  const selected = current.find((d) => d.id === params.get("doc"));
  const problems = current.filter((d) => PROBLEM.has(d.status)).length;
  const reading = current.filter((d) => d.status === "waiting" || d.status === "reading").length;

  function search(event: FormEvent) {
    event.preventDefault();
    setParams(draft.trim() ? { q: draft.trim() } : {});
  }

  return (
    <div className="flex h-full w-full">
      <section aria-label="Documents" className="flex w-[440px] shrink-0 flex-col border-r border-line px-5 pt-7 pb-5">
        <h1 className="text-[22px] font-semibold tracking-tight">Documents</h1>
        <p className="pt-1 pb-4 text-ink-3">
          {current.length} {current.length === 1 ? "file" : "files"}
          {reading > 0 && ` · reading ${reading}`}
          {problems > 0 && ` · ${problems} can’t be read`}
        </p>
        <form onSubmit={search} className="pb-3">
          <input
            aria-label="Search the documents"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Search every page"
            className="h-9 w-full rounded-lg border border-line-strong px-3 outline-none focus:border-ink"
          />
        </form>
        <div className="min-h-0 grow overflow-y-auto">
          {query ? (
            <SearchResults tenderId={tenderId} query={query} onOpen={open} />
          ) : (
            <Groups documents={current} selected={selected?.id} onOpen={open} />
          )}
        </div>
        <div className="border-t border-line pt-4">
          <AddDocuments tenderId={tenderId} />
        </div>
      </section>
      <section aria-label="Viewer" className="flex min-w-0 grow flex-col bg-subtle px-7 py-5">
        {selected ? (
          <Viewer
            document={selected}
            number={Number(params.get("page") ?? 1)}
            onPage={(n) => open(selected.id, n)}
          />
        ) : (
          <p className="m-auto text-ink-3">Choose a document to read it here.</p>
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
      <span className="flex justify-between px-1 py-1.5 text-xs font-semibold text-ink-2">
        <span>{name}</span>
        <span className="font-normal">{items.length}</span>
      </span>
      {items.map((d) => (
        <button
          key={d.id}
          onClick={() => props.onOpen(d.id)}
          className={`flex flex-col gap-0.5 rounded-md px-2 py-[7px] text-left ${d.id === props.selected ? "bg-selected" : "hover:bg-rail"}`}
        >
          <span className="flex w-full justify-between gap-2.5">
            <span className="min-w-0 truncate font-medium" dir="auto">
              {d.name}
            </span>
            <span className="shrink-0 text-ink-3">{d.page_count ? `${d.page_count} pp` : ""}</span>
          </span>
          {d.description && <span className="text-ink-3">{d.description}</span>}
          {PROBLEM.has(d.status) && <span className="text-attention">{d.note}</span>}
          {(d.status === "waiting" || d.status === "reading") && <span className="text-ink-3">Reading…</span>}
        </button>
      ))}
    </div>
  ));
}

/** Until the office groups the package, group by the folders inside it (ignoring one folder that holds everything). */
export function folderGroup(path: string, all: TenderDocument[]) {
  const top = path.split("/")[0];
  const shared = all.length > 1 && all.every((d) => d.path.startsWith(`${top}/`));
  const folders = path.split("/").slice(shared ? 1 : 0, -1);
  return folders[0] ?? "Documents";
}

function SearchResults(props: { tenderId: string; query: string; onOpen: (id: string, page: number) => void }) {
  const hits = useSearch(props.tenderId, props.query);
  if (hits.isPending) return <p className="px-1 text-ink-3">Searching…</p>;
  if (!hits.data?.length) return <p className="px-1 text-ink-2">Nothing found for “{props.query}”.</p>;
  return hits.data.map((hit) => (
    <button
      key={`${hit.document_id}-${hit.page}`}
      onClick={() => props.onOpen(hit.document_id, hit.page)}
      className="flex w-full flex-col gap-1 rounded-md px-2 py-2 text-left hover:bg-rail"
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
  ));
}

function Viewer({ document, number, onPage }: { document: TenderDocument; number: number; onPage: (n: number) => void }) {
  const page = usePage(document.status === "read" ? document.id : undefined, number);
  const count = document.page_count ?? 1;
  const nav = "flex size-[30px] items-center justify-center rounded-md border border-line-strong bg-white disabled:text-ink-4";

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between pb-3.5">
        <span className="min-w-0 truncate" dir="auto">
          <span className="font-medium">{document.name}</span>
          <span className="text-ink-3">
            {" "}
            · page {number} of {count}
          </span>
        </span>
        <span className="flex shrink-0 gap-1.5">
          <button aria-label="Previous page" disabled={number <= 1} onClick={() => onPage(number - 1)} className={nav}>
            ‹
          </button>
          <button aria-label="Next page" disabled={number >= count} onClick={() => onPage(number + 1)} className={nav}>
            ›
          </button>
          <a
            href={originalFile(document.id)}
            className="flex h-[30px] items-center rounded-md border border-line-strong bg-white px-3"
          >
            Open original
          </a>
        </span>
      </div>
      {document.status !== "read" ? (
        <p className="m-auto text-ink-2">{document.note ?? "This file is still being read."}</p>
      ) : hasImages(document) ? (
        <img
          src={pageImage(document.id, number)}
          alt={`${document.name}, page ${number}`}
          className="mx-auto min-h-0 max-w-full grow rounded-md bg-white object-contain shadow-sm"
        />
      ) : (
        <div
          dir="auto"
          className="mx-auto min-h-0 w-full max-w-[760px] grow overflow-y-auto rounded-md bg-white p-8 leading-relaxed whitespace-pre-wrap shadow-sm"
        >
          {page.data?.text}
        </div>
      )}
    </div>
  );
}
