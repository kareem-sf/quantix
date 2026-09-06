import { useCallback, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CornerDownRight, Plus } from "lucide-react";
import { tenderPath, useApi, useRefresh, useResource, type Schema } from "../api";
import { ErrorNotice, Loading, Modal, Status } from "../components/ui";
import { EvidencePicker } from "./EvidencePicker";
import { Citations, type SourceSelection } from "./Sources";
import "../styles/project-map.css";

type Node = Schema<"NodeRecord">;
type NodeKind = Schema<"NodeInput">["kind"];
type RelatedSelection = { kind: "finding" | "boq"; id: string };
const kinds: Record<NodeKind, string> = {
  building: "Building", area: "Area", discipline: "Discipline",
  work_item: "Work item", requirement: "Requirement",
};

export function ProjectMap({ tenderId, artifacts, onSource }: {
  tenderId: string; artifacts: Schema<"Artifact">[];
  onSource: (source: SourceSelection) => void;
}) {
  const map = useResource<Schema<"ProjectMapView">>(`${tenderPath(tenderId)}/project-map`);
  const [query, setQuery] = useState(""), [kind, setKind] = useState<NodeKind | "all">("all");
  const [state, setState] = useState("active"), [currentness, setCurrentness] = useState("all");
  const [creating, setCreating] = useState(false), [reviewing, setReviewing] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null), [related, setRelated] = useState<RelatedSelection | null>(null);
  const closeCreate = useCallback(() => setCreating(false), []), closeReview = useCallback(() => setReviewing(false), []);
  const closeNode = useCallback(() => setSelectedId(null), []), closeRelated = useCallback(() => setRelated(null), []);
  const nodes = map.data?.nodes ?? [], selected = nodes.find(node => node.id === selectedId);
  const ordered = hierarchy(nodes);
  const visible = ordered.filter(({ node, parents }) =>
    (kind === "all" || node.kind === kind) &&
    (state === "all" || (state === "active" ? node.state !== "withdrawn" : node.state === state)) &&
    (currentness === "all" || node.is_current === (currentness === "current")) &&
    [node.title, node.detail, ...parents].join(" ").toLowerCase().includes(query.trim().toLowerCase()),
  );
  return <div className="feature-page project-map-page">
    <div className="section-heading">
      <div><h2>Project map</h2><p className="muted">Connect the buildings, areas and work scope to their sources. Record what you have reviewed.</p></div>
      <div className="project-map-actions">
        <button type="button" className="button" onClick={() => setReviewing(true)} disabled={!artifacts.some(artifact => artifact.is_current)} >Record source review</button>
        <button type="button" className="button primary" onClick={() => setCreating(true)}><Plus size={18} />Propose map item</button>
      </div>
    </div>
    <ErrorNotice error={map.error} />
    {map.isPending ? <Loading>Loading project structure and review coverage…</Loading> : null}
    {map.isError ? <button type="button" className="text-button" onClick={() => void map.refetch()}>Reload project map</button> : null}
    {map.data ? <>
      <Coverage coverage={map.data.coverage} />
      <div className="map-filters">
        <label>Find a map item<input value={query} onChange={event => setQuery(event.target.value)} placeholder="Search building, area or scope…" /></label>
        <label>Item type<select value={kind} onChange={event => setKind(event.target.value as NodeKind | "all")}><option value="all">All types</option>{Object.entries(kinds).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
        <label>Decision<select value={state} onChange={event => setState(event.target.value)}><option value="active">Proposed and approved</option><option value="proposed">Proposed</option><option value="approved">Approved</option><option value="withdrawn">Withdrawn</option><option value="all">All decisions</option></select></label>
        <label>Source version<select value={currentness} onChange={event => setCurrentness(event.target.value)}><option value="all">All versions</option><option value="current">Current sources</option><option value="stale">Needs source review</option></select></label>
      </div>
      <div className="map-layout">
        <section aria-label="Project structure">
          <ul className="map-entities">{visible.map(({ node, parents }) => <li className="map-entity" key={node.id}>
            <div className="map-entity-heading"><button type="button" className="map-entity-title" onClick={() => setSelectedId(node.id)}>{parents.length ? <CornerDownRight size={17} /> : null}{node.title}</button><Status value={node.state} /></div>
            <div className="map-entity-meta"><span>{kinds[node.kind]}</span>{parents.length ? <span>Within {parents.join(" / ")}</span> : null}<span>{node.origin === "agent" ? "Proposed by the office" : "Recorded by the engineer"}</span>{!node.is_current ? <span className="map-warning">Source review needed</span> : node.state === "approved" ? <span>{node.approval_valid ? "Approval current" : "Approval needs review"}</span> : null}</div>
            <p className="map-entity-detail">{node.detail}</p>
          </li>)}</ul>
          {!visible.length ? <div className="map-empty"><h3>{nodes.length ? "No map items match these filters" : "Define the project from its sources"}</h3><p className="muted">{nodes.length ? "Change the filters to inspect the saved structure." : "Ask the Tender Manager to propose the project structure, or add a source-backed item for review."}</p></div> : null}
          {map.data.directory_areas.length ? <details className="map-directory-groups"><summary>Imported folder groups</summary><p className="field-help">These names come from the package folders. They have not been approved as project areas.</p><ul>{map.data.directory_areas.map(area => <li key={area}>{area}</li>)}</ul></details> : null}
        </section>
        <section className="map-review-rail" aria-label="Recorded source reviews"><h3>Recorded source reviews</h3><p className="field-help">A page or passage review covers only the stated scope. Earlier versions remain visible.</p>
          {map.data.review_scopes.length ? map.data.review_scopes.map(review => <ReviewRecord key={review.id} record={review} onSource={onSource} />) : <p className="muted map-empty">No engineer source reviews have been recorded.</p>}
        </section>
      </div>
    </> : null}
    {creating ? <NodeForm tenderId={tenderId} nodes={nodes} onClose={closeCreate} onSource={onSource} onSaved={id => { setCreating(false); setSelectedId(id); }} /> : null}
    {reviewing ? <ReviewForm tenderId={tenderId} artifacts={artifacts} onSource={onSource} onClose={closeReview} /> : null}
    {selected ? <NodeDetail tenderId={tenderId} node={selected} nodes={nodes} artifacts={artifacts} onClose={closeNode} onSource={onSource} onNode={setSelectedId} onRelated={setRelated} /> : null}
    {related ? <RelatedRecord tenderId={tenderId} selection={related} onClose={closeRelated} onSource={onSource} /> : null}
  </div>;
}

