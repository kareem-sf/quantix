import { useState } from "react";
import { FolderOpen, FileArchive, Upload } from "lucide-react";
import {
  choosePackage,
  nativeDesktop,
  tenderPath,
  useApi,
  useRefresh,
  type Schema,
} from "../api";
import { ErrorNotice, Modal } from "../components/ui";

export function NewTender({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (tender: Schema<"Tender">) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [name, setName] = useState(""),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  return (
    <Modal title="New tender" onClose={onClose}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          setPending(true);
          setError(null);
          try {
            const tender = await api.post<Schema<"Tender">>("/tenders", {
              name: name.trim(),
            } satisfies Schema<"CreateTender">);
            await refresh();
            onCreated(tender);
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <p className="muted">
          Keep the documents, decisions and work for this tender together.
        </p>
        <label>
          Tender name
          <input
            required
            maxLength={200}
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Enter the project or tender name"
          />
        </label>
        <ErrorNotice error={error} />
        <div className="form-actions">
          <button type="button" className="button" onClick={onClose}>
            Cancel
          </button>
          <button className="button primary" disabled={pending || !name.trim()}>
            {pending ? "Creating…" : "Create tender"}
          </button>
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
            await api.post<Schema<"Run">>(`${tenderPath(tenderId)}/imports`, {
              source_path: path.trim(),
            } satisfies Schema<"ImportRequest">);
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
