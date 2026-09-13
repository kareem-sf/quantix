/** Amicro Pulse Dots, adapted to inherit color and respect reduced motion. */
export function ActivityDots() {
  return (
    <span
      aria-hidden="true"
      className="quantix-activity-dots inline-flex shrink-0 items-center gap-1"
    >
      <span className="size-1 rounded-full bg-current" />
      <span className="size-1 rounded-full bg-current" />
      <span className="size-1 rounded-full bg-current" />
    </span>
  );
}
