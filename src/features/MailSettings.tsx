import { useEffect, useState, type FormEvent } from "react";
import { useApi, useRefresh, useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { FieldError } from "../components/FieldError";

export function MailSettings({
  onAttention,
}: {
  onAttention?: (attention: boolean) => void;
} = {}) {
  const settings = useResource<Schema<"MailSettings">>("/mail/settings");
  useEffect(() => {
    if (!settings.data) onAttention?.(!!settings.error);
  }, [onAttention, settings.data, settings.error]);
  if (settings.isPending) return <Loading>Loading mail settings…</Loading>;
  if (!settings.data) return <ErrorNotice error={settings.error} />;
  return (
    <MailSettingsForm settings={settings.data} onAttention={onAttention} />
  );
}

function MailSettingsForm({
  settings,
  onAttention,
}: {
  settings: Schema<"MailSettings">;
  onAttention?: (attention: boolean) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [account, setAccount] = useState<Schema<"MailSettingsPatch">>(() => ({
    smtp_host: settings.smtp_host,
    smtp_port: settings.smtp_port,
    smtp_security: settings.smtp_security,
    smtp_username: settings.smtp_username,
    from_address: settings.from_address,
    imap_host: settings.imap_host,
    imap_port: settings.imap_port,
    imap_username: settings.imap_username,
    imap_mailbox: settings.imap_mailbox,
  }));
  const [smtpPassword, setSmtpPassword] = useState("");
  const [imapPassword, setImapPassword] = useState("");
  const [clearSmtp, setClearSmtp] = useState(false),
    [clearImap, setClearImap] = useState(false);
  const [pending, setPending] = useState(false),
    [saved, setSaved] = useState(false);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    onAttention?.(!!error);
  }, [error, onAttention]);
  const changed = (patch: Partial<Schema<"MailSettingsPatch">>) => {
    setAccount((current) => ({ ...current, ...patch }));
    setSaved(false);
  };

  async function save(event: FormEvent) {
    event.preventDefault();
    if (pending) return;
    setPending(true);
    setSaved(false);
    setError(null);
    try {
      await api.patch<Schema<"MailSettings">>("/mail/settings", {
        ...account,
        ...(clearSmtp
          ? { smtp_password: "" }
          : smtpPassword
            ? { smtp_password: smtpPassword }
            : {}),
        ...(clearImap
          ? { imap_password: "" }
          : imapPassword
            ? { imap_password: imapPassword }
            : {}),
      } satisfies Schema<"MailSettingsPatch">);
      setSmtpPassword("");
      setImapPassword("");
      setClearSmtp(false);
      setClearImap(false);
      await refresh();
      setSaved(true);
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  return (
    <section
      className="correspondence mail-settings"
      aria-labelledby="mail-settings-heading"
    >
      <h2 id="mail-settings-heading">Supplier mail</h2>
      <p className="muted">
        Set the account used for quotation requests and supplier replies.
        Sending requires approval of each message.
      </p>
      <form onSubmit={save}>
        <fieldset disabled={pending}>
          <legend>Outgoing mail · SMTP</legend>
          <p className="connection-state">
            {settings.smtp_ready
              ? "SMTP configured · connection not tested"
              : "SMTP is not configured"}
          </p>
          <div className="mail-fields">
            <label>
              SMTP host
              <input
                value={account.smtp_host}
                maxLength={253}
                pattern="[A-Za-z0-9.-]*"
                onChange={(event) => changed({ smtp_host: event.target.value })}
              />
              <FieldError error={error} name="smtp_host" />
            </label>
            <label>
              SMTP port
              <input
                type="number"
                min={1}
                max={65535}
                required
                value={account.smtp_port}
                onChange={(event) =>
                  changed({ smtp_port: Number(event.target.value) })
                }
              />
              <FieldError error={error} name="smtp_port" />
            </label>
            <label>
              SMTP security
              <select
                value={account.smtp_security}
                onChange={(event) =>
                  changed({
                    smtp_security: event.target.value as "ssl" | "starttls",
                  })
                }
              >
                <option value="ssl">SSL / TLS</option>
                <option value="starttls">STARTTLS</option>
              </select>
              <FieldError error={error} name="smtp_security" />
            </label>
            <label>
              SMTP username
              <input
                value={account.smtp_username}
                maxLength={300}
                autoComplete="username"
                onChange={(event) =>
                  changed({ smtp_username: event.target.value })
                }
              />
              <FieldError error={error} name="smtp_username" />
            </label>
            <label>
              Sender email
              <input
                type="email"
                value={account.from_address}
                onChange={(event) =>
                  changed({ from_address: event.target.value })
                }
              />
              <FieldError error={error} name="from_address" />
            </label>
            <label>
              SMTP password
              <input
                type="password"
                autoComplete="new-password"
                value={smtpPassword}
                disabled={clearSmtp}
                onChange={(event) => {
                  setSmtpPassword(event.target.value);
                  setSaved(false);
                }}
              />
              <FieldError error={error} name="smtp_password" />
            </label>
          </div>
          <label className="correspondence-check">
            <input
              type="checkbox"
              checked={clearSmtp}
              onChange={(event) => {
                setClearSmtp(event.target.checked);
                setSaved(false);
              }}
            />
            Remove saved SMTP password when saving
          </label>
        </fieldset>
        <fieldset disabled={pending}>
          <legend>Incoming replies · IMAP SSL (optional)</legend>
          <p className="connection-state">
            {settings.imap_ready
              ? "IMAP configured · connection not tested"
              : "IMAP is not configured"}
          </p>
          <div className="mail-fields">
            <label>
              IMAP host
              <input
                value={account.imap_host}
                maxLength={253}
                pattern="[A-Za-z0-9.-]*"
                onChange={(event) => changed({ imap_host: event.target.value })}
              />
              <FieldError error={error} name="imap_host" />
            </label>
            <label>
              IMAP port
              <input
                type="number"
                min={1}
                max={65535}
                required
                value={account.imap_port}
                onChange={(event) =>
                  changed({ imap_port: Number(event.target.value) })
                }
              />
              <FieldError error={error} name="imap_port" />
            </label>
            <label>
              IMAP username
              <input
                value={account.imap_username}
                maxLength={300}
                onChange={(event) =>
                  changed({ imap_username: event.target.value })
                }
              />
              <FieldError error={error} name="imap_username" />
            </label>
            <label>
              Mailbox folder
              <input
                required
                value={account.imap_mailbox}
                maxLength={200}
                onChange={(event) =>
                  changed({ imap_mailbox: event.target.value })
                }
              />
              <FieldError error={error} name="imap_mailbox" />
            </label>
            <label>
              IMAP password
              <input
                type="password"
                autoComplete="new-password"
                value={imapPassword}
                disabled={clearImap}
                onChange={(event) => {
                  setImapPassword(event.target.value);
                  setSaved(false);
                }}
              />
              <FieldError error={error} name="imap_password" />
            </label>
          </div>
          <label className="correspondence-check">
            <input
              type="checkbox"
              checked={clearImap}
              onChange={(event) => {
                setClearImap(event.target.checked);
                setSaved(false);
              }}
            />
            Remove saved IMAP password when saving
          </label>
        </fieldset>
        <p className="field-help">
          Passwords are stored in this device’s credential store and never
          returned to this form. Leave a password blank to keep it. Your mail
          provider must support password or app-password access over TLS.
        </p>
        {settings.detail ? (
          <p className="field-help">{settings.detail}</p>
        ) : null}
        <ErrorNotice error={error} />
        {saved ? (
          <p role="status" className="success-text">
            Mail settings saved. Connections have not been tested.
          </p>
        ) : null}
        <button className="button primary" disabled={pending}>
          {pending ? "Saving…" : "Save mail settings"}
        </button>
      </form>
    </section>
  );
}
