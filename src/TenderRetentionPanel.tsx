import { useCallback, useEffect, useState } from "react";

import type { PortableTenderArchiveRecord } from "./bindings/PortableTenderArchiveRecord";
import type { TenderRetentionState } from "./bindings/TenderRetentionState";
import {
  archiveTender,
  createPortableTenderArchive,
  inspectPortableTenderArchives,
  inspectTenderRetention,
  restoreArchivedTender,
  trashTender,
} from "./quantixHost";

interface TenderRetentionPanelProps {
  tenderId: string;
  refreshToken: number;
  reportCommandFailure: () => void;
  onTenderStateChange: () => void;
  onTenderRemoved: () => void;
}

export function TenderRetentionPanel({
  tenderId,
  refreshToken,
  reportCommandFailure,
  onTenderStateChange,
  onTenderRemoved,
}: TenderRetentionPanelProps) {
  const [retention, setRetention] = useState<TenderRetentionState>("active");
  const [archives, setArchives] = useState<PortableTenderArchiveRecord[]>([]);
  const [rationale, setRationale] = useState("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const [nextRetention, nextArchives] = await Promise.all([
        inspectTenderRetention(tenderId),
        inspectPortableTenderArchives(tenderId),
      ]);
      setRetention(nextRetention);
      setArchives(nextArchives);
    } catch {
      reportCommandFailure();
    }
  }, [reportCommandFailure, tenderId]);

  useEffect(() => {
    void load();
  }, [load, refreshToken]);

  async function act(action: () => Promise<unknown>, removesTender = false) {
    if (busy) return;
    setBusy(true);
    try {
      await action();
      setRationale("");
      if (removesTender) {
        onTenderRemoved();
      } else {
        await load();
        onTenderStateChange();
      }
    } catch {
      reportCommandFailure();
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="workspace-section" aria-labelledby="retention-title">
      <div className="section-heading">
        <div>
          <p className="eyebrow">Retention lifecycle</p>
          <h2 id="retention-title">Archive and retention controls</h2>
        </div>
        <button
          type="button"
          disabled={busy}
          onClick={() => void act(() => createPortableTenderArchive(tenderId))}
        >
          Create verified Portable Tender Archive
        </button>
      </div>

      <p role="status">
        Tender Store is{" "}
        {retention === "archived" ? "archived read-only" : "active"}.
      </p>
      <label>
        Tendering Manager rationale
        <textarea
          value={rationale}
          disabled={busy}
          onChange={(event) => setRationale(event.target.value)}
        />
      </label>
      {retention === "active" ? (
        <button
          type="button"
          disabled={busy || !rationale.trim()}
          onClick={() =>
            void act(() => archiveTender(tenderId, rationale.trim()))
          }
        >
          Archive as reversible read-only
        </button>
      ) : (
        <button
          type="button"
          disabled={busy || !rationale.trim()}
          onClick={() =>
            void act(() => restoreArchivedTender(tenderId, rationale.trim()))
          }
        >
          Restore archived Tender to active storage
        </button>
      )}
      <button
        type="button"
        className="danger"
        disabled={busy || !rationale.trim()}
        onClick={() =>
          void act(() => trashTender(tenderId, rationale.trim()), true)
        }
      >
        Approve move of complete Tender Store to Trash
      </button>

      <h3>Portable Tender Archives</h3>
      {archives.length ? (
        <ul>
          {archives.map((archive) => (
            <li key={archive.archive_id}>
              {archive.relative_path} · {archive.content_object_count} Content
              Objects · <code>{archive.manifest_sha256}</code>
            </li>
          ))}
        </ul>
      ) : (
        <p>No Portable Tender Archive recorded.</p>
      )}
    </section>
  );
}
