import { IconChevronLeft, IconChevronRight } from "@tabler/icons-react";
import { useEffect, useMemo, useState, type MouseEvent } from "react";
import { useParams, useSearchParams } from "react-router";
import { pageImage, useDocuments } from "../documents/queries";
import { useBoq, quantity as formatQuantity } from "../estimate/queries";
import { firstName, useOffice } from "../office/queries";
import { APPROVE, Findings, Reopen, ReviewNote, SendBack, WITH_MANAGER } from "../review/Review";
import { DrawingWork } from "./TenderQueries";
import { CadDrawing, LayerList, type Box, type Shape } from "./CadDrawing";
import {
  CLOSED,
  alike,
  boxAround,
  findWords,
  onLayer,
  useChosen,
  useDrawingInfo,
  useMeasureObjects,
  useRegion,
  useRooms,
  useScreenCopy,
  useSetUnits,
  type ScreenCopy,
} from "./cad";
import {
  RESULTS,
  UNITS,
  percent,
  snap,
  useDecideMeasurement,
  useDecideScale,
  useMeasure,
  useRemoveMeasurement,
  useSetScale,
  useSheet,
  useTakeoff,
  useVertices,
  type Kind,
  type Measurement,
  type Point,
  type Sheet,
} from "./queries";

type Tool = "select" | Exclude<Kind, "volume"> | "enclosed" | "scale";
const TOOLS: [Tool, string][] = [
  ["select", "Select"],
  ["length", "Length"],
  ["area", "Area"],
  ["enclosed", "Enclosed"],
  ["count", "Count"],
  ["scale", "Scale"],
];
const LEAST: Record<Tool, number> = { select: 0, length: 2, area: 3, enclosed: 3, count: 1, scale: 2 };

/** A drawing's screen copy lies about its page's centre; a PDF's measurements are in points from the top left of the
 * sheet, a CAD drawing's in its own units. */
function frameOf(copy: ScreenCopy, sheet: Sheet) {
  const [ox, oy] = copy.header.origin;
  const pdf = sheet.kind === "pdf";
  const round = (v: number) => Math.round(v * 1000) / 1000;
  return {
    toRecord: ([x, y]: Point): Point => (pdf ? [round(x + ox), round(sheet.height - (y + oy))] : [round(x + ox), round(y + oy)]),
    fromRecord: ([x, y]: Point): Point => (pdf ? [x - ox, sheet.height - y - oy] : [x - ox, y - oy]),
    absolute: ([x, y]: Point): Point => [x + ox, y + oy],
    relative: ([x, y]: Point): Point => [x - ox, y - oy],
  };
}

