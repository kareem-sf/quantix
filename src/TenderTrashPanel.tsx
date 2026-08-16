import { useCallback, useEffect, useState } from "react";

import type { DeletionReceipt } from "./bindings/DeletionReceipt";
import type { TrashedTenderRecord } from "./bindings/TrashedTenderRecord";
import {
  inspectDeletionReceipts,
  inspectTrashedTenders,
  restoreTrashedTender,
} from "./quantixHost";

interface TenderTrashPanelProps {
  refreshToken: number;
  reportCommandFailure: () => void;
  onCatalogueChange: () => void;
}

export function TenderTrashPanel({
  refreshToken,
  reportCommandFailure,
  onCatalogueChange,
}: TenderTrashPanelProps) {
  const [trash, setTrash] = useState<TrashedTenderRecord[]>([]);
  const [receipts, setReceipts] = useState<DeletionReceipt[]>([]);
  const [rationales, setRationales] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const [nextTrash, nextReceipts] = await Promise.all([
        inspectTrashedTenders(),
        inspectDeletionReceipts(),
      ]);
      setTrash(nextTrash);
      setReceipts(nextReceipts);
    } catch {
      reportCommandFailure();
    }
  }, [reportCommandFailure]);

  useEffect(() => {
    void load();
  }, [load, refreshToken]);

  async function act(action: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    try {
      await action();
      setRationales({});
      await load();
      onCatalogueChange();
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  }

  const recoverable = trash.filter((record) => record.state === "trashed");
  if (!recoverable.length && !receipts.length) return null;

  return (
    <section aria-labelledby="trash-title">
      <h3 id="trash-title">Recoverable Trash and Deletion Receipts</h3>
      {recoverable.map((record) => {
        const rationale = rationales[record.deletion_id] ?? "";
        return (
          <fieldset key={record.deletion_id}>
            <legend>{record.tender_id}</legend>
            <p>
              Exact store retained at {record.relative_path} ·{" "}
              <code>{record.approval_manifest_sha256}</code>
            </p>
            <label>
              Tendering Manager rationale
              <textarea
                value={rationale}
                disabled={busy}
                onChange={(event) =>
                  setRationales((current) => ({
                    ...current,
                    [record.deletion_id]: event.target.value,
                  }))
                }
              />
            </label>
            <button
              type="button"
              disabled={busy || !rationale.trim()}
              onClick={() =>
                void act(() =>
                  restoreTrashedTender(record.deletion_id, rationale.trim()),
                )
              }
            >
              Restore without merge
            </button>
          </fieldset>
        );
      })}
      {receipts.map((receipt) => (
        <p key={receipt.receipt_id}>
          {receipt.tender_id} purged {receipt.purged_at} by {receipt.purged_by}{" "}
          · <code>{receipt.manifest_sha256}</code>
        </p>
      ))}
    </section>
  );
}
