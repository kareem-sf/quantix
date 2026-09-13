import type { Schema } from "../api";

type NamedTender = Pick<Schema<"Tender">, "name" | "name_source">;

/** While the package is still being analysed the tender has no real name yet. */
export function tenderDisplayName(tender: NamedTender) {
  return tender.name_source === "pending"
    ? "Analyzing tender package…"
    : tender.name;
}
