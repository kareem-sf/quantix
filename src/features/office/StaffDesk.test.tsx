import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../../api";
import { StaffDesk } from "./StaffDesk";

const profile = {
  display_name: "Amina El Masry",
  role: "Document controller",
  title: "Document Controller",
  specialisms: ["Drawing registers"],
  persona: "Keeps revision evidence explicit.",
  personality: {
    description: "Careful and direct.",
    traits: ["careful"],
    communication_style: "Direct",
    problem_solving_style: "Checks revisions",
    collaboration_style: "Raises questions",
    uncertainty_handling: "Names unknowns",
    initiative: "Moderate",
    explanation_style: "Evidence first",
    language_preferences: ["English"],
    working_habits: [],
  },
  responsibilities: ["Maintain the drawing register"],
  objectives: ["Prepare an attributable draft"],
  methods: ["Compare current revisions"],
  deliverables: ["Revision note"],
  success_criteria: ["Current artifact cited"],
  context_needs: ["Tender drawings"],
  requested_tool_ids: ["tool-read-source"],
  creation_reason: "Created for the drawing register review.",
  id: "staff-1",
  tender_id: "tender-a",
  version: 1,
  creator_run_id: "run-1",
  manager_profile_version: 1,
  created_at: "2026-09-10T08:00:00Z",
  updated_at: "2026-09-10T08:00:00Z",
  lifecycle: "available",
  portrait: { style: "notionists-v1", seed: "staff-1" },
} satisfies Schema<"StaffProfileRecord">;

const result: Schema<"StaffResult"> = {
  id: "result-1",
  tender_id: "tender-a",
  assignment_id: "assignment-1",
  root_run_id: "run-1",
  staff_id: "staff-1",
  staff_version: 1,
  work_order_id: "work-order-1",
  route_binding_id: "route-1",
  office_output: {
    summary: "Drawing register draft",
    source_ids: ["source-1"],
  },
  authored_notes: ["One revision needs engineer review."],
  source_ids_read: ["source-1"],
  source_bases: [
    {
      source_id: "source-1",
      artifact_id: "artifact-1",
      artifact_version: 2,
      artifact_hash: "hash-1",
      locator: "page 4",
    },
  ],
  web_sources: [],
  item_bases: [],
  trusted_recipients: [],
  source_recipients: [],
  approved_plan_id: null,
  usage: {},
  created_at: "2026-09-10T08:20:00Z",
  currentness: "needs_review",
};

function desk(profileVersion = profile.version): Schema<"StaffDesk"> {
  return {
    profile: { ...profile, version: profileVersion },
    version_numbers: [1, 2],
    work_orders: [
      {
        id: "work-order-1",
        tender_id: "tender-a",
        staff_id: "staff-1",
        staff_version: 1,
        creator_run_id: "run-1",
        scope_id: "scope-1",
        work_order: {
          brief: "Compare the current drawing revisions.",
          goal: "Prepare the drawing register draft.",
          source_ids: ["source-1"],
          expected_outputs: ["Revision note"],
          completion_checks: ["Cite each changed drawing"],
        },
        created_at: "2026-09-10T08:00:00Z",
      },
    ],
    assignments: [
      {
        id: "assignment-1",
        tender_id: "tender-a",
        root_run_id: "run-1",
        staff_id: "staff-1",
        staff_version: 1,
        work_order_id: "work-order-1",
        route_binding_id: "route-1",
        status: "completed",
        revision: 2,
        detail: "Register draft saved.",
        result_id: "result-1",
        parent_assignment_id: null,
        depth: 0,
        prerequisites: [],
        created_at: "2026-09-10T08:00:00Z",
        updated_at: "2026-09-10T08:20:00Z",
      },
    ],
    results: [result],
    receipts: [
      {
        id: "receipt-1",
        tender_id: "tender-a",
        actor_id: "staff-1",
        profile_id: "staff-1",
        profile_version: 1,
        assignment_id: "assignment-1",
        route_binding_id: "route-1",
        root_run_id: "run-1",
        source_id: "source-1",
        artifact_id: "artifact-1",
        artifact_version: 2,
        content_hash: "hash-1",
        locator: "page 4",
        text_offset: 0,
        text_length: 40,
        page: 4,
        region: null,
        visible_cells: [],
        method: "source_read",
        created_at: "2026-09-10T08:10:00Z",
      },
    ],
    messages: { items: [], next_cursor: null },
    partial_flags: {
      staff: false,
      assignments: false,
      messages: false,
      work_orders: false,
      results: false,
      receipts: false,
      versions: false,
    },
  };
}

