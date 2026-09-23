import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };

describe("Company rules", () => {
  it("adds rules under their topic and removes them", async () => {
    const service = fakeService({
      tenders: [tender],
      rules: [{ id: "r1", topic: "Exclusions", text: "Always exclude dewatering below 2 m.", created_at: "" }],
    });
    openApp("/rules");

    expect(await screen.findByText("Always exclude dewatering below 2 m.")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Topic"), "Markups");
    await userEvent.type(screen.getByLabelText("Rule"), "Overheads 5%, profit 7%.");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByText("Overheads 5%, profit 7%.")).toBeInTheDocument();
    expect(service.state.rules.map((r) => r.topic)).toEqual(["Exclusions", "Markups"]);

    await userEvent.click(screen.getAllByRole("button", { name: "Remove" })[0]);
    await waitFor(() => expect(service.state.rules.map((r) => r.topic)).toEqual(["Markups"]));
  });
});

describe("Tender outcome", () => {
  it("records how the tender went from the overview", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/tenders/t1");

    await userEvent.selectOptions(await screen.findByLabelText("Outcome"), "Won");
    await waitFor(() => expect(service.state.tenders[0].outcome).toBe("won"));
  });
});
