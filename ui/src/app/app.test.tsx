import { fireEvent, screen, waitFor, within } from "@testing-library/react";
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

  it("opens on the Desk, with what needs the engineer on every open tender", async () => {
    fakeService({
      tenders: [warehouse, { ...school, id: "t2" }],
      decisions: [
        {
          id: "q1", raised_by: "s1", title: "Site support period", text: "72 working days or 4 months?",
          options: ["72 days", "4 months"], subject_kind: null, subject_id: null, sources: null,
          status: "waiting", answer: null, created_at: "2026-09-28T09:00:00Z",
        },
      ],
    });
    const router = openApp("/");

    expect(await screen.findByRole("heading", { name: "2 things need you across 2 tenders" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/desk");
    const needs = screen.getByRole("region", { name: "Needs you" });
    await waitFor(() => expect(within(needs).getAllByText("Site support period")).toHaveLength(2));
    const closing = screen.getByRole("region", { name: "Closing dates" });
    expect(within(closing).getAllByRole("link").map((l) => l.textContent)).toEqual([
      expect.stringContaining("Al Noor Primary School"), // soonest due first
      expect.stringContaining("Riyadh Warehouse"),
    ]);
    await userEvent.click(within(screen.getByRole("region", { name: "Open tenders" })).getByRole("link", { name: "Riyadh Warehouse" }));
    expect(router.state.location.pathname).toBe("/tenders/t9");
  });

  it("keeps closed and archived tenders in the register", async () => {
    fakeService({
      tenders: [
        school,
        { ...warehouse, outcome: "lost" },
        { id: "t3", name: "Jubail Housing", due_date: null, created_at: "2026-08-01T08:00:00Z", archived: true },
      ],
    });
    openApp("/tenders");

    expect(await screen.findByRole("tab", { name: "Open 1" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("link", { name: "Al Noor Primary School" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Lost 1" }));
    expect(screen.getByRole("link", { name: "Riyadh Warehouse" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Archived 1" }));
    expect(screen.getByRole("link", { name: "Jubail Housing" })).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Find a tender"), "noor");
    expect(screen.getByText("None.")).toBeInTheDocument();
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

  it("keeps its shortcuts on an Arabic keyboard layout", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    await screen.findByRole("heading", { name: "Nothing needs you right now" });

    fireEvent.keyDown(document, { key: "ن", code: "KeyK", ctrlKey: true }); // K on an Arabic layout
    expect(screen.getByRole("dialog", { name: "Search or jump to" })).toBeInTheDocument();
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
