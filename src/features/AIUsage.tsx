import { useCallback, useState } from "react";
import {
  isActive,
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading, Modal, Status } from "../components/common";
import { billingDescription, ProviderUsageDetails } from "./AISetup";

export function AIUsage({ tenderId }: { tenderId: string }) {
  const usage = useResource<Schema<"AIUsageRecord">[]>(
    `${tenderPath(tenderId)}/ai-usage`,
    true,
  );
  const runs = useResource<Schema<"Run">[]>(
    `${tenderPath(tenderId)}/runs`,
    true,
  );
  const connections =
    useResource<Schema<"ConnectionRecord">[]>("/ai/connections");
  const policy = useResource<Schema<"TenderAIRecord">>(
    `${tenderPath(tenderId)}/ai-policy`,
  );
  const [selected, setSelected] = useState<Schema<"AIUsageRecord"> | null>(
    null,
  );
  const close = useCallback(() => setSelected(null), []);
  return (
    <section className="ai-usage">
      <h2>AI usage</h2>
      <p className="muted">
        Estimated spending and amounts held for work in progress. Your
        provider’s bill may differ.
      </p>
      <ErrorNotice error={usage.error || runs.error || connections.error} />
      <ErrorNotice error={policy.error} />
      {policy.data ? (
        <dl className="ai-usage-facts ai-usage-overview">
          <div>
            <dt>Estimated spending</dt>
            <dd>
              USD{" "}
              {policy.data.spent_usd.toLocaleString(undefined, {
                maximumFractionDigits: 4,
              })}
            </dd>
          </div>
          <div>
            <dt>Held for ongoing or unresolved work</dt>
            <dd>
              USD{" "}
              {policy.data.reserved_usd.toLocaleString(undefined, {
                maximumFractionDigits: 4,
              })}
            </dd>
          </div>
          {policy.data.tender_budget_usd != null ? (
            <div>
              <dt>Estimated budget left</dt>
              <dd>
                USD{" "}
                {Math.max(
                  0,
                  policy.data.tender_budget_usd -
                    policy.data.spent_usd -
                    policy.data.reserved_usd,
                ).toLocaleString(undefined, { maximumFractionDigits: 4 })}
              </dd>
            </div>
          ) : null}
        </dl>
      ) : null}
      {policy.data?.spend_history_may_be_incomplete ? (
        <p className="ai-warning">
          This workspace was restored. This list may omit spending after the
          backup.{" "}
          {policy.data.restore_reconciliation_required
            ? "Paid AI work remains paused until you review the Tender's AI budget against the provider accounts."
            : "The later budget review did not recreate missing provider usage."}
        </p>
      ) : null}
      {usage.isPending ? <Loading>Loading AI usage…</Loading> : null}
      {usage.data?.length === 0 ? (
        <p className="field-help">
          No AI usage has been recorded for this tender.
        </p>
      ) : null}
      {usage.data?.some((record) => record.estimated_cost_usd == null) ? (
        <p className="field-help">
          Some request costs are not established and are excluded from the
          estimated total. Shared subscription usage is not a complete bill for
          this Tender.
        </p>
      ) : null}
      {usage.data?.map((record) => {
        const run = runs.data?.find((item) => item.id === record.run_id),
          unresolved = ["uncertain", "reserved"].includes(record.status);
        return (
          <article className="ai-usage-row" key={record.id}>
            <div className="section-heading">
              <div>
                <strong>
                  {connections.data?.find(
                    (connection) => connection.id === record.connection_id,
                  )?.name ?? "Saved account"}{" "}
                  · {record.model_id}
                </strong>
                <p className="field-help">
                  {new Date(record.created_at).toLocaleString()} ·{" "}
                  {billingDescription(record.billing)}
                </p>
              </div>
              <Status value={record.status} />
            </div>
            {record.actual_model && record.actual_model !== record.model_id ? (
              <p className="field-help">
                Reported model: {record.actual_model}
              </p>
            ) : null}
            <p className="field-help">
              Estimated cost:{" "}
              {record.estimated_cost_usd === null
                ? "Not established"
                : `USD ${record.estimated_cost_usd.toLocaleString(undefined, { maximumFractionDigits: 6 })}`}
              {record.reserved_usd > 0
                ? ` · Held: USD ${record.reserved_usd.toLocaleString(undefined, { maximumFractionDigits: 6 })}`
                : ""}
            </p>
            <ProviderUsageDetails record={record} />
            <details className="ai-more-options">
              <summary>Usage details</summary>
              <dl className="ai-usage-facts">
                <div>
                  <dt>Requests</dt>
                  <dd>{record.requests}</dd>
                </div>
                <div>
                  <dt>Input tokens</dt>
                  <dd>{record.input_tokens.toLocaleString()}</dd>
                </div>
                <div>
                  <dt>Output tokens</dt>
                  <dd>{record.output_tokens.toLocaleString()}</dd>
                </div>
              </dl>
              <p className="field-help">{record.detail}</p>
            </details>
            {unresolved ? (
              <>
                <button
                  type="button"
                  className="button"
                  disabled={!run || isActive(run.status)}
                  onClick={() => setSelected(record)}
                >
                  Review uncertain spending
                </button>
                {run && isActive(run.status) ? (
                  <p className="field-help">
                    Wait for this run to stop before reviewing its spending.
                  </p>
                ) : null}
              </>
            ) : null}
          </article>
        );
      })}
      {selected ? (
        <UsageReconciliation
          tenderId={tenderId}
          record={selected}
          onClose={close}
        />
      ) : null}
    </section>
  );
}
function UsageReconciliation({
  tenderId,
  record,
  onClose,
}: {
  tenderId: string;
  record: Schema<"AIUsageRecord">;
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [cost, setCost] = useState(""),
    [rationale, setRationale] = useState(""),
    [confirmed, setConfirmed] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  return (
    <Modal title="Reconcile uncertain AI usage" onClose={onClose}>
      <form
        className="ai-form"
        onSubmit={async (event) => {
          event.preventDefault();
          if (!confirmed || !cost || busy) return;
          setBusy(true);
          setError(null);
          try {
            await api.post<Schema<"MutationReceipt">>(
              `${tenderPath(tenderId)}/ai-usage/${record.id}/reconcile`,
              {
                estimated_cost_usd: Number(cost),
                engineer_confirmed: true,
                rationale: rationale.trim(),
              } satisfies Schema<"AIReconcile">,
            );
            await refresh();
            onClose();
          } catch (failure) {
            setError(failure);
          } finally {
            setBusy(false);
          }
        }}
      >
        <p className="muted">
          Record a reviewed cost estimate for this unresolved request. This
          releases its reserved allowance and preserves your explanation.
        </p>
        <fieldset disabled={busy}>
          <label>
            Reviewed cost estimate (USD)
            <input
              type="number"
              min={0}
              step="any"
              required
              value={cost}
              onChange={(event) => {
                setCost(event.target.value);
                setConfirmed(false);
              }}
            />
          </label>
          <label>
            Basis for reconciliation
            <textarea
              required
              rows={3}
              maxLength={4000}
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
            />
          </label>
          <label className="checkbox-label">
            <input
              type="checkbox"
              required
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
            />
            I reviewed this usage and approve the recorded estimate.
          </label>
        </fieldset>
        <ErrorNotice error={error} />
        <div className="form-actions">
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            className="button primary"
            disabled={
              busy ||
              !confirmed ||
              cost === "" ||
              !Number.isFinite(Number(cost)) ||
              Number(cost) < 0 ||
              !rationale.trim()
            }
          >
            {busy ? "Recording…" : "Record reconciliation"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
