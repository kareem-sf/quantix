import { type KeyboardEvent as ReactKeyboardEvent, useState } from "react";
import {
  BriefcaseBusiness,
  FileSearch,
  FileText,
  Folder,
  Paperclip,
  Send,
} from "lucide-react";

import {
  insuranceTreatments,
  type AgentKey,
  type AgentTab,
  type DecisionVersion,
  type JourneyStage,
  type PrototypeMessage,
  type TenderSummary,
  type WorkspaceView,
} from "./TenderWorkspacePrototypeData";
import { Avatar } from "./TenderWorkspacePrototypePrimitives";

function EmptyWorkspace({ onChoose, onRecent }: { onChoose: () => void; onRecent: () => void }) {
  return (
    <section className="qxp-empty" aria-labelledby="empty-title">
      <span className="qxp-empty__mark" aria-hidden="true">
        <BriefcaseBusiness size={22} />
      </span>
      <h2 id="empty-title">Start a Tender</h2>
      <p>
        Choose the Tender package you received. Your Tendering Manager will review it and ask only
        what is needed.
      </p>
      <button className="qxp-primary" type="button" onClick={onChoose}>
        Choose Tender package
      </button>
      <button className="qxp-link" type="button" onClick={onRecent}>
        Open a recent Tender
      </button>
    </section>
  );
}

function ManagerIdentity({ status }: { status: string }) {
  return (
    <div className="qxp-manager-identity">
      <Avatar agent="manager" />
      <div>
        <strong>Tendering Manager</strong>
        <span>{status}</span>
      </div>
    </div>
  );
}

function Composer({
  value,
  onChange,
  onSend,
}: {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
}) {
  function onKeyDown(event: ReactKeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      onSend();
    }
  }

  return (
    <div className="qxp-composer">
      <button type="button" aria-label="Attach a file">
        <Paperclip size={18} aria-hidden="true" />
      </button>
      <textarea
        rows={1}
        aria-label="Message your Tendering Manager"
        placeholder="Message your Tendering Manager..."
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={onKeyDown}
      />
      <button
        className="qxp-composer__send"
        type="button"
        aria-label="Send message"
        disabled={!value.trim()}
        onClick={onSend}
      >
        <Send size={18} aria-hidden="true" />
      </button>
    </div>
  );
}

function IntakeStatus() {
  return (
    <section className="qxp-manager-flow" aria-live="polite">
      <ManagerIdentity status="Reviewing your Tender package" />
      <div className="qxp-manager-message">
        <p>
          I am preserving the package structure and checking the submission instructions first. You
          can switch Tenders while I continue.
        </p>
      </div>
      <div className="qxp-quiet-status">
        <span className="qxp-working-mark" aria-hidden="true" />
        <span>
          <strong>Reviewing 18 files</strong>
          <small>Reading submission instructions</small>
        </span>
      </div>
    </section>
  );
}

function QuestionAction({
  answer,
  onAnswerChange,
  onSubmit,
  onOpenEvidence,
}: {
  answer: string;
  onAnswerChange: (answer: string) => void;
  onSubmit: () => void;
  onOpenEvidence: () => void;
}) {
  return (
    <section className="qxp-current-action" aria-labelledby="decision-title">
      <p className="qxp-action-label">Needs your decision</p>
      <h3 id="decision-title">How should we treat the six-year latent-defect insurance?</h3>
      <p>
        The Employer's Requirements place it with the main contractor, while the pricing schedule
        excludes it. I need your treatment before I prepare the Work Plan.
      </p>
      <button className="qxp-source-link" type="button" onClick={onOpenEvidence}>
        <FileSearch size={18} aria-hidden="true" />
        Review the two source clauses
      </button>
      <fieldset className="qxp-choice-list">
        <legend className="qxp-sr-only">Insurance treatment</legend>
        {insuranceTreatments.map((option) => (
          <label className={answer === option ? "is-selected" : ""} key={option}>
            <input
              type="radio"
              name="insurance-treatment"
              checked={answer === option}
              onChange={() => onAnswerChange(option)}
            />
            <span>{option}</span>
          </label>
        ))}
      </fieldset>
      <div className="qxp-action-footer">
        <button className="qxp-primary" type="button" disabled={!answer} onClick={onSubmit}>
          Answer Manager
        </button>
        <span>Two later decisions are expected.</span>
      </div>
    </section>
  );
}

