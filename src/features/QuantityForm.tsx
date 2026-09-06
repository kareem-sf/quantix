import { useState } from "react";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice } from "../components/ui";
import { EvidencePicker } from "./EvidencePicker";
import { decimalPattern } from "./RateForm";

export function QuantityForm({
  item,
  tenderId,
}: {
  item: Schema<"EstimateItem">;
  tenderId: string;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [quantity, setQuantity] = useState(""),
    [calculation, setCalculation] = useState(""),
    [sources, setSources] = useState<string[]>([]),
    [note, setNote] = useState(""),
    [checked, setChecked] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null),
    [saved, setSaved] = useState(false);
  return (
    <form
      onSubmit={async (event) => {
        event.preventDefault();
        if (!checked || !sources.length) return;
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
      </label>
      <EvidencePicker
        tenderId={tenderId}
        selected={sources}
        onChange={setSources}
      />
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
      <label className="checkbox-label">
        <input
          required
          type="checkbox"
          checked={checked}
          onChange={(event) => setChecked(event.target.checked)}
        />
        I confirm this calculation and its supporting sources.
      </label>
      <ErrorNotice error={error} />
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
