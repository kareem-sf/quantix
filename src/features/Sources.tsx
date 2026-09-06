import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, FileText } from "lucide-react";
import { tenderPath, useApi, type Schema } from "../api";
import { ErrorNotice, Loading, Modal } from "../components/ui";
import { Measurements } from "./Measurements";

export type SourceSelection =
  { sourceId: string } | { artifactId: string; artifact?: Schema<"Artifact">; page?: number };
export function Citations({
  ids,
  tenderId,
  onOpen,
}: {
  ids: string[];
  tenderId: string;
  onOpen: (source: SourceSelection) => void;
}) {
  return ids.length ? (
    <div className="citations">
      {ids.map((id) => (
        <Citation key={id} id={id} tenderId={tenderId} onOpen={onOpen} />
      ))}
    </div>
  ) : null;
}
function Citation({
  id,
  tenderId,
  onOpen,
}: {
  id: string;
  tenderId: string;
  onOpen: (source: SourceSelection) => void;
}) {
  const api = useApi();
  const path = `${tenderPath(tenderId)}/evidence/${encodeURIComponent(id)}`;
  const evidence = useQuery({
    queryKey: [path],
    queryFn: () => api.get<Schema<"Evidence">>(path),
    staleTime: Infinity,
    retry: false,
  });
  return (
    <button
      type="button"
      className="source-chip"
      onClick={() => onOpen({ sourceId: id })}
      title={evidence.data?.relative_path}
    >
      {evidence.data
        ? `${evidence.data.artifact_name} · ${evidence.data.locator}`
        : evidence.isError
          ? "Source unavailable"
          : "Source…"}
    </button>
  );
}

