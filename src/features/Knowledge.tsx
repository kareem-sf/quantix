import { createDraftScope, useFormDraft } from "./useFormDraft";
import { FieldError } from "../components/FieldError";
import { useCallback, useState, type FormEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  useApi,
  useRefresh,
  useResource,
  tenderPath,
  type Schema,
} from "../api";
import {
  Empty,
  ErrorNotice,
  Loading,
  Modal,
  Status,
} from "../components/common";
import { EvidencePicker } from "./EvidencePicker";
import { Citations, SourceDrawer, type SourceSelection } from "./Sources";

type Category = Schema<"KnowledgeCreate">["category"];
type SourceVisit = { tenderId: string; selection: SourceSelection };
const categories: Record<Category, string> = {
  preference: "Preference",
  method: "Method",
  reference: "Reference",
  price: "Price",
  tax: "Tax",
};
const recheckReasons: Record<
  Schema<"KnowledgeRecord">["revalidation_reasons"][number],
  string
> = {
  source_revision_changed: "A supporting file has a newer version.",
  source_evidence_changed: "The supporting source text has changed.",
  source_unavailable: "A supporting source is unavailable.",
  recheck_date_reached: "The recorded recheck date has been reached.",
  commercial_use_requires_fresh_validation:
    "Prices and tax rules require current verification before commercial use.",
  withdrawn: "This note was withdrawn and must not be reused.",
};

