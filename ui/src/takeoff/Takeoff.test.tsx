import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { TenderDocument } from "../documents/queries";
import type { BoqItem } from "../estimate/queries";
import { fakeService, openApp } from "../test/app";
import { drawingInfo, screenCopy } from "../test/drawing";
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
  scans_to_read: 0,
  opened: 0,
  cited: 0,
};
const sheet: Sheet = { document_id: "d1", name: "A-101.pdf", page: 1, width: 612, height: 792, scale: null, kind: "pdf", units: null, lines: false };
const scaled: Sheet = {
  ...sheet,
  scale: {
    id: "sc1",
    metres_per_point: 0.1,
    ratio: 283,
    line: [],
    length_m: 40,
    dimension: "40.00",
    status: "reviewed",
    proposed_by: "s2",
    reviewed_by: "s1",
    review_note: "Checked against the title block.",
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
  reviewed_by: "s1",
  review_note: null,
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
  status: "reviewed",
  proposed_by: "s2",
  reviewed_by: "s1",
  review_note: "Three doors on the plan.",
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
    expect(screen.getByText("· about 1:283")).toBeInTheDocument(); // to compare with the title block
    expect(screen.getByRole("option", { name: "A-101.pdf · page 1 · needs you" })).toBeInTheDocument(); // found from the list

    const sheetArea = screen.getByRole("region", { name: "Drawing" });
    await userEvent.click(within(sheetArea).getByRole("button", { name: "Send back" }));
    await userEvent.type(within(sheetArea).getByLabelText("What to put right"), "Use the 40.00 m dimension on grid A.");
    await userEvent.click(within(sheetArea).getByRole("button", { name: "Send" }));
    await waitFor(() =>
      expect(service.state.decided).toContainEqual({ id: "sc1", approve: false, reason: "Use the 40.00 m dimension on grid A." }),
    );

    await userEvent.click(within(panel).getByRole("button", { name: "Approve" }));
    await waitFor(() => expect(service.state.measurements[0].status).toBe("approved"));
  });
});

const cadDocument: TenderDocument = { ...drawing, id: "d3", path: "Drawings/A-201.dwg", name: "A-201.dwg", kind: "cad" };
const cadSheet: Sheet = { document_id: "d3", name: "A-201.dwg", page: 1, width: 20, height: 20, scale: null, kind: "cad", units: null, lines: false };

describe("Takeoff from a CAD drawing", () => {
  it("sets the drawing's units from what it says", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [cadDocument],
      sheets: [cadSheet],
      drawing: drawingInfo,
      screen: screenCopy(),
    });
    openApp("/tenders/t1/takeoff?doc=d3&page=1");

    expect(await screen.findByText("No units yet: the drawing says millimetres")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Set units" }));
    await waitFor(() => expect(service.state.units).toEqual([{ document_id: "d3", units: "millimetres" }]));
  });

  it("measures the objects the engineer chooses on the drawing", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [cadDocument],
      sheets: [cadSheet],
      drawing: drawingInfo,
      screen: screenCopy(),
      items: [wall],
    });
    openApp("/tenders/t1/takeoff?doc=d3&page=1");

    const view = await screen.findByRole("img", { name: "Model: 3 objects" });
    fireEvent.pointerDown(view, { clientX: 400, clientY: 300 }); // the drawing's centre, on the wall line
    fireEvent.pointerUp(view, { clientX: 400, clientY: 300 });
    expect(await screen.findByText("1 object chosen")).toBeInTheDocument();
    expect(await screen.findByText(/1 to count · 20 m long/)).toBeInTheDocument(); // Quantix's figures

    expect(screen.getByText("line on A-WALL")).toBeInTheDocument(); // what the one object is
    const panel = screen.getByRole("complementary", { name: "Measurements" });
    await userEvent.click(within(panel).getByRole("button", { name: "Length" }));
    await userEvent.type(screen.getByLabelText("What is it"), "Wall");
    await userEvent.selectOptions(screen.getByLabelText("BOQ item"), "5.1 · External wall");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(service.state.measured[0]).toEqual({
        document_id: "d3",
        page: 1,
        kind: "length",
        label: "Wall",
        unit: "m",
        multiplier: null,
        boq_item: "5.1",
        choice: { stamp: "st1", objects: [0] },
      }),
    );
    const asked = service.fetch.mock.calls.map(([input]) => (input instanceof Request ? input.url : String(input)));
    expect(asked.some((url) => url.includes("/vertices"))).toBe(false); // a drawing has no PDF vertices to snap to
  });

  it("measures 3D solids by volume, and by weight with a density", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [cadDocument],
      sheets: [cadSheet],
      drawing: drawingInfo,
      screen: screenCopy(),
      volume: 0.061,
    });
    openApp("/tenders/t1/takeoff?doc=d3&page=1");

    const view = await screen.findByRole("img", { name: "Model: 3 objects" });
    fireEvent.pointerDown(view, { clientX: 400, clientY: 300 });
    fireEvent.pointerUp(view, { clientX: 400, clientY: 300 });
    expect(await screen.findByText(/0.061 m³ solid/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Volume" }));
    await userEvent.type(screen.getByLabelText("What is it"), "Ladder steel");
    await userEvent.selectOptions(screen.getByLabelText("Unit"), "kg");
    expect(screen.getByText("Density in kg per m³")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Multiplier"), "7850");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(service.state.measured[0]).toMatchObject({ kind: "volume", unit: "kg", multiplier: "7850" }),
    );
  });
});

