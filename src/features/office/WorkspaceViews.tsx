import { useState } from "react";
import {
  ArrowUpRight,
  BriefcaseBusiness,
  ChevronRight,
  FileText,
  FolderOpen,
  Plus,
  Search,
} from "lucide-react";
import { tenderPath, useResource, type Schema } from "@/api";
import { Button } from "@/components/ui/button";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { Separator } from "@/components/ui/separator";
import { MicroButton } from "@/components/ui/micro-button";
import { ErrorNotice, Loading, statusLabel } from "@/components/common";
import { SourceDrawer, type SourceSelection } from "../Sources";
import { PlanReview } from "../PlanReview";
import { WorkDecisions } from "../WorkDecisions";
import { ActivitySection } from "../Work";
import { WorkProductLibrary } from "../WorkProductLibrary";
import { sourceSelectionKey } from "./workspace-state";
import { tenderDisplayName } from "@/app/tender-name";

export function WorkspaceContext({
  overview,
  artifacts,
  onImport,
  onDocuments,
  onOffice,
  onSource,
}: {
  overview: Schema<"Overview">;
  artifacts: Schema<"Artifact">[];
  onImport: () => void;
  onDocuments: () => void;
  onOffice: () => void;
  onSource: (source: SourceSelection) => void;
}) {
  const sources = artifacts.filter((file) => file.is_current);
  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-muted-foreground">Tender</p>
      <div className="flex items-start gap-3">
        <BriefcaseBusiness className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0">
          <p className="truncate text-sm" dir="auto">
            {tenderDisplayName(overview.tender)}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            {statusLabel(overview.tender.status)} · Revision{" "}
            {overview.tender.revision}
          </p>
        </div>
      </div>
      <Button
        variant="ghost"
        className="h-8 justify-between px-0 font-normal"
        onClick={onOffice}
      >
        Open the office
        <ChevronRight />
      </Button>
      <Separator />
      <div className="flex items-center justify-between">
        <p className="text-xs text-muted-foreground">Sources</p>
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label="Import sources"
          title="Import sources"
          onClick={onImport}
        >
          <Plus />
        </Button>
      </div>
      {sources.slice(0, 3).map((source) => (
        <Button
          key={source.id}
          variant="ghost"
          className="h-8 justify-start gap-2 px-0 font-normal"
          onClick={() =>
            onSource({
              artifactId: source.id,
              artifact: source,
              version: source.version,
              contentHash: source.content_hash,
            })
          }
        >
          <FileText className="text-muted-foreground" />
          <span className="truncate" dir="auto">
            {source.name}
          </span>
        </Button>
      ))}
      <p className="text-xs text-muted-foreground">
        {overview.coverage.registered} imported · {overview.coverage.extracted}{" "}
        extracted
      </p>
      <Button
        variant="ghost"
        className="h-8 justify-start gap-2 px-0 font-normal text-muted-foreground"
        onClick={onDocuments}
      >
        <FolderOpen />
        View all documents
      </Button>
    </div>
  );
}

