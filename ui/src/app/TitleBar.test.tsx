import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { Decision, Staff } from "../office/queries";
import { fakeService, openApp, type FakeState } from "../test/app";

const school = { id: "t1", name: "Al Noor Primary School", due_date: "2026-10-14", created_at: "2026-09-23T08:00:00Z" };
const warehouse = { id: "t9", name: "Riyadh Warehouse", due_date: null, created_at: "2026-09-20T08:00:00Z" };
const rania: Staff = { id: "s1", name: "Rania Farouk", role: "Tender Manager", is_manager: true, status: "active", now: null, profile: {} };
const omar: Staff = { ...rania, id: "s2", name: "Omar Haddad", role: "Quantity Surveyor", is_manager: false };
const question: Decision = {
  id: "q1", raised_by: "s1", title: "Site support period", text: "72 working days or 4 months?", options: ["72 days"],
  subject_kind: null, subject_id: null, sources: null, status: "waiting", answer: null, created_at: "2026-09-28T09:00:00Z",
};
const bar = () => screen.getByRole("banner");

describe("The title bar", () => {
  it("says where the engineer is: the tender and its screen, or the firm's screen", async () => {
    fakeService({ tenders: [school] });
    const router = openApp("/tenders/t1/estimate");

    await waitFor(() => expect(bar()).toHaveTextContent("Al Noor Primary SchoolEstimate"));
    for (const [path, shown] of [
      ["/tenders/t1/submission", "Al Noor Primary SchoolSubmission"],
      ["/library", "Company library"],
      ["/rules", "Company rules"],
      ["/desk", "Desk"],
      ["/tenders", "Tenders"],
      ["/settings", "Settings"],
      ["/new", "New tender"],
    ]) {
      await act(() => router.navigate(path));
      expect(bar()).toHaveTextContent(shown);
      if (!path.startsWith("/tenders/")) expect(bar()).not.toHaveTextContent("Al Noor Primary School");
    }
  });

  it("goes back and forward, folds the sidebar and opens search from its buttons", async () => {
    fakeService({ tenders: [school] });
    const router = openApp("/tenders/t1");
    await screen.findByRole("heading", { name: "Nothing needs you right now" });
    await act(() => router.navigate("/tenders/t1/estimate"));

    await userEvent.click(within(bar()).getByRole("button", { name: "Back (Alt+Left)" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t1"));
    await userEvent.click(within(bar()).getByRole("button", { name: "Forward (Alt+Right)" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t1/estimate"));

    await userEvent.click(within(bar()).getByRole("button", { name: "Sidebar (Ctrl+B)" }));
    expect(within(screen.getByRole("navigation", { name: "Quantix" })).queryByText("Company")).not.toBeInTheDocument();

    await userEvent.click(within(bar()).getByRole("button", { name: /Search or jump to…/ }));
    expect(screen.getByRole("dialog", { name: "Search or jump to" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    await userEvent.click(within(bar()).getByRole("button", { name: "Search or jump to (Ctrl+K)" })); // in a narrower window
    expect(screen.getByRole("dialog", { name: "Search or jump to" })).toBeInTheDocument();
  });

  it("says in a line what the team is doing, and opens the team from it", async () => {
    const cases: [Partial<FakeState>, string][] = [
      [{ staff: [] }, "No team yet"],
      [{ staff: [rania, omar] }, "The team is idle"],
      [{ staff: [rania, omar], officeState: "working" }, "The team is working"],
      [{ staff: [rania, { ...omar, now: "Measuring the slab on A-201" }], officeState: "working" }, "Omar: Measuring the slab on A-201"],
      [{ staff: [rania, { ...omar, now: "Measuring the slab on A-201" }], officeState: "paused" }, "The team is stopped"],
    ];
    for (const [state, line] of cases) {
      fakeService({ tenders: [school], ...state });
      openApp("/tenders/t1");
      expect(await within(bar()).findByRole("button", { name: line })).toHaveAttribute("title", line);
      cleanup();
    }

    fakeService({ tenders: [school], staff: [rania] });
    openApp("/tenders/t1");
    await userEvent.click(await within(bar()).findByRole("button", { name: "The team is idle" }));
    expect(await screen.findByRole("complementary", { name: "Team" })).toBeInTheDocument();
  });

  it("counts what needs the engineer on every open tender on its bell, which opens the Desk", async () => {
    fakeService({ tenders: [school, warehouse], decisions: [question] }); // the fake's question waits on each tender
    const router = openApp("/tenders/t1");

    const bell = await within(bar()).findByRole("link", { name: "2 decisions need you" });
    expect(bell).toHaveTextContent("2");
    await userEvent.click(bell);
    expect(router.state.location.pathname).toBe("/desk");
    cleanup();

    fakeService({ tenders: [school, { ...warehouse, outcome: "lost" }, { ...warehouse, id: "t8", archived: true }], decisions: [question] });
    openApp("/desk");
    expect(await within(bar()).findByRole("link", { name: "1 decision needs you" })).toBeInTheDocument();
    cleanup();

    fakeService({ tenders: [school] });
    openApp("/desk");
    expect(await within(bar()).findByRole("link", { name: "Nothing needs you" })).not.toHaveTextContent(/\d/);
  });

  it("marks the Team button when a question waits, and shows whether the team is open", async () => {
    fakeService({ tenders: [school], staff: [rania], decisions: [question] });
    openApp("/tenders/t1");

    const team = await within(bar()).findByRole("button", { name: "Team", pressed: false });
    expect(await within(team).findByLabelText("A question waits for you")).toBeInTheDocument();
    await userEvent.click(team);
    expect(team).toHaveAttribute("aria-pressed", "true");
  });

  it("has no team or status away from a tender", async () => {
    fakeService({ tenders: [school] });
    openApp("/settings");

    await screen.findByRole("heading", { name: "Settings" });
    expect(within(bar()).queryByRole("button", { name: /team/i })).not.toBeInTheDocument();
  });
});
