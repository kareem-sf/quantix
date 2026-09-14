import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading, Status } from "../components/common";
import { Citations, type SourceSelection } from "./Sources";
import { ExternalLink } from "../components/ExternalLink";
import { FieldError } from "../components/FieldError";
import { createDraftScope, useFormDraft } from "./useFormDraft";

type Props = {
  tenderId: string;
  items: Schema<"EstimateItem">[];
  onSource: (selection: SourceSelection) => void;
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
};

function object(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}
function savedText(proposal: Schema<"RateProposalRecord">, name: string) {
  const value = object(proposal.basis.item)?.[name];
  return typeof value === "string" ? value : null;
}

// Schema amounts have at most six decimal places. Integer arithmetic retains
// all twelve product places and avoids floating-point changes to proposal text.
function decimalUnits(value: string) {
  const [whole, fraction = ""] = value.split(".");
  return BigInt(whole) * 1_000_000n + BigInt(fraction.padEnd(6, "0"));
}
function decimalProduct(quantity: string, rate: string) {
  return decimalUnits(quantity) * decimalUnits(rate);
}
function productText(value: bigint) {
  const digits = value.toString().padStart(13, "0");
  const fraction = digits.slice(-12).replace(/0+$/, "");
  return `${digits.slice(0, -12)}${fraction ? `.${fraction}` : ""}`;
}
function proposedRate(proposal: Schema<"RateProposalRecord">) {
  return (
    proposal.payload.unit_rate ??
    productText(
      (proposal.payload.components ?? []).reduce(
        (sum, component) =>
          sum + decimalProduct(component.quantity, component.unit_rate),
        0n,
      ),
    )
  );
}
function basisChanged(
  proposal: Schema<"RateProposalRecord">,
  item: Schema<"EstimateItem"> | undefined,
) {
  const saved = object(proposal.basis.item);
  if (!saved || !item || proposal.status !== "proposed") return false;
  const fields = [
    "source_id",
    "artifact_id",
    "description",
    "unit",
    "quantity_cell",
    "supplied_quantity",
    "confirmed",
    "unit_rate",
    "currency",
    "tax_basis",
    "vat_percent",
  ] as const;
  return fields.some((key) => key in saved && saved[key] !== item[key]);
}
function sourceUrl(value: string) {
  try {
    const url = new URL(value);
    return (
      ["http:", "https:"].includes(url.protocol) &&
      !url.username &&
      !url.password
    );
  } catch {
    return false;
  }
}

export function RateProposals(props: Props) {
  return <ProposalWorkspace key={props.tenderId} {...props} />;
}

