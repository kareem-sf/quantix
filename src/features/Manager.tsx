import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { FileText, ListChecks, Play, Sparkles, Square } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { BrandMark } from "../app/BrandMark";
import { Empty, ErrorNotice, Loading } from "../components/common";
import { AnimatedBorder } from "@/components/ui/animated-border";
import { ArrowIcon } from "@/components/ui/animated-icons";
import { linkUnderline } from "@/components/ui/animated-link";
import { AnimatedNumber } from "@/components/ui/animated-number";
import { Button } from "@/components/ui/button";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item";
import { ProgressiveBlur } from "@/components/ui/progressive-blur";
import { Typewriter } from "@/components/ui/typewriter";
import { cn } from "@/lib/utils";
import { Composer } from "./Composer";
import { ManagerMessage } from "./ManagerMessage";
import { ModelPicker } from "./ModelPicker";
import { PendingMessage } from "./PendingMessage";
import { PlanReview } from "./PlanReview";
import { RunRow } from "./RunRow";
import { AnalysisStages } from "./AnalysisStages";
import { ThinkingPicker } from "./ThinkingPicker";
import { ManagerPlanning } from "./ManagerPlanning";
import type { SourceSelection } from "./Sources";
import { parseRouteContext } from "../navigation/routes";

type MessagePage = Schema<"MessagePage">;

const suggestions = [
  "summarise the scope and the key risks",
  "find BOQ items the drawings do not cover",
  "draft clarification questions for the client",
  "check what the submission must include",
];