export function Takeoff() {
  const { tenderId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const takeoff = useTakeoff(tenderId);
  const documentId = params.get("doc");
  const page = Number(params.get("page") ?? 1);
  const sheet = useSheet(documentId, page);
  const isCad = sheet.data?.kind === "cad";
  // a CAD drawing, or a PDF printed from CAD, is drawn from its own lines; a scan is shown as a picture
  const vector = isCad || Boolean(sheet.data?.lines);
  const vertices = useVertices(documentId, page, sheet.data?.kind === "pdf" && !vector);
  const [tool, setTool] = useState<Tool>("select");
  const [draft, setDraft] = useState<Point[]>([]);
  const [finished, setFinished] = useState(false);
  const [zoom, setZoom] = useState(1);
  const selected = params.get("m");

  // what a check found, shown from the Queries screen
  const shown = useMemo(() => (params.get("show") ?? "").split(",").filter(Boolean).map(Number), [params]);
  const copy = useScreenCopy(documentId, page, vector);
  const info = useDrawingInfo(documentId, page, vector);
  const rooms = useRooms(documentId, isCad && page === 1);
  const region = useRegion(documentId ?? "", page);
  const [chosen, setChosen] = useState<number[]>([]);
  const [hidden, setHidden] = useState<Set<number>>(new Set());
  const [words, setWords] = useState("");
  const [nth, setNth] = useState(0);
  useEffect(() => {
    setChosen([]);
    setWords("");
    const names = new Set(copy.data?.header.hidden ?? []);
    setHidden(new Set((copy.data?.header.layers ?? []).flatMap((name, i) => (names.has(name) ? [i] : []))));
  }, [copy.data]);
  useEffect(() => {
    setTool("select");
    setDraft([]);
    setFinished(false);
  }, [documentId, page]);
  const meanings = useMemo(
    () => new Map((info.data?.layers ?? []).flatMap((l) => (l.meaning ? [[l.name.toLowerCase(), l.meaning]] : []))),
    [info.data],
  );

  const onSheet = (takeoff.data?.measurements ?? []).filter((m) => m.document_id === documentId && m.page === page);
  const open = (doc: string, number: number) => setParams({ doc, page: String(number) });
  // no sheet chosen: open the one waiting for the engineer, else the last measured, else the package's first drawing
  const documents = useDocuments(tenderId);
  useEffect(() => {
    if (documentId || !takeoff.data || !documents.data) return;
    const waiting = [
      ...takeoff.data.sheets.filter((s) => s.scale?.status === "reviewed"),
      ...takeoff.data.measurements.filter((m) => m.status === "reviewed"),
    ][0];
    const measured = takeoff.data.sheets.at(-1);
    const drawing = documents.data.find((d) => d.kind === "cad" && d.status === "read") ??
      documents.data.find((d) => d.kind === "pdf" && d.status === "read");
    const first = waiting ?? measured ?? (drawing && { document_id: drawing.id, page: 1 });
    if (first) setParams({ doc: first.document_id, page: String(first.page) }, { replace: true });
  }, [documentId, takeoff.data, documents.data, setParams]);
  const pickObject = (object: number | null, add: boolean) => {
    if (object === null) return setChosen(add ? chosen : []);
    if (!add) return setChosen([object]);
    setChosen(chosen.includes(object) ? chosen.filter((o) => o !== object) : [...chosen, object]);
  };
  const choose = (next: Tool) => {
    setTool(next);
    setDraft([]);
    setFinished(false);
    region.reset();
  };
  const addPoint = (point: Point) => {
    if (tool === "select" || finished) return;
    const points = [...draft, point];
    setDraft(points);
    if (tool === "scale" && points.length === 2) setFinished(true);
  };

  const frame = copy.data && sheet.data ? frameOf(copy.data, sheet.data) : null;
  const tools = TOOLS.filter(([key]) => {
    if (!vector) return key !== "enclosed";
    if (isCad) return key === "select" || (key !== "scale" && page === 1); // points only in model space, full size
    return true;
  });
  const metres = sheet.data?.scale?.metres_per_point ?? null; // a PDF's scale, or a CAD drawing's units
  const found = useMemo(() => (copy.data ? findWords(copy.data, words) : []), [copy.data, words]);
  const shapes: Shape[] = frame
    ? onSheet
        .filter((m) => (m.object_count === null || m.object_count === undefined) && m.points.length)
        .map((m) => ({
          id: m.id,
          kind: m.kind,
          points: m.points.map((p) => frame.fromRecord(p as Point)),
          selected: m.id === selected,
          waiting: m.status === "reviewed",
        }))
    : [];
  const focus = useMemo(() => {
    if (!copy.data) return null;
    const m = onSheet.find((x) => x.id === selected);
    if (found.length && words.trim()) {
      const box = boxAround(copy.data, [found[nth % found.length]]);
      return box ? { box, key: `word-${words}-${nth}` } : null;
    }
    if (m?.objects?.length) {
      const box = boxAround(copy.data, m.objects);
      return box ? { box, key: `m-${m.id}` } : null;
    }
    const shape = shapes.find((s) => s.id === selected);
    if (shape) {
      const xs = shape.points.map((p) => p[0]);
      const ys = shape.points.map((p) => p[1]);
      return { box: [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)] as Box, key: `m-${shape.id}` };
    }
    if (shown.length) {
      const box = boxAround(copy.data, shown);
      return box ? { box, key: `shown-${shown.join(",")}` } : null;
    }
    return null;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [copy.data, selected, found, nth, shown, takeoff.data]);

  // keys, as in CAD: Esc lets go, Enter finishes, Backspace takes the last point back
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)) return;
      if (event.key === "Escape") {
        if (tool !== "select" && !finished && draft.length) setDraft([]);
        else if (chosen.length) setChosen([]);
        else if (tool !== "select") choose("select");
      } else if (event.key === "Enter" && tool !== "select" && !finished && draft.length >= LEAST[tool] && tool !== "scale") {
        setFinished(true);
      } else if (event.key === "Backspace" && tool !== "select" && !finished && draft.length) {
        setDraft(draft.slice(0, -1));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const placing = tool !== "select" && !finished;
  const toolbar = (
    <div role="toolbar" aria-label="Measuring tools" className="flex gap-0.5 rounded-lg bg-white p-[3px] shadow-[0_0_0_1px_var(--color-line-strong)]">
      {tools.map(([key, label]) => (
        <button
          key={key}
          aria-pressed={tool === key}
          onClick={() => choose(key)}
          className={`h-7 rounded-md px-2.5 text-[13px] ${tool === key ? "bg-ink text-white" : "text-ink-2"}`}
        >
          {label}
        </button>
      ))}
    </div>
  );
  const prompt = placing && (
    <p className="pb-2 text-ink-2">
      {tool === "scale"
        ? "Click both ends of a dimension printed on the drawing."
        : tool === "enclosed"
          ? "Click inside an area the drawing’s lines close off. Hide the layers whose lines cross it first."
          : `Click the points of the ${tool}${tool === "count" ? " (one per item)" : ""}${vector ? "; shift keeps it square" : ""}, then finish.`}{" "}
      {vector && tool !== "enclosed" && draft.length > 0 && metres !== null && tool !== "count" && (
        <span className="text-ink">{soFar(tool, draft, metres)} </span>
      )}
      {draft.length >= LEAST[tool] && tool !== "scale" && tool !== "enclosed" && (
        <button onClick={() => setFinished(true)} className="font-medium text-ink underline underline-offset-4">
          Finish
        </button>
      )}
      {region.isPending && <span className="text-ink-3">Finding the outline…</span>}
      {region.isError && <span className="text-attention">{region.error.message}</span>}
    </p>
  );

  return (
    // on a narrow screen the measurements sit under the drawing, which keeps the whole width
    <div className="flex h-full w-full @max-3xl:flex-col">
      <section aria-label="Drawing" className="flex min-h-0 min-w-0 grow flex-col bg-subtle px-6 py-5 @max-3xl:overflow-hidden">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 pb-3">
          <h1 className="text-[24px] font-semibold tracking-tight">Takeoff</h1>
          <SheetPicker
            tenderId={tenderId}
            sheets={takeoff.data?.sheets ?? []}
            needsYou={
              new Set([
                ...(takeoff.data?.sheets ?? []).filter((s) => s.scale?.status === "reviewed"),
                ...(takeoff.data?.measurements ?? []).filter((m) => m.status === "reviewed"),
              ].map((s) => `${s.document_id}|${s.page}`))
            }
            current={{ documentId, page }}
            onOpen={open}
          />
        </div>
        {sheet.data && vector ? (
          <>
            <div className="flex flex-wrap items-center justify-between gap-3 pb-2">
              {toolbar}
              <FindWords
                value={words}
                count={found.length}
                nth={found.length ? nth % found.length : 0}
                onChange={(v) => {
                  setWords(v);
                  setNth(0);
                }}
                onNext={() => setNth(nth + 1)}
              />
            </div>
            <div className="flex flex-wrap items-center justify-between gap-3 pb-2">
              {isCad ? (
                <UnitsNote tenderId={tenderId} documentId={sheet.data.document_id} scale={sheet.data.scale} header={info.data?.header_units} />
              ) : (
                <ScaleNote tenderId={tenderId} sheet={sheet.data} />
              )}
              {chosen.length > 0 && (
                <button onClick={() => setChosen([])} className="text-ink-2 underline underline-offset-4">
                  Clear the choice
                </button>
              )}
            </div>
            {tool === "select" && (
              <p className="pb-2 text-ink-3">
                Click objects to choose them, shift-click to add more, shift-drag a box to choose what it holds. Scroll
                to zoom, drag to move. Quantix measures what you choose from the drawing itself.
              </p>
            )}
            {prompt}
            <div className="min-h-0 grow">
              {copy.data && frame ? (
                <CadDrawing
                  copy={copy.data}
                  hiddenLayers={hidden}
                  marked={onSheet.find((m) => m.id === selected)?.objects ?? shown}
                  chosen={chosen}
                  rooms={rooms.data}
                  onPick={tool === "select" ? pickObject : undefined}
                  onWindow={tool === "select" ? (objects) => setChosen([...new Set([...chosen, ...objects])]) : undefined}
                  shapes={shapes}
                  draft={draft}
                  closed={tool === "area" || tool === "enclosed"}
                  snap={tool !== "enclosed"}
                  onPoint={
                    !placing
                      ? undefined
                      : tool === "enclosed"
                        ? (point, visible) => {
                            if (region.isPending) return;
                            const [x0, y0] = frame.absolute([visible[0], visible[1]]);
                            const [x1, y1] = frame.absolute([visible[2], visible[3]]);
                            region.mutate(
                              {
                                stamp: copy.data!.header.stamp,
                                point: frame.absolute(point),
                                hidden: [...hidden].map((layer) => copy.data!.header.layers[layer]),
                                within: [x0, y0, x1, y1],
                              },
                              {
                                onSuccess: (found) => {
                                  setDraft(found.ring.map((p) => frame.relative(p as Point)));
                                  setFinished(true);
                                },
                              },
                            );
                          }
                        : addPoint
                  }
                  focus={focus}
                  found={words.trim() && found.length ? [found[nth % found.length]] : undefined}
                />
              ) : (
                <p className="text-ink-2">{copy.isError ? copy.error.message : "Opening the drawing…"}</p>
              )}
            </div>
          </>
        ) : sheet.data ? (
          <>
            <div className="flex flex-wrap items-center justify-between gap-3 pb-2">
              {toolbar}
              <span className="flex gap-1">
                {[1, 1.5, 2].map((z) => (
                  <button
                    key={z}
                    onClick={() => setZoom(z)}
                    className={`h-7 rounded-md px-2 text-xs ${zoom === z ? "bg-white font-semibold shadow-sm" : "text-ink-2"}`}
                  >
                    {z * 100}%
                  </button>
                ))}
              </span>
            </div>
            <div className="pb-2">
              <ScaleNote tenderId={tenderId} sheet={sheet.data} />
            </div>
            {prompt}
            <div className="min-h-0 grow overflow-auto">
              <Drawing
                sheet={sheet.data}
                zoom={zoom}
                vertices={vertices.data ?? []}
                measurements={onSheet}
                selected={selected}
                draft={draft}
                drawing={placing}
                onPoint={addPoint}
                onSelect={(id) => setParams({ doc: documentId!, page: String(page), m: id })}
              />
            </div>
          </>
        ) : (
          <p className="m-auto max-w-sm text-center text-ink-2">
            Choose a sheet above. The team’s measurements appear on it, and you can measure too.
          </p>
        )}
      </section>
      <aside
        aria-label="Measurements"
        className="flex w-[320px] shrink-0 flex-col gap-3 overflow-y-auto border-l border-line px-5 pt-7 pb-5 @max-3xl:h-[38%] @max-3xl:w-full @max-3xl:border-t @max-3xl:border-l-0 @max-3xl:pt-4"
      >
        {vector && sheet.data && copy.data && chosen.length > 0 ? (
          <ObjectsForm
            tenderId={tenderId}
            documentId={sheet.data.document_id}
            page={page}
            copy={copy.data}
            chosen={chosen}
            needs={isCad ? "units" : "scale"}
            onAlike={() => setChosen(alike(copy.data!, hidden, chosen))}
            onDone={() => setChosen([])}
          />
        ) : finished && sheet.data ? (
          tool === "scale" ? (
            <ScaleForm
              tenderId={tenderId}
              sheet={sheet.data}
              line={frame && vector ? draft.map(frame.toRecord) : draft}
              onDone={() => choose("select")}
            />
          ) : (
            <MeasureForm
              tenderId={tenderId}
              sheet={sheet.data}
              kind={tool === "enclosed" ? "area" : (tool as Exclude<Kind, "volume">)}
              points={frame && vector ? draft.map(frame.toRecord) : draft}
              note={tool === "enclosed" && region.data ? enclosed(region.data, isCad) : undefined}
              onDone={() => choose("select")}
            />
          )
        ) : vector && copy.data ? (
          <>
            <SheetPanel
              tenderId={tenderId}
              measurements={onSheet}
              selected={selected}
              needs={isCad ? "needs units" : "needs a scale"}
              footer={
                isCad
                  ? "Quantix calculates every length, area, volume and count from the drawing’s own objects and its units."
                  : "Quantix calculates every length, area and count from the drawing’s own lines, or the marks, and the sheet’s scale."
              }
              onShow={(id) => setParams({ doc: documentId!, page: String(page), m: id })}
            />
            <LayerList
              copy={copy.data}
              hidden={hidden}
              meanings={meanings}
              onToggle={(layer) => {
                const next = new Set(hidden);
                if (next.has(layer)) next.delete(layer);
                else next.add(layer);
                setHidden(next);
              }}
              onChoose={(layer) => setChosen(onLayer(copy.data!, layer))}
            />
            {documentId && isCad && <DrawingWork tenderId={tenderId} documentId={documentId} />}
          </>
        ) : (
          <SheetPanel
            tenderId={tenderId}
            measurements={onSheet}
            selected={selected}
            needs="needs a scale"
            footer="Quantix calculates every length, area and count from the marks and the sheet’s scale."
          />
        )}
      </aside>
    </div>
  );
}

