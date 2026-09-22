import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type Connection = components["schemas"]["ConnectionOut"];
export type Provider = Connection["provider"];
export type OfficeSettings = components["schemas"]["Settings"];

export const PROVIDERS: { id: Provider; label: string }[] = [
  { id: "anthropic", label: "Anthropic" },
  { id: "openai", label: "OpenAI" },
  { id: "google", label: "Google" },
  { id: "xai", label: "xAI" },
  { id: "openai_compatible", label: "OpenAI-compatible service" },
];

export function useConnections() {
  return useQuery({ queryKey: ["connections"], queryFn: async () => must(await api.GET("/ai/connections")) });
}

export function useModels(connectionId: string) {
  return useQuery({
    queryKey: ["connections", connectionId, "models"],
    queryFn: async () =>
      must(await api.GET("/ai/connections/{connection_id}/models", { params: { path: { connection_id: connectionId } } })),
  });
}

export function useSettings() {
  return useQuery({ queryKey: ["settings"], queryFn: async () => must(await api.GET("/settings")) });
}

function useRefresh(...keys: string[]) {
  const client = useQueryClient();
  return () => Promise.all(keys.map((key) => client.invalidateQueries({ queryKey: [key] })));
}

export function useAddConnection() {
  const refresh = useRefresh("connections");
  return useMutation({
    mutationFn: async (body: { provider: Provider; api_key: string; base_url: string | null }) =>
      must(await api.POST("/ai/connections", { body })),
    onSuccess: refresh,
  });
}

export function useRemoveConnection() {
  const refresh = useRefresh("connections", "settings");
  return useMutation({
    mutationFn: async (id: string) => {
      const result = await api.DELETE("/ai/connections/{connection_id}", { params: { path: { connection_id: id } } });
      if (!result.response.ok) must(result);
    },
    onSuccess: refresh,
  });
}

export function useCheckModel(connectionId: string) {
  const refresh = useRefresh("connections");
  return useMutation({
    mutationFn: async (model: string) =>
      must(
        await api.POST("/ai/connections/{connection_id}/checks", {
          params: { path: { connection_id: connectionId } },
          body: { model },
        }),
      ),
    onSuccess: refresh,
  });
}

export function useUpdateSettings() {
  const refresh = useRefresh("settings");
  return useMutation({
    mutationFn: async (body: Partial<OfficeSettings>) => must(await api.PATCH("/settings", { body })),
    onSuccess: refresh,
  });
}
