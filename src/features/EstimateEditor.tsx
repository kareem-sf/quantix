import { useCallback, useState } from "react";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice, Modal, Status } from "../components/common";
import { Citations, type SourceSelection } from "./Sources";
import { DecisionForm } from "./Work";
import { RateForm } from "./RateForm";
import { QuantityForm } from "./QuantityForm";
import { FieldError } from "../components/FieldError";
import { createDraftScope, useFormDraft } from "./useFormDraft";

export function EstimateEditor({
  item,
  tenderId,
  defaultCurrency,
  onSource,
  onClose,
}: {
  item: Schema<"EstimateItem">;
  tenderId: string;
  defaultCurrency: string;
  onSource: (source: SourceSelection) => void;
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [proposalId, setProposalId] = useState<string | null>(null);
  const sections = useFormDraft(
    createDraftScope(
      "estimate",
      tenderId,
      `row-view-${item.id}`,
      item.source_id,
    ),
    {
      source: !item.confirmed,
      rate: false,
      components: false,
      quantity: false,
    },
    ["source", "rate", "components", "quantity"],
  );
  const rememberSection = (
    name: keyof typeof sections.value,
    open: boolean,
  ) => {
    if (sections.value[name] !== open) sections.setField(name, open);
  };
  const closeApproval = useCallback(() => setProposalId(null), []);
  return (
    <Modal drawer title="Review estimate row" onClose={onClose}>
      <div className="estimate-editor">
        <h3>{item.description}</h3>
        {item.source_proposal ? (
          <section aria-label="Proposed source row">
            <p className="muted">
              Source BOQ proposal ·{" "}
              {item.confirmed
                ? "Engineer confirmation recorded"
                : "Not yet confirmed by an engineer"}
            </p>
            <dl>
              <dt>Row reference</dt>
              <dd>{item.row_reference || "Not recorded"}</dd>
            </dl>
            <h4>Exact source excerpt</h4>
            <blockquote className="whitespace-pre-wrap">
              {item.source_excerpt || "Not recorded"}
            </blockquote>
          </section>
        ) : null}
        <Citations
          ids={[item.source_id]}
          tenderId={tenderId}
          onOpen={onSource}
        />
        <dl className="estimate-row-facts">
          <dt>Supplied quantity</dt>
          <dd>
            {item.supplied_quantity ?? "Unresolved"} {item.unit}
          </dd>
          <dt>Quantity used</dt>
          <dd>
            {item.effective_quantity ?? "Unresolved"} {item.unit} ·{" "}
            {item.quantity_basis === "approved_measurement"
              ? "Approved quantity proposal"
              : "Supplied BOQ"}
          </dd>
          <dt>Unit rate</dt>
          <dd>
            {item.unit_rate ?? "Not priced"} {item.currency}
          </dd>
          <dt>Tax basis</dt>
          <dd>
            {item.tax_basis === "unknown"
              ? "Not established"
              : item.tax_basis.replaceAll("_", " ")}
            {item.vat_percent !== null ? ` · ${item.vat_percent}%` : ""}
          </dd>
          <dt>Source status</dt>
          <dd>
            <Status value={item.confirmed ? "confirmed" : "needs_review"} />
          </dd>
        </dl>
        {item.issues.length ? (
          <ul className="estimate-issues">
            {item.issues.map((issue) => (
              <li key={issue}>{issue}</li>
            ))}
          </ul>
        ) : null}
        <details
          className="editor-section"
          open={sections.value.source}
          onToggle={(event) =>
            rememberSection("source", event.currentTarget.open)
          }
        >
          <summary>Confirm the supplied source row</summary>
          <p className="muted">
            Check the description, unit and quantity in the original source
            before confirming this row.
          </p>
          <SourceConfirmation item={item} tenderId={tenderId} />
        </details>
        <details
          className="editor-section"
          open={sections.value.rate}
          onToggle={(event) =>
            rememberSection("rate", event.currentTarget.open)
          }
        >
          <summary>Rate and tax treatment</summary>
          <p className="muted">
            Record a direct unit rate with its dated source, location and
            conditions.
          </p>
          <RateForm
            key={`${item.id}-rate`}
            item={item}
            tenderId={tenderId}
            defaultCurrency={defaultCurrency}
          />
        </details>
        {item.components.length ? (
          <details
            className="editor-section"
            open={sections.value.components}
            onToggle={(event) =>
              rememberSection("components", event.currentTarget.open)
            }
          >
            <summary>Current rate build-up</summary>
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Component</th>
                    <th>Quantity</th>
                    <th>Unit</th>
                    <th>Rate</th>
                  </tr>
                </thead>
                <tbody>
                  {item.components.map((part, index) => (
                    <tr key={index}>
                      <td>{part.name}</td>
                      <td>{part.quantity}</td>
                      <td>{part.unit}</td>
                      <td>{part.unit_rate}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </details>
        ) : null}
        <details
          className="editor-section"
          open={sections.value.quantity}
          onToggle={(event) =>
            rememberSection("quantity", event.currentTarget.open)
          }
        >
          <summary>Propose a different quantity</summary>
          <p className="muted">
            The supplied quantity remains the default. A quantity proposal is
            recorded separately and needs approval before use.
          </p>
          <QuantityForm item={item} tenderId={tenderId} />
        </details>
        {item.quantity_proposals.length ? (
          <section className="editor-section">
            <h3>Quantity proposals</h3>
            {item.quantity_proposals.map((proposal) => (
              <article className="quantity-proposal" key={proposal.id}>
                <div className="section-heading">
                  <strong>
                    {proposal.quantity} {item.unit}
                  </strong>
                  <Status value={proposal.status} />
                </div>
                <p className="muted">
                  {proposal.origin === "agent"
                    ? "Tender Office agent proposal"
                    : "Engineer proposal"}
                  {proposal.run_id ? ` · run ${proposal.run_id}` : ""}
                </p>
                <p>{proposal.calculation}</p>
                {proposal.origin === "agent" &&
                proposal.status === "proposed" ? (
                  <p className="muted">
                    Review the dimensions, grouping, deductions, arithmetic and
                    BOQ unit before approval. The agent's calculation has not
                    received engineer approval.
                  </p>
                ) : null}
                <Citations
                  ids={proposal.source_ids}
                  tenderId={tenderId}
                  onOpen={onSource}
                />
                {proposal.status === "proposed" ? (
                  <button
                    className="text-button"
                    onClick={() => setProposalId(proposal.id)}
                  >
                    Approve proposed quantity
                  </button>
                ) : null}
              </article>
            ))}
          </section>
        ) : null}
        {proposalId ? (
          <DecisionForm
            draftScope={createDraftScope(
              "estimate",
              tenderId,
              `quantity-approval-${proposalId}`,
              1,
            )}
            title="Approve proposed quantity"
            action="Approve quantity"
            description="Review the source dimensions, grouping, deductions, arithmetic and BOQ unit. This proposal will become the quantity used for pricing. The original supplied quantity and calculation remain in the record."
            onClose={closeApproval}
            onSubmit={async (rationale) => {
              await api.post<Schema<"EstimateView">>(
                `${tenderPath(tenderId)}/estimate/quantity-proposals/${proposalId}/approve`,
                {
                  engineer_confirmed: true,
                  rationale,
                } satisfies Schema<"EngineerDecision">,
              );
              await refresh();
              closeApproval();
            }}
          />
        ) : null}
      </div>
    </Modal>
  );
}
function SourceConfirmation({
  item,
  tenderId,
}: {
  item: Schema<"EstimateItem">;
  tenderId: string;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const hasApprovedMeasurement =
    item.quantity_basis === "approved_measurement" &&
    item.effective_quantity !== null;
  const measurementResolvesQuantity =
    hasApprovedMeasurement && item.supplied_quantity === null;
  const draft = useFormDraft(
    createDraftScope(
      "estimate",
      tenderId,
      `source-confirmation-${item.id}`,
      item.source_id,
    ),
    { cell: item.quantity_cell ?? "", note: "" },
    ["cell", "note"],
  );
  const { cell, note } = draft.value;
  const setCell = (value: string) => draft.setField("cell", value),
    setNote = (value: string) => draft.setField("note", value);
  const [checked, setChecked] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null),
    [saved, setSaved] = useState(false);
  return (
    <form
      onSubmit={async (event) => {
        event.preventDefault();
        if (!checked || pending) return;
        const acceptedRevision = draft.revision;
        setPending(true);
        setError(null);
        setSaved(false);
        try {
          await api.patch<Schema<"EstimateItem">>(
            `${tenderPath(tenderId)}/estimate/items/${item.id}`,
            {
              engineer_confirmed: true,
              rationale: note,
              confirm_source: true,
              ...(cell ? { quantity_cell: cell } : {}),
            } satisfies Schema<"ItemUpdate">,
          );
          await refresh();
          draft.markAccepted(acceptedRevision);
          setChecked(false);
          setSaved(true);
        } catch (failure) {
          setError(failure);
        } finally {
          setPending(false);
        }
      }}
    >
      {measurementResolvesQuantity ? (
        <p className="field-help">
          The supplied quantity is unresolved. The approved quantity proposal
          provides {item.effective_quantity} {item.unit}. Confirm the source
          description, unit and measurement basis; the original quantity warning
          stays in the record.
        </p>
      ) : null}
      {!item.source_proposal ? (
        <label>
          Quantity source cell
          <select
            value={cell}
            onChange={(event) => setCell(event.target.value)}
            required={!hasApprovedMeasurement}
          >
            <option value="">
              {hasApprovedMeasurement
                ? "Use the approved measured quantity"
                : "Choose the quantity cell"}
            </option>
            {Object.entries(item.quantity_candidates).map(
              ([address, value]) => (
                <option key={address} value={address}>
                  {address} · {value}
                </option>
              ),
            )}
          </select>
        </label>
      ) : (
        <p className="field-help">
          Check the exact cited excerpt and row reference. The supplied quantity
          is {item.supplied_quantity ?? "unresolved"} {item.unit}.
        </p>
      )}
      <label>
        Source review note
        <textarea
          rows={2}
          required
          maxLength={4000}
          value={note}
          onChange={(event) => setNote(event.target.value)}
        />
        <FieldError error={error} name="rationale" />
      </label>
      <label className="checkbox-label">
        <input
          type="checkbox"
          required
          checked={checked}
          onChange={(event) => setChecked(event.target.checked)}
        />
        {measurementResolvesQuantity
          ? "I have checked this source row and its approved measurement basis."
          : item.source_proposal
            ? "I have checked this source row, exact excerpt, unit and quantity."
            : "I have checked this source row and quantity cell."}
      </label>
      <ErrorNotice error={error || draft.error} />
      {saved ? (
        <p role="status" className="success-text">
          Source confirmation recorded.
        </p>
      ) : null}
      <button
        className="button primary"
        disabled={
          pending ||
          !checked ||
          (!cell && !hasApprovedMeasurement && !item.source_proposal) ||
          !note.trim()
        }
      >
        {pending ? "Saving…" : "Confirm source row"}
      </button>
    </form>
  );
}
