import { IconArrowUp, IconChevronRight } from "@tabler/icons-react";
import { useState, type FormEvent } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { AddDocuments } from "../documents/AddDocuments";
import { useDocuments } from "../documents/queries";
import { money, useEstimate, useGates } from "../estimate/queries";
import { Face } from "../office/Face";
import { Prose, tidy } from "../office/Prose";
import { TEAM, firstName, useDecisions, useMessages, useOffice, useSend } from "../office/queries";
import { Opening } from "../app/Opening";
import { dueSentence } from "./due";
import { usePackages } from "../subcontract/queries";
import { useSubmission } from "../submission/queries";
import { useDeleteTender, useSetOutcome, useTender } from "./queries";

export function Overview() {
  const { tenderId = "" } = useParams();
  const tender = useTender(tenderId);
  const documents = useDocuments(tenderId);
  const office = useOffice(tenderId);
  const decisions = useDecisions(tenderId);
  const gates = useGates(tenderId);

  if (tender.isError) return <p className="pt-14 text-ink-2">{tender.error.message}</p>;
  if (!tender.data || !documents.data || !office.data) return <Opening error={documents.isError || office.isError} />;

  const questions = (decisions.data ?? []).filter((d) => d.status === "waiting");
  const approvals = [
    gates.data?.facts && {
      key: "facts",
      title: `${gates.data.facts} tender ${gates.data.facts === 1 ? "fact" : "facts"} to approve`,
      text: "Method of measurement, currency or VAT, as the office read them",
      to: `/tenders/${tenderId}/estimate`,
    },
    gates.data?.takeoff && {
      key: "takeoff",
      title: `${gates.data.takeoff} takeoff ${gates.data.takeoff === 1 ? "mark" : "marks"} to check`,
      text: "Scales and measurements the team drew on the drawings",
      to: `/tenders/${tenderId}/takeoff`,
    },
    gates.data?.pricing && {
      key: "pricing",
      title: `${gates.data.pricing} ${gates.data.pricing === 1 ? "price" : "prices"} to approve`,
      text: "Rates and markups from the office, each with its build-up or source",
      to: `/tenders/${tenderId}/estimate?show=waiting`,
    },
    gates.data?.subcontract && {
      key: "subcontract",
      title: `${gates.data.subcontract} ${gates.data.subcontract === 1 ? "quote" : "quotes"} to choose`,
      text: "Levelled subcontract and supplier quotes with the office’s recommendation",
      to: `/tenders/${tenderId}/subcontract`,
    },
    gates.data?.submission && {
      key: "submission",
      title: `${gates.data.submission} ${gates.data.submission === 1 ? "draft" : "drafts"} to review`,
      text: "Submission documents the office drafted from the tender’s requirements",
      to: `/tenders/${tenderId}/submission?show=review`,
    },
    gates.data?.boq && {
      key: "boq",
      title: `${gates.data.boq} BOQ ${gates.data.boq === 1 ? "item" : "items"} to approve`,
      text: "Entered by the office from the client’s BOQ, each with its page",
      to: `/tenders/${tenderId}/estimate?show=waiting`,
    },
  ].filter((a): a is { key: string; title: string; text: string; to: string } => Boolean(a));
  const waiting = [...questions, ...approvals];
  const manager = office.data.staff.find((m) => m.is_manager);
  const current = documents.data.filter((d) => d.status !== "replaced");
  const read = current.filter((d) => d.status === "read").length;
  const reading = current.filter((d) => d.status === "waiting" || d.status === "reading").length;
  const problems = current.length - read - reading;
  const state = { working: " The office is working.", paused: " The office is paused.", idle: "" }[office.data.state];

  return (
    <div className="flex min-h-full w-[720px] flex-col pt-14">
      <span className="text-ink-3">{tender.data.name}</span>
      <h1 className="mt-1.5 mb-1 text-[28px] font-semibold tracking-tight">
        {waiting.length === 0
          ? "Nothing needs you right now"
          : `${waiting.length} ${waiting.length === 1 ? "decision needs" : "decisions need"} you`}
      </h1>
      <p className="flex flex-wrap items-center gap-x-2 text-sm text-ink-2">
        <span>
          {dueSentence(tender.data.due_date, new Date())}
          {state}
        </span>
        <Outcome tenderId={tenderId} outcome={tender.data.outcome} />
      </p>
      {manager && gates.data?.manager ? (
        <p className="mt-1 text-sm text-ink-2">
          {gates.data.manager} {gates.data.manager === 1 ? "piece" : "pieces"} of work with {firstName(manager)} for
          review before {gates.data.manager === 1 ? "it comes" : "they come"} to you.
        </p>
      ) : null}

      {!office.data.ai_ready && (
        <p className="mt-6 text-ink-2">
          Choose the office’s AI in{" "}
          <Link to="/settings" className="font-medium text-ink underline underline-offset-4">
            Settings
          </Link>{" "}
          so the team can start work.
        </p>
      )}
      {manager && <ManagerNote tenderId={tenderId} managerId={manager.id} />}

      {waiting.length > 0 && (
        <>
          <h2 className="mt-9 mb-2 font-semibold text-ink-2">Needs you</h2>
          <div className="border-t border-line">
            {approvals.map((a) => (
              <Link key={a.key} to={a.to} className="flex items-center gap-3.5 border-b border-line px-1 py-3.5">
                <span className="size-[7px] shrink-0 rounded-full bg-attention" />
                <span className="flex min-w-0 grow flex-col gap-0.5">
                  <span className="text-sm font-medium">{a.title}</span>
                  <span className="truncate text-ink-3">{a.text}</span>
                </span>
                <IconChevronRight className="size-4 shrink-0 text-ink-4" stroke={1.75} />
              </Link>
            ))}
            {questions.map((d) => {
              const asker = office.data!.staff.find((m) => m.id === d.raised_by);
              return (
                <Link
                  key={d.id}
                  to={`/tenders/${tenderId}/decisions/${d.id}`}
                  className="flex items-center gap-3.5 border-b border-line px-1 py-3.5"
                >
                  <span className="size-[7px] shrink-0 rounded-full bg-attention" />
                  <span className="flex min-w-0 grow flex-col gap-0.5">
                    <span className="text-sm font-medium">{d.title}</span>
                    <span className="truncate text-ink-3">{d.text}</span>
                  </span>
                  {asker && <span className="text-ink-3">{firstName(asker)}</span>}
                  <IconChevronRight className="size-4 shrink-0 text-ink-4" stroke={1.75} />
                </Link>
              );
            })}
          </div>
        </>
      )}

      {current.length === 0 ? (
        <div className="mt-9 flex flex-col gap-3 border-t border-line pt-5">
          <h2 className="text-sm font-medium">Add the tender package</h2>
          <p className="text-ink-2">Choose the folder the client sent. Your original files are never changed.</p>
          <AddDocuments tenderId={tenderId} primary />
        </div>
      ) : (
        <div className="mt-9 flex flex-col">
          <h2 className="pb-2 font-semibold text-ink-2">Where things stand</h2>
          <Standing
            to={`/tenders/${tenderId}/documents`}
            label="Documents"
            text={`${read} of ${current.length} read${reading > 0 ? ` · reading ${reading}` : ""}${problems > 0 ? ` · ${problems} can’t be read` : ""}`}
            done={(read + problems) / current.length}
          />
          <Progress tenderId={tenderId} />
        </div>
      )}
      <DeleteTender tenderId={tenderId} name={tender.data.name} />

      <div className="grow" />
      <AskOffice tenderId={tenderId} to={manager?.id ?? TEAM} name={manager ? firstName(manager) : undefined} />
    </div>
  );
}

