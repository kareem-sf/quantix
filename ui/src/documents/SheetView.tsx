import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import type { components } from "../api/schema";
import { useSheet, type WorkbookSheet } from "./queries";
import { searchTerms, Tabs, termsPattern, useCtrlWheel, Waiting, ZoomControls, zoomStep } from "./Viewer";

type Style = components["schemas"]["Style"];
type Cell = components["schemas"]["Cell"];

const HEADING = 20; // the column letters' row, in pixels
const NUMBERS = 44; // the row numbers' column
const GRID = "1px solid #e1e1e3"; // Excel's gridlines
const FONT = 'Calibri, Carlito, "Segoe UI", Arial, sans-serif'; // Excel's default font, then its look-alikes

/** A workbook sheet as Excel shows it: column letters and row numbers, the sheet's widths and heights, merged cells,
 * fonts, fills, borders and number formats, frozen rows and columns, and sheets as tabs along the foot. Opened from
 * a search, the cells holding the words searched for are marked. */
export function SheetView(props: {
  documentId: string;
  page: number;
  query: string;
  toolbar: HTMLElement | null;
  onPage: (page: number) => void;
}) {
  const [hidden, setHidden] = useState(false);
  const sheet = useSheet(props.documentId, props.page, hidden);
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null);
  const [zoom, setZoom] = useState(1);
  const pattern = useMemo(() => termsPattern(searchTerms(props.query)), [props.query]);

  const wheel = useCallback((factor: number) => setZoom((z) => Math.min(Math.max(z * factor, 0.25), 4)), []);
  useCtrlWheel(scroller, wheel);
  const data = sheet.data;
  const width = data ? NUMBERS + data.columns.reduce((sum, c) => sum + c.width, 0) : 0;
  const fit = () => scroller && width && setZoom(Math.min(Math.max((scroller.clientWidth - 2) / width, 0.25), 1));

  // bring the first cell found into view
  useEffect(() => {
    scroller?.querySelector("[data-found]")?.scrollIntoView?.({ block: "center", inline: "nearest" });
  }, [scroller, data, pattern]);

  if (sheet.isError) return <Waiting>{sheet.error.message}</Waiting>;
  if (!data) return <Waiting>Opening…</Waiting>;
  const hiddenCount = data.hidden_rows + data.hidden_columns;

  return (
    <>
      {props.toolbar &&
        createPortal(
          <ZoomControls
            scale={zoom}
            onOut={() => setZoom((z) => zoomStep(z, -1))}
            onIn={() => setZoom((z) => zoomStep(z, 1))}
            onFit={fit}
            fit="Fit the sheet’s width"
          />,
          props.toolbar,
        )}
      <div ref={setScroller} tabIndex={0} className="min-h-0 grow overflow-auto bg-white outline-none">
        <Grid sheet={data} zoom={zoom} pattern={pattern} />
        {(data.more_rows > 0 || data.more_columns > 0) && (
          <p className="sticky left-0 px-4 py-3 text-ink-3">
            {data.more_rows > 0 && `${data.more_rows.toLocaleString("en-US")} more rows`}
            {data.more_rows > 0 && data.more_columns > 0 && " and "}
            {data.more_columns > 0 && `${data.more_columns.toLocaleString("en-US")} more columns`} aren’t shown here. Open
            the original to see them.
          </p>
        )}
      </div>
      <div className="flex shrink-0 items-stretch border-t border-line bg-rail">
        <div className="min-w-0 grow">
          <Tabs
            label="Sheets"
            tabs={data.sheets.map((s) => ({ name: s.hidden ? `${s.name} (hidden)` : s.name, muted: s.hidden }))}
            current={props.page}
            onPick={props.onPage}
          />
        </div>
        {(hiddenCount > 0 || hidden) && (
          <label className="flex shrink-0 cursor-pointer items-center gap-2 px-4 text-ink-2">
            <input type="checkbox" checked={hidden} onChange={(e) => setHidden(e.target.checked)} />
            Show hidden {hiddenLabel(data)}
          </label>
        )}
      </div>
    </>
  );
}

function hiddenLabel(sheet: WorkbookSheet) {
  const parts = [
    sheet.hidden_rows ? `${sheet.hidden_rows} ${sheet.hidden_rows === 1 ? "row" : "rows"}` : "",
    sheet.hidden_columns ? `${sheet.hidden_columns} ${sheet.hidden_columns === 1 ? "column" : "columns"}` : "",
  ].filter(Boolean);
  return parts.join(" and ") || "rows and columns";
}