/** The region Quantix found around a click, in words. */
function enclosed(found: { area_m2?: number | null; perimeter_m?: number | null }, cad: boolean): string {
  if (found.area_m2 === null || found.area_m2 === undefined)
    return `The lines close off this area. Set the ${cad ? "units" : "scale"} to measure it.`;
  return `The lines close off ${formatQuantity(String(found.area_m2))} m², ${formatQuantity(String(found.perimeter_m))} m round.`;
}

/** What the points placed so far measure, as the engineer places them. Quantix works the saved figure out again. */
function soFar(tool: Tool, points: Point[], metres: number): string {
  if (tool === "area" && points.length >= 3) {
    let twice = 0;
    points.forEach(([x, y], i) => {
      const [px, py] = points[(i + points.length - 1) % points.length];
      twice += px * y - x * py;
    });
    return `${formatQuantity(String(Math.abs(twice / 2) * metres * metres))} m² so far`;
  }
  let length = 0;
  for (let i = 1; i < points.length; i++)
    length += Math.hypot(points[i][0] - points[i - 1][0], points[i][1] - points[i - 1][1]);
  return points.length > 1 ? `${formatQuantity(String(length * metres))} m so far` : "";
}

function FindWords(props: { value: string; count: number; nth: number; onChange: (v: string) => void; onNext: () => void }) {
  return (
    <span className="flex items-center gap-2">
      <input
        aria-label="Find words on the drawing"
        placeholder="Find words"
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") props.onNext();
          if (e.key === "Escape") props.onChange("");
        }}
        className="h-7 w-44 rounded-md border border-line-strong bg-white px-2 text-[13px] outline-none focus:border-ink"
      />
      {props.value.trim() && (
        <span className="text-xs text-ink-3">
          {props.count ? `${props.nth + 1} of ${props.count}` : "Not on this page"}
        </span>
      )}
    </span>
  );
}

