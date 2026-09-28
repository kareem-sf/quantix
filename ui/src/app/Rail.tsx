import {
  IconAddressBook,
  IconBook2,
  IconBuilding,
  IconChevronUp,
  IconListCheck,
  IconSettings,
} from "@tabler/icons-react";
import { useEffect, useRef, useState } from "react";
import { Link, NavLink, useLocation, useParams } from "react-router";
import { Face } from "../office/Face";
import { firstName, useOffice } from "../office/queries";
import { dueShort, dueSource } from "../tenders/due";
import { useTenders } from "../tenders/queries";
import { Logo } from "./Logo";

const STAGES = [
  ["Overview", ""],
  ["Office", "/office"],
  ["Documents", "/documents"],
  ["Takeoff", "/takeoff"],
  ["Queries", "/queries"],
  ["Estimate", "/estimate"],
  ["Subcontract", "/subcontract"],
  ["Submission", "/submission"],
] as const;

const SETTINGS = [
  ["Company details", "/company", IconBuilding],
  ["Directory", "/directory", IconAddressBook],
  ["Company library", "/library", IconBook2],
  ["Company rules", "/rules", IconListCheck],
  ["Settings", "/settings", IconSettings],
] as const;

export function Rail() {
  const { tenderId } = useParams();
  const tenders = useTenders();

  return (
    <nav aria-label="Quantix" className="flex w-60 shrink-0 flex-col border-r border-line bg-rail py-[18px]">
      <div className="flex items-center justify-between px-5 pb-4">
        <Link to="/" className="flex items-center gap-2 text-[15px] font-semibold">
          <Logo className="h-3.5" />
          Quantix
        </Link>
        <Link
          to="/new"
          className="rounded-md border border-line-strong bg-white px-2 py-0.5 text-xs text-ink-2 hover:text-ink"
        >
          New tender
        </Link>
      </div>
      <div className="flex min-h-0 grow flex-col gap-0.5 overflow-y-auto px-3">
        <span className="px-2 pb-1.5 text-xs text-ink-3">Tenders</span>
        {tenders.isError && <span className="px-2 text-ink-2">Waiting for the Quantix service…</span>}
        {tenders.data?.map((tender) => (
          <div key={tender.id} className="flex flex-col gap-0.5">
            <Link
              to={`/tenders/${tender.id}`}
              className={`flex flex-col gap-px rounded-md px-2 py-[7px] ${tender.id === tenderId ? "bg-selected" : "hover:bg-selected/60"}`}
            >
              <span className={tender.id === tenderId ? "font-medium" : ""}>{tender.name}</span>
              <span className="text-xs text-ink-3" title={dueSource(tender.due_date_source)?.title}>
                {dueShort(tender.due_date)}
              </span>
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
      </div>
      <div className="border-t border-line px-3 pt-2.5">
        <SettingsMenu />
      </div>
    </nav>
  );
}

function SettingsMenu() {
  const [open, setOpen] = useState(false);
  const menu = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const { pathname } = useLocation();
  const here = SETTINGS.some(([, path]) => pathname.startsWith(path));

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: MouseEvent) => {
      if (!menu.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      setOpen(false);
      button.current?.focus();
    };
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={menu} className="relative">
      {open && (
        <div
          id="settings-screens"
          className="absolute inset-x-0 bottom-full z-10 mb-1.5 flex flex-col gap-0.5 rounded-lg border border-line-strong bg-white p-1 shadow-[0_8px_24px_rgb(0_0_0/0.08)] transition-[opacity,translate] duration-150 starting:translate-y-1 starting:opacity-0"
        >
          {SETTINGS.map(([label, path, Icon]) => (
            <NavLink
              key={path}
              to={path}
              onClick={() => setOpen(false)}
              className={({ isActive }) =>
                `flex items-center gap-2 rounded-md px-2 py-[7px] ${isActive ? "bg-selected font-medium text-ink" : "text-ink-2 hover:bg-selected/60 hover:text-ink"}`
              }
            >
              <Icon size={16} stroke={1.75} className="text-ink-3" />
              {label}
            </NavLink>
          ))}
        </div>
      )}
      <button
        ref={button}
        type="button"
        aria-expanded={open}
        aria-controls="settings-screens"
        onClick={() => setOpen((o) => !o)}
        className={`flex w-full items-center gap-2 rounded-md px-2 py-[7px] hover:bg-selected/60 ${open || here ? "text-ink" : "text-ink-2 hover:text-ink"} ${open ? "bg-selected/60" : ""}`}
      >
        <IconSettings size={16} stroke={1.75} className="text-ink-3" />
        <span className="grow text-left">Settings</span>
        <IconChevronUp size={14} className={`text-ink-3 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
    </div>
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
            <span className="line-clamp-2 text-xs leading-snug text-ink-2">{m.now ?? "Idle"}</span>
          </span>
        </Link>
      ))}
    </>
  );
}