export function Manager({
  overview,
  artifacts,
  settings,
  onImport,
  onSettings,
  onSource,
  onDocuments,
  onReviewDocuments,
  onRecord,
  onRepair,
  onCustomizeManager,
}: {
  overview: Schema<"Overview">;
  artifacts: Schema<"Artifact">[];
  settings?: Schema<"Settings">;
  onImport: () => void;
  onSettings: () => void;
  onSource: (source: SourceSelection) => void;
  onDocuments?: () => void;
  onReviewDocuments?: () => void | Promise<void>;
  onRecord?: (view: string, recordId: string) => void;
  onRepair?: (target: string) => void;
  onCustomizeManager?: () => void;
}) {
  const tenderId = overview.tender.id;
  const api = useApi();
  const refresh = useRefresh();
  const scrollRef = useRef<HTMLDivElement>(null);
  const atBottomRef = useRef(true);
  const cursorInitialized = useRef(false);
  const historyAnchor = useRef<{ id: string; top: number } | null>(null);
  const reviewInFlight = useRef(false);
  const reviewAttempt = useRef<string | null>(null);
  const policy = useResource<Schema<"TenderAIRecord">>(
    `${tenderPath(tenderId)}/ai-policy`,
  );
  const messages = useResource<Schema<"Message">[] | MessagePage>(
    `${tenderPath(tenderId)}/messages?limit=50`,
    true,
  );
  const runs = useResource<Schema<"Run">[]>(
    `${tenderPath(tenderId)}/runs`,
    true,
  );
  const pendingQuery = useResource<Schema<"PendingInstruction"> | null>(
    `${tenderPath(tenderId)}/pending-message`,
    true,
  );
  const [history, setHistory] = useState<Schema<"Message">[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [loadingEarlier, setLoadingEarlier] = useState(false);
  const [historyError, setHistoryError] = useState<unknown>(null);
  const [reviewPlanId, setReviewPlanId] = useState<string | null>(null);
  const [stopping, setStopping] = useState(false);
  const [stopError, setStopError] = useState<unknown>(null);
  const [stopNotice, setStopNotice] = useState("");
  const [reviewDocumentsError, setReviewDocumentsError] =
    useState<unknown>(null);
  const [reviewingDocuments, setReviewingDocuments] = useState(false);
  const [aiPickerOpen, setAiPickerOpen] = useState(false);

  useEffect(() => {
    const incoming = Array.isArray(messages.data)
      ? messages.data
      : (messages.data?.items ?? []);
    if (!messages.data) return;
    setHistory((current) => mergeMessages(current, incoming));
    if (!cursorInitialized.current) {
      setNextCursor(
        Array.isArray(messages.data)
          ? null
          : (messages.data.next_cursor ?? null),
      );
      cursorInitialized.current = true;
    }
  }, [messages.data]);

  const activeRuns = overview.active_runs;
  const runList = Array.isArray(runs.data) ? runs.data : [];
  const attachedRunIds = new Set(
    history
      .filter((message) => message.role === "engineer" && message.run_id)
      .map((message) => message.run_id!),
  );
  // Registering and analysing a package show their stages instead of a generic line.
  const packageRun = activeRuns.find((run) =>
    ["import", "analysis"].includes(run.kind),
  );
  const latestRun = runList
    .filter((run) =>
      ["manager", "conversation", "import", "analysis"].includes(run.kind),
    )
    .sort((a, b) => b.created_at.localeCompare(a.created_at))[0];

  useLayoutEffect(() => {
    const scroll = scrollRef.current;
    if (!scroll) return;
    const anchor = historyAnchor.current;
    if (anchor) {
      const element = Array.from(
        scroll.querySelectorAll<HTMLElement>("[data-message-id]"),
      ).find((node) => node.dataset.messageId === anchor.id);
      if (element)
        scroll.scrollTop += element.getBoundingClientRect().top - anchor.top;
      historyAnchor.current = null;
    } else if (atBottomRef.current) scroll.scrollTop = scroll.scrollHeight;
    // Run state now renders after the history, so a run that starts, stops or
    // fails changes the end of the conversation without adding a message.
  }, [history, activeRuns, latestRun?.id, latestRun?.status]);

  const pending =
    pendingQuery.data && !Array.isArray(pendingQuery.data)
      ? pendingQuery.data
      : null;
  const policyRecord =
    policy.data && !Array.isArray(policy.data) ? policy.data : undefined;
  const aiMissing = policyRecord ? policyRecord.manager === null : false;
  const openTarget = (target: string) => {
    if (onRepair) onRepair(target);
    else window.location.hash = target;
  };

  const loadEarlier = useCallback(async () => {
    if (!nextCursor || loadingEarlier) return;
    setLoadingEarlier(true);
    atBottomRef.current = false;
    setHistoryError(null);
    try {
      const page = await api.get<MessagePage>(
        `${tenderPath(tenderId)}/messages?limit=50&cursor=${encodeURIComponent(nextCursor)}`,
      );
      // Anchor the message currently being read. Total-height deltas also
      // include concurrent replies appended below and would shift the view.
      const scroll = scrollRef.current;
      const nodes = scroll
        ? Array.from(scroll.querySelectorAll<HTMLElement>("[data-message-id]"))
        : [];
      const anchor =
        nodes.find(
          (node) =>
            node.getBoundingClientRect().bottom >
            (scroll?.getBoundingClientRect().top ?? 0),
        ) ?? nodes[0];
      if (anchor?.dataset.messageId)
        historyAnchor.current = {
          id: anchor.dataset.messageId,
          top: anchor.getBoundingClientRect().top,
        };
      setHistory((current) => mergeMessages(page.items, current));
      setNextCursor(page.next_cursor ?? null);
    } catch (failure) {
      setHistoryError(failure);
    } finally {
      setLoadingEarlier(false);
    }
  }, [api, loadingEarlier, nextCursor, tenderId]);

  async function stopAllWork() {
    if (stopping) return;
    setStopping(true);
    setStopError(null);
    setStopNotice("");
    try {
      await api.post<Schema<"MutationReceipt">>(
        `${tenderPath(tenderId)}/work/stop`,
      );
      await refresh();
      setStopNotice(
        "Stop requested. Review each run below for its final status.",
      );
    } catch (failure) {
      setStopError(failure);
    } finally {
      setStopping(false);
    }
  }

  async function reviewDocuments() {
    if (reviewInFlight.current || pending) return;
    reviewInFlight.current = true;
    setReviewingDocuments(true);
    setReviewDocumentsError(null);
    try {
      if (onReviewDocuments) await onReviewDocuments();
      else
        await api.post<Schema<"MessageSubmission">>(
          `${tenderPath(tenderId)}/messages`,
          {
            content: "Review the tender documents and suggest the next steps.",
            action: "review_documents",
            idempotency_key:
              (reviewAttempt.current ??= `review-documents-${tenderId}-${Date.now()}`),
          } satisfies Schema<"MessageRequest">,
        );
      reviewAttempt.current = null;
      await refresh();
    } catch (failure) {
      setReviewDocumentsError(failure);
    } finally {
      reviewInFlight.current = false;
      setReviewingDocuments(false);
    }
  }

  function openPlanReview(planId: string) {
    if (onRecord) onRecord("plan-review", planId);
    else setReviewPlanId(planId);
  }

  if (reviewPlanId)
    return (
      <PlanReview
        tenderId={tenderId}
        planId={reviewPlanId}
        onBack={() => setReviewPlanId(null)}
        onSource={onSource}
      />
    );

  const conversationEmpty =
    !messages.isPending &&
    history.length === 0 &&
    overview.artifact_count > 0 &&
    activeRuns.length === 0 &&
    !pending;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1 flex-col">
        {activeRuns.length || onCustomizeManager ? (
          <header className="flex shrink-0 items-center justify-end gap-2 px-4 pt-3">
            {onCustomizeManager ? (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={onCustomizeManager}
              >
                <Sparkles data-icon="inline-start" />
                Customize Manager
              </Button>
            ) : null}
            {activeRuns.length ? (
              <Button
                type="button"
                variant="destructive"
                size="sm"
                disabled={stopping}
                onClick={() => void stopAllWork()}
              >
                <Square data-icon="inline-start" />
                {stopping ? "Stopping…" : "Stop all work"}
              </Button>
            ) : null}
          </header>
        ) : null}
        <div className="relative flex min-h-0 flex-1 flex-col">
          <ProgressiveBlur position="top" height="1.75rem" blurAmount="3px" />
          <div
            ref={scrollRef}
            className="manager-scroll min-h-0 flex-1 overflow-y-auto p-0"
            aria-label="Manager conversation"
            onScroll={() => {
              const element = scrollRef.current;
              if (element)
                atBottomRef.current =
                  element.scrollHeight -
                    element.scrollTop -
                    element.clientHeight <
                  70;
            }}
          >
            <div className="mx-auto flex w-full max-w-3xl flex-col gap-6 px-4 py-6">
              <ErrorNotice
                error={
                  messages.error ||
                  pendingQuery.error ||
                  runs.error ||
                  historyError ||
                  stopError ||
                  reviewDocumentsError
                }
              />
              {stopNotice ? (
                <p className="text-sm text-muted-foreground" role="status">
                  {stopNotice}
                </p>
              ) : null}
              {overview.artifact_count > 0 ? (
                <ImportSummary
                  artifactCount={overview.artifact_count}
                  onDocuments={onDocuments}
                  onReviewDocuments={() => void reviewDocuments()}
                  onChooseAI={() => setAiPickerOpen(true)}
                  ready={!!policyRecord?.manager}
                  disabled={reviewingDocuments || !!pending}
                  busy={activeRuns.length > 0}
                />
              ) : null}
              {overview.artifact_count === 0 && activeRuns.length === 0 ? (
                <Empty
                  title="Add the tender documents"
                  action={
                    <Button onClick={onImport}>
                      <FileText data-icon="inline-start" />
                      Import package
                    </Button>
                  }
                >
                  The manager will use the package to understand the project and
                  propose the work.
                </Empty>
              ) : null}
              {conversationEmpty ? (
                <div className="flex flex-col items-center gap-3 py-10 text-center">
                  <BrandMark size={64} />
                  <h2 className="text-xl font-semibold tracking-tight">
                    What should we work on?
                  </h2>
                  <p className="max-w-md text-sm text-muted-foreground">
                    Ask the Tender Manager to{" "}
                    <Typewriter
                      words={suggestions}
                      className="text-foreground"
                    />
                  </p>
                </div>
              ) : null}
              {nextCursor ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  className="self-center text-muted-foreground"
                  disabled={loadingEarlier}
                  onClick={() => void loadEarlier()}
                >
                  {loadingEarlier
                    ? "Loading earlier messages…"
                    : "Load earlier messages"}
                </Button>
              ) : null}
              {messages.isPending && !history.length ? (
                <Loading>Loading conversation…</Loading>
              ) : null}
              {history.map((message) => (
                <Fragment key={message.id}>
                  <ManagerMessage
                    message={message}
                    tenderId={tenderId}
                    onSource={onSource}
                    onArtifact={
                      onRecord
                        ? (target) => {
                            const record = parseRouteContext(target);
                            if (
                              record.kind === "tender" &&
                              record.tenderId === tenderId &&
                              record.recordId &&
                              record.view &&
                              [
                                "plan",
                                "plan-review",
                                "finding",
                                "decisions",
                                "output",
                                "office-result",
                                "staff-result",
                                "work-product",
                                "calculation",
                              ].includes(record.view)
                            )
                              onRecord(record.view, record.recordId);
                            else openTarget(target);
                          }
                        : undefined
                    }
                  />
                  {message.role === "engineer" &&
                  message.run_id &&
                  history.find(
                    (item) =>
                      item.role === "engineer" &&
                      item.run_id === message.run_id,
                  )?.id === message.id ? (
                    <ManagerPlanning
                      tenderId={tenderId}
                      runId={message.run_id}
                      startedAt={
                        runList.find((run) => run.id === message.run_id)
                          ?.created_at ?? message.created_at
                      }
                      status={
                        runList.find((run) => run.id === message.run_id)?.status
                      }
                      onSource={onSource}
                      onRunDetails={() =>
                        onRecord
                          ? onRecord("run", message.run_id!)
                          : openTarget(
                              `/tenders/${encodeURIComponent(tenderId)}/work?view=run&record=${encodeURIComponent(message.run_id!)}`,
                            )
                      }
                    />
                  ) : null}
                </Fragment>
              ))}
              {/* Work in progress is just the line below: the run card repeated
                  the same thing as a status panel. Stopped work still shows its
                  card, because that is the only place the reason appears, and it
                  belongs at the end of the conversation where the engineer is
                  reading rather than above the history where it scrolls away. */}
              {packageRun ? <AnalysisStages run={packageRun} /> : null}
              {activeRuns
                .filter(
                  (run) =>
                    ["manager", "conversation"].includes(run.kind) &&
                    !attachedRunIds.has(run.id),
                )
                .map((run) => (
                  <ManagerPlanning
                    key={run.id}
                    tenderId={tenderId}
                    runId={run.id}
                    status={run.status}
                    startedAt={run.created_at}
                    onSource={onSource}
                    onRunDetails={() =>
                      onRecord
                        ? onRecord("run", run.id)
                        : openTarget(
                            `/tenders/${encodeURIComponent(tenderId)}/work?view=run&record=${encodeURIComponent(run.id)}`,
                          )
                    }
                  />
                ))}
              {!activeRuns.length &&
              latestRun &&
              ["failed", "cancelled", "interrupted"].includes(
                latestRun.status,
              ) ? (
                <RunRow run={latestRun} compact />
              ) : null}
              {overview.plan &&
              !history.some((message) =>
                message.result_links?.some(
                  (link) =>
                    link.kind === "plan" && link.id === overview.plan?.id,
                ),
              ) ? (
                <PlanCard
                  plan={overview.plan}
                  onReview={() => openPlanReview(overview.plan!.id)}
                />
              ) : null}
              {pending ? (
                <PendingMessage
                  key={pending.id}
                  tenderId={tenderId}
                  pending={pending}
                  onRepair={onRepair}
                  onChanged={async () => {
                    await pendingQuery.refetch();
                    await refresh();
                  }}
                />
              ) : null}
            </div>
          </div>
          <ProgressiveBlur position="bottom" height="2rem" blurAmount="3px" />
        </div>
        <Composer
          tenderId={tenderId}
          onImport={onImport}
          blocked={aiMissing}
          onBlocked={() => setAiPickerOpen(true)}
          modelPicker={
            <>
              <ModelPicker
                tenderId={tenderId}
                policy={policyRecord}
                policyPending={policy.isPending}
                busy={activeRuns.length > 0}
                open={aiPickerOpen}
                onOpenChange={setAiPickerOpen}
                onManageAccounts={onSettings}
                onAdvanced={() =>
                  openTarget(
                    `/tenders/${encodeURIComponent(tenderId)}/work?view=ai`,
                  )
                }
              />
              <ThinkingPicker
                tenderId={tenderId}
                busy={activeRuns.length > 0}
              />
            </>
          }
          busy={activeRuns.length > 0}
          hasPending={!!pending}
          onSend={async (content, idempotencyKey) => {
            const result = await api.post<Schema<"MessageSubmission">>(
              `${tenderPath(tenderId)}/messages`,
              {
                content,
                idempotency_key: idempotencyKey,
              } satisfies Schema<"MessageRequest">,
            );
            await refresh();
            return result;
          }}
        />
      </div>
    </div>
  );
}

