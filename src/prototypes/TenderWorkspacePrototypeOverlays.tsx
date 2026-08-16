import {
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { ArrowLeft, FileSearch, FileText } from "lucide-react";

import {
  agentCopy,
  insuranceTreatments,
  roomMessages,
  type AgentKey,
  type AgentTab,
  type DecisionVersion,
  type RoomFilter,
} from "./TenderWorkspacePrototypeData";
import { Avatar } from "./TenderWorkspacePrototypePrimitives";
import { TenderList } from "./TenderWorkspacePrototypeShells";

function FocusedView({
  title,
  description,
  mode = "review",
  onClose,
  onBack,
  returnFocusTarget,
  children,
}: {
  title: string;
  description?: string;
  mode?: "review" | "sheet" | "room";
  onClose: () => void;
  onBack?: () => void;
  returnFocusTarget?: HTMLElement | null;
  children: ReactNode;
}) {
  const panelRef = useRef<HTMLDivElement>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    returnFocusRef.current = returnFocusTarget ?? (document.activeElement as HTMLElement | null);
    const frame = window.requestAnimationFrame(() => {
      panelRef.current?.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    });

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        closeRef.current();
        return;
      }
      if (event.key !== "Tab" || !panelRef.current) return;
      const focusable = Array.from(
        panelRef.current.querySelectorAll<HTMLElement>(
          "[data-autofocus], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex='-1'])",
        ),
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }

    document.addEventListener("keydown", onKeyDown);
    return () => {
      window.cancelAnimationFrame(frame);
      document.removeEventListener("keydown", onKeyDown);
      window.requestAnimationFrame(() => {
        if (document.querySelector("[role='dialog']")) return;
        const originalTarget = returnFocusRef.current;
        const target =
          originalTarget?.isConnected && !originalTarget.closest("[inert]")
            ? originalTarget
            : document.querySelector<HTMLElement>("[data-workspace-focus]");
        target?.focus();
      });
    };
  }, [returnFocusTarget]);

  return (
    <div
      className={`qxp-overlay qxp-overlay--${mode}`}
      role="presentation"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) onClose();
      }}
    >
      <div
        className="qxp-focused-view"
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="focused-view-title"
        aria-describedby={description ? "focused-view-description" : undefined}
      >
        <header className="qxp-focused-view__header">
          <div>
            {onBack ? (
              <button className="qxp-back" type="button" onClick={onBack}>
                <ArrowLeft size={18} aria-hidden="true" />
                Back
              </button>
            ) : null}
            <h2 id="focused-view-title" tabIndex={-1} data-autofocus>
              {title}
            </h2>
            {description ? <p id="focused-view-description">{description}</p> : null}
          </div>
          <button className="qxp-close" type="button" aria-label={`Close ${title}`} onClick={onClose}>
            Close
          </button>
        </header>
        <div className="qxp-focused-view__body">{children}</div>
      </div>
    </div>
  );
}

export function TenderChooser({
  selectedTenderId,
  onSelect,
  onStart,
  onClose,
}: {
  selectedTenderId: string | null;
  onSelect: (tenderId: string) => void;
  onStart: () => void;
  onClose: () => void;
}) {
  return (
    <FocusedView title="Your Tenders" description="Choose where you want to continue." mode="sheet" onClose={onClose}>
      <TenderList
        selectedTenderId={selectedTenderId}
        onStart={() => {
          onStart();
          onClose();
        }}
        onSelect={(tenderId) => {
          onSelect(tenderId);
          onClose();
        }}
      />
    </FocusedView>
  );
}

