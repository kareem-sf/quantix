import { useId, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useApi, useRefresh, useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { FieldError as ApiFieldError } from "../components/FieldError";
import { TypographyH1, TypographyMuted } from "../components/typography";
import {
  availableSettingsSections,
  isSettingsSection,
  type SettingsSectionId,
} from "../app/settings-sections";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Backups } from "./Backups";
import { MailSettings } from "./MailSettings";
import { Knowledge } from "./Knowledge";
import { AIConnections } from "./AIConnections";
import { BenchmarkAdoptionReviewPanel } from "./BenchmarkAdoptionReviewPanel";
import { Diagnostics } from "./Diagnostics";
import { FactoryReset } from "./FactoryReset";
import { ManagerPersonality } from "./ManagerPersonality";
import { AgentLibrary } from "./AgentLibrary";
import { SandboxSettings } from "./SandboxSettings";
import { CompanyLibrary } from "./CompanyLibrary";
import { About } from "./About";
import { createDraftScope, useFormDraft } from "./useFormDraft";

export type SettingsSection = SettingsSectionId;

type SettingsProps = {
  section?: string;
  onSection?: (section: SettingsSection) => void;
  connectionId?: string;
  onConnectionClose?: () => void;
};

export function Settings(props: SettingsProps) {
  const settings = useResource<Schema<"Settings">>("/settings");
  if (settings.isPending) return <Loading>Loading settings…</Loading>;
  if (!settings.data) return <ErrorNotice error={settings.error} />;
  return <SettingsLayout settings={settings.data} {...props} />;
}

function SettingsLayout({
  settings,
  section,
  onSection,
  connectionId,
  onConnectionClose,
}: SettingsProps & { settings: Schema<"Settings"> }) {
  const health = useResource<Schema<"Health">>("/health");
  const api = useApi();
  const [localSection, setLocalSection] = useState<SettingsSection>("accounts");
  const [backupAttention, setBackupAttention] = useState(false);
  const capabilities = health.data?.capabilities ?? [];
  const backupsEnabled = capabilities.includes("backups");
  const pendingRestore = useQuery({
    queryKey: ["/backups/pending"],
    queryFn: ({ signal }) =>
      api.get<Schema<"RestoreReady"> | null>("/backups/pending", signal),
    enabled: backupsEnabled,
    retry: false,
  });
  const latestRestore = useQuery({
    queryKey: ["/backups/latest"],
    queryFn: ({ signal }) =>
      api.get<Schema<"RestoreOutcome"> | null>("/backups/latest", signal),
    enabled: backupsEnabled,
    retry: false,
  });
  const needsRecovery =
    !!pendingRestore.data ||
    !!pendingRestore.error ||
    !!latestRestore.error ||
    backupAttention;
  const sections = availableSettingsSections(capabilities);
  const active =
    isSettingsSection(section) && sections.some((item) => item.id === section)
      ? section
      : localSection;
  const select = (next: SettingsSection) => {
    setLocalSection(next);
    onSection?.(next);
  };

  return (
    <div className="@container flex flex-col gap-8">
      <header className="flex flex-col gap-1">
        <TypographyH1>Settings</TypographyH1>
        <TypographyMuted>
          Accounts, knowledge and data for your Tender Office.
        </TypographyMuted>
      </header>

      {/* Section navigation lives in the app sidebar while Settings is open. */}
      <section className="flex min-w-0 flex-col gap-6">
        <ErrorNotice error={health.error} />
        {needsRecovery && active !== "backups" ? (
          <Alert>
            <AlertDescription className="flex flex-wrap items-center gap-2">
              A backup or recovery needs review.
              <Button
                variant="link"
                className="h-auto p-0"
                onClick={() => select("backups")}
              >
                Open recovery
              </Button>
            </AlertDescription>
          </Alert>
        ) : null}
        {active === "preferences" ? <Preferences settings={settings} /> : null}
        {active === "diagnostics" ? (
          <DiagnosticsSection home={settings.home} />
        ) : null}
        {/* Sections below have not been rebuilt on shadcn yet. */}
        {active !== "preferences" && active !== "diagnostics" ? (
          <div className="legacy-screen">
            {active === "accounts" ? (
              <div className="space-y-6">
                <AIConnections
                  connectionId={connectionId}
                  onConnectionClose={onConnectionClose}
                />
                <BenchmarkAdoptionReviewPanel />
              </div>
            ) : null}
            {active === "manager" ? <ManagerPersonality /> : null}
            {active === "agents" ? <AgentLibrary /> : null}
            {active === "code" ? <SandboxSettings /> : null}
            {active === "knowledge" ? <Knowledge /> : null}
            {active === "library" ? <CompanyLibrary /> : null}
            {active === "mail" ? <MailSettings /> : null}
            {active === "backups" ? (
              <Backups onAttention={setBackupAttention} />
            ) : null}
            {active === "reset" ? (
              <FactoryReset
                onOpenBackups={
                  backupsEnabled ? () => select("backups") : undefined
                }
              />
            ) : null}
            {active === "about" ? <About /> : null}
          </div>
        ) : null}
      </section>
    </div>
  );
}

