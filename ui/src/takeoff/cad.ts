import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type DrawingInfo = components["schemas"]["DrawingOut"];
export type Chosen = components["schemas"]["Chosen"];
export type Room = components["schemas"]["RoomOut"];
export type Problem = components["schemas"]["ProblemOut"];
export type TenderQuery = components["schemas"]["QueryOut"];
export type LayerMap = components["schemas"]["LayerMapOut"];

/** A drawing page as the Takeoff screen draws it: segments about the page's centre, the object each belongs to,
 * each object's layer, type, flags, extent and colour, and the texts (object, words, x, y, height, rotation and
 * whether it prints: a PDF's words written invisibly over their strokes don't). */
export type ScreenCopy = {
  header: {
    stamp: string;
    page: number;
    name: string;
    kind: string;
    origin: [number, number];
    extents: [number, number, number, number];
    layers: string[];
    hidden: string[];
    types: string[];
    objects: number;
    segments: number;
    texts: [number, string, number, number, number, number, number][];
  };
  segments: Float32Array;
  owner: Uint32Array;
  meta: Uint32Array;
  boxes: Float32Array;
  colours: Uint32Array; // 0xRRGGBB, as the object shows on white paper
};

export const CLOSED = 1;
export const ANNOTATION = 2;

/** Reads the service's packed copy of a drawing page (see quantix.documents.cad.screen_copy). */
export function parseScreen(buffer: ArrayBuffer): ScreenCopy {
  const magic = new TextDecoder().decode(new Uint8Array(buffer, 0, 4));
  if (magic !== "QXD2") throw new Error("This isn’t a drawing Quantix can show.");
  const size = new DataView(buffer).getUint32(4, true);
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 8, size))) as ScreenCopy["header"];
  let offset = 8 + size;
  const segments = new Float32Array(buffer, offset, header.segments * 4);
  offset += header.segments * 16;
  const owner = new Uint32Array(buffer, offset, header.segments);
  offset += header.segments * 4;
  const meta = new Uint32Array(buffer, offset, header.objects * 3);
  offset += header.objects * 12;
  const boxes = new Float32Array(buffer, offset, header.objects * 4);
  offset += header.objects * 16;
  const colours = new Uint32Array(buffer, offset, header.objects);
  return { header, segments, owner, meta, boxes, colours };
}

export function useScreenCopy(documentId: string | null, page: number, enabled: boolean) {
  return useQuery({
    queryKey: ["screen", documentId, page],
    enabled: Boolean(documentId) && enabled,
    staleTime: Infinity,
    queryFn: async () => {
      const response = await fetch(`${window.location.origin}/api/documents/${documentId}/pages/${page}/screen`);
      if (!response.ok) throw new Error("Quantix couldn’t load this drawing.");
      return parseScreen(await response.arrayBuffer());
    },
  });
}

export function useDrawingInfo(documentId: string | null, page: number, enabled = true) {
  return useQuery({
    queryKey: ["drawing", documentId, page],
    enabled: Boolean(documentId) && enabled,
    queryFn: async () =>
      must(
        await api.GET("/documents/{document_id}/drawing", {
          params: { path: { document_id: documentId! }, query: { page } },
        }),
      ),
  });
}

export function useChosen(documentId: string, page: number, stamp: string | undefined, objects: number[]) {
  return useQuery({
    queryKey: ["chosen", documentId, page, stamp, objects],
    enabled: Boolean(stamp) && objects.length > 0,
    queryFn: async () =>
      must(
        await api.POST("/documents/{document_id}/pages/{number}/choose", {
          params: { path: { document_id: documentId, number: page } },
          body: { stamp: stamp!, objects },
        }),
      ),
  });
}

export function useRooms(documentId: string | null, enabled: boolean) {
  return useQuery({
    queryKey: ["rooms", documentId],
    enabled: Boolean(documentId) && enabled,
    queryFn: async () =>
      must(await api.GET("/documents/{document_id}/rooms", { params: { path: { document_id: documentId! } } })),
  });
}

function useRefresh(tenderId: string) {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [["takeoff", tenderId], ["sheet"], ["drawing"], ["gates", tenderId], ["queries", tenderId], ["layer-maps", tenderId]].map(
        (queryKey) => client.invalidateQueries({ queryKey }),
      ),
    );
}

export function useSetUnits(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { document_id: string; units: string }) =>
      must(await api.POST("/tenders/{tender_id}/units", { params: { path: { tender_id: tenderId } }, body })),
    onSuccess: refresh,
  });
}

