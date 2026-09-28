import { renderAsync } from "docx-preview";
import { useCallback, useEffect, useRef, useState, type MouseEvent } from "react";
import { createPortal } from "react-dom";
import { usePage } from "./queries";
import { useCtrlWheel, useSize, Waiting, ZoomControls, zoomStep } from "./Viewer";

const MARGIN = 48; // grey either side of the page

/** A Word document laid out as Word lays it out: its pages, fonts, headings, lists, tables, pictures, headers and
 * footers. Quantix reads a Word document in parts of about 3,000 characters; a source that names a part opens the
 * document where that part starts. */
export default function WordView(props: { documentId: string; url: string; page: number; toolbar: HTMLElement | null }) {
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<"opening" | "ready" | "failed">("opening");
  const [pageWidth, setPageWidth] = useState(0);
  const [chosen, setChosen] = useState<number | null>(null); // null: fitted to the window, never above 100%
  const { width } = useSize(scroller);
  const part = usePage(props.page > 1 ? props.documentId : undefined, props.page);
  const room = width - MARGIN;
  const fitted = room > 200 && pageWidth ? Math.min(Math.max(room / pageWidth, 0.25), 2) : 1;
  const zoom = chosen ?? Math.min(fitted, 1);

  useEffect(() => {
    let live = true;
    (async () => {
      const response = await fetch(props.url);
      if (!response.ok) throw new Error(`The file couldn’t be loaded (${response.status}).`);
      const holder = document.createElement("div"); // laid out off screen, so a second render never mixes in
      await renderAsync(await response.blob(), holder, holder, {
        className: "docx",
        inWrapper: true,
        breakPages: true,
        ignoreLastRenderedPageBreak: false, // break pages where Word last broke them
        useBase64URL: true, // pictures as data, which the window's security rules allow
        renderChanges: false,
        renderComments: false,
      });
      if (!live || !body.current) return;
      body.current.replaceChildren(...holder.childNodes);
      setPageWidth(body.current.querySelector<HTMLElement>("section.docx")?.offsetWidth ?? 0);
      setState("ready");
    })().catch(() => live && setState("failed"));
    return () => {
      live = false;
    };
  }, [props.url]);

  // open where the cited part starts, and show it briefly
  useEffect(() => {
    if (state !== "ready" || !part.data?.text || !body.current) return;
    const first = part.data.text.split("\n").find((line) => line.trim())?.split(" | ")[0] ?? "";
    const needle = plain(first).slice(0, 80);
    if (!needle) return;
    const found = [...body.current.querySelectorAll<HTMLElement>("p")].find((p) => plain(p.textContent ?? "").includes(needle));
    if (!found) return;
    found.scrollIntoView({ block: "center" });
    found.classList.add("cited");
    const done = setTimeout(() => found.classList.remove("cited"), 2400);
    return () => clearTimeout(done);
  }, [state, part.data]);

  const wheel = useCallback(
    (factor: number) => setChosen((z) => Math.min(Math.max((z ?? zoom) * factor, 0.25), 5)),
    [zoom],
  );
  useCtrlWheel(scroller, wheel);

  // links inside the document move within it; a web link would take the whole window away from Quantix
  const click = (event: MouseEvent) => {
    const link = (event.target as HTMLElement).closest("a");
    if (link && !link.getAttribute("href")?.startsWith("#")) event.preventDefault();
  };

  return (
    <>
      {props.toolbar &&
        state === "ready" &&
        createPortal(
          <ZoomControls
            scale={zoom}
            onOut={() => setChosen(zoomStep(zoom, -1))}
            onIn={() => setChosen(zoomStep(zoom, 1))}
            onFit={() => setChosen(fitted)}
            fit="Fit the page’s width"
          />,
          props.toolbar,
        )}
      <div ref={setScroller} tabIndex={0} onClick={click} className="word-view absolute inset-0 overflow-auto outline-none">
        <div ref={body} style={{ zoom }} />
      </div>
      {state !== "ready" && (
        <div className="absolute inset-0 flex bg-subtle">
          <Waiting>{state === "failed" ? "Quantix couldn’t show this Word document. Open the original." : "Opening…"}</Waiting>
        </div>
      )}
    </>
  );
}

function plain(text: string) {
  return text.replace(/\s+/g, " ").trim().toLowerCase();
}
