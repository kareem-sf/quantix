import { useEffect, useMemo, useRef, useState, type PointerEvent, type WheelEvent } from "react";
import { ANNOTATION, pick, type Room, type ScreenCopy } from "./cad";

type View = { cx: number; cy: number; scale: number }; // the centre in drawing units, and pixels per unit

const INK: [number, number, number] = [0.25, 0.25, 0.28];
const MARKED = "#e66c14"; // a measurement's objects
const CHOSEN = "#2563eb"; // what the engineer is choosing

const VERTEX = `
attribute vec2 position;
uniform vec2 centre;
uniform vec2 scale;
void main() { gl_Position = vec4((position - centre) * scale, 0.0, 1.0); }`;
const FRAGMENT = `
precision mediump float;
uniform vec3 colour;
void main() { gl_FragColor = vec4(colour, 1.0); }`;

/** A CAD drawing page, drawn with WebGL: every segment of every object, the texts over it, a measurement's objects
 * in orange and the objects being chosen in blue. Drag to pan, scroll to zoom, click an object to choose it
 * (shift-click adds it to the choice). */
export function CadDrawing(props: {
  copy: ScreenCopy;
  hiddenLayers: Set<number>;
  marked: number[];
  chosen: number[];
  rooms?: Room[];
  onPick?: (object: number | null, add: boolean) => void; // without it the drawing is only looked at
}) {
  const { copy } = props;
  const box = useRef<HTMLDivElement>(null);
  const lines = useRef<HTMLCanvasElement>(null);
  const overlay = useRef<HTMLCanvasElement>(null);
  const gl = useRef<{ context: WebGLRenderingContext; program: WebGLProgram; buffer: WebGLBuffer; count: number } | null>(
    null,
  );
  const [size, setSize] = useState({ width: 800, height: 600 });
  const [view, setView] = useState<View | null>(null);
  const drag = useRef<{ x: number; y: number; moved: boolean; view: View } | null>(null);

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

  // fit the page when it opens
  useEffect(() => {
    const [x0, y0, x1, y1] = copy.header.extents;
    const scale = Math.min(size.width / Math.max(x1 - x0, 1e-9), size.height / Math.max(y1 - y0, 1e-9)) * 0.94;
    setView({ cx: (x0 + x1) / 2, cy: (y0 + y1) / 2, scale });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [copy]);

  // the segments of the layers shown, uploaded once per change
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
    const buffer = gl.current?.buffer ?? context.createBuffer()!;
    const kept = new Float32Array(copy.segments.length);
    let n = 0;
    for (let s = 0; s < copy.owner.length; s++) {
      if (props.hiddenLayers.has(copy.meta[copy.owner[s] * 3])) continue;
      kept.set(copy.segments.subarray(s * 4, s * 4 + 4), n);
      n += 4;
    }
    context.bindBuffer(context.ARRAY_BUFFER, buffer);
    context.bufferData(context.ARRAY_BUFFER, kept.subarray(0, n), context.STATIC_DRAW);
    gl.current = { context, program, buffer, count: n / 2 };
  }, [copy, props.hiddenLayers]);

  // draw
  useEffect(() => {
    if (!view) return;
    const ratio = window.devicePixelRatio || 1;
    const g = gl.current;
    if (g && lines.current) {
      const { context, program, buffer, count } = g;
      lines.current.width = size.width * ratio;
      lines.current.height = size.height * ratio;
      context.viewport(0, 0, size.width * ratio, size.height * ratio);
      context.clearColor(1, 1, 1, 1);
      context.clear(context.COLOR_BUFFER_BIT);
      context.useProgram(program);
      context.bindBuffer(context.ARRAY_BUFFER, buffer);
      const position = context.getAttribLocation(program, "position");
      context.enableVertexAttribArray(position);
      context.vertexAttribPointer(position, 2, context.FLOAT, false, 0, 0);
      context.uniform2f(context.getUniformLocation(program, "centre"), view.cx, view.cy);
      context.uniform2f(context.getUniformLocation(program, "scale"), (2 * view.scale) / size.width, (2 * view.scale) / size.height);
      context.uniform3f(context.getUniformLocation(program, "colour"), ...INK);
      context.drawArrays(context.LINES, 0, count);
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
    const toScreen = (x: number, y: number): [number, number] => [
      size.width / 2 + (x - view.cx) * view.scale,
      size.height / 2 - (y - view.cy) * view.scale,
    ];
    // texts, where they are big enough to read
    paint.fillStyle = "rgb(64,64,70)";
    paint.textBaseline = "alphabetic";
    let shown = 0;
    for (const [object, text, x, y, height, rotation] of copy.header.texts) {
      if (props.hiddenLayers.has(copy.meta[object * 3])) continue;
      const pixels = height * view.scale;
      if (pixels < 6 || pixels > 400) continue;
      const [sx, sy] = toScreen(x, y);
      if (sx < -500 || sx > size.width + 500 || sy < -200 || sy > size.height + 200) continue;
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
        const [sx, sy] = toScreen(x - copy.header.origin[0], y - copy.header.origin[1]);
        if (i) paint!.lineTo(sx, sy);
        else paint!.moveTo(sx, sy);
      });
      paint.closePath();
      paint.fillStyle = "rgba(37, 99, 235, 0.06)";
      paint.fill();
    }
    const outline = (objects: number[], colour: string, width: number) => {
      paint!.strokeStyle = colour;
      paint!.lineWidth = width;
      paint!.beginPath();
      for (const object of objects) {
        let drew = false;
        for (let s = starts[object]; s >= 0 && s < copy.owner.length && copy.owner[s] === object; s++) {
          const [ax, ay] = toScreen(copy.segments[s * 4], copy.segments[s * 4 + 1]);
          const [bx, by] = toScreen(copy.segments[s * 4 + 2], copy.segments[s * 4 + 3]);
          paint!.moveTo(ax, ay);
          paint!.lineTo(bx, by);
          drew = true;
        }
        if (!drew) {
          // a block reference or a text: its extent
          const b = object * 4;
          const [x0, y0] = toScreen(copy.boxes[b], copy.boxes[b + 3]);
          const [x1, y1] = toScreen(copy.boxes[b + 2], copy.boxes[b + 1]);
          paint!.rect(x0 - 3, y0 - 3, Math.max(x1 - x0, 0) + 6, Math.max(y1 - y0, 0) + 6);
        }
      }
      paint!.stroke();
    };
    outline(props.marked, MARKED, 2.5);
    outline(props.chosen, CHOSEN, 2.5);
  }, [view, size, copy, props.hiddenLayers, props.marked, props.chosen, props.rooms, starts]);

  const toDrawing = (event: { clientX: number; clientY: number }): [number, number] => {
    const rect = box.current!.getBoundingClientRect();
    const v = view!;
    return [v.cx + (event.clientX - rect.left - size.width / 2) / v.scale, v.cy - (event.clientY - rect.top - size.height / 2) / v.scale];
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
    drag.current = { x: event.clientX, y: event.clientY, moved: false, view };
    (event.target as Element).setPointerCapture?.(event.pointerId);
  };
  const onPointerMove = (event: PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const dx = event.clientX - d.x;
    const dy = event.clientY - d.y;
    if (Math.hypot(dx, dy) > 3) d.moved = true;
    if (d.moved) setView({ ...d.view, cx: d.view.cx - dx / d.view.scale, cy: d.view.cy + dy / d.view.scale });
  };
  const onPointerUp = (event: PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    if (!d || d.moved || !view || !props.onPick) return;
    const [x, y] = toDrawing(event);
    props.onPick(pick(copy, props.hiddenLayers, x, y, 8 / view.scale), event.shiftKey);
  };

  const fit = () => {
    const [x0, y0, x1, y1] = copy.header.extents;
    const scale = Math.min(size.width / Math.max(x1 - x0, 1e-9), size.height / Math.max(y1 - y0, 1e-9)) * 0.94;
    setView({ cx: (x0 + x1) / 2, cy: (y0 + y1) / 2, scale });
  };

  return (
    <div
      ref={box}
      role="img"
      aria-label={`${copy.header.name}: ${copy.header.objects.toLocaleString("en-US")} objects`}
      className={`relative h-full min-h-[360px] w-full touch-none overflow-hidden rounded-md bg-white shadow-sm ${props.onPick ? "cursor-crosshair" : "cursor-grab active:cursor-grabbing"}`}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
    >
      <canvas ref={lines} className="absolute inset-0 h-full w-full" />
      <canvas ref={overlay} className="pointer-events-none absolute inset-0 h-full w-full" />
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

/** The objects' layers, to show or hide each. */
export function LayerList(props: {
  copy: ScreenCopy;
  hidden: Set<number>;
  meanings: Map<string, string>;
  onToggle: (layer: number) => void;
}) {
  const counts = useMemo(() => {
    const found = new Map<number, number>();
    for (let object = 0; object < props.copy.header.objects; object++) {
      if (props.copy.meta[object * 3 + 2] & ANNOTATION) continue;
      const layer = props.copy.meta[object * 3];
      found.set(layer, (found.get(layer) ?? 0) + 1);
    }
    return [...found.entries()].sort((a, b) => b[1] - a[1]);
  }, [props.copy]);
  return (
    <details className="flex flex-col gap-1">
      <summary className="cursor-pointer text-[15px] font-semibold">Layers ({counts.length})</summary>
      <div className="flex max-h-72 flex-col gap-0.5 overflow-y-auto pt-1">
        {counts.map(([layer, n]) => {
          const name = props.copy.header.layers[layer];
          const meaning = props.meanings.get(name.toLowerCase());
          return (
            <label key={layer} className="flex items-center gap-2 text-[13px]">
              <input type="checkbox" checked={!props.hidden.has(layer)} onChange={() => props.onToggle(layer)} />
              <span className="min-w-0 grow truncate">{name}</span>
              <span className="text-xs text-ink-3">{meaning ?? n}</span>
            </label>
          );
        })}
      </div>
    </details>
  );
}
