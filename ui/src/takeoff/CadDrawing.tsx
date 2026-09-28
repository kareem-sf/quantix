import { useEffect, useMemo, useRef, useState, type PointerEvent, type WheelEvent } from "react";
import { inWindow, pick, snapTo, type Room, type ScreenCopy, type Snap } from "./cad";

type View = { cx: number; cy: number; scale: number }; // the centre in drawing units, and pixels per unit
export type Point = [number, number];
export type Box = [number, number, number, number]; // left, bottom, right, top
/** A measurement taken by points, in the drawing's units about its centre. */
export type Shape = { id: string; kind: string; points: Point[]; selected: boolean; waiting: boolean };

const MARKED = "#e66c14"; // a measurement's objects
const CHOSEN = "#2563eb"; // what the engineer is choosing
const HOVER = "rgba(37, 99, 235, 0.45)"; // what a click would choose
const INK = "#18181b";
const ATTENTION = "#c2410c";
const SNAP_PIXELS = 10; // a click this close to a line lands on it
const PICK_PIXELS = 8;

const VERTEX = `
attribute vec2 position;
attribute vec4 colour;
uniform vec2 centre;
uniform vec2 scale;
varying vec4 tint;
void main() {
  gl_Position = vec4((position - centre) * scale, 0.0, 1.0);
  gl_PointSize = 1.0;
  tint = colour;
}`;
const FRAGMENT = `
precision mediump float;
varying vec4 tint;
void main() { gl_FragColor = tint; }`;

/** A drawing page, drawn with WebGL in its own colours: every segment of every object, the texts over it, a
 * measurement's objects in orange, the objects being chosen in blue and the measurements taken by points. Scroll to
 * zoom and drag to move. With onPick, a click chooses the object under it (shift-click adds it) and a shift-drag
 * chooses what a box takes: dragged left to right, what lies wholly inside it; right to left, what it touches. With
 * onPoint, clicks place points instead, snapped to the lines' ends, middles and crossings, or onto a line; shift
 * keeps the new line square to the last point. */
