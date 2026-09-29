import { IconHome, IconListDetails, IconPlus, IconSelector, IconSettings, type Icon } from "@tabler/icons-react";
import { useEffect, useRef, useState } from "react";
import { Link, NavLink, useNavigate } from "react-router";
import type { Tender } from "../api/client";
import { dueShort, dueSource } from "../tenders/due";
import { useNeedsYou } from "../tenders/needsYou";
import { useSuggestedLessons } from "../review/queries";
import { useDesk, useTenders } from "../tenders/queries";
import { placeIn } from "./place";
import { COMPANY_SCREENS, TENDER_SCREENS } from "./screens";
import { useShell } from "./context";

/** The sidebar: the open tender and its screens, then what the firm keeps across tenders, then Settings. Only one
 * tender at a time; the others are a click away in the switcher. Ctrl+B folds it to icons. */
export function Rail({ tender }: { tender?: Tender }) {
  const { folded } = useShell();
  const tenders = useTenders();

  return (
    <nav
      aria-label="Quantix"
      className={`relative z-20 flex shrink-0 flex-col border-r border-line bg-rail px-2 py-2.5 select-none ${folded ? "w-[52px]" : "w-56"}`}
    >
      {tenders.isError && !folded && <span className="px-2 py-1 text-ink-2">Waiting for the Quantix service…</span>}
      <Office />
      {tender && <Switcher tender={tender} tenders={tenders.data ?? []} />}
      <div className="-mx-2 flex min-h-0 grow flex-col gap-0.5 overflow-x-hidden overflow-y-auto px-2">
        {tender ? <Screens tenderId={tender.id} /> : tenders.data && <Item to="/new" label="Start a tender" icon={IconPlus} />}
        {!folded && <span className="px-2 pt-5 pb-1 text-xs text-ink-3">Company</span>}
        {folded && <span className="my-2 border-t border-line" />}
        <Company />
        <span className="grow" />
        <Item to="/settings" label="Settings" icon={IconSettings} />
      </div>
    </nav>
  );
}

function Item(props: {
  to: string;
  label: string;
  icon: Icon;
  end?: boolean;
  count?: number;
  countLabel?: string;
  quiet?: boolean; // a count to look at some time, not one that waits for the engineer
}) {
  const { folded } = useShell();
  const { icon: Glyph, count = 0 } = props;
  return (
    <NavLink
      to={props.to}
      end={props.end}
      draggable={false}
      title={folded ? props.label : undefined}
      aria-label={folded ? props.label : undefined}
      className={({ isActive }) =>
        `relative flex h-[30px] shrink-0 items-center gap-2.5 rounded-md ${folded ? "justify-center" : "px-2"} ${isActive ? "bg-white font-semibold text-ink shadow-[0_0_0_1px_var(--color-line-strong)]" : "text-ink-2 hover:bg-selected hover:text-ink"}`
      }
    >
      <Glyph className="size-4 shrink-0 text-ink-3" stroke={1.75} />
      {!folded && <span className="grow truncate">{props.label}</span>}
      {count > 0 &&
        (folded ? (
          <span
            aria-label={props.countLabel}
            className={`absolute top-1 right-1.5 size-[7px] rounded-full ${props.quiet ? "bg-ink-4" : "bg-attention"}`}
          />
        ) : (
          <span
            aria-label={props.countLabel}
            className={`min-w-[18px] rounded-full px-1.5 text-center text-[11px] leading-[18px] font-medium ${props.quiet ? "bg-selected text-ink-2" : "bg-attention text-white"}`}
          >
            {count}
          </span>
        ))}
    </NavLink>
  );
}

/** The engineer's own screens: the Desk, counting what needs them on every open tender, and the register. */
function Office() {
  const desk = useDesk();
  const waiting = (desk.data ?? []).filter((t) => t.outcome === "open" && !t.archived).reduce((n, t) => n + t.waiting, 0);
  return (
    <div className="mb-2 flex flex-col gap-0.5">
      <Item
        to="/desk"
        label="Desk"
        icon={IconHome}
        count={waiting}
        countLabel={`${waiting} ${waiting === 1 ? "decision needs" : "decisions need"} you across your tenders`}
      />
      <Item to="/tenders" label="Tenders" icon={IconListDetails} end />
    </div>
  );
}

/** What the firm keeps across tenders; Company rules counts the rules the office suggests. */
function Company() {
  const suggested = useSuggestedLessons().data?.length ?? 0;
  return COMPANY_SCREENS.map(([label, path, icon]) => (
    <Item
      key={path}
      to={path}
      label={label}
      icon={icon}
      quiet
      count={path === "/rules" ? suggested : 0}
      countLabel={`${suggested} suggested ${suggested === 1 ? "rule" : "rules"}`}
    />
  ));
}