function SheetPicker(props: {
  tenderId: string;
  sheets: Sheet[];
  needsYou: Set<string>; // "document|page" of sheets with a scale or measurements waiting for the engineer
  current: { documentId: string | null; page: number };
  onOpen: (doc: string, page: number) => void;
}) {
  const documents = useDocuments(props.tenderId);
  const pdfs = (documents.data ?? []).filter((d) => ["pdf", "cad"].includes(d.kind) && d.status === "read");
  const measured = new Set(props.sheets.map((s) => s.document_id));
  const value = props.current.documentId ? `${props.current.documentId}|${props.current.page}` : "";
  return (
    <span className="flex items-center gap-3">
      <select
        aria-label="Sheet"
        value={value}
        onChange={(e) => {
          const [doc, page] = e.target.value.split("|");
          if (doc) props.onOpen(doc, Number(page));
        }}
        className="h-8 max-w-[380px] min-w-0 rounded-md border border-line-strong bg-white px-2 text-[13px]"
      >
        <option value="">Choose a sheet…</option>
        {props.sheets.length > 0 && (
          <optgroup label="Measured">
            {props.sheets.map((s) => (
              <option key={`${s.document_id}|${s.page}`} value={`${s.document_id}|${s.page}`}>
                {s.name} · page {s.page}
                {s.scale ? "" : s.kind === "cad" ? " · no units" : " · no scale"}
                {props.needsYou.has(`${s.document_id}|${s.page}`) ? " · needs you" : ""}
              </option>
            ))}
          </optgroup>
        )}
        <optgroup label="Drawings">
          {pdfs
            .filter((d) => !measured.has(d.id))
            .map((d) => (
              <option key={d.id} value={`${d.id}|1`}>
                {d.name}
              </option>
            ))}
        </optgroup>
      </select>
      {props.current.documentId && (
        <PagePicker
          documentId={props.current.documentId}
          page={props.current.page}
          count={pdfs.find((d) => d.id === props.current.documentId)?.page_count ?? 1}
          onOpen={props.onOpen}
        />
      )}
    </span>
  );
}

