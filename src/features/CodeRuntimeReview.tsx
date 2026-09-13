import type { Schema } from "../api";

type CodeRuntime = Schema<"ReviewedCodeRuntime">;

const runtimeCopy: Record<
  CodeRuntime["engine"],
  { title: string; detail: string; tool: string }
> = {
  monty: {
    title: "Monty calculation sandbox",
    detail: "Runs small Python calculations without network or file access.",
    tool: "execute_tool_code",
  },
  python: {
    title: "Isolated Python workspace",
    detail:
      "Runs reviewed analysis methods in the exact local container image.",
    tool: "execute_python_analysis",
  },
};

export function CodeRuntimeReview({
  options,
  value,
  disabled,
  onChange,
}: {
  options: CodeRuntime[];
  value: CodeRuntime[];
  disabled: boolean;
  onChange: (value: CodeRuntime[], requiredToolIds: string[]) => void;
}) {
  const candidates = [
    ...options.map(
      (option) =>
        value.find((selected) => selected.engine === option.engine) ?? option,
    ),
    ...value.filter(
      (selected) =>
        !options.some((option) => option.engine === selected.engine),
    ),
  ];
  if (!candidates.length) return null;

  function toggle(candidate: CodeRuntime) {
    const selected = value.some((item) => item.engine === candidate.engine);
    onChange(
      selected
        ? value.filter((item) => item.engine !== candidate.engine)
        : [...value, candidate],
      selected ? [] : [runtimeCopy[candidate.engine].tool],
    );
  }

  function update(candidate: CodeRuntime, change: Partial<CodeRuntime>) {
    onChange(
      value.map((item) =>
        item.engine === candidate.engine ? { ...item, ...change } : item,
      ),
      [],
    );
  }

  return (
    <section
      className="delegation-list-section"
      aria-labelledby="delegation-code-runtimes-title"
    >
      <h3 id="delegation-code-runtimes-title">Calculation workspaces</h3>
      <p className="field-help">
        Select an exact reviewed runtime. Selection allows its calculation tool
        within this plan; it does not start work or approve results.
      </p>
      <div className="delegation-check-list">
        {candidates.map((candidate) => {
          const selected = value.find(
            (item) => item.engine === candidate.engine,
          );
          const current = selected ?? candidate;
          const ceiling =
            options.find(
              (option) =>
                option.engine === candidate.engine &&
                option.fingerprint === candidate.fingerprint &&
                option.version === candidate.version &&
                option.image_id === candidate.image_id,
            ) ?? candidate;
          const copy = runtimeCopy[candidate.engine];
          return (
            <fieldset
              key={candidate.engine}
              className={`delegation-tool ${selected ? "selected" : ""}`}
            >
              <legend className="sr-only">{copy.title}</legend>
              <label>
                <input
                  type="checkbox"
                  checked={Boolean(selected)}
                  disabled={disabled}
                  onChange={() => toggle(candidate)}
                />
                <span>
                  <strong>{copy.title}</strong>
                  <span>{copy.detail}</span>
                  <small>Version {current.version}</small>
                  <small>
                    {current.seconds} seconds · {current.memory_mib} MiB ·{" "}
                    {current.cpus} CPU · {current.output_mib} MiB output ·{" "}
                    {current.tool_calls} calls
                  </small>
                </span>
              </label>
              <details className="delegation-route-advanced">
                <summary>More options</summary>
                {selected ? (
                  <div className="grid gap-2 sm:grid-cols-2">
                    <Limit
                      label="Maximum run time"
                      value={current.seconds}
                      min={1}
                      max={ceiling.seconds}
                      disabled={disabled}
                      onChange={(seconds) => update(candidate, { seconds })}
                    />
                    <Limit
                      label="Maximum memory (MiB)"
                      value={current.memory_mib}
                      min={16}
                      max={ceiling.memory_mib}
                      disabled={disabled}
                      onChange={(memory_mib) =>
                        update(candidate, { memory_mib })
                      }
                    />
                    <Limit
                      label="Maximum CPUs"
                      value={current.cpus}
                      min={1}
                      max={ceiling.cpus}
                      disabled={disabled}
                      onChange={(cpus) => update(candidate, { cpus })}
                    />
                    <Limit
                      label="Maximum processes"
                      value={current.processes}
                      min={1}
                      max={ceiling.processes}
                      disabled={disabled}
                      onChange={(processes) => update(candidate, { processes })}
                    />
                    <Limit
                      label="Maximum output (MiB)"
                      value={current.output_mib}
                      min={1}
                      max={ceiling.output_mib}
                      disabled={disabled}
                      onChange={(output_mib) =>
                        update(candidate, { output_mib })
                      }
                    />
                    <Limit
                      label="Maximum tool calls"
                      value={current.tool_calls}
                      min={0}
                      max={ceiling.tool_calls}
                      disabled={disabled}
                      onChange={(tool_calls) =>
                        update(candidate, { tool_calls })
                      }
                    />
                  </div>
                ) : null}
                <dl className="mt-2 grid gap-1 text-xs">
                  <Proof
                    label="Runtime fingerprint"
                    value={candidate.fingerprint}
                  />
                  {candidate.image_id ? (
                    <Proof label="Container image" value={candidate.image_id} />
                  ) : null}
                  <Proof
                    label="Libraries"
                    value={
                      Object.entries(candidate.library_versions ?? {})
                        .map(([name, version]) => `${name} ${version}`)
                        .join(" · ") || "None recorded"
                    }
                  />
                  <Proof
                    label="Fixed ceilings"
                    value={`${candidate.cpus} CPU · ${candidate.output_mib} MiB output`}
                  />
                </dl>
              </details>
            </fieldset>
          );
        })}
      </div>
    </section>
  );
}

function Limit({
  label,
  value,
  min,
  max,
  disabled,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  disabled: boolean;
  onChange: (value: number) => void;
}) {
  return (
    <label className="text-xs">
      {label}
      <input
        key={value}
        type="number"
        defaultValue={value}
        min={min}
        max={max}
        step={1}
        disabled={disabled}
        onBlur={(event) => {
          const next = Number(event.currentTarget.value);
          if (Number.isFinite(next))
            onChange(Math.min(max, Math.max(min, Math.trunc(next))));
        }}
      />
    </label>
  );
}

function Proof({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-muted-foreground">{label}</dt>
      <dd>
        <code className="wrap-anywhere">{value}</code>
      </dd>
    </div>
  );
}
