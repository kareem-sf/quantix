import { useCallback, useState } from "react";
import { RefreshCw } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { Empty, ErrorNotice, Loading, Status } from "../components/ui";
import { Citations, type SourceSelection } from "./Sources";
import { EstimateEditor } from "./EstimateEditor";
import { Outputs } from "./Outputs";
import { RateProposals } from "./RateProposals";

export function Estimate({
  tenderId,
  defaultCurrency,
  onSource,
  outputsAvailable = false,
}: {
  tenderId: string;
  defaultCurrency: string;
  onSource: (source: SourceSelection) => void;
  outputsAvailable?: boolean;
}) {
  const api = useApi(),
    refresh = useRefresh(),
    base = tenderPath(tenderId);
  const estimate = useResource<Schema<"EstimateView">>(`${base}/estimate`);
  const [refreshing, setRefreshing] = useState(false),
    [selected, setSelected] = useState<string | null>(null),
    [query, setQuery] = useState(""),
    [filter, setFilter] = useState("all");
  const [page, setPage] = useState(0);
  const [refreshError, setRefreshError] = useState<unknown>(null);
  const closeEditor = useCallback(() => setSelected(null), []);
  if (estimate.isPending) return <Loading>Loading estimate…</Loading>;
  if (!estimate.data) return <ErrorNotice error={estimate.error} />;
  const view = estimate.data;
  const visible = view.items.filter(
    (item) =>
      item.description.toLowerCase().includes(query.toLowerCase()) &&
      (filter === "all" ||
        (filter === "unconfirmed" && !item.confirmed) ||
        (filter === "unpriced" && item.unit_rate === null) ||
        (filter === "measured" &&
          item.quantity_basis === "approved_measurement")),
  );
  const selectedItem = view.items.find((item) => item.id === selected);
  return (
    <div className="feature-page estimate-page">
      <div className="section-heading">
        <div>
          <h2>Estimate</h2>
          <p className="muted">
            Supplied quantities, checked rates and recorded assumptions.
          </p>
        </div>
        <button
          className="button"
          disabled={refreshing}
          onClick={async () => {
            setRefreshing(true);
            setRefreshError(null);
            try {
              await api.post<Schema<"EstimateView">>(
                `${base}/estimate/refresh`,
              );
              await refresh();
            } catch (error) {
              setRefreshError(error);
            } finally {
              setRefreshing(false);
            }
          }}
        >
          <RefreshCw size={17} />
          {refreshing ? "Refreshing…" : "Refresh source rows"}
        </button>
      </div>
      <ErrorNotice error={estimate.error || refreshError} />
      <div className={`estimate-state ${view.complete ? "complete" : ""}`}>
        <strong>
          {view.complete ? "Pricing complete" : "Estimate needs review"}
        </strong>
        <p>{view.coverage_note}</p>
        {view.refresh_required ? (
          <p>
            Source documents have changed. Refresh the BOQ rows before
            continuing.
          </p>
        ) : null}
        {view.blocking_reasons.length ? (
          <ul>
            {view.blocking_reasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        ) : null}
      </div>
      {view.totals.length ? (
        <div className="estimate-totals">
          {view.totals.map((total) => (
            <section key={total.currency}>
              <h3>{total.currency}</h3>
              <dl>
                <dt>Priced subtotal excluding VAT</dt>
                <dd>{total.priced_subtotal_ex_vat}</dd>
                <dt>Complete total excluding VAT</dt>
                <dd>{total.total_ex_vat ?? "Not established"}</dd>
                <dt>Complete total including VAT</dt>
                <dd>{total.total_inc_vat ?? "Not established"}</dd>
              </dl>
            </section>
          ))}
        </div>
      ) : null}
      {view.items.length ? (
        <>
          <div className="file-filters">
            <input
              className="estimate-search"
              aria-label="Search estimate descriptions"
              placeholder="Search descriptions…"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setPage(0);
              }}
            />
            <select
              aria-label="Filter estimate rows"
              value={filter}
              onChange={(event) => {
                setFilter(event.target.value);
                setPage(0);
              }}
            >
              <option value="all">All rows</option>
              <option value="unconfirmed">Source not confirmed</option>
              <option value="unpriced">Without a rate</option>
              <option value="measured">Measured quantity</option>
            </select>
          </div>
          <div className="table-scroll">
            <table className="estimate-table">
              <thead>
                <tr>
                  <th>Description and source</th>
                  <th>Unit</th>
                  <th className="numeric">Supplied quantity</th>
                  <th className="numeric">Quantity used</th>
                  <th className="numeric">Unit rate</th>
                  <th className="numeric">Amount excluding VAT</th>
                  <th>Review</th>
                </tr>
              </thead>
              <tbody>
                {visible.slice(page * 40, page * 40 + 40).map((item) => (
                  <tr key={item.id}>
                    <td>
                      <button
                        className="file-title estimate-description"
                        title={item.description}
                        onClick={() => setSelected(item.id)}
                      >
                        {item.description}
                      </button>
                      <div className="citations">
                        <button
                          className="source-chip"
                          onClick={() => onSource({ sourceId: item.source_id })}
                        >
                          {item.document} · {item.locator}
                        </button>
                      </div>
                    </td>
                    <td>{item.unit || "Not identified"}</td>
                    <td className="numeric">
                      {item.supplied_quantity ?? "Unresolved"}
                    </td>
                    <td className="numeric">
                      {item.effective_quantity ?? "Unresolved"}
                      <small>
                        {item.quantity_basis === "approved_measurement"
                          ? "Approved measurement"
                          : "Supplied BOQ"}
                      </small>
                    </td>
                    <td className="numeric">
                      {item.unit_rate ?? "Not priced"}
                      <small>{item.currency ?? ""}</small>
                    </td>
                    <td className="numeric">
                      {item.line_ex_vat ?? "Not established"}
                    </td>
                    <td>
                      <Status
                        value={item.confirmed ? "confirmed" : "needs_review"}
                      />
                      <button
                        className="text-button"
                        onClick={() => setSelected(item.id)}
                      >
                        Review row
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {visible.length === 0 ? (
            <p className="muted">No estimate rows match these filters.</p>
          ) : null}
          {visible.length > 40 ? (
            <div className="pagination">
              <span>
                {page * 40 + 1}–{Math.min(visible.length, page * 40 + 40)} of{" "}
                {visible.length} rows
              </span>
              <button
                className="button"
                disabled={page === 0}
                onClick={() => setPage((value) => value - 1)}
              >
                Previous
              </button>
              <button
                className="button"
                disabled={(page + 1) * 40 >= visible.length}
                onClick={() => setPage((value) => value + 1)}
              >
                Next
              </button>
            </div>
          ) : null}
        </>
      ) : (
        <Empty title="No BOQ rows identified yet">
          Refresh the source rows after importing the bill of quantities.
          Identified rows need to be checked against their sources.
        </Empty>
      )}
      <RateProposals
        tenderId={tenderId}
        items={view.items}
        onSource={onSource}
      />
      {outputsAvailable ? <Outputs tenderId={tenderId} /> : null}
      {selectedItem ? (
        <EstimateEditor
          item={selectedItem}
          tenderId={tenderId}
          defaultCurrency={defaultCurrency}
          onSource={onSource}
          onClose={closeEditor}
        />
      ) : null}
    </div>
  );
}
