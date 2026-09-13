import { useDeferredValue, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { tenderPath, useApi, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";

export function EvidencePicker({
  tenderId,
  selected,
  onChange,
}: {
  tenderId: string;
  selected: string[];
  onChange: (ids: string[]) => void;
}) {
  const api = useApi();
  const [query, setQuery] = useState(""),
    [labels, setLabels] = useState<Record<string, string>>({});
  const deferred = useDeferredValue(query.trim());
  const results = useQuery({
    queryKey: [tenderPath(tenderId), "evidence-picker", deferred],
    queryFn: () =>
      api.get<Schema<"Evidence">[]>(
        `${tenderPath(tenderId)}/search?q=${encodeURIComponent(deferred)}`,
      ),
    enabled: !!deferred,
  });
  return (
    <div className="evidence-picker">
      <label>
        Supporting sources
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search imported source text…"
        />
      </label>
      {selected.length ? (
        <div className="selected-evidence">
          {selected.map((id, index) => (
            <span key={id}>
              {labels[id] ?? `Saved source ${index + 1}`}
              <button
                type="button"
                className="icon-button"
                aria-label={`Remove source ${index + 1}`}
                onClick={() =>
                  onChange(selected.filter((value) => value !== id))
                }
              >
                <X size={13} />
              </button>
            </span>
          ))}
        </div>
      ) : null}
      <ErrorNotice error={results.error} />
      {deferred && results.isPending ? (
        <Loading>Searching sources…</Loading>
      ) : null}
      {deferred && results.data ? (
        <div className="evidence-options">
          {results.data.length === 0 ? (
            <p className="muted">No matching source text.</p>
          ) : (
            results.data.slice(0, 8).map((source) => (
              <button
                type="button"
                disabled={selected.includes(source.id)}
                key={source.id}
                onClick={() => {
                  onChange([...selected, source.id]);
                  setLabels((current) => ({
                    ...current,
                    [source.id]: `${source.artifact_name} · ${source.locator}`,
                  }));
                  setQuery("");
                }}
              >
                <strong>
                  {source.artifact_name} · {source.locator}
                </strong>
                <p>{source.text.slice(0, 240)}</p>
              </button>
            ))
          )}
        </div>
      ) : null}
    </div>
  );
}
