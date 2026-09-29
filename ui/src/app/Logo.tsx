/**
 * The Quantix mark, Weave: an X whose second stroke passes under the first (brand/logo). Mono draws it in the
 * text colour; brand draws it in its colours for a light surface, a copper ribbon with an ink accent piece.
 */
export function Logo({ tone = "mono", className }: { tone?: "mono" | "brand"; className?: string }) {
  const ribbon = tone === "brand" ? "#E0703A" : "currentColor";
  const accent = tone === "brand" ? "#1F2328" : "currentColor";
  return (
    <svg viewBox="17.27 100 722.46 557" className={className} aria-hidden="true" focusable="false">
      <path d="M17.27 100L574.27 657L739.73 657L182.73 100Z" fill={ribbon} />
      <path d="M182.73 657L360.12 479.62L277.38 396.88L17.27 657Z" fill={ribbon} />
      <path d="M479.62 360.12L739.73 100L574.27 100L396.88 277.38Z" fill={accent} />
    </svg>
  );
}
