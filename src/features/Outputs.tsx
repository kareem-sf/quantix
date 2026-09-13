import { useCallback, useEffect, useId, useState } from "react";
import { FileText } from "lucide-react";
import { tenderPath, useApi, useResource, type Schema } from "../api";
import { ErrorNotice, Loading, Status } from "../components/common";
import { recordRoute } from "../navigation/routes";
import { Button } from "@/components/ui/button";
import { MicroButton } from "@/components/ui/micro-button";
import { NativeSelect } from "@/components/ui/native-select";
import { cn } from "@/lib/utils";
import { OutputForm } from "./OutputForm";
import { Submissions } from "./Submissions";
import { SubmissionRequirements } from "./SubmissionRequirements";
import { Citations, type SourceSelection } from "./Sources";
import { OutputWorkingRecords } from "./OutputWorkingRecords";

export type SubmissionView = "requirements" | "documents" | "package";
export const outputLabels: Record<Schema<"OutputRequest">["kind"], string> = {
  boq_xlsx: "BOQ workbook",
  analysis_docx: "Tender analysis",
  technical_docx: "Technical document",
  registers_xlsx: "Tender registers",
  comparison_xlsx: "Quotation comparison",
  programme_xlsx: "Construction programme",
  client_boq: "Client BOQ copy",
};
function isOutputKind(value: string): value is Schema<"OutputRequest">["kind"] {
  return Object.hasOwn(outputLabels, value);
}

const views = [
  ["requirements", "Requirements"],
  ["documents", "Documents"],
  ["package", "Package"],
] as const;

