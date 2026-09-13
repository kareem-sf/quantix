import { act, renderHook } from "@testing-library/react";
import { vi } from "vitest";
import { useTrayCurrentWork } from "./useTrayCurrentWork";

const mocks = vi.hoisted(() => ({
  listen: vi.fn(),
  nativeDesktop: vi.fn(() => true),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("../../api", () => ({ nativeDesktop: mocks.nativeDesktop }));

beforeEach(() => {
  mocks.listen.mockReset();
  mocks.nativeDesktop.mockReturnValue(true);
});

it("navigates to the selected Tender without starting work and releases late listeners", async () => {
  let release!: (remove: () => void) => void;
  const remove = vi.fn();
  mocks.listen.mockReturnValue(
    new Promise<() => void>((resolve) => {
      release = resolve;
    }),
  );
  const navigate = vi.fn();
  const view = renderHook(() => useTrayCurrentWork("tender one", navigate));
  expect(mocks.listen.mock.calls[0][0]).toBe("quantix://tray/current-work");
  const handler = mocks.listen.mock.calls[0][1];
  act(() => handler());
  expect(navigate).toHaveBeenCalledWith("/tenders/tender%20one/work");
  view.unmount();
  await act(async () => release(remove));
  expect(remove).toHaveBeenCalledOnce();
  handler();
  expect(navigate).toHaveBeenCalledTimes(1);
});

it("exposes an actual registration failure and makes no native call in a browser", async () => {
  mocks.listen.mockRejectedValue(new Error("Synthetic listener failure"));
  const navigate = vi.fn();
  const view = renderHook(() => useTrayCurrentWork("one", navigate));
  await act(async () => {});
  expect(view.result.current?.message).toContain("Open Work");
  view.unmount();
  mocks.listen.mockClear();
  mocks.nativeDesktop.mockReturnValue(false);
  renderHook(() => useTrayCurrentWork("one", navigate));
  expect(mocks.listen).not.toHaveBeenCalled();
});
