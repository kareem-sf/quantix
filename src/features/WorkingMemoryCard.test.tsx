import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../api";
import { WorkingMemoryCard } from "./WorkingMemoryCard";

function note(
  overrides: Partial<Schema<"WorkingMemoryRecord">> = {},
): Schema<"WorkingMemoryRecord"> {
  return {
    id: "memory-a",
    tender_id: "tender-a",
    run_id: null,
    actor_id: "engineer",
    kind: "assumption",
    title: "Concrete allowance",
    content: "Allow for the specified concrete waste.",
    source_ids: ["source-a"],
    valid_until: "2026-10-01",
    created_at: "2026-09-13T08:00:00Z",
    dependencies: [
      {
        source_id: "source-a",
        artifact_id: "artifact-a",
        artifact_version: 2,
        content_hash: "a".repeat(64),
        evidence_hash: "b".repeat(64),
        current: true,
        reason: null,
      },
    ],
    state: "current",
    review_reasons: [],
    ...overrides,
  };
}

it("shows stale reasons and source inspection without offering promotion", async () => {
  const onSource = vi.fn();
  const api = {} as Api;
  render(
    <ApiContext.Provider value={api}>
      <WorkingMemoryCard
        tenderId="tender-a"
        note={note({
          state: "needs_review",
          review_reasons: ["source_revision_changed"],
          dependencies: [
            {
              ...note().dependencies[0],
              current: false,
              reason: "source_revision_changed",
            },
          ],
        })}
        onSource={onSource}
        onPromoted={vi.fn()}
      />
    </ApiContext.Provider>,
  );

  expect(screen.getByText("A supporting file changed.")).toBeVisible();
  await userEvent.click(
    screen.getByRole("button", { name: "Open working note source 1" }),
  );
  expect(onSource).toHaveBeenCalledWith({
    sourceId: "source-a",
    artifactId: "artifact-a",
    version: 2,
    contentHash: "a".repeat(64),
  });
  await userEvent.click(screen.getByText("More options"));
  expect(
    screen.getByText(/A stale note cannot become company knowledge/),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Promote to company knowledge" }),
  ).not.toBeInTheDocument();
});

it("retries an interrupted promotion with the exact same engineer decision", async () => {
  const writes: unknown[] = [];
  let attempts = 0;
  const promoted = { id: "knowledge-a", title: "Concrete allowance" };
  const api = {
    post: vi.fn(async (_path: string, body: unknown) => {
      writes.push(body);
      attempts += 1;
      if (attempts === 1)
        throw new Error("The local response was interrupted.");
      return promoted;
    }),
  } as unknown as Api;
  const onPromoted = vi.fn();
  render(
    <ApiContext.Provider value={api}>
      <WorkingMemoryCard
        tenderId="tender-a"
        note={note()}
        onSource={vi.fn()}
        onPromoted={onPromoted}
      />
    </ApiContext.Provider>,
  );
  await userEvent.click(screen.getByText("More options"));
  await userEvent.selectOptions(
    screen.getByLabelText("Knowledge category"),
    "method",
  );
  await userEvent.type(screen.getByLabelText("Verified on"), "2026-09-13");
  await userEvent.type(screen.getByLabelText("Recheck after"), "2027-03-13");
  await userEvent.type(
    screen.getByLabelText("Promotion rationale"),
    "Reviewed against the current synthetic source.",
  );
  await userEvent.click(screen.getByLabelText(/I reviewed this exact note/));
  await userEvent.click(
    screen.getByRole("button", { name: "Promote to company knowledge" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The local response was interrupted.",
  );
  await userEvent.click(
    screen.getByRole("button", { name: "Promote to company knowledge" }),
  );

  await waitFor(() => expect(onPromoted).toHaveBeenCalledWith(promoted));
  expect(writes).toHaveLength(2);
  expect(writes[1]).toEqual(writes[0]);
  expect(api.post).toHaveBeenLastCalledWith(
    "/tenders/tender-a/memory/memory-a/promote",
    {
      category: "method",
      engineer_confirmed: true,
      rationale: "Reviewed against the current synthetic source.",
      verified_on: "2026-09-13",
      recheck_after: "2027-03-13",
    },
  );
  expect(screen.getByText(/Settings → Knowledge/)).toBeVisible();
});
