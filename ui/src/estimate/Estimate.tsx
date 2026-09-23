import { useState } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { Face } from "../office/Face";
import { firstName, useOffice, type Staff } from "../office/queries";
import {
  money,
  quantity,
  statusDot,
  statusLabel,
  useApproveAll,
  useApproveAllRates,
  useBoq,
  useDecide,
  useDecideMarkups,
  useDecideRate,
  useEstimate,
  type BoqItem,
  type Fact,
  type Priced,
  type Rate,
  type Summary,
  type Markups,
} from "./queries";

const COLUMNS = "grid grid-cols-[56px_minmax(0,1fr)_84px_40px_84px_104px_132px] gap-3";

/** One status for the row: the BOQ line first, then its rate. */
function rowStatus(item: BoqItem, rate: Rate | null | undefined): [label: string, dot: string] {
  if (item.status === "proposed") return ["Needs you", "bg-attention"];
  if (!rate) return ["Not priced", "bg-ink-4"];
  if (rate.status === "proposed") return ["Rate needs you", "bg-attention"];
  return [rate.status === "office_approved" ? "Priced by the office" : "Priced", "bg-approved"];
}

export function Estimate() {
  const { tenderId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const boq = useBoq(tenderId);
  const estimate = useEstimate(tenderId);
  const office = useOffice(tenderId);
  const approveItems = useApproveAll(tenderId);
  const approveRates = useApproveAllRates(tenderId);
  const filter = params.get("show") ?? "all";

  const items = boq.data?.items ?? [];
  const facts = boq.data?.facts ?? [];
  const priced = new Map((estimate.data?.items ?? []).map((p) => [p.id, p]));
  const summary = estimate.data?.summary;
  const waitingItems = items.filter((i) => i.status === "proposed").length;
  const waitingRates = (estimate.data?.items ?? []).filter((p) => p.rate?.status === "proposed").length;
  const waiting = waitingItems + waitingRates;
  const shown = items.filter((i) => {
    const rate = priced.get(i.id)?.rate;
    if (filter === "waiting") return i.status === "proposed" || rate?.status === "proposed";
    if (filter === "unpriced") return !rate;
    return true;
  });
  const selected = items.find((i) => i.id === params.get("item"));
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));
  const keep = (extra: Record<string, string>) => setParams({ ...(filter === "all" ? {} : { show: filter }), ...extra });

  return (
    <div className="relative flex h-full w-full">
      <section aria-label="Bill of quantities" className="flex min-w-0 grow flex-col px-8 pt-7">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <div className="flex flex-col gap-1">
            <h1 className="text-[22px] font-semibold tracking-tight">Estimate</h1>
            <span className="text-ink-2">
              {summary?.priced ?? 0} of {items.length} {items.length === 1 ? "item" : "items"} priced
              {waiting > 0 && ` · ${waiting} need${waiting === 1 ? "s" : ""} you`}
            </span>
          </div>
          <div className="flex items-center gap-3">
            {summary && (
              <span className="flex flex-col items-end gap-0.5">
                <span className="text-xs whitespace-nowrap text-ink-3">Net, before markups</span>
                <span className="text-lg font-semibold">
                  {summary.currency} {money(summary.net)}
                </span>
              </span>
            )}
            <button
              onClick={() => keep({ view: "summary" })}
              className="h-[34px] rounded-lg border border-line-strong bg-white px-3 text-[13px] whitespace-nowrap"
            >
              Markups and summary
            </button>
            {waiting > 0 && (
              <button
                onClick={() => {
                  if (waitingItems) approveItems.mutate();
                  if (waitingRates) approveRates.mutate();
                }}
                disabled={approveItems.isPending || approveRates.isPending}
                className="h-[34px] rounded-lg bg-ink px-3.5 text-[13px] whitespace-nowrap text-white disabled:bg-line-strong"
              >
                Approve all {waiting}
              </button>
            )}
          </div>
        </div>

        {facts.length > 0 && (
          <div className="mt-5 flex flex-col gap-1.5">
            {facts.map((f) => (
              <FactLine key={f.id} tenderId={tenderId} fact={f} />
            ))}
          </div>
        )}

        <div className="mt-[18px] flex gap-[18px] border-b border-line">
          {[
            ["all", "All items"],
            ["waiting", `Needs you · ${waiting}`],
            ["unpriced", `Not priced · ${summary?.unpriced.length ?? 0}`],
          ].map(([key, label]) => (
            <button
              key={key}
              onClick={() => setParams(key === "all" ? {} : { show: key })}
              className={`h-8 text-[13px] ${filter === key ? "font-semibold shadow-[inset_0_-2px_0_var(--color-ink)]" : "text-ink-2"}`}
            >
              {label}
            </button>
          ))}
        </div>

        {items.length === 0 ? (
          <p className="pt-6 text-ink-2">
            No BOQ yet. Ask the office to enter the client’s BOQ, for example: “Enter the BOQ from the package.”
          </p>
        ) : (
          <div className="min-h-0 grow overflow-y-auto">
            <div className={`${COLUMNS} border-b border-line-strong px-2 pt-3.5 pb-2 text-xs text-ink-3`}>
              <span>Item</span>
              <span>Description</span>
              <span className="text-right">Qty</span>
              <span>Unit</span>
              <span className="text-right">Rate</span>
              <span className="text-right">Amount</span>
              <span>Status</span>
            </div>
            {shown.map((item, index) => {
              const heading = item.section && item.section !== shown[index - 1]?.section;
              const row = priced.get(item.id);
              const [label, dot] = rowStatus(item, row?.rate);
              return (
                <div key={item.id}>
                  {heading && <div className="px-2 pt-4 pb-1 text-xs font-semibold text-ink-2">{item.section}</div>}
                  <button
                    onClick={() => keep({ item: item.id })}
                    className={`${COLUMNS} w-full items-center border-b border-subtle px-2 py-3 text-left ${item.id === selected?.id ? "rounded-md bg-subtle" : "hover:bg-rail"}`}
                  >
                    <span className="text-ink-3">{item.item}</span>
                    <span className="min-w-0 truncate" dir="auto">
                      {item.description}
                    </span>
                    <span className="text-right">{quantity(item.quantity)}</span>
                    <span className="text-ink-3">
                      <bdi>{item.unit}</bdi>
                    </span>
                    <span className="text-right">{money(row?.rate?.rate)}</span>
                    <span className="text-right">{money(row?.amount)}</span>
                    <span className="flex items-center gap-1.5 text-ink-2">
                      <span className={`size-[7px] shrink-0 rounded-full ${dot}`} />
                      {label}
                    </span>
                  </button>
                </div>
              );
            })}
          </div>
        )}
        <div className="flex gap-[18px] border-t border-line px-2 py-3 text-xs text-ink-3">
          Quantix calculates every rate, amount and total from the approved quantities and build-ups.
        </div>
      </section>
      {params.get("view") === "summary" && summary ? (
        <SummaryPanel tenderId={tenderId} summary={summary} markups={estimate.data?.markups ?? null} />
      ) : (
        selected && (
          <ItemPanel tenderId={tenderId} item={selected} priced={priced.get(selected.id)} people={people} />
        )
      )}
    </div>
  );
}

