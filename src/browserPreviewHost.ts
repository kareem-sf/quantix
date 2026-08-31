import type { InvokeArgs } from "@tauri-apps/api/core";

import type { ApplicationSettingsView } from "./bindings/ApplicationSettingsView";
import type { GeneralApplicationPreferences } from "./bindings/GeneralApplicationPreferences";
import type { ManagerWorkspaceProjection } from "./bindings/ManagerWorkspaceProjection";
import type { PackageIntakeProgress } from "./bindings/PackageIntakeProgress";
import type { RuntimeReadiness } from "./bindings/RuntimeReadiness";
import type { SetupOutcome } from "./bindings/SetupOutcome";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { QuantixDoctorReport } from "./bindings/QuantixDoctorReport";
import type { WorkspaceSearchProjection } from "./bindings/WorkspaceSearchProjection";
import type { TenderBackupRecord } from "./bindings/TenderBackupRecord";
import type { TenderIntegrityReport } from "./bindings/TenderIntegrityReport";
import type { TenderRecoveryDecision } from "./bindings/TenderRecoveryDecision";
import type { TenderRecoveryRecord } from "./bindings/TenderRecoveryRecord";
import type { TrashedTenderRecord } from "./bindings/TrashedTenderRecord";
import type { DeletionReceipt } from "./bindings/DeletionReceipt";

const setupOutcome: SetupOutcome = {
  state: "ready",
  setup_performed: false,
  issues: [],
};

const idleUpdateStatus: UpdateStatus = {
  state: "idle",
  offer: null,
  decision_history: [],
  diagnostic: null,
};

const readyDocumentTools: RuntimeReadiness = {
  state: "ready",
  issues: [],
  uv_version: "browser-preview",
  ocr_version: "browser-preview",
  repair_available: false,
};

const browserDoctor: QuantixDoctorReport = {
  revision: "browser-preview",
  healthy: false,
  findings: [
    {
      code: "ai_selection_required",
      area: "default_ai",
      severity: "warning",
      title: "Choose an AI default for new Tenders",
      cause: "The browser preview has no connected AI provider.",
      affected_capability: "AI-assisted work in newly created Tenders",
      impact: "Local-only Tender work remains available.",
      safe_remediation: "Open the native app to connect an AI provider.",
      repair_action: null,
    },
  ],
};

