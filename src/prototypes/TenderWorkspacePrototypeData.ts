export type VariantKey = "A" | "B" | "C";
export type WorkspaceView = "manager" | "work" | "files";
export type JourneyStage = "empty" | "intake" | "question" | "plan" | "working";
export type Overlay = "tenders" | "plan" | "team" | "agent" | "evidence" | "correction" | null;
export type AgentTab = "conversation" | "context" | "activity" | "outputs";
export type AgentKey = "requirements" | "commercial";
export type RoomFilter = "all" | "needs-you" | "handoffs" | "outputs";

export interface TenderSummary {
  id: string;
  name: string;
  phase: string;
  deadline: string;
  needsEngineer: boolean;
  availableInPrototype: boolean;
}

export interface PrototypeMessage {
  id: number;
  author: "engineer" | "manager";
  body: string;
}

export interface DecisionVersion {
  version: number;
  treatment: string;
  reason: string;
}

export const variants: Array<{ key: VariantKey; name: string }> = [
  { key: "A", name: "Tender shelf" },
  { key: "B", name: "Conversation canvas" },
  { key: "C", name: "Manager brief" },
];

export const tenders: TenderSummary[] = [
  {
    id: "north-coast",
    name: "North Coast Medical Campus",
    phase: "Plan preparation",
    deadline: "28 Aug, 12:00",
    needsEngineer: true,
    availableInPrototype: true,
  },
  {
    id: "east-harbour",
    name: "East Harbour Civic Centre",
    phase: "Estimating · scenario preview",
    deadline: "4 Sep, 15:00",
    needsEngineer: false,
    availableInPrototype: false,
  },
  {
    id: "wadi-solar",
    name: "Wadi Solar Expansion",
    phase: "Plan review · scenario preview",
    deadline: "11 Sep, 13:00",
    needsEngineer: true,
    availableInPrototype: false,
  },
];

export const stageForTender: Record<string, JourneyStage> = {
  "north-coast": "question",
};

export const insuranceTreatments = [
  "Carry the insurance in our main contract price",
  "Qualify it pending the Client's clarification",
];

export const agentCopy: Record<
  AgentKey,
  { role: string; initials: string; objective: string; output: string }
> = {
  requirements: {
    role: "Requirements Analyst",
    initials: "RA",
    objective: "Resolve the insurance conflict and complete the compliance register.",
    output: "Compliance register v2",
  },
  commercial: {
    role: "Commercial Reviewer",
    initials: "CR",
    objective:
      "Assess contract departures and confirm the commercial treatment of Clause 9.4.",
    output: "Commercial risk note v1",
  },
};

export const roomMessages = [
  {
    kind: "handoffs" as const,
    agent: "manager" as const,
    label: "Handoff",
    body: "Requirements and Commercial: compare the two insurance clauses and return one treatment recommendation.",
    ref: "Task T-014 · Handoff H-014",
  },
  {
    kind: "needs-you" as const,
    agent: "commercial" as const,
    label: "Blocker",
    body: "The Cost Estimate is paused until the current insurance treatment is confirmed.",
    ref: "Task T-021 · Blocker B-006",
  },
  {
    kind: "outputs" as const,
    agent: "requirements" as const,
    label: "Finding",
    body: "Clause 9.4 makes the main contractor responsible. I linked the exact source passage.",
    ref: "Evidence EVD-00481",
  },
  {
    kind: "outputs" as const,
    agent: "commercial" as const,
    label: "Finding",
    body: "The pricing schedule excludes the same cover. Pricing plus a clarification protects compliance and exposure.",
    ref: "Evidence EVD-00507",
  },
  {
    kind: "needs-you" as const,
    agent: "manager" as const,
    label: "Needs you",
    body: "The combined recommendation is ready for the Tendering Engineer.",
    ref: "Decision D-009",
  },
];
