import { useState, type FormEvent } from "react";
import { useDecideLesson, useSuggestedLessons } from "../review/queries";
import { useAddRule, useChangeRule, useRemoveRule, useRules, type Rule } from "./queries";

export function Rules() {
  const rules = useRules();
  const remove = useRemoveRule();
  const rows = rules.data ?? [];
  const topics = [...new Set(rows.map((r) => r.topic))];

  return (
    <div className="flex w-full max-w-[860px] flex-col px-8 pt-7">
      <h1 className="text-[24px] font-semibold tracking-tight">Company rules</h1>
      <span className="text-ink-2">
        How your firm tenders: standard markups, exclusions, qualifications and house style. Every team follows them.
      </span>
      {rows.some((r) => r.example) && (
        <p className="pt-2 text-ink-3">
          Rules marked Example are where Quantix starts any firm, in any country. Adjust them to how yours tenders, or
          remove them.
        </p>
      )}
      <Suggested />
      {rows.length === 0 && rules.data && <p className="pt-6 text-ink-2">No rules yet. Add the first one below.</p>}
      {topics.map((topic) => (
        <div key={topic} className="flex flex-col">
          <span className="px-2 pt-6 pb-1 text-xs font-semibold text-ink-2">{topic}</span>
          {rows
            .filter((r) => r.topic === topic)
            .map((r) => (
              <RuleRow key={r.id} rule={r} onRemove={() => remove.mutate(r.id)} />
            ))}
        </div>
      ))}
      <AddRule topics={topics} />
    </div>
  );
}

/** One rule: read, adjusted in place, or removed. */
function RuleRow({ rule, onRemove }: { rule: Rule; onRemove: () => void }) {
  const change = useChangeRule();
  const [draft, setDraft] = useState<string | null>(null); // the text being edited
  if (draft !== null)
    return (
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (draft.trim()) change.mutate({ id: rule.id, topic: rule.topic, text: draft }, { onSuccess: () => setDraft(null) });
        }}
        className="flex flex-col gap-2 border-b border-subtle px-2 py-2.5"
      >
        <textarea
          aria-label="Edit the rule"
          autoFocus
          rows={3}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          className="rounded-lg border border-line-strong px-3 py-2 leading-relaxed outline-none focus:border-ink"
          dir="auto"
        />
        <span className="flex gap-2">
          <button disabled={!draft.trim() || change.isPending} className="h-8 rounded-lg bg-ink px-3 text-white">
            Save
          </button>
          <button type="button" onClick={() => setDraft(null)} className="h-8 rounded-lg px-3 text-ink-2 hover:text-ink">
            Cancel
          </button>
        </span>
        {change.isError && <p className="text-attention">{change.error.message}</p>}
      </form>
    );
  return (
    <div className="flex items-start gap-4 border-b border-subtle px-2 py-2.5">
      <span className="grow leading-relaxed" dir="auto">
        {rule.text}
        {rule.example && (
          <span className="ml-2 rounded-md bg-selected px-1.5 py-px align-[1px] text-[11px] text-ink-3">Example</span>
        )}
      </span>
      <span className="flex shrink-0 gap-3">
        <button onClick={() => setDraft(rule.text)} className="text-ink-3 hover:text-ink">
          Edit
        </button>
        <button onClick={onRemove} className="text-ink-3 hover:text-ink">
          Remove
        </button>
      </span>
    </div>
  );
}

/** What the office learned from work that needed correcting, on any tender: the team on that tender follows it
 * already; kept, it becomes a company rule every later team follows. */
function Suggested() {
  const lessons = useSuggestedLessons();
  const decide = useDecideLesson();
  if (!lessons.data?.length) return null;
  return (
    <div className="mt-6 flex flex-col rounded-xl border border-line-strong px-4 pt-3 pb-1">
      <span className="font-semibold">Suggested by the office</span>
      <span className="pb-2 text-ink-3">
        {lessons.data.length} {lessons.data.length === 1 ? "lesson" : "lessons"} from work that needed correcting. Keep
        one and every later tender follows it.
      </span>
      {lessons.data.map((lesson) => (
        <div key={lesson.id} className="flex items-start gap-4 border-t border-subtle py-2.5">
          <span className="flex min-w-0 grow flex-col gap-0.5">
            <span className="leading-relaxed" dir="auto">
              {lesson.text}
            </span>
            <span className="text-xs text-ink-3">
              {lesson.topic} · from {lesson.source}
              {lesson.tender_name && ` on ${lesson.tender_name}`}
            </span>
          </span>
          <span className="flex shrink-0 gap-1.5">
            <button
              onClick={() => decide.mutate({ id: lesson.id, status: "kept" })}
              disabled={decide.isPending}
              className="h-7 rounded-md border border-line-strong bg-white px-2.5 hover:bg-rail"
            >
              Keep as a rule
            </button>
            <button
              onClick={() => decide.mutate({ id: lesson.id, status: "dropped" })}
              disabled={decide.isPending}
              className="h-7 rounded-md px-2.5 text-ink-3 hover:bg-selected hover:text-ink"
            >
              Drop
            </button>
          </span>
        </div>
      ))}
      {decide.isError && <p className="pb-2 text-attention">{decide.error.message}</p>}
    </div>
  );
}

function AddRule({ topics }: { topics: string[] }) {
  const add = useAddRule();
  const [topic, setTopic] = useState("");
  const [text, setText] = useState("");
  const field = "rounded-lg border border-line-strong px-3 outline-none focus:border-ink";

  function submit(event: FormEvent) {
    event.preventDefault();
    add.mutate({ topic, text }, { onSuccess: () => setText("") });
  }

  return (
    <form onSubmit={submit} className="mt-6 flex flex-col gap-2 border-t border-line pt-5">
      <h2 className="font-semibold text-ink-2">Add a rule</h2>
      <input
        aria-label="Topic"
        required
        list="rule-topics"
        placeholder="Topic, e.g. Markups"
        value={topic}
        onChange={(e) => setTopic(e.target.value)}
        className={`${field} h-9 w-64`}
      />
      <datalist id="rule-topics">
        {[...new Set([...topics, "Markups", "Exclusions", "Qualifications", "House style"])].map((t) => (
          <option key={t} value={t} />
        ))}
      </datalist>
      <textarea
        aria-label="Rule"
        required
        rows={3}
        placeholder="For example: Overheads 5% and profit 7% unless the engineer says otherwise."
        value={text}
        onChange={(e) => setText(e.target.value)}
        className={`${field} py-2`}
      />
      <button className="h-9 self-start rounded-lg bg-ink px-4 text-white">Add</button>
      {add.isError && <p className="text-attention">{add.error.message}</p>}
    </form>
  );
}
