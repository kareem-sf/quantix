import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fakeService, openApp } from "../test/app";
import { layout } from "../test/layout";
import type { TenderDocument } from "./queries";

// The Word renderer lays pages out on a real screen, which jsdom hasn't got: the stand-in lays out the file as it is,
// so each synthetic file below is written as the pages the renderer would make of it.
const renderAsync = vi.hoisted(() => vi.fn(async (file: Blob, body: HTMLElement) => void (body.innerHTML = await file.text())));
vi.mock("docx-preview", () => ({ renderAsync }));

const scrolled = vi.fn(); // jsdom can't scroll
beforeEach(() => Object.defineProperty(Element.prototype, "scrollIntoView", { value: scrolled, configurable: true }));
afterEach(() => {
  delete (Element.prototype as Partial<Element>).scrollIntoView;
  scrolled.mockClear();
  renderAsync.mockClear();
  vi.restoreAllMocks();
});

const tender = { id: "t1", name: "Synthetic school", due_date: null, created_at: "2026-09-23T10:00:00Z" };

function wordDocument(id: string, name: string): TenderDocument {
  return {
    id,
    path: `Conditions/${name}`,
    name,
    kind: "word",
    size: 100,
    status: "read",
    note: null,
    page_count: 6,
    group_name: null,
    description: null,
    scans_to_read: 0,
    opened: 0,
    cited: 0,
  };
}

const conditions = wordDocument("d1", "Particular Conditions.docx");
const laidOut = `<div class="docx-wrapper"><section class="docx">
  <p>1 General</p>
  <p><span>14.2</span>  <span>Retention   Money</span></p>
  <p>The Employer shall retain five percent of each payment.</p>
  <p>See <a href="https://example.com/conditions">the published conditions</a> and <a href="#_Toc2">the contents</a>.</p>
</section></div>`;

describe("A Word document in the viewer", () => {
  it("lays the document out as Word does, and opens where the cited part starts", async () => {
    fakeService({
      tenders: [tender],
      documents: [conditions],
      files: { d1: laidOut },
      pages: { d1: "\n14.2 Retention Money\nThe Employer shall retain five percent of each payment." },
    });
    openApp("/tenders/t1/documents?doc=d1&page=3");

    expect(await screen.findByText("Opening…")).toBeInTheDocument();
    const cited = (await screen.findByText("Retention Money")).closest("p")!;
    await waitFor(() => expect(cited).toHaveClass("cited"));
    expect(scrolled.mock.contexts).toContain(cited);
    expect(scrolled).toHaveBeenCalledWith({ block: "center" });
    expect(renderAsync).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.anything(),
      expect.anything(),
      expect.objectContaining({ breakPages: true, ignoreLastRenderedPageBreak: false, renderChanges: false, renderComments: false }),
    );

    await waitFor(() => expect(cited).not.toHaveClass("cited"), { timeout: 4000 }); // shown briefly
    expect(screen.queryByText("Opening…")).not.toBeInTheDocument();
  });

  it("fits the page's width without enlarging it, and zooms with the controls and Ctrl + wheel", async () => {
    const { resize } = layout();
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(794); // an A4 page
    const service = fakeService({ tenders: [tender], documents: [conditions], files: { d1: laidOut } });
    openApp("/tenders/t1/documents?doc=d1&page=1");

    const scroller = (await screen.findByText("The Employer shall retain five percent of each payment.")).closest(".word-view")!;
    const zoom = screen.getByLabelText("Zoom");
    resize(scroller, 500, 700);
    expect(zoom).toHaveTextContent("57%");
    resize(scroller, 1400, 700);
    expect(zoom).toHaveTextContent("100%"); // a wide window leaves the page at its own size

    await userEvent.click(screen.getByRole("button", { name: "Fit" }));
    expect(zoom).toHaveTextContent("170%");
    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(zoom).toHaveTextContent("175%");
    await userEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(zoom).toHaveTextContent("150%");
    fireEvent.wheel(scroller, { deltaY: 400 });
    expect(zoom).toHaveTextContent("150%");
    fireEvent.wheel(scroller, { deltaY: 400, ctrlKey: true });
    expect(zoom).toHaveTextContent("55%");

    const asked = service.fetch.mock.calls.map(([input]) => (input instanceof Request ? input.url : String(input)));
    expect(asked.some((url) => url.includes("/documents/d1/pages/"))).toBe(false); // the first part is the start
  });

  it("follows links within the document, and keeps web links from taking the window away", async () => {
    fakeService({ tenders: [tender], documents: [conditions], files: { d1: laidOut } });
    openApp("/tenders/t1/documents?doc=d1&page=1");

    expect(fireEvent.click(await screen.findByRole("link", { name: "the published conditions" }))).toBe(false);
    expect(fireEvent.click(screen.getByRole("link", { name: "the contents" }))).toBe(true);
  });

  it("says when a Word document can't be shown", async () => {
    fakeService({
      tenders: [tender],
      documents: [conditions, wordDocument("d2", "Specification.docx")],
      files: { d2: "<p>A file the renderer can't read</p>" },
    });
    openApp("/tenders/t1/documents?doc=d1&page=1");
    const message = "Quantix couldn’t show this Word document. Open the original.";

    expect(await screen.findByText(message)).toBeInTheDocument(); // the file couldn't be fetched
    renderAsync.mockRejectedValueOnce(new Error("Unsupported file."));
    await userEvent.click(within(screen.getByRole("region", { name: "Documents" })).getByText("Specification.docx"));
    await waitFor(() => expect(renderAsync).toHaveBeenCalledTimes(1));
    expect(await screen.findByText(message)).toBeInTheDocument();
    expect(screen.queryByText("A file the renderer can't read")).not.toBeInTheDocument();
  });
});