export function EvidenceReview({
  returnLabel,
  onClose,
}: {
  returnLabel: string;
  onClose: () => void;
}) {
  return (
    <FocusedView
      title="Insurance responsibility"
      description="Two current source clauses conflict. Review both before deciding."
      mode="sheet"
      onClose={onClose}
    >
      <div className="qxp-evidence-summary">
        <strong>Why this matters</strong>
        <p>
          Pricing the cover changes the estimate. Qualifying it may reduce compliance. The Manager
          will preserve your treatment in the Work Plan.
        </p>
      </div>
      <article className="qxp-source-card">
        <header>
          <strong>Employer's Requirements</strong>
          <span>01 Requirements / Volume 2.pdf · page 146 · Clause 9.4</span>
        </header>
        <blockquote>
          The Contractor shall provide latent-defect insurance for a period of six years following
          Practical Completion.
        </blockquote>
        <footer>Registered source · SHA-256 ending 9c41 · current version</footer>
      </article>
      <article className="qxp-source-card qxp-source-card--conflict">
        <header>
          <strong>Pricing Schedule</strong>
          <span>02 Commercial / Pricing Schedule.xlsx · row 118</span>
        </header>
        <blockquote>Latent-defect insurance: excluded from Contractor's price.</blockquote>
        <footer>Registered source · SHA-256 ending a722 · current version</footer>
      </article>
      <details className="qxp-technical-details">
        <summary>Technical provenance</summary>
        <dl>
          <div><dt>Evidence references</dt><dd>EVD-00481, EVD-00507</dd></div>
          <div><dt>Registered</dt><dd>14 Aug 2026, 09:42 by Tendering Engineer</dd></div>
          <div><dt>Extraction</dt><dd>Verified against the original source passage</dd></div>
        </dl>
      </details>
      <button className="qxp-primary" type="button" onClick={onClose}>
        {returnLabel}
      </button>
    </FocusedView>
  );
}

export function PlanReview({
  planVersion,
  deadline,
  onChangeRequested,
  onApprove,
  onClose,
}: {
  planVersion: number;
  deadline: string;
  onChangeRequested: () => void;
  onApprove: () => void;
  onClose: () => void;
}) {
  const [changeSection, setChangeSection] = useState("Work and sequence");
  const [change, setChange] = useState("");
  const [revision, setRevision] = useState<{ section: string; request: string } | null>(null);
  const baseSections = [
    ["Outcome and deadline", `Submit a compliant priced response by ${deadline}.`],
    ["Risks", "Insurance conflict, incomplete BMS quantities, and a five-day validity discrepancy."],
    ["Work and sequence", "Requirements → commercial review → estimate → response → independent review."],
    ["Team", "Tendering Manager, Requirements Analyst, Commercial Reviewer, Cost Estimator, Independent Reviewer."],
    ["Access and limits", "Registered Tender sources and approved Company Knowledge only. No external communication."],
    ["Independent review", "The Independent Reviewer cannot author the work they assess."],
    ["Assumptions", "No design development beyond the issued information; missing quantities remain explicit allowances."],
  ];

  function requestChange() {
    if (!change.trim()) return;
    setRevision({ section: changeSection, request: change.trim() });
    onChangeRequested();
    setChange("");
  }

  const sections = baseSections.map(([title, body]) =>
    revision?.section === title
      ? [title, `${revision.request} (Revision applied in Work Plan v${planVersion}.)`]
      : [title, body],
  );
  const exactVersion = planVersion;
  return (
    <FocusedView
      title={`Work Plan v${exactVersion}`}
      description="Review the exact plan that will control delegation, access, and work."
      mode="review"
      onClose={onClose}
    >
      {revision ? (
        <div className="qxp-notice" role="status">
          The Manager created v{exactVersion} and applied your requested change to{" "}
          {revision.section}. v{exactVersion - 1} remains in History.
        </div>
      ) : null}
      <div className="qxp-plan-overview">
        <div><small>Outcome</small><strong>Verified priced submission</strong></div>
        <div><small>Deadline</small><strong>{deadline}</strong></div>
        <div><small>Expected from you</small><strong>2 later decisions</strong></div>
      </div>
      <div className="qxp-plan-sections">
        {sections.map(([title, body], index) => (
          <details key={title} open={index === 0}>
            <summary>{title}</summary>
            <p>{body}</p>
          </details>
        ))}
      </div>
      <section className="qxp-change-request" aria-labelledby="change-title">
        <h3 id="change-title">Request a plan change</h3>
        <label>
          Plan section
          <select value={changeSection} onChange={(event) => setChangeSection(event.target.value)}>
            {sections.map(([title]) => <option key={title}>{title}</option>)}
          </select>
        </label>
        <label>
          What should change in {changeSection}?
          <textarea rows={3} value={change} onChange={(event) => setChange(event.target.value)} />
        </label>
        <button className="qxp-secondary" type="button" disabled={!change.trim()} onClick={requestChange}>
          Ask the Manager to revise this section
        </button>
      </section>
      <div className="qxp-formal-approval">
        <div>
          <strong>Approve exact Work Plan v{exactVersion}</strong>
          <p>All unblocked tasks start automatically inside this plan. Material changes return to you.</p>
        </div>
        <button className="qxp-primary" type="button" onClick={onApprove}>
          Approve and proceed
        </button>
      </div>
    </FocusedView>
  );
}

