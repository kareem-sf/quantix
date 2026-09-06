import { useCallback, useState, type FormEvent } from "react";
import { useQuery } from "@tanstack/react-query";
import { ClipboardCheck, Plus } from "lucide-react";
import { tenderPath, useApi, useRefresh, useResource, type Schema } from "../api";
import { ErrorNotice, Loading, Modal } from "../components/ui";
import { EvidencePicker } from "./EvidencePicker";
import { Citations, SourceDrawer, type SourceSelection } from "./Sources";
import "../styles/requirements.css";

type Requirement = Schema<"RequirementRecord">;
type Deliverable = Schema<"RequirementProposal">["deliverable_kind"];
type Decision = Schema<"RequirementDecision">["decision"];
const deliverables: Record<Deliverable, string> = {
  boq_xlsx: "Consolidated BOQ workbook",
  client_boq: "Priced client BOQ",
  analysis_docx: "Tender analysis",
  technical_docx: "Technical document",
  registers_xlsx: "Tender registers",
  comparison_xlsx: "Quotation comparison",
  programme_xlsx: "Construction programme",
};
const decisionLabels: Record<Decision, string> = {
  approve: "Approve this requirement",
  withdraw: "Withdraw this requirement",
  satisfied: "Reviewed — satisfied by linked documents",
  exception: "Reviewed — record an exception",
  reopen: "Reopen completion review",
};
const sourceReasons: Record<string, string> = {
  source_revision_changed: "A supporting file has a newer or different revision.",
  source_evidence_changed: "The supporting source text or its location has changed.",
  source_bytes_changed: "The saved source file no longer matches its recorded hash.",
  source_unavailable: "A preserved supporting source is unavailable.",
};

function reviewLabel(requirement: Requirement) {
  if (requirement.status === "withdrawn") return "Withdrawn";
  if (requirement.status === "proposed") return "Awaiting requirement approval";
  if (!requirement.is_current) return "Source needs attention";
  if (requirement.review_status === "pending") return "Completion review needed";
  if (!requirement.review_is_current) return "Completion review needs updating";
  return requirement.review_status === "satisfied" ? "Reviewed — satisfied" : "Reviewed — exception";
}

export function SubmissionRequirements({ tenderId }: { tenderId: string }) {
  return <RequirementRegister key={tenderId} tenderId={tenderId} />;
}

function RequirementRegister({ tenderId }: { tenderId: string }) {
  const api = useApi(), base = tenderPath(tenderId);
  const [creating, setCreating] = useState(false), [selected, setSelected] = useState<string | null>(null);
  const [includeWithdrawn, setIncludeWithdrawn] = useState(false), [offset, setOffset] = useState(0);
  const [source, setSource] = useState<SourceSelection | null>(null);
  const requirements = useResource<Requirement[]>(`${base}/requirements?include_withdrawn=${includeWithdrawn}&offset=${offset}&limit=30`);
  const artifacts = useQuery({
    queryKey: [base, "requirement-source-artifacts"],
    queryFn: () => api.get<Schema<"Artifact">[]>(`${base}/artifacts?include_history=true`),
    enabled: !!source,
  });
  const closeCreate = useCallback(() => setCreating(false), []);
  const closeDetail = useCallback(() => setSelected(null), []);
  const closeSource = useCallback(() => setSource(null), []);
  return <section className="submission-requirements" aria-labelledby="submission-requirements-heading">
    <div className="section-heading">
      <div><h2 id="submission-requirements-heading"><ClipboardCheck size={21} /> Submission requirements</h2><p className="muted">Record what the tender asks for, approve the requirement and review the documents that address it.</p></div>
      <button className="button" onClick={() => setCreating(true)}><Plus size={17} /> Propose a requirement</button>
    </div>
    <p className="requirements-intro">Source references support each requirement. A saved document alone does not establish compliance; the engineer records satisfaction or an exception after review.</p>
    <label className="requirements-checkbox"><input type="checkbox" checked={includeWithdrawn} onChange={event => { setIncludeWithdrawn(event.target.checked); setOffset(0); }} />Show withdrawn requirements</label>
    <ErrorNotice error={requirements.error} />
    {requirements.isPending ? <Loading>Loading submission requirements…</Loading> : null}
    {requirements.data?.length === 0 ? <p className="requirements-empty">No requirements are recorded in this view. Propose one from the tender instructions or ask the Tender Manager to identify them.</p> : null}
    <div className="requirements-list">{requirements.data?.map(requirement => <article key={requirement.id} className="requirement-card">
      <div className="requirement-card-heading"><button className="requirement-title" onClick={() => setSelected(requirement.id)}>{requirement.title}</button><span className={`requirement-state ${requirement.review_is_current && requirement.review_status === "satisfied" ? "requirement-satisfied" : ""}`}>{reviewLabel(requirement)}</span></div>
      <p>{requirement.detail}</p>
      <div className="requirement-meta"><span>{deliverables[requirement.deliverable_kind]}</span><span>{requirement.linked_outputs.length} linked document(s)</span>{requirement.due_date ? <span className={requirement.overdue ? "requirement-overdue" : ""}>Due {requirement.due_date}{requirement.overdue ? " · date has passed" : ""}</span> : null}</div>
      <Citations ids={requirement.source_ids} tenderId={tenderId} onOpen={setSource} />
      <button className="button" onClick={() => setSelected(requirement.id)}>Review requirement</button>
    </article>)}</div>
    <div className="inline-actions"><button className="button" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - 30))}>Previous requirements</button><button className="button" disabled={(requirements.data?.length ?? 0) < 30} onClick={() => setOffset(offset + 30)}>More requirements</button></div>
    {creating ? <RequirementCreate tenderId={tenderId} onClose={closeCreate} onCreated={identifier => { setCreating(false); setSelected(identifier); setOffset(0); }} /> : null}
    {selected ? <RequirementDetail tenderId={tenderId} requirementId={selected} onClose={closeDetail} onSource={setSource} /> : null}
    {source ? <SourceDrawer tenderId={tenderId} selection={source} artifacts={artifacts.data ?? []} onClose={closeSource} /> : null}
  </section>;
}

