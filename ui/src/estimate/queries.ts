import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type BoqItem = components["schemas"]["ItemOut"];
export type Fact = components["schemas"]["FactOut"];

const LIVE = 2000;

export function useBoq(tenderId: string) {
  return useQuery({
    queryKey: ["boq", tenderId],
    queryFn: async () => must(await api.GET("/tenders/{tender_id}/boq", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: LIVE,
  });
}

export function useGates(tenderId: string) {
  return useQuery({
    queryKey: ["gates", tenderId],
    queryFn: async () => must(await api.GET("/tenders/{tender_id}/gates", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: LIVE,
  });
}

function useRefresh(tenderId: string) {
  const client = useQueryClient();
  return () =>
    Promise.all(["boq", "gates"].map((key) => client.invalidateQueries({ queryKey: [key, tenderId] })));
}

async function ok(result: { response: Response; error?: unknown }) {
  if (!result.response.ok) must(result as { data?: undefined; error?: unknown; response: Response });
}

export function useDecide(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { kind: "item" | "fact"; id: string; approve: boolean; reason?: string }) => {
      const decision = { approve: body.approve, reason: body.reason ?? null };
      await ok(
        body.kind === "item"
          ? await api.POST("/boq/{item_id}/decision", { params: { path: { item_id: body.id } }, body: decision })
          : await api.POST("/facts/{fact_id}/decision", { params: { path: { fact_id: body.id } }, body: decision }),
      );
    },
    onSuccess: refresh,
  });
}

export function useApproveAll(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async () =>
      must(await api.POST("/tenders/{tender_id}/boq/approve-all", { params: { path: { tender_id: tenderId } } })),
    onSuccess: refresh,
  });
}

export type Priced = components["schemas"]["PricedItem"];
export type Rate = NonNullable<Priced["rate"]>;
export type Summary = components["schemas"]["SummaryOut"];
export type Markups = components["schemas"]["MarkupsOut"];
export type LibraryEntry = components["schemas"]["LibraryOut"];

export function useEstimate(tenderId: string) {
  return useQuery({
    queryKey: ["estimate", tenderId],
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/estimate", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: LIVE,
  });
}

function usePricingRefresh(tenderId: string) {
  const client = useQueryClient();
  return () =>
    Promise.all(
      ["estimate", "boq", "gates", "library"].map((key) =>
        client.invalidateQueries({ queryKey: key === "library" ? [key] : [key, tenderId] }),
      ),
    );
}

export function useDecideRate(tenderId: string) {
  const refresh = usePricingRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; approve: boolean; reason?: string; save_to_library?: boolean }) =>
      ok(
        await api.POST("/rates/{rate_id}/decision", {
          params: { path: { rate_id: body.id } },
          body: { approve: body.approve, reason: body.reason ?? null, save_to_library: body.save_to_library ?? false },
        }),
      ),
    onSuccess: refresh,
  });
}

export function useApproveAllRates(tenderId: string) {
  const refresh = usePricingRefresh(tenderId);
  return useMutation({
    mutationFn: async () =>
      must(await api.POST("/tenders/{tender_id}/rates/approve-all", { params: { path: { tender_id: tenderId } } })),
    onSuccess: refresh,
  });
}

export function useDecideMarkups(tenderId: string) {
  const refresh = usePricingRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; approve: boolean }) =>
      ok(
        await api.POST("/markups/{markups_id}/decision", {
          params: { path: { markups_id: body.id } },
          body: { approve: body.approve, reason: null },
        }),
      ),
    onSuccess: refresh,
  });
}

export function useLibrary(query: string) {
  return useQuery({
    queryKey: ["library", query],
    queryFn: async () => must(await api.GET("/library", { params: { query: { q: query } } })),
  });
}

export function useAddToLibrary() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: Omit<LibraryEntry, "id">) => must(await api.POST("/library", { body })),
    onSuccess: () => client.invalidateQueries({ queryKey: ["library"] }),
  });
}

export function useRemoveFromLibrary() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) =>
      ok(await api.DELETE("/library/{resource_id}", { params: { path: { resource_id: id } } })),
    onSuccess: () => client.invalidateQueries({ queryKey: ["library"] }),
  });
}

export function money(value: string | null | undefined) {
  if (value === null || value === undefined) return "";
  return Number(value).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export function statusLabel(status: string) {
  return (
    { proposed: "Needs you", approved: "Approved", office_approved: "Approved by the office" }[status] ?? status
  );
}

export function statusDot(status: string) {
  return status === "proposed" ? "bg-attention" : "bg-approved";
}

export function quantity(value: string | null | undefined) {
  if (value === null || value === undefined) return "—";
  return Number(value).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 3 });
}
