import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type Package = components["schemas"]["PackageOut"];
export type Quote = components["schemas"]["QuoteOut"];
export type Company = components["schemas"]["CompanyOut"];

export function usePackages(tenderId: string) {
  return useQuery({
    queryKey: ["packages", tenderId],
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/packages", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: 2000,
  });
}

async function ok(result: { response: Response; error?: unknown }) {
  if (!result.response.ok) must(result as { data?: undefined; error?: unknown; response: Response });
}

function useRefresh(tenderId: string) {
  const client = useQueryClient();
  return () =>
    Promise.all(
      ["packages", "estimate", "gates"].map((key) => client.invalidateQueries({ queryKey: [key, tenderId] })),
    );
}

export function useChoose(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (body: { packageId: string; quoteId: string }) =>
      ok(
        await api.POST("/packages/{package_id}/choice", {
          params: { path: { package_id: body.packageId } },
          body: { quote_id: body.quoteId },
        }),
      ),
    onSuccess: refresh,
  });
}

export function useMarkSent(tenderId: string) {
  const refresh = useRefresh(tenderId);
  return useMutation({
    mutationFn: async (id: string) =>
      ok(await api.POST("/enquiries/{enquiry_id}/sent", { params: { path: { enquiry_id: id } } })),
    onSuccess: refresh,
  });
}

export function useDirectory(query: string) {
  return useQuery({
    queryKey: ["directory", query],
    queryFn: async () => must(await api.GET("/directory", { params: { query: { q: query } } })),
  });
}

/** The firm being added may be one already in the directory: `firms` are their names. */
export class MaybeSameFirm extends Error {
  constructor(
    message: string,
    readonly firms: string[],
  ) {
    super(message);
  }
}

export function useAddCompany() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: components["schemas"]["CompanyIn"]) => {
      const result = await api.POST("/directory", { body });
      const detail = (result.error as { detail?: { message?: string; firms?: string[] } } | undefined)?.detail;
      if (result.response.status === 409 && detail?.message && detail.firms) {
        throw new MaybeSameFirm(detail.message, detail.firms);
      }
      return must(result);
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ["directory"] }),
  });
}

export function useMergeCompany() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, into }: { id: string; into: string }) =>
      ok(await api.POST("/directory/{company_id}/merge", { params: { path: { company_id: id } }, body: { into } })),
    onSuccess: () => client.invalidateQueries({ queryKey: ["directory"] }),
  });
}

export function useRemoveCompany() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) =>
      ok(await api.DELETE("/directory/{company_id}", { params: { path: { company_id: id } } })),
    onSuccess: () => client.invalidateQueries({ queryKey: ["directory"] }),
  });
}