function Grid({ sheet, zoom, pattern }: { sheet: WorkbookSheet; zoom: number; pattern: RegExp | null }) {
  const found = useMemo(() => (pattern ? new RegExp(pattern.source, "iu") : null), [pattern]);
  // each cell by where it starts, to share a border drawn on either side of an edge, as Excel does
  const at = useMemo(() => {
    const map = new Map<string, Cell>();
    sheet.rows.forEach((row, r) => row.cells.forEach((cell) => map.set(`${r}:${cell.column}`, cell)));
    return map;
  }, [sheet]);
  const left = useMemo(() => {
    const offsets = [NUMBERS];
    sheet.columns.forEach((c, i) => offsets.push(offsets[i] + c.width));
    return offsets;
  }, [sheet]);
  const top = useMemo(() => {
    const offsets = [HEADING];
    sheet.rows.forEach((row, i) => offsets.push(offsets[i] + row.height));
    return offsets;
  }, [sheet]);
  const width = left[left.length - 1];
  const grid = sheet.gridlines ? GRID : undefined;
  const heading = "sticky border-r border-b border-line-strong bg-subtle px-1 text-center text-[11px] font-normal text-ink-3";

  return (
    <table
      dir={sheet.right_to_left ? "rtl" : "ltr"}
      className="border-collapse text-ink"
      style={{ width, tableLayout: "fixed", zoom, fontFamily: FONT, fontSize: "14.667px" }}
    >
      <colgroup>
        <col style={{ width: NUMBERS }} />
        {sheet.columns.map((c) => (
          <col key={c.letter} style={{ width: c.width }} />
        ))}
      </colgroup>
      <thead>
        <tr style={{ height: HEADING }}>
          <th className={`${heading} top-0 z-40`} style={{ insetInlineStart: 0 }} />
          {sheet.columns.map((c) => (
            <th key={c.letter} className={`${heading} top-0 z-30 ${c.hidden ? "italic text-ink-4" : ""}`}>
              {c.letter}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {sheet.rows.map((row, r) => (
          <tr key={row.number} style={{ height: row.height }}>
            <th
              className={`${heading} ${r < sheet.frozen_rows ? "z-30" : "z-20"} ${row.hidden ? "italic text-ink-4" : ""}`}
              style={{ insetInlineStart: 0, top: r < sheet.frozen_rows ? top[r] : undefined }}
            >
              {row.number}
            </th>
            {row.cells.map((cell, i) => {
              const style = sheet.styles[cell.style];
              const next = row.cells[i + 1];
              const right = at.get(`${r}:${cell.column + cell.columns}`);
              const below = at.get(`${r + cell.rows}:${cell.column}`);
              const frozenRow = r < sheet.frozen_rows;
              const frozenColumn = cell.column < sheet.frozen_columns;
              const hit = Boolean(found && cell.text && found.test(cell.text));
              return (
                <td
                  key={cell.column}
                  rowSpan={cell.rows > 1 ? cell.rows : undefined}
                  colSpan={cell.columns > 1 ? cell.columns : undefined}
                  dir="auto"
                  data-found={hit || undefined}
                  className={hit ? "outline-2 -outline-offset-2 outline-attention" : undefined}
                  style={{
                    ...look(style),
                    // Excel draws an edge once, from either cell; gridlines only where no border is drawn
                    borderInlineEnd: style.right ?? (right ? sheet.styles[right.style].left : null) ?? edge(style, grid),
                    borderBottom: style.bottom ?? (below ? sheet.styles[below.style].top : null) ?? edge(style, grid),
                    borderTop: r === 0 ? (style.top ?? undefined) : undefined,
                    borderInlineStart: cell.column === 0 ? (style.left ?? undefined) : undefined,
                    // text runs on into empty cells beside it, as in Excel, and is cut short by one that isn't
                    overflow: style.wrap || (next && next.column === cell.column + cell.columns && next.text) ? "hidden" : "visible",
                    position: frozenRow || frozenColumn ? "sticky" : undefined,
                    top: frozenRow ? top[r] : undefined,
                    insetInlineStart: frozenColumn ? left[cell.column] : undefined,
                    zIndex: frozenRow || frozenColumn ? (frozenRow && frozenColumn ? 15 : 10) : undefined,
                    backgroundColor: style.fill ?? (frozenRow || frozenColumn ? "#ffffff" : undefined),
                  }}
                >
                  {cell.text}
                </td>
              );
            })}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** A cell's gridline: none under a fill, as in Excel. */
function edge(style: Style, grid: string | undefined) {
  return style.fill ? `1px solid ${style.fill}` : grid;
}

function look(style: Style): CSSProperties {
  return {
    fontWeight: style.bold ? 700 : undefined,
    fontStyle: style.italic ? "italic" : undefined,
    textDecoration: [style.underline && "underline", style.strike && "line-through"].filter(Boolean).join(" ") || undefined,
    fontFamily: style.font ? `"${style.font}", ${FONT}` : undefined,
    fontSize: style.size ? `${(style.size * 4) / 3}px` : undefined,
    color: style.color ?? undefined,
    textAlign: style.align === "end" ? "end" : ((style.align ?? undefined) as CSSProperties["textAlign"]),
    verticalAlign: style.valign ?? "bottom",
    whiteSpace: style.wrap ? "pre-wrap" : "pre",
    overflowWrap: style.wrap ? "anywhere" : undefined,
    paddingInline: 3,
    paddingInlineStart: 3 + style.indent * 9,
    lineHeight: 1.25,
  };
}