function Standing(props: { to: string; label: string; text: string; done: number }) {
  return (
    <Link to={props.to} className="grid grid-cols-[140px_minmax(0,1fr)_160px] items-center gap-4 px-1 py-2 hover:bg-rail">
      <span className="font-medium">{props.label}</span>
      <span className="min-w-0 text-ink-2">{props.text}</span>
      <span className="relative h-1 overflow-hidden rounded-sm bg-selected">
        <span
          className="absolute inset-y-0 left-0 rounded-sm bg-ink"
          style={{ width: `${Math.round(Math.min(1, props.done) * 100)}%` }}
        />
      </span>
    </Link>
  );
}

/** The BOQ, the price, the packages and the submission, once each has something in it. */
function Progress({ tenderId }: { tenderId: string }) {
  const estimate = useEstimate(tenderId);
  const packages = usePackages(tenderId);
  const submission = useSubmission(tenderId);
  const items = estimate.data?.items ?? [];
  const summary = estimate.data?.summary;
  const decided = items.filter((i) => i.item_status === "approved" || i.item_status === "office_approved").length;
  const chosen = (packages.data ?? []).filter((p) => p.selected_quote_id).length;
  const requirements = submission.data?.requirements ?? [];
  const ready = requirements.filter((r) => r.state === "ready").length;
  return (
    <>
      {items.length > 0 && (
        <Standing
          to={`/tenders/${tenderId}/estimate`}
          label="BOQ"
          text={`${items.length} items · ${decided} approved`}
          done={decided / items.length}
        />
      )}
      {summary && items.length > 0 && (
        <Standing
          to={`/tenders/${tenderId}/estimate`}
          label="Estimate"
          text={`${summary.priced} of ${summary.items} priced${summary.priced ? ` · ${summary.currency} ${money(summary.total)}` : ""}`}
          done={summary.priced / summary.items}
        />
      )}
      {(packages.data?.length ?? 0) > 0 && (
        <Standing
          to={`/tenders/${tenderId}/subcontract`}
          label="Subcontract"
          text={`${chosen} of ${packages.data!.length} packages chosen`}
          done={chosen / packages.data!.length}
        />
      )}
      {requirements.length > 0 && (
        <Standing
          to={`/tenders/${tenderId}/submission`}
          label="Submission"
          text={`${ready} of ${requirements.length} ready`}
          done={ready / requirements.length}
        />
      )}
    </>
  );
}

