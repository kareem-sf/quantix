import { useEffect, useRef, useState } from "react";
import { errorText, tenderPath, useApi, type Schema } from "../api";
import { Button } from "@/components/ui/button";
import { ErrorNotice } from "../components/common";

type Run = Schema<"LocalCodeRun">;
type Detail = Schema<"LocalCodeDetail">;
type File = Schema<"LocalCodeFile">;

export function LocalCodeInspector({
  tenderId,
  runId,
  actorId,
  assignmentId,
  onStopped,
}: {
  tenderId: string;
  runId: string;
  actorId?: string;
  assignmentId?: string;
  onStopped?: () => void | Promise<unknown>;
}) {
  const api = useApi();
  const [open, setOpen] = useState(false);
  const [items, setItems] = useState<Run[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const generation = useRef(0);
  const pending = useRef<AbortController | null>(null);
  const query = new URLSearchParams({ root_run_id: runId });
  if (actorId) query.set("actor_id", actorId);
  if (assignmentId) query.set("assignment_id", assignmentId);
  const scope = `${tenderPath(tenderId)}/local-code-runs?${query}`;
  useEffect(() => {
    generation.current++;
    pending.current?.abort();
    pending.current = null;
    setItems([]);
    setCursor(null);
    setLoaded(false);
    setBusy(false);
    setError(null);
    setOpen(false);
    return () => {
      generation.current++;
      pending.current?.abort();
    };
  }, [scope]);
  async function load(next: string | null = null) {
    if (pending.current) return;
    const controller = new AbortController();
    pending.current = controller;
    const current = generation.current;
    setBusy(true);
    setError(null);
    try {
      const page = await api.get<Schema<"LocalCodePage">>(
        `${scope}&limit=25${next ? `&cursor=${encodeURIComponent(next)}` : ""}`,
        controller.signal,
      );
      if (current !== generation.current) return;
      setItems((old) =>
        next
          ? [
              ...old,
              ...page.items.filter(
                (item) =>
                  !old.some(
                    (saved) =>
                      saved.id === item.id && saved.engine === item.engine,
                  ),
              ),
            ]
          : page.items,
      );
      setCursor(page.next_cursor ?? null);
      setLoaded(true);
    } catch (cause) {
      if (!controller.signal.aborted && current === generation.current)
        setError(cause);
    } finally {
      if (pending.current === controller) pending.current = null;
      if (current === generation.current) setBusy(false);
    }
  }
  async function stop() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.post(`/runs/${encodeURIComponent(runId)}/cancel`);
      await onStopped?.();
    } catch (cause) {
      setError(cause);
    } finally {
      setBusy(false);
    }
    await load();
  }
  return (
    <details
      className="rounded-lg border p-3 text-sm"
      open={open}
      onToggle={(event) => {
        setOpen(event.currentTarget.open);
        if (event.currentTarget.open && !loaded) void load();
      }}
    >
      <summary className="cursor-pointer font-medium">
        Local calculations and code
      </summary>
      <div className="mt-3 flex flex-col gap-3">
        <p>
          Saved local execution records. Opening a record checks its code,
          inputs and output hashes; it does not approve the result.
        </p>
        {error != null && <ErrorNotice error={error} />}
        {busy && <p role="status">Loading local code records…</p>}
        {loaded && !items.length && (
          <p>No matching local code records on this page.</p>
        )}
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={busy}
            onClick={() => void load()}
          >
            Refresh local progress
          </Button>
          {items.some(
            (item) => item.root_active && item.status === "running",
          ) && (
            <Button
              type="button"
              variant="outline"
              disabled={busy}
              onClick={() => void stop()}
            >
              Stop this work request
            </Button>
          )}
        </div>
        {items.some(
          (item) => item.root_active && item.status === "running",
        ) && (
          <p>
            Stop cancels the whole work request and its colleagues. Container
            cleanup may continue briefly.
          </p>
        )}
        {items.map((item) => (
          <LocalCodeCard
            key={`${scope}/${item.engine}/${item.id}/${item.status}/${item.phase}`}
            tenderId={tenderId}
            query={query.toString()}
            item={item}
          />
        ))}
        {cursor && (
          <Button
            type="button"
            variant="outline"
            disabled={busy}
            onClick={() => void load(cursor)}
          >
            Load more code records
          </Button>
        )}
      </div>
    </details>
  );
}

