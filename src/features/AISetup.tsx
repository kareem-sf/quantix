import { MicroButton } from "@/components/ui/micro-button";
import { FieldError } from "../components/FieldError";
import { useCallback, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Check, ChevronLeft, ChevronRight, Plus, Sparkles } from "lucide-react";
import { useApi, useRefresh, useResource, type Schema } from "../api";
import { ErrorNotice, Loading, Modal } from "../components/common";
import { ConnectionForm, ConnectionModels } from "./AIAdvancedConnections";
import { ExternalLink, webLink } from "../components/ExternalLink";
import { prepareSignInBrowser } from "../browser";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { cn } from "@/lib/utils";
import { ProviderLogo } from "@/components/ui/provider-logo";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";

type Account = Schema<"SetupAccount">;
type AccountView = Account;
type Service = Schema<"SetupService">;
type Method = Schema<"SetupMethod">;
type ActionBase = Schema<"SetupAction">;
type Action = ActionBase;
type ActionInput = Omit<Action, "accept_unknown_cost"> &
  Partial<Pick<Action, "accept_unknown_cost">>;
type Preview = Schema<"SetupCheckPreview">;

const directProviders = new Set([
  "openai",
  "anthropic",
  "google",
  "xai",
  "custom",
]);
const directMethods = new Set([
  "openai",
  "anthropic",
  "google",
  "xai",
  "custom",
]);
const directProtocols = new Set([
  "openai_responses",
  "openai_chat",
  "anthropic",
  "google",
]);
const subscriptionMethods = new Set(["codex", "grok_build"]);
const subscriptionProviders = new Set(["openai", "xai", "codex", "grok_build"]);
const accountPath = (id: string) =>
  `/ai/setup/accounts/${encodeURIComponent(id)}`;
const setupAction = (value: ActionInput): Action => ({
  accept_unknown_cost: false,
  ...value,
});

export function billingDescription(
  billing: Schema<"ConnectionRecord">["billing"],
) {
  return billing === "subscription"
    ? "Subscription account"
    : billing === "metered"
      ? "Paid by API usage"
      : "Cost needs review";
}

export function connectionBillingDescription(
  connection: Schema<"ConnectionRecord">,
) {
  if (connection.protocol === "grok_build")
    return "Grok subscription allowance";
  return billingDescription(connection.billing);
}

export function GrokSpendingNote({
  connection,
}: {
  connection: Schema<"ConnectionRecord">;
}) {
  if (connection.protocol !== "grok_build") return null;
  return (
    <div className="ai-grok-spending-note">
      <p className="field-help">
        <strong>
          Grok spending:{" "}
          {connection.settings.allow_provider_managed_extras === true
            ? "Provider-managed extras allowed"
            : "Subscription allowance only"}
          .
        </strong>
      </p>
      <p className="field-help">
        Extra credits and automatic top-up follow your Grok account. Quantix
        does not promise a separate Tender cap; keep paid extras off unless you
        explicitly approve them for the Tender.
      </p>
    </div>
  );
}

