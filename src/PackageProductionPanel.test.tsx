import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const host = vi.hoisted(() => ({
  assembleSubmissionPackage: vi.fn(),
  generateSubmissionSections: vi.fn(),
  inspectCoordinatedBidBaselines: vi.fn(),
  inspectCurrentSubmissionPackage: vi.fn(),
  inspectPackageProduction: vi.fn(),
  inspectSubmissionArtifactContent: vi.fn(),
  inspectSubmissionPackageItemContent: vi.fn(),
}));

vi.mock("./quantixHost", () => host);

import { PackageProductionPanel } from "./PackageProductionPanel";

const baseline = {
  baseline_id: "a".repeat(32),
  version: 3,
  manifest_sha256: "b".repeat(64),
  current: true,
  approval: { decision: "approve" },
};

const generation = {
  generation_id: "c".repeat(32),
  sequence: 1,
  baseline_id: baseline.baseline_id,
  baseline_version: baseline.version,
  baseline_manifest_sha256: baseline.manifest_sha256,
  artifact_versions: [
    {
      artifact_id: "d".repeat(32),
      version: 1,
      package_path: "02-Commercial/Commercial-Offer.xlsx",
      envelope_key: "commercial_submission",
      section_key: "commercial",
      language: "English / العربية",
      authoring_mode: "xlsx",
      availability: "available",
      size_bytes: 1024,
      content_sha256: "e".repeat(64),
      manifest_sha256: "3".repeat(64),
      exact_inputs: ["approved_tender_price:price-1:1:" + "4".repeat(64)],
      provenance: ["tender-record:" + "1".repeat(32) + ":2:" + "5".repeat(64)],
    },
  ],
  requirements: [
    {
      requirement_id: "f".repeat(64),
      kind: "execution_requirement",
      record: {
        record_id: "1".repeat(32),
        version: 2,
        manifest_sha256: "8".repeat(64),
        stable_key: "approved-commercial-total",
        title: "Display the exact approved total.",
      },
      mandatory: true,
      section_key: "commercial",
      package_path: "02-Commercial/Commercial-Offer.xlsx",
      envelope_key: "commercial_submission",
      language: "English / العربية",
      authoring_mode: "xlsx",
      availability: "available",
      generated_artifact: {
        artifact_id: "d".repeat(32),
        version: 1,
      },
      unchanged_source_artifact: null,
      content_sha256: "e".repeat(64),
      size_bytes: 1024,
      authored_fields: [
        {
          name: "approved_total",
          value: "1250.00 EGP",
          normalized_value: null,
          original_expression: null,
          timezone: null,
          uncertainty: null,
          basis_kind: "calculation_run",
          basis_reference: "calc-run-1",
          basis_description: "Exact approved tender total",
          basis_authority: null,
          evidence: [],
        },
      ],
      evidence: [
        {
          package_path: "Conditions.pdf",
          reference: {
            artifact_id: "6".repeat(32),
            version: 1,
            ordinal: 7,
          },
          location: {
            kind: "page",
            ordinal: 7,
            structural_path: "Document/Page[7]",
            provenance: [],
            page_number: 7,
            section: "Submission Instructions",
            paragraph_number: null,
            table_number: null,
            sheet_name: null,
            cell_range: null,
            original_text: "السعر المعتمد فقط",
            translated_text: "Only the approved price",
            language: "Arabic",
            direction: "right_to_left",
          },
        },
      ],
      calculation_references: ["calc-run-1:" + "7".repeat(64)],
      review_references: ["review-1"],
      decision_references: ["decision-1"],
      manifest_sha256: "9".repeat(64),
    },
  ],
  manifest_sha256: "2".repeat(64),
  created_at: "2026-08-12T00:00:00Z",
};