/** Closes the side panel; on narrow windows the panel covers the table. */
function Close() {
  const [params, setParams] = useSearchParams();
  const show = params.get("show");
  return (
    <button
      aria-label="Close"
      onClick={() => setParams(show ? { show } : {})}
      className="-mt-3 -mr-2 self-end text-lg leading-none text-ink-3 hover:text-ink"
    >
      ×
    </button>
  );
}

function FactLine({ tenderId, fact }: { tenderId: string; fact: Fact }) {
  const decide = useDecide(tenderId);
  return (
    <div className="flex items-center gap-3 rounded-lg bg-rail px-3 py-2">
      <span className={`size-[7px] shrink-0 rounded-full ${statusDot(fact.status)}`} />
      <span className="grow" dir="auto">
        <span className="font-medium">{fact.label}:</span> {fact.value}{" "}
        <Link
          to={`/tenders/${tenderId}/documents?doc=${fact.source.document_id}&page=${fact.source.page}`}
          className="text-ink-3 underline-offset-2 hover:underline"
        >
          {fact.source.document_name}, page {fact.source.page}
        </Link>
      </span>
      {fact.status === "proposed" ? (
        <span className="flex gap-2">
          <button onClick={() => decide.mutate({ kind: "fact", id: fact.id, approve: true })} className="font-medium">
            Approve
          </button>
          <button onClick={() => decide.mutate({ kind: "fact", id: fact.id, approve: false })} className="text-ink-2">
            Reject
          </button>
        </span>
      ) : (
        <span className="text-ink-3">{statusLabel(fact.status)}</span>
      )}
    </div>
  );
}

