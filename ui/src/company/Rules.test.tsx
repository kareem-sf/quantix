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

  it("says when there are no rules yet", async () => {
    fakeService({ tenders: [tender] });
    openApp("/rules");

    expect(await screen.findByText("No rules yet. Add the first one below.")).toBeInTheDocument();
    expect(screen.queryByText(/Rules marked Example/)).not.toBeInTheDocument();
  });

  it("leaves a rule as it was when the edit is cancelled, and won't save it empty", async () => {
    const text = "Always exclude dewatering below 2 m.";
    const service = fakeService({ tenders: [tender], rules: [{ id: "r1", topic: "Exclusions", text, example: false, created_at: "" }] });
    openApp("/rules");

    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));
    await userEvent.clear(screen.getByRole("textbox", { name: "Edit the rule" }));
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByText(text)).toBeInTheDocument();
    expect(service.state.rules[0].text).toBe(text);
  });

  it("says why a rule couldn't be changed or added", async () => {
    fakeService({
      tenders: [tender],
      rules: [{ id: "r1", topic: "Exclusions", text: "Always exclude dewatering below 2 m.", example: false, created_at: "" }],
      fail: { "/rules/r1": "Rules can't be changed while a tender is exporting.", "/rules": "That rule is already kept." },
    });
    openApp("/rules");

    await userEvent.click(await screen.findByRole("button", { name: "Edit" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Edit the rule" }), " Or 3 m.");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Rules can't be changed while a tender is exporting.")).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Topic"), "Exclusions");
    await userEvent.type(screen.getByLabelText("Rule"), "Always exclude dewatering below 2 m.");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByText("That rule is already kept.")).toBeInTheDocument();
  });

  it("says why a suggested rule couldn't be kept", async () => {
    const lesson = { id: "l1", text: "Price the cubic metre.", topic: "Rates", source: "the rate for C.2.7.3", status: "tender", created_at: "" };
    fakeService({ tenders: [tender], lessons: [lesson], fail: { "/lessons/l1": "The lesson was already dropped." } });
    openApp("/rules");

    expect(await screen.findByText(/1 lesson from work that needed correcting\./)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Keep as a rule" }));
    expect(await screen.findByText("The lesson was already dropped.")).toBeInTheDocument();
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
