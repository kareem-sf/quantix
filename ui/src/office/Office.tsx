import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { Face } from "./Face";
import {
  ENGINEER,
  TEAM,
  firstName,
  useMessages,
  useOffice,
  useSend,
  useStop,
  useTasks,
  type Message,
  type Staff,
} from "./queries";

export function Office() {
  const { tenderId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const office = useOffice(tenderId);
  const staff = office.data?.staff ?? [];
  const channel = params.get("with") ?? TEAM;
  const person = staff.find((m) => m.id === (params.get("person") ?? (channel === TEAM ? undefined : channel)));
  const active = staff.filter((m) => m.status === "active");

  return (
    <div className="flex h-full w-full">
      <section aria-label="Conversations" className="flex w-60 shrink-0 flex-col gap-0.5 border-r border-line px-3 py-7">
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
          onPerson={(id) => setParams({ ...(channel === TEAM ? {} : { with: channel }), person: id })}
        />
      </section>
      {person && <Profile tenderId={tenderId} member={person} onMessage={() => setParams({ with: person.id })} />}
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
  onPerson: (id: string) => void;
}) {
  const messages = useMessages(props.tenderId, props.channel);
  const send = useSend(props.tenderId);
  const stop = useStop(props.tenderId);
  const [draft, setDraft] = useState("");
  const end = useRef<HTMLDivElement>(null);
  const count = messages.data?.length ?? 0;
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
        {messages.data?.map((m) => <Line key={m.id} message={m} author={people.get(m.sender)} onPerson={props.onPerson} />)}
        <div ref={end} />
      </div>
      <form onSubmit={submit} className="px-8 pt-3.5 pb-6">
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
            ↑
          </button>
        </div>
        {send.isError && <p className="pt-2 text-attention">{send.error.message}</p>}
      </form>
    </>
  );
}

function Line({ message, author, onPerson }: { message: Message; author?: Staff; onPerson: (id: string) => void }) {
  const time = new Date(message.created_at).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  if (message.kind === "task" || message.kind === "note") {
    return (
      <div className="flex items-center gap-2 pl-10 text-ink-3">
        <span>→</span>
        <span className="grow">{message.text}</span>
        <span className="text-xs">{time}</span>
      </div>
    );
  }
  const mine = message.sender === ENGINEER;
  return (
    <div className="flex gap-3">
      {author ? (
        <button onClick={() => onPerson(author.id)} aria-label={`About ${author.name}`}>
          <Face id={author.id} size={28} />
        </button>
      ) : (
        <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-ink text-xs text-white">
          {mine ? "You" : "?"}
        </span>
      )}
      <div className="flex max-w-[640px] min-w-0 flex-col gap-1">
        <span>
          <span className="font-semibold">{mine ? "You" : author ? firstName(author) : "Office"}</span>{" "}
          <span className="text-ink-3">
            {author?.role ? `${author.role} · ` : ""}
            {time}
          </span>
          {message.kind === "concern" && <span className="text-xs font-medium text-attention"> · raised a concern</span>}
        </span>
        <span className="text-sm leading-relaxed whitespace-pre-wrap text-[#27272A]" dir="auto">
          {message.text}
        </span>
      </div>
    </div>
  );
}

function Profile({ tenderId, member, onMessage }: { tenderId: string; member: Staff; onMessage: () => void }) {
  const tasks = useTasks(tenderId);
  const mine = (tasks.data ?? []).filter((t) => t.staff_id === member.id);
  const p = member.profile as Record<string, string | number | undefined>;
  return (
    <aside aria-label={member.name} className="flex w-[340px] shrink-0 flex-col gap-5 border-l border-line px-6 pt-7 pb-6">
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
              <span className="shrink-0 text-ink-3">{t.status === "done" ? "done" : "working"}</span>
            </span>
          ))}
        </div>
      )}
      <div className="grow" />
      {member.status === "active" && (
        <button onClick={onMessage} className="h-[38px] rounded-lg bg-ink text-sm text-white">
          Message {firstName(member)}
        </button>
      )}
      <Link to={`/tenders/${tenderId}/office`} className="text-center text-ink-3 hover:text-ink">
        Close
      </Link>
    </aside>
  );
}
