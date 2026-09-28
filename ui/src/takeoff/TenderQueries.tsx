import { useState } from "react";
import { Link, useParams } from "react-router";
import { useDocuments } from "../documents/queries";
import { firstName, useOffice, type Staff } from "../office/queries";
import { Findings, Reopen, ReviewNote, SendBack, WITH_MANAGER } from "../review/Review";
import {
  useChecks,
  useDecideLayerMap,
  useDecideQuery,
  useLayerMaps,
  useQueries,
  type LayerMap,
  type TenderQuery,
} from "./cad";

const DECIDED = ["approved", "office_approved"];

/** What the office found for the client to answer: work drawn but not billed, documents that disagree and errors in
 * the BOQ, each with where it shows. With the layer maps the drawing checks rest on, and what Quantix's own checks
 * find, for the office to look into. */
export function TenderQueries() {
  const { tenderId = "" } = useParams();
  const queries = useQueries(tenderId);
  const maps = useLayerMaps(tenderId);
  const office = useOffice(tenderId);
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));
  const list = queries.data ?? [];
  const waiting = list.filter((q) => q.status === "reviewed").length;

  return (
    <div className="flex min-h-full w-full max-w-[784px] flex-col gap-2 px-8 pt-14 pb-10">
      <h1 className="text-[28px] font-semibold tracking-tight">Queries</h1>
      <p className="text-ink-2">
        {list.length === 0
          ? "The office hasn’t raised any queries yet."
          : `${list.length} ${list.length === 1 ? "query" : "queries"} for the client${waiting ? `, ${waiting} waiting for you` : ""}.`}{" "}
        Each goes to the client only once you approve it.
      </p>
      <div className="flex flex-col border-t border-line">
        {list.map((q) => (
          <QueryCard key={q.id} tenderId={tenderId} query={q} people={people} />
        ))}
      </div>
      {(maps.data ?? []).length > 0 && (
        <>
          <h2 className="mt-8 font-semibold text-ink-2">Layer maps</h2>
          <p className="text-ink-3">What the drawings’ layers and blocks are: rooms and the drawing checks rest on them.</p>
          <div className="flex flex-col border-t border-line">
            {(maps.data ?? []).map((m) => (
              <MapCard key={m.id} tenderId={tenderId} layerMap={m} people={people} />
            ))}
          </div>
        </>
      )}
      <Checks tenderId={tenderId} />
    </div>
  );
}

function QueryCard(props: { tenderId: string; query: TenderQuery; people: Map<string, Staff> }) {
  const decide = useDecideQuery(props.tenderId);
  const q = props.query;
  const by = props.people.get(q.proposed_by);
  return (
    <article className="flex flex-col gap-1.5 border-b border-line py-4">
      <span className="text-xs text-ink-3">
        {q.kind_label}
        {q.boq_item ? ` · BOQ ${q.boq_item}` : ""}
        {by ? ` · raised by ${firstName(by)}` : ""}
      </span>
      <h3 className="text-[15px] font-semibold">{q.title}</h3>
      <p className="leading-normal text-ink-2">{q.detail}</p>
      {q.figures.length > 0 && <p className="text-ink-2">Quantix’s takeoff: {q.figures.join("; ")}</p>}
      <div className="flex flex-wrap gap-x-3 gap-y-1">
        {q.sources.map((s, i) => (
          <Link
            key={i}
            to={`/tenders/${props.tenderId}/documents?doc=${s.document_id}&page=${s.page}`}
            className="text-ink-2 underline underline-offset-4"
            title={s.quote ?? `${s.objects.length} drawing objects`}
          >
            {s.document_name}, page {s.page}
            {s.objects.length > 0 ? ` (${s.objects.length} objects)` : ""}
          </Link>
        ))}
      </div>
      {q.governs && <p className="text-ink-2">Governs: {q.governs}</p>}
      <blockquote className="border-l-2 border-line-strong pl-3 leading-normal">{q.wording}</blockquote>
      {q.status === "proposed" && <span className="text-xs text-ink-3">{WITH_MANAGER}</span>}
      <ReviewNote reviewedBy={q.reviewed_by} note={q.review_note} people={props.people} />
      {q.status !== "approved" && <Findings kind="query" id={q.id} tenderId={props.tenderId} />}
      <span className="flex flex-wrap gap-3 pt-1">
        {q.status === "reviewed" && (
          <>
            <button onClick={() => decide.mutate({ id: q.id, approve: true })} className="font-medium">
              Approve for the client
            </button>
            <SendBack onSend={(reason) => decide.mutate({ id: q.id, approve: false, reason })} />
          </>
        )}
        {DECIDED.includes(q.status) && (
          <>
            <span className="text-ink-3">{q.status === "approved" ? "Approved" : "Approved by the office"}</span>
            <Reopen kind="query" id={q.id} />
          </>
        )}
      </span>
    </article>
  );
}