function LocalCodeCard({
  tenderId,
  query,
  item,
}: {
  tenderId: string;
  query: string;
  item: Run;
}) {
  const api = useApi();
  const [detail, setDetail] = useState<Detail | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);
  const controller = useRef<AbortController | null>(null);
  const base = `${tenderPath(tenderId)}/local-code-runs/${item.engine}/${item.id}`;
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      controller.current?.abort();
    };
  }, []);
  async function inspect() {
    if (busy) return;
    setBusy(true);
    setError(null);
    controller.current = new AbortController();
    try {
      const value = await api.get<Detail>(
        `${base}?${query}`,
        controller.current.signal,
      );
      if (alive.current) setDetail(value);
    } catch (cause) {
      if (alive.current && !controller.current.signal.aborted) setError(cause);
    } finally {
      if (alive.current) setBusy(false);
    }
  }
  async function download(file: File, kind: "input" | "output" | "log") {
    setError(null);
    try {
      const data = await api.blob(
        `${base}/files/${kind}/${encodeURIComponent(file.name)}?${query}`,
      );
      if (!alive.current) return;
      const url = URL.createObjectURL(data),
        link = document.createElement("a");
      link.href = url;
      link.download = file.name;
      document.body.append(link);
      link.click();
      link.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (cause) {
      if (alive.current) setError(cause);
    }
  }
  return (
    <article className="rounded-md border p-3 flex flex-col gap-2">
      <strong>
        {item.engine === "monty" ? "Monty tool composition" : "Isolated Python"}{" "}
        — {item.status}
      </strong>
      <p>
        {item.phase.replaceAll("_", " ")} · {item.duration_seconds ?? 0} seconds
      </p>
      {item.detail && <p>{item.detail}</p>}
      <p>
        {item.legacy_attribution
          ? "Historical record: colleague attribution was not captured."
          : `Recorded actor: ${item.actor_id ?? "unknown"}${item.assignment_id ? `; assignment ${item.assignment_id}` : ""}.`}
      </p>
      {error != null && (
        <p role="alert" className="text-destructive">
          {errorText(error)}
        </p>
      )}
      <Button
        type="button"
        variant="outline"
        disabled={busy}
        onClick={() => void inspect()}
      >
        {busy ? "Checking saved hashes…" : "Inspect code and evidence"}
      </Button>
      {detail && (
        <>
          <p>
            {detail.record_integrity === "verified"
              ? "Receipt, code and saved-file hashes verified."
              : "Legacy code and input/output hashes checked; a complete receipt hash was not captured."}
          </p>
          <details>
            <summary>Executed code</summary>
            <pre className="whitespace-pre-wrap break-all">{detail.code}</pre>
            <p className="break-all">Code SHA-256: {detail.code_sha256}</p>
          </details>
          <details>
            <summary>Runtime and reviewed limits</summary>
            <pre className="whitespace-pre-wrap break-all">
              {JSON.stringify(
                { runtime: detail.runtime, limits: detail.limits },
                null,
                2,
              )}
            </pre>
          </details>
          {detail.input_values && (
            <details>
              <summary>Captured input values</summary>
              <pre className="whitespace-pre-wrap break-all">
                {JSON.stringify(detail.input_values, null, 2)}
              </pre>
              <p className="break-all">Input SHA-256: {detail.inputs_sha256}</p>
            </details>
          )}
          {(
            [
              ["input", detail.inputs],
              ["output", detail.outputs],
              ["log", detail.logs],
            ] as const
          ).map(([kind, files]) =>
            files?.length ? (
              <section key={kind}>
                <h4 className="font-medium">
                  {kind === "input"
                    ? "Selected input copies"
                    : kind === "output"
                      ? "Validated output files"
                      : "Full recorded logs"}
                </h4>
                {files.map((file) => (
                  <div key={file.name} className="my-2">
                    <p>
                      {file.name} · {file.size_bytes} bytes
                      {file.source_version
                        ? ` · source version ${file.source_version}`
                        : ""}
                    </p>
                    <p className="break-all">SHA-256: {file.sha256}</p>
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => void download(file, kind)}
                    >
                      Download {file.name}
                    </Button>
                  </div>
                ))}
              </section>
            ) : null,
          )}
          <details>
            <summary>Result and callback receipts</summary>
            <pre className="whitespace-pre-wrap break-all">
              {JSON.stringify(
                { output: detail.output, calls: detail.calls },
                null,
                2,
              )}
            </pre>
          </details>
          {(detail.stdout || detail.stderr || detail.printed) && (
            <details>
              <summary>Printed text and log preview</summary>
              <p>
                Each preview shows up to 12,000 characters. Full Python logs are
                available above when longer.
              </p>
              <pre className="whitespace-pre-wrap break-all">
                {[detail.printed, detail.stdout, detail.stderr]
                  .filter(Boolean)
                  .join("\n")}
              </pre>
            </details>
          )}
        </>
      )}
    </article>
  );
}
