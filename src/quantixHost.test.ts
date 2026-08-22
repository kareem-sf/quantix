import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import {
  cancelChatGptLogin,
  disconnectChatGpt,
  inspectCurrentPublicReleaseGate,
  startChatGptLogin,
} from "./quantixHost";

describe("Quantix host ChatGPT login commands", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("starts the ChatGPT login without a payload", async () => {
    invokeMock.mockResolvedValue({ status: "awaiting_browser" });

    await startChatGptLogin();

    expect(invokeMock).toHaveBeenCalledWith("start_chatgpt_login");
  });

  it("cancels the ChatGPT login without a payload", async () => {
    invokeMock.mockResolvedValue(undefined);

    await cancelChatGptLogin();

    expect(invokeMock).toHaveBeenCalledWith("cancel_chatgpt_login");
  });

  it("disconnects ChatGPT and returns the updated settings view", async () => {
    const settings = { chatgpt: { state: "absent" } };
    invokeMock.mockResolvedValue(settings);

    await disconnectChatGpt();

    expect(invokeMock).toHaveBeenCalledWith("disconnect_chatgpt");
    expect(await disconnectChatGpt()).toBe(settings);
  });
});

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
