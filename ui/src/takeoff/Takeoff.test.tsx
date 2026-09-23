import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { TenderDocument } from "../documents/queries";
import type { BoqItem } from "../estimate/queries";
import { fakeService, openApp } from "../test/app";
import type { Measurement, Sheet } from "./queries";

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };
const drawing: TenderDocument = {
  id: "d1",
  path: "Drawings/A-101.pdf",
  name: "A-101.pdf",
  kind: "pdf",
  size: 1,
  status: "read",
  note: null,
  page_count: 1,
  group_name: null,
  description: null,
};
const sheet: Sheet = { document_id: "d1", name: "A-101.pdf", page: 1, width: 612, height: 792, scale: null };
const scaled: Sheet = {
  ...sheet,
  scale: {
    id: "sc1",
    metres_per_point: 0.1,
    line: [],
    length_m: 40,
    dimension: "40.00",
    status: "proposed",
    proposed_by: "s2",
  },
};
const wall: BoqItem = {
  id: "i1",
  section: null,
  item: "5.1",
  description: "External wall",
  unit: "m",
  quantity: "40.5",
  status: "approved",
  proposed_by: "s2",
  reason: null,
  source: { document_id: "d2", document_name: "Bill.xlsx", page: 1, quote: "" },
};
const doors: Measurement = {
  id: "m1",
  document_id: "d1",
  page: 1,
  kind: "count",
  label: "Doors",
  points: [
    [50, 50],
    [60, 60],
    [70, 70],
  ],
  unit: "nr",
  multiplier: null,
  quantity: "3",
  boq_item: "7.1",
  status: "proposed",
  proposed_by: "s2",
};

/** jsdom has no layout: give the drawing the page's own size so clicks land on page points. */
function pageSized(svg: Element) {
  Object.defineProperty(svg, "getBoundingClientRect", {
    value: () => ({ left: 0, top: 0, width: 612, height: 792, right: 612, bottom: 792, x: 0, y: 0 }),
  });
}

describe("Takeoff", () => {
  it("sets a sheet's scale from a printed dimension", async () => {
    const service = fakeService({ tenders: [tender], documents: [drawing], sheets: [sheet] });
    openApp("/tenders/t1/takeoff?doc=d1&page=1");

    expect(await screen.findByText(/No scale yet/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Scale" }));
    const svg = screen.getByLabelText("Measurements on the drawing");
    pageSized(svg);
    fireEvent.click(svg, { clientX: 100, clientY: 100 });
    fireEvent.click(svg, { clientX: 500, clientY: 100 });

    await userEvent.type(await screen.findByLabelText("Dimension as printed"), "40000");
    await userEvent.selectOptions(screen.getByLabelText("Printed in"), "millimetres");
    await userEvent.click(screen.getByRole("button", { name: "Set scale" }));
    await waitFor(() =>
      expect(service.state.scales[0]).toEqual({
        document_id: "d1",
        page: 1,
        line: [
          [101, 101], // the click at 100, 100 snapped onto the drawing's corner
          [500, 100],
        ],
        length_m: 40,
        dimension: "40000",
      }),
    );
  });

  it("measures a length and links it to a BOQ item", async () => {
    const service = fakeService({ tenders: [tender], documents: [drawing], sheets: [scaled], items: [wall] });
    openApp("/tenders/t1/takeoff?doc=d1&page=1");

    await userEvent.click(await screen.findByRole("button", { name: "Length" }));
    const svg = screen.getByLabelText("Measurements on the drawing");
    pageSized(svg);
    fireEvent.click(svg, { clientX: 100, clientY: 200 });
    fireEvent.click(svg, { clientX: 300, clientY: 200 });
    await userEvent.click(screen.getByRole("button", { name: "Finish" }));

    await userEvent.type(screen.getByLabelText("What is it"), "External wall");
    await userEvent.selectOptions(screen.getByLabelText("BOQ item"), "5.1 · External wall");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(service.state.measurements[0]).toMatchObject({
        kind: "length",
        label: "External wall",
        unit: "m",
        boq_item: "5.1",
        points: [
          [100, 200],
          [300, 200],
        ],
      }),
    );
  });

  it("shows the team's marks with their comparison, for approval", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [drawing],
      sheets: [scaled],
      measurements: [doors],
      comparison: [
        {
          boq_item_id: "i7",
          item: "7.1",
          description: "Doors",
          boq_quantity: "5",
          boq_unit: "nr",
          takeoff: "3",
          unit: "nr",
          result: "differs",
          difference: "-0.4000",
        },
      ],
    });
    openApp("/tenders/t1/takeoff?doc=d1&page=1");

    const panel = await screen.findByRole("complementary", { name: "Measurements" });
    expect(await within(panel).findByText("3 nr")).toBeInTheDocument();
    expect(within(panel).getByText("Differs -40.0%")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Approve scale" })).toBeInTheDocument();

    await userEvent.click(within(panel).getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(service.state.measurements[0].status).toBe("approved"));
  });
});
