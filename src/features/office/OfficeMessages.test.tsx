import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../../api";
import { OfficeMessages } from "./OfficeMessages";

function message(
  overrides: Partial<Schema<"OfficeMessage">> = {},
): Schema<"OfficeMessage"> {
  return {
    id: "message-1",
    tender_id: "tender-a",
    root_run_id: "run-1",
    sender: {
      id: "staff-1",
      version: 2,
      display_name: "Nour Selim",
      title: "Document Controller",
      kind: "staff",
      assignment_id: "assignment-1",
    },
    recipients: [
      {
        id: "manager-1",
        version: 1,
        display_name: "Tender Manager",
        title: "Tender Manager",
        kind: "manager",
        assignment_id: null,
      },
    ],
    assignment_id: "assignment-1",
    kind: "question",
    text: "The drawing revision needs a decision before I finish the register.",
    artifact_refs: [
      {
        kind: "source",
        id: "source-7",
        artifact_id: "artifact-7",
        artifact_version: 3,
        content_hash: "hash-7",
        locator: "page 4",
        filename: "scope.pdf",
      },
    ],
    reply_to: null,
    created_at: "2026-09-10T08:30:00Z",
    ...overrides,
  };
}

function renderMessages(api: Api, items: Schema<"OfficeMessage">[]) {
  return render(
    <ApiContext.Provider value={api}>
      <OfficeMessages
        tenderId="tender-a"
        page={{ items, next_cursor: null }}
        onSource={vi.fn()}
      />
    </ApiContext.Provider>,
  );
}

describe("OfficeMessages", () => {
  it("shows actual speakers and important exchanges while keeping notes expandable", async () => {
    const note = message({
      id: "note-1",
      kind: "note",
      text: "A source read was saved.",
    });
    const view = renderMessages({ get: vi.fn() } as unknown as Api, [
      message(),
      note,
    ]);

    expect(screen.getAllByText("Nour Selim")).not.toHaveLength(0);
    expect(screen.getAllByText(/Document Controller · v2/)).not.toHaveLength(0);
    expect(
      screen.getByText(/The drawing revision needs a decision/),
    ).toBeInTheDocument();
    expect(screen.getByText("1 routine note")).toBeInTheDocument();
    expect(screen.queryByText("A source read was saved.")).not.toBeVisible();

    await userEvent.click(screen.getByText("1 routine note"));
    expect(screen.getByText("A source read was saved.")).toBeVisible();
    view.unmount();
  });

  it("loads retained history through the documented paged endpoint and opens exact sources", async () => {
    const onSource = vi.fn();
    const older = message({
      id: "message-older",
      created_at: "2026-09-09T08:30:00Z",
      text: "Earlier handoff.",
    });
    const api = {
      get: vi.fn().mockResolvedValue({ items: [older], next_cursor: null }),
    } as unknown as Api;
    const view = render(
      <ApiContext.Provider value={api}>
        <OfficeMessages
          tenderId="tender-a"
          page={{ items: [message()], next_cursor: "cursor-1" }}
          onSource={onSource}
        />
      </ApiContext.Provider>,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Load earlier exchanges" }),
    );
    expect(await screen.findByText("Earlier handoff.")).toBeInTheDocument();
    expect(api.get).toHaveBeenCalledWith(
      "/tenders/tender-a/office/messages?limit=50&cursor=cursor-1",
      expect.any(AbortSignal),
    );

    view.rerender(
      <ApiContext.Provider value={api}>
        <OfficeMessages
          tenderId="tender-a"
          page={{
            items: [
              message({ id: "message-new", text: "A newer saved reply." }),
            ],
            next_cursor: null,
          }}
          onSource={onSource}
        />
      </ApiContext.Provider>,
    );
    expect(screen.getByText("Earlier handoff.")).toBeInTheDocument();
    expect(screen.getByText("A newer saved reply.")).toBeInTheDocument();

    await userEvent.click(
      screen.getAllByRole("button", { name: /scope\.pdf/ })[0],
    );
    expect(onSource).toHaveBeenCalledWith(
      expect.objectContaining({
        sourceId: "source-7",
        artifactId: "artifact-7",
        version: 3,
        page: 4,
      }),
    );
  });

  it("shows the durable delivery state and required reply from the server", async () => {
    const api = {
      get: vi.fn(),
      post: vi.fn().mockResolvedValue({
        message_id: "message-1",
        state: "acknowledged",
        extent: "received",
        required_response: "Confirm the curing time.",
        at: "2026-09-10T08:35:00Z",
      }),
    } as unknown as Api;
    renderMessages(api, [
      message({
        delivery_state: "acknowledged",
        required_response: "Confirm the curing time.",
      }),
    ]);

    expect(
      screen.getByText("Needs a reply: Confirm the curing time."),
    ).toBeInTheDocument();
    const received = screen.getByRole("button", { name: "Received" });
    expect(received).toBeDisabled();
    expect(api.post).not.toHaveBeenCalled();
  });

  it("marks a delivered message received and keeps the server reply", async () => {
    const api = {
      get: vi.fn(),
      post: vi.fn().mockResolvedValue({
        message_id: "message-1",
        state: "acknowledged",
        extent: "received",
        required_response: null,
        at: "2026-09-10T08:35:00Z",
      }),
    } as unknown as Api;
    renderMessages(api, [message({ delivery_state: "delivered" })]);

    await userEvent.click(
      screen.getByRole("button", { name: "Mark received" }),
    );
    expect(api.post).toHaveBeenCalledWith(
      "/tenders/tender-a/office/messages/message-1/acknowledge",
      expect.objectContaining({ message_id: "message-1", extent: "received" }),
    );
    expect(
      await screen.findByRole("button", { name: "Received" }),
    ).toBeInTheDocument();
  });
});
