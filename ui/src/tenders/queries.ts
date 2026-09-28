import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";

export function useTenders() {
  return useQuery({ queryKey: ["tenders"], queryFn: async () => must(await api.GET("/tenders")) });
}

export function useTender(id: string) {
  return useQuery({
    queryKey: ["tenders", id],
    queryFn: async () => must(await api.GET("/tenders/{tender_id}", { params: { path: { tender_id: id } } })),
  });
}

export function useCreateTender() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: { name: string; due_date: string | null }) => must(await api.POST("/tenders", { body })),
    onSuccess: () => client.invalidateQueries({ queryKey: ["tenders"] }),
  });
}

export function useSetOutcome(id: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (outcome: "open" | "submitted" | "won" | "lost") =>
      must(await api.PATCH("/tenders/{tender_id}", { params: { path: { tender_id: id } }, body: { outcome } })),
    onSuccess: () => client.invalidateQueries({ queryKey: ["tenders"] }),
  });
}

export function useSetDueDate(id: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (due_date: string | null) =>
      must(await api.PATCH("/tenders/{tender_id}", { params: { path: { tender_id: id } }, body: { due_date } })),
    onSuccess: () => client.invalidateQueries({ queryKey: ["tenders"] }),
  });
}

export function useDeleteTender(id: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const result = await api.DELETE("/tenders/{tender_id}", { params: { path: { tender_id: id } } });
      if (!result.response.ok) must(result as { data?: undefined; error?: unknown; response: Response });
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ["tenders"] }),
  });
}
