import { useState } from "react";
import { tenderPath, useApi, type Schema } from "../api";
import { ErrorNotice } from "../components/common";

export function Watchers({ tenderId }: { tenderId: string }) {
  const api = useApi();
  const [error, setError] = useState<unknown>(null);
  const [created, setCreated] = useState<Schema<"WatchSpec"> | null>(null);
  async function addWatch() {
    setError(null);
    try {
      const spec = await api.post<Schema<"WatchSpec">>(
        `${tenderPath(tenderId)}/watches`,
        {
          scope: "this Tender",
          trigger: "addendum",
          budget: 10,
          idempotency_key: `watch-${Date.now()}`,
        },
      );
      setCreated(spec);
    } catch (failure) {
      setError(failure);
    }
  }
  return (
    <section className="watchers" aria-labelledby="watchers-heading">
      <h2 id="watchers-heading">Watches</h2>
      <p className="muted">
        A watch only notifies you. It cannot send a quotation or release a bid.
      </p>
      <ErrorNotice error={error} />
      <button type="button" className="button primary" onClick={addWatch}>
        Watch for addenda
      </button>
      {created ? (
        <p>Watch saved. Next: activate it after you review the displayed actions.</p>
      ) : null}
    </section>
  );
}
