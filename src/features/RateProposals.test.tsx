import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { RateProposals } from "./RateProposals";

const item: Schema<"EstimateItem"> = {
  id: "row-one",
  tender_id: "one",
  artifact_id: "boq-file",
  source_id: "boq-source",
  document: "Civil BOQ.xlsx",
  sheet: "Concrete",
  locator: "A2:E2",
  description: "Structural concrete",
  unit: "m3",
  unit_cell: "D2",
  quantity_cell: "E2",
  quantity_candidates: { E2: "12" },
  supplied_quantity: "12",
  effective_quantity: "10",
  quantity_basis: "approved_measurement",
  confirmed: false,
  issues: [],
  unit_rate: "100.00",
  rate_ex_vat: "100.00",
  components: [],
  currency: "EGP",
  tax_basis: "excluding_vat",
  vat_percent: null,
  provenance: null,
  line_ex_vat: "1000.00",
  line_inc_vat: null,
  quantity_proposals: [
    {
      id: "measurement-one",
      item_id: "row-one",
      quantity: "10",
      calculation: "Measured net concrete volume",
      source_ids: ["boq-source"],
      status: "approved",
      created_at: "2026-09-06T10:00:00Z",
      origin: "engineer",
    },
  ],
};
const proposal: Schema<"RateProposalRecord"> = {
  id: "proposal-one",
  tender_id: "one",
  item_id: "row-one",
  payload: {
    unit_rate: "123.450000",
    components: null,
    currency: "EGP",
    tax_basis: "unknown",
    vat_percent: null,
    provenance: {
      basis: "estimated",
      observed_on: "2026-09-01",
      source_ids: ["price-source"],
      urls: ["https://supplier.example/concrete"],
      geography: "Cairo",
      conditions: "Supply only; delivery excluded.",
    },
  },
  basis: {
    item,
    source_hash: "a".repeat(64),
    approved_measurements: [
      {
        id: "measurement-one",
        data: {
          quantity: "10",
          calculation: "Measured net concrete volume",
          source_ids: ["boq-source"],
        },
      },
    ],
    linked_source_versions: [],
  },
  basis_fingerprint: "b".repeat(64),
  approved_basis_fingerprint: null,
  source_ids: ["boq-source", "price-source"],
  run_id: "run-one",
  status: "proposed",
  is_current: true,
  created_at: "2026-09-06T10:00:00Z",
};

function setup(
  options: {
    proposal?: Partial<Schema<"RateProposalRecord">>;
    item?: Partial<Schema<"EstimateItem">>;
    noItem?: boolean;
    error?: string;
    hold?: boolean;
  } = {},
) {
  let saved = { ...proposal, ...options.proposal };
  const writes: { path: string; body: Record<string, unknown> }[] = [];
  let release: () => void = () => {};
  const held = new Promise<void>((resolve) => {
    release = resolve;
  });
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      const path = String(url).replace("http://localhost/api", "");
      if (init?.method === "POST") {
        writes.push({ path, body: JSON.parse(String(init.body)) });
        if (options.hold) await held;
        if (options.error)
          return new Response(JSON.stringify({ detail: options.error }), {
            status: 400,
          });
        saved = {
          ...saved,
          status: "approved",
          approved_basis_fingerprint: "c".repeat(64),
        };
        return new Response(JSON.stringify(saved));
      }
      if (path.includes("/evidence/"))
        return new Response(
          JSON.stringify({
            id: path.split("/").at(-1),
            artifact_name: "BOQ source",
            locator: "A2:E2",
            relative_path: "Civil BOQ.xlsx",
          }),
        );
      return new Response(JSON.stringify([saved]));
    },
  );
  const onSource = vi.fn();
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <RateProposals
          tenderId="one"
          items={options.noItem ? [] : [{ ...item, ...options.item }]}
          onSource={onSource}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { user: userEvent.setup({ delay: null }), writes, onSource, release };
}
async function review(user: ReturnType<typeof userEvent.setup>) {
  await user.click(
    await screen.findByRole("button", { name: /Review rate proposal:/ }),
  );
}
async function confirm(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByLabelText("Rate approval note"));
  await user.paste("Use this supply rate for the approved scope.");
  await user.click(screen.getByLabelText(/I have reviewed this rate/));
}