export function CadDrawing(props: {
  copy: ScreenCopy;
  hiddenLayers: Set<number>;
  marked: number[];
  chosen: number[];
  rooms?: Room[];
  onPick?: (object: number | null, add: boolean) => void; // without it (and onPoint) the drawing is only looked at
  onWindow?: (objects: number[]) => void;
  shapes?: Shape[];
  draft?: Point[]; // the points being placed
  closed?: boolean; // the draft is an area
  onPoint?: (point: Point, visible: Box) => void;
  snap?: boolean;
  focus?: { box: Box; key: string } | null; // zoom to this box when its key changes
  found?: number[]; // words found on the page, ringed
}) {
  const { copy } = props;
  const box = useRef<HTMLDivElement>(null);
  const lines = useRef<HTMLCanvasElement>(null);
  const overlay = useRef<HTMLCanvasElement>(null);
  const pointer = useRef<HTMLCanvasElement>(null);
  const gl = useRef<{
    context: WebGLRenderingContext;
    program: WebGLProgram;
    positions: WebGLBuffer;
    colours: WebGLBuffer;
    count: number;
  } | null>(null);
  const [size, setSize] = useState({ width: 800, height: 600 });
  const [view, setView] = useState<View | null>(null);
  const drag = useRef<{ x: number; y: number; moved: boolean; view: View; window: boolean } | null>(null);
  const [hover, setHover] = useState<{ at: Point; shift: boolean; object: number | null; snap: Snap | null } | null>(
    null,
  );
  const [band, setBand] = useState<[Point, Point] | null>(null); // a selection box being dragged, on screen

  // where each object's segments start, as the service lists them object by object
  const starts = useMemo(() => {
    const first = new Int32Array(copy.header.objects + 1).fill(-1);
    for (let s = copy.owner.length - 1; s >= 0; s--) first[copy.owner[s]] = s;
    return first;
  }, [copy]);

  useEffect(() => {
    const element = box.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      if (width > 0 && height > 0) setSize({ width: Math.floor(width), height: Math.floor(height) });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const fitTo = (b: Box, margin: number) => {
    const scale = Math.min(size.width / Math.max(b[2] - b[0], 1e-9), size.height / Math.max(b[3] - b[1], 1e-9)) * margin;
    setView({ cx: (b[0] + b[2]) / 2, cy: (b[1] + b[3]) / 2, scale });
  };
  const fit = () => fitTo(copy.header.extents, 0.94);

  // fit the page when it opens
  useEffect(() => {
    fit();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [copy]);

  // zoom to what is to be shown
  useEffect(() => {
    if (!props.focus) return;
    const [x0, y0, x1, y1] = props.focus.box;
    const pad = Math.max(x1 - x0, y1 - y0, 1e-6) * 0.15;
    fitTo([x0 - pad, y0 - pad, x1 + pad, y1 + pad], 1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.focus?.key]);

  // the segments of the layers shown, with their colours, uploaded once per change
  useEffect(() => {
    const canvas = lines.current;
    if (!canvas) return;
    let context: WebGLRenderingContext | null = null;
    try {
      context = canvas.getContext("webgl", { antialias: true, preserveDrawingBuffer: false });
    } catch {
      context = null;
    }
    if (!context) return;
    const program = gl.current?.program ?? link(context);
    const positions = gl.current?.positions ?? context.createBuffer()!;
    const colours = gl.current?.colours ?? context.createBuffer()!;
    const kept = new Float32Array(copy.segments.length);
    const tints = new Uint8Array(copy.owner.length * 8);
    let n = 0;
    for (let s = 0; s < copy.owner.length; s++) {
      const object = copy.owner[s];
      if (props.hiddenLayers.has(copy.meta[object * 3])) continue;
      kept.set(copy.segments.subarray(s * 4, s * 4 + 4), n * 4);
      const c = copy.colours[object];
      tints.set([(c >> 16) & 255, (c >> 8) & 255, c & 255, 255, (c >> 16) & 255, (c >> 8) & 255, c & 255, 255], n * 8);
      n++;
    }
    context.bindBuffer(context.ARRAY_BUFFER, positions);
    context.bufferData(context.ARRAY_BUFFER, kept.subarray(0, n * 4), context.STATIC_DRAW);
    context.bindBuffer(context.ARRAY_BUFFER, colours);
    context.bufferData(context.ARRAY_BUFFER, tints.subarray(0, n * 8), context.STATIC_DRAW);
    gl.current = { context, program, positions, colours, count: n * 2 };
  }, [copy, props.hiddenLayers]);

  const toScreen = (v: View, x: number, y: number): Point => [
    size.width / 2 + (x - v.cx) * v.scale,
    size.height / 2 - (y - v.cy) * v.scale,
  ];

  // draw the lines, then what goes over them
  useEffect(() => {
    if (!view) return;
    const ratio = window.devicePixelRatio || 1;
    const g = gl.current;
    if (g && lines.current) {
      const { context, program, positions, colours, count } = g;
      lines.current.width = size.width * ratio;
      lines.current.height = size.height * ratio;
      context.viewport(0, 0, size.width * ratio, size.height * ratio);
      context.clearColor(1, 1, 1, 1);
      context.clear(context.COLOR_BUFFER_BIT);
      context.useProgram(program);
      const position = context.getAttribLocation(program, "position");
      context.bindBuffer(context.ARRAY_BUFFER, positions);
      context.enableVertexAttribArray(position);
      context.vertexAttribPointer(position, 2, context.FLOAT, false, 0, 0);
      const colour = context.getAttribLocation(program, "colour");
      context.bindBuffer(context.ARRAY_BUFFER, colours);
      context.enableVertexAttribArray(colour);
      context.vertexAttribPointer(colour, 4, context.UNSIGNED_BYTE, true, 0, 0);
      context.uniform2f(context.getUniformLocation(program, "centre"), view.cx, view.cy);
      context.uniform2f(context.getUniformLocation(program, "scale"), (2 * view.scale) / size.width, (2 * view.scale) / size.height);
      context.drawArrays(context.LINES, 0, count);
      context.drawArrays(context.POINTS, 0, count); // dots, and lines too short to draw
    }
    const canvas = overlay.current;
    let paint: CanvasRenderingContext2D | null = null;
    try {
      paint = canvas?.getContext("2d") ?? null;
    } catch {
      paint = null;
    }
    if (!canvas || !paint) return;
    canvas.width = size.width * ratio;
    canvas.height = size.height * ratio;
    paint.setTransform(ratio, 0, 0, ratio, 0, 0);
    paint.clearRect(0, 0, size.width, size.height);
    const at = (x: number, y: number) => toScreen(view, x, y);
    // texts, where they are big enough to read
    paint.textBaseline = "alphabetic";
    let shown = 0;
    for (const [object, text, x, y, height, rotation, prints] of copy.header.texts) {
      if (!prints || props.hiddenLayers.has(copy.meta[object * 3])) continue;
      const pixels = height * view.scale;
      if (pixels < 6 || pixels > 400) continue;
      const [sx, sy] = at(x, y);
      if (sx < -500 || sx > size.width + 500 || sy < -200 || sy > size.height + 200) continue;
      const c = copy.colours[object];
      paint.fillStyle = `rgb(${(c >> 16) & 255},${(c >> 8) & 255},${c & 255})`;
      paint.save();
      paint.translate(sx, sy);
      paint.rotate((-rotation * Math.PI) / 180);
      paint.font = `${pixels}px Geist, system-ui, sans-serif`;
      text.split("\n").forEach((line, i) => paint!.fillText(line, 0, i * pixels * 1.4));
      paint.restore();
      if (++shown > 3000) break;
    }
    for (const room of props.rooms ?? []) {
      paint.beginPath();
      room.ring.forEach(([x, y], i) => {
        const [sx, sy] = at(x - copy.header.origin[0], y - copy.header.origin[1]);
        if (i) paint!.lineTo(sx, sy);
        else paint!.moveTo(sx, sy);
      });
      paint.closePath();
      paint.fillStyle = "rgba(37, 99, 235, 0.06)";
      paint.fill();
    }
    outline(paint, copy, starts, at, props.marked, MARKED, 2.5);
    outline(paint, copy, starts, at, props.chosen, CHOSEN, 2.5);
    for (const object of props.found ?? []) {
      const [x0, y0, x1, y1] = copy.boxes.subarray(object * 4, object * 4 + 4);
      const [ax, ay] = at(x0, y1);
      const [bx, by] = at(x1, y0);
      paint.strokeStyle = CHOSEN;
      paint.lineWidth = 2;
      paint.strokeRect(ax - 4, ay - 4, bx - ax + 8, by - ay + 8);
    }
    for (const shape of props.shapes ?? []) {
      const colour = shape.selected ? MARKED : shape.waiting ? ATTENTION : INK;
      drawPoints(paint, shape.points.map(([x, y]) => at(x, y)), shape.kind, colour, shape.selected ? 3 : 1.8);
    }
    if (props.draft?.length)
      drawPoints(paint, props.draft.map(([x, y]) => at(x, y)), props.closed ? "draft-area" : "draft", CHOSEN, 2);
  }, [view, size, copy, props.hiddenLayers, props.marked, props.chosen, props.rooms, props.shapes, props.draft, props.closed, props.found, starts]);

  // what follows the pointer: the object a click would choose, the point it would place, the box being dragged
  useEffect(() => {
    const canvas = pointer.current;
    let paint: CanvasRenderingContext2D | null = null;
    try {
      paint = canvas?.getContext("2d") ?? null;
    } catch {
      paint = null;
    }
    if (!canvas || !paint || !view) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = size.width * ratio;
    canvas.height = size.height * ratio;
    paint.setTransform(ratio, 0, 0, ratio, 0, 0);
    paint.clearRect(0, 0, size.width, size.height);
    const at = (x: number, y: number) => toScreen(view, x, y);
    if (band) {
      const [[ax, ay], [bx, by]] = band;
      const touching = bx < ax; // right to left takes what the box touches
      paint.setLineDash(touching ? [6, 4] : []);
      paint.strokeStyle = touching ? "#16a34a" : CHOSEN;
      paint.fillStyle = touching ? "rgba(22, 163, 74, 0.08)" : "rgba(37, 99, 235, 0.08)";
      paint.lineWidth = 1;
      paint.fillRect(Math.min(ax, bx), Math.min(ay, by), Math.abs(bx - ax), Math.abs(by - ay));
      paint.strokeRect(Math.min(ax, bx), Math.min(ay, by), Math.abs(bx - ax), Math.abs(by - ay));
      return;
    }
    if (!hover) return;
    if (hover.object !== null) outline(paint, copy, starts, at, [hover.object], HOVER, 4);
    if (props.onPoint) {
      const target = placed(hover, props.draft);
      const [sx, sy] = at(...target);
      const last = props.draft?.at(-1);
      if (last) {
        const [lx, ly] = at(...last);
        paint.strokeStyle = CHOSEN;
        paint.lineWidth = 1.5;
        paint.setLineDash([6, 4]);
        paint.beginPath();
        paint.moveTo(lx, ly);
        paint.lineTo(sx, sy);
        paint.stroke();
        paint.setLineDash([]);
      }
      if (hover.snap && !hover.shift) marker(paint, sx, sy, hover.snap.kind);
    }
  }, [hover, band, view, size, copy, starts, props.onPoint, props.draft]);

  const toDrawing = (event: { clientX: number; clientY: number }): Point => {
    const rect = box.current!.getBoundingClientRect();
    const v = view!;
    return [v.cx + (event.clientX - rect.left - size.width / 2) / v.scale, v.cy - (event.clientY - rect.top - size.height / 2) / v.scale];
  };
  const visible = (): Box => {
    const v = view!;
    const [w, h] = [size.width / 2 / v.scale, size.height / 2 / v.scale];
    return [v.cx - w, v.cy - h, v.cx + w, v.cy + h];
  };

  const onWheel = (event: WheelEvent) => {
    if (!view) return;
    const [x, y] = toDrawing(event);
    const factor = Math.exp(-event.deltaY * 0.0015);
    const scale = view.scale * factor;
    setView({ scale, cx: x - (x - view.cx) / factor, cy: y - (y - view.cy) / factor });
  };
  const onPointerDown = (event: PointerEvent) => {
    if (!view) return;
    const window = event.shiftKey && Boolean(props.onWindow) && !props.onPoint;
    drag.current = { x: event.clientX, y: event.clientY, moved: false, view, window };
    (event.target as Element).setPointerCapture?.(event.pointerId);
  };
  const onPointerMove = (event: PointerEvent) => {
    const d = drag.current;
    if (!view) return;
    if (!d) {
      const at = toDrawing(event);
      const within = (props.onPoint ? SNAP_PIXELS : PICK_PIXELS) / view.scale;
      const snap = props.onPoint && props.snap !== false ? snapTo(copy, props.hiddenLayers, at[0], at[1], within) : null;
      const object = props.onPick && !props.onPoint ? pick(copy, props.hiddenLayers, at[0], at[1], within) : null;
      setHover({ at, shift: event.shiftKey, object, snap });
      return;
    }
    const dx = event.clientX - d.x;
    const dy = event.clientY - d.y;
    if (Math.hypot(dx, dy) > 3) d.moved = true;
    if (!d.moved) return;
    if (d.window) {
      const rect = box.current!.getBoundingClientRect();
      setBand([
        [d.x - rect.left, d.y - rect.top],
        [event.clientX - rect.left, event.clientY - rect.top],
      ]);
    } else setView({ ...d.view, cx: d.view.cx - dx / d.view.scale, cy: d.view.cy + dy / d.view.scale });
  };
  const onPointerUp = (event: PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    if (!d || !view) return;
    if (d.window && d.moved) {
      setBand(null);
      const [ax, ay] = toDrawing({ clientX: d.x, clientY: d.y });
      const [bx, by] = toDrawing(event);
      const touching = event.clientX < d.x;
      const found = inWindow(copy, props.hiddenLayers, [Math.min(ax, bx), Math.min(ay, by), Math.max(ax, bx), Math.max(ay, by)], touching);
      props.onWindow?.(found);
      return;
    }
    if (d.moved) return;
    const at = toDrawing(event);
    if (props.onPoint) {
      const within = SNAP_PIXELS / view.scale;
      const snap = props.snap !== false ? snapTo(copy, props.hiddenLayers, at[0], at[1], within) : null;
      props.onPoint(placed({ at, shift: event.shiftKey, snap }, props.draft), visible());
      return;
    }
    if (!props.onPick) return;
    props.onPick(pick(copy, props.hiddenLayers, at[0], at[1], PICK_PIXELS / view.scale), event.shiftKey);
  };

  const cursor = props.onPoint ? "cursor-crosshair" : props.onPick ? "cursor-default" : "cursor-grab active:cursor-grabbing";
  return (
    <div
      ref={box}
      role="img"
      aria-label={`${copy.header.name}: ${copy.header.objects.toLocaleString("en-US")} objects`}
      className={`relative h-full min-h-[360px] w-full touch-none overflow-hidden rounded-md bg-white shadow-sm ${cursor}`}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerLeave={() => setHover(null)}
    >
      <canvas ref={lines} className="absolute inset-0 h-full w-full" />
      <canvas ref={overlay} className="pointer-events-none absolute inset-0 h-full w-full" />
      <canvas ref={pointer} className="pointer-events-none absolute inset-0 h-full w-full" />
      {hover?.snap && props.onPoint && !hover.shift && (
        <span className="pointer-events-none absolute bottom-2 left-2 rounded bg-white/90 px-1.5 text-xs text-ink-2 shadow-sm">
          {hover.snap.kind}
        </span>
      )}
      <button
        onPointerDown={(e) => e.stopPropagation()}
        onPointerUp={(e) => e.stopPropagation()}
        onClick={fit}
        className="absolute top-2 right-2 h-7 rounded-md bg-white px-2 text-xs text-ink-2 shadow-[0_0_0_1px_var(--color-line-strong)]"
      >
        Fit
      </button>
    </div>
  );
}

/** Where a click places its point: square to the last point with shift, else on the snap, else where it was. */
function placed(hover: { at: Point; shift: boolean; snap: Snap | null }, draft: Point[] | undefined): Point {
  const last = draft?.at(-1);
  if (hover.shift && last) {
    const [x, y] = hover.at;
    return Math.abs(x - last[0]) >= Math.abs(y - last[1]) ? [x, last[1]] : [last[0], y];
  }
  return hover.snap?.point ?? hover.at;
}

function outline(
  paint: CanvasRenderingContext2D,
  copy: ScreenCopy,
  starts: Int32Array,
  at: (x: number, y: number) => Point,
  objects: number[],
  colour: string,
  width: number,
) {
  paint.strokeStyle = colour;
  paint.lineWidth = width;
  paint.beginPath();
  for (const object of objects) {
    let drew = false;
    for (let s = starts[object]; s >= 0 && s < copy.owner.length && copy.owner[s] === object; s++) {
      const [ax, ay] = at(copy.segments[s * 4], copy.segments[s * 4 + 1]);
      const [bx, by] = at(copy.segments[s * 4 + 2], copy.segments[s * 4 + 3]);
      paint.moveTo(ax, ay);
      paint.lineTo(bx, by);
      drew = true;
    }
    if (!drew) {
      // a block reference or a text: its extent
      const b = object * 4;
      const [x0, y0] = at(copy.boxes[b], copy.boxes[b + 3]);
      const [x1, y1] = at(copy.boxes[b + 2], copy.boxes[b + 1]);
      paint.rect(x0 - 3, y0 - 3, Math.max(x1 - x0, 0) + 6, Math.max(y1 - y0, 0) + 6);
    }
  }
  paint.stroke();
}

function drawPoints(paint: CanvasRenderingContext2D, points: Point[], kind: string, colour: string, width: number) {
  paint.strokeStyle = colour;
  paint.fillStyle = colour;
  paint.lineWidth = width;
  if (kind === "count") {
    for (const [x, y] of points) {
      paint.beginPath();
      paint.arc(x, y, 4, 0, 2 * Math.PI);
      paint.fill();
    }
    return;
  }
  paint.beginPath();
  points.forEach(([x, y], i) => (i ? paint.lineTo(x, y) : paint.moveTo(x, y)));
  if (kind === "area" || kind === "draft-area") {
    paint.closePath();
    paint.globalAlpha = 0.12;
    paint.fill();
    paint.globalAlpha = 1;
  }
  if (kind.startsWith("draft")) paint.setLineDash([6, 4]);
  paint.stroke();
  paint.setLineDash([]);
  if (kind.startsWith("draft"))
    for (const [x, y] of points) {
      paint.beginPath();
      paint.arc(x, y, 3, 0, 2 * Math.PI);
      paint.fill();
    }
}

/** The mark CAD shows where a click will land: a square on an end, a triangle on a middle, a cross on a crossing,
 * an hourglass on a line. */
function marker(paint: CanvasRenderingContext2D, x: number, y: number, kind: Snap["kind"]) {
  const r = 6;
  paint.strokeStyle = "#16a34a";
  paint.lineWidth = 2;
  paint.beginPath();
  if (kind === "End") paint.rect(x - r, y - r, 2 * r, 2 * r);
  else if (kind === "Middle") {
    paint.moveTo(x, y - r);
    paint.lineTo(x + r, y + r);
    paint.lineTo(x - r, y + r);
    paint.closePath();
  } else if (kind === "Crossing") {
    paint.moveTo(x - r, y - r);
    paint.lineTo(x + r, y + r);
    paint.moveTo(x + r, y - r);
    paint.lineTo(x - r, y + r);
  } else {
    paint.moveTo(x - r, y - r);
    paint.lineTo(x + r, y - r);
    paint.lineTo(x - r, y + r);
    paint.lineTo(x + r, y + r);
    paint.closePath();
  }
  paint.stroke();
}

function link(context: WebGLRenderingContext): WebGLProgram {
  const compile = (type: number, source: string) => {
    const shader = context.createShader(type)!;
    context.shaderSource(shader, source);
    context.compileShader(shader);
    return shader;
  };
  const program = context.createProgram()!;
  context.attachShader(program, compile(context.VERTEX_SHADER, VERTEX));
  context.attachShader(program, compile(context.FRAGMENT_SHADER, FRAGMENT));
  context.linkProgram(program);
  return program;
}

/** The objects' layers: show or hide each, choose everything on one, and its colour on the drawing. */
export function LayerList(props: {
  copy: ScreenCopy;
  hidden: Set<number>;
  meanings: Map<string, string>;
  onToggle: (layer: number) => void;
  onChoose?: (layer: number) => void;
}) {
  const counts = useMemo(() => {
    const found = new Map<number, { n: number; colour: number }>();
    for (let object = 0; object < props.copy.header.objects; object++) {
      if (props.copy.meta[object * 3 + 2] & 2) continue; // annotation
      const layer = props.copy.meta[object * 3];
      const seen = found.get(layer);
      found.set(layer, { n: (seen?.n ?? 0) + 1, colour: seen?.colour ?? props.copy.colours[object] });
    }
    return [...found.entries()].sort((a, b) => b[1].n - a[1].n);
  }, [props.copy]);
  return (
    <details className="flex flex-col gap-1">
      <summary className="cursor-pointer text-[15px] font-semibold">Layers ({counts.length})</summary>
      <div className="flex max-h-72 flex-col gap-0.5 overflow-y-auto pt-1">
        {counts.map(([layer, { n, colour }]) => {
          const name = props.copy.header.layers[layer];
          const meaning = props.meanings.get(name.toLowerCase());
          return (
            <div key={layer} className="group flex items-center gap-2 text-[13px]">
              <label className="flex min-w-0 grow items-center gap-2">
                <input type="checkbox" checked={!props.hidden.has(layer)} onChange={() => props.onToggle(layer)} />
                <span aria-hidden className="size-2.5 shrink-0 rounded-full" style={{ background: `#${colour.toString(16).padStart(6, "0")}` }} />
                <span className="min-w-0 grow truncate">{name}</span>
              </label>
              {props.onChoose && (
                <button
                  onClick={() => props.onChoose!(layer)}
                  aria-label={`Choose everything on ${name}`}
                  className="hidden text-xs text-ink-2 underline underline-offset-4 group-hover:inline focus:inline"
                >
                  Choose
                </button>
              )}
              <span className="text-xs text-ink-3">{meaning ?? n.toLocaleString("en-US")}</span>
            </div>
          );
        })}
      </div>
    </details>
  );
}
