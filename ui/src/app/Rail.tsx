import { Link, NavLink, useParams } from "react-router";
import { dueShort } from "../tenders/due";
import { useTenders } from "../tenders/queries";

export function Rail() {
  const { tenderId } = useParams();
  const tenders = useTenders();

  return (
    <nav aria-label="Quantix" className="flex w-60 shrink-0 flex-col gap-0.5 border-r border-line bg-rail px-3 py-[18px]">
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
          {tender.id === tenderId && (
            <NavLink
              to={`/tenders/${tender.id}`}
              end
              className={({ isActive }) =>
                `rounded-md py-1.5 pr-2 pl-5 ${isActive ? "bg-white font-semibold shadow-[0_0_0_1px_var(--color-line)]" : "text-ink-2"}`
              }
            >
              Overview
            </NavLink>
          )}
        </div>
      ))}
      <div className="grow" />
      <NavLink
        to="/settings"
        className={({ isActive }) => `rounded-md px-2 py-[7px] ${isActive ? "font-semibold" : "text-ink-2 hover:text-ink"}`}
      >
        Settings
      </NavLink>
    </nav>
  );
}