function PagePicker(props: { documentId: string; page: number; count: number; onOpen: (doc: string, page: number) => void }) {
  if (props.count <= 1) return null;
  return (
    <span className="flex items-center gap-2 text-ink-2">
      <button disabled={props.page <= 1} onClick={() => props.onOpen(props.documentId, props.page - 1)} aria-label="Previous page" className="disabled:text-ink-4">
        <IconChevronLeft className="size-4" stroke={1.75} />
      </button>
      Page {props.page} of {props.count}
      <button
        disabled={props.page >= props.count}
        onClick={() => props.onOpen(props.documentId, props.page + 1)}
        aria-label="Next page"
        className="disabled:text-ink-4"
      >
        <IconChevronRight className="size-4" stroke={1.75} />
      </button>
    </span>
  );
}

function ScaleNote({ tenderId, sheet }: { tenderId: string; sheet: Sheet }) {
  const decide = useDecideScale(tenderId);
  const office = useOffice(tenderId);
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));
  if (!sheet.scale) return <span className="text-attention">No scale yet: use Scale on a printed dimension</span>;
  return (
    <div className="flex flex-wrap items-center gap-2 text-ink-2">
      Scale checked on the {sheet.scale.dimension} dimension
      <span className="text-ink-3" title="At the sheet's printed size: compare it with the scale in the title block">
        · about 1:{sheet.scale.ratio.toLocaleString("en-US")}
      </span>
      {sheet.scale.status === "proposed" && <span className="text-ink-3">· {WITH_MANAGER}</span>}
      {sheet.scale.status === "reviewed" && (
        <>
          <ReviewNote reviewedBy={sheet.scale.reviewed_by} note={sheet.scale.review_note} people={people} />
          <button onClick={() => decide.mutate({ id: sheet.scale!.id, approve: true })} className={APPROVE}>
            Approve scale
          </button>
          <SendBack onSend={(reason) => decide.mutate({ id: sheet.scale!.id, approve: false, reason })} />
        </>
      )}
      {["approved", "office_approved"].includes(sheet.scale.status) && <Reopen kind="scale" id={sheet.scale.id} />}
    </div>
  );
}

function Drawing(props: {
  sheet: Sheet;
  zoom: number;
  vertices: Point[];
  measurements: Measurement[];
  selected: string | null;
  draft: Point[];
  drawing: boolean;
  onPoint: (p: Point) => void;
  onSelect: (id: string) => void;
}) {
  const { width, height } = props.sheet;
  const toPoint = (event: MouseEvent<SVGSVGElement>): Point => {
    const box = event.currentTarget.getBoundingClientRect();
    const point: Point = [((event.clientX - box.left) / box.width) * width, ((event.clientY - box.top) / box.height) * height];
    return snap(point, props.vertices, (10 / box.width) * width); // within 10 screen pixels
  };
  const dot = Math.max(width, height) / 250;

  return (
    <div className="relative mx-auto" style={{ width: `${props.zoom * 100}%` }}>
      <img
        src={pageImage(props.sheet.document_id, props.sheet.page)}
        alt={`${props.sheet.name}, page ${props.sheet.page}`}
        className="block w-full rounded-md bg-white shadow-sm"
      />
      <svg
        aria-label="Measurements on the drawing"
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        className={`absolute inset-0 h-full w-full ${props.drawing ? "cursor-crosshair" : ""}`}
        onClick={(e) => props.drawing && props.onPoint(toPoint(e))}
      >
        {props.measurements.map((m) => {
          const colour = m.status === "reviewed" ? "var(--color-attention)" : "var(--color-ink)";
          const width = m.id === props.selected ? 3 : 1.8;
          const path = m.points.map((p) => p.join(",")).join(" ");
          const select = (e: MouseEvent) => {
            e.stopPropagation();
            props.onSelect(m.id);
          };
          if (m.kind === "count")
            return (
              <g key={m.id} onClick={select} className="cursor-pointer">
                {m.points.map(([x, y], i) => (
                  <circle key={i} cx={x} cy={y} r={dot} fill={colour} />
                ))}
              </g>
            );
          return m.kind === "area" ? (
            <polygon key={m.id} points={path} onClick={select} className="cursor-pointer" fill={colour} fillOpacity={0.12}
              stroke={colour} strokeWidth={width} vectorEffect="non-scaling-stroke" />
          ) : (
            <polyline key={m.id} points={path} onClick={select} className="cursor-pointer" fill="none" stroke={colour}
              strokeWidth={width} vectorEffect="non-scaling-stroke" />
          );
        })}
        {props.draft.length > 0 && (
          <g pointerEvents="none">
            <polyline points={props.draft.map((p) => p.join(",")).join(" ")} fill="none" stroke="#2563eb" strokeWidth={2}
              strokeDasharray="6 4" vectorEffect="non-scaling-stroke" />
            {props.draft.map(([x, y], i) => (
              <circle key={i} cx={x} cy={y} r={dot * 0.8} fill="#2563eb" />
            ))}
          </g>
        )}
      </svg>
    </div>
  );
}