function Decide(props: {
  who?: Staff;
  approveLabel: string;
  onApprove: () => void;
  onReject: (reason: string) => void;
  children?: React.ReactNode;
}) {
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  if (rejecting)
    return (
      <div className="flex flex-col gap-2">
        <textarea
          aria-label="Why reject it"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          placeholder={`Tell ${props.who ? firstName(props.who) : "the office"} what’s wrong`}
          rows={3}
          className="rounded-lg border border-line-strong px-3 py-2 outline-none focus:border-ink"
        />
        <div className="flex gap-2">
          <button onClick={() => props.onReject(reason)} className="h-[38px] grow rounded-lg bg-ink text-sm text-white">
            Send back
          </button>
          <button onClick={() => setRejecting(false)} className="h-[38px] rounded-lg border border-line-strong px-3.5 text-sm">
            Cancel
          </button>
        </div>
      </div>
    );
  return (
    <div className="flex flex-col gap-2">
      {props.children}
      <div className="flex gap-2">
        <button onClick={props.onApprove} className="h-[38px] grow rounded-lg bg-ink text-sm text-white">
          {props.approveLabel}
        </button>
        <button onClick={() => setRejecting(true)} className="h-[38px] rounded-lg border border-line-strong px-3.5 text-sm">
          Reject
        </button>
      </div>
    </div>
  );
}

function ItemPanel(props: { tenderId: string; item: BoqItem; priced?: Priced; people: Map<string, Staff> }) {
  const { tenderId, item } = props;
  const decideItem = useDecide(tenderId);
  const decideRate = useDecideRate(tenderId);
  const [save, setSave] = useState(false);
  const rate = props.priced?.rate;
  const enteredBy = props.people.get(item.proposed_by);
  const pricedBy = rate ? props.people.get(rate.proposed_by) : undefined;

  return (
    <aside aria-label={`Item ${item.item}`} className="flex w-[380px] shrink-0 flex-col gap-[18px] overflow-y-auto border-l border-line bg-white px-6 pt-7 pb-5 max-xl:absolute max-xl:inset-y-0 max-xl:right-0 max-xl:z-10 max-xl:bg-white max-xl:shadow-[-8px_0_24px_rgba(0,0,0,0.08)]">
      <Close />
      <div className="flex flex-col gap-1">
        <span className="text-ink-3">
          Item {item.item} · {quantity(item.quantity)} <bdi>{item.unit}</bdi>
        </span>
        <h2 className="text-[17px] leading-snug font-semibold" dir="auto">
          {item.description}
        </h2>
      </div>

      {rate && (
        <div className="flex flex-col">
          <div className="flex justify-between border-b border-line pb-1.5 text-xs text-ink-3">
            <span>{rate.lines ? `Build-up per ${item.unit}` : "Unit rate"}</span>
            <span>Cost</span>
          </div>
          {rate.lines?.map((line) => (
            <div key={line.resource} className="flex justify-between gap-3 border-b border-subtle py-2">
              <span className="flex flex-col gap-0.5">
                <span>{line.resource}</span>
                <span className="text-xs text-ink-3">
                  {quantity(line.quantity)} {line.unit}
                  {Number(line.wastage) > 0 && ` incl. ${Number(line.wastage) * 100}% waste`} × {money(line.rate)}
                </span>
              </span>
              <span>{money(line.cost)}</span>
            </div>
          ))}
          <div className="flex justify-between pt-2.5 font-semibold">
            <span>Rate</span>
            <span>{money(rate.rate)}</span>
          </div>
          <div className="flex justify-between pt-1 text-ink-2">
            <span>Amount</span>
            <span>{money(props.priced?.amount)}</span>
          </div>
        </div>
      )}

      {rate && (
        <div className="flex gap-2.5 rounded-[10px] bg-rail p-3 leading-normal">
          {pricedBy && <Face id={pricedBy.id} size={24} />}
          <span className="flex flex-col gap-1">
            <span className="font-medium">
              {rate.basis === "quote" ? "From a quote" : rate.basis === "library" ? "From the company library" : "Estimated"}
              {pricedBy && <span className="font-normal text-ink-3"> · {firstName(pricedBy)}</span>}
            </span>
            <span className="text-[#27272A]">{rate.note}</span>
            {rate.document_id && (
              <Link
                to={`/tenders/${tenderId}/documents?doc=${rate.document_id}&page=${rate.page}`}
                className="font-medium underline underline-offset-4"
              >
                {rate.source_document}, page {rate.page}
              </Link>
            )}
          </span>
        </div>
      )}

      <div className="flex flex-col gap-1.5">
        <span className="text-xs font-semibold text-ink-2">From the client’s BOQ</span>
        <Link
          to={`/tenders/${tenderId}/documents?doc=${item.source.document_id}&page=${item.source.page}`}
          className="underline underline-offset-4"
        >
          {item.source.document_name}, page {item.source.page}
        </Link>
        <p className="rounded-lg bg-rail p-3 leading-relaxed text-[#27272A]" dir="auto">
          {item.source.quote}
        </p>
        {enteredBy && <span className="text-ink-3">Entered by {firstName(enteredBy)}</span>}
      </div>

      <div className="grow" />
      {item.status === "proposed" ? (
        <Decide
          who={enteredBy}
          approveLabel="Approve item"
          onApprove={() => decideItem.mutate({ kind: "item", id: item.id, approve: true })}
          onReject={(reason) => decideItem.mutate({ kind: "item", id: item.id, approve: false, reason })}
        />
      ) : (
        rate?.status === "proposed" && (
          <Decide
            who={pricedBy}
            approveLabel="Approve rate"
            onApprove={() => decideRate.mutate({ id: rate.id, approve: true, save_to_library: save })}
            onReject={(reason) => decideRate.mutate({ id: rate.id, approve: false, reason })}
          >
            <label className="flex items-center gap-2 text-ink-2">
              <input type="checkbox" checked={save} onChange={(e) => setSave(e.target.checked)} className="accent-ink" />
              Save to the company library
            </label>
          </Decide>
        )
      )}
      {(decideItem.isError || decideRate.isError) && (
        <p className="text-attention">{(decideItem.error ?? decideRate.error)?.message}</p>
      )}
    </aside>
  );
}

