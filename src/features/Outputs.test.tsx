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
      return Response.json([]);
    },
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Outputs tenderId="one" />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  return { user: userEvent.setup({ delay: null }), writes };
}

it("requires a completed task for the selected technical document and keeps its exact task id", async () => {
  const { user, writes } = setup();
  await user.selectOptions(
    screen.getByLabelText("Other document type"),
    "technical_docx",
  );
  await user.click(
    screen.getByRole("button", { name: "Create selected document" }),
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
    screen.getByLabelText("Other document type"),
    "programme_xlsx",
  );
  await user.click(
    screen.getByRole("button", { name: "Create selected document" }),
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
