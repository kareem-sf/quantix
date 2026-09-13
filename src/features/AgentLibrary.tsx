import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Plus, RefreshCw, Sparkles } from "lucide-react";
import { useApi } from "../api";
import { Empty, ErrorNotice, Loading, Modal } from "../components/common";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { AgentDefinitionCard } from "./AgentDefinitionCard";
import { AgentDefinitionForm } from "./AgentDefinitionForm";
import { AgentManagerTaskForm } from "./AgentManagerTaskForm";
import {
  safeFilename,
  stableKey,
  type AgentDefinition,
  type AgentDefinitionExport,
  type FormMode,
  type ProfessionalProfile,
  type TaskMode,
  type TenderSummary,
} from "./agentLibraryTypes";

/** Manage provider-independent professional definitions for later Tender work. */
export function AgentLibrary() {
  const api = useApi();
  const [definitions, setDefinitions] = useState<AgentDefinition[] | null>(
    null,
  );
  const [tenders, setTenders] = useState<TenderSummary[] | null>(null);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [tenderError, setTenderError] = useState<unknown>(null);
  const [formMode, setFormMode] = useState<FormMode | null>(null);
  const [taskMode, setTaskMode] = useState<TaskMode | null>(null);
  const [copyTarget, setCopyTarget] = useState<AgentDefinition | null>(null);
  const [copyName, setCopyName] = useState("");
  const [copyKey, setCopyKey] = useState("");
  const [copyPending, setCopyPending] = useState(false);
  const [actionError, setActionError] = useState<unknown>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [history, setHistory] = useState<Record<string, AgentDefinition[]>>({});
  const [historyPending, setHistoryPending] = useState<string | null>(null);

  const loadDefinitions = useCallback(
    async (signal?: AbortSignal) => {
      try {
        const items = await api.get<AgentDefinition[]>(
          "/agent-definitions",
          signal,
        );
        setDefinitions(items);
        setLoadError(null);
      } catch (error) {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setLoadError(error);
          setDefinitions((current) => current ?? []);
        }
      }
    },
    [api],
  );

  const loadTenders = useCallback(
    async (signal?: AbortSignal) => {
      try {
        const items = await api.get<TenderSummary[]>("/tenders", signal);
        setTenders(items);
        setTenderError(null);
      } catch (error) {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setTenderError(error);
          setTenders((current) => current ?? []);
        }
      }
    },
    [api],
  );

  useEffect(() => {
    const controller = new AbortController();
    void Promise.all([
      loadDefinitions(controller.signal),
      loadTenders(controller.signal),
    ]);
    return () => controller.abort();
  }, [loadDefinitions, loadTenders]);

  function acceptDefinition(next: AgentDefinition) {
    setDefinitions((current) => {
      const kept = (current ?? []).filter((item) => item.id !== next.id);
      return next.lifecycle === "active" ? [next, ...kept] : kept;
    });
    setHistory((current) => {
      if (!(next.id in current)) return current;
      return {
        ...current,
        [next.id]: [
          next,
          ...current[next.id].filter((item) => item.version !== next.version),
        ],
      };
    });
  }

  async function saveDefinition(
    payload: {
      profile: ProfessionalProfile;
      generation_settings: AgentDefinition["generation_settings"];
    },
    mode: FormMode,
    idempotencyKey: string,
  ) {
    const next =
      mode.kind === "create"
        ? await api.post<AgentDefinition>("/agent-definitions", {
            ...payload,
            idempotency_key: idempotencyKey,
          })
        : await api.patch<AgentDefinition>(
            `/agent-definitions/${encodeURIComponent(mode.definition.id)}`,
            {
              expected_version: mode.definition.version,
              ...payload,
              idempotency_key: idempotencyKey,
            },
          );
    acceptDefinition(next);
    setFormMode(null);
    setNotice(
      mode.kind === "create"
        ? `${next.profile.display_name} is ready to reuse.`
        : `${next.profile.display_name} version ${next.version} is saved.`,
    );
  }

  async function showHistory(definition: AgentDefinition) {
    if (history[definition.id]) {
      setHistory((current) => {
        const next = { ...current };
        delete next[definition.id];
        return next;
      });
      return;
    }
    setHistoryPending(definition.id);
    setActionError(null);
    try {
      const versions = await api.get<AgentDefinition[]>(
        `/agent-definitions/${encodeURIComponent(definition.id)}/versions`,
      );
      setHistory((current) => ({ ...current, [definition.id]: versions }));
    } catch (error) {
      setActionError(error);
    } finally {
      setHistoryPending(null);
    }
  }

  async function exportDefinition(definition: AgentDefinition) {
    setActionError(null);
    try {
      const exported = await api.get<AgentDefinitionExport>(
        `/agent-definitions/${encodeURIComponent(definition.id)}/export?version=${definition.version}`,
      );
      const blob = new Blob([JSON.stringify(exported, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = `${safeFilename(definition.profile.display_name)}-v${definition.version}.json`;
      link.click();
      URL.revokeObjectURL(url);
      setNotice(
        `Exported ${definition.profile.display_name} version ${definition.version}.`,
      );
    } catch (error) {
      setActionError(error);
    }
  }

  function prepareDuplicate(definition: AgentDefinition) {
    setCopyTarget(definition);
    setCopyName(`${definition.profile.display_name} copy`);
    setCopyKey(stableKey("definition-copy"));
  }

  async function duplicateDefinition(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!copyTarget || copyPending) return;
    setCopyPending(true);
    setActionError(null);
    try {
      const next = await api.post<AgentDefinition>(
        `/agent-definitions/${encodeURIComponent(copyTarget.id)}/duplicate`,
        {
          source_version: copyTarget.version,
          display_name: copyName,
          idempotency_key: copyKey,
        },
      );
      acceptDefinition(next);
      setCopyTarget(null);
      setNotice(`${next.profile.display_name} is ready to edit or reuse.`);
    } catch (error) {
      setActionError(error);
    } finally {
      setCopyPending(false);
    }
  }

  async function retireDefinition(definition: AgentDefinition) {
    setActionError(null);
    try {
      const next = await api.post<AgentDefinition>(
        `/agent-definitions/${encodeURIComponent(definition.id)}/retire`,
        {
          expected_version: definition.version,
          idempotency_key: stableKey("definition-retire"),
        },
      );
      acceptDefinition(next);
      setNotice(
        `${definition.profile.display_name} is retired from new Tender work.`,
      );
    } catch (error) {
      setActionError(error);
    }
  }

  if (definitions === null) {
    return <Loading>Loading professional library…</Loading>;
  }

  return (
    <section
      className="flex flex-col gap-5"
      aria-labelledby="agent-library-heading"
    >
      <LibraryHeader
        onRefresh={() => loadDefinitions()}
        onGenerate={() => setTaskMode({ kind: "generate" })}
        onCreate={() => setFormMode({ kind: "create" })}
      />
      {notice ? (
        <Alert>
          <AlertTitle>Saved</AlertTitle>
          <AlertDescription>{notice}</AlertDescription>
        </Alert>
      ) : null}
      <ErrorNotice error={loadError ?? actionError} />
      {loadError ? (
        <Button
          type="button"
          variant="outline"
          className="w-fit"
          onClick={() => void loadDefinitions()}
        >
          Retry loading professionals
        </Button>
      ) : null}

      {definitions.length === 0 ? (
        <EmptyLibrary onGenerate={() => setTaskMode({ kind: "generate" })} />
      ) : (
        <div className="grid gap-4 lg:grid-cols-2">
          {definitions.map((definition) => (
            <AgentDefinitionCard
              key={definition.id}
              definition={definition}
              versions={history[definition.id]}
              historyPending={historyPending === definition.id}
              onUse={(item) => setTaskMode({ kind: "reuse", definition: item })}
              onEdit={(item) => setFormMode({ kind: "edit", definition: item })}
              onDuplicate={prepareDuplicate}
              onHistory={(item) => void showHistory(item)}
              onExport={(item) => void exportDefinition(item)}
              onRetire={(item) => void retireDefinition(item)}
            />
          ))}
        </div>
      )}

      {formMode ? (
        <Modal
          drawer
          legacy={false}
          title={
            formMode.kind === "create"
              ? "Create professional"
              : `Edit ${formMode.definition.profile.display_name}`
          }
          onClose={() => setFormMode(null)}
        >
          <AgentDefinitionForm
            key={
              formMode.kind === "create"
                ? "create"
                : `${formMode.definition.id}:${formMode.definition.version}`
            }
            mode={formMode}
            onSave={(payload, idempotencyKey) =>
              saveDefinition(payload, formMode, idempotencyKey)
            }
          />
        </Modal>
      ) : null}

      {taskMode ? (
        <Modal
          legacy={false}
          title={
            taskMode.kind === "generate"
              ? "Generate from a brief"
              : `Use ${taskMode.definition.profile.display_name}`
          }
          onClose={() => setTaskMode(null)}
        >
          <AgentManagerTaskForm
            mode={taskMode}
            tenders={tenders ?? []}
            tenderError={tenderError}
            onRetryTenders={() => loadTenders()}
            onComplete={(message) => {
              setTaskMode(null);
              setNotice(message);
            }}
          />
        </Modal>
      ) : null}

      {copyTarget ? (
        <DuplicateDialog
          target={copyTarget}
          name={copyName}
          pending={copyPending}
          error={actionError}
          onName={setCopyName}
          onClose={() => setCopyTarget(null)}
          onSubmit={duplicateDefinition}
        />
      ) : null}
    </section>
  );
}

function LibraryHeader({
  onRefresh,
  onGenerate,
  onCreate,
}: {
  onRefresh: () => Promise<void>;
  onGenerate: () => void;
  onCreate: () => void;
}) {
  return (
    <header className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
      <div className="flex max-w-2xl flex-col gap-1">
        <h2
          id="agent-library-heading"
          className="text-base font-medium"
        >
          Professional library
        </h2>
        <p className="text-sm text-muted-foreground">
          Save proven working profiles and reuse an exact version on a Tender.
          Tender files, AI accounts, spending and work permission are chosen
          separately.
        </p>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="ghost" onClick={() => void onRefresh()}>
          <RefreshCw data-icon="inline-start" />
          Refresh professionals
        </Button>
        <Button type="button" variant="outline" onClick={onGenerate}>
          <Sparkles data-icon="inline-start" />
          Generate from brief
        </Button>
        <Button type="button" onClick={onCreate}>
          <Plus data-icon="inline-start" />
          Create professional manually
        </Button>
      </div>
    </header>
  );
}

function EmptyLibrary({ onGenerate }: { onGenerate: () => void }) {
  return (
    <Empty
      title="No saved professionals yet"
      action={
        <Button type="button" onClick={onGenerate}>
          <Sparkles data-icon="inline-start" />
          Generate from brief
        </Button>
      }
    >
      Ask the Tender Manager to create one from a brief, or enter a complete
      profile yourself.
    </Empty>
  );
}

function DuplicateDialog({
  target,
  name,
  pending,
  error,
  onName,
  onClose,
  onSubmit,
}: {
  target: AgentDefinition;
  name: string;
  pending: boolean;
  error: unknown;
  onName: (name: string) => void;
  onClose: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <Modal
      legacy={false}
      title={`Duplicate ${target.profile.display_name}`}
      onClose={onClose}
    >
      <form className="flex flex-col gap-4" onSubmit={onSubmit}>
        <FieldGroup>
          <Field>
            <FieldLabel htmlFor="agent-copy-name">Copy name</FieldLabel>
            <Input
              id="agent-copy-name"
              required
              maxLength={120}
              value={name}
              onChange={(event) => onName(event.target.value)}
            />
            <FieldDescription>
              The copy starts at version 1 from {target.profile.display_name}
              version {target.version}.
            </FieldDescription>
          </Field>
        </FieldGroup>
        <ErrorNotice error={error} />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" disabled={pending}>
            {pending ? "Creating…" : "Create copy"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
