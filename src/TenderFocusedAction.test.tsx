import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { TenderProductionInspection } from "./bindings/TenderProductionInspection";
import type { WorkPlanApprovalRecord } from "./bindings/WorkPlanApprovalRecord";
import type { WorkPlanProposalInspection } from "./bindings/WorkPlanProposalInspection";

const host = vi.hoisted(() => ({
  activateTenderProduction: vi.fn(),
  approveProductionFindingException: vi.fn(),
  composeTenderOffice: vi.fn(),
  decideWorkPlanProposal: vi.fn(),
  inspectCurrentWorkPlan: vi.fn(),
  inspectProductionTaskReview: vi.fn(),
  inspectTenderProduction: vi.fn(),
  interruptAgentRun: vi.fn(),
  reviseWorkPlanProposal: vi.fn(),
  runProductionTask: vi.fn(),
}));

vi.mock("./quantixHost", () => host);

async function loadFocusedAction() {
  vi.resetModules();
  vi.doMock("./BidDecisionPanel", () => ({
    BidDecisionPanel: ({
      onTenderStateChange,
    }: {
      onTenderStateChange: () => void;
    }) => (
      <section aria-label="Bid Decision panel">
        <button type="button" onClick={onTenderStateChange}>
          Accept Bid Decision
        </button>
      </section>
    ),
  }));
  vi.doMock("./TenderOfficePanel", () => ({
    TenderOfficePanel: ({
      onTenderStateChange,
    }: {
      onTenderStateChange: () => void;
    }) => (
      <section aria-label="Work Plan panel">
        <button type="button" onClick={onTenderStateChange}>
          Compose Work Plan
        </button>
        <button type="button" onClick={onTenderStateChange}>
          Approve Exact Work Plan
        </button>
        <button type="button" onClick={onTenderStateChange}>
          Activate Exact Work Plan
        </button>
      </section>
    ),
  }));
  return (await import("./TenderFocusedAction")).TenderFocusedAction;
}

async function loadFocusedActionWithRealWorkPlanPanel() {
  vi.resetModules();
  vi.doMock("./BidDecisionPanel", () => ({
    BidDecisionPanel: () => <section aria-label="Bid Decision panel" />,
  }));
  return (await import("./TenderFocusedAction")).TenderFocusedAction;
}

const estimatorProfileId = "e".repeat(32);
const reviewerProfileId = "f".repeat(32);
const coordinatorProfileId = "c".repeat(32);
const budget = {
  provider_turns: 1,
  duration_seconds: 120,
  output_bytes: 262_144,
};

function planProfile(
  profileId: string,
  identity: string,
  capabilities: string[],
  profession: string,
) {
  return {
    profile_id: profileId,
    version: 1,
    identity,
    profession,
    seniority: "senior",
    capabilities,
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
    resource_budget: budget,
  };
}

