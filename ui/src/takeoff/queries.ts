import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type Sheet = components["schemas"]["Sheet"];
export type Measurement = components["schemas"]["MeasurementOut"];
export type Comparison = components["schemas"]["ComparisonOut"];
export type Kind = "length" | "area" | "count";
export type Point = [number, number];

export const UNITS: Record<Kind, string[]> = { length: ["m", "m2"], area: ["m2", "m3"], count: ["nr"] };

export function useTakeoff(tenderId: string) {
  return useQuery({
    queryKey: ["takeoff", tenderId],
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/takeoff", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: 2000,
  });
}

export function useSheet(documentId: string | null, page: number) {
  return useQuery({
    queryKey: ["sheet", documentId, page],
    enabled: Boolean(documentId),
    queryFn: async () =>
      must(
        await api.GET("/documents/{document_id}/pages/{number}/sheet", {
          params: { path: { document_id: documentId!, number: page } },
        }),
      ),
  });
}

/** The drawing's own corners and line ends, so clicks snap exactly onto the drawing. */
export function useVertices(documentId: string | null, page: number) {
  return useQuery({
    queryKey: ["vertices", documentId, page],
    enabled: Boolean(documentId),
    staleTime: Infinity,
    queryFn: async () =>
      must(
        await api.GET("/documents/{document_id}/pages/{number}/vertices", {
          params: { path: { document_id: documentId!, number: page } },
        }),
      ) as Point[],
  });
}

/** The nearest vertex within `within` page points, or the point itself. */
export function snap(point: Point, vertices: Point[], within: number): Point {
  let best: Point | null = null;
  let distance = within;
  for (const v of vertices) {
    const d = Math.hypot(v[0] - point[0], v[1] - point[1]);
    if (d <= distance) [best, distance] = [v, d];
  }
  return best ?? point;
}

function useRefresh(tenderId: string) {
  const client = useQueryClient();
  return () =>
    Promise.all(
      ["takeoff", "sheet", "gates"].map((key) =>
        client.invalidateQueries({ queryKey: key === "sheet" ? [key] : [key, tenderId] }),
      ),
    );
}

export function useSetScale(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { document_id: string; page: number; line: Point[]; length_m: number; dimension: string }) =>
      must(await api.POST("/tenders/{tender_id}/scales", { params: { path: { tender_id: tenderId } }, body })),
    onSuccess: refresh,
  });
}

export function useMeasure(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: {
      document_id: string;
      page: number;
      kind: Kind;
      label: string;
      points: Point[];
      unit: string;
      multiplier: string | null;
      boq_item: string | null;
    }) => must(await api.POST("/tenders/{tender_id}/measurements", { params: { path: { tender_id: tenderId } }, body })),
    onSuccess: refresh,
  });
}

export function useDecideMeasurement(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; approve: boolean }) => {
      const result = await api.POST("/measurements/{measurement_id}/decision", {
        params: { path: { measurement_id: body.id } },
        body: { approve: body.approve, reason: null },
      });
      if (!result.response.ok) must(result);
    },
    onSuccess: refresh,
  });
}

export function useDecideScale(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; approve: boolean }) => {
      const result = await api.POST("/scales/{scale_id}/decision", {
        params: { path: { scale_id: body.id } },
        body: { approve: body.approve, reason: null },
      });
      if (!result.response.ok) must(result);
    },
    onSuccess: refresh,
  });
}

export function useRemoveMeasurement(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (id: string) => {
      const result = await api.DELETE("/measurements/{measurement_id}", { params: { path: { measurement_id: id } } });
      if (!result.response.ok) must(result);
    },
    onSuccess: refresh,
  });
}

export const RESULTS: Record<string, string> = {
  matches: "Matches",
  differs: "Differs",
  unit_differs: "Unit differs",
  no_boq_quantity: "BOQ has no quantity",
  not_in_boq: "Missing from the BOQ",
  no_scale: "Needs a scale",
};

export function percent(value: string | null | undefined) {
  if (value === null || value === undefined) return "";
  const n = Number(value) * 100;
  return `${n > 0 ? "+" : ""}${n.toFixed(1)}%`;
}
