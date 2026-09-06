import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { Knowledge } from "./Knowledge";

const note: Schema<"KnowledgeRecord"> = {
  id: "note-one",
  title: "Check source quantities",
  content: "Check the supplied BOQ against its stated scope.",
  category: "method",
  source_tender_id: null,
  source_ids: [],
  verified_on: null,
  recheck_after: null,
  source_tender_name: null,
  sources: [],
  status: "approved",
  approved_at: "2026-09-06T10:00:00Z",
  approval_rationale: "Approved working method",
  withdrawn_at: null,
  withdrawal_rationale: null,
  sources_current: null,
  needs_recheck: false,
  commercial_revalidation_required: false,
  revalidation_reasons: [],
  use_limitations: "Review this guidance for each new tender.",
  audit: [],
};
const tender = (id: string) => ({
  id,
  name: id === "source-a" ? "Source tender A" : "Source tender B",
  status: "active",
  revision: 1,
  created_at: "",
  updated_at: "",
});
const evidence = (id: string) => ({
  id: `${id}-evidence`,
  artifact_id: `${id}-file`,
  artifact_name: `${id}.docx`,
  relative_path: `Notes/${id}.docx`,
  locator: "paragraph:2",
  text: `Scope note from ${id}`,
  kind: "text",
  metadata: {},
  score: 0,
  page: null,
  sheet: null,
  cell_range: null,
});
const artifact = (id: string) => ({
  id: `${id}-file`,
  tender_id: id,
  relative_path: `Notes/${id}.docx`,
  name: `${id}.docx`,
  version: 1,
  content_hash: "hash",
  size: 100,
  kind: "word",
  status: "extracted",
  area: "",
  metadata: {},
  warnings: [],
  is_current: true,
  created_at: "",
});

function setup(
  options: {
    records?: Schema<"KnowledgeRecord">[];
    createError?: string;
    withdrawError?: boolean;
  } = {},
) {
  let records = options.records ?? [];
  let withdrawalAttempts = 0;
  const writes: { path: string; body: Record<string, unknown> }[] = [],
    reads: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      const url = new URL(String(address)),
        path = url.pathname.replace("/api", "");
      reads.push(path + url.search);
      if (init?.method === "POST") {
        const body = JSON.parse(String(init.body));
        writes.push({ path, body });
        if (path === "/knowledge" && options.createError)
          return new Response(JSON.stringify({ detail: options.createError }), {
            status: 409,
          });
        if (path.endsWith("/withdraw")) {
          if (options.withdrawError && withdrawalAttempts++ === 0)
            return new Response(
              JSON.stringify({
                detail: "The withdrawal could not be recorded.",
              }),
              { status: 409 },
            );
          records = records.map((record) => ({
            ...record,
            status: "withdrawn",
            withdrawn_at: "2026-09-06T12:00:00Z",
            withdrawal_rationale: String(body.rationale),
            needs_recheck: true,
            revalidation_reasons: ["withdrawn"],
          }));
          return new Response(JSON.stringify(records[0]));
        }
        const created = {
          ...note,
          ...body,
          id: "new-note",
          status: "approved",
        } as Schema<"KnowledgeRecord">;
        records.push(created);
        return new Response(JSON.stringify(created));
      }
      if (path === "/tenders")
        return new Response(
          JSON.stringify([tender("source-a"), tender("source-b")]),
        );
      if (path.startsWith("/tenders/")) {
        const id = path.split("/")[2];
        return new Response(
          JSON.stringify(
            path.endsWith("/search")
              ? [evidence(id)]
              : path.endsWith("/artifacts")
                ? [artifact(id)]
                : evidence(id),
          ),
        );
      }
      if (path === "/knowledge")
        return new Response(
          JSON.stringify(
            records.filter(
              (record) =>
                (url.searchParams.get("include_withdrawn") === "true" ||
                  record.status === "approved") &&
                (!url.searchParams.has("category") ||
                  record.category === url.searchParams.get("category")),
            ),
          ),
        );
      return new Response(
        JSON.stringify(
          records.find((record) => path === `/knowledge/${record.id}`),
        ),
      );
    },
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <Knowledge />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { user: userEvent.setup({ delay: null }), writes, reads };
}

async function fill(
  user: ReturnType<typeof userEvent.setup>,
  name: string,
  text: string,
) {
  const input = screen.getByLabelText(name, { exact: true });
  await user.clear(input);
  await user.click(input);
  await user.paste(text);
}
async function createForm(user: ReturnType<typeof userEvent.setup>) {
  await user.click(
    await screen.findByRole("button", { name: "New reusable note" }),
  );
  await fill(user, "Title", "Reusable method");
  await fill(user, "Note", "Check the supplied scope for this tender.");
  await fill(user, "Approval note", "Approved for reuse as guidance");
}

