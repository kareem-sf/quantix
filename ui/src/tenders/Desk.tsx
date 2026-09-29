import { IconChevronRight } from "@tabler/icons-react";
import { Link, useNavigate } from "react-router";
import { useShell } from "../app/context";
import { placeIn } from "../app/place";
import { Opening } from "../app/Opening";
import { money } from "../estimate/queries";
import { Face } from "../office/Face";
import { useOffice } from "../office/queries";
import { dueShort } from "./due";
import { useNeedsYou } from "./needsYou";
import { useDesk, type TenderGlance } from "./queries";
import { STAGES, daysTo, stages, standing } from "./stage";

/** The engineer's desk: what needs them across every open tender, what closes next, who is working, and each tender
 * at a glance. */
export function Desk() {
  const desk = useDesk();
  if (!desk.data) return <Opening error={desk.isError} />;
  const active = desk.data
    .filter((t) => t.outcome === "open" && !t.archived)
    .sort((a, b) => due(a) - due(b) || Date.parse(b.created_at) - Date.parse(a.created_at));
  const waiting = active.filter((t) => t.waiting > 0);
  const total = waiting.reduce((n, t) => n + t.waiting, 0);
  const today = new Date();

  return (
    <div className="flex w-full max-w-[1120px] flex-col px-8 pt-8 pb-10">
      <span className="text-ink-3">
        {today.toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" })}
      </span>
      <h1 className="mt-1 text-[24px] font-semibold tracking-tight">
        {total === 0
          ? "Nothing needs you right now"
          : `${total} ${total === 1 ? "thing needs" : "things need"} you${waiting.length > 1 ? ` across ${waiting.length} tenders` : ""}`}
      </h1>
      <Figures tenders={desk.data} active={active} />

      <div className="mt-8 grid grid-cols-[minmax(0,1fr)_280px] gap-10 @max-5xl:grid-cols-1">
        <section aria-label="Needs you" className="flex flex-col">
          <h2 className="pb-2 font-semibold text-ink-2">Needs you</h2>
          {waiting.length === 0 && <p className="text-ink-3">Every tender is with the team.</p>}
          {waiting.map((t) => (
            <TenderNeeds key={t.id} tender={t} />
          ))}
        </section>
        <aside className="flex flex-col gap-8">
          <section aria-label="Closing dates" className="flex flex-col">
            <h2 className="pb-2 font-semibold text-ink-2">Closing dates</h2>
            {active.map((t) => (
              <Closing key={t.id} tender={t} />
            ))}
            {active.length === 0 && <p className="text-ink-3">No open tenders.</p>}
          </section>
          <Working tenders={active} />
        </aside>
      </div>

      <section aria-label="Open tenders" className="mt-10 flex flex-col">
        <div className="flex items-baseline justify-between pb-2">
          <h2 className="font-semibold text-ink-2">Open tenders</h2>
          <Link to="/tenders" className="text-ink-3 hover:text-ink">
            All tenders
          </Link>
        </div>
        <Register tenders={active} />
      </section>
    </div>
  );
}

function due(t: TenderGlance) {
  return t.due_date ? Date.parse(t.due_date) : Infinity;
}

/** A few figures across the tenders, each counted by Quantix. */
function Figures({ tenders, active }: { tenders: TenderGlance[]; active: TenderGlance[] }) {
  const week = active.filter((t) => t.due_date && daysTo(t.due_date) >= 0 && daysTo(t.due_date) <= 7).length;
  const priced = active.filter((t) => t.total !== null);
  const currencies = [...new Set(priced.map((t) => t.currency))];
  const year = new Date().getFullYear();
  const closed = tenders.filter((t) => t.outcome !== "open" && t.outcome_at && new Date(t.outcome_at).getFullYear() === year);
  const won = tenders.filter((t) => t.outcome === "won").length;
  const lost = tenders.filter((t) => t.outcome === "lost").length;
  const figures: [string, string, string][] = [
    ["Open tenders", String(active.length), week ? `${week} ${week === 1 ? "closes" : "close"} this week` : "None close this week"],
    [
      "Priced so far",
      currencies.length === 1
        ? `${currencies[0]} ${short(priced.reduce((n, t) => n + Number(t.total), 0))}`
        : currencies.length === 0
          ? "–"
          : `${priced.length} tenders`,
      `${priced.length} of ${active.length} open tenders priced, before VAT`,
    ],
    ["Closed this year", String(closed.length), `${closed.filter((t) => t.outcome === "submitted").length} awaiting a result`],
    ["Won", won + lost ? `${Math.round((100 * won) / (won + lost))}%` : "–", `${won} won of ${won + lost} decided`],
  ];
  return (
    <div className="mt-6 grid grid-cols-4 border-y border-line @max-3xl:grid-cols-2">
      {figures.map(([label, value, note], n) => (
        <div key={label} className={`flex flex-col py-3 ${n % 4 ? "border-l border-line pl-4" : ""} @max-3xl:[&:nth-child(3)]:border-l-0 @max-3xl:[&:nth-child(3)]:pl-0`}>
          <span className="text-xs text-ink-3">{label}</span>
          <span className="text-[20px] font-semibold tracking-tight">{value}</span>
          <span className="text-xs text-ink-3">{note}</span>
        </div>
      ))}
    </div>
  );
}

function short(value: number) {
  return new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 }).format(value);
}

