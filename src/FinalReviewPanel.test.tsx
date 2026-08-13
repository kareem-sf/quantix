import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const host = vi.hoisted(() => ({
  approvePackageFindingException: vi.fn(),
  inspectCurrentSubmissionPackage: vi.fn(),
  inspectFinalReview: vi.fn(),
  recordPackageManualVerification: vi.fn(),
  runPackageValidation: vi.fn(),
  runSubmissionSectionReview: vi.fn(),
}));

vi.mock("./quantixHost", () => host);

import { FinalReviewPanel } from "./FinalReviewPanel";

const itemId = "1".repeat(64);
const packageManifest = "2".repeat(64);
const policyManifest = "3".repeat(64);
const resultId = "4".repeat(32);
const assignmentId = "5".repeat(32);

const submissionPackage = {
  package_id: "a".repeat(32),
  version: 1,
  manifest_sha256: packageManifest,
  assessment: "complete",
  current: true,
  items: [
    {
      item_id: itemId,
      package_path: "Forms/Arabic-Form.pdf",
      content_sha256: "6".repeat(64),
      evidence: [
        {
          reference: {
            artifact_id: "7".repeat(32),
            version: 1,
            ordinal: 2,
          },
        },
      ],
    },
  ],
  sections: [
    {
      section_key: "forms",
      envelope_key: "technical",
      item_ids: [itemId],
      required_capabilities: ["document_control"],
    },
  ],
};

const inspection = {
  package: submissionPackage,
  policy: {
    policy_id: "8".repeat(32),
    version: 1,
    manifest_sha256: policyManifest,
    created_at: "2026-08-13T00:00:00Z",
    fixed_rules: [
      {
        rule_id: "quantix.rendering_model",
        category: "rendering",
        severity: "major",
        deterministic: false,
        source: null,
        manual_checklist: ["Inspect this exact PDF visually."],
        major_exception_allowed: true,
      },
    ],
    tender_rules: [],
  },
  validation_run: {
    run_id: "9".repeat(32),
    package_id: submissionPackage.package_id,
    package_version: 1,
    package_manifest_sha256: packageManifest,
    policy_id: "8".repeat(32),
    policy_version: 1,
    policy_manifest_sha256: policyManifest,
    validator_version: 1,
    renderer_version: 1,
    context_sha256: "b".repeat(64),
    results: [
      {
        result_id: resultId,
        item_id: itemId,
        content_sha256: submissionPackage.items[0].content_sha256,
        validation_context_sha256: "c".repeat(64),
        check_id: "quantix.rendering_model",
        check_version: 1,
        category: "rendering",
        outcome: "manual_verification_required",
        detail: "Visual form verification is required.",
        evidence_references: [packageManifest],
        reused_from_result_id: null,
      },
    ],
    manifest_sha256: "d".repeat(64),
    created_at: "2026-08-13T00:00:00Z",
  },
  manual_verifications: [],
  review_plan: {
    plan_id: "e".repeat(32),
    package_id: submissionPackage.package_id,
    package_version: 1,
    package_manifest_sha256: packageManifest,
    validation_run_id: "9".repeat(32),
    policy_manifest_sha256: policyManifest,
    assignments: [
      {
        assignment_id: assignmentId,
        section_key: "forms",
        envelope_key: "technical",
        language: "Arabic",
        item_ids: [itemId],
        required_capability: "document_control",
        risk_references: [],
        author_profile_versions: [],
        reviewer: {
          profile_id: "f".repeat(32),
          profile_version: 1,
          identity: "Independent Document Reviewer",
          capabilities: ["review_document_control"],
        },
        criteria: ["Inspect exact content."],
      },
    ],
    manifest_sha256: "0".repeat(64),
    created_at: "2026-08-13T00:00:00Z",
  },
  reviews: [],
  exceptions: [],
  report: {
    version: 1,
    manifest_sha256: "1".repeat(64),
    summaries: [
      { category: "coverage", references: [itemId] },
      { category: "departures", references: [] },
    ],
    deadline: { reference_id: "deadline-1" },
  },
  current: true,
  ready: false,
  live_blockers: [
    {
      code: "manual_verification_missing",
      reference_id: resultId,
      detail: "Exact-hash Manual Verification is missing.",
    },
  ],
  live_changes: [],
};

describe("FinalReviewPanel", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("shows exact readiness evidence and sends exact manual and review targets", async () => {
    host.inspectCurrentSubmissionPackage.mockResolvedValue(submissionPackage);
    host.inspectFinalReview.mockResolvedValue(inspection);
    host.recordPackageManualVerification.mockResolvedValue({
      ...inspection,
      manual_verifications: [
        {
          verification_id: "c".repeat(32),
          validation_result_id: resultId,
          package_id: submissionPackage.package_id,
          package_version: submissionPackage.version,
          package_manifest_sha256: packageManifest,
          item_id: itemId,
          content_sha256: submissionPackage.items[0].content_sha256,
          verifier_identity: "engineer_user",
          capability: "document_control",
          checks: ["Inspect this exact PDF visually."],
          evidence_references: [`source:${"7".repeat(32)}:1:2`],
          result: "passed",
          limitations: ["Checked against the exact immutable file hash."],
          manifest_sha256: "d".repeat(64),
          created_at: "2026-08-13T00:00:00Z",
        },
      ],
    });
    host.runSubmissionSectionReview.mockResolvedValue({
      run: { state: "completed" },
      final_review: inspection,
    });

    render(
      <FinalReviewPanel
        tenderId={"a".repeat(32)}
        runtimeReady
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    expect(await screen.findByText(/1 release blocker/i)).toBeTruthy();
    expect(screen.getByText(/departures: 0/i)).toBeTruthy();
    expect(
      screen.getAllByText(new RegExp(packageManifest)).length,
    ).toBeGreaterThan(0);

    fireEvent.change(screen.getByLabelText(/engineer outcome/i), {
      target: { value: "passed" },
    });
    fireEvent.change(screen.getByLabelText(/limitations/i), {
      target: { value: "Checked against the exact immutable file hash." },
    });
    fireEvent.click(
      screen.getByRole("button", { name: /record manual verification/i }),
    );
    await waitFor(() =>
      expect(host.recordPackageManualVerification).toHaveBeenCalledTimes(1),
    );
    expect(host.recordPackageManualVerification.mock.calls[0][0]).toMatchObject(
      {
        package_manifest_sha256: packageManifest,
        validation_result_id: resultId,
        item_id: itemId,
        checks: ["Inspect this exact PDF visually."],
        capability: "document_control",
        result: "passed",
        limitations: ["Checked against the exact immutable file hash."],
      },
    );

    fireEvent.click(
      screen.getByRole("button", { name: /run independent review/i }),
    );
    await waitFor(() =>
      expect(host.runSubmissionSectionReview).toHaveBeenCalledTimes(1),
    );
    expect(host.runSubmissionSectionReview.mock.calls[0][0]).toMatchObject({
      package_manifest_sha256: packageManifest,
      assignment_id: assignmentId,
    });
  });
});
