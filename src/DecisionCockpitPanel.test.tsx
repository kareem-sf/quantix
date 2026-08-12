import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { DecisionCockpit } from "./bindings/DecisionCockpit";

const host = vi.hoisted(() => ({
  approveBasisOfEstimate: vi.fn(),
  approveCalculationRule: vi.fn(),
  approveCommercialStrategy: vi.fn(),
  approveControlledBoqCalculationRun: vi.fn(),
  approveExternalRfiForIssue: vi.fn(),
  approvePricedCostBaseline: vi.fn(),
  approvePricingAdjustment: vi.fn(),
  approveProductionFindingException: vi.fn(),
  approveTenderPrice: vi.fn(),
  decideBidDecisionPackage: vi.fn(),
  decideChangeAssessment: vi.fn(),
  decideCoordinatedBidBaseline: vi.fn(),
  decideTenderQueryTreatment: vi.fn(),
  decideTenderRecord: vi.fn(),
  decideWorkPlanProposal: vi.fn(),
  inspectDecisionCockpit: vi.fn(),
  inspectEvidence: vi.fn(),
  inspectPricingWorkspace: vi.fn(),
  interpretExternalRfiResponse: vi.fn(),
  resolveIndeterminateAgentRun: vi.fn(),
  resolveTenderRecovery: vi.fn(),
  searchEvidence: vi.fn(),
  selectPricingScenario: vi.fn(),
}));

vi.mock("./quantixHost", () => host);

import { DecisionCockpitPanel } from "./DecisionCockpitPanel";