function PlanSummary({ planVersion, onReview }: { planVersion: number; onReview: () => void }) {
  return (
    <section className="qxp-current-action" aria-labelledby="plan-summary-title">
      <p className="qxp-action-label">Work Plan v{planVersion} is ready</p>
      <h3 id="plan-summary-title">A controlled seven-day review leading to a priced submission</h3>
      <p>
        I have assigned four specialists. Requirements and commercial risk go first; estimating
        starts after the scope conflict is resolved; an independent reviewer checks the package.
      </p>
      <ol className="qxp-plan-steps">
        <li>Build the compliance and clarification baseline.</li>
        <li>Prepare the controlled estimate and commercial strategy.</li>
        <li>Produce and independently review the Submission Package.</li>
      </ol>
      <div className="qxp-plan-facts">
        <span>
          <small>First review</small>
          Tomorrow, 10:00
        </span>
        <span>
          <small>Your decisions</small>2 expected
        </span>
      </div>
      <div className="qxp-action-footer">
        <button className="qxp-primary" type="button" onClick={onReview}>
          Review full Work Plan
        </button>
        <span>Formal approval happens in the exact plan review.</span>
      </div>
    </section>
  );
}

function WorkingSummary({
  decisionVersions,
  onOpenEvidence,
  onOpenWork,
  onCorrectDecision,
}: {
  decisionVersions: DecisionVersion[];
  onOpenEvidence: () => void;
  onOpenWork: () => void;
  onCorrectDecision: () => void;
}) {
  const currentDecision = decisionVersions[decisionVersions.length - 1];
  return (
    <section className="qxp-current-action" aria-labelledby="finding-title">
      <p className="qxp-action-label">
        {decisionVersions.length > 1 ? "Corrected decision is current" : "Finding ready for you"}
      </p>
      <h3 id="finding-title">The insurance conflict affects both price and qualification wording</h3>
      <p>
        Requirements and Commercial agree that the safest treatment is to price the cover and raise
        one clarification. No other work needs to stop.
      </p>
      {currentDecision ? (
        <div className="qxp-decision-current" role="status">
          <strong>Decision D-009 v{currentDecision.version}</strong>
          <span>{currentDecision.treatment}</span>
          {decisionVersions.length > 1 ? <small>v1 remains preserved in History.</small> : null}
        </div>
      ) : null}
      <button className="qxp-source-link" type="button" onClick={onOpenEvidence}>
        <FileSearch size={18} aria-hidden="true" />
        Review finding and Evidence
      </button>
      <div className="qxp-action-footer">
        <button className="qxp-primary" type="button" onClick={onOpenEvidence}>
          Review recommendation
        </button>
        <div className="qxp-inline-actions">
          <button className="qxp-link" type="button" onClick={onCorrectDecision}>
            Correct this decision
          </button>
          <button className="qxp-link" type="button" onClick={onOpenWork}>
            See affected Work
          </button>
        </div>
      </div>
    </section>
  );
}

