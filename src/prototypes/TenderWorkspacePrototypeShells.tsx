import type { ReactNode } from "react";
import { useState } from "react";
import {
  Bot,
  BriefcaseBusiness,
  ChevronDown,
  Ellipsis,
  Folder,
  ListTodo,
  PanelLeft,
  Users,
} from "lucide-react";

import {
  tenders,
  type TenderSummary,
  type WorkspaceView,
} from "./TenderWorkspacePrototypeData";
import { Brand } from "./TenderWorkspacePrototypePrimitives";

function TenderButton({
  tender,
  selected,
  onClick,
}: {
  tender: TenderSummary;
  selected: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className="qxp-tender-button"
      type="button"
      aria-current={selected ? "page" : undefined}
      disabled={!tender.availableInPrototype}
      title={!tender.availableInPrototype ? "Preview row — not part of this usability scenario" : undefined}
      onClick={onClick}
    >
      <span className="qxp-tender-button__copy">
        <strong>{tender.name}</strong>
        <small>{tender.phase}</small>
      </span>
      {tender.needsEngineer ? <span className="qxp-needs-label">Needs you</span> : null}
    </button>
  );
}

export function TenderList({
  selectedTenderId,
  onSelect,
  onStart,
}: {
  selectedTenderId: string | null;
  onSelect: (tenderId: string) => void;
  onStart: () => void;
}) {
  return (
    <div className="qxp-tender-list">
      <div className="qxp-tender-list__heading">
        <h2>Your Tenders</h2>
        <button type="button" onClick={onStart}>
          Start a Tender
        </button>
      </div>
      <nav aria-label="Your Tenders">
        {tenders.map((tender) => (
          <div
            className={selectedTenderId === tender.id ? "is-selected" : ""}
            key={tender.id}
          >
            <TenderButton
              tender={tender}
              selected={selectedTenderId === tender.id}
              onClick={() => onSelect(tender.id)}
            />
          </div>
        ))}
      </nav>
    </div>
  );
}

const workspaceNavigation: Array<{
  view: WorkspaceView;
  label: string;
  Icon: typeof Bot;
}> = [
  { view: "manager", label: "Manager", Icon: Bot },
  { view: "work", label: "Work", Icon: ListTodo },
  { view: "files", label: "Files", Icon: Folder },
];

function WorkspaceNavigation({
  view,
  onNavigate,
}: {
  view: WorkspaceView;
  onNavigate: (view: WorkspaceView) => void;
}) {
  return (
    <nav className="qxp-workspace-nav" aria-label="Tender workspace">
      {workspaceNavigation.map(({ view: destination, label, Icon }) => (
        <button
          className={view === destination ? "is-active" : ""}
          key={destination}
          type="button"
          aria-current={view === destination ? "page" : undefined}
          onClick={() => onNavigate(destination)}
        >
          <Icon size={18} aria-hidden="true" />
          <span>{label}</span>
        </button>
      ))}
    </nav>
  );
}

function TenderHeading({
  tender,
  onOpenTeam,
  onOpenTenders,
  showTenderTrigger = false,
}: {
  tender: TenderSummary | null;
  onOpenTeam: () => void;
  onOpenTenders: () => void;
  showTenderTrigger?: boolean;
}) {
  return (
    <div className="qxp-tender-heading">
      <div>
        {showTenderTrigger ? (
          <button className="qxp-tender-switch" type="button" onClick={onOpenTenders}>
            <BriefcaseBusiness size={18} aria-hidden="true" />
            {tender ? tender.name : "Choose a Tender"}
            <ChevronDown size={18} aria-hidden="true" />
          </button>
        ) : (
          <h1>{tender ? tender.name : "Tender workspace"}</h1>
        )}
        {tender ? <p>Submission deadline {tender.deadline}</p> : null}
      </div>
      {tender ? (
        <div className="qxp-tender-heading__actions">
          <button type="button" onClick={onOpenTeam}>
            <Users size={18} aria-hidden="true" />
            Team working
          </button>
          <button type="button" aria-label="Open Tender menu">
            <Ellipsis size={18} aria-hidden="true" />
            More
          </button>
        </div>
      ) : null}
    </div>
  );
}

