import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Schema } from "../../api";
import { StaffDraftContent } from "./StaffDraftContent";

const result: Schema<"StaffResult"> = {
  id: "result-all",
  tender_id: "tender-a",
  assignment_id: "assignment-1",
  root_run_id: "run-1",
  staff_id: "staff-1",
  staff_version: 3,
  work_order_id: "work-order-1",
  route_binding_id: "route-1",
  office_output: {
    summary: "Complete tender review draft",
    source_ids: ["source-main"],
    boq_item_proposals: [
      {
        source_id: "source-boq",
        row_reference: "Item 12",
        source_excerpt: "Item 12 Concrete slab m3 24.50",
        description: "Concrete slab",
        unit: "m3",
        quantity: "24.50",
      },
    ],
    findings: [
      {
        title: "Access risk",
        detail: "The access route needs confirmation.",
        kind: "risk",
        source_ids: ["source-risk"],
      },
    ],
    plan: {
      title: "Review plan",
      tasks: [
        {
          title: "Check access",
          description: "Compare the access note with the current drawing.",
          role: "Document reviewer",
          source_ids: ["source-plan"],
          ai_route: {
            connection_id: "connection-1",
            model_id: "model-1",
            reasoning: "medium",
            max_output_tokens: 1000,
            web_search: false,
            max_search_calls: 0,
          },
        },
      ],
    },
    web_findings: [
      {
        title: "Market note",
        detail: "Public guidance was found.",
        urls: ["https://example.com/guidance"],
      },
    ],
    price_proposals: [
      {
        item: "Door hardware",
        amount: "120",
        currency: "GBP",
        unit: "item",
        location: "London",
        tax_basis: "excluding VAT",
        basis: "observed",
        observed_on: "2026-09-10",
        valid_until: null,
        conditions: "Subject to supplier confirmation.",
        urls: ["https://example.com/price"],
      },
    ],
    quote_drafts: [
      {
        to: ["supplier@example.com"],
        cc: [],
        subject: "Request for door hardware quote",
        body: "Please confirm the current price.",
        attachment_ids: ["artifact-quote"],
        source_ids: ["source-quote"],
      },
    ],
    unit_rate_proposals: [
      {
        item_id: "boq-1",
        unit_rate: "35",
        currency: "GBP",
        tax_basis: "excluding_vat",
        vat_percent: "20",
        components: [
          { name: "Labour", quantity: "1", unit_rate: "25", unit: "hour" },
        ],
        provenance: {
          basis: "estimated",
          observed_on: "2026-09-10",
          source_ids: ["source-rate"],
          urls: [],
          geography: "London",
          conditions: "Budget estimate",
        },
      },
    ],
    project_map_nodes: [
      {
        kind: "area",
        title: "North block",
        detail: "Tender area.",
        source_ids: ["source-map"],
        parent_id: null,
      },
    ],
    submission_requirements: [
      {
        title: "Method statement",
        source_quote:
          "For alternative designs, submit the method statement unless using the client design.",
        applicability: "conditional",
        condition: "For alternative designs",
        exceptions: ["Unless using the client design"],
        detail: "Submit the method statement.",
        source_ids: ["source-requirement"],
        deliverable_kind: "technical_docx",
        due_date: "2026-10-01",
      },
    ],
    programme_proposal: {
      title: "Tender programme",
      start_date: "2026-10-01",
      working_week: [1, 2, 3, 4, 5],
      holidays: [],
      activities: [
        {
          id: "activity-1",
          title: "Mobilise",
          duration_days: 3,
          predecessor_ids: [],
          source_ids: ["source-programme"],
          assumptions: ["Site access available"],
        },
      ],
      assumptions: ["Engineer confirms access"],
    },
    drawing_measurements: [
      {
        artifact_id: "artifact-drawing",
        page: 2,
        mode: "length",
        points: [
          [1, 2],
          [3, 4],
        ],
        calibration_points: null,
        calibration_metres: "10",
        scope_label: "North elevation",
        source_ids: ["source-measurement"],
      },
    ],
    draft_documents: [
      { kind: "programme_xlsx", task_id: "task-1", programme: null },
    ],
    quantity_proposals: [
      {
        item_id: "boq-1",
        quantity: "12",
        calculation: "4 x 3",
        source_ids: ["source-quantity"],
      },
    ],
  },
  authored_notes: ["Engineer should confirm access."],
  source_ids_read: ["source-main"],
  source_bases: [
    {
      source_id: "source-main",
      artifact_id: "artifact-main",
      artifact_version: 4,
      artifact_hash: "hash-main",
      locator: "page 1",
    },
  ],
  web_sources: [{ url: "https://example.com/guidance", title: "Guidance" }],
  item_bases: [["boq-1", "source-quantity"]],
  trusted_recipients: ["manager-1"],
  source_recipients: [["source-main", ["manager-1"]]],
  approved_plan_id: "plan-1",
  usage: { requests: 1, tokens: 10 },
  created_at: "2026-09-10T08:00:00Z",
  currentness: "needs_review",
};

it("exposes every populated OfficeOutput section as inspectable draft content", () => {
  render(<StaffDraftContent result={result} onSource={vi.fn()} />);

  for (const label of [
    "Complete tender review draft",
    "Finding proposals",
    "Plan proposal",
    "Web research findings",
    "Price proposals",
    "Quote drafts",
    "Unit rate proposals",
    "Project map proposals",
    "Submission requirements",
    "Programme proposal",
    "Drawing measurements",
    "Draft documents",
    "Quantity proposals",
    "Source BOQ row proposals",
  ])
    expect(screen.getByText(label)).toBeInTheDocument();

  expect(
    screen.getByText("Engineer should confirm access."),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/This is draft engineering content/),
  ).toBeInTheDocument();
  expect(
    screen.getByText(
      "For alternative designs, submit the method statement unless using the client design.",
    ),
  ).toBeVisible();
  expect(screen.getByText("For alternative designs")).toBeVisible();
  expect(screen.getByText("Unless using the client design")).toBeVisible();
  expect(screen.getByText("Only when stated conditions apply")).toBeVisible();
  expect(screen.getByText("Item 12 Concrete slab m3 24.50")).toBeVisible();
  expect(screen.getByText("Item 12")).toBeVisible();
  expect(screen.getByRole("button", { name: "source-boq" })).toBeVisible();
});
