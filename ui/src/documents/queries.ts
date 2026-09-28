import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type TenderDocument = components["schemas"]["DocumentOut"];
export type SearchHit = components["schemas"]["SearchHit"];
export type WorkbookSheet = components["schemas"]["SheetView"];

const busy = (d: TenderDocument) => d.status === "waiting" || d.status === "reading";

export function useDocuments(tenderId: string) {
  return useQuery({
    queryKey: ["documents", tenderId],
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/documents", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: (query) => (query.state.data?.some(busy) ? 1500 : false),
  });
}

export function usePage(documentId: string | undefined, number: number) {
  return useQuery({
    queryKey: ["page", documentId, number],
    enabled: Boolean(documentId),
    queryFn: async () =>
      must(
        await api.GET("/documents/{document_id}/pages/{number}", {
          params: { path: { document_id: documentId!, number } },
        }),
      ),
  });
}

export function useSearch(tenderId: string, query: string) {
  return useQuery({
    queryKey: ["search", tenderId, query],
    enabled: query.trim().length > 0,
    queryFn: async () =>
      must(
        await api.GET("/tenders/{tender_id}/search", {
          params: { path: { tender_id: tenderId }, query: { q: query } },
        }),
      ),
  });
}

/** Sends the chosen files, keeping each file's folder path inside the package. */
export function useAddDocuments(tenderId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (files: File[]) => {
      const form = new FormData();
      for (const file of files) form.append("files", file, file.webkitRelativePath || file.name);
      const url = `${window.location.origin}/api/tenders/${tenderId}/documents`;
      const response = await fetch(url, { method: "POST", body: form });
      const body = await response.json();
      if (!response.ok) throw new Error(typeof body.detail === "string" ? body.detail : "The files couldn’t be added.");
      return body as components["schemas"]["Added"];
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ["documents", tenderId] }),
  });
}

/** A sheet of a workbook as Excel shows it. Stored files never change, so a sheet is fetched once. */
export function useSheet(documentId: string, number: number, hidden: boolean) {
  return useQuery({
    queryKey: ["workbook-sheet", documentId, number, hidden],
    staleTime: Infinity,
    placeholderData: keepPreviousData, // the last sheet stays on screen while the next one opens
    queryFn: async () =>
      must(
        await api.GET("/documents/{document_id}/sheets/{number}", {
          params: { path: { document_id: documentId, number }, query: { hidden } },
        }),
      ),
  });
}

/** Opens the file in the app the computer uses for its type (the service hands that app a read-only copy). */
export function useOpenOriginal() {
  return useMutation({
    mutationFn: async (documentId: string) => {
      const result = await api.POST("/documents/{document_id}/open", { params: { path: { document_id: documentId } } });
      if (!result.response.ok) must(result);
    },
  });
}

export function pageImage(documentId: string, number: number) {
  return `${window.location.origin}/api/documents/${documentId}/pages/${number}/image`;
}

export function originalFile(documentId: string) {
  return `${window.location.origin}/api/documents/${documentId}/file`;
}
