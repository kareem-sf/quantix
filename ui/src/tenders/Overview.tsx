import { IconChevronRight, IconInfoCircle } from "@tabler/icons-react";
import { useState, type FormEvent } from "react";
import { Link, useParams } from "react-router";
import { AddDocuments } from "../documents/AddDocuments";
import { useDocuments } from "../documents/queries";
import { money, useEstimate, useGates } from "../estimate/queries";
import { Face } from "../office/Face";
import { Prose, tidy } from "../office/Prose";
import { firstName, useMessages, useOffice } from "../office/queries";
import { useShell } from "../app/context";
import { Opening } from "../app/Opening";
import { useAudit, type Finding } from "../review/queries";
import { dueSentence, dueSource, type DueSource as DueSourceOut } from "./due";
import { useNeedsYou } from "./needsYou";
import { usePackages } from "../subcontract/queries";
import { useSubmission } from "../submission/queries";
import { useSetDueDate, useTender } from "./queries";
import { TenderMenu } from "./TenderMenu";

export function Overview() {
  const { tenderId = "" } = useParams();
  const tender = useTender(tenderId);
  const documents = useDocuments(tenderId);
  const office = useOffice(tenderId);
  const gates = useGates(tenderId);
  const { approvals, questions } = useNeedsYou(tenderId);

  if (tender.isError) return <p className="pt-14 text-ink-2">{tender.error.message}</p>;
  if (!tender.data || !documents.data || !office.data) return <Opening error={documents.isError || office.isError} />;

  const waiting = [...questions, ...approvals];
  const manager = office.data.staff.find((m) => m.is_manager);
  const current = documents.data.filter((d) => d.status !== "replaced");
  const read = current.filter((d) => d.status === "read").length;
  const reading = current.filter((d) => d.status === "waiting" || d.status === "reading").length;
  const problems = current.length - read - reading;
  const state = { working: " The office is working.", paused: " The office is paused.", idle: "" }[office.data.state];

  return (
    <div className="flex min-h-full w-full max-w-[784px] flex-col px-8 pt-14">
      <span className="flex items-center justify-between gap-4 text-ink-3">
        <span className="truncate">
          {tender.data.name}
          {tender.data.archived && " · archived"}
        </span>
        <TenderMenu
          tenderId={tenderId}
          name={tender.data.name}
          outcome={tender.data.outcome}
          archived={tender.data.archived}
        />
      </span>
      <h1 className="mt-1.5 mb-1 text-[28px] font-semibold tracking-tight">
        {waiting.length === 0
          ? "Nothing needs you right now"
          : `${waiting.length} ${waiting.length === 1 ? "decision needs" : "decisions need"} you`}
      </h1>
      <p className="flex flex-wrap items-center gap-x-2 text-sm text-ink-2">
        <DueDate tenderId={tenderId} due={tender.data.due_date} source={tender.data.due_date_source} />
        {state && <span>{state.trim()}</span>}
        {tender.data.outcome !== "open" && (
          <span>{{ submitted: "Submitted.", won: "Won.", lost: "Lost." }[tender.data.outcome]}</span>
        )}
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
          <Link to="/settings?section=ai" className="font-medium text-ink underline underline-offset-4">
            Settings
          </Link>{" "}
          so the team can start work.
        </p>
      )}
      {manager ? <ManagerNote tenderId={tenderId} managerId={manager.id} /> : office.data.ai_ready && <StartOffice />}

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
      {current.length > 0 && <Audit tenderId={tenderId} />}
    </div>
  );
}

