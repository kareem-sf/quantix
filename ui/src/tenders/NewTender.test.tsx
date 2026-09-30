import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

describe("Starting a tender", () => {
  it("won't start a tender without a name", async () => {
    fakeService();
    openApp("/new");

    const start = screen.getByRole("button", { name: "Start tender" });
    expect(start).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Tender name"), "   ");
    expect(start).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Tender name"), "Clinic");
    expect(start).toBeEnabled();
  });

  it("starts a tender with no submission date yet", async () => {
    const service = fakeService();
    const router = openApp("/new");

    await userEvent.type(screen.getByLabelText("Tender name"), "Riyadh Warehouse");
    await userEvent.click(screen.getByRole("button", { name: "Start tender" }));
    await waitFor(() => expect(router.state.location.pathname).toBe("/tenders/t1"));
    expect(service.state.tenders[0]).toMatchObject({ name: "Riyadh Warehouse", due_date: null });
    expect(await screen.findByText(/No due date yet\./)).toBeInTheDocument();
  });
});
