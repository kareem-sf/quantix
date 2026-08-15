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
  cancelProviderLogin: vi.fn(),
  connectAnthropic: vi.fn(),
  connectGemini: vi.fn(),
  chooseAndImportTenderPackage: vi.fn(),
  inspectManagerWorkspace: vi.fn(),
  logoutProvider: vi.fn(),
  disconnectAiProvider: vi.fn(),
  openProviderLogin: vi.fn(),
  rebindManagerIntakeProvider: vi.fn(),
  recordEngineerWorkspaceMessage: vi.fn(),
  refreshApplicationSettings: vi.fn(),
  retryManagerIntake: vi.fn(),
  selectManagerWorkspaceTender: vi.fn(),
  startManagerTender: vi.fn(),
  startProviderLogin: vi.fn(),
  updateAiExecutionSelection: vi.fn(),
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
  },
  files: {
    tender_document_count: 0,
    quantix_output_count: 0,
    tender_documents: [],
  },
  team: { active_agent_runs: 0, waiting_tasks: 0, needs_engineer: 0 },
  intake: null,
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

  it("opens application Settings and saves a live provider selection atomically", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    const settings = {
      ai_execution_selection: {
        connection_id: "codex_chatgpt",
        provider: "codex",
        model_id: "gpt-live-a",
        reasoning: { kind: "codex_effort", value: "medium" },
        catalogue_fetched_at: "2026-08-15T10:00:00Z",
        adapter_version: "0.147.0",
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
      active_provider_login: null,
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

    render(<ManagerWorkspace aiAvailable />);
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));

    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeTruthy();
    expect(screen.getByRole("option", { name: "Live model A" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "Live model B" })).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Model"), {
      target: { value: "gpt-live-b" },
    });

    await waitFor(() => {
      expect(host.updateAiExecutionSelection).toHaveBeenCalledWith({
        connection_id: "codex_chatgpt",
        model_id: "gpt-live-b",
        reasoning: { kind: "codex_effort", value: "high" },
      });
    });
    expect(screen.getByRole("navigation", { name: "Tenders" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Manager" })).toBeNull();
  });

  it("projects Codex-managed device login without exposing credentials", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    const disconnected = {
      ai_execution_selection: null,
      active_provider_login: null,
      provider_connections: [
        {
          connection_id: "codex_chatgpt",
          provider: "codex",
          display_name: "OpenAI account via Codex",
          status: "authentication_required",
          account_label: null,
          account_plan: null,
          models: [],
          catalogue_fetched_at: null,
          adapter_version: "0.147.0",
          status_summary: "Connect an OpenAI account.",
        },
      ],
    };
    host.refreshApplicationSettings.mockResolvedValue(disconnected);
    host.startProviderLogin.mockResolvedValue({
      ...disconnected,
      active_provider_login: {
        connection_id: "codex_chatgpt",
        login_id: "login-1",
        method: "device_code",
        status: "awaiting_user",
        authorization_url: "https://auth.openai.com/codex/device",
        user_code: "ABCD-EFGH",
        status_summary: "Enter the one-time code.",
      },
    });

    render(<ManagerWorkspace aiAvailable={false} />);
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Use device code" }),
    );

    expect(await screen.findByText("ABCD-EFGH")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Continue in browser" }),
    );
    expect(host.openProviderLogin).toHaveBeenCalledWith({
      login_id: "login-1",
    });
    expect(host.startProviderLogin).toHaveBeenCalledWith({
      method: "device_code",
    });
    expect(document.body.textContent).not.toContain("accessToken");
  });

  it("submits an Anthropic key once and renders only credential-free state", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    const disconnected = {
      ai_execution_selection: null,
      active_provider_login: null,
      provider_connections: [
        {
          connection_id: "codex_chatgpt",
          provider: "codex",
          display_name: "OpenAI account via Codex",
          status: "authentication_required",
          account_label: null,
          account_plan: null,
          models: [],
          catalogue_fetched_at: null,
          adapter_version: "0.147.0",
          status_summary: "Connect an OpenAI account.",
        },
        {
          connection_id: "anthropic_byok",
          provider: "anthropic",
          display_name: "Anthropic API key",
          status: "authentication_required",
          account_label: null,
          account_plan: null,
          models: [],
          catalogue_fetched_at: null,
          adapter_version: "anthropic-messages-v1",
          status_summary: "Add an Anthropic API key to connect.",
        },
      ],
    };
    const connected = {
      ...disconnected,
      provider_connections: disconnected.provider_connections.map(
        (connection) =>
          connection.connection_id === "anthropic_byok"
            ? {
                ...connection,
                status: "ready",
                account_label: "API key stored in the system credential vault",
                models: [
                  {
                    model_id: "claude-live",
                    display_name: "Claude Live",
                    description: "Live Anthropic model",
                    is_default: false,
                    input_modalities: ["text"],
                    reasoning_options: [
                      {
                        selection: { kind: "provider_default" },
                        label: "Provider default",
                        description: "Provider choice",
                        is_default: true,
                      },
                    ],
                  },
                ],
                catalogue_fetched_at: "2026-08-15T12:00:00Z",
              }
            : connection,
      ),
    };
    host.refreshApplicationSettings.mockResolvedValue(disconnected);
    host.connectAnthropic.mockResolvedValue(connected);
    host.updateAiExecutionSelection.mockResolvedValue({
      ...connected,
      ai_execution_selection: {
        connection_id: "anthropic_byok",
        provider: "anthropic",
        model_id: "claude-live",
        reasoning: { kind: "provider_default" },
        catalogue_fetched_at: "2026-08-15T12:00:00Z",
        adapter_version: "anthropic-messages-v1",
      },
    });

    render(<ManagerWorkspace aiAvailable={false} />);
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
    fireEvent.change(screen.getByLabelText("Provider"), {
      target: { value: "anthropic_byok" },
    });
    fireEvent.change(screen.getByLabelText("Anthropic API key"), {
      target: { value: "sk-ant-secret" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Connect Anthropic" }));

    await waitFor(() => {
      expect(host.connectAnthropic).toHaveBeenCalledWith({
        api_key: "sk-ant-secret",
      });
    });
    expect(document.body.textContent).not.toContain("sk-ant-secret");
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
            references: [],
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

  it("shows the truthful intake stage, exact Manager references, and source provenance", async () => {
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
            parse_state: "parsed",
          },
        ],
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
    render(<ManagerWorkspace aiAvailable />);

    expect(await screen.findByText("Waiting for your answer")).toBeTruthy();
    fireEvent.click(screen.getByText("2 references"));
    expect(screen.getByText("Bid bond validity")).toBeTruthy();
    expect(screen.getByText("01 Instructions/ITT.pdf")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Answer question" }));
    expect(document.activeElement).toBe(
      screen.getByRole("textbox", { name: "Message your Tendering Manager" }),
    );

    fireEvent.click(screen.getByRole("button", { name: "Files" }));
    expect(screen.getByText("01 Instructions/ITT.pdf")).toBeTruthy();
    fireEvent.click(screen.getByText("Provenance"));
    expect(screen.getByText("2,048 bytes")).toBeTruthy();
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
    render(<ManagerWorkspace aiAvailable />);

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
      },
      files: {
        tender_document_count: 0,
        quantix_output_count: 0,
        tender_documents: [],
      },
      team: { active_agent_runs: 0, waiting_tasks: 0, needs_engineer: 0 },
      intake: null,
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
    render(<ManagerWorkspace aiAvailable={false} />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Choose Tender Package" }),
    );

    await waitFor(() => {
      expect(host.startManagerTender).toHaveBeenCalledWith("directory");
    });
    expect(
      screen.getByText(/register it safely and wait for an AI Provider/i),
    ).toBeTruthy();
  });

  it("shows the canonical waiting state and requires an explicit provider rebind", async () => {
    const waiting: ManagerWorkspaceProjection = {
      ...projection,
      current_action: {
        kind: "configure_ai_provider",
        title: "Waiting for AI Provider",
        summary:
          "The Tender Package is safe. Quantix will continue only with the exact approved choice.",
        action_label: "Use selected AI",
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
    host.rebindManagerIntakeProvider.mockResolvedValue(undefined);
    render(<ManagerWorkspace aiAvailable />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Use selected AI" }),
    );

    await waitFor(() => {
      expect(host.rebindManagerIntakeProvider).toHaveBeenCalledWith(tenderId);
    });
    expect(
      screen.getAllByText("Waiting for AI Provider").length,
    ).toBeGreaterThan(0);
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
    const { container } = render(<ManagerWorkspace aiAvailable={false} />);

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
      );
    });
  });
});