export function SourceDrawer({
  tenderId,
  selection,
  artifacts,
  onClose,
}: {
  tenderId: string;
  selection: SourceSelection;
  artifacts: Schema<"Artifact">[];
  onClose: () => void;
}) {
  const api = useApi(),
    base = tenderPath(tenderId);
  const sourceId = "sourceId" in selection ? selection.sourceId : null;
  const source = useQuery({
    queryKey: [base, "source", sourceId],
    queryFn: () => api.get<Schema<"Evidence">>(`${base}/evidence/${sourceId}`),
    enabled: !!sourceId,
  });
  const artifactId =
    "artifactId" in selection ? selection.artifactId : source.data?.artifact_id;
  const knownArtifact =
    ("artifact" in selection ? selection.artifact : undefined) ??
    artifacts.find((item) => item.id === artifactId);
  const savedArtifact = useQuery({
    queryKey: [base, "artifact", artifactId],
    queryFn: () =>
      api.get<Schema<"Artifact">>(`${base}/artifacts/${artifactId}`),
    enabled: !!artifactId && !knownArtifact,
    retry: false,
  });
  const artifact = knownArtifact ?? savedArtifact.data;
  const [offset, setOffset] = useState(0);
  const evidence = useQuery({
    queryKey: [base, artifactId, "evidence", offset],
    queryFn: () =>
      api.get<Schema<"Evidence">[]>(
        `${base}/artifacts/${artifactId}/evidence?offset=${offset}&limit=30`,
      ),
    enabled: !!artifactId && !sourceId,
  });
  const [page, setPage] = useState<number | null>(null);
  const [measuring, setMeasuring] = useState(false);
  const [downloading, setDownloading] = useState(false),
    [downloadError, setDownloadError] = useState<unknown>(null);
  const activePage = page ?? ("artifactId" in selection ? selection.page : source.data?.page) ?? 1;
  const isPdf =
    artifact?.kind === "pdf" || artifact?.name.toLowerCase().endsWith(".pdf");
  const preview = useQuery({
    queryKey: [base, artifactId, "preview", activePage],
    queryFn: () =>
      api.blob(`${base}/artifacts/${artifactId}/preview?page=${activePage}`),
    enabled: !!artifactId && !!isPdf,
    retry: false,
  });
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!preview.data) {
      setImageUrl(null);
      return;
    }
    const url = URL.createObjectURL(preview.data);
    setImageUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [preview.data]);
  return (
    <Modal
      drawer
      title={artifact?.name ?? source.data?.artifact_name ?? "Source document"}
      onClose={onClose}
    >
      <div className="source-drawer-body">
        {measuring && artifactId ? (
          <Measurements
            tenderId={tenderId}
            artifactId={artifactId}
            onClose={() => setMeasuring(false)}
          />
        ) : (
          <>
            <p className="source-path">
              {artifact?.relative_path ?? source.data?.relative_path}
            </p>
            <div className="inline-actions">
              {isPdf && artifactId ? (
                <button
                  type="button"
                  className="button"
                  onClick={() => setMeasuring(true)}
                >
                  Measure drawing
                </button>
              ) : null}
              {artifact ? (
                <button
                  type="button"
                  className="button"
                  disabled={downloading}
                  onClick={async () => {
                    setDownloading(true);
                    setDownloadError(null);
                    try {
                      const blob = await api.blob(
                          `${base}/artifacts/${artifact.id}/original`,
                        ),
                        url = URL.createObjectURL(blob),
                        link = document.createElement("a");
                      link.href = url;
                      link.download = artifact.name;
                      document.body.append(link);
                      link.click();
                      link.remove();
                      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
                    } catch (failure) {
                      setDownloadError(failure);
                    } finally {
                      setDownloading(false);
                    }
                  }}
                >
                  {downloading ? "Downloading…" : "Download original"}
                </button>
              ) : null}
            </div>
            {artifact && !artifact.is_current ? (
              <p className="field-help">
                Earlier file revision · Version {artifact.version}. The original
                and source text remain available for review.
              </p>
            ) : null}
            <ErrorNotice
              error={
                source.error ||
                evidence.error ||
                savedArtifact.error ||
                downloadError
              }
            />
            {sourceId && source.isPending ? (
              <Loading>Loading source…</Loading>
            ) : null}
            {isPdf ? (
              <>
                <div className="preview-toolbar">
                  <button
                    className="icon-button"
                    disabled={activePage <= 1}
                    onClick={() => setPage(activePage - 1)}
                    aria-label="Previous page"
                  >
                    <ChevronLeft size={20} />
                  </button>
                  <span>Page {activePage}</span>
                  <button
                    className="icon-button"
                    onClick={() => setPage(activePage + 1)}
                    aria-label="Next page"
                  >
                    <ChevronRight size={20} />
                  </button>
                </div>
                {preview.isPending ? <Loading>Loading page…</Loading> : null}
                <ErrorNotice error={preview.error} />
                {imageUrl ? (
                  <img
                    className="source-preview"
                    src={imageUrl}
                    alt={`${artifact?.name ?? "Document"}, page ${activePage}`}
                  />
                ) : null}
              </>
            ) : null}
            {source.data ? (
              <SourceText evidence={source.data} highlighted />
            ) : null}
            {!sourceId ? (
              <>
                <h3>Source text</h3>
                {evidence.isPending ? (
                  <Loading>Loading source text…</Loading>
                ) : null}
                {evidence.data?.map((item) => (
                  <SourceText key={item.id} evidence={item} />
                ))}
                {evidence.data?.length === 0 ? (
                  <p className="muted">
                    No readable text is available for this document. Check its
                    reading status and reported issues.
                  </p>
                ) : null}
                <div className="inline-actions">
                  <button
                    className="button"
                    disabled={offset === 0}
                    onClick={() => setOffset(Math.max(0, offset - 30))}
                  >
                    Previous sources
                  </button>
                  <button
                    className="button"
                    disabled={!evidence.data || evidence.data.length < 30}
                    onClick={() => setOffset(offset + 30)}
                  >
                    Next sources
                  </button>
                </div>
              </>
            ) : null}
          </>
        )}
      </div>
    </Modal>
  );
}
function SourceText({
  evidence,
  highlighted = false,
}: {
  evidence: Schema<"Evidence">;
  highlighted?: boolean;
}) {
  const cells = evidence.metadata?.cells;
  return (
    <article className={`source-text ${highlighted ? "cited-text" : ""}`}>
      <h4>
        <FileText size={16} />
        {evidence.locator}
      </h4>
      <pre>{evidence.text}</pre>
      {Array.isArray(cells) && cells.length ? (
        <details>
          <summary>Cell values and formulas</summary>
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Cell</th>
                  <th>Value</th>
                  <th>Formula</th>
                  <th>Cached value</th>
                </tr>
              </thead>
              <tbody>
                {cells.map((cell: Record<string, unknown>, index: number) => (
                  <tr key={index}>
                    <td>{String(cell.coordinate ?? "")}</td>
                    <td>{String(cell.value ?? "")}</td>
                    <td>{String(cell.formula ?? "")}</td>
                    <td>{String(cell.cached_value ?? "")}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      ) : null}
    </article>
  );
}