function SheetPanel({
  tenderId,
  measurements,
  selected,
  needs,
  footer,
  onShow,
}: {
  tenderId: string;
  measurements: Measurement[];
  selected: string | null;
  needs: string; // what a quantity waits for: the sheet's scale, or the drawing's units
  footer: string;
  onShow?: (id: string) => void;
}) {
  const takeoff = useTakeoff(tenderId);
  const office = useOffice(tenderId);
  const decide = useDecideMeasurement(tenderId);
  const remove = useRemoveMeasurement(tenderId);
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));
  const results = new Map((takeoff.data?.comparison ?? []).map((c) => [c.item ?? c.description, c]));

  return (
    <>
      <h2 className="text-[15px] font-semibold">On this sheet</h2>
      {measurements.length === 0 && <p className="text-ink-3">Nothing measured on this sheet yet.</p>}
      {measurements.map((m) => {
        const compared = results.get(m.boq_item ?? m.label);
        const by = people.get(m.proposed_by);
        return (
          <div
            key={m.id}
            className={`flex flex-col gap-1 rounded-[10px] p-3 ${m.id === selected ? "bg-rail shadow-[0_0_0_1px_var(--color-line-strong)]" : "border-b border-subtle"}`}
          >
            <span className="flex justify-between gap-2.5">
              <span className="font-medium">{m.label}</span>
              <span className="font-semibold">
                {m.quantity === null
                  ? needs
                  : `${m.kind === "count" ? Number(m.quantity) : formatQuantity(m.quantity)} ${m.unit}`}
              </span>
            </span>
            {m.object_count !== null && m.object_count !== undefined && (
              <span className="flex justify-between gap-2.5 text-ink-3">
                <span>
                  {m.object_count === 0
                    ? "Nothing on the drawing matches"
                    : `${m.object_count.toLocaleString("en-US")} drawing ${m.object_count === 1 ? "object" : "objects"}`}
                </span>
                {onShow && m.object_count > 0 && (
                  <button onClick={() => onShow(m.id)} className="text-ink-2 underline underline-offset-4">
                    Show
                  </button>
                )}
              </span>
            )}
            {onShow && (m.object_count === null || m.object_count === undefined) && m.points.length > 0 && (
              <span className="flex justify-between gap-2.5 text-ink-3">
                <span>Marked on the drawing</span>
                <button onClick={() => onShow(m.id)} className="text-ink-2 underline underline-offset-4">
                  Show
                </button>
              </span>
            )}
            <span className="flex justify-between gap-2.5 text-ink-3">
              <span>{m.boq_item ? `BOQ ${m.boq_item}` : "No BOQ item"}</span>
              {compared && (
                <span className={compared.result === "matches" ? "" : "text-attention"}>
                  {RESULTS[compared.result]} {percent(compared.difference)}
                </span>
              )}
            </span>
            <span className="text-xs text-ink-3">{by ? `Measured by ${firstName(by)}` : "Measured by you"}</span>
            {m.status === "proposed" && <span className="text-xs text-ink-3">{WITH_MANAGER}</span>}
            <ReviewNote reviewedBy={m.reviewed_by} note={m.review_note} people={people} />
            {m.id === selected && <Findings kind="measurement" id={m.id} tenderId={tenderId} />}
            <span className="flex flex-wrap gap-3 pt-1">
              {m.status === "reviewed" && (
                <>
                  <button onClick={() => decide.mutate({ id: m.id, approve: true })} className={APPROVE}>
                    Approve
                  </button>
                  <SendBack onSend={(reason) => decide.mutate({ id: m.id, approve: false, reason })} />
                </>
              )}
              <button onClick={() => remove.mutate(m.id)} className="text-ink-3 hover:text-ink">
                Remove to redo
              </button>
            </span>
          </div>
        );
      })}
      <span className="pt-2 text-xs leading-normal text-ink-3">{footer}</span>
    </>
  );
}

const UNIT_NAMES = ["millimetres", "centimetres", "metres", "inches", "feet"];

/** A CAD drawing's units: what its header says, the office's proposal, and the engineer's decision. */
function UnitsNote(props: { tenderId: string; documentId: string; scale: Sheet["scale"]; header: string | null | undefined }) {
  const setUnits = useSetUnits(props.tenderId);
  const decide = useDecideScale(props.tenderId);
  const office = useOffice(props.tenderId);
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));
  const [choice, setChoice] = useState<string | null>(null);
  const units = choice ?? props.header ?? "millimetres"; // what the drawing says, until the engineer picks
  const scale = props.scale;
  if (!scale)
    return (
      <span className="flex flex-wrap items-center gap-2">
        <span className="text-attention">
          No units yet{props.header ? `: the drawing says ${props.header}` : ": the drawing doesn’t say"}
        </span>
        <select aria-label="Units" value={units} onChange={(e) => setChoice(e.target.value)} className="h-7 rounded-md border border-line-strong bg-white px-1.5 text-[13px]">
          {UNIT_NAMES.map((u) => (
            <option key={u}>{u}</option>
          ))}
        </select>
        <button onClick={() => setUnits.mutate({ document_id: props.documentId, units })} className="font-medium text-ink">
          Set units
        </button>
        {setUnits.isError && <span className="text-attention">{setUnits.error.message}</span>}
      </span>
    );
  return (
    <div className="flex flex-wrap items-center gap-2 text-ink-2">
      Drawing units: {scale.dimension}
      {scale.status === "proposed" && <span className="text-ink-3">· {WITH_MANAGER}</span>}
      {scale.status === "reviewed" && (
        <>
          <ReviewNote reviewedBy={scale.reviewed_by} note={scale.review_note} people={people} />
          <button onClick={() => decide.mutate({ id: scale.id, approve: true })} className={APPROVE}>
            Approve units
          </button>
          <SendBack onSend={(reason) => decide.mutate({ id: scale.id, approve: false, reason })} />
        </>
      )}
      {["approved", "office_approved"].includes(scale.status) && <Reopen kind="scale" id={scale.id} />}
    </div>
  );
}

