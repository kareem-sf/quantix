import { useQuery } from "@tanstack/react-query";
import { tenderPath, useApi, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { recordRoute, tenderRoute } from "../navigation/routes";

/** Follow persisted document kind and current record state, never error wording. */
export function OutputWorkingRecords({
  tenderId,
  output,
  onRepair,
}: {
  tenderId: string;
  output: Schema<"OutputRecord">;
  onRepair: (path: string) => void;
}) {
  const api = useApi(),
    base = tenderPath(tenderId);
  const hasEstimate = ["boq_xlsx", "analysis_docx", "client_boq"].includes(
    output.kind,
  );
  const hasFindings = ["analysis_docx", "registers_xlsx"].includes(output.kind);
  const estimate = useQuery({
    queryKey: [`${base}/estimate`],
    queryFn: ({ signal }) =>
      api.get<Schema<"EstimateView">>(`${base}/estimate`, signal),
    enabled: hasEstimate,
  });
  const findings = useQuery({
    queryKey: [`${base}/findings`],
    queryFn: ({ signal }) =>
      api.get<Schema<"Finding">[]>(`${base}/findings`, signal),
    enabled: hasFindings,
  });
  const items =
    estimate.data?.items.filter(
      (item) =>
        !item.confirmed ||
        item.unit_rate === null ||
        item.effective_quantity === null ||
        item.tax_basis === "unknown" ||
        item.vat_percent === null,
    ) ?? [];
  const decisions =
    findings.data?.filter(
      (finding) =>
        finding.state === "proposed" &&
        ["assumption", "exclusion"].includes(finding.kind),
    ) ?? [];
  return (
    <section className="commercial-working-records">
      <ErrorNotice error={estimate.error || findings.error} />
      {(hasEstimate && estimate.isPending) ||
      (hasFindings && findings.isPending) ? (
        <Loading>Checking working records…</Loading>
      ) : null}
      {estimate.data?.refresh_required ? (
        <p className="field-help">
          The source register changed.{" "}
          <button
            type="button"
            className="text-button"
            onClick={() =>
              onRepair(`${tenderRoute(tenderId, "estimate")}?view=boq`)
            }
          >
            Refresh BOQ source rows
          </button>
        </p>
      ) : null}
      {items.length || decisions.length ? (
        <>
          <h4>Working records requiring review</h4>
          <ul>
            {items.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  className="text-button"
                  onClick={() =>
                    onRepair(
                      recordRoute(tenderId, "estimate", {
                        view: "boq",
                        recordId: item.id,
                      }),
                    )
                  }
                >
                  {item.description}
                </button>
                <span className="field-help">
                  Source, quantity, rate or VAT decision is incomplete.
                </span>
              </li>
            ))}
            {decisions.map((finding) => (
              <li key={finding.id}>
                <button
                  type="button"
                  className="text-button"
                  onClick={() =>
                    onRepair(
                      recordRoute(tenderId, "work", {
                        view: "finding",
                        recordId: finding.id,
                      }),
                    )
                  }
                >
                  {finding.title}
                </button>
                <span className="field-help">
                  {finding.kind === "assumption" ? "Assumption" : "Exclusion"}{" "}
                  awaiting an engineer decision.
                </span>
              </li>
            ))}
          </ul>
        </>
      ) : null}
      {output.kind === "comparison_xlsx" ? (
        <button
          type="button"
          className="text-button"
          onClick={() =>
            onRepair(`${tenderRoute(tenderId, "estimate")}?view=proposals`)
          }
        >
          Review supplier rate proposals
        </button>
      ) : null}
    </section>
  );
}
