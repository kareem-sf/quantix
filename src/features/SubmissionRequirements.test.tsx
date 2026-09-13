import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { SubmissionRequirements } from "./SubmissionRequirements";

it.each(["conditional", undefined])(
  "requires fresh applicability review for %s requirements and keeps rationale after a refused decision",
  async (applicability) => {
    const user = userEvent.setup({ delay: null });
    const writes: unknown[] = [];
    const requirement = {
      id: "qualified",
      title: "Alternative design",
      detail: "Provide a design",
      deliverable_kind: "technical_docx",
      origin: "manager",
      status: "proposed",
      review_status: "pending",
      is_current: true,
      review_is_current: false,
      source_ids: [],
      sources: [],
      linked_outputs: [],
      warnings: [],
      recheck_reasons: [],
      audit: [],
      applicability,
      condition: "If an alternative is offered",
      exceptions: ["Unless the original design is retained"],
      source_quote:
        "If an alternative is offered, provide a design unless the original design is retained.",
      applicability_reviewed: false,
    };
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      async (address, init) => {
        if (init?.method === "POST") {
          writes.push(JSON.parse(String(init.body)));
          return Response.json(
            { detail: "Review source revision before continuing." },
            { status: 409 },
          );
        }
        return Response.json(
          String(address).endsWith("/requirements/qualified")
            ? requirement
            : [],
        );
      },
    );
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ApiContext.Provider value={api}>
          <SubmissionRequirements
            tenderId="qualified-tender"
            selectedId="qualified"
          />
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    const panel = await screen.findByRole("dialog", {
      name: "Alternative design",
    });
    expect(within(panel).getByText(requirement.source_quote)).toBeVisible();
    expect(within(panel).getByText(requirement.condition)).toBeVisible();
    expect(within(panel).getByText(requirement.exceptions[0])).toBeVisible();
    await user.type(
      within(panel).getByLabelText("Decision rationale"),
      "Checked the applicability against the design.",
    );
    await user.click(
      within(panel).getByRole("checkbox", {
        name: /I reviewed the requirement, its sources/,
      }),
    );
    const save = within(panel).getByRole("button", {
      name: "Record engineer decision",
    });
    expect(save).toBeDisabled();
    const review = within(panel).getByRole("checkbox", {
      name: /I checked when this requirement applies/,
    });
    expect(review).not.toBeChecked();
    await user.click(review);
    await user.click(save);
    expect(
      await within(panel).findByText(
        "Review source revision before continuing.",
      ),
    ).toBeVisible();
    expect(within(panel).getByLabelText("Decision rationale")).toHaveValue(
      "Checked the applicability against the design.",
    );
    expect(writes).toEqual([
      {
        decision: "approve",
        engineer_confirmed: true,
        rationale: "Checked the applicability against the design.",
        applicability_reviewed: true,
      },
    ]);
  },
);
it("opens the exact requirement record and routes to its required document type", async () => {
  const reads: string[] = [],
    navigate = vi.fn();
  const requirement = {
    id: "specific",
    title: "Construction programme",
    detail: "Provide the stated working sequence",
    deliverable_kind: "programme_xlsx",
    origin: "engineer",
    due_date: null,
    status: "approved",
    review_status: "pending",
    is_current: true,
    review_is_current: false,
    source_ids: [],
    sources: [],
    linked_outputs: [],
    warnings: [],
    recheck_reasons: [],
    reviewed_at: null,
    audit: [],
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address) => {
      const path = new URL(String(address)).pathname;
      reads.push(path);
      return Response.json(
        path.endsWith("/requirements/specific") ? requirement : [],
      );
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <SubmissionRequirements
          tenderId="one"
          selectedId="specific"
          onNavigate={navigate}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const panel = await screen.findByRole("dialog", {
    name: "Construction programme",
  });
  expect(reads).toContain("/api/tenders/one/requirements/specific");
  const user = userEvent.setup({ delay: null });
  await user.click(
    await within(panel).findByRole("button", {
      name: "Create required document",
    }),
  );
  expect(navigate).toHaveBeenCalledWith("documents", "new:programme_xlsx");
});