function DeleteTender({ tenderId, name }: { tenderId: string; name: string }) {
  const remove = useDeleteTender(tenderId);
  const navigate = useNavigate();
  const [asking, setAsking] = useState(false);
  if (!asking)
    return (
      <button onClick={() => setAsking(true)} className="mt-9 self-start text-ink-3 hover:text-attention">
        Delete this tender
      </button>
    );
  return (
    <div className="mt-9 flex flex-col gap-2.5 rounded-[10px] border border-line p-3.5">
      <span>
        Delete <span className="font-medium">{name}</span> and everything Quantix keeps for it: documents, BOQ,
        rates, the team and its conversations. Your original files and built packages stay.
      </span>
      <span className="flex gap-2">
        <button
          onClick={() => remove.mutate(undefined, { onSuccess: () => navigate("/") })}
          disabled={remove.isPending}
          className="h-[34px] rounded-lg bg-attention px-3.5 text-[13px] text-white"
        >
          Delete tender
        </button>
        <button onClick={() => setAsking(false)} className="h-[34px] rounded-lg border border-line-strong px-3.5 text-[13px]">
          Keep it
        </button>
      </span>
      {remove.isError && <span className="text-attention">{remove.error.message}</span>}
    </div>
  );
}

/** How the tender went. Later tenders use won and lost rates as benchmarks. */
function Outcome({ tenderId, outcome }: { tenderId: string; outcome: "open" | "submitted" | "won" | "lost" }) {
  const set = useSetOutcome(tenderId);
  return (
    <label className="flex items-center gap-1 text-ink-3">
      · Outcome
      <select
      aria-label="Outcome"
      value={outcome}
      onChange={(e) => set.mutate(e.target.value as typeof outcome)}
      className="rounded-md bg-transparent text-ink-3 outline-none hover:text-ink"
    >
      <option value="open">Open</option>
      <option value="submitted">Submitted</option>
      <option value="won">Won</option>
      <option value="lost">Lost</option>
      </select>
    </label>
  );
}

function ManagerNote({ tenderId, managerId }: { tenderId: string; managerId: string }) {
  const office = useOffice(tenderId);
  const chat = useMessages(tenderId, managerId);
  const manager = office.data?.staff.find((m) => m.id === managerId);
  const latest = [...(chat.data ?? [])].reverse().find((m) => m.sender === managerId);
  if (!manager) return null;
  return (
    <div className="mt-7 flex gap-3">
      <Face id={manager.id} size={28} />
      <div className="flex flex-col gap-1">
        <span className="font-medium">
          {manager.name} <span className="font-normal text-ink-3">Tender Manager</span>
        </span>
        <Prose
          text={tidy(latest?.text ?? manager.now ?? "Getting to know the tender.", firstName(manager)).split(/\n\s*\n/)[0]}
          className="text-[15px] text-[#27272A] [&>*]:line-clamp-4"
        />
        <Link to={`/tenders/${tenderId}/office?with=${manager.id}`} className="text-ink-3 hover:text-ink">
          Open your chat with {firstName(manager)}
        </Link>
      </div>
    </div>
  );
}

function AskOffice({ tenderId, to, name }: { tenderId: string; to: string; name?: string }) {
  const send = useSend(tenderId);
  const [draft, setDraft] = useState("");
  function submit(event: FormEvent) {
    event.preventDefault();
    if (draft.trim()) send.mutate({ channel: to, text: draft }, { onSuccess: () => setDraft("") });
  }
  return (
    <form onSubmit={submit} className="pt-4 pb-6">
      <div className="flex items-center gap-2 rounded-xl border border-line-strong py-1.5 pr-1.5 pl-3.5 shadow-[0_1px_2px_rgba(0,0,0,0.04)]">
        <input
          aria-label="Message the office"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={name ? `Ask ${name} anything` : "Tell the office what you need, for example: review the package"}
          className="grow bg-transparent text-sm outline-none"
        />
        <button
          aria-label="Send"
          disabled={!draft.trim() || send.isPending}
          className="flex size-8 items-center justify-center rounded-lg bg-ink text-white disabled:bg-line-strong"
        >
          <IconArrowUp className="size-4" stroke={1.75} />
        </button>
      </div>
      {send.isSuccess && !draft && <p className="pt-2 text-ink-3">Sent. Replies appear in the Office.</p>}
      {send.isError && <p className="pt-2 text-attention">{send.error.message}</p>}
    </form>
  );
}