export interface WorkspaceVariantProps {
  selectedTender: TenderSummary | null;
  selectedTenderId: string | null;
  view: WorkspaceView;
  content: ReactNode;
  onNavigate: (view: WorkspaceView) => void;
  onOpenTenders: () => void;
  onOpenTeam: () => void;
  onSelectTender: (tenderId: string) => void;
  onStartTender: () => void;
}

export function TenderShelfVariant(props: WorkspaceVariantProps) {
  const [shelfOpen, setShelfOpen] = useState(() =>
    window.matchMedia("(min-width: 821px)").matches,
  );
  return (
    <div className={`qxp-shell qxp-variant-a${shelfOpen ? "" : " is-shelf-collapsed"}`}>
      <header className="qxp-a-header">
        <button
          className="qxp-shelf-toggle"
          type="button"
          aria-expanded={shelfOpen}
          onClick={() => setShelfOpen((current) => !current)}
        >
          <PanelLeft size={18} aria-hidden="true" />
          <span>{shelfOpen ? "Hide Tenders" : "Show Tenders"}</span>
        </button>
        <Brand />
      </header>
      <aside
        className="qxp-a-shelf"
        aria-hidden={!shelfOpen ? true : undefined}
        inert={!shelfOpen ? true : undefined}
      >
        {shelfOpen ? (
          <TenderList
            selectedTenderId={props.selectedTenderId}
            onSelect={props.onSelectTender}
            onStart={props.onStartTender}
          />
        ) : null}
      </aside>
      <div className="qxp-a-workspace">
        <TenderHeading
          tender={props.selectedTender}
          onOpenTeam={props.onOpenTeam}
          onOpenTenders={props.onOpenTenders}
        />
        {props.selectedTender ? (
          <WorkspaceNavigation view={props.view} onNavigate={props.onNavigate} />
        ) : null}
        <main className="qxp-content">{props.content}</main>
      </div>
    </div>
  );
}

export function ConversationCanvasVariant(props: WorkspaceVariantProps) {
  return (
    <div className="qxp-shell qxp-variant-b">
      <header className="qxp-b-header">
        <Brand />
        <button className="qxp-b-tender" type="button" onClick={props.onOpenTenders}>
          <BriefcaseBusiness size={18} aria-hidden="true" />
          <span>{props.selectedTender?.name ?? "No Tender selected"}</span>
          <ChevronDown size={18} aria-hidden="true" />
        </button>
        {props.selectedTender ? (
          <button className="qxp-team-button" type="button" onClick={props.onOpenTeam}>
            <Users size={18} aria-hidden="true" />
            Team working
          </button>
        ) : (
          <span />
        )}
      </header>
      {props.selectedTender ? (
        <div className="qxp-b-navigation">
          <WorkspaceNavigation view={props.view} onNavigate={props.onNavigate} />
          <span>Due {props.selectedTender.deadline}</span>
        </div>
      ) : null}
      <main className="qxp-content qxp-content--canvas">{props.content}</main>
    </div>
  );
}

export function ManagerBriefVariant(props: WorkspaceVariantProps) {
  return (
    <div className="qxp-shell qxp-variant-c">
      <header className="qxp-c-header">
        <Brand />
      </header>
      <div className="qxp-c-frame">
        <TenderHeading
          tender={props.selectedTender}
          onOpenTeam={props.onOpenTeam}
          onOpenTenders={props.onOpenTenders}
          showTenderTrigger
        />
        {props.selectedTender ? (
          <WorkspaceNavigation view={props.view} onNavigate={props.onNavigate} />
        ) : null}
        <main className="qxp-content qxp-content--brief">{props.content}</main>
      </div>
    </div>
  );
}