function RequirementCreate({ tenderId, onClose, onCreated }: { tenderId: string; onClose: () => void; onCreated: (id: string) => void }) {
  const api = useApi(), refresh = useRefresh();
  const [title, setTitle] = useState(""), [detail, setDetail] = useState(""), [kind, setKind] = useState<Deliverable>("technical_docx"), [due, setDue] = useState("");
  const [sources, setSources] = useState<string[]>([]), [busy, setBusy] = useState(false), [error, setError] = useState<unknown>(null);
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy || !title.trim() || !detail.trim() || !sources.length) return;
    setBusy(true); setError(null);
    try {
      const created = await api.post<Requirement>(`${tenderPath(tenderId)}/requirements`, { title, detail, deliverable_kind: kind, due_date: due || null, source_ids: sources } satisfies Schema<"RequirementProposal">);
      void refresh(); onCreated(created.id);
    } catch (caught) { setError(caught); } finally { setBusy(false); }
  }
  return <Modal title="Propose a submission requirement" onClose={onClose}>
    <form className="requirement-form" onSubmit={event => void submit(event)}><p className="muted">Use the supplied tender instructions. This proposal needs a separate engineer approval.</p><ErrorNotice error={error} />
      <fieldset disabled={busy}>
        <label>Requirement title<input required maxLength={300} value={title} onChange={event => setTitle(event.target.value)} /></label>
        <label>What the tender requires<textarea required maxLength={6000} value={detail} onChange={event => setDetail(event.target.value)} /></label>
        <label>Required document type<select aria-label="Required document type" value={kind} onChange={event => setKind(event.target.value as Deliverable)}>{Object.entries(deliverables).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
        <label>Stated due date (optional)<input type="date" value={due} onChange={event => setDue(event.target.value)} /></label>
        <p className="muted">Enter a date only when the supporting instructions state it. A past date is shown for review.</p>
        <EvidencePicker tenderId={tenderId} selected={sources} onChange={setSources} />
        <div className="inline-actions"><button type="submit" className="button primary" disabled={!title.trim() || !detail.trim() || sources.length === 0 || sources.length > 50}>{busy ? "Saving proposal…" : "Save requirement proposal"}</button><button type="button" className="button" onClick={onClose}>Cancel</button></div>
      </fieldset>
    </form>
  </Modal>;
}

