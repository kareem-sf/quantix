import {
  IconBell,
  IconChevronLeft,
  IconChevronRight,
  IconCopy,
  IconLayoutSidebar,
  IconMinus,
  IconSearch,
  IconSquare,
  IconUsers,
  IconX,
} from "@tabler/icons-react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router";
import { firstName, useOffice } from "../office/queries";
import { useNeedsYou } from "../tenders/needsYou";
import { useDesk } from "../tenders/queries";
import type { Tender } from "../api/client";
import { screenOf } from "./screens";
import { useShell } from "./context";

/** Quantix's own title bar: back and forward, where the engineer is, search, what the team is doing, what needs
 * them and the team panel. Drag it to move the window; double-click it to maximise. */
export function TitleBar({ tender }: { tender?: Tender }) {
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const shell = useShell();
  const screen = screenOf(pathname);
  const inTender = pathname.startsWith("/tenders/");

  return (
    <header
      data-tauri-drag-region
      className="relative flex h-[38px] shrink-0 items-center gap-0.5 border-b border-line bg-rail pl-2 select-none"
    >
      <Tool label="Sidebar (Ctrl+B)" onClick={shell.toggleSidebar}>
        <IconLayoutSidebar className="size-4" stroke={1.75} />
      </Tool>
      <Tool label="Back (Alt+Left)" onClick={() => navigate(-1)}>
        <IconChevronLeft className="size-4" stroke={1.75} />
      </Tool>
      <Tool label="Forward (Alt+Right)" onClick={() => navigate(1)}>
        <IconChevronRight className="size-4" stroke={1.75} />
      </Tool>
      <span data-tauri-drag-region className="ml-2 flex min-w-0 max-w-[34%] items-center gap-1.5 text-ink-3">
        {inTender && tender && (
          <>
            <span data-tauri-drag-region className="truncate font-medium text-ink">
              {tender.name}
            </span>
            <IconChevronRight data-tauri-drag-region className="size-3.5 shrink-0" stroke={1.75} />
          </>
        )}
        <span data-tauri-drag-region className={`shrink-0 ${inTender && tender ? "" : "font-medium text-ink"}`}>
          {screen}
        </span>
      </span>
      <button
        onClick={shell.openPalette}
        className="absolute left-1/2 flex h-[26px] w-[min(340px,28vw)] -translate-x-1/2 items-center gap-2 rounded-md border border-line-strong bg-white px-2.5 text-ink-4 hover:border-ink-4"
      >
        <IconSearch className="size-3.5 shrink-0" stroke={1.75} />
        <span className="grow truncate text-left">Search or jump to…</span>
        <kbd className="shrink-0 rounded border border-line-strong px-1 font-sans text-[11px] text-ink-3">Ctrl K</kbd>
      </button>
      <span data-tauri-drag-region className="grow" />
      {tender && <Status tenderId={tender.id} />}
      <NeedsYou />
      {tender && <TeamButton tenderId={tender.id} />}
      {isTauri() ? <WindowButtons /> : <span className="w-2" />}
    </header>
  );
}

function Tool(props: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <button
      aria-label={props.label}
      title={props.label}
      onClick={props.onClick}
      className="flex size-7 items-center justify-center rounded-md text-ink-3 hover:bg-selected hover:text-ink"
    >
      {props.children}
    </button>
  );
}

/** What the team is doing, in one line; opens the team panel. */
function Status({ tenderId }: { tenderId: string }) {
  const office = useOffice(tenderId);
  const shell = useShell();
  if (!office.data) return null;
  const manager = office.data.staff.find((m) => m.is_manager);
  const busy = office.data.staff.find((m) => m.status === "active" && m.now);
  const working = office.data.state === "working";
  const text =
    office.data.state === "paused"
      ? "The team is stopped"
      : working && busy
        ? `${firstName(busy)}: ${busy.now}`
        : working
          ? "The team is working"
          : manager
            ? "The team is idle"
            : "No team yet";
  return (
    <button
      onClick={() => shell.showTeam()}
      title="What the team is doing"
      className="flex h-7 max-w-[260px] items-center gap-2 rounded-md px-2.5 text-ink-2 hover:bg-selected hover:text-ink"
    >
      <span
        className={`size-[7px] shrink-0 rounded-full ${working ? "bg-approved motion-safe:animate-pulse" : office.data.state === "paused" ? "bg-attention" : "bg-ink-4"}`}
      />
      <span className="truncate">{text}</span>
    </button>
  );
}

/** How many decisions wait for the engineer on every open tender; opens the Desk, where they are listed. */
function NeedsYou() {
  const desk = useDesk();
  const count = (desk.data ?? []).filter((t) => t.outcome === "open" && !t.archived).reduce((n, t) => n + t.waiting, 0);
  const label = count ? `${count} ${count === 1 ? "decision needs" : "decisions need"} you` : "Nothing needs you";
  return (
    <Link
      to="/desk"
      draggable={false}
      aria-label={label}
      title={label}
      className="relative flex size-7 items-center justify-center rounded-md text-ink-3 hover:bg-selected hover:text-ink"
    >
      <IconBell className="size-4" stroke={1.75} />
      {count > 0 && (
        <span className="absolute -top-0.5 -right-0.5 min-w-[16px] rounded-full bg-attention px-1 text-center text-[10px] leading-4 font-semibold text-white">
          {count}
        </span>
      )}
    </Link>
  );
}

function TeamButton({ tenderId }: { tenderId: string }) {
  const shell = useShell();
  const { questions } = useNeedsYou(tenderId);
  return (
    <button
      onClick={shell.toggleTeam}
      aria-pressed={shell.team.open}
      title="Team (Ctrl+J)"
      className={`relative ml-0.5 flex h-7 items-center gap-1.5 rounded-md px-2.5 ${shell.team.open ? "bg-selected text-ink" : "text-ink-2 hover:bg-selected hover:text-ink"}`}
    >
      <IconUsers className="size-4" stroke={1.75} />
      Team
      {questions.length > 0 && <span aria-label="A question waits for you" className="size-[7px] rounded-full bg-attention" />}
    </button>
  );
}

/** Minimise, maximise and close, drawn by Quantix since the window has no Windows frame. */
function WindowButtons() {
  const [maximised, setMaximised] = useState(false);
  useEffect(() => {
    const window = getCurrentWindow();
    const check = () => void window.isMaximized().then(setMaximised);
    check();
    const stop = window.onResized(check);
    return () => void stop.then((unlisten) => unlisten());
  }, []);
  const button = "flex h-[38px] w-[46px] items-center justify-center text-ink-2";
  return (
    <span className="ml-2 flex">
      <button aria-label="Minimise" onClick={() => void getCurrentWindow().minimize()} className={`${button} hover:bg-selected`}>
        <IconMinus className="size-4" stroke={1.5} />
      </button>
      <button
        aria-label={maximised ? "Restore" : "Maximise"}
        onClick={() => void getCurrentWindow().toggleMaximize()}
        className={`${button} hover:bg-selected`}
      >
        {maximised ? <IconCopy className="size-3.5" stroke={1.5} /> : <IconSquare className="size-3.5" stroke={1.5} />}
      </button>
      <button aria-label="Close" onClick={() => void getCurrentWindow().close()} className={`${button} hover:bg-[#c42b1c] hover:text-white`}>
        <IconX className="size-4" stroke={1.5} />
      </button>
    </span>
  );
}
