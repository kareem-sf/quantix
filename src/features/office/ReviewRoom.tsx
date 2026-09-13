import { useCallback, useEffect, useRef, useState } from "react";
import { errorText, tenderPath, useApi, type Schema } from "../../api";

type ReviewSession = Schema<"ReviewSession">;

type ReviewFilter = "all" | "checked" | "incomplete" | "needs_review";

export function ReviewRoom({
  tenderId,
  onDecide,
}: {
  tenderId: string;
  onDecide?: () => void;
}) {
  const api = useApi();
  const identityKey = tenderId;
  const requestRef = useRef<AbortController | null>(null);
  const [filter, setFilter] = useState<ReviewFilter>("all");
  const [items, setItems] = useState<ReviewSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const load = useCallback(async () => {
    requestRef.current?.abort();
    const controller = new AbortController();
    requestRef.current = controller;
    setLoading(true);
    setError(null);
    try {
      const query = new URLSearchParams();
      if (filter !== "all") query.set("status", filter);
      const suffix = query.toString() ? `?${query.toString()}` : "";
      const next = await api.get<ReviewSession[]>(
        `${tenderPath(tenderId)}/reviews${suffix}`,
        controller.signal,
      );
      if (controller.signal.aborted) return;
      setItems(next ?? []);
    } catch (failure) {
      if (controller.signal.aborted) return;
      setError(failure);
    } finally {
      if (requestRef.current === controller) {
        requestRef.current = null;
        setLoading(false);
      }
    }
  }, [api, filter, tenderId]);

  useEffect(() => {
    void load();
    return () => {
      requestRef.current?.abort();
      requestRef.current = null;
    };
  }, [identityKey, load]);

  const selected = items.find((item) => item.id === selectedId) ?? null;

  return (
    <section className="work-section-card" aria-labelledby="review-room-title">
      <h3 id="review-room-title">Review room</h3>
      <p className="office-muted">
        Independent checks of saved work. A review never approves a quantity,
        rate, or commercial decision by itself.
      </p>
      <div
        className="office-staff-filter"
        role="group"
        aria-label="Review status filter"
      >
        {(
          ["all", "checked", "incomplete", "needs_review"] as ReviewFilter[]
        ).map((value) => (
          <button
            key={value}
            type="button"
            className={filter === value ? "button primary" : "button"}
            aria-pressed={filter === value}
            onClick={() => {
              setFilter(value);
              setSelectedId(null);
            }}
          >
            {filterLabel(value)}
          </button>
        ))}
      </div>
      {error ? (
        <div className="office-inline-error" role="alert">
          <span>{errorText(error)}</span>
          <button
            type="button"
            className="text-button"
            disabled={loading}
            onClick={() => void load()}
          >
            Retry reviews
          </button>
        </div>
      ) : null}
      {loading && !items.length ? (
        <p className="office-muted" role="status">
          Loading reviews…
        </p>
      ) : null}
      {!loading && !error && !items.length ? (
        <p className="office-muted">
          No reviews in this view yet. Start one from a saved calculation.
        </p>
      ) : null}
      {items.length ? (
        <ul className="office-review-list">
          {items.map((item) => (
            <li key={item.id}>
              <button
                type="button"
                className="office-reference-link"
                aria-pressed={selectedId === item.id}
                onClick={() =>
                  setSelectedId((current) =>
                    current === item.id ? null : item.id,
                  )
                }
              >
                {item.subject_id} · {statusLabel(item.status)}
                {item.resolution ? " · Resolved" : ""}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {selected ? (
        <ReviewDetail
          tenderId={tenderId}
          session={selected}
          onChanged={load}
          onDecide={onDecide}
        />
      ) : null}
    </section>
  );
}

function ReviewDetail({
  tenderId,
  session,
  onChanged,
  onDecide,
}: {
  tenderId: string;
  session: ReviewSession;
  onChanged: () => void;
  onDecide?: () => void;
}) {
  const api = useApi();
  const [topic, setTopic] = useState("");
  const [agreed, setAgreed] = useState(true);
  const [detail, setDetail] = useState("");
  const [resolution, setResolution] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const post = useCallback(
    async (suffix: string, body: Record<string, unknown>) => {
      setBusy(true);
      setError(null);
      try {
        await api.post(
          `${tenderPath(tenderId)}/reviews/${encodeURIComponent(session.id)}${suffix}`,
          body,
        );
        onChanged();
      } catch (failure) {
        setError(failure);
      } finally {
        setBusy(false);
      }
    },
    [api, onChanged, session.id, tenderId],
  );

  return (
    <article className="office-notebook-note" aria-label="Review detail">
      <div className="office-row-heading">
        <strong>{session.subject_id}</strong>
        <span className="office-ai-label">{statusLabel(session.status)}</span>
        {session.author_conclusion_hidden ? (
          <span className="office-ai-label">Author conclusion hidden</span>
        ) : null}
      </div>
      {session.findings.length ? (
        <ul className="office-review-list">
          {session.findings.map((finding) => (
            <li key={finding.id}>
              <strong>{finding.topic}</strong> —{" "}
              {finding.agreed ? "Agreed" : "Disagreed"}: {finding.detail}
            </li>
          ))}
        </ul>
      ) : (
        <p className="office-muted">No checkable findings recorded.</p>
      )}
      {session.limitations.length ? (
        <ul className="office-review-list">
          {session.limitations.map((limitation, index) => (
            <li key={index} className="office-muted">
              Limitation: {limitation}
            </li>
          ))}
        </ul>
      ) : null}
      <details className="office-more-options">
        <summary>More options</summary>
        <dl className="office-facts-grid">
          <Fact
            label="Calculation"
            value={session.calculation_id ?? "None supplied"}
          />
          <Fact
            label="Checked basis"
            value={session.checked_basis_fingerprint ?? "Not recorded"}
          />
        </dl>
      </details>
      {session.resolution ? (
        <p className="office-muted">
          Resolved: {session.resolution} Any commercial effect still needs its
          own engineer decision.
        </p>
      ) : (
        <>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (!topic.trim() || !detail.trim()) return;
              void post("/contributions", {
                topic: topic.trim(),
                agreed,
                detail: detail.trim(),
                idempotency_key: `contribute-${session.id}-${Date.now()}`,
              });
            }}
          >
            <label className="office-field-label">
              Discussion topic
              <input
                value={topic}
                onChange={(event) => setTopic(event.target.value)}
                placeholder="Measured area"
              />
            </label>
            <label className="office-field-label">
              Finding detail
              <textarea
                value={detail}
                onChange={(event) => setDetail(event.target.value)}
                rows={2}
              />
            </label>
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={agreed}
                onChange={(event) => setAgreed(event.target.checked)}
              />
              Agreed
            </label>
            <button type="submit" className="button" disabled={busy}>
              Add finding
            </button>
          </form>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              if (!resolution.trim()) return;
              void post("/resolve", {
                resolution: resolution.trim(),
                idempotency_key: `resolve-${session.id}-${Date.now()}`,
              });
            }}
          >
            <label className="office-field-label">
              Resolution
              <textarea
                value={resolution}
                onChange={(event) => setResolution(event.target.value)}
                rows={2}
                placeholder="What was decided and what still needs an engineer decision"
              />
            </label>
            <button type="submit" className="button primary" disabled={busy}>
              Resolve review
            </button>
          </form>
        </>
      )}
      {onDecide ? (
        <button type="button" className="text-button" onClick={onDecide}>
          Record engineer decision
        </button>
      ) : null}
      {error ? (
        <div className="office-inline-error" role="alert">
          <span>{errorText(error)}</span>
        </div>
      ) : null}
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

function filterLabel(filter: ReviewFilter) {
  if (filter === "all") return "All";
  return statusLabel(filter);
}

function statusLabel(status: string) {
  if (status === "checked") return "Checked";
  if (status === "incomplete") return "Incomplete";
  if (status === "needs_review") return "Needs review";
  return status;
}
