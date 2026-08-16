import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, ArrowRight } from "lucide-react";

import {
  insuranceTreatments,
  stageForTender,
  tenders,
  variants,
  type AgentKey,
  type AgentTab,
  type DecisionVersion,
  type JourneyStage,
  type Overlay,
  type PrototypeMessage,
  type RoomFilter,
  type VariantKey,
  type WorkspaceView,
} from "./TenderWorkspacePrototypeData";
import {
  AgentWorkroom,
  DecisionCorrection,
  EvidenceReview,
  PlanReview,
  TeamRoom,
  TenderChooser,
} from "./TenderWorkspacePrototypeOverlays";
import { Brand } from "./TenderWorkspacePrototypePrimitives";
import {
  ConversationCanvasVariant,
  ManagerBriefVariant,
  TenderShelfVariant,
  type WorkspaceVariantProps,
} from "./TenderWorkspacePrototypeShells";
import { WorkspaceContent } from "./TenderWorkspacePrototypeViews";
import "./TenderWorkspacePrototype.css";

// Three variants of the Manager-led Tender workspace, switchable with ?variant=A|B|C.
// PROTOTYPE ONLY: in-memory state, realistic fixtures, and no production Host mutations.

function readVariant(): VariantKey {
  const value = new URLSearchParams(window.location.search).get("variant")?.toUpperCase();
  return value === "B" || value === "C" ? value : "A";
}

function usePrototypeVariant() {
  const [variant, setVariantState] = useState<VariantKey>(readVariant);

  const setVariant = useCallback((next: VariantKey) => {
    const url = new URL(window.location.href);
    url.searchParams.set("variant", next);
    window.history.replaceState({}, "", url);
    setVariantState(next);
  }, []);

  const cycle = useCallback(
    (direction: -1 | 1) => {
      const index = variants.findIndex((item) => item.key === variant);
      const next = variants[(index + direction + variants.length) % variants.length];
      setVariant(next.key);
    },
    [setVariant, variant],
  );

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const target = event.target as HTMLElement | null;
      if (
        target?.closest(
          "input, textarea, select, [contenteditable='true'], [role='dialog'], [role='tablist']",
        ) ||
        (event.key !== "ArrowLeft" && event.key !== "ArrowRight")
      ) {
        return;
      }
      event.preventDefault();
      cycle(event.key === "ArrowLeft" ? -1 : 1);
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [cycle]);

  return { variant, cycle };
}

function Startup() {
  return (
    <div className="qxp-startup" role="status" aria-live="polite">
      <Brand />
      <p>Opening your Tender workspace...</p>
    </div>
  );
}

function PrototypeSwitcher({
  variant,
  stage,
  selectedTenderName,
  onCycle,
  onRestart,
  onShowSlowStartup,
}: {
  variant: VariantKey;
  stage: JourneyStage;
  selectedTenderName: string | null;
  onCycle: (direction: -1 | 1) => void;
  onRestart: () => void;
  onShowSlowStartup: () => void;
}) {
  if (!import.meta.env.DEV) return null;
  const item = variants.find((candidate) => candidate.key === variant) ?? variants[0];
  return (
    <aside className="qxp-switcher" aria-label="Prototype controls">
      <span className="qxp-switcher__note">Prototype only</span>
      <button type="button" aria-label="Previous prototype variant" onClick={() => onCycle(-1)}>
        <ArrowLeft size={18} aria-hidden="true" />
      </button>
      <div aria-live="polite">
        <strong>{item.key} · {item.name}</strong>
        <span>{selectedTenderName ? `${selectedTenderName} · ${stage}` : `No Tender · ${stage}`}</span>
      </div>
      <button type="button" aria-label="Next prototype variant" onClick={() => onCycle(1)}>
        <ArrowRight size={18} aria-hidden="true" />
      </button>
      <button className="qxp-switcher__utility" type="button" onClick={onShowSlowStartup}>
        Show slow startup
      </button>
      <button className="qxp-switcher__utility" type="button" onClick={onRestart}>
        Restart scenario
      </button>
    </aside>
  );
}

function initialDecision(treatment = insuranceTreatments[0]): DecisionVersion {
  return {
    version: 1,
    treatment,
    reason: "Initial treatment recorded with the Manager's intake decision.",
  };
}

