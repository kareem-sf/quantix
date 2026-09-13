import { useState } from "react";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice } from "../components/common";
import { FieldError } from "../components/FieldError";
import { EvidencePicker } from "./EvidencePicker";
import { createDraftScope, useFormDraft } from "./useFormDraft";

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
  const draft = useFormDraft(
    createDraftScope("estimate", tenderId, `rate-${item.id}`, item.source_id),
    {
      rate: item.unit_rate ?? "",
      currency: item.currency ?? defaultCurrency,
      tax: (["including_vat", "excluding_vat"].includes(item.tax_basis)
        ? item.tax_basis
        : "unknown") as NonNullable<Schema<"ItemUpdate">["tax_basis"]>,
      vat: item.vat_percent ?? "",
      basis: (item.provenance?.basis ??
        "estimated") as Schema<"RateSource">["basis"],
      date: item.provenance?.observed_on ?? "",
      geography: item.provenance?.geography ?? "",
      conditions: item.provenance?.conditions ?? "",
      urls: item.provenance?.urls?.join("\n") ?? "",
      sources: item.provenance?.source_ids ?? [],
      note: "",
    },
    [
      "rate",
      "currency",
      "tax",
      "vat",
      "basis",
      "date",
      "geography",
      "conditions",
      "urls",
      "sources",
      "note",
    ],
  );
  const {
    rate,
    currency,
    tax,
    vat,
    basis,
    date,
    geography,
    conditions,
    urls,
    sources,
    note,
  } = draft.value;
  const setRate = (v: string) => draft.setField("rate", v),
    setCurrency = (v: string) => draft.setField("currency", v),
    setTax = (v: typeof tax) => draft.setField("tax", v),
    setVat = (v: string) => draft.setField("vat", v),
    setBasis = (v: typeof basis) => draft.setField("basis", v),
    setDate = (v: string) => draft.setField("date", v),
    setGeography = (v: string) => draft.setField("geography", v),
    setConditions = (v: string) => draft.setField("conditions", v),
    setUrls = (v: string) => draft.setField("urls", v),
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
          <FieldError error={error} name="unit_rate" />
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
          <FieldError error={error} name="currency" />
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
          <FieldError error={error} name="tax_basis" />
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
          <FieldError error={error} name="vat_percent" />
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
          <FieldError error={error} name="observed_on" />
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
        <FieldError error={error} name="geography" />
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
        <FieldError error={error} name="conditions" />
      </label>
      <label>
        Source links
        <textarea
          rows={2}
          value={urls}
          onChange={(event) => setUrls(event.target.value)}
          placeholder="One http:// or https:// link per line"
        />
        <FieldError error={error} name="urls" />
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
      <FieldError error={error} name="rationale" />
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
      <ErrorNotice error={error || draft.error} />
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
