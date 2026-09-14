import { useEffect, useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeft,
  FileText,
  Settings2,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import { tenderPath, useApi, type Schema } from "../../api";
import { ErrorNotice, Loading, Status } from "../../components/common";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Manager } from "../Manager";
import { Citations, type SourceSelection } from "../Sources";
import { tenderRoute } from "@/navigation/routes";
import { RightWorkspace } from "./RightWorkspace";
import { tenderDisplayName } from "../../app/tender-name";
import { Team } from "../Team";
import { WorkProductLibrary } from "../WorkProductLibrary";
import { CalculationInspector } from "../CalculationInspector";
import { sourceSelectionKey, useWorkspaceState } from "./workspace-state";
import {
  WorkspaceActivity,
  WorkspaceContext,
  WorkspaceDocuments,
  WorkspaceReviews,
} from "./WorkspaceViews";

export type TenderOfficeWorkspaceProps = {
  overview: Schema<"Overview">;
  artifacts: Schema<"Artifact">[];
  settings?: Schema<"Settings">;
  sourceSelection?: SourceSelection | null;
  recordView?: string;
  recordId?: string;
  onCloseSource?: () => void;
  onImport: () => void;
  onSettings: () => void;
  onSource: (source: SourceSelection) => void;
  onSourceSelectionChange?: (source: SourceSelection) => void;
  onDocuments?: () => void;
  onReviewDocuments?: () => void | Promise<void>;
  onRecord?: (view: string, recordId: string) => void;
  onRepair?: (target: string) => void;
  onCustomizeManager?: () => void;
};

/** Keep a Tender's workbench isolated while preserving its Manager during layout changes. */
export function TenderOfficeWorkspace(props: TenderOfficeWorkspaceProps) {
  return <TenderWorkspaceContent key={props.overview.tender.id} {...props} />;
}

function TenderWorkspaceContent({
  overview,
  artifacts,
  settings,
  sourceSelection = null,
  recordView,
  recordId,
  onCloseSource,
  onImport,
  onSettings,
  onSource,
  onSourceSelectionChange,
  onDocuments,
  onReviewDocuments,
  onRecord,
  onRepair,
  onCustomizeManager,
}: TenderOfficeWorkspaceProps) {
  const workspace = useWorkspaceState();
  const chatRef = useRef<HTMLElement>(null);
  const openingDocumentList = useRef(false);
  const selectedResult =
    recordId && isResultView(recordView)
      ? { id: recordId, view: recordView }
      : null;
  const sourceKey = sourceSelection ? sourceSelectionKey(sourceSelection) : "";
  const resultKey = recordId ? recordView + ":" + recordId : "";
  const open = workspace.open;
  useEffect(() => {
    if (!sourceKey && openingDocumentList.current) {
      openingDocumentList.current = false;
      open("documents");
    } else if (sourceKey) open("documents");
    else if (resultKey) open("reviews");
  }, [sourceKey, resultKey, open]);
  function showSource(selection: SourceSelection) {
    open("documents");
    onSource(selection);
  }
  function showRecord(view: string, id: string) {
    open("reviews");
    onRecord?.(view, id);
  }
  function reviewOverview() {
    onRepair?.(tenderRoute(overview.tender.id, "manager"));
  }
  function showDocumentList() {
    openingDocumentList.current = Boolean(sourceKey);
    onCloseSource?.();
    open("documents");
  }
  function returnToConversation() {
    reviewOverview();
    workspace.hide();
    requestAnimationFrame(() =>
      chatRef.current
        ?.querySelector<HTMLElement>('[aria-label="Message to Tender Manager"]')
        ?.focus(),
    );
  }
  const actions = [
    ...(onCustomizeManager
      ? [
          {
            label: "Customize Manager",
            icon: Sparkles,
            onSelect: onCustomizeManager,
          },
        ]
      : []),
    {
      label: "AI accounts and settings",
      icon: Settings2,
      onSelect: onSettings,
    },
    ...(onDocuments
      ? [
          {
            label: "Full document register",
            icon: FileText,
            onSelect: onDocuments,
          },
        ]
      : []),
  ];
  const chat = (
    <section
      ref={chatRef}
      role="group"
      aria-label="Manager conversation"
      className="flex min-h-0 min-w-0 flex-1 flex-col"
    >
      <Manager
        overview={overview}
        artifacts={artifacts}
        settings={settings}
        onImport={onImport}
        onSettings={onSettings}
        onSource={showSource}
        onDocuments={() => open("documents")}
        onReviewDocuments={onReviewDocuments}
        onRecord={showRecord}
        onRepair={onRepair}
      />
    </section>
  );

  return (
    <RightWorkspace
      workspace={workspace}
      title={tenderDisplayName(overview.tender)}
      chat={chat}
      actions={actions}
      context={(close) => (
        <WorkspaceContext
          overview={overview}
          artifacts={artifacts}
          onImport={() => {
            close();
            onImport();
          }}
          onDocuments={() => {
            close();
            showDocumentList();
          }}
          onTeam={() => {
            close();
            open("team");
          }}
          onSource={(selection) => {
            close();
            showSource(selection);
          }}
        />
      )}
      renderView={(view) => {
        if (view === "documents")
          return (
            <WorkspaceDocuments
              tenderId={overview.tender.id}
              artifacts={artifacts}
              selection={sourceSelection}
              onSource={showSource}
              onCloseSource={onCloseSource ? showDocumentList : undefined}
              onSelectionChange={onSourceSelectionChange ?? onSource}
              onImport={onImport}
              onRegister={onDocuments}
            />
          );
        if (view === "team")
          return (
            <Team
              tenderId={overview.tender.id}
              managerRunId={
                overview.active_runs.find((run) =>
                  ["manager", "conversation"].includes(run.kind),
                )?.id
              }
              onSource={showSource}
            />
          );
        if (view === "activity")
          return <WorkspaceActivity tenderId={overview.tender.id} />;
        if (selectedResult)
          return (
            <div className="flex flex-col gap-4">
              <div className="flex items-center gap-2">
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Back to reviews"
                  onClick={reviewOverview}
                >
                  <ArrowLeft />
                </Button>
                <h2 className="text-sm font-medium">Saved result</h2>
              </div>
              <ResultPane
                key={`${selectedResult.view}:${selectedResult.id}`}
                tenderId={overview.tender.id}
                tenderRevision={overview.tender.revision}
                workRevision={overview.active_runs
                  .map((run) => `${run.id}:${run.status}`)
                  .sort()
                  .join("|")}
                view={selectedResult.view}
                recordId={selectedResult.id}
                onSource={showSource}
                onBack={reviewOverview}
              />
            </div>
          );
        return (
          <WorkspaceReviews
            overview={overview}
            planId={
              recordView === "plan" || recordView === "plan-review"
                ? recordId
                : undefined
            }
            focusedFinding={recordView === "finding" ? recordId : undefined}
            onSource={showSource}
            onPlan={(id) => showRecord("plan-review", id)}
            onConversation={returnToConversation}
          />
        );
      }}
    />
  );
}

