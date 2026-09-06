import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { NewTender } from "./TenderDialogs";

it("keeps a tender name editable after the local service rejects creation", async () => {
  const user = userEvent.setup();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () =>
      new Response(JSON.stringify({ detail: "Unable to save tender." }), {
        status: 500,
      }),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <NewTender
          onClose={() => {}}
          onCreated={() => {
            throw new Error("Must not report success");
          }}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(screen.getByLabelText("Tender name")).toHaveFocus();
  await user.type(screen.getByLabelText("Tender name"), "Drainage works");
  await user.click(screen.getByRole("button", { name: "Create tender" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Unable to save tender.",
  );
  expect(screen.getByLabelText("Tender name")).toHaveValue("Drainage works");
});
