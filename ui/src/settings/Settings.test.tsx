import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";

describe("Settings", () => {
  it("adds a connection, checks a model and gives it to the office", async () => {
    const service = fakeService();
    openApp("/settings");

    expect(await screen.findByText("Add an AI connection so the office can start work.")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("API key"), "sk-ant-secret-9f2c");
    await userEvent.click(screen.getByRole("button", { name: "Add connection" }));

    expect(await screen.findByText("Key …9f2c")).toBeInTheDocument();
    expect(screen.getByLabelText("API key")).toHaveValue("");

    await userEvent.selectOptions(await screen.findByRole("combobox", { name: "Model" }), "model-a");
    await userEvent.click(screen.getByRole("button", { name: "Check" }));
    expect(await screen.findByText("Works, including the tools the office needs.")).toBeInTheDocument();

    await userEvent.selectOptions(await screen.findByLabelText("Office AI"), "model-a · Anthropic");
    await waitFor(() => expect(service.state.settings.office_ai).toEqual({ connection_id: "c1", model: "model-a" }));
  });

  it("asks for the address of an OpenAI-compatible service", async () => {
    fakeService();
    openApp("/settings");

    await userEvent.click(await screen.findByRole("radio", { name: "OpenAI-compatible service" }));
    expect(screen.getByLabelText("Service address")).toBeInTheDocument();
  });

  it("shows why a key was refused", async () => {
    fakeService({ fail: { "/ai/connections": "The key was refused. Check it and try again." } });
    openApp("/settings");

    await userEvent.type(await screen.findByLabelText("API key"), "wrong");
    await userEvent.click(screen.getByRole("button", { name: "Add connection" }));

    expect(await screen.findByText("The key was refused. Check it and try again.")).toBeInTheDocument();
  });

  it("shows the key being typed when asked", async () => {
    fakeService();
    openApp("/settings");

    const key = await screen.findByLabelText("API key");
    expect(key).toHaveAttribute("type", "password");
    await userEvent.click(screen.getByRole("button", { name: "Show the key" }));
    expect(key).toHaveAttribute("type", "text");
    await userEvent.click(screen.getByRole("button", { name: "Hide the key" }));
    expect(key).toHaveAttribute("type", "password");
  });

  it("says so plainly when the Quantix service isn't running", async () => {
    const service = fakeService();
    openApp("/settings");
    await screen.findByLabelText("API key");

    service.fetch.mockRejectedValue(new TypeError("Failed to fetch"));
    await userEvent.type(screen.getByLabelText("API key"), "sk-test");
    await userEvent.click(screen.getByRole("button", { name: "Add connection" }));
    expect(
      await screen.findByText("The Quantix service isn’t answering. Make sure Quantix is running, then try again."),
    ).toBeInTheDocument();
  });

  it("keeps a free web research key, and forgets it", async () => {
    const service = fakeService();
    openApp("/settings");

    await userEvent.type(await screen.findByLabelText("TinyFish key"), "tf-key-1234");
    await userEvent.click(screen.getByRole("button", { name: "Save the TinyFish key" }));
    expect(await screen.findByText("Key …1234")).toBeInTheDocument();
    expect(service.state.webKeys).toEqual({ firecrawl: null, tinyfish: "…1234" });

    await userEvent.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(service.state.webKeys.tinyfish).toBeNull());
    expect(await screen.findByLabelText("TinyFish key")).toBeInTheDocument();
  });

  it("shows why a web research key was refused", async () => {
    fakeService({ fail: { "/web/keys/firecrawl": "The key was refused." } });
    openApp("/settings");

    await userEvent.type(await screen.findByLabelText("Firecrawl key"), "wrong");
    await userEvent.click(screen.getByRole("button", { name: "Save the Firecrawl key" }));
    expect(await screen.findByText("The key was refused.")).toBeInTheDocument();
  });

  it("switches the office to fully autonomous", async () => {
    const service = fakeService();
    openApp("/settings");

    await userEvent.click(await screen.findByRole("radio", { name: /Fully autonomous/ }));
    await waitFor(() => expect(service.state.settings.office_mode).toBe("autonomous"));
  });
});

describe("The office's AI use", () => {
  it("shows how each AI has done and caps what one tender may use", async () => {
    const service = fakeService({
      usage: {
        models: [
          { model: "gpt-5-4-mini", turns: 50, finished: 45, calls: 400, calls_sent_back: 32, accepted: 26, sent_back: 29,
            tokens: 2_600_000 },
        ],
        tenders: [{ tender_id: "t1", name: "Substation earthworks", tokens: 3_400_000 }],
      },
    });
    openApp("/settings");

    const row = await screen.findByRole("row", { name: /gpt-5-4-mini/ });
    const cells = within(row).getAllByRole("cell").map((c) => c.textContent);
    expect(cells).toEqual(["gpt-5-4-mini", "45 of 50", "8%", "26 of 55", "100K"]);
    expect(screen.getByText("3.4M used")).toBeInTheDocument();

    const allowance = screen.getByLabelText("Allowance in million tokens");
    await userEvent.type(allowance, "lots");
    expect(screen.getByText("Give the allowance as a number of millions, such as 20 or 2.5.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    await userEvent.clear(allowance);
    await userEvent.type(allowance, "2.5");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(service.state.settings.tender_allowance).toBe(2_500_000));
    expect(await screen.findByText("3.4M used of 2.5M")).toBeInTheDocument();
  });

  it("says when no AI has worked yet", async () => {
    fakeService();
    openApp("/settings");
    expect(await screen.findByText("Nothing yet. Each turn the office works is counted here.")).toBeInTheDocument();
  });
});
