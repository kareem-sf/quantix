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
