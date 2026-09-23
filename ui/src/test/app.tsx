import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { vi } from "vitest";
import type { Tender } from "../api/client";
import { routes } from "../app/router";
import type { SearchHit, TenderDocument } from "../documents/queries";
import type { BoqItem, Fact, LibraryEntry, Markups, Priced, Summary } from "../estimate/queries";
import type { Decision, Message, Staff, Task } from "../office/queries";
import type { Comparison, Measurement, Sheet } from "../takeoff/queries";
import type { Connection, OfficeSettings } from "../settings/queries";
import type { Company, Package } from "../subcontract/queries";

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
  staff: Staff[];
  messages: Message[];
  decisions: Decision[];
  tasks: Task[];
  officeState: "working" | "paused" | "idle";
  items: BoqItem[];
  facts: Fact[];
  sheets: Sheet[];
  measurements: Measurement[];
  comparison: Comparison[];
  scales: unknown[];
  priced: Priced[];
  summary: Summary | null;
  markups: Markups | null;
  library: LibraryEntry[];
  packages: Package[];
  directory: Company[];
  decided: { id: string; approve: boolean; save_to_library?: boolean }[];
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
    staff: [],
    messages: [],
    decisions: [],
    tasks: [],
    officeState: "idle",
    items: [],
    facts: [],
    sheets: [],
    measurements: [],
    comparison: [],
    scales: [],
    priced: [],
    summary: null,
    markups: null,
    library: [],
    packages: [],
    directory: [],
    decided: [],
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
    const body = method === "GET" || method === "DELETE" || multipart ? undefined : await request.json().catch(() => undefined);

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

    const office = path.match(/^\/tenders\/(\w+)\/(office|messages|decisions|tasks)$/);
    if (office?.[2] === "office") {
      const waiting = state.decisions.filter((d) => d.status === "waiting").length;
      const ai_ready = state.settings.office_ai !== null;
      return json({ state: state.officeState, ai_ready, staff: state.staff, waiting });
    }
    if (office?.[2] === "messages" && method === "POST") {
      const message: Message = {
        id: state.messages.length + 1,
        sender: "engineer",
        channel: body.channel,
        kind: "message",
        text: body.text,
        created_at: "2026-09-23T10:00:00Z",
      };
      state.messages.push(message);
      return json(message, 201);
    }
    if (office?.[2] === "messages") {
      const channel = new URL(request.url).searchParams.get("channel") ?? "team";
      return json(state.messages.filter((m) => m.channel === channel));
    }
    if (office?.[2] === "decisions") return json(state.decisions);
    if (office?.[2] === "tasks") return json(state.tasks);
    const answer = path.match(/^\/decisions\/(\w+)\/answer$/);
    if (answer) {
      const decision = state.decisions.find((d) => d.id === answer[1])!;
      Object.assign(decision, { status: "answered", answer: body.answer });
      return json(decision);
    }
    if (path.match(/^\/tenders\/\w+\/office\/stop$/)) {
      state.officeState = "paused";
      return new Response(null, { status: 204 });
    }

    if (path.match(/^\/tenders\/\w+\/estimate$/)) {
      const summary = state.summary ?? {
        currency: "", priced: 0, items: 0, waiting: 0, net: "0.00", preliminaries: "0.00", overheads: "0.00",
        profit: "0.00", adjustment: "0.00", total: "0.00", vat_rate: null, vat: null, total_with_vat: null, unpriced: [],
      };
      return json({ items: state.priced, markups: state.markups, summary });
    }
    const rated = path.match(/^\/(rates|markups)\/(\w+)\/decision$/);
    if (rated) {
      state.decided.push({ id: rated[2], ...body });
      return json(null);
    }
    if (path.match(/^\/tenders\/\w+\/rates\/approve-all$/)) return json({ approved: 1 });
    if (path === "/library" && method === "GET") return json(state.library);
    if (path === "/library" && method === "POST") {
      const entry = { id: `l${state.library.length + 1}`, ...body };
      state.library.push(entry);
      return json(entry, 201);
    }
    if (path.startsWith("/library/") && method === "DELETE") {
      state.library = state.library.filter((l) => `/library/${l.id}` !== path);
      return new Response(null, { status: 204 });
    }
    if (path.match(/^\/tenders\/\w+\/packages$/)) return json(state.packages);
    const choice = path.match(/^\/packages\/(\w+)\/choice$/);
    if (choice) {
      state.packages.find((p) => p.id === choice[1])!.selected_quote_id = body.quote_id;
      return json(null);
    }
    const sent = path.match(/^\/enquiries\/(\w+)\/sent$/);
    if (sent) {
      for (const p of state.packages) for (const e of p.enquiries) if (e.id === sent[1]) e.status = "sent";
      return json(null);
    }
    if (path === "/directory" && method === "GET") return json(state.directory);
    if (path === "/directory" && method === "POST") {
      const company = { id: `co${state.directory.length + 1}`, added_by: "engineer", ...body };
      state.directory.push(company);
      return json(company, 201);
    }
    if (path.startsWith("/directory/") && method === "DELETE") {
      state.directory = state.directory.filter((c) => `/directory/${c.id}` !== path);
      return new Response(null, { status: 204 });
    }
    if (path.match(/^\/tenders\/\w+\/takeoff$/))
      return json({ sheets: state.sheets, measurements: state.measurements, comparison: state.comparison });
    if (path.match(/^\/documents\/\w+\/pages\/\d+\/vertices$/)) return json([[101, 101]]);
    const sheet = path.match(/^\/documents\/(\w+)\/pages\/(\d+)\/sheet$/);
    if (sheet) return json(state.sheets.find((s) => s.document_id === sheet[1] && s.page === Number(sheet[2])));
    if (path.match(/^\/tenders\/\w+\/scales$/)) {
      state.scales.push(body);
      return json({ id: "sc1", metres_per_point: 0.1, status: "approved", proposed_by: "engineer", ...body }, 201);
    }
    if (path.match(/^\/tenders\/\w+\/measurements$/)) {
      const m = { id: `m${state.measurements.length + 1}`, quantity: null, status: "approved", proposed_by: "engineer", ...body };
      state.measurements.push(m);
      return json(m, 201);
    }
    const measured = path.match(/^\/measurements\/(\w+)(\/decision)?$/);
    if (measured) {
      const m = state.measurements.find((x) => x.id === measured[1])!;
      m.status = method === "DELETE" ? "rejected" : body.approve ? "approved" : "rejected";
      return method === "DELETE" ? new Response(null, { status: 204 }) : json(null);
    }
    if (path.match(/^\/tenders\/\w+\/boq$/)) return json({ items: state.items, facts: state.facts });
    if (path.match(/^\/tenders\/\w+\/gates$/)) {
      const count = (list: { status: string }[]) => list.filter((r) => r.status === "proposed").length;
      return json({ boq: count(state.items), facts: count(state.facts), takeoff: count(state.measurements), pricing: 0, subcontract: 0 });
    }
    if (path.match(/^\/tenders\/\w+\/boq\/approve-all$/)) {
      const waiting = state.items.filter((i) => i.status === "proposed");
      for (const item of waiting) item.status = "approved";
      return json({ approved: waiting.length });
    }
    const decided = path.match(/^\/(boq|facts)\/(\w+)\/decision$/);
    if (decided) {
      const record = (decided[1] === "boq" ? state.items : state.facts).find((r) => r.id === decided[2])!;
      Object.assign(record, { status: body.approve ? "approved" : "rejected", reason: body.reason });
      return json(null);
    }

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