function Days({ date }: { date: string | null }) {
  if (!date) return <span className="text-ink-3">no due date</span>;
  const days = daysTo(date);
  const text = days < 0 ? "closed" : days === 0 ? "today" : days === 1 ? "tomorrow" : `${days} days`;
  return (
    <span className={`rounded-md px-1.5 py-px text-xs ${days >= 0 && days <= 4 ? "bg-attention/10 text-attention" : "bg-subtle text-ink-2"}`}>
      {text}
    </span>
  );
}

/** One tender's decisions, each one click from where it is taken: a screen, or the person who asked. */
function TenderNeeds({ tender }: { tender: TenderGlance }) {
  const { approvals, questions } = useNeedsYou(tender.id);
  const office = useOffice(tender.id);
  const navigate = useNavigate();
  const { showTeam } = useShell();
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));
  return (
    <div className="flex flex-col pb-4">
      <Link to={placeIn(tender.id)} className="flex items-center gap-2 py-2 font-semibold hover:text-ink-2">
        {tender.name} <Days date={tender.due_date} />
      </Link>
      <div className="border-t border-line">
        {approvals.map((a) => (
          <Link key={a.key} to={a.to} className="flex items-center gap-3 border-b border-line px-1 py-2.5 hover:bg-rail">
            <span className="size-[7px] shrink-0 rounded-full bg-attention" />
            <span className="flex min-w-0 grow flex-col">
              <span className="font-medium">{a.title}</span>
              <span className="truncate text-ink-3">{a.text}</span>
            </span>
            <IconChevronRight className="size-4 shrink-0 text-ink-4" stroke={1.75} />
          </Link>
        ))}
        {questions.map((q) => {
          const asker = people.get(q.raised_by);
          return (
            <button
              key={q.id}
              onClick={() => {
                navigate(`/tenders/${tender.id}`);
                showTeam(q.raised_by);
              }}
              className="flex w-full items-center gap-3 border-b border-line px-1 py-2.5 text-left hover:bg-rail"
            >
              {asker ? <Face id={asker.id} size={22} /> : <span className="size-[7px] shrink-0 rounded-full bg-attention" />}
              <span className="flex min-w-0 grow flex-col">
                <span className="font-medium">
                  {asker ? `${asker.name.split(" ")[0]} asks: ` : ""}
                  {q.title}
                </span>
                <span className="truncate text-ink-3">{q.text}</span>
              </span>
              <IconChevronRight className="size-4 shrink-0 text-ink-4" stroke={1.75} />
            </button>
          );
        })}
      </div>
    </div>
  );
}

function Closing({ tender }: { tender: TenderGlance }) {
  const date = tender.due_date ? new Date(`${tender.due_date}T00:00:00`) : null;
  const soon = tender.due_date !== null && daysTo(tender.due_date) >= 0 && daysTo(tender.due_date) <= 4;
  return (
    <Link to={placeIn(tender.id)} className="flex items-center gap-3 border-b border-line py-2 hover:bg-rail">
      <span
        className={`flex w-11 shrink-0 flex-col items-center rounded-md py-0.5 leading-tight ${soon ? "bg-attention/10 text-attention" : "bg-subtle"}`}
      >
        <span className="text-[15px] font-semibold">{date ? date.getDate() : "–"}</span>
        <span className="text-[10.5px] uppercase">{date ? date.toLocaleDateString("en-GB", { month: "short" }) : "date"}</span>
      </span>
      <span className="flex min-w-0 flex-col">
        <span className="truncate font-medium">{tender.name}</span>
        <span className="text-xs text-ink-3">
          {tender.due_date ? dueShort(tender.due_date).replace("due ", "Closes ") : "No due date yet"}
          {tender.total !== null && ` · ${tender.currency} ${short(Number(tender.total))}`}
        </span>
      </span>
    </Link>
  );
}

