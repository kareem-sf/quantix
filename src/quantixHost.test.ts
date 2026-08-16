import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import { inspectCurrentPublicReleaseGate } from "./quantixHost";

describe("Quantix Host release commands", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("passes the release candidate manifest with the Tauri command argument name", async () => {
    invokeMock.mockResolvedValue(null);

    await inspectCurrentPublicReleaseGate("a".repeat(64));

    expect(invokeMock).toHaveBeenCalledWith(
      "inspect_current_public_release_gate",
      { releaseCandidateManifestSha256: "a".repeat(64) },
    );
  });
});
