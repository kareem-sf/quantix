import { IconPlus } from "@tabler/icons-react";
import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Opening } from "../app/Opening";
import { Register } from "./Desk";
import { useDesk, type TenderGlance } from "./queries";

const TABS: [key: string, label: string, holds: (t: TenderGlance) => boolean][] = [
  ["open", "Open", (t) => !t.archived && t.outcome === "open"],
  ["submitted", "Submitted", (t) => !t.archived && t.outcome === "submitted"],
  ["won", "Won", (t) => !t.archived && t.outcome === "won"],
  ["lost", "Lost", (t) => !t.archived && t.outcome === "lost"],
  ["archived", "Archived", (t) => t.archived],
];

/** Every tender the firm has priced: open, submitted, won, lost and put away. Won and lost prices benchmark new
 * rates, so nothing is thrown away by archiving. */
export function Tenders() {
  const desk = useDesk();
  const [params, setParams] = useSearchParams();
  const [query, setQuery] = useState("");
  if (!desk.data) return <Opening error={desk.isError} />;
  const tab = TABS.find(([key]) => key === params.get("show")) ?? TABS[0];
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const shown = desk.data
    .filter(tab[2])
    .filter((t) => words.every((w) => t.name.toLowerCase().includes(w)))
    .sort((a, b) =>
      tab[0] === "open"
        ? (a.due_date ? Date.parse(a.due_date) : Infinity) - (b.due_date ? Date.parse(b.due_date) : Infinity)
        : Date.parse(b.outcome_at ?? b.created_at) - Date.parse(a.outcome_at ?? a.created_at),
    );

  return (
    <div className="flex w-full max-w-[1120px] flex-col px-8 pb-10">
      <div className="sticky top-0 z-10 -mx-8 bg-white px-8 pt-8">
        <div className="flex items-end justify-between gap-4">
          <div className="flex flex-col gap-1">
            <h1 className="text-[24px] font-semibold tracking-tight">Tenders</h1>
            <span className="text-ink-2">
              Every tender, open and closed. Won and lost prices stay to benchmark new rates.
            </span>
          </div>
          <span className="flex items-center gap-2">
            <input
              aria-label="Find a tender"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Find a tender"
              className="h-9 w-56 rounded-lg border border-line-strong px-3 outline-none focus:border-ink"
            />
            <Link to="/new" className="flex h-9 items-center gap-1.5 rounded-lg bg-ink px-3.5 text-white">
              <IconPlus className="size-4" stroke={1.75} />
              New tender
            </Link>
          </span>
        </div>
        <div role="tablist" aria-label="Tenders by outcome" className="mt-5 mb-1 flex gap-5 border-b border-line">
          {TABS.map(([key, label, holds]) => (
            <button
              key={key}
              role="tab"
              aria-selected={key === tab[0]}
              onClick={() => setParams(key === "open" ? {} : { show: key })}
              className={`h-8 ${key === tab[0] ? "font-semibold shadow-[inset_0_-2px_0_var(--color-ink)]" : "text-ink-2 hover:text-ink"}`}
            >
              {label} <span className="font-normal text-ink-3">{desk.data.filter(holds).length}</span>
            </button>
          ))}
        </div>
      </div>
      <Register tenders={shown} closed={tab[0] !== "open"} />
    </div>
  );
}