export function DecisionCorrection({
  versions,
  onSave,
  onClose,
}: {
  versions: DecisionVersion[];
  onSave: (version: DecisionVersion) => void;
  onClose: () => void;
}) {
  const current = versions[versions.length - 1];
  const [treatment, setTreatment] = useState(current?.treatment ?? insuranceTreatments[0]);
  const [reason, setReason] = useState("");
  const nextVersion = (current?.version ?? 0) + 1;
  const changed = Boolean(current && treatment !== current.treatment && reason.trim());

  return (
    <FocusedView
      title="Correct insurance treatment"
      description="Create a visible successor decision. The current record is never overwritten."
      mode="review"
      onClose={onClose}
    >
      <div className="qxp-decision-history">
        <strong>Current decision · D-009 v{current?.version ?? 1}</strong>
        <p>{current?.treatment}</p>
        <small>{current?.reason}</small>
      </div>
      <fieldset className="qxp-choice-list">
        <legend>Replacement treatment for v{nextVersion}</legend>
        {insuranceTreatments.map((option) => (
          <label className={treatment === option ? "is-selected" : ""} key={option}>
            <input
              type="radio"
              name="corrected-treatment"
              checked={treatment === option}
              onChange={() => setTreatment(option)}
            />
            <span>{option}</span>
          </label>
        ))}
      </fieldset>
      <label className="qxp-correction-reason">
        Why is this changing?
        <textarea rows={4} value={reason} onChange={(event) => setReason(event.target.value)} />
      </label>
      <div className="qxp-impact-warning">
        <strong>What happens after correction</strong>
        <p>
          The Cost Estimate and Commercial Response pause. The Manager updates their context and
          returns any required Work Plan Amendment. Unaffected review continues.
        </p>
      </div>
      <div className="qxp-formal-approval">
        <span>v{current?.version ?? 1} remains preserved in Decision History.</span>
        <button
          className="qxp-primary"
          type="button"
          disabled={!changed}
          onClick={() => onSave({ version: nextVersion, treatment, reason: reason.trim() })}
        >
          Record corrected decision v{nextVersion}
        </button>
      </div>
    </FocusedView>
  );
}

