import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Check, Mail, Plus, RefreshCw, Ruler, X } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { Empty, ErrorNotice, Loading, Modal } from "../components/common";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { type SourceSelection } from "./Sources";
import { EstimateEditor } from "./EstimateEditor";
import { createDraftScope, useFormDraft } from "./useFormDraft";
import { proposedRate, RateProposals } from "./RateProposals";
import { SourceBoqForm } from "./SourceBoqForm";
import { RetiredSourceRows, type RetiredSourceRow } from "./RetiredSourceRows";
import { Quotes } from "./Quotes";
import { TakeoffRow, useTakeoffRequest } from "./Takeoff";
import { AskManagerButton } from "./chat/AskManagerButton";
import {
  buildRows,
  COMPARISON,
  formatNumber,
  STATUS,
  takeoffNeedsDecision,
  type EstimateRow,
  type RowStatus,
} from "./estimate/model";
import { locatorLabel } from "@/lib/locator";

const PAGE_SIZE = 40;

type Line = Schema<"TakeoffLine">;
type Filter = "all" | RowStatus;

const FILTERS: { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "waiting", label: STATUS.waiting.label },
  { id: "check", label: STATUS.check.label },
  { id: "unpriced", label: STATUS.unpriced.label },
  { id: "priced", label: STATUS.priced.label },
];

export type EstimateView = "boq" | "takeoff" | "proposals" | "quotes";

/**
 * The estimate as one BOQ table. Each row carries its quantity (with the
 * drawing takeoff), its rate (with any proposed rate) and one plain status.
 * Work found on the drawings but missing from the BOQ is listed underneath,
 * and supplier quotes open in a side panel.
 */
