import { useState, type FormEvent, type ReactNode } from "react";
import {
  PROVIDERS,
  useAddConnection,
  useCheckModel,
  useConnections,
  useModels,
  useRemoveConnection,
  useSettings,
  useUpdateSettings,
  type Connection,
  type Provider,
} from "./queries";

const input = "h-9 rounded-lg border border-line-strong px-3 text-[13px] outline-none focus:border-ink";
const secondary = "h-9 rounded-lg border border-line-strong bg-white px-3 text-[13px] disabled:text-ink-4";
const primary = "h-9 rounded-lg bg-ink px-4 text-[13px] text-white disabled:bg-line-strong disabled:text-ink-3";

export function Settings() {
  return (
    <div className="flex w-[640px] flex-col gap-9 py-11">
      <h1 className="text-[22px] font-semibold tracking-tight">Settings</h1>
      <OfficeMode />
      <Connections />
      <OfficeAI />
    </div>
  );
}

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
        <span className="flex flex-col gap-0.5">
          <span className="font-medium">{connection.label}</span>
          <span className="text-ink-3">Key {connection.key_hint}</span>
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
      <div className="flex gap-2">
        <select
          aria-label="Service"
          value={provider}
          onChange={(e) => setProvider(e.target.value as Provider)}
          className={input}
        >
          {PROVIDERS.map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        <input
          aria-label="API key"
          type="password"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="API key"
          autoComplete="off"
          className={`${input} grow`}
        />
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
