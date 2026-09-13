import { useState } from "react";
import { useApi, useRefresh, useResource, type Schema } from "../api";
import { ErrorNotice } from "../components/common";

export function MailReplyCheck({ onSettings }: { onSettings?: () => void }) {
  const api = useApi(),
    refresh = useRefresh();
  const settings = useResource<Schema<"MailSettings">>("/mail/settings");
  const [busy, setBusy] = useState(false),
    [result, setResult] = useState<Schema<"SyncResult"> | null>(null),
    [error, setError] = useState<unknown>(null);
  return (
    <section className="commercial-mail-check" aria-label="Supplier mailbox">
      <div className="section-heading">
        <div>
          <h3>Supplier replies</h3>
          <p className="muted">
            Check up to 30 mailbox messages. Matching replies become evidence;
            supplier prices still need review.
          </p>
        </div>
        <button
          type="button"
          className="button"
          disabled={busy || !settings.data?.imap_ready}
          onClick={async () => {
            if (busy) return;
            setBusy(true);
            setError(null);
            setResult(null);
            try {
              setResult(
                await api.post<Schema<"SyncResult">>("/mail/sync", {
                  max_messages: 30,
                } satisfies Schema<"SyncRequest">),
              );
              await refresh();
            } catch (failure) {
              setError(failure);
            } finally {
              setBusy(false);
            }
          }}
        >
          {busy ? "Checking replies…" : "Check replies"}
        </button>
      </div>
      {settings.data &&
      (!settings.data.imap_ready || !settings.data.smtp_ready) ? (
        <p className="field-help">
          {!settings.data.imap_ready
            ? "Incoming mail is not configured."
            : "Outgoing mail is not configured."}{" "}
          {onSettings ? (
            <button type="button" className="text-button" onClick={onSettings}>
              Set up supplier mail
            </button>
          ) : (
            "Open Mail in Settings to configure the account."
          )}
        </p>
      ) : null}
      <ErrorNotice error={error || settings.error} />
      {result ? (
        <div role="status">
          <p>
            Checked {result.checked} messages · {result.matched} replies
            registered · {result.skipped} skipped
          </p>
          {result.more_available ? (
            <p className="field-help">
              More messages are available. Check again to read the next batch.
            </p>
          ) : null}
          {result.warnings.map((warning, index) => (
            <p className="warning-text" key={index}>
              {warning}
            </p>
          ))}
        </div>
      ) : null}
    </section>
  );
}
