import { Link, useParams } from "react-router";
import type { components } from "../api/schema";

type Source = components["schemas"]["DecisionSource"];

/** What an answer or a decision rests on, each opening the document page, BOQ line or web page it names. */
export function Sources({ sources }: { sources: Source[] }) {
  const { tenderId = "" } = useParams();
  return sources.map((s) => {
    const to = s.document_id
      ? `/tenders/${tenderId}/documents?doc=${s.document_id}&page=${s.page}`
      : s.boq_item_id && `/tenders/${tenderId}/estimate?item=${s.boq_item_id}`;
    if (s.url) {
      return (
        <a key={s.label} href={s.url} target="_blank" rel="noreferrer" className="self-start underline underline-offset-4">
          {s.label}
        </a>
      );
    }
    return to ? (
      <Link key={s.label} to={to} className="self-start underline underline-offset-4">
        {s.label}
      </Link>
    ) : (
      <span key={s.label}>{s.label}</span>
    );
  });
}
