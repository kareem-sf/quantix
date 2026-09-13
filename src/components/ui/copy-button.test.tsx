import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CopyButton } from "./copy-button";

it("copies the full value and confirms only after the clipboard write succeeds", async () => {
  const user = userEvent.setup();
  let finish!: () => void;
  const write = vi.spyOn(navigator.clipboard, "writeText").mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  render(<CopyButton value="full-file-hash-0123456789" />);
  const button = screen.getByRole("button", { name: "Copy Hash" });
  await user.hover(button);
  expect(screen.queryByText("Copied")).not.toBeInTheDocument();
  await user.click(button);
  expect(write).toHaveBeenCalledWith("full-file-hash-0123456789");
  expect(screen.getByRole("button", { name: "Copying…" })).toBeDisabled();
  await act(async () => finish());
  expect(screen.getByRole("button", { name: "Copied" })).toBeEnabled();
  expect(screen.getByRole("status")).toHaveTextContent("Copied to clipboard.");
});

it("supports keyboard copying without submitting its enclosing form", async () => {
  const user = userEvent.setup();
  const submit = vi.fn((event: React.FormEvent) => event.preventDefault());
  const write = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  render(
    <form onSubmit={submit}>
      <CopyButton value="hash" />
    </form>,
  );
  await user.tab();
  await user.keyboard("{Enter}");
  expect(write).toHaveBeenCalledWith("hash");
  expect(submit).not.toHaveBeenCalled();
});

it("provides a selectable value on failure and allows a retry", async () => {
  const user = userEvent.setup();
  const write = vi
    .spyOn(navigator.clipboard, "writeText")
    .mockRejectedValueOnce(new DOMException("Denied", "NotAllowedError"))
    .mockResolvedValue();
  render(<CopyButton value="original-full-hash" />);
  await user.click(screen.getByRole("button", { name: "Copy Hash" }));
  expect(screen.getByRole("alert")).toHaveTextContent("Could not copy");
  expect(
    screen.getByRole("textbox", { name: "Value to copy manually" }),
  ).toHaveValue("original-full-hash");
  expect(screen.queryByText("Copied")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Copy Hash" }));
  expect(write).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument();
});

it("handles unavailable clipboard access", async () => {
  const user = userEvent.setup();
  const descriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard")!;
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: undefined,
  });
  try {
    render(<CopyButton value="manual-hash" />);
    await user.click(screen.getByRole("button", { name: "Copy Hash" }));
    expect(screen.getByRole("textbox")).toHaveValue("manual-hash");
  } finally {
    Object.defineProperty(navigator, "clipboard", descriptor);
  }
});

it("clears confirmation when the source changes, including a pending write", async () => {
  const user = userEvent.setup();
  let finish!: () => void;
  vi.spyOn(navigator.clipboard, "writeText").mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  );
  const view = render(<CopyButton value="old-hash" />);
  await user.click(screen.getByRole("button", { name: "Copy Hash" }));
  view.rerender(<CopyButton value="new-hash" />);
  await act(async () => finish());
  expect(screen.getByRole("button", { name: "Copy Hash" })).toBeEnabled();
  expect(screen.queryByText("Copied")).not.toBeInTheDocument();
});

it("returns to the copy action after showing confirmation", async () => {
  const user = userEvent.setup();
  vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  render(<CopyButton value="hash" />);
  await user.click(screen.getByRole("button", { name: "Copy Hash" }));
  expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument();
  await waitFor(
    () =>
      expect(
        screen.getByRole("button", { name: "Copy Hash" }),
      ).toBeInTheDocument(),
    { timeout: 3000 },
  );
});
