/** Where the engineer was: the last tender opened and each tender's last screen, so Quantix reopens there and
 * switching tender goes back to where they left it. Kept in this window's storage; losing it only means starting
 * from the Overview. */
const KEY = "quantix.place";

type Place = { last?: string; screens: Record<string, string> };

function read(): Place {
  try {
    const place = JSON.parse(localStorage.getItem(KEY) ?? "null");
    return place?.screens ? place : { screens: {} };
  } catch {
    return { screens: {} };
  }
}

export function remember(path: string) {
  const tenderId = /^\/tenders\/([^/?]+)/.exec(path)?.[1];
  if (!tenderId || path.includes("/decisions/")) return; // a decision page is answered once, not returned to
  const place = read();
  place.last = tenderId;
  place.screens[tenderId] = path;
  try {
    localStorage.setItem(KEY, JSON.stringify(place));
  } catch {
    // storage is full or switched off: Quantix still works, it just won't reopen here
  }
}

export function lastTender(): string | undefined {
  return read().last;
}

/** The screen to open for a tender: where the engineer last was on it, else its Overview. */
export function placeIn(tenderId: string): string {
  return read().screens[tenderId] ?? `/tenders/${tenderId}`;
}

/** A setting of this window, such as whether the sidebar is folded. */
export function stored<T>(key: string, fallback: T): T {
  try {
    const value = localStorage.getItem(`quantix.${key}`);
    return value === null ? fallback : (JSON.parse(value) as T);
  } catch {
    return fallback;
  }
}

export function store(key: string, value: unknown) {
  try {
    localStorage.setItem(`quantix.${key}`, JSON.stringify(value));
  } catch {
    // not kept: the window opens with the default next time
  }
}
