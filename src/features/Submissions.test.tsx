import { useState } from "react";
import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { Submissions } from "./Submissions";

const outputs: Schema<"OutputRecord">[] = ["one", "two"].map((id) => ({
  id,
  tender_id: "tender",
  kind: "registers_xlsx",
  filename: `${id}.xlsx`,
  status: "draft",
  created_at: "2026-09-06T12:00:00Z",
  size: 100,
  sha256: id.repeat(32),
  source_ids: [],
  pricing_complete: true,
  blocking_reasons: [],
  metadata: {},
}));
function setup(options: { blocked?: boolean; refusal?: boolean } = {}) {
  const writes: { path: string; body: Record<string, unknown> }[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      const path = new URL(String(address)).pathname;
      if (init?.method === "POST") {
        const body = JSON.parse(String(init.body));
        writes.push({ path, body });
        if (path.endsWith("/preview"))
          return Response.json({
            fingerprint: "a".repeat(64),
            requirements: [],
            requirement_ids: [],
            blockers: [],
            outputs: outputs.filter((output) =>
              body.output_ids.includes(output.id),
            ),
            blocking_reasons: options.blocked
              ? ["Selected source version has changed."]
              : [],
            warnings: [
              "Approval covers only selected files.",
              "External transmission has not occurred.",
            ],
          });
        if (options.refusal)
          return Response.json(
            { detail: "Selected files changed. Review a fresh preview." },
            { status: 409 },
          );
        return Response.json({ id: "export", filename: "approved.zip" });
      }
      return Response.json([]);
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Submissions tenderId="tender" outputs={outputs} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { user: userEvent.setup({ delay: null }), writes };
}
async function review(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("checkbox", { name: "one.xlsx" }));
  await user.click(
    screen.getByRole("button", { name: "Review selected files" }),
  );
  return screen.findByRole("dialog", { name: "Review local export" });
}
async function approveInputs(
  user: ReturnType<typeof userEvent.setup>,
  dialog: HTMLElement,
) {
  await user.type(
    within(dialog).getByLabelText("Approved scope"),
    "Registers for engineering review only",
  );
  await user.type(
    within(dialog).getByLabelText("Export approval note"),
    "Reviewed exact workbook and declared limits",
  );
  await user.click(
    within(dialog).getByRole("checkbox", {
      name: "Approval covers only selected files.",
    }),
  );
  await user.click(
    within(dialog).getByRole("checkbox", {
      name: "External transmission has not occurred.",
    }),
  );
  await user.click(
    within(dialog).getByRole("checkbox", {
      name: "I completed the final review of these exact files and approve this local export.",
    }),
  );
}
it("binds final approval to the exact preview, selected outputs, explicit scope and every displayed gap", async () => {
  const { user, writes } = setup();
  const dialog = await review(user);
  expect(
    within(dialog).getByRole("button", { name: "Approve local export" }),
  ).toBeDisabled();
  await approveInputs(user, dialog);
  await user.click(
    within(dialog).getByRole("button", { name: "Approve local export" }),
  );
  await waitFor(() => expect(writes).toHaveLength(2));
  expect(writes[1].body).toEqual({
    output_ids: ["one"],
    requirement_ids: null,
    fingerprint: "a".repeat(64),
    engineer_confirmed: true,
    final_review_confirmed: true,
    acknowledged_scope: "Registers for engineering review only",
    acknowledged_gaps: [
      "Approval covers only selected files.",
      "External transmission has not occurred.",
    ],
    rationale: "Reviewed exact workbook and declared limits",
  });
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Local export approved. No files were sent.",
  );
});
it("blocks final approval when the preview has current source or file blockers", async () => {
  const { user, writes } = setup({ blocked: true });
  const dialog = await review(user);
  expect(
    within(dialog).getByText("Selected source version has changed."),
  ).toBeInTheDocument();
  await approveInputs(user, dialog);
  expect(
    within(dialog).getByRole("button", { name: "Approve local export" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(1);
});
it("retains the engineer scope after refusal and requires a fresh preview and renewed consent", async () => {
  const { user, writes } = setup({ refusal: true });
  const dialog = await review(user);
  await approveInputs(user, dialog);
  await user.click(
    within(dialog).getByRole("button", { name: "Approve local export" }),
  );
  expect(await within(dialog).findByRole("alert")).toHaveTextContent(
    "Selected files changed. Review a fresh preview.",
  );
  expect(within(dialog).getByLabelText("Approved scope")).toHaveValue(
    "Registers for engineering review only",
  );
  expect(
    within(dialog).getByRole("button", { name: "Approve local export" }),
  ).toBeDisabled();
  await user.click(
    within(dialog).getByRole("button", { name: "Refresh export review" }),
  );
  await waitFor(() => expect(writes).toHaveLength(3));
  expect(
    within(dialog).getByRole("checkbox", {
      name: /I completed the final review/,
    }),
  ).not.toBeChecked();
  expect(
    within(dialog).getByRole("button", { name: "Approve local export" }),
  ).toBeDisabled();
});

it("returns from a typed requirement repair with a new fingerprint and retained notes but renewed consent", async () => {
  const writes: Record<string, unknown>[] = [];
  let reviews = 0;
  const repair = vi.fn();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "POST") {
        const body = JSON.parse(String(init.body));
        writes.push(body);
        if (String(url).endsWith("/preview")) {
          reviews += 1;
          return Response.json({
            fingerprint: (reviews === 1 ? "a" : "b").repeat(64),
            outputs: [outputs[0]],
            requirements: [],
            requirement_ids: ["required-one"],
            warnings: [],
            blocking_reasons:
              reviews === 1 ? ["Completion review needed."] : [],
            blockers:
              reviews === 1
                ? [
                    {
                      code: "requirement_review",
                      message: "Completion review needed.",
                      target: {
                        kind: "requirement",
                        record_id: "required-one",
                        output_ids: [],
                      },
                    },
                  ]
                : [],
          });
        }
        return Response.json({ id: "approved" });
      }
      return Response.json([]);
    },
  );
  function Journey() {
    const [away, setAway] = useState(false),
      [record, setRecord] = useState<string>();
    return away ? (
      <button onClick={() => setAway(false)}>Return to package review</button>
    ) : (
      <Submissions
        tenderId="repair-tender"
        outputs={outputs}
        recordId={record}
        onReviewChange={(open) => setRecord(open ? "review" : undefined)}
        onRepair={(path) => {
          repair(path);
          setAway(true);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Journey />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const user = userEvent.setup({ delay: null });
  const first = await review(user);
  await user.type(
    within(first).getByLabelText("Approved scope"),
    "Reviewed programme only",
  );
  await user.type(
    within(first).getByLabelText("Export approval note"),
    "Requirement now linked",
  );
  await user.click(
    within(first).getByRole("checkbox", { name: /I completed/ }),
  );
  await user.click(
    within(first).getByRole("button", { name: "Review this requirement" }),
  );
  expect(repair).toHaveBeenCalledWith(
    "/tenders/repair-tender/submission?view=requirements&record=required-one",
  );
  await user.click(
    screen.getByRole("button", { name: "Return to package review" }),
  );
  const fresh = await screen.findByRole("dialog", {
    name: "Review local export",
  });
  expect(reviews).toBe(2);
  expect(within(fresh).getByLabelText("Approved scope")).toHaveValue(
    "Reviewed programme only",
  );
  expect(
    within(fresh).getByRole("checkbox", { name: /I completed/ }),
  ).not.toBeChecked();
  expect(
    within(fresh).getByRole("button", { name: "Approve local export" }),
  ).toBeDisabled();
  await user.click(
    within(fresh).getByRole("checkbox", { name: /I completed/ }),
  );
  await user.click(
    within(fresh).getByRole("button", { name: "Approve local export" }),
  );
  await waitFor(() => expect(writes).toHaveLength(3));
  expect(writes[2].fingerprint).toBe("b".repeat(64));
});
