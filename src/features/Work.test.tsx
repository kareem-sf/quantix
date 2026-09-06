import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Work } from "./Work";

it("lets an engineer start a ready task from an approved plan and shows start failures", async () => {
  const task = {
    id: "task",
    tender_id: "one",
    plan_id: "plan",
    title: "Review scope",
    description: "Check scope inclusions",
    role: "scope_review",
    status: "ready",
    source_ids: [],
    result: {},
    run_id: null,
    created_at: "",
    updated_at: "",
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "POST")
        return new Response(
          JSON.stringify({ detail: "Configure the AI connection first." }),
          { status: 400 },
        );
      return new Response(
        JSON.stringify(
          String(url).endsWith("/plans")
            ? [
                {
                  id: "plan",
                  title: "Scope review",
                  version: 1,
                  status: "approved",
                  tasks: [task],
                },
              ]
            : String(url).endsWith("/tasks")
              ? [task]
              : [],
        ),
      );
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Work tenderId="one" onChanges={() => {}} onSource={() => {}} />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const summary = await screen.findByText("Check scope inclusions", {
    selector: "details > p",
  });
  await user.click(summary.parentElement!.querySelector("summary")!);
  await user.click(screen.getByRole("button", { name: "Run task" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Configure the AI connection first.",
  );
});