function hierarchy(nodes: Node[]) {
  const byId = new Map(nodes.map(node => [node.id, node]));
  const result = nodes.map(node => {
    const parents: string[] = [], seen = new Set([node.id]);
    let parent = node.parent_id ? byId.get(node.parent_id) : undefined;
    while (parent && !seen.has(parent.id)) { seen.add(parent.id); parents.unshift(parent.title); parent = parent.parent_id ? byId.get(parent.parent_id) : undefined; }
    return { node, parents };
  });
  return result.sort((first, second) => [...first.parents, first.node.title].join(" / ").localeCompare([...second.parents, second.node.title].join(" / ")));
}

function Coverage({ coverage }: { coverage: Schema<"MapCoverage"> }) {
  const entries: [string, number][] = [
    ["Registered files", coverage.registered_files], ["Files with readable content", coverage.extracted_files],
    ["Extracted source passages", coverage.extracted_evidence], ["Passages cited in findings", coverage.evidence_cited_in_findings],
    ["Current recorded review scopes", coverage.current_review_scopes], ["Documents reviewed in full", coverage.reviewed_artifacts_in_full],
  ];
  return <section className="map-coverage"><h3>Source coverage</h3><dl>{entries.map(([label, count]) => <div key={label}><dt>{label}</dt><dd>{count.toLocaleString()}</dd></div>)}</dl><p className="field-help">{coverage.note}</p></section>;
}