export function TenderWorkspacePrototype() {
  const { variant, cycle } = usePrototypeVariant();
  const [slowStartup, setSlowStartup] = useState(false);
  const [selectedTenderId, setSelectedTenderId] = useState<string | null>(null);
  const [stage, setStage] = useState<JourneyStage>("empty");
  const [view, setView] = useState<WorkspaceView>("manager");
  const [overlay, setOverlay] = useState<Overlay>(null);
  const [answer, setAnswer] = useState("");
  const [messages, setMessages] = useState<PrototypeMessage[]>([]);
  const [planVersion, setPlanVersion] = useState(3);
  const [decisionVersions, setDecisionVersions] = useState<DecisionVersion[]>([]);
  const [evidenceReturnLabel, setEvidenceReturnLabel] = useState("Return to the Manager");
  const [agent, setAgent] = useState<AgentKey>("requirements");
  const [agentTab, setAgentTab] = useState<AgentTab>("conversation");
  const [agentReturnOverlay, setAgentReturnOverlay] = useState<"team" | null>(null);
  const [roomFilter, setRoomFilter] = useState<RoomFilter>("all");
  const intakeTimer = useRef<number | null>(null);
  const startupTimer = useRef<number | null>(null);
  const teamReturnFocus = useRef<HTMLElement | null>(null);

  const selectedTender = useMemo(
    () => tenders.find((tender) => tender.id === selectedTenderId) ?? null,
    [selectedTenderId],
  );

  useEffect(
    () => () => {
      if (intakeTimer.current !== null) window.clearTimeout(intakeTimer.current);
      if (startupTimer.current !== null) window.clearTimeout(startupTimer.current);
    },
    [],
  );

  function clearIntakeTimer() {
    if (intakeTimer.current !== null) {
      window.clearTimeout(intakeTimer.current);
      intakeTimer.current = null;
    }
  }

  function resetTenderSurface(tenderId: string | null, nextStage: JourneyStage) {
    clearIntakeTimer();
    setSelectedTenderId(tenderId);
    setStage(nextStage);
    setView("manager");
    setOverlay(null);
    setAnswer("");
    setMessages([]);
    setPlanVersion(3);
    setDecisionVersions(nextStage === "working" || nextStage === "plan" ? [initialDecision()] : []);
  }

  function selectTender(tenderId: string) {
    resetTenderSurface(tenderId, stageForTender[tenderId] ?? "question");
  }

  function startTender() {
    resetTenderSurface("north-coast", "intake");
    intakeTimer.current = window.setTimeout(() => {
      setStage("question");
      intakeTimer.current = null;
    }, 2400);
  }

  function restart() {
    resetTenderSurface(null, "empty");
    setAgent("requirements");
    setAgentTab("conversation");
    setRoomFilter("all");
  }

  function showSlowStartup() {
    if (startupTimer.current !== null) window.clearTimeout(startupTimer.current);
    setSlowStartup(true);
    startupTimer.current = window.setTimeout(() => {
      setSlowStartup(false);
      startupTimer.current = null;
    }, 1600);
  }

  function sendMessage(body: string) {
    const id = Date.now();
    setMessages((current) => [
      ...current,
      { id, author: "engineer", body },
      {
        id: id + 1,
        author: "manager",
        body: "Understood. I will keep that with this Tender and tell you before it changes the approved plan or a formal decision.",
      },
    ]);
  }

  function answerManager() {
    setDecisionVersions([initialDecision(answer)]);
    setStage("plan");
  }

  function openEvidence(returnLabel: string) {
    setEvidenceReturnLabel(returnLabel);
    setOverlay("evidence");
  }

  function openTeam() {
    teamReturnFocus.current = document.activeElement as HTMLElement | null;
    setOverlay("team");
  }

  function openAgent(nextAgent: AgentKey, nextTab: AgentTab) {
    setAgent(nextAgent);
    setAgentTab(nextTab);
    setAgentReturnOverlay(null);
    setOverlay("agent");
  }

  function openTeamAgent(nextAgent: AgentKey) {
    setAgent(nextAgent);
    setAgentTab("conversation");
    setAgentReturnOverlay("team");
    setOverlay("agent");
  }

  function recordCorrection(version: DecisionVersion) {
    setDecisionVersions((current) => [...current, version]);
    setAnswer(version.treatment);
    setMessages((current) => [
      ...current,
      {
        id: Date.now(),
        author: "manager",
        body: `Decision D-009 v${version.version} is now current. I paused the affected estimate and commercial response; unrelated work continues.`,
      },
    ]);
    setOverlay(null);
  }

  const content = (
    <WorkspaceContent
      selectedTender={selectedTender}
      stage={stage}
      view={view}
      answer={answer}
      messages={messages}
      planVersion={planVersion}
      decisionVersions={decisionVersions}
      onChoose={startTender}
      onRecent={() => selectTender("north-coast")}
      onAnswerChange={setAnswer}
      onAnswer={answerManager}
      onOpenEvidence={openEvidence}
      onOpenPlan={() => setOverlay("plan")}
      onOpenWork={() => setView("work")}
      onOpenTeam={openTeam}
      onOpenAgent={openAgent}
      onCorrectDecision={() => setOverlay("correction")}
      onSendMessage={sendMessage}
    />
  );

  const variantProps: WorkspaceVariantProps = {
    selectedTender,
    selectedTenderId,
    view,
    content,
    onNavigate: setView,
    onOpenTenders: () => setOverlay("tenders"),
    onOpenTeam: openTeam,
    onSelectTender: selectTender,
    onStartTender: startTender,
  };

  return (
    <div className="qxp-prototype">
      {slowStartup ? <Startup /> : null}
      <div
        className="qxp-prototype__workspace"
        aria-hidden={slowStartup || overlay !== null ? true : undefined}
        inert={slowStartup || overlay !== null ? true : undefined}
      >
        {variant === "A" ? <TenderShelfVariant {...variantProps} /> : null}
        {variant === "B" ? <ConversationCanvasVariant {...variantProps} /> : null}
        {variant === "C" ? <ManagerBriefVariant {...variantProps} /> : null}
      </div>

      {overlay === "tenders" ? (
        <TenderChooser
          key="tenders"
          selectedTenderId={selectedTenderId}
          onSelect={selectTender}
          onStart={startTender}
          onClose={() => setOverlay(null)}
        />
      ) : null}
      {overlay === "evidence" ? (
        <EvidenceReview
          key="evidence"
          returnLabel={evidenceReturnLabel}
          onClose={() => setOverlay(null)}
        />
      ) : null}
      {overlay === "plan" ? (
        <PlanReview
          key="plan"
          planVersion={planVersion}
          deadline={selectedTender?.deadline ?? "No deadline recorded"}
          onChangeRequested={() => setPlanVersion(4)}
          onApprove={() => {
            setStage("working");
            setOverlay(null);
          }}
          onClose={() => setOverlay(null)}
        />
      ) : null}
      {overlay === "correction" ? (
        <DecisionCorrection
          key="correction"
          versions={decisionVersions}
          onSave={recordCorrection}
          onClose={() => setOverlay(null)}
        />
      ) : null}
      {overlay === "team" ? (
        <TeamRoom
          key="team"
          filter={roomFilter}
          onFilter={setRoomFilter}
          onOpenAgent={openTeamAgent}
          onClose={() => setOverlay(null)}
          returnFocusTarget={teamReturnFocus.current}
        />
      ) : null}
      {overlay === "agent" ? (
        <AgentWorkroom
          key="agent"
          agent={agent}
          tab={agentTab}
          onAgentChange={setAgent}
          onTabChange={setAgentTab}
          onBack={() => setOverlay(agentReturnOverlay)}
          onClose={() => setOverlay(null)}
          planVersion={planVersion}
          returnFocusTarget={
            agentReturnOverlay === "team" ? teamReturnFocus.current : undefined
          }
        />
      ) : null}

      {overlay === null && !slowStartup ? (
        <PrototypeSwitcher
          variant={variant}
          stage={stage}
          selectedTenderName={selectedTender?.name ?? null}
          onCycle={cycle}
          onRestart={restart}
          onShowSlowStartup={showSlowStartup}
        />
      ) : null}
    </div>
  );
}
