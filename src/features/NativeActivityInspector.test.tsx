import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { ApiContext, type Api, type Schema } from "../api";
import { NativeActivityInspector } from "./NativeActivityInspector";

const session: Schema<"NativeSessionBinding"> = {
  id: "session-1",
  tender_id: "tender-a",
  run_id: "run-a",
  protocol: "codex",
  connection_id: "connection-a",
  connection_revision: 4,
  model_id: "gpt-5.6-sol",
  runtime_revision: "codex-cli-1.2.3",
  profile_id: "staff-a",
  profile_version: 2,
  scope_fingerprint: "a".repeat(64),
  settings_fingerprint: "b".repeat(64),
  provider_session_id: "provider-session-a",
  state: "completed",
};

const artifact: Schema<"NativeArtifact"> = {
  id: "artifact-a",
  tender_id: "tender-a",
  run_id: "run-a",
  filename: "analysis.xlsx",
  size_bytes: 2048,
  content_hash: "c".repeat(64),
  origin: "provider_code",
  status: "unreviewed",
  input_artifacts: [
    { artifact_id: "source-a", version: 3, content_hash: "d".repeat(64) },
  ],
  actor_id: "staff-a",
  assignment_id: "assignment-a",
  connection_id: "connection-a",
  model_id: "gpt-5.6-sol",
};

const cleanup: Schema<"NativeCleanupReceipt"> = {
  id: "cleanup-a",
  run_id: "run-a",
  connection_id: "connection-a",
  connection_revision: 4,
  kind: "container",
  state: "uncertain",
  created_at: "2026-09-13T08:00:00Z",
};

it("loads recorded sessions and files only when opened, downloads through the API, and explicitly reviews cleanup", async () => {
  const user = userEvent.setup();
  const createObjectURL = vi
    .spyOn(URL, "createObjectURL")
    .mockReturnValue("blob:test");
  const revokeObjectURL = vi
    .spyOn(URL, "revokeObjectURL")
    .mockImplementation(() => undefined);
  const api = {
    get: vi.fn(async (path: string) => {
      if (path.endsWith("/native-sessions")) return [session];
      if (path.endsWith("/native-artifacts")) return [artifact];
      if (path.endsWith("/native-cleanup")) return [cleanup];
      if (path.endsWith("/native-capabilities"))
        return [
          {
            id: "resume",
            origin: "client",
            supported: true,
            detail: "Resume supported.",
            source: "codex --help",
          },
        ];
      throw new Error(`Unexpected GET ${path}`);
    }),
    post: vi.fn(async () => ({ ok: true })),
    blob: vi.fn(async () => new Blob(["draft"])),
  } as unknown as Api;
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(() => undefined);
  try {
    render(
      <ApiContext.Provider value={api}>
        <NativeActivityInspector tenderId="tender-a" runId="run-a" />
      </ApiContext.Provider>,
    );
    expect(api.get).not.toHaveBeenCalled();
    await user.click(screen.getByText("Provider sessions and generated files"));

    expect(await screen.findByText("Codex client · gpt-5.6-sol")).toBeVisible();
    expect(screen.getByText("codex-cli-1.2.3")).toBeVisible();
    expect(screen.getByText("analysis.xlsx")).toBeVisible();
    expect(screen.getByText("Unreviewed provider draft")).toBeVisible();
    expect(screen.getByText(artifact.content_hash)).toBeVisible();
    expect(
      screen.getByText(/does not release or adjust spending holds/i),
    ).toBeVisible();

    await user.click(
      screen.getByRole("button", { name: "Download analysis.xlsx" }),
    );
    expect(api.blob).toHaveBeenCalledWith(
      "/tenders/tender-a/native-artifacts/artifact-a/content",
    );
    expect(createObjectURL).toHaveBeenCalled();
    expect(click).toHaveBeenCalled();

    const cleanupPanel = screen.getByRole("group", {
      name: "Container cleanup review",
    });
    const submit = within(cleanupPanel).getByRole("button", {
      name: "Record cleanup review",
    });
    expect(submit).toBeDisabled();
    await user.type(
      within(cleanupPanel).getByLabelText("Review rationale"),
      "Confirmed in the provider client.",
    );
    await user.click(
      within(cleanupPanel).getByLabelText(
        "I checked this cleanup in the provider client",
      ),
    );
    await user.click(submit);
    await waitFor(() =>
      expect(api.post).toHaveBeenCalledWith(
        "/tenders/tender-a/native-cleanup/cleanup-a/review",
        {
          engineer_confirmed: true,
          rationale: "Confirmed in the provider client.",
        },
      ),
    );
  } finally {
    createObjectURL.mockRestore();
    revokeObjectURL.mockRestore();
    click.mockRestore();
  }
});

it("shows an honest empty state without suggesting native activity", async () => {
  const api = {
    get: vi.fn(async () => []),
  } as unknown as Api;
  render(
    <ApiContext.Provider value={api}>
      <NativeActivityInspector tenderId="tender-a" runId="run-a" />
    </ApiContext.Provider>,
  );
  await userEvent.click(
    screen.getByText("Provider sessions and generated files"),
  );
  expect(
    await screen.findByText(
      "No provider sessions or generated files are recorded for this work.",
    ),
  ).toBeVisible();
});
