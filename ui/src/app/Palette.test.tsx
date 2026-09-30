import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Staff } from "../office/queries";
import { fakeService, openApp } from "../test/app";

const school = { id: "t1", name: "Al Noor Primary School", due_date: "2026-10-14", created_at: "2026-09-23T08:00:00Z" };
const warehouse = { id: "t9", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" };
const member = (id: string, name: string, role: string, is_manager = false, status = "active"): Staff => ({
  id, name, role, is_manager, status, now: null, profile: {},
});
const ready = { office_mode: "engineer" as const, office_ai: { connection_id: "c1", model: "m" }, tender_allowance: null, notifications: "all" as const };

async function search(path = "/tenders/t1") {
  const router = openApp(path);
  await screen.findByRole("heading", { level: 1 });
  await userEvent.keyboard("{Control>}k{/Control}");
  const dialog = screen.getByRole("dialog", { name: "Search or jump to" });
  return { router, dialog, box: within(dialog).getByRole("textbox", { name: "Search or jump to" }) };
}
const chosen = (dialog: HTMLElement) => within(dialog).getByRole("option", { selected: true });

describe("Search or jump to (Ctrl+K)", () => {
  it("moves through the results with the arrows and opens the one chosen with Enter", async () => {
    fakeService({ tenders: [school, warehouse] });
    const { router, dialog, box } = await search();

    expect(box).toHaveFocus();
    expect(chosen(dialog)).toHaveTextContent("OverviewCtrl 1");
    await userEvent.keyboard("{ArrowUp}");
    expect(chosen(dialog)).toHaveTextContent("Overview"); // already at the top
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{ArrowUp}");
    expect(chosen(dialog)).toHaveTextContent("TakeoffCtrl 3");
    await userEvent.keyboard("{Enter}");

    expect(router.state.location.pathname).toBe("/tenders/t1/takeoff");
    expect(screen.queryByRole("dialog", { name: "Search or jump to" })).not.toBeInTheDocument();
  });

  it("stops at the last result", async () => {
    fakeService({ tenders: [school] });
    const { dialog, box } = await search();

    await userEvent.type(box, "sett");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(chosen(dialog)).toHaveTextContent("Settings");
  });

  it("finds by every word typed, in any order, and says when nothing matches", async () => {
    fakeService({ tenders: [school, warehouse] });
    const { dialog, box } = await search();

    await userEvent.type(box, "ctrl 5");
    expect(within(dialog).getAllByRole("option").map((o) => o.textContent)).toEqual(["SubcontractCtrl 5"]);
    await userEvent.clear(box);
    await userEvent.type(box, "warehouse riyadh");
    expect(within(dialog).getAllByRole("option").map((o) => o.textContent)).toEqual(["Riyadh Warehouseno due date"]);

    await userEvent.clear(box);
    await userEvent.type(box, "bill of lading");
    expect(within(dialog).getByText("Nothing matches “bill of lading”.")).toBeInTheDocument();
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("dialog", { name: "Search or jump to" })).toBeInTheDocument(); // nothing to open
  });

  it("opens another tender where the engineer left it", async () => {
    localStorage.setItem("quantix.place", JSON.stringify({ last: "t1", screens: { t9: "/tenders/t9/estimate" } }));
    fakeService({ tenders: [school, warehouse] });
    const { router, dialog } = await search();

    expect(within(dialog).queryByRole("option", { name: /Al Noor Primary School/ })).not.toBeInTheDocument(); // the one open
    await userEvent.click(within(dialog).getByRole("option", { name: /Riyadh Warehouse/ }));
    expect(router.state.location.pathname).toBe("/tenders/t9/estimate");
  });

  it("messages anyone on the team, not those released", async () => {
    fakeService({
      tenders: [school],
      settings: ready,
      staff: [member("s1", "Rania Farouk", "Tender Manager", true), member("s2", "Omar Haddad", "Quantity Surveyor"),
        member("s3", "Nora Al-Otaibi", "Insurance Adviser", false, "released")],
    });
    const { dialog, box } = await search();

    await userEvent.type(box, "message");
    expect(await within(dialog).findByRole("option", { name: /^Message Omar/ })).toBeInTheDocument();
    expect(within(dialog).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Message RaniaTender Manager",
      "Message OmarQuantity Surveyor",
    ]);
    await userEvent.click(within(dialog).getByRole("option", { name: /Message Omar/ }));

    const panel = await screen.findByRole("complementary", { name: "Team" });
    expect(within(panel).getByRole("heading", { name: "Omar Haddad" })).toBeInTheDocument();
  });

  it("stops the office only while the team works", async () => {
    const service = fakeService({ tenders: [school], settings: ready, staff: [member("s1", "Rania Farouk", "Tender Manager", true)], officeState: "working" });
    const { dialog, box } = await search();

    await userEvent.type(box, "stop");
    await userEvent.click(await within(dialog).findByRole("option", { name: "Stop the office" }));
    await waitFor(() => expect(service.state.officeState).toBe("paused"));

    await userEvent.keyboard("{Control>}k{/Control}");
    await userEvent.type(screen.getByRole("textbox", { name: "Search or jump to" }), "stop");
    expect(await screen.findByText("Nothing matches “stop”.")).toBeInTheDocument();
  });

  it("offers the firm's screens and the tenders on a firm screen, and a pointer chooses too", async () => {
    fakeService({ tenders: [school, warehouse] });
    const { router, dialog } = await search("/library");

    const options = within(dialog).getAllByRole("option").map((o) => o.textContent);
    expect(options).not.toContain("OverviewCtrl 1");
    expect(options.slice(0, 3)).toEqual(["Al Noor Primary Schooldue 14 Oct", "Riyadh Warehouseno due date", "Company library"]);
    await userEvent.hover(within(dialog).getByRole("option", { name: "Directory" }));
    expect(chosen(dialog)).toHaveTextContent("Directory");
    await userEvent.keyboard("{Enter}");
    expect(router.state.location.pathname).toBe("/directory");
  });

  it("lists the keyboard shortcuts from its actions", async () => {
    fakeService({ tenders: [school] });
    const { box } = await search();

    await userEvent.type(box, "shortcuts{Enter}");
    expect(screen.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeInTheDocument();
  });

  it("closes with Esc or a click beside it, but not a click in it", async () => {
    fakeService({ tenders: [school] });
    const { box } = await search();

    await userEvent.click(box);
    expect(screen.getByRole("dialog", { name: "Search or jump to" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Search or jump to" })).not.toBeInTheDocument();

    await userEvent.keyboard("{Control>}k{/Control}");
    await userEvent.click(screen.getByRole("dialog", { name: "Search or jump to" }).parentElement!);
    expect(screen.queryByRole("dialog", { name: "Search or jump to" })).not.toBeInTheDocument();
  });
});
