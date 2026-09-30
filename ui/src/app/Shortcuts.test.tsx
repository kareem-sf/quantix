import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

const school = { id: "t1", name: "Al Noor Primary School", due_date: null, created_at: "2026-09-23T08:00:00Z" };

describe("Keyboard shortcuts (Ctrl+/)", () => {
  it("lists every shortcut by where it works, the screens in the order a tender is worked", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    await screen.findByRole("heading", { name: "Nothing needs you right now" });
    await userEvent.keyboard("{Control>}[Slash]{/Control}");

    const sheet = screen.getByRole("dialog", { name: "Keyboard shortcuts" });
    expect(within(sheet).getAllByRole("region").map((r) => r.getAttribute("aria-label"))).toEqual([
      "Anywhere", "Search (Ctrl K)", "Documents", "Takeoff",
    ]);
    const anywhere = within(sheet).getByRole("region", { name: "Anywhere" });
    const screens = ["Overview", "Documents", "Takeoff", "Estimate", "Subcontract", "Queries", "Submission"];
    screens.forEach((name, n) => expect(within(anywhere).getByText(`Ctrl ${n + 1}`).previousSibling).toHaveTextContent(name));
    expect(within(sheet).getByText("The shortcuts go by the key's place, so they work with an Arabic keyboard too.")).toBeInTheDocument();
  });

  it("closes on a click beside it, not in it", async () => {
    fakeService({ tenders: [school] });
    openApp("/tenders/t1");
    await screen.findByRole("heading", { name: "Nothing needs you right now" });
    await userEvent.keyboard("{Control>}[Slash]{/Control}");

    await userEvent.click(within(screen.getByRole("dialog", { name: "Keyboard shortcuts" })).getByRole("heading"));
    expect(screen.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("dialog", { name: "Keyboard shortcuts" }).parentElement!);
    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();

    await userEvent.keyboard("{Control>}[Slash]{/Control}");
    await userEvent.keyboard("{Control>}[Slash]{/Control}"); // the same keys close it
    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();
  });
});
