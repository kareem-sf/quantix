import type {
  Fact,
  FactKind,
  FactState,
} from "@/components/beautiful/fact-list";
import type { RunActivity } from "../activity/types";

/** One AI turn: what it wrote and thought, and the real actions it took. */
export type WorkLogBlock = {
  id: string;
  startedAt: string;
  endedAt?: string;
  /** The AI's own note about what it is doing next. */
  note: string;
  thinking: string;
  thinkingDone: boolean;
  thinkingMs: number | null;
  facts: (Fact & {
    tool?: string;
    assignmentId?: string;
    open?: FactOpen;
    recoverable?: boolean;
  })[];
  /** The model request has finished (successfully or not). */
  settled: boolean;
  failed: boolean;
};

export type FactOpen = {
  artifact_id?: string;
  source_id?: string;
  page?: number;
};

export type StaffLog = {
  assignmentId: string;
  name: string;
  blocks: WorkLogBlock[];
};

export type WorkLog = {
  blocks: WorkLogBlock[];
  staff: Map<string, StaffLog>;
};

const FACT_KINDS = new Set<FactKind>([
  "read",
  "search",
  "view",
  "check",
  "save",
  "hire",
  "assign",
  "calculate",
  "propose",
  "other",
]);

function asText(value: unknown) {
  return typeof value === "string" ? value : undefined;
}

function asLines(value: unknown) {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : undefined;
}

/** Older runs recorded no fact; describe their tool by name, never by guesswork. */
function fallbackLine(tool: string, running: boolean) {
  const words = tool.replaceAll("_", " ").trim();
  const [first, ...rest] = words.split(" ");
  const verbs: Record<string, [string, string]> = {
    inspect: ["Checking", "Checked"],
    list: ["Checking", "Checked"],
    read: ["Reading", "Read"],
    search: ["Searching", "Searched"],
    view: ["Looking at", "Looked at"],
    check: ["Checking", "Checked"],
    save: ["Saving", "Saved"],
    hire: ["Hiring", "Hired"],
    assign: ["Handing out", "Handed out"],
    calculate: ["Calculating", "Calculated"],
  };
  const verb = verbs[first];
  const plain: Record<string, string> = {
    "extraction coverage": "which documents could be read",
    "package map": "package overview",
  };
  const object = plain[rest.join(" ")] ?? rest.join(" ");
  const phrase = verb ? `${verb[running ? 0 : 1]} ${object}` : words;
  return phrase.charAt(0).toUpperCase() + phrase.slice(1);
}

function kindFromTool(tool: string): FactKind {
  const prefix = tool.split("_")[0];
  const kinds: Record<string, FactKind> = {
    read: "read",
    search: "search",
    view: "view",
    hire: "hire",
    assign: "assign",
    save: "save",
    calculate: "calculate",
    propose: "propose",
  };
  return kinds[prefix] ?? "check";
}

const HIDDEN_TOOLS = new Set([
  "proposal_format",
  "quantix_submit_result",
  "search_tools",
]);

function factFrom(item: RunActivity, previous?: WorkLogBlock["facts"][number]) {
  const tool = item.tool ?? "";
  const raw = (item.fact ?? null) as Record<string, unknown> | null;
  const state: FactState = ["prepared", "started", "queued"].includes(
    item.phase,
  )
    ? "running"
    : item.phase === "completed" || item.phase === "observed"
      ? "done"
      : "failed";
  const kind = asText(raw?.kind) as FactKind | undefined;
  return {
    id: item.operation_id ?? `event:${item.event_id}`,
    tool,
    kind:
      kind && FACT_KINDS.has(kind)
        ? kind
        : (previous?.kind ?? kindFromTool(tool)),
    line: asText(raw?.line) ?? fallbackLine(tool, state === "running"),
    subject: asText(raw?.subject) ?? previous?.subject,
    result: asText(raw?.result),
    state: (asText(raw?.state) as FactState | undefined) ?? state,
    durationMs: state === "running" ? null : (item.elapsed_ms ?? null),
    found: asLines(raw?.found),
    details: asLines(raw?.details),
    open: (raw?.open as FactOpen | undefined) ?? undefined,
    assignmentId: asText(raw?.assignment_id),
    recoverable: raw?.recoverable === true,
  };
}

function newBlock(id: string, startedAt: string): WorkLogBlock {
  return {
    id,
    startedAt,
    note: "",
    thinking: "",
    thinkingDone: false,
    thinkingMs: null,
    facts: [],
    settled: false,
    failed: false,
  };
}

/**
 * Group a run's activity into turns. Model requests open a turn; thinking,
 * notes and tool calls attach to the request they happened under. Staff work
 * (events carrying an assignment) is grouped per assignment the same way.
 */