function workPlanInspection(): WorkPlanProposalInspection {
  return {
    plan_id: "p".repeat(32),
    version: 2,
    bid_package_id: "k".repeat(32),
    bid_package_version: 1,
    bid_package_manifest_sha256: "1".repeat(64),
    capability_catalogue_version: 1,
    permission_policy_version: 1,
    profiles: [
      {
        archetype: "tender_office_coordinator",
        status: "proposed" as const,
        profile: planProfile(
          coordinatorProfileId,
          "Tendering Manager",
          ["tender_coordination"],
          "Tendering Manager Agent",
        ),
      },
      {
        archetype: "cost_estimator",
        status: "proposed" as const,
        profile: planProfile(
          estimatorProfileId,
          "Cost Estimator",
          ["cost_estimation"],
          "Senior Construction Cost Estimator",
        ),
      },
      {
        archetype: "independent_cost_reviewer",
        status: "proposed" as const,
        profile: planProfile(
          reviewerProfileId,
          "Independent Cost Reviewer",
          ["review_cost_estimation"],
          "Senior Cost Assurance Surveyor",
        ),
      },
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
        resource_budget: budget,
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
        resource_budget: budget,
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

afterEach(() => {
  cleanup();
  vi.doUnmock("./BidDecisionPanel");
  vi.doUnmock("./TenderOfficePanel");
  vi.resetModules();
});

describe("TenderFocusedAction", () => {
  it("routes bid acceptance into Work Plan review and refreshes after every mutation", async () => {
    const TenderFocusedAction = await loadFocusedAction();
    const onManagerRefresh = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    const view = render(
      <TenderFocusedAction
        tenderId="tender-1"
        actionKind="review_bid_decision"
        runtimeReady
        reportCommandFailure={vi.fn()}
        onManagerRefresh={onManagerRefresh}
        onClose={onClose}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: "Accept Bid Decision" }),
    );
    expect(onManagerRefresh).toHaveBeenCalledTimes(1);

    view.rerender(
      <TenderFocusedAction
        tenderId="tender-1"
        actionKind="prepare_work_plan"
        runtimeReady
        reportCommandFailure={vi.fn()}
        onManagerRefresh={onManagerRefresh}
        onClose={onClose}
      />,
    );
    expect(screen.getByRole("heading", { name: "Work Plan" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Compose Work Plan" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Approve Exact Work Plan" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Activate Exact Work Plan" }),
    );
    expect(onManagerRefresh).toHaveBeenCalledTimes(4);
  });

  it("keeps the focused Work Plan route available for an activation retry", async () => {
    const TenderFocusedAction = await loadFocusedAction();
    const onManagerRefresh = vi.fn().mockResolvedValue(undefined);
    render(
      <TenderFocusedAction
        tenderId="tender-1"
        actionKind="review_work_plan"
        runtimeReady
        reportCommandFailure={vi.fn()}
        onManagerRefresh={onManagerRefresh}
        onClose={vi.fn()}
      />,
    );

    const activate = screen.getByRole("button", {
      name: "Activate Exact Work Plan",
    });
    fireEvent.click(activate);
    fireEvent.click(activate);
    expect(onManagerRefresh).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("heading", { name: "Work Plan" })).toBeTruthy();
  });

  it("renders the exact Work Plan sections and starts production right after approval", async () => {
    const TenderFocusedAction = await loadFocusedActionWithRealWorkPlanPanel();
    const plan = workPlanInspection();
    host.inspectCurrentWorkPlan.mockResolvedValue(plan);
    host.inspectTenderProduction.mockResolvedValue(null);
    const approved: WorkPlanProposalInspection = {
      ...plan,
      approval: {
        approval_id: "q".repeat(32),
        plan_id: plan.plan_id,
        plan_version: plan.version,
        decision: "approve" as const,
        rationale: "Approve and start.",
        plan_manifest_sha256: plan.manifest_sha256,
        decided_by: "engineer_user",
        acting_role: "tendering_manager",
        approval_sha256: "3".repeat(64),
        created_at: "2026-08-30T09:04:00Z",
      } satisfies WorkPlanApprovalRecord,
    };
    host.decideWorkPlanProposal.mockResolvedValue(approved);
    host.activateTenderProduction.mockResolvedValue(productionInspection(plan));
    const onManagerRefresh = vi.fn().mockResolvedValue(undefined);
    render(
      <TenderFocusedAction
        tenderId="tender-1"
        actionKind="review_work_plan"
        runtimeReady
        reportCommandFailure={vi.fn()}
        onManagerRefresh={onManagerRefresh}
        onClose={vi.fn()}
      />,
    );

    expect(await screen.findByText("Tendering Manager")).toBeTruthy();
    expect(
      screen.getByRole("heading", {
        name: "Workstreams, dependencies, and milestones",
      }),
    ).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Outcome" })).toBeTruthy();
    expect(screen.getByText(/cost estimation ready/)).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Risks" })).toBeTruthy();
    expect(
      screen.getByText("Ground conditions may differ from the survey."),
    ).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Assumptions" })).toBeTruthy();
    expect(
      screen.getByText("Site access is available from day one."),
    ).toBeTruthy();
    expect(screen.getByText("What changed since v1")).toBeTruthy();
    expect(screen.getByText("Added specialist: Cost Estimator")).toBeTruthy();
    expect(
      screen.getByText(
        "No unresolved Tender Query record is bound to this package.",
      ),
    ).toBeTruthy();

    fireEvent.change(screen.getByLabelText("Attributable rationale"), {
      target: { value: "Approve and start." },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Approve exact Work Plan" }),
    );
    await waitFor(() => {
      expect(host.decideWorkPlanProposal).toHaveBeenCalledWith(
        "tender-1",
        plan.plan_id,
        plan.version,
        "approve",
        "Approve and start.",
      );
    });
    await waitFor(() => {
      expect(host.activateTenderProduction).toHaveBeenCalledWith(
        "tender-1",
        plan.plan_id,
        plan.version,
        plan.manifest_sha256,
      );
    });
  });
});