function ProposalWorkspace({
  tenderId,
  items,
  onSource,
  selectedId,
  onSelect,
}: Props) {
  const path = `${tenderPath(tenderId)}/estimate/rate-proposals`;
  const proposals = useResource<Schema<"RateProposalRecord">[]>(path);
  const [localSelected, setLocalSelected] = useState<string | null>(null);
  const selected = selectedId === undefined ? localSelected : selectedId;
  const setSelected = (id: string | null) => {
    setLocalSelected(id);
    onSelect?.(id);
  };
  const active = proposals.data?.find((proposal) => proposal.id === selected);
  const itemsById = new Map(items.map((item) => [item.id, item]));
  return (
    <section
      className="rate-proposals"
      aria-labelledby="rate-proposals-heading"
    >
      <h2 id="rate-proposals-heading">Rate proposals</h2>
      <p className="muted">
        Review proposed prices and their source conditions before applying them
        to the estimate.
      </p>
      <ErrorNotice error={proposals.error} />
      {proposals.isPending ? <Loading>Loading rate proposals…</Loading> : null}
      {proposals.data?.length === 0 ? (
        <p className="rate-proposal-empty">
          No rate proposals are recorded for this Tender.
        </p>
      ) : null}
      <ul className="rate-proposal-list">
        {proposals.data?.map((proposal) => {
          const item = itemsById.get(proposal.item_id);
          const name =
            item?.description ??
            savedText(proposal, "description") ??
            "Saved BOQ item";
          return (
            <li key={proposal.id}>
              <button
                className="rate-proposal-row"
                aria-label={`Review rate proposal: ${name}`}
                aria-expanded={selected === proposal.id}
                onClick={() => setSelected(proposal.id)}
              >
                <span>
                  <strong>{name}</strong>
                  <span className="muted">
                    {proposal.payload.currency} ·{" "}
                    {proposal.payload.provenance.observed_on} ·{" "}
                    {proposal.payload.provenance.basis === "observed"
                      ? "Observed price"
                      : "Estimated rate"}
                  </span>
                </span>
                <span>
                  <Status value={proposal.status} />
                  {!proposal.is_current ||
                  !item ||
                  basisChanged(proposal, item) ? (
                    <span className="warning-text">
                      Item or source basis changed
                    </span>
                  ) : null}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
      {active ? (
        <ProposalReview
          key={`${active.id}:${active.basis_fingerprint}`}
          tenderId={tenderId}
          proposal={active}
          item={itemsById.get(active.item_id)}
          onSource={onSource}
          onClose={() => setSelected(null)}
        />
      ) : null}
    </section>
  );
}

function ProposalReview({
  tenderId,
  proposal,
  item,
  onSource,
  onClose,
}: {
  tenderId: string;
  proposal: Schema<"RateProposalRecord">;
  item?: Schema<"EstimateItem">;
  onSource: Props["onSource"];
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh(),
    client = useQueryClient();
  const draft = useFormDraft(
    createDraftScope("estimate", tenderId, `rate-proposal-${proposal.id}`, 1),
    { rationale: "" },
    ["rationale"],
  );
  const { rationale } = draft.value;
  const setRationale = (value: string) => draft.setField("rationale", value);
  const [confirmed, setConfirmed] = useState(false);
  const [confirmSource, setConfirmSource] = useState(false),
    [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>(null),
    [saved, setSaved] = useState(false);
  const changed = basisChanged(proposal, item);
  const current = proposal.is_current && !!item && !changed;
  const mayApprove = proposal.status === "proposed" && current && !saved;
  const payload = proposal.payload,
    unit = savedText(proposal, "unit") ?? item?.unit ?? "unit";
  const existing =
    item?.unit_rate == null
      ? "Not priced"
      : `${item.unit_rate} ${item.currency ?? "Currency not established"} / ${item.unit}`;
  const taxLabels = {
    unknown: "Not established",
    excluding_vat: "Excluding VAT",
    including_vat: "Including VAT",
  };
  return (
    <section className="rate-proposal-review" aria-label="Review proposed rate">
      <div className="section-heading">
        <h3>Review proposed rate</h3>
        <button
          type="button"
          className="text-button"
          disabled={pending}
          onClick={onClose}
        >
          Close proposal
        </button>
      </div>
      <p className="rate-proposal-description">
        {item?.description ??
          savedText(proposal, "description") ??
          "Saved BOQ item"}
      </p>
      {proposal.status === "approved" ? (
        <p className="rate-proposal-state">
          This proposal has already been approved. Its recorded content remains
          available for inspection.
        </p>
      ) : null}
      {!item ? (
        <p className="rate-proposal-warning">
          The BOQ item is no longer in the current estimate. This proposal
          cannot be applied.
        </p>
      ) : changed ? (
        <p className="rate-proposal-warning">
          The current BOQ item differs from the proposal basis. Review the
          changed rate, quantity or source before requesting a new proposal.
        </p>
      ) : !proposal.is_current ? (
        <p className="rate-proposal-warning">
          The item or its supporting sources have changed since this proposal
          was prepared. This proposal cannot be applied.
        </p>
      ) : (
        <p className="rate-proposal-state">
          The item and source basis match the latest service check.
        </p>
      )}
      <dl className="rate-proposal-facts">
        <dt>Proposed unit rate</dt>
        <dd>
          {proposedRate(proposal)} {payload.currency} / {unit}
        </dd>
        <dt>Current unit rate</dt>
        <dd>{existing}</dd>
        <dt>Tax basis</dt>
        <dd>{taxLabels[payload.tax_basis]}</dd>
        <dt>VAT percentage</dt>
        <dd>
          {payload.vat_percent == null
            ? "Not established"
            : `${payload.vat_percent}%`}
        </dd>
      </dl>
      {payload.tax_basis === "unknown" ? (
        <p className="rate-proposal-warning">
          VAT treatment is not established. This proposal cannot establish
          totals including VAT.
        </p>
      ) : payload.vat_percent == null ? (
        <p className="rate-proposal-warning">
          The VAT percentage is not established. Totals including VAT remain
          incomplete.
        </p>
      ) : null}
      {payload.components?.length ? (
        <RateBuildUp
          components={payload.components}
          currency={payload.currency}
        />
      ) : null}
      <RateProvenance provenance={payload.provenance} />
      <h4>Source evidence</h4>
      <Citations
        ids={proposal.source_ids}
        tenderId={tenderId}
        onOpen={onSource}
      />
      <p className="field-help">
        Prepared {proposal.created_at}. An observed source price may have
        conditions that differ from this BOQ item.
      </p>
      <QuantityBasis
        item={item}
        proposal={proposal}
        tenderId={tenderId}
        onSource={onSource}
      />
      <details className="rate-proposal-basis">
        <summary>Exact item and source basis when proposed</summary>
        <p className="field-help">
          This saved basis includes the original item values, approved
          quantities and linked source versions. Approval is checked against
          it again by the service.
        </p>
        <pre>{JSON.stringify(proposal.basis, null, 2)}</pre>
        <p>Basis fingerprint</p>
        <code>{proposal.basis_fingerprint}</code>
        {proposal.approved_basis_fingerprint ? (
          <>
            <p>Installed basis fingerprint</p>
            <code>{proposal.approved_basis_fingerprint}</code>
          </>
        ) : null}
      </details>
      {mayApprove ? (
        <form
          className="rate-proposal-decision"
          onSubmit={async (event) => {
            event.preventDefault();
            if (pending || !confirmed || !rationale.trim() || !current) return;
            setPending(true);
            setError(null);
            try {
              const path = `${tenderPath(tenderId)}/estimate/rate-proposals`;
              const acceptedRevision = draft.revision;
              const approved = await api.post<Schema<"RateProposalRecord">>(
                `${path}/${encodeURIComponent(proposal.id)}/approve`,
                {
                  engineer_confirmed: true,
                  rationale: rationale.trim(),
                  confirm_source:
                    !!item &&
                    !item.confirmed &&
                    item.supplied_quantity !== null &&
                    confirmSource,
                } satisfies Schema<"RateApproval">,
              );
              client.setQueryData<Schema<"RateProposalRecord">[]>(
                [path],
                (existing) =>
                  existing?.map((record) =>
                    record.id === approved.id ? approved : record,
                  ),
              );
              draft.markAccepted(acceptedRevision);
              setSaved(true);
              setConfirmed(false);
              await refresh();
            } catch (failure) {
              setError(failure);
              setConfirmed(false);
              await refresh();
            } finally {
              setPending(false);
            }
          }}
        >
          {item?.unit_rate != null ? (
            <p className="rate-proposal-warning">
              Approval will replace the current rate shown above and its price
              conditions with this proposal. The service refuses approval if
              that saved basis has changed.
            </p>
          ) : null}
          <p className="muted">
            This decision applies the rate and its source conditions. Supplied
            and approved measured quantities stay as shown.
          </p>
          <fieldset disabled={pending}>
            <label>
              Rate approval note
              <textarea
                required
                rows={3}
                maxLength={4000}
                value={rationale}
                onChange={(event) => setRationale(event.target.value)}
              />
              <FieldError error={error} name="rationale" />
            </label>
            {!item?.confirmed ? (
              <>
                <label className="rate-proposal-check">
                  <input
                    type="checkbox"
                    checked={confirmSource}
                    disabled={item?.supplied_quantity == null}
                    onChange={(event) => setConfirmSource(event.target.checked)}
                  />
                  I have checked the source row, selected quantity cell and unit
                  against the supplied BOQ.
                </label>
                <p className="field-help">
                  {item?.supplied_quantity == null
                    ? "Resolve the supplied quantity cell in the BOQ item before confirming its source row."
                    : "Leave this unchecked to approve only the rate; the source row remains unconfirmed."}
                </p>
              </>
            ) : null}
            <label className="rate-proposal-check">
              <input
                type="checkbox"
                checked={confirmed}
                onChange={(event) => setConfirmed(event.target.checked)}
              />
              I have reviewed this rate, its source date, tax basis, conditions
              and any replacement of the current rate.
            </label>
            <button
              className="button primary"
              disabled={pending || !confirmed || !rationale.trim()}
            >
              {pending ? "Recording rate approval…" : "Approve proposed rate"}
            </button>
          </fieldset>
        </form>
      ) : null}
      <ErrorNotice error={error || draft.error} />
      {saved ? (
        <p role="status" className="success-text">
          Rate approval recorded. Review the updated estimate and any remaining
          source or VAT gaps.
        </p>
      ) : null}
    </section>
  );
}

function RateBuildUp({
  components,
  currency,
}: {
  components: Schema<"RateComponent">[];
  currency: string;
}) {
  return (
    <div className="rate-build-up">
      <table aria-label="Proposed rate build-up">
        <thead>
          <tr>
            <th>Component</th>
            <th>Quantity per BOQ unit</th>
            <th>Unit</th>
            <th>Rate ({currency})</th>
            <th>Amount ({currency})</th>
          </tr>
        </thead>
        <tbody>
          {components.map((component, index) => (
            <tr key={index}>
              <td>{component.name}</td>
              <td>{component.quantity}</td>
              <td>{component.unit}</td>
              <td>{component.unit_rate}</td>
              <td>
                {productText(
                  decimalProduct(component.quantity, component.unit_rate),
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function RateProvenance({ provenance }: { provenance: Schema<"RateSource"> }) {
  return (
    <div className="rate-proposal-provenance">
      <h4>Rate source and conditions</h4>
      <dl className="rate-proposal-facts">
        <dt>Price basis</dt>
        <dd>
          {provenance.basis === "observed"
            ? "Observed source price"
            : "Estimated rate"}
        </dd>
        <dt>Source date</dt>
        <dd>{provenance.observed_on}</dd>
        <dt>Location or market</dt>
        <dd>{provenance.geography}</dd>
        <dt>Conditions</dt>
        <dd>{provenance.conditions}</dd>
      </dl>
      {provenance.urls?.length ? (
        <ul className="rate-proposal-links">
          {provenance.urls.map((url, index) => (
            <li key={`${url}:${index}`}>
              {sourceUrl(url) ? (
                <ExternalLink href={url}>{url}</ExternalLink>
              ) : (
                <span>{url} · Unsupported link</span>
              )}
            </li>
          ))}
        </ul>
      ) : (
        <p className="field-help">No web source URL recorded.</p>
      )}
    </div>
  );
}

function QuantityBasis({
  item,
  proposal,
  tenderId,
  onSource,
}: {
  item?: Schema<"EstimateItem">;
  proposal: Schema<"RateProposalRecord">;
  tenderId: string;
  onSource: Props["onSource"];
}) {
  const unit = item?.unit ?? savedText(proposal, "unit") ?? "unit";
  return (
    <div className="rate-proposal-quantities">
      <h4>BOQ source and quantities</h4>
      <p>
        {item?.document ??
          savedText(proposal, "document") ??
          "Saved source document"}{" "}
        · {item?.sheet ?? savedText(proposal, "sheet")} ·{" "}
        {item?.locator ?? savedText(proposal, "locator")}
      </p>
      <dl className="rate-proposal-facts">
        <dt>Source row review</dt>
        <dd>
          {item
            ? item.confirmed
              ? "Confirmed by engineer"
              : "Not confirmed"
            : "Current row unavailable"}
        </dd>
        <dt>Quantity cell / unit cell</dt>
        <dd>
          {item?.quantity_cell ?? "Not selected"} /{" "}
          {item?.unit_cell ?? "Not established"}
        </dd>
        <dt>Supplied BOQ quantity</dt>
        <dd>
          {item?.supplied_quantity != null
            ? `${item.supplied_quantity} ${unit}`
            : "Not established"}
        </dd>
        {item?.quantity_basis === "approved_measurement" ? (
          <>
            <dt>Approved measured quantity</dt>
            <dd>
              {item.effective_quantity ?? "Not established"} {unit}
            </dd>
          </>
        ) : null}
        <dt>Quantity used in estimate</dt>
        <dd>
          {item?.effective_quantity != null
            ? `${item.effective_quantity} ${unit}`
            : "Not established"}{" "}
          ·{" "}
          {item?.quantity_basis === "approved_measurement"
            ? "Approved measurement"
            : "Supplied BOQ"}
        </dd>
      </dl>
      {item?.issues.length ? (
        <ul className="rate-proposal-issues">
          {item.issues.map((issue, index) => (
            <li key={index}>{issue}</li>
          ))}
        </ul>
      ) : null}
      {item?.quantity_proposals.length ? (
        <details>
          <summary>Recorded measurement proposals</summary>
          {item.quantity_proposals.map((measurement) => (
            <div className="rate-proposal-measurement" key={measurement.id}>
              <p>
                {measurement.quantity} {unit} ·{" "}
                <Status value={measurement.status} />
              </p>
              <p>{measurement.calculation}</p>
              <Citations
                ids={measurement.source_ids}
                tenderId={tenderId}
                onOpen={onSource}
              />
            </div>
          ))}
        </details>
      ) : null}
    </div>
  );
}
