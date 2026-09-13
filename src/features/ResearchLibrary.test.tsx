import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ApiContext, createApi } from "../api";
import { ResearchLibrary } from "./ResearchLibrary";

const receipt = {
  id: "receipt-1",
  tender_id: "tender-1",
  run_id: null,
  actor_id: "engineer",
  requested_url: "https://public.example/pump",
  final_url: "https://public.example/pump",
  redirect_chain: ["https://public.example/pump"],
  status_code: 200,
  content_type: "text/html",
  content_bytes: 120,
  content_sha256: "a".repeat(64),
  title: "Concrete pump",
  retrieved_at: "2026-09-13T00:00:00Z",
  passages: [
    {
      id: "passage-1",
      index: 0,
      text: "Output is 70 m3 per hour.",
      sha256: "b".repeat(64),
    },
  ],
  cited: false,
};

const workingNote = {
  id: "memory-1",
  tender_id: "tender-1",
  run_id: null,
  actor_id: "engineer",
  kind: "assumption",
  title: "Waste allowance",
  content: "Check the 5% allowance.",
  source_ids: [],
  valid_until: null,
  created_at: "2026-09-13T00:00:00Z",
  dependencies: [],
  state: "needs_review",
  review_reasons: ["validity_date_reached"],
};

function setup(fetcher: typeof fetch, focusCitationId?: string) {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    fetcher,
  );
  const user = userEvent.setup({ delay: null });
  render(
    <ApiContext.Provider value={api}>
      <ResearchLibrary tenderId="tender-1" focusCitationId={focusCitationId} />
    </ApiContext.Provider>,
  );
  return user;
}

it("keeps a fetched URL uncited until an exact saved passage is selected", async () => {
  const writes: Array<{ path: string; body: Record<string, unknown> }> = [];
  const user = setup(async (url, init) => {
    const parsed = new URL(String(url));
    if (init?.method === "POST") {
      const body = JSON.parse(String(init.body));
      writes.push({ path: parsed.pathname, body });
      if (parsed.pathname.endsWith("/fetch")) return Response.json(receipt);
      if (parsed.pathname.endsWith("/citations"))
        return Response.json({
          id: "citation-1",
          tender_id: "tender-1",
          run_id: null,
          actor_id: "engineer",
          receipt_id: "receipt-1",
          passage_ids: ["passage-1"],
          purpose: body.purpose,
          url: receipt.final_url,
          retrieved_at: receipt.retrieved_at,
          content_sha256: receipt.content_sha256,
          created_at: "2026-09-13T00:01:00Z",
        });
    }
    if (parsed.pathname.endsWith("/research/market")) return Response.json([]);
    if (parsed.pathname.endsWith("/research/browser/status"))
      return Response.json({
        ready: false,
        podman_available: false,
        image: "quantix-research-browser:1",
        detail:
          "Install Podman before setting up the isolated public-page reader.",
      });
    if (parsed.pathname.endsWith("/research")) return Response.json([]);
    if (parsed.pathname.endsWith("/memory/working")) return Response.json([]);
    if (parsed.pathname.endsWith("/memory"))
      return Response.json({
        working_memory: [],
        approved_decisions: [],
        company_knowledge: [],
      });
    return Response.json({ detail: "Unexpected request" }, { status: 404 });
  });

  expect(await screen.findByText("No public sources saved yet")).toBeVisible();
  await user.type(
    screen.getByLabelText("Public URL"),
    "https://public.example/pump",
  );
  await user.click(screen.getByRole("button", { name: "Open public source" }));
  expect(await screen.findByText("Output is 70 m3 per hour.")).toBeVisible();
  expect(screen.getByText(/Not cited/)).toBeVisible();
  await user.type(
    screen.getByLabelText("Citation purpose"),
    "Support the pump output observation.",
  );
  await user.click(screen.getByRole("button", { name: "Cite this passage" }));
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[1]).toEqual({
    path: "/api/tenders/tender-1/research/citations",
    body: {
      receipt_id: "receipt-1",
      passage_ids: ["passage-1"],
      purpose: "Support the pump output observation.",
      idempotency_key: expect.any(String),
    },
  });
  expect(
    await screen.findByRole("button", { name: "Exact passage cited" }),
  ).toBeDisabled();
});

it("opens an exact saved citation context from a work product", async () => {
  const focused = {
    id: "citation-1",
    tender_id: "tender-1",
    run_id: null,
    actor_id: "engineer",
    receipt_id: "receipt-1",
    passage_ids: ["passage-1"],
    purpose: "Support the pump output comparison.",
    url: receipt.final_url,
    title: receipt.title,
    retrieved_at: receipt.retrieved_at,
    content_sha256: receipt.content_sha256,
    work_product_reference: "public_citation:citation-1",
    created_at: "2026-09-13T00:01:00Z",
  };
  setup(async (url) => {
    const parsed = new URL(String(url));
    if (parsed.pathname.endsWith("/research/citations/citation-1"))
      return Response.json(focused);
    if (parsed.pathname.endsWith("/research/market")) return Response.json([]);
    if (parsed.pathname.endsWith("/research/browser/status"))
      return Response.json({
        ready: false,
        podman_available: false,
        image: "quantix-research-browser:1",
        detail: "Reader unavailable.",
      });
    if (parsed.pathname.endsWith("/research"))
      return Response.json([{ ...receipt, cited: true }]);
    if (parsed.pathname.endsWith("/memory/working")) return Response.json([]);
    return Response.json({
      working_memory: [],
      approved_decisions: [],
      company_knowledge: [],
    });
  }, "citation-1");

  expect(await screen.findByText("Saved citation context")).toBeVisible();
  expect(screen.getByText(/Support the pump output comparison/)).toBeVisible();
  expect(screen.getByText("Output is 70 m3 per hour.")).toBeVisible();
});

