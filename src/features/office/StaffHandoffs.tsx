import { useCallback, useEffect, useRef, useState } from "react";
import { errorText, tenderPath, useApi, type Schema } from "../../api";

type HandoffView = Schema<"HandoffView">;
type HandoffPage = Schema<"HandoffPage">;
type HandoffPayload = Schema<"HandoffPayload">;

const PAGE_SIZE = 50;
const ROW_PAGE_SIZE = 50;

type HandoffDirection = "received" | "sent";

export function StaffHandoffs({
  tenderId,
  staffId,
  refreshKey,
}: {
  tenderId: string;
  staffId: string;
  refreshKey?: number;
}) {
  const api = useApi();
  const identityKey = `${tenderId}\u0000${staffId}`;
  const requestControllerRef = useRef<AbortController | null>(null);
  const generationRef = useRef(0);
  const [direction, setDirection] = useState<HandoffDirection>("received");
  const [items, setItems] = useState<HandoffView[]>([]);
  const [total, setTotal] = useState(0);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [retryCursor, setRetryCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);

  const loadPage = useCallback(
    async (
      cursor: string | null,
      append: boolean,
      activeDirection: HandoffDirection,
    ) => {
      requestControllerRef.current?.abort();
      const controller = new AbortController();
      requestControllerRef.current = controller;
      const generation = generationRef.current;
      const requestIdentity = identityKey;
      const query = new URLSearchParams();
      query.set("staff_id", staffId);
      query.set("direction", activeDirection);
      query.set("limit", String(PAGE_SIZE));
      if (cursor) query.set("cursor", cursor);
      setLoading(true);
      setError(null);
      try {
        const page = await api.get<HandoffPage>(
          `${tenderPath(tenderId)}/handoffs?${query.toString()}`,
          controller.signal,
        );
        if (
          controller.signal.aborted ||
          generation !== generationRef.current ||
          requestIdentity !== identityKey
        )
          return;
        setItems((current) =>
          append ? [...current, ...(page.items ?? [])] : (page.items ?? []),
        );
        setTotal(page.total ?? 0);
        setNextCursor(page.next_cursor ?? null);
        setRetryCursor(cursor);
      } catch (failure) {
        if (
          controller.signal.aborted ||
          generation !== generationRef.current ||
          requestIdentity !== identityKey
        )
          return;
        setRetryCursor(cursor);
        setError(failure);
      } finally {
        if (
          !controller.signal.aborted &&
          generation === generationRef.current &&
          requestIdentity === identityKey
        ) {
          setLoading(false);
        }
        if (requestControllerRef.current === controller) {
          requestControllerRef.current = null;
        }
      }
    },
    [api, identityKey, staffId, tenderId],
  );

  useEffect(() => {
    generationRef.current += 1;
    requestControllerRef.current?.abort();
    requestControllerRef.current = null;
    setItems([]);
    setTotal(0);
    setNextCursor(null);
    setRetryCursor(null);
    setError(null);
    setLoading(true);
    void loadPage(null, false, direction);
    return () => {
      generationRef.current += 1;
      requestControllerRef.current?.abort();
      requestControllerRef.current = null;
    };
  }, [direction, identityKey, loadPage, refreshKey]);

  return (
    <section
      className="office-desk-section"
      aria-labelledby="staff-handoffs-title"
    >
      <h3 id="staff-handoffs-title">Handoffs</h3>
      <p className="office-muted">
        Complete staff-result tables handed to or from this colleague, with
        exact values.
        {total
          ? ` ${total} ${total === 1 ? "handoff" : "handoffs"} in this view.`
          : ""}
      </p>
      <div
        className="office-staff-filter"
        role="group"
        aria-label="Handoff direction"
      >
        <button
          type="button"
          className={direction === "received" ? "button primary" : "button"}
          aria-pressed={direction === "received"}
          onClick={() => setDirection("received")}
        >
          Received
        </button>
        <button
          type="button"
          className={direction === "sent" ? "button primary" : "button"}
          aria-pressed={direction === "sent"}
          onClick={() => setDirection("sent")}
        >
          Sent
        </button>
      </div>
      {error ? (
        <div className="office-inline-error" role="alert">
          <span>{errorText(error)}</span>
          <button
            type="button"
            className="text-button"
            disabled={loading}
            onClick={() =>
              void loadPage(retryCursor, Boolean(retryCursor), direction)
            }
          >
            Retry handoffs
          </button>
        </div>
      ) : null}
      {loading && !items.length ? (
        <p className="office-muted office-inline-loading" role="status">
          Loading handoffs…
        </p>
      ) : null}
      {!loading && !error && !items.length ? (
        <p className="office-muted">
          {direction === "received"
            ? "No handoffs received by this colleague yet."
            : "This colleague has not handed work to anyone yet."}
        </p>
      ) : null}
      {items.length ? (
        <div className="office-notebook-list">
          {items.map((item) => (
            <HandoffCard
              key={item.handoff.id}
              view={item}
              tenderId={tenderId}
            />
          ))}
        </div>
      ) : null}
      {nextCursor ? (
        <button
          type="button"
          className="office-history-button"
          disabled={loading}
          onClick={() => void loadPage(nextCursor, true, direction)}
        >
          {loading ? "Loading older handoffs…" : "Load older handoffs"}
        </button>
      ) : null}
    </section>
  );
}

