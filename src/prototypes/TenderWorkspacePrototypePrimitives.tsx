import { agentCopy, type AgentKey } from "./TenderWorkspacePrototypeData";

export function Avatar({
  agent,
  subtle = false,
}: {
  agent: AgentKey | "manager";
  subtle?: boolean;
}) {
  const initials = agent === "manager" ? "TM" : agentCopy[agent].initials;
  return (
    <span className={`qxp-avatar${subtle ? " qxp-avatar--subtle" : ""}`} aria-hidden="true">
      {initials}
    </span>
  );
}

export function Brand() {
  return <span className="qxp-brand">Quantix</span>;
}