export function Estimate({
  tenderId,
  defaultCurrency,
  onSource,
  view = "boq",
  selectedId,
  onSelect,
  onView,
  takeoffAvailable = true,
  quotesAvailable = true,
  onOpenManager,
}: {
  tenderId: string;
  defaultCurrency: string;
  onSource: (source: SourceSelection) => void;
  view?: EstimateView;
  /** A BOQ row, or a rate proposal or quote when `view` names one. */
  selectedId?: string | null;
  onSelect?: (id: string | null, view?: EstimateView) => void;
  onView?: (view: EstimateView) => void;
  takeoffAvailable?: boolean;
  quotesAvailable?: boolean;
  /** Takes the engineer to the chat after handing the Manager a job. */
  onOpenManager?: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh(),
    base = tenderPath(tenderId);
  const estimate = useResource<Schema<"EstimateView">>(`${base}/estimate`);
  const takeoffRequest = useTakeoffRequest(tenderId);
  const takeoff = useQuery({
    queryKey: [`${base}/takeoff`],
    queryFn: ({ signal }) => api.get<Line[]>(`${base}/takeoff`, signal),
    enabled: takeoffAvailable,
    // New lines appear while the team works on a requested takeoff.
    refetchInterval: takeoffRequest.asked ? 4000 : false,
    retry: 1,
  });
  const rateProposals = useResource<Schema<"RateProposalRecord">[]>(
    `${base}/estimate/rate-proposals`,
  );
  const [refreshing, setRefreshing] = useState(false);
  const [creating, setCreating] = useState(false);
  const [replacement, setReplacement] = useState<
    RetiredSourceRow | undefined
  >();
  const [createdItem, setCreatedItem] = useState<Schema<"EstimateItem"> | null>(
    null,
  );
  const [localSelected, setLocalSelected] = useState<string | null>(null);
  const [localQuotes, setLocalQuotes] = useState(false);
  const filters = useFormDraft(
    createDraftScope("estimate", tenderId, "table", 1),
    {
      query: "",
      filter: (view === "takeoff" || view === "proposals"
        ? "waiting"
        : "all") as Filter,
      page: 0,
    },
    ["query", "filter", "page"],
  );
  const { query, filter, page } = filters.value;
  const setPage = (value: number) => filters.setField("page", value);
  const [refreshError, setRefreshError] = useState<unknown>(null);

  if (estimate.isPending) return <Loading>Loading estimate…</Loading>;
  if (!estimate.data) return <ErrorNotice error={estimate.error} />;
  const data = estimate.data;
  const { rows, unmatched } = buildRows(
    data.items,
    takeoff.data,
    rateProposals.data,
  );

  // A link to a rate proposal opens the row it prices.
  const proposalItem =
    view === "proposals"
      ? (Array.isArray(rateProposals.data) ? rateProposals.data : []).find(
          (proposal) => proposal.id === selectedId,
        )?.item_id
      : undefined;
  const selected =
    selectedId === undefined
      ? localSelected
      : view === "quotes"
        ? null
        : (proposalItem ?? selectedId);
  const selectRow = (id: string | null) => {
    setLocalSelected(id);
    onSelect?.(id, "boq");
  };
  const quotesOpen = onView ? view === "quotes" : localQuotes;
  const openQuotes = (open: boolean) =>
    onView ? onView(open ? "quotes" : "boq") : setLocalQuotes(open);

  const selectedRow =
    rows.find((row) => row.item.id === selected) ??
    (createdItem?.id === selected && createdItem
      ? buildRows([createdItem], takeoff.data, rateProposals.data).rows[0]
      : undefined);
  const count = (id: Filter) =>
    id === "all" ? rows.length : rows.filter((row) => row.status === id).length;
  // A remembered filter whose rows are all gone has no chip left; show every row.
  const shown: Filter =
    filter !== "all" && count(filter) === 0 ? "all" : filter;
  const needle = query.trim().toLowerCase();
  const visible = rows.filter(
    (row) =>
      (shown === "all" || row.status === shown) &&
      (!needle ||
        row.item.description.toLowerCase().includes(needle) ||
        (row.item.row_reference ?? "").toLowerCase().includes(needle)),
  );
  const priced = data.items.length - data.unpriced_count;

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <h2 className="text-lg font-semibold tracking-tight">Estimate</h2>
          {data.items.length ? (
            <p
              className="text-sm text-muted-foreground"
              aria-label="Pricing state"
            >
              {[
                ...data.totals.map(
                  (total) =>
                    `${total.currency} ${formatNumber(total.total_ex_vat ?? total.priced_subtotal_ex_vat)} excluding VAT`,
                ),
                data.complete
                  ? "pricing complete"
                  : `${priced} of ${data.items.length} rows priced`,
              ].join(" · ")}
            </p>
          ) : null}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {quotesAvailable ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => openQuotes(true)}
            >
              <Mail data-icon="inline-start" />
              Supplier quotes
            </Button>
          ) : null}
          {takeoffAvailable ? (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={takeoffRequest.asking}
              onClick={() => void takeoffRequest.ask()}
            >
              <Ruler data-icon="inline-start" />
              Take off from drawings
            </Button>
          ) : null}
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={refreshing}
            onClick={async () => {
              setRefreshing(true);
              setRefreshError(null);
              try {
                await api.post<Schema<"EstimateView">>(
                  `${base}/estimate/refresh`,
                );
                await refresh();
              } catch (error) {
                setRefreshError(error);
              } finally {
                setRefreshing(false);
              }
            }}
          >
            <RefreshCw
              data-icon="inline-start"
              className={cn(refreshing && "animate-spin")}
            />
            {refreshing ? "Refreshing…" : "Refresh from documents"}
          </Button>
          <Button
            type="button"
            size="sm"
            onClick={() => {
              setReplacement(undefined);
              setCreating(true);
            }}
          >
            <Plus data-icon="inline-start" />
            Add BOQ row
          </Button>
        </div>
      </header>

      {takeoffRequest.asked ? (
        <p className="text-sm text-muted-foreground" role="status">
          Sent to the Tender Manager. Drawing quantities appear in the table as
          the team saves them.
        </p>
      ) : null}
      <ErrorNotice
        error={estimate.error || refreshError || takeoffRequest.error}
      />
      {data.refresh_required ? (
        <p className="text-sm text-(--warning-ink)">
          The documents changed. Refresh from documents before pricing.
        </p>
      ) : null}
      {!data.complete && data.blocking_reasons.length ? (
        <details className="text-sm text-muted-foreground">
          <summary className="w-fit cursor-pointer hover:text-foreground">
            What is still missing
          </summary>
          <ul className="ms-4 mt-1 flex list-disc flex-col gap-1">
            {data.blocking_reasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        </details>
      ) : null}

      {creating ? (
        <SourceBoqForm
          key={replacement?.id ?? "new"}
          replacement={replacement}
          tenderId={tenderId}
          onSource={onSource}
          onClose={() => setCreating(false)}
          onCreated={(item) => {
            setCreatedItem(item);
            setCreating(false);
            selectRow(item.id);
          }}
        />
      ) : null}
      <RetiredSourceRows
        tenderId={tenderId}
        rows={data.retired_source_rows ?? []}
        onSource={onSource}
        onReplace={(row) => {
          setReplacement(row);
          setCreating(true);
        }}
      />

      {rows.length ? (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <Input
              className="h-8 max-w-72 min-w-48 flex-1"
              aria-label="Search the BOQ"
              placeholder="Search items…"
              value={query}
              onChange={(event) => {
                filters.setField("query", event.target.value);
                setPage(0);
              }}
            />
            <div
              role="group"
              aria-label="Show rows"
              className="flex flex-wrap items-center gap-1"
            >
              {FILTERS.filter(
                (item) => item.id === "all" || count(item.id) > 0,
              ).map((item) => {
                const active = shown === item.id;
                return (
                  <button
                    key={item.id}
                    type="button"
                    aria-pressed={active}
                    onClick={() => {
                      filters.setField("filter", item.id);
                      setPage(0);
                    }}
                    className={cn(
                      "flex h-6.5 items-center gap-1.5 rounded-full px-2.5 text-xs font-medium transition-colors",
                      active
                        ? "bg-card text-foreground shadow-xs ring-1 ring-border"
                        : "text-muted-foreground hover:bg-muted",
                    )}
                  >
                    {item.label}
                    <span className="text-[10.5px] tabular-nums text-muted-foreground">
                      {count(item.id)}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="overflow-x-auto rounded-xl border bg-card">
            <Table aria-label="BOQ">
              <TableHeader className="bg-muted/40">
                <TableRow className="hover:bg-transparent">
                  <TableHead className="w-16 ps-4">Item</TableHead>
                  <TableHead>Description</TableHead>
                  <TableHead className="text-end">Quantity</TableHead>
                  <TableHead className="text-end">Rate</TableHead>
                  <TableHead className="text-end">Amount</TableHead>
                  <TableHead className="pe-4">Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {visible
                  .slice(page * PAGE_SIZE, page * PAGE_SIZE + PAGE_SIZE)
                  .map((row) => (
                    <BoqRow
                      key={row.item.id}
                      row={row}
                      tenderId={tenderId}
                      onOpen={() => selectRow(row.item.id)}
                      onSource={onSource}
                    />
                  ))}
                {visible.length === 0 ? (
                  <TableRow className="hover:bg-transparent">
                    <TableCell
                      colSpan={6}
                      className="h-20 text-center text-muted-foreground"
                    >
                      No rows match.
                    </TableCell>
                  </TableRow>
                ) : null}
              </TableBody>
            </Table>
          </div>

          {visible.length > PAGE_SIZE ? (
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-sm text-muted-foreground tabular-nums">
                {page * PAGE_SIZE + 1}–
                {Math.min(visible.length, page * PAGE_SIZE + PAGE_SIZE)} of{" "}
                {visible.length} rows
              </span>
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={page === 0}
                  onClick={() => setPage(page - 1)}
                >
                  Previous
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={(page + 1) * PAGE_SIZE >= visible.length}
                  onClick={() => setPage(page + 1)}
                >
                  Next
                </Button>
              </div>
            </div>
          ) : null}
        </>
      ) : (
        <Empty
          title="No BOQ rows yet"
          action={
            <AskManagerButton
              tenderId={tenderId}
              className="items-center"
              onSent={onOpenManager}
              request="Read the BOQ in the tender documents and add every row to the estimate for my review: item number, description, unit and quantity, with the page each row comes from."
            >
              Ask the Tender Manager to read the BOQ
            </AskManagerButton>
          }
        >
          The Tender Manager can read the BOQ from the documents, or you can add
          a row yourself.
        </Empty>
      )}

      {unmatched.length ? (
        <section
          aria-label="On the drawings, not in the BOQ"
          className="flex flex-col gap-2"
        >
          <h3 className="text-sm font-medium">
            On the drawings, not in the BOQ
            <span className="ms-1.5 text-muted-foreground tabular-nums">
              {unmatched.length}
            </span>
          </h3>
          <ul className="flex flex-col gap-2">
            {unmatched.map((line) => (
              <li key={line.id}>
                <TakeoffRow
                  tenderId={tenderId}
                  line={line}
                  onSource={onSource}
                />
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {selectedRow ? (
        <EstimateEditor
          key={selectedRow.item.id}
          item={selectedRow.item}
          tenderId={tenderId}
          defaultCurrency={defaultCurrency}
          onSource={onSource}
          onClose={() => selectRow(null)}
        >
          {selectedRow.takeoff.length ? (
            <section
              aria-label="From the drawings"
              className="flex flex-col gap-2"
            >
              <h4>From the drawings</h4>
              {selectedRow.takeoff.map((line) => (
                <TakeoffRow
                  key={line.id}
                  tenderId={tenderId}
                  line={line}
                  onSource={onSource}
                />
              ))}
            </section>
          ) : null}
          {selectedRow.rates.length ||
          (Array.isArray(rateProposals.data) &&
            rateProposals.data.some(
              (proposal) => proposal.item_id === selectedRow.item.id,
            )) ? (
            <RateProposals
              tenderId={tenderId}
              items={[selectedRow.item]}
              itemId={selectedRow.item.id}
              onSource={onSource}
              selectedId={view === "proposals" ? selectedId : undefined}
              onSelect={
                view === "proposals"
                  ? (id) => onSelect?.(id, "proposals")
                  : undefined
              }
            />
          ) : null}
        </EstimateEditor>
      ) : null}

      {quotesOpen ? (
        <Modal drawer title="Supplier quotes" onClose={() => openQuotes(false)}>
          <Quotes
            tenderId={tenderId}
            onSource={onSource}
            selectedId={view === "quotes" ? (selectedId ?? null) : undefined}
            onSelect={onSelect ? (id) => onSelect(id, "quotes") : undefined}
          />
        </Modal>
      ) : null}
    </div>
  );
}

function BoqRow({
  row,
  tenderId,
  onOpen,
  onSource,
}: {
  row: EstimateRow;
  tenderId: string;
  onOpen: () => void;
  onSource: (source: SourceSelection) => void;
}) {
  const { item } = row;
  const line = row.takeoff[0];
  const rate = row.rates[0];
  const decide =
    row.waiting === 1 && line && takeoffNeedsDecision(line) ? line : null;
  const quantity = formatNumber(item.effective_quantity);
  const unitRate = formatNumber(item.unit_rate);
  const amount = formatNumber(item.line_ex_vat);
  return (
    <TableRow className="align-top">
      <TableCell className="py-3 ps-4 text-muted-foreground tabular-nums">
        {item.row_reference || "—"}
      </TableCell>
      <TableCell className="min-w-64 py-3 whitespace-normal">
        <div className="flex flex-col items-start gap-1">
          <button
            type="button"
            className="text-start font-medium hover:underline"
            onClick={onOpen}
          >
            {item.description}
          </button>
          <button
            type="button"
            className="max-w-full truncate text-xs text-muted-foreground hover:text-foreground"
            onClick={() => onSource({ sourceId: item.source_id })}
          >
            {item.document} · {locatorLabel(item.locator)}
          </button>
        </div>
      </TableCell>
      <TableCell className="py-3 text-end">
        <div className="flex flex-col items-end gap-0.5">
          <span className="tabular-nums">
            {quantity ? `${quantity} ${item.unit}` : "Not found"}
          </span>
          {line ? (
            <span
              className={cn(
                "text-xs",
                takeoffNeedsDecision(line)
                  ? "text-(--warning-ink)"
                  : "text-muted-foreground",
              )}
            >
              Drawings{" "}
              {line.quantity
                ? `${formatNumber(line.quantity)} ${line.unit}`
                : "—"}
              {line.difference_percent
                ? ` (${line.difference_percent.startsWith("-") ? "" : "+"}${line.difference_percent}%)`
                : ` · ${COMPARISON[line.comparison]}`}
            </span>
          ) : item.quantity_basis === "approved_measurement" ? (
            <span className="text-xs text-muted-foreground">Measured</span>
          ) : null}
        </div>
      </TableCell>
      <TableCell className="py-3 text-end">
        <div className="flex flex-col items-end gap-0.5">
          <span className="tabular-nums">
            {unitRate ? `${unitRate} ${item.currency ?? ""}` : "—"}
          </span>
          {rate ? (
            <span className="text-xs text-(--warning-ink)">
              Proposed {formatNumber(proposedRate(rate))}{" "}
              {rate.payload.currency}
            </span>
          ) : null}
        </div>
      </TableCell>
      <TableCell className="py-3 text-end tabular-nums">
        {amount ?? "—"}
      </TableCell>
      <TableCell className="py-3 pe-4">
        <div className="flex flex-col items-start gap-1.5">
          <span className="inline-flex items-center gap-1.5 text-xs whitespace-nowrap">
            <span
              aria-hidden
              className={cn("size-1.5 rounded-full", STATUS[row.status].dot)}
            />
            {STATUS[row.status].label}
          </span>
          {decide ? (
            <TakeoffDecision tenderId={tenderId} line={decide} />
          ) : row.waiting ? (
            <Button
              type="button"
              variant="link"
              size="sm"
              className="h-auto p-0 text-xs"
              onClick={onOpen}
            >
              Review
            </Button>
          ) : null}
        </div>
      </TableCell>
    </TableRow>
  );
}

/** Accept or reject the drawing quantity for a row without opening it. */
function TakeoffDecision({ tenderId, line }: { tenderId: string; line: Line }) {
  const api = useApi();
  const refresh = useRefresh();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>(null);
  async function review(decision: "accepted" | "rejected") {
    setSaving(true);
    setError(null);
    try {
      await api.post<Line>(
        `${tenderPath(tenderId)}/takeoff/${encodeURIComponent(line.id)}/review`,
        { decision, note: "" } satisfies Schema<"TakeoffReview">,
      );
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setSaving(false);
    }
  }
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-1">
        <Button
          type="button"
          size="xs"
          variant="outline"
          disabled={saving}
          aria-label={`Accept the drawing quantity for ${line.description}`}
          onClick={() => void review("accepted")}
        >
          <Check data-icon="inline-start" />
          Accept
        </Button>
        <Button
          type="button"
          size="xs"
          variant="ghost"
          disabled={saving}
          aria-label={`Reject the drawing quantity for ${line.description}`}
          onClick={() => void review("rejected")}
        >
          <X data-icon="inline-start" />
          Reject
        </Button>
      </div>
      <ErrorNotice error={error} />
    </div>
  );
}
