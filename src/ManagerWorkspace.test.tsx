import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { ManagerWorkspaceProjection } from "./bindings/ManagerWorkspaceProjection";

const host = vi.hoisted(() => ({
  archiveTender: vi.fn(),
  cancelChatGptLogin: vi.fn(),
  cancelRuntimePreparation: vi.fn(),
  chooseAndImportTenderPackage: vi.fn(),
  createBidDecisionPackage: vi.fn(),
  decideBidDecisionPackage: vi.fn(),
  inspectBidDecisionApprovalHistory: vi.fn(),
  inspectBidDecisionPackageRecords: vi.fn(),
  inspectComplianceMatrix: vi.fn(),
  inspectCurrentBidDecisionPackage: vi.fn(),
  invalidateBidDecisionApproval: vi.fn(),
  resolveBidDecisionReturnRework: vi.fn(),
  runBidDecisionPackageReview: vi.fn(),
  composeTenderOffice: vi.fn(),
  inspectCurrentWorkPlan: vi.fn(),
  inspectTenderProduction: vi.fn(),
  reviseWorkPlanProposal: vi.fn(),
  decideWorkPlanProposal: vi.fn(),
  activateTenderProduction: vi.fn(),
  inspectPackageIntakeProgress: vi.fn(),
  cancelPackageIntake: vi.fn(),
  checkQuantixUpdate: vi.fn(),
  confirmAiExecutionSelection: vi.fn(),
  ensureQuantixSetup: vi.fn(),
  inspectManagerWorkspace: vi.fn(),
  inspectTenderIntegrity: vi.fn(),
  inspectTenderBackups: vi.fn(),
  inspectTenderRecoveries: vi.fn(),
  inspectDeletionReceipts: vi.fn(),
  inspectApplicationSettings: vi.fn(),
  inspectAgentRun: vi.fn(),
  inspectDiagnosticTimeline: vi.fn(),
  inspectDiagnosticsStatus: vi.fn(),
  inspectTrashedTenders: vi.fn(),
  inspectRuntimeReadiness: vi.fn(),
  inspectRuntimePreparationProgress: vi.fn(),
  disconnectChatGpt: vi.fn(),
  rebindManagerIntakeProvider: vi.fn(),
  recordEngineerWorkspaceMessage: vi.fn(),
  refreshApplicationSettings: vi.fn(),
  repairRuntimeReadiness: vi.fn(),
  reviseTender: vi.fn(),
  resumeManagerIntakes: vi.fn(),
  searchManagerWorkspace: vi.fn(),
  retryManagerIntake: vi.fn(),
  restoreArchivedTender: vi.fn(),
  restoreTrashedTender: vi.fn(),
  selectManagerWorkspaceTender: vi.fn(),
  startManagerTender: vi.fn(),
  startChatGptDeviceLogin: vi.fn(),
  startChatGptLogin: vi.fn(),
  trashTender: vi.fn(),
  purgeTrashedTender: vi.fn(),
  purgeRecoveryRequiredTender: vi.fn(),
  trashRecoveryRequiredTender: vi.fn(),
  prepareTenderRecovery: vi.fn(),
  startTenderDeepDiagnostics: vi.fn(),
  stopTenderDeepDiagnostics: vi.fn(),
  updateAiExecutionSelection: vi.fn(),
  updateTenderAiExecution: vi.fn(),
  resolveTenderRecovery: vi.fn(),
  updateGeneralApplicationPreferences: vi.fn(),
  openDiagnosticLogs: vi.fn(),
  exportDiagnosticsSupportBundle: vi.fn(),
  inspectQuantixDoctor: vi.fn(),
  repairQuantixDoctor: vi.fn(),
  validateQuantixUpdateRestart: vi.fn(),
}));

const notifications = vi.hoisted(() => ({
  enableAttentionNotifications: vi.fn(),
  notifyAttentionRequired: vi.fn(),
}));

const defaultMatchMedia = window.matchMedia;

type TestMediaQueryList = MediaQueryList & {
  setMatches(matches: boolean): void;
};

function installResponsiveMatchMedia(initialWidth: number) {
  let width = initialWidth;
  const lists = new Map<string, TestMediaQueryList>();

  const matchesQuery = (query: string) => {
    const minWidth = query.match(/^\(min-width: (\d+)px\)$/)?.[1];
    if (minWidth) return width >= Number(minWidth);
    const maxWidth = query.match(/^\(max-width: (\d+)px\)$/)?.[1];
    if (maxWidth) return width <= Number(maxWidth);
    return false;
  };

  const matchMedia = vi.fn((query: string) => {
    const eventTarget = new EventTarget();
    let matches = matchesQuery(query);
    const list = {
      get matches() {
        return matches;
      },
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: eventTarget.addEventListener.bind(eventTarget),
      removeEventListener: eventTarget.removeEventListener.bind(eventTarget),
      dispatchEvent: eventTarget.dispatchEvent.bind(eventTarget),
      setMatches(nextMatches: boolean) {
        if (matches === nextMatches) return;
        matches = nextMatches;
        const event = new Event("change") as MediaQueryListEvent;
        Object.defineProperties(event, {
          matches: { value: matches },
          media: { value: query },
        });
        eventTarget.dispatchEvent(event);
      },
    } as unknown as TestMediaQueryList;
    lists.set(query, list);
    return list;
  });

  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: matchMedia,
  });

  return {
    setWidth(nextWidth: number) {
      width = nextWidth;
      lists.forEach((list, query) => list.setMatches(matchesQuery(query)));
      fireEvent(window, new Event("resize"));
    },
  };
}

vi.mock("./quantixHost", () => host);
vi.mock("./applicationNotifications", () => notifications);

import { ManagerWorkspace } from "./ManagerWorkspace";

const tenderId = "a".repeat(32);
const messageId = "b".repeat(32);
const applicationFacts = {
  general_preferences: {
    appearance: "system" as const,
    reduced_motion: false,
    larger_text: false,
    notify_when_attention_needed: false,
  },
  ai_execution_approval: null,
  storage: {
    application_home: "A:\\Quantix-test",
    tender_backups_are_preserved: true,
    trash_requires_explicit_purge: true,
  },
  diagnostics: {
    quantix_version: "0.1.0",
    installation_schema_version: 25,
    tender_schema_version: 36,
  },
};

const projection: ManagerWorkspaceProjection = {
  catalogue: [
    {
      tender_id: tenderId,
      name: "West Campus MEP",
      revision: 1,
      phase: "intake",
      needs_engineer: true,
      state: "active" as const,
      can_archive: false,
      can_delete: false,
      last_activity_at: "2026-08-14T10:00:00Z",
    },
  ],
  selected_tender: {
    tender_id: tenderId,
    name: "West Campus MEP",
    revision: 1,
    phase: "intake",
    needs_engineer: true,
    state: "active" as const,
    can_archive: false,
    can_delete: false,
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
        references: [],
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
    tasks: [],
  },
  files: {
    tender_document_count: 0,
    quantix_output_count: 0,
    tender_documents: [],
    quantix_outputs: [],
  },
  team: {
    active_agent_runs: 0,
    waiting_tasks: 0,
    needs_engineer: 0,
    events: [],
    agent_runs: [],
  },
  intake: null,
  ai_execution: {
    revision: 1n,
    selection: null,
    readiness: "local_only",
    status_summary: "Local-only execution is available for this fixture.",
  },
  capability_readiness: {
    state: "not_planned",
    gaps: [],
    blocker_codes: [],
  },
  doctor_blockers: [],
};

const tenderAiProviderConnection = {
  connection_id: "codex_chatgpt",
  provider: "codex" as const,
  display_name: "OpenAI account via Codex",
  status: "ready" as const,
  account_label: "engineer@example.com",
  account_plan: "plus",
  models: [
    {
      model_id: "gpt-live-a",
      display_name: "Live model A",
      description: "Balanced live model",
      is_default: true,
      input_modalities: ["text"],
      reasoning_options: [
        {
          selection: { kind: "codex_effort", value: "medium" } as const,
          label: "medium",
          description: "Balanced",
          is_default: true,
        },
      ],
    },
    {
      model_id: "gpt-live-b",
      display_name: "Live model B",
      description: "Deeper live model",
      is_default: false,
      input_modalities: ["text"],
      reasoning_options: [
        {
          selection: { kind: "codex_effort", value: "high" } as const,
          label: "high",
          description: "Deeper",
          is_default: true,
        },
        {
          selection: { kind: "codex_effort", value: "xhigh" } as const,
          label: "xhigh",
          description: "Deepest",
          is_default: false,
        },
      ],
    },
  ],
  catalogue_fetched_at: "2026-08-21T08:00:00Z",
  adapter_version: "codex-v1",
  status_summary: "Ready to run Tender work.",
};

beforeEach(() => {
  host.inspectPackageIntakeProgress.mockResolvedValue(null);
  host.inspectCurrentBidDecisionPackage.mockResolvedValue(null);
  host.inspectCurrentWorkPlan.mockResolvedValue(null);
  host.inspectTenderProduction.mockResolvedValue(null);
  host.inspectBidDecisionApprovalHistory.mockResolvedValue({
    approvals: [],
    next_sequence: null,
  });
  host.inspectComplianceMatrix.mockResolvedValue({
    rows: [],
    next_ordinal: null,
  });
  host.inspectBidDecisionPackageRecords.mockResolvedValue({
    records: [],
    next_ordinal: null,
  });
  host.cancelPackageIntake.mockResolvedValue(true);
  host.ensureQuantixSetup.mockResolvedValue({ state: "ready", warnings: [] });
  host.inspectTrashedTenders.mockResolvedValue([]);
  host.inspectDeletionReceipts.mockResolvedValue([]);
  host.inspectRuntimeReadiness.mockResolvedValue({
    state: "ready",
    issues: [],
    uv_version: "0.4.0",
    ocr_version: "1.0.0",
    repair_available: false,
  });
  host.inspectApplicationSettings.mockResolvedValue({
    ...applicationFacts,
    ai_execution_selection: null,
    ai_execution_approval: null,
    provider_connections: [],
    chatgpt: {
      state: "absent",
      account_id: null,
      plan_type: null,
      expires_at_ms: null,
      login_phase: "idle",
    },
  });
  host.cancelChatGptLogin.mockResolvedValue(undefined);
  host.resumeManagerIntakes.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: defaultMatchMedia,
  });
  vi.clearAllMocks();
});

