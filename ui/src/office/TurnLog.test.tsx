import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { fakeService } from "../test/app";
import type { Turn, TurnStep } from "./queries";
import { TurnLog } from "./TurnLog";

/** Rania's turn at work, 45 seconds long and done unless said otherwise. */
const turn = (over: Partial<Turn> = {}, log: TurnStep[] = []) => ({
  id: 7, staff_id: "s1", started_at: "2026-09-23T10:43:00Z", ended_at: "2026-09-23T10:43:45Z", running: false, ended: "done",
  note: null, doing: null, steps: log.length, ...over, log,
});

function show(shown: ReturnType<typeof turn>, technical = false) {
  fakeService({ turns: [shown] });
  const onTechnical = vi.fn();
  const queries = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queries}>
      <TurnLog turn={shown} name="Rania" technical={technical} onTechnical={onTechnical} />
    </QueryClientProvider>,
  );
  return { line: screen.getByRole("button", { expanded: false }), onTechnical };
}

describe("A turn at work in the chat", () => {
  it("says in words how long it took and how it ended", () => {
    const cases: [Partial<Turn>, string][] = [
      [{ running: true, ended: null, ended_at: null, doing: "Reading ITT.pdf" }, "Working · Reading ITT.pdf"],
      [{ ended_at: "2026-09-23T10:44:30Z", doing: "Pricing 3.1" }, "Worked for 1 min 30 s · Pricing 3.1"],
      [{ ended_at: "2026-09-23T10:45:00Z" }, "Worked for 2 min"],
      [{ ended_at: "2026-09-23T10:43:00Z" }, "Worked for 1 s"],
      [{ ended: "out_of_steps" }, "Worked for 45 s and ran out of steps; carries on by itself"],
      [{ ended: "tool_failed" }, "Worked for 45 s; one step kept failing"],
      [{ ended: "ai_failed" }, "Stopped after 45 s: the AI service failed."],
      [{ ended: "stopped" }, "Stopped by you after 45 s"],
      [{ ended: "failed" }, "Stopped after 45 s: Quantix hit a problem of its own"],
      [{ ended: null }, "Cut off when Quantix closed"],
    ];
    for (const [over, words] of cases) {
      expect(show(turn(over)).line).toHaveTextContent(words);
      cleanup();
    }
  });

  it("says a running turn is thinking before its first step", async () => {
    const { line } = show(turn({ running: true, ended: null, ended_at: null }));

    await userEvent.click(line);
    expect(await screen.findByText("Thinking…")).toBeInTheDocument();
  });

  it("says when a turn ran before its steps were kept, and when nothing was written or done", async () => {
    await userEvent.click(show(turn()).line);
    expect(await screen.findByText("This turn ran before Quantix kept each turn's steps.")).toBeInTheDocument();
    cleanup();

    await userEvent.click(show(turn({}, [{ kind: "brief", text: "Tender: Synthetic school." }])).line);
    expect(await screen.findByText("Nothing was written or done in this turn.")).toBeInTheDocument();
  });

  it("marks a step still waiting for Quantix while the turn runs", async () => {
    const reading: TurnStep = { kind: "tool", tool: "read_page", args: '{"page":4}', doing: "Reading ITT.pdf, page 4", result: null, sent_back: null };
    await userEvent.click(show(turn({ running: true, ended: null, ended_at: null }, [reading])).line);

    expect(await screen.findByText("Reading ITT.pdf, page 4")).toBeInTheDocument();
    expect(screen.getByText("working…")).toBeInTheDocument();
  });

  it("shows in the technical view what was sent, even half-written, and why the turn ended", async () => {
    const steps: TurnStep[] = [
      { kind: "tool", tool: "read_page", args: '{"page": 4', doing: null, result: null, sent_back: null },
      { kind: "tool", tool: "list_documents", args: "{}", doing: null, result: "3 documents", sent_back: null },
    ];
    const { line, onTechnical } = show(turn({ ended: "tool_failed", note: "read_page kept failing: no such page." }, steps), true);
    await userEvent.click(line);

    expect(await screen.findByText('{"page": 4')).toBeInTheDocument();
    expect(screen.getAllByText("Sent")).toHaveLength(1); // nothing to show for a call sent with nothing
    expect(screen.getByText("3 documents")).toBeInTheDocument();
    expect(screen.getByText("Ended: tool_failed")).toBeInTheDocument();
    expect(screen.getByText("read_page kept failing: no such page.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("switch", { name: "Technical details" }));
    expect(onTechnical).toHaveBeenCalledWith(false);
  });
});
