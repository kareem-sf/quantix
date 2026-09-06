import { useState } from "react";
import { useApi, useRefresh, useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/ui";
import { Backups } from "./Backups";
import { MailSettings } from "./MailSettings";
import { Knowledge } from "./Knowledge";

export function Settings() {
  const settings = useResource<Schema<"Settings">>("/settings");
  if (settings.isPending) return <Loading>Loading settings…</Loading>;
  if (!settings.data) return <ErrorNotice error={settings.error} />;
  return <SettingsForm settings={settings.data} />;
}
function SettingsForm({ settings }: { settings: Schema<"Settings"> }) {
  const health = useResource<Schema<"Health">>("/health");
  const api = useApi(),
    refresh = useRefresh();
  const [key, setKey] = useState(""),
    [currency, setCurrency] = useState(settings.default_currency),
    [preferences, setPreferences] = useState(settings.preferences ?? "");
  const [pending, setPending] = useState(false),
    [saved, setSaved] = useState(false),
    [error, setError] = useState<unknown>(null);
  return (
    <div className="settings-page">
      <h1>Settings</h1>
      <p className="page-description">
        Your connection and working preferences.
      </p>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          setPending(true);
          setError(null);
          setSaved(false);
          try {
            await api.patch<Schema<"Settings">>("/settings", {
              default_currency: currency.toUpperCase(),
              preferences,
              ...(key.trim() ? { api_key: key.trim() } : {}),
            } satisfies Schema<"SettingsPatch">);
            setKey("");
            await refresh();
            setSaved(true);
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <section>
          <h2>AI connection</h2>
          <p className="connection-state">
            {settings.provider_ready
              ? "Configured · connection not tested"
              : "No API key configured"}
          </p>
          {settings.provider_detail ? (
            <p className="muted">{settings.provider_detail}</p>
          ) : null}
          <label>
            API key
            <input
              type="password"
              autoComplete="new-password"
              value={key}
              onChange={(event) => {
                setKey(event.target.value);
                setSaved(false);
              }}
              placeholder={
                settings.provider_ready
                  ? "Enter a key only to replace the saved key"
                  : "Enter your provider API key"
              }
            />
          </label>
          <p className="field-help">
            Saved keys are never returned to this form.
          </p>
          <label>
            Model
            <input readOnly value={settings.model} />
          </label>
        </section>
        <section>
          <h2>Working preferences</h2>
          <label>
            Default currency
            <input
              required
              pattern="[A-Za-z]{3}"
              maxLength={3}
              value={currency}
              onChange={(event) =>
                setCurrency(event.target.value.toUpperCase())
              }
            />
          </label>
          <label>
            Instructions for the office
            <textarea
              rows={4}
              maxLength={10000}
              value={preferences}
              onChange={(event) => setPreferences(event.target.value)}
              placeholder="Working preferences to apply when preparing tenders"
            />
          </label>
        </section>
        <section>
          <h2>Local files</h2>
          <p className="muted">
            Tender records and imported documents are stored on this device.
          </p>
          <label>
            Data folder
            <input readOnly value={settings.home} />
          </label>
        </section>
        <ErrorNotice error={error} />
        {saved ? (
          <p role="status" className="success-text">
            Settings saved. The connection has not been tested.
          </p>
        ) : null}
        <button className="button primary" disabled={pending}>
          {pending ? "Saving…" : "Save settings"}
        </button>
      </form>
      {health.data?.capabilities?.includes("knowledge") ? <Knowledge /> : null}
      {health.data?.capabilities?.includes("quotations") ? (
        <MailSettings />
      ) : null}
      {health.data?.capabilities?.includes("backups") ? <Backups /> : null}
    </div>
  );
}
