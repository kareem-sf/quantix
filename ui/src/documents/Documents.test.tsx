import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { fakeService, openApp } from "../test/app";
import { drawingInfo, screenCopy } from "../test/drawing";
import { folderGroup, subfolder } from "./Documents";
import type { TenderDocument, WorkbookSheet } from "./queries";

// PDF.js and the Word renderer draw on a real screen, which jsdom hasn't got: stand-ins show what they were given.
vi.mock("./PdfView", () => ({
  default: (props: { url: string; page: number; query: string }) => (
    <p>
      PDF {props.url} at page {props.page} marking “{props.query}”
    </p>
  ),
}));
vi.mock("./WordView", () => ({
  default: (props: { url: string; page: number }) => (
    <p>
      Word {props.url} at part {props.page}
    </p>
  ),
}));

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };

function doc(id: string, path: string, extra: Partial<TenderDocument> = {}): TenderDocument {
  return {
    id,
    path,
    name: path.split("/").pop()!,
    kind: "word",
    size: 100,
    status: "read",
    note: null,
    page_count: 1,
    group_name: null,
    description: null,
    scans_to_read: 0,
    opened: 0,
    cited: 0,
    ...extra,
  };
}

const plain = {
  bold: false,
  italic: false,
  underline: false,
  strike: false,
  font: null,
  size: null,
  color: null,
  fill: null,
  align: null,
  valign: null,
  wrap: false,
  indent: 0,
  top: null,
  right: null,
  bottom: null,
  left: null,
};

describe("folder groups", () => {
  it("uses the folders inside the package, not the package folder itself", () => {
    const all = [doc("a", "Pkg/ITT.docx"), doc("b", "Pkg/Drawings/A-101.pdf")];
    expect(folderGroup("Pkg/ITT.docx", all)).toBe("Documents");
    expect(folderGroup("Pkg/Drawings/A-101.pdf", all)).toBe("Drawings");
    const loose = [doc("c", "Conditions/ITT.docx"), doc("d", "BOQ.xlsx")];
    expect(folderGroup("Conditions/ITT.docx", loose)).toBe("Conditions");
  });

  it("shows the folders below the group, such as a revision", () => {
    const all = [doc("a", "Pkg/BOQ/Rev0 (Old)/Part 1/Earth.xlsx"), doc("b", "Pkg/BOQ/Rev01 (Update)/Earth R-8486.xlsx")];
    expect(subfolder("Pkg/BOQ/Rev0 (Old)/Part 1/Earth.xlsx", all)).toBe("Rev0 (Old) / Part 1");
    expect(subfolder("Pkg/BOQ/Rev01 (Update)/Earth R-8486.xlsx", all)).toBe("Rev01 (Update)");
    expect(subfolder("Pkg/ITT.docx", [...all, doc("c", "Pkg/ITT.docx")])).toBe("");
  });
});

