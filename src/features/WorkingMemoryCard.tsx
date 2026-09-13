import { useMemo, useState } from "react";
import { errorText, tenderPath, useApi, type Schema } from "../api";
import type { SourceSelection } from "./Sources";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

type WorkingMemory = Schema<"WorkingMemoryRecord">;
type Promotion = Schema<"MemoryPromotionRequest">;

export function WorkingMemoryCard({
  tenderId,
  note,
  onSource,
  onPromoted,
}: {
  tenderId: string;
  note: WorkingMemory;
  onSource: (source: SourceSelection) => void;
  onPromoted: (record: Schema<"KnowledgeRecord">) => void;
}) {
  const api = useApi();
  const [category, setCategory] = useState<Promotion["category"]>("reference");
  const [verifiedOn, setVerifiedOn] = useState("");
  const [recheckAfter, setRecheckAfter] = useState("");
  const [rationale, setRationale] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [promoted, setPromoted] = useState(false);
  const [advanced, setAdvanced] = useState(false);
  const command = useMemo<Promotion>(
    () => ({
      category,
      engineer_confirmed: true,
      rationale: rationale.trim(),
      verified_on: verifiedOn || null,
      recheck_after: recheckAfter || null,
    }),
    [category, rationale, recheckAfter, verifiedOn],
  );

  async function promote() {
    if (!confirmed || !rationale.trim() || pending) return;
    setPending(true);
    setError(null);
    try {
      const record = await api.post<Schema<"KnowledgeRecord">>(
        `${tenderPath(tenderId)}/memory/${encodeURIComponent(note.id)}/promote`,
        command,
      );
      setPromoted(true);
      onPromoted(record);
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }

  return (
    <article className="rounded-lg border p-3">
      <div className="flex items-start justify-between gap-2">
        <div>
          <strong>{note.title}</strong>
          <p className="text-xs text-muted-foreground">
            {note.kind} · saved {formatDate(note.created_at)}
          </p>
        </div>
        <Badge variant={note.state === "current" ? "secondary" : "destructive"}>
          {note.state === "current" ? "Current" : "Needs review"}
        </Badge>
      </div>
      <p className="mt-2 text-sm" dir="auto">
        {note.content}
      </p>
      <p className="mt-2 text-xs text-muted-foreground">
        Valid until: {note.valid_until ?? "No date recorded"}
      </p>
      {note.review_reasons.length ? (
        <ul className="mt-2 list-disc pl-5 text-xs text-destructive">
          {note.review_reasons.map((reason) => (
            <li key={reason}>{reasonLabel(reason)}</li>
          ))}
        </ul>
      ) : null}
      {note.dependencies.length ? (
        <section className="mt-3" aria-label="Working note sources">
          <h4 className="text-xs font-medium">Supporting sources</h4>
          <div className="mt-1 flex flex-col gap-1">
            {note.dependencies.map((dependency, index) => (
              <button
                type="button"
                className="office-reference-link"
                key={dependency.source_id}
                aria-label={`Open working note source ${index + 1}`}
                onClick={() =>
                  onSource({
                    sourceId: dependency.source_id,
                    artifactId: dependency.artifact_id,
                    version: dependency.artifact_version,
                    contentHash: dependency.content_hash,
                  })
                }
              >
                Source {index + 1} · version {dependency.artifact_version} ·{" "}
                {dependency.current ? "current" : "changed"}
              </button>
            ))}
          </div>
        </section>
      ) : null}
      <details
        className="mt-3 text-xs"
        onToggle={(event) => setAdvanced(event.currentTarget.open)}
      >
        <summary
          className="cursor-pointer"
          aria-label={`More options for ${note.title}`}
        >
          More options
        </summary>
        {advanced ? (
          <>
            <dl className="mt-2 grid gap-1">
              <Proof label="Working note ID" value={note.id} />
              {note.dependencies.map((dependency, index) => (
                <div key={dependency.source_id}>
                  <Proof
                    label={`Source ${index + 1} content SHA-256`}
                    value={dependency.content_hash}
                  />
                  <Proof
                    label={`Source ${index + 1} evidence SHA-256`}
                    value={dependency.evidence_hash}
                  />
                </div>
              ))}
            </dl>
            {note.state === "needs_review" ? (
              <p className="mt-3 text-muted-foreground">
                Inspect the changed sources and save a current working note
                before promotion. A stale note cannot become company knowledge.
              </p>
            ) : promoted ? (
              <p className="mt-3">
                Promoted. Inspect it in Settings → Knowledge.
              </p>
            ) : (
              <form
                className="mt-3 flex flex-col gap-2"
                onSubmit={(event) => {
                  event.preventDefault();
                  void promote();
                }}
              >
                <label>
                  Knowledge category
                  <select
                    value={category}
                    disabled={pending}
                    onChange={(event) => {
                      setCategory(event.target.value as Promotion["category"]);
                      setConfirmed(false);
                    }}
                  >
                    <option value="reference">Reference</option>
                    <option value="method">Method</option>
                    <option value="preference">Preference</option>
                    <option value="price">Price</option>
                    <option value="tax">Tax</option>
                  </select>
                </label>
                <label>
                  Verified on
                  <Input
                    type="date"
                    value={verifiedOn}
                    disabled={pending}
                    onChange={(event) => {
                      setVerifiedOn(event.target.value);
                      setConfirmed(false);
                    }}
                  />
                </label>
                <label>
                  Recheck after
                  <Input
                    type="date"
                    value={recheckAfter}
                    disabled={pending}
                    onChange={(event) => {
                      setRecheckAfter(event.target.value);
                      setConfirmed(false);
                    }}
                  />
                </label>
                <label>
                  Promotion rationale
                  <Textarea
                    required
                    maxLength={4000}
                    value={rationale}
                    disabled={pending}
                    onChange={(event) => {
                      setRationale(event.target.value);
                      setConfirmed(false);
                    }}
                  />
                </label>
                <label className="checkbox-label">
                  <input
                    type="checkbox"
                    checked={confirmed}
                    disabled={pending}
                    onChange={(event) => setConfirmed(event.target.checked)}
                  />
                  I reviewed this exact note and its supporting sources for
                  reuse.
                </label>
                {error ? (
                  <p role="alert" className="text-destructive">
                    {errorText(error)}
                  </p>
                ) : null}
                <Button
                  type="submit"
                  size="sm"
                  disabled={pending || !confirmed || !rationale.trim()}
                >
                  {pending ? "Promoting…" : "Promote to company knowledge"}
                </Button>
              </form>
            )}
          </>
        ) : null}
      </details>
    </article>
  );
}

function Proof({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-muted-foreground">{label}</dt>
      <dd>
        <code className="wrap-anywhere">{value}</code>
      </dd>
    </div>
  );
}

function reasonLabel(value: string) {
  if (value === "source_revision_changed") return "A supporting file changed.";
  if (value === "source_evidence_changed")
    return "Supporting source text changed.";
  if (value === "source_unavailable")
    return "A supporting source is unavailable.";
  if (value === "validity_date_reached")
    return "The validity date was reached.";
  return value.replaceAll("_", " ");
}

function formatDate(value: string) {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}
