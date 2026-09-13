import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  ChevronLeft,
  ChevronRight,
  FileSearch,
  LockKeyhole,
  Save,
  Search,
  ShieldCheck,
} from "lucide-react";
import { ApiError, errorText, tenderPath, useApi, type Schema } from "../api";
import {
  NativeToolsReview,
  nativeSelectionErrors,
  type NativeChoices,
} from "./AdvancedWorkTools";
import { CodeRuntimeReview } from "./CodeRuntimeReview";

export type DelegationReviewState = {
  dirty: boolean;
  editing: boolean;
  saving: boolean;
  conflict: boolean;
};

type DelegationReviewProps = {
  tenderId: string;
  planId: string;
  review: Schema<"PlanReview">;
  onReviewUpdated: (review: Schema<"PlanReview">) => void;
  onRefreshReview: () => Promise<Schema<"PlanReview"> | null>;
  onStateChange?: (state: DelegationReviewState) => void;
};

type DelegationDraft = {
  source_scope: Schema<"DelegationProposalEdit">["source_scope"];
  artifact_ids: string[];
  tool_ids: string[];
  native_tools: NativeChoices;
  code_runtimes: Schema<"ReviewedCodeRuntime">[];
  allowed_draft_outputs: string[];
  route_option_ids: string[];
  max_staff: number;
  max_assignments: number;
  max_depth: number;
  max_concurrency: number;
  max_requests: number;
  max_search_calls: number;
};

type ArtifactPage = Schema<"DelegationArtifactOptionPage">;

const ARTIFACT_PAGE_SIZE = 50;

