import type { Schema } from "../../api";

type Item = Schema<"EstimateItem">;
type Line = Schema<"TakeoffLine">;
type RateProposal = Schema<"RateProposalRecord">;

export type RowStatus = "waiting" | "check" | "unpriced" | "priced";

/** One BOQ row with everything the team proposed for it. */
export type EstimateRow = {
  item: Item;
  /** Drawing takeoff lines measured for this row. */
  takeoff: Line[];
  /** Rate proposals still waiting for the engineer. */
  rates: RateProposal[];
  /** Decisions waiting for the engineer on this row. */
  waiting: number;
  status: RowStatus;
};

export const STATUS: Record<RowStatus, { label: string; dot: string }> = {
  waiting: { label: "Waiting for you", dot: "bg-amber-500" },
  check: { label: "Check the source", dot: "bg-sky-500" },
  unpriced: { label: "Not priced", dot: "bg-muted-foreground/50" },
  priced: { label: "Priced", dot: "bg-(--success)" },
};

export const COMPARISON: Record<Line["comparison"], string> = {
  matches: "matches the BOQ",
  differs: "differs from the BOQ",
  unit_differs: "unit differs",
  no_boq_quantity: "BOQ has no quantity",
  not_in_boq: "not in the BOQ",
  not_on_drawings: "not on the drawings",
};

export function takeoffNeedsDecision(line: Line) {
  return line.status === "proposed" && line.comparison !== "matches";
}

function list<T>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : [];
}

/** Join BOQ rows, drawing takeoff lines and rate proposals into one table. */
export function buildRows(
  items: Item[],
  takeoffLines: unknown,
  rateProposals: unknown,
): { rows: EstimateRow[]; unmatched: Line[] } {
  const lines = list<Line>(takeoffLines);
  const proposals = list<RateProposal>(rateProposals);
  const ids = new Set(items.map((item) => item.id));
  const rows = items.map((item) => {
    const takeoff = lines
      .filter((line) => line.boq_item_id === item.id)
      .sort(
        (a, b) =>
          Number(takeoffNeedsDecision(b)) - Number(takeoffNeedsDecision(a)),
      );
    const rates = proposals.filter(
      (proposal) =>
        proposal.item_id === item.id &&
        proposal.status === "proposed" &&
        proposal.is_current,
    );
    const waiting =
      item.quantity_proposals.filter(
        (proposal) => proposal.status === "proposed",
      ).length +
      takeoff.filter(takeoffNeedsDecision).length +
      rates.length;
    const status: RowStatus = waiting
      ? "waiting"
      : !item.confirmed
        ? "check"
        : item.unit_rate === null || item.effective_quantity === null
          ? "unpriced"
          : "priced";
    return { item, takeoff, rates, waiting, status };
  });
  // Work found on the drawings with no BOQ row to sit under.
  const unmatched = lines.filter(
    (line) => !line.boq_item_id || !ids.has(line.boq_item_id),
  );
  return { rows, unmatched };
}

/** Group thousands in a decimal string without changing its digits. */
export function formatNumber(value: string | null | undefined) {
  if (value == null || value === "") return null;
  const [whole, fraction] = value.split(".");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return fraction === undefined ? grouped : `${grouped}.${fraction}`;
}