describe("Documents", () => {
  it("adds a folder from the overview, keeping its folder paths", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/tenders/t1");

    expect(await screen.findByText("Add the tender package")).toBeInTheDocument();
    const folder = new File(["%PDF"], "Conditions.pdf");
    Object.defineProperty(folder, "webkitRelativePath", { value: "Package/Conditions.pdf" });
    await userEvent.upload(screen.getByLabelText("Tender folder"), folder);

    expect(await screen.findByText("1 of 1 read")).toBeInTheDocument();
    expect(service.state.documents[0].path).toBe("Package/Conditions.pdf");
  });

  it("shows what the office made of each document and how much of it was read", async () => {
    const described = { group_name: "Specifications", description: "Technical specification for earthworks." };
    fakeService({
      tenders: [tender],
      documents: [
        doc("d1", "Pkg/Spec.pdf", { ...described, kind: "pdf", scans_to_read: 541, opened: 12, cited: 3 }),
        doc("d2", "Pkg/ITT.docx", { opened: 1 }),
      ],
    });
    openApp("/tenders/t1/documents");

    expect(await screen.findByText("Specifications")).toBeInTheDocument();
    expect(screen.getByText("Technical specification for earthworks.")).toBeInTheDocument();
    expect(screen.getByText("541 pages still to read by OCR · 12 pages opened by the office · 3 cited")).toBeInTheDocument();
    expect(screen.getByText("1 page opened by the office")).toBeInTheDocument();
  });

  it("opens each kind of file in its own view, and says why one can't be shown", async () => {
    fakeService({
      tenders: [tender],
      documents: [
        doc("d1", "Conditions/ITT.docx", { page_count: 4 }),
        doc("d2", "Drawings/A-101.pdf", { kind: "pdf", page_count: 12 }),
        doc("d3", "Photos/Site.jpg", { kind: "image" }),
        doc("d4", "Old/Bill.xls", { kind: "old_spreadsheet", status: "unreadable", note: "Old Excel files can’t be read." }),
      ],
    });
    const router = openApp("/tenders/t1/documents?doc=d1&page=3");

    const list = await screen.findByRole("region", { name: "Documents" });
    expect(await within(list).findByText("4 files · 1 can’t be read")).toBeInTheDocument();
    expect(await screen.findByText(/Word .*\/api\/documents\/d1\/file at part 3/)).toBeInTheDocument();
    expect(screen.getByText("Word document")).toBeInTheDocument();

    await userEvent.click(within(list).getByText("A-101.pdf"));
    expect(await screen.findByText(/PDF .*\/api\/documents\/d2\/file at page 1/)).toBeInTheDocument();
    expect(screen.getByText("PDF · 12 pages")).toBeInTheDocument();

    await userEvent.click(within(list).getByText("Site.jpg"));
    expect(await screen.findByRole("img", { name: "Site.jpg" })).toHaveAttribute(
      "src",
      expect.stringContaining("/api/documents/d3/pages/1/image"),
    );

    await userEvent.click(within(list).getByText("Bill.xls"));
    const viewer = screen.getByRole("region", { name: "Viewer" });
    expect(await within(viewer).findByText("Old Excel files can’t be read.")).toBeInTheDocument();
    expect(router.state.location.search).toContain("doc=d4");
  });

  it("shows a workbook as Excel does, a sheet at a time", async () => {
    const bill: WorkbookSheet = {
      sheets: [
        { name: "Bill 1", hidden: false },
        { name: "Rates", hidden: true },
      ],
      number: 1,
      right_to_left: false,
      gridlines: true,
      frozen_rows: 1,
      frozen_columns: 0,
      columns: [
        { letter: "A", width: 64, hidden: false },
        { letter: "B", width: 300, hidden: false },
        { letter: "C", width: 90, hidden: true },
      ],
      rows: [
        { number: 1, height: 20, hidden: false, cells: [{ column: 0, text: "Earthworks", style: 1, rows: 1, columns: 2 }] },
        {
          number: 2,
          height: 20,
          hidden: false,
          cells: [
            { column: 0, text: "3.1", style: 0, rows: 1, columns: 1 },
            { column: 1, text: "Excavation to reduce levels", style: 0, rows: 1, columns: 1 },
          ],
        },
      ],
      styles: [plain, { ...plain, bold: true, fill: "#ffff00", align: "center" }],
      more_rows: 0,
      more_columns: 0,
      hidden_rows: 0,
      hidden_columns: 1,
    };
    const labour = { column: 0, text: "Labour", style: 0, rows: 1, columns: 1 };
    const rates: WorkbookSheet = { ...bill, number: 2, rows: [{ number: 1, height: 20, hidden: false, cells: [labour] }] };
    fakeService({
      tenders: [tender],
      documents: [doc("d1", "BOQ.xlsx", { kind: "spreadsheet", page_count: 2 })],
      workbook: { 1: bill, 2: rates },
    });
    const router = openApp("/tenders/t1/documents?doc=d1&page=1&q=excavation");

    const title = await screen.findByRole("cell", { name: "Earthworks" });
    expect(title).toHaveAttribute("colspan", "2");
    expect(title).toHaveStyle({ fontWeight: "700", backgroundColor: "#ffff00", textAlign: "center" });
    expect(screen.getByRole("cell", { name: "Excavation to reduce levels" })).toHaveAttribute("data-found", "true");
    expect(screen.queryByRole("columnheader", { name: "C" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("Show hidden 1 column"));
    expect(await screen.findByRole("columnheader", { name: "C" })).toBeInTheDocument();

    await userEvent.click(within(screen.getByRole("navigation", { name: "Sheets" })).getByText("Rates (hidden)"));
    expect(await screen.findByRole("cell", { name: "Labour" })).toBeInTheDocument();
    expect(router.state.location.search).toContain("page=2");
  });

  it("draws a CAD drawing, with its layouts as tabs", async () => {
    fakeService({
      tenders: [tender],
      documents: [doc("d1", "Drawings/A-201.dwg", { kind: "cad", page_count: 2 })],
      drawing: { ...drawingInfo, pages: [...drawingInfo.pages, { ...drawingInfo.pages[0], number: 2, name: "A1 sheet", kind: "paper" }] },
      screen: screenCopy(),
    });
    const router = openApp("/tenders/t1/documents?doc=d1&page=1");

    expect(await screen.findByRole("img", { name: /objects$/ })).toBeInTheDocument();
    const layouts = await screen.findByRole("navigation", { name: "Layouts" });
    expect(within(layouts).getByText("Model")).toHaveAttribute("aria-current", "page");
    await userEvent.click(within(layouts).getAllByRole("button")[1]);
    expect(router.state.location.search).toContain("page=2");
  });

  it("opens the original in its own app, and says when none can", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [doc("d1", "A-101.dwg", { kind: "cad", status: "reading" })],
    });
    openApp("/tenders/t1/documents?doc=d1");

    expect(await screen.findByText("Drawing · Quantix is still reading it")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Open original" }));
    await waitFor(() => expect(service.state.opened).toEqual(["d1"]));

    service.state.fail["/documents/d1/open"] = "No app on this computer opens .dwg files.";
    await userEvent.click(screen.getByRole("button", { name: "Open original" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("No app on this computer opens .dwg files.");
  });

  it("searches every page, opens the hit with the words marked, and clears the search", async () => {
    fakeService({
      tenders: [tender],
      documents: [doc("d1", "Conditions.pdf", { kind: "pdf", page_count: 46 })],
      hits: [{ document_id: "d1", name: "Conditions.pdf", page: 12, snippet: "the [tender] [security] of one percent" }],
    });
    const router = openApp("/tenders/t1/documents");

    await userEvent.type(await screen.findByLabelText("Search the documents"), "tender security{Enter}");
    const hit = await screen.findByRole("button", { name: /Conditions.pdf · page 12/ });
    expect(within(hit).getByText("security").tagName).toBe("MARK");
    expect(screen.getByText("1 page found")).toBeInTheDocument();

    await userEvent.click(hit);
    await waitFor(() => expect(router.state.location.search).toContain("page=12"));
    expect(await screen.findByText(/at page 12 marking “tender security”/)).toBeInTheDocument();
    expect(hit).toHaveAttribute("aria-current", "true");

    await userEvent.click(screen.getByRole("button", { name: "Clear the search" }));
    expect(screen.getByLabelText("Search the documents")).toHaveValue("");
    const list = screen.getByRole("region", { name: "Documents" });
    expect(await within(list).findByText("Conditions.pdf")).toBeInTheDocument(); // the list is back, the file still open
    expect(router.state.location.search).not.toContain("q=");
    expect(router.state.location.search).toContain("doc=d1");
  });

  it("clears a search with Escape, says when nothing is found, and goes back to all documents", async () => {
    fakeService({ tenders: [tender], documents: [doc("d1", "Conditions.pdf", { kind: "pdf", page_count: 46 })] });
    const router = openApp("/tenders/t1/documents?doc=d1&page=abc");

    expect(await screen.findByText(/at page 1 marking/)).toBeInTheDocument(); // no such page: the first
    const field = screen.getByLabelText("Search the documents");
    await userEvent.type(field, "bid bond{Enter}");
    expect(await screen.findByText("Nothing found for “bid bond”.")).toBeInTheDocument();
    expect(router.state.location.search).toContain("doc=d1"); // the open file stays open
    await userEvent.type(field, "{Escape}");
    expect(field).toHaveValue("");
    expect(router.state.location.search).not.toContain("q=");

    await userEvent.type(field, "bond{Enter}");
    await userEvent.click(screen.getByRole("button", { name: "All documents" })); // on a narrow screen
    expect(router.state.location.search).toBe("?q=bond");
    expect(screen.getByText("Choose a document to see it here.")).toBeInTheDocument();
  });

  it("follows the reading as it happens", async () => {
    const service = fakeService({
      tenders: [tender],
      documents: [doc("d1", "ITT.docx", { status: "reading" }), doc("d2", "Spec.docx", { status: "waiting" })],
    });
    openApp("/tenders/t1/documents");

    const list = await screen.findByRole("region", { name: "Documents" });
    expect(await within(list).findByText("2 files · reading 2")).toBeInTheDocument();
    expect(within(list).getByText("Reading…")).toBeInTheDocument();
    expect(within(list).getByText("Waiting to be read")).toBeInTheDocument();

    for (const d of service.state.documents) d.status = "read";
    expect(await within(list).findByText("2 files", {}, { timeout: 4000 })).toBeInTheDocument();
    expect(within(list).queryByText("Reading…")).not.toBeInTheDocument();
  });

  it("says what each file is and how much of it there is, and why part of it can't be read", async () => {
    fakeService({
      tenders: [tender],
      documents: [
        doc("d1", "Addendum.pdf", { kind: "pdf", page_count: null }),
        doc("d2", "Rates.xlsx", { kind: "spreadsheet", page_count: 1 }),
        doc("d3", "Survey.pdf", { kind: "pdf", page_count: 3, status: "failed", note: "Page 2 is damaged and couldn’t be read." }),
      ],
    });
    openApp("/tenders/t1/documents?doc=d1");
    const list = await screen.findByRole("region", { name: "Documents" });
    const viewer = screen.getByRole("region", { name: "Viewer" });

    expect(await within(viewer).findByText("PDF")).toBeInTheDocument();
    await userEvent.click(within(list).getByText("Rates.xlsx"));
    expect(await within(viewer).findByText("Workbook · 1 sheet")).toBeInTheDocument();
    await userEvent.click(within(list).getByText("Survey.pdf"));
    expect(await within(viewer).findByText("PDF · 3 pages")).toBeInTheDocument();
    expect(within(viewer).getByText("Page 2 is damaged and couldn’t be read.")).toBeInTheDocument();
    expect(await within(viewer).findByText(/PDF .*\/api\/documents\/d3\/file at page 1/)).toBeInTheDocument(); // shown all the same
  });

  it("adds files from a browser, and says why they couldn't be added", async () => {
    const service = fakeService({ tenders: [tender] });
    openApp("/tenders/t1/documents");

    const picker = await screen.findByLabelText("Tender files");
    const opened = vi.fn();
    picker.addEventListener("click", opened);
    await userEvent.click(screen.getByRole("button", { name: "Add files" }));
    expect(opened).toHaveBeenCalled(); // the browser's file picker

    await userEvent.upload(picker, [new File(["%PDF"], "ITT.pdf"), new File(["%PDF"], "Drawings.pdf")]);
    expect(await screen.findByText("2 files added.")).toBeInTheDocument();
    expect(service.state.documents.map((d) => d.path)).toEqual(["ITT.pdf", "Drawings.pdf"]);

    service.state.fail["/tenders/t1/documents"] = "A file over 500 MB can’t be added.";
    await userEvent.upload(picker, new File(["%PDF"], "Survey.pdf"));
    expect(await screen.findByText("A file over 500 MB can’t be added.")).toBeInTheDocument();
  });

  it("says when a drawing can't be drawn", async () => {
    fakeService({ tenders: [tender], documents: [doc("d1", "Drawings/A-201.dwg", { kind: "cad" })], drawing: drawingInfo });
    openApp("/tenders/t1/documents?doc=d1&page=1");

    expect(await screen.findByText("Quantix couldn’t load this drawing.")).toBeInTheDocument();
  });
});
