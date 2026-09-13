import { useEffect, useId, useRef, useState } from "react";
import { FileArchive, FolderOpen, FolderUp, Upload, X } from "lucide-react";
import {
  choosePackage,
  nativeDesktop,
  tenderPath,
  useApi,
  useRefresh,
  type Api,
  type Schema,
} from "../api";
import { ErrorNotice, Modal } from "../components/common";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";

/** Without a name, Quantix names the Tender from its package and then identifies the project. */
export const createTender = (api: Api, name?: string) =>
  api.post<Schema<"Tender">>(
    "/tenders",
    (name?.trim()
      ? { name: name.trim() }
      : {}) satisfies Schema<"CreateTender">,
  );

export const startPackageImport = (api: Api, tenderId: string, path: string) =>
  api.post<Schema<"Run">>(`${tenderPath(tenderId)}/imports`, {
    source_path: path.trim(),
  } satisfies Schema<"ImportRequest">);

/** Explorer's "Copy as path" wraps the path in quotes. */
const cleanPath = (value: string) => value.trim().replace(/^"(.*)"$/, "$1");

const packageName = (path: string) =>
  cleanPath(path)
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .pop()
    ?.replace(/\.zip$/i, "") ?? "";

/**
 * Files dropped on the desktop window arrive as native drag-drop events with
 * real paths (a browser page never learns the path of a dropped folder).
 */
function usePackageDrop(enabled: boolean, onDrop: (path: string) => void) {
  const [over, setOver] = useState(false);
  const latest = useRef(onDrop);
  latest.current = onDrop;
  useEffect(() => {
    if (!enabled) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent(({ payload }) => {
          if (payload.type === "leave") setOver(false);
          else if (payload.type !== "drop") setOver(true);
          else {
            setOver(false);
            if (payload.paths[0]) latest.current(payload.paths[0]);
          }
        }),
      )
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [enabled]);
  return over;
}

/** One step: bring in the package; Quantix identifies the project and names the tender. */
export function NewTender({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (tender: Schema<"Tender">) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const desktop = nativeDesktop();
  const pathId = useId();
  const [path, setPath] = useState(""),
    [created, setCreated] = useState<Schema<"Tender"> | null>(null),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);

  function accept(selected: string) {
    setPath(selected);
    setError(null);
  }
  const dragging = usePackageDrop(desktop && !pending, accept);

  async function choose(kind: "directory" | "zip") {
    try {
      const selected = await choosePackage(kind);
      if (selected) accept(selected);
    } catch (failure) {
      setError(failure);
    }
  }

  // If the tender was created but its import failed, leaving opens the tender.
  async function leave() {
    if (!created) return onClose();
    await refresh();
    onCreated(created);
  }

  const source = cleanPath(path);
  return (
    <Modal title="New tender" onClose={() => void leave()} legacy={false}>
      <form
        className="flex flex-col gap-5"
        onSubmit={async (event) => {
          event.preventDefault();
          setPending(true);
          setError(null);
          try {
            const tender = created ?? (await createTender(api));
            setCreated(tender);
            await startPackageImport(api, tender.id, source);
            await refresh();
            onCreated(tender);
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <FieldGroup className="gap-5">
          {!desktop ? (
            <Field>
              <FieldLabel htmlFor={pathId}>Project folder or ZIP</FieldLabel>
              <Input
                id={pathId}
                autoFocus
                value={path}
                disabled={pending}
                onChange={(event) => accept(event.target.value)}
                placeholder="D:\Tenders\Project"
              />
              <FieldDescription>
                Paste the full path. Drag and drop works in the desktop app.
              </FieldDescription>
            </Field>
          ) : source ? (
            <div className="flex items-center gap-3 rounded-xl border p-3">
              {/\.zip$/i.test(source) ? (
                <FileArchive className="size-5 shrink-0 text-muted-foreground" />
              ) : (
                <FolderOpen className="size-5 shrink-0 text-muted-foreground" />
              )}
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium">
                  {packageName(source)}
                </p>
                <p
                  className="truncate text-xs text-muted-foreground"
                  title={source}
                >
                  {source}
                </p>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="Remove the package"
                disabled={pending}
                onClick={() => setPath("")}
              >
                <X />
              </Button>
            </div>
          ) : (
            <div
              data-dragging={dragging || undefined}
              className="flex flex-col items-center gap-3 rounded-xl border border-dashed px-4 py-7 text-center transition-colors data-dragging:border-primary data-dragging:bg-accent"
            >
              <FolderUp className="size-6 text-muted-foreground" />
              <div className="flex flex-col gap-1">
                <p className="text-sm font-medium">
                  Drop the project folder or ZIP here
                </p>
                <p className="text-xs text-muted-foreground">
                  Files and folders are kept exactly as they are.
                </p>
              </div>
              <div className="flex flex-wrap justify-center gap-2">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void choose("directory")}
                >
                  <FolderOpen data-icon="inline-start" />
                  Choose folder
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void choose("zip")}
                >
                  <FileArchive data-icon="inline-start" />
                  Choose ZIP
                </Button>
              </div>
            </div>
          )}
        </FieldGroup>
        <ErrorNotice error={error} />
        <div className="flex justify-end gap-2">
          <Button type="button" variant="outline" onClick={() => void leave()}>
            {created ? "Open tender" : "Cancel"}
          </Button>
          <Button type="submit" disabled={pending || !source}>
            {pending ? null : <Upload data-icon="inline-start" />}
            {pending
              ? "Importing…"
              : created
                ? "Try the import again"
                : "Create tender"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}

export function ImportPackage({
  tenderId,
  onClose,
}: {
  tenderId: string;
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [path, setPath] = useState(""),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  async function choose(kind: "directory" | "zip") {
    try {
      const selected = await choosePackage(kind);
      if (selected) setPath(selected);
    } catch (failure) {
      setError(failure);
    }
  }
  return (
    <Modal title="Import tender package" onClose={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          setPending(true);
          setError(null);
          try {
            await startPackageImport(api, tenderId, path);
            await refresh();
            onClose();
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <div className="import-area">
          <Upload size={30} />
          <h3>Bring in the complete package</h3>
          <p>
            Choose a folder or ZIP file. Original files and their folder
            structure are preserved.
          </p>
          {nativeDesktop() ? (
            <div className="inline-actions">
              <button
                type="button"
                className="button"
                onClick={() => void choose("directory")}
              >
                <FolderOpen size={18} />
                Choose folder
              </button>
              <button
                type="button"
                className="button"
                onClick={() => void choose("zip")}
              >
                <FileArchive size={18} />
                Choose ZIP
              </button>
            </div>
          ) : null}
        </div>
        <label>
          Package path
          <input
            required
            value={path}
            onChange={(event) => setPath(event.target.value)}
            placeholder="Full local path to a folder or ZIP file"
          />
        </label>
        <ErrorNotice error={error} />
        <div className="form-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button className="button primary" disabled={pending || !path.trim()}>
            {pending ? "Starting import…" : "Import package"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
