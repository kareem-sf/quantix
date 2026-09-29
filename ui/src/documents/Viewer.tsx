import { IconChevronLeft, IconChevronRight, IconMinus, IconPlus } from "@tabler/icons-react";
import { lazy, Suspense, useEffect, useState, type ReactNode } from "react";
import { CadView } from "./CadView";
import { ImageView } from "./ImageView";
import { originalFile, pageImage, useOpenOriginal, type TenderDocument } from "./queries";
import { SheetView } from "./SheetView";

// PDF.js and the Word renderer are large: they load when a PDF or a Word document is first opened
const PdfView = lazy(() => import("./PdfView"));
const WordView = lazy(() => import("./WordView"));

/** What a file is, in plain words, and how much of it there is. */
function about(document: TenderDocument) {
  const count = document.page_count ?? 0;
  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
  switch (document.kind) {
    case "pdf":
      return count ? `PDF · ${plural(count, "page")}` : "PDF";
    case "spreadsheet":
      return count ? `Workbook · ${plural(count, "sheet")}` : "Workbook";
    case "word":
      return "Word document";
    case "cad":
      return "Drawing";
    case "image":
      return "Image";
    default:
      return "";
  }
}

/** A document opened as its own app would show it: PDFs and Word documents as pages, workbooks as sheets,
 * drawings drawn, images zoomable. The page (a sheet, a layout, a part of a Word document) comes from the address,
 * so every source opens exactly where it points. */