function Working({ tenders }: { tenders: TenderGlance[] }) {
  const busy = tenders.filter((t) => t.team === "working");
  return (
    <section aria-label="Working now" className="flex flex-col">
      <h2 className="pb-2 font-semibold text-ink-2">Working now</h2>
      {busy.length === 0 && <p className="text-ink-3">No team is working.</p>}
      {busy.map((t) => (
        <Link key={t.id} to={placeIn(t.id)} className="flex items-start gap-2.5 py-1.5 hover:text-ink-2">
          <span className="mt-1.5 size-[7px] shrink-0 rounded-full bg-approved motion-safe:animate-pulse" />
          <span className="flex min-w-0 flex-col">
            <span className="truncate font-medium">{t.name}</span>
            <span className="line-clamp-2 text-xs text-ink-3">{t.doing ?? "Working"}</span>
          </span>
        </Link>
      ))}
    </section>
  );
}

/** Tenders as rows: where each stands, its total and what waits; a row opens the tender where the engineer left it.
 * On a narrow screen "where it stands" gives way, and the table scrolls sideways rather than the screen. */
export function Register({ tenders, closed = false }: { tenders: TenderGlance[]; closed?: boolean }) {
  const navigate = useNavigate();
  if (tenders.length === 0) return <p className="border-t border-line py-3 text-ink-3">None.</p>;
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[520px] border-collapse">
        <thead>
          <tr className="border-b border-line-strong text-left text-xs text-ink-3">
            <th className="py-2 pr-3 font-normal">Tender</th>
            <th className="py-2 pr-3 font-normal">{closed ? "Outcome" : "Closes"}</th>
            <th className="py-2 pr-3 font-normal @max-3xl:hidden">{closed ? "Stage reached" : "Where it stands"}</th>
            <th className="py-2 pr-3 text-right font-normal">Tender total excl. VAT</th>
            <th className="py-2 text-right font-normal">Needs you</th>
          </tr>
        </thead>
        <tbody>
          {tenders.map((t) => (
            <tr
              key={t.id}
              onClick={() => navigate(placeIn(t.id))}
              className="cursor-pointer border-b border-line hover:bg-rail"
            >
              <td className="py-2.5 pr-3">
                <span className="flex items-center gap-2 font-medium">
                  {t.team === "working" && (
                    <span title="The team is working" className="size-[7px] shrink-0 rounded-full bg-approved" />
                  )}
                  <Link to={placeIn(t.id)} onClick={(e) => e.stopPropagation()} className="hover:underline">
                    {t.name}
                  </Link>
                </span>
              </td>
              <td className="py-2.5 pr-3 whitespace-nowrap">{closed ? <Outcome tender={t} /> : <Due tender={t} />}</td>
              <td className="py-2.5 pr-3 @max-3xl:hidden">
                <span className="flex items-center gap-2.5 text-ink-2">
                  <Pips tender={t} />
                  {standing(t)}
                </span>
              </td>
              <td className="py-2.5 pr-3 text-right whitespace-nowrap">
                {t.total !== null ? `${t.currency} ${money(t.total)}` : <span className="text-ink-4">–</span>}
              </td>
              <td className="py-2.5 text-right">
                {t.waiting > 0 ? (
                  <span className="rounded-full bg-attention px-2 py-px text-xs font-medium text-white">{t.waiting}</span>
                ) : (
                  <span className="text-ink-4">–</span>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Due({ tender }: { tender: TenderGlance }) {
  return (
    <span className="flex items-center gap-2">
      {tender.due_date ? dueShort(tender.due_date).replace("due ", "") : ""}
      <Days date={tender.due_date} />
    </span>
  );
}

function Outcome({ tender }: { tender: TenderGlance }) {
  const word = { open: "Open", submitted: "Submitted", won: "Won", lost: "Lost" }[tender.outcome];
  const on = tender.outcome_at
    ? ` ${new Date(tender.outcome_at).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" })}`
    : "";
  return (
    <span className={tender.outcome === "won" ? "text-approved" : "text-ink-2"}>
      {word}
      {on}
    </span>
  );
}

function Pips({ tender }: { tender: TenderGlance }) {
  const done = stages(tender);
  return (
    <span className="flex shrink-0 gap-[3px]" aria-hidden>
      {STAGES.map((s, n) => (
        <span key={s} title={s} className={`h-1 w-3.5 rounded-sm ${done[n] ? "bg-ink" : "bg-line-strong"}`} />
      ))}
    </span>
  );
}
