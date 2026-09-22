const DAY = 86_400_000;

function parse(isoDate: string): Date {
  const [y, m, d] = isoDate.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** "due 14 Oct", for the sidebar. */
export function dueShort(isoDate: string | null): string {
  if (!isoDate) return "no due date";
  return `due ${parse(isoDate).toLocaleDateString("en-GB", { day: "numeric", month: "short" })}`;
}

/** "Due 14 October, 21 days left", for the overview. */
export function dueSentence(isoDate: string | null, today: Date): string {
  if (!isoDate) return "No due date yet.";
  const due = parse(isoDate);
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const days = Math.round((due.getTime() - start.getTime()) / DAY);
  const label = due.toLocaleDateString("en-GB", { day: "numeric", month: "long" });
  if (days < 0) return `Was due ${label}.`;
  if (days === 0) return `Due today, ${label}.`;
  if (days === 1) return `Due tomorrow, ${label}.`;
  return `Due ${label}, ${days} days left.`;
}
