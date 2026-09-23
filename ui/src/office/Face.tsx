/** An illustrated portrait drawn from the person's id, so it never changes and needs nothing from the AI. */

const HAIR: [back: string, front: string][] = [
  ["", "M12.5 17 C12.5 11 15.5 9.5 20 9.5 C24.5 9.5 27.5 11 27.5 17 C25.5 14 23 13.3 20 13.3 C17 13.3 14.5 14 12.5 17 Z"],
  [
    "M11 18 C11 10 15 7.5 20 7.5 C25 7.5 29 10 29 18 L30 31 L10 31 Z",
    "M12.5 16.5 C13 11 16 9.5 20 9.5 C24 9.5 27 11 27.5 16.5 C24 13 17 13 12.5 16.5 Z",
  ],
  ["", "M13 15.5 C13 11.5 16 10.3 20 10.3 C24 10.3 27 11.5 27 15.5 C24.5 13.2 15.5 13.2 13 15.5 Z"],
  [
    "M16.5 7.5 a3.5 3.5 0 1 0 7 0 a3.5 3.5 0 1 0 -7 0 Z",
    "M12.5 16.5 C12.5 11 16 9.8 20 9.8 C24 9.8 27.5 11 27.5 16.5 C25 13.5 15 13.5 12.5 16.5 Z",
  ],
  ["M10 21 C10 11 14.5 7.5 20 7.5 C25.5 7.5 30 11 30 21 L32 34 L8 34 Z", ""],
];
const SKIN = ["#E0AC8A", "#C98E6B", "#B97A56", "#9C6644", "#6B4430", "#F1C9A5"];
const HAIR_COLOUR = ["#231A15", "#1B1412", "#5A3825", "#3B2A4A", "#16100C", "#7A5A3A"];
const TOP = ["#5B6F82", "#8B3A3A", "#2F5D50", "#C27C3A", "#3B2A4A", "#4A5568"];
const GROUND = ["#E4ECE9", "#F2EBE1", "#ECE8F3", "#E6ECF3", "#F3E5E5", "#EEF0E4"];

function hash(text: string) {
  let h = 2166136261;
  for (const c of text) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return h >>> 0;
}

export function Face({ id, size = 24 }: { id: string; size?: number }) {
  const h = hash(id);
  const pick = <T,>(list: T[], shift: number) => list[(h >>> shift) % list.length];
  const [back, front] = pick(HAIR, 0);
  const hair = pick(HAIR_COLOUR, 5);
  return (
    <svg width={size} height={size} viewBox="0 0 40 40" aria-hidden="true" className="shrink-0 rounded-full">
      <rect width="40" height="40" fill={pick(GROUND, 9)} />
      {back && <path d={back} fill={hair} />}
      <path d="M7 40 C7 31 13 28 20 28 C27 28 33 31 33 40 Z" fill={pick(TOP, 13)} />
      <circle cx="20" cy="18" r="7.5" fill={pick(SKIN, 17)} />
      {front && <path d={front} fill={hair} />}
      <circle cx="17.3" cy="18.4" r="0.9" fill="#2A211C" />
      <circle cx="22.7" cy="18.4" r="0.9" fill="#2A211C" />
    </svg>
  );
}
