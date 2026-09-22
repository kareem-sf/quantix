import { useState } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { Face } from "../office/Face";
import { firstName, useOffice, type Staff } from "../office/queries";
import {
  quantity,
  statusDot,
  statusLabel,
  useApproveAll,
  useBoq,
  useDecide,
  type BoqItem,
  type Fact,
} from "./queries";

const COLUMNS = "grid grid-cols-[64px_minmax(0,1fr)_96px_48px_150px] gap-3";

export function Estimate() {
  const { tenderId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const boq = useBoq(tenderId);
  const office = useOffice(tenderId);
  const approveAll = useApproveAll(tenderId);
  const filter = params.get("show") ?? "all";

  const items = boq.data?.items ?? [];
  const facts = boq.data?.facts ?? [];
  const waiting = items.filter((i) => i.status === "proposed").length;
  const shown = filter === "waiting" ? items.filter((i) => i.status === "proposed") : items;
  const selected = items.find((i) => i.id === params.get("item"));
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));
  const select = (id: string) => setParams({ ...(filter === "all" ? {} : { show: filter }), item: id });

  return (
    <div className="flex h-full w-full">
      <section aria-label="Bill of quantities" className="flex min-w-0 grow flex-col px-8 pt-7">
        <div className="flex items-end justify-between gap-4">
          <div className="flex flex-col gap-1">
            <h1 className="text-[22px] font-semibold tracking-tight">Estimate</h1>
            <span className="text-ink-2">
              {items.length} BOQ {items.length === 1 ? "item" : "items"}
              {waiting > 0 && ` · ${waiting} need${waiting === 1 ? "s" : ""} you`}
            </span>
          </div>
          {waiting > 0 && (
            <button
              onClick={() => approveAll.mutate()}
              disabled={approveAll.isPending}
              className="h-[34px] rounded-lg bg-ink px-3.5 text-[13px] text-white disabled:bg-line-strong"
            >
              Approve all {waiting}
            </button>
          )}
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
              <span>Status</span>
            </div>
            {shown.map((item, index) => {
              const heading = item.section && item.section !== shown[index - 1]?.section;
              return (
                <div key={item.id}>
                  {heading && <div className="px-2 pt-4 pb-1 text-xs font-semibold text-ink-2">{item.section}</div>}
                  <button
                    onClick={() => select(item.id)}
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
                    <span className="flex items-center gap-1.5 text-ink-2">
                      <span className={`size-[7px] shrink-0 rounded-full ${statusDot(item.status)}`} />
                      {statusLabel(item.status)}
                    </span>
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </section>
      {selected && <ItemPanel tenderId={tenderId} item={selected} by={people.get(selected.proposed_by)} />}
    </div>
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

function ItemPanel({ tenderId, item, by }: { tenderId: string; item: BoqItem; by?: Staff }) {
  const decide = useDecide(tenderId);
  const [reason, setReason] = useState("");
  const [rejecting, setRejecting] = useState(false);

  return (
    <aside aria-label={`Item ${item.item}`} className="flex w-[380px] shrink-0 flex-col gap-[18px] border-l border-line px-6 pt-7 pb-5">
      <div className="flex flex-col gap-1">
        <span className="text-ink-3">
          Item {item.item} · {quantity(item.quantity)} <bdi>{item.unit}</bdi>
        </span>
        <h2 className="text-[17px] leading-snug font-semibold" dir="auto">
          {item.description}
        </h2>
      </div>
      <div className="flex flex-col gap-1.5">
        <span className="text-xs font-semibold text-ink-2">From the client’s BOQ</span>
        <Link
          to={`/tenders/${tenderId}/documents?doc=${item.source.document_id}&page=${item.source.page}`}
          className="font-medium underline underline-offset-4"
        >
          {item.source.document_name}, page {item.source.page}
        </Link>
        <p className="rounded-lg bg-rail p-3 leading-relaxed text-[#27272A]" dir="auto">
          {item.source.quote}
        </p>
      </div>
      {by && (
        <div className="flex items-center gap-2.5 text-ink-2">
          <Face id={by.id} size={22} />
          Entered by {firstName(by)}, {by.role}
        </div>
      )}
      <div className="grow" />
      {item.status === "proposed" &&
        (rejecting ? (
          <div className="flex flex-col gap-2">
            <textarea
              aria-label="Why reject it"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder={`Tell ${by ? firstName(by) : "the office"} what’s wrong`}
              rows={3}
              className="rounded-lg border border-line-strong px-3 py-2 outline-none focus:border-ink"
            />
            <div className="flex gap-2">
              <button
                onClick={() => decide.mutate({ kind: "item", id: item.id, approve: false, reason })}
                className="h-[38px] grow rounded-lg bg-ink text-sm text-white"
              >
                Send back
              </button>
              <button onClick={() => setRejecting(false)} className="h-[38px] rounded-lg border border-line-strong px-3.5 text-sm">
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <div className="flex gap-2">
            <button
              onClick={() => decide.mutate({ kind: "item", id: item.id, approve: true })}
              className="h-[38px] grow rounded-lg bg-ink text-sm text-white"
            >
              Approve item
            </button>
            <button onClick={() => setRejecting(true)} className="h-[38px] rounded-lg border border-line-strong px-3.5 text-sm">
              Reject
            </button>
          </div>
        ))}
      {decide.isError && <p className="text-attention">{decide.error.message}</p>}
    </aside>
  );
}