export function Viewer(props: {
  document: TenderDocument;
  page: number;
  query: string;
  onPage: (page: number) => void;
}) {
  const { document } = props;
  const open = useOpenOriginal();
  const [toolbar, setToolbar] = useState<HTMLElement | null>(null);
  useEffect(() => open.reset(), [document.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const busy = document.status === "waiting" || document.status === "reading";
  const problem = document.status === "unreadable" || document.status === "failed";
  const viewable = ["pdf", "word", "spreadsheet", "image"].includes(document.kind) || (document.kind === "cad" && document.status === "read");
  const view = !viewable ? null : document.kind === "pdf" ? (
    <PdfView
      key={document.id}
      url={originalFile(document.id)}
      name={document.name}
      page={props.page}
      query={props.query}
      toolbar={toolbar}
      onPage={props.onPage}
    />
  ) : document.kind === "word" ? (
    <WordView key={document.id} documentId={document.id} url={originalFile(document.id)} page={props.page} toolbar={toolbar} />
  ) : document.kind === "spreadsheet" ? (
    <SheetView key={document.id} documentId={document.id} page={props.page} query={props.query} toolbar={toolbar} onPage={props.onPage} />
  ) : document.kind === "cad" ? (
    <CadView key={document.id} documentId={document.id} page={props.page} onPage={props.onPage} />
  ) : (
    <ImageView key={document.id} src={pageImage(document.id, 1)} name={document.name} toolbar={toolbar} />
  );

  return (
    <div className="flex h-full min-w-0 flex-col">
      <header className="flex min-h-[52px] flex-wrap items-center gap-x-3 gap-y-2 border-b border-line bg-white px-5 py-2.5">
        <span className="flex min-w-[180px] flex-1 flex-col">
          <span className="truncate font-medium" dir="auto" title={document.path}>
            {document.name}
          </span>
          <span className="truncate text-xs text-ink-3">
            {about(document)}
            {busy && " · Quantix is still reading it"}
          </span>
        </span>
        <span className="flex shrink-0 items-center gap-3">
          <span ref={setToolbar} className="flex items-center gap-2" />
          <button
            onClick={() => open.mutate(document.id)}
            disabled={open.isPending}
            title="Opens in the app your computer uses for this type of file. Quantix keeps the file as supplied."
            className="h-[30px] rounded-md border border-line-strong bg-white px-3 hover:bg-subtle disabled:text-ink-4"
          >
            Open original
          </button>
        </span>
      </header>
      {open.isError && (
        <p role="alert" className="border-b border-line bg-white px-5 py-2 text-attention">
          {open.error.message}
        </p>
      )}
      {problem && viewable && (
        <p className="border-b border-line bg-white px-5 py-2 text-attention">{document.note}</p>
      )}
      <div className="relative flex min-h-0 grow flex-col bg-subtle">
        {view ? (
          <Suspense fallback={<Waiting>Opening…</Waiting>}>{view}</Suspense>
        ) : (
          <Waiting>{document.note ?? "Quantix is still reading this file. It shows here once it is read."}</Waiting>
        )}
      </div>
    </div>
  );
}

export function Waiting({ children }: { children: ReactNode }) {
  return <p className="m-auto max-w-sm px-6 text-center text-ink-2">{children}</p>;
}

const group = "flex h-[30px] items-center rounded-md border border-line-strong bg-white";
const step = "flex h-full w-7 items-center justify-center text-ink-2 hover:bg-subtle hover:text-ink disabled:text-ink-4 disabled:hover:bg-transparent";

/** Page ‹ n › of N, with the number typed to jump. */
export function PageControls(props: { page: number; count: number; onPage: (page: number) => void }) {
  const [draft, setDraft] = useState(String(props.page));
  useEffect(() => setDraft(String(props.page)), [props.page]);
  const go = () => {
    const n = Math.round(Number(draft));
    if (Number.isFinite(n) && n >= 1 && n <= props.count) props.onPage(n);
    else setDraft(String(props.page));
  };
  return (
    <span className="flex items-center gap-1.5 text-ink-2">
      <span className={group}>
        <button aria-label="Previous page" disabled={props.page <= 1} onClick={() => props.onPage(props.page - 1)} className={`${step} rounded-l-md`}>
          <IconChevronLeft className="size-4" stroke={1.75} />
        </button>
        <input
          aria-label="Page"
          value={draft}
          inputMode="numeric"
          onChange={(e) => setDraft(e.target.value)}
          onBlur={go}
          onKeyDown={(e) => {
            if (e.key === "Enter") go();
            if (e.key === "Escape") setDraft(String(props.page));
          }}
          className="h-full w-11 border-x border-line-strong text-center text-ink outline-none focus:bg-subtle"
        />
        <button
          aria-label="Next page"
          disabled={props.page >= props.count}
          onClick={() => props.onPage(props.page + 1)}
          className={`${step} rounded-r-md`}
        >
          <IconChevronRight className="size-4" stroke={1.75} />
        </button>
      </span>
      <span className="whitespace-nowrap">of {props.count.toLocaleString("en-US")}</span>
    </span>
  );
}

/** Zoom out, the zoom, zoom in, and fit. */
export function ZoomControls(props: {
  scale: number | null;
  onOut: () => void;
  onIn: () => void;
  onFit: () => void;
  fit?: string; // what fitting does, e.g. "Fit the whole page"
}) {
  return (
    <span className="flex items-center gap-1.5">
      <span className={group}>
        <button aria-label="Zoom out" onClick={props.onOut} className={`${step} rounded-l-md`}>
          <IconMinus className="size-3.5" stroke={2} />
        </button>
        <span aria-label="Zoom" className="w-12 border-x border-line-strong text-center text-ink-2">
          {props.scale === null ? "–" : `${Math.round(props.scale * 100)}%`}
        </span>
        <button aria-label="Zoom in" onClick={props.onIn} className={`${step} rounded-r-md`}>
          <IconPlus className="size-3.5" stroke={2} />
        </button>
      </span>
      <button onClick={props.onFit} title={props.fit} className="h-[30px] rounded-md px-2 text-ink-2 hover:bg-white hover:text-ink">
        Fit
      </button>
    </span>
  );
}

/** Tabs along the foot of a workbook or a drawing, as Excel and CAD programs show sheets and layouts. */
export function Tabs(props: { label: string; tabs: { name: string; muted?: boolean }[]; current: number; onPick: (n: number) => void }) {
  if (props.tabs.length <= 1) return null;
  return (
    <nav aria-label={props.label} className="flex shrink-0 gap-0.5 overflow-x-auto border-t border-line bg-rail px-2 py-1.5">
      {props.tabs.map((tab, i) => (
        <button
          key={i}
          aria-current={props.current === i + 1 ? "page" : undefined}
          onClick={() => props.onPick(i + 1)}
          dir="auto"
          className={`h-7 shrink-0 rounded-md px-3 whitespace-nowrap ${
            props.current === i + 1
              ? "bg-white font-medium text-ink shadow-[0_0_0_1px_var(--color-line-strong)]"
              : `${tab.muted ? "text-ink-4 italic" : "text-ink-2"} hover:bg-white hover:text-ink`
          }`}
        >
          {tab.name}
        </button>
      ))}
    </nav>
  );
}

/** Zoom steps shared by the sheet, Word and image views. */
const STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4, 5];
export function zoomStep(scale: number, direction: 1 | -1) {
  const next = direction > 0 ? STEPS.find((s) => s > scale + 0.001) : [...STEPS].reverse().find((s) => s < scale - 0.001);
  return next ?? scale;
}

/** An element's inner size, kept up to date as the window and panels change. Zero until it is laid out. */
export function useSize(element: HTMLElement | null) {
  const [size, setSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => setSize({ width: element.clientWidth, height: element.clientHeight }));
    observer.observe(element);
    return () => observer.disconnect();
  }, [element]);
  return size;
}

/** Ctrl + wheel (and a touchpad pinch, which arrives the same way) zooms, as in every document app. */
export function useCtrlWheel(element: HTMLElement | null, onZoom: (factor: number, event: WheelEvent) => void) {
  useEffect(() => {
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      onZoom(Math.exp(-event.deltaY / 400), event);
    };
    element.addEventListener("wheel", wheel, { passive: false });
    return () => element.removeEventListener("wheel", wheel);
  }, [element, onZoom]);
}

/** The words of a search worth marking in what it found. */
export function searchTerms(query: string) {
  return query
    .split(/\s+/)
    .map((word) => word.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter((word) => word.length >= 3);
}

export function termsPattern(terms: string[]) {
  if (!terms.length) return null;
  return new RegExp(`(${terms.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "giu");
}
