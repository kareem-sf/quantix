import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { Face } from "./Face";
import { firstName, useAnswer, useDecisions, useOffice } from "./queries";

export function DecisionPage() {
  const { tenderId = "", decisionId = "" } = useParams();
  const navigate = useNavigate();
  const decisions = useDecisions(tenderId);
  const office = useOffice(tenderId);
  const answer = useAnswer(tenderId);
  const [choice, setChoice] = useState("");
  const [own, setOwn] = useState("");

  const waiting = (decisions.data ?? []).filter((d) => d.status === "waiting");
  const decision = decisions.data?.find((d) => d.id === decisionId);
  if (!decision) return decisions.data ? <p className="pt-14 text-ink-2">This decision can’t be found.</p> : null;
  const asker = office.data?.staff.find((m) => m.id === decision.raised_by);
  const position = waiting.findIndex((d) => d.id === decision.id);
  const next = waiting.find((d) => d.id !== decision.id);
  const reply = own.trim() || choice;

  function confirm() {
    answer.mutate(
      { id: decision!.id, answer: reply },
      { onSuccess: () => navigate(next ? `/tenders/${tenderId}/decisions/${next.id}` : `/tenders/${tenderId}`) },
    );
  }

  return (
    <div className="flex w-[640px] flex-col gap-6 pt-12">
      <div className="flex items-center justify-between text-ink-2">
        <Link to={`/tenders/${tenderId}`} className="hover:text-ink">
          ‹ Overview
        </Link>
        {position >= 0 && (
          <span>
            Decision {position + 1} of {waiting.length}
          </span>
        )}
      </div>
      <h1 className="text-[26px] leading-tight font-semibold tracking-tight">{decision.title}</h1>
      <div className="flex gap-2.5 leading-relaxed">
        {asker && <Face id={asker.id} size={24} />}
        <span className="flex flex-col gap-0.5">
          {asker && (
            <span className="font-medium">
              {firstName(asker)} <span className="font-normal text-ink-3">{asker.role}</span>
            </span>
          )}
          <span className="text-[15px] text-[#27272A]" dir="auto">
            {decision.text}
          </span>
        </span>
      </div>

      {decision.status === "waiting" ? (
        <>
          <fieldset className="flex flex-col gap-2">
            <legend className="pb-2 font-semibold text-ink-2">Your decision</legend>
            {decision.options.map((option, i) => (
              <label
                key={option}
                className={`flex cursor-pointer items-start gap-3 rounded-[10px] border px-3.5 py-3 ${choice === option && !own ? "border-ink bg-rail" : "border-line"}`}
              >
                <input
                  type="radio"
                  name="decision"
                  checked={choice === option && !own}
                  onChange={() => setChoice(option)}
                  className="mt-0.5 accent-ink"
                />
                <span className="grow text-sm font-medium">{option}</span>
                <span className="text-xs text-ink-4">{i + 1}</span>
              </label>
            ))}
            <textarea
              aria-label="Or answer in your own words"
              value={own}
              onChange={(e) => setOwn(e.target.value)}
              placeholder="Or answer in your own words"
              rows={2}
              className="rounded-[10px] border border-line px-3.5 py-3 text-sm outline-none focus:border-ink"
            />
          </fieldset>
          {answer.isError && <p className="text-attention">{answer.error.message}</p>}
          <button
            disabled={!reply || answer.isPending}
            onClick={confirm}
            className="h-10 self-start rounded-lg bg-ink px-5 text-sm text-white disabled:bg-line-strong disabled:text-ink-3"
          >
            Confirm
          </button>
        </>
      ) : (
        <p className="border-t border-line pt-4 text-ink-2">
          You answered: <span className="text-ink">{decision.answer}</span>
        </p>
      )}
    </div>
  );
}