const emptyWorkspace: ManagerWorkspaceProjection = {
  catalogue: [],
  selected_tender: null,
  conversation: null,
  current_action: {
    kind: "start_tender",
    title: "Start a Tender",
    summary:
      "Choose a Tender Package and the Tender Manager will take it from there.",
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
  external_rfis: [],
  doctor_blockers: [],
};

const previewTenderId = "7".repeat(32);
const recoveryTenderIds = ["6".repeat(32), "c".repeat(32)] as const;
const recoveryBackupId = "b".repeat(32);
const recoveryRecordId = "r".repeat(32);
const previewConversationId = "8".repeat(32);
let previewTenderName = "North District Civic Centre";
let previewMessageSequence = 3;
const previewMessages: NonNullable<
  ManagerWorkspaceProjection["conversation"]
>["messages"] = [
  {
    message_id: "9".repeat(32),
    sequence: 1,
    author: "system",
    kind: "status",
    body: "North District Civic Centre workspace is ready.",
    created_at: "2026-08-20T11:10:00Z",
    references: [],
  },
  {
    message_id: "a".repeat(32),
    sequence: 2,
    author: "manager",
    kind: "blocker",
    body: "AI-assisted work is waiting for an exact provider, model, and reasoning selection. Local Tender work remains available.",
    created_at: "2026-08-20T11:12:00Z",
    references: [],
  },
];

function workspacePreviewEnabled(): boolean {
  return new URLSearchParams(globalThis.location?.search ?? "").has(
    "workspace-preview",
  );
}

function recoveryPreviewEnabled(): boolean {
  return new URLSearchParams(globalThis.location?.search ?? "").has(
    "tender-recovery-preview",
  );
}

function recoveryPreviewTender(tenderId: string) {
  return {
    tender_id: tenderId,
    name:
      tenderId === recoveryTenderIds[0]
        ? "Juhayna Recovery Tender"
        : "Annex Recovery Tender",
    revision: 0,
    phase: "intake" as const,
    needs_engineer: true,
    state: "recovery_required" as const,
    can_archive: false,
    can_delete: true,
    last_activity_at: null,
  };
}

function recoveryPreviewWorkspace(
  selectedTenderId?: string,
): ManagerWorkspaceProjection {
  const catalogue = recoveryTenderIds
    .filter((tenderId) => !recoveryPreviewHiddenTenderIds.has(tenderId))
    .map((tenderId) =>
      recoveryPreviewAppliedTenderIds.has(tenderId)
        ? {
            ...recoveryPreviewTender(tenderId),
            name: `Recovered Tender ${tenderId.slice(0, 8)}`,
            revision: 3,
            phase: "tender_planning" as const,
            needs_engineer: false,
            state: "active" as const,
            can_archive: true,
            can_delete: true,
            last_activity_at: "2026-08-20T11:21:00Z",
          }
        : recoveryPreviewTender(tenderId),
    );
  const selectedTender = catalogue.find(
    (tender) => tender.tender_id === selectedTenderId,
  );
  return {
    ...emptyWorkspace,
    catalogue,
    selected_tender: selectedTender?.state === "active" ? selectedTender : null,
    conversation:
      selectedTender?.state === "active"
        ? {
            conversation_id: "recovered-conversation",
            messages: [
              {
                message_id: "recovered-message",
                sequence: 1,
                author: "system",
                kind: "status",
                body: "Tender recovery was approved and the workspace is available.",
                created_at: "2026-08-20T11:21:00Z",
                references: [],
              },
            ],
            latest_meaningful_message_id: "recovered-message",
          }
        : null,
  };
}

function recoveryPreviewIntegrity(tenderId: string): TenderIntegrityReport {
  return {
    tender_id: tenderId,
    state: "recovery_required",
    issues: ["database_integrity_invalid", "audit_chain_invalid"],
    recovery_choices: ["restore_verified_backup", "purge_tender"],
  };
}

function recoveryPreviewBackup(tenderId: string): TenderBackupRecord {
  return {
    backup_id: recoveryBackupId,
    tender_id: tenderId,
    state: "ready",
    source: {
      tender_id: tenderId,
      name: `Tender ${tenderId.slice(0, 8)}`,
      revision: 3,
      lifecycle_phase: "intake",
      audit_event_count: 3n,
      audit_chain_head: "preview-backup-chain",
    },
    content_object_count: 4n,
    manifest_sha256: "d".repeat(64),
    archive_size_bytes: 4096n,
    diagnostic_code: null,
    created_at: "2026-08-20T11:00:00Z",
  };
}

let recoveryPreviewRecord: TenderRecoveryRecord | null = null;
const recoveryPreviewAppliedTenderIds = new Set<string>();
const recoveryPreviewHiddenTenderIds = new Set<string>();
let recoveryPreviewTrashRecord: TrashedTenderRecord | null = null;
let recoveryPreviewDeletionReceipt: DeletionReceipt | null = null;

function previewWorkspace(): ManagerWorkspaceProjection {
  const tender = {
    tender_id: previewTenderId,
    name: previewTenderName,
    revision: 4,
    phase: "tender_planning" as const,
    needs_engineer: true,
    state: "active" as const,
    can_archive: false,
    can_delete: false,
    last_activity_at: "2026-08-20T11:12:00Z",
  };
  return {
    catalogue: [tender],
    selected_tender: tender,
    conversation: {
      conversation_id: previewConversationId,
      messages: [...previewMessages],
      latest_meaningful_message_id:
        previewMessages[previewMessages.length - 1]?.message_id ?? null,
    },
    current_action: {
      kind: "configure_ai_provider",
      title: "Connect this Tender to AI",
      summary:
        "Choose the exact provider, model, and reasoning level for new Agent Runs. Existing records remain local and available.",
      action_label: "Set up AI & Models",
      requires_engineer: true,
    },
    work: {
      needs_engineer: 1,
      working: 1,
      waiting: 1,
      done: 1,
      cancelled: 0,
      failed: 0,
      tasks: [
        {
          production_task_id: "task-engineer-decision",
          task_id: "task-plan-review",
          task_key: "confirm_scope",
          objective: "Confirm the exclusions and commercial assumptions",
          state: "needs_engineer",
          status_detail:
            "The estimator needs an Engineer decision before pricing continues.",
          dependencies: ["clarification-07"],
          agent: {
            profile_id: "commercial-lead",
            profile_version: 1,
            identity: "Commercial Lead",
            profession: "Commercial management",
          },
          current_run_id: null,
          output_count: 1,
        },
        {
          production_task_id: "task-working",
          task_id: "task-document-review",
          task_key: "review_requirements",
          objective: "Review technical submission requirements",
          state: "working",
          status_detail: "Requirements are being traced to exact source pages.",
          dependencies: [],
          agent: {
            profile_id: "technical-reviewer",
            profile_version: 2,
            identity: "Technical Reviewer",
            profession: "Technical compliance",
          },
          current_run_id: "run-technical-review",
          output_count: 0,
        },
        {
          production_task_id: "task-waiting",
          task_id: "task-cost-plan",
          task_key: "prepare_cost_plan",
          objective: "Prepare the governed cost plan",
          state: "waiting",
          status_detail: "Waiting for the exact Tender AI selection.",
          dependencies: ["confirm_scope"],
          agent: null,
          current_run_id: null,
          output_count: 0,
        },
        {
          production_task_id: "task-done",
          task_id: "task-register-package",
          task_key: "register_package",
          objective: "Register the Tender Package",
          state: "done",
          status_detail: "The source package is registered and attributable.",
          dependencies: [],
          agent: {
            profile_id: "tendering-manager",
            profile_version: 1,
            identity: "Tendering Manager",
            profession: "Tender coordination",
          },
          current_run_id: null,
          output_count: 1,
        },
      ],
    },
    files: {
      tender_document_count: 2,
      quantix_output_count: 1,
      tender_documents: [
        {
          artifact_id: "source-instructions",
          version: 1,
          package_path: "01 Instructions/Instructions to Tenderers.pdf",
          document_type: "instructions_to_tenderers",
          media_type: "application/pdf",
          sha256: "b".repeat(64),
          size_bytes: 1843200n,
          registration_state: "registered",
          parse_state: "parsed",
          exception: null,
        },
        {
          artifact_id: "source-boq",
          version: 1,
          package_path: "03 Commercial/Bill of Quantities.xlsx",
          document_type: "bill_of_quantities",
          media_type:
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
          sha256: "c".repeat(64),
          size_bytes: 753664n,
          registration_state: "registered",
          parse_state: "parsed",
          exception: null,
        },
      ],
      quantix_outputs: [
        {
          artifact_id: "submission-requirements-register",
          version: 2,
          production_task_id: "task-register-package",
          author_run_id: "run-package-register",
          payload_sha256: "d".repeat(64),
          created_at: "2026-08-20T11:11:00Z",
        },
      ],
    },
    team: {
      active_agent_runs: 1,
      waiting_tasks: 1,
      needs_engineer: 1,
      events: [
        {
          message_id: "e".repeat(32),
          sequence: 1,
          author: "manager",
          kind: "finding",
          body: "The returnable schedule requires a signed commercial declaration.",
          created_at: "2026-08-20T11:14:00Z",
          references: [],
        },
        {
          message_id: "f".repeat(32),
          sequence: 2,
          author: "manager",
          kind: "handoff",
          body: "Technical requirements were handed to the compliance specialist with exact source references.",
          created_at: "2026-08-20T11:15:00Z",
          references: [],
        },
      ],
      agent_runs: [],
    },
    intake: null,
    ai_execution: {
      revision: 1n,
      selection: null,
      readiness: "local_only",
      status_summary:
        "No AI provider is selected; local-only Tender work remains available.",
    },
    capability_readiness: {
      state: "blocked",
      gaps: [
        {
          capability: "ai_execution",
          reason: "An exact Tender AI selection is required.",
          affected_work: ["prepare_cost_plan"],
        },
      ],
      blocker_codes: ["ai_selection_required"],
    },
    external_rfis: [],
    doctor_blockers: [
      {
        code: "ai_selection_required",
        area: "ai_execution",
        title: "Tender AI selection required",
        detail:
          "Local work is available; AI-required tasks wait without provider fallback.",
      },
    ],
  };
}

let preferences: GeneralApplicationPreferences = {
  appearance: "system",
  reduced_motion: false,
  larger_text: false,
  notify_when_attention_needed: false,
};

function applicationSettings(): ApplicationSettingsView {
  return {
    general_preferences: preferences,
    ai_execution_selection: null,
    ai_execution_approval: null,
    chatgpt: {
      state: "absent",
      account_id: null,
      plan_type: null,
      expires_at_ms: null,
      login_phase: "idle",
    },
    provider_connections: [],
    storage: {
      application_home: "Browser preview (no local files)",
      tender_backups_are_preserved: true,
      trash_requires_explicit_purge: true,
    },
    diagnostics: {
      quantix_version: "browser-preview",
      installation_schema_version: 24n,
      tender_schema_version: 35n,
    },
  };
}

function readPreferences(payload?: InvokeArgs): GeneralApplicationPreferences {
  const command = (payload as Record<string, unknown> | undefined)?.command as
    { preferences?: GeneralApplicationPreferences } | undefined;
  return command?.preferences ?? preferences;
}

export function invokeBrowserPreviewHost(
  command: string,
  payload?: InvokeArgs,
): unknown {
  if (!recoveryPreviewEnabled()) {
    recoveryPreviewAppliedTenderIds.clear();
    recoveryPreviewHiddenTenderIds.clear();
    recoveryPreviewRecord = null;
    recoveryPreviewTrashRecord = null;
    recoveryPreviewDeletionReceipt = null;
  }
  switch (command) {
    case "ensure_quantix_setup":
      return setupOutcome;
    case "check_quantix_update":
    case "validate_quantix_update_restart":
      return idleUpdateStatus;
    case "inspect_manager_workspace":
      if (recoveryPreviewEnabled()) return recoveryPreviewWorkspace();
      return workspacePreviewEnabled() ? previewWorkspace() : emptyWorkspace;
    case "select_manager_workspace_tender": {
      if (!recoveryPreviewEnabled()) {
        return workspacePreviewEnabled() ? previewWorkspace() : emptyWorkspace;
      }
      const command = (payload as Record<string, unknown> | undefined)
        ?.command as { tender_id?: string } | undefined;
      return recoveryPreviewWorkspace(command?.tender_id);
    }
    case "inspect_tender_integrity": {
      const command = (payload as Record<string, unknown> | undefined)
        ?.command as { tender_id?: string } | undefined;
      const tenderId = command?.tender_id ?? recoveryTenderIds[0];
      if (!recoveryPreviewEnabled()) {
        throw new Error(
          "Tender recovery is only available in the recovery preview.",
        );
      }
      return recoveryPreviewIntegrity(tenderId);
    }
    case "inspect_tender_backups": {
      const command = (payload as Record<string, unknown> | undefined)
        ?.command as { tender_id?: string } | undefined;
      const tenderId = command?.tender_id ?? recoveryTenderIds[0];
      if (!recoveryPreviewEnabled()) return [] satisfies TenderBackupRecord[];
      return [recoveryPreviewBackup(tenderId)];
    }
    case "inspect_tender_recoveries": {
      if (!recoveryPreviewEnabled()) return [] satisfies TenderRecoveryRecord[];
      const command = (payload as Record<string, unknown> | undefined)
        ?.command as { tender_id?: string } | undefined;
      return recoveryPreviewRecord &&
        recoveryPreviewRecord.tender_id === command?.tender_id
        ? [recoveryPreviewRecord]
        : [];
    }
    case "prepare_tender_recovery": {
      const command = (payload as Record<string, unknown> | undefined)
        ?.command as { tender_id?: string; backup_id?: string } | undefined;
      const tenderId = command?.tender_id ?? recoveryTenderIds[0];
      recoveryPreviewRecord = {
        recovery_id: recoveryRecordId,
        tender_id: tenderId,
        backup_id: command?.backup_id ?? recoveryBackupId,
        state: "awaiting_approval",
        backup_source: recoveryPreviewBackup(tenderId).source,
        current_source: null,
        diagnostic_code: "preview_database_integrity_invalid",
        decision_record: null,
        created_at: "2026-08-20T11:20:00Z",
      };
      return recoveryPreviewRecord;
    }
    case "resolve_tender_recovery": {
      const command = (payload as Record<string, unknown> | undefined)
        ?.command as
        | {
            tender_id?: string;
            recovery_id?: string;
            decision?: TenderRecoveryDecision;
            rationale?: string;
          }
        | undefined;
      if (!recoveryPreviewRecord) {
        throw new Error("No prepared recovery exists in the recovery preview.");
      }
      const decision = command?.decision ?? "reject";
      recoveryPreviewRecord = {
        ...recoveryPreviewRecord,
        state: decision === "approve_replacement" ? "applied" : "rejected",
        decision_record: {
          decision,
          rationale: command?.rationale ?? "Browser preview decision",
          decided_by: "Engineer",
          manifest_sha256: "d".repeat(64),
          current_audit_chain_head: null,
          decided_at: "2026-08-20T11:21:00Z",
        },
      };
      if (decision === "approve_replacement") {
        recoveryPreviewAppliedTenderIds.add(recoveryPreviewRecord.tender_id);
      }
      return recoveryPreviewRecord;
    }
    case "trash_recovery_required_tender": {
      const recoveryCommand = (payload as Record<string, unknown> | undefined)
        ?.command as { tender_id?: string; rationale?: string } | undefined;
      const tenderId = recoveryCommand?.tender_id ?? recoveryTenderIds[0];
      const tender = recoveryPreviewTender(tenderId);
      recoveryPreviewHiddenTenderIds.add(tenderId);
      recoveryPreviewTrashRecord = {
        deletion_id: "d".repeat(32),
        tender_id: tenderId,
        tender_name: tender.name,
        state: "trashed",
        relative_path: `${tenderId}-${"d".repeat(32)}`,
        rationale:
          recoveryCommand?.rationale ?? "Browser preview recovery deletion",
        decided_by: "engineer_user",
        acting_role: "tendering_engineer",
        approval_manifest_sha256: "a".repeat(64),
        diagnostic_code: null,
        created_at: "2026-08-20T11:25:00Z",
        updated_at: "2026-08-20T11:25:00Z",
        deletion_source: "recovery_required",
        integrity_code: "recovery_required",
        provider_reference_discovery: "incomplete",
      } as TrashedTenderRecord;
      return recoveryPreviewTrashRecord;
    }
    case "restore_trashed_tender": {
      if (!recoveryPreviewTrashRecord) {
        throw new Error("No recovery Tender is currently in Trash.");
      }
      const restored = {
        ...recoveryPreviewTrashRecord,
        state: "restored" as const,
        updated_at: "2026-08-20T11:26:00Z",
      };
      recoveryPreviewHiddenTenderIds.delete(restored.tender_id);
      recoveryPreviewTrashRecord = null;
      return restored;
    }
    case "purge_recovery_required_tender": {
      const recoveryCommand = (payload as Record<string, unknown> | undefined)
        ?.command as
        | {
            tender_id?: string;
            rationale?: string;
            confirmation_tender_name?: string;
          }
        | undefined;
      const tenderId = recoveryCommand?.tender_id ?? recoveryTenderIds[0];
      const tenderName = recoveryPreviewTender(tenderId).name;
      if (recoveryCommand?.confirmation_tender_name !== tenderName) {
        throw new Error("The Tender name confirmation does not match.");
      }
      recoveryPreviewHiddenTenderIds.add(tenderId);
      recoveryPreviewTrashRecord = null;
      recoveryPreviewDeletionReceipt = {
        receipt_id: "e".repeat(32),
        deletion_id: "d".repeat(32),
        tender_id: tenderId,
        audit_event_count: 0n,
        audit_chain_head: "",
        local_deletion_completed: true,
        erased_copy_classes: [
          "tender_store",
          "tender_backup",
          "portable_tender_archive",
          "delivery_export",
          "agent_run_workspace",
          "staging_item",
          "quarantine_item",
          "tender_log",
        ],
        provider_cleanup_status: "incomplete",
        provider_thread_count: 0,
        confirmed_provider_thread_deletions: 0,
        external_copy_exclusions: ["original_tender_package"],
        purged_by: "engineer_user",
        acting_role: "tendering_engineer",
        purged_at: "2026-08-20T11:27:00Z",
        manifest_sha256: "f".repeat(64),
        deletion_source: "recovery_required",
        integrity_code: "recovery_required",
        provider_reference_discovery: "incomplete",
      } as DeletionReceipt;
      return recoveryPreviewDeletionReceipt;
    }
    case "search_manager_workspace":
      if (workspacePreviewEnabled()) {
        const query = String(
          (
            (payload as Record<string, unknown> | undefined)?.command as
              { query?: string } | undefined
          )?.query ?? "",
        );
        return {
          query,
          groups: [
            {
              kind: "conversation",
              hits: [
                {
                  kind: "conversation",
                  reference: "manager-message:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                  version: null,
                  title: "Tender AI is waiting",
                  detail:
                    "Local work remains available without provider fallback.",
                },
              ],
            },
            {
              kind: "work",
              hits: [
                {
                  kind: "work",
                  reference: "production-task:task-waiting",
                  version: 1,
                  title: "Prepare the governed cost plan",
                  detail: "Waiting for the exact Tender AI selection.",
                },
              ],
            },
            {
              kind: "files",
              hits: [
                {
                  kind: "files",
                  reference: "artifact:source-instructions",
                  version: 1,
                  title: "Instructions to Tenderers.pdf",
                  detail: "01 Instructions/Instructions to Tenderers.pdf",
                },
              ],
            },
            { kind: "evidence", hits: [] },
            { kind: "agents", hits: [] },
          ],
        } satisfies WorkspaceSearchProjection;
      }
      return {
        query: "",
        groups: (
          ["conversation", "work", "files", "evidence", "agents"] as const
        ).map((kind) => ({ kind, hits: [] })),
      } satisfies WorkspaceSearchProjection;
    case "inspect_quantix_doctor":
      return browserDoctor;
    case "inspect_package_intake_progress":
      return null as PackageIntakeProgress | null;
    case "cancel_package_intake":
      return false;
    case "start_manager_tender":
    case "choose_and_import_tender_package":
      // The native folder picker is intentionally unavailable in a browser.
      // Returning null has the same semantics as cancelling that picker.
      return null;
    case "record_engineer_workspace_message": {
      const command = (payload as Record<string, unknown> | undefined)
        ?.command as { body?: string } | undefined;
      const body = command?.body?.trim();
      if (!workspacePreviewEnabled() || !body) return previewWorkspace();
      previewMessageSequence += 1;
      previewMessages.push({
        message_id: `preview-message-${previewMessageSequence}`,
        sequence: previewMessageSequence,
        author: "engineer",
        kind: "routine",
        body,
        created_at: new Date().toISOString(),
        references: [],
      });
      return previewWorkspace();
    }
    case "revise_tender": {
      const command = (payload as Record<string, unknown> | undefined)
        ?.command as { name?: string } | undefined;
      if (workspacePreviewEnabled() && command?.name?.trim()) {
        previewTenderName = command.name.trim();
      }
      return {
        tender_id: previewTenderId,
        name: previewTenderName,
        revision: 5,
        lifecycle_phase: "tender_planning",
        audit_event_count: 5n,
        audit_chain_head: "preview-audit-chain",
      };
    }
    case "inspect_application_settings":
    case "refresh_application_settings":
      return applicationSettings();
    case "update_general_application_preferences":
      preferences = readPreferences(payload);
      return applicationSettings();
    case "inspect_document_tool_readiness":
    case "prepare_document_tools":
      return readyDocumentTools;
    case "inspect_trashed_tenders":
      return recoveryPreviewTrashRecord ? [recoveryPreviewTrashRecord] : [];
    case "inspect_deletion_receipts":
      return recoveryPreviewDeletionReceipt
        ? [recoveryPreviewDeletionReceipt]
        : [];
    case "cancel_document_tool_preparation":
      return false;
    case "notify_startup_display_ready":
    case "report_startup_splash_preferences":
    case "resume_manager_intakes":
      return undefined;
    default:
      throw new Error(
        `The browser preview does not implement the native command "${command}". Open Quantix with \`npm run tauri dev\` to use local files and operating-system integrations.`,
      );
  }
}

export async function installBrowserPreviewHost(): Promise<void> {
  const { mockIPC } = await import("@tauri-apps/api/mocks");
  mockIPC(invokeBrowserPreviewHost, { shouldMockEvents: true });
  document.documentElement.dataset.quantixRuntime = "browser-preview";
}
