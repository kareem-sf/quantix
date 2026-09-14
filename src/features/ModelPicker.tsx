import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Check,
  ChevronDown,
  CircleAlert,
  KeyRound,
  Settings2,
  SlidersHorizontal,
  Sparkles,
} from "lucide-react";
import { tenderPath, useApi, useRefresh, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { AnimatedBorder } from "@/components/ui/animated-border";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Squircle } from "@/components/ui/effect-filters";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { InputGroupButton } from "@/components/ui/input-group";
import { ProviderLogo, hasProviderMark } from "@/components/ui/provider-logo";
import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from "@/components/ui/popover";
import { cn } from "@/lib/utils";
import {
  AccountSetup,
  connectionBillingDescription,
  dataDestination,
} from "./AISetup";

type Account = Schema<"SetupAccount">;
type Policy = Schema<"TenderAIRecord">;

export type ModelOption = {
  account: Account;
  model: Schema<"ModelRecord">;
  metered: boolean;
  toolsReady: boolean;
  priced: boolean;
};

/** Shared look for the round chips in the Manager prompt box. */
export const composerChip =
  "rounded-full bg-foreground/[0.04] shadow-[inset_0_0_0_1px_color-mix(in_oklch,var(--foreground)_8%,transparent)] hover:bg-foreground/[0.08] aria-expanded:bg-foreground/[0.08]";

/** Tile colour for a provider that has no mark of its own. */
const providerTones: Record<string, string> = {
  custom: "bg-violet-600",
};

/**
 * An account's own provider logo, so the engineer recognises the service at a
 * glance. Providers without a drawn mark keep a coloured initial tile.
 */
/** Fixed tile and mark sizes; percentages left the marks unbounded. */
const badgeSizes = {
  sm: { tile: "size-5", mark: "size-3.5", text: "text-[10px]" },
  md: { tile: "size-8", mark: "size-5", text: "text-xs" },
} as const;

function ProviderBadge({
  account,
  size = "md",
  className,
}: {
  account: Account;
  size?: keyof typeof badgeSizes;
  className?: string;
}) {
  const providerId = account.connection.provider_id;
  const scale = badgeSizes[size];
  if (hasProviderMark(providerId))
    return (
      <span
        className={cn(
          "inline-flex shrink-0 items-center justify-center rounded-[28%] bg-foreground/[0.04] text-foreground shadow-[inset_0_0_0_1px_color-mix(in_oklch,var(--foreground)_9%,transparent)]",
          scale.tile,
          className,
        )}
      >
        <ProviderLogo providerId={providerId} className={scale.mark} />
      </span>
    );
  return (
    <Squircle
      className={cn(
        "font-semibold text-white",
        scale.tile,
        scale.text,
        className,
      )}
      surfaceClassName={providerTones[providerId] ?? "bg-primary"}
    >
      {initial(account)}
    </Squircle>
  );
}

const isMetered = (account: Account) =>
  account.connection.billing === "metered" ||
  account.connection.billing === "unknown";

/**
 * An account can run this Tender once its access check passed for a model.
 * Tender approval is recorded per account and checked model.
 */
export function modelOptions(accounts: Account[]): ModelOption[] {
  return accounts.flatMap((account) => {
    if (
      !account.supported ||
      !account.connection.enabled ||
      account.stage !== "ready" ||
      account.check.status !== "passed" ||
      !account.check.model_id
    )
      return [];
    const model = account.models.find(
      (item) => item.model_id === account.check.model_id,
    );
    if (!model) return [];
    return [
      {
        account,
        model,
        metered: isMetered(account),
        toolsReady: model.capabilities?.tools === true,
        priced: !!model.pricing,
      },
    ];
  });
}

function initial(account: Account) {
  return (account.service_title || account.connection.name || "?")
    .trim()
    .charAt(0)
    .toUpperCase();
}

