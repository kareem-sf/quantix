import { useParams } from "react-router";
import { dueSentence } from "./due";
import { useTender } from "./queries";

export function Overview() {
  const { tenderId = "" } = useParams();
  const tender = useTender(tenderId);

  if (tender.isError) return <p className="pt-14 text-ink-2">{tender.error.message}</p>;
  if (!tender.data) return null;

  return (
    <div className="flex w-[720px] flex-col pt-14">
      <h1 className="text-[28px] font-semibold tracking-tight">{tender.data.name}</h1>
      <p className="mt-1 text-sm text-ink-2">{dueSentence(tender.data.due_date, new Date())}</p>
      <p className="mt-9 border-t border-line pt-4 text-ink-2">
        The office starts work once the tender package is added.
      </p>
    </div>
  );
}
