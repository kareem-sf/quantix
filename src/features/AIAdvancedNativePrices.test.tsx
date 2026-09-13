import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ApiContext, createApi, type Schema } from "../api";
import { ConnectionModels } from "./AIAdvancedConnections";

it("saves the separate documented hosted-code rate from More options", async () => {
  const account = { id: "account", name: "Synthetic", enabled: true } as Schema<"ConnectionRecord">;
  const model = { model_id: "exact", display_name: "Exact model", source: "provider", capabilities: { tools: true },
    pricing: { input_per_million: 1, output_per_million: 2, source: "Original token reference", as_of: "2026-09-12" } };
  const saved: Array<Record<string, unknown>> = [];
  const api = createApi({ base_url: "http://localhost/api", token: "fixture" }, async (_url, options) => {
    if (options?.method === "POST") {
      const value = JSON.parse(String(options.body));
      saved.push(value);
      return Response.json(value);
    }
    return Response.json([model]);
  });
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <ApiContext.Provider value={api}><ConnectionModels connection={account} onClose={vi.fn()} /></ApiContext.Provider>
  </QueryClientProvider>);
  fireEvent.click(await screen.findByRole("button", { name: "Edit capabilities and prices" }));
  fireEvent.click(screen.getByText("More options"));
  fireEvent.change(screen.getByLabelText("Hosted code per 20-minute session (USD)"), { target: { value: "0.05" } });
  fireEvent.change(screen.getByLabelText("Hosted-code pricing page"), { target: { value: "https://developers.openai.com/api/docs/pricing" } });
  fireEvent.change(screen.getByLabelText("Hosted-code price checked on"), { target: { value: "2026-09-12" } });
  fireEvent.click(screen.getByRole("button", { name: "Save model record" }));
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(saved[0].pricing).toMatchObject({ input_per_million: 1, output_per_million: 2,
    code_execution_per_session: 0.05, code_execution_source: "https://developers.openai.com/api/docs/pricing", code_execution_as_of: "2026-09-12" });
});
