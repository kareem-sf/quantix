import type { TenderGlance } from "./queries";

/** The stages a tender goes through, as the Desk shows them, each done or not from Quantix's own counts. */
export const STAGES = ["Documents", "BOQ", "Pricing", "Quotes", "Submission"] as const;

export function stages(t: TenderGlance): boolean[] {
  return [
    t.documents > 0 && t.read === t.documents,
    t.items > 0,
    t.items > 0 && t.priced === t.items,
    t.packages === 0 ? t.items > 0 && t.priced === t.items : t.chosen === t.packages,
    t.requirements > 0 && t.ready === t.requirements,
  ];
}

/** Where the tender stands, in a few words: the first stage not done and how far it has got. */
export function standing(t: TenderGlance): string {
  if (t.documents === 0) return "No documents yet";
  const done = stages(t);
  const at = done.indexOf(false);
  if (at === -1) return "Ready to build";
  return [
    `Reading · ${t.read} of ${t.documents}`,
    "BOQ not entered",
    `Pricing · ${t.priced} of ${t.items}`,
    `Quotes · ${t.chosen} of ${t.packages} chosen`,
    t.requirements ? `Submission · ${t.ready} of ${t.requirements} ready` : "Submission · no checklist",
  ][at];
}

/** Days from today to a date, counted in whole days. */
export function daysTo(isoDate: string, today = new Date()): number {
  const [y, m, d] = isoDate.split("-").map(Number);
  const start = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  return Math.round((new Date(y, m - 1, d).getTime() - start.getTime()) / 86_400_000);
}
