import { useState } from "react";
import { errorText, tenderPath, useApi, type Schema } from "../api";

export function NativeCleanupReview({
  tenderId,
  receipt,
  onReviewed,
}: {
  tenderId: string;
  receipt: Schema<"NativeCleanupReceipt">;
  onReviewed: () => Promise<void>;
}) {
  const api = useApi();
  const [confirmed, setConfirmed] = useState(false);
  const [rationale, setRationale] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const label = receipt.kind === "container" ? "Container" : "Upload";

  async function submit() {
    if (!confirmed || !rationale.trim() || saving) return;
    setSaving(true);
    setError(null);
    try {
      await api.post<Schema<"MutationReceipt">>(
        `${tenderPath(tenderId)}/native-cleanup/${encodeURIComponent(receipt.id)}/review`,
        {
          engineer_confirmed: true,
          rationale: rationale.trim(),
        } satisfies Schema<"NativeCleanupReview">,
      );
      await onReviewed();
    } catch (failure) {
      setError(failure);
    } finally {
      setSaving(false);
    }
  }

  return (
    <fieldset
      className="space-y-2 rounded-lg border p-3"
      aria-label={`${label} cleanup review`}
    >
      <legend className="px-1 text-sm font-medium">
        {label} cleanup · {receipt.state}
      </legend>
      <p className="text-xs text-muted-foreground">
        Connection revision {receipt.connection_revision} · receipt {receipt.id}
      </p>
      <label className="flex flex-col gap-1 text-xs">
        Review rationale
        <textarea
          rows={2}
          value={rationale}
          maxLength={4000}
          disabled={saving}
          onChange={(event) => setRationale(event.target.value)}
        />
      </label>
      <label className="checkbox-label">
        <input
          type="checkbox"
          checked={confirmed}
          disabled={saving}
          onChange={(event) => setConfirmed(event.target.checked)}
        />
        I checked this cleanup in the provider client
      </label>
      <p className="text-xs text-muted-foreground">
        This records your provider cleanup check. It does not release or adjust
        spending holds.
      </p>
      {error ? (
        <p role="alert" className="text-sm text-destructive">
          {errorText(error)}
        </p>
      ) : null}
      <button
        type="button"
        className="button"
        disabled={!confirmed || !rationale.trim() || saving}
        onClick={() => void submit()}
      >
        {saving ? "Recording…" : "Record cleanup review"}
      </button>
    </fieldset>
  );
}
