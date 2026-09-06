import { invoke, isTauri } from "@tauri-apps/api/core";
import { createContext, useContext } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { components } from "./bindings/api";

export type Schema<K extends keyof components["schemas"]> =
  components["schemas"][K];
export type Connection = { base_url: string; token: string };

export function createApi(
  connection: Connection,
  fetcher: typeof fetch = fetch,
) {
  async function request(path: string, method = "GET", body?: unknown) {
    const response = await fetcher(
      `${connection.base_url.replace(/\/$/, "")}${path}`,
      {
        method,
        headers: {
          Authorization: `Bearer ${connection.token}`,
          ...(body === undefined ? {} : { "Content-Type": "application/json" }),
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      },
    ).catch((error: unknown) => {
      if (error instanceof TypeError)
        throw new Error(
          "The local service could not be reached. Reopen Quantix and try again.",
        );
      throw error;
    });
    if (!response.ok) {
      let detail = `Request failed (${response.status}).`;
      try {
        const error = await response.json();
        if (typeof error.detail === "string") detail = error.detail;
        else if (Array.isArray(error.detail)) {
          const messages = error.detail.flatMap((item: unknown) =>
            typeof item === "object" &&
            item !== null &&
            "msg" in item &&
            typeof item.msg === "string"
              ? [item.msg]
              : [],
          );
          if (messages.length) detail = [...new Set(messages)].join(" ");
        }
      } catch {
        /* Preserve HTTP status if error is not JSON. */
      }
      throw new Error(detail);
    }
    return response;
  }
  return {
    get: async <T>(path: string): Promise<T> => (await request(path)).json(),
    post: async <T>(path: string, body?: unknown): Promise<T> =>
      (await request(path, "POST", body)).json(),
    patch: async <T>(path: string, body: unknown): Promise<T> =>
      (await request(path, "PATCH", body)).json(),
    blob: async (path: string): Promise<Blob> => (await request(path)).blob(),
  };
}

export type Api = ReturnType<typeof createApi>;
export const ApiContext = createContext<Api | null>(null);
export function useApi() {
  const api = useContext(ApiContext);
  if (!api) throw new Error("The local service is not connected.");
  return api;
}
export function useResource<T>(path: string, poll = false) {
  const api = useApi();
  return useQuery({
    queryKey: [path],
    queryFn: () => api.get<T>(path),
    refetchInterval: poll ? 2000 : false,
    retry: 1,
  });
}
export function useRefresh() {
  const client = useQueryClient();
  return () => client.invalidateQueries();
}
export const nativeDesktop = () => isTauri();
export async function connect(): Promise<Api> {
  const connection = isTauri()
    ? await invoke<Connection>("connection_info")
    : {
        base_url: import.meta.env.VITE_QUANTIX_API_BASE ?? "",
        token: import.meta.env.VITE_QUANTIX_API_TOKEN ?? "",
      };
  if (!connection.base_url || !connection.token)
    throw new Error(
      "The local service connection is missing. Start Quantix using the project development command.",
    );
  return createApi(connection);
}
export const choosePackage = (kind: "directory" | "zip") =>
  invoke<string | null>("choose_package", { kind });
export function errorText(error: unknown) {
  return error instanceof Error
    ? error.message
    : "The operation could not be completed.";
}
export const tenderPath = (id: string) => `/tenders/${encodeURIComponent(id)}`;
export const isActive = (status: string) =>
  status === "queued" || status === "running";
