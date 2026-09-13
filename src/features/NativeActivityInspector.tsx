import { useState } from "react";
import { errorText, tenderPath, useApi, type Schema } from "../api";
import { NativeCleanupReview } from "./NativeCleanupReview";
import { NativeArtifactCard, NativeSessionCard } from "./NativeActivityCards";

type CapabilityMap = Record<string, Schema<"NativeClientCapability">[]>;

export function NativeActivityInspector({
  tenderId,
  runId,
  profileId,
  assignmentId,
}: {
  tenderId: string;
  runId: string;
  profileId?: string;
  assignmentId?: string;
}) {
  const api = useApi();
  const [sessions, setSessions] = useState<Schema<"NativeSessionBinding">[]>(
    [],
  );
  const [artifacts, setArtifacts] = useState<Schema<"NativeArtifact">[]>([]);
  const [cleanup, setCleanup] = useState<Schema<"NativeCleanupReceipt">[]>([]);
  const [capabilities, setCapabilities] = useState<CapabilityMap>({});
  const [loaded, setLoaded] = useState(false);
  const [loading, setLoading] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);
  const [downloading, setDownloading] = useState<string | null>(null);

  async function load() {
    if (loading) return;
    setLoading(true);
    setErrors([]);
    const base = tenderPath(tenderId);
    const [sessionResult, artifactResult, cleanupResult] =
      await Promise.allSettled([
        api.get<Schema<"NativeSessionBinding">[]>(`${base}/native-sessions`),
        api.get<Schema<"NativeArtifact">[]>(
          `${base}/runs/${encodeURIComponent(runId)}/native-artifacts`,
        ),
        api.get<Schema<"NativeCleanupReceipt">[]>(`${base}/native-cleanup`),
      ]);
    const nextErrors: string[] = [];
    const nextSessions =
      sessionResult.status === "fulfilled"
        ? sessionResult.value.filter(
            (item) =>
              item.run_id === runId &&
              (!profileId || item.profile_id === profileId),
          )
        : [];
    const nextArtifacts =
      artifactResult.status === "fulfilled"
        ? artifactResult.value.filter(
            (item) =>
              (!profileId || item.actor_id === profileId) &&
              (!assignmentId || item.assignment_id === assignmentId),
          )
        : [];
    const nextCleanup =
      cleanupResult.status === "fulfilled"
        ? cleanupResult.value.filter((item) => item.run_id === runId)
        : [];
    for (const result of [sessionResult, artifactResult, cleanupResult]) {
      if (result.status === "rejected")
        nextErrors.push(errorText(result.reason));
    }
    setSessions(nextSessions);
    setArtifacts(nextArtifacts);
    setCleanup(nextCleanup);

    const connections = [
      ...new Set(nextSessions.map((item) => item.connection_id)),
    ];
    const capabilityResults = await Promise.allSettled(
      connections.map(
        async (connectionId) =>
          [
            connectionId,
            await api.get<Schema<"NativeClientCapability">[]>(
              `/ai/connections/${encodeURIComponent(connectionId)}/native-capabilities`,
            ),
          ] as const,
      ),
    );
    const nextCapabilities: CapabilityMap = {};
    for (const result of capabilityResults) {
      if (result.status === "fulfilled")
        nextCapabilities[result.value[0]] = result.value[1];
      else nextErrors.push(errorText(result.reason));
    }
    setCapabilities(nextCapabilities);
    setErrors([...new Set(nextErrors)]);
    setLoaded(true);
    setLoading(false);
  }

  async function download(artifact: Schema<"NativeArtifact">) {
    setDownloading(artifact.id);
    setErrors([]);
    try {
      const blob = await api.blob(
        `${tenderPath(tenderId)}/native-artifacts/${encodeURIComponent(artifact.id)}/content`,
      );
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = artifact.filename;
      document.body.append(link);
      link.click();
      link.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (failure) {
      setErrors([errorText(failure)]);
    } finally {
      setDownloading(null);
    }
  }

  const empty =
    loaded && !sessions.length && !artifacts.length && !cleanup.length;
  return (
    <details
      className="rounded-lg border p-3 text-sm"
      onToggle={(event) => {
        if (event.currentTarget.open && !loaded) void load();
      }}
    >
      <summary className="cursor-pointer font-medium">
        Provider sessions and generated files
      </summary>
      <div className="mt-3 flex flex-col gap-3">
        {loading ? <p role="status">Loading provider activity…</p> : null}
        {errors.map((message) => (
          <p role="alert" className="text-sm text-destructive" key={message}>
            {message}
          </p>
        ))}
        {empty ? (
          <p className="text-muted-foreground">
            No provider sessions or generated files are recorded for this work.
          </p>
        ) : null}
        {sessions.map((session) => (
          <NativeSessionCard
            key={session.id}
            session={session}
            capabilities={capabilities[session.connection_id] ?? []}
          />
        ))}
        {artifacts.map((artifact) => (
          <NativeArtifactCard
            key={artifact.id}
            artifact={artifact}
            downloading={downloading === artifact.id}
            onDownload={() => void download(artifact)}
          />
        ))}
        {cleanup.map((receipt) => (
          <NativeCleanupReview
            key={receipt.id}
            tenderId={tenderId}
            receipt={receipt}
            onReviewed={load}
          />
        ))}
      </div>
    </details>
  );
}