function HandoffCard({
  view,
  tenderId,
}: {
  view: HandoffView;
  tenderId: string;
}) {
  const api = useApi();
  const { handoff } = view;
  const [payload, setPayload] = useState<HandoffPayload | null>(null);
  const [offset, setOffset] = useState(0);
  const [loadingRows, setLoadingRows] = useState(false);
  const [rowsError, setRowsError] = useState<unknown>(null);
  const rowsRequestRef = useRef<AbortController | null>(null);

  const loadRows = useCallback(
    async (nextOffset: number) => {
      rowsRequestRef.current?.abort();
      const controller = new AbortController();
      rowsRequestRef.current = controller;
      setLoadingRows(true);
      setRowsError(null);
      try {
        const query = new URLSearchParams({
          offset: String(nextOffset),
          limit: String(ROW_PAGE_SIZE),
        });
        const next = await api.get<HandoffPayload>(
          `${tenderPath(tenderId)}/handoffs/${encodeURIComponent(handoff.id)}?${query.toString()}`,
          controller.signal,
        );
        if (controller.signal.aborted) return;
        setPayload(next);
        setOffset(nextOffset);
      } catch (failure) {
        if (controller.signal.aborted) return;
        setRowsError(failure);
      } finally {
        if (rowsRequestRef.current === controller) {
          rowsRequestRef.current = null;
          setLoadingRows(false);
        }
      }
    },
    [api, handoff.id, tenderId],
  );

  useEffect(
    () => () => {
      rowsRequestRef.current?.abort();
      rowsRequestRef.current = null;
    },
    [],
  );

  const totalRows = payload?.total_rows ?? null;
  const lastOffset =
    totalRows === null ? null : Math.max(0, totalRows - ROW_PAGE_SIZE);
  const rows = payload?.items ?? [];

  return (
    <article className="office-notebook-note">
      <div className="office-row-heading">
        <strong>
          {view.direction === "received" ? "From " : "To "}
          {view.counterpart_display_name}
        </strong>
        {handoff.applicability === "current" ? (
          <span className="office-ai-label">Current basis</span>
        ) : (
          <span className="office-ai-label">
            Needs review — source basis changed
          </span>
        )}
      </div>
      <p>{handoff.purpose}</p>
      {!payload ? (
        <button
          type="button"
          className="text-button"
          disabled={loadingRows}
          onClick={() => void loadRows(0)}
        >
          {loadingRows ? "Loading exact rows…" : "Show exact rows"}
        </button>
      ) : null}
      {rowsError ? (
        <div className="office-inline-error" role="alert">
          <span>{errorText(rowsError)}</span>
          <button
            type="button"
            className="text-button"
            disabled={loadingRows}
            onClick={() => void loadRows(offset)}
          >
            Retry exact rows
          </button>
        </div>
      ) : null}
      {payload ? (
        <>
          <p className="office-muted">
            Rows {offset + 1}–{offset + rows.length} of {payload.total_rows}.
            Values are exact as handed over.
          </p>
          <dl className="office-handoff-rows">
            {rows.map((row, index) => (
              <div key={`${offset + index}`}>
                <dt>Row {offset + index + 1}</dt>
                <dd>
                  {Object.entries(row).map(([key, value]) => (
                    <span key={key} className="office-handoff-cell">
                      <span className="office-muted">{key}: </span>
                      {typeof value === "string"
                        ? value
                        : JSON.stringify(value)}
                    </span>
                  ))}
                </dd>
              </div>
            ))}
          </dl>
          <div className="office-staff-filter">
            <button
              type="button"
              className="text-button"
              disabled={loadingRows || offset === 0}
              onClick={() => void loadRows(Math.max(0, offset - ROW_PAGE_SIZE))}
            >
              Previous rows
            </button>
            <button
              type="button"
              className="text-button"
              disabled={
                loadingRows ||
                payload.next_offset === null ||
                payload.next_offset === undefined
              }
              onClick={() => void loadRows(payload.next_offset ?? offset)}
            >
              Next rows
            </button>
            {lastOffset !== null && lastOffset !== offset ? (
              <button
                type="button"
                className="text-button"
                disabled={loadingRows}
                onClick={() => void loadRows(lastOffset)}
              >
                Last rows
              </button>
            ) : null}
          </div>
        </>
      ) : null}
      <details className="office-more-options">
        <summary>More options</summary>
        <dl className="office-facts-grid">
          <Fact label="Result record" value={handoff.result_id} />
          <Fact label="Basis fingerprint" value={handoff.basis_fingerprint} />
          <Fact label="Applicability" value={handoff.applicability} />
          <Fact label="Handed over" value={formatDate(handoff.created_at)} />
        </dl>
      </details>
    </article>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value || "Not recorded."}</dd>
    </div>
  );
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}
