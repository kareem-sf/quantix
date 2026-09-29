import { useEscape } from "./keys";
import { TENDER_SCREENS } from "./screens";

const GROUPS: [group: string, keys: [keys: string, what: string][]][] = [
  [
    "Anywhere",
    [
      ["Ctrl K", "Search or jump to a tender, a screen or a person"],
      ["Ctrl J", "Open or close the team"],
      ["Ctrl B", "Fold or unfold the sidebar"],
      ...TENDER_SCREENS.map(([label], n): [string, string] => [`Ctrl ${n + 1}`, label]),
      ["Alt ←  Alt →", "Back and forward"],
      ["Esc", "Close a panel, a menu or this list"],
      ["Ctrl /", "These shortcuts"],
    ],
  ],
  [
    "Search (Ctrl K)",
    [
      ["↑ ↓", "Move through the results"],
      ["Enter", "Open the one chosen"],
    ],
  ],
  ["Documents", [["Ctrl + wheel", "Zoom the page"]]],
  [
    "Takeoff",
    [
      ["Shift + click", "Choose more objects"],
      ["Shift + drag", "Choose what a box holds"],
      ["Enter", "Finish the measurement"],
      ["Backspace", "Take the last point back"],
      ["Esc", "Let go of the points, then the choice, then the tool"],
    ],
  ],
];

/** Ctrl+/: every keyboard shortcut Quantix has, in one list. */
export function Shortcuts({ onClose }: { onClose: () => void }) {
  useEscape(onClose);
  return (
    <div className="fixed inset-0 z-50 flex animate-fade justify-center bg-ink/15 px-4 pt-[10vh]" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-label="Keyboard shortcuts"
        onMouseDown={(e) => e.stopPropagation()}
        className="flex h-fit max-h-[80vh] w-[560px] max-w-full animate-pop flex-col gap-5 overflow-y-auto rounded-xl border border-line-strong bg-white p-5 shadow-[0_24px_64px_rgb(0_0_0/0.18)]"
      >
        <h2 className="text-[15px] font-semibold">Keyboard shortcuts</h2>
        {GROUPS.map(([group, keys]) => (
          <section key={group} aria-label={group} className="flex flex-col gap-1.5">
            <span className="text-xs font-medium text-ink-3">{group}</span>
            {keys.map(([k, what]) => (
              <div key={k + what} className="flex items-center justify-between gap-4">
                <span className="text-ink-2">{what}</span>
                <kbd className="shrink-0 rounded border border-line-strong bg-rail px-1.5 font-sans text-xs whitespace-nowrap text-ink-2">
                  {k}
                </kbd>
              </div>
            ))}
          </section>
        ))}
        <p className="text-xs text-ink-3">
          The shortcuts go by the key's place, so they work with an Arabic keyboard too.
        </p>
      </div>
    </div>
  );
}
