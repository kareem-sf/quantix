import type { Schema } from "../api";
import { CalculationInspector } from "./CalculationInspector";

type Product = Schema<"WorkProductVersion">;
type RowPage = Schema<"WorkProductRowPage">;

export function WorkProductView({
  product,
  rows,
  tenderId,
  onSource,
}: {
  product: Product;
  rows: RowPage;
  tenderId: string;
  onSource: (sourceId: string) => void;
}) {
  const pageRows = rows.items ?? [];
  const chart = safeChart(product, pageRows);
  return (
    <div className="flex flex-col gap-4">
      {product.sanitized_content ? (
        <p className="whitespace-pre-wrap text-sm" dir="auto">
          {product.sanitized_content}
        </p>
      ) : null}
      {chart ? (
        <div className="flex flex-col gap-2">
          <SafeChart title={product.title} {...chart} />
          {pageRows.length > 20 ? (
            <details className="text-xs">
              <summary className="cursor-pointer">More chart rows</summary>
              <SafeTable rows={pageRows} />
            </details>
          ) : null}
        </div>
      ) : (
        <SafeTable rows={pageRows} />
      )}
      {rows.total ? (
        <p className="text-xs text-muted-foreground">
          Showing {pageRows.length.toLocaleString()} of{" "}
          {rows.total.toLocaleString()} rows.
        </p>
      ) : null}
      {product.source_refs?.length ? (
        <section className="flex flex-col gap-2" aria-label="Product sources">
          <h4 className="text-xs font-medium">Sources</h4>
          <div className="flex flex-wrap gap-2">
            {(product.source_refs ?? []).map((reference, index) => (
              <button
                type="button"
                className="text-button"
                key={reference}
                aria-label={`Open source ${reference}`}
                onClick={() => onSource(reference)}
              >
                Source {index + 1}
              </button>
            ))}
          </div>
        </section>
      ) : null}
      <details className="text-xs">
        <summary className="cursor-pointer">Exact version proof</summary>
        <dl className="mt-2 grid gap-1">
          <Proof label="Version ID" value={product.id} />
          <Proof label="Content SHA-256" value={product.sha256} />
          <Proof label="Basis" value={product.basis} />
          <Proof label="Author" value={product.author} />
          <Proof
            label="Source references"
            value={(product.source_refs ?? []).join(" · ") || "None"}
          />
          <Proof
            label="Method references"
            value={(product.method_refs ?? []).join(" · ") || "None"}
          />
          {(product.method_refs ?? []).map((reference, index) => (
            <CalculationInspector
              key={reference}
              tenderId={tenderId}
              calculationId={reference}
              label={`Method reference ${index + 1}`}
            />
          ))}
        </dl>
      </details>
    </div>
  );
}

function SafeTable({ rows }: { rows: Record<string, unknown>[] }) {
  if (!rows.length)
    return (
      <p className="text-sm text-muted-foreground">No table rows saved.</p>
    );
  const allColumns = [...new Set(rows.flatMap((row) => Object.keys(row)))];
  const columns = allColumns.filter(validFieldName).slice(0, 20);
  const omitted = allColumns.filter((column) => !columns.includes(column));
  const hasComplex = rows.some((row) =>
    columns.some((column) => !isScalar(row[column])),
  );
  if (!columns.length)
    return (
      <div className="flex flex-col gap-2">
        <p className="text-sm text-muted-foreground">
          This draft has no displayable scalar columns.
        </p>
        <RawRows rows={rows} omitted={omitted.length} />
      </div>
    );
  return (
    <div className="flex flex-col gap-2">
      <div className="max-w-full overflow-x-auto">
        <table className="w-full text-left text-xs">
          <thead>
            <tr>
              {columns.map((column) => (
                <th className="border-b p-2" key={column}>
                  {column}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, index) => (
              <tr key={index}>
                {columns.map((column) => (
                  <td className="border-b p-2" key={column}>
                    {scalar(row[column])}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {omitted.length || hasComplex ? (
        <RawRows rows={rows} omitted={omitted.length} />
      ) : null}
    </div>
  );
}

function RawRows({
  rows,
  omitted,
}: {
  rows: Record<string, unknown>[];
  omitted: number;
}) {
  return (
    <details className="text-xs">
      <summary className="cursor-pointer">More row data (JSON)</summary>
      <p className="mt-1 text-muted-foreground">
        {omitted
          ? `${omitted} additional or unsupported fields are preserved below.`
          : "Nested values are preserved below."}
      </p>
      <pre className="mt-2 max-h-80 overflow-auto whitespace-pre-wrap rounded bg-muted p-2">
        {JSON.stringify(rows, null, 2)}
      </pre>
    </details>
  );
}

function SafeChart({
  title,
  labels,
  values,
}: {
  title: string;
  labels: string[];
  values: number[];
}) {
  const maximum = Math.max(1, ...values.map((value) => Math.abs(value)));
  return (
    <div
      className="space-y-2 rounded-lg border p-3"
      role="img"
      aria-label={`${title} chart`}
    >
      {labels.slice(0, 20).map((label, index) => (
        <div
          className="grid grid-cols-[minmax(6rem,1fr)_3fr_auto] items-center gap-2"
          key={`${label}:${index}`}
        >
          <span className="truncate text-xs">{label}</span>
          <span className="h-2 overflow-hidden rounded bg-muted">
            <span
              className="block h-full rounded bg-primary"
              style={{
                width: `${Math.max(2, (Math.abs(values[index]) / maximum) * 100)}%`,
              }}
            />
          </span>
          <span className="text-xs tabular-nums">{values[index]}</span>
        </div>
      ))}
    </div>
  );
}

function safeChart(product: Product, rows: Record<string, unknown>[]) {
  if (product.kind !== "chart" || !rows.length) return null;
  const category = product.view_schema.category_field;
  const value = product.view_schema.value_field;
  if (
    typeof category !== "string" ||
    typeof value !== "string" ||
    !validFieldName(category) ||
    !validFieldName(value)
  )
    return null;
  const points = rows.map((row) => ({
    label: scalar(row[category]),
    value: row[value],
  }));
  if (
    points.some(
      (point) =>
        !point.label ||
        typeof point.value !== "number" ||
        !Number.isFinite(point.value) ||
        point.value < 0,
    )
  )
    return null;
  return {
    labels: points.map((point) => point.label),
    values: points.map((point) => point.value as number),
  };
}

function validFieldName(value: string) {
  return /^[\p{L}\p{N}][\p{L}\p{N} _.-]{0,79}$/u.test(value);
}

function scalar(value: unknown) {
  return typeof value === "string" || typeof value === "number"
    ? String(value)
    : typeof value === "boolean"
      ? value
        ? "Yes"
        : "No"
      : "";
}

function isScalar(value: unknown) {
  return (
    value == null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  );
}

function Proof({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-muted-foreground">{label}</dt>
      <dd>
        <code className="wrap-anywhere">{value}</code>
      </dd>
    </div>
  );
}