it("requires an explicit approval and retains the note when saving fails", async () => {
  const { user, writes } = setup({
    createError: "The approved note could not be saved.",
  });
  await createForm(user);
  expect(
    screen.getByRole("button", { name: "Approve and save note" }),
  ).toBeDisabled();
  await user.click(
    screen.getByRole("checkbox", { name: "I approve this note for reuse." }),
  );
  await user.click(
    screen.getByRole("button", { name: "Approve and save note" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The approved note could not be saved.",
  );
  expect(screen.getByLabelText("Note", { exact: true })).toHaveValue(
    "Check the supplied scope for this tender.",
  );
  expect(writes[0].body).toMatchObject({
    engineer_confirmed: true,
    rationale: "Approved for reuse as guidance",
  });
});

it("keeps supporting sources in their selected tender and source buttons do not submit", async () => {
  const { user, writes, reads } = setup();
  await createForm(user);
  await user.click(
    screen.getByRole("checkbox", { name: "I approve this note for reuse." }),
  );
  await user.selectOptions(
    screen.getByLabelText("Supporting tender"),
    "source-a",
  );
  await fill(user, "Supporting sources", "scope");
  await user.click(
    await screen.findByRole("button", { name: /source-a.docx · paragraph:2/ }),
  );
  await user.click(
    await screen.findByRole("button", {
      name: "source-a.docx · paragraph:2",
    }),
  );
  const sourceDialog = await screen.findByRole("dialog", {
    name: "source-a.docx",
  });
  expect(
    await within(sourceDialog).findByText("Scope note from source-a"),
  ).toBeInTheDocument();
  expect(writes).toHaveLength(0);
  expect(
    reads.some((path) =>
      path.startsWith("/tenders/source-a/artifacts?include_history=true"),
    ),
  ).toBe(true);
  await user.keyboard("{Escape}");
  await user.selectOptions(
    screen.getByLabelText("Supporting tender"),
    "source-b",
  );
  await fill(user, "Supporting sources", "scope");
  await user.click(
    await screen.findByRole("button", { name: /source-b.docx · paragraph:2/ }),
  );
  await user.click(
    screen.getByRole("button", { name: "Approve and save note" }),
  );
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0].body).toMatchObject({
    source_tender_id: "source-b",
    source_ids: ["source-b-evidence"],
    engineer_confirmed: true,
  });
});

it("filters categories and keeps fresh commercial verification separate from approval", async () => {
  const price = {
    ...note,
    id: "price-note",
    title: "Earlier price reference",
    category: "price",
    verified_on: "2026-09-01",
    needs_recheck: true,
    commercial_revalidation_required: true,
    revalidation_reasons: [
      "commercial_use_requires_fresh_validation",
      "source_revision_changed",
    ],
  } as Schema<"KnowledgeRecord">;
  const { user, reads } = setup({ records: [note, price] });
  await user.selectOptions(
    screen.getByLabelText("Filter note category"),
    "price",
  );
  await user.click(
    await screen.findByRole("button", {
      name: "Earlier price reference",
    }),
  );
  const dialog = await screen.findByRole("dialog", {
    name: "Earlier price reference",
  });
  expect(
    within(dialog).getByText(
      "Prices and tax rules require current verification before commercial use.",
    ),
  ).toBeInTheDocument();
  expect(
    within(dialog).getByText("A supporting file has a newer version."),
  ).toBeInTheDocument();
  expect(within(dialog).getByText("2026-09-01")).toBeInTheDocument();
  expect(reads.some((path) => path.includes("category=price"))).toBe(true);
});

it("keeps an approved note unchanged until withdrawal is accepted", async () => {
  const { user, writes } = setup({ records: [note], withdrawError: true });
  await user.click(await screen.findByRole("button", { name: note.title }));
  await user.click(
    await screen.findByRole("button", { name: "Withdraw note" }),
  );
  await fill(
    user,
    "Reason for withdrawal",
    "Method replaced by revised guidance",
  );
  await user.click(
    screen.getByRole("checkbox", {
      name: "I confirm this note should no longer be reused.",
    }),
  );
  await user.click(screen.getByRole("button", { name: "Confirm withdrawal" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The withdrawal could not be recorded.",
  );
  await user.click(screen.getByRole("button", { name: "Confirm withdrawal" }));
  await screen.findByText("Method replaced by revised guidance");
  expect(screen.getByText(note.content, { exact: true })).toBeInTheDocument();
  expect(writes[1].body).toMatchObject({
    engineer_confirmed: true,
    rationale: "Method replaced by revised guidance",
  });
});