export function TeamRoom({
  filter,
  onFilter,
  onOpenAgent,
  onClose,
  returnFocusTarget,
}: {
  filter: RoomFilter;
  onFilter: (filter: RoomFilter) => void;
  onOpenAgent: (agent: AgentKey) => void;
  onClose: () => void;
  returnFocusTarget?: HTMLElement | null;
}) {
  const visible = roomMessages.filter((message) => filter === "all" || message.kind === filter);
  return (
    <FocusedView
      title="Team working"
      description="Meaningful Tender Office communication, coordinated by your Tendering Manager."
      mode="room"
      onClose={onClose}
      returnFocusTarget={returnFocusTarget}
    >
      <div className="qxp-room-presence">
        <span>3 specialists working</span><span>1 waiting</span><strong>1 finding needs you</strong>
      </div>
      <div className="qxp-filter-bar" aria-label="Filter team messages">
        {(["all", "needs-you", "handoffs", "outputs"] as RoomFilter[]).map((item) => (
          <button
            className={filter === item ? "is-active" : ""}
            key={item}
            type="button"
            aria-pressed={filter === item}
            onClick={() => onFilter(item)}
          >
            {item === "all" ? "All messages" : item === "needs-you" ? "Needs you" : item === "handoffs" ? "Handoffs" : "Outputs"}
          </button>
        ))}
      </div>
      <div className="qxp-room-thread">
        {visible.map((message) => (
          <article key={message.ref}>
            <button
              className="qxp-agent-identity"
              type="button"
              disabled={message.agent === "manager"}
              onClick={() => message.agent !== "manager" && onOpenAgent(message.agent)}
            >
              <Avatar agent={message.agent} subtle />
              <span>{message.agent === "manager" ? "Tendering Manager" : agentCopy[message.agent].role}</span>
            </button>
            <div>
              <span className={`qxp-message-kind qxp-message-kind--${message.kind}`}>{message.label}</span>
              <p>{message.body}</p>
              <button className="qxp-reference" type="button">{message.ref}</button>
            </div>
          </article>
        ))}
      </div>
      <div className="qxp-room-composer">
        <label htmlFor="team-message">Message the Tender Office</label>
        <div>
          <input id="team-message" placeholder="Ask the Manager or a specialist..." />
          <button className="qxp-primary" type="button">Send</button>
        </div>
        <small>The Manager coordinates instructions and routes scope changes through amendment.</small>
      </div>
    </FocusedView>
  );
}

const agentTabs: AgentTab[] = ["conversation", "context", "activity", "outputs"];