export function useMeasureObjects(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: {
      document_id: string;
      page: number;
      kind: "length" | "area" | "count" | "volume";
      label: string;
      unit: string;
      stamp: string;
      objects: number[];
      multiplier: string | null;
      boq_item: string | null;
    }) =>
      must(
        await api.POST("/tenders/{tender_id}/drawing-measurements", {
          params: { path: { tender_id: tenderId } },
          body: {
            document_id: body.document_id,
            page: body.page,
            kind: body.kind,
            label: body.label,
            unit: body.unit,
            multiplier: body.multiplier,
            boq_item: body.boq_item,
            choice: { stamp: body.stamp, objects: body.objects },
          },
        }),
      ),
    onSuccess: refresh,
  });
}

export function useChecks(tenderId: string, documentId: string | null) {
  return useQuery({
    queryKey: ["checks", tenderId, documentId],
    queryFn: async () =>
      must(
        await api.GET("/tenders/{tender_id}/checks", {
          params: { path: { tender_id: tenderId }, query: documentId ? { document_id: documentId } : {} },
        }),
      ),
  });
}

export function useQueries(tenderId: string) {
  return useQuery({
    queryKey: ["queries", tenderId],
    refetchInterval: 4000,
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/queries", { params: { path: { tender_id: tenderId } } })),
  });
}

export function useLayerMaps(tenderId: string) {
  return useQuery({
    queryKey: ["layer-maps", tenderId],
    refetchInterval: 4000,
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/layer-maps", { params: { path: { tender_id: tenderId } } })),
  });
}

export function useDecideQuery(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; approve: boolean; reason?: string }) => {
      const result = await api.POST("/queries/{query_id}/decision", {
        params: { path: { query_id: body.id } },
        body: { approve: body.approve, reason: body.reason ?? null },
      });
      if (!result.response.ok) must(result);
    },
    onSuccess: refresh,
  });
}

export function useDecideLayerMap(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; approve: boolean; reason?: string }) => {
      const result = await api.POST("/layer-maps/{map_id}/decision", {
        params: { path: { map_id: body.id } },
        body: { approve: body.approve, reason: body.reason ?? null },
      });
      if (!result.response.ok) must(result);
    },
    onSuccess: refresh,
  });
}

/** The region around a point that the page's lines close off, found by the service from the lines themselves. */
export function useRegion(documentId: string, page: number) {
  return useMutation({
    mutationFn: async (body: { stamp: string; point: [number, number]; hidden: string[]; within: number[] }) =>
      must(
        await api.POST("/documents/{document_id}/pages/{number}/region", {
          params: { path: { document_id: documentId, number: page } },
          body,
        }),
      ),
  });
}

/** Where each segment lies, in a grid over the page, so the object under the pointer and the points to snap to are
 * found among a few segments instead of all of them. A segment across many cells is kept aside and always looked at. */
type Grid = { x0: number; y0: number; size: number; n: number; start: Int32Array; cells: Int32Array; wide: number[] };
const grids = new WeakMap<ScreenCopy, Grid>();
const WIDE = 64; // cells a segment may cover before it is kept aside

function gridOf(copy: ScreenCopy): Grid {
  const found = grids.get(copy);
  if (found) return found;
  const [x0, y0, x1, y1] = copy.header.extents;
  const count = copy.owner.length;
  const n = Math.min(1024, Math.max(8, Math.ceil(Math.sqrt(count / 8))));
  const size = Math.max(x1 - x0, y1 - y0, 1e-9) / n;
  const cell = (v: number, o: number) => Math.min(n - 1, Math.max(0, Math.floor((v - o) / size)));
  const span = (s: number) => {
    const [ax, ay, bx, by] = copy.segments.subarray(s * 4, s * 4 + 4);
    return [cell(Math.min(ax, bx), x0), cell(Math.min(ay, by), y0), cell(Math.max(ax, bx), x0), cell(Math.max(ay, by), y0)];
  };
  const counts = new Int32Array(n * n + 1);
  const wide: number[] = [];
  for (let s = 0; s < count; s++) {
    const [c0, r0, c1, r1] = span(s);
    if ((c1 - c0 + 1) * (r1 - r0 + 1) > WIDE) continue;
    for (let r = r0; r <= r1; r++) for (let c = c0; c <= c1; c++) counts[r * n + c + 1]++;
  }
  for (let i = 1; i <= n * n; i++) counts[i] += counts[i - 1];
  const start = counts.slice();
  const cells = new Int32Array(counts[n * n]);
  const fill = counts.slice(0, n * n);
  for (let s = 0; s < count; s++) {
    const [c0, r0, c1, r1] = span(s);
    if ((c1 - c0 + 1) * (r1 - r0 + 1) > WIDE) {
      wide.push(s);
      continue;
    }
    for (let r = r0; r <= r1; r++) for (let c = c0; c <= c1; c++) cells[fill[r * n + c]++] = s;
  }
  const grid = { x0, y0, size, n, start, cells, wide };
  grids.set(copy, grid);
  return grid;
}

