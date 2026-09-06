import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, vi } from "vitest";
import App from "./App";

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

it("preserves an engineer draft while inspecting Work and returning to Manager", async () => {
  vi.stubEnv("VITE_QUANTIX_API_BASE", "http://localhost/api");
  vi.stubEnv("VITE_QUANTIX_API_TOKEN", "test");
  const tender = {
    id: "one",
    name: "Drainage works",
    status: "active",
    revision: 1,
    created_at: "",
    updated_at: "",
  };
  vi.stubGlobal("fetch", async (url: string) => {
    let data: unknown = [];
    if (url.endsWith("/tenders")) data = [tender];
    else if (url.endsWith("/tenders/one"))
      data = {
        tender,
        artifact_count: 0,
        evidence_count: 0,
        coverage: {
          registered: 0,
          extracted: 0,
          needs_attention: 0,
          unsupported: 0,
          failed: 0,
        },
        areas: [],
        findings: [],
        plan: null,
        active_runs: [],
      };
    else if (url.endsWith("/settings"))
      data = {
        provider_ready: true,
        model: "gpt-6-astra",
        home: "C:/local",
        default_currency: "EGP",
      };
    return new Response(JSON.stringify(data));
  });
  const user = userEvent.setup();
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <App />
    </QueryClientProvider>,
  );
  const input = await screen.findByRole("textbox", {
    name: "Message to Tender Manager",
  });
  await user.type(input, "Check the drainage quantities first");
  await user.click(screen.getByRole("button", { name: "Work" }));
  expect(
    await screen.findByRole("heading", { name: "Work" }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Manager" }));
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("Check the drainage quantities first");
  await user.click(screen.getByRole("button", { name: "Settings" }));
  expect(
    await screen.findByRole("heading", { name: "Settings" }),
  ).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Drainage works" }));
  expect(
    screen.getByRole("textbox", { name: "Message to Tender Manager" }),
  ).toHaveValue("Check the drainage quantities first");
});