export function AgentWorkroom({
  agent,
  tab,
  onAgentChange,
  onTabChange,
  onBack,
  onClose,
  planVersion,
  returnFocusTarget,
}: {
  agent: AgentKey;
  tab: AgentTab;
  onAgentChange: (agent: AgentKey) => void;
  onTabChange: (tab: AgentTab) => void;
  onBack: () => void;
  onClose: () => void;
  planVersion: number;
  returnFocusTarget?: HTMLElement | null;
}) {
  const copy = agentCopy[agent];

  function onTabKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>, currentTab: AgentTab) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const index = agentTabs.indexOf(currentTab);
    const direction = event.key === "ArrowLeft" ? -1 : 1;
    const next = agentTabs[(index + direction + agentTabs.length) % agentTabs.length];
    onTabChange(next);
    window.requestAnimationFrame(() => document.getElementById(`agent-tab-${next}`)?.focus());
  }

  return (
    <FocusedView
      title={copy.role}
      description="Inspect attributable communication, exact supplied context, activity, and outputs for this Agent Run."
      mode="room"
      onBack={onBack}
      onClose={onClose}
      returnFocusTarget={returnFocusTarget}
    >
      <div className="qxp-agent-run-header">
        <Avatar agent={agent} />
        <div><strong>Agent Run AR-2026-0814-{agent === "requirements" ? "031" : "032"}</strong><span>Working inside approved Work Plan v{planVersion}</span></div>
        <label>
          Inspect Agent
          <select value={agent} onChange={(event) => onAgentChange(event.target.value as AgentKey)}>
            <option value="requirements">Requirements Analyst</option>
            <option value="commercial">Commercial Reviewer</option>
          </select>
        </label>
      </div>
      <div className="qxp-agent-tabs" role="tablist" aria-label="Agent workroom">
        {agentTabs.map((item) => (
          <button
            id={`agent-tab-${item}`}
            className={tab === item ? "is-active" : ""}
            key={item}
            type="button"
            role="tab"
            aria-selected={tab === item}
            aria-controls={`agent-panel-${item}`}
            tabIndex={tab === item ? 0 : -1}
            onClick={() => onTabChange(item)}
            onKeyDown={(event) => onTabKeyDown(event, item)}
          >
            {item[0].toUpperCase() + item.slice(1)}
          </button>
        ))}
      </div>
      {agentTabs.map((panel) => (
        <section
          id={`agent-panel-${panel}`}
          key={panel}
          role="tabpanel"
          aria-labelledby={`agent-tab-${panel}`}
          className="qxp-agent-panel"
          hidden={tab !== panel}
        >
        {panel === "conversation" ? (
          <div className="qxp-agent-conversation">
            <article><strong>Tendering Manager</strong><p>{copy.objective}</p><small>Task assignment · 10:04</small></article>
            <article className="qxp-handoff-card"><strong>Cross-Agent handoff H-014</strong><p>Requirements and Commercial share only the two registered insurance passages and their attributable findings.</p><small>Manager-authorized Thread Exposure</small></article>
            <article><strong>{copy.role}</strong><p>{agent === "requirements" ? "I verified Clause 9.4 against the source and linked it to the conflict." : "I compared the exclusion against Clause 9.4 and returned a pricing-and-clarification recommendation."}</p><small>Complete Agent message · 10:17</small></article>
          </div>
        ) : null}
        {panel === "context" ? (
          <div className="qxp-context-view">
            <div className="qxp-context-callout"><strong>Actually supplied to this Run</strong><span>Exact immutable context, not the Agent's broader access ceiling.</span></div>
            <dl>
              <div><dt>Agent Profile Version</dt><dd>{copy.role} v3 · approved instructions</dd></div>
              <div><dt>Task objective</dt><dd>{copy.objective}</dd></div>
              <div><dt>Registered inputs</dt><dd>Volume 2.pdf v1; Pricing Schedule.xlsx v1</dd></div>
              <div><dt>Data Views</dt><dd>Insurance clauses EVD-00481 and EVD-00507 only</dd></div>
              <div><dt>Thread Exposure</dt><dd>Manager assignment and cross-Agent handoff H-014</dd></div>
              <div><dt>Permission Grant</dt><dd>Read exact inputs; create draft Evidence and one task output</dd></div>
            </dl>
            <details className="qxp-technical-details"><summary>Permission ceiling and ungranted requests</summary><p>Ceiling also permits reading the wider registered package after Manager approval. No wider access was requested or granted for this Run. External communication and Secret data are prohibited.</p></details>
            <button className="qxp-secondary" type="button">Compare with prior Run</button>
          </div>
        ) : null}
        {panel === "activity" ? (
          <div className="qxp-activity-view">
            <ol><li><strong>Opened the two granted source passages</strong><span>10:06</span></li><li><strong>Verified the extracted wording against the originals</strong><span>10:09</span></li><li><strong>Sent attributable finding to the Manager handoff</strong><span>10:17</span></li></ol>
            <details className="qxp-technical-details"><summary>Technical activity</summary><dl><div><dt>10:06:12</dt><dd>read_registered_evidence · EVD-00481 · success</dd></div><div><dt>10:06:18</dt><dd>read_registered_evidence · EVD-00507 · success</dd></div><div><dt>10:17:03</dt><dd>record_agent_finding · FND-0019 · success</dd></div></dl></details>
          </div>
        ) : null}
        {panel === "outputs" ? (
          <div className="qxp-output-view">
            <article><FileText size={20} aria-hidden="true" /><div><strong>{copy.output}</strong><p>Current Artifact Version · attributable to this Agent Run</p><button className="qxp-reference" type="button">Open output and citations</button></div></article>
            <article><FileSearch size={20} aria-hidden="true" /><div><strong>Insurance conflict finding</strong><p>Links both exact Evidence references and handoff H-014.</p><button className="qxp-reference" type="button">Open Evidence</button></div></article>
          </div>
        ) : null}
        </section>
      ))}
    </FocusedView>
  );
}