export function ProviderUsageDetails({
  record,
}: {
  record: Pick<
    Schema<"AIUsageRecord">,
    | "provider_reported_cost_usd"
    | "provider_cost_is_partial"
    | "provider_usage_is_incomplete"
    | "cached_input_tokens"
  >;
}) {
  // Show only what the provider actually reported; empty facts are noise.
  const facts = [
    record.provider_reported_cost_usd != null
      ? [
          "Provider-reported cost",
          `USD ${record.provider_reported_cost_usd}${record.provider_cost_is_partial === true ? " · Partial" : ""}`,
        ]
      : null,
    record.cached_input_tokens
      ? ["Cached input tokens", record.cached_input_tokens.toLocaleString()]
      : null,
    record.provider_usage_is_incomplete === true
      ? ["Provider usage record", "Incomplete"]
      : null,
  ].filter((fact): fact is [string, string] => fact !== null);
  if (!facts.length) return null;
  return (
    <div className="ai-provider-usage">
      <dl className="ai-usage-facts">
        {facts.map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
      {record.provider_reported_cost_usd != null ? (
        <p className="field-help">
          Provider-reported cost is separate from Quantix’s estimate and may
          differ from the final bill.
        </p>
      ) : null}
    </div>
  );
}

export function dataDestination(connection: Schema<"ConnectionRecord">) {
  let host = "";
  try {
    host = connection.base_url ? new URL(connection.base_url).hostname : "";
  } catch {
    /* Keep the connection's safe display fields when an older record is malformed. */
  }
  const local = ["localhost", "127.0.0.1", "[::1]"].includes(host);
  const upstream = Array.isArray(connection.settings.upstream_providers)
    ? connection.settings.upstream_providers.filter(
        (item): item is string => typeof item === "string",
      )
    : [];
  const region =
    typeof connection.settings.region === "string"
      ? connection.settings.region
      : typeof connection.settings.location === "string"
        ? connection.settings.location
        : "";
  const providerNames: Record<string, string> = {
    codex: "OpenAI through your ChatGPT account",
    grok_build: "xAI Grok through your Grok account",
    google: "Google Gemini",
  };
  const project =
    typeof connection.settings.project === "string"
      ? connection.settings.project
      : "";
  return `${local ? "The model server on this device" : host || providerNames[connection.provider_id] || connection.provider_id}${project ? ` · project ${project}` : ""}${region ? ` · ${region}` : ""}${upstream.length ? ` · onward to ${upstream.join(", ")}` : ""}`;
}

export function accountStageLabel(account: Account) {
  if (account.supported === false) return "Older setup";
  if (!account.connection.enabled) return "Paused";
  const subscription = isSubscriptionAccount(account);
  const labels: Record<Account["stage"], string> = {
    needs_preparation: subscription
      ? account.software.state === "attention"
        ? "Needs client repair"
        : "Needs client preparation"
      : "Ready to connect",
    preparing: subscription ? "Preparing official client" : "Connecting",
    needs_credentials: subscription ? "Sign in to provider" : "Add API key",
    needs_sign_in: subscription ? "Sign in to provider" : "Add API key",
    discovering: "Reading models",
    choose_model: "Choose a model",
    ready_to_check: "Ready to check",
    checking: "Checking connection",
    ready: "Ready",
    attention: "Needs attention",
    cancelled: "Setup paused",
  };
  return labels[account.stage];
}

export function accountNextAction(account: Account) {
  if (account.supported === false)
    return "Review this saved account or remove it.";
  if (!account.connection.enabled)
    return "Enable this account in More options.";
  const subscription = isSubscriptionAccount(account);
  const actions: Record<Account["stage"], string> = {
    needs_preparation: subscription
      ? account.software.state === "attention"
        ? "Repair the official client."
        : "Prepare the official client."
      : "Connect an API key.",
    preparing: subscription
      ? "Wait while the official client is prepared."
      : "Wait while the connection is checked.",
    needs_credentials: subscription
      ? "Sign in through the official client."
      : "Save an API key.",
    needs_sign_in: subscription
      ? "Sign in through the official client."
      : "Save an API key.",
    discovering: "Wait while Quantix reads the provider model list.",
    choose_model: "Choose or enter a model ID.",
    ready_to_check: "Check this connection.",
    checking: "Wait for the connection check to finish.",
    ready: "Choose it from the AI button in a Tender's message box.",
    attention: "Review the issue, then try again.",
    cancelled: "Continue this connection.",
  };
  return actions[account.stage];
}

function isDirectMethod(method: Method) {
  return (
    method.access_kind !== "subscription" &&
    directMethods.has(method.id) &&
    directProviders.has(method.provider_id)
  );
}

function isSubscriptionMethod(method: Method) {
  return (
    method.access_kind === "subscription" ||
    (subscriptionMethods.has(method.id) &&
      subscriptionProviders.has(method.provider_id))
  );
}

function isDirectAccount(account: Account) {
  return (
    directProviders.has(account.connection.provider_id) &&
    directProtocols.has(account.connection.protocol) &&
    (account.connection.auth_type === "api_key" ||
      account.connection.auth_type === "environment")
  );
}

function isSubscriptionAccount(account: Account) {
  return (
    (account.access_kind === "subscription" ||
      account.connection.billing === "subscription") &&
    account.connection.auth_type === "client_login" &&
    ((account.connection.provider_id === "codex" &&
      account.connection.protocol === "codex") ||
      (account.connection.provider_id === "grok_build" &&
        account.connection.protocol === "grok_build"))
  );
}

function isSupported(account: AccountView) {
  return account.supported;
}

export function AISetup({
  headingId = "ai-setup-heading",
  connectionId,
  onConnectionClose,
}: {
  headingId?: string;
  connectionId?: string;
  onConnectionClose?: () => void;
} = {}) {
  const accounts = useResource<AccountView[]>("/ai/setup/accounts", true);
  const health = useResource<Schema<"Health">>("/health");
  const [adding, setAdding] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const closeAdd = useCallback(() => setAdding(false), []);
  const closeAccount = useCallback(() => {
    setSelected(null);
    onConnectionClose?.();
  }, [onConnectionClose]);
  useEffect(() => {
    if (connectionId) setSelected(connectionId);
  }, [connectionId]);
  const setupAvailable = (health.data?.ai_setup_revision ?? 0) >= 7;
  const active =
    accounts.data?.filter(
      (account) =>
        isSupported(account) &&
        (isDirectAccount(account) || isSubscriptionAccount(account)),
    ) ?? [];
  const retired =
    accounts.data?.filter(
      (account) =>
        !isSupported(account) ||
        (!isDirectAccount(account) && !isSubscriptionAccount(account)),
    ) ?? [];
  const addTitle =
    health.data && !setupAvailable
      ? "Restart Quantix to finish updating AI connections."
      : undefined;
  const addButton = (
    <Button
      type="button"
      size="sm"
      disabled={!setupAvailable}
      title={addTitle}
      onClick={() => setAdding(true)}
    >
      <Plus data-icon="inline-start" />
      Add account
    </Button>
  );
  return (
    <section className="flex min-w-0 flex-col gap-4">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="min-w-0">
          <h2 id={headingId} className="text-base font-medium">
            AI accounts
          </h2>
          <p className="text-sm text-muted-foreground">
            The AI that does your tender work. Any provider with an API key, or
            an eligible subscription.
          </p>
        </div>
        {active.length ? addButton : null}
      </div>
      <ErrorNotice error={accounts.error || health.error} />
      {accounts.isPending ? <Loading>Loading AI accounts…</Loading> : null}
      {active.length === 0 && !accounts.isPending ? (
        <Empty className="border">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <Sparkles />
            </EmptyMedia>
            <EmptyTitle>No AI connected yet</EmptyTitle>
            <EmptyDescription>
              Paste an API key from OpenAI, Anthropic, Google, xAI or any
              OpenAI-compatible provider. Quantix finds the models for you.
            </EmptyDescription>
          </EmptyHeader>
          <EmptyContent>{addButton}</EmptyContent>
        </Empty>
      ) : (
        <ul className="divide-y overflow-hidden rounded-xl border bg-card">
          {active.map((account) => (
            <AccountCard
              account={account}
              key={account.id}
              onOpen={() => setSelected(account.id)}
            />
          ))}
        </ul>
      )}
      {retired.length ? (
        <details className="text-sm">
          <summary className="w-fit cursor-pointer text-xs text-muted-foreground">
            Older connections ({retired.length})
          </summary>
          <ul className="mt-2 divide-y overflow-hidden rounded-xl border bg-card">
            {retired.map((account) => (
              <AccountCard
                account={account}
                retired
                key={account.id}
                onOpen={() => setSelected(account.id)}
              />
            ))}
          </ul>
        </details>
      ) : null}
      {adding && setupAvailable ? (
        <AddAI
          onClose={closeAdd}
          onStarted={(id) => {
            setAdding(false);
            setSelected(id);
          }}
        />
      ) : null}
      {selected ? (
        <AccountSetup
          key={selected}
          accountId={selected}
          onClose={closeAccount}
        />
      ) : null}
    </section>
  );
}

/** One line per account: who it is, what it uses, and whether it is ready. */
function AccountCard({
  account,
  retired = false,
  onOpen,
}: {
  account: AccountView;
  retired?: boolean;
  onOpen: () => void;
}) {
  const model = account.models.find(
    (item) => item.model_id === account.selected_model_id,
  );
  const ready =
    !retired && account.stage === "ready" && account.connection.enabled;
  const detail = retired
    ? "Kept for inspection only"
    : ready
      ? (model?.display_name ?? account.selected_model_id ?? "")
      : accountNextAction(account);
  return (
    <li>
      <button
        type="button"
        onClick={onOpen}
        className="flex w-full min-w-0 items-center gap-3 px-4 py-3 text-start transition-colors hover:bg-muted/50"
      >
        <span
          aria-hidden="true"
          className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted"
        >
          <ProviderLogo
            providerId={account.connection.provider_id}
            name={account.service_title}
          />
        </span>
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-sm font-medium">
            {account.connection.name}
          </span>
          <span className="truncate text-xs text-muted-foreground">
            {detail}
          </span>
        </span>
        <span
          className={cn(
            "flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-xs",
            ready
              ? "bg-(--success-tint) text-(--success)"
              : account.stage === "attention"
                ? "bg-amber-500/10 text-amber-700 dark:text-amber-400"
                : "bg-muted text-muted-foreground",
          )}
        >
          {ready ? <Check className="size-3.5" /> : null}
          {retired ? "Older setup" : accountStageLabel(account)}
        </span>
        <ChevronRight
          aria-hidden
          className="size-4 shrink-0 text-muted-foreground/60 rtl:-scale-x-100"
        />
      </button>
    </li>
  );
}

/**
 * Adding an account is one screen: choose the provider, paste the key, connect.
 * Quantix looks up the models itself, so nothing else is asked for.
 */
function AddAI({
  onClose,
  onStarted,
}: {
  onClose: () => void;
  onStarted: (id: string) => void;
}) {
  const services = useResource<Service[]>("/ai/setup/services");
  const presets = useResource<Schema<"ProviderPreset">[]>("/ai/providers");
  const api = useApi();
  const refresh = useRefresh();
  const [service, setService] = useState<Service | null>(null);
  const [method, setMethod] = useState<Method | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const directServices = (services.data ?? [])
    .map((item) => ({
      ...item,
      methods: item.methods.filter(
        (candidate) =>
          isDirectMethod(candidate) || isSubscriptionMethod(candidate),
      ),
    }))
    .filter((item) => directProviders.has(item.id) && item.methods.length);

  async function start(
    item: Service,
    selected: Method,
    connection?: Schema<"ConnectionInput">,
  ) {
    if (busy || !selected.available) return;
    setBusy(true);
    setError(null);
    try {
      const account = await api.post<Account>("/ai/setup/accounts", {
        service_id: item.id,
        method_id: selected.id,
        ...(connection ? { connection, name: connection.name } : {}),
      } satisfies Schema<"SetupStart">);
      await refresh();
      onStarted(account.id);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }

  function choose(item: Service) {
    setService(item);
    // Only a provider that also offers a subscription has anything to ask.
    setMethod(item.methods.length === 1 ? item.methods[0] : null);
  }

  return (
    <Modal
      legacy={false}
      title={service?.title ?? "Add an AI account"}
      onClose={onClose}
    >
      <div className="flex min-w-0 flex-col gap-4">
        <ErrorNotice error={services.error || presets.error || error} />
        {services.isPending ? <Loading>Loading providers…</Loading> : null}
        {!service ? (
          <div className="grid gap-2 @sm:grid-cols-2">
            {directServices.map((item) => (
              <button
                type="button"
                key={item.id}
                onClick={() => choose(item)}
                className="flex min-w-0 items-center gap-3 rounded-xl border p-3 text-start transition-colors hover:bg-muted/50"
              >
                <span
                  aria-hidden="true"
                  className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted"
                >
                  <ProviderLogo providerId={item.id} name={item.title} />
                </span>
                <span className="flex min-w-0 flex-col">
                  <span className="truncate text-sm font-medium">
                    {item.title}
                  </span>
                  <span className="truncate text-xs text-muted-foreground">
                    {item.methods.some(isSubscriptionMethod)
                      ? "API key or subscription"
                      : "API key"}
                  </span>
                </span>
              </button>
            ))}
          </div>
        ) : (
          <>
            <Button
              type="button"
              variant="link"
              size="sm"
              className="h-auto w-fit p-0 text-muted-foreground"
              disabled={busy}
              onClick={() => {
                setService(null);
                setMethod(null);
              }}
            >
              <ChevronLeft
                data-icon="inline-start"
                className="rtl:-scale-x-100"
              />
              All providers
            </Button>
            {!method ? (
              <div className="flex flex-col gap-2">
                {service.methods.map((candidate) => (
                  <div key={candidate.id} className="flex flex-col gap-1">
                    <button
                      type="button"
                      disabled={busy || !candidate.available}
                      onClick={() => setMethod(candidate)}
                      className="flex min-w-0 items-center justify-between gap-3 rounded-xl border p-3 text-start transition-colors enabled:hover:bg-muted/50 disabled:opacity-60"
                    >
                      <span className="flex min-w-0 flex-col">
                        <span className="text-sm font-medium">
                          {candidate.title}
                        </span>
                        <span className="text-xs text-muted-foreground">
                          {candidate.detail}
                        </span>
                      </span>
                      <ChevronRight
                        aria-hidden
                        className="size-4 shrink-0 text-muted-foreground/60 rtl:-scale-x-100"
                      />
                    </button>
                    {!candidate.available ? (
                      <p className="text-xs text-muted-foreground">
                        {candidate.unavailable_reason}
                      </p>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : isSubscriptionMethod(method) ? (
              <div className="flex flex-col items-start gap-3">
                <p className="text-sm text-muted-foreground">
                  Sign in through the provider&rsquo;s own client on the next
                  screen. Quantix never copies subscription credentials into API
                  fields.
                </p>
                <Button
                  type="button"
                  disabled={busy}
                  onClick={() => void start(service, method)}
                >
                  {busy ? "Saving…" : "Continue"}
                </Button>
              </div>
            ) : (
              <ConnectForm
                service={service}
                method={method}
                preset={presets.data?.find(
                  (item) => item.id === method.provider_id,
                )}
                busy={busy}
                onConnect={(connection) =>
                  void start(service, method, connection)
                }
              />
            )}
          </>
        )}
      </div>
    </Modal>
  );
}

/** A readable account name from the address the engineer pasted. */
function nameFromAddress(address: string) {
  try {
    const host = new URL(
      /^https?:\/\//i.test(address) ? address : `https://${address}`,
    ).hostname;
    const label = host
      .replace(/^www\./, "")
      .replace(/^api\./, "")
      .split(".")[0];
    return label ? label.charAt(0).toUpperCase() + label.slice(1) : "";
  } catch {
    return "";
  }
}

/** Address and key in one place; everything else has a sensible default. */
function ConnectForm({
  service,
  method,
  preset,
  busy,
  onConnect,
}: {
  service: Service;
  method: Method;
  preset?: Schema<"ProviderPreset">;
  busy: boolean;
  onConnect: (connection: Schema<"ConnectionInput">) => void;
}) {
  const ownEndpoint = method.requires_details;
  const [name, setName] = useState(ownEndpoint ? "" : service.title);
  const [address, setAddress] = useState("");
  const [key, setKey] = useState("");
  const [protocol, setProtocol] = useState<
    Schema<"ConnectionInput">["protocol"]
  >(ownEndpoint ? "openai_chat" : (preset?.default_protocol ?? "openai_chat"));
  const [sessionOnly, setSessionOnly] = useState(false);
  useEffect(() => {
    if (!ownEndpoint && preset?.default_protocol)
      setProtocol(preset.default_protocol);
  }, [ownEndpoint, preset?.default_protocol]);
  // An account the engineer does not name is named after its address.
  const chosenName =
    name.trim() || (ownEndpoint ? nameFromAddress(address) : service.title);
  const valid =
    !!key.trim() && !!chosenName && (!ownEndpoint || !!address.trim());
  return (
    <form
      className="flex min-w-0 flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (!valid || busy) return;
        onConnect({
          name: chosenName,
          provider_id: method.provider_id,
          protocol,
          auth_type: "api_key",
          billing: preset?.billing ?? (ownEndpoint ? "unknown" : "metered"),
          enabled: true,
          base_url: ownEndpoint ? address.trim() : (preset?.base_url ?? null),
          settings: {},
          session_only: sessionOnly,
          allow_insecure_http: false,
          credentials: { api_key: key.trim() },
        } satisfies Schema<"ConnectionInput">);
      }}
    >
      <fieldset className="flex min-w-0 flex-col gap-4" disabled={busy}>
        {ownEndpoint ? (
          <label className="flex min-w-0 flex-col gap-1.5 text-sm">
            API address
            <Input
              required
              type="text"
              maxLength={2000}
              value={address}
              onChange={(event) => setAddress(event.target.value)}
              placeholder="https://api.example.com/v1"
            />
            <span className="text-xs text-muted-foreground">
              Any provider that speaks the OpenAI API works here. Paste the
              provider&rsquo;s website if you don&rsquo;t know the exact
              address; Quantix looks for the API on it.
            </span>
          </label>
        ) : null}
        <label className="flex min-w-0 flex-col gap-1.5 text-sm">
          API key
          <Input
            required
            type="password"
            autoComplete="off"
            value={key}
            onChange={(event) => setKey(event.target.value)}
            placeholder="Paste your API key"
          />
          <span className="text-xs text-muted-foreground">
            Kept in this device&rsquo;s credential store. Quantix never shows it
            again.
          </span>
        </label>
        <details className="text-sm">
          <summary className="w-fit cursor-pointer text-xs text-muted-foreground">
            More options
          </summary>
          <div className="mt-3 flex min-w-0 flex-col gap-3">
            <label className="flex min-w-0 flex-col gap-1.5 text-sm">
              Account name
              <Input
                maxLength={150}
                value={name}
                placeholder={chosenName || service.title}
                onChange={(event) => setName(event.target.value)}
              />
            </label>
            {ownEndpoint ? (
              <label className="flex min-w-0 flex-col gap-1.5 text-sm">
                API style
                <NativeSelect
                  className="w-full"
                  value={protocol}
                  onChange={(event) =>
                    setProtocol(
                      event.target
                        .value as Schema<"ConnectionInput">["protocol"],
                    )
                  }
                >
                  <NativeSelectOption value="openai_chat">
                    Chat completions (most providers)
                  </NativeSelectOption>
                  <NativeSelectOption value="openai_responses">
                    Responses (needed for online research)
                  </NativeSelectOption>
                </NativeSelect>
              </label>
            ) : null}
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                type="checkbox"
                className="size-4"
                checked={sessionOnly}
                onChange={(event) => setSessionOnly(event.target.checked)}
              />
              Keep this key only until Quantix closes
            </label>
          </div>
        </details>
      </fieldset>
      <div className="flex flex-wrap items-center gap-3">
        <Button type="submit" disabled={busy || !valid}>
          {busy ? "Connecting…" : "Connect"}
        </Button>
        {!ownEndpoint && webLink(method.docs_url) ? (
          <ExternalLink
            className="text-xs text-muted-foreground underline underline-offset-4"
            href={webLink(method.docs_url)!}
          >
            Where to find the key
          </ExternalLink>
        ) : null}
      </div>
    </form>
  );
}

export function AccountSetup({
  accountId,
  onClose,
}: {
  accountId: string;
  onClose: () => void;
}) {
  const api = useApi();
  const client = useQueryClient();
  const refresh = useRefresh();
  const path = accountPath(accountId);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const account = useQuery<AccountView>({
    queryKey: [path],
    queryFn: ({ signal }) => api.get<AccountView>(path, signal),
    enabled: !busy,
    staleTime: 0,
    refetchOnMount: "always",
    refetchInterval: busy ? false : 1500,
  });
  const refreshAccount = useCallback(() => {
    void account.refetch();
  }, [account.refetch]);
  const act = async (action: Action) => {
    if (busy || (account.data?.active && action.action !== "cancel")) return;
    setBusy(true);
    setError(null);
    try {
      await client.cancelQueries({ queryKey: [path], exact: true });
      if (
        action.action === "sign_in" &&
        account.data?.connection.protocol === "grok_build"
      ) {
        await prepareSignInBrowser(account.data.connection.protocol);
      }
      const result = await api.post<AccountView>(`${path}/actions`, action);
      client.setQueryData([path], result);
      await refresh();
    } catch (failure) {
      setError(failure);
      void account.refetch();
    } finally {
      setBusy(false);
    }
  };
  const item = account.data;
  if (!item)
    return (
      <Modal title="AI account" onClose={onClose}>
        <ErrorNotice error={account.error || error} />
        {account.isPending ? <Loading>Loading account…</Loading> : null}
      </Modal>
    );
  if (item.supported === false)
    return <RetiredAccount account={item} onClose={onClose} />;
  if (isSubscriptionAccount(item))
    return (
      <SubscriptionAccount
        account={item}
        busy={busy}
        error={account.error || error}
        onAction={act}
        onClose={onClose}
      />
    );
  if (!isDirectAccount(item))
    return <RetiredAccount account={item} onClose={onClose} />;
  return (
    <DirectAccount
      account={item}
      busy={busy}
      error={account.error || error}
      onAction={act}
      onRefresh={refreshAccount}
      onClose={onClose}
      onSaved={(saved) => {
        client.setQueryData([path], saved);
        void refresh();
      }}
    />
  );
}

function SubscriptionAccount({
  account,
  busy,
  error,
  onAction,
  onClose,
}: {
  account: AccountView;
  busy: boolean;
  error: unknown;
  onAction: (action: Action) => Promise<void>;
  onClose: () => void;
}) {
  const [advanced, setAdvanced] = useState(false);
  const retryableCheck =
    account.stage === "attention" &&
    !!account.selected_model_id &&
    ["failed", "interrupted"].includes(account.check.status);
  const canCheck =
    !!account.selected_model_id &&
    (["ready_to_check", "ready"].includes(account.stage) || retryableCheck);
  const active = account.active;
  return (
    <Modal legacy={false} title={account.connection.name} onClose={onClose}>
      <div className="flex min-w-0 flex-col gap-4">
        <ErrorNotice error={error} />
        <p className="text-xs text-muted-foreground">
          {account.service_title} · Official subscription
        </p>
        <div
          className={cn(
            "flex min-w-0 flex-col gap-1 rounded-xl border p-3",
            account.stage === "ready"
              ? "border-emerald-500/40 bg-emerald-500/5"
              : "bg-muted/40",
          )}
          role="status"
        >
          <div className="flex items-center gap-2">
            <span
              aria-hidden="true"
              className={cn(
                "size-2 shrink-0 rounded-full",
                account.stage === "ready"
                  ? "bg-emerald-500"
                  : account.stage === "attention"
                    ? "bg-amber-500"
                    : "bg-sky-500",
              )}
            />
            <h3 className="text-sm font-medium">
              {subscriptionStageLabel(account)}
            </h3>
          </div>
          <p className="text-sm text-muted-foreground">{account.detail}</p>
          {account.stage !== "ready" ? (
            <p className="text-xs text-muted-foreground">
              <strong className="font-medium text-foreground">Next:</strong>{" "}
              {subscriptionNextAction(account)}
            </p>
          ) : null}
          {active ? <Loading>Working…</Loading> : null}
        </div>
        {active ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="w-fit"
            disabled={busy}
            onClick={() => void onAction(setupAction({ action: "cancel" }))}
          >
            Cancel
          </Button>
        ) : null}
        {account.stage === "needs_preparation" ||
        account.stage === "cancelled" ||
        (account.stage === "attention" &&
          account.software.state !== "ready") ? (
          <Button
            type="button"
            className="w-fit"
            disabled={busy || active}
            onClick={() =>
              void onAction(
                setupAction({
                  action:
                    account.software.state === "attention"
                      ? "repair"
                      : "prepare",
                }),
              )
            }
          >
            {account.stage === "cancelled"
              ? "Continue setup"
              : account.software.state === "attention"
                ? "Repair official client"
                : "Prepare connection"}
          </Button>
        ) : null}
        {account.stage === "needs_sign_in" ? (
          <SubscriptionSignIn
            account={account}
            busy={busy || active}
            onAction={onAction}
          />
        ) : null}
        {account.models.length > 0 &&
        !["needs_sign_in", "needs_preparation", "preparing"].includes(
          account.stage,
        ) ? (
          <SubscriptionModelChoice
            account={account}
            disabled={busy || active}
            onSelect={(modelId) =>
              onAction(
                setupAction({ action: "select_model", model_id: modelId }),
              )
            }
          />
        ) : null}
        {retryableCheck ? (
          <div className="flex flex-col items-start gap-2 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3">
            <p className="text-sm text-muted-foreground">
              The last check failed. Sign in again or correct the subscription
              connection before retrying.
            </p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() =>
                void onAction(
                  setupAction({ action: "sign_in", sign_in_method: "browser" }),
                )
              }
            >
              Sign in again
            </Button>
          </div>
        ) : null}
        {canCheck ? (
          <SubscriptionCheck
            account={account}
            busy={busy || active}
            onAction={onAction}
            onCheck={(preview) =>
              onAction(
                setupAction({
                  action: "check",
                  check_fingerprint: preview.fingerprint,
                  maximum_cost_usd: preview.maximum_cost_usd,
                }),
              )
            }
          />
        ) : null}
        {account.stage === "attention" &&
        account.connection.protocol === "grok_build" &&
        !canCheck ? (
          <GrokRecovery
            account={account}
            busy={busy || active}
            onAction={onAction}
          />
        ) : null}
        {account.stage === "attention" &&
        account.connection.protocol === "codex" &&
        !retryableCheck &&
        account.software.state === "ready" ? (
          <SubscriptionSignIn
            account={account}
            busy={busy || active}
            onAction={onAction}
            retry
          />
        ) : null}
        {account.stage === "ready" ? (
          <Button
            type="button"
            className="w-fit"
            disabled={busy}
            onClick={onClose}
          >
            Done
          </Button>
        ) : null}
        <details
          className="border-t pt-3"
          open={advanced}
          onToggle={(event) => setAdvanced(event.currentTarget.open)}
        >
          <summary className="w-fit cursor-pointer text-xs text-muted-foreground">
            More options
          </summary>
          {advanced ? (
            <div className="legacy-screen mt-3">
              <SubscriptionOptions
                account={account}
                busy={busy || active}
                onAction={onAction}
                onRemoved={onClose}
              />
            </div>
          ) : null}
        </details>
      </div>
    </Modal>
  );
}

function subscriptionStageLabel(account: AccountView) {
  if (account.stage === "needs_preparation" || account.stage === "preparing")
    return "Preparing connection";
  if (account.stage === "needs_sign_in") return "Sign in required";
  if (account.stage === "discovering") return "Reading models";
  if (account.stage === "choose_model") return "Choose a model";
  if (account.stage === "ready_to_check") return "Ready to check";
  if (account.stage === "checking") return "Checking connection";
  if (account.stage === "ready") return "Ready";
  if (account.stage === "cancelled") return "Setup paused";
  return "Needs attention";
}

function subscriptionNextAction(account: AccountView) {
  if (account.stage === "needs_sign_in")
    return "Complete sign-in through the official provider page.";
  if (account.stage === "discovering")
    return "Wait while the official client reads available models.";
  if (account.stage === "choose_model") return "Choose a model.";
  if (account.stage === "ready_to_check") return "Check this subscription.";
  if (account.stage === "ready")
    return "Choose it from the AI button in a Tender's message box.";
  if (account.stage === "attention")
    return account.connection.protocol === "grok_build"
      ? "Refresh Grok usage or review its official usage page."
      : "Review the issue, then try again.";
  return account.stage === "cancelled"
    ? "Continue setup."
    : "Wait while Quantix prepares the official client.";
}

function SubscriptionSignIn({
  account,
  busy,
  onAction,
  retry = false,
}: {
  account: AccountView;
  busy: boolean;
  onAction: (action: Action) => Promise<void>;
  retry?: boolean;
}) {
  const supportsDeviceCode =
    account.connection.protocol === "codex" ||
    account.connection.protocol === "grok_build";
  return (
    <section className="flex min-w-0 flex-col gap-3 border-t pt-4">
      <p className="text-sm text-muted-foreground">
        Complete sign-in in the official {account.service_title} account.
        Quantix does not copy subscription credentials into API fields.
      </p>
      {account.user_code ? (
        <p className="flex flex-col gap-1 rounded-lg border bg-muted/40 p-3 text-xs text-muted-foreground">
          Sign-in code{" "}
          <strong className="font-mono text-lg tracking-[0.2em] text-foreground">
            {account.user_code}
          </strong>
        </p>
      ) : null}
      <div className="flex flex-wrap items-center gap-2">
        {webLink(account.login_url) ? (
          <ExternalLink
            className="button primary"
            href={webLink(account.login_url)!}
          >
            Open official sign-in
          </ExternalLink>
        ) : (
          <Button
            type="button"
            disabled={busy}
            onClick={() =>
              void onAction(
                setupAction({ action: "sign_in", sign_in_method: "browser" }),
              )
            }
          >
            {retry ? "Sign in again" : "Sign in with browser"}
          </Button>
        )}
        {supportsDeviceCode ? (
          <Button
            type="button"
            variant="outline"
            disabled={busy}
            onClick={() =>
              void onAction(
                setupAction({
                  action: "sign_in",
                  sign_in_method: "device_code",
                }),
              )
            }
          >
            Use a device code
          </Button>
        ) : null}
      </div>
      {account.active ? (
        <p className="text-xs text-muted-foreground">
          Finish sign-in in the provider window. Quantix will continue
          automatically.
        </p>
      ) : (
        <Button
          type="button"
          variant="link"
          size="sm"
          className="h-auto w-fit p-0 text-muted-foreground"
          disabled={busy}
          onClick={() => void onAction(setupAction({ action: "refresh" }))}
        >
          I’ve signed in — continue
        </Button>
      )}
    </section>
  );
}

function SubscriptionModelChoice({
  account,
  disabled,
  onSelect,
}: {
  account: AccountView;
  disabled: boolean;
  onSelect: (modelId: string) => Promise<void>;
}) {
  const [id, setId] = useState(
    account.selected_model_id ?? account.recommended_model_id ?? "",
  );
  useEffect(
    () =>
      setId(account.selected_model_id ?? account.recommended_model_id ?? ""),
    [account.selected_model_id, account.recommended_model_id],
  );
  return (
    <section className="flex min-w-0 flex-col gap-3 border-t pt-4">
      <h3 className="text-sm font-medium">Model</h3>
      <label className="flex min-w-0 flex-col gap-1.5 text-sm">
        Choose a model
        <NativeSelect
          className="w-full"
          value={id}
          disabled={disabled}
          onChange={(event) => setId(event.target.value)}
        >
          <NativeSelectOption value="">Select a model</NativeSelectOption>
          {account.models.map((model) => (
            <NativeSelectOption key={model.model_id} value={model.model_id}>
              {model.display_name} · {model.model_id}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </label>
      <Button
        type="button"
        variant="outline"
        size="sm"
        className="w-fit"
        disabled={disabled || !id}
        onClick={() => void onSelect(id)}
      >
        Save model selection
      </Button>
    </section>
  );
}

function SubscriptionCheck({
  account,
  busy,
  onAction,
  onCheck,
}: {
  account: AccountView;
  busy: boolean;
  onAction: (action: Action) => Promise<void>;
  onCheck: (preview: Preview) => Promise<void>;
}) {
  const api = useApi();
  const path = accountPath(account.id) + "/check-preview";
  const preview = useQuery<Preview>({
    queryKey: [
      path,
      account.connection.revision,
      account.selected_model_id,
      account.check.status,
    ],
    queryFn: ({ signal }) => api.get<Preview>(path, signal),
    enabled: account.stage !== "ready" && !account.active,
  });
  if (account.stage === "ready")
    return (
      <p className="text-xs text-muted-foreground">
        Subscription check passed
        {account.check.checked_at
          ? " · " + new Date(account.check.checked_at).toLocaleString()
          : ""}
        .
      </p>
    );
  return (
    <section className="flex min-w-0 flex-col gap-3 border-t pt-4">
      <h3 className="text-sm font-medium">Check subscription access</h3>
      <p className="text-xs text-muted-foreground">
        Quantix sends a short generic sample through the official client. Tender
        content is not sent.
      </p>
      <ErrorNotice error={preview.error} />
      {preview.isPending ? (
        <Loading>Preparing the subscription check…</Loading>
      ) : null}
      {preview.data ? (
        <>
          <p className="text-sm text-muted-foreground">{preview.data.detail}</p>
          {preview.data.limit_description ? (
            <p className="text-xs text-muted-foreground">
              {preview.data.limit_description}
            </p>
          ) : null}
          {account.connection.protocol === "grok_build" &&
          !preview.data.allowed ? (
            <div className="flex flex-col gap-2 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3">
              <p className="text-sm text-amber-700 dark:text-amber-400">
                Grok did not confirm the required subscription allowance.
              </p>
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  type="button"
                  size="sm"
                  disabled={busy}
                  onClick={() =>
                    void onAction(setupAction({ action: "refresh_usage" }))
                  }
                >
                  Refresh Grok usage
                </Button>
                <ExternalLink className="text-button" href="https://grok.com/">
                  Open Grok usage settings
                </ExternalLink>
              </div>
            </div>
          ) : null}
          <Button
            type="button"
            className="w-fit"
            disabled={busy || preview.isFetching || !preview.data.allowed}
            onClick={() => void onCheck(preview.data!)}
          >
            {account.check.status === "failed" ||
            account.check.status === "interrupted"
              ? "Check again"
              : "Check subscription"}
          </Button>
        </>
      ) : null}
    </section>
  );
}

function GrokRecovery({
  account,
  busy,
  onAction,
}: {
  account: AccountView;
  busy: boolean;
  onAction: (action: Action) => Promise<void>;
}) {
  return (
    <section className="flex flex-col gap-2 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3">
      <p className="text-sm text-amber-700 dark:text-amber-400">
        {account.detail}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          type="button"
          size="sm"
          disabled={busy}
          onClick={() =>
            void onAction(setupAction({ action: "refresh_usage" }))
          }
        >
          Refresh Grok usage
        </Button>
        <ExternalLink className="text-button" href="https://grok.com/">
          Open Grok usage settings
        </ExternalLink>
      </div>
    </section>
  );
}

function SubscriptionOptions({
  account,
  busy,
  onAction,
  onRemoved,
}: {
  account: AccountView;
  busy: boolean;
  onAction: (action: Action) => Promise<void>;
  onRemoved: () => void;
}) {
  const [extras, setExtras] = useState(
    account.connection.settings.allow_provider_managed_extras === true,
  );
  const [name, setName] = useState(account.connection.name);
  const [removing, setRemoving] = useState(false);
  const savedExtras =
    account.connection.settings.allow_provider_managed_extras === true;
  return (
    <div className="ai-account-options">
      <p className="field-help">
        Subscription access and provider billing remain separate from API keys
        and Tender budgets.
      </p>
      <label>
        Account name
        <input
          maxLength={150}
          value={name}
          disabled={busy}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <button
        type="button"
        className="button"
        disabled={
          busy || !name.trim() || name.trim() === account.connection.name
        }
        onClick={() =>
          void onAction(setupAction({ action: "rename", name: name.trim() }))
        }
      >
        Save name
      </button>
      <div className="inline-actions">
        <MicroButton
          kind="refresh"
          disabled={busy}
          onClick={() => void onAction(setupAction({ action: "refresh" }))}
        >
          Refresh account
        </MicroButton>
        <button
          type="button"
          className="text-button"
          disabled={busy}
          onClick={() => void onAction(setupAction({ action: "sign_out" }))}
        >
          Sign out
        </button>
        <button
          type="button"
          className="text-button"
          disabled={busy}
          onClick={() => void onAction(setupAction({ action: "repair" }))}
        >
          Repair official client
        </button>
        <button
          type="button"
          className="text-button"
          disabled={busy}
          onClick={() =>
            void onAction(setupAction({ action: "remove_software" }))
          }
        >
          Remove official client
        </button>
        <MicroButton
          kind="delete"
          disabled={busy}
          onClick={() => setRemoving(true)}
        >
          Remove account
        </MicroButton>
      </div>
      {removing ? (
        <RemoveAccount
          account={account}
          onClose={() => setRemoving(false)}
          onRemoved={onRemoved}
        />
      ) : null}
      {account.connection.protocol === "grok_build" ? (
        <fieldset disabled={busy}>
          <legend>Grok spending</legend>
          <label className="checkbox-label">
            <input
              type="radio"
              name={"grok-extras-" + account.id}
              checked={!extras}
              onChange={() => setExtras(false)}
            />
            Subscription allowance only
          </label>
          <label className="checkbox-label">
            <input
              type="radio"
              name={"grok-extras-" + account.id}
              checked={extras}
              onChange={() => setExtras(true)}
            />
            Allow provider-managed extras
          </label>
          <p className="field-help">
            Extras follow your Grok account. Changing this setting requires
            renewed Tender approval.
          </p>
          <button
            type="button"
            className="button"
            disabled={busy || extras === savedExtras}
            onClick={() =>
              void onAction(
                setupAction({
                  action: "set_subscription_extras",
                  allow_provider_managed_extras: extras,
                }),
              )
            }
          >
            Save spending preference
          </button>
        </fieldset>
      ) : null}
    </div>
  );
}

function DirectAccount({
  account,
  busy,
  error,
  onAction,
  onRefresh,
  onClose,
  onSaved,
}: {
  account: AccountView;
  busy: boolean;
  error: unknown;
  onAction: (action: Action) => Promise<void>;
  onRefresh: () => void;
  onClose: () => void;
  onSaved: (account: AccountView) => void;
}) {
  const [advanced, setAdvanced] = useState(false);
  const missingKey =
    account.connection.credential_state === "missing" ||
    account.stage === "needs_credentials";
  const manualRecovery =
    account.stage === "attention" &&
    account.connection.credential_state !== "missing" &&
    !account.active;
  const retryableCheck =
    account.stage === "attention" &&
    !!account.selected_model_id &&
    ["failed", "interrupted"].includes(account.check.status);
  const canChooseModel =
    ["choose_model", "ready_to_check"].includes(account.stage) ||
    account.models.length > 0 ||
    manualRecovery;
  const canCheck =
    !!account.selected_model_id &&
    (["ready_to_check", "ready"].includes(account.stage) || retryableCheck);
  const chooseModel = (modelId: string) =>
    onAction(setupAction({ action: "select_model", model_id: modelId }));
  return (
    <Modal legacy={false} title={account.connection.name} onClose={onClose}>
      <div className="flex min-w-0 flex-col gap-4">
        <ErrorNotice error={error} />
        <p className="text-xs text-muted-foreground">
          {account.service_title} · API connection
        </p>
        <DirectSteps account={account} />
        <div
          className={cn(
            "flex min-w-0 flex-col gap-1 rounded-xl border p-3",
            account.stage === "ready"
              ? "border-emerald-500/40 bg-emerald-500/5"
              : "bg-muted/40",
          )}
          role="status"
        >
          <div className="flex items-center gap-2">
            <span
              aria-hidden="true"
              className={cn(
                "size-2 shrink-0 rounded-full",
                account.stage === "ready"
                  ? "bg-emerald-500"
                  : account.stage === "attention"
                    ? "bg-amber-500"
                    : "bg-sky-500",
              )}
            />
            <h3 className="text-sm font-medium">
              {accountStageLabel(account)}
            </h3>
          </div>
          <p className="text-sm text-muted-foreground">{account.detail}</p>
          {account.stage !== "ready" ? (
            <p className="text-xs text-muted-foreground">
              <strong className="font-medium text-foreground">Next:</strong>{" "}
              {accountNextAction(account)}
            </p>
          ) : null}
          {account.active ? <Loading>Working…</Loading> : null}
        </div>
        {account.active ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="w-fit"
            disabled={busy}
            onClick={() => void onAction(setupAction({ action: "cancel" }))}
          >
            Cancel
          </Button>
        ) : null}
        {missingKey ? (
          <DirectKeyForm
            account={account}
            disabled={busy || account.active}
            onSaved={onSaved}
          />
        ) : null}
        {!missingKey && canChooseModel ? (
          <DirectModelChoice
            account={account}
            disabled={busy || account.active}
            onSelect={chooseModel}
          />
        ) : null}
        {retryableCheck ? (
          <div className="flex flex-col items-start gap-2 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3">
            <p className="text-sm text-muted-foreground">
              If the key or endpoint caused the failure, correct it before
              checking again.
            </p>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => setAdvanced(true)}
            >
              Correct API key or endpoint
            </Button>
          </div>
        ) : null}
        {canCheck ? (
          <AccessCheck
            account={account}
            busy={busy || account.active}
            onCheck={(preview, accept) =>
              onAction(
                setupAction({
                  action: "check",
                  check_fingerprint: preview.fingerprint,
                  maximum_cost_usd: preview.maximum_cost_usd,
                  accept_unknown_cost: accept,
                }),
              )
            }
          />
        ) : null}
        {account.stage === "attention" && !canCheck ? (
          <Button
            type="button"
            className="w-fit"
            disabled={busy || account.active}
            onClick={() => void onAction(setupAction({ action: "refresh" }))}
          >
            Refresh models
          </Button>
        ) : null}
        {account.stage === "cancelled" ? (
          <Button
            type="button"
            className="w-fit"
            disabled={busy || account.active}
            onClick={() => void onAction(setupAction({ action: "refresh" }))}
          >
            Continue connection
          </Button>
        ) : null}
        {account.stage === "ready" ? (
          <Button
            type="button"
            className="w-fit"
            disabled={busy}
            onClick={onClose}
          >
            Done
          </Button>
        ) : null}
        <details
          className="border-t pt-3"
          open={advanced}
          onToggle={(event) => setAdvanced(event.currentTarget.open)}
        >
          <summary className="w-fit cursor-pointer text-xs text-muted-foreground">
            More options
          </summary>
          {advanced ? (
            <div className="legacy-screen mt-3">
              <AccountOptions
                account={account}
                disabled={busy || account.active}
                onAction={onAction}
                onRefresh={onRefresh}
                onRemoved={onClose}
              />
            </div>
          ) : null}
        </details>
      </div>
    </Modal>
  );
}

function DirectSteps({ account }: { account: AccountView }) {
  const stage = account.stage;
  const index =
    stage === "ready"
      ? 2
      : ["ready_to_check", "checking"].includes(stage)
        ? 2
        : ["choose_model"].includes(stage)
          ? 1
          : account.connection.credential_state === "missing"
            ? 0
            : 1;
  return (
    <ol
      className="flex min-w-0 flex-wrap items-center gap-x-5 gap-y-2"
      aria-label="API connection steps"
    >
      {["API key", "Model", "Check"].map((label, position) => (
        <li
          key={label}
          className={cn(
            "flex items-center gap-2 text-xs",
            position <= index ? "text-foreground" : "text-muted-foreground",
          )}
          aria-current={position === index ? "step" : undefined}
        >
          <span
            className={cn(
              "flex size-5 shrink-0 items-center justify-center rounded-full text-xs",
              position < index
                ? "bg-emerald-500 text-white"
                : position === index
                  ? "bg-primary text-primary-foreground"
                  : "bg-muted text-muted-foreground",
            )}
          >
            {position < index ? <Check className="size-3" /> : position + 1}
          </span>
          {label}
        </li>
      ))}
    </ol>
  );
}

function DirectKeyForm({
  account,
  disabled,
  onSaved,
}: {
  account: AccountView;
  disabled: boolean;
  onSaved: (account: AccountView) => void;
}) {
  const api = useApi();
  const [key, setKey] = useState("");
  const [sessionOnly, setSessionOnly] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const connection = account.connection;
  return (
    <form
      className="flex min-w-0 flex-col gap-3 border-t pt-4"
      onSubmit={async (event) => {
        event.preventDefault();
        if (disabled || busy || !key.trim()) return;
        setBusy(true);
        setError(null);
        const values: Schema<"ConnectionInput"> = {
          name: connection.name,
          provider_id: connection.provider_id,
          protocol: connection.protocol,
          auth_type: "api_key",
          billing: connection.billing,
          enabled: connection.enabled,
          base_url: connection.base_url,
          environment_key: null,
          settings: connection.settings,
          allow_insecure_http: connection.allow_insecure_http,
          session_only: sessionOnly,
          credentials: { api_key: key.trim() },
        };
        try {
          const saved = await api.patch<AccountView>(
            `${accountPath(account.id)}/configure`,
            { connection: values } satisfies Schema<"SetupConfigure">,
          );
          setKey("");
          onSaved(saved);
        } catch (failure) {
          setError(failure);
        } finally {
          setBusy(false);
        }
      }}
    >
      <h3 className="text-sm font-medium">Save an API key</h3>
      <p className="text-xs text-muted-foreground">
        The key is write-only. Quantix never returns or pre-fills it.
      </p>
      <fieldset
        className="flex min-w-0 flex-col gap-3"
        disabled={disabled || busy}
      >
        <label className="flex min-w-0 flex-col gap-1.5 text-sm">
          API key
          <Input
            type="password"
            autoComplete="off"
            required
            value={key}
            onChange={(event) => setKey(event.target.value)}
            placeholder="Paste your API key"
          />
          <FieldError error={error} path={["credentials", "api_key"]} />
        </label>
        <label className="flex items-center gap-2 text-sm text-muted-foreground">
          <input
            type="checkbox"
            className="size-4"
            checked={sessionOnly}
            onChange={(event) => setSessionOnly(event.target.checked)}
          />
          Keep this key only until Quantix closes
        </label>
      </fieldset>
      <ErrorNotice error={error} />
      {/* Base UI buttons default to type="button"; this one must submit the form. */}
      <Button
        type="submit"
        className="w-fit"
        disabled={disabled || busy || !key.trim()}
      >
        {busy ? "Saving…" : "Save API key"}
      </Button>
    </form>
  );
}

/** What one model costs and how much it can read, in the engineer's terms. */
function modelSummary(model: Schema<"ModelRecord">) {
  const parts: string[] = [];
  const price = model.pricing;
  if (price)
    parts.push(
      `$${price.input_per_million} in / $${price.output_per_million} out per million`,
    );
  const window = model.capabilities?.context_window;
  if (window)
    parts.push(`${Math.round(window / 1000).toLocaleString()}k input`);
  return parts.join(" · ");
}

function DirectModelChoice({
  account,
  disabled,
  onSelect,
}: {
  account: AccountView;
  disabled: boolean;
  onSelect: (modelId: string) => Promise<void> | void;
}) {
  const api = useApi();
  const client = useQueryClient();
  const models = account.models;
  const [id, setId] = useState(
    account.selected_model_id ?? account.recommended_model_id ?? "",
  );
  const [manualEntry, setManualEntry] = useState(false);
  const [query, setQuery] = useState("");
  const [manualBusy, setManualBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  useEffect(
    () =>
      setId(account.selected_model_id ?? account.recommended_model_id ?? ""),
    [account.selected_model_id, account.recommended_model_id],
  );
  // Typing an ID is the fallback for a provider with no model list; a provider
  // that later returns one takes over again.
  const manual = manualEntry || models.length === 0;
  const term = query.trim().toLowerCase();
  const shown = term
    ? models.filter((model) =>
        `${model.display_name} ${model.model_id}`.toLowerCase().includes(term),
      )
    : models;
  const save = async () => {
    setError(null);
    if (!manual) {
      await onSelect(id);
      return;
    }
    setManualBusy(true);
    try {
      await api.post<Schema<"ModelRecord">>(
        `/ai/connections/${encodeURIComponent(account.connection.id)}/models`,
        {
          model_id: id.trim(),
          display_name: id.trim(),
        } satisfies Schema<"ModelInput">,
      );
      await client.invalidateQueries({ queryKey: [accountPath(account.id)] });
      await onSelect(id.trim());
    } catch (failure) {
      setError(failure);
    } finally {
      setManualBusy(false);
    }
  };
  return (
    <section className="flex min-w-0 flex-col gap-3 border-t pt-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="text-sm font-medium">Model</h3>
        {models.length ? (
          <span className="text-xs text-muted-foreground">
            {models.length} available
          </span>
        ) : null}
      </div>
      {manual ? (
        <label className="flex min-w-0 flex-col gap-1.5 text-sm">
          Model ID
          <Input
            value={id}
            disabled={disabled || manualBusy}
            onChange={(event) => setId(event.target.value)}
            placeholder="For example, provider-model-name"
          />
          <FieldError error={error} name="model_id" />
          <span className="text-xs text-muted-foreground">
            {models.length
              ? "Enter the exact ID from the provider's documentation."
              : "This provider did not return a model list. Enter the exact ID from its documentation."}
          </span>
        </label>
      ) : (
        <>
          {models.length > 6 ? (
            <Input
              type="search"
              value={query}
              disabled={disabled}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search models"
              aria-label="Search models"
            />
          ) : null}
          <ul
            className="max-h-64 min-w-0 divide-y overflow-y-auto rounded-lg border"
            aria-label="Available models"
          >
            {shown.map((model) => {
              const chosen = model.model_id === id;
              const summary = modelSummary(model);
              return (
                <li key={model.model_id}>
                  <button
                    type="button"
                    disabled={disabled}
                    aria-pressed={chosen}
                    onClick={() => setId(model.model_id)}
                    className={cn(
                      "flex w-full min-w-0 items-center gap-2 px-3 py-2 text-start transition-colors enabled:hover:bg-muted/50",
                      chosen && "bg-muted",
                    )}
                  >
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-sm">
                        {model.display_name}
                        {model.model_id === account.recommended_model_id ? (
                          <span className="ms-2 text-xs text-muted-foreground">
                            suggested
                          </span>
                        ) : null}
                      </span>
                      <span className="truncate text-xs text-muted-foreground">
                        {model.model_id}
                        {summary ? ` · ${summary}` : ""}
                      </span>
                    </span>
                    {chosen ? (
                      <Check className="size-4 shrink-0 text-(--success)" />
                    ) : null}
                  </button>
                </li>
              );
            })}
            {!shown.length ? (
              <li className="px-3 py-2 text-sm text-muted-foreground">
                No model matches that search.
              </li>
            ) : null}
          </ul>
        </>
      )}
      <ErrorNotice error={error} />
      <div className="flex min-w-0 flex-wrap items-center gap-3">
        <Button
          type="button"
          size="sm"
          className="order-1"
          disabled={disabled || manualBusy || !id.trim()}
          onClick={() => void save()}
        >
          {manualBusy ? "Saving…" : "Use this model"}
        </Button>
        {models.length > 0 ? (
          <Button
            type="button"
            variant="link"
            size="sm"
            className="order-2 h-auto p-0 text-muted-foreground"
            disabled={disabled || manualBusy}
            onClick={() => {
              setManualEntry(!manual);
              setId("");
            }}
          >
            {manual ? "Choose from the list" : "Enter a model ID"}
          </Button>
        ) : null}
      </div>
    </section>
  );
}

function AccessCheck({
  account,
  busy,
  onCheck,
}: {
  account: AccountView;
  busy: boolean;
  onCheck: (preview: Preview, acceptUnknownCost: boolean) => void;
}) {
  const api = useApi();
  const path = `${accountPath(account.id)}/check-preview`;
  const preview = useQuery<Preview>({
    queryKey: [
      path,
      account.connection.revision,
      account.selected_model_id,
      account.check.status,
      account.software.version,
    ],
    queryFn: ({ signal }) => api.get<Preview>(path, signal),
    enabled: account.stage !== "ready" && !account.active,
  });
  const [unknownConsent, setUnknownConsent] = useState(false);
  useEffect(() => setUnknownConsent(false), [preview.data?.fingerprint]);
  if (account.stage === "ready")
    return (
      <p className="text-xs text-muted-foreground">
        Access check passed
        {account.check.checked_at
          ? ` · ${new Date(account.check.checked_at).toLocaleString()}`
          : ""}
        .
      </p>
    );
  const cost = preview.data?.maximum_cost_usd;
  const unknownCost = preview.data?.requires_unknown_cost_consent === true;
  return (
    <section className="flex min-w-0 flex-col gap-3 border-t pt-4">
      <h3 className="text-sm font-medium">Check connection</h3>
      <p className="text-xs text-muted-foreground">
        Quantix sends a short generic sample. No Tender content is sent.
      </p>
      <ErrorNotice error={preview.error} />
      {preview.isPending ? (
        <Loading>Preparing the connection check…</Loading>
      ) : null}
      {preview.data ? (
        <>
          {!preview.data.allowed ? (
            <p className="text-sm text-amber-700 dark:text-amber-400">
              {preview.data.detail}
            </p>
          ) : unknownCost ? (
            <div className="flex flex-col gap-2 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3">
              <p className="text-sm text-amber-700 dark:text-amber-400">
                The check cost is not established. Quantix will not claim a
                monetary cap.
              </p>
              <label className="flex items-start gap-2 text-sm">
                <input
                  type="checkbox"
                  className="mt-0.5 size-4"
                  checked={unknownConsent}
                  onChange={(event) => setUnknownConsent(event.target.checked)}
                />
                I understand the cost is unknown and allow this short connection
                check.
              </label>
            </div>
          ) : cost != null && cost > 0 ? (
            <p className="text-sm">
              Maximum estimated cost:{" "}
              <strong>
                USD{" "}
                {cost.toLocaleString(undefined, { maximumFractionDigits: 6 })}
              </strong>
              .
            </p>
          ) : (
            <p className="text-xs text-muted-foreground">
              {preview.data.detail}
            </p>
          )}
          <Button
            type="button"
            className="w-fit"
            disabled={
              busy ||
              preview.isFetching ||
              !preview.data.allowed ||
              (unknownCost && !unknownConsent)
            }
            onClick={() => onCheck(preview.data!, unknownCost)}
          >
            {account.check.status === "failed" ||
            account.check.status === "interrupted"
              ? "Check again"
              : "Check connection"}
          </Button>
        </>
      ) : null}
    </section>
  );
}

function AccountOptions({
  account,
  disabled,
  onAction,
  onRefresh,
  onRemoved,
}: {
  account: AccountView;
  disabled: boolean;
  onAction: (action: Action) => Promise<void>;
  onRefresh: () => void;
  onRemoved: () => void;
}) {
  const api = useApi();
  const providers = useResource<Schema<"ProviderPreset">[]>("/ai/providers");
  const [editing, setEditing] = useState(false);
  const [models, setModels] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [name, setName] = useState(account.connection.name);
  const closeEdit = useCallback(() => {
    setEditing(false);
    onRefresh();
  }, [onRefresh]);
  const closeModels = useCallback(() => {
    setModels(false);
    onRefresh();
  }, [onRefresh]);
  return (
    <div className="ai-account-options">
      <ErrorNotice error={providers.error} />
      <label>
        Account name
        <input
          maxLength={150}
          value={name}
          disabled={disabled}
          onChange={(event) => setName(event.target.value)}
        />
      </label>
      <button
        type="button"
        className="button"
        disabled={
          disabled || !name.trim() || name.trim() === account.connection.name
        }
        onClick={() =>
          void onAction(setupAction({ action: "rename", name: name.trim() }))
        }
      >
        Save name
      </button>
      <p className="field-help">
        Data destination: {dataDestination(account.connection)}
      </p>
      <div className="inline-actions">
        <button
          type="button"
          className="button"
          disabled={disabled || !providers.data?.length}
          onClick={() => setEditing(true)}
        >
          Endpoint and key settings
        </button>
        <button
          type="button"
          className="button"
          disabled={disabled}
          onClick={() => setModels(true)}
        >
          Model details
        </button>
        <button
          type="button"
          className="text-button"
          disabled={disabled}
          onClick={() => void onAction(setupAction({ action: "refresh" }))}
        >
          Refresh models
        </button>
        <MicroButton
          kind="delete"
          disabled={disabled}
          onClick={() => setRemoving(true)}
        >
          Remove account
        </MicroButton>
      </div>
      {editing && providers.data?.length ? (
        <ConnectionForm
          connection={account.connection}
          providers={providers.data}
          onClose={closeEdit}
          onSave={async (connection) => {
            await api.patch<AccountView>(
              `${accountPath(account.id)}/configure`,
              { connection } satisfies Schema<"SetupConfigure">,
            );
          }}
        />
      ) : null}
      {models ? (
        <ConnectionModels
          connection={account.connection}
          provider={providers.data?.find(
            (provider) => provider.id === account.connection.provider_id,
          )}
          onClose={closeModels}
        />
      ) : null}
      {removing ? (
        <RemoveAccount
          account={account}
          onClose={() => setRemoving(false)}
          onRemoved={onRemoved}
        />
      ) : null}
    </div>
  );
}

function RetiredAccount({
  account,
  onClose,
}: {
  account: AccountView;
  onClose: () => void;
}) {
  const [removing, setRemoving] = useState(false);
  return (
    <Modal title={account.connection.name} onClose={onClose}>
      <div className="ai-setup-panel ai-retired-panel">
        <p className="field-help">
          This saved account uses an older connection flow. It is read-only and
          is not offered for new Tender setup.
        </p>
        <p className="field-help">
          Close this window and choose Add AI account above to connect an
          OpenAI, Anthropic, Google, xAI or custom API account.
        </p>
        <p className="field-help">
          Data destination: {dataDestination(account.connection)}
        </p>
        <p className="field-help">
          Access: {account.connection.credential_state.replaceAll("_", " ")}
        </p>
        {account.connection.last_error ? (
          <ErrorNotice error={new Error(account.connection.last_error)} />
        ) : null}
        {account.models.length ? (
          <div className="ai-retired-models">
            <h3>Saved models</h3>
            <ul>
              {account.models.map((model) => (
                <li key={model.model_id}>
                  {model.display_name} <code>{model.model_id}</code>
                </li>
              ))}
            </ul>
          </div>
        ) : (
          <p className="field-help">
            No saved models are recorded for this account.
          </p>
        )}
        <MicroButton kind="delete" onClick={() => setRemoving(true)}>
          Remove saved account
        </MicroButton>
        {removing ? (
          <RemoveAccount
            account={account}
            onClose={() => setRemoving(false)}
            onRemoved={onClose}
          />
        ) : null}
      </div>
    </Modal>
  );
}

function RemoveAccount({
  account,
  onClose,
  onRemoved,
}: {
  account: AccountView;
  onClose: () => void;
  onRemoved: () => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  return (
    <Modal title="Remove saved AI account" onClose={onClose}>
      <form
        className="ai-form"
        onSubmit={async (event) => {
          event.preventDefault();
          if (!confirmed || busy) return;
          setBusy(true);
          setError(null);
          try {
            await api.delete<Schema<"MutationReceipt">>(
              `/ai/connections/${account.connection.id}`,
            );
            await refresh();
            onRemoved();
          } catch (failure) {
            setError(failure);
          } finally {
            setBusy(false);
          }
        }}
      >
        <p className="muted">
          Remove “{account.connection.name}” and its saved models and
          credentials. Tender routes using it need another approved account.
        </p>
        <label className="checkbox-label">
          <input
            type="checkbox"
            required
            checked={confirmed}
            disabled={busy}
            onChange={(event) => setConfirmed(event.target.checked)}
          />
          I want to remove this saved account.
        </label>
        <ErrorNotice error={error} />
        <div className="form-actions">
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <MicroButton
            kind="delete"
            type="submit"
            disabled={busy || !confirmed}
          >
            {busy ? "Removing…" : "Remove account"}
          </MicroButton>
        </div>
      </form>
    </Modal>
  );
}
