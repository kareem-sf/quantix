import { Link, useParams } from "react-router";
import type { components } from "../api/schema";

type Source = components["schemas"]["DecisionSource"];

/** What an answer or a decision rests on, each opening the document page or BOQ line it names. */
export function Sources({ sources }: { sources: Source[] }) {
  const { tenderId = "" } = useParams();
  return sources.map((s) => {
    const to = s.document_id
      ? `/tenders/${tenderId}/documents?doc=${s.document_id}&page=${s.page}`
      : s.boq_item_id && `/tenders/${tenderId}/estimate?item=${s.boq_item_id}`;
    return to ? (
      <Link key={s.label} to={to} className="self-start underline underline-offset-4">
        {s.label}
      </Link>
    ) : (
      <span key={s.label}>{s.label}</span>
    );
  });
}