const units = { ...scaled.scale!, id: "u1", metres_per_point: 0.001, ratio: 0, line: [], dimension: "millimetres", status: "approved" };

/** The screen copy's page is 20 units square about its centre; jsdom's drawing is 800 × 600 pixels, so it is drawn
 * 28.2 pixels to the unit about the point 400, 300. */
const at = (x: number, y: number) => ({ clientX: 400 + x * 28.2, clientY: 300 - y * 28.2 });

describe("Measuring on a drawing's own lines", () => {
  it("places points on a CAD drawing, snapped onto its lines' ends", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [cadDocument],
      sheets: [{ ...cadSheet, scale: units }],
      drawing: drawingInfo,
      screen: screenCopy(),
    });
    openApp("/tenders/t1/takeoff?doc=d3&page=1");
    const view = await screen.findByRole("img", { name: "Model: 3 objects" });
    expect(screen.queryByRole("button", { name: "Scale" })).not.toBeInTheDocument(); // a CAD drawing has units

    await userEvent.click(screen.getByRole("button", { name: "Length" }));
    for (const [x, y] of [
      [-9.9, 0.1], // near the wall's west end
      [9.85, -0.1], // near its east end
    ]) {
      fireEvent.pointerDown(view, at(x, y));
      fireEvent.pointerUp(view, at(x, y));
    }
    expect(screen.getByText("0.02 m so far")).toBeInTheDocument(); // 20 of the drawing's millimetres, as placed
    await userEvent.click(screen.getByRole("button", { name: "Finish" }));
    await userEvent.type(screen.getByLabelText("What is it"), "Kerb");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(service.state.measurements[0]).toMatchObject({
        document_id: "d3",
        page: 1,
        kind: "length",
        points: [
          [4990, 4000], // snapped onto the ends, in the drawing's own units
          [5010, 4000],
        ],
      }),
    );
  });

  it("takes the area the lines close off around a click", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [cadDocument],
      sheets: [{ ...cadSheet, scale: units }],
      drawing: drawingInfo,
      screen: screenCopy(),
    });
    openApp("/tenders/t1/takeoff?doc=d3&page=1");
    const view = await screen.findByRole("img", { name: "Model: 3 objects" });
    await userEvent.click(screen.getByRole("button", { name: "Enclosed" }));
    fireEvent.pointerDown(view, at(0, 3));
    fireEvent.pointerUp(view, at(0, 3));
    expect(await screen.findByText("New area")).toBeInTheDocument();
    expect(screen.getByText("The lines close off 100.00 m², 40.00 m round.")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("What is it"), "Yard");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(service.state.measurements[0]).toMatchObject({
        kind: "area",
        points: [
          [4995, 3998],
          [5005, 3998],
          [5005, 4008],
          [4995, 4008],
        ],
      }),
    );
  });

  it("draws a PDF printed from CAD from its lines, and chooses by box, by likeness and by words", async () => {
    const pdfDrawing = { ...drawing, id: "d4", name: "Site.pdf", path: "Drawings/Site.pdf" };
    const lines: Sheet = { ...scaled, document_id: "d4", name: "Site.pdf", width: 20, height: 20, lines: true };
    const service = fakeService({
      tenders: [tender],
      documents: [pdfDrawing],
      sheets: [lines],
      drawing: drawingInfo,
      screen: screenCopy(),
      items: [wall],
    });
    openApp("/tenders/t1/takeoff?doc=d4&page=1");
    const view = await screen.findByRole("img", { name: "Model: 3 objects" });
    expect(screen.getByText(/Scale checked on the 40.00 dimension/)).toBeInTheDocument();

    // a box dragged left to right with shift takes what lies wholly inside it: the wall, not the door above it
    fireEvent.pointerDown(view, { ...at(-11, -1), shiftKey: true });
    fireEvent.pointerMove(view, { ...at(11, 1), shiftKey: true });
    fireEvent.pointerUp(view, { ...at(11, 1), shiftKey: true });
    expect(await screen.findByText("1 object chosen")).toBeInTheDocument();
    expect(screen.getByText("line on A-WALL")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Clear the choice" }));

    await userEvent.type(screen.getByLabelText("Find words on the drawing"), "kitchen");
    expect(screen.getByText("1 of 1")).toBeInTheDocument();
    await userEvent.clear(screen.getByLabelText("Find words on the drawing"));
    await userEvent.click(screen.getByRole("button", { name: "Fit" })); // finding the word zoomed to it

    fireEvent.pointerDown(view, at(0, 0));
    fireEvent.pointerUp(view, at(0, 0));
    await userEvent.click(within(screen.getByRole("complementary", { name: "Measurements" })).getByRole("button", { name: "Length" }));
    await userEvent.type(screen.getByLabelText("What is it"), "Wall");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(service.state.measured[0]).toMatchObject({ document_id: "d4", page: 1, kind: "length" }));
    const asked = service.fetch.mock.calls.map(([input]) => (input instanceof Request ? input.url : String(input)));
    expect(asked.some((url) => url.includes("/vertices"))).toBe(false); // it snaps to the lines themselves
  });
});
