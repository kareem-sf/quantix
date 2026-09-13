import { useState, type FormEvent } from "react";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice, Modal } from "../components/common";
import { FieldError } from "../components/FieldError";
import { EvidencePicker } from "./EvidencePicker";
import { Citations, type SourceSelection } from "./Sources";
import { createDraftScope, useFormDraft } from "./useFormDraft";

export function SourceBoqForm({
  tenderId,
  onSource,
  onClose,
  onCreated,
  replacement,
}: {
  tenderId: string;
  onSource: (source: SourceSelection) => void;
  onClose: () => void;
  onCreated: (item: Schema<"EstimateItem">) => void;
  replacement?: {
    id: string;
    row_reference: string;
    description: string;
    unit: string;
    supplied_quantity: string | null;
  };
}) {
  const api = useApi(),
    refresh = useRefresh();
  const draft = useFormDraft(
    createDraftScope(
      "estimate",
      tenderId,
      replacement
        ? `source-row-replacement-${replacement.id}`
        : "source-row-new",
      1,
    ),
    {
      source_id: "",
      row_reference: replacement?.row_reference ?? "",
      source_excerpt: "",
      description: replacement?.description ?? "",
      unit: replacement?.unit ?? "",
      quantity: replacement?.supplied_quantity ?? "",
    },
    [
      "source_id",
      "row_reference",
      "source_excerpt",
      "description",
      "unit",
      "quantity",
    ],
  );
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null);
  const valid = Object.values(draft.value).every((value) => value.trim());
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!valid || busy) return;
    setBusy(true);
    setError(null);
    const acceptedRevision = draft.revision;
    try {
      const item = await api.post<Schema<"EstimateItem">>(
        `${tenderPath(tenderId)}/estimate/source-rows`,
        {
          ...draft.value,
          replaces_item_id: replacement?.id ?? null,
        } satisfies Schema<"SourceBoqProposal">,
      );
      draft.markAccepted(acceptedRevision);
      void refresh();
      onCreated(item);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal title="Add BOQ row from source" onClose={onClose}>
      <form onSubmit={(event) => void submit(event)}>
        {replacement ? (
          <p>
            Replacement for {replacement.row_reference}. Choose evidence from
            the revised file and check every prefilled value against its exact
            excerpt.
          </p>
        ) : null}
        <p>
          Choose a source page or passage and copy the exact BOQ row. The unit
          and quantity must appear in the excerpt. This saves an unconfirmed
          proposal for engineer review.
        </p>
        <ErrorNotice error={error || draft.error} />
        <fieldset disabled={busy}>
          <EvidencePicker
            tenderId={tenderId}
            selected={draft.value.source_id ? [draft.value.source_id] : []}
            onChange={(ids) => draft.setField("source_id", ids.at(-1) ?? "")}
          />
          <FieldError error={error} name="source_id" />
          {draft.value.source_id ? (
            <Citations
              ids={[draft.value.source_id]}
              tenderId={tenderId}
              onOpen={onSource}
            />
          ) : null}
          <label>
            Row reference
            <input
              required
              value={draft.value.row_reference}
              onChange={(event) =>
                draft.setField("row_reference", event.target.value)
              }
            />
            <FieldError error={error} name="row_reference" />
          </label>
          <label>
            Exact source excerpt
            <textarea
              required
              rows={4}
              value={draft.value.source_excerpt}
              onChange={(event) =>
                draft.setField("source_excerpt", event.target.value)
              }
            />
            <FieldError error={error} name="source_excerpt" />
          </label>
          <label>
            Description
            <input
              required
              value={draft.value.description}
              onChange={(event) =>
                draft.setField("description", event.target.value)
              }
            />
            <FieldError error={error} name="description" />
          </label>
          <label>
            Unit
            <input
              required
              value={draft.value.unit}
              onChange={(event) => draft.setField("unit", event.target.value)}
            />
            <FieldError error={error} name="unit" />
          </label>
          <label>
            Supplied quantity
            <input
              required
              inputMode="decimal"
              value={draft.value.quantity}
              onChange={(event) =>
                draft.setField("quantity", event.target.value)
              }
            />
            <FieldError error={error} name="quantity" />
          </label>
          <div className="inline-actions">
            <button className="button primary" type="submit" disabled={!valid}>
              {busy ? "Saving proposal…" : "Save BOQ row proposal"}
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
