import { useState } from "react";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice } from "../components/ui";
import { EvidencePicker } from "./EvidencePicker";

export const decimalPattern = "[0-9]{1,12}(?:\\.[0-9]{1,6})?";
export function RateForm({
  item,
  tenderId,
  defaultCurrency,
}: {
  item: Schema<"EstimateItem">;
  tenderId: string;
  defaultCurrency: string;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [rate, setRate] = useState(item.unit_rate ?? ""),
    [currency, setCurrency] = useState(item.currency ?? defaultCurrency),
    [tax, setTax] = useState<NonNullable<Schema<"ItemUpdate">["tax_basis"]>>(
      (["including_vat", "excluding_vat"].includes(item.tax_basis)
        ? item.tax_basis
        : "unknown") as NonNullable<Schema<"ItemUpdate">["tax_basis"]>,
    ),
    [vat, setVat] = useState(item.vat_percent ?? "");
  const [basis, setBasis] = useState<Schema<"RateSource">["basis"]>(
      item.provenance?.basis ?? "estimated",
    ),
    [date, setDate] = useState(item.provenance?.observed_on ?? ""),
    [geography, setGeography] = useState(item.provenance?.geography ?? ""),
    [conditions, setConditions] = useState(item.provenance?.conditions ?? ""),
    [urls, setUrls] = useState(item.provenance?.urls?.join("\n") ?? ""),
    [sources, setSources] = useState<string[]>(
      item.provenance?.source_ids ?? [],
    );
  const [note, setNote] = useState(""),
    [checked, setChecked] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null),
    [saved, setSaved] = useState(false);
  return (
    <form
      onSubmit={async (event) => {
        event.preventDefault();
        if (!checked) return;
        setPending(true);
        setError(null);
        setSaved(false);
        try {
          await api.patch<Schema<"EstimateItem">>(
            `${tenderPath(tenderId)}/estimate/items/${item.id}`,
            {
              engineer_confirmed: true,
              confirm_source: false,
              rationale: note,
              unit_rate: rate,
              currency: currency.toUpperCase(),
              tax_basis: tax,
              vat_percent: vat || null,
              provenance: {
                basis,
                observed_on: date,
                source_ids: sources,
                urls: urls
                  .split(/\r?\n/)
                  .map((value) => value.trim())
                  .filter(Boolean),
                geography,
                conditions,
              },
            } satisfies Schema<"ItemUpdate">,
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
      <div className="form-grid">
        <label>
          Unit rate
          <input
            required
            inputMode="decimal"
            pattern={decimalPattern}
            value={rate}
            onChange={(event) => setRate(event.target.value)}
          />
        </label>
        <label>
          Currency
          <input
            required
            maxLength={3}
            pattern="[A-Z]{3}"
            value={currency}
            onChange={(event) => setCurrency(event.target.value.toUpperCase())}
          />
        </label>
        <label>
          Tax basis
          <select
            value={tax}
            onChange={(event) => setTax(event.target.value as typeof tax)}
          >
            <option value="unknown">Not established</option>
            <option value="excluding_vat">Excluding VAT</option>
            <option value="including_vat">Including VAT</option>
          </select>
        </label>
        <label>
          VAT percentage
          <input
            inputMode="decimal"
            pattern={decimalPattern}
            value={vat}
            onChange={(event) => setVat(event.target.value)}
            placeholder="Leave blank if unknown"
          />
        </label>
        <label>
          Rate basis
          <select
            value={basis}
            onChange={(event) => setBasis(event.target.value as typeof basis)}
          >
            <option value="estimated">Estimated rate</option>
            <option value="observed">Observed price</option>
          </select>
        </label>
        <label>
          Source date
          <input
            required
            type="date"
            value={date}
            onChange={(event) => setDate(event.target.value)}
          />
        </label>
      </div>
      <label>
        Location or market
        <input
          required
          maxLength={200}
          value={geography}
          onChange={(event) => setGeography(event.target.value)}
        />
      </label>
      <label>
        Rate conditions
        <textarea
          required
          rows={3}
          maxLength={2000}
          value={conditions}
          onChange={(event) => setConditions(event.target.value)}
          placeholder="Supply basis, delivery, specification and other conditions"
        />
      </label>
      <label>
        Source links
        <textarea
          rows={2}
          value={urls}
          onChange={(event) => setUrls(event.target.value)}
          placeholder="One http:// or https:// link per line"
        />
      </label>
      <p className="field-help">
        Links are recorded as supplied. They are not treated as independently
        verified prices.
      </p>
      <EvidencePicker
        tenderId={tenderId}
        selected={sources}
        onChange={setSources}
      />
      <label>
        Rate decision note
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
        I confirm this rate, its source basis and tax treatment.
      </label>
      {item.components.length ? (
        <p className="warning-text">
          Saving a direct rate replaces the current component build-up for this
          row.
        </p>
      ) : null}
      <ErrorNotice error={error} />
      {saved ? (
        <p role="status" className="success-text">
          Rate decision recorded.
        </p>
      ) : null}
      <button
        className="button primary"
        disabled={pending || !checked || !note.trim()}
      >
        {pending ? "Saving…" : "Save rate decision"}
      </button>
    </form>
  );
}
