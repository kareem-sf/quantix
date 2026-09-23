import { useState, type FormEvent } from "react";
import { money, useAddToLibrary, useLibrary, useRemoveFromLibrary, type LibraryEntry } from "../estimate/queries";

const KINDS: [LibraryEntry["kind"], string][] = [
  ["labour", "Labour"],
  ["plant", "Plant"],
  ["material", "Material"],
  ["subcontract", "Subcontract"],
  ["unit_rate", "Unit rate"],
];
const COLUMNS = "grid grid-cols-[96px_minmax(0,1fr)_56px_110px_96px_minmax(0,0.8fr)_64px] gap-3";

export function Library() {
  const [query, setQuery] = useState("");
  const library = useLibrary(query);
  const remove = useRemoveFromLibrary();
  const rows = library.data ?? [];

  return (
    <div className="flex w-full max-w-[1100px] flex-col px-8 pt-7">
      <div className="flex items-end justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="text-[22px] font-semibold tracking-tight">Company library</h1>
          <span className="text-ink-2">Rates the firm reuses across tenders. Staff check each date before relying on it.</span>
        </div>
        <input
          aria-label="Search the library"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search"
          className="h-9 w-64 rounded-lg border border-line-strong px-3 outline-none focus:border-ink"
        />
      </div>
      <div className={`${COLUMNS} mt-5 border-b border-line-strong px-2 pb-2 text-xs text-ink-3`}>
        <span>Kind</span>
        <span>Name</span>
        <span>Unit</span>
        <span className="text-right">Rate</span>
        <span>Dated</span>
        <span>Source</span>
        <span />
      </div>
      {rows.length === 0 && (
        <p className="px-2 py-4 text-ink-2">
          {query ? "Nothing matches." : "Empty. Approve a rate with “Save to the company library”, or add one below."}
        </p>
      )}
      {rows.map((r) => (
        <div key={r.id} className={`${COLUMNS} items-center border-b border-subtle px-2 py-2.5`}>
          <span className="text-ink-3">{KINDS.find(([k]) => k === r.kind)?.[1]}</span>
          <span className="min-w-0 truncate" dir="auto">
            {r.name}
          </span>
          <span className="text-ink-3">
            <bdi>{r.unit}</bdi>
          </span>
          <span className="text-right">
            {money(r.rate)} {r.currency}
          </span>
          <span className="text-ink-3">{r.dated}</span>
          <span className="min-w-0 truncate text-ink-3">{r.source}</span>
          <button onClick={() => remove.mutate(r.id)} className="text-right text-ink-3 hover:text-ink">
            Remove
          </button>
        </div>
      ))}
      {remove.isError && <p className="pt-2 text-attention">{remove.error.message}</p>}
      <AddEntry />
    </div>
  );
}

function AddEntry() {
  const add = useAddToLibrary();
  const blank = { kind: "material" as LibraryEntry["kind"], name: "", unit: "", rate: "", currency: "SAR", source: "" };
  const [entry, setEntry] = useState(blank);
  const field = "h-9 rounded-lg border border-line-strong px-3 outline-none focus:border-ink";
  const set = (key: keyof typeof blank) => (e: { target: { value: string } }) => setEntry({ ...entry, [key]: e.target.value });

  function submit(event: FormEvent) {
    event.preventDefault();
    add.mutate(
      { ...entry, dated: new Date().toISOString().slice(0, 10) },
      { onSuccess: () => setEntry(blank) },
    );
  }

  return (
    <form onSubmit={submit} className="mt-6 flex flex-col gap-2 border-t border-line pt-5">
      <h2 className="font-semibold text-ink-2">Add a rate</h2>
      <div className="flex flex-wrap gap-2">
        <select aria-label="Kind" value={entry.kind} onChange={set("kind")} className={field}>
          {KINDS.map(([k, label]) => (
            <option key={k} value={k}>
              {label}
            </option>
          ))}
        </select>
        <input aria-label="Name" required placeholder="Name" value={entry.name} onChange={set("name")} className={`${field} grow`} />
        <input aria-label="Unit" required placeholder="Unit" value={entry.unit} onChange={set("unit")} className={`${field} w-20`} />
        <input aria-label="Rate" required inputMode="decimal" placeholder="Rate" value={entry.rate} onChange={set("rate")} className={`${field} w-28`} />
        <input aria-label="Currency" required value={entry.currency} onChange={set("currency")} className={`${field} w-20`} />
        <input aria-label="Source" required placeholder="Where the rate comes from" value={entry.source} onChange={set("source")} className={`${field} grow`} />
        <button className="h-9 rounded-lg bg-ink px-4 text-white">Add</button>
      </div>
      {add.isError && <p className="text-attention">{add.error.message}</p>}
    </form>
  );
}