/** The open tender's screens, each counting what waits for the engineer there. The Overview counts everything. */
function Screens({ tenderId }: { tenderId: string }) {
  const { approvals, questions } = useNeedsYou(tenderId);
  const total = approvals.length + questions.length;
  return TENDER_SCREENS.map(([label, path, icon]) => {
    const here = path
      ? approvals.filter((a) => a.to.split("?")[0] === `/tenders/${tenderId}${path}`).length
      : total;
    return (
      <Item
        key={label}
        to={`/tenders/${tenderId}${path}`}
        end
        label={label}
        icon={icon}
        count={here}
        countLabel={`${here} ${here === 1 ? "decision needs" : "decisions need"} you${path ? ` in ${label}` : ""}`}
      />
    );
  });
}

/** The open tender, and a list to switch to another, soonest due first. Switching opens the screen the engineer
 * was last on in that tender. */
function Switcher({ tender, tenders }: { tender: Tender; tenders: Tender[] }) {
  const { folded } = useShell();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const due = (t: Tender) => (t.due_date ? Date.parse(t.due_date) : Infinity);
  const sorted = tenders.filter((t) => !t.archived || t.id === tender.id).sort((a, b) => due(a) - due(b) || Date.parse(b.created_at) - Date.parse(a.created_at));

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: MouseEvent) => !box.current?.contains(e.target as Node) && setOpen(false);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={box} className="relative mb-2">
      <button
        aria-expanded={open}
        aria-label={`${tender.name}, ${dueShort(tender.due_date)}. Switch tender`}
        title={folded ? tender.name : undefined}
        onClick={() => setOpen(!open)}
        className={`flex w-full items-center gap-2 rounded-lg border border-line-strong bg-white text-left hover:border-ink-4 ${folded ? "h-9 justify-center" : "px-2.5 py-2"}`}
      >
        <Initials name={tender.name} />
        {!folded && (
          <>
            <span className="flex min-w-0 grow flex-col">
              <span className="line-clamp-2 leading-snug font-semibold">{tender.name}</span>
              <span className="text-xs text-ink-3" title={dueSource(tender.due_date_source)?.title}>
                {dueShort(tender.due_date)}
              </span>
            </span>
            <IconSelector className="size-4 shrink-0 text-ink-3" stroke={1.75} />
          </>
        )}
      </button>
      {open && (
        <div
          role="menu"
          aria-label="Tenders"
          className="absolute top-full left-0 z-30 mt-1 flex w-72 flex-col rounded-lg border border-line-strong bg-white p-1 shadow-[0_12px_32px_rgb(0_0_0/0.12)]"
        >
          {sorted.map((t) => (
            <button
              key={t.id}
              role="menuitem"
              onClick={() => {
                setOpen(false);
                navigate(placeIn(t.id));
              }}
              className={`flex items-center gap-2.5 rounded-md px-2 py-1.5 text-left hover:bg-selected ${t.id === tender.id ? "bg-selected/70" : ""}`}
            >
              <Initials name={t.name} />
              <span className="flex min-w-0 grow flex-col">
                <span className="truncate font-medium">{t.name}</span>
                <span className="text-xs text-ink-3">{dueShort(t.due_date)}</span>
              </span>
              <Waiting tenderId={t.id} />
            </button>
          ))}
          <Link
            to="/new"
            draggable={false}
            onClick={() => setOpen(false)}
            className="mt-1 flex items-center gap-2 border-t border-line px-2 pt-2 pb-1.5 text-ink-2 hover:text-ink"
          >
            <IconPlus className="size-4" stroke={1.75} />
            New tender
          </Link>
        </div>
      )}
    </div>
  );
}

function Waiting({ tenderId }: { tenderId: string }) {
  const { approvals, questions } = useNeedsYou(tenderId);
  const count = approvals.length + questions.length;
  if (!count) return null;
  return (
    <span className="min-w-[18px] rounded-full bg-attention px-1.5 text-center text-[11px] leading-[18px] font-medium text-white">
      {count}
    </span>
  );
}

/** Two letters for a tender, so it can be told apart when the sidebar is folded. */
function Initials({ name }: { name: string }) {
  const letters = name
    .split(/[\s·&,-]+/)
    .filter((w) => /^[A-Za-z0-9]/.test(w))
    .slice(0, 2)
    .map((w) => w[0].toUpperCase())
    .join("");
  return (
    <span className="flex size-6 shrink-0 items-center justify-center rounded-md bg-selected text-[10.5px] font-semibold text-ink-2">
      {letters || "T"}
    </span>
  );
}
