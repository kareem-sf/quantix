import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const host = vi.hoisted(() => ({
  generateSubmissionSections: vi.fn(),
  inspectCoordinatedBidBaselines: vi.fn(),
  inspectPackageProduction: vi.fn(),
  inspectSubmissionArtifactContent: vi.fn(),
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

afterEach(cleanup);

describe("PackageProductionPanel", () => {
  it("binds generation to the exact approved Host baseline and reviews publication", async () => {
    host.inspectCoordinatedBidBaselines.mockResolvedValue({
      lifecycle_phase: "package_production",
      items: [baseline],
    });
    host.inspectPackageProduction.mockResolvedValue(null);
    host.generateSubmissionSections.mockResolvedValue(generation);
    host.inspectSubmissionArtifactContent.mockResolvedValue({
      artifact_id: generation.artifact_versions[0].artifact_id,
      version: 1,
      media_type:
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
      content_sha256: generation.artifact_versions[0].content_sha256,
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
    expect(onTenderStateChange).toHaveBeenCalledOnce();
  });
});
