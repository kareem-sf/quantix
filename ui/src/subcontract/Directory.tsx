import { useState, type FormEvent } from "react";
import { useAddCompany, useDirectory, useRemoveCompany } from "./queries";

const COLUMNS = "grid grid-cols-[minmax(0,1fr)_104px_minmax(0,1fr)_minmax(0,0.9fr)_64px] gap-3";

export function Directory() {
  const [query, setQuery] = useState("");
  const directory = useDirectory(query);
  const remove = useRemoveCompany();
  const rows = directory.data ?? [];

  return (
    <div className="flex w-full max-w-[1100px] flex-col px-8 pt-7">
      <div className="flex items-end justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="text-[22px] font-semibold tracking-tight">Directory</h1>
          <span className="text-ink-2">Subcontractors and suppliers the office sends enquiries to.</span>
        </div>
        <input
          aria-label="Search the directory"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search by name or trade"
          className="h-9 w-64 rounded-lg border border-line-strong px-3 outline-none focus:border-ink"
        />
      </div>
      <div className={`${COLUMNS} mt-5 border-b border-line-strong px-2 pb-2 text-xs text-ink-3`}>
        <span>Company</span>
        <span>Kind</span>
        <span>Trades</span>
        <span>Contact</span>
        <span />
      </div>
      {rows.length === 0 && directory.data && (
        <p className="px-2 py-4 text-ink-2">{query ? "Nothing matches." : "Empty. Add a company below."}</p>
      )}
      {rows.map((c) => (
        <div key={c.id} className={`${COLUMNS} items-center border-b border-subtle px-2 py-2.5`}>
          <span className="min-w-0 truncate" dir="auto">
            {c.name}
          </span>
          <span className="text-ink-3">{c.kind === "supplier" ? "Supplier" : "Subcontractor"}</span>
          <span className="min-w-0 truncate text-ink-2" dir="auto">
            {c.trades}
          </span>
          <span className="min-w-0 truncate text-ink-3">{[c.email, c.phone].filter(Boolean).join(" · ")}</span>
          <button onClick={() => remove.mutate(c.id)} className="text-right text-ink-3 hover:text-ink">
            Remove
          </button>
        </div>
      ))}
      {remove.isError && <p className="pt-2 text-attention">{remove.error.message}</p>}
      <AddCompany />
    </div>
  );
}

function AddCompany() {
  const add = useAddCompany();
  const blank = { name: "", kind: "subcontractor" as "subcontractor" | "supplier", trades: "", email: "", phone: "" };
  const [company, setCompany] = useState(blank);
  const field = "h-9 rounded-lg border border-line-strong px-3 outline-none focus:border-ink";
  const set = (key: keyof typeof blank) => (e: { target: { value: string } }) =>
    setCompany({ ...company, [key]: e.target.value });

  function submit(event: FormEvent) {
    event.preventDefault();
    add.mutate(
      { ...company, email: company.email || null, phone: company.phone || null },
      { onSuccess: () => setCompany(blank) },
    );
  }

  return (
    <form onSubmit={submit} className="mt-6 flex flex-col gap-2 border-t border-line pt-5">
      <h2 className="font-semibold text-ink-2">Add a company</h2>
      <div className="flex flex-wrap gap-2">
        <input aria-label="Company" required placeholder="Company" value={company.name} onChange={set("name")} className={`${field} grow`} />
        <select aria-label="Kind" value={company.kind} onChange={set("kind")} className={field}>
          <option value="subcontractor">Subcontractor</option>
          <option value="supplier">Supplier</option>
        </select>
        <input aria-label="Trades" required placeholder="Trades, e.g. waterproofing" value={company.trades} onChange={set("trades")} className={`${field} grow`} />
        <input aria-label="Email" type="email" placeholder="Email" value={company.email} onChange={set("email")} className={`${field} w-52`} />
        <input aria-label="Phone" placeholder="Phone" value={company.phone} onChange={set("phone")} className={`${field} w-36`} />
        <button className="h-9 rounded-lg bg-ink px-4 text-white">Add</button>
      </div>
      {add.isError && <p className="text-attention">{add.error.message}</p>}
    </form>
  );
}