export function buildWorkLog(items: RunActivity[]): WorkLog {
  const staff = new Map<string, StaffLog>();
  const manager: WorkLogBlock[] = [];
  const byOperation = new Map<string, WorkLogBlock>();
  const factIndex = new Map<string, { block: WorkLogBlock; index: number }>();
  const thinkingStart = new Map<string, number>();

  const listFor = (item: RunActivity) => {
    if (!item.assignment_id) return manager;
    let log = staff.get(item.assignment_id);
    if (!log) {
      log = {
        assignmentId: item.assignment_id,
        name: item.actor_label,
        blocks: [],
      };
      staff.set(item.assignment_id, log);
    }
    return log.blocks;
  };
  const blockFor = (item: RunActivity) => {
    const key = item.parent_operation_id ?? "";
    const existing = key ? byOperation.get(key) : undefined;
    if (existing) return existing;
    const list = listFor(item);
    const last = list.at(-1);
    if (!key && last) return last;
    const block = newBlock(key || `event:${item.event_id}`, item.created_at);
    if (key) byOperation.set(key, block);
    list.push(block);
    return block;
  };

  for (const item of items) {
    const category = item.category;
    if (category === "model_request" && item.operation_id) {
      let block = byOperation.get(item.operation_id);
      if (!block) {
        block = newBlock(item.operation_id, item.created_at);
        byOperation.set(item.operation_id, block);
        listFor(item).push(block);
      }
      if (!["started", "prepared", "delta"].includes(item.phase)) {
        block.settled = true;
        block.endedAt = item.created_at;
        // A step cut short because the whole job stopped is not a failure of its
        // own; the stop is explained once, under the work.
        block.failed = item.phase === "failed";
      }
      continue;
    }
    if (
      category === "reasoning_summary" ||
      category === "reasoning" ||
      category === "note"
    ) {
      const block = blockFor(item);
      const text = item.preview ?? "";
      const field = category === "note" ? "note" : "thinking";
      if (item.phase === "completed") block[field] = text || block[field];
      else block[field] = `${block[field]}${text}`;
      if (field === "thinking") {
        if (!thinkingStart.has(block.id))
          thinkingStart.set(block.id, Date.parse(item.created_at));
        if (item.phase === "completed") {
          block.thinkingDone = true;
          block.thinkingMs =
            Date.parse(item.created_at) - (thinkingStart.get(block.id) ?? 0);
        }
      } else if (block.thinking && !block.thinkingDone) {
        // The AI started writing, so it has stopped thinking.
        block.thinkingDone = true;
        block.thinkingMs =
          Date.parse(item.created_at) - (thinkingStart.get(block.id) ?? 0);
      }
      continue;
    }
    if (category === "tool" && item.tool && !HIDDEN_TOOLS.has(item.tool)) {
      const key = item.operation_id ?? `event:${item.event_id}`;
      const known = factIndex.get(key);
      if (known) {
        known.block.facts[known.index] = factFrom(
          item,
          known.block.facts[known.index],
        );
      } else {
        const block = blockFor(item);
        if (block.thinking && !block.thinkingDone) {
          block.thinkingDone = true;
          block.thinkingMs =
            Date.parse(item.created_at) - (thinkingStart.get(block.id) ?? 0);
        }
        block.facts.push(factFrom(item));
        factIndex.set(key, { block, index: block.facts.length - 1 });
      }
    }
  }

  const tidy = (blocks: WorkLogBlock[]) =>
    blocks
      .map((block) => ({
        ...block,
        // A call the AI corrected on its next try is one step, not a failure.
        facts: block.facts.filter(
          (fact, index) =>
            !(
              fact.state === "failed" &&
              fact.recoverable &&
              blocks.some((other) =>
                other.facts.some(
                  (later, laterIndex) =>
                    later.tool === fact.tool &&
                    later.state !== "failed" &&
                    (other !== block || laterIndex > index),
                ),
              )
            ),
        ),
      }))
      .filter(
        (block) =>
          block.note || block.thinking || block.facts.length || !block.settled,
      );

  const tidied = new Map<string, StaffLog>();
  for (const [id, log] of staff)
    tidied.set(id, { ...log, blocks: tidy(log.blocks) });
  const blocks = tidy(manager);
  // Words the AI wrote with proper spacing anywhere in this job.
  const vocabulary = jobVocabulary([
    ...blocks,
    ...[...tidied.values()].flatMap((log) => log.blocks),
  ]);
  const repaired = (list: WorkLogBlock[]) =>
    list.map((block) =>
      block.note
        ? { ...block, note: repairGluedWords(block.note, vocabulary) }
        : block,
    );
  for (const [id, log] of tidied)
    tidied.set(id, { ...log, blocks: repaired(log.blocks) });
  return { blocks: repaired(blocks), staff: tidied };
}

