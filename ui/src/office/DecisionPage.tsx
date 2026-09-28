import { IconChevronLeft } from "@tabler/icons-react";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { Findings } from "../review/Review";
import { isChecked } from "../review/queries";
import { Face } from "./Face";
import { Prose } from "./Prose";
import { firstName, useAnswer, useDecisions, useOffice } from "./queries";
import { Sources } from "./Sources";

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
  // an escalation about a record: the Manager puts his recommended correction first
  const escalated = isChecked(decision.subject_kind) && Boolean(decision.subject_id);

  function confirm() {
    answer.mutate(
      { id: decision!.id, answer: reply },
      { onSuccess: () => navigate(next ? `/tenders/${tenderId}/decisions/${next.id}` : `/tenders/${tenderId}`) },
    );
  }

  return (
    <div className="flex w-full max-w-[704px] flex-col gap-6 px-8 pt-12">
      <div className="flex items-center justify-between text-ink-2">
        <Link to={`/tenders/${tenderId}`} className="flex items-center gap-1 hover:text-ink">
          <IconChevronLeft className="size-4" stroke={1.75} />
          Overview
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
          <Prose text={decision.text} signer={asker ? firstName(asker) : undefined} className="text-[15px] text-[#27272A]" />
        </span>
      </div>

      {decision.sources && decision.sources.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <span className="font-semibold text-ink-2">Where it shows</span>
          <Sources sources={decision.sources} />
        </div>
      )}
      {isChecked(decision.subject_kind) && decision.subject_id && (
        <Findings kind={decision.subject_kind} id={decision.subject_id} tenderId={tenderId} />
      )}

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
                {i === 0 && escalated && <span className="text-xs font-medium text-ink-2">Recommended</span>}
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