function ReviewRecord({ record, onSource }: { record: Schema<"ReviewRecord">; onSource: (source: SourceSelection) => void }) {
  return <article className="map-review-record"><strong>{record.scope_label}</strong><p>{record.scope_type === "artifact" ? "Whole document reviewed" : record.scope_type === "page" ? `Reviewed page ${record.page}` : `Reviewed passage · ${record.locator}`}</p><p className={record.is_current ? "field-help" : "map-warning"}>{record.is_current ? "Current source version" : "Earlier source version · Review again"}</p>
    <button type="button" className="source-chip" onClick={() => onSource(record.source_ids.length && record.scope_type === "locator" ? { sourceId: record.source_ids[0] } : { artifactId: record.artifact_id, ...(record.scope_type === "page" && record.page ? { page: record.page } : {}) })}>{record.relative_path} · Version {record.version}</button>
    <details><summary>Review record</summary><p>{record.rationale}</p>{record.stale_reasons.map(reason => <p className="map-warning" key={reason}>{reason}</p>)}<p className="map-source-identity">Source hash: <code>{record.content_hash}</code></p><time dateTime={record.created_at}>{new Date(record.created_at).toLocaleString()}</time></details>
  </article>;
}

function NodeForm({ tenderId, nodes, onClose, onSource, onSaved }: { tenderId: string; nodes: Node[]; onClose: () => void; onSource: (source: SourceSelection) => void; onSaved: (id: string) => void }) {
  const api = useApi(), refresh = useRefresh();
  const [kind, setKind] = useState<NodeKind>("building"), [title, setTitle] = useState(""), [detail, setDetail] = useState(""), [parent, setParent] = useState("");
  const [sources, setSources] = useState<string[]>([]), [pending, setPending] = useState(false), [error, setError] = useState<unknown>(null);
  return <Modal title="Propose a project map item" onClose={onClose}><form className="map-form" onSubmit={async event => {
    event.preventDefault(); if (pending || !sources.length) return; setPending(true); setError(null);
    try { const saved = await api.post<Node>(`${tenderPath(tenderId)}/project-map/nodes`, { kind, title: title.trim(), detail: detail.trim(), source_ids: sources, parent_id: parent || null } satisfies Schema<"NodeInput">); await refresh(); onSaved(saved.id); }
    catch (failure) { setError(failure); } finally { setPending(false); }
  }}><p className="muted">Describe one building, area or part of the work and select its supporting sources. The item remains proposed until an engineer approves it.</p><fieldset disabled={pending}>
    <label>Item type<select value={kind} onChange={event => setKind(event.target.value as NodeKind)}>{Object.entries(kinds).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
    <label>Title<input required maxLength={300} value={title} onChange={event => setTitle(event.target.value)} /></label>
    <label>Within<select value={parent} onChange={event => setParent(event.target.value)}><option value="">Project level</option>{hierarchy(nodes).filter(({ node }) => node.is_current && node.state !== "withdrawn").map(({ node, parents }) => <option key={node.id} value={node.id}>{[...parents, node.title].join(" / ")}</option>)}</select></label>
    <label>Scope and description<textarea required maxLength={6000} rows={4} value={detail} onChange={event => setDetail(event.target.value)} /></label>
    <EvidencePicker tenderId={tenderId} selected={sources} onChange={ids => { if (ids.length <= 50) setSources(ids); else setError(new Error("Select up to 50 supporting sources.")); }} />
    <Citations ids={sources} tenderId={tenderId} onOpen={onSource} />
    </fieldset><ErrorNotice error={error} /><div className="form-actions"><button type="button" className="button" onClick={onClose} disabled={pending}>Cancel</button><button className="button primary" disabled={pending || !title.trim() || !detail.trim() || !sources.length}>{pending ? "Saving…" : "Save proposed item"}</button></div></form></Modal>;
}

function NodeDetail({ tenderId, node, nodes, artifacts, onClose, onSource, onNode, onRelated }: {
  tenderId: string; node: Node; nodes: Node[]; artifacts: Schema<"Artifact">[];
  onClose: () => void; onSource: (source: SourceSelection) => void; onNode: (id: string) => void; onRelated: (selection: RelatedSelection) => void;
}) {
  const [decision, setDecision] = useState<Schema<"NodeDecision">["decision"] | null>(null);
  const closeDecision = useCallback(() => setDecision(null), []);
  const parent = nodes.find(item => item.id === node.parent_id);
  return <Modal drawer title={node.title} onClose={onClose}><div className="map-detail">
    <div className="map-entity-meta"><span>{kinds[node.kind]}</span><Status value={node.state} /><span>{node.origin === "agent" ? "Office proposal" : "Engineer proposal"}</span></div>
    {parent ? <p className="field-help">Within <button type="button" className="text-button" onClick={() => onNode(parent.id)}>{parent.title}</button></p> : null}
    <p className="map-detail-text">{node.detail}</p>
    {node.is_current ? <p className="field-help">{node.approval_valid ? "Engineer approval applies to the current source basis." : "Sources are current. Engineer approval is recorded separately."}</p> : <div className="map-warning"><p>Sources or related project structure have changed. Review the current documents and propose a new item.</p>{node.stale_reasons.map(reason => <p key={reason}>{reason}</p>)}</div>}
    <section><h3>Supporting sources</h3><Citations tenderId={tenderId} ids={node.source_ids} onOpen={onSource} /><details className="map-source-identity"><summary>Saved source versions</summary>{node.source_manifest.map(source => <p key={source.source_id}>{source.relative_path} · Version {source.version} · {source.locator}<br />File hash: <code>{source.content_hash}</code><br />Source text hash: <code>{source.evidence_hash}</code></p>)}</details></section>
    <RelatedLinks tenderId={tenderId} node={node} artifacts={artifacts} onSource={onSource} onRelated={onRelated} />
    <section><h3>Engineer decision</h3><p className="field-help">Approval records the project structure and stated scope. Prices, quantities and submission decisions remain separate.</p><div className="project-map-actions">
      {node.state !== "withdrawn" && !node.approval_valid ? <button type="button" className="button primary" disabled={!node.is_current} onClick={() => setDecision("approve")}>Approve map item</button> : null}
      {node.state !== "withdrawn" ? <button type="button" className="button" onClick={() => setDecision("withdraw")}>Withdraw map item</button> : null}
    </div></section>
    {node.decisions.length ? <section><h3>Decision history</h3>{node.decisions.map(entry => <article className="map-review-record" key={entry.id}><strong>{entry.decision === "approve" ? "Approved" : "Withdrawn"}</strong><p>{entry.rationale}</p><time dateTime={entry.created_at}>{new Date(entry.created_at).toLocaleString()}</time></article>)}</section> : null}
    {decision ? <NodeDecisionForm tenderId={tenderId} node={node} decision={decision} onClose={closeDecision} /> : null}
  </div></Modal>;
}

function NodeDecisionForm({ tenderId, node, decision, onClose }: { tenderId: string; node: Node; decision: Schema<"NodeDecision">["decision"]; onClose: () => void }) {
  const api = useApi(), refresh = useRefresh();
  const [rationale, setRationale] = useState(""), [confirmed, setConfirmed] = useState(false), [pending, setPending] = useState(false), [error, setError] = useState<unknown>(null);
  return <Modal title={decision === "approve" ? "Approve project map item" : "Withdraw project map item"} onClose={onClose}><form className="map-form" onSubmit={async event => {
    event.preventDefault(); if (!confirmed || pending) return; setPending(true); setError(null);
    try { await api.post<Node>(`${tenderPath(tenderId)}/project-map/nodes/${node.id}/decision`, { decision, engineer_confirmed: true, rationale: rationale.trim() } satisfies Schema<"NodeDecision">); await refresh(); onClose(); }
    catch (failure) { setError(failure); setConfirmed(false); } finally { setPending(false); }
  }}><p className="muted">{decision === "approve" ? `Approve the stated scope and supporting sources for “${node.title}”.` : `Withdraw “${node.title}” from the active project structure. Its sources and decision history remain available.`}</p><fieldset disabled={pending}><label>Decision note<textarea required maxLength={4000} rows={3} value={rationale} onChange={event => setRationale(event.target.value)} /></label><label className="checkbox-label"><input type="checkbox" required checked={confirmed} onChange={event => setConfirmed(event.target.checked)} />{decision === "approve" ? "I reviewed the item and its sources and approve this project scope." : "I confirm this item should be withdrawn from the active structure."}</label></fieldset><ErrorNotice error={error} /><div className="form-actions"><button type="button" className="button" disabled={pending} onClick={onClose}>Cancel</button><button className="button primary" disabled={pending || !confirmed || !rationale.trim()}>{pending ? "Recording…" : decision === "approve" ? "Record approval" : "Record withdrawal"}</button></div></form></Modal>;
}

function RelatedLinks({ tenderId, node, artifacts, onSource, onRelated }: { tenderId: string; node: Node; artifacts: Schema<"Artifact">[]; onSource: (source: SourceSelection) => void; onRelated: (selection: RelatedSelection) => void }) {
  const api = useApi(), base = tenderPath(tenderId);
  const findings = useQuery({ queryKey: [`${base}/findings`], queryFn: () => api.get<Schema<"Finding">[]>(`${base}/findings`), enabled: !!node.related_finding_ids.length });
  const estimate = useQuery({ queryKey: [`${base}/estimate`], queryFn: () => api.get<Schema<"EstimateView">>(`${base}/estimate`), enabled: !!node.related_boq_item_ids.length });
  return <section><h3>Related tender records</h3><ErrorNotice error={findings.error || estimate.error} /><ul className="map-detail-list">
    {node.related_artifact_ids.map(id => <li key={id}><button type="button" className="text-button" onClick={() => onSource({ artifactId: id })}>{artifacts.find(artifact => artifact.id === id)?.name ?? node.source_manifest.find(source => source.artifact_id === id)?.relative_path ?? "Open related file"}</button></li>)}
    {node.related_finding_ids.map((id, index) => <li key={id}><button type="button" className="text-button" onClick={() => onRelated({ kind: "finding", id })}>{findings.data?.find(finding => finding.id === id)?.title ?? `Related finding ${index + 1}`}</button></li>)}
    {node.related_boq_item_ids.map((id, index) => <li key={id}><button type="button" className="text-button" onClick={() => onRelated({ kind: "boq", id })}>{estimate.data?.items.find(item => item.id === id)?.description ?? `Related BOQ row ${index + 1}`}</button></li>)}
  </ul>{!node.related_artifact_ids.length && !node.related_finding_ids.length && !node.related_boq_item_ids.length ? <p className="field-help">No other tender records are linked to this item.</p> : null}</section>;
}

function RelatedRecord({ tenderId, selection, onClose, onSource }: { tenderId: string; selection: RelatedSelection; onClose: () => void; onSource: (source: SourceSelection) => void }) {
  const api = useApi(), base = tenderPath(tenderId);
  const findings = useQuery({ queryKey: [`${base}/findings`], queryFn: () => api.get<Schema<"Finding">[]>(`${base}/findings`), enabled: selection.kind === "finding" });
  const estimate = useQuery({ queryKey: [`${base}/estimate`], queryFn: () => api.get<Schema<"EstimateView">>(`${base}/estimate`), enabled: selection.kind === "boq" });
  const finding = selection.kind === "finding" ? findings.data?.find(item => item.id === selection.id) : undefined;
  const row = selection.kind === "boq" ? estimate.data?.items.find(item => item.id === selection.id) : undefined;
  const pending = selection.kind === "finding" ? findings.isPending : estimate.isPending;
  return <Modal title={selection.kind === "finding" ? "Related finding" : "Related BOQ row"} onClose={onClose}>
    {pending ? <Loading>Loading the saved record…</Loading> : null}<ErrorNotice error={findings.error || estimate.error} />
    {finding ? <><h3>{finding.title}</h3><p className="map-detail-text">{finding.detail}</p><Status value={finding.state} /><Citations tenderId={tenderId} ids={finding.source_ids} onOpen={onSource} /></> : row ? <><h3>{row.description}</h3><dl className="estimate-row-facts"><dt>Supplied quantity</dt><dd>{row.supplied_quantity ?? "Unresolved"} {row.unit}</dd><dt>Quantity used</dt><dd>{row.effective_quantity ?? "Unresolved"} {row.unit} · {row.quantity_basis === "approved_measurement" ? "Approved measurement" : "Supplied BOQ"}</dd><dt>Unit rate</dt><dd>{row.unit_rate ?? "Not priced"} {row.currency}</dd><dt>Source review</dt><dd>{row.confirmed ? "Confirmed" : "Needs review"}</dd></dl><Citations tenderId={tenderId} ids={[row.source_id]} onOpen={onSource} /><p className="field-help">Open Estimate to make a price, source or quantity decision.</p></> : !pending ? <p className="muted">This record is no longer available in the current tender view.</p> : null}
  </Modal>;
}

function ReviewForm({ tenderId, artifacts, onSource, onClose }: { tenderId: string; artifacts: Schema<"Artifact">[]; onSource: (source: SourceSelection) => void; onClose: () => void }) {
  const api = useApi(), refresh = useRefresh();
  const [artifactId, setArtifactId] = useState(""), [scope, setScope] = useState<Schema<"ReviewInput">["scope_type"]>("locator");
  const [label, setLabel] = useState(""), [page, setPage] = useState(""), [passage, setPassage] = useState<Schema<"Evidence"> | null>(null), [rationale, setRationale] = useState("");
  const [whole, setWhole] = useState(false), [confirmed, setConfirmed] = useState(false), [pending, setPending] = useState(false), [error, setError] = useState<unknown>(null);
  const artifact = artifacts.find(item => item.id === artifactId), isPdf = artifact?.kind === "pdf" || artifact?.name.toLowerCase().endsWith(".pdf");
  const valid = !!artifact && !!label.trim() && !!rationale.trim() && confirmed && (scope === "artifact" ? whole : scope === "page" ? !!isPdf && /^\d+$/.test(page) && +page > 0 : !!passage);
  function resetScope() { setConfirmed(false); setWhole(false); setPassage(null); setPage(""); }
  return <Modal title="Record an engineer source review" onClose={onClose}><form className="map-form" onSubmit={async event => {
    event.preventDefault(); if (!valid || pending) return; setPending(true); setError(null);
    try { await api.post<Schema<"ReviewRecord">>(`${tenderPath(tenderId)}/project-map/reviews`, { artifact_id: artifactId, scope_type: scope, scope_label: label.trim(), engineer_confirmed: true, rationale: rationale.trim(), whole_document_reviewed: scope === "artifact" && whole, ...(scope === "page" ? { page: Number(page) } : {}), ...(scope === "locator" && passage ? { locator: passage.locator } : {}) } satisfies Schema<"ReviewInput">); await refresh(); onClose(); }
    catch (failure) { setError(failure); setConfirmed(false); } finally { setPending(false); }
  }}><p className="muted">Record only the document, page or passage you have checked. Importing a file and extracting its text do not record an engineer review.</p><fieldset disabled={pending}>
    <label>Reviewed document<select required value={artifactId} onChange={event => { setArtifactId(event.target.value); resetScope(); if (scope === "page") setScope("locator"); }}><option value="">Choose a current source file</option>{artifacts.filter(item => item.is_current).map(item => <option key={item.id} value={item.id}>{item.relative_path}</option>)}</select></label>
    <label>Review covers<select value={scope} onChange={event => { setScope(event.target.value as Schema<"ReviewInput">["scope_type"]); resetScope(); }}><option value="locator">One source passage or worksheet range</option><option value="page" disabled={!isPdf}>One PDF page</option><option value="artifact">The whole document</option></select></label>
    {scope === "page" ? <label>Reviewed page<input required type="number" min={1} step={1} value={page} onChange={event => { setPage(event.target.value); setConfirmed(false); }} /></label> : null}
    {scope === "locator" && artifactId ? <ReviewPassage key={artifactId} tenderId={tenderId} artifactId={artifactId} selected={passage} onChange={value => { setPassage(value); setConfirmed(false); }} /> : null}
    {artifact && (scope !== "page" || (/^\d+$/.test(page) && +page > 0)) ? <button type="button" className="text-button" onClick={() => onSource(scope === "locator" && passage ? { sourceId: passage.id } : { artifactId: artifact.id, artifact, ...(scope === "page" ? { page: Number(page) } : {}) })}>Open selected source</button> : null}
    <label>Reviewed scope<input required maxLength={500} value={label} onChange={event => setLabel(event.target.value)} placeholder="State exactly what you checked" /></label>
    <label>Review note<textarea required rows={3} maxLength={6000} value={rationale} onChange={event => setRationale(event.target.value)} /></label>
    {scope === "artifact" ? <label className="checkbox-label"><input type="checkbox" required checked={whole} onChange={event => setWhole(event.target.checked)} />I reviewed this whole document, including its pages, sheets and applicable attachments.</label> : <p className="field-help">This records a partial review. The remaining document will stay outside the reviewed scope.</p>}
    <label className="checkbox-label"><input type="checkbox" required checked={confirmed} onChange={event => setConfirmed(event.target.checked)} />I confirm that I carried out the source review described here.</label>
  </fieldset><ErrorNotice error={error} /><div className="form-actions"><button type="button" className="button" disabled={pending} onClick={onClose}>Cancel</button><button className="button primary" disabled={pending || !valid}>{pending ? "Recording…" : "Record source review"}</button></div></form></Modal>;
}

function ReviewPassage({ tenderId, artifactId, selected, onChange }: { tenderId: string; artifactId: string; selected: Schema<"Evidence"> | null; onChange: (source: Schema<"Evidence"> | null) => void }) {
  const [offset, setOffset] = useState(0);
  const passages = useResource<Schema<"Evidence">[]>(`${tenderPath(tenderId)}/artifacts/${artifactId}/evidence?offset=${offset}&limit=50`);
  const available = passages.data ?? [], choices = selected && !available.some(source => source.id === selected.id) ? [selected, ...available] : available;
  return <div className="map-review-passages"><ErrorNotice error={passages.error} />{passages.isPending ? <Loading>Loading source passages…</Loading> : null}<label>Reviewed source passage<select required value={selected?.id ?? ""} onChange={event => onChange(choices.find(source => source.id === event.target.value) ?? null)}><option value="">Choose the exact passage or range</option>{choices.map(source => <option key={source.id} value={source.id}>{source.locator} · {source.text.slice(0, 100)}</option>)}</select></label>
    {selected ? <p className="map-passage-preview">{selected.text.slice(0, 1000)}</p> : null}
    {passages.data && !passages.data.length ? <p className="field-help">No extracted passage is available on this page of the register. Use the PDF page or whole-document option only if you reviewed that scope.</p> : null}
    <div className="inline-actions"><button type="button" className="text-button" disabled={offset === 0 || passages.isPending} onClick={() => setOffset(value => Math.max(0, value - 50))}>Previous passages</button><button type="button" className="text-button" disabled={available.length < 50 || passages.isPending} onClick={() => setOffset(value => value + 50)}>More passages</button></div>
  </div>;
}
