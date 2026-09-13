import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { AdvancedAIConnections } from "./AIAdvancedConnections";

it("keeps removal behind confirmation and preserves the account on a failed delete", async () => {
  const user = userEvent.setup();
  let deletes = 0;
  const account = {
    id: "synthetic-account",
    name: "Synthetic account",
    provider_id: "openai",
    protocol: "openai_responses",
    billing: "metered",
    credential_state: "missing",
    enabled: true,
    status: "configured",
    last_error: null,
  };
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (url, init) => {
      if (init?.method === "DELETE") {
        deletes++;
        return Response.json(
          { detail: "Synthetic removal failed. Try again." },
          { status: 409 },
        );
      }
      return Response.json(
        String(url).endsWith("/ai/connections") ? [account] : [],
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
        <AdvancedAIConnections />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(
    await screen.findByRole("button", { name: "Remove connection" }),
  );
  let dialog = within(
    screen.getByRole("dialog", { name: "Remove AI connection" }),
  );
  expect(
    dialog.getByRole("button", { name: "Remove connection" }),
  ).toBeDisabled();
  expect(deletes).toBe(0);
  await user.click(dialog.getByRole("button", { name: "Cancel" }));
  expect(deletes).toBe(0);
  await waitFor(() =>
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
  );
  await user.click(screen.getByRole("button", { name: "Remove connection" }));
  dialog = within(screen.getByRole("dialog"));
  await user.click(
    dialog.getByRole("checkbox", { name: "I want to remove this connection." }),
  );
  await user.click(dialog.getByRole("button", { name: "Remove connection" }));
  await waitFor(() => expect(deletes).toBe(1));
  expect(
    await dialog.findByText("Synthetic removal failed. Try again."),
  ).toBeInTheDocument();
  expect(
    dialog.getByRole("button", { name: "Remove connection" }),
  ).toBeEnabled();
});
