import { useState, type FormEvent } from "react";
import { useAddRule, useRemoveRule, useRules } from "./queries";

export function Rules() {
  const rules = useRules();
  const remove = useRemoveRule();
  const rows = rules.data ?? [];
  const topics = [...new Set(rows.map((r) => r.topic))];

  return (
    <div className="flex w-full max-w-[860px] flex-col px-8 pt-7">
      <h1 className="text-[22px] font-semibold tracking-tight">Company rules</h1>
      <span className="text-ink-2">
        How your firm tenders: standard markups, exclusions, qualifications and house style. Every team follows them.
      </span>
      {rows.length === 0 && rules.data && <p className="pt-6 text-ink-2">No rules yet. Add the first one below.</p>}
      {topics.map((topic) => (
        <div key={topic} className="flex flex-col">
          <span className="px-2 pt-6 pb-1 text-xs font-semibold text-ink-2">{topic}</span>
          {rows
            .filter((r) => r.topic === topic)
            .map((r) => (
              <div key={r.id} className="flex items-start gap-4 border-b border-subtle px-2 py-2.5">
                <span className="grow leading-relaxed" dir="auto">
                  {r.text}
                </span>
                <button onClick={() => remove.mutate(r.id)} className="shrink-0 text-ink-3 hover:text-ink">
                  Remove
                </button>
              </div>
            ))}
        </div>
      ))}
      <AddRule topics={topics} />
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