function deskWithSecondAssignment() {
  const base = desk();
  const firstReceipt = base.receipts?.[0];
  return {
    ...base,
    assignments: [
      ...(base.assignments ?? []),
      {
        ...base.assignments![0],
        id: "assignment-2",
        work_order_id: "work-order-2",
        detail: "Second source inspection is queued.",
      },
    ],
    receipts: firstReceipt
      ? [
          ...base.receipts!,
          {
            ...firstReceipt,
            id: "receipt-2",
            assignment_id: "assignment-2",
            source_id: "source-2",
          },
        ]
      : [],
    partial_flags: { ...base.partial_flags!, receipts: true },
  } satisfies Schema<"StaffDesk">;
}

function deskMessage(id: string, text: string): Schema<"OfficeMessage"> {
  return {
    id,
    tender_id: "tender-a",
    root_run_id: "run-1",
    sender: {
      id: "staff-1",
      version: 1,
      display_name: "Amina El Masry",
      title: "Document Controller",
      kind: "staff",
      assignment_id: "assignment-1",
    },
    recipients: [],
    assignment_id: "assignment-1",
    kind: "question",
    text,
    artifact_refs: [],
    reply_to: null,
    created_at: "2026-09-10T08:30:00Z",
  };
}

describe("StaffDesk", () => {
  it("loads the actual desk, selects saved versions, opens exact results and links receipts", async () => {
    const onSource = vi.fn();
    const onOpenResult = vi.fn();
    const api = {
      get: vi.fn(async (path: string) => {
        if (path.includes("/office/results/result-1")) return result;
        if (path.includes("?version=2")) return desk(2);
        return desk();
      }),
    } as unknown as Api;
    const view = render(
      <ApiContext.Provider value={api}>
        <StaffDesk
          tenderId="tender-a"
          staffId="staff-1"
          onSource={onSource}
          onOpenResult={onOpenResult}
        />
      </ApiContext.Provider>,
    );

    expect(
      await screen.findByRole("heading", { name: "Amina El Masry" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Drawing register draft")).toBeInTheDocument();
    await userEvent.selectOptions(
      screen.getByLabelText("Inspect saved profile version"),
      "2",
    );
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/staff/staff-1?version=2",
      expect.any(AbortSignal),
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Open exact draft result" }),
    );
    expect(api.get).not.toHaveBeenCalledWith(
      "/tenders/tender-a/office/results/result-1",
      expect.any(AbortSignal),
    );
    expect(onOpenResult).toHaveBeenCalledWith("result-1");
    expect(
      screen.queryByText(/This is draft engineering content/),
    ).not.toBeInTheDocument();
    view.rerender(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-1" onSource={onSource} />
      </ApiContext.Provider>,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Open exact draft result" }),
    );
    expect(
      await screen.findByText(/This is draft engineering content/),
    ).toBeInTheDocument();
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/office/results/result-1",
      expect.any(AbortSignal),
    );

    await userEvent.click(
      screen.getByRole("button", { name: /source-1 · page 4 · source_read/ }),
    );
    expect(onSource).toHaveBeenCalledWith(
      expect.objectContaining({
        sourceId: "source-1",
        artifactId: "artifact-1",
        version: 2,
        page: 4,
      }),
    );
  });

  it("ignores a late desk response after switching staff identities", async () => {
    let resolveFirst!: (value: Schema<"StaffDesk">) => void;
    let resolveSecond!: (value: Schema<"StaffDesk">) => void;
    const first = new Promise<Schema<"StaffDesk">>((resolve) => {
      resolveFirst = resolve;
    });
    const second = new Promise<Schema<"StaffDesk">>((resolve) => {
      resolveSecond = resolve;
    });
    const api = {
      get: vi.fn((path: string) => (path.includes("staff-1") ? first : second)),
    } as unknown as Api;
    const view = render(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-1" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );
    await waitFor(() =>
      expect(api.get).toHaveBeenCalledWith(
        "/tenders/tender-a/staff/staff-1",
        expect.any(AbortSignal),
      ),
    );
    view.rerender(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-2" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );
    const secondProfile = {
      ...desk().profile,
      id: "staff-2",
      display_name: "Second staff",
    };
    await actResolve(resolveFirst, desk());
    await actResolve(resolveSecond, { ...desk(), profile: secondProfile });
    expect(
      await screen.findByRole("heading", { name: "Second staff" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Amina El Masry" }),
    ).not.toBeInTheDocument();
  });

  it("starts each bounded history endpoint at its own offset zero", async () => {
    const paged = desk();
    paged.partial_flags = {
      ...paged.partial_flags!,
      work_orders: true,
      results: true,
      receipts: true,
    };
    const api = {
      get: vi.fn(async (path: string) => {
        if (path.includes("/work-orders"))
          return { items: [], next_offset: 0, has_more: false };
        if (path.includes("/results?"))
          return { items: [], next_offset: 0, has_more: false };
        if (path.includes("/receipts?"))
          return { items: [], next_offset: 0, has_more: false };
        return paged;
      }),
    } as unknown as Api;
    render(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-1" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );
    expect(
      await screen.findByRole("heading", { name: "Amina El Masry" }),
    ).toBeInTheDocument();

    await userEvent.click(
      screen.getByRole("button", { name: "Load older work orders" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Load older results" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Load more source inspections" }),
    );
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/staff/staff-1/work-orders?offset=0&limit=20",
      expect.any(AbortSignal),
    );
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/staff/staff-1/results?offset=0&limit=20",
      expect.any(AbortSignal),
    );
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/office/assignments/assignment-1/receipts?offset=0&limit=20",
      expect.any(AbortSignal),
    );
  });

  it("keeps a second assignment receipt page available after the first is exhausted", async () => {
    const fullDesk = deskWithSecondAssignment();
    const api = {
      get: vi.fn(async (path: string) => {
        if (path.includes("/receipts?") && path.includes("assignment-1")) {
          return { items: [], next_offset: null, has_more: false };
        }
        if (path.includes("/receipts?") && path.includes("assignment-2")) {
          return {
            items: fullDesk.receipts?.slice(-1),
            next_offset: null,
            has_more: false,
          };
        }
        return fullDesk;
      }),
    } as unknown as Api;
    render(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-1" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );
    expect(
      await screen.findByRole("heading", { name: "Amina El Masry" }),
    ).toBeInTheDocument();
    const initialButtons = screen.getAllByRole("button", {
      name: "Load more source inspections",
    });
    expect(initialButtons).toHaveLength(2);
    await userEvent.click(initialButtons[0]);
    expect(
      screen.getAllByRole("button", { name: "Load more source inspections" }),
    ).toHaveLength(1);
    await userEvent.click(
      screen.getByRole("button", { name: "Load more source inspections" }),
    );
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/office/assignments/assignment-1/receipts?offset=0&limit=20",
      expect.any(AbortSignal),
    );
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/office/assignments/assignment-2/receipts?offset=0&limit=20",
      expect.any(AbortSignal),
    );
  });

  it("loads older assignment lifecycle records through the Tender-scoped page", async () => {
    const fullDesk = deskWithSecondAssignment();
    fullDesk.partial_flags = { ...fullDesk.partial_flags!, assignments: true };
    const olderAssignment = {
      ...fullDesk.assignments![0],
      id: "assignment-older",
      work_order_id: "work-order-older",
      detail: "Older completed assignment retained in office history.",
      created_at: "2026-09-01T08:00:00Z",
      updated_at: "2026-09-01T08:30:00Z",
    };
    const api = {
      get: vi.fn(async (path: string) =>
        path.includes("/office/assignments?")
          ? { items: [olderAssignment], next_offset: null, has_more: false }
          : fullDesk,
      ),
    } as unknown as Api;
    render(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-1" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );
    expect(
      await screen.findByRole("heading", { name: "Amina El Masry" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Load older assignments" }),
    );
    expect(
      await screen.findByText(
        "Older completed assignment retained in office history.",
      ),
    ).toBeInTheDocument();
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/office/assignments?staff_id=staff-1&offset=0&limit=20",
      expect.any(AbortSignal),
    );
  });

  it("re-enables the new staff controls when old work-order and history reads finish late", async () => {
    let resolveWorkOrderA!: (value: unknown) => void;
    let resolveHistoryA!: (value: unknown) => void;
    const workOrderA = new Promise((resolve) => {
      resolveWorkOrderA = resolve;
    });
    const historyA = new Promise((resolve) => {
      resolveHistoryA = resolve;
    });
    const deskA = desk();
    deskA.partial_flags = { ...deskA.partial_flags!, work_orders: true };
    deskA.messages = {
      items: [deskMessage("history-a", "A history")],
      next_cursor: "cursor-a",
    };
    const deskB = {
      ...deskA,
      profile: {
        ...deskA.profile,
        id: "staff-2",
        display_name: "Second staff",
      },
      messages: {
        items: [deskMessage("history-b", "B history")],
        next_cursor: "cursor-b",
      },
    };
    const api = {
      get: vi.fn((path: string) => {
        if (path.includes("/work-orders") && path.includes("staff-1"))
          return workOrderA;
        if (
          path.includes("office/messages") &&
          path.includes("cursor=cursor-a")
        )
          return historyA;
        if (path.endsWith("/staff/staff-1")) return Promise.resolve(deskA);
        if (path.endsWith("/staff/staff-2")) return Promise.resolve(deskB);
        return Promise.resolve(deskB);
      }),
    } as unknown as Api;
    const view = render(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-1" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );
    expect(
      await screen.findByRole("heading", { name: "Amina El Masry" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Load older work orders" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Load earlier exchanges" }),
    );

    view.rerender(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-2" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );
    expect(
      await screen.findByRole("heading", { name: "Second staff" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Load older work orders" }),
    ).toBeEnabled();
    expect(
      screen.getByRole("button", { name: "Load earlier exchanges" }),
    ).toBeEnabled();

    await actResolve(resolveWorkOrderA, {
      items: [],
      next_offset: null,
      has_more: false,
    });
    await actResolve(resolveHistoryA, { items: [], next_cursor: null });
    expect(
      screen.getByRole("button", { name: "Load older work orders" }),
    ).toBeEnabled();
    expect(
      screen.getByRole("button", { name: "Load earlier exchanges" }),
    ).toBeEnabled();
  });

  it("retires an available colleague through the desk action", async () => {
    const api = {
      get: vi.fn(async () => desk()),
      post: vi.fn(async () => ({
        staff: { ...profile, lifecycle: "retired" },
        replayed: false,
      })),
    } as unknown as Api;
    render(
      <ApiContext.Provider value={api}>
        <StaffDesk tenderId="tender-a" staffId="staff-1" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );
    await screen.findByRole("heading", { name: "Amina El Masry" });
    await userEvent.click(
      screen.getByRole("button", { name: "Retire colleague" }),
    );
    expect(api.post).toHaveBeenCalledWith(
      "/tenders/tender-a/staff/staff-1/lifecycle",
      expect.objectContaining({ target: "retired", staff_id: "staff-1" }),
    );
  });
});

async function actResolve<T>(resolve: (value: T) => void, value: T) {
  await act(async () => resolve(value));
}