export function WorkspaceDocuments({
  tenderId,
  artifacts,
  selection,
  onSource,
  onCloseSource,
  onSelectionChange,
  onImport,
  onRegister,
}: {
  tenderId: string;
  artifacts: Schema<"Artifact">[];
  selection: SourceSelection | null;
  onSource: (source: SourceSelection) => void;
  onCloseSource?: () => void;
  onSelectionChange?: (source: SourceSelection) => void;
  onImport: () => void;
  onRegister?: () => void;
}) {
  const [query, setQuery] = useState("");
  const [versions, setVersions] = useState("current");
  const files = artifacts.filter(
    (file) =>
      (versions === "all" || file.is_current) &&
      `${file.name} ${file.relative_path}`
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase()),
  );
  if (selection && onCloseSource)
    return (
      <SourceDrawer
        key={sourceSelectionKey(selection)}
        tenderId={tenderId}
        selection={selection}
        artifacts={artifacts}
        presentation="inline"
        onClose={onCloseSource}
        onSelectionChange={onSelectionChange ?? onSource}
      />
    );
  return (
    <section className="flex flex-col gap-5" aria-label="Workspace documents">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-medium">Documents</h2>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Import documents"
          title="Import documents"
          onClick={onImport}
        >
          <Plus />
        </Button>
      </div>
      <div className="flex flex-wrap gap-2">
        <InputGroup className="min-w-32 flex-1">
          <InputGroupAddon>
            <Search />
          </InputGroupAddon>
          <InputGroupInput
            aria-label="Find a workspace document"
            placeholder="Find a document…"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </InputGroup>
        <NativeSelect
          aria-label="Workspace document revisions"
          value={versions}
          onChange={(event) => setVersions(event.target.value)}
        >
          <NativeSelectOption value="current">Current files</NativeSelectOption>
          <NativeSelectOption value="all">All revisions</NativeSelectOption>
        </NativeSelect>
      </div>
      <div
        className="flex flex-col"
        role="list"
        aria-label="Workspace document list"
      >
        {files.map((file) => (
          <div role="listitem" key={file.id}>
            <Button
              variant="ghost"
              className="h-auto w-full justify-start gap-3 rounded-lg px-2 py-3 text-start font-normal whitespace-normal"
              onClick={() =>
                onSource({
                  artifactId: file.id,
                  artifact: file,
                  version: file.version,
                  contentHash: file.content_hash,
                })
              }
            >
              <FileText className="size-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm" dir="auto">
                  {file.name}
                </span>
                <span className="mt-1 block text-xs text-muted-foreground">
                  Version {file.version} ·{" "}
                  {file.is_current ? "Current" : "Earlier revision"} ·{" "}
                  {statusLabel(file.status)}
                </span>
              </span>
              <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
            </Button>
          </div>
        ))}
      </div>
      {!files.length ? (
        <p className="text-sm text-muted-foreground">
          {artifacts.length
            ? "No documents match this search."
            : "Import the Tender package to start reviewing documents."}
        </p>
      ) : null}
      {onRegister ? (
        <Button
          variant="ghost"
          className="self-start px-0 text-xs font-normal text-muted-foreground"
          onClick={onRegister}
        >
          <ArrowUpRight />
          Open full document register
        </Button>
      ) : (
        <MicroButton kind="upload" className="self-start" onClick={onImport}>
          Import package
        </MicroButton>
      )}
    </section>
  );
}

export function WorkspaceReviews({
  overview,
  planId,
  focusedFinding,
  onSource,
  onRepair,
  onPlan,
  onConversation,
  onPublicCitation,
}: {
  overview: Schema<"Overview">;
  planId?: string;
  focusedFinding?: string;
  onSource: (source: SourceSelection) => void;
  onRepair?: (target: string) => void;
  onPlan: (id: string) => void;
  onConversation: () => void;
  onPublicCitation: (citationId: string) => void;
}) {
  if (planId)
    return (
      <div className="legacy-screen">
        <PlanReview
          tenderId={overview.tender.id}
          planId={planId}
          onSource={onSource}
          onRepair={onRepair}
          onBack={onConversation}
        />
      </div>
    );
  return (
    <section className="flex flex-col gap-5" aria-label="Workspace reviews">
      <div>
        <h2 className="text-sm font-medium">Reviews</h2>
        <p className="mt-1 text-xs text-muted-foreground">
          Review the work and its sources before making a decision.
        </p>
      </div>
      {overview.plan ? (
        <Button
          variant="outline"
          className="h-auto justify-between gap-3 py-3 text-start whitespace-normal"
          onClick={() => onPlan(overview.plan!.id)}
        >
          <span className="min-w-0">
            <span className="block text-sm" dir="auto">
              {overview.plan.title}
            </span>
            <span className="mt-1 block text-xs font-normal text-muted-foreground">
              Review work plan
            </span>
          </span>
          <ChevronRight />
        </Button>
      ) : (
        <p className="text-sm text-muted-foreground">
          The Tender Manager has not proposed a work plan yet.
        </p>
      )}
      <WorkDecisions
        tenderId={overview.tender.id}
        focusedId={focusedFinding}
        onSource={onSource}
      />
      <WorkProductLibrary
        tenderId={overview.tender.id}
        tenderRevision={overview.tender.revision}
        workRevision={overview.active_runs
          .map((run) => `${run.id}:${run.status}`)
          .sort()
          .join("|")}
        onSource={(sourceId) => onSource({ sourceId })}
        onPublicCitation={onPublicCitation}
      />
    </section>
  );
}

export function WorkspaceActivity({ tenderId }: { tenderId: string }) {
  const runs = useResource<Schema<"Run">[]>(
    `${tenderPath(tenderId)}/runs`,
    true,
  );
  const messages = useResource<Schema<"Message">[]>(
    `${tenderPath(tenderId)}/messages`,
    true,
  );
  return (
    <div className="flex flex-col gap-3">
      <ErrorNotice error={runs.error || messages.error} />
      {runs.isPending || messages.isPending ? (
        <Loading>Loading activity…</Loading>
      ) : (
        <ActivitySection
          runs={runs.data ?? []}
          messages={messages.data ?? []}
        />
      )}
    </div>
  );
}
