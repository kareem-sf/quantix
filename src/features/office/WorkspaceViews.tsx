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
import { JobHistory } from "../activity/JobHistory";
import { DocumentGroups } from "../documents/DocumentGroups";
import { WorkProductLibrary } from "../WorkProductLibrary";
import { sourceSelectionKey } from "./workspace-state";
import { tenderDisplayName } from "@/app/tender-name";

export function WorkspaceContext({
  overview,
  artifacts,
  onImport,
  onDocuments,
  onTeam,
  onSource,
}: {
  overview: Schema<"Overview">;
  artifacts: Schema<"Artifact">[];
  onImport: () => void;
  onDocuments: () => void;
  onTeam: () => void;
  onSource: (source: SourceSelection) => void;
}) {
  const sources = artifacts.filter((file) => file.is_current);
  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-muted-foreground">Tender</p>
      <div className="flex items-start gap-3">
        <BriefcaseBusiness className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div className="min-w-0">
          <p className="truncate text-sm">
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
        onClick={onTeam}
      >
        Open the team
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
          <span className="truncate">{source.name}</span>
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
  revision,
}: {
  revision?: string;
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
  const open = (artifactId: string) => {
    const file = artifacts.find((item) => item.id === artifactId);
    onSource({
      artifactId,
      artifact: file,
      version: file?.version,
      contentHash: file?.content_hash,
    });
  };
  return (
    <section className="flex flex-col gap-4" aria-label="Workspace documents">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-medium">
          Documents
          {artifacts.length ? (
            <span className="font-normal text-muted-foreground">
              {" "}
              · {artifacts.filter((file) => file.is_current).length}
            </span>
          ) : null}
        </h2>
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
      {artifacts.length ? (
        <>
          <InputGroup>
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
          <DocumentGroups
            tenderId={tenderId}
            query={query}
            onOpen={open}
            revision={revision}
          />
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          Import the tender package to start reviewing documents.
        </p>
      )}
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

export function WorkspaceActivity({
  tenderId,
  onSource,
  onRaiseLimit,
}: {
  tenderId: string;
  onSource?: (source: SourceSelection) => void;
  onRaiseLimit?: () => void;
}) {
  const runs = useResource<Schema<"Run">[]>(
    `${tenderPath(tenderId)}/runs`,
    true,
  );
  return (
    <div className="flex flex-col gap-3">
      <ErrorNotice error={runs.error} />
      {runs.isPending ? (
        <Loading>Loading activity…</Loading>
      ) : (
        <JobHistory
          tenderId={tenderId}
          runs={runs.data ?? []}
          onSource={onSource}
          onRaiseLimit={onRaiseLimit}
        />
      )}
    </div>
  );
}
