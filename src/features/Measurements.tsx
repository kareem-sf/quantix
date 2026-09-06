import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Ruler } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading } from "../components/ui";
import { MeasurementCanvas, type MeasurementPoint } from "./MeasurementCanvas";
import "../styles/measurements.css";

type Measurement = Schema<"MeasurementRecord">;
type Calculation = Schema<"MeasurementCalculation">;
type Mode = Schema<"MeasurementInput">["mode"];
const labels: Record<Mode, string> = {
  length: "Length",
  area: "Area",
  count: "Count",
};
const units: Record<string, string[]> = {
  m: ["m", "lm", "rm", "م"],
  m2: ["m2", "m²", "sqm", "م2", "م²"],
  nr: ["nr", "no", "nos", "ea", "each", "عدد"],
};

export function Measurements(props: {
  tenderId: string;
  artifactId: string;
  onClose?: () => void;
}) {
  return (
    <MeasurementPanel
      key={`${props.tenderId}:${props.artifactId}`}
      {...props}
    />
  );
}

function MeasurementPanel({
  tenderId,
  artifactId,
  onClose,
}: {
  tenderId: string;
  artifactId: string;
  onClose?: () => void;
}) {
  const api = useApi(),
    base = tenderPath(tenderId);
  const [page, setPage] = useState(1),
    [offset, setOffset] = useState(0);
  const [selected, setSelected] = useState<Measurement | null>(null),
    [saving, setSaving] = useState(false);
  const records = useResource<Measurement[]>(
    `${base}/measurements?artifact_id=${encodeURIComponent(artifactId)}&offset=${offset}&limit=20`,
  );
  const loaded = useQuery({
    queryKey: [base, artifactId, "measurement-page-image", page],
    queryFn: async () => {
      const path = `${base}/artifacts/${encodeURIComponent(artifactId)}`;
      const [source, blob] = await Promise.all([
        api.get<Schema<"MeasurementPage">>(
          `${path}/measurement-page?page=${page}`,
        ),
        api.blob(`${path}/preview?page=${page}`),
      ]);
      return { source, blob };
    },
    retry: false,
  });
  const [image, setImage] = useState<{ sourceKey: string; url: string } | null>(
    null,
  );
  const sourceKey = loaded.data
    ? `${loaded.data.source.content_hash}:${loaded.data.source.page}`
    : null;
  useEffect(() => {
    if (!loaded.data) return;
    const { blob } = loaded.data,
      url = URL.createObjectURL(blob);
    setImage({
      sourceKey: `${loaded.data.source.content_hash}:${loaded.data.source.page}`,
      url,
    });
    return () => URL.revokeObjectURL(url);
  }, [loaded.data]);
  const url = sourceKey && image?.sourceKey === sourceKey ? image.url : null;
  return (
    <section className="measurements" aria-labelledby="measurements-heading">
      <div className="measurement-heading">
        <h3 id="measurements-heading">
          <Ruler size={19} /> Drawing measurements
        </h3>
        {onClose ? (
          <button className="button" onClick={onClose} disabled={saving}>
            Back to source
          </button>
        ) : null}
      </div>
      <p className="muted">
        Mark a defined scope and calibrate against a written dimension. Saved
        measurements remain proposals for engineer review before use in the BOQ.
      </p>
      <div className="measurement-pages">
        <button
          className="icon-button"
          aria-label="Previous drawing page"
          disabled={saving || page <= 1}
          onClick={() => {
            setSelected(null);
            setPage(page - 1);
          }}
        >
          <ChevronLeft size={19} />
        </button>
        <span>
          Page {page}
          {loaded.data ? ` of ${loaded.data.source.page_count}` : ""}
        </span>
        <button
          className="icon-button"
          aria-label="Next drawing page"
          disabled={
            saving || !loaded.data || page >= loaded.data.source.page_count
          }
          onClick={() => {
            setSelected(null);
            setPage(page + 1);
          }}
        >
          <ChevronRight size={19} />
        </button>
      </div>
      <ErrorNotice error={loaded.error} />
      {loaded.isPending ? <Loading>Opening the drawing page…</Loading> : null}
      {loaded.data && url ? (
        <>
          <p className="measurement-source">
            {loaded.data.source.relative_path} · Version{" "}
            {loaded.data.source.version} ·{" "}
            {loaded.data.source.is_current
              ? "Current source"
              : "Older source revision"}
          </p>
          {selected ? (
            <>
              <MeasurementCanvas
                key={`${page}:${selected.id}`}
                source={loaded.data.source}
                url={url}
                points={selected.points as MeasurementPoint[]}
                calibration={
                  (selected.calibration_points ?? []) as MeasurementPoint[]
                }
                mode={selected.mode}
              />
              <SavedMeasurement tenderId={tenderId} measurement={selected} />
              <button className="button" onClick={() => setSelected(null)}>
                New measurement on this page
              </button>
            </>
          ) : loaded.data.source.is_current ? (
            <MeasurementEditor
              key={`${artifactId}:${page}`}
              tenderId={tenderId}
              source={loaded.data.source}
              url={url}
              onSaving={setSaving}
              onSaved={setSelected}
            />
          ) : (
            <>
              <MeasurementCanvas
                source={loaded.data.source}
                url={url}
                points={[]}
                calibration={[]}
                mode="count"
              />
              <p className="measurement-caution">
                Open the current drawing version to create a new measurement.
                Older proposals remain available below.
              </p>
            </>
          )}
        </>
      ) : null}
      <div className="measurement-history">
        <h4>Saved proposals</h4>
        <ErrorNotice error={records.error} />
        {records.isPending ? <Loading>Loading measurements…</Loading> : null}
        {records.data?.length === 0 ? (
          <p className="muted">No saved measurements for this source.</p>
        ) : null}
        {records.data?.map((record) => (
          <button
            key={record.id}
            className="measurement-record"
            disabled={saving}
            onClick={() => {
              setPage(record.source.page);
              setSelected(record);
            }}
          >
            <span>
              <strong>{record.scope_label}</strong>
              <small>
                {record.origin === "agent" ? "Agent proposal" : "Engineer measurement"} · Page {record.source.page} · {labels[record.mode]} ·{" "}
                {record.is_current
                  ? "Current source"
                  : "Source needs recheck"}
              </small>
            </span>
            <strong>
              {record.quantity} {record.unit}
            </strong>
          </button>
        ))}
        <div className="inline-actions">
          <button
            className="button"
            disabled={saving || offset === 0}
            onClick={() => setOffset(Math.max(0, offset - 20))}
          >
            Previous proposals
          </button>
          <button
            className="button"
            disabled={saving || (records.data?.length ?? 0) < 20}
            onClick={() => setOffset(offset + 20)}
          >
            More proposals
          </button>
        </div>
      </div>
    </section>
  );
}

