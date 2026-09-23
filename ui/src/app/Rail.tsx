import { Link, NavLink, useParams } from "react-router";
import { Face } from "../office/Face";
import { firstName, useOffice } from "../office/queries";
import { dueShort } from "../tenders/due";
import { useTenders } from "../tenders/queries";

const STAGES = [
  ["Overview", ""],
  ["Office", "/office"],
  ["Documents", "/documents"],
  ["Takeoff", "/takeoff"],
  ["Estimate", "/estimate"],
  ["Subcontract", "/subcontract"],
] as const;

export function Rail() {
  const { tenderId } = useParams();
  const tenders = useTenders();

  return (
    <nav aria-label="Quantix" className="flex w-60 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-line bg-rail px-3 py-[18px]">
      <div className="flex items-center justify-between px-2 pb-4">
        <Link to="/" className="text-[15px] font-semibold">
          Quantix
        </Link>
        <Link
          to="/new"
          className="rounded-md border border-line-strong bg-white px-2 py-0.5 text-xs text-ink-2 hover:text-ink"
        >
          New tender
        </Link>
      </div>
      <span className="px-2 pb-1.5 text-xs text-ink-3">Tenders</span>
      {tenders.isError && <span className="px-2 text-ink-2">Waiting for the Quantix service…</span>}
      {tenders.data?.map((tender) => (
        <div key={tender.id} className="flex flex-col gap-0.5">
          <Link
            to={`/tenders/${tender.id}`}
            className={`flex flex-col gap-px rounded-md px-2 py-[7px] ${tender.id === tenderId ? "bg-selected" : "hover:bg-selected/60"}`}
          >
            <span className={tender.id === tenderId ? "font-medium" : ""}>{tender.name}</span>
            <span className="text-xs text-ink-3">{dueShort(tender.due_date)}</span>
          </Link>
          {tender.id === tenderId &&
            STAGES.map(([label, path]) => (
              <NavLink
                key={label}
                to={`/tenders/${tender.id}${path}`}
                end
                className={({ isActive }) =>
                  `rounded-md py-1.5 pr-2 pl-5 ${isActive ? "bg-white font-semibold shadow-[0_0_0_1px_var(--color-line)]" : "text-ink-2"}`
                }
              >
                {label}
              </NavLink>
            ))}
        </div>
      ))}
      {tenderId && <People tenderId={tenderId} />}
      <div className="grow" />
      <NavLink
        to="/directory"
        className={({ isActive }) => `rounded-md px-2 py-[7px] ${isActive ? "font-semibold" : "text-ink-2 hover:text-ink"}`}
      >
        Directory
      </NavLink>
      <NavLink
        to="/library"
        className={({ isActive }) => `rounded-md px-2 py-[7px] ${isActive ? "font-semibold" : "text-ink-2 hover:text-ink"}`}
      >
        Company library
      </NavLink>
      <NavLink
        to="/settings"
        className={({ isActive }) => `rounded-md px-2 py-[7px] ${isActive ? "font-semibold" : "text-ink-2 hover:text-ink"}`}
      >
        Settings
      </NavLink>
    </nav>
  );
}

function People({ tenderId }: { tenderId: string }) {
  const office = useOffice(tenderId);
  const active = (office.data?.staff ?? []).filter((m) => m.status === "active");
  if (active.length === 0) return null;
  return (
    <>
      <span className="px-2 pt-5 pb-1.5 text-xs text-ink-3">Office</span>
      {active.map((m) => (
        <Link
          key={m.id}
          to={`/tenders/${tenderId}/office?with=${m.id}`}
          className="flex items-start gap-[9px] rounded-md px-2 py-1.5 hover:bg-selected/60"
        >
          <Face id={m.id} size={24} />
          <span className="flex min-w-0 flex-col gap-px">
            <span className="font-medium">
              {firstName(m)} <span className="font-normal text-ink-3">{m.is_manager ? "Manager" : m.role}</span>
            </span>
            <span className="text-xs leading-snug text-ink-2">{m.now ?? "Idle"}</span>
          </span>
        </Link>
      ))}
    </>
  );
}
