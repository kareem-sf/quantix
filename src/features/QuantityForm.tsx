import { useState } from "react";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice } from "../components/common";
import { FieldError } from "../components/FieldError";
import { EvidencePicker } from "./EvidencePicker";
import { decimalPattern } from "./RateForm";
import { createDraftScope, useFormDraft } from "./useFormDraft";

export function QuantityForm({
  item,
  tenderId,
}: {
  item: Schema<"EstimateItem">;
  tenderId: string;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const draft = useFormDraft(
    createDraftScope(
      "estimate",
      tenderId,
      `quantity-${item.id}`,
      item.source_id,
    ),
    { quantity: "", calculation: "", sources: [] as string[], note: "" },
    ["quantity", "calculation", "sources", "note"],
  );
  const { quantity, calculation, sources, note } = draft.value;
  const setQuantity = (v: string) => draft.setField("quantity", v),
    setCalculation = (v: string) => draft.setField("calculation", v),
    setSources = (v: string[]) => draft.setField("sources", v),
    setNote = (v: string) => draft.setField("note", v);
  const [checked, setChecked] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null),
    [saved, setSaved] = useState(false);
  return (
    <form
      onSubmit={async (event) => {
        event.preventDefault();
        if (!checked || !sources.length || pending) return;
        const acceptedRevision = draft.revision;
        setPending(true);
        setError(null);
        setSaved(false);
        try {
          await api.post<Schema<"QuantityProposal">>(
            `${tenderPath(tenderId)}/estimate/items/${item.id}/quantity-proposals`,
            {
              engineer_confirmed: true,
              rationale: note,
              quantity,
              calculation,
              source_ids: sources,
            } satisfies Schema<"QuantityRequest">,
          );
          draft.markAccepted(acceptedRevision);
          setChecked(false);
          await refresh();
          setSaved(true);
        } catch (failure) {
          setError(failure);
        } finally {
          setPending(false);
        }
      }}
    >
      <label>
        Proposed quantity ({item.unit})
        <input
          required
          inputMode="decimal"
          pattern={decimalPattern}
          value={quantity}
          onChange={(event) => setQuantity(event.target.value)}
        />
        <FieldError error={error} name="quantity" />
      </label>
      <label>
        Calculation and quantity basis
        <textarea
          required
          rows={4}
          maxLength={6000}
          value={calculation}
          onChange={(event) => setCalculation(event.target.value)}
          placeholder="Record dimensions, grouping, deductions, arithmetic and the scope covered"
        />
        <FieldError error={error} name="calculation" />
      </label>
      <EvidencePicker
        tenderId={tenderId}
        selected={sources}
        onChange={setSources}
      />
      <FieldError error={error} name="source_ids" />
      <label>
        Quantity proposal note
        <textarea
          required
          rows={2}
          maxLength={4000}
          value={note}
          onChange={(event) => setNote(event.target.value)}
        />
      </label>
      <FieldError error={error} name="rationale" />
      <label className="checkbox-label">
        <input
          required
          type="checkbox"
          checked={checked}
          onChange={(event) => setChecked(event.target.checked)}
        />
        I confirm this calculation and its supporting sources.
      </label>
      <ErrorNotice error={error || draft.error} />
      {saved ? (
        <p role="status" className="success-text">
          Quantity proposal saved. The supplied quantity remains in use until
          approval.
        </p>
      ) : null}
      <button
        className="button primary"
        disabled={pending || !checked || !sources.length || !note.trim()}
      >
        {pending ? "Saving…" : "Save quantity proposal"}
      </button>
    </form>
  );
}
