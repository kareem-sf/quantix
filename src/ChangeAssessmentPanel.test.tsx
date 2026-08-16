import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ChangeAssessment } from "./bindings/ChangeAssessment";
import type { ChangeAssessmentPage } from "./bindings/ChangeAssessmentPage";

const host = vi.hoisted(() => ({
  decideChangeAssessment: vi.fn(),
  inspectChangeAssessments: vi.fn(),
}));

vi.mock("./quantixHost", () => host);

import { ChangeAssessmentPanel } from "./ChangeAssessmentPanel";

const assessment: ChangeAssessment = {
  assessment_sequence: 7,
  assessment_id: "assessment-7",
  relationship_id: "relationship-7",
  relationship_kind: "addendum",
  prior_source: {
    artifact_id: "source-prior",
    version: 1,
    package_path: "tender/prior.pdf",
    document_type: "pdf",
    sha256: "a".repeat(64),
    evidence_count: 1,
    evidence_preview: [
      {
        ordinal: 3,
        kind: "paragraph",
        structural_path: "page 2 / paragraph 3",
        original_text: "النص الملزم",
        translated_text: "Binding text",
        language: "arabic",
        text_sha256: "b".repeat(64),
        truncated: false,
      },
    ],
  },
  replacement_source: {
    artifact_id: "source-addendum",
    version: 1,
    package_path: "tender/addendum.pdf",
    document_type: "pdf",
    sha256: "c".repeat(64),
    evidence_count: 0,
    evidence_preview: [],
  },
  lifecycle_before: "intake",
  status: "pending",
  baseline_id: null,
  baseline_version: null,
  baseline_manifest_sha256: null,
  impacts: [],
  affected_commitments: [],
  proposed_rework: ["Classify the exact source change."],
  unchanged_scope: ["No current approved work is affected."],
  deadline_effect: "No verified deadline effect.",
  approval_consequences: [],
  decision: null,
  resolution_baseline_id: null,
  resolution_baseline_version: null,
  manifest_sha256: "d".repeat(64),
  created_at: "2026-08-12T00:00:00Z",
};

const page: ChangeAssessmentPage = {
  active: assessment,
  items: [assessment],
  next_before_sequence: null,
};

afterEach(cleanup);

describe("ChangeAssessmentPanel", () => {
  it("shows authoritative evidence and an explicit no-approval-impact result", async () => {
    host.inspectChangeAssessments.mockResolvedValue(page);

    render(
      <ChangeAssessmentPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={vi.fn()}
      />,
    );

    expect(
      await screen.findByText("Authoritative original-language text"),
    ).toBeTruthy();
    expect(
      screen.getByText("Derived translation / non-authoritative"),
    ).toBeTruthy();
    expect(
      screen.getByText(
        "No approval consequences; existing approvals remain valid.",
      ),
    ).toBeTruthy();
  });

  it("submits the exact active manifest and Manager classification", async () => {
    host.inspectChangeAssessments.mockResolvedValue(page);
    host.decideChangeAssessment.mockResolvedValue({});
    const onTenderStateChange = vi.fn();

    render(
      <ChangeAssessmentPanel
        tenderId="tender-1"
        refreshToken={0}
        reportCommandFailure={vi.fn()}
        onTenderStateChange={onTenderStateChange}
      />,
    );

    fireEvent.change(await screen.findByLabelText("Classification"), {
      target: { value: "irrelevant" },
    });
    fireEvent.change(screen.getByLabelText("Manager rationale"), {
      target: {
        value: "  Exact source change does not affect current work.  ",
      },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "Record immutable classification" }),
    );

    await waitFor(() =>
      expect(host.decideChangeAssessment).toHaveBeenCalledWith(
        "tender-1",
        "assessment-7",
        "d".repeat(64),
        "irrelevant",
        "Exact source change does not affect current work.",
      ),
    );
    expect(onTenderStateChange).toHaveBeenCalledOnce();
  });
});
