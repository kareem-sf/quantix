import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ManagerWorkspaceProjection } from "./bindings/ManagerWorkspaceProjection";

const host = vi.hoisted(() => ({
  chooseAndImportTenderPackage: vi.fn(),
  inspectManagerWorkspace: vi.fn(),
  recordEngineerWorkspaceMessage: vi.fn(),
  selectManagerWorkspaceTender: vi.fn(),
  startManagerTender: vi.fn(),
}));

vi.mock("./quantixHost", () => host);

import { ManagerWorkspace } from "./ManagerWorkspace";

const tenderId = "a".repeat(32);
const messageId = "b".repeat(32);

const projection: ManagerWorkspaceProjection = {
  catalogue: [
    {
      tender_id: tenderId,
      name: "West Campus MEP",
      revision: 1,
      phase: "intake",
      needs_engineer: true,
      available: true,
      last_activity_at: "2026-08-14T10:00:00Z",
    },
  ],
  selected_tender: {
    tender_id: tenderId,
    name: "West Campus MEP",
    revision: 1,
    phase: "intake",
    needs_engineer: true,
    available: true,
    last_activity_at: "2026-08-14T10:00:00Z",
  },
  conversation: {
    conversation_id: "c".repeat(32),
    latest_meaningful_message_id: messageId,
    messages: [
      {
        message_id: messageId,
        sequence: 1,
        author: "system",
        kind: "status",
        body: "West Campus MEP workspace is ready.",
        created_at: "2026-08-14T10:00:00Z",
      },
    ],
  },
  current_action: {
    kind: "add_tender_package",
    title: "Add the Tender Package",
    summary:
      "Give the Tender Manager the source documents to begin the review.",
    action_label: "Choose Tender Package",
    requires_engineer: true,
  },
  work: {
    needs_engineer: 0,
    working: 0,
    waiting: 0,
    done: 0,
    cancelled: 0,
    failed: 0,
  },
  files: { tender_document_count: 0, quantix_output_count: 0 },
  team: { active_agent_runs: 0, waiting_tasks: 0, needs_engineer: 0 },
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ManagerWorkspace", () => {
  it("resumes the Host-selected Tender into the minimal workspace", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace aiAvailable />);

    expect(
      await screen.findByRole("heading", { name: "West Campus MEP" }),
    ).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Tenders" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Manager" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Work" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Files" })).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: "Add the Tender Package" }),
    ).toBeTruthy();
    expect(screen.queryByText(/setup wizard/i)).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Work" }));
    expect(screen.getByRole("heading", { name: "Work" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Files" }));
    expect(screen.getByRole("heading", { name: "Files" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Hide Tenders" }));
    expect(screen.getByRole("button", { name: "Show Tenders" })).toBeTruthy();
  });

  it("keeps the Host-designated meaningful message visible after routine chatter", async () => {
    const routineMessages = Array.from({ length: 9 }, (_, index) => ({
      message_id: `${index}`.repeat(32),
      sequence: index + 2,
      author: "engineer" as const,
      kind: "routine" as const,
      body: `Routine note ${index + 1}`,
      created_at: `2026-08-14T10:${String(index + 1).padStart(2, "0")}:00Z`,
    }));
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      conversation: {
        ...projection.conversation!,
        messages: [projection.conversation!.messages[0], ...routineMessages],
      },
    });

    render(<ManagerWorkspace aiAvailable />);

    expect(
      await screen.findByText("West Campus MEP workspace is ready."),
    ).toBeTruthy();
    expect(screen.getByText("Routine note 9")).toBeTruthy();
  });

  it("records the Engineer message through the workspace Host boundary", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.recordEngineerWorkspaceMessage.mockResolvedValue({
      ...projection,
      conversation: {
        ...projection.conversation!,
        messages: [
          ...projection.conversation!.messages,
          {
            message_id: "d".repeat(32),
            sequence: 2,
            author: "engineer",
            kind: "routine",
            body: "Check the insurance exclusions first.",
            created_at: "2026-08-14T10:02:00Z",
          },
        ],
      },
    });
    render(<ManagerWorkspace aiAvailable />);
    const composer = await screen.findByRole("textbox", {
      name: "Message your Tendering Manager",
    });

    fireEvent.change(composer, {
      target: { value: "Check the insurance exclusions first." },
    });
    fireEvent.keyDown(composer, { key: "Enter" });

    await waitFor(() => {
      expect(host.recordEngineerWorkspaceMessage).toHaveBeenCalledWith(
        tenderId,
        "Check the insurance exclusions first.",
      );
    });
    expect(
      await screen.findByText("Check the insurance exclusions first."),
    ).toBeTruthy();
  });

  it("shows one primary package-led start when no Tender exists", async () => {
    const empty: ManagerWorkspaceProjection = {
      catalogue: [],
      selected_tender: null,
      conversation: null,
      current_action: {
        kind: "start_tender",
        title: "Start a Tender",
        summary:
          "Choose the Tender Package and the Tender Manager will take it from there.",
        action_label: "Choose Tender Package",
        requires_engineer: true,
      },
      work: {
        needs_engineer: 0,
        working: 0,
        waiting: 0,
        done: 0,
        cancelled: 0,
        failed: 0,
      },
      files: { tender_document_count: 0, quantix_output_count: 0 },
      team: { active_agent_runs: 0, waiting_tasks: 0, needs_engineer: 0 },
    };
    host.inspectManagerWorkspace.mockResolvedValue(empty);
    host.startManagerTender.mockResolvedValue(null);
    render(<ManagerWorkspace aiAvailable />);

    const start = await screen.findByRole("button", {
      name: "Choose Tender Package",
    });
    expect(
      screen.getAllByRole("button", { name: "Choose Tender Package" }),
    ).toHaveLength(1);
    fireEvent.click(start);

    await waitFor(() => {
      expect(host.startManagerTender).toHaveBeenCalledWith("directory");
    });
  });
});