const submissionPackage = {
  package_id: "a1".repeat(16),
  version: 1,
  tender_revision: 7,
  status: "proposed",
  assessment: "complete",
  current: true,
  currentness_facts: [],
  generation_id: generation.generation_id,
  generation_sequence: 1,
  generation_manifest_sha256: generation.manifest_sha256,
  baseline_id: baseline.baseline_id,
  baseline_version: baseline.version,
  baseline_manifest_sha256: baseline.manifest_sha256,
  baseline_approval_id: "a2".repeat(16),
  work_plan: {
    plan_id: "a3".repeat(16),
    plan_version: 1,
    plan_manifest_sha256: "a".repeat(64),
    plan_approval_id: "a4".repeat(16),
    plan_approval_sha256: "b".repeat(64),
    activation_id: "a5".repeat(16),
    authorized_profile_versions: [
      { profile_id: "a6".repeat(16), profile_version: 1 },
    ],
  },
  calculation_manifest_references: [
    {
      kind: "calculation_manifest",
      reference_id: "calc-run-1",
      version: 2,
      manifest_sha256: "7".repeat(64),
    },
  ],
  current_decision_references: [
    {
      kind: "approval",
      decision_id: "decision-1",
      subject_kind: "tender_record_version",
      subject_reference_id: generation.requirements[0].record.record_id,
      subject_version: generation.requirements[0].record.version,
      subject_manifest_sha256:
        generation.requirements[0].record.manifest_sha256,
    },
  ],
  submission_deadline: null,
  sections: [],
  items: [
    {
      item_id: "a7".repeat(32),
      package_path: generation.requirements[0].package_path,
      section_key: "commercial",
      envelope_key: "commercial_submission",
      language: "English",
      media_type:
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
      classifications: ["execution_requirement"],
      scope_record_ids: [generation.requirements[0].record.record_id],
      content_sha256: generation.requirements[0].content_sha256,
      size_bytes: 1024n,
      source: {
        kind: "generated",
        artifact_id: generation.artifact_versions[0].artifact_id,
        version: 1,
        manifest_sha256: generation.artifact_versions[0].manifest_sha256,
        content_sha256: generation.requirements[0].content_sha256,
        size_bytes: 1024n,
      },
      requirement_ids: [generation.requirements[0].requirement_id],
      evidence: generation.requirements[0].evidence,
      provenance: generation.artifact_versions[0].provenance,
      calculation_references: generation.requirements[0].calculation_references,
      review_references: generation.requirements[0].review_references,
      decision_references: generation.requirements[0].decision_references,
      authorship: [],
      validation_context_inputs: [],
      validation_context_sha256: "c".repeat(64),
    },
  ],
  coverage: [
    {
      requirement: generation.requirements[0],
      disposition: "covered",
      item_id: "a7".repeat(32),
      blockers: [],
      required_capabilities: ["commercial-pricing"],
      risk_references: [],
      manual_validation_required: false,
    },
  ],
  validation_context_inputs: [],
  validation_context_sha256: "d".repeat(64),
  dependency_currentness_sha256: "e".repeat(64),
  manifest_bytes: [123, 125],
  manifest_sha256: "f".repeat(64),
  created_at: "2026-08-12T00:01:00Z",
};

afterEach(cleanup);