function MeasurementEditor({
  tenderId,
  source,
  url,
  onSaved,
  onSaving,
}: {
  tenderId: string;
  source: Schema<"MeasurementPage">;
  url: string;
  onSaved: (record: Measurement) => void;
  onSaving: (saving: boolean) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [mode, setMode] = useState<Mode>("length"),
    [tool, setTool] = useState<"calibration" | "shape">("calibration");
  const [points, setPoints] = useState<MeasurementPoint[]>([]),
    [calibration, setCalibration] = useState<MeasurementPoint[]>([]);
  const [metres, setMetres] = useState(""),
    [scope, setScope] = useState(""),
    [rationale, setRationale] = useState(""),
    [consent, setConsent] = useState(false);
  const [result, setResult] = useState<Calculation | null>(null),
    [error, setError] = useState<unknown>(null),
    [busy, setBusy] = useState<"calculate" | "save" | null>(null);
  const requestSequence = useRef(0);
  useEffect(
    () => () => {
      requestSequence.current += 1;
    },
    [],
  );
  function changed() {
    requestSequence.current += 1;
    setResult(null);
    setConsent(false);
    setError(null);
  }
  function addPoint(point: MeasurementPoint) {
    changed();
    if (tool === "calibration" && mode !== "count")
      setCalibration((previous) =>
        previous.length >= 2 ? [point] : [...previous, point],
      );
    else
      setPoints((previous) =>
        previous.length < 500 ? [...previous, point] : previous,
      );
  }
  function reset() {
    changed();
    setPoints([]);
    setCalibration([]);
    setMetres("");
    setScope("");
    setRationale("");
    setBusy(null);
    setTool(mode === "count" ? "shape" : "calibration");
  }
  const input: Schema<"MeasurementInput"> = {
    artifact_id: source.artifact_id,
    page: source.page,
    mode,
    points,
    calibration_points: mode === "count" ? null : calibration,
    calibration_metres: mode === "count" ? null : metres,
  };
  const enough =
    points.length >= (mode === "count" ? 1 : mode === "area" ? 3 : 2) &&
    (mode === "count" || (calibration.length === 2 && Number(metres) > 0));
  async function calculate() {
    const sequence = ++requestSequence.current;
    setError(null);
    setResult(null);
    setConsent(false);
    setBusy("calculate");
    try {
      const next = await api.post<Calculation>(
        `${tenderPath(tenderId)}/measurements/calculate`,
        input,
      );
      if (sequence === requestSequence.current) setResult(next);
    } catch (caught) {
      if (sequence === requestSequence.current) setError(caught);
    } finally {
      if (sequence === requestSequence.current) setBusy(null);
    }
  }
  async function save() {
    if (!result || !consent || !scope.trim() || !rationale.trim()) return;
    setError(null);
    setBusy("save");
    onSaving(true);
    try {
      const saved = await api.post<Measurement>(
        `${tenderPath(tenderId)}/measurements`,
        {
          ...input,
          scope_label: scope,
          engineer_confirmed: true,
          rationale,
        } satisfies Schema<"MeasurementCreate">,
      );
      void refresh();
      onSaved(saved);
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(null);
      onSaving(false);
    }
  }
  return (
    <div className="measurement-editor">
      <fieldset disabled={!!busy} className="measurement-tools">
        <legend>1. Calibrate and mark</legend>
        <label>
          Measurement type
          <select
            value={mode}
            onChange={(event) => {
              const next = event.target.value as Mode;
              changed();
              setMode(next);
              setPoints([]);
              setTool(next === "count" ? "shape" : "calibration");
            }}
          >
            {Object.entries(labels).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
        {mode !== "count" ? (
          <label>
            Known length (m)
            <input
              type="number"
              min="0.000001"
              max="1000000"
              step="any"
              value={metres}
              onChange={(event) => {
                changed();
                setMetres(event.target.value);
              }}
            />
          </label>
        ) : null}
        <div className="inline-actions">
          {mode !== "count" ? (
            <button
              className={`button ${tool === "calibration" ? "measurement-active" : ""}`}
              aria-pressed={tool === "calibration"}
              onClick={() => setTool("calibration")}
            >
              Set calibration points
            </button>
          ) : null}
          <button
            className={`button ${tool === "shape" ? "measurement-active" : ""}`}
            aria-pressed={tool === "shape"}
            onClick={() => setTool("shape")}
          >
            Mark measurement
          </button>
          <button
            className="button"
            disabled={
              (tool === "calibration" ? calibration : points).length === 0
            }
            onClick={() => {
              changed();
              if (tool === "calibration")
                setCalibration(calibration.slice(0, -1));
              else setPoints(points.slice(0, -1));
            }}
          >
            Undo point
          </button>
        </div>
      </fieldset>
      <p className="measurement-help" aria-live="polite">
        {tool === "calibration" && mode !== "count"
          ? `Click the two ends of a written dimension. ${calibration.length} of 2 calibration points marked.`
          : mode === "area"
            ? "Click the boundary corners in order. The last point joins the first; do not repeat the first point."
            : mode === "count"
              ? "Click each object you have identified. Each mark counts once."
              : "Click the ends and bends of the length to measure."}{" "}
        {points.length} measurement marks.
      </p>
      <MeasurementCanvas
        source={source}
        url={url}
        points={points}
        calibration={mode === "count" ? [] : calibration}
        mode={mode}
        onPoint={addPoint}
        disabled={!!busy}
      />
      <CoordinateEntry onPoint={addPoint} disabled={!!busy} />
      <div className="inline-actions">
        <button
          className="button primary"
          disabled={!!busy || !enough}
          onClick={() => void calculate()}
        >
          {busy === "calculate" ? "Calculating…" : "Calculate quantity"}
        </button>
        <button className="button" disabled={busy === "save"} onClick={reset}>
          Cancel measurement
        </button>
      </div>
      <ErrorNotice error={error} />
      {result ? (
        <div className="measurement-review">
          <h4>2. Review the proposal</h4>
          <strong className="measurement-quantity">
            {result.quantity} {result.unit}
          </strong>
          <p>{result.calculation}</p>
          <p className="muted">{result.precision_note}</p>
          <fieldset disabled={!!busy} className="measurement-review-fields">
            <label>
              Measurement scope
              <input
                value={scope}
                maxLength={300}
                placeholder="e.g. East boundary, between grids A and D"
                onChange={(event) => setScope(event.target.value)}
              />
            </label>
            <label>
              Review note
              <textarea
                value={rationale}
                maxLength={4000}
                onChange={(event) => setRationale(event.target.value)}
              />
            </label>
            <label className="measurement-consent">
              <input
                type="checkbox"
                checked={consent}
                onChange={(event) => setConsent(event.target.checked)}
              />
              I reviewed the calibration, marked scope and written dimensions.
              Save as a quantity proposal.
            </label>
            <button
              className="button primary"
              disabled={!consent || !scope.trim() || !rationale.trim()}
              onClick={() => void save()}
            >
              {busy === "save" ? "Saving…" : "Save reviewed proposal"}
            </button>
          </fieldset>
        </div>
      ) : null}
    </div>
  );
}

function CoordinateEntry({
  onPoint,
  disabled,
}: {
  onPoint: (point: MeasurementPoint) => void;
  disabled: boolean;
}) {
  const [x, setX] = useState(""),
    [y, setY] = useState("");
  const valid =
    x !== "" &&
    y !== "" &&
    [Number(x), Number(y)].every(
      (value) => Number.isFinite(value) && value >= 0 && value <= 100,
    );
  return (
    <details className="measurement-coordinates">
      <summary>Enter a point using page percentages</summary>
      <fieldset disabled={disabled}>
        <label>
          From left (%)
          <input
            type="number"
            min={0}
            max={100}
            step="any"
            value={x}
            onChange={(event) => setX(event.target.value)}
          />
        </label>
        <label>
          From top (%)
          <input
            type="number"
            min={0}
            max={100}
            step="any"
            value={y}
            onChange={(event) => setY(event.target.value)}
          />
        </label>
        <button
          className="button"
          disabled={!valid}
          onClick={() => {
            if (valid) onPoint([Number(x) / 100, Number(y) / 100]);
          }}
        >
          Add point
        </button>
      </fieldset>
    </details>
  );
}

function SavedMeasurement({
  tenderId,
  measurement,
}: {
  tenderId: string;
  measurement: Measurement;
}) {
  return (
    <div className="measurement-review">
      <h4>Saved measurement proposal</h4>
      <strong className="measurement-quantity">
        {measurement.quantity} {measurement.unit}
      </strong>
      <p>
        <strong>{measurement.scope_label}</strong>
      </p>
      {measurement.origin === "agent" ? (
        <p className="measurement-caution">
          Agent-proposed geometry. {measurement.links.length
            ? "An engineer review was recorded for the BOQ link below. Quantity approval remains separate."
            : "The engineer has not reviewed this proposal. Inspect the drawing, supporting dimensions and marked scope before creating a BOQ link."}
        </p>
      ) : null}
      <p>{measurement.calculation}</p>
      <p className="muted">{measurement.precision_note}</p>
      <p>
        {measurement.is_current
          ? "Current drawing source."
          : "The drawing or measurement evidence changed or is unavailable. Review the current source before use."}
      </p>
      <details>
        <summary>Source, calibration and review</summary>
        <dl className="measurement-basis">
          <dt>Source</dt>
          <dd>
            {measurement.source.relative_path} · v{measurement.source.version} ·
            page {measurement.source.page}
          </dd>
          <dt>Source SHA-256</dt>
          <dd>{measurement.source.content_hash}</dd>
          <dt>Calibration</dt>
          <dd>
            {measurement.calibration_metres === null
              ? "Counted marks; no length scale."
              : `${measurement.calibration_metres} m; points ${JSON.stringify(measurement.calibration_points)}`}
          </dd>
          <dt>Marked points</dt>
          <dd>{JSON.stringify(measurement.points)}</dd>
          <dt>Coordinate reference</dt>
          <dd>
            Fractions from the top-left; page{" "}
            {measurement.source.page_size.join(" × ")} PDF units.
          </dd>
          <dt>Engineer review</dt>
          <dd>
            {measurement.reviewed_at
              ? <>{measurement.review_rationale} · {measurement.reviewed_at}</>
              : measurement.links.length
                ? measurement.links.map(link => <p key={link.proposal_id}>{link.rationale} · {link.created_at}</p>)
                : "No engineer review recorded."}
          </dd>
          <dt>Proposal origin</dt>
          <dd>{measurement.origin === "agent" ? "Tender Office agent" : "Engineer"}{measurement.run_id ? ` · run ${measurement.run_id}` : ""}</dd>
          {(measurement.supporting_sources ?? []).length ? <>
            <dt>Supporting references</dt>
            <dd>{measurement.supporting_sources?.map(source => <p key={source.source_id}>{source.relative_path} · {source.locator} · version {source.version}</p>)}</dd>
          </> : null}
          <dt>Calculation</dt>
          <dd>{measurement.calculation_version}</dd>
        </dl>
      </details>
      {measurement.is_current ? (
        <MeasurementBoqLink
          key={measurement.id}
          tenderId={tenderId}
          measurement={measurement}
        />
      ) : null}
    </div>
  );
}

function MeasurementBoqLink({
  tenderId,
  measurement,
}: {
  tenderId: string;
  measurement: Measurement;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const estimate = useResource<Schema<"EstimateView">>(
    `${tenderPath(tenderId)}/estimate`,
  );
  const [itemId, setItemId] = useState(""),
    [rationale, setRationale] = useState(""),
    [consent, setConsent] = useState(false),
    [busy, setBusy] = useState(false),
    [linked, setLinked] = useState(false),
    [error, setError] = useState<unknown>(null);
  const compatible =
    estimate.data?.items.filter((item) =>
      units[measurement.unit].includes(item.unit.trim().toLowerCase()),
    ) ?? [];
  async function link() {
    setBusy(true);
    setError(null);
    try {
      await api.post<Schema<"QuantityProposal">>(
        `${tenderPath(tenderId)}/measurements/${measurement.id}/link`,
        {
          item_id: itemId,
          engineer_confirmed: true,
          rationale,
        } satisfies Schema<"MeasurementLink">,
      );
      setLinked(true);
      void refresh();
    } catch (caught) {
      setError(caught);
    } finally {
      setBusy(false);
    }
  }
  return (
    <details className="measurement-link">
      <summary>Propose this quantity for a BOQ item</summary>
      <p className="muted">
        Linking creates a separate quantity proposal. Review and approve it in
        Estimate before it changes the effective BOQ quantity.
      </p>
      <ErrorNotice error={estimate.error || error} />
      {linked ? (
        <p role="status">
          BOQ quantity proposal created. Open Estimate to review and approve it.
        </p>
      ) : (
        <fieldset disabled={busy}>
          <label>
            Compatible BOQ item
            <select
              value={itemId}
              onChange={(event) => {
                setItemId(event.target.value);
                setConsent(false);
              }}
            >
              <option value="">Choose an item</option>
              {compatible.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.description} · {item.unit} · {item.locator}
                </option>
              ))}
            </select>
          </label>
          {estimate.data && !compatible.length ? (
            <p className="muted">
              No current BOQ items have a compatible unit ({measurement.unit}).
            </p>
          ) : null}
          <label>
            Link rationale
            <textarea
              value={rationale}
              maxLength={4000}
              onChange={(event) => setRationale(event.target.value)}
            />
          </label>
          <label className="measurement-consent">
            <input
              type="checkbox"
              checked={consent}
              onChange={(event) => setConsent(event.target.checked)}
            />
            I checked that this measurement matches the selected BOQ item and
            unit. I also reviewed the drawing, calibration or count marks,
            supporting dimensions and measured scope.
          </label>
          <button
            className="button"
            disabled={!itemId || !rationale.trim() || !consent}
            onClick={() => void link()}
          >
            {busy ? "Creating proposal…" : "Create BOQ quantity proposal"}
          </button>
        </fieldset>
      )}
      {measurement.links.length ? (
        <p className="muted">
          Previously linked to {measurement.links.length} BOQ item(s).
        </p>
      ) : null}
    </details>
  );
}
