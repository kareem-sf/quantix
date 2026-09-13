import { invoke, isTauri } from "@tauri-apps/api/core";
import { createContext, useContext } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { components } from "./bindings/api";
import { configureDiagnosticApi, notifyApiError } from "./diagnostics";

export type Schema<K extends keyof components["schemas"]> =
  components["schemas"][K];
export type Connection = { base_url: string; token: string };

export type ApiFieldError = {
  /** The server validation location, kept so forms can place the message. */
  path: Array<string | number>;
  message: string;
};

export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly requestId?: string,
    public readonly fieldErrors: ApiFieldError[] = [],
  ) {
    super(message);
    this.name = "ApiError";
  }
}

const requestIdPattern = /^[0-9a-f]{32}$/;
const sensitiveField =
  /(?:password|passphrase|api[_-]?key|token|secret|credential|authorization|private[_-]?key|access[_-]?key)/i;

function responseSanitizer(body: unknown) {
  const secrets = new Set<string>();
  function visit(value: unknown, sensitive = false, depth = 0) {
    if (depth > 10) return;
    if (sensitive && typeof value === "string" && value) secrets.add(value);
    else if (value && typeof value === "object") {
      for (const [key, nested] of Object.entries(value))
        visit(nested, sensitive || sensitiveField.test(key), depth + 1);
    }
  }
  visit(body);
  return (message: string) => {
    let clean = message;
    for (const secret of [...secrets].sort((a, b) => b.length - a.length))
      clean = clean.split(secret).join("[hidden]");
    return clean;
  };
}

export function createApi(
  connection: Connection,
  fetcher: typeof fetch = fetch,
) {
  async function request(
    path: string,
    method = "GET",
    body?: unknown,
    signal?: AbortSignal,
  ) {
    const response = await fetcher(
      `${connection.base_url.replace(/\/$/, "")}${path}`,
      {
        method,
        signal,
        headers: {
          Authorization: `Bearer ${connection.token}`,
          ...(body === undefined ? {} : { "Content-Type": "application/json" }),
        },
        ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      },
    ).catch((error: unknown) => {
      if (signal?.aborted) {
        throw new DOMException("The request was cancelled.", "AbortError");
      }
      if (error instanceof TypeError) {
        const failure = new Error(
          "The local service could not be reached. Reopen Quantix and try again.",
        );
        notifyApiError(failure, path);
        throw failure;
      }
      throw error;
    });
    if (!response.ok) {
      const sanitize = responseSanitizer(body);
      let detail = `Request failed (${response.status}).`;
      let fieldErrors: ApiFieldError[] = [];
      try {
        const error = await response.json();
        if (typeof error.detail === "string") detail = sanitize(error.detail);
        else if (Array.isArray(error.detail)) {
          fieldErrors = error.detail.flatMap((item: unknown) => {
            if (typeof item !== "object" || item === null) return [];
            const path =
              "loc" in item &&
              Array.isArray(item.loc) &&
              item.loc.every(
                (part: unknown): part is string | number =>
                  (typeof part === "string" &&
                    /^[A-Za-z_][A-Za-z0-9_]{0,119}$/.test(part) &&
                    sanitize(part) === part) ||
                  (typeof part === "number" &&
                    Number.isSafeInteger(part) &&
                    part >= 0),
              )
                ? (item.loc as Array<string | number>)
                : [];
            const message =
              "msg" in item && typeof item.msg === "string"
                ? sanitize(item.msg).slice(0, 1000)
                : "";
            return path.length && message ? [{ path, message }] : [];
          });
          const messages = fieldErrors.map((item) => item.message);
          if (messages.length) detail = [...new Set(messages)].join(" ");
        }
      } catch {
        /* Preserve HTTP status if error is not JSON. */
      }
      if (
        response.status === 404 &&
        path.startsWith("/ai/") &&
        detail.toLowerCase() === "not found"
      ) {
        detail =
          "Restart Quantix to finish updating. The running local service does not have this AI setup screen yet.";
      }
      const header = response.headers.get("X-Quantix-Request-Id");
      const requestId =
        header && requestIdPattern.test(header) ? header : undefined;
      const failure = new ApiError(
        detail,
        response.status,
        requestId,
        fieldErrors,
      );
      notifyApiError(failure, path);
      throw failure;
    }
    return response;
  }
  const api = {
    stream: (path: string, signal?: AbortSignal): Promise<Response> =>
      request(path, "GET", undefined, signal),
    get: async <T>(path: string, signal?: AbortSignal): Promise<T> =>
      (await request(path, "GET", undefined, signal)).json(),
    post: async <T>(
      path: string,
      body?: unknown,
      signal?: AbortSignal,
    ): Promise<T> => (await request(path, "POST", body, signal)).json(),
    patch: async <T>(path: string, body: unknown): Promise<T> =>
      (await request(path, "PATCH", body)).json(),
    put: async <T>(path: string, body: unknown): Promise<T> =>
      (await request(path, "PUT", body)).json(),
    delete: async <T>(path: string, body?: unknown): Promise<T> =>
      (await request(path, "DELETE", body)).json(),
    blob: async (path: string): Promise<Blob> => (await request(path)).blob(),
  };
  return api;
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
    queryFn: ({ signal }) => api.get<T>(path, signal),
    refetchInterval: poll ? 2000 : false,
    retry: 1,
  });
}
export function useRefresh() {
  const client = useQueryClient();
  return () => client.invalidateQueries();
}
export const nativeDesktop = () => isTauri();
export async function connect(signal?: AbortSignal): Promise<Api> {
  let connection: Connection;
  if (isTauri()) {
    try {
      connection = await invoke<Connection>("connection_info");
    } catch {
      // Tauri rejects Rust Result errors as strings. Normalize this boundary
      // without exposing arbitrary native payloads or losing the recovery hint.
      throw new Error(
        "The local service connection is not ready. Quantix will try again automatically.",
      );
    }
  } else {
    connection = {
      base_url: import.meta.env.VITE_QUANTIX_API_BASE ?? "",
      token: import.meta.env.VITE_QUANTIX_API_TOKEN ?? "",
    };
  }
  if (signal?.aborted)
    throw new DOMException("The request was cancelled.", "AbortError");
  if (!connection.base_url || !connection.token)
    throw new Error(
      "The local service connection is missing. Start Quantix using the project development command.",
    );
  const api = createApi(connection);
  const controller = new AbortController();
  const cancel = () => controller.abort();
  signal?.addEventListener("abort", cancel, { once: true });
  const timeout = setTimeout(cancel, 3000);
  try {
    // A published connection file is not proof the HTTP service is ready.
    await api.get<Schema<"Health">>("/health", controller.signal);
  } catch {
    if (signal?.aborted)
      throw new DOMException("The request was cancelled.", "AbortError");
    throw new Error(
      "The local service is not responding yet. Quantix will try again automatically.",
    );
  } finally {
    clearTimeout(timeout);
    signal?.removeEventListener("abort", cancel);
  }
  configureDiagnosticApi(api);
  return api;
}
export const choosePackage = (kind: "directory" | "zip") =>
  invoke<string | null>("choose_package", { kind });
export function errorText(error: unknown) {
  const message =
    error instanceof Error
      ? error.message
      : "The operation could not be completed.";
  return error instanceof ApiError && error.requestId
    ? `${message} Request reference: ${error.requestId}`
    : message;
}
export const tenderPath = (id: string) => `/tenders/${encodeURIComponent(id)}`;
export const isActive = (status: string) =>
  status === "queued" || status === "running";
