import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };

describe("Company rules", () => {
  it("marks the rules Quantix starts with as examples, and the engineer adjusts one", async () => {
    const service = fakeService({
      tenders: [tender],
      rules: [{ id: "r1", topic: "Rates", text: "Quotes older than 30 days need confirming.", example: true, created_at: "" }],
    });
    openApp("/rules");

    expect(await screen.findByText("Example")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Edit" }));
    const box = screen.getByRole("textbox", { name: "Edit the rule" });
    await userEvent.clear(box);
    await userEvent.type(box, "Quotes older than 14 days need confirming.");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Quotes older than 14 days need confirming.")).toBeInTheDocument();
    expect(screen.queryByText("Example")).not.toBeInTheDocument();
    expect(service.state.rules[0]).toMatchObject({ text: "Quotes older than 14 days need confirming.", example: false });
  });

  it("adds rules under their topic and removes them", async () => {
    const service = fakeService({
      tenders: [tender],
      rules: [{ id: "r1", topic: "Exclusions", text: "Always exclude dewatering below 2 m.", example: false, created_at: "" }],
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
  it("records how the tender went, and archives it, from the tender's menu", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/tenders/t1");

    await userEvent.click(await screen.findByRole("button", { name: "Tender actions" }));
    await userEvent.click(screen.getByRole("menuitemradio", { name: "Won" }));
    await waitFor(() => expect(service.state.tenders[0].outcome).toBe("won"));

    await userEvent.click(screen.getByRole("button", { name: "Tender actions" }));
    expect(screen.getByRole("menuitemradio", { name: "Won" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(screen.getByRole("menuitem", { name: "Archive tender" }));
    await waitFor(() => expect(service.state.tenders[0].archived).toBe(true));
  });
});