function SummaryPanel({ tenderId, summary, markups }: { tenderId: string; summary: Summary; markups: Markups | null }) {
  const decide = useDecideMarkups(tenderId);
  const pct = (value: string) => `${(Number(value) * 100).toFixed(1).replace(/\.0$/, "")}%`;
  const rows: [string, string][] = [
    ["Net cost", summary.net],
    [`Preliminaries${markups ? ` ${pct(markups.preliminaries)}` : ""}`, summary.preliminaries],
    [`Overheads${markups ? ` ${pct(markups.overheads)}` : ""}`, summary.overheads],
    [`Profit${markups ? ` ${pct(markups.profit)}` : ""}`, summary.profit],
    ["Adjustment", summary.adjustment],
  ];
  return (
    <aside aria-label="Price summary" className="flex w-[380px] shrink-0 flex-col gap-4 overflow-y-auto border-l border-line bg-white px-6 pt-7 pb-5 max-xl:absolute max-xl:inset-y-0 max-xl:right-0 max-xl:z-10 max-xl:bg-white max-xl:shadow-[-8px_0_24px_rgba(0,0,0,0.08)]">
      <Close />
      <h2 className="text-[17px] font-semibold">Price summary</h2>
      <div className="flex flex-col">
        {rows.map(([label, value]) => (
          <div key={label} className="flex justify-between border-b border-subtle py-2">
            <span className="text-ink-2">{label}</span>
            <span>{money(value)}</span>
          </div>
        ))}
        <div className="flex justify-between py-2.5 font-semibold">
          <span>Total {summary.vat !== null && "excluding VAT"}</span>
          <span>
            {summary.currency} {money(summary.total)}
          </span>
        </div>
        {summary.vat !== null && (
          <>
            <div className="flex justify-between border-t border-subtle py-2 text-ink-2">
              <span>VAT {summary.vat_rate && pct(summary.vat_rate)}</span>
              <span>{money(summary.vat)}</span>
            </div>
            <div className="flex justify-between py-2 font-semibold">
              <span>Total including VAT</span>
              <span>
                {summary.currency} {money(summary.total_with_vat)}
              </span>
            </div>
          </>
        )}
      </div>
      {summary.unpriced.length > 0 && (
        <p className="text-attention">
          {summary.unpriced.length} {summary.unpriced.length === 1 ? "item is" : "items are"} not priced yet.
        </p>
      )}
      {summary.waiting > 0 && (
        <p className="text-ink-2">
          The total includes {summary.waiting} {summary.waiting === 1 ? "rate" : "rates"} waiting for you.
        </p>
      )}
      {markups ? (
        <div className="flex flex-col gap-2 rounded-[10px] bg-rail p-3">
          <span className="font-medium">Markups · {statusLabel(markups.status)}</span>
          <span className="leading-normal text-[#27272A]">{markups.note}</span>
          {markups.status === "proposed" && (
            <span className="flex gap-3">
              <button onClick={() => decide.mutate({ id: markups.id, approve: true })} className="font-medium">
                Approve markups
              </button>
              <button onClick={() => decide.mutate({ id: markups.id, approve: false })} className="text-ink-2">
                Reject
              </button>
            </span>
          )}
        </div>
      ) : (
        <p className="text-ink-2">No markups yet. Ask the office to propose them.</p>
      )}
    </aside>
  );
}
