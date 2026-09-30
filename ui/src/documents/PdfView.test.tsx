import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { fakeService, openApp } from "../test/app";
import { layout } from "../test/layout";
import type { TenderDocument } from "./queries";

type Sent = { pageNumber?: number; scale?: number; source?: { div: HTMLElement } };

// PDF.js draws on a real screen, which jsdom hasn't got: a stand-in viewer keeps the page and the zoom, and sends the
// events PDF.js sends, so what Quantix does with them shows.
const pdf = vi.hoisted(() => {
  class EventBus {
    listeners = new Map<string, ((data: Sent) => void)[]>();
    on(name: string, listener: (data: Sent) => void) {
      this.listeners.set(name, [...(this.listeners.get(name) ?? []), listener]);
    }
    dispatch(name: string, data: Sent = {}) {
      for (const listener of this.listeners.get(name) ?? []) listener(data);
    }
  }
  class PDFLinkService {
    externalLinkEnabled = true;
    setViewer = vi.fn();
    setDocument = vi.fn();
    constructor() {
      links.push(this);
    }
  }
  class PDFViewer {
    eventBus: EventBus;
    pagesCount = 0;
    page = 1;
    scale = 1;
    value = "";
    values: string[] = []; // every zoom Quantix set, as PDF.js names them
    scrollPageIntoView = vi.fn(({ pageNumber }: { pageNumber: number }) => this.scrollTo(pageNumber));
    increaseScale = vi.fn(() => this.zoomTo(this.scale * 1.25));
    decreaseScale = vi.fn(() => this.zoomTo(this.scale * 0.8));
    updateScale = vi.fn();
    constructor(options: { eventBus: EventBus }) {
      this.eventBus = options.eventBus;
      viewers.push(this);
    }
    setDocument(document: { numPages: number } | null) {
      this.pagesCount = document?.numPages ?? 0;
      if (document) this.eventBus.dispatch("pagesinit");
    }
    get currentPageNumber() {
      return this.page;
    }
    set currentPageNumber(page: number) {
      this.scrollTo(page);
    }
    get currentScaleValue() {
      return this.value;
    }
    set currentScaleValue(value: string) {
      this.values.push(value);
      this.zoomTo({ auto: 1, "page-fit": 0.8, "page-width": 1.5 }[value] ?? Number(value));
      this.value = value;
    }
    zoomTo(scale: number) {
      this.scale = scale;
      this.value = String(scale);
      this.eventBus.dispatch("scalechanging", { scale });
    }
    /** The page at the top of the view changing, as scrolling (the engineer's, or PDF.js's own) changes it. */
    scrollTo(page: number) {
      if (page === this.page) return;
      this.page = page;
      this.eventBus.dispatch("pagechanging", { pageNumber: page });
    }
  }
  type Task = {
    promise: Promise<{ numPages: number }>;
    open: (pages: number) => void;
    fail: (name: string) => void;
    destroy: () => Promise<void>;
  };
  const viewers: PDFViewer[] = [];
  const links: PDFLinkService[] = [];
  const tasks: Task[] = [];
  const getDocument = vi.fn((_options: object) => {
    const task = { destroy: vi.fn(async () => {}) } as unknown as Task;
    task.promise = new Promise((resolve, reject) => {
      task.open = (numPages) => resolve({ numPages });
      task.fail = (name) => reject({ name });
    });
    tasks.push(task);
    return task;
  });
  return { EventBus, PDFLinkService, PDFViewer, getDocument, viewer: () => viewers.at(-1)!, links: () => links.at(-1)!, task: () => tasks.at(-1)! };
});

vi.mock("pdfjs-dist", () => ({ GlobalWorkerOptions: { workerSrc: "" }, AnnotationMode: { ENABLE: 2 }, getDocument: pdf.getDocument }));
vi.mock("pdfjs-dist/web/pdf_viewer.mjs", () => ({ EventBus: pdf.EventBus, PDFLinkService: pdf.PDFLinkService, PDFViewer: pdf.PDFViewer }));

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };

function pdfDocument(id: string, name: string, pages: number): TenderDocument {
  return {
    id,
    path: `Conditions/${name}`,
    name,
    kind: "pdf",
    size: 100,
    status: "read",
    note: null,
    page_count: pages,
    group_name: null,
    description: null,
    scans_to_read: 0,
    opened: 0,
    cited: 0,
  };
}

const conditions = pdfDocument("d1", "Conditions.pdf", 46);

