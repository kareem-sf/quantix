import { useCallback, useEffect, type ReactNode } from "react";
import { Navigate, useLocation, useNavigate } from "react-router-dom";
import { CloudOff, RefreshCcw } from "lucide-react";
import { tenderPath, useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { Estimate } from "../features/Estimate";
import { Files } from "../features/Files";
import { Outputs } from "../features/Outputs";
import { Quotes } from "../features/Quotes";
import { SourceDrawer, type SourceSelection } from "../features/Sources";
import { Work } from "../features/Work";
import { LiveOffice } from "../features/office/LiveOffice";
import { CurrentWork } from "../features/office/CurrentWork";
import { TenderOfficeWorkspace } from "../features/office/TenderOfficeWorkspace";
import {
  parseRouteContext,
  recordRoute,
  settingsRoute,
  sourceRoute,
  stripSourceQuery,
  tenderRoute,
  type RouteContext,
} from "../navigation/routes";
import { Button } from "@/components/ui/button";
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";
import { sectionForView, type TenderSection } from "./sections";

/** The workspace contract this interface was built against. */
export const WORKSPACE_REVISION = 2;

type TenderContext = Extract<RouteContext, { kind: "tender" }>;

type TenderWorkspaceProps = {
  health?: Schema<"Health">;
  settings?: Schema<"Settings">;
  onImport: () => void;
};

export function TenderWorkspace(props: TenderWorkspaceProps) {
  const location = useLocation();
  const context = parseRouteContext(`${location.pathname}${location.search}`);
  if (context.kind !== "tender") return <Navigate replace to="/" />;
  if (context.section === "recents")
    return <Navigate replace to={tenderRoute(context.tenderId, "manager")} />;
  return <TenderView key={context.tenderId} context={context} {...props} />;
}

function TenderView({
  context,
  health,
  settings,
  onImport,
}: TenderWorkspaceProps & { context: TenderContext }) {
  const navigate = useNavigate();
  const location = useLocation();
  const current = `${location.pathname}${location.search}`;
  const { tenderId } = context;
  const section = context.section as TenderSection;
  const overview = useResource<Schema<"Overview">>(tenderPath(tenderId), true);
  const activeRunCount = overview.data?.active_runs.length ?? 0;
  const artifacts = useResource<Schema<"Artifact">[]>(
    `${tenderPath(tenderId)}/artifacts`,
    activeRunCount > 0,
  );
  const documentState = JSON.stringify([
    overview.data?.coverage,
    overview.data?.tender.revision,
    activeRunCount,
  ]);
  const refetchArtifacts = artifacts.refetch;
  useEffect(() => {
    void refetchArtifacts();
  }, [documentState, refetchArtifacts]);

  const go = useCallback(
    (target: string, replace = false) => void navigate(target, { replace }),
    [navigate],
  );
  const here = stripSourceQuery(current);
  const sourceSelection = selectionFrom(context);

  function openSource(selection: SourceSelection, replace = false) {
    go(
      sourceRoute(tenderId, section, {
        sourceId: "sourceId" in selection ? selection.sourceId : undefined,
        artifactId: selection.artifactId,
        version: selection.version,
        contentHash: selection.contentHash,
        page: selection.page,
        pageCount: selection.pageCount,
        sheet: selection.sheet,
        cellRange: selection.cellRange,
        view: context.view,
        recordId: context.recordId,
        origin: here,
      }),
      replace,
    );
  }
  const closeSource = () => go(here);
  const openRecord = (view: string, recordId: string) =>
    go(
      recordRoute(
        tenderId,
        section === "manager" &&
          ["plan", "plan-review", "finding", "decisions"].includes(view)
          ? "manager"
          : sectionForView(view, section),
        {
          view,
          recordId,
          origin: here,
        },
      ),
    );
  const openSettings = () => go(settingsRoute(here));
  const openSettingsSection = (settingsSection: string) =>
    go(
      `/settings?${new URLSearchParams({ section: settingsSection, return: here })}`,
    );
  const showView = (view: string) =>
    go(`${tenderRoute(tenderId, section)}?${new URLSearchParams({ view })}`);

  const capabilities = health?.capabilities ?? [];
  const compatible = health?.workspace_revision === WORKSPACE_REVISION;

  let body: ReactNode;
  if (!health) body = <Loading>Checking the workspace…</Loading>;
  else if (!compatible)
    body = <WorkspaceUpdate health={health} onSettings={openSettings} />;
  else if (overview.isPending) body = <Loading>Loading tender…</Loading>;
  else if (!overview.data)
    body = (
      <div className="flex flex-col items-start gap-3 p-6">
        <ErrorNotice error={overview.error} />
        <Button variant="outline" onClick={() => void overview.refetch()}>
          <RefreshCcw data-icon="inline-start" />
          Try again
        </Button>
      </div>
    );
  else if (section === "manager")
    body = (
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <TenderOfficeWorkspace
          overview={overview.data}
          artifacts={artifacts.data ?? []}
          settings={settings}
          officeRevision={health.office_revision}
          sourceSelection={sourceSelection}
          recordView={context.view}
          recordId={context.recordId}
          onCloseSource={closeSource}
          onImport={onImport}
          onSettings={openSettings}
          onSource={(selection) => openSource(selection)}
          onSourceSelectionChange={(selection) => openSource(selection, true)}
          onDocuments={() => go(tenderRoute(tenderId, "documents"))}
          onRecord={openRecord}
          onRepair={(target) => go(target)}
          onCustomizeManager={() => openSettingsSection("manager")}
          renderLiveOffice={
            capabilities.includes("dynamic_office")
              ? ({
                  tenderId: officeTender,
                  onClose,
                  onSource,
                  onOpenResult,
                  onOpenOutput,
                }) => (
                  <div className="flex min-h-0 min-w-0 flex-col gap-4">
                    <CurrentWork tenderId={officeTender} onSource={onSource} />
                    <LiveOffice
                      tenderId={officeTender}
                      embedded
                      onSource={onSource}
                      onOpenResult={onOpenResult}
                      onOpenOutput={onOpenOutput}
                      onCustomizeManager={() => openSettingsSection("manager")}
                      onOpenManager={onClose}
                      onClose={onClose}
                    />
                  </div>
                )
              : undefined
          }
        />
      </div>
    );
  else
    body = (
      <SectionPage
        section={section}
        context={context}
        overview={overview.data}
        artifacts={artifacts}
        settings={settings}
        capabilities={capabilities}
        onImport={onImport}
        onSource={(selection) => openSource(selection)}
        onRecord={openRecord}
        onView={showView}
        onNavigate={go}
      />
    );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {body}
      {section !== "manager" && sourceSelection && compatible ? (
        <SourceDrawer
          key={sourceKey(sourceSelection)}
          tenderId={tenderId}
          selection={sourceSelection}
          artifacts={artifacts.data ?? []}
          onClose={closeSource}
          onSelectionChange={(selection) => openSource(selection, true)}
        />
      ) : null}
    </div>
  );
}

