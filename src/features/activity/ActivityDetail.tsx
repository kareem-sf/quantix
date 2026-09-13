import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useApi } from "../../api";
import { ErrorNotice, Loading } from "../../components/common";
import { Button } from "../../components/ui/button";
import type { SourceSelection } from "../Sources";
import type { RunActivity, RunActivityDetail } from "./types";

export function ActivityDetail({
  base,
  activity,
  historyKey,
  onSource,
  onBack,
}: {
  base: string;
  activity: RunActivity;
  historyKey: string;
  onSource?: (source: SourceSelection) => void;
  onBack: () => void;
}) {
  const api = useApi();
  const [eventId, setEventId] = useState(activity.event_id);
  const [offsets, setOffsets] = useState([0]);
  const offset = offsets.at(-1)!;
  const path = `${base}/${eventId}?offset=${offset}&limit=16000`;
  const detail = useQuery({
    queryKey: [path, historyKey],
    queryFn: ({ signal }) => api.get<RunActivityDetail>(path, signal),
    staleTime: Infinity,
    retry: false,
  });
  const record = detail.data?.activity ?? activity;
  function openEvent(id: number) {
    setEventId(id);
    setOffsets([0]);
  }
  return (
    <section className="min-w-0 space-y-4" aria-label="Activity details">
      <Button variant="ghost" size="sm" onClick={onBack}>
        Back to activity
      </Button>
      <div>
        <h3 className="wrap-anywhere text-base font-medium">
          {record.message}
        </h3>
        <p className="text-sm text-muted-foreground">
          {record.actor_label} · {record.category} · {record.phase}
        </p>
      </div>
      {detail.data ? (
        <div className="flex flex-wrap gap-2" aria-label="Operation captures">
          {detail.data.input_event_id != null ? (
            <Button
              variant="outline"
              size="sm"
              disabled={eventId === detail.data.input_event_id}
              onClick={() => openEvent(detail.data!.input_event_id!)}
            >
              {record.tool ? "Inputs" : "First step"}
            </Button>
          ) : null}
          {detail.data.output_event_id != null &&
          detail.data.output_event_id !== detail.data.input_event_id ? (
            <Button
              variant="outline"
              size="sm"
              disabled={eventId === detail.data.output_event_id}
              onClick={() => openEvent(detail.data!.output_event_id!)}
            >
              {record.tool ? "Outcome" : "Latest outcome"}
            </Button>
          ) : null}
          {eventId !== activity.event_id ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => openEvent(activity.event_id)}
            >
              Selected event
            </Button>
          ) : null}
        </div>
      ) : null}
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 text-xs">
        <dt>Time</dt>
        <dd className="wrap-anywhere">
          {new Date(record.created_at).toLocaleString()}
        </dd>
        <dt>Capture</dt>
        <dd>{record.capture_status.replaceAll("_", " ")}</dd>
        {record.tool ? (
          <>
            <dt>Tool</dt>
            <dd className="wrap-anywhere">{record.tool}</dd>
          </>
        ) : null}
        {record.provider ? (
          <>
            <dt>Provider</dt>
            <dd className="wrap-anywhere">
              {record.provider} · {record.model ?? "Model not reported"}
            </dd>
          </>
        ) : null}
        {record.elapsed_ms != null ? (
          <>
            <dt>Elapsed</dt>
            <dd>{(record.elapsed_ms / 1000).toFixed(1)} seconds</dd>
          </>
        ) : null}
        {record.assignment_id ? (
          <>
            <dt>Assignment</dt>
            <dd className="wrap-anywhere">{record.assignment_id}</dd>
          </>
        ) : null}
        {record.operation_id ? (
          <>
            <dt>Operation</dt>
            <dd className="wrap-anywhere">{record.operation_id}</dd>
          </>
        ) : null}
        {record.parent_operation_id ? (
          <>
            <dt>Parent operation</dt>
            <dd className="wrap-anywhere">{record.parent_operation_id}</dd>
          </>
        ) : null}
      </dl>
      {onSource && record.artifact_refs?.length ? (
        <div className="flex flex-wrap gap-2">
          {record.artifact_refs.map((reference, index) => (
            <Button
              key={`${reference.artifact_id}:${reference.page ?? "file"}`}
              variant="outline"
              size="sm"
              onClick={() =>
                onSource({
                  artifactId: reference.artifact_id,
                  page: reference.page ?? undefined,
                })
              }
            >
              Open referenced file {index + 1}
              {reference.page ? ` · page ${reference.page}` : ""}
            </Button>
          ))}
        </div>
      ) : null}
      {onSource && record.source_ids?.length ? (
        <div className="flex flex-wrap gap-2">
          {record.source_ids.map((sourceId, index) => (
            <Button
              key={sourceId}
              variant="outline"
              size="sm"
              onClick={() => onSource({ sourceId })}
            >
              Open source {index + 1}
            </Button>
          ))}
        </div>
      ) : null}
      <ErrorNotice error={detail.error} />
      {detail.isPending ? <Loading>Loading captured detail…</Loading> : null}
      {detail.error ? (
        <Button variant="outline" onClick={() => void detail.refetch()}>
          Retry details
        </Button>
      ) : null}
      {detail.data ? (
        <>
          {detail.data.redacted ? (
            <p className="text-xs text-muted-foreground">
              Sensitive fields were removed from this capture.
            </p>
          ) : null}
          {detail.data.unavailable_fields?.length ? (
            <p className="text-xs text-muted-foreground">
              Not captured: {detail.data.unavailable_fields.join(", ")}
            </p>
          ) : null}
          <pre className="max-h-[50dvh] overflow-auto whitespace-pre-wrap rounded-md border bg-muted/30 p-3 font-mono text-xs leading-relaxed wrap-anywhere">
            {detail.data.text || "No readable detail was captured."}
          </pre>
          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <span>
              Characters {Math.min(offset + 1, detail.data.total_chars)}–
              {detail.data.next_offset} of {detail.data.total_chars}
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={offsets.length === 1}
              onClick={() => setOffsets((current) => current.slice(0, -1))}
            >
              Previous detail page
            </Button>
            <Button
              variant="outline"
              size="sm"
              disabled={!detail.data.has_more}
              onClick={() =>
                setOffsets((current) => [...current, detail.data!.next_offset])
              }
            >
              Next detail page
            </Button>
          </div>
        </>
      ) : null}
    </section>
  );
}
