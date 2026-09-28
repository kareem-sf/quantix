import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeService, openApp } from "../test/app";
import type { LayerMap, TenderQuery } from "./cad";

const tender = { id: "t1", name: "Synthetic flat", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const skirting: TenderQuery = {
  id: "q1",
  kind: "missing",
  kind_label: "Missing from the BOQ",
  title: "Skirting to both rooms",
  detail: "Skirting runs round both rooms but no BOQ item covers it.",
  wording: "The drawings show skirting round both rooms; no BOQ item covers it. Please add an item or confirm.",
  governs: "The drawings govern the extent of the work (clause 1.5).",
  sources: [{ document_id: "d3", document_name: "A-101.dwg", page: 1, quote: "LIVING", objects: ["1F/2A"] }],
  boq_item: null,
  figures: ["Skirting: 48.700 m"],
  status: "reviewed",
  proposed_by: "s2",
  reviewed_by: "s1",
  review_note: "Checked the preambles: skirting isn't deemed included.",
};
const map: LayerMap = {
  id: "lm1",
  document_id: "d3",
  document_name: "A-101.dwg",
  layers: { "A-WALL": "walls", "A-FLOR": "floor_finish" },
  blocks: { "DOOR-900": "doors" },
  note: "From what each layer holds.",
  status: "reviewed",
  proposed_by: "s2",
  reviewed_by: "s1",
  review_note: "Checked each layer on the plan.",
};

describe("Queries", () => {
  it("shows a query with where it shows and Quantix's figures, for the engineer to approve", async () => {
    const service = fakeService({ tenders: [tender], queries: [skirting] });
    openApp("/tenders/t1/queries");

    const card = (await screen.findByText("Skirting to both rooms")).closest("article")!;
    expect(within(card).getByText("Quantix’s takeoff: Skirting: 48.700 m")).toBeInTheDocument();
    expect(within(card).getByRole("link", { name: "A-101.dwg, page 1 (1 objects)" })).toHaveAttribute(
      "href",
      "/tenders/t1/documents?doc=d3&page=1",
    );
    expect(within(card).getByText(/The drawings show skirting round both rooms/)).toBeInTheDocument();
    await userEvent.click(within(card).getByRole("button", { name: "Approve for the client" }));
    await waitFor(() => expect(service.state.decided).toContainEqual({ id: "q1", approve: true, reason: null }));
  });

  it("shows the layer map for approval and what Quantix's checks find", async () => {
    const service = fakeService({
      tenders: [tender],
      layerMaps: [map],
      checks: [{ severity: "warning", message: "BOQ line 8.3 (Skirting) is billed but the office found nothing of it on the drawings: raise a query.", document_id: null, page: 1, objects: [] }],
    });
    openApp("/tenders/t1/queries");

    expect(await screen.findByText("Worked out on A-101.dwg")).toBeInTheDocument();
    expect(screen.getByText("floor finish")).toBeInTheDocument();
    expect(await screen.findByText(/BOQ line 8.3 \(Skirting\) is billed/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Approve the map" }));
    await waitFor(() => expect(service.state.decided).toContainEqual({ id: "lm1", approve: true, reason: null }));
  });
});
