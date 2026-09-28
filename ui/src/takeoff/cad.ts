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
 * each object's layer, type, flags and extent, and the texts. */
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
    texts: [number, string, number, number, number, number][];
  };
  segments: Float32Array;
  owner: Uint32Array;
  meta: Uint32Array;
  boxes: Float32Array;
};

export const CLOSED = 1;
export const ANNOTATION = 2;

/** Reads the service's packed copy of a drawing page (see quantix.documents.cad.screen_copy). */
export function parseScreen(buffer: ArrayBuffer): ScreenCopy {
  const magic = new TextDecoder().decode(new Uint8Array(buffer, 0, 4));
  if (magic !== "QXD1") throw new Error("This isn’t a drawing Quantix can show.");
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
  return { header, segments, owner, meta, boxes };
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
      [["takeoff", tenderId], ["drawing"], ["gates", tenderId], ["queries", tenderId], ["layer-maps", tenderId]].map(
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
      kind: "length" | "area" | "count";
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

/** The objects nearest a point, within `within` drawing units, nearest first (by distance to their segments). */
export function pick(copy: ScreenCopy, hidden: Set<number>, x: number, y: number, within: number): number | null {
  const { segments, owner, meta, boxes } = copy;
  let best: number | null = null;
  let distance = within;
  for (let s = 0; s < owner.length; s++) {
    const object = owner[s];
    if (hidden.has(meta[object * 3]) || meta[object * 3 + 2] & ANNOTATION) continue;
    const b = object * 4;
    if (x < boxes[b] - within || x > boxes[b + 2] + within || y < boxes[b + 1] - within || y > boxes[b + 3] + within)
      continue;
    const d = segmentDistance(x, y, segments[s * 4], segments[s * 4 + 1], segments[s * 4 + 2], segments[s * 4 + 3]);
    if (d <= distance) [best, distance] = [object, d];
  }
  return best;
}

function segmentDistance(px: number, py: number, ax: number, ay: number, bx: number, by: number) {
  const dx = bx - ax;
  const dy = by - ay;
  const length = dx * dx + dy * dy;
  const t = length ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / length)) : 0;
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}
