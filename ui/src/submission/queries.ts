import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type Requirement = components["schemas"]["RequirementOut"];
export type Built = components["schemas"]["ExportOut"];

export function useSubmission(tenderId: string) {
  return useQuery({
    queryKey: ["submission", tenderId],
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/submission", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: 2000,
  });
}

async function ok(result: { response: Response; error?: unknown }) {
  if (!result.response.ok) must(result as { data?: undefined; error?: unknown; response: Response });
}

function useRefresh(tenderId: string) {
  const client = useQueryClient();
  return () =>
    Promise.all(["submission", "gates"].map((key) => client.invalidateQueries({ queryKey: [key, tenderId] })));
}

export function useDecideDraft(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; approve: boolean; reason?: string }) =>
      ok(
        await api.POST("/drafts/{draft_id}/decision", {
          params: { path: { draft_id: body.id } },
          body: { approve: body.approve, reason: body.reason ?? null },
        }),
      ),
    onSuccess: refresh,
  });
}

export function useMarkReady(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; ready: boolean; note?: string }) =>
      ok(
        await api.POST("/requirements/{requirement_id}/ready", {
          params: { path: { requirement_id: body.id } },
          body: { ready: body.ready, note: body.note ?? "" },
        }),
      ),
    onSuccess: refresh,
  });
}

export function useAttach(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { id: string; file: File }) => {
      const form = new FormData();
      form.append("file", body.file, body.file.name);
      const url = `${window.location.origin}/api/requirements/${body.id}/file`;
      const response = await fetch(url, { method: "POST", body: form });
      if (!response.ok) {
        const detail = (await response.json().catch(() => ({}))).detail;
        throw new Error(typeof detail === "string" ? detail : "The file couldn’t be added.");
      }
    },
    onSuccess: refresh,
  });
}

export function useAddRequirement(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { section: string; title: string }) =>
      ok(
        await api.POST("/tenders/{tender_id}/requirements", { params: { path: { tender_id: tenderId } }, body }),
      ),
    onSuccess: refresh,
  });
}

export function useBuild(tenderId: string) {
  return useMutation({
    mutationFn: async (spread: boolean) =>
      must(
        await api.POST("/tenders/{tender_id}/export", {
          params: { path: { tender_id: tenderId } },
          body: { spread_markups: spread },
        }),
      ),
  });
}

export function useOpenFolder() {
  return useMutation({
    mutationFn: async (folder: string) =>
      ok(await api.POST("/exports/{folder}/open", { params: { path: { folder } } })),
  });
}