// The AI's short working notes sometimes lose the space before a common word
// right before a tool call ("update mybrief", "check thatdocument"), while its
// thinking in the same job spells them correctly. Only a glued word that starts
// with one of these short words, whose rest is a word used separately in the
// same job, is split. Real words that start the same way are left alone.
const GLUED_STARTS = [
  "that",
  "this",
  "these",
  "those",
  "their",
  "right",
  "your",
  "our",
  "my",
];
const REAL_WORDS = new Set([
  "myself",
  "ourselves",
  "yourself",
  "yourselves",
  "rightmost",
  "rightward",
  "rightwards",
  "righteous",
  "thereby",
  "therein",
  "thereof",
  "thesis",
  "mystery",
  "mystic",
  "myriad",
]);

/** Words from the job's thinking, which keeps its spacing, and from what its tools reported. */
function jobVocabulary(blocks: WorkLogBlock[]) {
  const words = new Set<string>();
  for (const block of blocks) {
    const text = [
      block.thinking,
      ...block.facts.flatMap((fact) => [
        fact.line,
        fact.result ?? "",
        ...(fact.found ?? []),
      ]),
    ].join(" ");
    for (const word of text.toLowerCase().match(/[a-z]+/g) ?? [])
      words.add(word);
  }
  return words;
}

export function repairGluedWords(text: string, vocabulary: Set<string>) {
  return text.replace(/\b[a-z]{5,}\b/g, (word) => {
    if (REAL_WORDS.has(word) || vocabulary.has(word)) return word;
    for (const start of GLUED_STARTS) {
      const rest = word.slice(start.length);
      if (word.startsWith(start) && rest.length >= 3 && vocabulary.has(rest))
        return `${start} ${rest}`;
    }
    return word;
  });
}

const COUNT_WORDS: Record<string, [string, string]> = {
  read: ["read 1 document", "read {n} documents"],
  search: ["searched once", "searched {n} times"],
  view: ["looked at 1 page image", "looked at {n} page images"],
  calculate: ["ran 1 calculation", "ran {n} calculations"],
};

function counted(kind: string, count: number) {
  const words = COUNT_WORDS[kind];
  if (!words || !count) return null;
  return count === 1 ? words[0] : words[1].replace("{n}", String(count));
}

function checked(facts: WorkLogBlock["facts"]) {
  if (!facts.length) return null;
  const things = [
    ...new Set(
      facts.map((fact) =>
        fact.line
          .replace(/^(Checked|Checking)\s+/i, "")
          .replace(/^./, (letter) => letter.toLowerCase()),
      ),
    ),
  ];
  return `checked ${things.slice(0, 4).join(", ")}${things.length > 4 ? ` and ${things.length - 4} more` : ""}`;
}

/** Heading for a turn with no note: what it actually did. */
export function blockHeading(block: WorkLogBlock) {
  if (block.note.trim()) return block.note.trim();
  const done = block.facts.filter((fact) => fact.state !== "running");
  const reads = new Set(
    done
      .filter((fact) => fact.kind === "read")
      .map((fact) => fact.subject ?? fact.id),
  ).size;
  const parts = [
    counted("read", reads),
    counted("search", done.filter((fact) => fact.kind === "search").length),
    counted("view", done.filter((fact) => fact.kind === "view").length),
    counted(
      "calculate",
      done.filter((fact) => fact.kind === "calculate").length,
    ),
    checked(done.filter((fact) => fact.kind === "check")),
    ...done
      .filter((fact) => fact.kind === "hire")
      .map((fact) => `hired ${fact.subject}`),
    ...done
      .filter((fact) => fact.kind === "assign")
      .map((fact) => `handed work to ${fact.subject}`),
    ...done
      .filter((fact) => fact.kind === "save")
      .map((fact) => `saved ${fact.subject ?? "a draft"}`),
    ...done
      .filter((fact) => fact.kind === "propose")
      .map((fact) => `prepared ${fact.result ?? "records"} for your review`),
  ].filter(Boolean) as string[];
  if (!parts.length) return block.facts.length ? block.facts[0].line : "";
  const sentence = parts.join(", ");
  return sentence.charAt(0).toUpperCase() + sentence.slice(1);
}

