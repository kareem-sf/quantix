import { useState } from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, useLocation, useNavigate } from "react-router-dom";
import { ApiContext, createApi } from "../api";
import { ThemeProvider } from "../theme";
import {
  SidebarProvider,
  SidebarTrigger,
  useSidebar,
} from "../components/ui/sidebar";
import { AppSidebar } from "./AppSidebar";

function SidebarJourney() {
  const location = useLocation(),
    navigate = useNavigate();
  const [action, setAction] = useState("");
  const { state } = useSidebar();
  return (
    <>
      <AppSidebar
        tenders={[
          {
            id: "one",
            name: "Synthetic Tender",
            revision: 1,
            status: "active",
            created_at: "",
            updated_at: "",
            name_source: "engineer",
          },
        ]}
        tendersPending={false}
        tendersFailed={false}
        capabilities={[]}
        onNavigate={(target) => {
          setAction("");
          void navigate(target);
        }}
        onNewTender={() => setAction("New tender selected")}
      />
      <main>
        <SidebarTrigger aria-label="Open navigation" />
        <p>{action || location.pathname}</p>
        <output aria-label="Desktop sidebar preference">{state}</output>
      </main>
    </>
  );
}

it("closes the mobile Sheet after navigation and New tender without changing desktop collapse preference", async () => {
  const previousWidth = window.innerWidth;
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    value: 736,
  });
  try {
    const api = createApi(
      { base_url: "http://localhost/api", token: "test" },
      async () => Response.json({ findings: [], plan: null, active_runs: [] }),
    );
    render(
      <QueryClientProvider client={new QueryClient()}>
        <ApiContext.Provider value={api}>
          <ThemeProvider>
            <MemoryRouter initialEntries={["/tenders/one/manager"]}>
              <SidebarProvider defaultOpen={false}>
                <SidebarJourney />
              </SidebarProvider>
            </MemoryRouter>
          </ThemeProvider>
        </ApiContext.Provider>
      </QueryClientProvider>,
    );
    const user = userEvent.setup();
    for (const [label, result] of [
      ["Documents", "/tenders/one/documents"],
      ["New tender", "New tender selected"],
    ]) {
      await user.click(screen.getByRole("button", { name: "Open navigation" }));
      const sidebar = await screen.findByRole("dialog", { name: "Sidebar" });
      await user.click(within(sidebar).getByRole("button", { name: label }));
      await waitFor(() =>
        expect(
          screen.queryByRole("dialog", { name: "Sidebar" }),
        ).not.toBeInTheDocument(),
      );
      expect(screen.getByText(result)).toBeVisible();
      expect(
        screen.getByLabelText("Desktop sidebar preference"),
      ).toHaveTextContent("collapsed");
    }
  } finally {
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: previousWidth,
    });
  }
});

function LocationProbe() {
  const location = useLocation();
  return (
    <output aria-label="Location">{`${location.pathname}${location.search}`}</output>
  );
}

it("lists settings sections in the sidebar while Settings is open", async () => {
  const api = createApi(
    { base_url: "http://localhost/api", token: "test" },
    async () => Response.json({}),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ApiContext.Provider value={api}>
        <ThemeProvider>
          <MemoryRouter
            initialEntries={[
              "/settings?section=accounts&return=%2Ftenders%2Fone%2Fmanager",
            ]}
          >
            <SidebarProvider>
              <AppSidebar
                tenders={[]}
                tendersPending={false}
                tendersFailed={false}
                capabilities={["knowledge"]}
                onNavigate={() => undefined}
                onNewTender={() => undefined}
              />
              <LocationProbe />
            </SidebarProvider>
          </MemoryRouter>
        </ThemeProvider>
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
  const nav = screen.getByRole("navigation", { name: "Settings sections" });
  expect(
    within(nav).getByRole("button", { name: "AI accounts" }),
  ).toHaveAttribute("aria-current", "page");
  expect(
    within(nav).getByRole("button", { name: "Approved knowledge" }),
  ).toBeVisible();
  expect(
    within(nav).queryByRole("button", { name: "Mail" }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "New tender" }),
  ).not.toBeInTheDocument();
  expect(screen.queryByText("Tenders")).not.toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Back to the tender" }),
  ).toBeVisible();
});
