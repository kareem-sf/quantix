import { screen, waitFor } from "@testing-library/react";
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

    await userEvent.selectOptions(await screen.findByLabelText("Service"), "OpenAI-compatible service");
    expect(screen.getByLabelText("Service address")).toBeInTheDocument();
  });

  it("shows why a key was refused", async () => {
    fakeService({ fail: { "/ai/connections": "The key was refused. Check it and try again." } });
    openApp("/settings");

    await userEvent.type(await screen.findByLabelText("API key"), "wrong");
    await userEvent.click(screen.getByRole("button", { name: "Add connection" }));

    expect(await screen.findByText("The key was refused. Check it and try again.")).toBeInTheDocument();
  });

  it("switches the office to fully autonomous", async () => {
    const service = fakeService();
    openApp("/settings");

    await userEvent.click(await screen.findByRole("radio", { name: /Fully autonomous/ }));
    await waitFor(() => expect(service.state.settings.office_mode).toBe("autonomous"));
  });
});
