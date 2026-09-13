import { useState } from "react";
import { errorText, tenderPath, useApi, type Schema } from "../api";

export function CalculationInspector({
  tenderId,
  calculationId,
  label,
  initiallyOpen = false,
}: {
  tenderId: string;
  calculationId: string;
  label: string;
  initiallyOpen?: boolean;
}) {
  const api = useApi();
  const [record, setRecord] = useState<Schema<"CalculationRecord"> | null>(
    null,
  );
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>(null);

  async function load() {
    if (record || loading) return;
    setLoading(true);
    setError(null);
    try {
      setRecord(
        await api.get<Schema<"CalculationRecord">>(
          `${tenderPath(tenderId)}/calculations/${encodeURIComponent(calculationId)}`,
        ),
      );
    } catch (failure) {
      setError(failure);
    } finally {
      setLoading(false);
    }
  }

  return (
    <details
      className="mt-2 rounded border p-2"
      open={initiallyOpen || undefined}
      onToggle={(event) => {
        if (event.currentTarget.open) void load();
      }}
    >
      <summary className="cursor-pointer">{label}</summary>
      <p className="mt-2 text-muted-foreground">
        Reference: <code className="wrap-anywhere">{calculationId}</code>
      </p>
      {loading ? <p role="status">Loading saved calculation…</p> : null}
      {error ? (
        <p role="alert" className="text-destructive">
          {errorText(error)}
        </p>
      ) : null}
      {record ? <CalculationRecordView record={record} /> : null}
    </details>
  );
}

function CalculationRecordView({
  record,
}: {
  record: Schema<"CalculationRecord">;
}) {
  return (
    <div className="mt-3 flex flex-col gap-3">
      <dl className="grid gap-2 sm:grid-cols-2">
        <Fact
          label="Method"
          value={`${record.method_id} · v${record.method_version}`}
        />
        <Fact label="Status" value={record.status} />
        <Fact label="Precision" value={record.precision} />
        <Fact label="Rounding" value={record.rounding} />
      </dl>
      {(record.limitations ?? []).map((limitation) => (
        <p role="alert" className="text-destructive" key={limitation}>
          {limitation}
        </p>
      ))}
      <JsonBlock label="Typed inputs" value={record.typed_inputs} />
      <JsonBlock label="Units" value={record.units} />
      <JsonBlock label="Outputs" value={record.outputs} />
      <section>
        <h5 className="font-medium">Assumptions</h5>
        {record.assumptions?.length ? (
          <ul className="list-disc pl-5">
            {(record.assumptions ?? []).map((item, index) => (
              <li key={`${index}:${item}`}>{item}</li>
            ))}
          </ul>
        ) : (
          <p className="text-muted-foreground">No assumptions recorded.</p>
        )}
      </section>
      <dl className="grid gap-1">
        <Fact label="Formula SHA-256" value={record.formula_hash} />
        <Fact label="Basis SHA-256" value={record.basis_fingerprint} />
        <Fact label="Saved" value={formatDate(record.created_at)} />
      </dl>
    </div>
  );
}

function JsonBlock({ label, value }: { label: string; value: unknown }) {
  return (
    <details>
      <summary className="cursor-pointer font-medium">{label}</summary>
      <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap rounded bg-muted p-2">
        {JSON.stringify(value, null, 2)}
      </pre>
    </details>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="wrap-anywhere">{value}</dd>
    </div>
  );
}

function formatDate(value: string) {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}
