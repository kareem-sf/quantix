import { Link, useParams } from "react-router";
import { AddDocuments } from "../documents/AddDocuments";
import { useDocuments } from "../documents/queries";
import { dueSentence } from "./due";
import { useTender } from "./queries";

export function Overview() {
  const { tenderId = "" } = useParams();
  const tender = useTender(tenderId);
  const documents = useDocuments(tenderId);

  if (tender.isError) return <p className="pt-14 text-ink-2">{tender.error.message}</p>;
  if (!tender.data || !documents.data) return null;

  const current = documents.data.filter((d) => d.status !== "replaced");
  const read = current.filter((d) => d.status === "read").length;
  const reading = current.filter((d) => d.status === "waiting" || d.status === "reading").length;
  const problems = current.length - read - reading;

  return (
    <div className="flex w-[720px] flex-col pt-14">
      <h1 className="text-[28px] font-semibold tracking-tight">{tender.data.name}</h1>
      <p className="mt-1 text-sm text-ink-2">{dueSentence(tender.data.due_date, new Date())}</p>

      {current.length === 0 ? (
        <div className="mt-9 flex flex-col gap-3 border-t border-line pt-5">
          <h2 className="text-sm font-medium">Add the tender package</h2>
          <p className="text-ink-2">Choose the folder the client sent. Your original files are never changed.</p>
          <AddDocuments tenderId={tenderId} primary />
        </div>
      ) : (
        <div className="mt-9 flex flex-col border-t border-line pt-3">
          <h2 className="pb-2 font-semibold text-ink-2">Where things stand</h2>
          <Link
            to={`/tenders/${tenderId}/documents`}
            className="grid grid-cols-[140px_minmax(0,1fr)_160px] items-center gap-4 px-1 py-2"
          >
            <span className="font-medium">Documents</span>
            <span className="text-ink-2">
              {read} of {current.length} read
              {reading > 0 && ` · reading ${reading}`}
              {problems > 0 && ` · ${problems} can’t be read`}
            </span>
            <span className="relative h-1 overflow-hidden rounded-sm bg-selected">
              <span
                className="absolute inset-y-0 left-0 rounded-sm bg-ink"
                style={{ width: `${Math.round(((read + problems) / current.length) * 100)}%` }}
              />
            </span>
          </Link>
        </div>
      )}
    </div>
  );
}
