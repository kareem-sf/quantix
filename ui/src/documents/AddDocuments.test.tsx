import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { fakeService, openApp } from "../test/app";

// the desktop app: Tauri is there, its window answers, it can be listened to and the Windows folder picker returns a
// folder
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true, invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ isMaximized: async () => false, onResized: async () => () => {} }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => "C:\Tenders\Al Noor Package") }));

describe("Adding the tender package in the desktop app", () => {
  it("picks the folder with the Windows picker and has Quantix read it where it is", async () => {
    const service = fakeService({
      tenders: [{ id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" }],
    });
    openApp("/tenders/t1/documents");

    await userEvent.click(await screen.findByRole("button", { name: "Choose folder" }));
    await waitFor(() => expect(service.state.imported).toEqual([{ folder: "C:\Tenders\Al Noor Package", files: [] }]));
    expect(await screen.findByText("1 file added.")).toBeInTheDocument();
  });
});
