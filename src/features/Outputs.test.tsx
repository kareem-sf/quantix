import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Outputs } from "./Outputs";

function setup() {
  const writes: Record<string, unknown>[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      if (init?.method === "POST") {
        writes.push(JSON.parse(String(init.body)));
        return Response.json({ id: "out" });
      }
      if (new URL(String(address)).pathname.endsWith("/tasks"))
        return Response.json([
          {
            id: "complete",
            title: "Reviewed method statement",
            status: "completed",
          },
          { id: "waiting", title: "Unfinished report", status: "ready" },
        ]);
      if (new URL(String(address)).pathname.endsWith("/estimate"))
        return Response.json({ items: [], refresh_required: false });
      return Response.json([]);
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Outputs tenderId="one" view="documents" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { user: userEvent.setup({ delay: null }), writes };
}

it("requires a completed task for the selected technical document and keeps its exact task id", async () => {
  const { user, writes } = setup();
  await user.selectOptions(
    screen.getByLabelText("Document to create"),
    "technical_docx",
  );
  await user.click(
    screen.getByRole("button", { name: "Create draft document" }),
  );
  const dialog = screen.getByRole("dialog", {
    name: "Create technical document",
  });
  expect(
    within(dialog).queryByRole("option", { name: "Unfinished report" }),
  ).not.toBeInTheDocument();
  await user.selectOptions(
    await within(dialog).findByLabelText("Completed specialist task"),
    "complete",
  );
  await user.type(
    within(dialog).getByLabelText("Document review note"),
    "Prepare this completed method for review",
  );
  await user.click(within(dialog).getByRole("checkbox"));
  await user.click(
    within(dialog).getByRole("button", { name: "Create draft" }),
  );
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toEqual({
    kind: "technical_docx",
    task_id: "complete",
    engineer_confirmed: true,
    rationale: "Prepare this completed method for review",
  });
});

it("creates a programme only from explicit activities and calendar without inserting sample construction work", async () => {
  const { user, writes } = setup();
  await user.selectOptions(
    screen.getByLabelText("Document to create"),
    "programme_xlsx",
  );
  await user.click(
    screen.getByRole("button", { name: "Create draft document" }),
  );
  const dialog = screen.getByRole("dialog", {
    name: "Create construction programme",
  });
  expect(
    within(dialog).queryByLabelText("Activity title"),
  ).not.toBeInTheDocument();
  expect(
    within(dialog).getByRole("button", { name: "Create draft" }),
  ).toBeDisabled();
  await user.type(
    within(dialog).getByLabelText("Programme title"),
    "Engineer programme",
  );
  await user.type(within(dialog).getByLabelText("Start date"), "2026-09-07");
  await user.click(within(dialog).getByRole("checkbox", { name: "Monday" }));
  await user.click(
    within(dialog).getByRole("button", { name: "Add activity" }),
  );
  await user.type(within(dialog).getByLabelText("Activity ID"), "A1");
  await user.type(
    within(dialog).getByLabelText("Activity title"),
    "Survey the east boundary",
  );
  await user.type(
    within(dialog).getByLabelText("Duration in working days"),
    "2",
  );
  await user.type(
    within(dialog).getByLabelText("Document review note"),
    "Dates and activities supplied by the engineer",
  );
  await user.click(
    within(dialog).getByRole("checkbox", {
      name: "I reviewed these inputs and approve creating a draft.",
    }),
  );
  await user.click(
    within(dialog).getByRole("button", { name: "Create draft" }),
  );
  await waitFor(() => expect(writes).toHaveLength(1));
  expect(writes[0]).toMatchObject({
    kind: "programme_xlsx",
    programme: {
      title: "Engineer programme",
      start_date: "2026-09-07",
      working_week: [0],
      holidays: [],
      activities: [
        {
          id: "A1",
          title: "Survey the east boundary",
          duration_days: 2,
          predecessor_ids: [],
          source_ids: [],
          assumptions: [],
        },
      ],
      assumptions: [],
    },
  });
  expect(writes[0]).not.toHaveProperty("task_id");
});

it("keeps all seven document kinds grouped and restores programme drafts without consent", async () => {
  const { user, writes } = setup();
  const kinds = screen.getByLabelText("Document to create");
  expect(within(kinds).getAllByRole("option")).toHaveLength(7);
  await user.selectOptions(kinds, "programme_xlsx");
  await user.click(
    screen.getByRole("button", { name: "Create draft document" }),
  );
  let panel = screen.getByRole("dialog", {
    name: "Create construction programme",
  });
  await user.type(
    within(panel).getByLabelText("Programme title"),
    "North wing sequence",
  );
  await user.type(
    within(panel).getByLabelText("Document review note"),
    "Keep this draft across views",
  );
  await user.click(
    within(panel).getByRole("checkbox", { name: /I reviewed these inputs/ }),
  );
  await user.click(within(panel).getByRole("button", { name: "Cancel" }));
  await user.click(
    screen.getByRole("button", { name: "Create draft document" }),
  );
  panel = screen.getByRole("dialog", { name: "Create construction programme" });
  expect(within(panel).getByLabelText("Programme title")).toHaveValue(
    "North wing sequence",
  );
  expect(within(panel).getByLabelText("Document review note")).toHaveValue(
    "Keep this draft across views",
  );
  expect(
    within(panel).getByRole("checkbox", { name: /I reviewed these inputs/ }),
  ).not.toBeChecked();
  expect(writes).toEqual([]);
});

it("retains client workbook draft inputs but never saves mapping or quantity approval", async () => {
  const { user, writes } = setup();
  await user.selectOptions(
    screen.getByLabelText("Document to create"),
    "client_boq",
  );
  await user.click(
    screen.getByRole("button", { name: "Create draft document" }),
  );
  let panel = screen.getByRole("dialog", { name: "Create client BOQ copy" });
  await user.type(within(panel).getByLabelText("Workbook currency"), "EGP");
  await user.click(
    within(panel).getByRole("checkbox", {
      name: /I checked the workbook currency/,
    }),
  );
  await user.click(within(panel).getByRole("button", { name: "Cancel" }));
  const saved = Object.values(window.localStorage).join(" ");
  expect(saved).not.toContain("mappingReviewed");
  expect(saved).not.toContain("quantityApproved");
  await user.click(
    screen.getByRole("button", { name: "Create draft document" }),
  );
  panel = screen.getByRole("dialog", { name: "Create client BOQ copy" });
  expect(within(panel).getByLabelText("Workbook currency")).toHaveValue("EGP");
  expect(
    within(panel).getByRole("checkbox", {
      name: /I checked the workbook currency/,
    }),
  ).not.toBeChecked();
  expect(
    within(panel).getByRole("button", { name: "Create draft" }),
  ).toBeDisabled();
  expect(writes).toEqual([]);
});