/** "read 23 documents · searched 6 times · hired Layla Haddad" for a finished job. */
export function workSummary(log: WorkLog) {
  const facts = [
    ...log.blocks,
    ...[...log.staff.values()].flatMap((staff) => staff.blocks),
  ].flatMap((block) => block.facts.filter((fact) => fact.state === "done"));
  const reads = new Set(
    facts
      .filter((fact) => fact.kind === "read")
      .map((fact) => fact.subject ?? fact.id),
  ).size;
  const parts = [
    counted("read", reads),
    counted("search", facts.filter((fact) => fact.kind === "search").length),
    counted("view", facts.filter((fact) => fact.kind === "view").length),
    ...facts
      .filter((fact) => fact.kind === "hire")
      .map((fact) => `hired ${fact.subject}`),
    ...facts
      .filter((fact) => fact.kind === "assign")
      .map(
        (fact) =>
          `asked ${fact.subject}${fact.result ? ` for ${fact.result.charAt(0).toLowerCase()}${fact.result.slice(1)}` : ""}`,
      ),
    ...facts
      .filter((fact) => fact.kind === "save")
      .map((fact) => `saved ${fact.subject ?? "a draft"}`),
  ].filter(Boolean) as string[];
  return parts;
}

/** The engineering-terms reason a job stopped. */
export function stopReason(status: string, error?: string | null) {
  const text = (error ?? "").toLowerCase();
  if (status === "cancelled") return { reason: "you stopped it", limit: false };
  if (
    text.includes("ai steps allowed") ||
    text.includes("request allowance") ||
    text.includes("work limit")
  )
    return {
      reason: "it used all the AI steps allowed for one job",
      limit: false,
    };
  if (
    text.includes("allowance") ||
    text.includes("spending") ||
    text.includes("budget")
  )
    return {
      reason: "the spending limit for this tender was reached",
      limit: true,
    };
  if (text.includes("stopped its reply") || text.includes("dropped its reply"))
    return { reason: "the AI service dropped its reply", limit: false };
  if (status === "interrupted")
    return { reason: "Quantix restarted before it finished", limit: false };
  // Service failures in plain words; the setup detail lives in Settings.
  if (text.includes("longer than its output limit"))
    return {
      reason:
        "the AI's reply was too long for its output limit. Continue, or raise the limit",
      limit: true,
    };
  if (text.includes("run out of credit"))
    return {
      reason:
        "the AI account has run out of credit. Add credit with the provider, then continue",
      limit: false,
    };
  if (text.includes("after repeated attempts"))
    return {
      reason:
        "the AI kept getting one step wrong, so nothing was saved. Continue to try again",
      limit: false,
    };
  if (text.includes("too large for the provider") || text.includes("http 413"))
    return {
      reason:
        "the job's request grew too large for the AI service. Continue to pick up from here",
      limit: false,
    };
  if (text.includes("rate or usage limit"))
    return {
      reason:
        "the AI service is busy or its account limit was reached. Try again shortly",
      limit: false,
    };
  if (text.includes("connection failed") || text.includes("timed out"))
    return { reason: "Quantix could not reach the AI service", limit: false };
  if (text.includes("unavailable"))
    return {
      reason: "the AI service was unavailable. Try again shortly",
      limit: false,
    };
  if (/provider (rejected|denied|could not find|returned http)/.test(text))
    return {
      reason:
        "the AI service refused the request. Check the AI connection in Settings",
      limit: false,
    };
  if (text.includes("could not be found in the selected tender"))
    return {
      reason: "it asked for something that is not in this tender",
      limit: false,
    };
  if (text.includes("tool call was refused"))
    return { reason: "one of its steps was refused", limit: false };
  if (
    text.includes("could not correct its proposal") ||
    text.includes("require tender source evidence")
  )
    return {
      reason:
        "it could not back its answer with the tender documents, so nothing was saved",
      limit: false,
    };
  if (text.includes("structured output"))
    return {
      reason: "the AI's answer came back incomplete, so nothing was saved",
      limit: false,
    };
  if (
    text.includes("already has a different proposal") ||
    text.includes("already uses item number")
  )
    return {
      reason:
        "a BOQ row clashed with one already saved, so nothing from this job was saved. Continue to redo it",
      limit: false,
    };
  if (text.includes("no captured tender manager profile"))
    return {
      reason:
        "the Tender Manager wasn't set up when this job started. Run it again",
      limit: false,
    };
  if (text.includes("worker"))
    return {
      reason:
        "the AI connection stopped working. Check the AI connection in Settings",
      limit: false,
    };
  return {
    reason: error?.trim()
      ? error
          .trim()
          .replace(/\.$/, "")
          .replace(/\bprovider\b/gi, "AI service")
      : "it could not finish",
    limit: false,
  };
}