/** The segments that could lie within a box: from the grid cells it covers, and the wide segments. */
function segmentsIn(copy: ScreenCopy, left: number, bottom: number, right: number, top: number): Set<number> {
  const g = gridOf(copy);
  const cell = (v: number, o: number) => Math.min(g.n - 1, Math.max(0, Math.floor((v - o) / g.size)));
  const found = new Set<number>(g.wide);
  for (let r = cell(bottom, g.y0); r <= cell(top, g.y0); r++)
    for (let c = cell(left, g.x0); c <= cell(right, g.x0); c++)
      for (let i = g.start[r * g.n + c]; i < g.start[r * g.n + c + 1]; i++) found.add(g.cells[i]);
  return found;
}

function usable(copy: ScreenCopy, hidden: Set<number>, object: number) {
  return !hidden.has(copy.meta[object * 3]) && !(copy.meta[object * 3 + 2] & ANNOTATION);
}

/** The objects nearest a point, within `within` drawing units, nearest first (by distance to their segments). */
export function pick(copy: ScreenCopy, hidden: Set<number>, x: number, y: number, within: number): number | null {
  const { segments, owner } = copy;
  let best: number | null = null;
  let distance = within;
  for (const s of segmentsIn(copy, x - within, y - within, x + within, y + within)) {
    const object = owner[s];
    if (!usable(copy, hidden, object)) continue;
    const d = segmentDistance(x, y, segments[s * 4], segments[s * 4 + 1], segments[s * 4 + 2], segments[s * 4 + 3]);
    if (d <= distance) [best, distance] = [object, d];
  }
  return best;
}

export type Snap = { point: [number, number]; kind: "End" | "Middle" | "Crossing" | "On line" };

/** Where a click near the drawing's lines lands: a line's end or middle, where two lines cross, or else the nearest
 * point on a line; null when no line is within `within`. Words are never snapped to. */
export function snapTo(copy: ScreenCopy, hidden: Set<number>, x: number, y: number, within: number): Snap | null {
  const { segments, owner, meta, header } = copy;
  const text = header.types.indexOf("Text");
  const near: [number, number][] = [];
  for (const s of segmentsIn(copy, x - within, y - within, x + within, y + within)) {
    const object = owner[s];
    if (!usable(copy, hidden, object) || meta[object * 3 + 1] === text) continue;
    const d = segmentDistance(x, y, segments[s * 4], segments[s * 4 + 1], segments[s * 4 + 2], segments[s * 4 + 3]);
    if (d <= within) near.push([s, d]);
  }
  if (!near.length) return null;
  near.sort((a, b) => a[1] - b[1]);
  let best: Snap | null = null;
  let score = Infinity;
  const consider = (px: number, py: number, kind: Snap["kind"], weight: number) => {
    const d = Math.hypot(px - x, py - y);
    if (d <= within && d * weight < score) [best, score] = [{ point: [px, py], kind }, d * weight];
  };
  const closest = near.slice(0, 24);
  for (const [s] of closest) {
    const [ax, ay, bx, by] = segments.subarray(s * 4, s * 4 + 4);
    consider(ax, ay, "End", 0.4);
    consider(bx, by, "End", 0.4);
    consider((ax + bx) / 2, (ay + by) / 2, "Middle", 0.6);
  }
  for (let i = 0; i < closest.length; i++)
    for (let j = i + 1; j < closest.length; j++) {
      const hit = crossing(segments, closest[i][0], closest[j][0]);
      if (hit) consider(hit[0], hit[1], "Crossing", 0.4);
    }
  if (best) return best;
  const [s] = near[0];
  const [ax, ay, bx, by] = segments.subarray(s * 4, s * 4 + 4);
  const dx = bx - ax;
  const dy = by - ay;
  const length = dx * dx + dy * dy;
  const t = length ? Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / length)) : 0;
  return { point: [ax + t * dx, ay + t * dy], kind: "On line" };
}

