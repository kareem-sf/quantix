import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../../api";
import { LiveOffice } from "./LiveOffice";
import { useOffice } from "./useOffice";

vi.mock("./useOffice", () => ({
  useOffice: vi.fn(),
}));

const mockedUseOffice = vi.mocked(useOffice);

const manager: Schema<"ManagerProfile"> = {
  id: "manager-1",
  version: 1,
  display_name: "Tender Manager",
  title: "Tender Manager",
  persona: "Coordinates evidence and asks for engineer decisions.",
  personality: {
    description: "Clear and careful.",
    traits: ["careful"],
    communication_style: "Plain language",
    problem_solving_style: "Evidence first",
    collaboration_style: "Direct",
    uncertainty_handling: "Names unknowns",
    initiative: "Moderate",
    explanation_style: "Concise",
    language_preferences: ["English"],
    working_habits: [],
  },
  working_preferences: ["Keep decisions visible"],
  created_at: "2026-09-10T08:00:00Z",
  updated_at: "2026-09-10T08:00:00Z",
};

const profile: Schema<"StaffProfileRecord"> = {
  display_name: "Amal Kassem",
  role: "Temporary works coordinator",
  title: "Temporary Works Coordinator",
  specialisms: ["Temporary works"],
  persona: "Reviews temporary works evidence against the tender brief.",
  personality: {
    description: "Methodical.",
    traits: ["methodical"],
    communication_style: "Concise",
    problem_solving_style: "Checks source revisions",
    collaboration_style: "Raises questions early",
    uncertainty_handling: "Marks unknowns",
    initiative: "Moderate",
    explanation_style: "Shows evidence",
    language_preferences: ["English"],
    working_habits: [],
  },
  responsibilities: ["Compare temporary works notes"],
  objectives: ["Prepare a source-backed draft"],
  methods: ["Read current revisions"],
  deliverables: ["Draft finding"],
  success_criteria: ["Every finding has a source"],
  context_needs: ["Current tender drawings"],
  requested_tool_ids: ["tool-source-read"],
  creation_reason: "Created for the temporary works review.",
  id: "staff-1",
  tender_id: "tender-a",
  version: 1,
  creator_run_id: "run-1",
  manager_profile_version: 1,
  created_at: "2026-09-10T08:05:00Z",
  updated_at: "2026-09-10T08:05:00Z",
  lifecycle: "active",
  portrait: { style: "notionists-v1", seed: "staff-1" },
};

