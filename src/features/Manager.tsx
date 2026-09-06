import { useCallback, useState } from "react";
import { ClipboardList, File, MessageSquare, PanelRight } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  isActive,
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { Empty, ErrorNotice, Loading, Modal, Status } from "../components/ui";
import { Composer } from "./Composer";
import { Citations, type SourceSelection } from "./Sources";
import { DecisionForm, PlanView, RunRow } from "./Work";

export function Manager({
  overview,
  artifacts,
  settings,
  onImport,
  onSettings,
  onSource,
}: {
  overview: Schema<"Overview">;
  artifacts: Schema<"Artifact">[];
  settings?: Schema<"Settings">;
  onImport: () => void;
  onSettings: () => void;
  onSource: (source: SourceSelection) => void;
}) {
  const tenderId = overview.tender.id,
    api = useApi(),
    refresh = useRefresh();
  const messages = useResource<Schema<"Message">[]>(
    `${tenderPath(tenderId)}/messages`,
    true,
  );
  const runs = useResource<Schema<"Run">[]>(
    `${tenderPath(tenderId)}/runs`,
    true,
  );
  const [detailsOpen, setDetailsOpen] = useState(false);
  const closeDetails = useCallback(() => setDetailsOpen(false), []);
  const latestRun = runs.data
    ?.filter((run) => run.kind === "manager" || run.kind === "import")
    .sort((a, b) => b.created_at.localeCompare(a.created_at))[0];
  const analyses = overview.findings.filter(
    (finding) => finding.origin === "agent",
  );
  const processing = overview.findings.filter(
    (finding) => finding.origin !== "agent",
  );
  const changes = () => {
    requestAnimationFrame(() =>
      document
        .querySelector<HTMLTextAreaElement>(
          '[aria-label="Message to Tender Manager"]',
        )
        ?.focus(),
    );
  };
  return (
    <div className="manager-layout">
      <div className="manager-column">
        <div className="manager-scroll">
          <div className="manager-heading">
            <span className="manager-mark">
              <ClipboardList size={25} />
            </span>
            <div>
              <h2>Tender Manager</h2>
              <p className="muted">
                {overview.artifact_count
                  ? "Review the sources, agree the scope and direct the work."
                  : "Start with the tender documents."}
              </p>
            </div>
            <button
              className="icon-button details-toggle"
              onClick={() => setDetailsOpen(true)}
              aria-label="Open project details"
            >
              <PanelRight size={20} />
            </button>
          </div>
          {settings && !settings.provider_ready ? (
            <div className="setup-note">
              <p>
                Configure the AI connection to work with the Tender Manager.
                Document importing and source inspection remain available.
              </p>
              <button className="text-button" onClick={onSettings}>
                Open settings
              </button>
            </div>
          ) : null}
          {overview.active_runs.map((run) => (
            <RunRow key={run.id} run={run} compact />
          ))}
          {latestRun &&
          ["failed", "cancelled", "interrupted"].includes(latestRun.status) ? (
            <RunRow run={latestRun} compact />
          ) : null}
          <ErrorNotice error={messages.error || runs.error} />
          {overview.artifact_count === 0 &&
          overview.active_runs.length === 0 ? (
            <Empty
              title="Add the tender documents"
              action={
                <button className="button primary" onClick={onImport}>
                  Import package
                </button>
              }
            >
              The manager will use the package to understand the project and
              propose the work.
            </Empty>
          ) : null}
          {messages.isPending ? <Loading>Loading conversation…</Loading> : null}
          {messages.data?.map((message) => (
            <article
              className={`message message-${message.role}`}
              key={message.id}
            >
              <div className="message-label">
                {message.role === "engineer"
                  ? "You"
                  : message.role === "manager"
                    ? "Tender Manager"
                    : "Office record"}
              </div>
              <div className="markdown">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                  {message.content}
                </ReactMarkdown>
              </div>
              <Citations
                ids={message.source_ids}
                tenderId={tenderId}
                onOpen={onSource}
              />
            </article>
          ))}
          {analyses.length ? (
            <section className="analysis-section">
              <h2>Document analysis</h2>
              {analyses.map((finding) => (
                <FindingRow
                  key={finding.id}
                  finding={finding}
                  tenderId={tenderId}
                  onSource={onSource}
                />
              ))}
            </section>
          ) : overview.artifact_count > 0 && !overview.plan ? (
            <section className="analysis-section">
              <h2>Document analysis</h2>
              <p className="muted">
                No findings have been recorded yet. Ask the manager to review
                the documents and propose a plan.
              </p>
            </section>
          ) : null}
          {processing.length ? (
            <details className="processing-findings">
              <summary>
                Document reading notes <span>{processing.length}</span>
              </summary>
              <p className="field-help">
                These notes describe imported files and what could be read. The
                manager's review is recorded separately.
              </p>
              {processing.map((finding) => (
                <FindingRow
                  key={finding.id}
                  finding={finding}
                  tenderId={tenderId}
                  onSource={onSource}
                />
              ))}
            </details>
          ) : null}
          {overview.plan ? (
            <PlanView
              plan={overview.plan}
              tenderId={tenderId}
              onChanges={changes}
              onSource={onSource}
            />
          ) : null}
        </div>
        <Composer
          tenderId={tenderId}
          onImport={onImport}
          busy={overview.active_runs.some(
            (run) => run.kind === "manager" && isActive(run.status),
          )}
          onSend={async (content) => {
            await api.post<Schema<"Run">>(`${tenderPath(tenderId)}/messages`, {
              content,
            } satisfies Schema<"MessageRequest">);
            await refresh();
          }}
        />
      </div>
      <ContextRail
        overview={overview}
        artifacts={artifacts}
        onSource={onSource}
      />
      {detailsOpen ? (
        <Modal title="Project details" onClose={closeDetails}>
          <div className="project-details-modal">
            <ContextRail
              overview={overview}
              artifacts={artifacts}
              onSource={(selection) => {
                closeDetails();
                onSource(selection);
              }}
            />
          </div>
        </Modal>
      ) : null}
    </div>
  );
}
function FindingRow({
  finding,
  tenderId,
  onSource,
}: {
  finding: Schema<"Finding">;
  tenderId: string;
  onSource: (source: SourceSelection) => void;
}) {
  const api = useApi(),
    refresh = useRefresh();
  const [decision, setDecision] = useState<
    Schema<"DecisionRequest">["decision"] | null
  >(null);
  const close = useCallback(() => setDecision(null), []);
  return (
    <article className="finding-row">
      <div className="finding-heading">
        <strong>{finding.title}</strong>
        <Status value={finding.state} />
      </div>
      <p>{finding.detail}</p>
      {finding.is_stale ? (
        <p className="warning-text">
          A source has changed. Review this finding against the current
          revision.
        </p>
      ) : null}
      <Citations
        ids={finding.source_ids}
        tenderId={tenderId}
        onOpen={onSource}
      />
      <div className="finding-actions">
        {finding.state === "proposed" ? (
          <>
            <button
              className="text-button"
              onClick={() => setDecision("accept")}
            >
              Accept
            </button>
            <button
              className="text-button muted"
              onClick={() => setDecision("reject")}
            >
              Reject
            </button>
          </>
        ) : finding.state === "accepted" ? (
          <button
            className="text-button"
            onClick={() => setDecision("resolve")}
          >
            Mark resolved
          </button>
        ) : null}
      </div>
      {decision ? (
        <DecisionForm
          title={`${decision === "accept" ? "Accept" : decision === "reject" ? "Reject" : "Resolve"} finding`}
          action="Record decision"
          description={finding.title}
          onClose={close}
          onSubmit={async (rationale) => {
            await api.post(
              `${tenderPath(tenderId)}/findings/${finding.id}/decision`,
              { decision, rationale } satisfies Schema<"DecisionRequest">,
            );
            await refresh();
            close();
          }}
        />
      ) : null}
    </article>
  );
}
function ContextRail({
  overview,
  artifacts,
  onSource,
}: {
  overview: Schema<"Overview">;
  artifacts: Schema<"Artifact">[];
  onSource: (source: SourceSelection) => void;
}) {
  const questions = overview.findings.filter(
    (finding) =>
      finding.kind === "question" &&
      finding.state !== "resolved" &&
      finding.state !== "rejected",
  );
  return (
    <aside className="context-rail" aria-label="Project details">
      <section>
        <h3>Project scope</h3>
        <dl className="scope-values">
          <dt>Areas in the register</dt>
          <dd>
            {overview.areas.length
              ? overview.areas.join(", ")
              : "Not identified"}
          </dd>
        </dl>
      </section>
      <section>
        <h3>Document coverage</h3>
        <dl className="coverage-values">
          <dt>Registered</dt>
          <dd>{overview.coverage.registered ?? 0}</dd>
          <dt>Read</dt>
          <dd>{overview.coverage.extracted ?? 0}</dd>
          <dt>Needs attention</dt>
          <dd>{overview.coverage.needs_attention ?? 0}</dd>
          <dt>Unsupported</dt>
          <dd>{overview.coverage.unsupported ?? 0}</dd>
          <dt>Failed</dt>
          <dd>{overview.coverage.failed ?? 0}</dd>
        </dl>
        <p className="field-help">
          Read files still need analysis and an engineer's review. Findings and
          decisions are recorded separately.
        </p>
        {artifacts
          .filter((file) => file.is_current)
          .slice(0, 4)
          .map((file) => (
            <button
              key={file.id}
              className="coverage-file"
              onClick={() => onSource({ artifactId: file.id })}
            >
              <File size={27} />
              <span>
                <strong>{file.name}</strong>
                <Status value={file.status} />
              </span>
            </button>
          ))}
      </section>
      <section>
        <h3>Open questions</h3>
        {questions.length ? (
          questions
            .slice(0, 5)
            .map((question) => <p key={question.id}>{question.title}</p>)
        ) : (
          <p className="muted">
            <MessageSquare size={16} /> No open questions recorded.
          </p>
        )}
      </section>
    </aside>
  );
}
