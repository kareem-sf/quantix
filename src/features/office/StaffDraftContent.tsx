import type { ReactNode } from "react";
import {
  ExternalLink as ExternalLinkIcon,
  FileCheck2,
  FileText,
} from "lucide-react";
import { ExternalLink } from "../../components/ExternalLink";
import type { SourceSelection } from "../Sources";
import type { Schema } from "../../api";

type StaffResult = Schema<"StaffResult">;
type OfficeOutput = Schema<"OfficeOutput">;
type AIRoute = Schema<"AIRoute">;
type ConstructionProgramme = Schema<"ConstructionProgramme">;
type ProgrammeActivity = Schema<"ProgrammeActivity">;
type FindingProposal = Schema<"FindingProposal">;
type TaskProposal = Schema<"TaskProposal">;

export type StaffDraftContentProps = {
  result: StaffResult;
  onSource: (source: SourceSelection) => void;
  compact?: boolean;
  renderEvidence?: (ids: string[]) => ReactNode;
};

/**
 * Renders the complete immutable staff draft as inspectable engineering
 * sections. This is shared by the desk and the workspace result pane; it does
 * not imply engineer acceptance or publish any proposal.
 */
export function StaffDraftContent({
  result,
  onSource,
  compact = false,
  renderEvidence,
}: StaffDraftContentProps) {
  const output = result.office_output;
  const sourceIds = unique([
    ...(output.source_ids ?? []),
    ...(result.source_ids_read ?? []),
  ]);

  return (
    <article
      className={`staff-draft-content${compact ? " staff-draft-content-compact" : ""}`}
    >
      <header className="staff-draft-heading">
        <div>
          <div className="staff-draft-title-row">
            <FileCheck2 size={18} aria-hidden="true" />
            <h3>{output.summary}</h3>
          </div>
          <p className="office-muted">
            Staff draft ·{" "}
            {result.currentness === "current" ? "Current" : "Needs review"}
          </p>
        </div>
        <span
          className={`office-currentness office-currentness-${result.currentness}`}
        >
          {result.currentness === "current" ? "Current draft" : "Needs review"}
        </span>
      </header>

      <p className="staff-draft-warning">
        This is draft engineering content. A source receipt records what the
        staff member inspected; it is not engineer review or approval.
      </p>

      {sourceIds.length ? (
        <DraftSection title="Evidence inspected by staff">
          {renderEvidence ? (
            renderEvidence(sourceIds)
          ) : (
            <SourceLinks ids={sourceIds} onSource={onSource} />
          )}
        </DraftSection>
      ) : null}

      {output.findings?.length ? (
        <DraftSection title="Finding proposals">
          <div className="staff-draft-record-list">
            {output.findings.map((finding, index) => (
              <FindingRecord
                key={`${finding.title}:${index}`}
                finding={finding}
                onSource={onSource}
              />
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.plan ? (
        <DraftSection title="Plan proposal">
          <h4>{output.plan.title}</h4>
          <div className="staff-draft-record-list">
            {output.plan.tasks.map((task, index) => (
              <TaskRecord
                key={`${task.title}:${index}`}
                task={task}
                onSource={onSource}
              />
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.web_findings?.length ? (
        <DraftSection title="Web research findings">
          <div className="staff-draft-record-list">
            {output.web_findings.map((finding, index) => (
              <article
                key={`${finding.title}:${index}`}
                className="staff-draft-record"
              >
                <h4>{finding.title}</h4>
                <p>{finding.detail}</p>
                <UrlLinks urls={finding.urls} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.price_proposals?.length ? (
        <DraftSection title="Price proposals">
          <div className="staff-draft-record-list">
            {output.price_proposals.map((proposal, index) => (
              <article
                key={`${proposal.item}:${index}`}
                className="staff-draft-record"
              >
                <h4>{proposal.item}</h4>
                <dl className="staff-draft-facts">
                  <Fact
                    label="Amount"
                    value={`${proposal.amount} ${proposal.currency}`}
                  />
                  <Fact label="Unit" value={proposal.unit} />
                  <Fact label="Location" value={proposal.location} />
                  <Fact label="Tax basis" value={proposal.tax_basis} />
                  <Fact label="Basis" value={proposal.basis} />
                  <Fact
                    label="Observed on"
                    value={proposal.observed_on ?? "Not recorded."}
                  />
                  <Fact
                    label="Valid until"
                    value={proposal.valid_until ?? "Not recorded."}
                  />
                  <Fact label="Conditions" value={proposal.conditions} />
                </dl>
                <UrlLinks urls={proposal.urls} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.quote_drafts?.length ? (
        <DraftSection title="Quote drafts">
          <div className="staff-draft-record-list">
            {output.quote_drafts.map((draft, index) => (
              <article
                key={`${draft.subject}:${index}`}
                className="staff-draft-record"
              >
                <h4>{draft.subject}</h4>
                <dl className="staff-draft-facts">
                  <Fact label="To" value={draft.to.join(", ")} />
                  <Fact
                    label="Cc"
                    value={draft.cc?.join(", ") ?? "Not recorded."}
                  />
                  <Fact
                    label="Attachments"
                    value={draft.attachment_ids?.join(", ") ?? "None recorded."}
                  />
                </dl>
                <p className="staff-draft-long-text">{draft.body}</p>
                <SourceLinks ids={draft.source_ids ?? []} onSource={onSource} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.unit_rate_proposals?.length ? (
        <DraftSection title="Unit rate proposals">
          <div className="staff-draft-record-list">
            {output.unit_rate_proposals.map((proposal, index) => (
              <article
                key={`${proposal.item_id}:${index}`}
                className="staff-draft-record"
              >
                <h4>Item {proposal.item_id}</h4>
                <dl className="staff-draft-facts">
                  <Fact
                    label="Unit rate"
                    value={proposal.unit_rate ?? "Not recorded."}
                  />
                  <Fact label="Currency" value={proposal.currency} />
                  <Fact label="Tax basis" value={proposal.tax_basis} />
                  <Fact
                    label="VAT"
                    value={proposal.vat_percent ?? "Not recorded."}
                  />
                  <Fact label="Rate basis" value={proposal.provenance.basis} />
                  <Fact
                    label="Observed on"
                    value={proposal.provenance.observed_on}
                  />
                  <Fact
                    label="Geography"
                    value={proposal.provenance.geography}
                  />
                  <Fact
                    label="Conditions"
                    value={proposal.provenance.conditions}
                  />
                </dl>
                <RateComponents components={proposal.components ?? []} />
                <SourceLinks
                  ids={proposal.provenance.source_ids ?? []}
                  onSource={onSource}
                />
                <UrlLinks urls={proposal.provenance.urls ?? []} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.project_map_nodes?.length ? (
        <DraftSection title="Project map proposals">
          <div className="staff-draft-record-list">
            {output.project_map_nodes.map((node, index) => (
              <article
                key={`${node.kind}:${node.title}:${index}`}
                className="staff-draft-record"
              >
                <h4>{node.title}</h4>
                <dl className="staff-draft-facts">
                  <Fact label="Kind" value={node.kind} />
                  <Fact
                    label="Parent"
                    value={node.parent_id ?? "Not recorded."}
                  />
                </dl>
                <p>{node.detail}</p>
                <SourceLinks ids={node.source_ids} onSource={onSource} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.submission_requirements?.length ? (
        <DraftSection title="Submission requirements">
          <div className="staff-draft-record-list">
            {output.submission_requirements.map((requirement, index) => (
              <article
                key={`${requirement.title}:${index}`}
                className="staff-draft-record"
              >
                <h4>{requirement.title}</h4>
                <p>{requirement.detail}</p>
                <dl className="staff-draft-facts">
                  <Fact
                    label="Deliverable"
                    value={requirement.deliverable_kind}
                  />
                  <Fact
                    label="Applicability"
                    value={
                      requirement.applicability === "unconditional"
                        ? "Always applies"
                        : requirement.applicability === "conditional"
                          ? "Only when stated conditions apply"
                          : "Not established — needs review"
                    }
                  />
                  <Fact
                    label="Conditions"
                    value={requirement.condition || "None recorded"}
                  />
                  <Fact
                    label="Due date"
                    value={requirement.due_date ?? "Not recorded."}
                  />
                </dl>
                <h5>Exact source clause</h5>
                <blockquote className="whitespace-pre-wrap">
                  {requirement.source_quote ||
                    "No exact clause recorded. Inspect the cited source."}
                </blockquote>
                <h5>Stated exceptions</h5>
                {requirement.exceptions?.length ? (
                  <ul>
                    {requirement.exceptions.map((exception, exceptionIndex) => (
                      <li key={exceptionIndex}>{exception}</li>
                    ))}
                  </ul>
                ) : (
                  <p>None recorded</p>
                )}
                <SourceLinks ids={requirement.source_ids} onSource={onSource} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.programme_proposal ? (
        <DraftSection title="Programme proposal">
          <ProgrammeContent
            programme={output.programme_proposal}
            onSource={onSource}
          />
        </DraftSection>
      ) : null}

      {output.boq_item_proposals?.length ? (
        <DraftSection title="Source BOQ row proposals">
          <div className="staff-draft-record-list">
            {output.boq_item_proposals.map((row, index) => (
              <article
                className="staff-draft-record"
                key={`${row.source_id}:${row.row_reference}:${index}`}
              >
                <h4>{row.description}</h4>
                <p>Draft source row — requires engineer confirmation.</p>
                <dl className="staff-draft-facts">
                  <Fact label="Row reference" value={row.row_reference} />
                  <Fact
                    label="Supplied quantity"
                    value={`${row.quantity} ${row.unit}`}
                  />
                </dl>
                <h5>Exact source excerpt</h5>
                <blockquote className="whitespace-pre-wrap">
                  {row.source_excerpt}
                </blockquote>
                <SourceLinks ids={[row.source_id]} onSource={onSource} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.drawing_measurements?.length ? (
        <DraftSection title="Drawing measurements">
          <div className="staff-draft-record-list">
            {output.drawing_measurements.map((measurement, index) => (
              <article
                key={`${measurement.artifact_id}:${measurement.page}:${index}`}
                className="staff-draft-record"
              >
                <h4>{measurement.scope_label}</h4>
                <dl className="staff-draft-facts">
                  <Fact label="Artifact" value={measurement.artifact_id} />
                  <Fact label="Page" value={String(measurement.page)} />
                  <Fact label="Mode" value={measurement.mode} />
                  <Fact
                    label="Calibration"
                    value={measurement.calibration_metres ?? "Not recorded."}
                  />
                </dl>
                <p>
                  Points:{" "}
                  {measurement.points
                    .map(([x, y]) => `(${x}, ${y})`)
                    .join(" · ") || "None recorded."}
                </p>
                {measurement.calibration_points?.length ? (
                  <p>
                    Calibration points:{" "}
                    {measurement.calibration_points
                      .map(([x, y]) => `(${x}, ${y})`)
                      .join(" · ")}
                  </p>
                ) : null}
                <SourceLinks ids={measurement.source_ids} onSource={onSource} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.draft_documents?.length ? (
        <DraftSection title="Draft documents">
          <div className="staff-draft-record-list">
            {output.draft_documents.map((document, index) => (
              <article
                key={`${document.kind}:${index}`}
                className="staff-draft-record"
              >
                <h4>{document.kind}</h4>
                <Fact
                  label="Task"
                  value={document.task_id ?? "Not recorded."}
                />
                {document.programme ? (
                  <ProgrammeContent
                    programme={document.programme}
                    onSource={onSource}
                  />
                ) : null}
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {output.quantity_proposals?.length ? (
        <DraftSection title="Quantity proposals">
          <div className="staff-draft-record-list">
            {output.quantity_proposals.map((proposal, index) => (
              <article
                key={`${proposal.item_id}:${index}`}
                className="staff-draft-record"
              >
                <h4>Item {proposal.item_id}</h4>
                <dl className="staff-draft-facts">
                  <Fact label="Quantity" value={proposal.quantity} />
                  <Fact label="Calculation" value={proposal.calculation} />
                </dl>
                <SourceLinks ids={proposal.source_ids} onSource={onSource} />
              </article>
            ))}
          </div>
        </DraftSection>
      ) : null}

      {result.authored_notes?.length ? (
        <DraftSection title="Authored notes">
          <ul className="staff-draft-list">
            {result.authored_notes.map((note, index) => (
              <li key={`${note}:${index}`}>{note}</li>
            ))}
          </ul>
        </DraftSection>
      ) : null}

      {result.source_bases?.length ? (
        <DraftSection title="Exact evidence basis">
          <div className="staff-draft-record-list">
            {result.source_bases.map((basis) => (
              <button
                key={`${basis.source_id}:${basis.artifact_version}:${basis.locator}`}
                type="button"
                className="office-reference-link"
                onClick={() =>
                  onSource({
                    sourceId: basis.source_id,
                    artifactId: basis.artifact_id,
                    version: basis.artifact_version,
                    contentHash: basis.artifact_hash,
                    origin: basis.locator,
                  })
                }
              >
                <FileText size={15} aria-hidden="true" />
                <span>
                  {basis.source_id} · {basis.locator} · artifact v
                  {basis.artifact_version}
                </span>
              </button>
            ))}
          </div>
        </DraftSection>
      ) : null}

      <details className="staff-draft-more-options">
        <summary>More options</summary>
        <dl className="staff-draft-facts">
          <Fact label="Result reference" value={result.id} />
          <Fact label="Assignment" value={result.assignment_id} />
          <Fact
            label="Staff version"
            value={`Version ${result.staff_version}`}
          />
          <Fact label="Work order" value={result.work_order_id} />
          <Fact label="Root run" value={result.root_run_id} />
          <Fact label="Route binding" value={result.route_binding_id} />
          <Fact
            label="Approved plan"
            value={result.approved_plan_id ?? "Not recorded."}
          />
          <Fact
            label="Trusted recipients"
            value={result.trusted_recipients?.join(", ") ?? "None recorded."}
          />
        </dl>
        {result.item_bases?.length ? (
          <StructuredList
            title="Item bases"
            values={result.item_bases.map(
              ([key, value]) => `${key} · ${value}`,
            )}
          />
        ) : null}
        {result.source_recipients?.length ? (
          <StructuredList
            title="Source recipients"
            values={result.source_recipients.map(
              ([sourceId, recipients]) =>
                `${sourceId} · ${recipients.join(", ")}`,
            )}
          />
        ) : null}
        {result.web_sources?.length ? (
          <StructuredRecords
            title="Web source records"
            values={result.web_sources}
          />
        ) : null}
        {result.usage && Object.keys(result.usage).length ? (
          <StructuredRecords title="Usage record" values={[result.usage]} />
        ) : null}
      </details>
    </article>
  );
}

function DraftSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="staff-draft-section">
      <h4>{title}</h4>
      {children}
    </section>
  );
}

function FindingRecord({
  finding,
  onSource,
}: {
  finding: FindingProposal;
  onSource: (source: SourceSelection) => void;
}) {
  return (
    <article className="staff-draft-record">
      <h4>{finding.title}</h4>
      <p className="office-muted">{finding.kind}</p>
      <p>{finding.detail}</p>
      <SourceLinks ids={finding.source_ids ?? []} onSource={onSource} />
    </article>
  );
}

function TaskRecord({
  task,
  onSource,
}: {
  task: TaskProposal;
  onSource: (source: SourceSelection) => void;
}) {
  return (
    <article className="staff-draft-record">
      <h4>{task.title}</h4>
      <p>{task.description}</p>
      <dl className="staff-draft-facts">
        <Fact label="Role" value={task.role} />
      </dl>
      <SourceLinks ids={task.source_ids ?? []} onSource={onSource} />
      {task.ai_route ? <RouteDetails route={task.ai_route} /> : null}
    </article>
  );
}

function RouteDetails({ route }: { route: AIRoute }) {
  return (
    <details className="staff-draft-route">
      <summary>Route basis</summary>
      <dl className="staff-draft-facts">
        <Fact label="Connection" value={route.connection_id} />
        <Fact label="Model" value={route.model_id} />
        <Fact label="Reasoning" value={route.reasoning ?? "Not recorded."} />
        <Fact label="Max output" value={String(route.max_output_tokens)} />
        <Fact
          label="Web search"
          value={route.web_search ? "Allowed" : "Not used"}
        />
        <Fact label="Max searches" value={String(route.max_search_calls)} />
      </dl>
    </details>
  );
}

function RateComponents({
  components,
}: {
  components: NonNullable<Schema<"UnitRateProposalInput">["components"]>;
}) {
  return components.length ? (
    <div className="staff-draft-subsection">
      <h5>Rate components</h5>
      <ul className="staff-draft-list">
        {components.map((component, index) => (
          <li key={`${component.name}:${index}`}>
            {component.name} · {component.quantity} {component.unit} ·{" "}
            {component.unit_rate}
          </li>
        ))}
      </ul>
    </div>
  ) : null;
}

function ProgrammeContent({
  programme,
  onSource,
}: {
  programme: ConstructionProgramme;
  onSource: (source: SourceSelection) => void;
}) {
  return (
    <div className="staff-draft-programme">
      <h5>{programme.title}</h5>
      <dl className="staff-draft-facts">
        <Fact label="Start date" value={programme.start_date} />
        <Fact label="Working week" value={programme.working_week.join(", ")} />
        <Fact
          label="Holidays"
          value={programme.holidays?.join(", ") ?? "None recorded."}
        />
      </dl>
      {programme.activities.length ? (
        <div className="staff-draft-subsection">
          <h5>Activities</h5>
          <div className="staff-draft-record-list">
            {programme.activities.map((activity) => (
              <ActivityRecord
                key={activity.id}
                activity={activity}
                onSource={onSource}
              />
            ))}
          </div>
        </div>
      ) : null}
      <StructuredList
        title="Assumptions"
        values={programme.assumptions ?? []}
      />
    </div>
  );
}

function ActivityRecord({
  activity,
  onSource,
}: {
  activity: ProgrammeActivity;
  onSource: (source: SourceSelection) => void;
}) {
  return (
    <article className="staff-draft-record">
      <h5>{activity.title}</h5>
      <dl className="staff-draft-facts">
        <Fact label="Duration" value={`${activity.duration_days} days`} />
        <Fact
          label="Predecessors"
          value={activity.predecessor_ids?.join(", ") ?? "None recorded."}
        />
      </dl>
      <StructuredList title="Assumptions" values={activity.assumptions ?? []} />
      <SourceLinks ids={activity.source_ids ?? []} onSource={onSource} />
    </article>
  );
}

function SourceLinks({
  ids,
  onSource,
}: {
  ids: string[];
  onSource: (source: SourceSelection) => void;
}) {
  const uniqueIds = unique(ids);
  return uniqueIds.length ? (
    <div className="staff-draft-source-links" aria-label="Source references">
      {uniqueIds.map((id) => (
        <button
          key={id}
          type="button"
          className="text-button"
          onClick={() => onSource({ sourceId: id })}
        >
          <FileText size={14} aria-hidden="true" />
          {id}
        </button>
      ))}
    </div>
  ) : null;
}

function UrlLinks({ urls }: { urls: string[] }) {
  return urls.length ? (
    <div className="staff-draft-url-links" aria-label="Web references">
      {urls.map((url) => (
        <ExternalLink key={url} href={url}>
          <ExternalLinkIcon size={13} aria-hidden="true" />
          <span>{url}</span>
        </ExternalLink>
      ))}
    </div>
  ) : null;
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value || "Not recorded."}</dd>
    </div>
  );
}

function StructuredList({
  title,
  values,
}: {
  title: string;
  values: string[];
}) {
  return values.length ? (
    <div className="staff-draft-subsection">
      <h5>{title}</h5>
      <ul className="staff-draft-list">
        {values.map((value, index) => (
          <li key={`${value}:${index}`}>{value}</li>
        ))}
      </ul>
    </div>
  ) : null;
}

function StructuredRecords({
  title,
  values,
}: {
  title: string;
  values: Record<string, unknown>[];
}) {
  return values.length ? (
    <div className="staff-draft-subsection">
      <h5>{title}</h5>
      {values.map((value, index) => (
        <dl key={index} className="staff-draft-facts">
          {Object.entries(value).map(([key, nested]) => (
            <Fact
              key={key}
              label={key.replaceAll("_", " ")}
              value={structuredText(nested)}
            />
          ))}
        </dl>
      ))}
    </div>
  ) : null;
}

function structuredText(value: unknown): string {
  if (value === null || value === undefined || value === "")
    return "Not recorded.";
  if (Array.isArray(value))
    return value.map((item) => structuredText(item)).join(" · ");
  if (typeof value === "object")
    return Object.entries(value)
      .map(
        ([key, nested]) =>
          `${key.replaceAll("_", " ")}: ${structuredText(nested)}`,
      )
      .join(" · ");
  return String(value);
}

function unique(values: string[]) {
  return [...new Set(values.filter(Boolean))];
}
