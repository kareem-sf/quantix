import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type Waiting = components["schemas"]["Waiting"];
export type Finding = components["schemas"]["FindingOut"];
export type Reopenable = "boq" | "fact" | "scale" | "measurement" | "rate" | "markups" | "draft";
export type Checked = Exclude<Reopenable, "scale"> | "recommendation";
const CHECKED: string[] = ["boq", "fact", "measurement", "rate", "markups", "draft", "recommendation"];
export const isChecked = (kind: string | null): kind is Checked => CHECKED.includes(kind ?? "");

/** What Quantix's checks find in a record now, with the Manager's reason for each warning he accepted. */
export function useFindings(kind: Checked, id: string) {
  return useQuery({
    queryKey: ["findings", kind, id],
    queryFn: async () =>
      must(await api.GET("/records/{kind}/{record_id}/findings", { params: { path: { kind, record_id: id } } })),
  });
}

/** What the staff proposed that the Tender Manager hasn't reviewed yet. */
export function useReviewQueue(tenderId: string) {
  return useQuery({
    queryKey: ["review", tenderId],
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/review", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: 2000,
  });
}

/** Send back work that is already approved, so the office does it again. */
export function useReopen() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: { kind: Reopenable; id: string; reason: string }) => {
      const result = await api.POST("/records/{kind}/{record_id}/reopen", {
        params: { path: { kind: body.kind, record_id: body.id } },
        body: { reason: body.reason },
      });
      if (!result.response.ok) must(result as { data?: undefined; error?: unknown; response: Response });
    },
    onSuccess: () => client.invalidateQueries(),
  });
}