const cockpit: DecisionCockpit = {
  tender_id: "tender-1",
  tender_revision: 12,
  lifecycle_phase: "bid_decision",
  pending_decisions: [
    {
      decision_id: "BidDecision:package-1:4",
      kind: "bid_decision",
      title: "Proceed, hold, or decline",
      summary: "Govern the exact independently reviewed package.",
      target: {
        kind: "bid_decision_package",
        object_id: "package-1",
        version: 4,
        manifest_sha256: "a".repeat(64),
      },
      responsible: {
        kind: "tendering_manager",
        label: "Tendering Manager",
        profile_id: null,
        profile_version: null,
      },
      lifecycle_gate: "bid_decision",
      urgency: "immediate",
      urgency_reason: "This decision is blocking controlled work.",
      deadline: "2026-08-13T09:00:00Z",
      status: "ready",
      ready: true,
      blocking_consequences: ["Production cannot start."],
      allowed_actions: ["accept", "return", "reject"],
      facts: [
        {
          kind: "agent_recommendation",
          label: "Bid recommendation",
          value: "Proceed with disclosed conditions.",
          evidence: [],
        },
        {
          kind: "verified_fact",
          label: "Submission deadline",
          value: "13 August 2026",
          evidence: [],
        },
        {
          kind: "approved_assumption",
          label: "Working assumption",
          value: "Client permits one clarification cycle.",
          evidence: [],
        },
        {
          kind: "deterministic_result",
          label: "Resource total",
          value: "640 hours",
          evidence: [],
        },
        {
          kind: "unresolved_gap",
          label: "Bond wording",
          value: "External clarification remains open.",
          evidence: [],
        },
        {
          kind: "prior_engineer_decision",
          label: "Prior decision",
          value: "Returned package v3.",
          evidence: [],
        },
      ],
      evidence: [
        {
          artifact_id: "artifact-1",
          version: 2,
          location_ordinal: 7,
          label: "Instructions · page 12 · table 3",
          original_text: "النص الملزم",
          translated_text: "Binding text",
        },
      ],
      changes_since_prior_review: ["Resource estimate changed."],
      dependencies: [],
      unresolved_queries: [],
      assumptions: [],
      calculations: [],
      findings: [],
      exceptions: [],
      independent_review: {
        kind: "verified_fact",
        label: "Independent review",
        value: "Passed",
        evidence: [],
      },
      group_members: [],
    },
  ],
};

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("DecisionCockpitPanel", () => {
  it("recovers from an inventory inspection error without leaving the cockpit", async () => {
    host.inspectDecisionCockpit
      .mockRejectedValueOnce(new Error("temporarily unavailable"))
      .mockResolvedValueOnce(cockpit);
    const reportCommandFailure = vi.fn();

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={reportCommandFailure}
        onTenderStateChange={vi.fn()}
      />,
    );
    expect(
      await screen.findByRole("alert", {
        name: /formal decision inventory could not be inspected/i,
      }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry cockpit" }));

    expect(
      await screen.findByRole("heading", { name: "Decision Cockpit" }),
    ).toBeTruthy();
    expect(
      (await screen.findAllByText("Proceed, hold, or decline")).length,
    ).toBeGreaterThan(0);
    expect(reportCommandFailure).toHaveBeenCalledTimes(1);
  });

  it("separates trust classes and exposes only exact legal actions", async () => {
    host.inspectDecisionCockpit.mockResolvedValue(cockpit);

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    expect(
      (await screen.findAllByText("Proceed, hold, or decline")).length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText("Agent recommendation").length).toBeGreaterThan(
      0,
    );
    expect(screen.getAllByText("Verified fact").length).toBeGreaterThan(0);
    expect(screen.getByText("Approved assumption")).toBeTruthy();
    expect(screen.getByText("Deterministic result")).toBeTruthy();
    expect(screen.getByText("Unresolved gap")).toBeTruthy();
    expect(screen.getByText("Prior Engineer decision")).toBeTruthy();
    expect(screen.getByText("Accept · Return · Reject")).toBeTruthy();
    expect(screen.queryByText(/approve all/i)).toBeNull();
    expect(screen.queryByText(/draft|upload|chat/i)).toBeNull();
  });

  it("navigates to exact source provenance and restores decision focus", async () => {
    host.inspectDecisionCockpit.mockResolvedValue(cockpit);
    host.inspectEvidence.mockResolvedValue({
      artifact_id: "artifact-1",
      version: 2,
      state: "completed",
      exception: null,
      language: "arabic",
      direction: "right_to_left",
      docling_schema_version: "1.0",
      docling_json_sha256: "b".repeat(64),
      locations: [
        {
          ordinal: 7,
          kind: "table",
          structural_path: "body/table[3]",
          provenance: [],
          section: "Instructions",
          paragraph_number: null,
          table_number: 3,
          sheet_name: null,
          cell_range: null,
          original_text: "النص الملزم",
          translated_text: "Binding text",
          language: "arabic",
          direction: "right_to_left",
        },
        {
          ordinal: 8,
          kind: "cell",
          structural_path: "workbook/sheet[2]/D14",
          provenance: [],
          section: null,
          paragraph_number: null,
          table_number: null,
          sheet_name: "Commercials",
          cell_range: "D14",
          original_text: "12.5%",
          translated_text: null,
          language: "english",
          direction: "left_to_right",
        },
      ],
    });

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );
    const source = await screen.findByRole("button", {
      name: /open exact evidence.*instructions/i,
    });
    fireEvent.click(source);

    await waitFor(() =>
      expect(host.inspectEvidence).toHaveBeenCalledWith(
        "tender-1",
        "artifact-1",
        2,
      ),
    );
    expect(
      screen.getByRole("heading", { name: /Instructions.*Table 3/i }),
    ).toBeTruthy();
    expect(screen.getByText(/Authoritative source text.*arabic/i)).toBeTruthy();
    expect(
      screen.getByText("Derived translation — non-authoritative"),
    ).toBeTruthy();
    expect(screen.getByText("Binding text").closest("blockquote")?.dir).toBe(
      "auto",
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Next evidence location" }),
    );
    expect(
      screen.getByRole("heading", { name: /Commercials.*D14/i }),
    ).toBeTruthy();
    expect(screen.getByText("12.5%")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Previous evidence location" }),
    );
    expect(
      screen.getByRole("heading", { name: /Instructions.*Table 3/i }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back to decision" }));
    await waitFor(() => expect(document.activeElement).toBe(source));
  });

  it("fails visibly when an exact evidence ordinal is absent", async () => {
    host.inspectDecisionCockpit.mockResolvedValue(cockpit);
    host.inspectEvidence.mockResolvedValue({
      artifact_id: "artifact-1",
      version: 2,
      state: "completed",
      exception: null,
      language: "english",
      direction: "left_to_right",
      docling_schema_version: "1.0",
      docling_json_sha256: "b".repeat(64),
      locations: [],
    });
    const reportCommandFailure = vi.fn();

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={reportCommandFailure}
        onTenderStateChange={vi.fn()}
      />,
    );
    const source = await screen.findByRole("button", {
      name: /open exact evidence.*instructions/i,
    });
    fireEvent.click(source);

    expect(
      await screen.findByText(
        "Exact source evidence is unavailable. Try again.",
      ),
    ).toBeTruthy();
    expect(screen.queryByText("Back to decision")).toBeNull();
    expect(reportCommandFailure).toHaveBeenCalledTimes(1);
  });

  it("keeps artifact-level evidence at artifact scope instead of inventing a location", async () => {
    const artifactCockpit: DecisionCockpit = {
      ...cockpit,
      pending_decisions: [
        {
          ...cockpit.pending_decisions[0],
          evidence: [
            {
              artifact_id: "artifact-1",
              version: 2,
              location_ordinal: null,
              label: "Registered source artifact",
              original_text: null,
              translated_text: null,
            },
          ],
        },
      ],
    };
    host.inspectDecisionCockpit.mockResolvedValue(artifactCockpit);
    host.inspectEvidence.mockResolvedValue({
      artifact_id: "artifact-1",
      version: 2,
      state: "completed",
      exception: null,
      language: "arabic",
      direction: "right_to_left",
      docling_schema_version: "1.0",
      docling_json_sha256: "b".repeat(64),
      locations: [
        {
          ordinal: 1,
          kind: "paragraph",
          structural_path: "body/paragraph[1]",
          provenance: [],
          section: null,
          paragraph_number: 1,
          table_number: null,
          sheet_name: null,
          cell_range: null,
          original_text: "Unasserted first location",
          translated_text: null,
          language: "english",
          direction: "left_to_right",
        },
      ],
    });

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: /open exact evidence registered source artifact/i,
      }),
    );

    const artifactHeading = await screen.findByRole("heading", {
      name: "Artifact-level source reference",
    });
    const artifactDetail = artifactHeading.closest("section");
    expect(artifactDetail).not.toBeNull();
    expect(within(artifactDetail!).getByText(/artifact-1.*v2/i)).toBeTruthy();
    expect(screen.queryByText("Unasserted first location")).toBeNull();
    expect(
      screen.queryByRole("navigation", { name: "Exact evidence locations" }),
    ).toBeNull();
  });

  it("detects a stale target before routing to the exact domain gate", async () => {
    host.inspectDecisionCockpit
      .mockResolvedValueOnce(cockpit)
      .mockResolvedValueOnce({ ...cockpit, pending_decisions: [] });

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Open exact decision gate" }),
    );

    expect(
      await screen.findByRole("alert", {
        name: /decision changed/i,
      }),
    ).toBeTruthy();
  });

  it("dispatches a conditional cockpit action through the exact target-bound domain command", async () => {
    host.inspectDecisionCockpit.mockResolvedValue(cockpit);
    host.decideBidDecisionPackage.mockResolvedValue({});

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "Accept exact target" }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "  Independent evidence supports proceeding.  " },
    });
    fireEvent.change(
      screen.getByLabelText("Decision conditions (one per line)"),
      {
        target: { value: "  Confirm bond wording\nMaintain query watch  " },
      },
    );
    fireEvent.change(
      screen.getByLabelText("Decision exceptions (one per line)"),
      {
        target: { value: "Client response remains outstanding" },
      },
    );
    fireEvent.click(screen.getByRole("button", { name: "Confirm Accept" }));

    await waitFor(() =>
      expect(host.decideBidDecisionPackage).toHaveBeenCalledWith(
        "tender-1",
        "package-1",
        4,
        "a".repeat(64),
        "accept",
        "Independent evidence supports proceeding.",
        ["Confirm bond wording", "Maintain query watch"],
        ["Client response remains outstanding"],
        [],
      ),
    );
  });

  it("moves focus to the selected next decision after a successful inline action", async () => {
    const nextDecision = {
      ...cockpit.pending_decisions[0],
      decision_id: "QueryTreatment:query-next:2",
      kind: "query_treatment" as const,
      title: "Decide the next Query treatment",
      target: {
        kind: "tender_query" as const,
        object_id: "query-next",
        version: 2,
        manifest_sha256: "b".repeat(64),
      },
      allowed_actions: ["apply_treatment" as const],
    };
    const initial = {
      ...cockpit,
      pending_decisions: [cockpit.pending_decisions[0], nextDecision],
    };
    const refreshed = { ...cockpit, pending_decisions: [nextDecision] };
    host.inspectDecisionCockpit
      .mockResolvedValueOnce(initial)
      .mockResolvedValueOnce(initial)
      .mockResolvedValueOnce(refreshed);
    host.decideBidDecisionPackage.mockResolvedValue({});

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Accept exact target" }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Proceed on the exact reviewed package." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Confirm Accept" }));

    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole("button", {
          name: /decide the next query treatment/i,
        }),
      ),
    );
  });

  it("dispatches an Awaiting Approval Tender Recovery decision through its exact recovery command", async () => {
    const recoveryCockpit: DecisionCockpit = {
      ...cockpit,
      lifecycle_phase: "intake",
      pending_decisions: [
        {
          ...cockpit.pending_decisions[0],
          decision_id: "TenderRecovery:recovery-1:7",
          kind: "tender_recovery",
          title: "Approve verified Tender Recovery replacement",
          target: {
            kind: "tender_recovery",
            object_id: "recovery-1",
            version: 7,
            manifest_sha256: "c".repeat(64),
          },
          lifecycle_gate: "recovery",
          allowed_actions: ["approve_replacement", "reject"],
        },
      ],
    };
    host.inspectDecisionCockpit.mockResolvedValue(recoveryCockpit);
    host.resolveTenderRecovery.mockResolvedValue({});

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Approve Replacement exact target",
      }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: {
        value: "Replace the damaged store with this exact verified backup.",
      },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm Approve Replacement" }),
    );

    await waitFor(() =>
      expect(host.resolveTenderRecovery).toHaveBeenCalledWith(
        "tender-1",
        "recovery-1",
        "approve_replacement",
        "Replace the damaged store with this exact verified backup.",
      ),
    );
  });

  it("binds Query rationale and treatment details as distinct exact values", async () => {
    const queryCockpit: DecisionCockpit = {
      ...cockpit,
      pending_decisions: [
        {
          ...cockpit.pending_decisions[0],
          decision_id: "QueryTreatment:query-1:3",
          kind: "query_treatment",
          target: {
            kind: "tender_query",
            object_id: "query-1",
            version: 3,
            manifest_sha256: "b".repeat(64),
          },
          allowed_actions: ["apply_treatment"],
        },
      ],
    };
    host.inspectDecisionCockpit.mockResolvedValue(queryCockpit);
    host.decideTenderQueryTreatment.mockResolvedValue({});

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", {
        name: "Apply Treatment exact target",
      }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Approve the bounded commercial treatment." },
    });
    fireEvent.change(screen.getByLabelText("Exact treatment details"), {
      target: { value: "Carry a visible qualification in section C." },
    });
    fireEvent.click(
      screen.getByLabelText("Close exact Query after applying this treatment"),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm Apply Treatment" }),
    );

    await waitFor(() =>
      expect(host.decideTenderQueryTreatment).toHaveBeenCalledWith({
        tender_id: "tender-1",
        query_id: "query-1",
        query_version: 3,
        treatment: "qualification",
        rationale: "Approve the bounded commercial treatment.",
        treatment_details: "Carry a visible qualification in section C.",
        closes_query: true,
      }),
    );
  });

  it("binds an RFI response interpretation, treatment, and rationale independently", async () => {
    const responseCockpit: DecisionCockpit = {
      ...cockpit,
      pending_decisions: [
        {
          ...cockpit.pending_decisions[0],
          decision_id: "ExternalRfiResponseInterpretation:response-1:query-1:5",
          kind: "external_rfi_response_interpretation",
          target: {
            kind: "external_rfi_response",
            object_id: "response-1:query-1",
            version: 5,
            manifest_sha256: "d".repeat(64),
          },
          allowed_actions: ["apply_treatment"],
          dependencies: [
            {
              target: {
                kind: "tender_query",
                object_id: "query-1",
                version: 2,
                manifest_sha256: "e".repeat(64),
              },
              label: "Query version issued in the approved RFI",
              status: "stale",
            },
            {
              target: {
                kind: "tender_query",
                object_id: "query-1",
                version: 5,
                manifest_sha256: "f".repeat(64),
              },
              label: "Current unresolved Query decision basis",
              status: "unresolved",
            },
          ],
        },
      ],
    };
    host.inspectDecisionCockpit.mockResolvedValue(responseCockpit);
    host.interpretExternalRfiResponse.mockResolvedValue({});

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", {
        name: "Apply Treatment exact target",
      }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Approve the attributable commercial response." },
    });
    fireEvent.change(screen.getByLabelText("Exact treatment details"), {
      target: { value: "Qualify the responsibility boundary in the offer." },
    });
    fireEvent.change(screen.getByLabelText("Exact response interpretation"), {
      target: { value: "The Employer retains the temporary-works design." },
    });
    fireEvent.click(screen.getByLabelText("Material response interpretation"));
    fireEvent.click(
      screen.getByLabelText("Close exact Query after applying this treatment"),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm Apply Treatment" }),
    );

    await waitFor(() =>
      expect(host.interpretExternalRfiResponse).toHaveBeenCalledWith({
        tender_id: "tender-1",
        response_link_id: "response-1",
        query_id: "query-1",
        issued_query_version: 2,
        base_query_version: 5,
        base_query_manifest_sha256: "f".repeat(64),
        material: false,
        interpretation: "The Employer retains the temporary-works design.",
        treatment: "qualification",
        rationale: "Approve the attributable commercial response.",
        treatment_details: "Qualify the responsibility boundary in the offer.",
        closes_query: true,
      }),
    );
  });

  it("binds a Major Finding exception consequence separately from its rationale", async () => {
    const exceptionCockpit: DecisionCockpit = {
      ...cockpit,
      pending_decisions: [
        {
          ...cockpit.pending_decisions[0],
          decision_id: "ProductionFindingException:finding-1:2",
          kind: "production_finding_exception",
          target: {
            kind: "production_review_finding",
            object_id: "finding-1",
            version: 2,
            manifest_sha256: "c".repeat(64),
          },
          allowed_actions: ["approve_exception"],
          dependencies: [
            {
              target: {
                kind: "production_task",
                object_id: "task-1",
                version: 1,
                manifest_sha256: null,
              },
              label: "Exact Production Task",
              status: "current",
            },
            {
              target: {
                kind: "production_review",
                object_id: "review-1",
                version: 2,
                manifest_sha256: null,
              },
              label: "Independent Review",
              status: "current",
            },
            {
              target: {
                kind: "production_artifact",
                object_id: "artifact-2",
                version: 2,
                manifest_sha256: "c".repeat(64),
              },
              label: "Exact reviewed Artifact",
              status: "current",
            },
          ],
        },
      ],
    };
    host.inspectDecisionCockpit.mockResolvedValue(exceptionCockpit);
    host.approveProductionFindingException.mockResolvedValue({});

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", {
        name: "Approve Exception exact target",
      }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Accept this permitted Major Finding exception." },
    });
    fireEvent.change(screen.getByLabelText("Exact exception consequence"), {
      target: {
        value: "Disclose the residual programme risk in Final Review.",
      },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Confirm Approve Exception" }),
    );

    await waitFor(() =>
      expect(host.approveProductionFindingException).toHaveBeenCalledWith(
        "tender-1",
        "task-1",
        "finding-1",
        "review-1",
        "artifact-2",
        2,
        "c".repeat(64),
        "Accept this permitted Major Finding exception.",
        "Disclose the residual programme risk in Final Review.",
      ),
    );
  });

  it("reports a recorded exact action even when the follow-up refresh fails", async () => {
    host.inspectDecisionCockpit
      .mockResolvedValueOnce(cockpit)
      .mockResolvedValueOnce(cockpit)
      .mockRejectedValueOnce(new Error("refresh unavailable"));
    host.decideBidDecisionPackage.mockResolvedValue({});
    const onTenderStateChange = vi.fn();
    const reportCommandFailure = vi.fn();

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={reportCommandFailure}
        onTenderStateChange={onTenderStateChange}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Accept exact target" }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Proceed on the exact reviewed package." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Confirm Accept" }));

    await waitFor(() => expect(onTenderStateChange).toHaveBeenCalledTimes(1));
    expect(
      screen.getByText(/accept recorded.*refresh.*unavailable/i),
    ).toBeTruthy();
    expect(
      screen.queryByText(/decision was rejected.*no substitute action/i),
    ).toBeNull();
    expect(reportCommandFailure).toHaveBeenCalledTimes(1);
  });

  it("refuses an action that is no longer legal for the unchanged exact target", async () => {
    host.inspectDecisionCockpit
      .mockResolvedValueOnce(cockpit)
      .mockResolvedValueOnce({
        ...cockpit,
        pending_decisions: [
          {
            ...cockpit.pending_decisions[0],
            ready: false,
            status: "blocked",
            allowed_actions: ["return"],
          },
        ],
      });

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Accept exact target" }),
    );
    fireEvent.change(screen.getByLabelText("Decision rationale"), {
      target: { value: "Proceed on the prior ready state." },
    });
    fireEvent.click(screen.getByRole("button", { name: "Confirm Accept" }));

    expect(
      await screen.findByRole("alert", { name: /decision changed/i }),
    ).toBeTruthy();
    expect(host.decideBidDecisionPackage).not.toHaveBeenCalled();
  });

  it("follows an exact dependency and returns focus to its provenance link", async () => {
    const bid = cockpit.pending_decisions[0];
    const query = {
      ...bid,
      decision_id: "QueryTreatment:query-1:2",
      kind: "query_treatment" as const,
      title: "Decide Query treatment",
      target: {
        kind: "tender_query" as const,
        object_id: "query-1",
        version: 2,
        manifest_sha256: "q".repeat(64),
      },
      allowed_actions: ["apply_treatment" as const],
      dependencies: [],
    };
    host.inspectDecisionCockpit.mockResolvedValue({
      ...cockpit,
      pending_decisions: [
        {
          ...bid,
          dependencies: [
            {
              target: query.target,
              label: "Release-blocking Query",
              status: "unresolved",
            },
          ],
          group_members: [
            {
              target: query.target,
              condition: "Resolve before grouped approval.",
              status: "blocked",
            },
          ],
        },
        query,
      ],
    });

    render(
      <DecisionCockpitPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );
    const dependency = await screen.findByRole("button", {
      name: /open dependency release-blocking query/i,
    });
    fireEvent.click(dependency);

    expect(
      screen.getByRole("heading", { name: "Decide Query treatment" }),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Back to prior decision" }),
    );
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole("button", {
          name: /open dependency release-blocking query/i,
        }),
      ),
    );
    const groupedTarget = screen.getByRole("button", {
      name: /open grouped target query-1/i,
    });
    fireEvent.click(groupedTarget);
    expect(
      screen.getByRole("heading", { name: "Decide Query treatment" }),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Back to prior decision" }),
    );
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole("button", {
          name: /open grouped target query-1/i,
        }),
      ),
    );
  });

  it("returns from a non-cockpit domain dependency to the exact provenance link", async () => {
    const dependencyCockpit: DecisionCockpit = {
      ...cockpit,
      pending_decisions: [
        {
          ...cockpit.pending_decisions[0],
          dependencies: [
            {
              target: {
                kind: "tender_query",
                object_id: "query-not-pending",
                version: 6,
                manifest_sha256: "9".repeat(64),
              },
              label: "Approved Query provenance",
              status: "approved",
            },
          ],
        },
      ],
    };
    host.inspectDecisionCockpit.mockResolvedValue(dependencyCockpit);

    render(
      <>
        <h2 id="query-register-title">Query Register</h2>
        <DecisionCockpitPanel
          tenderId="tender-1"
          refreshToken={0}
          reportCommandFailure={vi.fn()}
          onTenderStateChange={vi.fn()}
        />
      </>,
    );
    const dependency = await screen.findByRole("button", {
      name: /open dependency approved query provenance/i,
    });
    const queryHeading = screen.getByRole("heading", {
      name: "Query Register",
    });
    queryHeading.scrollIntoView = vi.fn();
    screen.getByRole("heading", { name: "Decision Cockpit" }).scrollIntoView =
      vi.fn();
    fireEvent.click(dependency);

    expect(document.activeElement).toBe(queryHeading);
    expect(
      screen.getByRole("status", { name: "Exact dependency target" })
        .textContent,
    ).toContain(`query-not-pending · v6 · manifest ${"9".repeat(64)}`);
    fireEvent.click(
      screen.getByRole("button", { name: "Return to cockpit provenance" }),
    );
    await waitFor(() => expect(document.activeElement).toBe(dependency));
  });
});