export function Knowledge() {
  const [category, setCategory] = useState<Category | "all">("all"),
    [includeWithdrawn, setIncludeWithdrawn] = useState(false),
    [offset, setOffset] = useState(0),
    [creating, setCreating] = useState(false),
    [selected, setSelected] = useState<string | null>(null),
    [source, setSource] = useState<SourceVisit | null>(null);
  const params = new URLSearchParams({
    include_withdrawn: String(includeWithdrawn),
    offset: String(offset),
    limit: "50",
  });
  if (category !== "all") params.set("category", category);
  const notes = useResource<Schema<"KnowledgeRecord">[]>(
    `/knowledge?${params}`,
  );
  const closeCreate = useCallback(() => setCreating(false), []),
    closeDetail = useCallback(() => setSelected(null), []),
    closeSource = useCallback(() => setSource(null), []);
  return (
    <section className="knowledge" aria-labelledby="knowledge-heading">
      <div className="section-heading">
        <h2 id="knowledge-heading">Reusable notes</h2>
        <button
          type="button"
          className="button"
          onClick={() => setCreating(true)}
        >
          New reusable note
        </button>
      </div>
      <p className="muted knowledge-intro">
        Only notes you approve here can be reused. Each tender still needs its
        own source review.
      </p>
      <div className="knowledge-filters">
        <label>
          Category
          <select
            aria-label="Filter note category"
            value={category}
            onChange={(event) => {
              setCategory(event.target.value as Category | "all");
              setOffset(0);
            }}
          >
            <option value="all">All categories</option>
            {Object.entries(categories).map(([value, label]) => (
              <option value={value} key={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Status
          <select
            aria-label="Filter note status"
            value={includeWithdrawn ? "all" : "approved"}
            onChange={(event) => {
              setIncludeWithdrawn(event.target.value === "all");
              setOffset(0);
            }}
          >
            <option value="approved">Approved notes</option>
            <option value="all">Include withdrawn notes</option>
          </select>
        </label>
      </div>
      <ErrorNotice error={notes.error} />
      {notes.isPending ? <Loading>Loading reusable notes…</Loading> : null}
      {notes.data?.length === 0 ? (
        <Empty
          title={
            offset > 0
              ? "No more notes on this page"
              : category === "all"
                ? "No reusable notes yet"
                : "No notes in this category"
          }
        >
          {offset > 0
            ? "Return to the previous page to inspect earlier notes."
            : "Approve a working preference, method or reference when it is suitable for reuse."}
        </Empty>
      ) : null}
      <div className="knowledge-list">
        {notes.data?.map((note) => (
          <article className="knowledge-row" key={note.id}>
            <div className="knowledge-row-heading">
              <button
                type="button"
                className="knowledge-title"
                onClick={() => setSelected(note.id)}
              >
                {note.title}
              </button>
              <Status value={note.status} />
            </div>
            <div className="knowledge-meta">
              <span>{categories[note.category]}</span>
              <span>Approved {dateTime(note.approved_at)}</span>
              {note.needs_recheck ? (
                <span className="knowledge-recheck-label">
                  Check before reuse
                </span>
              ) : null}
            </div>
            <p className="knowledge-excerpt">{note.content}</p>
            {note.source_tender_name ? (
              <p className="field-help">
                Supporting tender: {note.source_tender_name}
              </p>
            ) : null}
            {note.category === "price" || note.category === "tax" ? (
              <p className="field-help knowledge-commercial">
                Current verification is required before using price or tax
                notes.
              </p>
            ) : null}
          </article>
        ))}
      </div>
      {offset > 0 || (notes.data?.length ?? 0) >= 50 ? (
        <div className="pagination">
          <span>
            {notes.data?.length
              ? `Notes ${offset + 1}–${offset + notes.data.length}`
              : `Page ${offset / 50 + 1}`}
          </span>
          <button
            type="button"
            className="button"
            disabled={offset === 0}
            onClick={() => setOffset((value) => Math.max(0, value - 50))}
          >
            Previous notes
          </button>
          <button
            type="button"
            className="button"
            disabled={(notes.data?.length ?? 0) < 50}
            onClick={() => setOffset((value) => value + 50)}
          >
            Next notes
          </button>
        </div>
      ) : null}
      {creating ? (
        <CreateNote
          onClose={closeCreate}
          onSaved={(record) => {
            setCreating(false);
            setSelected(record.id);
          }}
          onSource={setSource}
        />
      ) : null}
      {selected ? (
        <NoteDetail id={selected} onClose={closeDetail} onSource={setSource} />
      ) : null}
      {source ? (
        <NoteSource
          key={
            source.tenderId +
            ("sourceId" in source.selection
              ? source.selection.sourceId
              : source.selection.artifactId)
          }
          visit={source}
          onClose={closeSource}
        />
      ) : null}
    </section>
  );
}

function CreateNote({
  onClose,
  onSaved,
  onSource,
}: {
  onClose: () => void;
  onSaved: (note: Schema<"KnowledgeRecord">) => void;
  onSource: (source: SourceVisit) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const tenders = useResource<Schema<"Tender">[]>("/tenders");
  const draft = useFormDraft(
    createDraftScope("knowledge", "office", "new-note", 1),
    {
      title: "",
      content: "",
      category: "preference" as Category,
      sourceTender: "",
      sourceIds: [] as string[],
      verifiedOn: "",
      recheckAfter: "",
      rationale: "",
    },
    [
      "title",
      "content",
      "category",
      "sourceTender",
      "sourceIds",
      "verifiedOn",
      "recheckAfter",
      "rationale",
    ],
  );
  const {
    title,
    content,
    category,
    sourceTender,
    sourceIds,
    verifiedOn,
    recheckAfter,
    rationale,
  } = draft.value;
  const setTitle = (v: string) => draft.setField("title", v),
    setContent = (v: string) => draft.setField("content", v),
    setCategory = (v: Category) => draft.setField("category", v),
    setSourceTender = (v: string) => draft.setField("sourceTender", v),
    setSourceIds = (v: string[]) => draft.setField("sourceIds", v),
    setVerifiedOn = (v: string) => draft.setField("verifiedOn", v),
    setRecheckAfter = (v: string) => draft.setField("recheckAfter", v),
    setRationale = (v: string) => draft.setField("rationale", v);
  const [confirmed, setConfirmed] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null),
    [scopeNotice, setScopeNotice] = useState("");
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!confirmed || pending) return;
    setPending(true);
    setError(null);
    try {
      const acceptedRevision = draft.revision;
      const record = await api.post<Schema<"KnowledgeRecord">>("/knowledge", {
        title: title.trim(),
        content: content.trim(),
        category,
        source_tender_id: sourceTender || null,
        source_ids: sourceIds,
        verified_on: verifiedOn || null,
        recheck_after: recheckAfter || null,
        engineer_confirmed: true,
        rationale: rationale.trim(),
      } satisfies Schema<"KnowledgeCreate">);
      draft.markAccepted(acceptedRevision);
      await refresh();
      onSaved(record);
    } catch (failure) {
      setError(failure);
    } finally {
      setPending(false);
    }
  }
  return (
    <Modal title="Approve reusable note" onClose={onClose}>
      <form className="knowledge-form" onSubmit={submit}>
        <p className="muted">
          Record guidance you have reviewed for future use. Supporting documents
          remain evidence from their original tender.
        </p>
        <fieldset disabled={pending}>
          <label>
            Title
            <input
              required
              maxLength={200}
              value={title}
              onChange={(event) => setTitle(event.target.value)}
            />
            <FieldError error={error} name="title" />
          </label>
          <label>
            Category
            <select
              value={category}
              onChange={(event) => setCategory(event.target.value as Category)}
            >
              {Object.entries(categories).map(([value, label]) => (
                <option value={value} key={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Note
            <textarea
              required
              rows={5}
              maxLength={20000}
              value={content}
              onChange={(event) => setContent(event.target.value)}
            />
            <FieldError error={error} name="content" />
          </label>
          {category === "price" || category === "tax" ? (
            <p className="knowledge-commercial field-help">
              Prices and tax rules require current verification before
              commercial use.
            </p>
          ) : null}
          <div className="knowledge-date-fields">
            <label>
              Verified on
              <input
                type="date"
                value={verifiedOn}
                onChange={(event) => setVerifiedOn(event.target.value)}
              />
            </label>
            <label>
              Recheck after
              <input
                type="date"
                value={recheckAfter}
                onChange={(event) => setRecheckAfter(event.target.value)}
              />
            </label>
          </div>
          <p className="field-help">
            Dates are optional. Record only a verification you have carried out.
          </p>
          <label>
            Supporting tender
            <select
              value={sourceTender}
              disabled={tenders.isPending}
              onChange={(event) => {
                setSourceTender(event.target.value);
                if (sourceIds.length)
                  setScopeNotice(
                    "Supporting sources were cleared when the tender changed.",
                  );
                setSourceIds([]);
              }}
            >
              <option value="">No supporting tender</option>
              {tenders.data?.map((tender) => (
                <option value={tender.id} key={tender.id}>
                  {tender.name}
                </option>
              ))}
            </select>
          </label>
          <ErrorNotice error={tenders.error} />
          {scopeNotice ? (
            <p className="field-help" role="status">
              {scopeNotice}
            </p>
          ) : null}
          {sourceTender ? (
            <>
              <EvidencePicker
                key={sourceTender}
                tenderId={sourceTender}
                selected={sourceIds}
                onChange={(ids) => {
                  if (ids.length > 100)
                    setError(new Error("Choose up to 100 supporting sources."));
                  else setSourceIds(ids);
                }}
              />
              {sourceIds.length ? (
                <div className="knowledge-source-check">
                  <p className="field-help">
                    Inspect the selected source references:
                  </p>
                  <Citations
                    ids={sourceIds}
                    tenderId={sourceTender}
                    onOpen={(selection) =>
                      onSource({ tenderId: sourceTender, selection })
                    }
                  />
                </div>
              ) : null}
            </>
          ) : null}
          <label>
            Approval note
            <textarea
              required
              rows={2}
              maxLength={4000}
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
            />
            <FieldError error={error} name="rationale" />
          </label>
          <label className="knowledge-confirmation">
            <input
              type="checkbox"
              required
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
            />
            I approve this note for reuse.
          </label>
        </fieldset>
        <ErrorNotice error={error} />
        <div className="form-actions">
          <button
            type="button"
            className="button"
            disabled={pending}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="button primary"
            disabled={
              pending ||
              !confirmed ||
              !title.trim() ||
              !content.trim() ||
              !rationale.trim()
            }
          >
            {pending ? "Saving…" : "Approve and save note"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function NoteDetail({
  id,
  onClose,
  onSource,
}: {
  id: string;
  onClose: () => void;
  onSource: (source: SourceVisit) => void;
}) {
  const note = useResource<Schema<"KnowledgeRecord">>(
    `/knowledge/${encodeURIComponent(id)}`,
  );
  if (!note.data)
    return (
      <Modal title="Reusable note" onClose={onClose}>
        {note.isPending ? (
          <Loading>Loading the approved note…</Loading>
        ) : (
          <ErrorNotice error={note.error} />
        )}
      </Modal>
    );
  return (
    <NoteRecord
      record={note.data}
      error={note.error}
      onClose={onClose}
      onSource={onSource}
    />
  );
}

function NoteRecord({
  record,
  error,
  onClose,
  onSource,
}: {
  record: Schema<"KnowledgeRecord">;
  error: unknown;
  onClose: () => void;
  onSource: (source: SourceVisit) => void;
}) {
  const [withdrawing, setWithdrawing] = useState(false);
  const closeWithdrawal = useCallback(() => setWithdrawing(false), []);
  const reasons = record.revalidation_reasons.map(
    (reason) => recheckReasons[reason],
  );
  const commercial = record.category === "price" || record.category === "tax";
  if (
    commercial &&
    !reasons.includes(recheckReasons.commercial_use_requires_fresh_validation)
  )
    reasons.push(recheckReasons.commercial_use_requires_fresh_validation);
  return (
    <Modal drawer title={record.title} onClose={onClose}>
      <div className="knowledge-detail">
        <ErrorNotice error={error} />
        <div className="knowledge-meta">
          <span>{categories[record.category]}</span>
          <Status value={record.status} />
        </div>
        <p className="knowledge-content">{record.content}</p>
        {record.needs_recheck || commercial ? (
          <section className="knowledge-recheck">
            <h3>Check before reuse</h3>
            {reasons.length ? (
              <ul>
                {reasons.map((reason) => (
                  <li key={reason}>{reason}</li>
                ))}
              </ul>
            ) : (
              <p>This note needs another review before reuse.</p>
            )}
          </section>
        ) : null}
        <p className="field-help">{record.use_limitations}</p>
        <dl className="knowledge-dates">
          <dt>Approved</dt>
          <dd>{dateTime(record.approved_at)}</dd>
          <dt>Verified on</dt>
          <dd>{record.verified_on ?? "Not recorded"}</dd>
          <dt>Recheck after</dt>
          <dd>{record.recheck_after ?? "Not recorded"}</dd>
        </dl>
        <section className="knowledge-detail-section">
          <h3>Approval</h3>
          <p>{record.approval_rationale}</p>
        </section>
        <section className="knowledge-detail-section">
          <h3>Supporting sources</h3>
          <p className="muted">
            {record.source_tender_name
              ? `Original tender: ${record.source_tender_name}`
              : record.source_tender_id
                ? "Supporting tender name unavailable."
                : "No supporting tender was recorded."}
          </p>
          {record.sources.length ? (
            record.sources.map((source) => (
              <article className="knowledge-source" key={source.source_id}>
                <button
                  type="button"
                  className="source-chip"
                  disabled={!source.available}
                  onClick={() =>
                    onSource({
                      tenderId: record.source_tender_id ?? source.tender_id,
                      selection: { sourceId: source.source_id },
                    })
                  }
                >
                  {source.artifact_name} · {source.locator}
                </button>
                <p className="field-help">
                  {source.relative_path} · Saved version {source.version}
                </p>
                <p className="field-help">
                  {!source.available
                    ? "Source unavailable"
                    : source.is_current
                      ? "Current file version"
                      : "An earlier file version was approved with this note."}
                </p>
                <details>
                  <summary>Recorded source identity</summary>
                  <dl>
                    <dt>File hash</dt>
                    <dd>
                      <code>{source.content_hash}</code>
                    </dd>
                    <dt>Source text hash</dt>
                    <dd>
                      <code>{source.evidence_hash}</code>
                    </dd>
                  </dl>
                </details>
              </article>
            ))
          ) : (
            <p className="field-help">No source references were recorded.</p>
          )}
        </section>
        {record.status === "withdrawn" ? (
          <section className="knowledge-detail-section">
            <h3>Withdrawal</h3>
            <p>{record.withdrawal_rationale}</p>
            {record.withdrawn_at ? (
              <p className="field-help">{dateTime(record.withdrawn_at)}</p>
            ) : null}
          </section>
        ) : (
          <button
            type="button"
            className="button knowledge-withdraw"
            onClick={() => setWithdrawing(true)}
          >
            Withdraw note
          </button>
        )}
        {record.audit.length ? (
          <details className="knowledge-audit">
            <summary>Decision history</summary>
            {record.audit.map((entry) => (
              <article key={entry.id}>
                <strong>
                  {entry.action === "approve" ? "Approval" : "Withdrawal"}
                </strong>
                <p>{entry.rationale}</p>
                <time dateTime={entry.created_at}>
                  {dateTime(entry.created_at)}
                </time>
              </article>
            ))}
          </details>
        ) : null}
        {withdrawing ? (
          <WithdrawNote record={record} onClose={closeWithdrawal} />
        ) : null}
      </div>
    </Modal>
  );
}

function WithdrawNote({
  record,
  onClose,
}: {
  record: Schema<"KnowledgeRecord">;
  onClose: () => void;
}) {
  const api = useApi(),
    refresh = useRefresh(),
    client = useQueryClient();
  const draft = useFormDraft(
    createDraftScope(
      "knowledge",
      record.source_tender_id ?? "office",
      `withdraw-${record.id}`,
      1,
    ),
    { rationale: "" },
    ["rationale"],
  );
  const { rationale } = draft.value;
  const setRationale = (value: string) => draft.setField("rationale", value);
  const [confirmed, setConfirmed] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<unknown>(null);
  return (
    <Modal title="Withdraw reusable note" onClose={onClose}>
      <form
        className="knowledge-form"
        onSubmit={async (event) => {
          event.preventDefault();
          if (!confirmed || pending) return;
          setPending(true);
          setError(null);
          try {
            const acceptedRevision = draft.revision;
            const result = await api.post<Schema<"KnowledgeRecord">>(
              `/knowledge/${record.id}/withdraw`,
              {
                engineer_confirmed: true,
                rationale: rationale.trim(),
              } satisfies Schema<"KnowledgeDecision">,
            );
            draft.markAccepted(acceptedRevision);
            client.setQueryData([`/knowledge/${record.id}`], result);
            await refresh();
            onClose();
          } catch (failure) {
            setError(failure);
          } finally {
            setPending(false);
          }
        }}
      >
        <p className="muted">
          Withdraw “{record.title}” from reuse. Its approved text, sources and
          decision history remain available.
        </p>
        <fieldset disabled={pending}>
          <label>
            Reason for withdrawal
            <textarea
              required
              rows={3}
              maxLength={4000}
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
            />
            <FieldError error={error} name="rationale" />
          </label>
          <label className="knowledge-confirmation">
            <input
              type="checkbox"
              required
              checked={confirmed}
              onChange={(event) => setConfirmed(event.target.checked)}
            />
            I confirm this note should no longer be reused.
          </label>
        </fieldset>
        <ErrorNotice error={error} />
        <div className="form-actions">
          <button
            type="button"
            className="button"
            disabled={pending}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="button primary"
            disabled={pending || !confirmed || !rationale.trim()}
          >
            {pending ? "Recording…" : "Confirm withdrawal"}
          </button>
        </div>
      </form>
    </Modal>
  );
}

function NoteSource({
  visit,
  onClose,
}: {
  visit: SourceVisit;
  onClose: () => void;
}) {
  const artifacts = useResource<Schema<"Artifact">[]>(
    `${tenderPath(visit.tenderId)}/artifacts?include_history=true`,
  );
  if (!artifacts.data)
    return (
      <Modal drawer title="Supporting source" onClose={onClose}>
        {artifacts.isPending ? (
          <Loading>Opening the original tender source…</Loading>
        ) : (
          <>
            <ErrorNotice error={artifacts.error} />
            <button
              type="button"
              className="button"
              onClick={() => void artifacts.refetch()}
            >
              Try again
            </button>
          </>
        )}
      </Modal>
    );
  return (
    <SourceDrawer
      tenderId={visit.tenderId}
      selection={visit.selection}
      artifacts={artifacts.data}
      onClose={onClose}
    />
  );
}

function dateTime(value: string) {
  return new Date(value).toLocaleString();
}