/** Measure the objects the engineer chose on a drawing: Quantix gives their count, length and area first. */
function ObjectsForm(props: {
  tenderId: string;
  documentId: string;
  page: number;
  copy: ScreenCopy;
  chosen: number[];
  needs: "units" | "scale"; // what turns drawing units into metres: a CAD drawing's units, or a PDF sheet's scale
  onAlike: () => void;
  onDone: () => void;
}) {
  const totals = useChosen(props.documentId, props.page, props.copy.header.stamp, props.chosen);
  const measure = useMeasureObjects(props.tenderId);
  const boq = useBoq(props.tenderId);
  const [kind, setKind] = useState<Kind>("count");
  const [label, setLabel] = useState("");
  const [unit, setUnit] = useState(UNITS.count[0]);
  const [multiplier, setMultiplier] = useState("");
  const [item, setItem] = useState("");
  const needsMultiplier =
    (kind === "length" && unit === "m2") || (kind === "area" && unit === "m3") || (kind === "volume" && unit !== "m3");
  const t = totals.data;
  const kinds: Kind[] = t?.volume_m3 ? ["count", "length", "area", "volume"] : ["count", "length", "area"];
  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        measure.mutate(
          {
            document_id: props.documentId,
            page: props.page,
            kind,
            label,
            unit,
            stamp: props.copy.header.stamp,
            objects: props.chosen,
            multiplier: needsMultiplier ? multiplier : null,
            boq_item: item || null,
          },
          { onSuccess: props.onDone },
        );
      }}
    >
      <h2 className="text-[15px] font-semibold">
        {props.chosen.length.toLocaleString("en-US")} {props.chosen.length === 1 ? "object" : "objects"} chosen
      </h2>
      {props.chosen.length === 1 && <p className="text-ink-3">{describe(props.copy, props.chosen[0])}</p>}
      {t && (
        <p className="leading-normal text-ink-2">
          {t.count.toLocaleString("en-US")} to count
          {t.length_m ? ` · ${t.length_m.toLocaleString("en-US")} m long` : ""}
          {t.area_m2 ? ` · ${t.area_m2.toLocaleString("en-US")} m² enclosed` : ""}
          {t.volume_m3 ? ` · ${t.volume_m3.toLocaleString("en-US")} m³ solid` : ""}
          {t.length_m === null && ` · set the ${props.needs} for lengths and areas`}
        </p>
      )}
      <button type="button" onClick={props.onAlike} className="self-start text-ink-2 underline underline-offset-4">
        Choose all like {props.chosen.length === 1 ? "this" : "these"}
      </button>
      <div role="radiogroup" aria-label="Measure as" className="flex gap-1">
        {kinds.map((k) => (
          <button
            type="button"
            key={k}
            aria-pressed={kind === k}
            onClick={() => {
              setKind(k);
              setUnit(UNITS[k][0]);
            }}
            className={`h-7 rounded-md px-2.5 text-[13px] ${kind === k ? "bg-ink text-white" : "text-ink-2 shadow-[0_0_0_1px_var(--color-line-strong)]"}`}
          >
            {{ count: "Count", length: "Length", area: "Area", volume: "Volume" }[k]}
          </button>
        ))}
      </div>
      <label className="flex flex-col gap-1">
        <span className="text-ink-2">What is it</span>
        <input aria-label="What is it" required value={label} onChange={(e) => setLabel(e.target.value)}
          className="h-9 rounded-lg border border-line-strong px-3 outline-none focus:border-ink" />
      </label>
      <label className="flex flex-col gap-1">
        <span className="text-ink-2">Unit</span>
        <select aria-label="Unit" value={unit} onChange={(e) => setUnit(e.target.value)} className="h-9 rounded-lg border border-line-strong px-2">
          {UNITS[kind].map((u) => (
            <option key={u}>{u}</option>
          ))}
        </select>
      </label>
      {needsMultiplier && (
        <label className="flex flex-col gap-1">
          <span className="text-ink-2">
            {kind === "volume" ? `Density in ${unit} per m³` : `${kind === "length" ? "Height" : "Thickness"} in metres`}
          </span>
          <input aria-label="Multiplier" required inputMode="decimal" value={multiplier} onChange={(e) => setMultiplier(e.target.value)}
            className="h-9 rounded-lg border border-line-strong px-3 outline-none focus:border-ink" />
        </label>
      )}
      <label className="flex flex-col gap-1">
        <span className="text-ink-2">BOQ item</span>
        <select aria-label="BOQ item" value={item} onChange={(e) => setItem(e.target.value)} className="h-9 rounded-lg border border-line-strong px-2">
          <option value="">Not in the BOQ</option>
          {(boq.data?.items ?? []).map((i) => (
            <option key={i.id} value={i.item}>
              {i.item} · {i.description.slice(0, 40)}
            </option>
          ))}
        </select>
      </label>
      {measure.isError && <p className="text-attention">{measure.error.message}</p>}
      <div className="flex gap-2">
        <button className="h-[38px] grow rounded-lg bg-ink text-sm text-white">Save</button>
        <button type="button" onClick={props.onDone} className="h-[38px] rounded-lg border border-line-strong px-3.5 text-sm">
          Cancel
        </button>
      </div>
    </form>
  );
}

