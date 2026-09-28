import { IconEye, IconEyeOff } from "@tabler/icons-react";
import { useState, type FormEvent, type ReactNode } from "react";
import { Logo } from "../app/Logo";
import { ProviderLogo } from "./ProviderLogo";
import {
  PROVIDERS,
  useAddConnection,
  useCheckModel,
  useConnections,
  useModels,
  useRemoveConnection,
  useSettings,
  useRemoveWebKey,
  useSetWebKey,
  useUpdateSettings,
  useUsage,
  useWebKeys,
  type Connection,
  type Provider,
  type WebService,
} from "./queries";

const input = "h-9 rounded-lg border border-line-strong px-3 text-[13px] outline-none focus:border-ink";
const secondary = "h-9 rounded-lg border border-line-strong bg-white px-3 text-[13px] disabled:text-ink-4";
const primary = "h-9 rounded-lg bg-ink px-4 text-[13px] text-white disabled:bg-line-strong disabled:text-ink-3";

export function Settings() {
  return (
    <div className="flex w-full max-w-[704px] flex-col gap-9 px-8 py-11">
      <h1 className="text-[22px] font-semibold tracking-tight">Settings</h1>
      <OfficeMode />
      <Connections />
      <OfficeAI />
      <WebResearch />
      <Scorecard />
      <Allowance />
      <About />
    </div>
  );
}

const compact = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="pb-1 font-semibold text-ink-2">{title}</h2>
      {children}
    </section>
  );
}

function OfficeMode() {
  const settings = useSettings();
  const update = useUpdateSettings();
  const mode = settings.data?.office_mode;
  const options = [
    {
      id: "engineer" as const,
      title: "Engineer in the loop",
      text: "The office stops at each gate for your approval: scope, method, quantities, rates, subcontracts, final price and release.",
    },
    {
      id: "autonomous" as const,
      title: "Fully autonomous",
      text: "The office approves its own gates and marks each decision for your review. Building and releasing the package still need you.",
    },
  ];
  return (
    <Section title="How the office works">
      {options.map((option) => (
        <label
          key={option.id}
          className={`flex cursor-pointer gap-3 rounded-[10px] border p-3.5 ${mode === option.id ? "border-ink bg-rail" : "border-line"}`}
        >
          <input
            type="radio"
            name="office-mode"
            checked={mode === option.id}
            onChange={() => update.mutate({ office_mode: option.id })}
            className="mt-0.5 accent-ink"
          />
          <span className="flex flex-col gap-0.5">
            <span className="text-sm font-medium">{option.title}</span>
            <span className="leading-normal text-ink-3">{option.text}</span>
          </span>
        </label>
      ))}
    </Section>
  );
}

function Connections() {
  const connections = useConnections();
  return (
    <Section title="AI connections">
      {connections.data?.length === 0 && (
        <p className="text-ink-2">Add an AI connection so the office can start work.</p>
      )}
      {connections.data?.map((connection) => <ConnectionRow key={connection.id} connection={connection} />)}
      <AddConnection />
    </Section>
  );
}

