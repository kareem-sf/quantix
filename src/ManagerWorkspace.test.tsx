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
import type { AgentRunHistoryPage } from "./bindings/AgentRunHistoryPage";
import type { AgentRunInspection } from "./bindings/AgentRunInspection";
import type { AgentRunSummary } from "./bindings/AgentRunSummary";
import type { ApplicationSettingsView } from "./bindings/ApplicationSettingsView";
import type { BasisOfEstimateVersion } from "./bindings/BasisOfEstimateVersion";
import type { CalculationWorkspaceInspection } from "./bindings/CalculationWorkspaceInspection";
import type { ChangeAssessment } from "./bindings/ChangeAssessment";
import type { ChangeAssessmentPage } from "./bindings/ChangeAssessmentPage";
import type { EstimateWorkspaceInspection } from "./bindings/EstimateWorkspaceInspection";
import type { ExternalRfiDraft } from "./bindings/ExternalRfiDraft";
import type { EvidenceDocument } from "./bindings/EvidenceDocument";
import type { EvidenceLocation } from "./bindings/EvidenceLocation";
import type { ProductionTaskReviewInspection } from "./bindings/ProductionTaskReviewInspection";
import type { TenderProductionInspection } from "./bindings/TenderProductionInspection";
import type { TenderQuery } from "./bindings/TenderQuery";
import type { TenderQueryPage } from "./bindings/TenderQueryPage";
import type { TenderOfficeMessage } from "./bindings/TenderOfficeMessage";
import type { TenderRecordInspection } from "./bindings/TenderRecordInspection";
import type { WorkPlanProposalInspection } from "./bindings/WorkPlanProposalInspection";
import type { WorkspaceMessageReference } from "./bindings/WorkspaceMessageReference";
import type { WorkspaceTaskRow } from "./bindings/WorkspaceTaskRow";

