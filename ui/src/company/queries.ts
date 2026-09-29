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

/** The engineer adjusts a rule; an example adjusted becomes the firm's own. */
export function useChangeRule() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: { id: string; topic: string; text: string }) =>
      must(
        await api.PATCH("/rules/{rule_id}", {
          params: { path: { rule_id: body.id } },
          body: { topic: body.topic, text: body.text },
        }),
      ),
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

export type Profile = components["schemas"]["ProfileOut"];

/** The firm's details for the title block and letterhead of every document Quantix writes. */
export function useProfile() {
  return useQuery({ queryKey: ["company"], queryFn: async () => must(await api.GET("/company")) });
}

export function useSaveProfile() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: Omit<Profile, "has_logo">) => must(await api.PUT("/company", { body })),
    onSuccess: (profile) => client.setQueryData(["company"], profile),
  });
}

export function useSaveLogo() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (file: File) => {
      const form = new FormData();
      form.append("file", file, file.name);
      const response = await fetch(`${window.location.origin}/api/company/logo`, { method: "PUT", body: form });
      if (!response.ok) {
        const detail = (await response.json().catch(() => ({}))).detail;
        throw new Error(typeof detail === "string" ? detail : "The logo couldn’t be saved.");
      }
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ["company"] }),
  });
}

export function useRemoveLogo() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const result = await api.DELETE("/company/logo");
      if (!result.response.ok) must(result as { data?: undefined; error?: unknown; response: Response });
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ["company"] }),
  });
}