it("shows exact proposed price, provenance, VAT uncertainty and separate quantities before approval", async () => {
  const { user, writes } = setup();
  await review(user);
  const reviewPanel = screen.getByRole("region", {
    name: "Review proposed rate",
  });
  expect(
    within(reviewPanel).getByText("123.450000 EGP / m3"),
  ).toBeInTheDocument();
  expect(within(reviewPanel).getByText("100.00 EGP / m3")).toBeInTheDocument();
  expect(screen.getByText("2026-09-01")).toBeInTheDocument();
  expect(screen.getByText("Cairo")).toBeInTheDocument();
  expect(
    screen.getByText("Supply only; delivery excluded."),
  ).toBeInTheDocument();
  expect(
    screen.getByRole("link", { name: "https://supplier.example/concrete" }),
  ).toHaveAttribute("href", "https://supplier.example/concrete");
  expect(
    screen.getByText(/VAT treatment is not established/),
  ).toBeInTheDocument();
  expect(
    screen.getByText("Supplied BOQ quantity").nextElementSibling,
  ).toHaveTextContent("12 m3");
  expect(
    screen.getByText("Approved measured quantity").nextElementSibling,
  ).toHaveTextContent("10 m3");
  expect(
    screen.getByRole("button", { name: "Approve proposed rate" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
});

it("approves only the stored proposal without implicitly confirming sources or changing quantities", async () => {
  const { user, writes } = setup();
  await review(user);
  expect(screen.getByText(/will replace the current rate/)).toBeInTheDocument();
  await confirm(user);
  await user.click(
    screen.getByRole("button", { name: "Approve proposed rate" }),
  );
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toEqual({
    path: "/tenders/one/estimate/rate-proposals/proposal-one/approve",
    body: {
      engineer_confirmed: true,
      rationale: "Use this supply rate for the approved scope.",
      confirm_source: false,
    },
  });
  expect(await screen.findByText(/Rate approval recorded/)).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Approve proposed rate" }),
  ).not.toBeInTheDocument();
});

it("confirms a source row only when its separate checkbox is explicitly selected", async () => {
  const { user, writes } = setup();
  await review(user);
  const sourceConfirmation = screen.getByLabelText(
    /I have checked the source row/,
  );
  expect(sourceConfirmation).not.toBeChecked();
  await confirm(user);
  await user.click(sourceConfirmation);
  await user.click(
    screen.getByRole("button", { name: "Approve proposed rate" }),
  );
  await waitFor(() => expect(writes[0]?.body.confirm_source).toBe(true));
});

it.each([
  { proposal: { is_current: false }, noItem: false },
  { proposal: { status: "approved" as const }, noItem: false },
  { proposal: {}, noItem: true },
])(
  "blocks approval of stale, decided or unavailable items: %j",
  async (options) => {
    const { user, writes } = setup(options);
    await review(user);
    expect(
      screen.queryByRole("button", { name: "Approve proposed rate" }),
    ).not.toBeInTheDocument();
    expect(writes).toHaveLength(0);
  },
);

it("blocks a proposal when a newer current rate differs from its saved basis", async () => {
  const { user, writes } = setup({ item: { unit_rate: "140.00" } });
  await review(user);
  expect(
    screen.getByText(/The current BOQ item differs from the proposal basis/),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Approve proposed rate" }),
  ).not.toBeInTheDocument();
  expect(writes).toHaveLength(0);
});

it("preserves the decision and backend refusal without retrying or changing the installed rate", async () => {
  const { user, writes } = setup({
    error: "The item basis changed. Review the current rate.",
  });
  await review(user);
  await confirm(user);
  await user.click(
    screen.getByRole("button", { name: "Approve proposed rate" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The item basis changed",
  );
  expect(screen.getByLabelText("Rate approval note")).toHaveValue(
    "Use this supply rate for the approved scope.",
  );
  expect(
    screen.getByRole("button", { name: "Approve proposed rate" }),
  ).toBeDisabled();
  expect(screen.getByText("100.00 EGP / m3")).toBeInTheDocument();
  expect(writes).toHaveLength(1);
});

it("disables duplicate approval while the original request is in flight", async () => {
  const { user, writes, release } = setup({ hold: true });
  await review(user);
  await confirm(user);
  await user.click(
    screen.getByRole("button", { name: "Approve proposed rate" }),
  );
  expect(
    screen.getByRole("button", { name: "Recording rate approval…" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(1);
  release();
  expect(await screen.findByText(/Rate approval recorded/)).toBeInTheDocument();
});

it("shows component arithmetic exactly without floating point rounding", async () => {
  const { user } = setup({
    proposal: {
      payload: {
        ...proposal.payload,
        unit_rate: null,
        components: [
          {
            name: "Concrete supply",
            quantity: "0.1",
            unit_rate: "0.2",
            unit: "m3",
          },
          {
            name: "Placement",
            quantity: "2.5",
            unit_rate: "100.000001",
            unit: "hour",
          },
        ],
      },
    },
  });
  await review(user);
  expect(
    screen.getByRole("table", { name: "Proposed rate build-up" }),
  ).toBeInTheDocument();
  expect(screen.getByText("250.0200025 EGP / m3")).toBeInTheDocument();
  expect(screen.getByText("0.02")).toBeInTheDocument();
  expect(screen.getByText("250.0000025")).toBeInTheDocument();
});

it("does not offer source confirmation when the row is already confirmed", async () => {
  const confirmedItem = { ...item, confirmed: true };
  const { user } = setup({
    item: confirmedItem,
    proposal: { basis: { ...proposal.basis, item: confirmedItem } },
  });
  await review(user);
  expect(
    screen.queryByLabelText(/I have checked the source row/),
  ).not.toBeInTheDocument();
});

it("retains the proposed unit when the current BOQ row changes unit", async () => {
  const { user } = setup({ item: { unit: "m2" } });
  await review(user);
  expect(screen.getByText("123.450000 EGP / m3")).toBeInTheDocument();
  expect(screen.getByText("100.00 EGP / m2")).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Approve proposed rate" }),
  ).not.toBeInTheDocument();
});

it("keeps source confirmation unavailable until a supplied quantity cell is resolved", async () => {
  const unresolved = {
    ...item,
    supplied_quantity: null,
    effective_quantity: null,
    quantity_cell: null,
  };
  const { user, writes } = setup({
    item: unresolved,
    proposal: { basis: { ...proposal.basis, item: unresolved } },
  });
  await review(user);
  expect(screen.getByLabelText(/I have checked the source row/)).toBeDisabled();
  await confirm(user);
  await user.click(
    screen.getByRole("button", { name: "Approve proposed rate" }),
  );
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0].body.confirm_source).toBe(false);
  expect(writes[0].body).not.toHaveProperty("quantity_cell");
});