it("shows separate working notes, decisions and company knowledge", async () => {
  setup(async (url) => {
    const parsed = new URL(String(url));
    if (parsed.pathname.endsWith("/research/market")) return Response.json([]);
    if (parsed.pathname.endsWith("/research/browser/status"))
      return Response.json({
        ready: false,
        podman_available: false,
        image: "quantix-research-browser:1",
        detail:
          "Install Podman before setting up the isolated public-page reader.",
      });
    if (parsed.pathname.endsWith("/research")) return Response.json([]);
    if (parsed.pathname.endsWith("/memory/working"))
      return Response.json([workingNote]);
    return Response.json({
      working_memory: [],
      approved_decisions: [
        { id: "decision-1", target_id: "finding-1", decision: "accept" },
      ],
      company_knowledge: [
        { id: "knowledge-1", title: "Plain language", needs_recheck: false },
      ],
    });
  });

  expect(await screen.findByText("Waste allowance")).toBeVisible();
  expect(screen.getByText(/Accepted decision · finding-1/)).toBeVisible();
  expect(screen.getByText("Plain language")).toBeVisible();
});

it("reloads mounted memory against an authoritative Tender revision", async () => {
  let memoryCalls = 0;
  let resolveOld!: (response: Response) => void;
  let resolveCurrent!: (response: Response) => void;
  const memoryResponse = (state: "current" | "needs_review") =>
    Response.json([
      {
        ...workingNote,
        id: "memory-linked",
        title: "Linked allowance",
        content: "Check against the revised bill.",
        source_ids: ["source-1"],
        state,
        review_reasons: state === "current" ? [] : ["source_revision_changed"],
      },
    ]);
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      const parsed = new URL(String(url));
      if (parsed.pathname.endsWith("/research/market"))
        return Response.json([]);
      if (parsed.pathname.endsWith("/research/browser/status"))
        return Response.json({
          ready: false,
          podman_available: false,
          state: "not_installed",
          image: "quantix-research-browser:1",
          image_id: null,
          detail: "Private runtime not installed.",
        });
      if (parsed.pathname.endsWith("/research")) return Response.json([]);
      if (parsed.pathname.endsWith("/memory"))
        return Response.json({
          working_memory: [],
          approved_decisions: [],
          company_knowledge: [],
        });
      memoryCalls += 1;
      return new Promise<Response>((resolve) => {
        if (memoryCalls === 1) resolveOld = resolve;
        else resolveCurrent = resolve;
      });
    },
  );
  const view = render(
    <ApiContext.Provider value={api}>
      <ResearchLibrary tenderId="tender-1" tenderRevision={1} />
    </ApiContext.Provider>,
  );
  await waitFor(() => expect(memoryCalls).toBe(1));
  view.rerender(
    <ApiContext.Provider value={api}>
      <ResearchLibrary tenderId="tender-1" tenderRevision={2} />
    </ApiContext.Provider>,
  );
  await waitFor(() => expect(memoryCalls).toBe(2));
  await act(async () => resolveCurrent(memoryResponse("needs_review")));
  await waitFor(() => expect(screen.getByText("Needs review")).toBeVisible());
  await act(async () => resolveOld(memoryResponse("current")));
  expect(screen.getByText("Needs review")).toBeVisible();
  expect(screen.queryByText("assumption")).not.toBeInTheDocument();
});

it("loads the fifty-first public receipt and the 101st working note", async () => {
  const publicReceipts = Array.from({ length: 50 }, (_, index) => ({
    ...receipt,
    id: `receipt-${index + 1}`,
    title: `Public source ${index + 1}`,
  }));
  const notes = Array.from({ length: 101 }, (_, index) => ({
    ...workingNote,
    id: `memory-${101 - index}`,
    title:
      index === 100 ? "Oldest working note" : `Working note ${101 - index}`,
    state: "current",
    review_reasons: [],
  }));
  setup(async (url) => {
    const parsed = new URL(String(url));
    const offset = Number(parsed.searchParams.get("offset") ?? 0);
    if (parsed.pathname.endsWith("/research/browser/status"))
      return Response.json(browserUnavailable());
    if (parsed.pathname.endsWith("/research/market")) return Response.json([]);
    if (parsed.pathname.endsWith("/research"))
      return Response.json(
        offset === 0
          ? publicReceipts
          : offset === 50
            ? [{ ...receipt, id: "receipt-51", title: "Public source 51" }]
            : [],
      );
    if (parsed.pathname.endsWith("/memory/working"))
      return Response.json(notes.slice(offset, offset + 50));
    return Response.json({
      working_memory: [],
      approved_decisions: [],
      company_knowledge: [],
    });
  });

  await screen.findByText("Public source 1");
  await userEvent.click(
    screen.getByRole("button", { name: "Load older public sources" }),
  );
  expect(await screen.findByText("Public source 51")).toBeVisible();
  await userEvent.click(
    screen.getByRole("button", { name: "Load older working notes" }),
  );
  await screen.findByText("Working note 51");
  await userEvent.click(
    screen.getByRole("button", { name: "Load older working notes" }),
  );
  expect(await screen.findByText("Oldest working note")).toBeVisible();
  expect(screen.getByText("Working note 101")).toBeVisible();
});

