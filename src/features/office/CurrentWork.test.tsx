import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, type Api, type Schema } from "../../api";
import { CurrentWork } from "./CurrentWork";

const brief = {
  id: "brief-2",
  tender_id: "tender-a",
  version: 2,
  run_id: "run-a",
  author: "manager-a",
  created_at: "2026-09-13T08:00:00Z",
  outcome: "Compare supplier delivery for the ground slab.",
  status: "waiting_for_engineer",
  next_step: "Confirm whether pumping is included.",
  done_when: ["Both quotes compared on a delivered basis."],
  steps: [
    { title: "Read supplier A terms", state: "done", note: "" },
    {
      title: "Read supplier B terms",
      state: "blocked",
      note: "Quote is unreadable.",
    },
  ],
  settled: [
    {
      text: "Supplier A delivers in 5 working days.",
      source_ids: ["source-a"],
    },
  ],
  open_questions: [
    {
      text: "Is pumping included?",
      owner: "engineer",
      affects: "placement cost",
    },
  ],
  work_product_ids: ["product-a"],
  work_products: [
    {
      product_id: "product-a",
      title: "Delivery comparison",
      kind: "comparison",
      version: 3,
      dependency_state: "needs_review",
    },
  ],
  dependency_state: "needs_review",
  review_reasons: ["source_revision_changed"],
  progress_current: true,
} satisfies Schema<"WorkBrief">;

function renderWith(get: Api["get"], onSource = vi.fn()) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={{ get } as unknown as Api}>
        <CurrentWork tenderId="tender-a" onSource={onSource} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return onSource;
}

it("shows the Manager's saved progress with the next step first and details under More options", async () => {
  const onSource = renderWith(vi.fn(async () => ({ brief })) as Api["get"]);
  const card = await screen.findByRole("region", { name: "Current work" });
  expect(within(card).getByText(brief.outcome)).toBeVisible();
  expect(within(card).getByText("Waiting for you")).toBeVisible();
  expect(
    within(card).getByText("Confirm whether pumping is included."),
  ).toBeVisible();
  const steps = within(card).getByRole("list", { name: "Steps" });
  expect(within(steps).getByText("Blocked:")).toBeInTheDocument();
  expect(within(steps).getByText("Quote is unreadable.")).toBeVisible();
  expect(
    within(card).getByText("For you · Affects placement cost"),
  ).toBeVisible();
  expect(within(card).getByRole("status")).toHaveTextContent(
    "A source behind this work changed",
  );
  expect(within(card).queryByText("Delivery comparison")).not.toBeVisible();

  await userEvent.click(within(card).getByText("More options"));
  expect(within(card).getByText("Delivery comparison")).toBeVisible();
  expect(within(card).getByText(/version 3 · needs review/)).toBeVisible();
  await userEvent.click(within(card).getByRole("button", { name: "Source 1" }));
  expect(onSource).toHaveBeenCalledWith({ sourceId: "source-a" });
});

it("stays out of the way until a brief is saved", async () => {
  const get = vi.fn(async () => ({ brief: null })) as Api["get"];
  renderWith(get);
  await vi.waitFor(() => expect(get).toHaveBeenCalled());
  expect(screen.queryByRole("region", { name: "Current work" })).toBeNull();
});

it("labels an outdated brief and links its drafts and later work without presenting the old next step", async () => {
  renderWith(
    vi.fn(async () => ({
      brief: {
        ...brief,
        progress_current: false,
        latest_work_run_id: "later-run",
      },
    })) as Api["get"],
  );
  expect(await screen.findByText(/Later work was saved/)).toBeVisible();
  expect(screen.queryByText("Next:")).not.toBeInTheDocument();
  expect(
    screen.getByRole("link", { name: "View later work and results" }),
  ).toHaveAttribute(
    "href",
    "#/tenders/tender-a/work?view=run&record=later-run",
  );
  await userEvent.click(screen.getByText("More options"));
  expect(
    screen.getByRole("link", { name: "Delivery comparison" }),
  ).toHaveAttribute(
    "href",
    "#/tenders/tender-a/manager?view=work-product&record=product-a",
  );
  expect(screen.getByText(brief.next_step)).toBeVisible();
});

it("keeps a failed read visible instead of hiding the work", async () => {
  renderWith(
    vi.fn(async () => {
      throw new Error("Service temporarily unavailable");
    }) as Api["get"],
  );
  // The shared resource hook retries once before reporting the failure.
  expect(
    await screen.findByText(
      "Service temporarily unavailable",
      {},
      { timeout: 4000 },
    ),
  ).toBeVisible();
});
