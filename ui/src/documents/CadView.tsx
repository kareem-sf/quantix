import { useMemo } from "react";
import { CadDrawing } from "../takeoff/CadDrawing";
import { useDrawingInfo, useScreenCopy } from "../takeoff/cad";
import { Tabs, Waiting } from "./Viewer";

const NONE: number[] = [];

/** A DWG or DXF drawing drawn from its own objects, as a CAD program shows it: model space and each layout as tabs
 * along the foot, layers that are off or frozen left out. Scroll to zoom, drag to move. */
export function CadView(props: { documentId: string; page: number; onPage: (page: number) => void }) {
  const copy = useScreenCopy(props.documentId, props.page, true);
  const info = useDrawingInfo(props.documentId, props.page);
  const hidden = useMemo(() => {
    const off = new Set(copy.data?.header.hidden ?? []);
    return new Set((copy.data?.header.layers ?? []).flatMap((name, i) => (off.has(name) ? [i] : [])));
  }, [copy.data]);

  return (
    <>
      <div className="flex min-h-0 grow p-3">
        {copy.data ? (
          <CadDrawing copy={copy.data} hiddenLayers={hidden} marked={NONE} chosen={NONE} />
        ) : (
          <Waiting>{copy.isError ? copy.error.message : "Opening the drawing…"}</Waiting>
        )}
      </div>
      <Tabs
        label="Layouts"
        tabs={(info.data?.pages ?? []).map((p) => ({ name: p.kind === "model" ? "Model" : p.name }))}
        current={props.page}
        onPick={props.onPage}
      />
    </>
  );
}
