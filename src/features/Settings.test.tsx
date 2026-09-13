import { render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi } from "../api";
import { Settings } from "./Settings";

function setup(
  section?: string,
  configure?: {
    connectionId?: string;
    onConnectionClose?: () => void;
    save?: (body: Record<string, unknown>) => Promise<Response>;
    capabilities?: string[];
  },
) {
  const reads: string[] = [],
    writes: Record<string, unknown>[] = [];
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async (address, init) => {
      const path = new URL(String(address)).pathname;
      if (init?.method === "PATCH") {
        const body = JSON.parse(String(init.body));
        writes.push(body);
        return configure?.save ? configure.save(body) : Response.json({});
      }
      reads.push(path);
      if (path === "/api/settings")
        return Response.json({
          default_currency: "EGP",
          preferences: "",
          home: "C:/synthetic",
        });
      if (path.endsWith("/health"))
        return Response.json({
          ai_setup_revision: 7,
          capabilities: configure?.capabilities ?? ["knowledge", "quotations"],
        });
      if (path.endsWith("/accounts/specific"))
        return Response.json({
          id: "specific",
          supported: false,
          service_title: "OpenAI",
          models: [],
          connection: {
            name: "Specific saved account",
            provider_id: "openai",
            protocol: "openai_responses",
            credential_state: "missing",
            settings: {},
            base_url: "https://api.openai.com/v1",
          },
        });
      return Response.json([]);
    },
  );
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const renderSettings = (current?: string) => (
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <Settings
          section={current}
          connectionId={configure?.connectionId}
          onConnectionClose={configure?.onConnectionClose}
        />
      </ApiContext.Provider>
    </QueryClientProvider>
  );
  const ui = render(renderSettings(section));
  return {
    reads,
    writes,
    user: userEvent.setup({ delay: null }),
    switchSection: (next: string) => ui.rerender(renderSettings(next)),
  };
}
it("keeps sections focused and opens reusable knowledge without saving account preferences", async () => {
  const { user, writes, switchSection } = setup();
  await screen.findByRole("heading", { name: "AI accounts" });
  expect(
    screen.queryByRole("button", { name: "New reusable note" }),
  ).not.toBeInTheDocument();
  // Section navigation lives in the app sidebar (see AppSidebar.test.tsx).
  switchSection("knowledge");
  await user.click(
    await screen.findByRole("button", { name: "New reusable note" }),
  );
  const dialog = screen.getByRole("dialog", { name: "Approve reusable note" });
  expect(
    within(dialog).getByRole("button", { name: "Approve and save note" }),
  ).toBeDisabled();
  expect(dialog.querySelector("form form")).toBeNull();
  expect(writes).toEqual([]);
});
it("opens the exact account requested by a repair and closes through the return callback", async () => {
  const close = vi.fn();
  const { user, reads } = setup("accounts", {
    connectionId: "specific",
    onConnectionClose: close,
  });
  const panel = await screen.findByRole("dialog", {
    name: "Specific saved account",
  });
  expect(reads).toContain("/api/ai/setup/accounts/specific");
  await user.click(within(panel).getByRole("button", { name: "Close" }));
  expect(close).toHaveBeenCalledOnce();
});
it("keeps a newer preference draft when an earlier save finishes", async () => {
  let finish!: (response: Response) => void;
  const { user, writes, switchSection } = setup("preferences", {
    save: () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  });
  const field = await screen.findByLabelText("Instructions for the office");
  await user.type(field, "Earlier note");
  await user.click(screen.getByRole("button", { name: "Save preferences" }));
  await waitFor(() => expect(writes).toHaveLength(1));
  await user.type(field, " with later detail");
  finish(Response.json({}));
  await screen.findByText("Working preferences saved.");
  switchSection("accounts");
  switchSection("preferences");
  expect(
    await screen.findByLabelText("Instructions for the office"),
  ).toHaveValue("Earlier note with later detail");
  expect(writes[0]).toEqual({
    default_currency: "EGP",
    preferences: "Earlier note",
  });
});

it("opens the reset section directly and explains desktop support without sending a reset", async () => {
  const { writes } = setup("reset");
  expect(
    await screen.findByRole("heading", { name: "Reset Quantix" }),
  ).toBeVisible();
  expect(screen.getByText(/Open the Quantix desktop app/)).toBeVisible();
  expect(writes).toEqual([]);
});