describe("PackageProductionPanel", () => {
  it("binds generation to the exact approved Host baseline and reviews publication", async () => {
    host.inspectCoordinatedBidBaselines.mockResolvedValue({
      lifecycle_phase: "package_production",
      items: [baseline],
    });
    host.inspectPackageProduction.mockResolvedValue(null);
    host.inspectCurrentSubmissionPackage.mockResolvedValue(null);
    host.generateSubmissionSections.mockResolvedValue(generation);
    host.assembleSubmissionPackage.mockResolvedValue(submissionPackage);
    host.inspectSubmissionArtifactContent.mockResolvedValue({
      artifact_id: generation.artifact_versions[0].artifact_id,
      version: 1,
      media_type:
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
      content_sha256: generation.artifact_versions[0].content_sha256,
      bytes: [80, 75, 3, 4],
    });
    host.inspectSubmissionPackageItemContent.mockResolvedValue({
      content_sha256: submissionPackage.items[0].content_sha256,
      bytes: [80, 75, 3, 4],
    });
    const onTenderStateChange = vi.fn();

    render(
      <PackageProductionPanel
        tenderId="tender-1"
        runtimeReady
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={onTenderStateChange}
      />,
    );

    expect(
      await screen.findByText("Exact approved Coordinated Bid Baseline"),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Generate exact sections" }),
    );

    await waitFor(() =>
      expect(host.generateSubmissionSections).toHaveBeenCalledWith(
        "tender-1",
        baseline.baseline_id,
        baseline.version,
        baseline.manifest_sha256,
      ),
    );
    expect(
      await screen.findByText("02-Commercial/Commercial-Offer.xlsx"),
    ).toBeTruthy();
    expect(screen.getAllByText(/commercial_submission/).length).toBeGreaterThan(
      0,
    );
    expect(screen.getByText(/1250\.00 EGP/)).toBeTruthy();
    expect(screen.getByText(/Mandatory · commercial · xlsx/)).toBeTruthy();
    expect(screen.getByText(/Record manifest/)).toBeTruthy();
    expect(screen.getByText(/Requirement manifest/)).toBeTruthy();
    expect(screen.getByText(/Generated Artifact .* v1/)).toBeTruthy();
    expect(screen.getByText("السعر المعتمد فقط")).toBeTruthy();
    expect(screen.getAllByText(/calc-run-1/).length).toBeGreaterThan(0);
    expect(screen.getByText(/review-1/)).toBeTruthy();
    expect(screen.getByText(/approved_tender_price:price-1/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /load exact bytes/i }));
    await waitFor(() =>
      expect(host.inspectSubmissionArtifactContent).toHaveBeenCalledWith(
        "tender-1",
        generation.artifact_versions[0].artifact_id,
        1,
        generation.artifact_versions[0].manifest_sha256,
      ),
    );
    expect(await screen.findByText(/4 exact bytes loaded/i)).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Assemble exact package" }),
    );
    await waitFor(() =>
      expect(host.assembleSubmissionPackage).toHaveBeenCalledWith(
        "tender-1",
        generation.generation_id,
        generation.manifest_sha256,
      ),
    );
    expect(await screen.findByText("Canonical coverage manifest")).toBeTruthy();
    await waitFor(() =>
      expect(document.activeElement?.id).toBe("submission-package-detail"),
    );
    expect(screen.getByText(/Verified requirement/)).toBeTruthy();
    expect(screen.getByText(/Exact package item/)).toBeTruthy();
    expect(
      screen.getAllByText(
        new RegExp(`${submissionPackage.items[0].source.artifact_id} v1`),
      ).length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText(/1024 bytes/).length).toBeGreaterThan(0);
    expect(
      screen.getAllByText(submissionPackage.items[0].source.manifest_sha256)
        .length,
    ).toBeGreaterThan(0);
    expect(screen.getAllByText(/calc-run-1/).length).toBeGreaterThan(0);
    fireEvent.click(
      screen.getByRole("button", { name: /load and verify exact item bytes/i }),
    );
    await waitFor(() =>
      expect(host.inspectSubmissionPackageItemContent).toHaveBeenCalledWith(
        "tender-1",
        submissionPackage.package_id,
        1,
        submissionPackage.manifest_sha256,
        submissionPackage.items[0].item_id,
      ),
    );
    expect(onTenderStateChange).toHaveBeenCalledTimes(2);
  });

  it("keeps stale blocked coverage reviewable and reports exact-byte failures", async () => {
    const stale = {
      ...submissionPackage,
      assessment: "blocked",
      current: false,
      currentness_facts: [
        {
          code: "baseline_changed",
          current: false,
          reference_id: baseline.baseline_id,
          expected_value: baseline.manifest_sha256,
          actual_value: null,
        },
      ],
      coverage: [
        {
          ...submissionPackage.coverage[0],
          disposition: "unsupported",
          blockers: [
            {
              code: "unsupported_authoring",
              requirement_id: generation.requirements[0].requirement_id,
              detail: "The exact source format requires manual submission.",
            },
          ],
        },
      ],
    };
    host.inspectCoordinatedBidBaselines.mockResolvedValue({
      lifecycle_phase: "final_review",
      items: [baseline],
    });
    host.inspectPackageProduction.mockResolvedValue(generation);
    host.inspectCurrentSubmissionPackage.mockResolvedValue(stale);
    host.inspectSubmissionPackageItemContent.mockRejectedValue(
      new Error("unavailable"),
    );
    const reportCommandFailure = vi.fn();

    render(
      <PackageProductionPanel
        tenderId="tender-1"
        runtimeReady
        refreshToken={0}
        reportCommandFailure={reportCommandFailure}
        onTenderStateChange={vi.fn()}
      />,
    );

    expect(await screen.findByText(/immutable package is stale/i)).toBeTruthy();
    expect(screen.getByText(/baseline changed/i)).toBeTruthy();
    expect(screen.getByText(/unsupported authoring/i)).toBeTruthy();
    expect(
      (
        screen.getByRole("button", {
          name: "Generate exact sections",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(
      (
        screen.getByRole("button", {
          name: "Assemble exact package",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    fireEvent.click(
      screen.getByRole("button", { name: /load and verify exact item bytes/i }),
    );
    expect(
      (await screen.findByRole("alert", { name: "" })).textContent,
    ).toMatch(/Retry the exact item load/i);
    expect(reportCommandFailure).toHaveBeenCalledTimes(1);
  });

  it("navigates an exact decision to its structured record subject", async () => {
    const secondRequirement = {
      ...generation.requirements[0],
      requirement_id: "b".repeat(64),
      record: {
        ...generation.requirements[0].record,
        record_id: "2".repeat(32),
        version: 4,
        manifest_sha256: "a".repeat(64),
        title: "Second exact submission subject.",
      },
      decision_references: ["decision-2"],
    };
    const decisionPackage = {
      ...submissionPackage,
      current_decision_references: [
        {
          kind: "approval",
          decision_id: "decision-2",
          subject_kind: "tender_record_version",
          subject_reference_id: secondRequirement.record.record_id,
          subject_version: secondRequirement.record.version,
          subject_manifest_sha256: secondRequirement.record.manifest_sha256,
        },
      ],
      coverage: [
        submissionPackage.coverage[0],
        {
          ...submissionPackage.coverage[0],
          requirement: secondRequirement,
          disposition: "missing",
          item_id: null,
        },
      ],
    };
    host.inspectCoordinatedBidBaselines.mockResolvedValue({
      lifecycle_phase: "final_review",
      items: [baseline],
    });
    host.inspectPackageProduction.mockResolvedValue(generation);
    host.inspectCurrentSubmissionPackage.mockResolvedValue(decisionPackage);

    render(
      <PackageProductionPanel
        tenderId="tender-1"
        runtimeReady
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    fireEvent.click(
      await screen.findByRole("button", { name: /approval decision-2/i }),
    );
    expect(
      await screen.findByRole("heading", {
        name: "Second exact submission subject.",
      }),
    ).toBeTruthy();
  });

  it("does not navigate non-record decisions through unrelated global IDs", async () => {
    const unrelatedDecisionPackage = {
      ...submissionPackage,
      current_decision_references: [
        {
          kind: "approval",
          decision_id: "decision-1",
          subject_kind: "approved_tender_price",
          subject_reference_id: "price-1",
          subject_version: 3,
          subject_manifest_sha256: "6".repeat(64),
        },
      ],
    };
    host.inspectCoordinatedBidBaselines.mockResolvedValue({
      lifecycle_phase: "final_review",
      items: [baseline],
    });
    host.inspectPackageProduction.mockResolvedValue(generation);
    host.inspectCurrentSubmissionPackage.mockResolvedValue(
      unrelatedDecisionPackage,
    );

    render(
      <PackageProductionPanel
        tenderId="tender-1"
        runtimeReady
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    expect(
      await screen.findByText(/approved tender price price-1 v3/i),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: /approval decision-1/i }),
    ).toBeNull();
  });

  it("keeps Package Production actions available when fresh assembly is blocked", async () => {
    const unavailableRequirement = {
      ...generation.requirements[0],
      availability: "missing",
      generated_artifact: null,
      unchanged_source_artifact: null,
      content_sha256: null,
      size_bytes: null,
    };
    const blockedGeneration = {
      ...generation,
      artifact_versions: [],
      requirements: [unavailableRequirement],
    };
    const blocked = {
      ...submissionPackage,
      assessment: "blocked",
      coverage: [
        {
          ...submissionPackage.coverage[0],
          requirement: unavailableRequirement,
          disposition: "missing",
          item_id: null,
          blockers: [
            {
              code: "missing_item",
              requirement_id: generation.requirements[0].requirement_id,
              detail: "No exact approved item is available.",
            },
          ],
        },
      ],
    };
    host.inspectCoordinatedBidBaselines.mockResolvedValue({
      lifecycle_phase: "package_production",
      items: [baseline],
    });
    host.inspectPackageProduction.mockResolvedValue(blockedGeneration);
    host.inspectCurrentSubmissionPackage.mockResolvedValue(null);
    host.assembleSubmissionPackage.mockResolvedValue(blocked);

    render(
      <PackageProductionPanel
        tenderId="tender-1"
        runtimeReady
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    const assemble = await screen.findByRole("button", {
      name: "Assemble exact package",
    });
    fireEvent.click(assemble);
    expect(await screen.findByText(/missing item/i)).toBeTruthy();
    expect(
      screen.getByText(/missing — no immutable source item/i),
    ).toBeTruthy();
    expect(screen.queryByText(/undefined/)).toBeNull();
    expect((assemble as HTMLButtonElement).disabled).toBe(false);
    expect(
      (
        screen.getByRole("button", {
          name: "Generate exact sections",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);
  });
});
