import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronDown,
  FileText,
  FolderInput,
  TriangleAlert,
} from "lucide-react";
import { isActive, tenderPath, useApi, type Schema } from "../../api";
import { ErrorNotice, Loading } from "../../components/common";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { GlideMenu } from "@/components/beautiful/glide-menu";
import { cn } from "@/lib/utils";
import { rtlDir } from "@/lib/text-direction";

type State = Schema<"DocumentGroupsState">;
type Document = Schema<"GroupedDocument">;

const NEW_GROUP = "__new__";

function MoveForm({
  document,
  groups,
  onMove,
  onCancel,
}: {
  document: Document;
  groups: string[];
  onMove: (group: string) => Promise<void>;
  onCancel: () => void;
}) {
  const [choice, setChoice] = useState(groups[0] ?? NEW_GROUP);
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const target = choice === NEW_GROUP ? name.trim() : choice;
  return (
    <form
      className="bui-fade-in flex flex-wrap items-center gap-1.5 px-2 pb-2 ps-8"
      aria-label={`Move ${document.name}`}
      onSubmit={async (event) => {
        event.preventDefault();
        if (!target) return;
        setSaving(true);
        try {
          await onMove(target);
        } finally {
          setSaving(false);
        }
      }}
    >
      <NativeSelect
        aria-label="Move to group"
        value={choice}
        onChange={(event) => setChoice(event.target.value)}
      >
        {groups.map((group) => (
          <NativeSelectOption key={group} value={group}>
            {group}
          </NativeSelectOption>
        ))}
        <NativeSelectOption value={NEW_GROUP}>New group…</NativeSelectOption>
      </NativeSelect>
      {choice === NEW_GROUP ? (
        <Input
          aria-label="New group name"
          className="h-7 w-40"
          maxLength={60}
          value={name}
          onChange={(event) => setName(event.target.value)}
        />
      ) : null}
      <Button type="submit" size="xs" disabled={!target || saving}>
        Move
      </Button>
      <Button type="button" size="xs" variant="ghost" onClick={onCancel}>
        Cancel
      </Button>
    </form>
  );
}

/**
 * The tender's documents in the groups the package analysis chose, each with a
 * short English description. Only real problems are flagged. An engineer can
 * move a file to another group and that choice is kept.
 */