function RequirementDetail({ tenderId, requirementId, onClose, onSource }: { tenderId: string; requirementId: string; onClose: () => void; onSource: (source: SourceSelection) => void }) {
  const requirement = useResource<Requirement>(`${tenderPath(tenderId)}/requirements/${encodeURIComponent(requirementId)}`);
  const current = requirement.data;
  return <Modal title={current?.title ?? "Submission requirement"} onClose={onClose}>
    <div className="requirement-detail"><ErrorNotice error={requirement.error} />{requirement.isPending ? <Loading>Opening requirement…</Loading> : null}
      {current ? <>
        <span className="requirement-state">{reviewLabel(current)}</span><p>{current.detail}</p>
        <dl className="requirement-facts"><dt>Required document</dt><dd>{deliverables[current.deliverable_kind]}</dd><dt>Origin</dt><dd>{current.origin === "manager" ? "Tender Manager proposal" : "Engineer proposal"}</dd><dt>Due date</dt><dd>{current.due_date ?? "No date recorded"}</dd><dt>Approval state</dt><dd>{current.status}</dd></dl>
        {current.warnings.map(warning => <p className="requirement-warning" key={warning}>{warning}</p>)}
        {current.recheck_reasons.map(reason => <p className="requirement-warning" key={reason}>{sourceReasons[reason] ?? reason}</p>)}
        <Citations ids={current.source_ids} tenderId={tenderId} onOpen={onSource} />
        <details><summary>Recorded source versions</summary><div className="requirement-source-list">{current.sources.map(source => <div key={source.source_id}><strong>{source.artifact_name} · {source.locator}</strong><p>{source.relative_path} · version {source.version}</p><p>{source.is_current ? "Current preserved source" : "Source needs recheck"}</p><small>SHA-256 {source.content_hash}</small></div>)}</div></details>
        {current.reviewed_at ? <div className="requirement-review-note"><h4>Recorded completion review</h4><p>{current.review_rationale}</p><small>{current.reviewed_at}{current.review_is_current ? " · current" : " · needs updating"}</small></div> : null}
        <RequirementLinks key={`${current.id}:${current.audit.length}`} tenderId={tenderId} requirement={current} />
        {current.status !== "withdrawn" ? <RequirementDecisionForm key={`${current.id}:${current.audit.length}`} tenderId={tenderId} requirement={current} /> : <p className="muted">This requirement was withdrawn. Its source and review history remain available.</p>}
        <details><summary>Engineer decision history ({current.audit.length})</summary><ol className="requirement-audit">{current.audit.map(event => <li key={event.id}><strong>{event.action === "link_output" ? "Document linked" : event.action === "unlink_output" ? "Document link removed" : decisionLabels[event.action]}</strong><p>{event.rationale}</p><small>{event.created_at}</small></li>)}</ol></details>
      </> : null}
    </div>
  </Modal>;
}

function RequirementDecisionForm({ tenderId, requirement }: { tenderId: string; requirement: Requirement }) {
  const api = useApi(), refresh = useRefresh();
  const allowed: Decision[] = !requirement.is_current ? ["withdraw"] : requirement.status === "proposed" ? ["approve", "withdraw"] : ["satisfied", "exception", "reopen", "withdraw"];
  const [decision, setDecision] = useState<Decision>(allowed[0]), [rationale, setRationale] = useState(""), [consent, setConsent] = useState(false), [busy, setBusy] = useState(false), [error, setError] = useState<unknown>(null);
  const chosen = allowed.includes(decision) ? decision : allowed[0];
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (busy || !consent || !rationale.trim()) return;
    setBusy(true); setError(null);
    try {
      await api.post<Requirement>(`${tenderPath(tenderId)}/requirements/${requirement.id}/decision`, { decision: chosen, engineer_confirmed: true, rationale } satisfies Schema<"RequirementDecision">);
      setConsent(false); setRationale(""); void refresh();
    } catch (caught) { setError(caught); } finally { setBusy(false); }
  }
  const satisfiedBlocked = chosen === "satisfied" && (!requirement.linked_outputs.length || requirement.linked_outputs.some(output => !output.is_current));
  return <form className="requirement-form requirement-decision" onSubmit={event => void submit(event)}><h4>Engineer decision</h4><ErrorNotice error={error} /><fieldset disabled={busy}>
    <label>Decision<select aria-label="Requirement decision" value={chosen} onChange={event => { setDecision(event.target.value as Decision); setConsent(false); }}>{allowed.map(action => <option key={action} value={action}>{decisionLabels[action]}</option>)}</select></label>
    {satisfiedBlocked ? <p className="requirement-warning">Link the current document that addresses the requirement before marking it satisfied.</p> : null}
    {chosen === "exception" ? <p className="muted">Explain the outstanding requirement and why this exception is being recorded. A selected exception will remain visible in the final export review.</p> : null}
    <label>Decision rationale<textarea required value={rationale} maxLength={4000} onChange={event => setRationale(event.target.value)} /></label>
    <label className="requirements-checkbox"><input type="checkbox" checked={consent} onChange={event => setConsent(event.target.checked)} />I reviewed the requirement, its sources and any linked documents, and confirm this decision.</label>
    <button type="submit" className="button primary" disabled={!consent || !rationale.trim() || satisfiedBlocked}>{busy ? "Recording decision…" : "Record engineer decision"}</button>
  </fieldset></form>;
}