function MapCard(props: { tenderId: string; layerMap: LayerMap; people: Map<string, Staff> }) {
  const decide = useDecideLayerMap(props.tenderId);
  const m = props.layerMap;
  const entries = [...Object.entries(m.layers), ...Object.entries(m.blocks).map(([k, v]) => [`block ${k}`, v])];
  return (
    <article className="flex flex-col gap-1.5 border-b border-line py-4">
      <span className="text-xs text-ink-3">Worked out on {m.document_name}</span>
      <div className="flex flex-wrap gap-x-4 gap-y-1">
        {entries.map(([name, meaning]) => (
          <span key={name} className="text-[13px]">
            <span className="font-medium">{name}</span> <span className="text-ink-3">{meaning.replace(/_/g, " ")}</span>
          </span>
        ))}
      </div>
      <p className="text-ink-2">{m.note}</p>
      {m.status === "proposed" && <span className="text-xs text-ink-3">{WITH_MANAGER}</span>}
      <ReviewNote reviewedBy={m.reviewed_by} note={m.review_note} people={props.people} />
      <span className="flex flex-wrap gap-3 pt-1">
        {m.status === "reviewed" && (
          <>
            <button onClick={() => decide.mutate({ id: m.id, approve: true })} className="font-medium">
              Approve the map
            </button>
            <SendBack onSend={(reason) => decide.mutate({ id: m.id, approve: false, reason })} />
          </>
        )}
        {DECIDED.includes(m.status) && <Reopen kind="layers" id={m.id} />}
      </span>
    </article>
  );
}

function Checks({ tenderId }: { tenderId: string }) {
  const documents = useDocuments(tenderId);
  const drawings = (documents.data ?? []).filter((d) => d.kind === "cad" && d.status === "read");
  const [documentId, setDocumentId] = useState<string | null>(null);
  const checks = useChecks(tenderId, documentId);
  const found = checks.data ?? [];
  return (
    <>
      <h2 className="mt-8 font-semibold text-ink-2">What Quantix’s checks find</h2>
      <p className="text-ink-3">Leads for the office to look into: a query is raised only for what matters to the price.</p>
      <select
        aria-label="Checks of"
        value={documentId ?? ""}
        onChange={(e) => setDocumentId(e.target.value || null)}
        className="mt-1 h-8 max-w-[380px] rounded-md border border-line-strong bg-white px-2 text-[13px]"
      >
        <option value="">The BOQ, and across the drawings</option>
        {drawings.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name}
          </option>
        ))}
      </select>
      <div className="flex flex-col border-t border-line">
        {checks.isLoading && <p className="py-3 text-ink-3">Checking…</p>}
        {checks.isError && <p className="py-3 text-attention">{checks.error.message}</p>}
        {!checks.isLoading && found.length === 0 && <p className="py-3 text-ink-3">Nothing found.</p>}
        {found.map((p, i) => (
          <div key={i} className="flex flex-col gap-0.5 border-b border-line py-3 leading-normal">
            <span>{p.message}</span>
            {p.document_id && (
              <Link
                to={
                  p.objects.length
                    ? `/tenders/${tenderId}/takeoff?doc=${p.document_id}&page=${p.page}`
                    : `/tenders/${tenderId}/documents?doc=${p.document_id}&page=${p.page}`
                }
                className="text-ink-2 underline underline-offset-4"
              >
                {p.objects.length ? "Open the drawing" : "Open the page"}
              </Link>
            )}
          </div>
        ))}
      </div>
    </>
  );
}