function ManagerView({
  stage,
  answer,
  messages,
  planVersion,
  decisionVersions,
  onAnswerChange,
  onAnswer,
  onOpenEvidence,
  onOpenPlan,
  onOpenWork,
  onOpenTeam,
  onCorrectDecision,
  onSendMessage,
}: {
  stage: JourneyStage;
  answer: string;
  messages: PrototypeMessage[];
  planVersion: number;
  decisionVersions: DecisionVersion[];
  onAnswerChange: (answer: string) => void;
  onAnswer: () => void;
  onOpenEvidence: (returnLabel: string) => void;
  onOpenPlan: () => void;
  onOpenWork: () => void;
  onOpenTeam: () => void;
  onCorrectDecision: () => void;
  onSendMessage: (message: string) => void;
}) {
  const [composer, setComposer] = useState("");

  function send() {
    const value = composer.trim();
    if (!value) return;
    onSendMessage(value);
    setComposer("");
  }

  if (stage === "intake") return <IntakeStatus />;

  const managerStatus = stage === "working" ? "Coordinating approved work" : "Waiting for you";
  const introduction =
    stage === "question"
      ? "I found the submission deadline and reviewed the package structure. I need one answer before I prepare the Work Plan."
      : stage === "plan"
        ? "I have prepared the complete Work Plan and delegation. Review the exact plan when you are ready; nothing starts before your approval."
        : "The approved team is working. I have one material finding for you; unrelated work is continuing.";

  return (
    <section
      className="qxp-manager-flow"
      aria-label="Conversation with Tendering Manager"
      data-workspace-focus
      tabIndex={-1}
    >
      <ManagerIdentity status={managerStatus} />
      <article className="qxp-manager-message">
        <p>{introduction}</p>
      </article>
      {stage === "question" ? (
        <QuestionAction
          answer={answer}
          onAnswerChange={onAnswerChange}
          onSubmit={onAnswer}
          onOpenEvidence={() => onOpenEvidence("Return to the Manager's question")}
        />
      ) : null}
      {stage === "plan" ? <PlanSummary planVersion={planVersion} onReview={onOpenPlan} /> : null}
      {stage === "working" ? (
        <WorkingSummary
          decisionVersions={decisionVersions}
          onOpenEvidence={() => onOpenEvidence("Return to the Manager's finding")}
          onOpenWork={onOpenWork}
          onCorrectDecision={onCorrectDecision}
        />
      ) : null}
      {messages.map((message) => (
        <article className={`qxp-chat-message qxp-chat-message--${message.author}`} key={message.id}>
          <strong>{message.author === "engineer" ? "You" : "Tendering Manager"}</strong>
          <p>{message.body}</p>
        </article>
      ))}
      {stage === "working" ? (
        <button className="qxp-team-status" type="button" onClick={onOpenTeam}>
          <span>3 specialists working</span>
          <span>1 finding needs you</span>
          <strong>Open Team working</strong>
        </button>
      ) : null}
      <Composer value={composer} onChange={setComposer} onSend={send} />
    </section>
  );
}

function WorkView({
  onOpenEvidence,
  onOpenAgent,
}: {
  onOpenEvidence: (returnLabel: string) => void;
  onOpenAgent: (agent: AgentKey, tab: AgentTab) => void;
}) {
  return (
    <section className="qxp-list-page" aria-labelledby="work-title">
      <header>
        <h2 id="work-title">Work</h2>
        <p>Your Manager coordinates sequence and delegation. Open a task only when you need detail.</p>
      </header>
      <section className="qxp-work-group">
        <h3>Needs you <span>1</span></h3>
        <button className="qxp-work-row qxp-work-row--attention" type="button" onClick={() => onOpenEvidence("Return to Work")}>
          <span><strong>Confirm insurance treatment</strong><small>Requirements Analyst and Commercial Reviewer</small></span>
          <span>Needs you</span>
        </button>
      </section>
      <section className="qxp-work-group">
        <h3>Working <span>2</span></h3>
        <button className="qxp-work-row" type="button" onClick={() => onOpenAgent("requirements", "conversation")}>
          <span><strong>Complete compliance register</strong><small>Requirements Analyst · Reviewing 64 requirements</small></span>
          <span>Working</span>
        </button>
        <button className="qxp-work-row" type="button" onClick={() => onOpenAgent("commercial", "conversation")}>
          <span><strong>Review contract departures</strong><small>Commercial Reviewer · Comparing 12 clauses</small></span>
          <span>Working</span>
        </button>
      </section>
      <section className="qxp-work-group qxp-work-group--done">
        <h3>Done <span>1</span></h3>
        <div className="qxp-work-row">
          <span><strong>Register Tender Package</strong><small>Tendering Manager · 18 source files preserved</small></span>
          <span>Done</span>
        </div>
      </section>
    </section>
  );
}

