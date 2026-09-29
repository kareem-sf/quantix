import "pdfjs-dist/web/pdf_viewer.css";
import { ASSETS, pdfjs } from "./pdfjs";
import { EventBus, PDFLinkService, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { PageControls, searchTerms, termsPattern, useCtrlWheel, Waiting, ZoomControls } from "./Viewer";

const FITS = ["auto", "page-fit", "page-width"];
const MOVES = ["wheel", "pointerdown", "keydown", "touchstart"];

/** A PDF as a PDF reader shows it: every page drawn from the file itself, sharp at any zoom, with text that can be
 * selected and copied. Pages scroll one after another; the page in view is kept in the address. Opened from a
 * search, the words searched for are marked on each page. */
export default function PdfView(props: {
  url: string;
  name: string;
  page: number;
  query: string;
  toolbar: HTMLElement | null;
  onPage: (page: number) => void;
}) {
  const [container, setContainer] = useState<HTMLDivElement | null>(null);
  const [pdfViewer, setPdfViewer] = useState<PDFViewer | null>(null);
  const [count, setCount] = useState(0);
  const [current, setCurrent] = useState(props.page);
  const [scale, setScale] = useState<number | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // The page asked for (by a source, a search hit, the address), held in view until the engineer moves: a long PDF
  // learns each page's size only as it draws it, so pages of other sizes before it would push it along.
  const held = useRef<number | null>(props.page);
  const report = useRef(props.onPage);
  report.current = props.onPage;
  const pattern = useMemo(() => termsPattern(searchTerms(props.query)), [props.query]);
  const marks = useRef(pattern);
  marks.current = pattern;

  useEffect(() => {
    if (!container) return;
    const eventBus = new EventBus();
    const links = new PDFLinkService({ eventBus });
    links.externalLinkEnabled = false; // a web link would take the whole window away from Quantix
    const pdfViewer = new PDFViewer({
      container,
      eventBus,
      linkService: links,
      annotationMode: pdfjs.AnnotationMode.ENABLE, // links inside the document work; forms are shown, not filled
      imagesRightClickMinSize: -1,
      enableAutoLinking: false,
    });
    links.setViewer(pdfViewer);
    setPdfViewer(pdfViewer);
    const hold = () => {
      const page = held.current;
      if (page !== null && container.clientWidth > 0) pdfViewer.scrollPageIntoView({ pageNumber: page });
    };
    eventBus.on("pagesinit", () => {
      pdfViewer.currentScaleValue = "auto";
      held.current = Math.min(Math.max(held.current ?? 1, 1), pdfViewer.pagesCount);
      hold();
    });
    eventBus.on("pagerendered", hold);
    eventBus.on("pagechanging", ({ pageNumber }: { pageNumber: number }) => {
      if (held.current !== null) return setCurrent(held.current);
      setCurrent(pageNumber);
      report.current(pageNumber);
    });
    eventBus.on("scalechanging", ({ scale }: { scale: number }) => setScale(scale));
    eventBus.on("textlayerrendered", ({ source }: { source: { div: HTMLElement } }) => mark(source.div, marks.current));

    // the engineer moving through the document lets go of the page asked for
    const move = () => (held.current = null);
    for (const event of MOVES) container.addEventListener(event, move);

    const task = pdfjs.getDocument({
      url: props.url,
      ...ASSETS,
      rangeChunkSize: 1 << 20, // a large tender PDF opens by the megabyte, only the parts on screen
      disableAutoFetch: true,
      disableStream: true,
    });
    task.promise.then(
      (pdf) => {
        pdfViewer.setDocument(pdf);
        links.setDocument(pdf);
        setCount(pdf.numPages);
      },
      (error: { name?: string }) =>
        setProblem(
          error?.name === "PasswordException"
            ? "This PDF is protected with a password, so it can't be shown here. Open the original."
            : error?.name === "InvalidPDFException"
              ? "This PDF is damaged and can't be shown here. Open the original."
              : "Quantix couldn’t show this PDF. Open the original.",
        ),
    );
    return () => {
      for (const event of MOVES) container.removeEventListener(event, move);
      setPdfViewer(null);
      pdfViewer.setDocument(null as never);
      links.setDocument(null);
      void task.destroy();
    };
  }, [container, props.url]);

  // a page chosen elsewhere (a source, a search hit) while the document is open
  useEffect(() => {
    if (!pdfViewer?.pagesCount || pdfViewer.currentPageNumber === props.page) return;
    held.current = Math.min(Math.max(props.page, 1), pdfViewer.pagesCount);
    setCurrent(held.current);
    pdfViewer.scrollPageIntoView({ pageNumber: held.current });
  }, [props.page, pdfViewer]);

  // a fitted zoom stays fitted when the window or the panel changes size, as in a PDF reader
  useEffect(() => {
    if (!container || !pdfViewer || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      if (!pdfViewer.pagesCount || !container.clientWidth) return;
      if (FITS.includes(pdfViewer.currentScaleValue)) pdfViewer.currentScaleValue = pdfViewer.currentScaleValue;
      if (held.current !== null) pdfViewer.scrollPageIntoView({ pageNumber: held.current });
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [container, pdfViewer]);

  const zoom = useCallback(
    (factor: number, event: WheelEvent) =>
      pdfViewer?.updateScale({ scaleFactor: factor, origin: [event.clientX, event.clientY], drawingDelay: 300 }),
    [pdfViewer],
  );
  useCtrlWheel(container, zoom);

  const go = (page: number) => {
    held.current = null;
    if (pdfViewer) pdfViewer.currentPageNumber = page;
  };
  return (
    <>
      {props.toolbar &&
        count > 0 &&
        pdfViewer &&
        createPortal(
          <>
            <PageControls page={current} count={count} onPage={go} />
            <ZoomControls
              scale={scale}
              onOut={() => pdfViewer.decreaseScale()}
              onIn={() => pdfViewer.increaseScale()}
              onFit={() => (pdfViewer.currentScaleValue = "page-fit")}
              fit="Fit the whole page"
            />
          </>,
          props.toolbar,
        )}
      <div
        ref={setContainer}
        tabIndex={0}
        aria-label={`${props.name}, page ${current}`}
        className="pdf-view absolute inset-0 overflow-auto outline-none"
      >
        <div className="pdfViewer" />
      </div>
      {problem ? (
        <div className="absolute inset-0 flex bg-subtle">
          <Waiting>{problem}</Waiting>
        </div>
      ) : (
        count === 0 && (
          <div className="absolute inset-0 flex">
            <Waiting>Opening…</Waiting>
          </div>
        )
      )}
    </>
  );
}

/** Marks the words searched for in a page's text, as PDF.js marks its own finds. */
function mark(page: HTMLElement, pattern: RegExp | null) {
  if (!pattern) return;
  const test = new RegExp(pattern.source, "iu");
  for (const span of page.querySelectorAll<HTMLElement>(".textLayer span")) {
    const text = span.textContent ?? "";
    if (span.childElementCount || !test.test(text)) continue;
    span.replaceChildren(
      ...text.split(pattern).map((part, i) => {
        if (i % 2 === 0) return document.createTextNode(part);
        const hit = document.createElement("span");
        hit.className = "search-hit";
        hit.textContent = part;
        return hit;
      }),
    );
  }
}