function crossing(segments: Float32Array, i: number, j: number): [number, number] | null {
  const [ax, ay, bx, by] = segments.subarray(i * 4, i * 4 + 4);
  const [cx, cy, dx, dy] = segments.subarray(j * 4, j * 4 + 4);
  const rx = bx - ax;
  const ry = by - ay;
  const sx = dx - cx;
  const sy = dy - cy;
  const denominator = rx * sy - ry * sx;
  if (Math.abs(denominator) < 1e-12) return null;
  const t = ((cx - ax) * sy - (cy - ay) * sx) / denominator;
  const u = ((cx - ax) * ry - (cy - ay) * rx) / denominator;
  if (t <= 1e-6 || t >= 1 - 1e-6 || u < 0 || u > 1) return null; // ends meeting are ends, not crossings
  return [ax + t * rx, ay + t * ry];
}

/** The objects a box takes: those wholly inside it, or, dragged right to left as CAD does, those it touches. */
export function inWindow(copy: ScreenCopy, hidden: Set<number>, box: number[], touching: boolean): number[] {
  const [left, bottom, right, top] = box;
  const { boxes, segments, owner } = copy;
  const found = new Set<number>();
  for (let object = 0; object < copy.header.objects; object++) {
    const b = object * 4;
    if (!usable(copy, hidden, object) || Number.isNaN(boxes[b])) continue;
    if (boxes[b] >= left && boxes[b + 2] <= right && boxes[b + 1] >= bottom && boxes[b + 3] <= top) found.add(object);
  }
  if (touching)
    for (const s of segmentsIn(copy, left, bottom, right, top)) {
      const object = owner[s];
      if (found.has(object) || !usable(copy, hidden, object)) continue;
      if (clips(segments.subarray(s * 4, s * 4 + 4), left, bottom, right, top)) found.add(object);
    }
  return [...found].sort((a, b) => a - b);
}

/** Whether a segment crosses into a box (Liang–Barsky). */
function clips(s: Float32Array, left: number, bottom: number, right: number, top: number) {
  const [ax, ay, bx, by] = s;
  let t0 = 0;
  let t1 = 1;
  const dx = bx - ax;
  const dy = by - ay;
  for (const [p, q] of [
    [-dx, ax - left],
    [dx, right - ax],
    [-dy, ay - bottom],
    [dy, top - ay],
  ]) {
    if (p === 0) {
      if (q < 0) return false;
    } else {
      const r = q / p;
      if (p < 0) t0 = Math.max(t0, r);
      else t1 = Math.min(t1, r);
      if (t0 > t1) return false;
    }
  }
  return true;
}

/** Every shown object on the same layer, and of the same type, as one of the chosen. */
export function alike(copy: ScreenCopy, hidden: Set<number>, chosen: number[]): number[] {
  const kinds = new Set(chosen.map((o) => `${copy.meta[o * 3]}|${copy.meta[o * 3 + 1]}`));
  const found: number[] = [];
  for (let object = 0; object < copy.header.objects; object++)
    if (usable(copy, hidden, object) && kinds.has(`${copy.meta[object * 3]}|${copy.meta[object * 3 + 1]}`))
      found.push(object);
  return found;
}

/** Every shown object on a layer. */
export function onLayer(copy: ScreenCopy, layer: number): number[] {
  const found: number[] = [];
  for (let object = 0; object < copy.header.objects; object++)
    if (copy.meta[object * 3] === layer && !(copy.meta[object * 3 + 2] & ANNOTATION)) found.push(object);
  return found;
}

/** The box around objects, or null when none has one. */
export function boxAround(copy: ScreenCopy, objects: number[]): [number, number, number, number] | null {
  let box: [number, number, number, number] | null = null;
  for (const object of objects) {
    const [x0, y0, x1, y1] = copy.boxes.subarray(object * 4, object * 4 + 4);
    if (Number.isNaN(x0)) continue;
    box = box ? [Math.min(box[0], x0), Math.min(box[1], y0), Math.max(box[2], x1), Math.max(box[3], y1)] : [x0, y0, x1, y1];
  }
  return box;
}

/** The words on the page that hold what was typed, in reading order: the objects they belong to. */
export function findWords(copy: ScreenCopy, typed: string): number[] {
  const wanted = typed.trim().toLowerCase();
  if (!wanted) return [];
  return copy.header.texts
    .filter(([, text]) => text.toLowerCase().includes(wanted))
    .sort((a, b) => b[3] - a[3] || a[2] - b[2])
    .map(([object]) => object);
}

function segmentDistance(px: number, py: number, ax: number, ay: number, bx: number, by: number) {
  const dx = bx - ax;
  const dy = by - ay;
  const length = dx * dx + dy * dy;
  const t = length ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / length)) : 0;
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}
