import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Settings } from "./Settings";

it("treats configured credentials as untested and clears a saved secret from the form", async () => {
  const user = userEvent.setup();
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (_url, init) => {
      if (init?.method === "PATCH")
        expect(JSON.parse(String(init.body)).api_key).toBe("private-key");
      return new Response(
        JSON.stringify({
          provider_ready: true,
          model: "gpt-6-astra",
          default_currency: "EGP",
          home: "C:/local",
          provider_detail: "",
          preferences: "",
        }),
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
        <Settings />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  expect(
    await screen.findByText("Configured · connection not tested"),
  ).toBeInTheDocument();
  const key = screen.getByLabelText("API key");
  await user.type(screen.getByLabelText("Model"), "another-model");
  expect(screen.getByLabelText("Model")).toHaveValue("gpt-6-astra");
  await user.type(key, "private-key");
  await user.click(screen.getByRole("button", { name: "Save settings" }));
  await waitFor(() => expect(key).toHaveValue(""));
  expect(await screen.findByRole("status")).toHaveTextContent("Settings saved");
  expect(Object.values(window.localStorage).join(" ")).not.toContain(
    "private-key",
  );
});

it("opens the reusable-note approval form from Settings without submitting connection settings", async () => {
  const writes: string[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      const path = new URL(String(address)).pathname;
      if (init?.method !== "GET") writes.push(path);
      return new Response(
        JSON.stringify(
          path.endsWith("/health")
            ? { capabilities: ["knowledge"] }
            : path.endsWith("/settings")
              ? {
                  provider_ready: false,
                  model: "gpt-6-astra",
                  default_currency: "EGP",
                  home: "C:/local",
                  preferences: "",
                }
              : [],
        ),
      );
    },
  );
  const user = userEvent.setup();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <Settings />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  await user.click(
    await screen.findByRole("button", { name: "New reusable note" }),
  );
  const dialog = screen.getByRole("dialog", { name: "Approve reusable note" });
  expect(
    within(dialog).getByRole("button", { name: "Approve and save note" }),
  ).toBeDisabled();
  expect(within(dialog).getByLabelText("Supporting tender")).toHaveValue("");
  expect(dialog.querySelector("form form")).toBeNull();
  expect(writes).toHaveLength(0);
});
