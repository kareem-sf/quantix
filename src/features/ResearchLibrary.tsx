import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { ExternalLink, Globe2, RefreshCw } from "lucide-react";
import { tenderPath, useApi, type Schema } from "../api";
import { Empty, ErrorNotice, Loading } from "../components/common";
import type { SourceSelection } from "./Sources";
import { useOffsetList } from "./useOffsetList";
import { WorkingMemoryCard } from "./WorkingMemoryCard";
import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert";
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

type Passage = Schema<"ResearchPassage">;
type ResearchReceipt = Schema<"PublicResearchReceipt">;
type MemoryOverview = Schema<"MemoryOverview">;
type MarketObservation = Schema<"MarketObservation">;
type BrowserStatus = Schema<"BrowserResearchStatusModel">;

export function ResearchLibrary({
  tenderId,
  focusCitationId,
  tenderRevision,
  workRevision,
  onSource,
}: {
  tenderId: string;
  focusCitationId?: string;
  tenderRevision?: number;
  workRevision?: string;
  onSource?: (source: SourceSelection) => void;
}) {
  const api = useApi();
  const base = tenderPath(tenderId);
  const refreshKey = `${tenderRevision ?? 0}:${workRevision ?? ""}`;
  const receiptPages = useOffsetList<ResearchReceipt>({
    path: `${base}/research`,
    refreshKey,
  });
  const notePages = useOffsetList<Schema<"WorkingMemoryRecord">>({
    path: `${base}/memory/working`,
    refreshKey,
  });
  const receipts = receiptPages.items;
  const [memory, setMemory] = useState<MemoryOverview | null>(null);
  const [market, setMarket] = useState<MarketObservation[] | null>(null);
  const [browser, setBrowser] = useState<BrowserStatus | null>(null);
  const [focusedCitation, setFocusedCitation] =
    useState<Schema<"ResearchCitation"> | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [url, setUrl] = useState("");
  const [fetchKey, setFetchKey] = useState(() => key("public-fetch"));
  const [pending, setPending] = useState(false);
  const [purpose, setPurpose] = useState<Record<string, string>>({});
  const [citedPassages, setCitedPassages] = useState<Set<string>>(new Set());
  const [notice, setNotice] = useState<string | null>(null);
  const loadSequence = useRef(0);
  const readError = error || receiptPages.error || notePages.error;

  const load = useCallback(
    async (signal?: AbortSignal) => {
      const sequence = ++loadSequence.current;
      try {
        const [nextMarket, nextMemory, nextBrowser, nextCitation] =
          await Promise.all([
            api.get<MarketObservation[]>(`${base}/research/market`, signal),
            api.get<MemoryOverview>(`${base}/memory`, signal),
            api.get<BrowserStatus>(`${base}/research/browser/status`, signal),
            focusCitationId
              ? api.get<Schema<"ResearchCitation">>(
                  `${base}/research/citations/${encodeURIComponent(focusCitationId)}`,
                  signal,
                )
              : Promise.resolve(null),
          ]);
        if (sequence !== loadSequence.current) return;
        setMarket(nextMarket);
        setMemory(nextMemory);
        setBrowser(nextBrowser);
        setFocusedCitation(nextCitation);
        setError(null);
      } catch (failure) {
        if (sequence !== loadSequence.current) return;
        if (!(
          failure instanceof DOMException && failure.name === "AbortError"
        )) {
          setError(failure);
          setMarket((current) => current ?? []);
          setMemory(
            (current) =>
              current ?? {
                working_memory: [],
                approved_decisions: [],
                company_knowledge: [],
              },
          );
          setBrowser(
            (current) =>
              current ?? {
                ready: false,
                podman_available: false,
                state: "needs_repair",
                image: "localhost/quantix-research-browser:2",
                detail:
                  "The isolated public-page reader status is unavailable.",
              },
          );
        }
      }
    },
    [api, base, focusCitationId, tenderRevision, workRevision],
  );

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  useEffect(() => {
    if (!focusedCitation) return;
    document
      .getElementById(`research-receipt-${focusedCitation.receipt_id}`)
      ?.scrollIntoView?.({ block: "center" });
  }, [focusedCitation, receiptPages.items]);

  useEffect(() => {
    if (
      !focusedCitation ||
      receiptPages.items.some(
        (receipt) => receipt.id === focusedCitation.receipt_id,
      )
    )
      return;
    const controller = new AbortController();
    void api
      .get<ResearchReceipt>(
        `${base}/research/receipts/${encodeURIComponent(focusedCitation.receipt_id)}`,
        controller.signal,
      )
      .then((receipt) => receiptPages.prepend(receipt))
      .catch((failure) => {
        if (!(failure instanceof DOMException && failure.name === "AbortError"))
          setError(failure);
      });
    return () => controller.abort();
  }, [api, base, focusedCitation, receiptPages.items, receiptPages.prepend]);

  async function fetchPublic(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      const receipt = await api.post<ResearchReceipt>(
        `${base}/research/fetch`,
        {
          url,
          max_bytes: 1_000_000,
          idempotency_key: fetchKey,
        },
      );
      receiptPages.prepend(receipt, true);
      acceptReceipt();
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  async function fetchRendered() {
    if (pending || !url) return;
    setPending(true);
    setError(null);
    try {
      const receipt = await api.post<ResearchReceipt>(
        `${base}/research/browser/fetch`,
        { url, max_bytes: 1_000_000, idempotency_key: fetchKey },
      );
      receiptPages.prepend(receipt, true);
      acceptReceipt();
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  function acceptReceipt() {
    setUrl("");
    setFetchKey(key("public-fetch"));
    setNotice(
      "The public source is saved. Select an exact passage before citing it.",
    );
  }

  async function cite(receipt: ResearchReceipt, passage: Passage) {
    setError(null);
    try {
      await api.post(`${base}/research/citations`, {
        receipt_id: receipt.id,
        passage_ids: [passage.id],
        purpose: purpose[passage.id] ?? "",
        idempotency_key: key("public-citation"),
      });
      setCitedPassages((current) => new Set(current).add(passage.id));
      receiptPages.prepend({ ...receipt, cited: true });
      setNotice("Exact passage cited");
    } catch (failure) {
      setError(failure);
    }
  }

  async function setupBrowser() {
    setPending(true);
    setError(null);
    try {
      const status = await api.post<BrowserStatus>(
        `${base}/research/browser/setup`,
      );
      setBrowser(status);
      setNotice("The isolated rendered-page reader is ready.");
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  function retryReads() {
    return Promise.all([load(), receiptPages.reload(), notePages.reload()]);
  }

  if (
    memory === null ||
    market === null ||
    browser === null ||
    (receiptPages.loading && !receiptPages.items.length) ||
    (notePages.loading && !notePages.items.length)
  ) {
    return <Loading>Loading research and memory…</Loading>;
  }

  return (
    <section className="flex flex-col gap-6" aria-labelledby="research-title">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 id="research-title" className="text-lg font-semibold tracking-tight">
            Research and memory
          </h2>
          <p className="text-sm text-muted-foreground">
            Keep public passages, market observations and Tender working notes
            traceable to their exact basis.
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            Rendered reader: {browserStateLabel(browser.state)}
          </p>
        </div>
        <Button type="button" variant="ghost" onClick={() => void retryReads()}>
          <RefreshCw data-icon="inline-start" />
          Refresh research
        </Button>
      </header>
      {notice ? (
        <Alert>
          <AlertTitle>Research updated</AlertTitle>
          <AlertDescription>{notice}</AlertDescription>
        </Alert>
      ) : null}
      {readError ? (
        <div className="flex flex-col items-start gap-2">
          <ErrorNotice error={readError} />
          <Button
            type="button"
            variant="outline"
            onClick={() => void retryReads()}
          >
            Retry research reads
          </Button>
        </div>
      ) : null}
      {focusedCitation ? (
        <Alert>
          <AlertTitle>Saved citation context</AlertTitle>
          <AlertDescription>
            {focusedCitation.purpose} · retrieved{" "}
            {formatDate(focusedCitation.retrieved_at)}
          </AlertDescription>
        </Alert>
      ) : null}
      {!browser.ready ? (
        <Alert>
          <AlertTitle>Rendered page reader not ready</AlertTitle>
          <AlertDescription>{browser.detail}</AlertDescription>
          {browser.podman_available &&
          ["setup_required", "stopped", "needs_repair"].includes(
            browser.state,
          ) ? (
            <AlertAction>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={pending}
                onClick={() => void setupBrowser()}
              >
                {pending ? "Setting up…" : "Set up reader"}
              </Button>
            </AlertAction>
          ) : null}
          {browser.image_id ? (
            <details className="mt-2 text-xs">
              <summary className="cursor-pointer">Exact reader image</summary>
              <code className="wrap-anywhere">{browser.image_id}</code>
            </details>
          ) : null}
        </Alert>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle>Open a public source</CardTitle>
          <CardDescription>
            Quantix blocks private addresses and saves the exact retrieved
            passages. Opening a URL does not cite it.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form
            className="flex flex-col gap-3 sm:flex-row sm:items-end"
            onSubmit={fetchPublic}
          >
            <Field className="flex-1">
              <FieldLabel htmlFor="research-public-url">Public URL</FieldLabel>
              <Input
                id="research-public-url"
                type="url"
                required
                maxLength={3000}
                value={url}
                onChange={(event) => {
                  setUrl(event.target.value);
                  setFetchKey(key("public-fetch"));
                }}
              />
            </Field>
            <Button type="submit" disabled={pending}>
              <Globe2 data-icon="inline-start" />
              {pending ? "Opening…" : "Open public source"}
            </Button>
            {browser.ready ? (
              <Button
                type="button"
                variant="outline"
                disabled={pending || !url}
                onClick={() => void fetchRendered()}
              >
                Open rendered page
              </Button>
            ) : null}
          </form>
        </CardContent>
      </Card>

      {receipts.length === 0 && !error && !receiptPages.error ? (
        <Empty title="No public sources saved yet">
          Open a relevant supplier, manufacturer or public reference page when
          the Tender needs outside evidence.
        </Empty>
      ) : (
        <div className="flex flex-col gap-4">
          {receipts.map((receipt) => (
            <Card
              key={receipt.id}
              id={`research-receipt-${receipt.id}`}
              className={
                focusedCitation?.receipt_id === receipt.id
                  ? "ring-2 ring-primary/40"
                  : undefined
              }
            >
              <CardHeader>
                <CardTitle>{receipt.title || receipt.final_url}</CardTitle>
                <CardDescription>
                  Retrieved {formatDate(receipt.retrieved_at)} ·{" "}
                  {receipt.cited ? "Cited" : "Not cited"}
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-col gap-4">
                <a
                  href={receipt.final_url}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 text-sm underline underline-offset-4"
                >
                  Open saved source URL
                  <ExternalLink data-icon="inline-end" />
                </a>
                {receipt.renderer_image_id || receipt.renderer_fingerprint ? (
                  <details className="text-xs">
                    <summary className="cursor-pointer">
                      Exact rendered-reader proof
                    </summary>
                    {receipt.renderer_image_id ? (
                      <p className="mt-1">
                        Image:{" "}
                        <code className="wrap-anywhere">
                          {receipt.renderer_image_id}
                        </code>
                      </p>
                    ) : null}
                    {receipt.renderer_fingerprint ? (
                      <p className="mt-1">
                        Runtime:{" "}
                        <code className="wrap-anywhere">
                          {receipt.renderer_fingerprint}
                        </code>
                      </p>
                    ) : null}
                  </details>
                ) : null}
                {receipt.passages.map((passage) => (
                  <article
                    key={passage.id}
                    className={`rounded-lg border p-3 ${
                      focusedCitation?.passage_ids.includes(passage.id)
                        ? "bg-primary/5 ring-1 ring-primary/30"
                        : ""
                    }`}
                  >
                    <p className="text-sm" dir="auto">
                      {passage.text}
                    </p>
                    <FieldGroup className="mt-3">
                      <Field>
                        <FieldLabel htmlFor={`citation-purpose-${passage.id}`}>
                          Citation purpose
                        </FieldLabel>
                        <Textarea
                          id={`citation-purpose-${passage.id}`}
                          required
                          maxLength={1000}
                          value={purpose[passage.id] ?? ""}
                          onChange={(event) =>
                            setPurpose((current) => ({
                              ...current,
                              [passage.id]: event.target.value,
                            }))
                          }
                        />
                        <FieldDescription>
                          State which finding or market observation this passage
                          supports.
                        </FieldDescription>
                      </Field>
                    </FieldGroup>
                    <Button
                      type="button"
                      size="sm"
                      className="mt-3"
                      disabled={
                        !purpose[passage.id]?.trim() ||
                        citedPassages.has(passage.id)
                      }
                      onClick={() => void cite(receipt, passage)}
                    >
                      {citedPassages.has(passage.id)
                        ? "Exact passage cited"
                        : "Cite this passage"}
                    </Button>
                  </article>
                ))}
              </CardContent>
            </Card>
          ))}
        </div>
      )}
      {receiptPages.nextOffset != null ? (
        <Button
          type="button"
          variant="ghost"
          disabled={receiptPages.loading}
          onClick={() => void receiptPages.loadMore()}
        >
          {receiptPages.loading
            ? "Loading older public sources…"
            : "Load older public sources"}
        </Button>
      ) : null}

      <MemoryInspector
        overview={memory}
        notes={notePages.items}
        notesLoading={notePages.loading}
        notesHaveMore={notePages.nextOffset != null}
        onLoadMoreNotes={() => void notePages.loadMore()}
        market={market}
        onSource={(source) => onSource?.(source)}
        notesUnavailable={Boolean(error || notePages.error)}
        onPromoted={(record) => {
          setMemory((current) =>
            current
              ? {
                  ...current,
                  company_knowledge: [
                    record,
                    ...current.company_knowledge.filter(
                      (item) => item.id !== record.id,
                    ),
                  ],
                }
              : current,
          );
          setNotice(
            "Working note promoted. Inspect it in Settings → Knowledge.",
          );
        }}
      />
    </section>
  );
}

export function MemoryInspector({
  overview,
  notes,
  notesLoading,
  notesHaveMore,
  onLoadMoreNotes,
  notesUnavailable,
  market,
  onSource,
  onPromoted,
}: {
  overview: MemoryOverview;
  notes: Schema<"WorkingMemoryRecord">[];
  notesLoading: boolean;
  notesHaveMore: boolean;
  onLoadMoreNotes: () => void;
  notesUnavailable: boolean;
  market: MarketObservation[];
  onSource: (source: SourceSelection) => void;
  onPromoted: (record: Schema<"KnowledgeRecord">) => void;
}) {
  return (
    <section
      className="grid gap-4 lg:grid-cols-2"
      aria-label="Memory inspector"
    >
      <MemoryCard title="Working notes">
        {notes.map((item) => (
          <WorkingMemoryCard
            key={item.id}
            tenderId={item.tender_id}
            note={item}
            onSource={onSource}
            onPromoted={onPromoted}
          />
        ))}
        {!notes.length && !notesLoading && !notesUnavailable ? (
          <p className="text-sm text-muted-foreground">
            No working notes have been saved yet.
          </p>
        ) : null}
        {notesHaveMore ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={notesLoading}
            onClick={onLoadMoreNotes}
          >
            {notesLoading ? "Loading older notes…" : "Load older working notes"}
          </Button>
        ) : null}
      </MemoryCard>
      <MemoryCard title="Approved decisions">
        {overview.approved_decisions.map((item, index) => {
          const target = recordText(item, "target_id", "Tender record");
          return (
            <p
              key={`${target}-${index}`}
              className="rounded-lg border p-3 text-sm"
            >
              {decisionLabel(recordText(item, "decision", "approved"))} decision
              · {target}
            </p>
          );
        })}
      </MemoryCard>
      <MemoryCard title="Company knowledge">
        {overview.company_knowledge.map((item, index) => {
          const identifier = recordText(item, "id", `knowledge-${index}`);
          return (
            <p key={identifier} className="rounded-lg border p-3 text-sm">
              {recordText(item, "title", identifier)}
              {recordBool(item, "needs_recheck") ? " · Needs recheck" : ""}
            </p>
          );
        })}
      </MemoryCard>
      <MemoryCard title="Market observations">
        {market.map((item) => (
          <p key={item.id} className="rounded-lg border p-3 text-sm">
            {item.product} · {item.value} {item.currency}/{item.unit}
            {item.needs_recheck ? " · Needs recheck" : ""}
          </p>
        ))}
      </MemoryCard>
    </section>
  );
}

function MemoryCard({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-2">{children}</CardContent>
    </Card>
  );
}

function key(prefix: string) {
  return `${prefix}-${typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : Math.random().toString(36).slice(2)}`;
}

function formatDate(value: string) {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

function decisionLabel(value?: string) {
  return value === "accept"
    ? "Accepted"
    : value === "reject"
      ? "Rejected"
      : value === "resolve"
        ? "Resolved"
        : "Approved";
}

function recordText(
  value: Record<string, unknown>,
  key: string,
  fallback: string,
) {
  return typeof value[key] === "string" ? value[key] : fallback;
}

function recordBool(value: Record<string, unknown>, key: string) {
  return value[key] === true;
}

function browserStateLabel(value: BrowserStatus["state"]) {
  if (value === "ready") return "Ready";
  if (value === "busy") return "Busy";
  if (value === "stopped") return "Private runtime stopped";
  if (value === "setup_required") return "Setup required";
  if (value === "needs_repair") return "Needs repair";
  if (value === "prerequisite_required") return "System prerequisite required";
  return "Private runtime not installed";
}
