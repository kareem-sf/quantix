import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type Rule = components["schemas"]["RuleOut"];

export function useRules() {
  return useQuery({ queryKey: ["rules"], queryFn: async () => must(await api.GET("/rules")) });
}

export function useAddRule() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: { topic: string; text: string }) => must(await api.POST("/rules", { body })),
    onSuccess: () => client.invalidateQueries({ queryKey: ["rules"] }),
  });
}

export function useRemoveRule() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const result = await api.DELETE("/rules/{rule_id}", { params: { path: { rule_id: id } } });
      if (!result.response.ok) must(result as { data?: undefined; error?: unknown; response: Response });
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ["rules"] }),
  });
}