/** Quantix's audit of the whole tender, once pricing has started: what keeps it from release. */
function Audit({ tenderId }: { tenderId: string }) {
  const estimate = useEstimate(tenderId);
  const audit = useAudit(tenderId);
  if (!estimate.data?.items.length || !audit.data) return null;
  const blockers = audit.data.filter((f) => f.severity === "blocker");
  const warnings = audit.data.filter((f) => f.severity === "warning" && !f.reason);
  const accepted = audit.data.filter((f) => f.reason);
  return (
    <div className="mt-9 flex flex-col">
      <h2 className="pb-2 font-semibold text-ink-2">Before release</h2>
      {blockers.length + warnings.length === 0 ? (
        <p className="px-1 text-ink-2">
          Quantix’s audit is clear: nothing keeps the tender from release.{" "}
          <Link to={`/tenders/${tenderId}/submission`} className="font-medium text-ink underline underline-offset-4">
            Build the package
          </Link>
        </p>
      ) : (
        <p className="px-1 pb-2 text-ink-2">
          {[
            blockers.length && `${blockers.length} ${blockers.length === 1 ? "thing" : "things"} must be fixed`,
            warnings.length && `${warnings.length} ${warnings.length === 1 ? "needs" : "need"} checking`,
          ]
            .filter(Boolean)
            .join(" and ")}{" "}
          before the tender can go.
        </p>
      )}
      <ul className="flex flex-col">
        {[...blockers, ...warnings].map((f, n) => (
          <AuditLine key={n} tenderId={tenderId} finding={f} />
        ))}
      </ul>
      {accepted.length > 0 && (
        <details className="group border-t border-line">
          <summary className="flex cursor-pointer list-none items-center gap-1.5 px-1 py-2.5 text-ink-3 hover:text-ink">
            <IconChevronRight className="size-4 transition-transform group-open:rotate-90" stroke={1.75} />
            {accepted.length} {accepted.length === 1 ? "warning" : "warnings"} accepted by the office, with the reason
          </summary>
          <ul className="flex flex-col">
            {accepted.map((f, n) => (
              <AuditLine key={n} tenderId={tenderId} finding={f} />
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

function AuditLine({ tenderId, finding: f }: { tenderId: string; finding: Finding }) {
  return (
    <li className="flex gap-3 border-t border-line px-1 py-2.5 leading-normal">
      <span
        className={`mt-[7px] size-[7px] shrink-0 rounded-full ${f.severity === "blocker" ? "bg-attention" : f.reason ? "bg-approved" : "bg-ink-4"}`}
      />
      <span className="flex min-w-0 flex-col gap-0.5">
        <span className={f.reason ? "text-ink-2" : ""}>{f.message}</span>
        {f.refs.map((r) =>
          r.document_id ? (
            <Link
              key={r.label}
              to={`/tenders/${tenderId}/documents?doc=${r.document_id}&page=${r.page}`}
              className="self-start text-ink-3 underline underline-offset-4"
            >
              {r.label}
            </Link>
          ) : null,
        )}
        {f.reason && (
          <span className="text-ink-3">
            Accepted by {f.accepted_by ?? "the Manager"}: {f.reason}
          </span>
        )}
      </span>
    </li>
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
          text={`${summary.priced} of ${summary.items} priced${summary.priced ? ` · ${summary.currency} ${money(summary.total)} before VAT` : ""}`}
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

/** When the tender closes, and where that date comes from; the engineer can change it. */
function DueDate({ tenderId, due, source }: { tenderId: string; due: string | null; source?: DueSourceOut | null }) {
  const set = useSetDueDate(tenderId);
  const [value, setValue] = useState<string | null>(null); // the date being entered, while changing it
  if (value === null)
    return (
      <span className="inline-flex items-center gap-1">
        {dueSentence(due, new Date())}
        {due && <DueSourceNote tenderId={tenderId} source={source} />}{" "}
        <button type="button" onClick={() => setValue(due ?? "")} className="text-ink-3 hover:text-ink">
          {due ? "Change" : "Set the due date"}
        </button>
      </span>
    );
  const save = (e: FormEvent) => {
    e.preventDefault();
    set.mutate(value || null, { onSuccess: () => setValue(null) });
  };
  return (
    <form onSubmit={save} className="flex items-center gap-2">
      <input
        type="date"
        aria-label="Due date"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        className="rounded-md border border-subtle px-2 py-0.5 text-ink"
      />
      <button type="submit" disabled={set.isPending} className="font-medium text-ink">
        Save
      </button>
      <button type="button" onClick={() => setValue(null)} className="text-ink-3 hover:text-ink">
        Cancel
      </button>
    </form>
  );
}

/** Where the due date comes from, on hover or focus: the engineer's own date, or the page that states it. */
function DueSourceNote({ tenderId, source }: { tenderId: string; source?: DueSourceOut | null }) {
  const said = dueSource(source);
  if (!said || !source) return null;
  return (
    <span className="group relative inline-flex">
      <button
        type="button"
        aria-label={`Where the due date comes from: ${said.title}`}
        className="inline-flex text-ink-3 hover:text-ink"
      >
        <IconInfoCircle size={15} stroke={1.75} />
      </button>
      <span
        role="note"
        className="invisible absolute top-full left-1/2 z-20 mt-1.5 w-72 -translate-x-1/2 rounded-lg border border-subtle bg-white p-3 text-[13px] leading-snug text-ink-2 shadow-md group-focus-within:visible group-hover:visible"
      >
        <span className="mb-1 block font-medium text-ink">{said.title}</span>
        {source.basis === "document" && source.document_id ? (
          <>
            “{source.quote}”{" "}
            <Link
              to={`/tenders/${tenderId}/documents?doc=${source.document_id}&page=${source.page}`}
              className="text-ink underline underline-offset-4"
            >
              {source.document_name}, page {source.page}
            </Link>
            {source.set_by && <span className="mt-1 block text-ink-3">Entered by {source.set_by}.</span>}
          </>
        ) : (
          said.text
        )}
      </span>
    </span>
  );
}

function ManagerNote({ tenderId, managerId }: { tenderId: string; managerId: string }) {
  const office = useOffice(tenderId);
  const chat = useMessages(tenderId, managerId);
  const manager = office.data?.staff.find((m) => m.id === managerId);
  const latest = [...(chat.data ?? [])].reverse().find((m) => m.sender === managerId);
  const { showTeam } = useShell();
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
        <button onClick={() => showTeam(manager.id)} className="self-start text-ink-3 hover:text-ink">
          Open your chat with {firstName(manager)}
        </button>
      </div>
    </div>
  );
}

/** Before the Tender Manager joins: the office starts when the engineer first writes to it, in the team panel. */
function StartOffice() {
  const { showTeam } = useShell();
  return (
    <div className="mt-7 flex items-center gap-4 rounded-xl border border-line-strong px-4 py-3.5">
      <span className="grow text-ink-2">
        Tell the office what you need, for example “review the package”. The Tender Manager joins and hires the team
        the tender needs.
      </span>
      <button onClick={() => showTeam()} className="h-8 shrink-0 rounded-lg bg-ink px-3.5 text-white">
        Write to the office
      </button>
    </div>
  );
}
