import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../../api";
import { SubmissionChecklist } from "./SubmissionChecklist";

function requirement(
  id: string,
  title: string,
  changes: Partial<Schema<"RequirementRecord">>,
) {
  return {
    id,
    tender_id: "one",
    title,
    detail: "",
    source_ids: [],
    deliverable_kind: "technical_docx",
    source_quote: "",
    applicability: "unconditional",
    condition: "",
    origin: "manager",
    run_id: null,
    created_at: "",
    status: "approved",
    sources: [],
    is_current: true,
    recheck_reasons: [],
    linked_outputs: [],
    review_status: "pending",
    review_is_current: true,
    reviewed_at: null,
    review_rationale: null,
    overdue: false,
    warnings: [],
    audit: [],
    applicability_reviewed: true,
    ...changes,
  } as unknown as Schema<"RequirementRecord">;
}

const source = {
  source_id: "s",
  artifact_id: "a",
  artifact_name: "4-تعليمات وضع الأسعار.pdf",
  relative_path: "",
  locator: "page 1",
  version: 1,
  content_hash: "",
  evidence_hash: "",
  kind: "text",
  is_current: true,
  available: true,
  recheck_reasons: [],
};
const file = {
  output_id: "o",
  filename: "Site visit certificate.docx",
  kind: "technical_docx",
  sha256: "",
  basis_fingerprint: "",
  linked_at: "",
  link_rationale: "",
  is_current: true,
  available: true,
  recheck_reasons: [],
};

it("lists what the tender asks for with its source, file and plain status", async () => {
  const user = userEvent.setup();
  const onOpen = vi.fn();
  const onBuild = vi.fn();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      Response.json([
        requirement("bond", "Bid bond (1%)", {
          review_status: "satisfied",
          sources: [source],
          linked_outputs: [{ ...file, filename: "Guarantee.pdf" }],
        }),
        requirement("visit", "Site visit certificate", {
          linked_outputs: [file],
        }),
        requirement("boq", "Priced BOQ", {}),
        requirement("arabic", "Financial offer in Arabic", {
          status: "proposed",
          sources: [source],
        }),
      ]),
  );
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <ApiContext.Provider value={api}>
        <SubmissionChecklist tenderId="one" onOpen={onOpen} onBuild={onBuild} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const checklist = await screen.findByRole("region", {
    name: "Submission checklist",
  });
  expect(within(checklist).getByText("1 of 4 ready")).toBeInTheDocument();
  const rows = within(checklist)
    .getAllByRole("button")
    .filter((row) => row.hasAttribute("data-checklist-row"));
  expect(rows[0]).toHaveTextContent("Bid bond (1%)");
  expect(rows[0]).toHaveTextContent("ready");
  expect(rows[0]).toHaveTextContent("from 4-تعليمات وضع الأسعار.pdf · page 1");
  expect(rows[1]).toHaveTextContent("drafted · review");
  expect(rows[1]).toHaveTextContent("Site visit certificate.docx");
  expect(rows[2]).toHaveTextContent("missing");
  expect(rows[3]).toHaveTextContent("waiting for your approval");
  expect(
    within(checklist).getByRole("button", { name: "Build submission package" }),
  ).toBeDisabled();
  expect(within(checklist).getByText("3 not ready yet")).toBeInTheDocument();
  await user.click(rows[2]);
  expect(onOpen).toHaveBeenCalledWith("boq");
});

it("says when a ready item is an exception handled outside Quantix", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      Response.json([
        requirement("bond", "Bid bond", { review_status: "exception" }),
        requirement("boq", "Priced BOQ", { review_status: "satisfied" }),
      ]),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <SubmissionChecklist
          tenderId="one"
          onOpen={() => undefined}
          onBuild={() => undefined}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(await screen.findByText("2 of 2 ready")).toBeInTheDocument();
  const rows = within(
    screen.getByRole("region", { name: "Submission checklist" }),
  ).getAllByRole("button");
  expect(rows[0]).toHaveTextContent("ready · exception");
  expect(rows[1]).toHaveTextContent(/ready$/);
  expect(
    screen.getByRole("button", { name: "Build submission package" }),
  ).toBeEnabled();
});
