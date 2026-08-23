import { FileStack, PanelRightClose, Users } from "lucide-react";
import type { ReactNode } from "react";
import {
  Dialog as AriaDialog,
  Modal as AriaModal,
  ModalOverlay as AriaModalOverlay,
} from "react-aria-components";

import type { ManagerWorkspaceProjection } from "./bindings/ManagerWorkspaceProjection";
import "./WorkspaceContextPanel.css";

export type WorkspaceContextPresentation = "rail" | "drawer";

export interface WorkspaceContextPanelProps {
  projection: ManagerWorkspaceProjection;
  phase: string;
  tools: ReactNode;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  presentation: WorkspaceContextPresentation;
}

interface WorkspaceContextContentProps extends Omit<
  WorkspaceContextPanelProps,
  "isOpen" | "presentation"
> {
  titleId: string;
}

function WorkspaceContextContent({
  projection,
  phase,
  tools,
  onOpenChange,
  titleId,
}: WorkspaceContextContentProps) {
  const selected = projection.selected_tender;
  if (!selected) return null;

  return (
    <>
      <header className="workspace-context__header">
        <div>
          <span>Workspace</span>
          <h2 id={titleId}>{selected.name}</h2>
          <p>{phase}</p>
        </div>
        <button
          type="button"
          aria-label="Close Tender workspace"
          onClick={() => onOpenChange(false)}
        >
          <PanelRightClose size={18} aria-hidden="true" />
        </button>
      </header>

      <div className="workspace-context__body">
        <section className="workspace-context__tools">{tools}</section>
        <section aria-labelledby="context-team">
          <div className="workspace-context__section-heading">
            <Users size={16} aria-hidden="true" />
            <h3 id="context-team">Team activity</h3>
          </div>
          <dl className="workspace-context__metrics">
            <div>
              <dt>Working</dt>
              <dd>{projection.team.active_agent_runs}</dd>
            </div>
            <div>
              <dt>Waiting</dt>
              <dd>{projection.team.waiting_tasks}</dd>
            </div>
            <div>
              <dt>Needs you</dt>
              <dd>{projection.team.needs_engineer}</dd>
            </div>
          </dl>
        </section>

        <section aria-labelledby="context-records">
          <div className="workspace-context__section-heading">
            <FileStack size={16} aria-hidden="true" />
            <h3 id="context-records">Tender records</h3>
          </div>
          <dl className="workspace-context__rows">
            <div>
              <dt>Tender documents</dt>
              <dd>{projection.files.tender_document_count}</dd>
            </div>
            <div>
              <dt>Quantix outputs</dt>
              <dd>{projection.files.quantix_output_count}</dd>
            </div>
            <div>
              <dt>Open work</dt>
              <dd>
                {projection.work.needs_engineer +
                  projection.work.working +
                  projection.work.waiting}
              </dd>
            </div>
          </dl>
        </section>
      </div>
    </>
  );
}

interface WorkspaceContextFrameProps {
  children: ReactNode;
  className: string;
}

function WorkspaceContextFrame({
  children,
  className,
}: WorkspaceContextFrameProps) {
  return (
    <aside
      id="tender-workspace-panel"
      className={className}
      aria-label="Tender workspace"
    >
      {children}
    </aside>
  );
}

export function WorkspaceContextPanel({
  projection,
  phase,
  tools,
  isOpen,
  onOpenChange,
  presentation,
}: WorkspaceContextPanelProps) {
  if (!isOpen || !projection.selected_tender) return null;

  const titleId =
    presentation === "drawer"
      ? "workspace-context-drawer-title"
      : "workspace-context-rail-title";
  const content = (
    <WorkspaceContextContent
      projection={projection}
      phase={phase}
      tools={tools}
      onOpenChange={onOpenChange}
      titleId={titleId}
    />
  );

  if (presentation === "rail") {
    return (
      <WorkspaceContextFrame className="workspace-context workspace-context--rail">
        {content}
      </WorkspaceContextFrame>
    );
  }

  return (
    <AriaModalOverlay
      className="workspace-context__overlay"
      isOpen={isOpen}
      isDismissable
      isKeyboardDismissDisabled={false}
      onOpenChange={onOpenChange}
    >
      <AriaModal className="workspace-context__modal">
        <AriaDialog
          id="tender-workspace-panel"
          className="workspace-context workspace-context--drawer"
          aria-label="Tender workspace"
        >
          {content}
        </AriaDialog>
      </AriaModal>
    </AriaModalOverlay>
  );
}
