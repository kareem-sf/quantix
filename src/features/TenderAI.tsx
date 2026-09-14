import { useCallback, useState } from "react";
import { GenerationControls } from "../components/GenerationControls";
import { useQuery } from "@tanstack/react-query";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading } from "../components/common";
import {
  AccountSetup,
  connectionBillingDescription,
  dataDestination,
  GrokSpendingNote,
} from "./AISetup";

type Route = Schema<"AIRoute">;
const newRoute = (connectionId: string): Route => ({
  connection_id: connectionId,
  model_id: "",
  reasoning: null,
  max_output_tokens: 8192,
  web_search: false,
  max_search_calls: 3,
});
const directProviders = new Set([
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
const directConnection = (connection: Schema<"ConnectionRecord">) =>
  directProviders.has(connection.provider_id) &&
  directProtocols.has(connection.protocol) &&
  ["api_key", "environment"].includes(connection.auth_type);
const subscriptionConnection = (connection: Schema<"ConnectionRecord">) =>
  connection.billing === "subscription" &&
  connection.auth_type === "client_login" &&
  ((connection.provider_id === "codex" && connection.protocol === "codex") ||
    (connection.provider_id === "grok_build" &&
      connection.protocol === "grok_build"));
export function TenderAI({
  tenderId,
  initiallyEditing = false,
}: {
  tenderId: string;
  initiallyEditing?: boolean;
}) {
  const api = useApi();
  const policy = useResource<Schema<"TenderAIRecord">>(
    `${tenderPath(tenderId)}/ai-policy`,
  );
  const accounts = useQuery({
    queryKey: ["/ai/setup/accounts"],
    queryFn: () => api.get<Schema<"SetupAccount">[]>("/ai/setup/accounts"),
    refetchInterval: (query) =>
      query.state.data?.some((account) => account.active) ? 1500 : false,
  });
  const connections =
    useResource<Schema<"ConnectionRecord">[]>("/ai/connections");
  const supportedConnectionIds = new Set(
    accounts.data
      ?.filter((account) => account.supported)
      .map((account) => account.connection.id),
  );
  const directConnections =
    connections.data?.filter(
      (connection) =>
        supportedConnectionIds.has(connection.id) &&
        (directConnection(connection) || subscriptionConnection(connection)),
    ) ?? [];
  return (
    <section className="tender-ai">
      <ErrorNotice
        error={policy.error || accounts.error || connections.error}
      />
      {policy.isPending || accounts.isPending || connections.isPending ? (
        <Loading>Loading Tender AI setup…</Loading>
      ) : null}
      {policy.data && accounts.data && connections.data ? (
        <SimplePolicy
          key={policy.data.revision}
          tenderId={tenderId}
          policy={policy.data}
          accounts={accounts.data}
          connections={directConnections}
          initiallyEditing={initiallyEditing}
        />
      ) : null}
    </section>
  );
}

function SimplePolicy({
  tenderId,
  policy,
  accounts,
  connections,
  initiallyEditing,
}: {
  tenderId: string;
  policy: Schema<"TenderAIRecord">;
  accounts: Schema<"SetupAccount">[];
  connections: Schema<"ConnectionRecord">[];
  initiallyEditing: boolean;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const approved = accounts.find(
    (account) => account.connection.id === policy.manager?.connection_id,
  );
  const approvedSupported = approved?.supported === true ? approved : undefined;
  const available = accounts.filter(
    (account) =>
      account.supported &&
      account.stage === "ready" &&
      account.connection.enabled,
  );
  const initial =
    approvedSupported ??
    (!policy.manager && available.length === 1 ? available[0] : undefined);
  const [editing, setEditing] = useState(
      initiallyEditing ||
        !policy.manager ||
        policy.restore_reconciliation_required,
    ),
    [advanced, setAdvanced] = useState(false),
    [changeModel, setChangeModel] = useState(false);
  const [accountId, setAccountId] = useState(initial?.id ?? ""),
    [modelId, setModelId] = useState(
      initial?.connection.id === policy.manager?.connection_id
        ? (policy.manager?.model_id ?? "")
        : (initial?.selected_model_id ?? initial?.recommended_model_id ?? ""),
    );
  const [budget, setBudget] = useState(
      policy.tender_budget_usd?.toString() ?? "",
    ),
    [restored, setRestored] = useState(false),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null),
    [checkingAccount, setCheckingAccount] = useState<string | null>(null);
  const closeAccount = useCallback(() => setCheckingAccount(null), []);
  const account = accounts.find((item) => item.id === accountId),
    model = account?.models.find((item) => item.model_id === modelId);
  const modelChecked =
    !!account &&
    account.supported &&
    !account.active &&
    account.connection.enabled &&
    account.stage === "ready" &&
    account.check.status === "passed" &&
    account.check.model_id === modelId;
  const accountOptions = available;
  const additionalTeam =
    (policy.specialist &&
      (policy.specialist.connection_id !== policy.manager?.connection_id ||
        policy.specialist.model_id !== policy.manager?.model_id)) ||
    Object.keys(policy.role_routes).length > 0 ||
    policy.fallback_routes.length > 0;
  const metered =
    !!account && ["metered", "unknown"].includes(account.connection.billing);
  const paidExposure =
    metered ||
    (account?.connection.protocol === "grok_build" &&
      account.connection.settings.allow_provider_managed_extras === true);
  const retainedRoutes = [
    ...(policy.specialist &&
    Object.entries(policy.specialist).some(
      ([field, value]) => policy.manager?.[field as keyof Route] !== value,
    )
      ? [policy.specialist]
      : []),
    ...Object.values(policy.role_routes),
    ...policy.fallback_routes,
  ];
  const otherMetered =
    !metered &&
    retainedRoutes.some((route) =>
      connections.some(
        (connection) =>
          connection.id === route.connection_id &&
          ["metered", "unknown"].includes(connection.billing),
      ),
    );
  const restoreBudgetsReady =
    !otherMetered ||
    (policy.run_budget_usd != null &&
      policy.run_budget_usd > 0 &&
      policy.tender_budget_usd != null &&
      policy.tender_budget_usd > 0);
  const budgetValid =
    !metered ||
    (budget !== "" &&
      Number.isFinite(Number(budget)) &&
      Number(budget) > 0 &&
      Number(budget) <= 10000000);
  const restoreReady =
    !paidExposure ||
    !policy.restore_reconciliation_required ||
    (restored && restoreBudgetsReady);
  const currentConnection =
    approved?.connection ??
    connections.find(
      (connection) => connection.id === policy.manager?.connection_id,
    );
  const currentModel = approvedSupported?.models.find(
    (item) => item.model_id === policy.manager?.model_id,
  );
  const retiredCurrent = !!approved && approved.supported === false;
  const extrasNeedApproval =
    !retiredCurrent &&
    currentConnection?.protocol === "grok_build" &&
    currentConnection.settings.allow_provider_managed_extras === true &&
    policy.provider_managed_extras?.[currentConnection.id] !==
      currentConnection.revision;
  return (
    <div className="tender-ai-body">
      <div className="section-heading">
        <h3>AI for this Tender</h3>
        {policy.manager ? (
          <button
            type="button"
            className="text-button"
            aria-expanded={editing}
            onClick={() => setEditing((value) => !value)}
          >
            {editing ? "Close" : "Change"}
          </button>
        ) : null}
      </div>
      {policy.manager ? (
        retiredCurrent ? (
          <div className="ai-tender-summary">
            <p className="ai-warning">
              <strong>This Tender still records a retired AI account.</strong>
            </p>
            <p className="field-help">
              It remains in the Tender history, but it cannot be selected for
              new work. Open Settings → AI accounts and add an API account, then
              choose Change here.
            </p>
          </div>
        ) : (
          <div className="ai-tender-summary">
            <p>
              <strong>
                {currentConnection?.name ?? "Account unavailable"}
              </strong>{" "}
              · {currentModel?.display_name ?? policy.manager.model_id}
            </p>
            <p className="field-help">
              Tender content may be sent to{" "}
              {currentConnection
                ? dataDestination(currentConnection)
                : "the previously approved account"}
              .
            </p>
            {policy.tender_budget_usd != null ? (
              <p className="field-help">
                Budget for metered AI routes: USD{" "}
                {policy.tender_budget_usd.toLocaleString()} · Estimated
                allowance left: USD{" "}
                {Math.max(
                  0,
                  policy.tender_budget_usd -
                    policy.spent_usd -
                    policy.reserved_usd,
                ).toLocaleString(undefined, { maximumFractionDigits: 2 })}
              </p>
            ) : currentConnection ? (
              <p className="field-help">
                {connectionBillingDescription(currentConnection)}
              </p>
            ) : null}
            {currentConnection ? (
              <GrokSpendingNote connection={currentConnection} />
            ) : null}
            {extrasNeedApproval ? (
              <p className="ai-warning">
                The current Grok extras setting needs your approval for this
                Tender. Select Change, review the setting and confirm your AI
                choice before continuing work.
              </p>
            ) : null}
            {additionalTeam ? (
              <p className="field-help">
                This Tender has additional team choices. Review them in More
                options.
              </p>
            ) : null}
          </div>
        )
      ) : (
        <p className="field-help">
          Choose the account and model the Tender Manager and specialists will
          use.
        </p>
      )}
      {policy.spend_history_may_be_incomplete ? (
        <p className="ai-warning">
          This workspace was restored. Later spending may be missing.{" "}
          {policy.restore_reconciliation_required
            ? "Review your provider account before allowing further paid work."
            : "The budget review did not recover missing spending records."}
        </p>
      ) : null}
      {editing ? (
        <>
          <form
            className="ai-form ai-simple-policy"
            onSubmit={async (event) => {
              event.preventDefault();
              if (
                busy ||
                !account ||
                !model ||
                !modelChecked ||
                !budgetValid ||
                !restoreReady
              )
                return;
              setBusy(true);
              setError(null);
              try {
                await api.post<Schema<"TenderAIRecord">>(
                  `${tenderPath(tenderId)}/ai-setup`,
                  {
                    account_id: account.id,
                    ...(account.connection.protocol === "grok_build"
                      ? { account_revision: account.connection.revision }
                      : {}),
                    model_id: model.model_id,
                    budget_usd: metered ? Number(budget) : null,
                    restore_budget_reviewed: restored,
                    engineer_confirmed: true,
                  } satisfies Schema<"SimpleTenderAIInput">,
                );
                await refresh();
                setEditing(false);
              } catch (failure) {
                setError(failure);
              } finally {
                setBusy(false);
              }
            }}
          >
            <fieldset disabled={busy}>
              {!accountOptions.length ? (
                <p className="field-help">
                  Add an API account and complete its access check in Settings →
                  AI accounts. A retired account already recorded on this Tender
                  cannot be repaired here.
                </p>
              ) : (
                <label>
                  AI account
                  <select
                    required
                    value={accountId}
                    onChange={(event) => {
                      const next = accountOptions.find(
                        (item) => item.id === event.target.value,
                      );
                      setAccountId(event.target.value);
                      setModelId(
                        next?.selected_model_id ??
                          next?.recommended_model_id ??
                          "",
                      );
                      setRestored(false);
                      setChangeModel(false);
                    }}
                  >
                    <option value="">Choose an account</option>
                    {accountOptions.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.connection.name} · {item.method_title}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {account ? (
                <>
                  <div className="ai-model-choice">
                    <div className="section-heading">
                      <h3>Model</h3>
                      <button
                        type="button"
                        className="text-button"
                        aria-expanded={changeModel}
                        onClick={() => setChangeModel((value) => !value)}
                      >
                        Change
                      </button>
                    </div>
                    <p>
                      <strong>{model?.display_name ?? "Choose a model"}</strong>
                    </p>
                    <p className="field-help">{account.recommendation}</p>
                    {changeModel || !model ? (
                      <label>
                        Model
                        <select
                          required
                          value={modelId}
                          onChange={(event) => {
                            setModelId(event.target.value);
                            setRestored(false);
                          }}
                        >
                          <option value="">Choose a model</option>
                          {account.models.map((item) => (
                            <option key={item.model_id} value={item.model_id}>
                              {item.display_name}
                              {item.model_id === account.recommended_model_id
                                ? " · Suggested for quality"
                                : ""}
                            </option>
                          ))}
                        </select>
                      </label>
                    ) : null}
                  </div>
                  {!modelChecked ? (
                    <div className="ai-tender-check">
                      <p className="field-help">
                        Complete an access check for this model before using it
                        for this Tender. The check sends a short sample without
                        Tender content.
                      </p>
                      <button
                        type="button"
                        className="button"
                        disabled={busy}
                        onClick={async () => {
                          if (busy) return;
                          setBusy(true);
                          setError(null);
                          try {
                            if (
                              model &&
                              account.selected_model_id !== model.model_id
                            )
                              await api.post<Schema<"SetupAccount">>(
                                `/ai/setup/accounts/${encodeURIComponent(account.id)}/actions`,
                                {
                                  action: "select_model",
                                  model_id: model.model_id,
                                  accept_unknown_cost: false,
                                } satisfies Schema<"SetupAction">,
                              );
                            await refresh();
                            setCheckingAccount(account.id);
                          } catch (failure) {
                            setError(failure);
                          } finally {
                            setBusy(false);
                          }
                        }}
                      >
                        {busy
                          ? "Opening…"
                          : model
                            ? "Check this model"
                            : "Finish account setup"}
                      </button>
                    </div>
                  ) : null}
                  <div className="ai-data-permission">
                    <h3>Where Tender content goes</h3>
                    <p>{dataDestination(account.connection)}</p>
                    <p className="field-help">
                      This account and model will be used by the Tender Manager
                      and specialists. Quantix will not switch to another AI
                      without your approval.
                    </p>
                  </div>
                  <GrokSpendingNote connection={account.connection} />
                  {account.connection.protocol === "grok_build" ? (
                    <p className="field-help">
                      Grok can analyse Tender documents and carry out supported
                      specialist work. Grok web and X search are not available
                      in Quantix yet.
                    </p>
                  ) : null}
                  {metered ? (
                    <>
                      <label>
                        Tender AI budget (USD)
                        <input
                          type="number"
                          required
                          min="0.000001"
                          max={10000000}
                          step="any"
                          value={budget}
                          onChange={(event) => {
                            setBudget(event.target.value);
                            setRestored(false);
                          }}
                        />
                      </label>
                      <p className="field-help">
                        Your limit includes recorded spending and amounts held
                        for work in progress. Estimated costs may differ from
                        the provider’s bill.
                      </p>
                      {budgetValid ? (
                        <p className="field-help">
                          Estimated allowance left: USD{" "}
                          {Math.max(
                            0,
                            Number(budget) -
                              policy.spent_usd -
                              policy.reserved_usd,
                          ).toLocaleString(undefined, {
                            maximumFractionDigits: 6,
                          })}
                          .
                        </p>
                      ) : null}
                    </>
                  ) : account.connection.protocol !== "grok_build" ? (
                    <p className="field-help">
                      {connectionBillingDescription(account.connection)}. No
                      paid usage budget is required for this selection.
                    </p>
                  ) : null}
                  {otherMetered ? (
                    <p className="field-help">
                      Existing budgets for other metered AI routes are kept: per
                      run{" "}
                      {policy.run_budget_usd == null
                        ? "not set"
                        : `USD ${policy.run_budget_usd}`}
                      ; Tender total{" "}
                      {policy.tender_budget_usd == null
                        ? "not set"
                        : `USD ${policy.tender_budget_usd}`}
                      . These budgets do not cap Grok provider-managed extras.
                    </p>
                  ) : null}
                  {policy.restore_reconciliation_required && paidExposure ? (
                    <>
                      <label className="checkbox-label">
                        <input
                          type="checkbox"
                          required
                          checked={restored}
                          onChange={(event) =>
                            setRestored(event.target.checked)
                          }
                        />
                        I reviewed my provider accounts, including spending
                        missing after this restore, and approve the displayed
                        spending arrangements for further work.
                      </label>
                      {!restoreBudgetsReady ? (
                        <p className="ai-warning">
                          Set budgets for the other metered AI accounts before
                          resuming paid work.{" "}
                          <button
                            type="button"
                            className="text-button"
                            onClick={() => setAdvanced(true)}
                          >
                            Review budgets in More options
                          </button>
                        </p>
                      ) : null}
                    </>
                  ) : null}
                </>
              ) : null}
            </fieldset>
            <ErrorNotice error={error} />
            {account ? (
              <p className="field-help">
                Selecting the button records your approval to send this Tender’s
                content to the displayed account
                {metered ? " within this budget" : ""}
                {account.connection.protocol === "grok_build"
                  ? ` with the spending setting “${connectionBillingDescription(account.connection)}”`
                  : ""}
                .
              </p>
            ) : null}
            <button
              className="button primary"
              disabled={
                busy ||
                !account ||
                !model ||
                !modelChecked ||
                !budgetValid ||
                !restoreReady
              }
            >
              {busy
                ? "Saving…"
                : policy.restore_reconciliation_required && paidExposure
                  ? "Approve spending and use this AI"
                  : "Use this AI for this Tender"}
            </button>
          </form>
          <details
            className="ai-more-options"
            open={advanced}
            onToggle={(event) => setAdvanced(event.currentTarget.open)}
          >
            <summary>More options</summary>
            {advanced ? (
              <PolicyForm
                tenderId={tenderId}
                policy={policy}
                connections={connections}
              />
            ) : null}
          </details>
        </>
      ) : null}
      {checkingAccount ? (
        <AccountSetup accountId={checkingAccount} onClose={closeAccount} />
      ) : null}
    </div>
  );
}
function PolicyForm({
  tenderId,
  policy,
  connections,
}: {
  tenderId: string;
  policy: Schema<"TenderAIRecord">;
  connections: Schema<"ConnectionRecord">[];
}) {
  const api = useApi(),
    refresh = useRefresh();
  const tasks = useResource<Schema<"Task">[]>(`${tenderPath(tenderId)}/tasks`);
  const [allowed, setAllowed] = useState(policy.allowed_connection_ids),
    [manager, setManager] = useState(policy.manager),
    [specialist, setSpecialist] = useState(policy.specialist);
  const [roles, setRoles] = useState(() =>
    Object.entries(policy.role_routes).map(([role, route], index) => ({
      key: index + 1,
      role,
      route: route as Route | null,
    })),
  );
  const [fallbacks, setFallbacks] = useState<(Route | null)[]>(
    policy.fallback_routes,
  );
  const [runBudget, setRunBudget] = useState(
      policy.run_budget_usd?.toString() ?? "",
    ),
    [tenderBudget, setTenderBudget] = useState(
      policy.tender_budget_usd?.toString() ?? "",
    ),
    [maxRequests, setMaxRequests] = useState(String(policy.max_requests));
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>(null),
    [notice, setNotice] = useState("");
  const [restoreReviewed, setRestoreReviewed] = useState(false);
  const available = connections.filter((connection) =>
    allowed.includes(connection.id),
  );
  const grokConnections = available.filter(
    (connection) => connection.protocol === "grok_build",
  );
  const extrasApprovals = Object.fromEntries(
    grokConnections
      .filter(
        (connection) =>
          connection.settings.allow_provider_managed_extras === true,
      )
      .map((connection) => [connection.id, connection.revision]),
  );
  const routes = [
    manager,
    specialist,
    ...roles.map((role) => role.route),
    ...fallbacks,
  ].filter((route): route is Route => route !== null);
  const metered = routes.some((route) => {
    const connection = connections.find(
      (item) => item.id === route.connection_id,
    );
    return !connection || ["metered", "unknown"].includes(connection.billing);
  });
  const incomplete =
    routes.some(
      (route) =>
        !route.connection_id ||
        !route.model_id ||
        !allowed.includes(route.connection_id),
    ) ||
    roles.some((role) => !role.role.trim() || !role.route) ||
    new Set(roles.map((role) => role.role.trim())).size !== roles.length ||
    fallbacks.some((route) => !route);
  const paidExposure =
    metered ||
    routes.some((route) =>
      grokConnections.some(
        (connection) =>
          connection.id === route.connection_id &&
          connection.settings.allow_provider_managed_extras === true,
      ),
    );
  const requiresBudgets = metered;
  const budgetsValid =
    (!requiresBudgets || (Number(runBudget) > 0 && Number(tenderBudget) > 0)) &&
    (runBudget === "" ||
      (Number.isFinite(Number(runBudget)) &&
        Number(runBudget) > 0 &&
        Number(runBudget) <= 1000000)) &&
    (tenderBudget === "" ||
      (Number.isFinite(Number(tenderBudget)) &&
        Number(tenderBudget) > 0 &&
        Number(tenderBudget) <= 10000000));
  const recoveryReady =
    !paidExposure || !policy.restore_reconciliation_required || restoreReviewed;
  const changed = () => {
    setRestoreReviewed(false);
    setNotice("");
  };
  return (
    <form
      className="ai-form"
      onSubmit={async (event) => {
        event.preventDefault();
        if (busy || incomplete || !budgetsValid || !recoveryReady) return;
        setBusy(true);
        setError(null);
        setNotice("");
        try {
          await api.put<Schema<"TenderAIRecord">>(
            `${tenderPath(tenderId)}/ai-policy`,
            {
              allowed_connection_ids: allowed,
              ...(grokConnections.length
                ? { provider_managed_extras: extrasApprovals }
                : {}),
              manager,
              specialist,
              role_routes: Object.fromEntries(
                roles
                  .filter((role) => role.route)
                  .map((role) => [role.role.trim(), role.route!]),
              ),
              fallback_routes: fallbacks.filter(
                (route): route is Route => route !== null,
              ),
              run_budget_usd: runBudget ? Number(runBudget) : null,
              tender_budget_usd: tenderBudget ? Number(tenderBudget) : null,
              max_requests: Number(maxRequests),
              restore_budget_reviewed: restoreReviewed,
              engineer_confirmed: true,
              rationale:
                "Engineer approved the displayed AI accounts, team selections, alternatives, Grok spending preferences and usage limits.",
            } satisfies Schema<"TenderAIInput">,
          );
          await refresh();
          setNotice("Tender AI permissions and budgets saved.");
        } catch (failure) {
          setError(failure);
        } finally {
          setBusy(false);
        }
      }}
    >
      <p className="muted">
        Choose which connections may receive this tender's content. Review their
        data terms in Settings. Each work plan shows its proposed AI team before
        approval.
      </p>
      <fieldset disabled={busy}>
        <legend>Connections allowed for this tender</legend>
        {!connections.length ? (
          <p className="field-help">
            Add an AI connection and its models in Settings first.
          </p>
        ) : null}
        {connections.map((connection) => (
          <label
            key={connection.id}
            className="ai-connection-permission checkbox-label"
          >
            <input
              type="checkbox"
              checked={allowed.includes(connection.id)}
              disabled={!connection.enabled && !allowed.includes(connection.id)}
              onChange={(event) => {
                changed();
                if (event.target.checked)
                  setAllowed((current) => [...current, connection.id]);
                else {
                  setAllowed((current) =>
                    current.filter((id) => id !== connection.id),
                  );
                  if (manager?.connection_id === connection.id)
                    setManager(null);
                  if (specialist?.connection_id === connection.id)
                    setSpecialist(null);
                  setRoles((current) =>
                    current.filter(
                      (role) => role.route?.connection_id !== connection.id,
                    ),
                  );
                  setFallbacks((current) =>
                    current.filter(
                      (route) => route?.connection_id !== connection.id,
                    ),
                  );
                  setNotice(
                    "Routes using that connection were removed from this draft.",
                  );
                }
              }}
            />
            <span>
              <strong>{connection.name}</strong>
              <span className="field-help">
                {connection.provider_id} · {connection.billing} billing ·{" "}
                {connection.base_url ?? "Original client runtime"}
                {connection.enabled ? "" : " · Disabled"}
              </span>
            </span>
          </label>
        ))}
        {allowed
          .filter(
            (id) => !connections.some((connection) => connection.id === id),
          )
          .map((id) => (
            <div className="ai-warning" key={id}>
              A previously allowed connection is unavailable.{" "}
              <button
                type="button"
                className="text-button"
                onClick={() => {
                  changed();
                  setAllowed((current) =>
                    current.filter((value) => value !== id),
                  );
                  if (manager?.connection_id === id) setManager(null);
                  if (specialist?.connection_id === id) setSpecialist(null);
                  setRoles((current) =>
                    current.filter((role) => role.route?.connection_id !== id),
                  );
                  setFallbacks((current) =>
                    current.filter((route) => route?.connection_id !== id),
                  );
                }}
              >
                Remove unavailable permission and routes
              </button>
            </div>
          ))}
        <RouteEditor
          label="Tender Manager"
          value={manager}
          connections={available}
          onChange={(route) => {
            changed();
            setManager(route);
          }}
        />
        <RouteEditor
          label="Default specialist"
          value={specialist}
          connections={available}
          onChange={(route) => {
            changed();
            setSpecialist(route);
          }}
          emptyLabel="Use the Tender Manager route"
        />
        <details className="ai-route-overrides">
          <summary>Routes for specific roles</summary>
          <p className="field-help">
            Match the role recorded in a work plan. Unlisted roles use the
            default specialist, then the Manager route.
          </p>
          <ErrorNotice error={tasks.error} />
          <datalist id={`ai-roles-${tenderId}`}>
            {[...new Set(tasks.data?.map((task) => task.role) ?? [])].map(
              (role) => (
                <option key={role} value={role} />
              ),
            )}
          </datalist>
          {roles.map((entry, index) => (
            <div className="ai-role-entry" key={entry.key}>
              <label>
                Specialist role
                <input
                  list={`ai-roles-${tenderId}`}
                  required
                  value={entry.role}
                  onChange={(event) => {
                    changed();
                    setRoles((current) =>
                      current.map((role) =>
                        role.key === entry.key
                          ? { ...role, role: event.target.value }
                          : role,
                      ),
                    );
                  }}
                />
              </label>
              <RouteEditor
                label={`Route for role ${index + 1}`}
                value={entry.route}
                connections={available}
                onChange={(route) => {
                  changed();
                  setRoles((current) =>
                    current.map((role) =>
                      role.key === entry.key ? { ...role, route } : role,
                    ),
                  );
                }}
              />
              <button
                type="button"
                className="text-button"
                onClick={() => {
                  changed();
                  setRoles((current) =>
                    current.filter((role) => role.key !== entry.key),
                  );
                }}
              >
                Remove role route
              </button>
            </div>
          ))}
          <button
            type="button"
            className="button"
            onClick={() => {
              changed();
              setRoles((current) => [
                ...current,
                {
                  key: Math.max(0, ...current.map((role) => role.key)) + 1,
                  role: "",
                  route: null,
                },
              ]);
            }}
          >
            Add role route
          </button>
        </details>
        <details className="ai-route-overrides">
          <summary>Saved alternatives for manual selection</summary>
          <p className="field-help">
            These account and model choices are retained for review. Quantix
            pauses if the selected AI cannot continue. Choose and approve a
            different AI yourself before resuming work; saving these
            alternatives does not enable automatic switching.
          </p>
          {fallbacks.map((route, index) => (
            <div className="ai-role-entry" key={index}>
              <RouteEditor
                label={`Saved alternative ${index + 1}`}
                value={route}
                connections={available}
                onChange={(next) => {
                  changed();
                  setFallbacks((current) =>
                    current.map((item, position) =>
                      position === index ? next : item,
                    ),
                  );
                }}
              />
              <div className="inline-actions">
                <button
                  type="button"
                  className="text-button"
                  disabled={index === 0}
                  onClick={() => {
                    changed();
                    setFallbacks((current) => {
                      const next = [...current];
                      [next[index - 1], next[index]] = [
                        next[index],
                        next[index - 1],
                      ];
                      return next;
                    });
                  }}
                >
                  Move earlier
                </button>
                <button
                  type="button"
                  className="text-button"
                  onClick={() => {
                    changed();
                    setFallbacks((current) =>
                      current.filter((_, position) => position !== index),
                    );
                  }}
                >
                  Remove saved alternative
                </button>
              </div>
            </div>
          ))}
          <button
            type="button"
            className="button"
            disabled={fallbacks.length >= 5}
            onClick={() => {
              changed();
              setFallbacks((current) => [...current, null]);
            }}
          >
            Add saved alternative
          </button>
        </details>
        <h3>Approved usage limits</h3>
        <p className="field-help">
          Recorded estimated spend for metered AI routes: USD{" "}
          {policy.spent_usd.toLocaleString(undefined, {
            maximumFractionDigits: 6,
          })}{" "}
          · Reserved: USD{" "}
          {policy.reserved_usd.toLocaleString(undefined, {
            maximumFractionDigits: 6,
          })}
          . Metered routes need recorded model prices and both budgets.
        </p>
        {policy.spend_history_may_be_incomplete ? (
          <p className="ai-warning">
            This workspace was restored. Spending after the backup may be
            missing from these figures. Review the provider accounts before
            setting an allowance for further work.
          </p>
        ) : null}
        {Number.isFinite(Number(tenderBudget)) && Number(tenderBudget) > 0 ? (
          <p className="field-help">
            Allowance left under the entered Tender limit: USD{" "}
            {Math.max(
              0,
              Number(tenderBudget) - policy.spent_usd - policy.reserved_usd,
            ).toLocaleString(undefined, { maximumFractionDigits: 6 })}
            . Recorded spending and reservations are already deducted. Missing
            spending after a restore must also be considered when choosing this
            limit.
          </p>
        ) : null}
        {policy.restore_reconciliation_required && paidExposure ? (
          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={restoreReviewed}
              onChange={(event) => setRestoreReviewed(event.target.checked)}
            />
            I reviewed provider accounts, including spending missing from this
            record, and approve the displayed spending arrangements. API budgets
            include recorded spending and reservations. Grok extras follow its
            account settings. This review does not reconstruct missing usage.
          </label>
        ) : null}
        <div className="ai-form-grid">
          <label>
            Budget per run (USD)
            <input
              type="number"
              min="0.000001"
              max={1000000}
              step="any"
              required={requiresBudgets}
              value={runBudget}
              onChange={(event) => {
                changed();
                setRunBudget(event.target.value);
              }}
            />
          </label>
          <label>
            Total tender budget (USD)
            <input
              type="number"
              min="0.000001"
              max={10000000}
              step="any"
              required={requiresBudgets}
              value={tenderBudget}
              onChange={(event) => {
                changed();
                setTenderBudget(event.target.value);
              }}
            />
          </label>
          <label>
            Maximum model requests per run
            <input
              type="number"
              min={1}
              max={100}
              step={1}
              required
              value={maxRequests}
              onChange={(event) => {
                changed();
                setMaxRequests(event.target.value);
              }}
            />
          </label>
        </div>
        {grokConnections.map((connection) => (
          <section className="ai-grok-approval" key={connection.id}>
            <h3>{connection.name}</h3>
            <GrokSpendingNote connection={connection} />
          </section>
        ))}
        <p className="field-help">
          Selecting the button approves these accounts receiving this Tender’s
          content, the displayed team and alternatives, and these usage limits.
          {grokConnections.length
            ? " It also approves the displayed Grok spending preferences. Provider-managed extras follow Grok’s account settings and have no guaranteed separate Tender cap."
            : ""}
        </p>
      </fieldset>
      <ErrorNotice error={error} />
      {notice ? (
        <p role="status" className="field-help">
          {notice}
        </p>
      ) : null}
      <button
        className="button primary"
        disabled={
          busy ||
          incomplete ||
          !budgetsValid ||
          !recoveryReady ||
          !/^\d+$/.test(maxRequests) ||
          Number(maxRequests) < 1 ||
          Number(maxRequests) > 100
        }
      >
        {busy ? "Saving…" : "Approve these AI choices"}
      </button>
    </form>
  );
}

function RouteEditor({
  label,
  value,
  connections,
  onChange,
  emptyLabel = "No route selected",
}: {
  label: string;
  value: Route | null;
  connections: Schema<"ConnectionRecord">[];
  onChange: (route: Route | null) => void;
  emptyLabel?: string;
}) {
  const api = useApi();
  const path = `/ai/connections/${value?.connection_id}/models`;
  const models = useQuery({
    queryKey: [path],
    queryFn: () => api.get<Schema<"ModelRecord">[]>(path),
    enabled: !!value?.connection_id,
  });
  const model = models.data?.find((item) => item.model_id === value?.model_id);
  return (
    <fieldset className="ai-route-editor">
      <legend>{label}</legend>
      <label>
        Connection
        <select
          value={value?.connection_id ?? ""}
          onChange={(event) =>
            onChange(event.target.value ? newRoute(event.target.value) : null)
          }
        >
          <option value="">{emptyLabel}</option>
          {connections.map((connection) => (
            <option
              key={connection.id}
              value={connection.id}
              disabled={!connection.enabled}
            >
              {connection.name} · {connection.billing}
            </option>
          ))}
        </select>
      </label>
      {value ? (
        <>
          <ErrorNotice error={models.error} />
          {models.isPending ? <Loading>Loading saved models…</Loading> : null}
          <label>
            Model
            <select
              required
              value={value.model_id}
              onChange={(event) =>
                onChange({ ...value, model_id: event.target.value })
              }
            >
              <option value="">Choose a saved model</option>
              {value.model_id && !model ? (
                <option value={value.model_id}>
                  {value.model_id} · Unavailable in the saved catalog
                </option>
              ) : null}
              {models.data?.map((item) => (
                <option key={item.model_id} value={item.model_id}>
                  {item.display_name} · {item.model_id}
                </option>
              ))}
            </select>
          </label>
          {model?.capabilities?.tools !== true ? (
            <p className="ai-warning">
              Tool support must be confirmed in AI Connections before this model
              can run Tender Office work.
            </p>
          ) : null}
          <GenerationControls
            connectionId={value.connection_id}
            modelId={value.model_id}
            reasoningLevels={model?.capabilities?.reasoning ?? []}
            value={{
              temperature: value.temperature ?? null,
              top_p: value.top_p ?? null,
              reasoning: value.reasoning ?? null,
              max_output_tokens: value.max_output_tokens,
              output_mode: value.output_mode ?? "auto",
              max_search_calls: value.max_search_calls,
            }}
            webSearch={value.web_search}
            onWebSearch={(enabled) =>
              onChange({ ...value, web_search: enabled })
            }
            onChange={(settings) => onChange({ ...value, ...settings })}
          />
          {models.data?.length === 0 ? (
            <p className="field-help">
              Discover or add models for this connection in Settings.
            </p>
          ) : null}
        </>
      ) : null}
    </fieldset>
  );
}
