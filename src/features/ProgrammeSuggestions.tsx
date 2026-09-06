import { useState } from "react";
import { tenderPath, useResource, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/ui";

export function ProgrammeSuggestions({ tenderId, onSelect }: {
  tenderId: string;
  onSelect: (programme: Schema<"ConstructionProgramme">) => void;
}) {
  const proposals = useResource<Schema<"ProgrammeProposalRecord">[]>(`${tenderPath(tenderId)}/programme-proposals`);
  const [selected, setSelected] = useState("");
  const proposal = proposals.data?.find(item => item.run_id === selected);
  return <section className="programme-suggestions">
    <h3>Proposed by the Tender Manager</h3>
    <ErrorNotice error={proposals.error} />
    {proposals.isPending ? <Loading>Loading saved programme proposals…</Loading> : null}
    {proposals.data?.length ? <>
      <label>Saved programme<select value={selected} onChange={event => setSelected(event.target.value)}>
        <option value="">Choose a proposal</option>
        {proposals.data.map(item => <option key={item.run_id} value={item.run_id} disabled={!item.is_current}>{item.programme.title} · {new Date(item.created_at).toLocaleDateString()}{item.is_current ? "" : " · Sources changed"}</option>)}
      </select></label>
      {proposal ? <p className="field-help">{proposal.programme.activities.length} proposed activities. Review the working calendar, durations, dependencies and assumptions before creating the draft.</p> : null}
      <button type="button" className="button" disabled={!proposal?.is_current} onClick={() => { if (proposal?.is_current) onSelect(proposal.programme); }}>Use proposal in this form</button>
    </> : proposals.data ? <p className="field-help">Ask the Tender Manager to propose a construction programme, or enter your own reviewed activities below.</p> : null}
  </section>;
}