function snapshot(
  staff: Schema<"StaffProfileRecord">[] = [],
): Schema<"OfficeSnapshot"> {
  return {
    manager,
    staff,
    assignments: staff.length
      ? [
          {
            id: "assignment-1",
            tender_id: "tender-a",
            root_run_id: "run-1",
            staff_id: "staff-1",
            staff_version: 1,
            work_order_id: "work-order-1",
            route_binding_id: "route-1",
            status: "waiting",
            revision: 1,
            detail: "Waiting for the current drawing revision.",
            result_id: null,
            parent_assignment_id: null,
            depth: 0,
            prerequisites: [],
            created_at: "2026-09-10T08:05:00Z",
            updated_at: "2026-09-10T08:10:00Z",
          },
        ]
      : [],
    messages: { items: [], next_cursor: null },
    cursor: null,
    sequence: staff.length ? 4 : 0,
    instance_id: "instance-a",
    interrupted_assignment_count: 0,
    staff_total: staff.length,
    assignments_total: staff.length,
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

function setOffice(value: Schema<"OfficeSnapshot">) {
  mockedUseOffice.mockReturnValue({
    snapshot: value,
    connectionState: "connected",
    error: null,
    events: value.staff?.length
      ? [
          {
            event_id: "staff-created-1",
            sequence: 1,
            tender_id: "tender-a",
            event_type: "staff_created",
            actor_id: "staff-1",
            assignment_id: "assignment-1",
            record_ref: { staff_id: "staff-1" },
            payload: null,
            occurred_at: "2026-09-10T08:05:00Z",
          },
        ]
      : [],
    arrivalIds: value.staff?.length ? ["staff-created-1"] : [],
    refresh: vi.fn(),
    retry: vi.fn(),
  });
}

function renderOffice(
  value: Schema<"OfficeSnapshot">,
  onOpenManager = vi.fn(),
) {
  setOffice(value);
  return render(
    <ApiContext.Provider
      value={{ get: vi.fn(), post: vi.fn() } as unknown as Api}
    >
      <LiveOffice
        tenderId="tender-a"
        onSource={vi.fn()}
        onOpenManager={onOpenManager}
      />
    </ApiContext.Provider>,
  );
}

describe("LiveOffice", () => {
  it("keeps the empty office Manager-only and gives one next action", async () => {
    const onOpenManager = vi.fn();
    renderOffice(snapshot(), onOpenManager);

    expect(screen.getAllByText("Tender Manager")).not.toHaveLength(0);
    expect(
      screen.getByText("The Manager is the only colleague so far."),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("Temporary Works Coordinator"),
    ).not.toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: /Open Manager conversation/ }),
    );
    expect(onOpenManager).toHaveBeenCalledOnce();
  });

  it("renders arbitrary generated staff from actual assignment state and event arrival", () => {
    renderOffice(snapshot([profile]));

    expect(screen.getByText("Amal Kassem")).toBeInTheDocument();
    expect(screen.getByText("Waiting for a reply")).toBeInTheDocument();
    expect(
      screen.getByText("Waiting for the current drawing revision."),
    ).toBeInTheDocument();
    expect(document.querySelector("[data-staff-id='staff-1']")).toHaveClass(
      "office-staff-card-arriving",
    );
    expect(screen.queryByText("Quantity Surveyor")).not.toBeInTheDocument();
  });

  it("loads a full staff history page without inventing a roster", async () => {
    const roster = Array.from({ length: 100 }, (_, index) => ({
      ...profile,
      id: `staff-${index + 1}`,
      display_name: `Colleague ${index + 1}`,
      portrait: { style: "remote-style", seed: `staff-${index + 1}` },
    }));
    const additional = {
      ...roster[99],
      id: "staff-101",
      display_name: "Colleague 101",
      portrait: { style: "remote-style", seed: "staff-101" },
    };
    const api = {
      get: vi
        .fn()
        .mockResolvedValue({ items: [additional], next_cursor: null }),
    } as unknown as Api;
    setOffice(snapshot(roster));
    render(
      <ApiContext.Provider value={api}>
        <LiveOffice tenderId="tender-a" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );

    expect(screen.getByText("Colleague 100")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Load more colleagues" }),
    );
    expect(await screen.findByText("Colleague 101")).toBeInTheDocument();
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/staff?limit=50&lifecycle=available",
      expect.any(AbortSignal),
    );
  });

  it("filters retired colleagues into history and reactivates one for later work", async () => {
    const retired = {
      ...profile,
      id: "staff-retired",
      display_name: "Retired Colleague",
      lifecycle: "retired",
    };
    const api = {
      get: vi.fn().mockImplementation((url: string) => {
        if (String(url).includes("lifecycle=history")) {
          return Promise.resolve({ items: [retired], next_cursor: null });
        }
        return Promise.resolve({ items: [], next_cursor: null });
      }),
      post: vi.fn().mockResolvedValue({
        staff: { ...retired, lifecycle: "available", version: 2 },
        replayed: false,
      }),
    } as unknown as Api;
    setOffice(snapshot([profile]));
    render(
      <ApiContext.Provider value={api}>
        <LiveOffice tenderId="tender-a" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );

    expect(screen.getByText("Amal Kassem")).toBeInTheDocument();
    expect(screen.queryByText("Retired Colleague")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "History" }));
    expect(await screen.findByText("Retired Colleague")).toBeInTheDocument();
    expect(screen.queryByText("Amal Kassem")).not.toBeInTheDocument();
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/staff?limit=50&lifecycle=history",
      expect.any(AbortSignal),
    );

    await userEvent.click(
      screen.getByRole("button", {
        name: "Reactivate Retired Colleague for later work",
      }),
    );
    expect(api.post).toHaveBeenCalledWith(
      "/tenders/tender-a/staff/staff-retired/lifecycle",
      expect.objectContaining({
        target: "available",
        staff_id: "staff-retired",
      }),
    );
  });

  it("keeps the retired colleague visible when reactivation conflicts", async () => {
    const retired = {
      ...profile,
      id: "staff-retired",
      display_name: "Retired Colleague",
      lifecycle: "retired",
    };
    const api = {
      get: vi.fn().mockResolvedValue({ items: [retired], next_cursor: null }),
      post: vi
        .fn()
        .mockRejectedValue(new Error("The staff profile changed; refresh.")),
    } as unknown as Api;
    setOffice(snapshot([]));
    render(
      <ApiContext.Provider value={api}>
        <LiveOffice tenderId="tender-a" onSource={vi.fn()} />
      </ApiContext.Provider>,
    );

    await userEvent.click(screen.getByRole("button", { name: "History" }));
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Reactivate Retired Colleague for later work",
      }),
    );
    expect(
      await screen.findByText("The staff profile changed; refresh."),
    ).toBeInTheDocument();
    expect(screen.getByText("Retired Colleague")).toBeInTheDocument();
  });

  it("removes arrival travel when reduced motion is selected", async () => {
    renderOffice(snapshot([profile]));
    await userEvent.selectOptions(
      screen.getByLabelText("Motion preference"),
      "reduced",
    );
    expect(screen.getByLabelText("Live office")).toHaveAttribute(
      "data-motion",
      "reduced",
    );
    expect(document.querySelector(".office-staff-card-arriving")).toBeTruthy();
  });
});
