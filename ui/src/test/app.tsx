import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { vi } from "vitest";
import type { Tender } from "../api/client";
import { routes } from "../app/router";
import type { SearchHit, TenderDocument, WorkbookSheet } from "../documents/queries";
import type { BoqItem, Fact, LibraryEntry, Markups, Priced, Summary } from "../estimate/queries";
import type { Decision, Message, Staff, Task, Turn, TurnStep } from "../office/queries";
import type { Comparison, Measurement, Sheet } from "../takeoff/queries";
import type { DrawingInfo, LayerMap, Problem, TenderQuery } from "../takeoff/cad";
import type { Connection, OfficeSettings, Usage, WebKeys } from "../settings/queries";
import type { Profile, Rule } from "../company/queries";
import type { Company, Package } from "../subcontract/queries";
import type { Requirement } from "../submission/queries";
import type { Finding, Lesson } from "../review/queries";

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

export interface FakeState {
  tenders: (Omit<Tender, "outcome" | "archived"> & Partial<Pick<Tender, "outcome" | "archived">>)[];
  rules: Rule[];
  company: Profile;
  connections: Connection[];
  settings: OfficeSettings;
  models: string[];
  documents: TenderDocument[];
  pages: Record<string, string>;
  /** The supplied files, by document id, as a viewer fetches them. */
  files: Record<string, string>;
  /** A workbook's sheets as the Documents screen shows them, by sheet number; and the files opened in their apps. */
  workbook: Record<number, WorkbookSheet>;
  opened: string[];
  /** What the desktop app asked Quantix to import from this computer. */
  imported: { folder: string | null; files: string[] }[];
  hits: SearchHit[];
  staff: Staff[];
  messages: Message[];
  decisions: Decision[];
  tasks: Task[];
  officeState: "working" | "paused" | "idle";
  /** Why the office paused itself. */
  notice: string | null;
  /** Each turn with its steps. */
  turns: (Turn & { log: TurnStep[] })[];
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
  requirements: Requirement[];
  columns: unknown[];
  exports: { spread_markups: boolean }[];
  /** The built packages the engineer opened in Explorer. */
  folders: string[];
  decided: { id: string; approve: boolean; save_to_library?: boolean }[];
  reopened: { kind: string; id: string; reason: string }[];
  /** What the checks find, by record id. */
  findings: Record<string, Finding[]>;
  /** The tender audit. */
  audit: Finding[];
  /** What the office learned on the tender. */
  lessons: Lesson[];
  /** How each AI has done, and what each tender used. */
  usage: Usage;
  /** The web research keys, as hints. */
  webKeys: WebKeys;
  /** Answer the next POST to this path with this error detail. */
  fail: Record<string, string>;
  /** A CAD drawing: what it holds, its packed screen copy, and what the engineer set and measured on it. */
  drawing: DrawingInfo | null;
  screen: ArrayBuffer | null;
  /** The volume of 3D solids among the objects chosen, in m³. */
  volume: number | null;
  units: unknown[];
  measured: unknown[];
  queries: TenderQuery[];
  layerMaps: LayerMap[];
  checks: Problem[];
}

