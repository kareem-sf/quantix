import { useState } from "react";
import { RefreshCw } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { Empty, ErrorNotice, Loading, Status } from "../components/common";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";
import { Citations, type SourceSelection } from "./Sources";
import { EstimateEditor } from "./EstimateEditor";
import { createDraftScope, useFormDraft } from "./useFormDraft";
import { RateProposals } from "./RateProposals";
import { SourceBoqForm } from "./SourceBoqForm";
import { RetiredSourceRows, type RetiredSourceRow } from "./RetiredSourceRows";

const PAGE_SIZE = 40;

export function Estimate({
  tenderId,
  defaultCurrency,
  onSource,
  view: focusedView = "boq",
  selectedId,
  onSelect,
}: {
  tenderId: string;
  defaultCurrency: string;
  onSource: (source: SourceSelection) => void;
  view?: "boq" | "proposals";
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
}) {
  const api = useApi(),
    refresh = useRefresh(),
    base = tenderPath(tenderId);
  const estimate = useResource<Schema<"EstimateView">>(`${base}/estimate`);
  const [refreshing, setRefreshing] = useState(false);
  const [creating, setCreating] = useState(false);
  const [replacement, setReplacement] = useState<
    RetiredSourceRow | undefined
  >();
  const [createdItem, setCreatedItem] = useState<Schema<"EstimateItem"> | null>(
    null,
  );
  const [localSelected, setLocalSelected] = useState<string | null>(null);
  const selected = selectedId === undefined ? localSelected : selectedId;
  const setSelected = (id: string | null) => {
    setLocalSelected(id);
    onSelect?.(id);
  };
  const filters = useFormDraft(
    createDraftScope("estimate", tenderId, "view", 1),
    { query: "", filter: "all", page: 0 },
    ["query", "filter", "page"],
  );
  const { query, filter, page } = filters.value;
  const setQuery = (value: string) => filters.setField("query", value);
  const setFilter = (value: string) => filters.setField("filter", value);
  const setPage = (value: number | ((current: number) => number)) =>
    filters.setField(
      "page",
      typeof value === "function" ? value(filters.value.page) : value,
    );
  const [refreshError, setRefreshError] = useState<unknown>(null);
  const closeEditor = () => setSelected(null);
  if (estimate.isPending) return <Loading>Loading estimate…</Loading>;
  if (!estimate.data) return <ErrorNotice error={estimate.error} />;
  const view = estimate.data;
  const visible = view.items.filter(
    (item) =>
      item.description.toLowerCase().includes(query.toLowerCase()) &&
      (filter === "all" ||
        (filter === "unconfirmed" && !item.confirmed) ||
        (filter === "unpriced" && item.unit_rate === null) ||
        (filter === "measured" &&
          item.quantity_basis === "approved_measurement")),
  );
  const selectedItem =
    view.items.find((item) => item.id === selected) ??
    (createdItem?.id === selected ? createdItem : undefined);
  return (
    <div className="flex flex-col gap-5">
      {focusedView === "boq" ? (
        <>
          <header className="flex flex-wrap items-start justify-between gap-3">
            <div className="flex max-w-2xl flex-col gap-1">
              <h2 className="text-lg font-semibold tracking-tight">Estimate</h2>
              <p className="text-sm text-muted-foreground">
                Supplied quantities, checked rates and recorded assumptions.
              </p>
            </div>
            <Button
              type="button"
              variant="outline"
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
              {refreshing ? "Refreshing…" : "Refresh source rows"}
            </Button>
          </header>
          <ErrorNotice error={estimate.error || refreshError} />
          <Button
            className="self-start"
            onClick={() => {
              setReplacement(undefined);
              setCreating(true);
            }}
          >
            Add BOQ row from source
          </Button>
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
                setSelected(item.id);
              }}
            />
          ) : null}

          <section
            className={cn(
              "flex flex-col gap-2 rounded-xl border p-4",
              view.complete ? "bg-emerald-500/5" : "bg-amber-500/5",
            )}
            aria-label="Pricing state"
          >
            <div className="flex flex-wrap items-center gap-2">
              <span
                aria-hidden="true"
                className={cn(
                  "size-2 rounded-full",
                  view.complete ? "bg-emerald-500" : "bg-amber-500",
                )}
              />
              <strong className="text-sm font-medium">
                {view.complete ? "Pricing complete" : "Estimate needs review"}
              </strong>
            </div>
            <p className="text-sm text-muted-foreground">
              {view.coverage_note}
            </p>
            {view.refresh_required ? (
              <p className="text-sm text-muted-foreground">
                Source documents have changed. Refresh the BOQ rows before
                continuing.
              </p>
            ) : null}
            {view.blocking_reasons.length ? (
              <ul className="ms-4 flex list-disc flex-col gap-1 text-sm text-muted-foreground">
                {view.blocking_reasons.map((reason) => (
                  <li key={reason}>{reason}</li>
                ))}
              </ul>
            ) : null}
          </section>

          <RetiredSourceRows
            tenderId={tenderId}
            rows={view.retired_source_rows ?? []}
            onSource={onSource}
            onReplace={(row) => {
              setReplacement(row);
              setCreating(true);
            }}
          />
          {view.totals.length ? (
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {view.totals.map((total) => (
                <section
                  key={total.currency}
                  className="flex flex-col gap-2 rounded-xl border bg-card p-4"
                >
                  <h3 className="text-sm font-semibold tracking-tight">
                    {total.currency}
                  </h3>
                  <dl className="flex flex-col gap-1.5 text-sm">
                    <div className="flex items-baseline justify-between gap-2">
                      <dt className="text-muted-foreground">
                        Priced subtotal excluding VAT
                      </dt>
                      <dd className="tabular-nums">
                        {total.priced_subtotal_ex_vat}
                      </dd>
                    </div>
                    <div className="flex items-baseline justify-between gap-2">
                      <dt className="text-muted-foreground">
                        Complete total excluding VAT
                      </dt>
                      <dd className="tabular-nums">
                        {total.total_ex_vat ?? "Not established"}
                      </dd>
                    </div>
                    <div className="flex items-baseline justify-between gap-2">
                      <dt className="text-muted-foreground">
                        Complete total including VAT
                      </dt>
                      <dd className="tabular-nums">
                        {total.total_inc_vat ?? "Not established"}
                      </dd>
                    </div>
                  </dl>
                </section>
              ))}
            </div>
          ) : null}

          {view.items.length ? (
            <>
              <div className="flex flex-wrap items-center gap-2">
                <Input
                  className="min-w-60 flex-1"
                  aria-label="Search estimate descriptions"
                  placeholder="Search descriptions…"
                  dir="auto"
                  value={query}
                  onChange={(event) => {
                    setQuery(event.target.value);
                    setPage(0);
                  }}
                />
                <NativeSelect
                  aria-label="Filter estimate rows"
                  value={filter}
                  onChange={(event) => {
                    setFilter(event.target.value);
                    setPage(0);
                  }}
                >
                  <NativeSelectOption value="all">All rows</NativeSelectOption>
                  <NativeSelectOption value="unconfirmed">
                    Source not confirmed
                  </NativeSelectOption>
                  <NativeSelectOption value="unpriced">
                    Without a rate
                  </NativeSelectOption>
                  <NativeSelectOption value="measured">
                    Measured quantity
                  </NativeSelectOption>
                </NativeSelect>
              </div>

              <div className="overflow-x-auto rounded-xl border bg-card">
                <Table aria-label="Estimate rows">
                  <TableHeader className="bg-muted/40">
                    <TableRow className="hover:bg-transparent">
                      <TableHead className="ps-4">
                        Description and source
                      </TableHead>
                      <TableHead>Unit</TableHead>
                      <TableHead className="text-end">
                        Supplied quantity
                      </TableHead>
                      <TableHead className="text-end">Quantity used</TableHead>
                      <TableHead className="text-end">Unit rate</TableHead>
                      <TableHead className="text-end">
                        Amount excluding VAT
                      </TableHead>
                      <TableHead className="pe-4">Review</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {visible
                      .slice(page * PAGE_SIZE, page * PAGE_SIZE + PAGE_SIZE)
                      .map((item) => (
                        <TableRow key={item.id}>
                          <TableCell className="min-w-72 py-3 ps-4 whitespace-normal">
                            <div className="flex flex-col items-start gap-1">
                              <Button
                                type="button"
                                variant="link"
                                className="h-auto justify-start p-0 text-start font-medium whitespace-normal text-foreground"
                                dir="auto"
                                title={item.description}
                                onClick={() => setSelected(item.id)}
                              >
                                {item.description}
                              </Button>
                              <button
                                type="button"
                                className="inline-flex max-w-full items-center gap-1.5 rounded-full border bg-background px-2.5 py-1 text-xs transition-colors hover:bg-accent"
                                onClick={() =>
                                  onSource({ sourceId: item.source_id })
                                }
                              >
                                <span className="truncate" dir="auto">
                                  {item.document} · {item.locator}
                                </span>
                              </button>
                            </div>
                          </TableCell>
                          <TableCell className="text-muted-foreground">
                            {item.unit || "Not identified"}
                          </TableCell>
                          <TableCell className="text-end tabular-nums">
                            {item.supplied_quantity ?? "Unresolved"}
                          </TableCell>
                          <TableCell className="text-end">
                            <div className="flex flex-col items-end">
                              <span className="tabular-nums">
                                {item.effective_quantity ?? "Unresolved"}
                              </span>
                              <span className="text-xs text-muted-foreground">
                                {item.quantity_basis === "approved_measurement"
                                  ? "Approved measurement"
                                  : "Supplied BOQ"}
                              </span>
                            </div>
                          </TableCell>
                          <TableCell className="text-end">
                            <div className="flex flex-col items-end">
                              <span className="tabular-nums">
                                {item.unit_rate ?? "Not priced"}
                              </span>
                              <span className="text-xs text-muted-foreground">
                                {item.currency ?? ""}
                              </span>
                            </div>
                          </TableCell>
                          <TableCell className="text-end tabular-nums">
                            {item.line_ex_vat ?? "Not established"}
                          </TableCell>
                          <TableCell className="pe-4">
                            <div className="flex flex-col items-start gap-1">
                              <Status
                                value={
                                  item.confirmed ? "confirmed" : "needs_review"
                                }
                              />
                              <Button
                                type="button"
                                variant="link"
                                size="sm"
                                className="h-auto p-0"
                                onClick={() => setSelected(item.id)}
                              >
                                Review row
                              </Button>
                            </div>
                          </TableCell>
                        </TableRow>
                      ))}
                    {visible.length === 0 ? (
                      <TableRow className="hover:bg-transparent">
                        <TableCell
                          colSpan={7}
                          className="h-24 text-center text-muted-foreground"
                        >
                          No estimate rows match these filters.
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
                      onClick={() => setPage((value) => value - 1)}
                    >
                      Previous
                    </Button>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={(page + 1) * PAGE_SIZE >= visible.length}
                      onClick={() => setPage((value) => value + 1)}
                    >
                      Next
                    </Button>
                  </div>
                </div>
              ) : null}
            </>
          ) : (
            <Empty title="No current BOQ rows">
              Add a BOQ row from a PDF or Word source using the action above, or
              import an Excel BOQ and choose Refresh source rows. Each proposed
              row needs engineer confirmation against its source.
            </Empty>
          )}

          <details className="rounded-xl border bg-card p-4">
            <summary className="w-fit cursor-pointer text-sm font-medium">
              More options
            </summary>
            <p className="mt-2 text-sm text-muted-foreground">
              Supplied bill quantities stay in use until you approve a
              measurement. Scenario totals do not change accepted rates.
            </p>
          </details>
        </>
      ) : (
        <div className="legacy-screen flex flex-col gap-5">
          <RateProposals
            tenderId={tenderId}
            items={view.items}
            onSource={onSource}
            selectedId={selected}
            onSelect={setSelected}
          />
          <section className="quantity-proposals">
            <h2>Quantity proposals</h2>
            <p className="muted">
              Inspect the original BOQ row and calculation before approving a
              different quantity.
            </p>
            {view.items
              .filter((item) => item.quantity_proposals.length)
              .map((item) => (
                <article className="document-row" key={item.id}>
                  <div>
                    <strong>{item.description}</strong>
                    <p className="field-help">
                      {item.quantity_proposals.length} proposal(s) · Supplied{" "}
                      {item.supplied_quantity ?? "unresolved"} {item.unit}
                    </p>
                  </div>
                  <button
                    type="button"
                    className="button"
                    onClick={() => setSelected(item.id)}
                  >
                    Review quantity proposals
                  </button>
                </article>
              ))}
            {!view.items.some((item) => item.quantity_proposals.length) ? (
              <p className="field-help">
                No quantity proposals have been recorded. Open a BOQ row to
                propose a different quantity.
              </p>
            ) : null}
          </section>
        </div>
      )}
      {selectedItem ? (
        <div className="legacy-screen">
          <EstimateEditor
            key={selectedItem.id}
            item={selectedItem}
            tenderId={tenderId}
            defaultCurrency={defaultCurrency}
            onSource={onSource}
            onClose={closeEditor}
          />
        </div>
      ) : null}
    </div>
  );
}