function SectionPage({
  section,
  context,
  overview,
  artifacts,
  settings,
  capabilities,
  onImport,
  onSource,
  onRecord,
  onView,
  onNavigate,
}: {
  section: Exclude<TenderSection, "manager">;
  context: TenderContext;
  overview: Schema<"Overview">;
  artifacts: { data?: Schema<"Artifact">[]; isPending: boolean };
  settings?: Schema<"Settings">;
  capabilities: string[];
  onImport: () => void;
  onSource: (selection: SourceSelection) => void;
  onRecord: (view: string, recordId: string) => void;
  onView: (view: string) => void;
  onNavigate: (target: string) => void;
}) {
  const { tenderId } = context;

  if (section === "documents")
    return (
      <Page legacy={false}>
        {artifacts.isPending ? (
          <Loading>Loading document register…</Loading>
        ) : (
          <Files
            tenderId={tenderId}
            artifacts={artifacts.data ?? []}
            onImport={onImport}
            onSource={onSource}
            meaningAvailable={capabilities.includes("meaning_search")}
            activeRuns={overview.active_runs}
            onWork={() => onNavigate(tenderRoute(tenderId, "work"))}
          />
        )}
      </Page>
    );

  if (section === "work")
    return (
      <Page legacy={false}>
        <Work
          tenderId={tenderId}
          onChanges={() => {
            onNavigate(tenderRoute(tenderId, "manager"));
            requestAnimationFrame(() =>
              document
                .querySelector<HTMLTextAreaElement>(
                  '[aria-label="Message to Tender Manager"]',
                )
                ?.focus(),
            );
          }}
          onSource={onSource}
          onRecord={onRecord}
          recordId={context.recordId}
          recordView={context.view}
          onView={onView}
        />
      </Page>
    );

  if (section === "estimate") {
    const view =
      context.view === "proposals" || context.view === "quotes"
        ? context.view
        : "boq";
    const views = [
      {
        id: "boq",
        label: "BOQ",
        available: capabilities.includes("estimates"),
      },
      {
        id: "proposals",
        label: "Proposals",
        available: capabilities.includes("estimates"),
      },
      {
        id: "quotes",
        label: "Supplier quotations",
        available: capabilities.includes("quotations"),
      },
    ] as const;
    const active = views.find((item) => item.id === view)!;
    const selectRecord = (recordId: string | null) =>
      onNavigate(
        recordId
          ? recordRoute(tenderId, "estimate", { view, recordId })
          : `${tenderRoute(tenderId, "estimate")}?view=${view}`,
      );
    return (
      <>
        <Tabs
          value={view}
          onValueChange={(value) => onView(String(value))}
          className="shrink-0 border-b px-4 py-1.5"
        >
          <TabsList variant="line" aria-label="Estimate views">
            {views.map((item) => (
              <TabsTrigger key={item.id} value={item.id}>
                {item.label}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <Page legacy={false}>
          {!active.available ? (
            <Unavailable what={active.label} />
          ) : view === "quotes" ? (
            <Quotes
              tenderId={tenderId}
              onSource={onSource}
              selectedId={context.recordId ?? null}
              onSelect={selectRecord}
            />
          ) : (
            <Estimate
              tenderId={tenderId}
              defaultCurrency={settings?.default_currency ?? ""}
              onSource={onSource}
              view={view}
              selectedId={context.recordId ?? null}
              onSelect={selectRecord}
            />
          )}
        </Page>
      </>
    );
  }

  return (
    <Page legacy={false}>
      {capabilities.includes("outputs") ? (
        <Outputs
          tenderId={tenderId}
          view={context.view}
          recordId={context.recordId}
          onNavigate={(view, recordId) =>
            onNavigate(
              recordId
                ? recordRoute(tenderId, "submission", { view, recordId })
                : `${tenderRoute(tenderId, "submission")}?view=${view}`,
            )
          }
          onRepair={onNavigate}
          onSource={onSource}
        />
      ) : (
        <Unavailable what="Submission" />
      )}
    </Page>
  );
}

function Page({
  children,
  legacy = true,
}: {
  children: ReactNode;
  /** Screens not yet rebuilt on shadcn keep their element-level styles. */
  legacy?: boolean;
}) {
  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <div
        className={cn(
          "mx-auto w-full max-w-6xl px-6 py-6",
          legacy && "legacy-screen",
        )}
      >
        {children}
      </div>
    </div>
  );
}

function Unavailable({ what }: { what: string }) {
  return (
    <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <CloudOff />
        </EmptyMedia>
        <EmptyTitle>
          <h2 className="text-base font-medium">{`${what} isn't available yet`}</h2>
        </EmptyTitle>
        <EmptyDescription>
          The running local service doesn't offer this. Quit Quantix and open it
          again after updating.
        </EmptyDescription>
      </EmptyHeader>
    </Empty>
  );
}

function WorkspaceUpdate({
  health,
  onSettings,
}: {
  health: Schema<"Health">;
  onSettings: () => void;
}) {
  return (
    <Empty className="flex-1">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <RefreshCcw />
        </EmptyMedia>
        <EmptyTitle>
          <h1 className="text-lg font-semibold tracking-tight">
            Workspace update required
          </h1>
        </EmptyTitle>
        <EmptyDescription>
          This window and the running local service are different versions, so
          Tender work stays closed to protect your records. Quit Quantix and
          open it again to finish updating. Settings stay available.
        </EmptyDescription>
      </EmptyHeader>
      <EmptyContent>
        <Button variant="outline" onClick={onSettings}>
          Open Settings
        </Button>
        <details className="text-xs text-muted-foreground">
          <summary className="cursor-pointer">More options</summary>
          <p className="mt-1">
            The service reports workspace version {health.workspace_revision}.
            This window needs version {WORKSPACE_REVISION}.
          </p>
        </details>
      </EmptyContent>
    </Empty>
  );
}

function selectionFrom(context: TenderContext): SourceSelection | null {
  const locator = {
    version: context.version,
    contentHash: context.contentHash,
    page: context.page,
    pageCount: context.pageCount,
    sheet: context.sheet,
    cellRange: context.cellRange,
  };
  if (context.sourceId)
    return {
      sourceId: context.sourceId,
      artifactId: context.artifactId,
      ...locator,
    };
  if (context.artifactId) return { artifactId: context.artifactId, ...locator };
  return null;
}

function sourceKey(selection: SourceSelection) {
  return JSON.stringify([
    "sourceId" in selection ? selection.sourceId : null,
    selection.artifactId,
    selection.version,
    selection.contentHash,
  ]);
}