describe("ManagerWorkspace", () => {
  it("requires exact confirmation and shows a content-free permanent deletion receipt", async () => {
    const trashedRecord = {
      deletion_id: "d".repeat(32),
      tender_id: tenderId,
      tender_name: "Permanent Delete Tender",
      state: "trashed" as const,
      relative_path: `${tenderId}-${"d".repeat(32)}`,
      rationale: "Delete locally.",
      decided_by: "engineer_user",
      acting_role: "tendering_engineer",
      approval_manifest_sha256: "f".repeat(64),
      diagnostic_code: null,
      created_at: "2026-08-15T12:00:00Z",
      updated_at: "2026-08-15T12:00:00Z",
    };
    const receipt = {
      receipt_id: "e".repeat(32),
      deletion_id: trashedRecord.deletion_id,
      tender_id: tenderId,
      audit_event_count: 12n,
      audit_chain_head: "a".repeat(64),
      local_deletion_completed: true,
      erased_copy_classes: [
        "tender_store",
        "tender_backup",
        "portable_tender_archive",
        "delivery_export",
      ],
      provider_cleanup_status: "pending" as const,
      provider_thread_count: 1,
      confirmed_provider_thread_deletions: 0,
      external_copy_exclusions: [
        "original_source_packages",
        "application_provider_credentials",
      ],
      purged_by: "engineer_user",
      acting_role: "tendering_engineer",
      purged_at: "2026-08-15T12:10:00Z",
      manifest_sha256: "b".repeat(64),
    };
    let purged = false;
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.inspectTrashedTenders.mockImplementation(() =>
      Promise.resolve(purged ? [] : [trashedRecord]),
    );
    host.inspectDeletionReceipts.mockImplementation(() =>
      Promise.resolve(purged ? [receipt] : []),
    );
    host.purgeTrashedTender.mockImplementation(() => {
      purged = true;
      return Promise.resolve(receipt);
    });

    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", { name: /Archived & Trash/ }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Permanent Delete" }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Erase every Quantix-controlled copy." },
    });
    const confirm = screen.getByLabelText(
      `Type ${trashedRecord.tender_name} to confirm`,
    );
    const dialog = screen.getByRole("dialog");
    expect(
      within(dialog).getByRole("button", { name: "Permanent Delete" }),
    ).toHaveProperty("disabled", true);
    fireEvent.change(confirm, { target: { value: trashedRecord.tender_name } });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Permanent Delete" }),
    );

    await waitFor(() => {
      expect(host.purgeTrashedTender).toHaveBeenCalledWith(
        trashedRecord.deletion_id,
        "Erase every Quantix-controlled copy.",
        trashedRecord.tender_name,
      );
    });
    expect(await screen.findByText("Provider cleanup")).toBeTruthy();
    expect(screen.getByText("0/1")).toBeTruthy();
    expect(document.body.textContent).not.toContain(trashedRecord.tender_name);
  });

  it("moves a safe Tender to recoverable Trash and restores the same identity", async () => {
    const terminalTender = {
      ...projection.selected_tender!,
      name: "Terminal Trash Tender",
      phase: "declined" as const,
      needs_engineer: false,
      can_archive: true,
      can_delete: true,
    };
    const terminalProjection = {
      ...projection,
      catalogue: [terminalTender],
      selected_tender: terminalTender,
    };
    const emptyProjection = {
      ...projection,
      catalogue: [],
      selected_tender: null,
      conversation: null,
    };
    const trashedRecord = {
      deletion_id: "d".repeat(32),
      tender_id: tenderId,
      tender_name: terminalTender.name,
      state: "trashed" as const,
      relative_path: `${tenderId}-${"d".repeat(32)}`,
      rationale: "Remove from active work.",
      decided_by: "engineer_user",
      acting_role: "tendering_engineer",
      approval_manifest_sha256: "f".repeat(64),
      diagnostic_code: null,
      created_at: "2026-08-15T12:00:00Z",
      updated_at: "2026-08-15T12:00:00Z",
    };
    host.inspectManagerWorkspace
      .mockResolvedValueOnce(terminalProjection)
      .mockResolvedValue(emptyProjection);
    host.inspectTrashedTenders.mockResolvedValue([trashedRecord]);
    host.trashTender.mockResolvedValue(trashedRecord);
    host.restoreTrashedTender.mockResolvedValue({
      ...trashedRecord,
      state: "restored",
    });
    host.selectManagerWorkspaceTender.mockResolvedValue(terminalProjection);

    render(<ManagerWorkspace />);
    await screen.findByRole("heading", { name: terminalTender.name });
    fireEvent.click(screen.getByLabelText(`Manage ${terminalTender.name}`));
    fireEvent.click(screen.getByRole("menuitem", { name: "Move to Trash" }));
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Remove from active work." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Move to Trash" }));
    await waitFor(() => {
      expect(host.trashTender).toHaveBeenCalledWith(
        tenderId,
        "Remove from active work.",
      );
    });

    expect(await screen.findByText(terminalTender.name)).toBeTruthy();
    expect(screen.getByText(/Quantix never purges/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Restore" }));
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Return the same Tender." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Restore Tender" }));
    await waitFor(() => {
      expect(host.restoreTrashedTender).toHaveBeenCalledWith(
        trashedRecord.deletion_id,
        "Return the same Tender.",
      );
      expect(host.selectManagerWorkspaceTender).toHaveBeenCalledWith(tenderId);
    });
  });

  it("archives a Host-qualified Tender and restores its read-only workspace", async () => {
    const terminalTender = {
      ...projection.selected_tender!,
      name: "Terminal Archive Tender",
      phase: "declined" as const,
      needs_engineer: false,
      can_archive: true,
      can_delete: true,
    };
    const archivedTender = {
      ...terminalTender,
      state: "archived" as const,
      can_archive: false,
      can_delete: true,
    };
    const terminalProjection = {
      ...projection,
      catalogue: [terminalTender],
      selected_tender: terminalTender,
    };
    const catalogueProjection = {
      ...projection,
      catalogue: [archivedTender],
      selected_tender: null,
      conversation: null,
    };
    const archivedProjection = {
      ...projection,
      catalogue: [archivedTender],
      selected_tender: archivedTender,
    };
    host.inspectManagerWorkspace
      .mockResolvedValueOnce(terminalProjection)
      .mockResolvedValue(catalogueProjection);
    host.archiveTender.mockResolvedValue({
      decision_id: "d".repeat(32),
      tender_id: tenderId,
      state: "archived",
      rationale: "Keep terminal history.",
      decided_by: "engineer_user",
      acting_role: "tendering_engineer",
      manifest_sha256: "f".repeat(64),
      decided_at: "2026-08-15T12:00:00Z",
    });
    host.restoreArchivedTender.mockResolvedValue({
      decision_id: "e".repeat(32),
      tender_id: tenderId,
      state: "active",
      rationale: "Resume the same Tender.",
      decided_by: "engineer_user",
      acting_role: "tendering_engineer",
      manifest_sha256: "a".repeat(64),
      decided_at: "2026-08-15T12:05:00Z",
    });
    host.selectManagerWorkspaceTender
      .mockResolvedValueOnce(archivedProjection)
      .mockResolvedValueOnce(terminalProjection);

    render(<ManagerWorkspace />);
    await screen.findByRole("heading", { name: "Terminal Archive Tender" });
    fireEvent.click(screen.getByLabelText("Manage Terminal Archive Tender"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Archive" }));
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Keep terminal history." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Archive Tender" }));
    await waitFor(() => {
      expect(host.archiveTender).toHaveBeenCalledWith(
        tenderId,
        "Keep terminal history.",
      );
    });

    fireEvent.click(screen.getByRole("button", { name: /Archived & Trash/ }));
    fireEvent.click(
      await screen.findByRole("button", { name: /Terminal Archive Tender/ }),
    );
    expect(await screen.findByText("Archived · read-only")).toBeTruthy();
    expect(
      screen.queryByRole("textbox", { name: "Message your Tendering Manager" }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Choose Tender Package" }),
    ).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Restore" }));
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Resume the same Tender." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Restore Tender" }));
    await waitFor(() => {
      expect(host.restoreArchivedTender).toHaveBeenCalledWith(
        tenderId,
        "Resume the same Tender.",
      );
    });
  });

  it("resumes the Host-selected Tender into the minimal workspace", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace />);

    await screen.findByRole("heading", {
      name: "West Campus MEP",
      level: 1,
    });
    expect(
      screen.getAllByRole("heading", { name: "West Campus MEP" }),
    ).toHaveLength(2);
    expect(screen.queryByText("Tender office")).toBeNull();
    expect(
      screen
        .getByTestId("manager-workspace")
        .classList.contains("has-workspace-bar"),
    ).toBe(true);
    expect(screen.getByRole("navigation", { name: "Tenders" })).toBeTruthy();
    expect(screen.getAllByRole("main")).toHaveLength(1);
    expect(
      screen.getByRole("heading", { name: "Add the Tender Package" }),
    ).toBeTruthy();
    expect(screen.queryByText(/setup wizard/i)).toBeNull();

    const contextTrigger = screen.getByRole("button", {
      name: "Hide Tender workspace",
    });
    const context = screen.getByRole("complementary", {
      name: "Tender workspace",
    });
    expect(within(context).getByText("Current action")).toBeTruthy();
    expect(within(context).getByText("Team activity")).toBeTruthy();
    expect(within(context).getByText("Tender records")).toBeTruthy();
    expect(within(context).getByText("Capabilities")).toBeTruthy();
    expect(
      within(context).getByRole("button", { name: "Manager" }),
    ).toBeTruthy();
    expect(within(context).getByRole("button", { name: "Work" })).toBeTruthy();
    expect(within(context).getByRole("button", { name: "Files" })).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(document.activeElement).toBe(contextTrigger));
    await waitFor(() =>
      expect(
        screen.queryByRole("complementary", { name: "Tender workspace" }),
      ).toBeNull(),
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Show Tender workspace" }),
    );
    const reopenedWorkspace = await screen.findByRole("complementary", {
      name: "Tender workspace",
    });
    fireEvent.click(
      within(reopenedWorkspace).getByRole("button", { name: "Work" }),
    );
    expect(screen.getByRole("heading", { name: "Work" })).toBeTruthy();
    fireEvent.click(
      within(reopenedWorkspace).getByRole("button", { name: "Files" }),
    );
    expect(screen.getByRole("heading", { name: "Files" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("heading", { name: "Work" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Forward" }));
    expect(screen.getByRole("heading", { name: "Files" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Hide Tenders" }));
    expect(screen.getByRole("button", { name: "Show Tenders" })).toBeTruthy();
  });

  it("routes review bid decision into the focused governed panel", async () => {
    const reviewProjection: ManagerWorkspaceProjection = {
      ...projection,
      current_action: {
        kind: "review_bid_decision",
        title: "Review the Bid Decision",
        summary: "Confirm the exact bid decision package.",
        action_label: "Review Bid Decision",
        requires_engineer: true,
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(reviewProjection);

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Review Bid Decision" }),
    );

    expect(await screen.findByTestId("tender-focused-action")).toBeTruthy();
    expect(
      await screen.findByRole("heading", {
        name: "Compliance Matrix & Bid Decision Package",
      }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Back to Manager" }),
    ).toBeTruthy();
  });

  it("opens a recovery center for a recovery-required Tender without selecting it", async () => {
    const recoveryTender = {
      ...projection.catalogue[0],
      name: "Broken Tender",
      state: "recovery_required" as const,
      can_archive: false,
      can_delete: false,
    };
    const recoveryProjection: ManagerWorkspaceProjection = {
      ...projection,
      catalogue: [recoveryTender],
      selected_tender: null,
      conversation: null,
    };
    host.inspectManagerWorkspace.mockResolvedValue(recoveryProjection);
    host.inspectTenderIntegrity.mockResolvedValue({
      tender_id: tenderId,
      state: "recovery_required",
      issues: ["database_integrity_invalid"],
      recovery_choices: ["restore_verified_backup"],
    });
    host.inspectTenderBackups.mockResolvedValue([]);
    host.inspectTenderRecoveries.mockResolvedValue([]);

    render(
      <StrictMode>
        <ManagerWorkspace />
      </StrictMode>,
    );

    const tender = await screen.findByRole("button", {
      name: `Open recovery center for ${recoveryTender.name}`,
    });
    expect(tender).not.toHaveProperty("disabled", true);
    fireEvent.click(tender);

    expect(host.selectManagerWorkspaceTender).not.toHaveBeenCalled();
    expect(
      await screen.findByRole("heading", { name: /Recovery Required/i }),
    ).toBeTruthy();
    expect(host.inspectTenderIntegrity).toHaveBeenCalledWith(tenderId);
    expect(await screen.findByText("database_integrity_invalid")).toBeTruthy();
  });

  it("keeps a recovery-required Tender available after cold-start projection restore", async () => {
    const recoveryTender = {
      ...projection.catalogue[0],
      name: "Original Recovery Tender",
      state: "recovery_required" as const,
      can_archive: false,
      can_delete: false,
    };
    const recoveryProjection: ManagerWorkspaceProjection = {
      ...projection,
      catalogue: [recoveryTender],
      selected_tender: null,
      conversation: null,
    };
    host.inspectTenderIntegrity.mockResolvedValue({
      tender_id: tenderId,
      state: "recovery_required",
      issues: ["schema_mismatch"],
      recovery_choices: [],
    });
    host.inspectTenderBackups.mockResolvedValue([]);
    host.inspectTenderRecoveries.mockResolvedValue([]);

    render(<ManagerWorkspace initialProjection={recoveryProjection} />);

    const recoveryButton = await screen.findByRole("button", {
      name: `Open recovery center for ${recoveryTender.name}`,
    });
    expect(recoveryButton).toBeTruthy();
    expect(screen.queryByText("Tender workspace unavailable")).toBeNull();

    fireEvent.click(recoveryButton);

    expect(
      await screen.findByRole("heading", {
        name: `Recover ${recoveryTender.name}`,
      }),
    ).toBeTruthy();
    expect(
      await screen.findByRole("button", { name: "Move to Trash" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Delete Permanently" }),
    ).toBeTruthy();
    expect(screen.getByText("schema_mismatch")).toBeTruthy();
  });

  it("exposes recovery inspection from the Tender actions menu and returns to the prior surface", async () => {
    const recoveryTender = {
      ...projection.catalogue[0],
      name: "Broken Tender",
      state: "recovery_required" as const,
      can_archive: false,
      can_delete: false,
    };
    const recoveryProjection: ManagerWorkspaceProjection = {
      ...projection,
      catalogue: [recoveryTender],
      selected_tender: null,
      conversation: null,
    };
    host.inspectManagerWorkspace.mockResolvedValue(recoveryProjection);
    host.inspectTenderIntegrity.mockResolvedValue({
      tender_id: tenderId,
      state: "recovery_required",
      issues: ["schema_mismatch"],
      recovery_choices: ["restore_verified_backup"],
    });
    host.inspectTenderBackups.mockResolvedValue([]);
    host.inspectTenderRecoveries.mockResolvedValue([]);

    render(<ManagerWorkspace />);
    const recoveryButton = await screen.findByRole("button", {
      name: `Open recovery center for ${recoveryTender.name}`,
    });
    fireEvent.keyDown(
      recoveryButton.closest(".manager-workspace__tender-row")!,
      { key: "F10", shiftKey: true },
    );
    fireEvent.click(
      await screen.findByRole("menuitem", { name: /Inspect recovery/ }),
    );

    expect(
      await screen.findByRole("heading", { name: /Recovery Required/i }),
    ).toBeTruthy();
    expect(host.selectManagerWorkspaceTender).not.toHaveBeenCalled();
    expect(host.inspectTenderIntegrity).toHaveBeenCalledWith(tenderId);

    fireEvent.click(screen.getByRole("button", { name: /Close recovery/i }));
    await waitFor(() =>
      expect(
        screen.queryByRole("heading", { name: /Recovery Required/i }),
      ).toBeNull(),
    );
    expect(
      screen.getByRole("button", {
        name: `Open recovery center for ${recoveryTender.name}`,
      }),
    ).toBeTruthy();
  });

  it("approves a prepared recovery with rationale before opening the repaired Tender", async () => {
    const recoveryTender = {
      ...projection.catalogue[0],
      name: "Broken Tender",
      state: "recovery_required" as const,
      can_archive: false,
      can_delete: false,
    };
    const recoveryProjection: ManagerWorkspaceProjection = {
      ...projection,
      catalogue: [recoveryTender],
      selected_tender: null,
      conversation: null,
    };
    const recoveryId = "r".repeat(32);
    host.inspectManagerWorkspace.mockResolvedValue(recoveryProjection);
    host.inspectTenderIntegrity.mockResolvedValue({
      tender_id: tenderId,
      state: "recovery_required",
      issues: ["database_integrity_invalid"],
      recovery_choices: ["restore_verified_backup"],
    });
    host.inspectTenderBackups.mockResolvedValue([
      {
        backup_id: "b".repeat(32),
        tender_id: tenderId,
        state: "ready",
        source: null,
        content_object_count: 1n,
        manifest_sha256: "a".repeat(64),
        archive_size_bytes: 1n,
        diagnostic_code: null,
        created_at: "2026-08-15T12:00:00Z",
      },
    ]);
    host.inspectTenderRecoveries.mockResolvedValue([
      {
        recovery_id: recoveryId,
        tender_id: tenderId,
        backup_id: "b".repeat(32),
        state: "awaiting_approval",
        backup_source: null,
        current_source: null,
        diagnostic_code: null,
        decision_record: null,
        created_at: "2026-08-15T12:00:00Z",
      },
    ]);
    host.resolveTenderRecovery.mockResolvedValue({
      recovery_id: recoveryId,
      tender_id: tenderId,
      backup_id: "b".repeat(32),
      state: "applied",
      backup_source: null,
      current_source: null,
      diagnostic_code: null,
      decision_record: null,
      created_at: "2026-08-15T12:00:00Z",
    });
    host.selectManagerWorkspaceTender.mockResolvedValue(projection);

    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: `Open recovery center for ${recoveryTender.name}`,
      }),
    );
    await screen.findByRole("heading", { name: /Recovery Required/i });

    fireEvent.change(screen.getByLabelText(/Engineer rationale/i), {
      target: { value: "Restore the verified Tender backup." },
    });
    fireEvent.click(screen.getByRole("button", { name: /Approve/i }));

    await waitFor(() => {
      expect(host.resolveTenderRecovery).toHaveBeenCalledWith(
        tenderId,
        recoveryId,
        "approve_replacement",
        "Restore the verified Tender backup.",
      );
      expect(host.selectManagerWorkspaceTender).toHaveBeenCalledWith(tenderId);
    });
  });

  it("allows a recovery-required Tender to use recovery-specific Trash actions while keeping edit actions disabled", async () => {
    const recoveryTender = {
      ...projection.catalogue[0],
      name: "Broken Tender",
      state: "recovery_required" as const,
      can_archive: false,
      can_delete: true,
    };
    const recoveryProjection = {
      ...projection,
      catalogue: [recoveryTender],
      selected_tender: null,
      conversation: null,
    } as ManagerWorkspaceProjection;
    const trashedRecord = {
      deletion_id: "d".repeat(32),
      tender_id: tenderId,
      tender_name: recoveryTender.name,
      state: "trashed" as const,
      deletion_source: "recovery_required" as const,
      integrity_code: "schema_mismatch",
      provider_reference_discovery: "incomplete" as const,
      relative_path: `${tenderId}-${"d".repeat(32)}`,
      rationale: "Remove the unrecoverable local Store.",
      decided_by: "engineer_user",
      acting_role: "tendering_engineer",
      approval_manifest_sha256: "f".repeat(64),
      diagnostic_code: "schema_mismatch",
      created_at: "2026-08-15T12:00:00Z",
      updated_at: "2026-08-15T12:00:00Z",
    };
    host.inspectManagerWorkspace.mockResolvedValue(recoveryProjection);
    host.inspectTenderIntegrity.mockResolvedValue({
      tender_id: tenderId,
      state: "recovery_required",
      issues: ["schema_mismatch"],
      recovery_choices: ["restore_verified_backup", "purge_tender"],
    });
    host.inspectTenderBackups.mockResolvedValue([]);
    host.inspectTenderRecoveries.mockResolvedValue([]);
    host.trashRecoveryRequiredTender.mockResolvedValue(trashedRecord);

    render(<ManagerWorkspace />);
    const recoveryButton = await screen.findByRole("button", {
      name: `Open recovery center for ${recoveryTender.name}`,
    });
    fireEvent.contextMenu(
      recoveryButton.closest(".manager-workspace__tender-row")!,
    );

    const menu = await screen.findByRole("menu");
    expect(
      within(menu).getByRole("menuitem", { name: "Move to Trash" }),
    ).not.toHaveProperty("disabled", true);
    expect(
      within(menu).getByRole("menuitem", { name: /Delete Permanently/ }),
    ).not.toHaveProperty("disabled", true);
    expect(
      within(menu)
        .getByRole("menuitem", { name: /Archive/ })
        .getAttribute("aria-disabled"),
    ).toBe("true");
    expect(
      within(menu)
        .getByRole("menuitem", { name: /Rename/ })
        .getAttribute("aria-disabled"),
    ).toBe("true");

    fireEvent.click(
      within(menu).getByRole("menuitem", { name: "Move to Trash" }),
    );
    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("button", { name: "Move to Trash" }),
    ).toHaveProperty("disabled", true);
    fireEvent.change(
      within(dialog).getByLabelText(/Reason for moving to Trash/),
      {
        target: { value: "Remove the unrecoverable local Store." },
      },
    );
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Move to Trash" }),
    );
    await waitFor(() => {
      expect(host.trashRecoveryRequiredTender).toHaveBeenCalledWith(
        tenderId,
        "Remove the unrecoverable local Store.",
      );
    });
  });

  it("requires rationale and exact name before permanently deleting a recovery-required Tender", async () => {
    const recoveryTender = {
      ...projection.catalogue[0],
      name: "Broken Tender",
      state: "recovery_required" as const,
      can_archive: false,
      can_delete: true,
    };
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      catalogue: [recoveryTender],
      selected_tender: null,
      conversation: null,
    } as ManagerWorkspaceProjection);
    host.inspectTenderIntegrity.mockResolvedValue({
      tender_id: tenderId,
      state: "recovery_required",
      issues: ["schema_mismatch"],
      recovery_choices: ["purge_tender"],
    });
    host.inspectTenderBackups.mockResolvedValue([]);
    host.inspectTenderRecoveries.mockResolvedValue([]);
    host.purgeRecoveryRequiredTender.mockResolvedValue({
      receipt_id: "e".repeat(32),
      deletion_id: "d".repeat(32),
      tender_id: tenderId,
      deletion_source: "recovery_required",
      integrity_code: "schema_mismatch",
      provider_reference_discovery: "incomplete",
      audit_event_count: 1n,
      audit_chain_head: "a".repeat(64),
      local_deletion_completed: true,
      erased_copy_classes: ["tender_store"],
      provider_cleanup_status: "incomplete",
      provider_thread_count: 1,
      confirmed_provider_thread_deletions: 0,
      external_copy_exclusions: ["original_source_packages"],
      purged_by: "engineer_user",
      acting_role: "tendering_engineer",
      purged_at: "2026-08-15T12:10:00Z",
      manifest_sha256: "b".repeat(64),
    });

    render(<ManagerWorkspace />);
    const recoveryButton = await screen.findByRole("button", {
      name: `Open recovery center for ${recoveryTender.name}`,
    });
    fireEvent.keyDown(
      recoveryButton.closest(".manager-workspace__tender-row")!,
      { key: "F10", shiftKey: true },
    );
    fireEvent.click(
      await screen.findByRole("menuitem", { name: /Delete Permanently/ }),
    );
    const dialog = await screen.findByRole("dialog");
    const deleteButton = within(dialog).getByRole("button", {
      name: "Delete Permanently",
    });
    expect(deleteButton).toHaveProperty("disabled", true);
    fireEvent.change(
      within(dialog).getByLabelText(/Reason for permanent deletion/),
      {
        target: { value: "Erase the corrupted Quantix Store." },
      },
    );
    expect(deleteButton).toHaveProperty("disabled", true);
    const confirmation = within(dialog).getByLabelText(
      `Type ${recoveryTender.name} to confirm`,
    );
    fireEvent.change(confirmation, {
      target: { value: ` ${recoveryTender.name} ` },
    });
    expect(deleteButton).toHaveProperty("disabled", true);
    fireEvent.change(confirmation, { target: { value: recoveryTender.name } });
    fireEvent.click(deleteButton);
    await waitFor(() => {
      expect(host.purgeRecoveryRequiredTender).toHaveBeenCalledWith(
        tenderId,
        "Erase the corrupted Quantix Store.",
        recoveryTender.name,
      );
    });
  });

  it("restores recovery-origin Trash back to catalogue state without opening the damaged workspace and renders a safe incomplete receipt", async () => {
    const record = {
      deletion_id: "d".repeat(32),
      tender_id: tenderId,
      tender_name: "Broken Tender",
      state: "trashed" as const,
      deletion_source: "recovery_required" as const,
      integrity_code: "schema_mismatch",
      provider_reference_discovery: "incomplete" as const,
      relative_path: `${tenderId}-${"d".repeat(32)}`,
      rationale: "Remove the damaged Store from active work.",
      decided_by: "engineer_user",
      acting_role: "tendering_engineer",
      approval_manifest_sha256: "f".repeat(64),
      diagnostic_code: "schema_mismatch",
      created_at: "2026-08-15T12:00:00Z",
      updated_at: "2026-08-15T12:00:00Z",
    };
    const recoveryTender = {
      ...projection.catalogue[0],
      name: record.tender_name,
      state: "recovery_required" as const,
      can_archive: false,
      can_delete: true,
    };
    const recoveryProjection = {
      ...projection,
      catalogue: [recoveryTender],
      selected_tender: null,
      conversation: null,
    } as ManagerWorkspaceProjection;
    const receipt = {
      receipt_id: "e".repeat(32),
      deletion_id: record.deletion_id,
      tender_id: tenderId,
      deletion_source: "recovery_required" as const,
      integrity_code: "schema_mismatch",
      provider_reference_discovery: "incomplete" as const,
      audit_event_count: 1n,
      audit_chain_head: "a".repeat(64),
      local_deletion_completed: true,
      erased_copy_classes: ["tender_store"],
      provider_cleanup_status: "incomplete" as const,
      provider_thread_count: 1,
      confirmed_provider_thread_deletions: 0,
      external_copy_exclusions: ["original_source_packages"],
      purged_by: "engineer_user",
      acting_role: "tendering_engineer",
      purged_at: "2026-08-15T12:10:00Z",
      manifest_sha256: "b".repeat(64),
    };
    host.inspectManagerWorkspace.mockResolvedValue(recoveryProjection);
    host.inspectTrashedTenders.mockResolvedValue([record]);
    host.inspectDeletionReceipts.mockResolvedValue([receipt]);
    host.restoreTrashedTender.mockResolvedValue({
      ...record,
      state: "restored",
    });
    host.inspectTenderIntegrity.mockResolvedValue({
      tender_id: tenderId,
      state: "recovery_required",
      issues: ["schema_mismatch"],
      recovery_choices: ["purge_tender"],
    });
    host.inspectTenderBackups.mockResolvedValue([]);
    host.inspectTenderRecoveries.mockResolvedValue([]);

    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", { name: /Archived & Trash/ }),
    );
    expect(await screen.findByText("Recovery-required Store")).toBeTruthy();
    expect(screen.getByText("incomplete")).toBeTruthy();
    expect(document.body.textContent).not.toContain(record.relative_path);
    expect(document.body.textContent).not.toContain(record.deletion_id);
    expect(document.body.textContent).not.toContain("provider-thread");

    fireEvent.click(screen.getByRole("button", { name: "Restore" }));
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Return it for recovery inspection." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Restore Tender" }));
    await waitFor(() => {
      expect(host.restoreTrashedTender).toHaveBeenCalledWith(
        record.deletion_id,
        "Return it for recovery inspection.",
      );
    });
    expect(host.selectManagerWorkspaceTender).not.toHaveBeenCalled();
    expect(
      await screen.findByRole("heading", { name: /Recovery Required/i }),
    ).toBeTruthy();
  });

  it("uses the recovery-specific purge command for recovery-origin Trash", async () => {
    const record = {
      deletion_id: "d".repeat(32),
      tender_id: tenderId,
      tender_name: "Broken Tender",
      state: "trashed" as const,
      deletion_source: "recovery_required" as const,
      integrity_code: "schema_mismatch",
      provider_reference_discovery: "incomplete" as const,
      relative_path: `${tenderId}-${"d".repeat(32)}`,
      rationale: "Remove the damaged Store from active work.",
      decided_by: "engineer_user",
      acting_role: "tendering_engineer",
      approval_manifest_sha256: "f".repeat(64),
      diagnostic_code: null,
      created_at: "2026-08-15T12:00:00Z",
      updated_at: "2026-08-15T12:00:00Z",
    };
    const receipt = {
      receipt_id: "e".repeat(32),
      deletion_id: record.deletion_id,
      tender_id: tenderId,
      deletion_source: "recovery_required" as const,
      integrity_code: "schema_mismatch",
      provider_reference_discovery: "incomplete" as const,
      audit_event_count: 1n,
      audit_chain_head: "a".repeat(64),
      local_deletion_completed: true,
      erased_copy_classes: ["tender_store" as const],
      provider_cleanup_status: "incomplete" as const,
      provider_thread_count: 0,
      confirmed_provider_thread_deletions: 0,
      external_copy_exclusions: ["original_source_packages"],
      purged_by: "engineer_user",
      acting_role: "tendering_engineer",
      purged_at: "2026-08-15T12:10:00Z",
      manifest_sha256: "b".repeat(64),
    };
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.inspectTrashedTenders.mockResolvedValue([record]);
    host.inspectDeletionReceipts.mockResolvedValue([]);
    host.purgeRecoveryRequiredTender.mockResolvedValue(receipt);

    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", { name: /Archived & Trash/ }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Permanent Delete" }),
    );
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText("Decision rationale"), {
      target: { value: "Permanently remove the corrupted local Store." },
    });
    fireEvent.change(
      within(dialog).getByLabelText(`Type ${record.tender_name} to confirm`),
      { target: { value: record.tender_name } },
    );
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Permanent Delete" }),
    );

    await waitFor(() => {
      expect(host.purgeRecoveryRequiredTender).toHaveBeenCalledWith(
        tenderId,
        "Permanently remove the corrupted local Store.",
        record.tender_name,
      );
    });
    expect(host.purgeTrashedTender).not.toHaveBeenCalled();
  });

  it("keeps desktop Tender workspace in a separate structural rail", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace />);

    await screen.findByRole("heading", {
      name: "West Campus MEP",
      level: 1,
    });
    const workspace = screen.getByTestId("manager-workspace");
    const context = await screen.findByRole("complementary", {
      name: "Tender workspace",
    });
    const main = screen.getByRole("main");

    expect(workspace.classList.contains("has-context")).toBe(true);
    expect(
      workspace.querySelector(".manager-workspace__sidebar"),
    ).not.toBeNull();
    // Catches the production break where search remains embedded in the main conversation.
    expect(
      within(context).getByRole("searchbox", {
        name: "Search this Tender",
      }),
    ).toBeTruthy();
    // Catches the production break where the workspace view navigation is not relocated with the rail.
    for (const name of ["Manager", "Work", "Team", "Files"]) {
      expect(within(context).getByRole("button", { name })).toBeTruthy();
    }
    // Catches the production break where the rail is still nested inside the workspace main.
    expect(main.contains(context)).toBe(false);
    // Catches the production break where the Tendering Manager composer leaves the main conversation.
    expect(
      within(main).getByRole("textbox", {
        name: "Message your Tendering Manager",
      }),
    ).toBeTruthy();
    // Catches the production break where the conversation is rendered outside the main region.
    expect(
      within(main).getByText("West Campus MEP workspace is ready."),
    ).toBeTruthy();
    expect(
      context.closest(".manager-workspace__context-motion"),
    ).not.toBeNull();
    expect(
      context.closest(".manager-workspace__context-motion")?.parentElement,
    ).toBe(workspace);
    expect(within(context).getByText("Current action")).toBeTruthy();
    expect(within(context).getByText("Tender records")).toBeTruthy();
  });

  it("presents compact Tender workspace as a modal drawer and restores trigger focus", async () => {
    installResponsiveMatchMedia(760);
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace />);

    await screen.findByRole("heading", {
      name: "West Campus MEP",
      level: 1,
    });
    const sidebarToggle = await screen.findByRole("button", {
      name: "Show Tenders",
    });
    fireEvent.click(sidebarToggle);
    expect(screen.getByRole("navigation", { name: "Tenders" })).toBeTruthy();

    const contextTrigger = screen.getByRole("button", {
      name: "Show Tender workspace",
    });
    fireEvent.click(contextTrigger);
    const drawer = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    expect(drawer.getAttribute("role")).toBe("dialog");
    expect(document.querySelector('[aria-hidden="true"]')).not.toBeNull();
    expect(screen.queryByRole("navigation", { name: "Tenders" })).toBeNull();
    expect(
      screen.getByRole("button", { name: "Show Tenders", hidden: true }),
    ).toBeTruthy();

    fireEvent.keyDown(drawer, { key: "Escape" });
    await waitFor(() => expect(document.activeElement).toBe(contextTrigger));
    expect(
      screen.queryByRole("dialog", { name: "Tender workspace" }),
    ).toBeNull();

    fireEvent.click(contextTrigger);
    const reopenedDrawer = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    fireEvent.click(
      within(reopenedDrawer).getByRole("button", {
        name: "Close Tender workspace",
      }),
    );
    await waitFor(() => expect(document.activeElement).toBe(contextTrigger));
  });

  it("closes the compact Tender workspace after choosing a center view", async () => {
    installResponsiveMatchMedia(760);
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace />);

    await screen.findByRole("heading", {
      name: "West Campus MEP",
      level: 1,
    });
    const contextTrigger = screen.getByRole("button", {
      name: "Show Tender workspace",
    });
    fireEvent.click(contextTrigger);
    const drawer = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });

    fireEvent.click(within(drawer).getByRole("button", { name: "Work" }));

    expect(await screen.findByRole("heading", { name: "Work" })).toBeTruthy();
    await waitFor(() => {
      expect(
        screen.queryByRole("dialog", { name: "Tender workspace" }),
      ).toBeNull();
      expect(document.activeElement).toBe(contextTrigger);
    });
  });

  it("does not retain an open context presentation across a rail-to-drawer resize", async () => {
    const media = installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace />);
    await screen.findByRole("heading", {
      name: "West Campus MEP",
      level: 1,
    });
    expect(
      await screen.findByRole("complementary", { name: "Tender workspace" }),
    ).toBeTruthy();

    media.setWidth(760);
    await waitFor(() => {
      expect(
        screen.queryByRole("complementary", { name: "Tender workspace" }),
      ).toBeNull();
      expect(
        screen.queryByRole("dialog", { name: "Tender workspace" }),
      ).toBeNull();
    });
    expect(screen.getByRole("navigation", { name: "Tenders" })).toBeTruthy();

    const contextTrigger = screen.getByRole("button", {
      name: "Show Tender workspace",
    });
    fireEvent.click(contextTrigger);
    expect(
      await screen.findByRole("dialog", { name: "Tender workspace" }),
    ).toBeTruthy();
    expect(screen.queryByRole("navigation", { name: "Tenders" })).toBeNull();

    media.setWidth(1440);
    await waitFor(() => {
      expect(
        screen.getByRole("complementary", { name: "Tender workspace" }),
      ).toBeTruthy();
      expect(
        screen.queryByRole("dialog", { name: "Tender workspace" }),
      ).toBeNull();
    });
    expect(screen.getByRole("navigation", { name: "Tenders" })).toBeTruthy();
  });

  it("preserves the Tender draft and selected view while context opens and closes", async () => {
    installResponsiveMatchMedia(760);
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace />);
    const composer = await screen.findByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    fireEvent.change(composer, {
      target: { value: "Keep this draft while I inspect context." },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Show Tender workspace" }),
    );
    const workspaceDrawer = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    fireEvent.click(
      within(workspaceDrawer).getByRole("button", { name: "Files" }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Tender workspace" }),
      ).toBeNull(),
    );
    expect(screen.getByRole("heading", { name: "Files" })).toBeTruthy();

    const contextTrigger = screen.getByRole("button", {
      name: "Show Tender workspace",
    });
    fireEvent.click(contextTrigger);
    expect(
      await screen.findByRole("dialog", { name: "Tender workspace" }),
    ).toBeTruthy();
    const drawer = screen.getByRole("dialog", { name: "Tender workspace" });
    fireEvent.keyDown(drawer, { key: "Escape" });
    await waitFor(() => expect(document.activeElement).toBe(contextTrigger));

    fireEvent.click(
      screen.getByRole("button", { name: "Show Tender workspace" }),
    );
    const reopenedWorkspaceDrawer = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    fireEvent.click(
      within(reopenedWorkspaceDrawer).getByRole("button", { name: "Manager" }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Tender workspace" }),
      ).toBeNull(),
    );
    expect(
      (
        screen.getByRole("textbox", {
          name: "Message your Tendering Manager",
        }) as HTMLTextAreaElement
      ).value,
    ).toBe("Keep this draft while I inspect context.");
  });

  it("does not expose Tender workspace over Settings, retention, or an empty workspace", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace />);
    await screen.findByRole("heading", {
      name: "West Campus MEP",
      level: 1,
    });
    expect(
      await screen.findByRole("complementary", { name: "Tender workspace" }),
    ).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeTruthy();
    await waitFor(() =>
      expect(
        screen.queryByRole("complementary", { name: "Tender workspace" }),
      ).toBeNull(),
    );

    fireEvent.click(screen.getByRole("button", { name: /Archived & Trash/ }));
    expect(
      await screen.findByRole("heading", { name: "Archived & Trash" }),
    ).toBeTruthy();
    await waitFor(() =>
      expect(
        screen.queryByRole("complementary", { name: "Tender workspace" }),
      ).toBeNull(),
    );
  });

  it("opens application Settings and saves an advanced ChatGPT model selection atomically", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    const settings = {
      ...applicationFacts,
      ai_execution_selection: {
        connection_id: "codex_chatgpt",
        provider: "codex",
        model_id: "gpt-live-a",
        reasoning: { kind: "codex_effort", value: "medium" },
        catalogue_fetched_at: "2026-08-15T10:00:00Z",
        adapter_version: "0.147.0",
      },
      ai_execution_approval: {
        connection_id: "codex_chatgpt",
        provider: "codex",
        account_fingerprint: "account-fingerprint",
        model_id: "gpt-live-a",
        reasoning: { kind: "codex_effort", value: "medium" },
        data_destination: "OpenAI through the connected ChatGPT account",
        approved_at: "2026-08-15T10:01:00Z",
      },
      provider_connections: [
        {
          connection_id: "codex_chatgpt",
          provider: "codex",
          display_name: "OpenAI account via Codex",
          status: "ready",
          account_label: "engineer@example.com",
          account_plan: "plus",
          models: [
            {
              model_id: "gpt-live-a",
              display_name: "Live model A",
              description: "First live model",
              is_default: true,
              input_modalities: ["text"],
              reasoning_options: [
                {
                  selection: { kind: "codex_effort", value: "medium" },
                  label: "medium",
                  description: "Balanced",
                  is_default: true,
                },
              ],
            },
            {
              model_id: "gpt-live-b",
              display_name: "Live model B",
              description: "Second live model",
              is_default: false,
              input_modalities: ["text"],
              reasoning_options: [
                {
                  selection: { kind: "codex_effort", value: "high" },
                  label: "high",
                  description: "Deeper",
                  is_default: true,
                },
              ],
            },
          ],
          catalogue_fetched_at: "2026-08-15T10:00:00Z",
          adapter_version: "0.147.0",
          status_summary: "Ready to run Tender work.",
        },
      ],
      chatgpt: {
        state: "connected",
        account_id: "engineer-account",
        plan_type: "plus",
        expires_at_ms: 1_800_000_000_000n,
        login_phase: "completed",
      },
    };
    host.refreshApplicationSettings.mockResolvedValue(settings);
    host.updateAiExecutionSelection.mockResolvedValue({
      ...settings,
      ai_execution_selection: {
        ...settings.ai_execution_selection,
        model_id: "gpt-live-b",
        reasoning: { kind: "codex_effort", value: "high" },
      },
    });
    host.updateGeneralApplicationPreferences.mockImplementation(
      async ({ preferences }) => ({
        ...settings,
        general_preferences: preferences,
      }),
    );
    notifications.enableAttentionNotifications.mockResolvedValue(true);

    render(<ManagerWorkspace />);
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));

    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeTruthy();
    expect(screen.getByRole("heading", { name: "General" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Data & Storage" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Updates" })).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "About & Diagnostics" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "ChatGPT & Models" }));
    fireEvent.click(screen.getByText("Advanced model settings"));
    fireEvent.click(screen.getByRole("button", { name: /Live model A/ }));
    expect(screen.getByRole("option", { name: /^Live model A/ })).toBeTruthy();
    expect(screen.getByRole("option", { name: /^Live model B/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("option", { name: /^Live model B/ }));
    expect(
      screen.getByRole("heading", { name: "ChatGPT & Models" }),
    ).toBeTruthy();

    await waitFor(() => {
      expect(host.updateAiExecutionSelection).toHaveBeenCalledWith({
        connection_id: "codex_chatgpt",
        model_id: "gpt-live-b",
        reasoning: { kind: "codex_effort", value: "high" },
      });
    });
    fireEvent.click(screen.getByRole("button", { name: "General" }));
    fireEvent.click(screen.getByRole("button", { name: /Appearance/ }));
    fireEvent.click(screen.getByRole("option", { name: "Dark" }));
    await waitFor(() => {
      expect(host.updateGeneralApplicationPreferences).toHaveBeenCalledWith({
        preferences: {
          ...settings.general_preferences,
          appearance: "dark",
        },
      });
    });
    expect(document.documentElement.dataset.quantixAppearance).toBe("dark");
    fireEvent.click(
      screen.getByRole("switch", { name: /Notify when I am needed/ }),
    );
    await waitFor(() => {
      expect(notifications.enableAttentionNotifications).toHaveBeenCalledTimes(
        1,
      );
      expect(host.updateGeneralApplicationPreferences).toHaveBeenLastCalledWith(
        {
          preferences: {
            ...settings.general_preferences,
            appearance: "dark",
            notify_when_attention_needed: true,
          },
        },
      );
    });
    expect(screen.getByRole("navigation", { name: "Tenders" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Manager" })).toBeNull();
  }, 10_000);

  it("shows the persisted Tender AI selection before the live catalogue hydrates", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      ai_execution: {
        revision: 2n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-5.3-codex-spark",
          reasoning: { kind: "codex_effort", value: "low" },
          catalogue_fetched_at: "2026-08-21T08:00:00Z",
          adapter_version: "codex-v1",
        },
        readiness: "ready",
        status_summary:
          "The selected AI provider, model, and reasoning capability are ready.",
      },
    });
    host.inspectApplicationSettings.mockResolvedValue({
      ...applicationFacts,
      ai_execution_selection: null,
      ai_execution_approval: null,
      provider_connections: [],
    });

    render(<ManagerWorkspace />);

    const controls = await screen.findByRole("group", {
      name: "Tender AI selection",
    });
    expect(
      within(controls).getByRole("button", { name: /Tender AI provider/ }),
    ).toBeTruthy();
    const savedModel = within(controls).getByRole("button", {
      name: /Tender AI model/,
    });
    expect(savedModel.hasAttribute("disabled")).toBe(true);
    const savedReasoning = within(controls).getByRole("button", {
      name: /Tender AI reasoning/,
    });
    expect(savedReasoning.hasAttribute("disabled")).toBe(true);
    expect(controls.textContent).toContain(
      "Saved selection · gpt-5.3-codex-spark",
    );
    expect(controls.textContent).toContain(
      'Saved selection · {"kind":"codex_effort","value":"low"}',
    );
    expect(
      screen.queryByRole("dialog", { name: "Tender AI selection" }),
    ).toBeNull();
  });

  it("shows inline Tender AI controls and persists exact provider, model, and reasoning changes", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.inspectApplicationSettings.mockResolvedValue({
      ...applicationFacts,
      ai_execution_selection: null,
      provider_connections: [tenderAiProviderConnection],
    });
    host.updateTenderAiExecution.mockImplementation(
      async ({ expected_revision, selection }) => ({
        revision: expected_revision + 1n,
        selection,
        readiness: selection ? "ready" : "local_only",
        status_summary: selection
          ? "The selected AI provider, model, and reasoning capability are ready."
          : "Local-only execution is available for this fixture.",
      }),
    );

    render(<ManagerWorkspace />);

    const controls = await screen.findByRole("group", {
      name: "Tender AI selection",
    });
    expect(
      within(controls).getByRole("button", { name: /Tender AI provider/ }),
    ).toBeTruthy();
    expect(
      within(controls).getByRole("button", { name: /Tender AI model/ }),
    ).toBeTruthy();
    expect(
      within(controls).getByRole("button", { name: /Tender AI reasoning/ }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("dialog", { name: "Tender AI selection" }),
    ).toBeNull();

    const selects = Array.from(controls.querySelectorAll("select"));
    expect(selects).toHaveLength(3);
    fireEvent.change(selects[0]!, {
      target: { value: "codex_chatgpt" },
    });
    await waitFor(() => {
      expect(host.updateTenderAiExecution).toHaveBeenLastCalledWith({
        tender_id: tenderId,
        expected_revision: 1n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-live-a",
          reasoning: { kind: "codex_effort", value: "medium" },
          catalogue_fetched_at: "2026-08-21T08:00:00Z",
          adapter_version: "codex-v1",
        },
      });
    });

    fireEvent.change(selects[1]!, { target: { value: "gpt-live-b" } });
    await waitFor(() => {
      expect(host.updateTenderAiExecution).toHaveBeenLastCalledWith({
        tender_id: tenderId,
        expected_revision: 2n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-live-b",
          reasoning: { kind: "codex_effort", value: "high" },
          catalogue_fetched_at: "2026-08-21T08:00:00Z",
          adapter_version: "codex-v1",
        },
      });
    });

    fireEvent.change(selects[2]!, {
      target: {
        value: JSON.stringify({ kind: "codex_effort", value: "xhigh" }),
      },
    });
    await waitFor(() => {
      expect(host.updateTenderAiExecution).toHaveBeenLastCalledWith({
        tender_id: tenderId,
        expected_revision: 3n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-live-b",
          reasoning: { kind: "codex_effort", value: "xhigh" },
          catalogue_fetched_at: "2026-08-21T08:00:00Z",
          adapter_version: "codex-v1",
        },
      });
    });
  });

  it("refreshes inline Tender AI choices from Settings after returning to the workspace", async () => {
    const selectedProjection: ManagerWorkspaceProjection = {
      ...projection,
      ai_execution: {
        revision: 4n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-live-a",
          reasoning: { kind: "codex_effort", value: "medium" },
          catalogue_fetched_at: "2026-08-21T08:00:00Z",
          adapter_version: "codex-v1",
        },
        readiness: "ready",
        status_summary: "The selected AI provider is ready.",
      },
    };
    const initialSettings = {
      ...applicationFacts,
      ai_execution_selection: null,
      provider_connections: [tenderAiProviderConnection],
    };
    const updatedSettings = {
      ...initialSettings,
      provider_connections: [
        {
          ...tenderAiProviderConnection,
          models: [
            ...tenderAiProviderConnection.models,
            {
              model_id: "gpt-live-c",
              display_name: "New live model",
              description: "Newly refreshed ChatGPT model",
              is_default: false,
              input_modalities: ["text"],
              reasoning_options: [
                {
                  selection: {
                    kind: "codex_effort" as const,
                    value: "medium",
                  },
                  label: "medium",
                  description: "Balanced",
                  is_default: true,
                },
              ],
            },
          ],
        },
      ],
    };
    host.inspectManagerWorkspace.mockResolvedValue(selectedProjection);
    host.inspectApplicationSettings
      .mockResolvedValueOnce(initialSettings)
      .mockResolvedValueOnce(updatedSettings);
    host.refreshApplicationSettings.mockResolvedValue(initialSettings);

    render(<ManagerWorkspace />);

    const initialControls = await screen.findByRole("group", {
      name: "Tender AI selection",
    });
    expect(initialControls.textContent).toContain("OpenAI account via Codex");
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Back to workspace" }),
    );

    const refreshedControls = await screen.findByRole("group", {
      name: "Tender AI selection",
    });
    fireEvent.click(
      within(refreshedControls).getByRole("button", {
        name: /Tender AI model/,
      }),
    );
    expect(
      await screen.findByRole("option", { name: /New live model/ }),
    ).toBeTruthy();
    expect(refreshedControls.textContent).toContain("OpenAI account via Codex");
    expect(host.updateTenderAiExecution).not.toHaveBeenCalled();
  });

  it("uses null for Local only and keeps the saved draft when Shift+Enter is used", async () => {
    const selectedProjection: ManagerWorkspaceProjection = {
      ...projection,
      ai_execution: {
        revision: 4n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-live-a",
          reasoning: { kind: "codex_effort", value: "medium" },
          catalogue_fetched_at: "2026-08-21T08:00:00Z",
          adapter_version: "codex-v1",
        },
        readiness: "ready",
        status_summary: "The selected AI provider is ready.",
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(selectedProjection);
    host.inspectApplicationSettings.mockResolvedValue({
      ...applicationFacts,
      ai_execution_selection: null,
      provider_connections: [tenderAiProviderConnection],
    });
    host.updateTenderAiExecution.mockResolvedValue({
      ...selectedProjection.ai_execution,
      revision: 5n,
      selection: null,
      readiness: "local_only",
      status_summary: "Local-only execution is available for this fixture.",
    });
    host.recordEngineerWorkspaceMessage.mockResolvedValue(selectedProjection);

    render(<ManagerWorkspace />);

    const controls = await screen.findByRole("group", {
      name: "Tender AI selection",
    });
    const selects = Array.from(controls.querySelectorAll("select"));
    expect(selects).toHaveLength(3);
    fireEvent.change(selects[0]!, { target: { value: "local_only" } });
    await waitFor(() => {
      expect(host.updateTenderAiExecution).toHaveBeenCalledWith({
        tender_id: tenderId,
        expected_revision: 4n,
        selection: null,
      });
    });

    const composer = screen.getByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    fireEvent.change(composer, { target: { value: "First line" } });
    fireEvent.keyDown(composer, { key: "Enter", shiftKey: true });
    expect(host.recordEngineerWorkspaceMessage).not.toHaveBeenCalled();
    fireEvent.change(composer, {
      target: { value: "First line\nSecond line" },
    });
    expect((composer as HTMLTextAreaElement).value).toBe(
      "First line\nSecond line",
    );
    fireEvent.keyDown(composer, { key: "Enter" });

    await waitFor(() => {
      expect(host.recordEngineerWorkspaceMessage).toHaveBeenCalledWith(
        tenderId,
        "First line\nSecond line",
        [],
        [],
      );
    });
  });

  it("settles local runtime readiness independently of a stalled settings catalogue", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      files: {
        ...projection.files,
        tender_document_count: 1,
      },
      current_action: {
        ...projection.current_action,
        kind: "review_intake",
        title: "Review the registered package",
        summary: "The registered package is ready for local document work.",
      },
    });
    host.inspectRuntimeReadiness.mockResolvedValue({
      state: "ready",
      issues: [],
      uv_version: "0.4.0",
      ocr_version: "1.0.0",
      repair_available: false,
    });
    host.inspectApplicationSettings.mockReturnValue(new Promise(() => {}));

    render(<ManagerWorkspace />);

    expect(
      await screen.findByRole("heading", {
        name: "Review the registered package",
      }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("heading", { name: "Checking document tools" }),
    ).toBeNull();
  });

  it("keeps a transient runtime probe failure checking until a retry is ready", async () => {
    vi.useFakeTimers();
    const documentProjection = {
      ...projection,
      files: { ...projection.files, tender_document_count: 1 },
    };
    const transient = {
      state: "repair_required" as const,
      issues: ["runtime_probe_failed" as const],
      uv_version: null,
      ocr_version: null,
      repair_available: false,
    };
    const ready = {
      state: "ready" as const,
      issues: [],
      uv_version: "0.4.0",
      ocr_version: "1.0.0",
      repair_available: false,
    };
    host.inspectRuntimeReadiness
      .mockResolvedValueOnce(transient)
      .mockResolvedValueOnce(ready);
    host.inspectManagerWorkspace.mockResolvedValue(documentProjection);

    render(
      <StrictMode>
        <ManagerWorkspace initialProjection={documentProjection} />
      </StrictMode>,
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(1);
    expect(
      screen.getByRole("heading", { name: "Checking document tools" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: /Prepare document tools/i }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Cancel preparation" }),
    ).toBeNull();
    expect(host.resumeManagerIntakes).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(500);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(2);
    expect(host.resumeManagerIntakes).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByRole("heading", { name: "Checking document tools" }),
    ).toBeNull();
  });

  it("does not overlap or retain a transient runtime retry after unmount", async () => {
    vi.useFakeTimers();
    const documentProjection = {
      ...projection,
      files: { ...projection.files, tender_document_count: 1 },
    };
    const transient = {
      state: "repair_required" as const,
      issues: ["runtime_probe_failed" as const],
      uv_version: null,
      ocr_version: null,
      repair_available: false,
    };
    let resolveRetry: ((value: typeof transient) => void) | undefined;
    const retry = new Promise<typeof transient>((resolve) => {
      resolveRetry = resolve;
    });
    host.inspectRuntimeReadiness
      .mockResolvedValueOnce(transient)
      .mockReturnValueOnce(retry);

    const { unmount } = render(
      <ManagerWorkspace initialProjection={documentProjection} />,
    );
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(1);

    await act(async () => {
      vi.advanceTimersByTime(500);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(2);

    await act(async () => {
      vi.advanceTimersByTime(5_000);
      await Promise.resolve();
    });
    expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(2);

    unmount();
    resolveRetry?.(transient);
    await act(async () => {
      await Promise.resolve();
      vi.advanceTimersByTime(5_000);
    });
    expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(2);
  });

  it("clears a scheduled transient runtime retry on unmount", async () => {
    vi.useFakeTimers();
    const documentProjection = {
      ...projection,
      files: { ...projection.files, tender_document_count: 1 },
    };
    host.inspectRuntimeReadiness.mockResolvedValueOnce({
      state: "repair_required" as const,
      issues: ["runtime_probe_failed" as const],
      uv_version: null,
      ocr_version: null,
      repair_available: false,
    });

    const { unmount } = render(
      <ManagerWorkspace initialProjection={documentProjection} />,
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(1);

    unmount();
    await act(async () => {
      vi.advanceTimersByTime(5_000);
      await Promise.resolve();
    });
    expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(1);
  });

  it("drives the Quantix-owned ChatGPT browser sign-in without exposing credentials", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    const disconnected = {
      ...applicationFacts,
      ai_execution_selection: null,
      chatgpt: {
        state: "absent",
        account_id: null,
        plan_type: null,
        expires_at_ms: null,
        login_phase: "idle",
      },
      provider_connections: [
        {
          connection_id: "codex_chatgpt",
          provider: "codex",
          display_name: "ChatGPT",
          status: "authentication_required",
          account_label: null,
          account_plan: null,
          models: [],
          catalogue_fetched_at: null,
          adapter_version: "0.147.0",
          status_summary: "Connect ChatGPT.",
        },
      ],
    };
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnected)
      .mockResolvedValue({
        ...disconnected,
        chatgpt: {
          ...disconnected.chatgpt,
          login_phase: "awaiting_browser",
        },
      });
    host.startChatGptLogin.mockResolvedValue({ status: "awaiting_browser" });

    render(<ManagerWorkspace />);
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "ChatGPT & Models" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Connect ChatGPT" }),
    );

    expect(host.startChatGptLogin).toHaveBeenCalledWith();
    expect(
      await screen.findByText(
        "Finish signing in in your browser. Quantix will connect automatically.",
      ),
    ).toBeTruthy();
    expect(document.body.textContent).not.toContain("accessToken");
  });

  it("offers the explicit one-time-code fallback from ChatGPT settings", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    const disconnected = {
      ...applicationFacts,
      ai_execution_selection: null,
      ai_execution_approval: null,
      chatgpt: {
        state: "absent",
        account_id: null,
        plan_type: null,
        expires_at_ms: null,
        login_phase: "idle",
      },
      provider_connections: [
        {
          connection_id: "codex_chatgpt",
          provider: "codex",
          display_name: "ChatGPT",
          status: "authentication_required",
          account_label: null,
          account_plan: null,
          models: [],
          catalogue_fetched_at: null,
          adapter_version: "codex-v1",
          status_summary: "Connect ChatGPT.",
        },
      ],
    };
    host.refreshApplicationSettings
      .mockResolvedValueOnce(disconnected)
      .mockResolvedValue({
        ...disconnected,
        chatgpt: {
          ...disconnected.chatgpt,
          login_phase: "awaiting_device",
        },
      });
    host.startChatGptDeviceLogin.mockResolvedValue({
      verification_url: "https://auth.openai.com/codex/device",
      user_code: "BUILD-2026",
    });

    render(<ManagerWorkspace />);
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "ChatGPT & Models" }),
    );
    fireEvent.click(await screen.findByText("Having trouble signing in?"));
    fireEvent.click(
      screen.getByRole("button", { name: "Sign in on another device" }),
    );

    expect(host.startChatGptDeviceLogin).toHaveBeenCalledWith();
    expect(await screen.findByText("BUILD-2026")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Open OpenAI sign-in page" }),
    ).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/API key|accessToken|OAuth/i);
  });

  it("keeps the Host-designated meaningful message visible after routine chatter", async () => {
    const routineMessages = Array.from({ length: 9 }, (_, index) => ({
      message_id: `${index}`.repeat(32),
      sequence: index + 2,
      author: "engineer" as const,
      kind: "routine" as const,
      body: `Routine note ${index + 1}`,
      created_at: `2026-08-14T10:${String(index + 1).padStart(2, "0")}:00Z`,
      references: [],
    }));
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      conversation: {
        ...projection.conversation!,
        messages: [projection.conversation!.messages[0], ...routineMessages],
      },
    });

    render(<ManagerWorkspace />);

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
            references: [],
          },
        ],
      },
    });
    render(<ManagerWorkspace />);
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
        [],
        [],
      );
    });
    expect(
      await screen.findByText("Check the insurance exclusions first."),
    ).toBeTruthy();
  });

  it("searches canonical Tender records and attaches allowed evidence context", async () => {
    installResponsiveMatchMedia(760);
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.searchManagerWorkspace.mockResolvedValue({
      query: "bond",
      groups: [
        { kind: "conversation", hits: [] },
        { kind: "work", hits: [] },
        { kind: "files", hits: [] },
        {
          kind: "evidence",
          hits: [
            {
              kind: "evidence",
              reference: "artifact-bid-bond:7",
              version: 2,
              title: "Bid bond validity",
              detail: "Section 4.2 — validity is not stated",
            },
          ],
        },
        { kind: "agents", hits: [] },
      ],
    });

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Show Tender workspace" }),
    );
    const workspaceDrawer = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    const search = within(workspaceDrawer).getByRole("searchbox", {
      name: "Search this Tender",
    });
    fireEvent.change(search, { target: { value: "bond" } });
    fireEvent.click(
      within(workspaceDrawer).getByRole("button", { name: "Search" }),
    );
    expect(
      await screen.findByRole("button", { name: /Bid bond validity/ }),
    ).toBeTruthy();
    expect(
      screen.getByRole("region", { name: "Tender search results" }),
    ).toBeTruthy();
    expect(host.searchManagerWorkspace).toHaveBeenCalledWith(tenderId, "bond");

    fireEvent.click(
      screen.getByRole("button", { name: "Attach allowed context" }),
    );
    const attached = screen.getByLabelText("Attached context");
    expect(within(attached).getByText(/Bid bond validity/)).toBeTruthy();
  });

  it("shows the truthful intake stage, exact Manager references, and source provenance", async () => {
    installResponsiveMatchMedia(760);
    const intakeProjection: ManagerWorkspaceProjection = {
      ...projection,
      conversation: {
        ...projection.conversation!,
        latest_meaningful_message_id: "e".repeat(32),
        messages: [
          ...projection.conversation!.messages,
          {
            message_id: "e".repeat(32),
            sequence: 2,
            author: "manager",
            kind: "question",
            body: "What is the confirmed bid bond validity period?",
            created_at: "2026-08-14T10:03:00Z",
            references: [
              {
                kind: "tender_record",
                reference: "f".repeat(32),
                version: 2,
                evidence_ordinal: null,
                label: "Bid bond validity",
                detail: "Tender query",
              },
              {
                kind: "source_evidence",
                reference: "1".repeat(32),
                version: 1,
                evidence_ordinal: 7,
                label: "01 Instructions/ITT.pdf",
                detail: "Section 4.2 — validity is not stated",
              },
            ],
          },
        ],
      },
      current_action: {
        kind: "answer_manager_question",
        title: "Answer the Tendering Manager",
        summary: "One material detail is missing from the source package.",
        action_label: "Answer question",
        requires_engineer: true,
      },
      files: {
        tender_document_count: 1,
        quantix_output_count: 1,
        tender_documents: [
          {
            artifact_id: "1".repeat(32),
            version: 1,
            package_path: "01 Instructions/ITT.pdf",
            document_type: "pdf_document",
            media_type: "application/pdf",
            sha256: "2".repeat(64),
            size_bytes: 2048n,
            registration_state: "registered",
            parse_state: "parsed",
            exception: null,
          },
        ],
        quantix_outputs: [],
      },
      intake: {
        intake_run_id: "3".repeat(32),
        stage: "waiting_for_engineer",
        status: "needs_engineer",
        label: "Waiting for your answer",
        summary:
          "The Tendering Manager found information that is genuinely missing.",
        parseable_document_count: 1,
        parsed_document_count: 1,
        extraction_run_count: 1,
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(intakeProjection);
    render(<ManagerWorkspace />);

    expect(await screen.findByText("Waiting for your answer")).toBeTruthy();
    fireEvent.click(screen.getByText("2 references"));
    expect(screen.getByText("Bid bond validity")).toBeTruthy();
    expect(screen.getByText("01 Instructions/ITT.pdf")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Answer question" }));
    expect(document.activeElement).toBe(
      screen.getByRole("textbox", { name: "Message your Tendering Manager" }),
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "Show Tender workspace" }),
    );
    const workspaceDrawer = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    fireEvent.click(
      within(workspaceDrawer).getByRole("button", { name: "Files" }),
    );
    fireEvent.click(
      within(workspaceDrawer).getByRole("button", {
        name: "Close Tender workspace",
      }),
    );
    expect(screen.getByText("01 Instructions/ITT.pdf")).toBeTruthy();
    fireEvent.click(screen.getByText("Provenance"));
    expect(screen.getByText("2,048 bytes")).toBeTruthy();
  });

  it("distinguishes registered files from intake exceptions with exact codes", async () => {
    installResponsiveMatchMedia(760);
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      files: {
        ...projection.files,
        tender_document_count: 2,
        tender_documents: [
          {
            artifact_id: "registered-document",
            version: 1,
            package_path: "01 Instructions/ITT.pdf",
            document_type: "pdf_document",
            media_type: "application/pdf",
            sha256: "a".repeat(64),
            size_bytes: 2048n,
            registration_state: "registered",
            parse_state: "parsed",
            exception: null,
          },
          {
            artifact_id: "exception-document",
            version: 1,
            package_path: "04 Supporting/legacy.xlsm",
            document_type: "unknown",
            media_type: null,
            sha256: null,
            size_bytes: 4096n,
            registration_state: "exception",
            parse_state: "not_requested",
            exception: "macro_bearing",
          },
        ],
      },
    });
    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Show Tender workspace" }),
    );
    const workspaceDrawer = await screen.findByRole("dialog", {
      name: "Tender workspace",
    });
    fireEvent.click(
      within(workspaceDrawer).getByRole("button", { name: "Files" }),
    );
    fireEvent.click(
      within(workspaceDrawer).getByRole("button", {
        name: "Close Tender workspace",
      }),
    );

    expect(screen.getByText("Registered documents")).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: "Registration exceptions" }),
    ).toBeTruthy();
    expect(screen.getByText("Registered · parsed")).toBeTruthy();
    const exceptionRow = screen
      .getByText("04 Supporting/legacy.xlsm")
      .closest("li");
    expect(exceptionRow).toBeTruthy();
    expect(
      within(exceptionRow!).getByText("Registration exception · macro_bearing"),
    ).toBeTruthy();
    fireEvent.click(within(exceptionRow!).getByText("Provenance"));
    expect(within(exceptionRow!).getByText("macro_bearing")).toBeTruthy();
  });

  it("retries a failed intake through the Host boundary", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      current_action: {
        kind: "retry_intake",
        title: "Intake needs attention",
        summary: "The source package is safe and can be retried.",
        action_label: "Retry intake",
        requires_engineer: true,
      },
      intake: {
        intake_run_id: "3".repeat(32),
        stage: "failed",
        status: "failed",
        label: "Intake needs attention",
        summary: "The local AI runtime stopped before the review completed.",
        parseable_document_count: 1,
        parsed_document_count: 1,
        extraction_run_count: 0,
      },
    });
    host.retryManagerIntake.mockResolvedValue(undefined);
    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Retry intake" }),
    );
    await waitFor(() => {
      expect(host.retryManagerIntake).toHaveBeenCalledWith(tenderId);
    });
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
        tasks: [],
      },
      files: {
        tender_document_count: 0,
        quantix_output_count: 0,
        tender_documents: [],
        quantix_outputs: [],
      },
      team: {
        active_agent_runs: 0,
        waiting_tasks: 0,
        needs_engineer: 0,
        events: [],
        agent_runs: [],
      },
      intake: null,
      ai_execution: null,
      capability_readiness: null,
      doctor_blockers: [],
    };
    host.inspectManagerWorkspace.mockResolvedValue(empty);
    host.startManagerTender.mockResolvedValue(null);
    render(<ManagerWorkspace />);

    const startButton = await screen.findByRole("button", {
      name: "Choose Tender Package",
    });
    const workspace = screen.getByTestId("manager-workspace");
    const emptyMain = startButton.closest("main");
    const workspaceMain = startButton.closest(".manager-workspace__main");
    expect(emptyMain?.classList.contains("manager-workspace__empty-main")).toBe(
      true,
    );
    expect(workspaceMain?.parentElement).toBe(workspace);
    expect(
      startButton.closest(".manager-workspace__surface--empty"),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: /Tender workspace/ }),
    ).toBeNull();
    expect(
      screen.queryByRole("complementary", { name: "Tender workspace" }),
    ).toBeNull();
    expect(screen.queryByText("Tender office")).toBeNull();
    expect(
      screen.getByRole("menubar", { name: "Application commands" }),
    ).toBeTruthy();
    expect(
      document.querySelector(".manager-workspace__sidebar-brand strong"),
    ).toBeNull();
    expect(
      screen
        .getByTestId("manager-workspace")
        .classList.contains("has-workspace-bar"),
    ).toBe(false);
    expect(
      screen.getAllByRole("button", { name: "Choose Tender Package" }),
    ).toHaveLength(1);

    startButton.focus();
    fireEvent.click(screen.getByRole("button", { name: "Hide Tenders" }));
    expect(screen.getByRole("button", { name: "Show Tenders" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Choose Tender Package" })).toBe(
      startButton,
    );
    expect(document.activeElement).toBe(startButton);
    fireEvent.click(screen.getByRole("button", { name: "Show Tenders" }));
    expect(screen.getByRole("button", { name: "Choose Tender Package" })).toBe(
      startButton,
    );

    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(
      await screen.findByRole("heading", { name: "Start a Tender" }),
    ).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: "Choose Tender Package" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Continue without AI" }),
    );

    await waitFor(() => {
      expect(host.startManagerTender).toHaveBeenCalledWith("directory", true);
    });
  });

  it("registers a Tender package even while every AI Provider is unavailable", async () => {
    const empty: ManagerWorkspaceProjection = {
      ...projection,
      catalogue: [],
      selected_tender: null,
      conversation: null,
      current_action: {
        kind: "start_tender",
        title: "Start a Tender",
        summary: "Choose the Tender Package.",
        action_label: "Choose Tender Package",
        requires_engineer: true,
      },
      intake: null,
    };
    host.inspectManagerWorkspace.mockResolvedValue(empty);
    host.startManagerTender.mockResolvedValue(null);
    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Choose Tender Package" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Continue without AI" }),
    );

    await waitFor(() => {
      expect(host.startManagerTender).toHaveBeenCalledWith("directory", true);
    });
    expect(
      screen.queryByText(
        /copies the package into its private application home/i,
      ),
    ).toBeNull();
  });

  it("renders package stage details and sends the exact cancellation id", async () => {
    const progress = {
      operation_id: "package-operation-42",
      kind: "start_tender",
      stage: "reading_package",
      source_kind: "directory",
      source_name: "Bid Package",
      current_relative_path: "01 Instructions/ITT.pdf",
      discovered_count: 7,
      processed_count: 3,
      registered_count: 2,
      exception_count: 1,
      total_count: 7,
      cancellable: true,
      cancellation_requested: false,
      started_at_epoch_ms: Date.now() - 5_000,
      updated_at_epoch_ms: Date.now(),
    };
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      catalogue: [],
      selected_tender: null,
      conversation: null,
      current_action: {
        kind: "start_tender",
        title: "Start a Tender",
        summary: "Choose the Tender Package.",
        action_label: "Choose Tender Package",
        requires_engineer: true,
      },
      intake: null,
    });
    host.inspectPackageIntakeProgress
      .mockResolvedValueOnce(null)
      .mockResolvedValue(progress);
    host.startManagerTender.mockImplementation(
      () =>
        new Promise((resolve) => window.setTimeout(() => resolve(null), 200)),
    );
    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Choose Tender Package" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Continue without AI" }),
    );
    expect(
      (await screen.findAllByText("Copying and verifying documents")).length,
    ).toBeGreaterThan(0);
    expect(await screen.findByText("01 Instructions/ITT.pdf")).toBeTruthy();
    expect(await screen.findByText("3 of 7 processed")).toBeTruthy();
    const packagePanel = document.querySelector(".workspace-operation-panel");
    const workspace = screen.getByTestId("manager-workspace");
    expect(packagePanel).toBeTruthy();
    expect(
      packagePanel?.closest(".manager-workspace__main")?.parentElement,
    ).toBe(workspace);
    expect(
      packagePanel?.closest(".manager-workspace__surface--empty"),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(host.cancelPackageIntake).toHaveBeenCalledWith(
        "package-operation-42",
      );
    });
  });

  it("keeps Settings, retention, and the selected Tender view stable across sidebar toggles", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    render(<ManagerWorkspace />);

    await screen.findByRole("heading", {
      name: "West Campus MEP",
      level: 1,
    });
    const workspaceRail = await screen.findByRole("complementary", {
      name: "Tender workspace",
    });
    fireEvent.click(
      within(workspaceRail).getByRole("button", { name: "Files" }),
    );
    expect(screen.getByRole("heading", { name: "Files" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Hide Tenders" }));
    expect(screen.getByRole("button", { name: "Show Tenders" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Files" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Show Tenders" }));
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Hide Tenders" }));
    expect(screen.getByRole("button", { name: "Show Tenders" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Settings" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Show Tenders" }));
    fireEvent.click(screen.getByRole("button", { name: /Archived & Trash/ }));
    expect(
      await screen.findByRole("heading", { name: "Archived & Trash" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Hide Tenders" }));
    expect(screen.getByRole("button", { name: "Show Tenders" })).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: "Archived & Trash" }),
    ).toBeTruthy();
  });

  it("only elevates slow Tender navigation into the centered opening panel", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.selectManagerWorkspaceTender.mockImplementation(
      () =>
        new Promise((resolve) =>
          window.setTimeout(() => resolve(projection), 650),
        ),
    );
    render(<ManagerWorkspace />);
    await screen.findByRole("heading", { name: "West Campus MEP" });
    fireEvent.click(
      screen.getByRole("button", { name: /West Campus MEP.*Intake/ }),
    );
    expect(
      screen.queryByRole("heading", { name: "Opening West Campus MEP…" }),
    ).toBeNull();
    expect(
      await screen.findByRole("heading", { name: "Opening West Campus MEP…" }),
    ).toBeTruthy();
  });

  it("retries a failed workspace operation in its originating action", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.recordEngineerWorkspaceMessage
      .mockRejectedValueOnce(new Error("message store unavailable"))
      .mockResolvedValue(projection);
    render(<ManagerWorkspace />);
    const initialInspectCalls = host.inspectManagerWorkspace.mock.calls.length;
    const composer = await screen.findByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    fireEvent.change(composer, { target: { value: "Retry this note" } });
    fireEvent.keyDown(composer, { key: "Enter" });
    const retry = await screen.findByRole("button", {
      name: "Retry saving message",
    });
    fireEvent.click(retry);
    await waitFor(() => {
      expect(host.recordEngineerWorkspaceMessage).toHaveBeenCalledTimes(2);
    });
    expect(host.inspectManagerWorkspace).toHaveBeenCalledTimes(
      initialInspectCalls,
    );
  });

  it("shows the canonical waiting state and routes to explicit provider approval", async () => {
    const waiting: ManagerWorkspaceProjection = {
      ...projection,
      current_action: {
        kind: "configure_ai_provider",
        title: "Waiting for AI Provider",
        summary:
          "The Tender Package is safe. Quantix will continue only with the exact approved choice.",
        action_label: "Choose an AI provider",
        requires_engineer: true,
      },
      intake: {
        intake_run_id: "3".repeat(32),
        stage: "waiting_for_provider",
        status: "waiting",
        label: "Waiting for AI Provider",
        summary:
          "The Tender Package is registered safely while the exact choice is unavailable.",
        parseable_document_count: 1,
        parsed_document_count: 0,
        extraction_run_count: 0,
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(waiting);
    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Open Settings" }),
    );
    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeTruthy();
  });

  it("renders an active intake as paused when the AI office is unavailable", async () => {
    const pausedProjection: ManagerWorkspaceProjection = {
      ...projection,
      current_action: {
        kind: "observe_intake",
        title: "Reading Tender documents",
        summary: "Quantix is deriving exact source evidence.",
        action_label: "Intake in progress",
        requires_engineer: false,
      },
      intake: {
        intake_run_id: "3".repeat(32),
        stage: "reading_documents",
        status: "working",
        label: "Reading Tender documents",
        summary:
          "Quantix is deriving exact source evidence from the registered documents.",
        parseable_document_count: 1,
        parsed_document_count: 0,
        extraction_run_count: 0,
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(pausedProjection);
    host.recordEngineerWorkspaceMessage.mockResolvedValue(pausedProjection);
    const { container } = render(<ManagerWorkspace />);

    expect(
      await screen.findByText("Paused — AI office unavailable"),
    ).toBeTruthy();
    expect(
      screen.getByRole("heading", { name: "Tender intake paused" }),
    ).toBeTruthy();
    expect(
      screen.queryByText("Quantix is deriving exact source evidence."),
    ).toBeNull();
    expect(
      container.querySelector(".manager-view__status .is-working"),
    ).toBeNull();
    const composer = screen.getByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    expect((composer as HTMLTextAreaElement).disabled).toBe(false);
    fireEvent.change(composer, { target: { value: "Keep this note queued." } });
    fireEvent.keyDown(composer, { key: "Enter" });
    await waitFor(() => {
      expect(host.recordEngineerWorkspaceMessage).toHaveBeenCalledWith(
        tenderId,
        "Keep this note queued.",
        [],
        [],
      );
    });
  });
});