function RequirementLinks({ tenderId, requirement }: { tenderId: string; requirement: Requirement }) {
  const api = useApi(), refresh = useRefresh();
  const outputs = useResource<Schema<"OutputRecord">[]>(`${tenderPath(tenderId)}/outputs`);
  const [selected, setSelected] = useState(""), [rationale, setRationale] = useState(""), [consent, setConsent] = useState(false), [busy, setBusy] = useState(false), [error, setError] = useState<unknown>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const linkable = outputs.data?.filter(output => output.kind === requirement.deliverable_kind && !requirement.linked_outputs.some(link => link.output_id === output.id)) ?? [];
  async function change(event: FormEvent) {
    event.preventDefault();
    if (busy || !consent || !rationale.trim() || (!selected && !removing)) return;
    setBusy(true); setError(null);
    const base = `${tenderPath(tenderId)}/requirements/${requirement.id}/outputs`;
    try {
      if (removing) await api.post<Requirement>(`${base}/${removing}/unlink`, { engineer_confirmed: true, rationale } satisfies Schema<"EngineerDecision">);
      else await api.post<Requirement>(base, { output_id: selected, engineer_confirmed: true, rationale } satisfies Schema<"RequirementOutputLink">);
      setSelected(""); setRemoving(null); setConsent(false); setRationale(""); void refresh();
    } catch (caught) { setError(caught); } finally { setBusy(false); }
  }
  return <section className="requirement-links"><h4>Linked generated documents</h4>
    {requirement.linked_outputs.length === 0 ? <p className="muted">No generated documents are linked.</p> : <ul>{requirement.linked_outputs.map(output => <li key={output.output_id}><strong>{output.filename}</strong><span>{output.is_current ? "Current document and working basis" : "Document needs attention"}</span>{output.recheck_reasons.map(reason => <p className="requirement-warning" key={reason}>{reason}</p>)}<small>{output.link_rationale}</small>{requirement.status === "approved" ? <button type="button" className="button" disabled={busy} onClick={() => { setRemoving(output.output_id); setSelected(""); setConsent(false); }}>Remove this link</button> : null}</li>)}</ul>}
    {requirement.status === "approved" && (requirement.is_current || removing) ? <form className="requirement-form" onSubmit={event => void change(event)}><ErrorNotice error={error || outputs.error} /><fieldset disabled={busy}>
      {removing ? <p>Remove the link to <strong>{requirement.linked_outputs.find(output => output.output_id === removing)?.filename}</strong>. The document and earlier decision history remain saved.</p> : <><label>Generated document<select aria-label="Generated document for requirement" value={selected} onChange={event => { setSelected(event.target.value); setConsent(false); }}><option value="">Choose a document</option>{linkable.map(output => <option key={output.id} value={output.id}>{output.filename}</option>)}</select></label>{outputs.isPending ? <Loading>Loading generated documents…</Loading> : linkable.length === 0 ? <p className="muted">Create a {deliverables[requirement.deliverable_kind].toLowerCase()} draft to link it here.</p> : null}</>}
      <label>Document link rationale<textarea required maxLength={4000} value={rationale} onChange={event => setRationale(event.target.value)} /></label><label className="requirements-checkbox"><input type="checkbox" checked={consent} onChange={event => setConsent(event.target.checked)} />I reviewed this document link. Changing links requires a new completion review.</label>
      <div className="inline-actions"><button className="button" type="submit" disabled={!consent || !rationale.trim() || (!selected && !removing)}>{busy ? "Saving document link…" : removing ? "Remove reviewed link" : "Link reviewed document"}</button>{removing ? <button className="button" type="button" onClick={() => { setRemoving(null); setConsent(false); }}>Cancel removal</button> : null}</div>
    </fieldset></form> : null}
  </section>;
}