/** An in-memory stand-in for the local service at the fetch boundary, following the same contract. */
export function fakeService(initial: Partial<FakeState> = {}) {
  const state: FakeState = {
    tenders: [],
    rules: [],
    company: { name: "", address: "", cr_number: "", vat_number: "", has_logo: false },
    connections: [],
    settings: { office_mode: "engineer", office_ai: null, tender_allowance: null, notifications: "all" },
    models: ["model-b", "model-a"],
    documents: [],
    pages: {},
    files: {},
    workbook: {},
    opened: [],
    imported: [],
    hits: [],
    staff: [],
    messages: [],
    decisions: [],
    tasks: [],
    officeState: "idle",
    notice: null,
    turns: [],
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
    requirements: [],
    columns: [],
    exports: [],
    folders: [],
    decided: [],
    reopened: [],
    findings: {},
    audit: [],
    lessons: [],
    usage: { models: [], tenders: [] },
    webKeys: { firecrawl: null, tinyfish: null },
    fail: {},
    drawing: null,
    screen: null,
    volume: null,
    units: [],
    measured: [],
    queries: [],
    layerMaps: [],
    checks: [],
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
          scans_to_read: 0,
          opened: 0,
          cited: 0,
        });
      }
      return json({ added: files.length, unchanged: 0 }, 201);
    }
    if (path.match(/^\/tenders\/\w+\/documents\/import$/) && method === "POST") {
      state.imported.push(body); // the folder or files the desktop pickers chose, read from where they are
      return json({ added: 1, unchanged: 0 });
    }
    if (documents) return json(state.documents);
    if (path.match(/^\/tenders\/\w+\/search$/)) return json(state.hits);
    const page = path.match(/^\/documents\/(\w+)\/pages\/(\d+)$/);
    if (page) return json({ number: Number(page[2]), text: state.pages[page[1]] ?? "", has_text: true });
    const file = path.match(/^\/documents\/(\w+)\/file$/);
    if (file) return file[1] in state.files ? new Response(state.files[file[1]]) : json({ detail: "Document not found." }, 404);
    const workbook = path.match(/^\/documents\/\w+\/sheets\/(\d+)$/);
    if (workbook) {
      const hidden = new URL(request.url).searchParams.get("hidden") === "true";
      const sheet = state.workbook[Number(workbook[1])];
      return sheet ? json({ ...sheet, columns: sheet.columns.filter((c) => hidden || !c.hidden) }) : json({ detail: "This workbook has no such sheet." }, 404);
    }
    const opened = path.match(/^\/documents\/(\w+)\/open$/);
    if (opened && method === "POST") {
      state.opened.push(opened[1]);
      return new Response(null, { status: 204 });
    }

    const office = path.match(/^\/tenders\/(\w+)\/(office|messages|decisions|tasks)$/);
    if (office?.[2] === "office") {
      const waiting = state.decisions.filter((d) => d.status === "waiting").length;
      const ai_ready = state.settings.office_ai !== null;
      const notice = state.officeState === "paused" ? state.notice : null;
      return json({ state: state.officeState, notice, ai_ready, staff: state.staff, waiting });
    }
    if (path.match(/^\/tenders\/\w+\/turns$/)) {
      const person = new URL(request.url).searchParams.get("staff_id");
      return json(state.turns.filter((t) => !person || t.staff_id === person).map(({ log, ...turn }) => ({ ...turn, steps: log.length })));
    }
    const turn = path.match(/^\/turns\/(\d+)$/);
    if (turn) return json(state.turns.find((t) => t.id === Number(turn[1])));
    if (office?.[2] === "messages" && method === "POST") {
      const message: Message = {
        id: state.messages.length + 1,
        sender: "engineer",
        channel: body.channel,
        kind: "message",
        text: body.text,
        sources: null,
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
    if (path.match(/^\/tenders\/\w+\/submission$/)) return json({ requirements: state.requirements, columns: state.columns });
    const draftDecision = path.match(/^\/drafts\/(\w+)\/decision$/);
    if (draftDecision) {
      const r = state.requirements.find((x) => x.draft?.id === draftDecision[1])!;
      state.decided.push({ id: draftDecision[1], ...body });
      r.draft!.status = body.approve ? "approved" : "rejected";
      r.state = body.approve ? "ready" : "missing";
      return json(null);
    }
    const dropped = path.match(/^\/requirements\/(\w+)$/);
    if (dropped && method === "DELETE") {
      state.requirements = state.requirements.filter((r) => r.id !== dropped[1]);
      return new Response(null, { status: 204 });
    }
    const readied = path.match(/^\/requirements\/(\w+)\/ready$/);
    if (readied) {
      const r = state.requirements.find((x) => x.id === readied[1])!;
      Object.assign(r, { ready_note: body.ready ? body.note : null, state: body.ready ? "ready" : "missing" });
      return json(null);
    }
    if (path.match(/^\/tenders\/\w+\/requirements$/) && method === "POST") {
      state.requirements.push({
        id: `rq${state.requirements.length + 1}`, section: body.section, title: body.title, document_id: null,
        document_name: null, page: null, quote: null, added_by: "engineer", reviewed_by: null, state: "missing", draft: null,
        ready_note: null, file_name: null,
      });
      return json(null, 201);
    }
    const attached = path.match(/^\/requirements\/(\w+)\/file$/);
    if (attached) {
      const file = (await request.formData()).get("file") as File;
      Object.assign(state.requirements.find((r) => r.id === attached[1])!, { file_name: file.name, state: "ready" });
      return json(null);
    }
    const folder = path.match(/^\/exports\/([^/]+)\/open$/);
    if (folder) {
      state.folders.push(decodeURIComponent(folder[1]));
      return new Response(null, { status: 204 });
    }
    if (path.match(/^\/tenders\/\w+\/export$/)) {
      state.exports.push(body);
      return json({
        folder: "Synthetic school 2026-09-23 1000",
        files: ["Priced Bill.xlsx", "Checklist.xlsx"],
        priced_total: "133048.09",
        summary_total: "133048.08",
        factor: "1.1",
        not_ready: ["Bid bond"],
      });
    }
    if (path === "/directory" && method === "GET") return json(state.directory);
    if (path === "/directory" && method === "POST") {
      const { different_from: differentFrom, ...fields } = body;
      const firms = state.directory
        .filter((c) => fields.name.toLowerCase().includes(c.name.toLowerCase()) && !differentFrom.includes(c.name))
        .map((c) => c.name);
      const message = `${fields.name} may be the same firm as ${firms[0]}.`;
      if (firms.length) return json({ detail: { message, firms } }, 409);
      const company = { id: `co${state.directory.length + 1}`, added_by: "engineer", aliases: [], ...fields };
      state.directory.push(company);
      return json(company, 201);
    }
    const merged = path.match(/^\/directory\/(\w+)\/merge$/);
    if (merged && method === "POST") {
      const duplicate = state.directory.find((c) => c.id === merged[1])!;
      state.directory = state.directory.filter((c) => c !== duplicate);
      state.directory.find((c) => c.id === body.into)!.aliases.push(duplicate.name);
      return new Response(null, { status: 204 });
    }
    if (path.startsWith("/directory/") && method === "DELETE") {
      state.directory = state.directory.filter((c) => `/directory/${c.id}` !== path);
      return new Response(null, { status: 204 });
    }
    if (path.match(/^\/tenders\/\w+\/takeoff$/))
      return json({ sheets: state.sheets, measurements: state.measurements, comparison: state.comparison });
    if (path.match(/^\/documents\/\w+\/pages\/\d+\/screen$/) && state.screen)
      return new Response(state.screen, { headers: { "Content-Type": "application/octet-stream" } });
    if (path.match(/^\/documents\/\w+\/drawing$/) && state.drawing) return json(state.drawing);
    if (path.match(/^\/documents\/\w+\/rooms$/)) return json([]);
    if (path.match(/^\/documents\/\w+\/pages\/\d+\/choose$/))
      return json({ objects: body.objects, keys: body.objects.map((o: number) => `K${o}`), count: body.objects.length, length_m: 20, area_m2: null, volume_m3: state.volume });
    if (path.match(/^\/tenders\/\w+\/units$/)) {
      state.units.push(body);
      return json({ id: "u1", name: body.units, metres: 0.001, status: "approved", note: body.units }, 201);
    }
    if (path.match(/^\/tenders\/\w+\/drawing-measurements$/)) {
      state.measured.push(body);
      return json({ id: "m9", quantity: "20.000" }, 201);
    }
    if (path.match(/^\/documents\/\w+\/pages\/\d+\/region$/)) {
      const [x, y] = body.point; // the square of 10 drawing units around the click
      return json({ ring: [[x - 5, y - 5], [x + 5, y - 5], [x + 5, y + 5], [x - 5, y + 5]], area_m2: 100, perimeter_m: 40 });
    }
    if (path.match(/^\/tenders\/\w+\/queries$/)) return json(state.queries);
    if (path.match(/^\/tenders\/\w+\/layer-maps$/)) return json(state.layerMaps);
    if (path.match(/^\/tenders\/\w+\/checks$/)) return json(state.checks);
    const queried = path.match(/^\/(queries|layer-maps)\/(\w+)\/decision$/);
    if (queried) {
      state.decided.push({ id: queried[2], ...body });
      const record = [...state.queries, ...state.layerMaps].find((r) => r.id === queried[2]);
      if (record) record.status = body.approve ? "approved" : "rejected";
      return json(null);
    }
    if (path.match(/^\/documents\/\w+\/pages\/\d+\/vertices$/)) return json([[101, 101]]);
    const sheet = path.match(/^\/documents\/(\w+)\/pages\/(\d+)\/sheet$/);
    if (sheet) return json(state.sheets.find((s) => s.document_id === sheet[1] && s.page === Number(sheet[2])));
    if (path.match(/^\/tenders\/\w+\/scales$/)) {
      state.scales.push(body);
      return json({ id: "sc1", metres_per_point: 0.1, ratio: 283, status: "approved", proposed_by: "engineer", ...body }, 201);
    }
    if (path.match(/^\/tenders\/\w+\/measurements$/)) {
      const m = { id: `m${state.measurements.length + 1}`, quantity: null, status: "approved", proposed_by: "engineer", ...body };
      state.measurements.push(m);
      return json(m, 201);
    }
    const scaled = path.match(/^\/scales\/(\w+)\/decision$/);
    if (scaled) {
      state.decided.push({ id: scaled[1], ...body });
      return json(null);
    }
    const measured = path.match(/^\/measurements\/(\w+)(\/decision)?$/);
    if (measured) {
      const m = state.measurements.find((x) => x.id === measured[1])!;
      m.status = method === "DELETE" ? "rejected" : body.approve ? "approved" : "rejected";
      return method === "DELETE" ? new Response(null, { status: 204 }) : json(null);
    }
    if (path.match(/^\/tenders\/\w+\/boq$/)) return json({ items: state.items, facts: state.facts });
    if (path.match(/^\/tenders\/\w+\/gates$/)) {
      const count = (list: { status: string }[], status = "reviewed") => list.filter((r) => r.status === status).length;
      const manager = [state.items, state.facts, state.measurements].reduce((n, list) => n + count(list, "proposed"), 0);
      const pricing = state.priced.filter((p) => p.rate?.status === "reviewed").length + (state.markups?.status === "reviewed" ? 1 : 0);
      const enquiries = state.packages.flatMap((p) => p.enquiries).filter((e) => e.status === "draft" && e.reviewed_by).length;
      return json({ manager, boq: count(state.items), facts: count(state.facts), takeoff: count(state.measurements), drawings: 0, pricing, subcontract: 0, enquiries, submission: 0 });
    }
    if (path.match(/^\/tenders\/\w+\/review$/)) return json([]);
    if (path.match(/^\/tenders\/\w+\/audit$/)) return json(state.audit);
    if (path.match(/^\/tenders\/\w+\/lessons$/)) return json(state.lessons.filter((l) => l.status !== "dropped"));
    if (path === "/lessons") return json(state.lessons.filter((l) => l.status === "tender").map((l) => ({ tender_name: state.tenders[0]?.name ?? null, ...l })));
    const lesson = state.lessons.find((l) => path === `/lessons/${l.id}`);
    if (lesson && method === "PATCH") {
      lesson.status = body.status;
      if (body.status === "kept")
        state.rules.push({ id: `r${state.rules.length + 1}`, topic: lesson.topic, text: lesson.text, example: false, created_at: "" });
      return json(lesson);
    }
    if (path === "/ai/usage") return json(state.usage);
    const checked = path.match(/^\/records\/\w+\/(\w+)\/findings$/);
    if (checked) return json(state.findings[checked[1]] ?? []);
    const reopened = path.match(/^\/records\/(\w+)\/(\w+)\/reopen$/);
    if (reopened) {
      state.reopened.push({ kind: reopened[1], id: reopened[2], reason: body.reason });
      return new Response(null, { status: 204 });
    }
    if (path.match(/^\/tenders\/\w+\/boq\/approve-all$/)) {
      const waiting = state.items.filter((i) => i.status === "reviewed");
      for (const item of waiting) item.status = "approved";
      return json({ approved: waiting.length });
    }
    const decided = path.match(/^\/(boq|facts)\/(\w+)\/decision$/);
    if (decided) {
      const record = (decided[1] === "boq" ? state.items : state.facts).find((r) => r.id === decided[2])!;
      Object.assign(record, { status: body.approve ? "approved" : "rejected", reason: body.reason });
      return json(null);
    }

    if (path === "/company" && method === "GET") return json(state.company);
    if (path === "/company" && method === "PUT") {
      state.company = { ...body, has_logo: state.company.has_logo };
      return json(state.company);
    }
    if (path === "/company/logo" && method !== "GET") {
      state.company = { ...state.company, has_logo: method === "PUT" };
      return new Response(null, { status: 204 });
    }
    if (path === "/rules" && method === "GET") return json(state.rules);
    if (path === "/rules" && method === "POST") {
      const rule = { id: `r${state.rules.length + 1}`, created_at: "2026-09-23T10:00:00Z", example: false, ...body };
      state.rules.push(rule);
      return json(rule, 201);
    }
    const rule = state.rules.find((r) => path === `/rules/${r.id}`);
    if (rule && method === "PATCH") {
      Object.assign(rule, body, { example: false });
      return json(rule);
    }
    if (path.startsWith("/rules/") && method === "DELETE") {
      state.rules = state.rules.filter((r) => `/rules/${r.id}` !== path);
      return new Response(null, { status: 204 });
    }
    if (path === "/tenders" && method === "GET") return json(state.tenders.map((t) => ({ outcome: "open", archived: false, ...t })));
    if (path === "/desk") {
      // every tender at a glance; the fake keeps one tender's records, so each tender shows them
      const reviewed = (list: { status: string }[]) => list.filter((r) => r.status === "reviewed").length;
      const waiting = reviewed(state.items) + reviewed(state.facts) + reviewed(state.measurements) +
        state.decisions.filter((d) => d.status === "waiting").length;
      return json(state.tenders.map((t) => ({
        outcome: "open", archived: false, outcome_at: null, ...t,
        team: state.officeState, doing: null, waiting,
        documents: state.documents.length, read: state.documents.filter((d) => d.status !== "waiting" && d.status !== "reading").length,
        items: state.items.length, priced: state.priced.filter((i) => i.rate).length,
        currency: state.summary?.currency ?? "", total: state.summary?.priced ? state.summary.total : null,
        packages: state.packages.length, chosen: state.packages.filter((p) => p.selected_quote_id).length,
        requirements: state.requirements.length, ready: state.requirements.filter((r) => r.state === "ready").length,
      })));
    }
    if (path === "/tenders" && method === "POST") {
      const tender = { id: `t${state.tenders.length + 1}`, created_at: "2026-09-23T10:00:00Z", ...body };
      state.tenders.unshift(tender);
      return json(tender, 201);
    }
    const tender = state.tenders.find((t) => path === `/tenders/${t.id}`);
    if (tender && method === "PATCH") {
      Object.assign(tender, body);
      if ("due_date" in body) {  // the engineer's own date, as the service records it
        const set_at = "2026-09-28T07:40:00Z";
        tender.due_date_source = body.due_date ? { basis: "engineer", set_by: null, set_at, document_id: null, document_name: null, page: null, quote: null } : null;
      }
    }
    if (tender && method === "DELETE") {
      state.tenders = state.tenders.filter((t) => t !== tender);
      return new Response(null, { status: 204 });
    }
    if (path.startsWith("/tenders/")) return tender ? json({ outcome: "open", archived: false, ...tender }) : json({ detail: "Tender not found." }, 404);

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
      connection.checks[body.model] = { ok: true, message: "Works, including the tools the office needs.", checked_at: "", sees_images: true };
      return json(connection);
    }
    if (connection && method === "DELETE") {
      state.connections = state.connections.filter((c) => c !== connection);
      return new Response(null, { status: 204 });
    }

    if (path === "/web/keys") return json(state.webKeys);
    const webKey = path.match(/^\/web\/keys\/(firecrawl|tinyfish)$/);
    if (webKey) {
      state.webKeys = { ...state.webKeys, [webKey[1]]: method === "PUT" ? `…${body.api_key.slice(-4)}` : null };
      return method === "PUT" ? json(state.webKeys) : new Response(null, { status: 204 });
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
