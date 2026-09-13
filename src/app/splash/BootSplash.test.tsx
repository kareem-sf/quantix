import { act, render } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { BootSplash } from "./BootSplash";

const native = vi.hoisted(() => ({ desktop: true, invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => native.desktop,
  invoke: native.invoke,
}));

afterEach(() => {
  native.desktop = true;
  native.invoke.mockReset();
});

it("finishes the desktop splash only once the first screen is ready", async () => {
  let reveal = () => undefined as void;
  native.invoke.mockImplementation(
    () => new Promise<void>((resolve) => (reveal = resolve)),
  );
  const onLift = vi.fn();
  const onDone = vi.fn();
  const { rerender } = render(
    <BootSplash ready={false} onLift={onLift} onDone={onDone} />,
  );
  expect(native.invoke).not.toHaveBeenCalled();

  rerender(<BootSplash ready onLift={onLift} onDone={onDone} />);
  expect(native.invoke).toHaveBeenCalledWith("finish_splash");
  expect(onDone).not.toHaveBeenCalled();

  await act(async () => reveal());
  expect(onLift).toHaveBeenCalledOnce();
  expect(onDone).toHaveBeenCalledOnce();
});

it("finishes at once in a browser, where there is no splash", async () => {
  native.desktop = false;
  const onDone = vi.fn();
  await act(async () => {
    render(<BootSplash ready={false} onDone={onDone} />);
  });
  expect(native.invoke).not.toHaveBeenCalled();
  expect(onDone).toHaveBeenCalledOnce();
});