export function DelegationReview({
  tenderId,
  planId,
  review,
  onReviewUpdated,
  onRefreshReview,
  onStateChange,
}: DelegationReviewProps) {
  const api = useApi();
  const options = review.delegation_options;
  const serverDraft = useMemo(() => draftFromReview(review), [review]);
  const [draft, setDraft] = useState<DelegationDraft>(() => serverDraft);
  const [editing, setEditing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [scopeLoading, setScopeLoading] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [saveError, setSaveError] = useState<unknown>(null);
  const [artifactPage, setArtifactPage] = useState<ArtifactPage | null>(() => {
    const initial = review.delegation_options?.artifacts ?? [];
    return {
      items: initial,
      next_offset:
        initial.length === ARTIFACT_PAGE_SIZE ? ARTIFACT_PAGE_SIZE : null,
      total: initial.length,
    };
  });
  const [artifactTotal, setArtifactTotal] = useState<number | null>(null);
  const [artifactOffset, setArtifactOffset] = useState(0);
  const [artifactQuery, setArtifactQuery] = useState("");
  const [artifactSearch, setArtifactSearch] = useState("");
  const [artifactLoading, setArtifactLoading] = useState(false);
  const [artifactError, setArtifactError] = useState<unknown>(null);

  const dirty = !sameDraft(draft, serverDraft);
  const selectedNativeRoutes = (options?.route_options ?? []).filter((route) =>
    draft.route_option_ids.includes(route.id),
  );
  const nativeErrors = nativeSelectionErrors(
    draft.native_tools,
    selectedNativeRoutes,
  );
  const historical = isHistoricalReview(review);
  const state: DelegationReviewState = {
    dirty,
    editing,
    saving: saving || scopeLoading,
    conflict,
  };

  useEffect(() => {
    onStateChange?.(state);
  }, [conflict, dirty, editing, onStateChange, saving, scopeLoading]);

  if (!options) {
    return (
      <section
        className="review-card delegation-review"
        aria-labelledby="delegation-review-title"
      >
        <DelegationHeading status={review.plan_status} />
        <div className="delegation-unavailable" role="status">
          <AlertTriangle size={18} />
          <p>
            {historical
              ? "This historical task review has no delegation scope. Request a new proposed work plan before using adaptive staffing."
              : "The current review does not contain a complete delegation envelope. Resolve the listed plan blockers before adaptive staffing can be reviewed or approved."}
          </p>
        </div>
      </section>
    );
  }

  const envelope = review.delegation;
  const artifactOptions = options.artifacts ?? [];
  const toolOptions = options.tools ?? [];
  const routeOptions = options.route_options ?? [];
  const availableArtifacts = artifactPage?.items ?? artifactOptions;
  const selectedArtifacts = new Set(draft.artifact_ids);
  const selectedTools = new Set(draft.tool_ids);
  const selectedRoutes = new Set(draft.route_option_ids);
  const selectedOutputs = new Set(draft.allowed_draft_outputs);
  const sourceCount = draft.artifact_ids.length;
  const displayedScope =
    draft.source_scope === "reviewed_tender"
      ? envelope
        ? `All ${sourceCount} current reviewed source${sourceCount === 1 ? "" : "s"}`
        : "Whole current Tender · scope needs review"
      : `${sourceCount} selected current source${sourceCount === 1 ? "" : "s"}`;

  function setValue<K extends keyof DelegationDraft>(
    key: K,
    value: DelegationDraft[K],
  ) {
    setDraft((current) => ({ ...current, [key]: value }));
    setSaveError(null);
    setConflict(false);
  }

  async function setSourceScope(value: DelegationDraft["source_scope"]) {
    if (scopeLoading || saving) return;
    setSaveError(null);
    setConflict(false);
    if (value === "selected_sources") {
      setDraft((current) => ({ ...current, source_scope: value }));
      return;
    }
    setScopeLoading(true);
    try {
      const ids = new Set<string>();
      let offset: number | null = 0;
      while (offset !== null) {
        const next: ArtifactPage = await api.get<ArtifactPage>(
          `${tenderPath(tenderId)}/plans/${encodeURIComponent(planId)}/delegation/artifacts?offset=${offset}&limit=50`,
        );
        if (next.total > 500)
          throw new Error(
            "This Tender exceeds the current 500-file work scope. Choose Selected sources for this assignment.",
          );
        for (const item of next.items ?? []) ids.add(item.artifact_id);
        if (
          ids.size > 500 ||
          (next.next_offset != null &&
            (next.next_offset <= offset || next.next_offset >= 500))
        )
          throw new Error(
            "The source list changed. Reload it before selecting the whole Tender.",
          );
        offset = next.next_offset ?? null;
      }
      setDraft((current) => ({
        ...current,
        source_scope: value,
        artifact_ids: [...ids],
      }));
    } catch (failure) {
      setSaveError(failure);
    } finally {
      setScopeLoading(false);
    }
  }

  function toggleValue(
    key:
      | "artifact_ids"
      | "tool_ids"
      | "route_option_ids"
      | "allowed_draft_outputs",
    id: string,
  ) {
    const current = new Set(draft[key]);
    if (current.has(id)) current.delete(id);
    else current.add(id);
    setValue(key, [...current]);
    if (key === "route_option_ids")
      setDraft((value) => ({
        ...value,
        native_tools: Object.fromEntries(
          Object.entries(value.native_tools).filter(([id]) => current.has(id)),
        ),
      }));
  }

  async function loadArtifacts(offset: number, query = artifactSearch) {
    setArtifactLoading(true);
    setArtifactError(null);
    try {
      const params = new URLSearchParams({
        offset: String(offset),
        limit: String(ARTIFACT_PAGE_SIZE),
      });
      if (query.trim()) params.set("query", query.trim());
      const page = await api.get<ArtifactPage>(
        `${tenderPath(tenderId)}/plans/${encodeURIComponent(planId)}/delegation/artifacts?${params.toString()}`,
      );
      setArtifactPage(page);
      setArtifactTotal(page.total);
      setArtifactOffset(offset);
    } catch (failure) {
      setArtifactError(failure);
    } finally {
      setArtifactLoading(false);
    }
  }

  async function save() {
    if (!dirty || saving || scopeLoading || nativeErrors.length) return;
    setSaving(true);
    setSaveError(null);
    setConflict(false);
    try {
      await api.patch<Schema<"DelegationProposal">>(
        `${tenderPath(tenderId)}/plans/${encodeURIComponent(planId)}/delegation`,
        {
          expected_version: review.delegation_proposal_version,
          source_scope: draft.source_scope,
          artifact_ids: draft.artifact_ids,
          tool_ids: draft.tool_ids,
          ...(Object.keys(draft.native_tools).length
            ? { native_tools: draft.native_tools }
            : {}),
          ...(draft.code_runtimes.length
            ? { code_runtimes: draft.code_runtimes }
            : {}),
          allowed_draft_outputs: draft.allowed_draft_outputs,
          route_option_ids: draft.route_option_ids,
          max_staff: draft.max_staff,
          max_assignments: draft.max_assignments,
          max_depth: draft.max_depth,
          max_concurrency: draft.max_concurrency,
          max_requests: draft.max_requests,
          max_search_calls: draft.max_search_calls,
        } satisfies Schema<"DelegationProposalEdit">,
      );
      const latest = await onRefreshReview();
      if (!latest) {
        setSaveError(
          new Error(
            "The delegation choices were saved, but the current review could not be reloaded. Reload the review before approving.",
          ),
        );
        return;
      }
      onReviewUpdated(latest);
      setEditing(false);
      setConflict(false);
    } catch (failure) {
      setSaveError(failure);
      if (isConflict(failure)) setConflict(true);
    } finally {
      setSaving(false);
    }
  }

  async function reloadReview() {
    const latest = await onRefreshReview();
    if (latest) {
      onReviewUpdated(latest);
      setConflict(false);
      setSaveError(null);
    }
  }

  function discardDraft() {
    setDraft(draftFromReview(review));
    setEditing(false);
    setConflict(false);
    setSaveError(null);
  }

  return (
    <section
      className="review-card delegation-review"
      aria-labelledby="delegation-review-title"
    >
      <DelegationHeading status={review.plan_status} />
      <p className="delegation-lead">
        {historical ? (
          review.ai_summary
        ) : (
          <>
            The Tender Manager starts one run and creates or assigns colleagues
            as needed within this displayed scope and the shared limits below.
            Staff do not receive independent budgets or hidden permissions.
          </>
        )}
      </p>

      {conflict ? (
        <div className="delegation-conflict" role="alert">
          <AlertTriangle size={18} />
          <div>
            <strong>
              These delegation choices changed while you were editing.
            </strong>
            <p>
              Your edits are still here. Reload the current review to compare
              them before saving again.
            </p>
            <div className="inline-actions">
              <button
                type="button"
                className="text-button"
                onClick={() => void reloadReview()}
                disabled={saving}
              >
                Reload current review
              </button>
              <button
                type="button"
                className="text-button"
                onClick={discardDraft}
                disabled={saving}
              >
                Discard my draft
              </button>
            </div>
          </div>
        </div>
      ) : null}
      {saveError ? (
        <div className="delegation-error" role="alert">
          {errorText(saveError)}
        </div>
      ) : null}

      <div className="delegation-summary-grid">
        <div>
          <span className="delegation-label">Purpose</span>
          <p>{envelope?.purpose ?? "Current Tender work plan"}</p>
        </div>
        <div>
          <span className="delegation-label">Reviewed source scope</span>
          <p>{displayedScope}</p>
          <span className="field-help">
            {draft.source_scope === "reviewed_tender"
              ? "Assigned staff can inspect the current files in this reviewed Tender scope."
              : "Assigned staff can inspect only these selected current files. New or revised files require a fresh review."}
          </span>
        </div>
        <div>
          <span className="delegation-label">Shared work limits</span>
          <p>
            {draft.max_staff} staff · {draft.max_assignments} assignments
          </p>
          <span className="field-help">
            {draft.max_requests} requests · {draft.max_search_calls} online
            searches
          </span>
        </div>
        <div>
          <span className="delegation-label">Aggregate budgets</span>
          <p>
            Run {money(envelope?.run_budget_usd)} · Tender{" "}
            {money(envelope?.tender_budget_usd)}
          </p>
          <span className="field-help">
            One shared root allowance covers the Manager and colleagues.
          </span>
        </div>
      </div>

      <section
        className="delegation-list-section"
        aria-labelledby="delegation-routes-title"
      >
        <div className="delegation-section-heading">
          <div>
            <h3 id="delegation-routes-title">Displayed route options</h3>
            <p className="field-help">
              Only selected options can be used for this approved run.
            </p>
          </div>
          <ShieldCheck size={19} aria-hidden="true" />
        </div>
        <div className="delegation-route-list">
          {routeOptions.map((route) => {
            const selected = selectedRoutes.has(route.id);
            return (
              <div
                className={`delegation-option ${selected ? "selected" : ""}`}
                key={route.id}
              >
                <input
                  type="checkbox"
                  aria-label={`Use ${route.account_name} · ${modelLabel(route)}`}
                  checked={selected}
                  disabled={!editing || saving}
                  onChange={() => toggleValue("route_option_ids", route.id)}
                />
                <span className="delegation-option-copy">
                  <strong>
                    {route.account_name} · {modelLabel(route)}
                  </strong>
                  <span>
                    {destinationLabel(route.data_destination)} ·{" "}
                    {billingLabel(route.billing)}
                  </span>
                  {route.provider_managed_extras ? (
                    <span className="warning-text">
                      Provider managed extras are enabled; charges may exceed
                      subscription limits.
                    </span>
                  ) : null}
                  <small className="delegation-route-detail">
                    {route.readiness === "ready"
                      ? "Ready"
                      : `Needs attention: ${route.readiness}`}{" "}
                    · {route.provider}
                  </small>
                  <details className="delegation-route-advanced">
                    <summary>Route details</summary>
                    <small>
                      Connection revision {route.connection_revision} · Model
                      revision {route.model_revision}
                      <br />
                      Reasoning {route.route.reasoning || "Provider default"} ·
                      Output {route.route.max_output_tokens.toLocaleString()}{" "}
                      tokens
                      {route.route.web_search
                        ? ` · Hosted search up to ${route.route.max_search_calls} calls`
                        : ""}
                    </small>
                  </details>
                </span>
              </div>
            );
          })}
        </div>
      </section>

      <section
        className="delegation-list-section"
        aria-labelledby="delegation-tools-title"
      >
        <div className="delegation-section-heading">
          <div>
            <h3 id="delegation-tools-title">Allowed Tender tools</h3>
            <p className="field-help">
              These are the actual supported operations available inside the
              reviewed scope.
            </p>
          </div>
          <LockKeyhole size={19} aria-hidden="true" />
        </div>
        <div className="delegation-check-list">
          {toolOptions.map((tool) => (
            <label
              className={`delegation-tool ${selectedTools.has(tool.id) ? "selected" : ""}`}
              key={tool.id}
            >
              <input
                type="checkbox"
                checked={selectedTools.has(tool.id)}
                disabled={!editing || saving}
                onChange={() => toggleValue("tool_ids", tool.id)}
              />
              <span>
                <strong>{toolLabel(tool.id)}</strong>
                <span>{tool.description}</span>
                <small>
                  {tool.read_only ? "Read only" : "Draft operation"} · version{" "}
                  {tool.version}
                </small>
              </span>
            </label>
          ))}
          {draft.tool_ids
            .filter((id) => !toolOptions.some((tool) => tool.id === id))
            .map((id) => (
              <label
                className="delegation-tool selected"
                key={`requested:${id}`}
              >
                <input
                  type="checkbox"
                  checked
                  disabled={!editing || saving}
                  onChange={() => toggleValue("tool_ids", id)}
                />
                <span>
                  <strong>{toolLabel(id)}</strong>
                  <span>Requested by a selected method.</span>
                  <small>Review this capability before saving the draft.</small>
                </span>
              </label>
            ))}
        </div>
      </section>

      <section
        className="delegation-list-section"
        aria-labelledby="delegation-outputs-title"
      >
        <div className="delegation-section-heading">
          <div>
            <h3 id="delegation-outputs-title">Allowed draft outputs</h3>
            <p className="field-help">
              Colleagues may prepare drafts in these kinds; an engineer still
              reviews and accepts every result.
            </p>
          </div>
          <FileSearch size={19} aria-hidden="true" />
        </div>
        <div className="delegation-output-list">
          {(options.supported_draft_outputs?.length
            ? options.supported_draft_outputs
            : (envelope?.allowed_draft_outputs ?? [])
          ).map((kind) => (
            <label className="delegation-output" key={kind}>
              <input
                type="checkbox"
                checked={selectedOutputs.has(kind)}
                disabled={!editing || saving}
                onChange={() => toggleValue("allowed_draft_outputs", kind)}
              />
              <span>{outputLabel(kind)}</span>
            </label>
          ))}
        </div>
      </section>
      <NativeToolsReview
        routes={selectedNativeRoutes}
        artifacts={artifactPage?.items ?? options.artifacts ?? []}
        allowedArtifactIds={draft.artifact_ids}
        value={draft.native_tools}
        disabled={!editing || saving}
        onChange={(native_tools) =>
          setDraft((value) => ({ ...value, native_tools }))
        }
      />
      <CodeRuntimeReview
        options={options.code_runtimes ?? []}
        value={draft.code_runtimes}
        disabled={!editing || saving}
        onChange={(code_runtimes, requiredToolIds) => {
          setDraft((value) => ({
            ...value,
            code_runtimes,
            tool_ids: [...new Set([...value.tool_ids, ...requiredToolIds])],
          }));
          setSaveError(null);
          setConflict(false);
        }}
      />

      {editing && !historical ? (
        <section
          className="delegation-editor"
          aria-labelledby="delegation-editor-title"
        >
          <div className="delegation-section-heading">
            <div>
              <h3 id="delegation-editor-title">Edit reviewed choices</h3>
              <p className="field-help">
                Changes stay local until you choose Save delegation choices.
              </p>
            </div>
            <Save size={19} aria-hidden="true" />
          </div>

          <fieldset className="delegation-scope-options">
            <legend>Source scope</legend>
            <label>
              <input
                type="radio"
                name={`delegation-scope-${planId}`}
                checked={draft.source_scope === "reviewed_tender"}
                disabled={saving || scopeLoading}
                onChange={() => void setSourceScope("reviewed_tender")}
              />
              <span>
                <strong>Whole reviewed Tender</strong>
                <small>Load all current source files in this Tender.</small>
              </span>
            </label>
            <label>
              <input
                type="radio"
                name={`delegation-scope-${planId}`}
                checked={draft.source_scope === "selected_sources"}
                disabled={saving || scopeLoading}
                onChange={() => void setSourceScope("selected_sources")}
              />
              <span>
                <strong>Selected sources</strong>
                <small>
                  Choose the exact current artifacts below. Later pages remain
                  searchable.
                </small>
              </span>
            </label>
          </fieldset>
          {scopeLoading ? (
            <p role="status">Loading the complete current source list…</p>
          ) : null}

          <div className="delegation-source-picker">
            <div className="delegation-source-picker-heading">
              <div>
                <h4>Current source artifacts</h4>
                <p className="field-help">
                  {artifactTotal != null
                    ? `${artifactTotal} current artifacts match this search.`
                    : "The review starts with a bounded first page; search or page through the current catalog to find later sources."}
                </p>
              </div>
              <span className="delegation-selection-count">
                {selectedArtifacts.size} selected
              </span>
            </div>
            <form
              className="delegation-search"
              onSubmit={(event) => {
                event.preventDefault();
                setArtifactSearch(artifactQuery);
                void loadArtifacts(0, artifactQuery);
              }}
            >
              <label htmlFor={`delegation-artifact-search-${planId}`}>
                Search current sources
              </label>
              <div>
                <input
                  id={`delegation-artifact-search-${planId}`}
                  value={artifactQuery}
                  onChange={(event) => setArtifactQuery(event.target.value)}
                  placeholder="Search file name or path"
                />
                <button
                  type="submit"
                  className="button"
                  disabled={artifactLoading}
                >
                  <Search size={16} /> Search
                </button>
              </div>
            </form>
            {artifactError ? (
              <div className="delegation-error" role="alert">
                {errorText(artifactError)}
              </div>
            ) : null}
            <div className="delegation-artifact-list" aria-live="polite">
              {availableArtifacts.map((artifact) => (
                <label
                  className={`delegation-artifact ${selectedArtifacts.has(artifact.artifact_id) ? "selected" : ""}`}
                  key={artifact.artifact_id}
                >
                  <input
                    type="checkbox"
                    checked={selectedArtifacts.has(artifact.artifact_id)}
                    disabled={
                      saving || draft.source_scope === "reviewed_tender"
                    }
                    onChange={() =>
                      toggleValue("artifact_ids", artifact.artifact_id)
                    }
                  />
                  <span>
                    <strong>{artifact.display_name}</strong>
                    <small>
                      {artifact.relative_path} · v{artifact.version} ·{" "}
                      {artifact.status}
                    </small>
                  </span>
                </label>
              ))}
              {!availableArtifacts.length ? (
                <p className="field-help">
                  No current source artifacts match this search.
                </p>
              ) : null}
            </div>
            <div className="delegation-pagination">
              <button
                type="button"
                className="text-button"
                disabled={artifactLoading || artifactOffset === 0}
                onClick={() =>
                  void loadArtifacts(
                    Math.max(0, artifactOffset - ARTIFACT_PAGE_SIZE),
                  )
                }
              >
                <ChevronLeft size={16} /> Previous sources
              </button>
              <span>
                {artifactTotal != null
                  ? `${artifactOffset + 1}–${Math.min(artifactOffset + ARTIFACT_PAGE_SIZE, artifactTotal)} of ${artifactTotal}`
                  : `First page · ${availableArtifacts.length} shown`}
              </span>
              <button
                type="button"
                className="text-button"
                disabled={artifactLoading || !artifactPage?.next_offset}
                onClick={() =>
                  void loadArtifacts(
                    artifactPage?.next_offset ??
                      artifactOffset + ARTIFACT_PAGE_SIZE,
                  )
                }
              >
                Next sources <ChevronRight size={16} />
              </button>
            </div>
          </div>

          <div className="delegation-limit-grid">
            <LimitInput
              label="Maximum staff"
              value={draft.max_staff}
              min={1}
              max={100}
              onChange={(value) => setValue("max_staff", value)}
            />
            <LimitInput
              label="Maximum assignments"
              value={draft.max_assignments}
              min={1}
              max={1000}
              onChange={(value) => setValue("max_assignments", value)}
            />
            <LimitInput
              label="Maximum delegation depth"
              value={draft.max_depth}
              min={1}
              max={8}
              onChange={(value) => setValue("max_depth", value)}
            />
            <LimitInput
              label="Simultaneous assignments"
              value={draft.max_concurrency}
              min={1}
              max={8}
              onChange={(value) => setValue("max_concurrency", value)}
            />
            <LimitInput
              label="Maximum requests"
              value={draft.max_requests}
              min={1}
              max={10000}
              onChange={(value) => setValue("max_requests", value)}
            />
            <LimitInput
              label="Maximum online searches"
              value={draft.max_search_calls}
              min={0}
              max={10000}
              onChange={(value) => setValue("max_search_calls", value)}
            />
          </div>

          {envelope ? (
            <details className="delegation-advanced">
              <summary>More options</summary>
              <dl>
                <div>
                  <dt>Serial depth</dt>
                  <dd>{envelope.max_depth}</dd>
                </div>
                <div>
                  <dt>Concurrency</dt>
                  <dd>{envelope.max_concurrency}</dd>
                </div>
                <div>
                  <dt>Run allowance</dt>
                  <dd>{money(envelope.run_budget_usd)}</dd>
                </div>
                <div>
                  <dt>Tender allowance</dt>
                  <dd>{money(envelope.tender_budget_usd)}</dd>
                </div>
              </dl>
              <p className="field-help">
                These values are fixed by the current backend authority contract
                and are shown for review.
              </p>
            </details>
          ) : null}

          <div className="delegation-editor-actions">
            <button
              type="button"
              className="text-button"
              onClick={() => {
                setDraft(serverDraft);
                setEditing(false);
                setSaveError(null);
                setConflict(false);
              }}
              disabled={saving || scopeLoading}
            >
              Cancel edits
            </button>
            <button
              type="button"
              className="button primary"
              onClick={() => void save()}
              disabled={
                saving ||
                scopeLoading ||
                !dirty ||
                nativeErrors.length > 0 ||
                !draft.route_option_ids.length ||
                !draft.allowed_draft_outputs.length
              }
            >
              <Save size={16} />{" "}
              {saving ? "Saving…" : "Save delegation choices"}
            </button>
          </div>
        </section>
      ) : historical ? (
        <div className="delegation-review-actions delegation-historical-actions">
          <p className="field-help">
            {review.plan_status === "superseded"
              ? "This saved scope has been superseded and cannot authorize new assignments."
              : "These are the approved delegation limits. A new proposed plan is needed to change them."}
          </p>
        </div>
      ) : (
        <div className="delegation-review-actions">
          <p className="field-help">
            {dirty
              ? "You have unsaved delegation choices. Save or cancel them before approval."
              : "Review every scope, route and shared limit before the explicit approval action below."}
          </p>
          <button
            type="button"
            className="button"
            onClick={() => {
              setEditing(true);
              setSaveError(null);
            }}
            disabled={saving}
          >
            Edit delegation choices
          </button>
        </div>
      )}
    </section>
  );
}

function DelegationHeading({
  status,
}: {
  status: Schema<"PlanReview">["plan_status"];
}) {
  return (
    <div className="delegation-heading">
      <div>
        <h2 id="delegation-review-title">Delegation scope and shared limits</h2>
        <p className="field-help">
          {status === "proposed" || !status
            ? "The exact staff scope and shared limits the Tender Manager may use after approval."
            : "The saved staff scope and shared limits for this work plan."}
        </p>
      </div>
      <span className="delegation-review-badge">
        <Check size={15} />{" "}
        {status === "approved"
          ? "Approved scope"
          : status === "superseded"
            ? "Superseded review"
            : "Explicit review"}
      </span>
    </div>
  );
}

function isHistoricalReview(review: Schema<"PlanReview">) {
  return (
    review.plan_status === "approved" || review.plan_status === "superseded"
  );
}

function LimitInput({
  label,
  value,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
}) {
  const [text, setText] = useState(String(value));

  useEffect(() => {
    setText(String(value));
  }, [value]);

  return (
    <label>
      <span>{label}</span>
      <input
        type="number"
        min={min}
        max={max}
        step={1}
        value={text}
        onChange={(event) => {
          const nextText = event.target.value;
          setText(nextText);
          if (!nextText) return;
          const next = Number(nextText);
          if (Number.isFinite(next))
            onChange(Math.min(max, Math.max(min, Math.trunc(next))));
        }}
        onBlur={() => {
          if (!text.trim()) setText(String(value));
        }}
      />
      <small>
        Allowed {min.toLocaleString()}–{max.toLocaleString()}
      </small>
    </label>
  );
}

function draftFromReview(review: Schema<"PlanReview">): DelegationDraft {
  const proposal = review.delegation_options?.selected;
  const envelope = review.delegation;
  return {
    source_scope:
      proposal?.source_scope ?? envelope?.source_scope ?? "reviewed_tender",
    artifact_ids:
      proposal?.artifact_ids ??
      envelope?.artifacts?.map((item) => item.artifact_id) ??
      [],
    tool_ids:
      proposal?.tool_ids ??
      envelope?.tools?.map((item) => item.id) ??
      review.delegation_options?.tools?.map((item) => item.id) ??
      [],
    native_tools: proposal?.native_tools ?? envelope?.native_tools ?? {},
    code_runtimes: proposal?.code_runtimes ?? envelope?.code_runtimes ?? [],
    allowed_draft_outputs:
      proposal?.allowed_draft_outputs ??
      envelope?.allowed_draft_outputs ??
      review.delegation_options?.supported_draft_outputs ??
      [],
    route_option_ids:
      proposal?.route_option_ids ??
      envelope?.route_options?.map((item) => item.id) ??
      review.delegation_options?.route_options?.map((item) => item.id) ??
      [],
    max_staff: proposal?.max_staff ?? envelope?.max_staff ?? 4,
    max_assignments:
      proposal?.max_assignments ?? envelope?.max_assignments ?? 12,
    max_depth: proposal?.max_depth ?? envelope?.max_depth ?? 2,
    max_concurrency:
      proposal?.max_concurrency ?? envelope?.max_concurrency ?? 2,
    max_requests:
      proposal?.max_requests ??
      envelope?.max_requests ??
      review.snapshot.max_requests,
    max_search_calls:
      proposal?.max_search_calls ??
      envelope?.max_search_calls ??
      Math.min(
        10000,
        review.snapshot.max_requests *
          Math.max(
            0,
            ...(review.delegation_options?.route_options ?? []).map((item) =>
              item.route.web_search ? item.route.max_search_calls : 0,
            ),
          ),
      ),
  };
}

function sameDraft(left: DelegationDraft, right: DelegationDraft) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function isConflict(error: unknown) {
  return error instanceof ApiError
    ? error.status === 409
    : typeof error === "object" &&
        error !== null &&
        "status" in error &&
        error.status === 409;
}

function money(value: number | null | undefined) {
  return value == null ? "Not set" : `USD ${value.toLocaleString()}`;
}

function modelLabel(route: Schema<"DelegationRouteOption">) {
  return typeof route.model.display_name === "string"
    ? route.model.display_name
    : route.route.model_id;
}

function destinationLabel(destination: string) {
  if (destination === "Official grok_build client") return "xAI · Grok";
  if (destination === "Official codex client") return "OpenAI · ChatGPT";
  return destination;
}

function billingLabel(billing: Schema<"DelegationRouteOption">["billing"]) {
  return billing === "metered"
    ? "Metered API"
    : billing === "subscription"
      ? "Subscription"
      : billing === "local"
        ? "Local runtime"
        : "Unknown billing";
}

function toolLabel(id: string) {
  return id
    .split(/[._-]/u)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

function outputLabel(value: string) {
  if (value === "boq_item_proposals") return "Source BOQ row proposals";
  return value
    .split("_")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export default DelegationReview;