function FilesView({
  onOpenEvidence,
  onOpenAgent,
}: {
  onOpenEvidence: (returnLabel: string) => void;
  onOpenAgent: (agent: AgentKey, tab: AgentTab) => void;
}) {
  return (
    <section className="qxp-list-page" aria-labelledby="files-title">
      <header>
        <h2 id="files-title">Files</h2>
        <p>Received sources stay separate from attributable Quantix work.</p>
      </header>
      <section className="qxp-file-group">
        <h3>Tender documents</h3>
        <button className="qxp-file-row" type="button" onClick={() => onOpenEvidence("Return to Files")}>
          <Folder size={18} aria-hidden="true" />
          <span><strong>01 Employer's Requirements</strong><small>12 source documents</small></span>
          <span>Open</span>
        </button>
        <button className="qxp-file-row" type="button" onClick={() => onOpenEvidence("Return to Files")}>
          <Folder size={18} aria-hidden="true" />
          <span><strong>02 Commercial</strong><small>6 source documents</small></span>
          <span>Open</span>
        </button>
      </section>
      <section className="qxp-file-group">
        <h3>Quantix work</h3>
        <button className="qxp-file-row" type="button" onClick={() => onOpenAgent("requirements", "outputs")}>
          <FileText size={18} aria-hidden="true" />
          <span><strong>Compliance register v2</strong><small>Requirements Analyst · current draft</small></span>
          <span>Review</span>
        </button>
        <button className="qxp-file-row" type="button" onClick={() => onOpenAgent("commercial", "outputs")}>
          <FileText size={18} aria-hidden="true" />
          <span><strong>Commercial risk note v1</strong><small>Commercial Reviewer · current draft</small></span>
          <span>Review</span>
        </button>
      </section>
    </section>
  );
}

export function WorkspaceContent({
  selectedTender,
  stage,
  view,
  answer,
  messages,
  planVersion,
  decisionVersions,
  onChoose,
  onRecent,
  onAnswerChange,
  onAnswer,
  onOpenEvidence,
  onOpenPlan,
  onOpenWork,
  onOpenTeam,
  onOpenAgent,
  onCorrectDecision,
  onSendMessage,
}: {
  selectedTender: TenderSummary | null;
  stage: JourneyStage;
  view: WorkspaceView;
  answer: string;
  messages: PrototypeMessage[];
  planVersion: number;
  decisionVersions: DecisionVersion[];
  onChoose: () => void;
  onRecent: () => void;
  onAnswerChange: (answer: string) => void;
  onAnswer: () => void;
  onOpenEvidence: (returnLabel: string) => void;
  onOpenPlan: () => void;
  onOpenWork: () => void;
  onOpenTeam: () => void;
  onOpenAgent: (agent: AgentKey, tab: AgentTab) => void;
  onCorrectDecision: () => void;
  onSendMessage: (message: string) => void;
}) {
  if (!selectedTender || stage === "empty") {
    return <EmptyWorkspace onChoose={onChoose} onRecent={onRecent} />;
  }
  if (view === "work") {
    return <WorkView onOpenEvidence={onOpenEvidence} onOpenAgent={onOpenAgent} />;
  }
  if (view === "files") {
    return <FilesView onOpenEvidence={onOpenEvidence} onOpenAgent={onOpenAgent} />;
  }
  return (
    <ManagerView
      stage={stage}
      answer={answer}
      messages={messages}
      planVersion={planVersion}
      decisionVersions={decisionVersions}
      onAnswerChange={onAnswerChange}
      onAnswer={onAnswer}
      onOpenEvidence={onOpenEvidence}
      onOpenPlan={onOpenPlan}
      onOpenWork={onOpenWork}
      onOpenTeam={onOpenTeam}
      onCorrectDecision={onCorrectDecision}
      onSendMessage={onSendMessage}
    />
  );
}