it("loads an exact cited receipt when it is older than the first page", async () => {
  const focused = {
    id: "citation-old",
    tender_id: "tender-1",
    run_id: null,
    actor_id: "engineer",
    receipt_id: "receipt-old",
    passage_ids: ["passage-old"],
    purpose: "Support the historical comparison.",
    url: "https://public.example/old",
    title: "Older cited source",
    retrieved_at: "2026-01-01T00:00:00Z",
    content_sha256: "c".repeat(64),
    work_product_reference: "public_citation:citation-old",
    created_at: "2026-01-01T00:01:00Z",
  };
  const oldReceipt = {
    ...receipt,
    id: "receipt-old",
    title: "Older cited source",
    final_url: focused.url,
    requested_url: focused.url,
    content_sha256: focused.content_sha256,
    passages: [
      {
        id: "passage-old",
        index: 0,
        text: "Historical cited passage.",
        sha256: "d".repeat(64),
      },
    ],
    cited: true,
  };
  setup(async (url) => {
    const parsed = new URL(String(url));
    if (parsed.pathname.endsWith("/research/citations/citation-old"))
      return Response.json(focused);
    if (parsed.pathname.endsWith("/research/receipts/receipt-old"))
      return Response.json(oldReceipt);
    if (parsed.pathname.endsWith("/research/browser/status"))
      return Response.json(browserUnavailable());
    if (parsed.pathname.endsWith("/research/market")) return Response.json([]);
    if (parsed.pathname.endsWith("/research"))
      return Response.json(
        Array.from({ length: 50 }, (_, index) => ({
          ...receipt,
          id: `recent-${index}`,
          title: `Recent source ${index + 1}`,
        })),
      );
    if (parsed.pathname.endsWith("/memory/working")) return Response.json([]);
    return Response.json({
      working_memory: [],
      approved_decisions: [],
      company_knowledge: [],
    });
  }, "citation-old");

  expect(await screen.findByText("Older cited source")).toBeVisible();
  expect(screen.getByText("Historical cited passage.")).toBeVisible();
  expect(screen.getByText("Saved citation context")).toBeVisible();
});

it("keeps last successful research data and never calls an unavailable read empty", async () => {
  let failing = false;
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url) => {
      const parsed = new URL(String(url));
      if (
        failing &&
        (parsed.pathname.endsWith("/research") ||
          parsed.pathname.endsWith("/memory/working"))
      )
        return Response.json(
          { detail: "The saved research read is temporarily unavailable." },
          { status: 503 },
        );
      if (parsed.pathname.endsWith("/research/browser/status"))
        return Response.json(browserUnavailable());
      if (parsed.pathname.endsWith("/research/market"))
        return Response.json([]);
      if (parsed.pathname.endsWith("/research"))
        return Response.json([receipt]);
      if (parsed.pathname.endsWith("/memory/working"))
        return Response.json([
          { ...workingNote, state: "current", review_reasons: [] },
        ]);
      return Response.json({
        working_memory: [],
        approved_decisions: [],
        company_knowledge: [],
      });
    },
  );
  const view = render(
    <ApiContext.Provider value={api}>
      <ResearchLibrary tenderId="tender-1" tenderRevision={1} />
    </ApiContext.Provider>,
  );
  expect(await screen.findByText("Concrete pump")).toBeVisible();
  expect(screen.getByText("Waste allowance")).toBeVisible();

  failing = true;
  view.rerender(
    <ApiContext.Provider value={api}>
      <ResearchLibrary tenderId="tender-1" tenderRevision={2} />
    </ApiContext.Provider>,
  );
  expect(
    await screen.findByRole("button", { name: "Retry research reads" }),
  ).toBeVisible();
  expect(screen.getByText("Concrete pump")).toBeVisible();
  expect(screen.getByText("Waste allowance")).toBeVisible();
  expect(
    screen.queryByText("No public sources saved yet"),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByText("No working notes have been saved yet."),
  ).not.toBeInTheDocument();
});

function browserUnavailable() {
  return {
    ready: false,
    podman_available: false,
    state: "not_installed",
    image: "localhost/quantix-research-browser:2",
    image_id: null,
    detail: "Private runtime not installed.",
  };
}
