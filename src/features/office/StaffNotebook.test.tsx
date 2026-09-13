import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../../api";
import { StaffNotebook } from "./StaffNotebook";

type NotebookEntry = Schema<"NotebookEntry">;

function entry(
  partial: Partial<NotebookEntry> & { id: string },
): NotebookEntry {
  return {
    tender_id: "tender-a",
    staff_id: "staff-1",
    assignment_id: null,
    kind: "finding",
    text: `Note ${partial.id}.`,
    refs: [],
    applicability: "current_assignment",
    supersedes_id: null,
    created_at: "2026-09-10T08:05:00Z",
    actor_id: "staff-1",
    profile_version: 1,
    route_binding_id: null,
    root_run_id: null,
    source_scope: "unbound",
    current: true,
    stale: false,
    ...partial,
  };
}

function page(items: NotebookEntry[], next_cursor: string | null) {
  return { items, next_cursor, total: items.length };
}

function renderNotebook(api: Api, props = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ApiContext.Provider value={api}>
        <StaffNotebook
          tenderId="tender-a"
          staffId="staff-1"
          onSource={vi.fn()}
          {...props}
        />
      </ApiContext.Provider>
    </QueryClientProvider>,
  );
}

describe("StaffNotebook", () => {
  it("loads notes and pages without repeating entries", async () => {
    const first = entry({ id: "note-1" });
    const second = entry({ id: "note-2" });
    const api = {
      get: vi
        .fn()
        .mockResolvedValueOnce(page([first], "cursor-1"))
        .mockResolvedValueOnce(page([second], null)),
    } as unknown as Api;
    renderNotebook(api);

    expect(await screen.findByText("Note note-1.")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Load older notes" }),
    );
    expect(await screen.findByText("Note note-2.")).toBeInTheDocument();
    expect(screen.getAllByText(/Note note-/)).toHaveLength(2);
  });

  it("requests open questions and corrected history through the matching views", async () => {
    const calls: string[] = [];
    const api = {
      get: vi.fn().mockImplementation((url: string) => {
        calls.push(url);
        const params = new URLSearchParams(url.split("?")[1] ?? "");
        if (params.get("kind") === "open_question") {
          return Promise.resolve(
            page([entry({ id: "note-q", kind: "open_question" })], null),
          );
        }
        if (params.get("current") === "false") {
          return Promise.resolve(
            page(
              [entry({ id: "note-old", current: false, stale: true })],
              null,
            ),
          );
        }
        return Promise.resolve(page([entry({ id: "note-1" })], null));
      }),
    } as unknown as Api;
    renderNotebook(api);

    expect(await screen.findByText("Note note-1.")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Open questions" }),
    );
    expect(await screen.findByText("Note note-q.")).toBeInTheDocument();
    expect(calls.some((url) => url.includes("kind=open_question"))).toBe(true);

    await userEvent.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("Note note-old.")).toBeInTheDocument();
    expect(screen.getByText("Corrected — history is kept")).toBeInTheDocument();
    expect(calls.some((url) => url.includes("current=false"))).toBe(true);
  });

  it("retries a failed notes read without losing the desk", async () => {
    const api = {
      get: vi
        .fn()
        .mockRejectedValueOnce(new Error("Notebook unavailable."))
        .mockResolvedValueOnce(page([entry({ id: "note-1" })], null)),
    } as unknown as Api;
    renderNotebook(api);

    expect(
      await screen.findByText("Notebook unavailable."),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Retry work notes" }),
    );
    expect(await screen.findByText("Note note-1.")).toBeInTheDocument();
  });

  it("opens saved staff results and work outputs from note references", async () => {
    const onOpenResult = vi.fn();
    const onOpenOutput = vi.fn();
    const noted = entry({
      id: "note-refs",
      refs: ["source:evidence-1", "staff_result:result-1", "output:output-1"],
    });
    const api = {
      get: vi
        .fn()
        .mockImplementation((url: string) =>
          url.includes("/evidence/")
            ? Promise.reject(new Error("Evidence unavailable."))
            : Promise.resolve(page([noted], null)),
        ),
    } as unknown as Api;
    renderNotebook(api, { onOpenResult, onOpenOutput });

    await userEvent.click(
      await screen.findByRole("button", { name: "Open saved staff result" }),
    );
    expect(onOpenResult).toHaveBeenCalledWith("result-1");
    await userEvent.click(
      screen.getByRole("button", { name: "Open work output" }),
    );
    expect(onOpenOutput).toHaveBeenCalledWith("output-1");
    const note = (await screen.findByText("Note note-refs.")).closest(
      "article",
    );
    expect(note).not.toBeNull();
    expect(
      within(note as HTMLElement).getByRole("button", {
        name: "Source unavailable",
      }),
    ).toBeInTheDocument();
  });
});
