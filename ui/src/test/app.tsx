import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { vi } from "vitest";
import type { Tender } from "../api/client";
import { routes } from "../app/router";
import type { SearchHit, TenderDocument } from "../documents/queries";
import type { Connection, OfficeSettings } from "../settings/queries";

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

export interface FakeState {
  tenders: Tender[];
  connections: Connection[];
  settings: OfficeSettings;
  models: string[];
  documents: TenderDocument[];
  pages: Record<string, string>;
  hits: SearchHit[];
  /** Answer the next POST to this path with this error detail. */
  fail: Record<string, string>;
}

/** An in-memory stand-in for the local service at the fetch boundary, following the same contract. */
export function fakeService(initial: Partial<FakeState> = {}) {
  const state: FakeState = {
    tenders: [],
    connections: [],
    settings: { office_mode: "engineer", office_ai: null },
    models: ["model-b", "model-a"],
    documents: [],
    pages: {},
    hits: [],
    fail: {},
    ...initial,
  };
  const fetch = vi.fn(async (input: Request | string, init?: RequestInit) => {
    // openapi-fetch passes a Request; file uploads pass a URL and options (jsdom's FormData can't go in a Request).
    const request =
      typeof input === "string"
        ? {
            url: input,
            method: init?.method ?? "GET",
            headers: new Headers({ "content-type": "multipart/form-data" }),
            formData: async () => init?.body as FormData,
            json: async () => JSON.parse(String(init?.body)),
          }
        : input;
    const path = new URL(request.url).pathname.replace(/^\/api/, "");
    const method = request.method;
    if (method !== "GET" && state.fail[path]) return json({ detail: state.fail[path] }, 400);
    const multipart = request.headers.get("content-type")?.startsWith("multipart/form-data");
    const body = method === "GET" || method === "DELETE" || multipart ? undefined : await request.json();

    const documents = path.match(/^\/tenders\/(\w+)\/documents$/);
    if (documents && method === "POST") {
      const files = (await request.formData()).getAll("files") as File[];
      for (const file of files) {
        state.documents.push({
          id: `d${state.documents.length + 1}`,
          path: file.name,
          name: file.name.split("/").pop()!,
          kind: file.name.endsWith(".pdf") ? "pdf" : "word",
          size: file.size,
          status: "read",
          note: null,
          page_count: 1,
          group_name: null,
          description: null,
        });
      }
      return json({ added: files.length, unchanged: 0 }, 201);
    }
    if (documents) return json(state.documents);
    if (path.match(/^\/tenders\/\w+\/search$/)) return json(state.hits);
    const page = path.match(/^\/documents\/(\w+)\/pages\/(\d+)$/);
    if (page) return json({ number: Number(page[2]), text: state.pages[page[1]] ?? "", has_text: true });

    if (path === "/tenders" && method === "GET") return json(state.tenders);
    if (path === "/tenders" && method === "POST") {
      const tender = { id: `t${state.tenders.length + 1}`, created_at: "2026-09-23T10:00:00Z", ...body };
      state.tenders.unshift(tender);
      return json(tender, 201);
    }
    const tender = state.tenders.find((t) => path === `/tenders/${t.id}`);
    if (path.startsWith("/tenders/")) return tender ? json(tender) : json({ detail: "Tender not found." }, 404);

    if (path === "/ai/connections" && method === "GET") return json(state.connections);
    if (path === "/ai/connections" && method === "POST") {
      const connection: Connection = {
        id: `c${state.connections.length + 1}`,
        provider: body.provider,
        label: body.provider === "anthropic" ? "Anthropic" : body.provider,
        base_url: body.base_url,
        key_hint: `…${body.api_key.slice(-4)}`,
        checks: {},
      };
      state.connections.push(connection);
      return json(connection, 201);
    }
    const connection = state.connections.find((c) => path.startsWith(`/ai/connections/${c.id}`));
    if (connection && path.endsWith("/models")) return json(state.models);
    if (connection && path.endsWith("/checks")) {
      connection.checks[body.model] = { ok: true, message: "Works, including the tools the office needs.", checked_at: "" };
      return json(connection);
    }
    if (connection && method === "DELETE") {
      state.connections = state.connections.filter((c) => c !== connection);
      return new Response(null, { status: 204 });
    }

    if (path === "/settings" && method === "GET") return json(state.settings);
    if (path === "/settings" && method === "PATCH") {
      state.settings = { ...state.settings, ...body };
      return json(state.settings);
    }
    return json({ detail: `No fake for ${method} ${path}` }, 500);
  });
  vi.stubGlobal("fetch", fetch);
  return { state, fetch };
}

export function openApp(path: string) {
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  const queries = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queries}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}
