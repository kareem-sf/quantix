import { useState, type FormEvent } from "react";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
import { Button } from "@/components/ui/button";
import { ErrorNotice, Modal } from "../components/common";
import { FieldError } from "../components/FieldError";
import type { SourceSelection } from "./Sources";
import { createDraftScope, useFormDraft } from "./useFormDraft";

export type RetiredSourceRow = NonNullable<
  Schema<"EstimateView">["retired_source_rows"]
>[number];

export function RetiredSourceRows({
  tenderId,
  rows,
  onSource,
  onReplace,
}: {
  tenderId: string;
  rows: RetiredSourceRow[];
  onSource: (source: SourceSelection) => void;
  onReplace: (row: RetiredSourceRow) => void;
}) {
  const [excluding, setExcluding] = useState<RetiredSourceRow | null>(null);
  if (!rows.length) return null;
  return (
    <section
      className="flex flex-col gap-3 rounded-xl border border-amber-500/40 p-4"
      aria-label="BOQ rows affected by revised sources"
    >
      <h3 className="text-sm font-semibold">
        BOQ rows affected by revised sources
      </h3>
      <p className="text-sm text-muted-foreground">
        These rows came from earlier file revisions. Inspect the revised source
        and confirm a replacement row, or record why the row is no longer
        required. Estimate completeness remains blocked until each row is
        resolved.
      </p>
      {rows.map((row) => (
        <article
          className="flex flex-col gap-2 rounded-lg border bg-card p-3"
          key={row.id}
        >
          <h4 className="text-sm font-medium">{row.description}</h4>
          <p className="text-sm text-muted-foreground">
            {row.row_reference} · {row.supplied_quantity ?? "Unresolved"}{" "}
            {row.unit}
          </p>
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={() => onSource({ sourceId: row.source_id })}
            >
              Inspect previous source
            </Button>
            {row.current_artifact_id ? (
              <Button
                size="sm"
                variant="outline"
                onClick={() =>
                  onSource({ artifactId: row.current_artifact_id! })
                }
              >
                Inspect revised file
              </Button>
            ) : null}
            <Button size="sm" onClick={() => onReplace(row)}>
              Add replacement row
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setExcluding(row)}>
              No longer required
            </Button>
          </div>
        </article>
      ))}
      {excluding ? (
        <ExclusionDecision
          key={excluding.id}
          tenderId={tenderId}
          row={excluding}
          onClose={() => setExcluding(null)}
        />
      ) : null}
    </section>
  );
}

function ExclusionDecision({
  tenderId,
  row,
  onClose,
}: {
  tenderId: string;
  row: RetiredSourceRow;
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const draft = useFormDraft(
    createDraftScope("estimate", tenderId, `exclude-source-row-${row.id}`, 1),
    { rationale: "" },
    ["rationale"],
  );
  const [consent, setConsent] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy || !consent || !draft.value.rationale.trim()) return;
    setBusy(true);
    setError(null);
    const acceptedRevision = draft.revision;
    try {
      await api.post(
        `${tenderPath(tenderId)}/estimate/source-rows/${encodeURIComponent(row.id)}/exclude`,
        {
          engineer_confirmed: true,
          rationale: draft.value.rationale,
          current_artifact_id: row.current_artifact_id ?? null,
        } satisfies Schema<"SourceRowExclusion">,
      );
      draft.markAccepted(acceptedRevision);
      void refresh();
      onClose();
    } catch (failure) {
      setError(failure);
      setConsent(false);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal title="Record a row no longer required" onClose={onClose}>
      <form onSubmit={(event) => void submit(event)}>
        <p>
          {row.description} · {row.row_reference}
        </p>
        <p>
          This records an engineer decision that the old row does not need a
          replacement. Its source and history remain available.
        </p>
        <ErrorNotice error={error || draft.error} />
        <fieldset disabled={busy}>
          <label>
            Reason this row is no longer required
            <textarea
              required
              maxLength={4000}
              value={draft.value.rationale}
              onChange={(event) =>
                draft.setField("rationale", event.target.value)
              }
            />
            <FieldError error={error} name="rationale" />
          </label>
          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={consent}
              onChange={(event) => setConsent(event.target.checked)}
            />
            I checked the revised source and confirm this row is no longer
            required.
          </label>
          <div className="inline-actions">
            <button
              className="button primary"
              disabled={!consent || !draft.value.rationale.trim()}
              type="submit"
            >
              {busy ? "Recording exclusion…" : "Record exclusion"}
            </button>
            <button className="button" type="button" onClick={onClose}>
              Cancel
            </button>
          </div>
        </fieldset>
      </form>
    </Modal>
  );
}