function ConnectionRow({ connection }: { connection: Connection }) {
  const models = useModels(connection.id);
  const check = useCheckModel(connection.id);
  const remove = useRemoveConnection();
  const [model, setModel] = useState("");
  const result = model ? connection.checks[model] : undefined;

  return (
    <div className="flex flex-col gap-2.5 border-t border-subtle py-3">
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-2.5">
          <ProviderLogo provider={connection.provider} size={20} />
          <span className="flex flex-col gap-0.5">
            <span className="font-medium">{connection.label}</span>
            <span className="text-ink-3">Key {connection.key_hint}</span>
          </span>
        </span>
        <button onClick={() => remove.mutate(connection.id)} className="text-ink-3 hover:text-ink">
          Remove
        </button>
      </div>
      <div className="flex items-center gap-2">
        {models.data && models.data.length > 0 ? (
          <select aria-label="Model" value={model} onChange={(e) => setModel(e.target.value)} className={`${input} grow`}>
            <option value="">Choose a model…</option>
            {models.data.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        ) : (
          <input
            aria-label="Model"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder={models.isPending ? "Loading models…" : "Model name"}
            className={`${input} grow`}
          />
        )}
        <button disabled={!model || check.isPending} onClick={() => check.mutate(model)} className={secondary}>
          {check.isPending ? "Checking…" : "Check"}
        </button>
      </div>
      {check.isPending && (
        <p className="text-ink-3">The model is asked to use a tool. Some services take up to a minute to answer.</p>
      )}
      {models.isError && <p className="text-attention">{models.error.message}</p>}
      {check.isError && <p className="text-attention">{check.error.message}</p>}
      {result && !check.isPending && (
        <p className={`flex items-center gap-1.5 ${result.ok ? "text-ink-2" : "text-attention"}`}>
          <span className={`size-1.5 rounded-full ${result.ok ? "bg-approved" : "bg-attention"}`} />
          {result.message}
        </p>
      )}
    </div>
  );
}

function AddConnection() {
  const add = useAddConnection();
  const [provider, setProvider] = useState<Provider>("anthropic");
  const [key, setKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [address, setAddress] = useState("");

  function submit(event: FormEvent) {
    event.preventDefault();
    add.mutate(
      { provider, api_key: key, base_url: provider === "openai_compatible" ? address : null },
      {
        onSuccess: () => {
          setKey("");
          setAddress("");
        },
      },
    );
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-2.5 border-t border-subtle pt-3">
      <div role="radiogroup" aria-label="Service" className="flex flex-wrap gap-1.5">
        {PROVIDERS.map((p) => (
          <button
            key={p.id}
            type="button"
            role="radio"
            aria-checked={provider === p.id}
            onClick={() => setProvider(p.id)}
            className={`flex h-9 items-center gap-2 rounded-lg border px-3 text-[13px] ${provider === p.id ? "border-ink bg-rail font-medium" : "border-line-strong text-ink-2 hover:text-ink"}`}
          >
            <ProviderLogo provider={p.id} />
            {p.label}
          </button>
        ))}
      </div>
      <div className="flex gap-2">
        <div className="relative flex grow">
          <input
            aria-label="API key"
            type={showKey ? "text" : "password"}
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="API key"
            autoComplete="off"
            spellCheck={false}
            className={`${input} grow pr-9`}
          />
          <button
            type="button"
            aria-label={showKey ? "Hide the key" : "Show the key"}
            aria-pressed={showKey}
            onClick={() => setShowKey(!showKey)}
            className="absolute inset-y-0 right-0 flex w-9 items-center justify-center text-ink-3 hover:text-ink"
          >
            {showKey ? <IconEyeOff className="size-4" stroke={1.75} /> : <IconEye className="size-4" stroke={1.75} />}
          </button>
        </div>
      </div>
      {provider === "openai_compatible" && (
        <input
          aria-label="Service address"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          placeholder="https://api.example.com/v1"
          className={input}
        />
      )}
      {add.isError && <p className="text-attention">{add.error.message}</p>}
      <button type="submit" disabled={!key.trim() || add.isPending} className={`${primary} self-start`}>
        {add.isPending ? "Checking the key…" : "Add connection"}
      </button>
    </form>
  );
}

function OfficeAI() {
  const connections = useConnections();
  const settings = useSettings();
  const update = useUpdateSettings();
  const choices = (connections.data ?? []).flatMap((connection) =>
    Object.entries(connection.checks)
      .filter(([, check]) => check.ok)
      .map(([model]) => ({ value: `${connection.id}|${model}`, label: `${model} · ${connection.label}` })),
  );
  const current = settings.data?.office_ai;

  return (
    <Section title="The office’s AI">
      {choices.length === 0 ? (
        <p className="text-ink-2">Check a model above, then choose it here.</p>
      ) : (
        <select
          aria-label="Office AI"
          value={current ? `${current.connection_id}|${current.model}` : ""}
          onChange={(e) => {
            const [connection_id, model] = e.target.value.split("|");
            update.mutate({ office_ai: { connection_id, model } });
          }}
          className={input}
        >
          <option value="" disabled>
            Choose the model the office works with…
          </option>
          {choices.map((choice) => (
            <option key={choice.value} value={choice.value}>
              {choice.label}
            </option>
          ))}
        </select>
      )}
      {update.isError && <p className="text-attention">{update.error.message}</p>}
    </Section>
  );
}

/** How each AI the office worked with has done, from its turns and the reviews of what it filed. */
function Scorecard() {
  const usage = useUsage();
  const models = usage.data?.models ?? [];
  const cell = "py-2 pl-3 text-right";
  return (
    <Section title="How each AI has done">
      {models.length === 0 ? (
        <p className="text-ink-2">Nothing yet. Each turn the office works is counted here.</p>
      ) : (
        <>
          <p className="leading-normal text-ink-3">
            Work accepted counts the records filed on its turns that the Tender Manager or you accepted, out of those
            accepted or sent back.
          </p>
          <table className="w-full text-[13px] tabular-nums">
            <thead className="text-ink-3">
              <tr>
                <th className="py-1.5 text-left font-normal">Model</th>
                <th className="pl-3 text-right font-normal">Turns finished</th>
                <th className="pl-3 text-right font-normal">Calls sent back</th>
                <th className="pl-3 text-right font-normal">Work accepted</th>
                <th className="pl-3 text-right font-normal">Tokens per accepted record</th>
              </tr>
            </thead>
            <tbody>
              {models.map((m) => (
                <tr key={m.model} className="border-t border-subtle">
                  <td className="py-2 break-all">{m.model}</td>
                  <td className={cell}>
                    {m.finished} of {m.turns}
                  </td>
                  <td className={cell}>{m.calls ? `${Math.round((100 * m.calls_sent_back) / m.calls)}%` : "–"}</td>
                  <td className={cell}>{m.accepted + m.sent_back ? `${m.accepted} of ${m.accepted + m.sent_back}` : "–"}</td>
                  <td className={cell}>{m.accepted ? compact.format(m.tokens / m.accepted) : "–"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </Section>
  );
}

/** The most AI tokens one tender's office may use before it pauses, and what each tender has used. */
function WebResearch() {
  const keys = useWebKeys();
  return (
    <Section title="Web research">
      <p className="leading-normal text-ink-3">
        The office looks up prices, suppliers, subcontractors and datasheets on the web, in general words only, never
        the client’s or the project’s name. It works without a key. A free Firecrawl key allows more searches a day; a
        free TinyFish key is the backup when Firecrawl can’t answer.
      </p>
      <WebKeyRow service="firecrawl" label="Firecrawl" note="Optional" hint={keys.data?.firecrawl} />
      <WebKeyRow service="tinyfish" label="TinyFish" note="Backup" hint={keys.data?.tinyfish} />
    </Section>
  );
}

function WebKeyRow(props: { service: WebService; label: string; note: string; hint: string | null | undefined }) {
  const save = useSetWebKey();
  const remove = useRemoveWebKey();
  const [key, setKey] = useState("");

  function submit(event: FormEvent) {
    event.preventDefault();
    save.mutate({ service: props.service, api_key: key }, { onSuccess: () => setKey("") });
  }

  return (
    <div className="flex flex-col gap-2 border-t border-subtle py-3">
      <div className="flex items-center justify-between">
        <span className="flex flex-col gap-0.5">
          <span className="font-medium">{props.label}</span>
          <span className="text-ink-3">{props.hint ? `Key ${props.hint}` : props.note}</span>
        </span>
        {props.hint && (
          <button onClick={() => remove.mutate(props.service)} className="text-ink-3 hover:text-ink">
            Remove
          </button>
        )}
      </div>
      {!props.hint && (
        <form onSubmit={submit} className="flex items-center gap-2">
          <input
            aria-label={`${props.label} key`}
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder="Free key"
            className={`${input} grow`}
          />
          <button aria-label={`Save the ${props.label} key`} disabled={!key.trim() || save.isPending} className={secondary}>
            {save.isPending ? "Checking…" : "Save"}
          </button>
        </form>
      )}
      {save.isError && <p className="text-attention">{save.error.message}</p>}
    </div>
  );
}

function Allowance() {
  const settings = useSettings();
  const update = useUpdateSettings();
  const usage = useUsage();
  const allowance = settings.data?.tender_allowance ?? null;
  const [draft, setDraft] = useState<string | null>(null); // millions of tokens, as typed
  const shown = draft ?? (allowance ? String(allowance / 1_000_000) : "");
  const valid = shown.trim() === "" || Number(shown) > 0;

  function save(event: FormEvent) {
    event.preventDefault();
    const tender_allowance = shown.trim() ? Math.round(Number(shown) * 1_000_000) : null;
    update.mutate({ tender_allowance }, { onSuccess: () => setDraft(null) });
  }

  return (
    <Section title="AI allowance per tender">
      <p className="leading-normal text-ink-3">
        The office pauses on a tender once it has used this many AI tokens, read and written. It checks before each
        turn, so a turn under way finishes. Leave it empty for no limit.
      </p>
      <form onSubmit={save} className="flex items-center gap-2">
        <input
          aria-label="Allowance in million tokens"
          inputMode="decimal"
          value={shown}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="No limit"
          className={`${input} w-28 text-right tabular-nums`}
        />
        <span className="text-ink-2">million tokens</span>
        <button disabled={draft === null || !valid || update.isPending} className={secondary}>
          Save
        </button>
      </form>
      {!valid && <p className="text-attention">Give the allowance as a number of millions, such as 20 or 2.5.</p>}
      {update.isError && <p className="text-attention">{update.error.message}</p>}
      {usage.data?.tenders.map((t) => (
        <p key={t.tender_id} className="flex justify-between border-t border-subtle pt-2 tabular-nums">
          <span>{t.name}</span>
          <span className={allowance && t.tokens >= allowance ? "text-attention" : "text-ink-2"}>
            {compact.format(t.tokens)} used{allowance ? ` of ${compact.format(allowance)}` : ""}
          </span>
        </p>
      ))}
    </Section>
  );
}

/** Who makes Quantix: its mark, the QS Mind house it belongs to, and its founder's credit. */
function About() {
  return (
    <Section title="About Quantix">
      <div className="flex items-center gap-3">
        <Logo tone="brand" className="h-5" />
        <span className="flex flex-col gap-0.5">
          <span className="font-medium">Quantix</span>
          <span className="text-ink-3">Your tendering office, on your desktop. Part of QS Mind.</span>
        </span>
      </div>
      <p className="border-t border-subtle pt-3">
        <a
          href="https://kareemsafwat.com"
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-2.5 text-ink-2 hover:text-ink"
        >
          <FounderMark />
          <span>
            Founded &amp; developed by{" "}
            <span className="font-medium text-ink underline underline-offset-4">Kareem Safwat</span>
          </span>
        </a>
      </p>
    </Section>
  );
}

/** Kareem Safwat's K, as brand/founder/k-mark.svg draws it. */
function FounderMark() {
  return (
    <svg viewBox="0 0 300 300" fill="none" className="size-7 shrink-0" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id="quantix-founder-k" x1="80" y1="70" x2="220" y2="230" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#FEF08A" />
          <stop offset="50%" stopColor="#D97706" />
          <stop offset="100%" stopColor="#78350F" />
        </linearGradient>
      </defs>
      <line x1="80" y1="70" x2="80" y2="230" stroke="url(#quantix-founder-k)" strokeWidth="8" strokeLinecap="square" />
      <line x1="80" y1="150" x2="220" y2="70" stroke="url(#quantix-founder-k)" strokeWidth="8" strokeLinecap="square" />
      <line x1="80" y1="150" x2="220" y2="230" stroke="url(#quantix-founder-k)" strokeWidth="8" strokeLinecap="square" />
    </svg>
  );
}