/** One object in words: what it is and the layer it is on. */
function describe(copy: ScreenCopy, object: number): string {
  const type = copy.header.types[copy.meta[object * 3 + 1]] ?? "Object";
  const layer = copy.header.layers[copy.meta[object * 3]];
  const text = copy.header.texts.find(([o]) => o === object)?.[1];
  const closed = copy.meta[object * 3 + 2] & CLOSED ? "closed " : "";
  const words = text ? `“${text.split(/\r?\n/)[0]}”, ` : "";
  return `${words}${closed}${type.toLowerCase()} on ${layer}`;
}

function MeasureForm(props: {
  tenderId: string;
  sheet: Sheet;
  kind: Exclude<Kind, "volume">;
  points: Point[];
  note?: string; // what Quantix found, before it is saved
  onDone: () => void;
}) {
  const measure = useMeasure(props.tenderId);
  const boq = useBoq(props.tenderId);
  const [label, setLabel] = useState("");
  const [unit, setUnit] = useState(UNITS[props.kind][0]);
  const [multiplier, setMultiplier] = useState("");
  const [item, setItem] = useState("");
  const needsMultiplier = (props.kind === "length" && unit === "m2") || (props.kind === "area" && unit === "m3");

  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        measure.mutate(
          {
            document_id: props.sheet.document_id,
            page: props.sheet.page,
            kind: props.kind,
            label,
            points: props.points,
            unit,
            multiplier: needsMultiplier ? multiplier : null,
            boq_item: item || null,
          },
          { onSuccess: props.onDone },
        );
      }}
    >
      <h2 className="text-[15px] font-semibold">New {props.kind}</h2>
      {props.note && <p className="leading-normal text-ink-2">{props.note}</p>}
      <label className="flex flex-col gap-1">
        <span className="text-ink-2">What is it</span>
        <input aria-label="What is it" required value={label} onChange={(e) => setLabel(e.target.value)}
          className="h-9 rounded-lg border border-line-strong px-3 outline-none focus:border-ink" />
      </label>
      <label className="flex flex-col gap-1">
        <span className="text-ink-2">Unit</span>
        <select aria-label="Unit" value={unit} onChange={(e) => setUnit(e.target.value)} className="h-9 rounded-lg border border-line-strong px-2">
          {UNITS[props.kind].map((u) => (
            <option key={u}>{u}</option>
          ))}
        </select>
      </label>
      {needsMultiplier && (
        <label className="flex flex-col gap-1">
          <span className="text-ink-2">{props.kind === "length" ? "Height" : "Thickness"} in metres</span>
          <input aria-label="Multiplier" required inputMode="decimal" value={multiplier} onChange={(e) => setMultiplier(e.target.value)}
            className="h-9 rounded-lg border border-line-strong px-3 outline-none focus:border-ink" />
        </label>
      )}
      <label className="flex flex-col gap-1">
        <span className="text-ink-2">BOQ item</span>
        <select aria-label="BOQ item" value={item} onChange={(e) => setItem(e.target.value)} className="h-9 rounded-lg border border-line-strong px-2">
          <option value="">Not in the BOQ</option>
          {(boq.data?.items ?? []).map((i) => (
            <option key={i.id} value={i.item}>
              {i.item} · {i.description.slice(0, 40)}
            </option>
          ))}
        </select>
      </label>
      {measure.isError && <p className="text-attention">{measure.error.message}</p>}
      <div className="flex gap-2">
        <button className="h-[38px] grow rounded-lg bg-ink text-sm text-white">Save</button>
        <button type="button" onClick={props.onDone} className="h-[38px] rounded-lg border border-line-strong px-3.5 text-sm">
          Cancel
        </button>
      </div>
    </form>
  );
}

function ScaleForm(props: { tenderId: string; sheet: Sheet; line: Point[]; onDone: () => void }) {
  const setScale = useSetScale(props.tenderId);
  const [dimension, setDimension] = useState("");
  const [printedIn, setPrintedIn] = useState<"m" | "mm">("m");
  return (
    <form
      className="flex flex-col gap-3"
      onSubmit={(e) => {
        e.preventDefault();
        setScale.mutate(
          {
            document_id: props.sheet.document_id,
            page: props.sheet.page,
            line: props.line,
            length_m: Number(dimension.replace(/,/g, "")) / (printedIn === "mm" ? 1000 : 1),
            dimension,
          },
          { onSuccess: props.onDone },
        );
      }}
    >
      <h2 className="text-[15px] font-semibold">Set the scale</h2>
      <p className="leading-normal text-ink-2">Type the dimension exactly as printed between the two points.</p>
      <div className="flex gap-2">
        <input aria-label="Dimension as printed" required inputMode="decimal" value={dimension} onChange={(e) => setDimension(e.target.value)}
          placeholder="40.00" className="h-9 min-w-0 grow rounded-lg border border-line-strong px-3 outline-none focus:border-ink" />
        <select aria-label="Printed in" value={printedIn} onChange={(e) => setPrintedIn(e.target.value as "m" | "mm")}
          className="h-9 rounded-lg border border-line-strong px-2">
          <option value="m">metres</option>
          <option value="mm">millimetres</option>
        </select>
      </div>
      {setScale.isError && <p className="text-attention">{setScale.error.message}</p>}
      <div className="flex gap-2">
        <button className="h-[38px] grow rounded-lg bg-ink text-sm text-white">Set scale</button>
        <button type="button" onClick={props.onDone} className="h-[38px] rounded-lg border border-line-strong px-3.5 text-sm">
          Cancel
        </button>
      </div>
    </form>
  );
}