function DiagnosticsSection({ home }: { home: string }) {
  const homeId = useId();
  return (
    <div className="flex flex-col gap-6">
      <div className="legacy-screen">
        <Diagnostics open />
      </div>
      <Card>
        <CardHeader>
          <CardTitle>Local files</CardTitle>
          <CardDescription>
            Tender records and preserved imported files are stored on this
            device.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Field>
            <FieldLabel htmlFor={homeId}>Data folder</FieldLabel>
            <Input
              id={homeId}
              readOnly
              value={home}
              className="font-mono text-xs"
            />
          </Field>
        </CardContent>
      </Card>
    </div>
  );
}

function Preferences({ settings }: { settings: Schema<"Settings"> }) {
  const api = useApi(),
    refresh = useRefresh();
  const currencyId = useId();
  const preferencesId = useId();
  const draft = useFormDraft(
    createDraftScope("settings", "office", "preferences", 1),
    {
      currency: settings.default_currency,
      preferences: settings.preferences ?? "",
    },
    ["currency", "preferences"],
  );
  const [pending, setPending] = useState(false),
    [saved, setSaved] = useState(false),
    [error, setError] = useState<unknown>(null);
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          <h2>Working preferences</h2>
        </CardTitle>
        <CardDescription>
          Tell the Tender Manager how you prepare tenders.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form
          onSubmit={async (event) => {
            event.preventDefault();
            if (pending) return;
            const revision = draft.revision;
            setPending(true);
            setError(null);
            setSaved(false);
            try {
              await api.patch<Schema<"Settings">>("/settings", {
                default_currency: draft.value.currency.toUpperCase(),
                preferences: draft.value.preferences,
              } satisfies Schema<"SettingsPatch">);
              draft.markAccepted(revision);
              await refresh();
              setSaved(true);
            } catch (failure) {
              setError(failure);
            } finally {
              setPending(false);
            }
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={currencyId}>Default currency</FieldLabel>
              <Input
                id={currencyId}
                required
                pattern="[A-Za-z]{3}"
                maxLength={3}
                className="w-28 uppercase"
                value={draft.value.currency}
                onChange={(event) => {
                  draft.setField("currency", event.target.value.toUpperCase());
                  setSaved(false);
                }}
              />
              <FieldDescription>
                The three-letter code used for new estimates.
              </FieldDescription>
              <ApiFieldError error={error} name="default_currency" />
            </Field>
            <Field>
              <FieldLabel htmlFor={preferencesId}>
                Instructions for the office
              </FieldLabel>
              <Textarea
                id={preferencesId}
                rows={6}
                maxLength={10000}
                value={draft.value.preferences}
                onChange={(event) => {
                  draft.setField("preferences", event.target.value);
                  setSaved(false);
                }}
                placeholder="Working preferences to apply when preparing tenders"
              />
              <ApiFieldError error={error} name="preferences" />
            </Field>
            <ErrorNotice error={error || draft.error} />
            <div className="flex items-center gap-3">
              <Button type="submit" disabled={pending}>
                {pending ? "Saving…" : "Save preferences"}
              </Button>
              {saved ? (
                <p role="status" className="text-sm text-muted-foreground">
                  Working preferences saved.
                </p>
              ) : null}
            </div>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  );
}
