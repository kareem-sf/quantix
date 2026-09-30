import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const menu = () => screen.queryByRole("menu", { name: "Tender actions" });

describe("The tender's menu", () => {
  it("records that the tender was submitted, and the Overview says so", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/tenders/t1");

    await userEvent.click(await screen.findByRole("button", { name: "Tender actions" }));
    expect(screen.getByRole("menuitemradio", { name: "Open" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(screen.getByRole("menuitemradio", { name: "Submitted" }));
    await waitFor(() => expect(service.state.tenders[0].outcome).toBe("submitted"));
    expect(await screen.findByText("Submitted.")).toBeInTheDocument();
    expect(menu()).not.toBeInTheDocument();
  });

  it("restores an archived tender", async () => {
    const service = fakeService({ tenders: [{ ...tender, archived: true }] });
    openApp("/tenders/t1");

    expect(await screen.findByText("Synthetic school · archived")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Tender actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Restore from the archive" }));
    await waitFor(() => expect(service.state.tenders[0].archived).toBe(false));
    expect(await within(screen.getByRole("main")).findByText("Synthetic school")).toBeInTheDocument();
  });

  it("closes on Esc or a click elsewhere, and forgets a delete not confirmed", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/tenders/t1");

    await userEvent.click(await screen.findByRole("button", { name: "Tender actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete this tender" }));
    expect(screen.getByText(/and everything Quantix keeps for it/)).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(menu()).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Tender actions" }));
    expect(screen.getByRole("menuitem", { name: "Delete this tender" })).toBeInTheDocument(); // asked again, not half-way
    await userEvent.click(screen.getByRole("heading", { level: 1 }));
    expect(menu()).not.toBeInTheDocument();
    expect(service.state.tenders).toHaveLength(1);
  });

  it("says why the tender couldn't be changed or deleted", async () => {
    const service = fakeService({ tenders: [tender], fail: { "/tenders/t1": "The tender is being exported." } });
    openApp("/tenders/t1");

    await userEvent.click(await screen.findByRole("button", { name: "Tender actions" }));
    await userEvent.click(screen.getByRole("menuitemradio", { name: "Won" }));
    expect(await screen.findByText("The tender is being exported.")).toBeInTheDocument();
    expect(menu()).toBeInTheDocument();

    await userEvent.click(screen.getByRole("menuitem", { name: "Delete this tender" }));
    await userEvent.click(screen.getByRole("button", { name: "Delete tender" }));
    await waitFor(() => expect(screen.getAllByText("The tender is being exported.")).toHaveLength(1));
    expect(service.state.tenders).toHaveLength(1);
  });
});