function ImportSummary({
  artifactCount,
  onDocuments,
  onReviewDocuments,
  onChooseAI,
  ready,
  disabled,
  busy,
}: {
  artifactCount: number;
  onDocuments?: () => void;
  onReviewDocuments: () => void;
  onChooseAI: () => void;
  ready: boolean;
  disabled: boolean;
  busy: boolean;
}) {
  return (
    <Item variant="outline" className="bg-card">
      <ItemMedia variant="icon" className="size-9 rounded-lg bg-muted">
        <FileText />
      </ItemMedia>
      <ItemContent>
        <ItemTitle>Package imported</ItemTitle>
        <ItemDescription>
          <AnimatedNumber value={artifactCount} /> document
          {artifactCount === 1 ? "" : "s"} registered.
        </ItemDescription>
        {onDocuments ? (
          <Button
            type="button"
            variant="link"
            size="sm"
            className={cn(
              "group/arrow h-auto w-fit justify-start gap-1 px-0 text-muted-foreground hover:text-foreground",
              linkUnderline,
            )}
            onClick={onDocuments}
          >
            Open document register
            <ArrowIcon className="size-3.5" />
          </Button>
        ) : null}
      </ItemContent>
      <ItemActions>
        {ready ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled}
            onClick={onReviewDocuments}
          >
            {busy ? "Review documents when ready" : "Review documents"}
          </Button>
        ) : (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="relative"
            onClick={onChooseAI}
          >
            <Sparkles data-icon="inline-start" />
            Choose AI
            <AnimatedBorder radius={8} />
          </Button>
        )}
      </ItemActions>
    </Item>
  );
}

function PlanCard({
  plan,
  onReview,
}: {
  plan: Schema<"WorkPlan">;
  onReview: () => void;
}) {
  return (
    <Item variant="outline" className="bg-card">
      <ItemMedia variant="icon" className="size-9 rounded-lg bg-muted">
        <ListChecks />
      </ItemMedia>
      <ItemContent>
        <ItemTitle>
          <h3 className="text-sm font-medium">{plan.title}</h3>
        </ItemTitle>
        <ItemDescription>
          {plan.tasks.length} tasks ·{" "}
          {plan.status === "proposed" ? "Ready for your review" : plan.status}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        <Button type="button" size="sm" onClick={onReview}>
          <Play data-icon="inline-start" />
          Review plan
        </Button>
      </ItemActions>
    </Item>
  );
}

function mergeMessages(...groups: Schema<"Message">[][]) {
  const byId = new Map<string, Schema<"Message">>();
  groups.flat().forEach((message) => byId.set(message.id, message));
  return [...byId.values()].sort(
    (a, b) =>
      a.created_at.localeCompare(b.created_at) || a.id.localeCompare(b.id),
  );
}
