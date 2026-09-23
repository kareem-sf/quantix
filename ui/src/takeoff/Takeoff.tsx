import { useState, type MouseEvent } from "react";
import { useParams, useSearchParams } from "react-router";
import { pageImage, useDocuments } from "../documents/queries";
import { useBoq, quantity as formatQuantity } from "../estimate/queries";
import { firstName, useOffice } from "../office/queries";
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

type Tool = "select" | Kind | "scale";
const TOOLS: [Tool, string][] = [
  ["select", "Select"],
  ["length", "Length"],
  ["area", "Area"],
  ["count", "Count"],
  ["scale", "Scale"],
];
const LEAST: Record<Tool, number> = { select: 0, length: 2, area: 3, count: 1, scale: 2 };

export function Takeoff() {
  const { tenderId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const takeoff = useTakeoff(tenderId);
  const documentId = params.get("doc");
  const page = Number(params.get("page") ?? 1);
  const sheet = useSheet(documentId, page);
  const vertices = useVertices(documentId, page);
  const [tool, setTool] = useState<Tool>("select");
  const [draft, setDraft] = useState<Point[]>([]);
  const [finished, setFinished] = useState(false);
  const [zoom, setZoom] = useState(1);
  const selected = params.get("m");

  const onSheet = (takeoff.data?.measurements ?? []).filter((m) => m.document_id === documentId && m.page === page);
  const open = (doc: string, number: number) => setParams({ doc, page: String(number) });
  const choose = (next: Tool) => {
    setTool(next);
    setDraft([]);
    setFinished(false);
  };
  const addPoint = (point: Point) => {
    if (tool === "select" || finished) return;
    const points = [...draft, point];
    setDraft(points);
    if (tool === "scale" && points.length === 2) setFinished(true);
  };

  return (
    <div className="flex h-full w-full">
      <section aria-label="Drawing" className="flex min-w-0 grow flex-col bg-subtle px-6 py-5">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 pb-3">
          <h1 className="text-[22px] font-semibold tracking-tight">Takeoff</h1>
          <SheetPicker
            tenderId={tenderId}
            sheets={takeoff.data?.sheets ?? []}
            current={{ documentId, page }}
            onOpen={open}
          />
        </div>
        {sheet.data ? (
          <>
            <div className="flex flex-wrap items-center justify-between gap-3 pb-2">
              <div role="toolbar" aria-label="Measuring tools" className="flex gap-0.5 rounded-lg bg-white p-[3px] shadow-[0_0_0_1px_var(--color-line-strong)]">
                {TOOLS.map(([key, label]) => (
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
            <p className="pb-2">
              <ScaleNote tenderId={tenderId} sheet={sheet.data} />
            </p>
            {tool !== "select" && !finished && (
              <p className="pb-2 text-ink-2">
                {tool === "scale"
                  ? "Click both ends of a dimension printed on the drawing."
                  : `Click the points of the ${tool}${tool === "count" ? " (one per item)" : ""}, then finish.`}{" "}
                {draft.length >= LEAST[tool] && tool !== "scale" && (
                  <button onClick={() => setFinished(true)} className="font-medium text-ink underline underline-offset-4">
                    Finish
                  </button>
                )}
              </p>
            )}
            <div className="min-h-0 grow overflow-auto">
              <Drawing
                sheet={sheet.data}
                zoom={zoom}
                vertices={vertices.data ?? []}
                measurements={onSheet}
                selected={selected}
                draft={draft}
                drawing={tool !== "select" && !finished}
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
      <aside aria-label="Measurements" className="flex w-[320px] shrink-0 flex-col gap-3 overflow-y-auto border-l border-line px-5 pt-7 pb-5">
        {finished && sheet.data ? (
          tool === "scale" ? (
            <ScaleForm tenderId={tenderId} sheet={sheet.data} line={draft} onDone={() => choose("select")} />
          ) : (
            <MeasureForm tenderId={tenderId} sheet={sheet.data} kind={tool as Kind} points={draft} onDone={() => choose("select")} />
          )
        ) : (
          <SheetPanel tenderId={tenderId} measurements={onSheet} selected={selected} />
        )}
      </aside>
    </div>
  );
}

function SheetPicker(props: {
  tenderId: string;
  sheets: Sheet[];
  current: { documentId: string | null; page: number };
  onOpen: (doc: string, page: number) => void;
}) {
  const documents = useDocuments(props.tenderId);
  const pdfs = (documents.data ?? []).filter((d) => d.kind === "pdf" && d.status === "read");
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
        className="h-8 max-w-[260px] rounded-md border border-line-strong bg-white px-2 text-[13px]"
      >
        <option value="">Choose a sheet…</option>
        {props.sheets.length > 0 && (
          <optgroup label="Measured">
            {props.sheets.map((s) => (
              <option key={`${s.document_id}|${s.page}`} value={`${s.document_id}|${s.page}`}>
                {s.name} · page {s.page}
                {s.scale ? "" : " · no scale"}
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
      <button disabled={props.page <= 1} onClick={() => props.onOpen(props.documentId, props.page - 1)} aria-label="Previous page">
        ‹
      </button>
      Page {props.page} of {props.count}
      <button
        disabled={props.page >= props.count}
        onClick={() => props.onOpen(props.documentId, props.page + 1)}
        aria-label="Next page"
      >
        ›
      </button>
    </span>
  );
}

function ScaleNote({ tenderId, sheet }: { tenderId: string; sheet: Sheet }) {
  const decide = useDecideScale(tenderId);
  if (!sheet.scale) return <span className="text-attention">No scale yet: use Scale on a printed dimension</span>;
  return (
    <span className="flex items-center gap-2 text-ink-2">
      Scale checked on the {sheet.scale.dimension} dimension
      {sheet.scale.status === "proposed" && (
        <button onClick={() => decide.mutate({ id: sheet.scale!.id, approve: true })} className="font-medium text-ink">
          Approve scale
        </button>
      )}
    </span>
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
          const colour = m.status === "proposed" ? "var(--color-attention)" : "var(--color-ink)";
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

function SheetPanel({ tenderId, measurements, selected }: { tenderId: string; measurements: Measurement[]; selected: string | null }) {
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
                  ? "needs a scale"
                  : `${m.kind === "count" ? Number(m.quantity) : formatQuantity(m.quantity)} ${m.unit}`}
              </span>
            </span>
            <span className="flex justify-between gap-2.5 text-ink-3">
              <span>{m.boq_item ? `BOQ ${m.boq_item}` : "No BOQ item"}</span>
              {compared && (
                <span className={compared.result === "matches" ? "" : "text-attention"}>
                  {RESULTS[compared.result]} {percent(compared.difference)}
                </span>
              )}
            </span>
            <span className="text-xs text-ink-3">{by ? `Measured by ${firstName(by)}` : "Measured by you"}</span>
            <span className="flex gap-3 pt-1">
              {m.status === "proposed" && (
                <>
                  <button onClick={() => decide.mutate({ id: m.id, approve: true })} className="font-medium">
                    Approve
                  </button>
                  <button onClick={() => decide.mutate({ id: m.id, approve: false })} className="text-ink-2">
                    Reject
                  </button>
                </>
              )}
              <button onClick={() => remove.mutate(m.id)} className="text-ink-3 hover:text-ink">
                Remove to redo
              </button>
            </span>
          </div>
        );
      })}
      <span className="pt-2 text-xs leading-normal text-ink-3">
        Quantix calculates every length, area and count from the marks and the sheet’s scale.
      </span>
    </>
  );
}

function MeasureForm(props: { tenderId: string; sheet: Sheet; kind: Kind; points: Point[]; onDone: () => void }) {
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