export function Outputs({
  tenderId,
  view,
  recordId,
  onNavigate,
  onRepair,
  onSource,
}: {
  tenderId: string;
  view?: string;
  recordId?: string;
  onNavigate?: (view: SubmissionView, recordId?: string) => void;
  onRepair?: (path: string) => void;
  onSource?: (source: SourceSelection) => void;
}) {
  const api = useApi(),
    base = tenderPath(tenderId);
  const outputs = useResource<Schema<"OutputRecord">[]>(`${base}/outputs`);
  const [localView, setLocalView] = useState<SubmissionView>("requirements");
  const kindFieldId = useId();
  const activeView =
    view === "requirements" || view === "documents" || view === "package"
      ? view
      : localView;
  const [kind, setKind] = useState<Schema<"OutputRequest">["kind"] | null>(
      null,
    ),
    [error, setError] = useState<unknown>(null),
    [downloading, setDownloading] = useState<string | null>(null);
  const [createKind, setCreateKind] =
    useState<Schema<"OutputRequest">["kind"]>("boq_xlsx");
  const navigate = (next: SubmissionView, id?: string) => {
    setLocalView(next);
    onNavigate?.(next, id);
  };
  useEffect(() => {
    const requested = recordId?.startsWith("new:") ? recordId.slice(4) : "";
    if (isOutputKind(requested))
      setKind(requested as Schema<"OutputRequest">["kind"]);
  }, [recordId]);
  const close = useCallback(() => {
    setKind(null);
    if (recordId?.startsWith("new:")) onNavigate?.("documents");
  }, [recordId, onNavigate]);
  const selectedOutput = outputs.data?.find((output) => output.id === recordId);
  async function download(output: Schema<"OutputRecord">) {
    setDownloading(output.id);
    setError(null);
    try {
      const blob = await api.blob(`${base}/outputs/${output.id}/download`),
        url = URL.createObjectURL(blob),
        link = document.createElement("a");
      link.href = url;
      link.download = output.filename;
      document.body.append(link);
      link.click();
      link.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (failure) {
      setError(failure);
    } finally {
      setDownloading(null);
    }
  }
  return (
    <div className="flex flex-col gap-5">
      <header className="flex max-w-2xl flex-col gap-1">
        <h2 className="text-lg font-semibold tracking-tight">Submission</h2>
        <p className="text-sm text-muted-foreground">
          Review requirements, prepare documents, then approve the exact local
          package.
        </p>
      </header>
      <nav
        aria-label="Submission sections"
        className="flex w-fit max-w-full flex-wrap gap-0.5 rounded-lg bg-muted p-0.5"
      >
        {views.map(([id, label]) => (
          <button
            type="button"
            key={id}
            aria-current={activeView === id ? "page" : undefined}
            className={cn(
              "rounded-md px-3 py-1.5 text-sm transition-colors",
              activeView === id
                ? "bg-background text-foreground shadow-xs"
                : "text-muted-foreground hover:text-foreground",
            )}
            onClick={() => navigate(id)}
          >
            {label}
          </button>
        ))}
      </nav>
      {activeView === "requirements" ? (
        <div className="legacy-screen">
          <SubmissionRequirements
            tenderId={tenderId}
            selectedId={recordId}
            onSelect={(id) => navigate("requirements", id ?? undefined)}
            onNavigate={navigate}
            onSource={onSource}
          />
        </div>
      ) : null}
      {activeView === "documents" ? (
        <section className="flex flex-col gap-4">
          <div className="flex flex-col gap-3 rounded-xl border bg-card p-4">
            <div className="flex flex-col gap-0.5">
              <h3 className="text-sm font-semibold tracking-tight">
                Draft documents
              </h3>
              <p className="text-xs text-muted-foreground">
                Create a review copy from the current Tender records.
              </p>
            </div>
            <div className="flex flex-wrap items-end gap-2">
              <div className="flex min-w-56 flex-1 flex-col gap-1.5">
                <label
                  htmlFor={kindFieldId}
                  className="text-xs text-muted-foreground"
                >
                  Document to create
                </label>
                <NativeSelect
                  id={kindFieldId}
                  value={createKind}
                  onChange={(event) =>
                    setCreateKind(event.target.value as typeof createKind)
                  }
                >
                  <optgroup label="Pricing">
                    <option value="boq_xlsx">BOQ workbook</option>
                    <option value="client_boq">Client BOQ copy</option>
                    <option value="comparison_xlsx">
                      Quotation comparison
                    </option>
                  </optgroup>
                  <optgroup label="Technical and planning">
                    <option value="technical_docx">Technical document</option>
                    <option value="programme_xlsx">
                      Construction programme
                    </option>
                  </optgroup>
                  <optgroup label="Tender review">
                    <option value="analysis_docx">Tender analysis</option>
                    <option value="registers_xlsx">Tender registers</option>
                  </optgroup>
                </NativeSelect>
              </div>
              <Button type="button" onClick={() => setKind(createKind)}>
                Create draft document
              </Button>
            </div>
          </div>

          <ErrorNotice error={outputs.error || error} />
          {outputs.isPending ? (
            <Loading>Loading draft documents…</Loading>
          ) : null}
          {outputs.data?.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No draft documents have been created. Choose the document required
              by the Tender.
            </p>
          ) : null}
          {recordId &&
          !recordId.startsWith("new:") &&
          outputs.data &&
          !selectedOutput ? (
            <p role="alert" className="text-sm text-destructive">
              This document is unavailable. Choose a saved draft below.
            </p>
          ) : null}

          {outputs.data?.map((output) => {
            const open = selectedOutput?.id === output.id;
            return (
              <article
                key={output.id}
                className={cn(
                  "flex flex-col gap-3 rounded-xl border bg-card p-4",
                  open && "border-primary",
                )}
                aria-label={open ? "Selected draft document" : undefined}
              >
                <div className="flex flex-wrap items-start gap-3">
                  <span
                    className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"
                    aria-hidden="true"
                  >
                    <FileText className="size-4" />
                  </span>
                  <div className="flex min-w-0 flex-1 flex-col gap-1">
                    <Button
                      type="button"
                      variant="link"
                      className="h-auto justify-start p-0 text-start font-medium whitespace-normal text-foreground"
                      dir="auto"
                      onClick={() => navigate("documents", output.id)}
                    >
                      {output.filename}
                    </Button>
                    <div className="flex flex-wrap items-center gap-2">
                      <Status value={output.status} />
                      <span className="text-xs text-muted-foreground">
                        {isOutputKind(output.kind)
                          ? outputLabels[output.kind]
                          : output.kind}{" "}
                        · {new Date(output.created_at).toLocaleString()}
                      </span>
                    </div>
                    {!open && output.blocking_reasons.length ? (
                      <p className="text-xs text-muted-foreground">
                        {output.blocking_reasons.length} item(s) need review.
                      </p>
                    ) : null}
                  </div>
                  <MicroButton
                    kind="download"
                    type="button"
                    disabled={downloading === output.id}
                    onClick={() => void download(output)}
                  >
                    {downloading === output.id
                      ? "Downloading…"
                      : "Download draft"}
                  </MicroButton>
                </div>

                {open ? (
                  <div className="quantix-reveal flex flex-col gap-3 border-t pt-3">
                    <p className="text-xs text-muted-foreground">
                      This saved draft stays unchanged. Create an updated copy
                      after repairing its working records.
                    </p>
                    {output.blocking_reasons.map((reason) => (
                      <p
                        className="text-sm text-amber-700 dark:text-amber-400"
                        key={reason}
                      >
                        {reason}
                      </p>
                    ))}
                    <div className="flex flex-wrap items-center gap-1.5">
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={!isOutputKind(output.kind)}
                        onClick={() => {
                          if (isOutputKind(output.kind)) setKind(output.kind);
                        }}
                      >
                        Create updated draft
                      </Button>
                      {typeof output.metadata?.task_id === "string" &&
                      onRepair ? (
                        <Button
                          type="button"
                          variant="ghost"
                          size="sm"
                          onClick={() =>
                            onRepair(
                              recordRoute(tenderId, "work", {
                                view: "tasks",
                                recordId: String(output.metadata?.task_id),
                              }),
                            )
                          }
                        >
                          Open linked task
                        </Button>
                      ) : null}
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => navigate("package")}
                      >
                        Review package selection
                      </Button>
                    </div>
                    {onSource ? (
                      <Citations
                        tenderId={tenderId}
                        ids={output.source_ids}
                        onOpen={onSource}
                      />
                    ) : null}
                    {onRepair ? (
                      <div className="legacy-screen">
                        <OutputWorkingRecords
                          tenderId={tenderId}
                          output={output}
                          onRepair={onRepair}
                        />
                      </div>
                    ) : null}
                  </div>
                ) : null}
              </article>
            );
          })}

          {kind ? (
            <OutputForm
              key={kind}
              tenderId={tenderId}
              kind={kind}
              onClose={close}
              onCreated={(output) => {
                setKind(null);
                navigate("documents", output.id);
              }}
              onTask={
                onRepair
                  ? (id) =>
                      onRepair(
                        recordRoute(tenderId, "work", {
                          view: "tasks",
                          recordId: id,
                        }),
                      )
                  : undefined
              }
            />
          ) : null}
        </section>
      ) : null}
      {activeView === "package" ? (
        <div className="legacy-screen">
          <Submissions
            tenderId={tenderId}
            outputs={outputs.data ?? []}
            recordId={recordId}
            onReviewChange={(open) =>
              navigate("package", open ? "review" : undefined)
            }
            onRepair={onRepair}
          />
        </div>
      ) : null}
    </div>
  );
}
