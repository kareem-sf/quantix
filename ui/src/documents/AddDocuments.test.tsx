import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { open } from "@tauri-apps/plugin-dialog";
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

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };

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

  it("adds the files picked, and nothing when the picker is closed", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/tenders/t1/documents");
    const picked = vi.mocked(open as () => Promise<string[] | null>);

    picked.mockResolvedValueOnce(null);
    await userEvent.click(await screen.findByRole("button", { name: "Add files" }));
    expect(open).toHaveBeenLastCalledWith({ multiple: true, title: "Add files" });
    expect(screen.getByRole("button", { name: "Add files" })).toBeEnabled();
    expect(service.state.imported).toEqual([]);

    picked.mockResolvedValueOnce(["C:\\Tenders\\ITT.pdf", "C:\\Tenders\\BOQ.xlsx"]);
    await userEvent.click(screen.getByRole("button", { name: "Add files" }));
    await waitFor(() =>
      expect(service.state.imported).toEqual([{ folder: null, files: ["C:\\Tenders\\ITT.pdf", "C:\\Tenders\\BOQ.xlsx"] }]),
    );
  });

  it("says why a folder couldn't be read", async () => {
    fakeService({ tenders: [tender], fail: { "/tenders/t1/documents/import": "Quantix can’t open that folder: access is denied." } });
    openApp("/tenders/t1/documents");

    await userEvent.click(await screen.findByRole("button", { name: "Choose folder" }));
    expect(await screen.findByText("Quantix can’t open that folder: access is denied.")).toBeInTheDocument();
  });
});
