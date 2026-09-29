import { useState, type FormEvent } from "react";
import { MaybeSameFirm, useAddCompany, useDirectory, useMergeCompany, useRemoveCompany, type Company } from "./queries";

const COLUMNS = "grid grid-cols-[minmax(0,1fr)_104px_minmax(0,1fr)_minmax(0,0.9fr)_150px] gap-3";

export function Directory() {
  const [query, setQuery] = useState("");
  const directory = useDirectory(query);
  const everyone = useDirectory("");
  const remove = useRemoveCompany();
  const [merging, setMerging] = useState<string | null>(null);
  const rows = directory.data ?? [];

  return (
    <div className="flex w-full max-w-[1100px] flex-col px-8 pt-7">
      <div className="flex items-end justify-between gap-4">
        <div className="flex flex-col gap-1">
          <h1 className="text-[24px] font-semibold tracking-tight">Directory</h1>
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
        <div key={c.id} className="border-b border-subtle">
          <div className={`${COLUMNS} items-center px-2 py-2.5`}>
            <span className="flex min-w-0 flex-col">
              <span className="truncate" dir="auto">
                {c.name}
              </span>
              {c.aliases.length > 0 && (
                <span className="truncate text-xs text-ink-3" dir="auto">
                  Also {c.aliases.join(" · ")}
                </span>
              )}
            </span>
            <span className="text-ink-3">{c.kind === "supplier" ? "Supplier" : "Subcontractor"}</span>
            <span className="min-w-0 truncate text-ink-2" dir="auto">
              {c.trades}
            </span>
            <span className="min-w-0 truncate text-ink-3">
              {[c.email, c.phone].filter(Boolean).join(" · ")}
              {c.website && (
                <>
                  {(c.email || c.phone) && " · "}
                  <a href={c.website} target="_blank" rel="noreferrer" className="underline underline-offset-2">
                    Website
                  </a>
                </>
              )}
            </span>
            <span className="flex justify-end gap-3">
              <button onClick={() => setMerging(merging === c.id ? null : c.id)} className="text-ink-3 hover:text-ink">
                Same firm as…
              </button>
              <button onClick={() => remove.mutate(c.id)} className="text-ink-3 hover:text-ink">
                Remove
              </button>
            </span>
          </div>
          {merging === c.id && (
            <Merge
              company={c}
              others={(everyone.data ?? []).filter((o) => o.id !== c.id)}
              onDone={() => setMerging(null)}
            />
          )}
        </div>
      ))}
      {remove.isError && <p className="pt-2 text-attention">{remove.error.message}</p>}
      <AddCompany />
    </div>
  );
}

/** The engineer's word that a firm was entered twice: its enquiries and quotes go to the other, which keeps its name. */
function Merge(props: { company: Company; others: Company[]; onDone: () => void }) {
  const merge = useMergeCompany();
  const [into, setInto] = useState("");
  return (
    <div className="flex flex-col gap-1 px-2 pb-3">
      <div className="flex items-center gap-2">
        <span className="text-ink-2" dir="auto">
          {props.company.name} is the same firm as
        </span>
        <select
          aria-label="The same firm as"
          value={into}
          onChange={(e) => setInto(e.target.value)}
          className="h-8 rounded-lg border border-line-strong px-2 outline-none focus:border-ink"
        >
          <option value="">Choose a firm</option>
          {props.others.map((o) => (
            <option key={o.id} value={o.id}>
              {o.name}
            </option>
          ))}
        </select>
        <button
          disabled={!into || merge.isPending}
          onClick={() => merge.mutate({ id: props.company.id, into }, { onSuccess: props.onDone })}
          className="h-8 rounded-lg bg-ink px-3 text-white disabled:opacity-40"
        >
          Merge
        </button>
        <button onClick={props.onDone} className="text-ink-3 hover:text-ink">
          Cancel
        </button>
      </div>
      <span className="text-xs text-ink-3">Its enquiries and quotes move to that firm, which keeps this name too.</span>
      {merge.isError && <p className="text-attention">{merge.error.message}</p>}
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

  function save(differentFrom: string[]) {
    add.mutate(
      { ...company, email: company.email || null, phone: company.phone || null, different_from: differentFrom },
      { onSuccess: () => setCompany(blank) },
    );
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    save([]);
  }

  return (
    <form onSubmit={submit} className="mt-6 flex flex-col gap-2 border-t border-line pt-5">
      <h2 className="font-semibold text-ink-2">Add a company</h2>
      <div className="grid grid-cols-[minmax(0,1fr)_150px_minmax(0,1fr)] gap-2">
        <input aria-label="Company" required placeholder="Company" value={company.name} onChange={set("name")} className={field} />
        <select aria-label="Kind" value={company.kind} onChange={set("kind")} className={field}>
          <option value="subcontractor">Subcontractor</option>
          <option value="supplier">Supplier</option>
        </select>
        <input aria-label="Trades" required placeholder="Trades, e.g. waterproofing" value={company.trades} onChange={set("trades")} className={field} />
      </div>
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] gap-2">
        <input aria-label="Email" type="email" placeholder="Email" value={company.email} onChange={set("email")} className={field} />
        <input aria-label="Phone" placeholder="Phone" value={company.phone} onChange={set("phone")} className={field} />
        <button className="h-9 rounded-lg bg-ink px-4 text-white">Add</button>
      </div>
      {add.error instanceof MaybeSameFirm ? (
        <div className="flex items-center gap-3">
          <p className="text-attention">{add.error.message}</p>
          <button
            type="button"
            onClick={() => save((add.error as MaybeSameFirm).firms)}
            className="shrink-0 text-ink underline-offset-2 hover:underline"
          >
            Add as a different firm
          </button>
        </div>
      ) : (
        add.isError && <p className="text-attention">{add.error.message}</p>
      )}
    </form>
  );
}
