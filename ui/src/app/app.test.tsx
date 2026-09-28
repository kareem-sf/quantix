import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

const school = { id: "t1", name: "Al Noor Primary School", due_date: "2026-10-14", created_at: "2026-09-23T08:00:00Z" };
const warehouse = { id: "t9", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" };

describe("Quantix shell", () => {
  it("starts the first tender and opens its overview", async () => {
    fakeService();
    const router = openApp("/");

    await userEvent.type(await screen.findByLabelText("Tender name"), "Al Noor Primary School");
    await userEvent.type(screen.getByLabelText(/Submission date/), "2026-10-14");
    await userEvent.click(screen.getByRole("button", { name: "Start tender" }));

    expect(await screen.findByRole("heading", { name: "Nothing needs you right now" })).toBeInTheDocument();
    expect(within(screen.getByRole("main")).getByText("Al Noor Primary School")).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/tenders/t1");
    const sidebar = screen.getByRole("navigation", { name: "Quantix" });
    expect(sidebar).toHaveTextContent("Al Noor Primary School");
    expect(sidebar).toHaveTextContent("due 14 Oct");
  });

  it("opens the newest tender from the start page", async () => {
    fakeService({
      tenders: [{ id: "t9", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" }],
    });
    openApp("/");

    expect(await screen.findByRole("heading", { name: "Nothing needs you right now" })).toBeInTheDocument();
    expect(within(screen.getByRole("main")).getByText("Riyadh Warehouse")).toBeInTheDocument();
    expect(screen.getByText("No due date yet.")).toBeInTheDocument();
  });

  it("shows the service's reason when a tender can't be created", async () => {
    fakeService({ fail: { "/tenders": "The disk is full." } });
    openApp("/new");

    await userEvent.type(screen.getByLabelText("Tender name"), "Clinic");
    await userEvent.click(screen.getByRole("button", { name: "Start tender" }));

    expect(await screen.findByText("Couldn’t create the tender: The disk is full.")).toBeInTheDocument();
  });

  it("keeps the firm's screens in the sidebar, apart from Settings", async () => {
    fakeService();
    const router = openApp("/library");
    const sidebar = screen.getByRole("navigation", { name: "Quantix" });

    for (const name of ["Company library", "Directory", "Company rules", "Company details", "Settings"]) {
      expect(within(sidebar).getByRole("link", { name })).toBeInTheDocument();
    }
    expect(within(sidebar).getByRole("link", { name: "Company library" })).toHaveAttribute("aria-current", "page");
    await userEvent.click(within(sidebar).getByRole("link", { name: "Company rules" }));
    expect(router.state.location.pathname).toBe("/rules");
  });

  it("shows one tender at a time, and switching goes back to where the engineer was in it", async () => {
    fakeService({ tenders: [school, warehouse] });
    const router = openApp("/tenders/t1/estimate");
    const sidebar = screen.getByRole("navigation", { name: "Quantix" });

    await within(sidebar).findByText("Al Noor Primary School");
    expect(sidebar).not.toHaveTextContent("Riyadh Warehouse");
    await userEvent.click(within(sidebar).getByRole("button", { name: /Switch tender/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: /Riyadh Warehouse/ }));
    expect(router.state.location.pathname).toBe("/tenders/t9");
    await within(sidebar).findByText("Riyadh Warehouse");

    await userEvent.click(within(sidebar).getByRole("button", { name: /Switch tender/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: /Al Noor Primary School/ }));
    expect(router.state.location.pathname).toBe("/tenders/t1/estimate");
  });

  it("reopens the tender and screen the engineer was last on", async () => {
    fakeService({ tenders: [school, warehouse] });
    openApp("/tenders/t9/documents");
    await screen.findByRole("heading", { name: "Documents" });
    cleanup();

    const router = openApp("/");
    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t9/documents"));
  });

  it("jumps with Ctrl+K and the keyboard, and folds the sidebar with Ctrl+B", async () => {
    fakeService({ tenders: [school, warehouse] });
    const router = openApp("/tenders/t1");
    await screen.findByRole("heading", { name: "Nothing needs you right now" });

    await userEvent.keyboard("{Control>}k{/Control}");
    const search = screen.getByRole("dialog", { name: "Search or jump to" });
    await userEvent.type(within(search).getByRole("textbox"), "estimate{Enter}");
    expect(router.state.location.pathname).toBe("/tenders/t1/estimate");
    expect(screen.queryByRole("dialog", { name: "Search or jump to" })).not.toBeInTheDocument();

    await userEvent.keyboard("{Control>}2{/Control}");
    expect(router.state.location.pathname).toBe("/tenders/t1/documents");

    const sidebar = screen.getByRole("navigation", { name: "Quantix" });
    expect(within(sidebar).getByText("Company")).toBeInTheDocument();
    await userEvent.keyboard("{Control>}b{/Control}");
    expect(within(sidebar).queryByText("Company")).not.toBeInTheDocument(); // folded to icons
    expect(within(sidebar).getByRole("link", { name: "Estimate" })).toBeInTheDocument();
  });

  it("opens and closes the team beside any screen", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1/estimate");
    await screen.findByRole("heading", { name: "Estimate" });

    expect(screen.queryByRole("complementary", { name: "Team" })).not.toBeInTheDocument();
    await userEvent.keyboard("{Control>}j{/Control}");
    const panel = await screen.findByRole("complementary", { name: "Team" });
    expect(within(panel).getByRole("tab", { name: "Team room" })).toBeInTheDocument();
    await userEvent.click(within(panel).getByRole("button", { name: "Close the team panel" }));
    expect(screen.queryByRole("complementary", { name: "Team" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Team/, pressed: false }));
    expect(await screen.findByRole("complementary", { name: "Team" })).toBeInTheDocument();
  });

  it("says so when a tender doesn't exist", async () => {
    fakeService();
    openApp("/tenders/missing");

    expect(await screen.findByText("Tender not found.")).toBeInTheDocument();
  });
});