const host = vi.hoisted(() => ({
  archiveTender: vi.fn(),
  approveBasisOfEstimate: vi.fn(),
  approveProductionFindingException: vi.fn(),
  approveExternalRfiForIssue: vi.fn(),
  cancelChatGptLogin: vi.fn(),
  cancelRuntimePreparation: vi.fn(),
  chooseAndImportTenderPackage: vi.fn(),
  createBidDecisionPackage: vi.fn(),
  createExternalRfiDraft: vi.fn(),
  decideBidDecisionPackage: vi.fn(),
  decideChangeAssessment: vi.fn(),
  decideTenderQueryTreatment: vi.fn(),
  decideTenderRecord: vi.fn(),
  exportApprovedExternalRfi: vi.fn(),
  inspectArtifactVersions: vi.fn(),
  inspectBidDecisionApprovalHistory: vi.fn(),
  inspectBidDecisionPackageRecords: vi.fn(),
  inspectChangeAssessments: vi.fn(),
  inspectComplianceMatrix: vi.fn(),
  inspectCalculationWorkspace: vi.fn(),
  inspectEstimateWorkspace: vi.fn(),
  inspectCurrentBidDecisionPackage: vi.fn(),
  inspectEvidence: vi.fn(),
  inspectExternalRfis: vi.fn(),
  inspectExternalRfiEligibleQueries: vi.fn(),
  inspectExternalRfiResponseCandidates: vi.fn(),
  inspectProductionTaskReview: vi.fn(),
  inspectTenderQueries: vi.fn(),
  inspectTenderRecord: vi.fn(),
  interpretExternalRfiResponse: vi.fn(),
  interruptAgentRun: vi.fn(),
  invalidateBidDecisionApproval: vi.fn(),
  registerExternalRfiResponse: vi.fn(),
  resolveBidDecisionReturnRework: vi.fn(),
  reviseExternalRfiDraft: vi.fn(),
  runBidDecisionPackageReview: vi.fn(),
  runBasisOfEstimateReview: vi.fn(),
  runExternalRfiReview: vi.fn(),
  runProductionTask: vi.fn(),
  searchEvidence: vi.fn(),
  composeTenderOffice: vi.fn(),
  inspectCurrentWorkPlan: vi.fn(),
  inspectTenderProduction: vi.fn(),
  createTenderBackup: vi.fn(),
  inspectStartupReconciliation: vi.fn(),
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
  inspectAgentRunHistory: vi.fn(),
  inspectDiagnosticTimeline: vi.fn(),
  inspectDiagnosticsStatus: vi.fn(),
  inspectTrashedTenders: vi.fn(),
  inspectRuntimeReadiness: vi.fn(),
  inspectRuntimePreparationProgress: vi.fn(),
  disconnectChatGpt: vi.fn(),
  rebindManagerIntakeProvider: vi.fn(),
  recordEngineerWorkspaceMessage: vi.fn(),
  inspectAiProviders: vi.fn(),
  refreshApplicationSettings: vi.fn(),
  removeAiProvider: vi.fn(),
  saveAiProvider: vi.fn(),
  setActiveAiProvider: vi.fn(),
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
    installation_schema_version: 25n,
    tender_schema_version: 36n,
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
  external_rfis: [],
  estimate: null,
  pricing: null,
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

const planBudget = {
  provider_turns: 1,
  duration_seconds: 120,
  output_bytes: 262_144,
};

function planProfile(
  profileId: string,
  archetype: string,
  identity: string,
  profession: string,
) {
  return {
    archetype,
    status: "proposed" as const,
    profile: {
      profile_id: profileId,
      version: 1,
      identity,
      profession,
      seniority: "senior",
      capabilities: ["cost_estimation"],
      objective: "Deliver the verified Tender output for this specialty.",
      behavior: "Works only from verified records and exact inputs.",
      skepticism: "Challenges every unsupported input.",
      risk_tolerance: "low",
      instructions:
        "Deliver the verified Tender output for this specialty. Works only from verified records and exact inputs. Challenges every unsupported input.",
      output_contract_json: "{}",
      review_policy: "independent_review",
      permissions: {
        data_scopes: ["tender_analysis"],
        data_classifications: ["tender_internal" as const],
        allowed_actions: ["read"],
        allowed_tools: [],
        network_allowed: false,
        workspace_write_allowed: true,
      },
      prohibited_actions: ["approve_own_work"],
      resource_budget: planBudget,
    },
  };
}

function workPlanInspection(): WorkPlanProposalInspection {
  const estimatorProfileId = "e".repeat(32);
  const reviewerProfileId = "f".repeat(32);
  return {
    plan_id: "p".repeat(32),
    version: 2,
    bid_package_id: "k".repeat(32),
    bid_package_version: 1,
    bid_package_manifest_sha256: "1".repeat(64),
    capability_catalogue_version: 1,
    permission_policy_version: 1,
    profiles: [
      planProfile(
        estimatorProfileId,
        "cost_estimator",
        "Cost Estimator",
        "Senior Construction Cost Estimator",
      ),
      planProfile(
        reviewerProfileId,
        "independent_cost_reviewer",
        "Independent Cost Reviewer",
        "Senior Cost Assurance Surveyor",
      ),
    ],
    workstreams: [
      {
        workstream_key: "cost_estimation",
        name: "Cost Estimating",
        capability: "cost_estimation",
        accountable_profile_id: estimatorProfileId,
        dependencies: ["tender_analysis"],
        deadlines: ["2026-10-01T00:00:00Z"],
        milestones: ["cost_estimation_ready"],
        resource_budget: planBudget,
      },
    ],
    tasks: [
      {
        task_key: "cost_estimation_production",
        workstream_key: "cost_estimation",
        profile_id: estimatorProfileId,
        profile_version: 1,
        objective: "Develop the evidence-linked estimate.",
        exact_inputs: [],
        dependencies: [],
        deadline: "2026-10-01T00:00:00Z",
        milestone: "cost_estimation_ready",
        review_profile_id: reviewerProfileId,
        review_profile_version: 1,
        major_finding_policy: "remediation_required" as const,
        permissions: {
          data_scopes: ["tender_analysis"],
          data_classifications: ["tender_internal" as const],
          allowed_actions: ["read"],
          allowed_tools: [],
          network_allowed: false,
          workspace_write_allowed: true,
        },
        resource_budget: planBudget,
        output_contract_json: "{}",
      },
    ],
    outcome: [
      {
        workstream: "Cost Estimating",
        milestone: "cost_estimation_ready",
        deadline: "2026-10-01T00:00:00Z",
      },
    ],
    risks: [
      {
        record_id: "r".repeat(32),
        version: 1,
        title: "Ground conditions may differ from the survey.",
      },
    ],
    assumptions: [
      {
        record_id: "s".repeat(32),
        version: 1,
        title: "Site access is available from day one.",
      },
    ],
    query_bindings: [],
    capability_gaps: [],
    blocker_codes: [],
    revision_actions: [
      {
        action: "add_profile" as const,
        archetype: "cost_estimator",
        identity: "Cost Estimator",
      },
    ],
    approval: null,
    current: true,
    created_by: "engineer_user",
    created_at: "2026-08-30T09:00:00Z",
    manifest_sha256: "2".repeat(64),
  };
}

function productionInspection(
  plan: WorkPlanProposalInspection,
): TenderProductionInspection {
  return {
    activation_id: "t".repeat(32),
    plan_id: plan.plan_id,
    plan_version: plan.version,
    plan_manifest_sha256: plan.manifest_sha256,
    active: true,
    tasks: [],
    activated_by: "engineer_user",
    acting_role: "tendering_manager",
    created_at: "2026-08-30T09:05:00Z",
  };
}

function workTask(overrides: Partial<WorkspaceTaskRow> = {}): WorkspaceTaskRow {
  return {
    production_task_id: "task-default",
    task_id: "2".repeat(32),
    task_key: "cost_estimation_production",
    objective: "Develop the evidence-linked estimate.",
    state: "waiting",
    status_detail: "ready",
    dependencies: [],
    agent: {
      profile_id: "e".repeat(32),
      profile_version: 1,
      identity: "Cost Estimator",
      profession: "Senior Construction Cost Estimator",
    },
    current_run_id: null,
    output_count: 0,
    ...overrides,
  };
}

function workTasksProjection(
  tasks: WorkspaceTaskRow[],
): ManagerWorkspaceProjection {
  return {
    ...projection,
    selected_tender: {
      ...projection.selected_tender!,
      phase: "active_production" as const,
      needs_engineer: false,
    },
    work: {
      needs_engineer: 1,
      working: 1,
      waiting: 2,
      done: 1,
      cancelled: 0,
      failed: 1,
      tasks,
    },
  };
}

function sixStateTasks(): WorkspaceTaskRow[] {
  return [
    workTask({
      production_task_id: "task-waiting-blocked",
      task_key: "risk_review",
      objective: "Review the ground risks.",
      state: "waiting",
      status_detail: "blocked",
      dependencies: ["cost_estimation_production"],
    }),
    workTask({
      production_task_id: "task-waiting-ready",
      task_key: "cost_estimation_production",
      objective: "Develop the evidence-linked estimate.",
      state: "waiting",
      status_detail: "ready",
    }),
    workTask({
      production_task_id: "task-working-running",
      task_key: "boq_takeoff",
      objective: "Take off the BOQ quantities.",
      state: "working",
      status_detail: "running",
      current_run_id: "5".repeat(32),
    }),
    workTask({
      production_task_id: "task-needs-engineer",
      task_key: "estimate_review",
      objective: "Review the estimate.",
      state: "needs_engineer",
      status_detail: "review_ready",
      dependencies: ["boq_takeoff"],
    }),
    workTask({
      production_task_id: "task-paused",
      task_key: "pricing_scenario",
      objective: "Price the tender scenarios.",
      state: "paused",
      status_detail: "suspended",
    }),
    workTask({
      production_task_id: "task-done",
      task_key: "tender_analysis",
      objective: "Analyse the tender package.",
      state: "done",
      status_detail: "ready_for_integration",
      output_count: 1,
    }),
    workTask({
      production_task_id: "task-failed",
      task_key: "compliance_check",
      objective: "Check the compliance matrix.",
      state: "failed",
      status_detail: "failed",
    }),
  ];
}

const focusedProductionTaskId = "task-working-running";

const rfiQueryRef = {
  query_id: "1".repeat(32),
  version: 1,
  manifest_sha256: "2".repeat(64),
};

const rfiQuestion =
  "Who carries the installation responsibility for the pumphouse works?";

function eligibleQueryPage() {
  return {
    items: [
      {
        query_ref: rfiQueryRef,
        question: rfiQuestion,
        ambiguity_or_gap:
          "The cited wording leaves the responsibility boundary unresolved.",
        due_at: "2030-01-01T00:00:00Z",
        affected_task_keys: ["cost_estimation_production"],
      },
    ],
    next_cursor: null,
    total_count: 1,
  };
}

function rfiDraftFixture(
  overrides: Partial<ExternalRfiDraft> = {},
): ExternalRfiDraft {
  return {
    rfi_id: "4".repeat(32),
    version: 1,
    query_refs: [rfiQueryRef],
    current_query_refs: [rfiQueryRef],
    questions: [
      {
        query_id: rfiQueryRef.query_id,
        query_version: 1,
        question: rfiQuestion,
        ambiguity_or_gap:
          "The cited wording leaves the responsibility boundary unresolved.",
      },
    ],
    source_evidence: [
      { kind: "source_evidence", reference: `${"5".repeat(32)}#1`, version: 1 },
    ],
    contractual_context:
      "The tender documents leave the responsibility boundary unresolved.",
    response_need: "Confirm the responsible party before pricing.",
    attachments: [],
    due_at: "2030-01-01T00:00:00Z",
    recipient: {
      organization: "Employer Procurement Team",
      attention: "Tender Clarifications Manager",
      email: null,
    },
    affected_task_keys: ["cost_estimation_production"],
    affected_commitments: ["Tender price qualification"],
    review: null,
    approval: null,
    exports: [],
    responses: [],
    interpretations: [],
    current: true,
    evidence_current: true,
    revision_allowed: true,
    approved_for_issue: false,
    manifest_sha256: "3".repeat(64),
    created_at: "2026-08-30T09:00:00Z",
    ...overrides,
  };
}

function rfiReviewFixture() {
  return {
    review_id: "8".repeat(32),
    rfi_id: "4".repeat(32),
    rfi_version: 1,
    rfi_manifest_sha256: "3".repeat(64),
    reviewer_run_id: "9".repeat(32),
    reviewer_profile_id: "f".repeat(32),
    reviewer_profile_version: 1,
    outcome: "passed" as const,
    findings: [],
    manifest_sha256: "b".repeat(64),
    created_at: "2026-08-30T10:00:00Z",
  };
}

function rfiApprovalFixture() {
  return {
    approval_id: "c".repeat(32),
    rfi_id: "4".repeat(32),
    rfi_version: 1,
    rfi_manifest_sha256: "3".repeat(64),
    review_id: "8".repeat(32),
    review_manifest_sha256: "b".repeat(64),
    rationale: "Approved exact wording.",
    approved_by: "engineer_user",
    acting_role: "tendering_manager",
    approval_sha256: "d".repeat(64),
    created_at: "2026-08-30T11:00:00Z",
  };
}

function rfiSummary(
  overrides: Partial<ManagerWorkspaceProjection["external_rfis"][number]> = {},
) {
  return {
    rfi_id: "4".repeat(32),
    version: 1,
    status: "awaiting_review" as const,
    question_count: 1,
    response_count: 0,
    approval_pending: false,
    export_pending: false,
    interpretation_pending: false,
    ...overrides,
  };
}

function rfiProjection(
  currentAction: ManagerWorkspaceProjection["current_action"],
  externalRfis: ManagerWorkspaceProjection["external_rfis"],
): ManagerWorkspaceProjection {
  return {
    ...projection,
    current_action: currentAction,
    external_rfis: externalRfis,
  };
}

function focusedProductionInspection(): TenderProductionInspection {
  return {
    ...productionInspection(workPlanInspection()),
    tasks: [
      {
        production_task_id: focusedProductionTaskId,
        plan_manifest_sha256: "2".repeat(64),
        task: {
          task_key: "boq_takeoff",
          workstream_key: "cost_estimation",
          profile_id: "e".repeat(32),
          profile_version: 1,
          objective: "Take off the BOQ quantities.",
          exact_inputs: [
            {
              kind: "source_evidence",
              reference: `${evidenceArtifactId}#7`,
              version: 2,
            },
          ],
          dependencies: [],
          deadline: "2026-10-01T00:00:00Z",
          milestone: "boq_takeoff_ready",
          review_profile_id: "f".repeat(32),
          review_profile_version: 1,
          major_finding_policy: "remediation_required" as const,
          permissions: {
            data_scopes: ["tender_analysis"],
            data_classifications: ["tender_internal" as const],
            allowed_actions: ["read"],
            allowed_tools: [],
            network_allowed: false,
            workspace_write_allowed: true,
          },
          resource_budget: planBudget,
          output_contract_json: "{}",
        },
        state: "running",
        run_ids: ["5".repeat(32)],
        artifact_version_count: 1,
        review_count: 0,
        finding_count: 0,
        open_blocking_finding_count: 0,
        latest_artifact: null,
        latest_review_result: null,
        query_control_available: false,
        ready_for_integration: false,
        created_at: "2026-08-30T09:10:00Z",
        updated_at: "2026-08-30T10:00:00Z",
      },
    ],
  };
}

function focusedTaskReview(): ProductionTaskReviewInspection {
  return {
    production_task_id: focusedProductionTaskId,
    artifact_versions: [
      {
        payload: {
          summary: "Ground-floor slab takeoff on a C30/37 basis.",
          evidence_references: [`${evidenceArtifactId}#7`],
          gaps: [],
          coordination_observations: [],
        },
        artifact_id: "4".repeat(32),
        version: 1,
        author_run_id: "7".repeat(32),
        prior_version: null,
        remediation_review_id: null,
        payload_sha256: "d".repeat(64),
        output_validation_passed: true,
        evidence_verified: true,
        data_scopes: ["tender_analysis"],
        data_classifications: ["tender_internal" as const],
        created_at: "2026-08-30T10:00:00Z",
      },
    ],
    reviews: [],
    readiness: null,
  };
}

const evidenceArtifactId = "8".repeat(32);
const conflictingArtifactId = "7".repeat(32);
const citedRecordId = "6".repeat(32);
const citedQueryId = "5".repeat(32);

function evidenceLocation(ordinal: number, text: string): EvidenceLocation {
  return {
    ordinal,
    kind: "paragraph",
    structural_path: `pdf/page-3/paragraph-${ordinal}`,
    provenance: [
      {
        page_number: 3,
        char_start: 0,
        char_end: text.length,
        bounding_box: null,
      },
    ],
    section: "Concrete works",
    paragraph_number: ordinal,
    table_number: null,
    sheet_name: null,
    cell_range: null,
    original_text: text,
    translated_text: null,
    language: "english",
    direction: "left_to_right",
  };
}

function instructionsDocument(): EvidenceDocument {
  return {
    artifact_id: evidenceArtifactId,
    version: 2,
    state: "parsed",
    exception: null,
    language: "english",
    direction: "left_to_right",
    pipeline_version: "pipeline-v1",
    markdown_sha256: "b".repeat(64),
    locations: [
      evidenceLocation(7, "Ground-floor slabs shall be concrete class C30/37."),
      evidenceLocation(8, "Concrete cover shall be 40 mm to reinforcement."),
    ],
  };
}

function addendumDocument(): EvidenceDocument {
  return {
    artifact_id: conflictingArtifactId,
    version: 1,
    state: "parsed",
    exception: null,
    language: "english",
    direction: "left_to_right",
    pipeline_version: "pipeline-v1",
    markdown_sha256: "c".repeat(64),
    locations: [
      evidenceLocation(2, "Amend ground-floor slabs to concrete class C35/45."),
    ],
  };
}

function proposedRecord(): TenderRecordInspection {
  return {
    record_id: citedRecordId,
    stable_key: "slab-concrete-strength",
    version: 1,
    kind: "requirement",
    title: "Slab concrete strength",
    verification_status: "proposed",
    trust_class: "ai_proposal",
    fields: [
      {
        name: "concrete_class",
        value: "C30/37",
        basis_kind: "evidence",
        basis_reference: null,
        basis_description: null,
        basis_authority: null,
        original_expression: null,
        normalized_value: null,
        timezone: null,
        uncertainty: null,
        evidence: [
          {
            reference: {
              artifact_id: evidenceArtifactId,
              version: 2,
              ordinal: 7,
            },
            package_path: "01 Instructions/ITT.pdf",
            location: evidenceLocation(
              7,
              "Ground-floor slabs shall be concrete class C30/37.",
            ),
          },
        ],
      },
    ],
    generation_instruction: null,
    contradictions: [
      {
        field_name: "concrete_class",
        summary: "The addendum states C35/45 for the same slab.",
        evidence: [
          {
            reference: {
              artifact_id: evidenceArtifactId,
              version: 2,
              ordinal: 7,
            },
            package_path: "01 Instructions/ITT.pdf",
            location: evidenceLocation(
              7,
              "Ground-floor slabs shall be concrete class C30/37.",
            ),
          },
          {
            reference: {
              artifact_id: conflictingArtifactId,
              version: 1,
              ordinal: 2,
            },
            package_path: "02 Addenda/Addendum-1.pdf",
            location: evidenceLocation(
              2,
              "Amend ground-floor slabs to concrete class C35/45.",
            ),
          },
        ],
      },
    ],
    source_relationships: [],
    reviews: [],
    author_run_id: "1".repeat(32),
    author_profile_id: "2".repeat(32),
    created_at: "2026-08-30T09:30:00Z",
  };
}

function decidedRecord(): TenderRecordInspection {
  return {
    ...proposedRecord(),
    verification_status: "verified",
    trust_class: "engineer_verified",
    reviews: [
      {
        review_id: "4".repeat(32),
        outcome: "verified",
        rationale: "Confirmed against the instructions.",
        reviewer_kind: "engineer_user",
        reviewer_run_id: null,
        decided_by: "engineer_user",
        created_at: "2026-08-30T10:30:00Z",
      },
    ],
  };
}

function pendingQuery(): TenderQuery {
  return {
    query_id: citedQueryId,
    version: 1,
    query_type: "contradiction",
    question: "Which concrete class applies to the ground-floor slab?",
    ambiguity_or_gap: "The addendum contradicts the instructions.",
    owner_profile_id: "3".repeat(32),
    owner_profile_version: 1,
    evidence: [
      {
        kind: "source_evidence",
        reference: `${evidenceArtifactId}#7`,
        version: 2,
      },
    ],
    affected_records: [{ record_id: citedRecordId, version: 1 }],
    affected_task_keys: ["cost_estimation_production"],
    due_at: "2026-09-30T00:00:00Z",
    material: true,
    release_blocking: false,
    proposed_treatments: [
      {
        treatment: "approved_assumption",
        rationale: "Carry the addendum value as a stated assumption.",
        proposed_by: "engineer_user",
        proposed_by_run_id: null,
      },
    ],
    responses: [],
    approved_treatment: null,
    invalidations: [],
    status: "treatment_proposed",
    overdue: false,
    current: true,
    source_run_id: null,
    created_by: "engineer_user",
    manifest_sha256: "a".repeat(64),
    created_at: "2026-08-30T10:05:00Z",
  };
}

function emptyQueryPage(): TenderQueryPage {
  return {
    query_register_open: true,
    owner_profiles: [],
    production_task_keys: [],
    items: [],
    next_cursor: null,
    total_current_count: 0,
    overdue_count: 0,
    release_blocking_count: 0,
  };
}

const addendumAssessmentId = "9".repeat(32);
const addendumAssessmentManifest = "d".repeat(64);

function addendumAssessment(): ChangeAssessment {
  return {
    assessment_sequence: 1,
    assessment_id: addendumAssessmentId,
    relationship_id: "e".repeat(32),
    relationship_kind: "addendum",
    prior_source: {
      artifact_id: evidenceArtifactId,
      version: 2,
      package_path: "01 Instructions/ITT.pdf",
      document_type: "pdf_document",
      sha256: "a".repeat(64),
      evidence_count: 1,
      evidence_preview: [],
    },
    replacement_source: {
      artifact_id: conflictingArtifactId,
      version: 1,
      package_path: "02 Addenda/Addendum-1.pdf",
      document_type: "pdf_document",
      sha256: "c".repeat(64),
      evidence_count: 0,
      evidence_preview: [],
    },
    lifecycle_before: "bid_decision",
    status: "pending",
    baseline_id: null,
    baseline_version: null,
    baseline_manifest_sha256: null,
    impacts: [
      {
        kind: "tender_record",
        object_id: citedRecordId,
        object_version: 1,
        dependencies: [],
        consequence: "stale",
        summary:
          "Current requirement record 'Slab concrete strength' cites the prior source.",
      },
      {
        kind: "production_task",
        object_id: "k".repeat(32),
        object_version: 0,
        dependencies: [],
        consequence: "reopen",
        summary:
          "Production task 'cost_estimation_production' consumes the affected exact package/input.",
      },
    ],
    affected_commitments: [],
    proposed_rework: [
      "Re-extract and re-verify only Tender Records bound to the superseded source Evidence.",
    ],
    unchanged_scope: [
      "3 current Tender Records have no typed dependency on the prior source.",
    ],
    deadline_effect:
      "One or more exact deadline commitments depend on the prior Source Artifact Version and must be revalidated before baseline approval.",
    approval_consequences: [],
    decision: null,
    resolution_baseline_id: null,
    resolution_baseline_version: null,
    manifest_sha256: addendumAssessmentManifest,
    created_at: "2026-08-30T12:00:00Z",
  };
}

function changeAssessmentPage(): ChangeAssessmentPage {
  const assessment = addendumAssessment();
  return {
    active: assessment,
    items: [assessment],
    next_before_sequence: null,
  };
}

function reviewChangeAction(): ManagerWorkspaceProjection["current_action"] {
  return {
    kind: "review_change",
    title: "Review the Tender change",
    summary: "A source change needs an impact decision before work continues.",
    action_label: "Review change",
    requires_engineer: true,
  };
}

function changeSummaryMessage() {
  return {
    message_id: "m".repeat(32),
    sequence: 2,
    author: "manager" as const,
    kind: "finding" as const,
    body: "A new addendum for '01 Instructions/ITT.pdf' arrived and was registered as '02 Addenda/Addendum-1.pdf'. The earlier version of the document stays preserved unchanged. It makes 1 tender record out of date, and the affected work needs targeted rework before the bid can continue. I need your decision on this change before work continues.",
    created_at: "2026-08-30T12:00:00Z",
    references: [],
  };
}

function questionProjection(): ManagerWorkspaceProjection {
  return {
    ...projection,
    current_action: {
      kind: "answer_manager_question",
      title: "Answer the Manager's question",
      summary:
        "Your answer will become an attributable Engineer input for the Tender intake.",
      action_label: "Reply to Manager",
      requires_engineer: true,
    },
    conversation: {
      conversation_id: "c".repeat(32),
      latest_meaningful_message_id: "m".repeat(32),
      messages: [
        {
          message_id: messageId,
          sequence: 1,
          author: "system",
          kind: "status",
          body: "West Campus MEP workspace is ready.",
          created_at: "2026-08-30T09:00:00Z",
          references: [],
        },
        {
          message_id: "m".repeat(32),
          sequence: 2,
          author: "manager",
          kind: "question",
          body: "Which concrete class applies to the ground-floor slab?",
          created_at: "2026-08-30T10:00:00Z",
          references: [
            {
              kind: "tender_record",
              reference: citedRecordId,
              version: 1,
              evidence_ordinal: null,
              label: "Slab concrete strength",
              detail: "Proposed requirement",
            },
            {
              kind: "source_evidence",
              reference: evidenceArtifactId,
              version: 2,
              evidence_ordinal: 7,
              label: "01 Instructions/ITT.pdf",
              detail: "Concrete class passage",
            },
          ],
        },
      ],
    },
  };
}

function teamRoomMessage(
  overrides: Partial<TenderOfficeMessage> & {
    message_id: string;
    sequence: number;
  },
): TenderOfficeMessage {
  return {
    author: "manager",
    kind: "routine",
    body: "Room message.",
    created_at: new Date().toISOString(),
    references: [],
    ...overrides,
  };
}

function teamRoomMessageReference(
  overrides: Partial<WorkspaceMessageReference> & { reference: string },
): WorkspaceMessageReference {
  return {
    kind: "tender_record",
    version: 1,
    evidence_ordinal: null,
    label: "Exact reference",
    detail: null,
    ...overrides,
  };
}

function daysAgoIso(days: number): string {
  const date = new Date();
  date.setDate(date.getDate() - days);
  date.setHours(10, 0, 0, 0);
  return date.toISOString();
}

function teamRoomProjection(): ManagerWorkspaceProjection {
  const handoffMessage = teamRoomMessage({
    message_id: "msg-handoff",
    sequence: 2,
    kind: "handoff",
    body: "Handing the lift-pit dimensions to the estimator.",
    created_at: daysAgoIso(3),
    references: [
      teamRoomMessageReference({
        kind: "tender_task",
        reference: "task-0000000000000000000000000000000",
        label: "Produce the verified estimate.",
      }),
    ],
  });
  const outputMessage = teamRoomMessage({
    message_id: "msg-output",
    sequence: 3,
    kind: "output",
    body: "Cost estimate v2 is recorded and ready for your review.",
    created_at: daysAgoIso(1),
    references: [
      teamRoomMessageReference({
        kind: "artifact_version",
        reference: "artifact-000000000000000000000000000",
        version: 2,
        label: "Cost estimate v2",
      }),
    ],
  });
  const blockerMessage = teamRoomMessage({
    message_id: "msg-blocker",
    sequence: 5,
    kind: "blocker",
    body: "The pumphouse duty pump schedule is missing.",
  });
  const questionMessage = teamRoomMessage({
    message_id: "msg-question",
    sequence: 6,
    kind: "question",
    body: "Which concrete class applies to the ground-floor slab?",
    references: [
      teamRoomMessageReference({
        reference: citedRecordId,
        label: "Slab concrete strength",
        detail: "Proposed requirement",
      }),
      teamRoomMessageReference({
        kind: "source_evidence",
        reference: evidenceArtifactId,
        version: 2,
        evidence_ordinal: 7,
        label: "01 Instructions/ITT.pdf",
        detail: "Concrete class passage",
      }),
    ],
  });
  return {
    ...projection,
    conversation: {
      conversation_id: "c".repeat(32),
      latest_meaningful_message_id: questionMessage.message_id,
      messages: [
        teamRoomMessage({
          message_id: "msg-start",
          sequence: 1,
          author: "system",
          kind: "status",
          body: "West Campus MEP workspace is ready.",
          created_at: daysAgoIso(3),
        }),
        handoffMessage,
        outputMessage,
        teamRoomMessage({
          message_id: "msg-routine",
          sequence: 4,
          author: "engineer",
          body: "Understood, keep me posted.",
          created_at: daysAgoIso(1),
        }),
        blockerMessage,
        questionMessage,
      ],
    },
    team: {
      ...projection.team,
      agent_runs: [
        {
          run_id: "5".repeat(32),
          task_id: "2".repeat(32),
          state: "running",
          agent: {
            profile_id: "e".repeat(32),
            profile_version: 1,
            identity: "Cost Estimator",
            profession: "Senior Construction Cost Estimator",
          },
          started_at: "2026-08-30T09:10:00Z",
          completed_at: null,
        },
      ],
    },
  };
}

async function openTeamRoom() {
  await screen.findByRole("textbox", {
    name: "Message your Tendering Manager",
  });
  const workspace = screen.getByRole("complementary", {
    name: "Tender workspace",
  });
  fireEvent.click(within(workspace).getByRole("button", { name: "Team" }));
  await screen.findByRole("heading", { name: "Team working" });
}

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
          selection: { kind: "effort", value: "medium" } as const,
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
          selection: { kind: "effort", value: "high" } as const,
          label: "high",
          description: "Deeper",
          is_default: true,
        },
        {
          selection: { kind: "effort", value: "xhigh" } as const,
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

function readyApplicationSettings(): ApplicationSettingsView {
  const selection = {
    connection_id: "codex_chatgpt",
    provider: "codex" as const,
    model_id: "gpt-live-a",
    reasoning: { kind: "effort" as const, value: "medium" },
    catalogue_fetched_at: "chatgpt-direct-v1",
    adapter_version: "chatgpt-direct-v1",
  };
  return {
    ...applicationFacts,
    ai_execution_selection: selection,
    ai_execution_approval: {
      ...selection,
      reasoning: { ...selection.reasoning },
      account_fingerprint:
        "117d68e191e9e848c1172767d9ca54204ef5e4b20d1ead8855ef0f17f906f695",
      data_destination: "ChatGPT subscription",
      approved_at: "2026-08-22T08:01:00Z",
    },
    provider_connections: [
      {
        ...tenderAiProviderConnection,
        catalogue_fetched_at: "chatgpt-direct-v1",
        adapter_version: "chatgpt-direct-v1",
      },
    ],
    chatgpt: {
      state: "connected",
      account_id: "engineer@example.com",
      plan_type: "plus",
      expires_at_ms: 1_800_000_000_000n,
      login_phase: "completed",
    },
  };
}

beforeEach(() => {
  host.inspectAiProviders.mockResolvedValue({
    connections: [],
    active_id: null,
    file_path: "C:\Users\engineer\.quantix\.env",
  });
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
  host.inspectStartupReconciliation.mockResolvedValue({
    removed_tender_candidates: 0,
    interrupted_backup_operations: 0,
    interrupted_recovery_operations: 0,
    completed_retention_operations: 0,
  });
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
    expect(within(context).getByText("Team activity")).toBeTruthy();
    expect(within(context).getByText("Tender records")).toBeTruthy();
    expect(within(context).queryByText("Current action")).toBeNull();
    expect(within(context).queryByText("Capabilities")).toBeNull();
    expect(within(context).queryByText("Document tools")).toBeNull();
    expect(within(context).queryByText("Approved AI")).toBeNull();
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

  it("routes prepare work plan into the focused governed panel", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      current_action: {
        kind: "prepare_work_plan" as const,
        title: "Prepare the work plan",
        summary:
          "The Tendering Manager is ready to propose the team and tasks.",
        action_label: "Prepare the Work Plan",
        requires_engineer: false,
      },
    });

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Prepare the Work Plan" }),
    );

    expect(await screen.findByTestId("tender-focused-action")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Work Plan" })).toBeTruthy();
    expect(
      await screen.findByRole("button", {
        name: "Compose Tender Office proposal",
      }),
    ).toBeTruthy();
  });

  it("routes review work plan into the focused governed panel", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      current_action: {
        kind: "review_work_plan" as const,
        title: "Review the Work Plan",
        summary: "Review the exact Work Plan proposal.",
        action_label: "Review the Work Plan",
        requires_engineer: true,
      },
    });

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Review the Work Plan" }),
    );

    expect(await screen.findByTestId("tender-focused-action")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Work Plan" })).toBeTruthy();
  });

  it("gathers routed questions into an evidence-linked External RFI draft", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(
      rfiProjection(
        {
          kind: "draft_external_rfi" as const,
          title: "Ask the client a controlled question",
          summary: "A Tender question is routed for a controlled External RFI.",
          action_label: "Start External RFI",
          requires_engineer: true,
        },
        [],
      ),
    );
    host.inspectExternalRfis.mockResolvedValue({
      items: [],
      next_cursor: null,
      total_current_count: 0,
      approved_for_issue_count: 0,
    });
    host.inspectExternalRfiEligibleQueries.mockResolvedValue(
      eligibleQueryPage(),
    );
    host.createExternalRfiDraft.mockResolvedValue(rfiDraftFixture());

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Start External RFI" }),
    );
    const surface = await screen.findByTestId("tender-rfi-review");
    const questionCheckbox = await within(surface).findByRole("checkbox", {
      name: /Who carries the installation responsibility/,
    });
    expect(questionCheckbox).toBeTruthy();

    fireEvent.click(questionCheckbox);
    fireEvent.change(
      within(surface).getByLabelText("Exact contractual context"),
      { target: { value: "The wording leaves the boundary unresolved." } },
    );
    fireEvent.change(within(surface).getByLabelText("Response needed"), {
      target: { value: "Confirm the responsible party." },
    });
    fireEvent.change(within(surface).getByLabelText("Response needed by"), {
      target: { value: "2030-01-01T00:00" },
    });
    fireEvent.change(within(surface).getByLabelText("Recipient organization"), {
      target: { value: "Employer Procurement Team" },
    });
    fireEvent.change(within(surface).getByLabelText("Attention"), {
      target: { value: "Tender Clarifications Manager" },
    });

    fireEvent.click(
      within(surface).getByRole("button", {
        name: "Create External RFI draft",
      }),
    );

    await waitFor(() =>
      expect(host.createExternalRfiDraft).toHaveBeenCalledWith(
        expect.objectContaining({
          tender_id: tenderId,
          query_refs: [rfiQueryRef],
          additional_evidence: [],
          contractual_context: "The wording leaves the boundary unresolved.",
          response_need: "Confirm the responsible party.",
          recipient: {
            organization: "Employer Procurement Team",
            attention: "Tender Clarifications Manager",
            email: null,
          },
          attachments: [],
        }),
      ),
    );
    await waitFor(() => {
      expect(
        (
          within(surface).getByRole("button", {
            name: "Create External RFI draft",
          }) as HTMLButtonElement
        ).disabled,
      ).toBe(true);
    });
  });

  it("revises, independently reviews, and approves the pending External RFI draft", async () => {
    installResponsiveMatchMedia(1440);
    let currentDraft = rfiDraftFixture();
    host.inspectManagerWorkspace.mockResolvedValue(
      rfiProjection(
        {
          kind: "review_external_rfi" as const,
          title: "Review the External RFI draft",
          summary:
            "A controlled question to the client is drafted from exact Tender questions and evidence.",
          action_label: "Review External RFI",
          requires_engineer: true,
        },
        [rfiSummary()],
      ),
    );
    host.inspectExternalRfis.mockImplementation(() =>
      Promise.resolve({
        items: [currentDraft],
        next_cursor: null,
        total_current_count: 1,
        approved_for_issue_count: 0,
      }),
    );
    host.inspectExternalRfiEligibleQueries.mockResolvedValue(
      eligibleQueryPage(),
    );
    host.reviseExternalRfiDraft.mockImplementation((command) => {
      currentDraft = {
        ...currentDraft,
        version: command.base_version + 1,
        contractual_context: command.contractual_context,
      };
      return Promise.resolve(currentDraft);
    });
    host.runExternalRfiReview.mockImplementation(() => {
      currentDraft = { ...currentDraft, review: rfiReviewFixture() };
      return Promise.resolve({
        run: { state: "completed" },
        rfi: currentDraft,
      });
    });
    host.approveExternalRfiForIssue.mockResolvedValue(rfiDraftFixture());

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Review External RFI" }),
    );
    const surface = await screen.findByTestId("tender-rfi-review");
    const reviewSection = await within(surface).findByTestId(
      "tender-rfi-review-section",
    );
    expect(await within(reviewSection).findByText(rfiQuestion)).toBeTruthy();

    fireEvent.click(
      within(reviewSection).getByRole("button", { name: "Revise draft" }),
    );
    fireEvent.change(
      within(reviewSection).getByLabelText("Exact contractual context"),
      {
        target: {
          value:
            "The wording leaves the boundary unresolved. Revision clarifies the split.",
        },
      },
    );
    fireEvent.click(
      within(reviewSection).getByRole("button", {
        name: "Publish revised draft",
      }),
    );
    await waitFor(() =>
      expect(host.reviseExternalRfiDraft).toHaveBeenCalledWith(
        expect.objectContaining({
          tender_id: tenderId,
          rfi_id: currentDraft.rfi_id,
          base_version: 1,
          additional_evidence: currentDraft.source_evidence,
        }),
      ),
    );

    fireEvent.click(
      await within(surface).findByRole("button", {
        name: "Run independent review",
      }),
    );
    await waitFor(() =>
      expect(host.runExternalRfiReview).toHaveBeenCalledWith({
        tender_id: tenderId,
        rfi_id: currentDraft.rfi_id,
        version: 2,
      }),
    );
    const reviewSectionAfter = within(surface).getByTestId(
      "tender-rfi-review-section",
    );
    expect(
      await within(reviewSectionAfter).findByText(
        "Independent review result: passed",
      ),
    ).toBeTruthy();

    fireEvent.change(
      within(reviewSectionAfter).getByLabelText("Approval rationale"),
      {
        target: {
          value:
            "The reviewed wording asks the exact question the bid needs answered.",
        },
      },
    );
    fireEvent.click(
      within(reviewSectionAfter).getByRole("button", {
        name: "Approve for issue",
      }),
    );
    await waitFor(() =>
      expect(host.approveExternalRfiForIssue).toHaveBeenCalledWith({
        tender_id: tenderId,
        rfi_id: currentDraft.rfi_id,
        version: 2,
        manifest_sha256: currentDraft.manifest_sha256,
        rationale:
          "The reviewed wording asks the exact question the bid needs answered.",
      }),
    );
  });

  it("exports the approved External RFI and shows the verified record for human delivery", async () => {
    installResponsiveMatchMedia(1440);
    const approval = rfiApprovalFixture();
    const approvedDraft = rfiDraftFixture({
      review: rfiReviewFixture(),
      approval,
      approved_for_issue: true,
    });
    host.inspectManagerWorkspace.mockResolvedValue(
      rfiProjection(
        {
          kind: "draft_external_rfi" as const,
          title: "Ask the client a controlled question",
          summary: "A Tender question is routed for a controlled External RFI.",
          action_label: "Start External RFI",
          requires_engineer: true,
        },
        [
          rfiSummary({
            status: "approved_for_issue",
            approval_pending: false,
            export_pending: true,
          }),
        ],
      ),
    );
    host.inspectExternalRfis.mockResolvedValue({
      items: [approvedDraft],
      next_cursor: null,
      total_current_count: 1,
      approved_for_issue_count: 1,
    });
    host.inspectExternalRfiEligibleQueries.mockResolvedValue(
      eligibleQueryPage(),
    );
    host.inspectExternalRfiResponseCandidates.mockResolvedValue({
      items: [],
      next_cursor: null,
    });
    host.exportApprovedExternalRfi.mockResolvedValue({
      export_id: "7".repeat(32),
      approval_id: approval.approval_id,
      path: "A:\\Quantix-test\\exports\\external-rfi-v1.txt",
      bytes_sha256: "e".repeat(64),
      size_bytes: 1234n,
      bytes_verified: true,
      manifest_sha256: "0".repeat(64),
      created_at: "2026-08-31T10:00:00Z",
    });

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Start External RFI" }),
    );
    const section = await screen.findByTestId("tender-rfi-response-section");
    expect(
      within(section).getByText(
        /You deliver that file to the recipient outside Quantix/,
      ),
    ).toBeTruthy();

    fireEvent.click(
      within(section).getByRole("button", { name: "Export verified file" }),
    );

    await waitFor(() =>
      expect(host.exportApprovedExternalRfi).toHaveBeenCalledWith({
        tender_id: tenderId,
        rfi_id: approvedDraft.rfi_id,
        version: 1,
        approval_sha256: approval.approval_sha256,
      }),
    );
    expect(await screen.findByTestId("tender-rfi-export-record")).toBeTruthy();
    expect(
      screen.getByText(/A:\\Quantix-test\\exports\\external-rfi-v1\.txt/),
    ).toBeTruthy();
    expect(screen.getByText(/File verified after writing/)).toBeTruthy();
  });

  it("registers a received response without replacing the outgoing questions or prior responses", async () => {
    installResponsiveMatchMedia(1440);
    const approval = rfiApprovalFixture();
    const priorResponse = {
      response_link_id: "a".repeat(32),
      rfi_id: "4".repeat(32),
      rfi_version: 1,
      approval_id: approval.approval_id,
      source_artifact_id: "6".repeat(32),
      source_artifact_version: 2,
      registered_by: "engineer_user",
      manifest_sha256: "1".repeat(64),
      created_at: "2026-08-30T12:00:00Z",
    };
    const issuedDraft = rfiDraftFixture({
      review: rfiReviewFixture(),
      approval,
      approved_for_issue: true,
      responses: [priorResponse],
    });
    host.inspectManagerWorkspace.mockResolvedValue(
      rfiProjection(
        {
          kind: "interpret_external_rfi_response" as const,
          title: "Interpret the received response",
          summary:
            "A response to your External RFI arrived. Record one interpretation as the Manager.",
          action_label: "Interpret response",
          requires_engineer: true,
        },
        [
          rfiSummary({
            status: "response_awaiting_interpretation",
            response_count: 1,
            interpretation_pending: true,
          }),
        ],
      ),
    );
    host.inspectExternalRfis.mockResolvedValue({
      items: [issuedDraft],
      next_cursor: null,
      total_current_count: 1,
      approved_for_issue_count: 1,
    });
    host.inspectExternalRfiEligibleQueries.mockResolvedValue(
      eligibleQueryPage(),
    );
    host.inspectExternalRfiResponseCandidates.mockResolvedValue({
      items: [
        {
          source_artifact_id: "9".repeat(32),
          source_artifact_version: 1,
          package_path: "responses/employer-reply.pdf",
        },
      ],
      next_cursor: null,
    });

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Interpret response" }),
    );
    const section = await screen.findByTestId("tender-rfi-response-section");
    expect(
      (await within(section).findAllByText(rfiQuestion)).length,
    ).toBeGreaterThan(0);
    expect(within(section).getByText(`${"6".repeat(32)} · v2`)).toBeTruthy();

    const responseSelect = within(section).getByLabelText(
      "Response document from the Tender package intake",
    );
    await within(responseSelect).findByRole("option", {
      name: /employer-reply\.pdf/,
    });
    fireEvent.change(responseSelect, {
      target: { value: `${"9".repeat(32)}:1` },
    });
    fireEvent.click(
      within(section).getByRole("button", { name: "Register response" }),
    );

    await waitFor(() =>
      expect(host.registerExternalRfiResponse).toHaveBeenCalledWith({
        tender_id: tenderId,
        rfi_id: issuedDraft.rfi_id,
        rfi_version: 1,
        approval_id: approval.approval_id,
        source_artifact_id: "9".repeat(32),
        source_artifact_version: 1,
      }),
    );
    expect(
      within(section).getByText(
        `${priorResponse.source_artifact_id} · v${priorResponse.source_artifact_version}`,
      ),
    ).toBeTruthy();
  });

  it("records one Manager interpretation and returns to the conversation", async () => {
    installResponsiveMatchMedia(1440);
    const approval = rfiApprovalFixture();
    const response = {
      response_link_id: "a".repeat(32),
      rfi_id: "4".repeat(32),
      rfi_version: 1,
      approval_id: approval.approval_id,
      source_artifact_id: "6".repeat(32),
      source_artifact_version: 2,
      registered_by: "engineer_user",
      manifest_sha256: "1".repeat(64),
      created_at: "2026-08-30T12:00:00Z",
    };
    const issuedDraft = rfiDraftFixture({
      review: rfiReviewFixture(),
      approval,
      approved_for_issue: true,
      responses: [response],
    });
    host.inspectManagerWorkspace.mockResolvedValue(
      rfiProjection(
        {
          kind: "interpret_external_rfi_response" as const,
          title: "Interpret the received response",
          summary:
            "A response to your External RFI arrived. Record one interpretation as the Manager.",
          action_label: "Interpret response",
          requires_engineer: true,
        },
        [
          rfiSummary({
            status: "response_awaiting_interpretation",
            response_count: 1,
            interpretation_pending: true,
          }),
        ],
      ),
    );
    host.inspectExternalRfis.mockResolvedValue({
      items: [issuedDraft],
      next_cursor: null,
      total_current_count: 1,
      approved_for_issue_count: 1,
    });
    host.inspectExternalRfiEligibleQueries.mockResolvedValue(
      eligibleQueryPage(),
    );
    host.inspectExternalRfiResponseCandidates.mockResolvedValue({
      items: [],
      next_cursor: null,
    });
    host.interpretExternalRfiResponse.mockResolvedValue(issuedDraft);

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Interpret response" }),
    );
    const section = await screen.findByTestId("tender-rfi-response-section");
    expect(
      await within(section).findByText(
        `source_artifact ${"6".repeat(32)} · v2`,
      ),
    ).toBeTruthy();

    fireEvent.change(within(section).getByLabelText("Interpretation"), {
      target: {
        value:
          "The bidder carries installation responsibility while the Employer retains design responsibility.",
      },
    });
    fireEvent.change(within(section).getByLabelText("Manager rationale"), {
      target: { value: "Preserve the confirmed responsibility split." },
    });
    fireEvent.change(
      within(section).getByLabelText("Exact treatment details"),
      {
        target: {
          value:
            "Qualify the price against the confirmed responsibility split.",
        },
      },
    );
    fireEvent.click(
      within(section).getByRole("button", { name: "Record interpretation" }),
    );

    await waitFor(() =>
      expect(host.interpretExternalRfiResponse).toHaveBeenCalledWith({
        tender_id: tenderId,
        response_link_id: response.response_link_id,
        query_id: rfiQueryRef.query_id,
        issued_query_version: 1,
        base_query_version: 1,
        base_query_manifest_sha256: rfiQueryRef.manifest_sha256,
        material: true,
        interpretation:
          "The bidder carries installation responsibility while the Employer retains design responsibility.",
        treatment: "qualification",
        rationale: "Preserve the confirmed responsibility split.",
        treatment_details:
          "Qualify the price against the confirmed responsibility split.",
        closes_query: false,
      }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("tender-rfi-review")).toBeNull(),
    );
  });

  it("starts the approved plan automatically and returns to the Manager conversation", async () => {
    const plan = workPlanInspection();
    const approvedPlan = {
      ...plan,
      approval: {
        approval_id: "q".repeat(32),
        plan_id: plan.plan_id,
        plan_version: plan.version,
        decision: "approve" as const,
        rationale: "Start the work now.",
        plan_manifest_sha256: plan.manifest_sha256,
        decided_by: "engineer_user",
        acting_role: "tendering_manager",
        approval_sha256: "3".repeat(64),
        created_at: "2026-08-30T09:04:00Z",
      },
    };
    const reviewAction = {
      kind: "review_work_plan" as const,
      title: "Review the Work Plan",
      summary: "Review the exact Work Plan proposal.",
      action_label: "Review the Work Plan",
      requires_engineer: true,
    };
    const systemMessage = {
      message_id: messageId,
      sequence: 1,
      author: "system" as const,
      kind: "status" as const,
      body: "West Campus MEP workspace is ready.",
      created_at: "2026-08-30T09:00:00Z",
      references: [],
    };
    const planMessage = {
      message_id: "m".repeat(32),
      sequence: 2,
      author: "manager" as const,
      kind: "output" as const,
      body: `The project office is ready: ${plan.profiles.length} specialists will deliver the work. Review Work Plan v${plan.version}.`,
      created_at: "2026-08-30T09:01:00Z",
      references: [],
    };
    const confirmationMessage = {
      message_id: "n".repeat(32),
      sequence: 3,
      author: "manager" as const,
      kind: "status" as const,
      body: `Work Plan v${plan.version} is approved and now in progress. Unblocked work starts automatically.`,
      created_at: "2026-08-30T09:05:00Z",
      references: [],
    };
    const planningProjection: ManagerWorkspaceProjection = {
      ...projection,
      current_action: reviewAction,
      conversation: {
        conversation_id: "c".repeat(32),
        latest_meaningful_message_id: planMessage.message_id,
        messages: [systemMessage, planMessage],
      },
    };
    const productionProjection: ManagerWorkspaceProjection = {
      ...projection,
      current_action: {
        kind: "review_work" as const,
        title: "Tender work is in progress",
        summary: "The Tendering Manager is coordinating the approved plan.",
        action_label: "View work",
        requires_engineer: false,
      },
      conversation: {
        conversation_id: "c".repeat(32),
        latest_meaningful_message_id: confirmationMessage.message_id,
        messages: [systemMessage, planMessage, confirmationMessage],
      },
    };
    host.inspectManagerWorkspace
      .mockResolvedValueOnce(planningProjection)
      .mockResolvedValue(productionProjection);
    host.inspectCurrentWorkPlan.mockResolvedValue(plan);
    host.decideWorkPlanProposal.mockResolvedValue(approvedPlan);
    host.activateTenderProduction.mockResolvedValue(productionInspection(plan));

    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Review the Work Plan" }),
    );
    expect(await screen.findByTestId("tender-focused-action")).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Attributable rationale"), {
      target: { value: "Start the work now." },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Approve exact Work Plan" }),
    );

    await waitFor(() => {
      expect(host.decideWorkPlanProposal).toHaveBeenCalledWith(
        tenderId,
        plan.plan_id,
        plan.version,
        "approve",
        "Start the work now.",
      );
    });
    await waitFor(() => {
      expect(host.activateTenderProduction).toHaveBeenCalledWith(
        tenderId,
        plan.plan_id,
        plan.version,
        plan.manifest_sha256,
      );
    });
    expect(
      await screen.findByText(
        `Work Plan v${plan.version} is approved and now in progress. Unblocked work starts automatically.`,
      ),
    ).toBeTruthy();
    await waitFor(() => {
      expect(screen.queryByTestId("tender-focused-action")).toBeNull();
    });
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

  it("creates a verified backup from the Tender menu and shows where to inspect it", async () => {
    const backup = {
      backup_id: "b".repeat(32),
      tender_id: tenderId,
      state: "ready" as const,
      source: null,
      content_object_count: 1n,
      manifest_sha256: "a".repeat(64),
      archive_size_bytes: 2n,
      diagnostic_code: null,
      created_at: "2026-08-30T09:30:00Z",
    };
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.createTenderBackup.mockResolvedValue(backup);
    host.inspectTenderIntegrity.mockResolvedValue({
      tender_id: tenderId,
      state: "ready",
      issues: [],
      recovery_choices: [],
    });
    host.inspectTenderBackups.mockResolvedValue([backup]);
    host.inspectTenderRecoveries.mockResolvedValue([]);

    render(<ManagerWorkspace />);
    await screen.findByRole("heading", { name: "West Campus MEP" });
    fireEvent.click(screen.getByLabelText("Manage West Campus MEP"));
    fireEvent.click(
      await screen.findByRole("menuitem", { name: /Create verified backup/ }),
    );
    await waitFor(() => {
      expect(host.createTenderBackup).toHaveBeenCalledWith(tenderId);
    });
    expect(
      await screen.findByText(
        `Verified backup created at ${new Date(backup.created_at).toLocaleString()}. Find it under Inspect backups.`,
      ),
    ).toBeTruthy();

    fireEvent.click(screen.getByLabelText("Manage West Campus MEP"));
    fireEvent.click(
      await screen.findByRole("menuitem", { name: /Inspect backups/ }),
    );
    expect(
      await screen.findByRole("heading", {
        name: "Backups for West Campus MEP",
      }),
    ).toBeTruthy();
    expect(screen.getByText("Verified and ready")).toBeTruthy();
    expect(screen.getByText(/content objects/)).toBeTruthy();
  });

  it("keeps backup actions out of a recovery-required Tender menu", async () => {
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

    render(<ManagerWorkspace />);
    const recoveryButton = await screen.findByRole("button", {
      name: `Open recovery center for ${recoveryTender.name}`,
    });
    fireEvent.keyDown(
      recoveryButton.closest(".manager-workspace__tender-row")!,
      { key: "F10", shiftKey: true },
    );
    expect(
      await screen.findByRole("menuitem", { name: /Inspect recovery/ }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("menuitem", { name: /Create verified backup/ }),
    ).toBeNull();
    expect(
      screen.queryByRole("menuitem", { name: /Inspect backups/ }),
    ).toBeNull();
  });

  it("surfaces one plain startup cleanup line when the Host finished interrupted work", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.inspectStartupReconciliation.mockResolvedValue({
      removed_tender_candidates: 1,
      interrupted_backup_operations: 1,
      interrupted_recovery_operations: 0,
      completed_retention_operations: 2,
    });

    render(<ManagerWorkspace />);
    const notice = await screen.findByRole("status", {
      name: "Startup cleanup",
    });
    expect(notice.textContent).toContain(
      "One unfinished Tender registration was cleaned up during startup.",
    );
    expect(notice.textContent).toContain(
      "An interrupted backup was safely closed.",
    );
    expect(notice.textContent).toContain(
      "2 interrupted Archived & Trash changes were finished safely.",
    );
    expect(within(notice).getByText(/Technical details/)).toBeTruthy();
  });

  it("stays silent about startup cleanup when the Host reported nothing", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);

    render(<ManagerWorkspace />);
    await screen.findByRole("heading", { name: "West Campus MEP" });
    await waitFor(() => {
      expect(host.inspectStartupReconciliation).toHaveBeenCalled();
    });
    expect(
      screen.queryByRole("status", { name: "Startup cleanup" }),
    ).toBeNull();
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
    expect(within(context).queryByText("Current action")).toBeNull();
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
        reasoning: { kind: "effort", value: "medium" },
        catalogue_fetched_at: "chatgpt-direct-v1",
        adapter_version: "chatgpt-direct-v1",
      },
      ai_execution_approval: {
        connection_id: "codex_chatgpt",
        provider: "codex",
        account_fingerprint:
          "117d68e191e9e848c1172767d9ca54204ef5e4b20d1ead8855ef0f17f906f695",
        model_id: "gpt-live-a",
        reasoning: { kind: "effort", value: "medium" },
        data_destination: "ChatGPT subscription",
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
                  selection: { kind: "effort", value: "medium" },
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
                  selection: { kind: "effort", value: "high" },
                  label: "high",
                  description: "Deeper",
                  is_default: true,
                },
              ],
            },
          ],
          catalogue_fetched_at: "chatgpt-direct-v1",
          adapter_version: "chatgpt-direct-v1",
          status_summary: "Ready to run Tender work.",
        },
      ],
      chatgpt: {
        state: "connected",
        account_id: "engineer@example.com",
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
        reasoning: { kind: "effort", value: "high" },
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
    fireEvent.click(screen.getByRole("button", { name: "AI Providers" }));
    fireEvent.click(screen.getByText("Advanced model settings"));
    fireEvent.click(screen.getByRole("button", { name: /Live model A/ }));
    expect(screen.getByRole("option", { name: /^Live model A/ })).toBeTruthy();
    expect(screen.getByRole("option", { name: /^Live model B/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("option", { name: /^Live model B/ }));
    expect(screen.getByRole("heading", { name: "AI Providers" })).toBeTruthy();

    await waitFor(() => {
      expect(host.updateAiExecutionSelection).toHaveBeenCalledWith({
        connection_id: "codex_chatgpt",
        model_id: "gpt-live-b",
        reasoning: { kind: "effort", value: "high" },
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

  it("keeps a persisted Tender AI selection out of the conversation workspace", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      ai_execution: {
        revision: 2n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-5.3-codex-spark",
          reasoning: { kind: "effort", value: "low" },
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

    expect(
      await screen.findByRole("log", { name: "Tender conversation" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("group", { name: "Tender AI selection" }),
    ).toBeNull();
    expect(screen.queryByText("gpt-5.3-codex-spark")).toBeNull();
  });

  it("keeps provider, model, and reasoning changes in Settings", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.inspectApplicationSettings.mockResolvedValue({
      ...applicationFacts,
      ai_execution_selection: null,
      provider_connections: [tenderAiProviderConnection],
    });

    render(<ManagerWorkspace />);

    expect(
      await screen.findByRole("log", { name: "Tender conversation" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("group", { name: "Tender AI selection" }),
    ).toBeNull();
    expect(host.updateTenderAiExecution).not.toHaveBeenCalled();
  });

  it("refreshes Settings without adding AI tooling to the workspace", async () => {
    const selectedProjection: ManagerWorkspaceProjection = {
      ...projection,
      ai_execution: {
        revision: 4n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-live-a",
          reasoning: { kind: "effort", value: "medium" },
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
                    kind: "effort" as const,
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

    await screen.findByRole("log", { name: "Tender conversation" });
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "Back to workspace" }),
    );

    expect(
      await screen.findByRole("log", { name: "Tender conversation" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("group", { name: "Tender AI selection" }),
    ).toBeNull();
    expect(host.updateTenderAiExecution).not.toHaveBeenCalled();
  });

  it("keeps the connected ChatGPT state after leaving Settings through a Tender", async () => {
    const readySettings = readyApplicationSettings();
    const disconnectedSettings: ApplicationSettingsView = {
      ...readySettings,
      ai_execution_selection: null,
      ai_execution_approval: null,
      provider_connections: [
        {
          ...readySettings.provider_connections[0]!,
          status: "authentication_required",
          account_label: null,
          account_plan: null,
          models: [],
          catalogue_fetched_at: null,
          status_summary:
            "Connect your ChatGPT subscription in Settings before retrying.",
        },
      ],
      chatgpt: {
        state: "absent",
        account_id: null,
        plan_type: null,
        expires_at_ms: null,
        login_phase: "idle",
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(projection);
    host.selectManagerWorkspaceTender.mockResolvedValue(projection);
    host.inspectApplicationSettings
      .mockResolvedValueOnce(disconnectedSettings)
      .mockResolvedValue(readySettings);
    host.refreshApplicationSettings.mockResolvedValue(readySettings);

    render(<ManagerWorkspace />);
    fireEvent.click(await screen.findByRole("button", { name: "Settings" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "AI Providers" }),
    );
    expect(
      await screen.findByText("ChatGPT is ready for new Tenders."),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /^West Campus MEP/ }));

    expect(
      await screen.findByRole("log", { name: "Tender conversation" }),
    ).toBeTruthy();
    expect(
      screen.queryByRole("group", { name: "Tender AI selection" }),
    ).toBeNull();
  });

  it("keeps the saved draft when Shift+Enter is used", async () => {
    const selectedProjection: ManagerWorkspaceProjection = {
      ...projection,
      ai_execution: {
        revision: 4n,
        selection: {
          connection_id: "codex_chatgpt",
          provider: "codex",
          model_id: "gpt-live-a",
          reasoning: { kind: "effort", value: "medium" },
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
    host.recordEngineerWorkspaceMessage.mockResolvedValue(selectedProjection);

    render(<ManagerWorkspace />);

    const composer = await screen.findByRole("textbox", {
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

  it("explains the application-home free-space requirement when document-tool preparation cannot start", async () => {
    const documentProjection = {
      ...projection,
      files: { ...projection.files, tender_document_count: 1 },
    };
    host.inspectManagerWorkspace.mockResolvedValue(documentProjection);
    host.inspectRuntimeReadiness.mockResolvedValue({
      state: "missing_executable",
      issues: ["ocr_executable_missing"],
      uv_version: "0.12.2",
      ocr_version: null,
      repair_available: true,
    });
    host.repairRuntimeReadiness.mockResolvedValue({
      state: "repair_required",
      issues: ["insufficient_disk_space"],
      uv_version: null,
      ocr_version: null,
      repair_available: true,
    });

    render(<ManagerWorkspace initialProjection={documentProjection} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Prepare document tools" }),
    );

    expect(
      await screen.findByText(
        /Quantix needs at least 2 GB of free space in the Application Home/i,
      ),
    ).toBeTruthy();
  });

  it("resumes durably parsed intakes after a stable unavailable runtime result", async () => {
    const documentProjection = {
      ...projection,
      files: { ...projection.files, tender_document_count: 1 },
    };
    host.inspectRuntimeReadiness.mockResolvedValue({
      state: "missing_executable",
      issues: ["ocr_executable_missing"],
      uv_version: "0.12.2",
      ocr_version: null,
      repair_available: true,
    });

    render(
      <StrictMode>
        <ManagerWorkspace initialProjection={documentProjection} />
      </StrictMode>,
    );

    await waitFor(() => {
      expect(host.inspectRuntimeReadiness).toHaveBeenCalledTimes(1);
      expect(host.resumeManagerIntakes).toHaveBeenCalledTimes(1);
    });
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
          adapter_version: "0.151.0",
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
      await screen.findByRole("button", { name: "AI Providers" }),
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
      await screen.findByRole("button", { name: "AI Providers" }),
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
    // Scoped to the ChatGPT card: that sign-in flow must never show credential
    // jargon or token material. The separate model-provider section on this pane
    // does legitimately carry an "API key" field for bring-your-own-key providers.
    const chatgptCard = document.querySelector(
      ".application-settings__chatgpt-card",
    );
    expect(chatgptCard).toBeTruthy();
    expect(chatgptCard?.textContent).not.toMatch(/API key|accessToken|OAuth/i);
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
        blocking_agent_run_id: null,
        retry_not_before_epoch_seconds: null,
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
        blocking_agent_run_id: null,
        retry_not_before_epoch_seconds: null,
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
      external_rfis: [],
      estimate: null,
      pricing: null,
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

  it("does not start AI Tender work with approval metadata rejected by the exact Settings contract", async () => {
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
    const settings = readyApplicationSettings();
    settings.ai_execution_approval!.data_destination =
      "A different destination";
    host.inspectManagerWorkspace.mockResolvedValue(empty);
    host.inspectApplicationSettings.mockResolvedValue(settings);
    host.startManagerTender.mockResolvedValue(null);
    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Choose Tender Package" }),
    );

    expect(
      await screen.findByRole("button", { name: "Continue without AI" }),
    ).toBeTruthy();
    expect(host.startManagerTender).not.toHaveBeenCalled();
  });

  it("starts AI Tender work when the shared exact Settings contract is ready", async () => {
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
    host.inspectApplicationSettings.mockResolvedValue(
      readyApplicationSettings(),
    );
    host.startManagerTender.mockResolvedValue(null);
    render(<ManagerWorkspace />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Choose Tender Package" }),
    );

    await waitFor(() => {
      expect(host.startManagerTender).toHaveBeenCalledWith("directory", false);
    });
    expect(
      screen.queryByRole("button", { name: "Continue without AI" }),
    ).toBeNull();
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
    vi.useFakeTimers();
    host.selectManagerWorkspaceTender.mockImplementation(
      () =>
        new Promise((resolve) =>
          window.setTimeout(() => resolve(projection), 650),
        ),
    );
    render(<ManagerWorkspace initialProjection={projection} />);
    await act(async () => {
      await Promise.resolve();
    });
    fireEvent.click(
      screen.getByRole("button", { name: /West Campus MEP.*Intake/ }),
    );
    expect(
      screen.queryByRole("heading", { name: "Opening West Campus MEP…" }),
    ).toBeNull();
    await act(async () => {
      vi.advanceTimersByTime(400);
      await Promise.resolve();
    });
    expect(
      screen.getByRole("heading", { name: "Opening West Campus MEP…" }),
    ).toBeTruthy();
    await act(async () => {
      vi.advanceTimersByTime(250);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      screen.queryByRole("heading", { name: "Opening West Campus MEP…" }),
    ).toBeNull();
    expect(
      screen.getByRole("heading", { name: "West Campus MEP", level: 1 }),
    ).toBeTruthy();
  });

  it("times out Tender opening at 10 seconds and retries with a fresh selection", async () => {
    vi.useFakeTimers();
    const nextTenderId = "d".repeat(32);
    const nextTender = {
      ...projection.selected_tender!,
      tender_id: nextTenderId,
      name: "East Campus HVAC",
    };
    const initialProjection = {
      ...projection,
      catalogue: [projection.selected_tender!, nextTender],
    };
    const expiredProjection = {
      ...initialProjection,
      selected_tender: {
        ...nextTender,
        name: "Expired Tender Snapshot",
      },
    };
    const retryProjection = {
      ...initialProjection,
      selected_tender: nextTender,
    };
    let resolveExpired: (value: ManagerWorkspaceProjection) => void = () =>
      undefined;
    let resolveRetry: (value: ManagerWorkspaceProjection) => void = () =>
      undefined;
    host.selectManagerWorkspaceTender
      .mockReturnValueOnce(
        new Promise<ManagerWorkspaceProjection>((resolve) => {
          resolveExpired = resolve;
        }),
      )
      .mockReturnValueOnce(
        new Promise<ManagerWorkspaceProjection>((resolve) => {
          resolveRetry = resolve;
        }),
      );

    render(<ManagerWorkspace initialProjection={initialProjection} />);
    await act(async () => {
      await Promise.resolve();
    });
    fireEvent.click(
      screen.getByRole("button", { name: /East Campus HVAC.*Intake/ }),
    );

    await act(async () => {
      vi.advanceTimersByTime(9_999);
      await Promise.resolve();
    });
    expect(
      screen.getByRole("heading", { name: "Opening East Campus HVAC…" }),
    ).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();

    await act(async () => {
      vi.advanceTimersByTime(1);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      screen.queryByRole("heading", { name: "Opening East Campus HVAC…" }),
    ).toBeNull();
    expect(
      screen.getByRole("heading", { name: "West Campus MEP", level: 1 }),
    ).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain(
      "Opening East Campus HVAC failed: This Tender took too long to open. Please try again.",
    );

    resolveExpired(expiredProjection);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      screen.queryByRole("heading", {
        name: "Expired Tender Snapshot",
        level: 1,
      }),
    ).toBeNull();
    expect(
      screen.getByRole("heading", { name: "West Campus MEP", level: 1 }),
    ).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", {
        name: "Retry opening east campus hvac",
      }),
    );
    expect(host.selectManagerWorkspaceTender).toHaveBeenCalledTimes(2);
    resolveRetry(retryProjection);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(
      screen.getByRole("heading", { name: "East Campus HVAC", level: 1 }),
    ).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
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
        blocking_agent_run_id: null,
        retry_not_before_epoch_seconds: null,
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
        blocking_agent_run_id: null,
        retry_not_before_epoch_seconds: null,
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(pausedProjection);
    host.recordEngineerWorkspaceMessage.mockResolvedValue(pausedProjection);
    const { container } = render(<ManagerWorkspace />);

    expect(
      await screen.findByText("Paused — AI office unavailable"),
    ).toBeTruthy();
    const conversation = container.querySelector<HTMLElement>(
      ".manager-view__conversation",
    );
    if (!conversation) throw new Error("Tender conversation is not rendered");
    expect(
      within(conversation).getByText("Reading Tender documents"),
    ).toBeTruthy();
    expect(
      within(conversation).getByText(
        "Quantix is deriving exact source evidence from the registered documents.",
      ),
    ).toBeTruthy();
    expect(
      within(conversation).getAllByText(
        "Quantix is deriving exact source evidence from the registered documents.",
      ),
    ).toHaveLength(1);
    expect(screen.queryByText("Current focus", { exact: true })).toBeNull();
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

  it("keeps the active Tender stage visible while an unrelated document-tool probe settles", async () => {
    const extractingProjection: ManagerWorkspaceProjection = {
      ...projection,
      ai_execution: {
        revision: 1n,
        selection: null,
        readiness: "ready",
        status_summary: "Ready to continue Tender work.",
      },
      files: {
        ...projection.files,
        tender_document_count: 123,
      },
      current_action: {
        kind: "observe_intake",
        title: "Deriving Tender facts",
        summary:
          "The Tender Analyst is extracting requirements, risks, deadlines, and gaps.",
        action_label: "Intake in progress",
        requires_engineer: false,
      },
      intake: {
        intake_run_id: "4".repeat(32),
        stage: "extracting_tender_facts",
        status: "working",
        label: "Deriving Tender facts",
        summary:
          "The Tender Analyst is extracting requirements, risks, deadlines, and gaps.",
        parseable_document_count: 123,
        parsed_document_count: 123,
        extraction_run_count: 0,
        blocking_agent_run_id: null,
        retry_not_before_epoch_seconds: null,
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(extractingProjection);
    host.inspectRuntimeReadiness.mockReturnValue(new Promise(() => {}));

    render(<ManagerWorkspace />);

    expect(
      await screen.findByRole("heading", { name: "Deriving Tender facts" }),
    ).toBeTruthy();
    expect(screen.queryByText("Checking document tools")).toBeNull();
    expect(await screen.findByText("Working")).toBeTruthy();
  });

  it("keeps the main workspace focused on the conversation instead of AI tooling chrome", async () => {
    const settings = readyApplicationSettings();
    host.inspectApplicationSettings.mockResolvedValue(settings);
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      ai_execution: {
        ...projection.ai_execution,
        selection: settings.ai_execution_selection,
        readiness: "ready",
        status_summary: "Ready to run Tender work.",
      },
    });

    render(<ManagerWorkspace />);

    const composer = await screen.findByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    expect(composer).toBeTruthy();
    expect(screen.queryByText("Tools & Context", { exact: true })).toBeNull();
    expect(screen.queryByText("Provider", { exact: true })).toBeNull();
    expect(screen.queryByText("Model", { exact: true })).toBeNull();
    expect(screen.queryByText("Reasoning", { exact: true })).toBeNull();
    expect(
      screen.queryByRole("button", { name: /OpenAI account via Codex/i }),
    ).toBeNull();
  });

  it("renders the next-step suggestion in the chat and routes it through the existing action", async () => {
    const retryProjection: ManagerWorkspaceProjection = {
      ...projection,
      current_action: {
        kind: "retry_intake",
        title: "Retry Tender intake",
        summary: "The previous intake attempt can be run again.",
        action_label: "Retry intake",
        requires_engineer: true,
      },
      intake: {
        intake_run_id: "3".repeat(32),
        stage: "reading_documents",
        status: "failed",
        label: "Tender intake failed",
        summary: "The previous intake attempt can be run again.",
        parseable_document_count: 1,
        parsed_document_count: 0,
        extraction_run_count: 0,
        blocking_agent_run_id: null,
        retry_not_before_epoch_seconds: null,
      },
    };
    host.inspectManagerWorkspace.mockResolvedValue(retryProjection);
    host.retryManagerIntake.mockResolvedValue(undefined);

    render(<ManagerWorkspace />);

    const suggestion = await screen.findByRole("button", {
      name: "Retry intake",
    });
    expect(suggestion.closest(".current-action")).toBeNull();
    fireEvent.click(suggestion);
    await waitFor(() => {
      expect(host.retryManagerIntake).toHaveBeenCalledWith(tenderId);
    });
    expect(
      screen.getByRole("textbox", { name: "Message your Tendering Manager" }),
    ).toBeTruthy();
  });

  it("keeps the Manager surface quiet while the workspace is healthy and idle", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      ai_execution: {
        revision: 1n,
        selection: null,
        readiness: "ready",
        status_summary: "Ready to run Tender work.",
      },
      current_action: {
        kind: "observe_intake",
        title: "Tender intake is complete",
        summary: "The Tendering Manager is watching over the workspace.",
        action_label: "Intake complete",
        requires_engineer: false,
      },
      intake: null,
    });
    host.inspectApplicationSettings.mockResolvedValue(
      readyApplicationSettings(),
    );
    const { container } = render(<ManagerWorkspace />);

    await screen.findByRole("log", { name: "Tender conversation" });
    await waitFor(() => {
      expect(container.querySelector(".manager-view__status")).toBeNull();
    });
    expect(screen.queryByText("Ready")).toBeNull();
    expect(screen.queryByLabelText("Current Tender activity")).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Explain what happens next" }),
    ).toBeNull();
    expect(
      screen.getByRole("textbox", { name: "Message your Tendering Manager" }),
    ).toBeTruthy();
  });

  it("presents the AI provider blocker with one recovery action and technical details", async () => {
    const detail =
      "The approved provider is unreachable right now. Records remain accessible.";
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      current_action: {
        kind: "observe_intake",
        title: "Tender intake is complete",
        summary: "The Tendering Manager is watching over the workspace.",
        action_label: "Intake complete",
        requires_engineer: false,
      },
      intake: null,
      ai_execution: {
        revision: 1n,
        selection: null,
        readiness: "provider_unavailable",
        status_summary: detail,
      },
      doctor_blockers: [
        {
          code: "ai_provider_unavailable",
          area: "ai_execution" as const,
          title: "AI provider unavailable",
          detail,
        },
      ],
    });
    const { container } = render(<ManagerWorkspace />);

    expect(
      await screen.findByText(
        "AI office unavailable — records remain accessible",
      ),
    ).toBeTruthy();
    const blocker = await screen.findByText("AI provider unavailable");
    expect(blocker.closest(".manager-view__blocker")).toBeTruthy();
    const details = document.querySelector(".manager-view__blocker__details");
    if (!details) throw new Error("Technical details disclosure is missing");
    expect(details.hasAttribute("open")).toBe(false);
    fireEvent.click(
      within(details as HTMLElement).getByText("Technical details"),
    );
    expect(details.hasAttribute("open")).toBe(true);
    expect(within(details as HTMLElement).getByText(detail)).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: "Restore AI connection" }),
    );
    expect(
      await screen.findByRole("heading", { name: "Settings" }),
    ).toBeTruthy();
    expect(container.querySelector(".manager-view__capability")).toBeNull();
  });

  it("names capability gaps next to the work plan action only when they gate it", async () => {
    const blockedReadiness = {
      state: "blocked" as const,
      gaps: [
        {
          capability: "cost_estimation",
          reason: "No qualified separate Agent Profile is assigned.",
          affected_work: ["cost_estimation"],
        },
      ],
      blocker_codes: ["capability_gap"],
    };
    const gated: ManagerWorkspaceProjection = {
      ...projection,
      current_action: {
        kind: "prepare_work_plan",
        title: "Prepare the work plan",
        summary: "The Tender Manager is ready to propose the team and tasks.",
        action_label: "Prepare plan",
        requires_engineer: false,
      },
      capability_readiness: blockedReadiness,
    };
    host.inspectManagerWorkspace.mockResolvedValue(gated);
    render(<ManagerWorkspace />);

    expect(
      await screen.findByText(
        "These skills still need a specialist before the work plan can continue: cost estimation.",
      ),
    ).toBeTruthy();
  });

  it("stays quiet about capability gaps that do not gate the current action", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      current_action: {
        kind: "review_work",
        title: "Tender work is in progress",
        summary: "The Tender Manager is coordinating the approved plan.",
        action_label: "View work",
        requires_engineer: false,
      },
      capability_readiness: {
        state: "blocked",
        gaps: [
          {
            capability: "cost_estimation",
            reason: "No qualified separate Agent Profile is assigned.",
            affected_work: ["cost_estimation"],
          },
        ],
        blocker_codes: ["capability_gap"],
      },
    });
    const { container } = render(<ManagerWorkspace />);

    await screen.findByRole("log", { name: "Tender conversation" });
    expect(screen.queryByText(/cost estimation/)).toBeNull();
    expect(container.querySelector(".manager-view__capability")).toBeNull();
  });

  it("presents the AI preflight as an inline card instead of a modal before starting a Tender", async () => {
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

    const note = await screen.findByLabelText("AI setup for the new Tender");
    expect(
      within(note).getByRole("heading", {
        name: "AI & Models is not fully set up",
      }),
    ).toBeTruthy();
    expect(
      within(note).getByRole("button", { name: "Set up AI & Models" }),
    ).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.click(
      within(note).getByRole("button", { name: "Continue without AI" }),
    );
    await waitFor(() => {
      expect(host.startManagerTender).toHaveBeenCalledWith("directory", true);
    });
  });

  it("opens the exact source from a message evidence reference and restores the conversation on close", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(questionProjection());
    host.inspectEvidence.mockResolvedValue(instructionsDocument());
    host.inspectTenderRecord.mockResolvedValue(proposedRecord());

    render(<ManagerWorkspace />);
    fireEvent.click(await screen.findByText("2 references"));
    fireEvent.click(screen.getByRole("button", { name: /Review evidence/ }));

    const review = await screen.findByTestId("tender-evidence-review");
    expect(host.inspectEvidence).toHaveBeenCalledWith(
      tenderId,
      evidenceArtifactId,
      2,
    );
    const highlighted = within(review).getByText(
      "Ground-floor slabs shall be concrete class C30/37.",
    );
    expect(
      highlighted.closest(".tender-evidence__location")?.className,
    ).toContain("is-highlighted");
    expect(
      screen.getByRole("button", { name: "Reply to Manager" }),
    ).toBeTruthy();

    fireEvent.click(
      within(review).getByRole("button", {
        name: "Back to Manager conversation",
      }),
    );
    expect(screen.queryByTestId("tender-evidence-review")).toBeNull();
    expect(
      screen.getByRole("button", { name: "Reply to Manager" }),
    ).toBeTruthy();
    expect(
      screen.getByText(
        "Which concrete class applies to the ground-floor slab?",
      ),
    ).toBeTruthy();
  });

  it("presents conflicting sources together when the cited record carries a contradiction", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(questionProjection());
    host.inspectEvidence.mockImplementation(
      (_tenderId: string, artifactId: string) =>
        artifactId === conflictingArtifactId
          ? Promise.resolve(addendumDocument())
          : Promise.resolve(instructionsDocument()),
    );
    host.inspectTenderRecord.mockResolvedValue(proposedRecord());

    render(<ManagerWorkspace />);
    fireEvent.click(await screen.findByText("2 references"));
    fireEvent.click(screen.getByRole("button", { name: /Review evidence/ }));

    const conflicts = await screen.findByRole("region", {
      name: "Sources that disagree",
    });
    expect(host.inspectTenderRecord).toHaveBeenCalledWith(
      tenderId,
      citedRecordId,
      1,
    );
    expect(
      await within(conflicts).findByText(
        "Amend ground-floor slabs to concrete class C35/45.",
      ),
    ).toBeTruthy();
    expect(
      within(conflicts).getByText("02 Addenda/Addendum-1.pdf"),
    ).toBeTruthy();
  });

  it("decides a cited record against its exact version and returns to the Manager conversation", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(questionProjection());
    host.inspectTenderRecord.mockResolvedValue(proposedRecord());
    host.inspectTenderQueries.mockResolvedValue(emptyQueryPage());
    host.decideTenderRecord.mockResolvedValue({
      record: decidedRecord(),
      review: decidedRecord().reviews[0],
    });

    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Review cited Tender records",
      }),
    );

    const surface = await screen.findByTestId("tender-record-decision");
    expect(within(surface).getByText("Slab concrete strength")).toBeTruthy();
    expect(host.inspectTenderRecord).toHaveBeenCalledWith(
      tenderId,
      citedRecordId,
      1,
    );
    fireEvent.change(within(surface).getByLabelText(/Decision rationale/), {
      target: { value: "Confirmed against the instructions." },
    });
    fireEvent.click(
      within(surface).getByRole("button", {
        name: "Verify against the source",
      }),
    );

    await waitFor(() => {
      expect(host.decideTenderRecord).toHaveBeenCalledWith(
        tenderId,
        citedRecordId,
        1,
        "verify",
        "Confirmed against the instructions.",
      );
    });
    await waitFor(() => {
      expect(screen.queryByTestId("tender-record-decision")).toBeNull();
    });
    expect(
      screen.getByRole("button", { name: "Reply to Manager" }),
    ).toBeTruthy();
    await waitFor(() => {
      expect(host.inspectManagerWorkspace.mock.calls.length).toBeGreaterThan(1);
    });
  });

  it("takes query treatment decisions one at a time after the record decision", async () => {
    host.inspectManagerWorkspace.mockResolvedValue(questionProjection());
    host.inspectTenderQueries.mockResolvedValue({
      ...emptyQueryPage(),
      items: [pendingQuery()],
      total_current_count: 1,
    });
    host.inspectTenderRecord
      .mockResolvedValueOnce(proposedRecord())
      .mockResolvedValueOnce(decidedRecord());
    host.decideTenderRecord.mockResolvedValue({
      record: decidedRecord(),
      review: decidedRecord().reviews[0],
    });
    host.decideTenderQueryTreatment.mockResolvedValue({
      ...pendingQuery(),
      approved_treatment: {
        decision_id: "0".repeat(32),
        query_id: citedQueryId,
        query_version: 1,
        treatment: "approved_assumption",
        rationale: "Carry the addendum value as a stated assumption.",
        treatment_details: "C35/45 applies; pricing uses the addendum value.",
        closes_query: false,
        decided_by: "engineer_user",
        acting_role: "tendering_manager",
        manifest_sha256: "d".repeat(64),
        created_at: "2026-08-30T11:00:00Z",
      },
    });

    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Review cited Tender records",
      }),
    );

    const surface = await screen.findByTestId("tender-record-decision");
    expect(
      within(surface).getByTestId("tender-record-decision-record"),
    ).toBeTruthy();
    expect(
      within(surface).queryByTestId("tender-record-decision-query"),
    ).toBeNull();

    fireEvent.change(within(surface).getByLabelText(/Decision rationale/), {
      target: { value: "The addendum governs; return for rework." },
    });
    fireEvent.click(
      within(surface).getByRole("button", {
        name: "Return to the Manager with this reason",
      }),
    );
    await waitFor(() => {
      expect(host.decideTenderRecord).toHaveBeenCalledWith(
        tenderId,
        citedRecordId,
        1,
        "reject",
        "The addendum governs; return for rework.",
      );
    });
    await waitFor(() => {
      expect(screen.queryByTestId("tender-record-decision")).toBeNull();
    });

    fireEvent.click(
      await screen.findByRole("button", {
        name: "Review cited Tender records",
      }),
    );
    const reopened = await screen.findByTestId("tender-record-decision");
    expect(
      within(reopened).getByTestId("tender-record-decision-query"),
    ).toBeTruthy();

    fireEvent.change(within(reopened).getByLabelText("Treatment"), {
      target: { value: "approved_assumption" },
    });
    fireEvent.change(within(reopened).getByLabelText(/Decision rationale/), {
      target: { value: "Carry the addendum value as a stated assumption." },
    });
    fireEvent.change(
      within(reopened).getByLabelText(/Exact treatment and consequence/),
      { target: { value: "C35/45 applies; pricing uses the addendum value." } },
    );
    fireEvent.click(
      within(reopened).getByRole("button", { name: "Approve treatment" }),
    );

    await waitFor(() => {
      expect(host.decideTenderQueryTreatment).toHaveBeenCalledWith({
        tender_id: tenderId,
        query_id: citedQueryId,
        query_version: 1,
        treatment: "approved_assumption",
        rationale: "Carry the addendum value as a stated assumption.",
        treatment_details: "C35/45 applies; pricing uses the addendum value.",
        closes_query: false,
      });
    });
    await waitFor(() => {
      expect(screen.queryByTestId("tender-record-decision")).toBeNull();
    });
    expect(
      screen.getByRole("button", { name: "Reply to Manager" }),
    ).toBeTruthy();
  });

  it("groups registered files by package folder and lists prior versions in a History disclosure", async () => {
    installResponsiveMatchMedia(760);
    const conditionsDocument = {
      artifact_id: "1".repeat(32),
      version: 1,
      package_path: "01 Conditions/conditions.pdf",
      document_type: "pdf_document",
      media_type: "application/pdf",
      sha256: "a".repeat(64),
      size_bytes: 2048n,
      registration_state: "registered",
      parse_state: "parsed",
      exception: null,
    };
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      files: {
        tender_document_count: 2,
        quantix_output_count: 1,
        tender_documents: [
          conditionsDocument,
          {
            artifact_id: "3".repeat(32),
            version: 1,
            package_path: "02 Addenda/Addendum-1.pdf",
            document_type: "pdf_document",
            media_type: "application/pdf",
            sha256: "c".repeat(64),
            size_bytes: 1024n,
            registration_state: "registered",
            parse_state: "parsed",
            exception: null,
          },
        ],
        quantix_outputs: [
          {
            artifact_id: "9".repeat(32),
            version: 1,
            production_task_id: "k".repeat(32),
            author_run_id: "r".repeat(32),
            payload_sha256: "p".repeat(64),
            created_at: "2026-08-30T12:00:00Z",
          },
        ],
      },
    });
    host.inspectArtifactVersions.mockResolvedValue({
      versions: [
        {
          artifact_id: "3".repeat(32),
          version: 1,
          digest: "c".repeat(64),
          created_at: "2026-08-31T08:00:00Z",
        },
        {
          artifact_id: "1".repeat(32),
          version: 1,
          digest: "a".repeat(64),
          created_at: "2026-08-30T07:00:00Z",
        },
      ],
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

    expect(screen.getByRole("heading", { name: "01 Conditions" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "02 Addenda" })).toBeTruthy();
    expect(
      within(
        screen.getByText("01 Conditions/conditions.pdf").closest("li")!,
      ).getByText("Registered · parsed"),
    ).toBeTruthy();

    const addendumRow = screen
      .getByText("02 Addenda/Addendum-1.pdf")
      .closest("li")!;
    fireEvent.click(within(addendumRow).getByText("History"));
    expect(
      await within(addendumRow).findByText(/Prior version · v1/),
    ).toBeTruthy();
    expect(within(addendumRow).getByText("a".repeat(64))).toBeTruthy();
    expect(within(addendumRow).getByText(/Current version · v1/)).toBeTruthy();
    expect(host.inspectArtifactVersions).toHaveBeenCalledWith(
      tenderId,
      "3".repeat(32),
    );
  });

  it("opens the change review from the current action and records the exact decision", async () => {
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      current_action: reviewChangeAction(),
    });
    host.inspectChangeAssessments.mockResolvedValue(changeAssessmentPage());
    host.decideChangeAssessment.mockResolvedValue({
      ...addendumAssessment(),
      status: "rework_required",
    });

    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Review change" }),
    );

    const surface = await screen.findByTestId("change-review");
    expect(within(surface).getByText("01 Instructions/ITT.pdf")).toBeTruthy();
    expect(within(surface).getByText("02 Addenda/Addendum-1.pdf")).toBeTruthy();
    expect(within(surface).getByText("What is now out of date")).toBeTruthy();
    expect(within(surface).getByText("Affected work")).toBeTruthy();
    expect(within(surface).getByText(/cites the prior source/)).toBeTruthy();
    expect(within(surface).getByLabelText("Classification")).toBeTruthy();

    fireEvent.change(within(surface).getByLabelText(/Decision rationale/), {
      target: { value: "The addendum governs; rework the affected records." },
    });
    fireEvent.click(
      within(surface).getByRole("button", { name: "Record decision" }),
    );

    await waitFor(() => {
      expect(host.decideChangeAssessment).toHaveBeenCalledWith(
        tenderId,
        addendumAssessmentId,
        addendumAssessmentManifest,
        "material",
        "The addendum governs; rework the affected records.",
      );
    });
    await waitFor(() => {
      expect(screen.queryByTestId("change-review")).toBeNull();
    });
    await waitFor(() => {
      expect(host.inspectManagerWorkspace.mock.calls.length).toBeGreaterThan(1);
    });
  });

  it("shows the Manager change summary when an addendum opens a change assessment", async () => {
    const summary = changeSummaryMessage();
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
      current_action: reviewChangeAction(),
      conversation: {
        conversation_id: "c".repeat(32),
        latest_meaningful_message_id: summary.message_id,
        messages: [projection.conversation!.messages[0], summary],
      },
    });
    host.inspectChangeAssessments.mockResolvedValue(changeAssessmentPage());

    render(<ManagerWorkspace />);

    expect(
      await screen.findByText(
        /A new addendum for '01 Instructions\/ITT\.pdf' arrived/,
      ),
    ).toBeTruthy();
    expect(
      screen.getByText(/It makes 1 tender record out of date/),
    ).toBeTruthy();
    expect(
      await screen.findByRole("button", { name: "Review change" }),
    ).toBeTruthy();
  });

  function openWorkView(): Promise<HTMLElement> {
    return screen.findByRole("heading", { name: "West Campus MEP", level: 1 });
  }

  it("shows the six plain-language work groups with Paused and Failed named exactly", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(
      workTasksProjection(sixStateTasks()),
    );

    render(<ManagerWorkspace />);
    await openWorkView();
    fireEvent.click(
      within(
        screen.getByRole("complementary", { name: "Tender workspace" }),
      ).getByRole("button", { name: "Work" }),
    );

    for (const label of [
      "Waiting",
      "Working",
      "Needs you",
      "Paused",
      "Done",
      "Failed",
    ]) {
      expect(
        screen.getByRole("heading", { name: label, level: 3 }),
      ).toBeTruthy();
    }
    expect(
      screen.queryByRole("heading", { name: "Needs attention" }),
    ).toBeNull();

    const waiting = screen.getByRole("region", { name: "Waiting" });
    expect(within(waiting).getByText("Review the ground risks.")).toBeTruthy();
    expect(
      within(waiting).getByText("Waiting for: cost estimation production"),
    ).toBeTruthy();
    expect(
      within(waiting).queryByText("Price the tender scenarios."),
    ).toBeNull();
    const paused = screen.getByRole("region", { name: "Paused" });
    expect(
      within(paused).getByText("Price the tender scenarios."),
    ).toBeTruthy();
    expect(
      within(paused).getByText(
        "Paused because the Work Plan changed. The Manager will resume or re-plan it.",
      ),
    ).toBeTruthy();
  });

  it("renders Done work quietly without loud accents", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(
      workTasksProjection(sixStateTasks()),
    );

    const { container } = render(<ManagerWorkspace />);
    await openWorkView();
    fireEvent.click(
      within(
        screen.getByRole("complementary", { name: "Tender workspace" }),
      ).getByRole("button", { name: "Work" }),
    );

    const doneGroup = screen.getByRole("region", { name: "Done" });
    const quietRow = within(doneGroup)
      .getByText("Analyse the tender package.")
      .closest("li");
    expect(quietRow?.classList.contains("is-quiet")).toBe(true);
    expect(
      quietRow?.querySelector('.workspace-task__state[data-state="done"]'),
    ).toBeTruthy();
    expect(container.querySelectorAll("li.is-quiet")).toHaveLength(1);
  });

  it("opens a focused task detail with exact context, evidence, and the current output", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(
      workTasksProjection(sixStateTasks()),
    );
    host.inspectTenderProduction.mockResolvedValue(
      focusedProductionInspection(),
    );
    host.inspectProductionTaskReview.mockResolvedValue(focusedTaskReview());

    render(<ManagerWorkspace />);
    await openWorkView();
    fireEvent.click(
      within(
        screen.getByRole("complementary", { name: "Tender workspace" }),
      ).getByRole("button", { name: "Work" }),
    );

    fireEvent.click(
      screen.getByRole("button", { name: /Take off the BOQ quantities/ }),
    );

    expect(await screen.findByTestId("work-task-detail")).toBeTruthy();
    expect(
      await screen.findByText(`${evidenceArtifactId}#7 · v2`),
    ).toBeTruthy();
    expect(screen.getByText(`${evidenceArtifactId}#7`)).toBeTruthy();
    expect(
      screen.getByText("Ground-floor slab takeoff on a C30/37 basis."),
    ).toBeTruthy();
    expect(screen.getByText("Output v1 · latest")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Back to Work" }));
    await waitFor(() =>
      expect(screen.queryByTestId("work-task-detail")).toBeNull(),
    );
    expect(screen.getByRole("heading", { name: "Work" })).toBeTruthy();
  });

  it("stops an in-flight run only after explaining what depends on it", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(
      workTasksProjection(sixStateTasks()),
    );
    host.inspectTenderProduction.mockResolvedValue(
      focusedProductionInspection(),
    );
    host.inspectProductionTaskReview.mockResolvedValue(focusedTaskReview());
    host.interruptAgentRun.mockResolvedValue(true);

    render(<ManagerWorkspace />);
    await openWorkView();
    fireEvent.click(
      within(
        screen.getByRole("complementary", { name: "Tender workspace" }),
      ).getByRole("button", { name: "Work" }),
    );

    fireEvent.click(
      screen.getByRole("button", { name: /Take off the BOQ quantities/ }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Stop" }));

    const consequence = await screen.findByRole("alert");
    expect(
      within(consequence).getByText(
        /1 task waits for this work: Review the estimate\./,
      ),
    ).toBeTruthy();
    expect(
      within(consequence).getByText(
        /Stopping now records the outcome as it stands/,
      ),
    ).toBeTruthy();
    expect(host.interruptAgentRun).not.toHaveBeenCalled();

    fireEvent.click(
      within(consequence).getByRole("button", { name: "Stop the work" }),
    );
    await waitFor(() => {
      expect(host.interruptAgentRun).toHaveBeenCalledWith(
        tenderId,
        "5".repeat(32),
      );
    });
  });

  it("routes scope changes to the Manager conversation instead of editing the task", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(
      workTasksProjection(sixStateTasks()),
    );
    host.inspectTenderProduction.mockResolvedValue(
      focusedProductionInspection(),
    );
    host.inspectProductionTaskReview.mockResolvedValue(focusedTaskReview());
    host.recordEngineerWorkspaceMessage.mockResolvedValue(
      workTasksProjection(sixStateTasks()),
    );

    render(<ManagerWorkspace />);
    await openWorkView();
    fireEvent.click(
      within(
        screen.getByRole("complementary", { name: "Tender workspace" }),
      ).getByRole("button", { name: "Work" }),
    );

    fireEvent.click(
      screen.getByRole("button", { name: /Take off the BOQ quantities/ }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Request a change through the Manager",
      }),
    );

    await waitFor(() => {
      expect(host.recordEngineerWorkspaceMessage).toHaveBeenCalledWith(
        tenderId,
        expect.stringContaining("boq_takeoff"),
        [],
        [],
      );
    });
    expect(
      await screen.findByRole("textbox", {
        name: "Message your Tendering Manager",
      }),
    ).toBeTruthy();
    expect(screen.queryByTestId("work-task-detail")).toBeNull();
  });

  it("opens Team working as a full room and closing returns to the same Manager context", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());

    render(<ManagerWorkspace />);
    const managerComposer = await screen.findByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    fireEvent.change(managerComposer, {
      target: { value: "Draft for the Manager" },
    });

    await openTeamRoom();

    expect(
      screen.queryByRole("textbox", {
        name: "Message your Tendering Manager",
      }),
    ).toBeNull();
    expect(
      screen.getByRole("log", { name: "Team room conversation" }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open workroom" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Back to Manager" }));

    const restoredComposer = await screen.findByRole("textbox", {
      name: "Message your Tendering Manager",
    });
    expect(restoredComposer).toHaveProperty("value", "Draft for the Manager");
    expect(
      screen.getByText(
        "Which concrete class applies to the ground-floor slab?",
      ),
    ).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Team working" })).toBeNull();
  });

  it("exposes Needs you, Handoffs, Outputs, and All messages filters in the room", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());

    render(<ManagerWorkspace />);
    await openTeamRoom();
    const filters = screen.getByRole("group", { name: "Message filters" });
    for (const label of ["All messages", "Needs you", "Handoffs", "Outputs"]) {
      expect(within(filters).getByRole("button", { name: label })).toBeTruthy();
    }
    expect(
      within(filters)
        .getByRole("button", { name: "All messages" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    const log = screen.getByRole("log", { name: "Team room conversation" });
    expect(
      within(log).getByText(
        "Which concrete class applies to the ground-floor slab?",
      ),
    ).toBeTruthy();

    fireEvent.click(within(filters).getByRole("button", { name: "Handoffs" }));
    expect(
      within(log).getByText(
        "Handing the lift-pit dimensions to the estimator.",
      ),
    ).toBeTruthy();
    expect(
      within(log).queryByText(
        "Which concrete class applies to the ground-floor slab?",
      ),
    ).toBeNull();

    fireEvent.click(within(filters).getByRole("button", { name: "Outputs" }));
    expect(
      within(log).getByText(
        "Cost estimate v2 is recorded and ready for your review.",
      ),
    ).toBeTruthy();
    expect(
      within(log).queryByText(
        "Handing the lift-pit dimensions to the estimator.",
      ),
    ).toBeNull();

    fireEvent.click(within(filters).getByRole("button", { name: "Needs you" }));
    expect(
      within(log).getByText(
        "Which concrete class applies to the ground-floor slab?",
      ),
    ).toBeTruthy();
    expect(
      within(log).getByText("The pumphouse duty pump schedule is missing."),
    ).toBeTruthy();
    expect(within(log).queryByText("Understood, keep me posted.")).toBeNull();

    fireEvent.click(
      within(filters).getByRole("button", { name: "All messages" }),
    );
    expect(within(log).getByText("Understood, keep me posted.")).toBeTruthy();
  });

  it("groups older messages under period headers while today flows chronologically", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());

    render(<ManagerWorkspace />);
    await openTeamRoom();
    const log = screen.getByRole("log", { name: "Team room conversation" });
    const threeDaysAgoHeading = new Intl.DateTimeFormat(undefined, {
      dateStyle: "long",
    }).format(
      (() => {
        const date = new Date();
        date.setDate(date.getDate() - 3);
        return date;
      })(),
    );
    expect(within(log).getByText("Yesterday")).toBeTruthy();
    expect(within(log).getByText(threeDaysAgoHeading)).toBeTruthy();
    expect(within(log).queryByText(/^Today$/)).toBeNull();

    const olderMessage = within(log)
      .getByText("Handing the lift-pit dimensions to the estimator.")
      .closest("article")!;
    const newerMessage = within(log)
      .getByText("Cost estimate v2 is recorded and ready for your review.")
      .closest("article")!;
    expect(
      olderMessage.compareDocumentPosition(newerMessage) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("sends the Engineer message from the room composer into the same conversation", async () => {
    installResponsiveMatchMedia(1440);
    const updated = teamRoomProjection();
    updated.conversation!.latest_meaningful_message_id = "msg-reply";
    updated.conversation!.messages.push(
      teamRoomMessage({
        message_id: "msg-reply",
        sequence: 7,
        author: "engineer",
        body: "Please confirm the pump schedule with the supplier.",
      }),
    );
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());
    host.recordEngineerWorkspaceMessage.mockResolvedValue(updated);

    render(<ManagerWorkspace />);
    await openTeamRoom();
    const composer = screen.getByRole("textbox", { name: "Message the Team" });
    fireEvent.change(composer, {
      target: { value: "Please confirm the pump schedule with the supplier." },
    });
    fireEvent.keyDown(composer, { key: "Enter" });

    await waitFor(() => {
      expect(host.recordEngineerWorkspaceMessage).toHaveBeenCalledWith(
        tenderId,
        "Please confirm the pump schedule with the supplier.",
        [],
        [],
      );
    });
    expect(
      await screen.findByText(
        "Please confirm the pump schedule with the supplier.",
      ),
    ).toBeTruthy();
    expect(composer).toHaveProperty("value", "");
  });

  it("renders attributable messages with their exact references in the room", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());

    render(<ManagerWorkspace />);
    await openTeamRoom();
    const log = screen.getByRole("log", { name: "Team room conversation" });
    expect(
      within(log).getAllByText("Tendering Manager").length,
    ).toBeGreaterThan(0);
    expect(within(log).getAllByText("You").length).toBeGreaterThan(0);
    expect(within(log).getByText("Question")).toBeTruthy();
    expect(within(log).getByText("Handoff")).toBeTruthy();
    expect(within(log).getByText("Blocker")).toBeTruthy();
    expect(within(log).getByText("Output")).toBeTruthy();

    expect(within(log).getByText("Slab concrete strength")).toBeTruthy();
    expect(
      within(log).getByRole("button", { name: /Review evidence/ }),
    ).toBeTruthy();
    expect(
      within(log).getAllByRole("button", { name: /Review record/ }).length,
    ).toBeGreaterThan(0);
    expect(
      within(log).getByText("Produce the verified estimate."),
    ).toBeTruthy();
    expect(within(log).getByText("Cost estimate v2")).toBeTruthy();
  });

  const workroomProviderSelection = {
    connection_id: "codex_chatgpt",
    provider: "codex" as const,
    model_id: "gpt-live-a",
    reasoning: { kind: "provider_default" } as const,
    catalogue_fetched_at: "2026-08-30T09:00:00Z",
    adapter_version: "1",
  };

  function workroomAgentRun(
    runId: string,
    overrides: Partial<AgentRunInspection> = {},
  ): AgentRunInspection {
    return {
      run_id: runId,
      retry_of_run_id: null,
      linked_retry_supported: false,
      state: "running",
      provider_selection: workroomProviderSelection,
      profile: {
        profile_id: "e".repeat(32),
        version: 1,
        identity: "Cost Estimator",
        profession: "Senior Construction Cost Estimator",
        seniority: "senior",
        capabilities: ["cost_estimation"],
        objective: "Produce the verified estimate.",
        behavior: "Works only from verified records.",
        skepticism: "Challenges unsupported inputs.",
        risk_tolerance: "low",
        instructions:
          "Estimate only from the supplied data views; never quote unregistered figures.",
        output_contract_json: "{}",
        review_policy: "self_review",
        permissions: {
          data_scopes: ["estimate"],
          data_classifications: ["tender_internal" as const],
          allowed_actions: ["read"],
          allowed_tools: [],
          network_allowed: false,
          workspace_write_allowed: false,
        },
        prohibited_actions: [],
        resource_budget: planBudget,
      },
      task: {
        task_id: "2".repeat(32),
        profile_id: "e".repeat(32),
        profile_version: 1,
        objective: "Produce the verified estimate.",
        exact_inputs: [
          {
            kind: "artifact",
            reference: "artifact-000000000000000000000000000",
            version: 2,
          },
        ],
        output_contract_json: "{}",
        review_policy: "self_review",
        deadline: "2026-09-15T00:00:00Z",
        permissions: {
          data_scopes: ["estimate"],
          data_classifications: ["tender_internal" as const],
          allowed_actions: ["read"],
          allowed_tools: [],
          network_allowed: false,
          workspace_write_allowed: false,
        },
        resource_budget: planBudget,
        repair_feedback: null,
      },
      permission_grant: {
        grant_id: "g".repeat(32),
        policy_version: 1,
        capability_catalogue_version: 1,
        work_plan_version: 1,
        profile_id: "e".repeat(32),
        profile_version: 1,
        task_id: "2".repeat(32),
        purpose: "Produce the verified estimate.",
        data_scopes: ["estimate"],
        data_classifications: ["tender_internal" as const],
        allowed_actions: ["read"],
        typed_tools: [],
        network_allowed: false,
        workspace_write_allowed: false,
        data_views: [
          {
            view_id: "view-1",
            schema_version: 1,
            relative_path: "views/boq-summary.csv",
            sha256: "f".repeat(64),
            data_scope: "estimate",
            data_classification: "tender_internal" as const,
            exact_inputs: [],
          },
        ],
        thread_exposure: {
          exact_inputs: [
            {
              kind: "artifact",
              reference: "artifact-thread-exposed",
              version: 2,
            },
          ],
          data_scopes: ["estimate"],
          data_classifications: ["tender_internal" as const],
        },
        workspace: {
          workspace_id: "w".repeat(32),
          read_only_inputs: "read-only inputs",
          working_area: "working area",
          staged_outputs: "staged outputs",
        },
        access_ceiling: {
          exact_inputs: [],
          data_scopes: ["estimate", "schedule"],
          data_classifications: [
            "tender_internal" as const,
            "sensitive" as const,
          ],
          allowed_actions: ["read", "write"],
          allowed_tools: ["boq_editor"],
        },
        resource_budget: planBudget,
        issued_at: "2026-08-30T09:00:00Z",
        expires_at: "2026-08-31T09:00:00Z",
      },
      access_requests: [],
      provider_thread_ref: "thread-1",
      provider_turn_ref: null,
      events: [],
      usage: {
        input_tokens: 10n,
        cached_input_tokens: null,
        output_tokens: 5n,
        reasoning_output_tokens: null,
        total_tokens: 15n,
        context_window: null,
        elapsed_milliseconds: 1_000n,
        rate_limit: null,
      },
      failure: null,
      proposed_result: null,
      recovery_decision: null,
      started_at: "2026-08-30T09:10:00Z",
      completed_at: null,
      ...overrides,
    };
  }

  function workroomHistoryRun(
    runId: string,
    overrides: Partial<AgentRunSummary> = {},
  ): AgentRunSummary {
    return {
      run_id: runId,
      retry_of_run_id: null,
      has_linked_retry: false,
      linked_retry_supported: false,
      state: "completed",
      provider_selection: workroomProviderSelection,
      profile_identity: "Cost Estimator",
      profile_profession: "Senior Construction Cost Estimator",
      profile_version: 1,
      task_id: "2".repeat(32),
      provider_thread_ref: "thread-1",
      provider_turn_ref: null,
      usage: {
        input_tokens: 10n,
        cached_input_tokens: null,
        output_tokens: 5n,
        reasoning_output_tokens: null,
        total_tokens: 15n,
        context_window: null,
        elapsed_milliseconds: 1_000n,
        rate_limit: null,
      },
      failure: null,
      has_proposed_result: false,
      recovery_decision: null,
      started_at: "2026-08-28T09:00:00Z",
      completed_at: "2026-08-28T09:30:00Z",
      ...overrides,
    };
  }

  function workroomHistoryPage(runs: AgentRunSummary[]): AgentRunHistoryPage {
    return {
      items: runs.map((run, index) => ({
        run_sequence: BigInt(index + 1),
        run,
      })),
      next_before_sequence: null,
      total_count: BigInt(runs.length),
    };
  }

  async function openAgentWorkroom() {
    fireEvent.click(screen.getByRole("button", { name: "Open workroom" }));
    const workroom = await screen.findByLabelText("Agent workroom");
    fireEvent.click(within(workroom).getByRole("button", { name: "Context" }));
    return workroom;
  }

  it("opens the workroom from the Agent identity", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());
    host.inspectAgentRun.mockResolvedValue(workroomAgentRun("5".repeat(32)));
    host.inspectAgentRunHistory.mockResolvedValue(workroomHistoryPage([]));

    render(<ManagerWorkspace />);
    await openTeamRoom();

    const identityButtons = screen.getAllByRole("button", {
      name: /Cost Estimator/,
    });
    expect(identityButtons).toHaveLength(1);
    fireEvent.click(identityButtons[0]);

    expect(host.inspectAgentRun).toHaveBeenCalledWith(tenderId, "5".repeat(32));
    const workroom = await screen.findByLabelText("Agent workroom");
    expect(within(workroom).getByText("Cost Estimator")).toBeTruthy();
    expect(
      within(workroom).getByText("Produce the verified estimate."),
    ).toBeTruthy();
    expect(
      within(workroom)
        .getByRole("button", { name: "Conversation" })
        .getAttribute("aria-current"),
    ).toBe("page");
    expect(
      within(workroom).getByRole("button", { name: "Close" }),
    ).toBeTruthy();
  });

  it("shows the exact supplied context for the selected run", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());
    host.inspectAgentRun.mockResolvedValue(workroomAgentRun("5".repeat(32)));
    host.inspectAgentRunHistory.mockResolvedValue(workroomHistoryPage([]));

    render(<ManagerWorkspace />);
    await openTeamRoom();
    const workroom = await openAgentWorkroom();

    const supplied = within(workroom).getByLabelText(
      "What this run actually received",
    );
    expect(
      within(supplied).getByText(
        "Estimate only from the supplied data views; never quote unregistered figures.",
      ),
    ).toBeTruthy();
    expect(
      within(supplied).getByText("artifact-000000000000000000000000000"),
    ).toBeTruthy();
    expect(within(supplied).getByText("views/boq-summary.csv")).toBeTruthy();
    expect(within(supplied).getByText("estimate")).toBeTruthy();
    expect(within(supplied).getByText(/artifact-thread-exposed/)).toBeTruthy();
    expect(within(supplied).getByText("Data scopes: estimate")).toBeTruthy();
    expect(
      within(supplied).getByText("Classifications: tender_internal"),
    ).toBeTruthy();
  });

  it("keeps the permission ceiling visually distinct from the supplied context", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());
    host.inspectAgentRun.mockResolvedValue(workroomAgentRun("5".repeat(32)));
    host.inspectAgentRunHistory.mockResolvedValue(workroomHistoryPage([]));

    render(<ManagerWorkspace />);
    await openTeamRoom();
    const workroom = await openAgentWorkroom();

    const supplied = within(workroom).getByLabelText(
      "What this run actually received",
    );
    const requests = within(workroom).getByLabelText(
      "Requested but not granted",
    );
    const ceiling = within(workroom).getByLabelText(
      "What this Agent could request",
    );
    expect(supplied.className).toBe("agent-workroom__supplied");
    expect(requests.className).toBe("agent-workroom__requests");
    expect(ceiling.className).toBe("agent-workroom__ceiling");
    expect(supplied.contains(ceiling)).toBe(false);
    expect(requests.contains(ceiling)).toBe(false);
    expect(within(ceiling).getByText("estimate, schedule")).toBeTruthy();
    expect(within(ceiling).getByText("read, write")).toBeTruthy();
    expect(within(supplied).queryByText("estimate, schedule")).toBeNull();
  });

  it("lists prior runs for the same Agent and task from history", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());
    host.inspectAgentRun.mockResolvedValue(workroomAgentRun("5".repeat(32)));
    host.inspectAgentRunHistory.mockResolvedValue(
      workroomHistoryPage([
        workroomHistoryRun("5".repeat(32), {
          state: "running",
          completed_at: null,
        }),
        workroomHistoryRun("7".repeat(32), {
          profile_version: 2,
        }),
        workroomHistoryRun("9".repeat(32), {
          task_id: "3".repeat(32),
        }),
      ]),
    );

    render(<ManagerWorkspace />);
    await openTeamRoom();
    const workroom = await openAgentWorkroom();

    expect(host.inspectAgentRunHistory).toHaveBeenCalledWith(tenderId, null, 4);
    const prior = within(workroom).getByLabelText("Prior runs");
    const list = await within(prior).findByRole("list");
    await waitFor(() => {
      expect(
        within(list).getByRole("button", { name: /completed/ }),
      ).toBeTruthy();
    });
    expect(within(list).getAllByRole("listitem")).toHaveLength(1);
    expect(within(list).queryByRole("button", { name: /running/ })).toBeNull();
  });

  it("compares context with a selected prior run and highlights changes", async () => {
    const priorRunId = "7".repeat(32);
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(teamRoomProjection());
    const currentRun = workroomAgentRun("5".repeat(32));
    host.inspectAgentRun.mockImplementation(
      (_tenderId: string, runId: string) =>
        Promise.resolve(
          runId === priorRunId
            ? workroomAgentRun(priorRunId, {
                state: "completed",
                started_at: "2026-08-28T09:00:00Z",
                completed_at: "2026-08-28T09:30:00Z",
                profile: {
                  ...currentRun.profile,
                  version: 2,
                  instructions:
                    "Estimate only from the prior data views; ask before using schedule scopes.",
                },
                permission_grant: {
                  ...currentRun.permission_grant,
                  data_scopes: ["estimate", "schedule"],
                  thread_exposure: {
                    ...currentRun.permission_grant.thread_exposure,
                    data_scopes: ["estimate", "schedule"],
                  },
                },
              })
            : currentRun,
        ),
    );
    host.inspectAgentRunHistory.mockResolvedValue(
      workroomHistoryPage([
        workroomHistoryRun(priorRunId, { profile_version: 2 }),
      ]),
    );

    render(<ManagerWorkspace />);
    await openTeamRoom();
    const workroom = await openAgentWorkroom();

    const prior = within(workroom).getByLabelText("Prior runs");
    fireEvent.click(
      await within(prior).findByRole("button", { name: /completed/ }),
    );

    const comparisonHeading = await within(workroom).findByText(
      "What changed against the selected prior run",
    );
    const comparison = comparisonHeading.closest("div") as HTMLElement;
    expect(within(comparison).getAllByText("Changed")).toHaveLength(3);
    expect(within(comparison).getAllByText("Same")).toHaveLength(2);
    expect(
      within(comparison).getByText(
        "Estimate only from the prior data views; ask before using schedule scopes.",
      ),
    ).toBeTruthy();
    expect(within(comparison).getByText("estimate, schedule")).toBeTruthy();
    expect(
      prior
        .querySelector('button[aria-current="true"]')
        ?.getAttribute("aria-current"),
    ).toBe("true");
  });

  function estimateBasisFixture(
    overrides: Partial<BasisOfEstimateVersion> = {},
  ): BasisOfEstimateVersion {
    return {
      basis_id: "3".repeat(32),
      version: 1,
      tender_revision: 4,
      author_run_id: "7".repeat(32),
      author_profile_id: "e".repeat(32),
      author_profile_version: 1,
      scope: "Ground-floor concrete works",
      pricing_date: "2026-08-10",
      currencies: ["EGP"],
      taxes: [],
      rate_sources: ["Supplier quotation"],
      productivity: ["One crew basis"],
      design_maturity: "detailed_design",
      gaps: [],
      exclusions: ["VAT"],
      supersedes_basis_manifest_sha256: null,
      remediates_review_manifest_sha256: null,
      boq_inventory_sha256: "a".repeat(64),
      query_inventory_sha256: "b".repeat(64),
      query_inventory: [],
      boq_rows: [
        {
          row_key: "BOQ-001",
          description: "Cable containment trunking",
          disposition: "priced" as const,
          evidence: [],
          calculation_run_id: "4".repeat(32),
          affected_queries: [],
        },
      ],
      cbs_components: [],
      resource_build_ups: [],
      quotations: [],
      allowances: [],
      material_assumptions: [],
      comparison_total_calculation_run_id: "6".repeat(32),
      aggregate_calculation: {
        aggregate_run_id: "1".repeat(32),
        author_run_id: "7".repeat(32),
        comparison_total_calculation_run_id: "6".repeat(32),
        comparison_total_manifest_sha256: "0".repeat(64),
        comparison_total_amount: "12345.68",
        rule_id: "1".repeat(32),
        rule_version: 1,
        rule_approval_id: "2".repeat(32),
        scenario_id: "9".repeat(32),
        scenario_version: 1,
        precision: 2,
        rounding_mode: "midpoint_away_from_zero" as const,
        engine_version: "calc-engine-1",
        inputs: [
          {
            build_up_id: "2".repeat(32),
            cbs_component_id: "5".repeat(32),
            calculation_run_id: "4".repeat(32),
            calculation_manifest_sha256: "0".repeat(64),
            amount: "12345.68",
            currency: "EGP",
          },
        ],
        final_amount: "12345.68",
        currency: "EGP",
        manifest_sha256: "0".repeat(64),
        approved_for_reliance: false,
      },
      total_amount: "12345.68",
      total_currency: "EGP",
      complete: true,
      reconciled: true,
      blockers: [],
      current: true,
      relied_upon: false,
      review: null,
      approval: null,
      manifest_sha256: "d".repeat(64),
      created_at: "2026-08-30T09:30:00Z",
      ...overrides,
    };
  }

  function estimateWorkspaceFixture(
    basis: BasisOfEstimateVersion | null,
  ): EstimateWorkspaceInspection {
    return {
      basis,
      boq_table_candidates: [],
      boq_table_candidate_next_cursor: null,
      basis_offset: 0,
      total_basis_version_count: basis ? 1 : 0,
      has_newer_basis: false,
      has_older_basis: false,
    };
  }

  function calculationWorkspaceFixture(): CalculationWorkspaceInspection {
    return {
      rule: null,
      recent_scenarios: [
        {
          scenario_id: "9".repeat(32),
          version: 1,
          name: "BOQ account base",
          quantity_unit: "mm",
          rate_basis_unit: "m",
          rate_currency: "USD",
          exchange_rate_id: "8".repeat(32),
          exchange_rate_version: 1,
          exchange_rate: {
            state: "provided" as const,
            value: "50",
            evidence: [],
          },
          exchange_rate_effective_date: "2026-08-01",
          pricing_date: "2026-08-10",
          exchange_rate_type: "spot" as const,
          output_currency: "EGP",
          rounding_policy_id: "7".repeat(32),
          rounding_policy_version: 1,
          precision: 2,
          rounding_mode: "midpoint_away_from_zero" as const,
          rationale: "Bind the exact pricing basis.",
          approved_by: "engineer_user",
          acting_role: "engineer_in_the_loop",
          manifest_sha256: "c".repeat(64),
          created_at: "2026-08-30T08:00:00Z",
        },
        {
          scenario_id: "6".repeat(32),
          version: 1,
          name: "Escalated FX",
          quantity_unit: "mm",
          rate_basis_unit: "m",
          rate_currency: "USD",
          exchange_rate_id: "8".repeat(32),
          exchange_rate_version: 1,
          exchange_rate: {
            state: "provided" as const,
            value: "55",
            evidence: [],
          },
          exchange_rate_effective_date: "2026-09-01",
          pricing_date: "2026-09-10",
          exchange_rate_type: "budget" as const,
          output_currency: "EGP",
          rounding_policy_id: "7".repeat(32),
          rounding_policy_version: 1,
          precision: 2,
          rounding_mode: "midpoint_away_from_zero" as const,
          rationale: "Escalated exchange-rate basis.",
          approved_by: "engineer_user",
          acting_role: "engineer_in_the_loop",
          manifest_sha256: "c".repeat(64),
          created_at: "2026-08-30T08:30:00Z",
        },
      ],
      recent_runs: [
        {
          calculation_run_id: "4".repeat(32),
          cost_estimator_run_id: "5".repeat(32),
          tender_revision: 4,
          rule_id: "1".repeat(32),
          rule_version: 1,
          rule_approval_id: "2".repeat(32),
          description: "BOQ-001 cable containment",
          scenario_id: "9".repeat(32),
          scenario_version: 1,
          scenario_name: "BOQ account base",
          scenario_manifest_sha256: "c".repeat(64),
          exchange_rate_id: "8".repeat(32),
          exchange_rate_version: 1,
          rounding_policy_id: "7".repeat(32),
          rounding_policy_version: 1,
          quantity: {
            state: "provided" as const,
            value: "12000",
            evidence: [],
          },
          quantity_unit: "mm",
          unit_rate: { state: "provided" as const, value: "0.5", evidence: [] },
          rate_basis_unit: "m",
          rate_currency: "USD",
          exchange_rate: {
            state: "provided" as const,
            value: "50",
            evidence: [],
          },
          exchange_rate_effective_date: "2026-08-01",
          pricing_date: "2026-08-10",
          exchange_rate_type: "spot" as const,
          output_currency: "EGP",
          precision: 2,
          rounding_mode: "midpoint_away_from_zero" as const,
          engine_version: "calc-engine-1",
          normalized_quantity: "12",
          unrounded_source_amount: "6000.00",
          unrounded_output_amount: "300000.0000",
          final_amount: "300000.00",
          status: "completed" as const,
          diagnostic_code: null,
          manifest_sha256: "d".repeat(64),
          approval: null,
          created_at: "2026-08-30T09:30:00Z",
        },
        {
          calculation_run_id: "0".repeat(32),
          cost_estimator_run_id: "a".repeat(32),
          tender_revision: 4,
          rule_id: "1".repeat(32),
          rule_version: 1,
          rule_approval_id: "2".repeat(32),
          description: "BOQ-001 cable containment at escalated FX",
          scenario_id: "6".repeat(32),
          scenario_version: 1,
          scenario_name: "Escalated FX",
          scenario_manifest_sha256: "c".repeat(64),
          exchange_rate_id: "8".repeat(32),
          exchange_rate_version: 1,
          rounding_policy_id: "7".repeat(32),
          rounding_policy_version: 1,
          quantity: {
            state: "provided" as const,
            value: "12000",
            evidence: [],
          },
          quantity_unit: "mm",
          unit_rate: { state: "provided" as const, value: "0.5", evidence: [] },
          rate_basis_unit: "m",
          rate_currency: "USD",
          exchange_rate: {
            state: "provided" as const,
            value: "55",
            evidence: [],
          },
          exchange_rate_effective_date: "2026-09-01",
          pricing_date: "2026-09-10",
          exchange_rate_type: "budget" as const,
          output_currency: "EGP",
          precision: 2,
          rounding_mode: "midpoint_away_from_zero" as const,
          engine_version: "calc-engine-1",
          normalized_quantity: "12",
          unrounded_source_amount: "6000.00",
          unrounded_output_amount: "330000.0000",
          final_amount: "330000.00",
          status: "completed" as const,
          diagnostic_code: null,
          manifest_sha256: "d".repeat(64),
          approval: null,
          created_at: "2026-08-30T10:30:00Z",
        },
      ],
      total_scenario_count: 2,
      total_run_count: 2,
      scenario_offset: 0,
      run_offset: 0,
      has_older_scenarios: false,
      has_older_runs: false,
    };
  }

  function estimateReviewProjection(): ManagerWorkspaceProjection {
    return {
      ...projection,
      selected_tender: {
        ...projection.selected_tender!,
        phase: "active_production" as const,
      },
      current_action: {
        kind: "review_basis_of_estimate" as const,
        title: "Approve the Basis of Estimate",
        summary:
          "The independent review passed. Your approval binds the exact basis version before any pricing may rely on it.",
        action_label: "Review estimate basis",
        requires_engineer: true,
      },
      estimate: {
        basis_id: "3".repeat(32),
        version: 1,
        status: "awaiting_approval" as const,
        boq_row_count: 1,
        finding_count: 1,
        calculation_run_count: 3,
      },
    };
  }

  it("opens the Basis of Estimate review from the action and approves bound to exact versions", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(estimateReviewProjection());
    host.inspectEstimateWorkspace.mockResolvedValue(
      estimateWorkspaceFixture(
        estimateBasisFixture({
          review: {
            review_id: "8".repeat(32),
            reviewer_run_id: "9".repeat(32),
            reviewer_profile_id: "f".repeat(32),
            reviewer_profile_version: 1,
            outcome: "passed" as const,
            findings: [
              {
                code: "minor_rounding_note",
                summary:
                  "One BOQ row rounds within the approved midpoint policy.",
                affected_boq_row_keys: ["BOQ-001"],
              },
            ],
            manifest_sha256: "b".repeat(64),
            created_at: "2026-08-30T10:00:00Z",
          },
        }),
      ),
    );

    render(<ManagerWorkspace />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Review estimate basis" }),
    );
    const surface = await screen.findByTestId("tender-estimate-review");
    expect(
      await within(surface).findByText("Cable containment trunking"),
    ).toBeTruthy();
    expect(within(surface).getByText("Reconciled total:")).toBeTruthy();
    expect(within(surface).getByText("12345.68 EGP")).toBeTruthy();
    const findings = await within(surface).findByTestId(
      "estimate-review-findings",
    );
    expect(within(findings).getByText("minor_rounding_note")).toBeTruthy();
    expect(
      within(findings).getByText(
        "One BOQ row rounds within the approved midpoint policy.",
      ),
    ).toBeTruthy();
    expect(within(findings).getByText(/Affected rows: BOQ-001/)).toBeTruthy();

    fireEvent.change(within(surface).getByLabelText("Approval rationale"), {
      target: {
        value:
          "The reviewed basis asks for the exact reconciliation the bid needs.",
      },
    });
    fireEvent.click(
      within(surface).getByRole("button", {
        name: "Approve basis for reliance",
      }),
    );
    await waitFor(() =>
      expect(host.approveBasisOfEstimate).toHaveBeenCalledWith({
        tender_id: tenderId,
        basis_id: "3".repeat(32),
        version: 1,
        manifest_sha256: "d".repeat(64),
        rationale:
          "The reviewed basis asks for the exact reconciliation the bid needs.",
      }),
    );
  });

  it("links the estimator task detail to the readable controlled calculation tables", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue(
      workTasksProjection([
        workTask({
          production_task_id: focusedProductionTaskId,
          task_key: "cost_estimation_production",
          objective: "Develop the evidence-linked estimate.",
          state: "working",
          status_detail: "running",
          current_run_id: "5".repeat(32),
        }),
      ]),
    );
    const estimatorProduction = focusedProductionInspection();
    estimatorProduction.tasks = [
      {
        ...estimatorProduction.tasks[0],
        task: {
          ...estimatorProduction.tasks[0].task,
          task_key: "cost_estimation_production",
          objective: "Develop the evidence-linked estimate.",
          exact_inputs: [
            {
              kind: "calculation_scenario_version",
              reference: "9".repeat(32),
              version: 1,
            },
          ],
        },
      },
    ];
    host.inspectTenderProduction.mockResolvedValue(estimatorProduction);
    host.inspectProductionTaskReview.mockResolvedValue(focusedTaskReview());
    host.inspectCalculationWorkspace.mockResolvedValue(
      calculationWorkspaceFixture(),
    );
    host.inspectEstimateWorkspace.mockResolvedValue(
      estimateWorkspaceFixture(estimateBasisFixture()),
    );

    render(<ManagerWorkspace />);
    await openWorkView();
    fireEvent.click(
      within(
        screen.getByRole("complementary", { name: "Tender workspace" }),
      ).getByRole("button", { name: "Work" }),
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: /Develop the evidence-linked estimate/,
      }),
    );
    const detail = await screen.findByTestId("work-task-detail");
    const estimating = await within(detail).findByTestId(
      "work-task-detail__estimating",
    );
    fireEvent.click(
      within(estimating).getByRole("button", {
        name: "Open controlled calculations",
      }),
    );

    const calculations = await screen.findByTestId("controlled-calculations");
    const differences = await within(calculations).findByTestId(
      "calculation-scenario-differences",
    );
    expect(within(differences).getByText("BOQ account base · v1")).toBeTruthy();
    expect(within(differences).getByText("Escalated FX · v1")).toBeTruthy();
    expect(within(differences).getByText("50 from 2026-08-01")).toBeTruthy();
    expect(within(differences).getByText("55 from 2026-09-01")).toBeTruthy();
    expect(within(calculations).getByText("300000.00 EGP")).toBeTruthy();
    expect(within(calculations).getByText("330000.00 EGP")).toBeTruthy();
    expect(within(calculations).getAllByText("12000 mm")).toHaveLength(2);
    expect(within(calculations).getAllByText("0.5 USD/m")).toHaveLength(2);
    expect(within(calculations).getByText("12345.68 EGP")).toBeTruthy();
    fireEvent.click(within(calculations).getByRole("button", { name: "Back" }));
    await waitFor(() =>
      expect(screen.queryByTestId("controlled-calculations")).toBeNull(),
    );
    expect(screen.getByTestId("work-task-detail")).toBeTruthy();
  });

  it("shows the Manager explanation when the estimator completes its publication", async () => {
    installResponsiveMatchMedia(1440);
    host.inspectManagerWorkspace.mockResolvedValue({
      ...projection,
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
            created_at: "2026-08-30T09:00:00Z",
            references: [],
          },
          {
            message_id: "m".repeat(32),
            sequence: 2,
            author: "manager",
            kind: "output",
            body: "The Cost Estimator completed the Basis of Estimate v1 with a reconciled total of 12345.68 EGP. Independent review is next.",
            created_at: "2026-08-30T10:00:00Z",
            references: [],
          },
        ],
      },
    });

    render(<ManagerWorkspace />);
    const conversation = await screen.findByRole("log", {
      name: "Tender conversation",
    });
    expect(
      await within(conversation).findByText(
        "The Cost Estimator completed the Basis of Estimate v1 with a reconciled total of 12345.68 EGP. Independent review is next.",
      ),
    ).toBeTruthy();
    expect(within(conversation).getByText("Output")).toBeTruthy();
  });
});