/** Waits for the PDF's view, lays it out 800 pixels wide and lets PDF.js finish opening the file. */
async function opened(name: string, pages = 46) {
  const view = await screen.findByLabelText(new RegExp(`^${name}, page`));
  Object.defineProperty(view, "clientWidth", { value: 800, configurable: true });
  await act(async () => pdf.task().open(pages));
  return view;
}

describe("A PDF in the viewer", () => {
  it("opens at the page the address names, fetching the file a part at a time", async () => {
    fakeService({ tenders: [tender], documents: [conditions] });
    openApp("/tenders/t1/documents?doc=d1&page=12");

    expect(await screen.findByText("Opening…")).toBeInTheDocument();
    await opened("Conditions.pdf");

    expect(screen.queryByText("Opening…")).not.toBeInTheDocument();
    expect(pdf.viewer().scrollPageIntoView).toHaveBeenLastCalledWith({ pageNumber: 12 });
    expect(screen.getByLabelText("Conditions.pdf, page 12")).toBeInTheDocument();
    expect(screen.getByLabelText("Page")).toHaveValue("12");
    expect(screen.getByText("of 46")).toBeInTheDocument();
    expect(screen.getByLabelText("Zoom")).toHaveTextContent("100%"); // fitted to the window as PDF.js fits it
    expect(pdf.getDocument).toHaveBeenLastCalledWith(
      expect.objectContaining({
        url: expect.stringContaining("/api/documents/d1/file"),
        rangeChunkSize: 1 << 20,
        disableAutoFetch: true,
        cMapUrl: "/pdfjs/cmaps/",
      }),
    );
    expect(pdf.links().externalLinkEnabled).toBe(false); // a web link in the PDF would take the window away
    // PDF.js's viewer finds the library, and its worker, where it looks for them
    const library = (globalThis as { pdfjsLib?: { getDocument: unknown; GlobalWorkerOptions: { workerSrc: string } } }).pdfjsLib;
    expect(library?.getDocument).toBe(pdf.getDocument);
    expect(library?.GlobalWorkerOptions.workerSrc).toMatch(/pdf\.worker\.min\.mjs/);
  });

  it("holds the page asked for while earlier pages of other sizes are drawn, until the engineer moves", async () => {
    fakeService({ tenders: [tender], documents: [conditions] });
    const router = openApp("/tenders/t1/documents?doc=d1&page=12");
    const view = await opened("Conditions.pdf");
    const viewer = pdf.viewer();

    act(() => viewer.scrollTo(11)); // a larger page drawn before it pushed page 12 down
    expect(screen.getByLabelText("Page")).toHaveValue("12");
    act(() => viewer.eventBus.dispatch("pagerendered"));
    expect(viewer.scrollPageIntoView).toHaveBeenLastCalledWith({ pageNumber: 12 });
    expect(viewer.page).toBe(12);
    expect(router.state.location.search).toContain("page=12");

    fireEvent.wheel(view, { deltaY: 120 }); // the engineer scrolls on
    act(() => viewer.scrollTo(13));
    expect(screen.getByLabelText("Page")).toHaveValue("13");
    expect(screen.getByLabelText("Conditions.pdf, page 13")).toBeInTheDocument();
    await waitFor(() => expect(router.state.location.search).toContain("page=13"));
    expect(router.state.historyAction).toBe("REPLACE"); // moving within a document adds no step to go back through
  });

  it("moves a page at a time, or to the page typed", async () => {
    fakeService({ tenders: [tender], documents: [conditions] });
    const router = openApp("/tenders/t1/documents?doc=d1&page=12");
    await opened("Conditions.pdf");

    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByLabelText("Page")).toHaveValue("13");
    await waitFor(() => expect(router.state.location.search).toContain("page=13"));
    await userEvent.click(screen.getByRole("button", { name: "Previous page" }));
    expect(pdf.viewer().page).toBe(12);

    const typed = screen.getByLabelText("Page");
    await userEvent.clear(typed);
    await userEvent.type(typed, "46{Enter}");
    expect(pdf.viewer().page).toBe(46);
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();
    await waitFor(() => expect(router.state.location.search).toContain("page=46"));

    await userEvent.clear(typed);
    await userEvent.type(typed, "99{Enter}"); // past the end: the page stays
    expect(typed).toHaveValue("46");
    await userEvent.clear(typed);
    await userEvent.type(typed, "3{Escape}");
    expect(typed).toHaveValue("46");
    expect(pdf.viewer().page).toBe(46);
  });

  it("goes to another page of the open PDF chosen from the search, and marks the words searched for", async () => {
    fakeService({
      tenders: [tender],
      documents: [conditions],
      hits: [
        { document_id: "d1", name: "Conditions.pdf", page: 12, snippet: "the [tender] [security] of one percent" },
        { document_id: "d1", name: "Conditions.pdf", page: 30, snippet: "return of the [tender] [security]" },
      ],
    });
    openApp("/tenders/t1/documents?q=tender+security&doc=d1&page=12");
    await opened("Conditions.pdf");
    const viewer = pdf.viewer();

    await userEvent.click(screen.getByRole("button", { name: /Conditions.pdf · page 30/ }));
    expect(viewer.scrollPageIntoView).toHaveBeenLastCalledWith({ pageNumber: 30 });
    expect(screen.getByLabelText("Page")).toHaveValue("30");

    const page = document.createElement("div");
    page.innerHTML =
      '<div class="textLayer"><span>Return of the Tender Security</span><span>Bid bond</span><span><span>security</span></span></div>';
    act(() => viewer.eventBus.dispatch("textlayerrendered", { source: { div: page } }));
    const marked = [...page.querySelectorAll(".search-hit")].map((hit) => hit.textContent);
    expect(marked).toEqual(["Tender", "Security", "security"]); // each word once, however PDF.js nested it
    expect(page.querySelector("span")).toHaveTextContent("Return of the Tender Security"); // the words themselves unchanged
  });

  it("zooms with the controls and Ctrl + wheel, and fits the whole page", async () => {
    fakeService({ tenders: [tender], documents: [conditions] });
    openApp("/tenders/t1/documents?doc=d1&page=1");
    const view = await opened("Conditions.pdf");
    const viewer = pdf.viewer();
    const zoom = screen.getByLabelText("Zoom");

    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(zoom).toHaveTextContent("125%");
    await userEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(zoom).toHaveTextContent("100%");
    await userEvent.click(screen.getByRole("button", { name: "Fit" }));
    expect(viewer.currentScaleValue).toBe("page-fit");
    expect(zoom).toHaveTextContent("80%");

    fireEvent.wheel(view, { deltaY: 120 });
    expect(viewer.updateScale).not.toHaveBeenCalled(); // a plain wheel scrolls
    fireEvent.wheel(view, { deltaY: -400, ctrlKey: true, clientX: 120, clientY: 80 });
    expect(viewer.updateScale).toHaveBeenCalledWith({ scaleFactor: Math.E, origin: [120, 80], drawingDelay: 300 });
  });

  it("keeps a fitted zoom fitted, and the page asked for in view, as the window changes size", async () => {
    const { resize } = layout();
    fakeService({ tenders: [tender], documents: [conditions] });
    openApp("/tenders/t1/documents?doc=d1&page=12");
    const view = await opened("Conditions.pdf");
    const viewer = pdf.viewer();
    viewer.scrollPageIntoView.mockClear();

    resize(view, 600, 700);
    expect(viewer.values).toEqual(["auto", "auto"]); // fitted again to the new width
    expect(viewer.scrollPageIntoView).toHaveBeenLastCalledWith({ pageNumber: 12 });

    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    resize(view, 900, 700);
    expect(viewer.values).toEqual(["auto", "auto"]); // a zoom the engineer chose stays as chosen
    expect(screen.getByLabelText("Zoom")).toHaveTextContent("125%");
  });

  it("says why a PDF can't be shown", async () => {
    fakeService({
      tenders: [tender],
      documents: [conditions, pdfDocument("d2", "Bond.pdf", 2), pdfDocument("d3", "Scan.pdf", 1)],
    });
    openApp("/tenders/t1/documents?doc=d1&page=1");
    const list = await screen.findByRole("region", { name: "Documents" });

    await screen.findByLabelText("Conditions.pdf, page 1");
    const first = pdf.task();
    await act(async () => first.fail("PasswordException"));
    expect(screen.getByText("This PDF is protected with a password, so it can't be shown here. Open the original.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Page")).not.toBeInTheDocument();

    await userEvent.click(within(list).getByText("Bond.pdf"));
    await screen.findByLabelText("Bond.pdf, page 1");
    expect(first.destroy).toHaveBeenCalled(); // the first file is let go
    await act(async () => pdf.task().fail("InvalidPDFException"));
    expect(screen.getByText("This PDF is damaged and can't be shown here. Open the original.")).toBeInTheDocument();

    await userEvent.click(within(list).getByText("Scan.pdf"));
    await screen.findByLabelText("Scan.pdf, page 1");
    await act(async () => pdf.task().fail("UnknownErrorException"));
    expect(screen.getByText("Quantix couldn’t show this PDF. Open the original.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open original" })).toBeEnabled();
  });
});