function ResultPane({
  tenderId,
  tenderRevision,
  workRevision,
  view,
  recordId,
  onSource,
  onBack,
}: {
  tenderId: string;
  tenderRevision: number;
  workRevision: string;
  view: string;
  recordId: string;
  onSource: (source: SourceSelection) => void;
  onBack: () => void;
}) {
  if (view === "work-product")
    return (
      <WorkProductLibrary
        tenderId={tenderId}
        tenderRevision={tenderRevision}
        workRevision={workRevision}
        focusedProductId={recordId}
        onSource={(sourceId) => onSource({ sourceId })}
        onBack={onBack}
      />
    );
  if (view === "calculation")
    return (
      <CalculationInspector
        tenderId={tenderId}
        calculationId={recordId}
        label="Saved calculation"
        initiallyOpen
      />
    );
  return (
    <SavedOutputResult
      tenderId={tenderId}
      recordId={recordId}
      onSource={onSource}
    />
  );
}

function SavedOutputResult({
  tenderId,
  recordId,
  onSource,
}: {
  tenderId: string;
  recordId: string;
  onSource: (source: SourceSelection) => void;
}) {
  const api = useApi();
  const outputs = useQuery({
    queryKey: [tenderPath(tenderId), "outputs"],
    queryFn: ({ signal }) =>
      api.get<Schema<"OutputRecord">[]>(
        `${tenderPath(tenderId)}/outputs`,
        signal,
      ),
    retry: false,
  });
  if (outputs.isPending) return <Loading>Loading saved document…</Loading>;
  if (outputs.error) return <ErrorNotice error={outputs.error} />;
  const output = outputs.data?.find((item) => item.id === recordId);
  if (!output)
    return (
      <ErrorNotice error={new Error("This saved result is unavailable.")} />
    );
  return (
    <article
      className="flex flex-col gap-3 rounded-xl border bg-card p-4 text-sm shadow-xs"
      aria-label="Saved work result"
    >
      <p className="text-xs text-muted-foreground">
        Work result · Draft document
      </p>
      <h3 className="font-medium" dir="auto">
        {output.filename}
      </h3>
      <div className="flex flex-wrap items-center gap-1.5">
        <Status value={output.status} />
        <Badge variant="secondary" className="font-normal">
          {output.kind}
        </Badge>
        <span className="text-xs text-muted-foreground">
          {formatDate(output.created_at)}
        </span>
      </div>
      {output.blocking_reasons.length ? (
        <Alert role="status">
          <TriangleAlert />
          <AlertTitle>Review before use</AlertTitle>
          <AlertDescription>
            <ul className="ms-4 list-disc">
              {output.blocking_reasons.map((reason) => (
                <li key={reason}>{reason}</li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      ) : null}
      {output.source_ids.length ? (
        <div className="flex flex-col gap-2">
          <h4 className="text-xs font-medium text-muted-foreground">
            Sources used
          </h4>
          <Citations
            ids={output.source_ids}
            tenderId={tenderId}
            onOpen={onSource}
          />
        </div>
      ) : null}
      <p className="text-muted-foreground">
        Open the Submission workspace when you need the full document review and
        download controls.
      </p>
    </article>
  );
}

function isResultView(value: string | undefined) {
  return (
    value === "work-product" || value === "calculation" || value === "output"
  );
}

function formatDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "the saved time"
    : date.toLocaleString();
}
