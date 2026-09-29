import {
  IconKeyboard,
  IconMessageCircle,
  IconPlayerStopFilled,
  IconPlus,
  IconSearch,
  IconSettings,
  type Icon,
} from "@tabler/icons-react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import type { Tender } from "../api/client";
import { firstName, useOffice, useStop } from "../office/queries";
import { dueShort } from "../tenders/due";
import { useTenders } from "../tenders/queries";
import { placeIn } from "./place";
import { COMPANY_SCREENS, TENDER_SCREENS } from "./screens";
import { useShell } from "./context";

type Entry = { group: string; label: string; hint?: string; icon: Icon; run: () => void };

/** Ctrl+K: type to jump to a tender or a screen, message someone on the team, or act. Arrows move, Enter opens,
 * Esc closes. */
export function Palette({ tender, onClose }: { tender?: Tender; onClose: () => void }) {
  const navigate = useNavigate();
  const shell = useShell();
  const tenders = useTenders();
  const office = useOffice(tender?.id ?? "");
  const stop = useStop(tender?.id ?? "");
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  const go = (path: string) => () => navigate(path);
  const entries: Entry[] = [
    ...(tender
      ? TENDER_SCREENS.map(([label, path, icon], n) => ({
          group: tender.name,
          label,
          hint: `Ctrl ${n + 1}`,
          icon,
          run: go(`/tenders/${tender.id}${path}`),
        }))
      : []),
    ...(tender
      ? (office.data?.staff ?? [])
          .filter((m) => m.status === "active")
          .map((m) => ({
            group: "Team",
            label: `Message ${firstName(m)}`,
            hint: m.is_manager ? "Tender Manager" : m.role,
            icon: IconMessageCircle,
            run: () => shell.showTeam(m.id),
          }))
      : []),
    ...(tenders.data ?? [])
      .filter((t) => t.id !== tender?.id)
      .map((t) => ({ group: "Tenders", label: t.name, hint: dueShort(t.due_date), icon: IconSearch, run: go(placeIn(t.id)) })),
    ...COMPANY_SCREENS.map(([label, path, icon]) => ({ group: "Company", label, icon, run: go(path) })),
    { group: "Actions", label: "New tender", icon: IconPlus, run: go("/new") },
    ...(tender && office.data?.state === "working"
      ? [{ group: "Actions", label: "Stop the office", icon: IconPlayerStopFilled, run: () => stop.mutate() }]
      : []),
    { group: "Actions", label: "Settings", icon: IconSettings, run: go("/settings") },
    { group: "Actions", label: "Keyboard shortcuts", hint: "Ctrl /", icon: IconKeyboard, run: shell.openShortcuts },
  ];
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const shown = entries.filter((e) => words.every((w) => `${e.label} ${e.hint ?? ""} ${e.group}`.toLowerCase().includes(w)));
  const current = Math.min(at, Math.max(shown.length - 1, 0));

  function open(entry: Entry | undefined) {
    if (!entry) return;
    onClose();
    entry.run();
  }

  return (
    <div className="fixed inset-0 z-50 flex animate-fade justify-center bg-ink/15 px-4 pt-[12vh]" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-label="Search or jump to"
        onMouseDown={(e) => e.stopPropagation()}
        className="flex h-fit max-h-[70vh] w-[580px] max-w-full animate-pop flex-col overflow-hidden rounded-xl border border-line-strong bg-white shadow-[0_24px_64px_rgb(0_0_0/0.18)]"
      >
        <div className="flex items-center gap-2.5 border-b border-line px-4">
          <IconSearch className="size-4 text-ink-3" stroke={1.75} />
          <input
            ref={input}
            aria-label="Search or jump to"
            value={query}
            placeholder="Jump to a tender or screen, message someone, or act"
            onChange={(e) => {
              setQuery(e.target.value);
              setAt(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                setAt(Math.max(0, Math.min(shown.length - 1, current + (e.key === "ArrowDown" ? 1 : -1))));
              } else if (e.key === "Enter") {
                e.preventDefault();
                open(shown[current]);
              } else if (e.key === "Escape") {
                onClose();
              }
            }}
            className="h-12 grow bg-transparent text-sm outline-none"
          />
        </div>
        <div role="listbox" aria-label="Results" className="overflow-y-auto p-1.5">
          {shown.length === 0 && <p className="px-3 py-3 text-ink-3">Nothing matches “{query}”.</p>}
          {shown.map((entry, n) => (
            <div key={`${entry.group}-${entry.label}`}>
              {entry.group !== shown[n - 1]?.group && (
                <div className="truncate px-2.5 pt-2.5 pb-1 text-xs text-ink-3">{entry.group}</div>
              )}
              <button
                role="option"
                aria-selected={n === current}
                onMouseEnter={() => setAt(n)}
                onClick={() => open(entry)}
                className={`flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left ${n === current ? "bg-selected" : ""}`}
              >
                <entry.icon className="size-4 shrink-0 text-ink-3" stroke={1.75} />
                <span className="grow truncate">{entry.label}</span>
                {entry.hint && <span className="shrink-0 text-xs text-ink-3">{entry.hint}</span>}
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
