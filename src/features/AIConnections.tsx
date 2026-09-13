import { AISetup } from "./AISetup";

export function AIConnections({
  connectionId,
  onConnectionClose,
}: { connectionId?: string; onConnectionClose?: () => void } = {}) {
  return (
    <section
      className="ai-connections"
      aria-labelledby="ai-connections-heading"
    >
      <AISetup
        headingId="ai-connections-heading"
        connectionId={connectionId}
        onConnectionClose={onConnectionClose}
      />
    </section>
  );
}
