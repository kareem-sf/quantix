import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, must } from "../api/client";
import type { components } from "../api/schema";

export type Staff = components["schemas"]["StaffOut"];
export type Message = components["schemas"]["MessageOut"];
export type Decision = components["schemas"]["DecisionOut"];
export type Task = components["schemas"]["TaskOut"];

export const TEAM = "team";
export const ENGINEER = "engineer";
const LIVE = 2000; // the office changes while agents work; the screens follow it

export function useOffice(tenderId: string) {
  return useQuery({
    queryKey: ["office", tenderId],
    queryFn: async () => must(await api.GET("/tenders/{tender_id}/office", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: LIVE,
  });
}

export function useMessages(tenderId: string, channel: string) {
  return useQuery({
    queryKey: ["messages", tenderId, channel],
    queryFn: async () =>
      must(
        await api.GET("/tenders/{tender_id}/messages", {
          params: { path: { tender_id: tenderId }, query: { channel } },
        }),
      ),
    refetchInterval: LIVE,
  });
}

export function useDecisions(tenderId: string) {
  return useQuery({
    queryKey: ["decisions", tenderId],
    queryFn: async () =>
      must(await api.GET("/tenders/{tender_id}/decisions", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: LIVE,
  });
}

export function useTasks(tenderId: string) {
  return useQuery({
    queryKey: ["tasks", tenderId],
    queryFn: async () => must(await api.GET("/tenders/{tender_id}/tasks", { params: { path: { tender_id: tenderId } } })),
    refetchInterval: LIVE,
  });
}

export function useSend(tenderId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: { channel: string; text: string }) =>
      must(await api.POST("/tenders/{tender_id}/messages", { params: { path: { tender_id: tenderId } }, body })),
    onSuccess: (_, body) => {
      client.invalidateQueries({ queryKey: ["messages", tenderId, body.channel] });
      client.invalidateQueries({ queryKey: ["office", tenderId] });
    },
  });
}

export function useAnswer(tenderId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (body: { id: string; answer: string }) =>
      must(
        await api.POST("/decisions/{decision_id}/answer", {
          params: { path: { decision_id: body.id } },
          body: { answer: body.answer },
        }),
      ),
    onSuccess: () => {
      client.invalidateQueries({ queryKey: ["decisions", tenderId] });
      client.invalidateQueries({ queryKey: ["office", tenderId] });
    },
  });
}

export function useStop(tenderId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      const result = await api.POST("/tenders/{tender_id}/office/stop", { params: { path: { tender_id: tenderId } } });
      if (!result.response.ok) must(result);
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ["office", tenderId] }),
  });
}

export function firstName(member: Staff) {
  return member.name.split(" ")[0];
}