export function ModelPicker({
  tenderId,
  policy,
  policyPending = false,
  busy,
  open,
  onOpenChange,
  onManageAccounts,
  onAdvanced,
}: {
  tenderId: string;
  policy?: Policy;
  policyPending?: boolean;
  /** Tender work is running; AI permissions cannot change until it stops. */
  busy: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onManageAccounts: () => void;
  onAdvanced: () => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const accounts = useQuery({
    queryKey: ["/ai/setup/accounts"],
    queryFn: ({ signal }) => api.get<Account[]>("/ai/setup/accounts", signal),
    staleTime: 15_000,
    retry: false,
  });
  // A setup check that finished in the account panel must not leave a stale
  // "Finish setup" row here, so the accounts are read again on every opening.
  const refetchAccounts = accounts.refetch;
  useEffect(() => {
    if (open) void refetchAccounts();
  }, [open, refetchAccounts]);
  const list = Array.isArray(accounts.data) ? accounts.data : [];
  const options = useMemo(() => modelOptions(list), [list]);
  const unfinished = list.filter(
    (account) =>
      account.supported &&
      account.connection.enabled &&
      !options.some((option) => option.account.id === account.id),
  );
  const manager = policy?.manager ?? null;
  const current = options.find(
    (option) =>
      option.account.id === manager?.connection_id &&
      option.model.model_id === manager?.model_id,
  );
  const currentAccount = list.find(
    (account) => account.id === manager?.connection_id,
  );

  const [confirming, setConfirming] = useState<ModelOption | null>(null);
  const [budget, setBudget] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [checkingAccount, setCheckingAccount] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState("");

  const needsBudget = (option: ModelOption) =>
    option.metered && !policy?.tender_budget_usd;
  const needsRestoreReview = (option: ModelOption) =>
    !!policy?.restore_reconciliation_required &&
    (option.metered ||
      (option.account.connection.protocol === "grok_build" &&
        option.account.connection.settings.allow_provider_managed_extras ===
          true));

  function changeOpen(next: boolean) {
    onOpenChange(next);
    if (!next) {
      setConfirming(null);
      setError(null);
      setBudget("");
    }
  }

  async function apply(option: ModelOption, budgetUsd: number | null) {
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      await api.post<Policy>(`${tenderPath(tenderId)}/ai-setup`, {
        account_id: option.account.id,
        ...(option.account.connection.protocol === "grok_build"
          ? { account_revision: option.account.connection.revision }
          : {}),
        model_id: option.model.model_id,
        budget_usd: budgetUsd,
        restore_budget_reviewed: false,
        engineer_confirmed: true,
      } satisfies Schema<"SimpleTenderAIInput">);
      await refresh();
      setAnnouncement(
        `${option.model.display_name} on ${option.account.connection.name} now runs this Tender.`,
      );
      changeOpen(false);
    } catch (failure) {
      setError(failure);
    } finally {
      setSaving(false);
    }
  }

  function choose(option: ModelOption) {
    if (busy || saving || !option.toolsReady) return;
    if (option === current) {
      changeOpen(false);
      return;
    }
    if (needsBudget(option) || needsRestoreReview(option)) {
      setError(null);
      setBudget("");
      setConfirming(option);
      return;
    }
    void apply(option, null);
  }

  const label = current
    ? current.model.display_name
    : manager
      ? manager.model_id
      : "Choose AI";
  const provider = current?.account ?? currentAccount;
  const budgetNumber = Number(budget);
  const budgetValid =
    budget !== "" &&
    Number.isFinite(budgetNumber) &&
    budgetNumber > 0 &&
    budgetNumber <= 10_000_000;

  return (
    <>
      <Popover open={open} onOpenChange={changeOpen}>
        <PopoverTrigger
          render={
            <InputGroupButton
              size="xs"
              variant="ghost"
              className={cn(
                composerChip,
                "relative h-7 max-w-[14rem] gap-1.5 ps-1 pe-2 text-xs",
              )}
              aria-label={
                manager
                  ? `AI for this Tender: ${provider ? `${provider.connection.name}, ` : ""}${label}. Change AI`
                  : "Choose AI for this Tender"
              }
            />
          }
        >
          {provider ? (
            <ProviderBadge account={provider} size="sm" />
          ) : (
            <Sparkles className="ms-1 size-3.5" />
          )}
          <span className="truncate">{label}</span>
          <ChevronDown className="size-3.5 opacity-60" />
          {!manager && !policyPending ? <AnimatedBorder radius={14} /> : null}
        </PopoverTrigger>
        <PopoverContent
          side="top"
          align="start"
          sideOffset={10}
          className="w-[min(23rem,calc(100vw-2rem))] gap-0 overflow-hidden p-0"
        >
          <PopoverHeader className="border-b px-3 py-2.5">
            <PopoverTitle>AI for this Tender</PopoverTitle>
            <PopoverDescription className="text-xs">
              Tender content goes only to the account you pick. Switch any time
              between pieces of work.
            </PopoverDescription>
          </PopoverHeader>
          {busy ? (
            <p className="flex items-start gap-2 border-b bg-amber-500/10 px-3 py-2 text-xs text-amber-800 dark:text-amber-300">
              <CircleAlert className="mt-0.5 size-3.5 shrink-0" />
              Finish or stop the current work to switch AI. The next instruction
              will use your choice.
            </p>
          ) : null}
          <div
            role="radiogroup"
            aria-label="Connected AI accounts"
            className="scroll-fade-y flex max-h-72 flex-col gap-0.5 overflow-y-auto p-1"
          >
            {accounts.isPending ? (
              <Loading>Loading AI accounts…</Loading>
            ) : null}
            {options.map((option) => {
              const selected = option === current;
              const { account, model } = option;
              return (
                <div key={account.id} className="flex items-center gap-0.5">
                  <button
                    type="button"
                    role="radio"
                    aria-checked={selected}
                    disabled={busy || saving || !option.toolsReady}
                    onClick={() => choose(option)}
                    className={cn(
                      "flex min-w-0 flex-1 items-center gap-3 rounded-lg px-2 py-2 text-start outline-none transition-colors hover:bg-accent focus-visible:bg-accent disabled:pointer-events-none disabled:opacity-50",
                      selected && "bg-accent/70",
                    )}
                  >
                    <ProviderBadge account={account} size="md" />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="truncate text-sm font-medium">
                        {model.display_name}
                      </span>
                      <span className="truncate text-xs text-muted-foreground">
                        {account.connection.name} ·{" "}
                        {connectionBillingDescription(account.connection)}
                      </span>
                      {!option.toolsReady ? (
                        <span className="text-xs text-amber-700 dark:text-amber-400">
                          This model can't use Tender tools
                        </span>
                      ) : option.metered && !option.priced ? (
                        <span className="text-xs text-amber-700 dark:text-amber-400">
                          Prices not recorded yet
                        </span>
                      ) : null}
                    </span>
                    {selected ? (
                      <Check className="size-4 shrink-0" aria-hidden="true" />
                    ) : option.metered ? (
                      <Badge variant="outline" className="font-normal">
                        Paid
                      </Badge>
                    ) : null}
                  </button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    className="text-muted-foreground"
                    aria-label={`Check another model for ${account.connection.name}`}
                    title="Check another model"
                    onClick={() => {
                      changeOpen(false);
                      setCheckingAccount(account.id);
                    }}
                  >
                    <Settings2 />
                  </Button>
                </div>
              );
            })}
            {unfinished.map((account) => (
              <button
                key={account.id}
                type="button"
                onClick={() => {
                  changeOpen(false);
                  setCheckingAccount(account.id);
                }}
                className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-start outline-none transition-colors hover:bg-accent focus-visible:bg-accent"
              >
                <ProviderBadge
                  account={account}
                  size="md"
                  className="opacity-70"
                />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm">
                    {account.connection.name}
                  </span>
                  <span className="truncate text-xs text-muted-foreground">
                    {account.detail || "Setup is not finished"}
                  </span>
                </span>
                <span className="shrink-0 text-xs font-medium">
                  Finish setup
                </span>
              </button>
            ))}
            {!accounts.isPending && !options.length && !unfinished.length ? (
              <div className="flex flex-col items-start gap-2 px-2 py-3 text-sm">
                <p className="text-muted-foreground">
                  No AI account is connected yet.
                </p>
                <Button
                  type="button"
                  size="sm"
                  onClick={() => {
                    changeOpen(false);
                    onManageAccounts();
                  }}
                >
                  <KeyRound data-icon="inline-start" />
                  Connect an AI account
                </Button>
              </div>
            ) : null}
          </div>
          {confirming ? (
            <form
              className="flex flex-col gap-3 border-t bg-muted/40 p-3"
              onSubmit={(event) => {
                event.preventDefault();
                if (needsRestoreReview(confirming)) return;
                if (needsBudget(confirming) && !budgetValid) return;
                void apply(
                  confirming,
                  needsBudget(confirming) ? budgetNumber : null,
                );
              }}
            >
              <div className="flex flex-col gap-1">
                <p className="text-sm font-medium">
                  Use {confirming.model.display_name} for this Tender
                </p>
                <p className="text-xs text-muted-foreground">
                  Tender content will be sent to{" "}
                  {dataDestination(confirming.account.connection)}.
                </p>
              </div>
              {needsRestoreReview(confirming) ? (
                <div className="flex flex-col items-start gap-2 text-xs text-amber-800 dark:text-amber-300">
                  <p>
                    This workspace was restored from a backup. Review spending
                    before approving paid AI work.
                  </p>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => {
                      changeOpen(false);
                      onAdvanced();
                    }}
                  >
                    Review spending
                  </Button>
                </div>
              ) : (
                <Field className="gap-1.5">
                  <FieldLabel htmlFor={`budget-${tenderId}`}>
                    Spending allowance for this Tender (USD)
                  </FieldLabel>
                  <Input
                    id={`budget-${tenderId}`}
                    type="number"
                    inputMode="decimal"
                    min="0.000001"
                    max={10_000_000}
                    step="any"
                    required
                    autoFocus
                    value={budget}
                    onChange={(event) => setBudget(event.target.value)}
                  />
                  <FieldDescription className="text-xs">
                    Covers all paid AI on this Tender. You enter it once;
                    switching accounts keeps it.
                  </FieldDescription>
                </Field>
              )}
              <div className="flex justify-end gap-2">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setConfirming(null)}
                >
                  Cancel
                </Button>
                {!needsRestoreReview(confirming) ? (
                  <Button
                    type="submit"
                    size="sm"
                    disabled={saving || !budgetValid}
                  >
                    {saving ? "Switching…" : "Approve and use"}
                  </Button>
                ) : null}
              </div>
            </form>
          ) : null}
          {error || accounts.error ? (
            <div className="px-3">
              <ErrorNotice error={error || accounts.error} />
            </div>
          ) : null}
          <div className="flex items-center justify-between gap-2 border-t p-1.5">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="text-muted-foreground"
              onClick={() => {
                changeOpen(false);
                onManageAccounts();
              }}
            >
              <KeyRound data-icon="inline-start" />
              Manage accounts
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="text-muted-foreground"
              onClick={() => {
                changeOpen(false);
                onAdvanced();
              }}
            >
              <SlidersHorizontal data-icon="inline-start" />
              Team and budgets
            </Button>
          </div>
        </PopoverContent>
      </Popover>
      <span className="sr-only" aria-live="polite">
        {announcement}
      </span>
      {checkingAccount ? (
        <AccountSetup
          accountId={checkingAccount}
          onClose={() => {
            setCheckingAccount(null);
            void refetchAccounts();
          }}
        />
      ) : null}
    </>
  );
}
