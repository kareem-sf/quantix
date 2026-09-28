import { IconArrowRight, IconArrowUp } from "@tabler/icons-react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { Face } from "./Face";
import { Prose } from "./Prose";
import {
  ENGINEER,
  TEAM,
  firstName,
  useAnswer,
  useDecisions,
  useMessages,
  useOffice,
  useSend,
  useStop,
  useTasks,
  useTurns,
  type Decision,
  type Message,
  type Staff,
  type Turn,
} from "./queries";
import { Sources } from "./Sources";
import { TurnLog } from "./TurnLog";

export function Office() {
  const { tenderId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const office = useOffice(tenderId);
  const staff = office.data?.staff ?? [];
  const channel = params.get("with") ?? TEAM;
  const asked = params.get("person"); // a profile the engineer opened; in a direct chat it shows beside the chat
  const person = staff.find((m) => m.id === (asked ?? (channel === TEAM ? undefined : channel)));
  const active = staff.filter((m) => m.status === "active");

  return (
    <div className="relative flex h-full w-full">
      <section
        aria-label="Conversations"
        className="flex w-60 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-line px-3 py-7 max-xl:w-52"
      >
        <h1 className="mx-2 mb-3.5 text-[22px] font-semibold tracking-tight">Office</h1>
        <Thread label="Team room" on={channel === TEAM} onClick={() => setParams({})} />
        {active.map((m) => (
          <Thread key={m.id} label={m.name} member={m} on={channel === m.id} onClick={() => setParams({ with: m.id })} />
        ))}
        {active.length === 0 && (
          <p className="px-2 pt-3 leading-normal text-ink-3">
            The Tender Manager joins when you first write to the office, and hires the team the tender needs.
          </p>
        )}
      </section>
      <section aria-label="Conversation" className="flex min-w-0 grow flex-col">
        <Conversation
          tenderId={tenderId}
          channel={channel}
          staff={staff}
          title={channel === TEAM ? "Team room" : (staff.find((m) => m.id === channel)?.name ?? "")}
          working={office.data?.state === "working"}
          notice={
            office.data?.state === "paused"
              ? (office.data.notice ?? "The office is stopped. Send a message to carry on.")
              : null
          }
          onPerson={(id) => setParams({ ...(channel === TEAM ? {} : { with: channel }), person: id })}
        />
      </section>
      {person && (
        <Profile
          tenderId={tenderId}
          member={person}
          overlay={Boolean(asked)}
          onMessage={() => setParams({ with: person.id })}
          onClose={() => setParams(channel === TEAM ? {} : { with: channel })}
        />
      )}
    </div>
  );
}

function Thread(props: { label: string; member?: Staff; on: boolean; onClick: () => void }) {
  return (
    <button
      onClick={props.onClick}
      className={`flex items-center gap-2.5 rounded-md px-2 py-[7px] text-left ${props.on ? "bg-selected font-semibold" : "hover:bg-rail"}`}
    >
      {props.member ? (
        <Face id={props.member.id} size={22} />
      ) : (
        <span className="flex size-[22px] items-center justify-center rounded-md bg-selected text-xs font-semibold text-ink-2">
          #
        </span>
      )}
      <span className="grow truncate">{props.label}</span>
    </button>
  );
}

export function Conversation(props: {
  tenderId: string;
  channel: string;
  staff: Staff[];
  title: string;
  working: boolean;
  notice?: string | null; // why the office is stopped, shown above the message box
  onPerson: (id: string) => void;
}) {
  const messages = useMessages(props.tenderId, props.channel);
  // the team room shows everyone's turns; a direct chat shows that person's, and the questions they put to you
  const direct = props.channel === TEAM ? undefined : props.channel;
  const turns = useTurns(props.tenderId, direct);
  const decisions = useDecisions(props.tenderId);
  const send = useSend(props.tenderId);
  const stop = useStop(props.tenderId);
  const [draft, setDraft] = useState("");
  const [technical, setTechnical] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  const timeline: Item[] = [
    ...(messages.data ?? []).map((message) => ({ at: message.created_at, message })),
    ...(turns.data ?? []).map((turn) => ({ at: turn.started_at, turn })),
    ...(decisions.data ?? []).filter((d) => d.raised_by === direct).map((decision) => ({ at: decision.created_at, decision })),
  ].sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
  const count = timeline.length;
  // one heading per run of the same person's work and words, as a chatbot shows its thinking above its reply
  const groups: { author: string | null; items: Item[] }[] = [];
  for (const item of timeline) {
    const author = authorOf(item);
    const last = groups.at(-1);
    if (author && last?.author === author) last.items.push(item);
    else groups.push({ author, items: [item] });
  }
  useEffect(() => {
    end.current?.scrollIntoView?.({ block: "end" });
  }, [count]);
  const people = new Map(props.staff.map((m) => [m.id, m]));

  function submit(event: FormEvent) {
    event.preventDefault();
    if (!draft.trim()) return;
    send.mutate({ channel: props.channel, text: draft }, { onSuccess: () => setDraft("") });
  }

  return (
    <>
      <div className="flex items-end justify-between border-b border-line px-8 pt-7 pb-3.5">
        <div className="flex flex-col gap-1">
          <h2 className="text-[17px] font-semibold">{props.title}</h2>
          <span className="text-ink-3">
            {props.channel === TEAM
              ? "Everything the team says to each other. Only real messages appear here."
              : "Your direct conversation."}
          </span>
        </div>
        {props.working && (
          <button onClick={() => stop.mutate()} className="text-ink-2 hover:text-ink">
            Stop the office
          </button>
        )}
      </div>
      <div className="flex min-h-0 grow flex-col gap-[18px] overflow-y-auto px-8 py-5">
        {count === 0 && <p className="text-ink-3">No messages yet.</p>}
        {groups.map((group) =>
          group.author === null ? (
            <Aside key={keyOf(group.items[0])} message={(group.items[0] as { message: Message }).message} />
          ) : (
            <Block key={keyOf(group.items[0])} author={group.author} person={people.get(group.author)} at={group.items[0].at} onPerson={props.onPerson}>
              {group.items.map((item) =>
                "turn" in item ? (
                  <TurnLog
                    key={keyOf(item)}
                    turn={item.turn}
                    name={people.get(item.turn.staff_id)?.name.split(" ")[0] ?? "Someone"}
                    technical={technical}
                    onTechnical={setTechnical}
                  />
                ) : "decision" in item ? (
                  <Question key={keyOf(item)} tenderId={props.tenderId} decision={item.decision} />
                ) : (
                  <Said key={keyOf(item)} message={item.message} person={people.get(item.message.sender)} />
                ),
              )}
            </Block>
          ),
        )}
        <div ref={end} />
      </div>
      <form onSubmit={submit} className="px-8 pt-3.5 pb-6">
        {props.notice && <p className="pb-2.5 text-[13px] text-attention">{props.notice}</p>}
        <div className="flex items-center gap-2 rounded-xl border border-line-strong py-1.5 pr-1.5 pl-3.5 shadow-[0_1px_2px_rgba(0,0,0,0.04)]">
          <input
            aria-label="Message"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={props.channel === TEAM ? "Message the team room" : `Message ${props.title.split(" ")[0]}`}
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
        {send.isError && <p className="pt-2 text-attention">{send.error.message}</p>}
      </form>
    </>
  );
}

type Item = { at: string; message: Message } | { at: string; turn: Turn } | { at: string; decision: Decision };

/** Whose heading an item goes under; none for the office's notes and handed-out tasks, which stand on their own. */
function authorOf(item: Item): string | null {
  if ("turn" in item) return item.turn.staff_id;
  if ("decision" in item) return item.decision.raised_by;
  return item.message.kind === "task" || item.message.kind === "note" ? null : item.message.sender;
}

function keyOf(item: Item): string {
  return "turn" in item ? `t${item.turn.id}` : "decision" in item ? `d${item.decision.id}` : `m${item.message.id}`;
}

function clock(at: string): string {
  return new Date(at).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
}

function Aside({ message }: { message: Message }) {
  return (
    <div className="flex items-center gap-2 pl-10 text-ink-3">
      <IconArrowRight className="size-3.5 shrink-0" stroke={1.75} />
      <span className="grow">{message.text}</span>
      <span className="text-xs">{clock(message.created_at)}</span>
    </div>
  );
}

/** One person's run of work and words under their face and name: their turns, messages and questions. */
function Block(props: { author: string; person?: Staff; at: string; onPerson: (id: string) => void; children: ReactNode }) {
  const { person } = props;
  const mine = props.author === ENGINEER;
  return (
    <div className="flex gap-3">
      {person ? (
        <button onClick={() => props.onPerson(person.id)} aria-label={`About ${person.name}`} className="self-start">
          <Face id={person.id} size={28} />
        </button>
      ) : (
        <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-ink text-xs text-white">
          {mine ? "You" : "?"}
        </span>
      )}
      <div className="flex max-w-[640px] min-w-0 grow flex-col gap-2">
        <span>
          <span className="font-semibold">{mine ? "You" : person ? firstName(person) : "Office"}</span>{" "}
          <span className="text-ink-3">
            {person?.role ? `${person.role} · ` : ""}
            {clock(props.at)}
          </span>
        </span>
        {props.children}
      </div>
    </div>
  );
}

/** A written "Sources: …" line repeats what shows under the message as links. */
function withoutSourcesLine(text: string): string {
  return text.replace(/\n\s*\**sources?\**\s*:\**[^\n]*\s*$/i, "");
}

function Said({ message, person }: { message: Message; person?: Staff }) {
  const linked = Boolean(message.sources && message.sources.length > 0);
  return (
    <div className="flex flex-col gap-1">
      {message.kind === "concern" && <span className="text-xs font-medium text-attention">Raised a concern</span>}
      {message.sender === ENGINEER ? (
        <span className="text-sm leading-relaxed whitespace-pre-wrap text-[#27272A] [overflow-wrap:anywhere]" dir="auto">
          {message.text}
        </span>
      ) : (
        <Prose
          text={linked ? withoutSourcesLine(message.text) : message.text}
          signer={person ? firstName(person) : undefined}
          className="text-sm text-[#27272A]"
        />
      )}
      {linked && (
        <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs text-ink-3">
          <span>From</span>
          <Sources sources={message.sources!} />
        </div>
      )}
    </div>
  );
}

/** A question put to the engineer, answered where it was asked: pick an option and send it, or open it in full. */
function Question({ tenderId, decision }: { tenderId: string; decision: Decision }) {
  const answer = useAnswer(tenderId);
  const [choice, setChoice] = useState("");
  const waiting = decision.status === "waiting";
  return (
    <div className="flex flex-col gap-2.5 rounded-xl border border-line-strong px-4 py-3.5" role="group" aria-label={decision.title}>
      <span className="text-xs font-semibold text-ink-2">{waiting ? "Needs your decision" : "You decided"}</span>
      <span className="font-semibold">{decision.title}</span>
      <Prose text={decision.text} className="text-sm text-[#27272A]" />
      {waiting ? (
        <>
          <div className="flex flex-col gap-1.5">
            {decision.options.map((option) => (
              <button
                key={option}
                onClick={() => setChoice(option)}
                aria-pressed={choice === option}
                className={`rounded-lg border px-3 py-2 text-left text-sm ${choice === option ? "border-ink bg-rail" : "border-line hover:bg-rail"}`}
              >
                {option}
              </button>
            ))}
          </div>
          <div className="flex items-center gap-4">
            <button
              disabled={!choice || answer.isPending}
              onClick={() => answer.mutate({ id: decision.id, answer: choice })}
              className="h-8 rounded-lg bg-ink px-3.5 text-sm text-white disabled:bg-line-strong"
            >
              Send answer
            </button>
            <Link to={`/tenders/${tenderId}/decisions/${decision.id}`} className="text-sm text-ink-2 hover:text-ink">
              Answer in your own words
            </Link>
          </div>
          {answer.isError && <p className="text-attention">{answer.error.message}</p>}
        </>
      ) : (
        <span className="text-sm text-ink-2">{decision.answer}</span>
      )}
    </div>
  );
}

function Profile(props: {
  tenderId: string;
  member: Staff;
  overlay: boolean;
  onMessage: () => void;
  onClose: () => void;
}) {
  const { tenderId, member, onMessage } = props;
  const tasks = useTasks(tenderId);
  const mine = (tasks.data ?? []).filter((t) => t.staff_id === member.id);
  const p = member.profile as Record<string, string | number | undefined>;
  return (
    <aside
      aria-label={member.name}
      className={`flex w-[340px] shrink-0 flex-col gap-5 overflow-y-auto border-l border-line bg-white px-6 pt-7 pb-6 ${
        props.overlay
          ? "max-[1400px]:absolute max-[1400px]:inset-y-0 max-[1400px]:right-0 max-[1400px]:z-10 max-[1400px]:shadow-[-8px_0_24px_rgba(0,0,0,0.08)]"
          : "max-[1400px]:hidden"
      }`}
    >
      <div className="flex items-center gap-3.5">
        <Face id={member.id} size={56} />
        <div className="flex flex-col gap-0.5">
          <span className="text-[17px] font-semibold">{member.name}</span>
          <span className="text-ink-2">
            {member.role}
            {p.experience_years ? ` · ${p.experience_years} years` : ""}
          </span>
        </div>
      </div>
      {[
        ["Background", p.background],
        ["How they work", p.working_style],
        ["What they believe", p.opinions],
        ["Now", member.status === "released" ? "Released from this tender" : (member.now ?? "Idle")],
      ].map(([label, text]) =>
        text ? (
          <div key={label as string} className="flex flex-col gap-1.5">
            <span className="text-xs font-semibold text-ink-2">{label}</span>
            <span className="leading-normal text-[#27272A]">{text}</span>
          </div>
        ) : null,
      )}
      {mine.length > 0 && (
        <div className="flex flex-col">
          <span className="pb-1.5 text-xs font-semibold text-ink-2">Their tasks</span>
          {mine.map((t) => (
            <span key={t.id} className="flex justify-between gap-2.5 border-t border-subtle py-2">
              <span>{t.title}</span>
              <span className="shrink-0 text-ink-3">
                {t.status === "done" ? "done" : member.status === "released" ? "not done" : "working"}
              </span>
            </span>
          ))}
        </div>
      )}
      <div className="grow" />
      {member.status === "active" && (
        <button onClick={onMessage} className="h-[38px] shrink-0 rounded-lg bg-ink text-sm text-white">
          Message {firstName(member)}
        </button>
      )}
      <button onClick={props.onClose} className="shrink-0 text-center text-ink-3 hover:text-ink">
        Close
      </button>
    </aside>
  );
}
