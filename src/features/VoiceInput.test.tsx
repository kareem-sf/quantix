import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";
import { VoiceInput } from "./VoiceInput";

it("keeps a transcript as unsent draft copy", async () => {
  const onTranscript = vi.fn();
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: {
      getUserMedia: vi.fn().mockResolvedValue({
        getTracks: () => [{ stop: vi.fn() }],
      }),
    },
  });
  render(<VoiceInput onTranscript={onTranscript} />);
  expect(
    screen.getByText(/do not approve work/i),
  ).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Push to talk" }));
  expect(await screen.findByRole("status")).toHaveTextContent("Recording");
});

it("uploads nothing when the microphone is denied", async () => {
  const onTranscript = vi.fn();
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: {
      getUserMedia: vi.fn().mockRejectedValue(new Error("denied")),
    },
  });
  render(<VoiceInput onTranscript={onTranscript} />);
  await userEvent.click(screen.getByRole("button", { name: "Push to talk" }));
  expect(
    await screen.findByText(/Microphone permission was denied/i),
  ).toBeInTheDocument();
  expect(onTranscript).not.toHaveBeenCalled();
});