export function DocumentGroups({
  tenderId,
  onOpen,
  query = "",
  revision = "",
}: {
  tenderId: string;
  onOpen: (artifactId: string) => void;
  query?: string;
  /** Changes when tender work starts or finishes, so new groups show up. */
  revision?: string;
}) {
  const api = useApi();
  const client = useQueryClient();
  const key = ["document-groups", tenderId, revision];
  const groups = useQuery<State>({
    queryKey: key,
    queryFn: ({ signal }) =>
      api.get<State>(`${tenderPath(tenderId)}/document-groups`, signal),
  });
  const [closed, setClosed] = useState<Record<string, boolean>>({});
  const [moving, setMoving] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [runId, setRunId] = useState<string | null>(null);
  const run = useQuery({
    queryKey: ["run", runId],
    enabled: runId !== null,
    queryFn: ({ signal }) =>
      api.get<Schema<"Run">>(`/runs/${encodeURIComponent(runId!)}`, signal),
    refetchInterval: ({ state }) =>
      state.data && !isActive(state.data.status) ? false : 1500,
  });
  const finished = run.data && !isActive(run.data.status);
  const analysing = runId !== null && !finished;

  if (groups.isPending) return <Loading>Loading documents…</Loading>;
  const state = groups.data;
  const needle = query.trim().toLocaleLowerCase();
  const names = (state?.groups ?? []).map((group) => group.name);

  async function analyse() {
    setError(null);
    try {
      const started = await api.post<Schema<"Run">>(
        `${tenderPath(tenderId)}/analysis`,
      );
      setRunId(started.id);
      await client.invalidateQueries();
    } catch (failure) {
      setError(failure);
    }
  }

  async function move(document: Document, group: string) {
    setError(null);
    try {
      const next = await api.put<State>(
        `${tenderPath(tenderId)}/documents/${encodeURIComponent(document.artifact_id)}/group`,
        { group },
      );
      client.setQueryData(key, next);
      setMoving(null);
    } catch (failure) {
      setError(failure);
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <ErrorNotice error={error ?? groups.error ?? run.error} />
      {finished && run.data?.error ? (
        <p role="alert" className="text-xs text-destructive wrap-anywhere">
          Grouping stopped: {run.data.error}
        </p>
      ) : null}
      {state && !state.grouped && state.total ? (
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          {analysing ? (
            <span role="status">
              Sorting the documents into groups. Follow it in Activity.
            </span>
          ) : (
            <>
              <span>These files have not been sorted into groups yet.</span>
              <Button
                type="button"
                size="xs"
                variant="outline"
                onClick={() => void analyse()}
              >
                Group documents now
              </Button>
            </>
          )}
        </div>
      ) : null}
      {(state?.groups ?? []).map((group) => {
        const documents = (group.documents ?? []).filter((document) =>
          needle
            ? `${document.name} ${document.label}`
                .toLocaleLowerCase()
                .includes(needle)
            : true,
        );
        if (!documents.length) return null;
        const open = needle ? true : !closed[group.name];
        return (
          <section key={group.name} aria-label={group.name}>
            <button
              type="button"
              aria-expanded={open}
              onClick={() =>
                setClosed((all) => ({ ...all, [group.name]: open }))
              }
              className="flex w-full items-center gap-1.5 rounded-md px-1 py-1 text-start text-sm font-medium hover:bg-muted/60"
            >
              <ChevronDown
                aria-hidden
                className={cn(
                  "size-4 text-muted-foreground transition-transform",
                  !open && "-rotate-90",
                )}
              />
              <span className="min-w-0 flex-1 truncate">{group.name}</span>
              <span className="text-xs font-normal text-muted-foreground tabular-nums">
                {documents.length}
              </span>
            </button>
            <div className="bui-collapse" data-open={open}>
              <div>
                <GlideMenu
                  className="mt-0.5 flex flex-col"
                  rowSelector="[data-document-row]"
                >
                  {documents.map((document) => (
                    <div
                      key={document.artifact_id}
                      data-document-row
                      className="group/doc relative z-10 rounded-md"
                    >
                      <div className="flex items-start gap-2 px-2 py-1.5">
                        <button
                          type="button"
                          onClick={() => onOpen(document.artifact_id)}
                          className="flex min-w-0 flex-1 items-start gap-2.5 text-start"
                        >
                          <FileText
                            aria-hidden
                            className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                          />
                          <span className="min-w-0 flex-1">
                            <span
                              dir={rtlDir(document.name)}
                              className="block truncate text-sm"
                            >
                              {document.name}
                            </span>
                            <span className="block text-xs text-muted-foreground wrap-anywhere">
                              {[
                                document.label,
                                document.pages ? `${document.pages} pp` : null,
                              ]
                                .filter(Boolean)
                                .join(" · ")}
                            </span>
                            {document.problem ? (
                              <span className="mt-0.5 flex items-start gap-1 text-xs text-(--warning-ink)">
                                <TriangleAlert
                                  aria-hidden
                                  className="mt-px size-3 shrink-0"
                                />
                                <span className="wrap-anywhere">
                                  {document.problem}
                                </span>
                              </span>
                            ) : null}
                          </span>
                        </button>
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-xs"
                          aria-label={`Move ${document.name} to another group`}
                          title="Move to another group"
                          className="opacity-0 group-hover/doc:opacity-100 focus-visible:opacity-100"
                          onClick={() =>
                            setMoving(
                              moving === document.artifact_id
                                ? null
                                : document.artifact_id,
                            )
                          }
                        >
                          <FolderInput />
                        </Button>
                      </div>
                      {moving === document.artifact_id ? (
                        <MoveForm
                          document={document}
                          groups={names.filter((name) => name !== group.name)}
                          onMove={(target) => move(document, target)}
                          onCancel={() => setMoving(null)}
                        />
                      ) : null}
                    </div>
                  ))}
                </GlideMenu>
              </div>
            </div>
          </section>
        );
      })}
      {state &&
      needle &&
      !(state.groups ?? []).some((group) =>
        (group.documents ?? []).some((document) =>
          `${document.name} ${document.label}`
            .toLocaleLowerCase()
            .includes(needle),
        ),
      ) ? (
        <p className="text-sm text-muted-foreground">
          No documents match this search.
        </p>
      ) : null}
    </div>
  );
}
