import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import * as transport from "./api";
import { useServiceConnection } from "./useServiceConnection";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

it("recovers from the native connection-file gap without showing a permanent failure", async () => {
  const api = transport.createApi({
    base_url: "http://localhost/api",
    token: "test",
  });
  const connect = vi
    .spyOn(transport, "connect")
    .mockRejectedValueOnce(
      new Error(
        "The local workspace is starting or unavailable. Reopen Quantix.",
      ),
    )
    .mockRejectedValueOnce(
      new Error("The local connection is incomplete. Reopen Quantix."),
    )
    .mockResolvedValue(api);
  const view = renderHook(() => useServiceConnection());
  await act(async () => {
    await vi.advanceTimersByTimeAsync(4000);
  });
  expect(connect).toHaveBeenCalledTimes(3);
  expect(view.result.current.api).toBe(api);
  expect(view.result.current.error).toBeNull();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(30000);
  });
  expect(connect).toHaveBeenCalledTimes(3);
});

it("shows a useful error for a lasting outage and still reconnects when the service returns", async () => {
  const api = transport.createApi({
    base_url: "http://localhost/api",
    token: "test",
  });
  const connect = vi
    .spyOn(transport, "connect")
    .mockRejectedValue(new Error("The service is starting."));
  const view = renderHook(() => useServiceConnection());
  await act(async () => {
    await vi.advanceTimersByTimeAsync(25000);
  });
  expect(view.result.current.error).toBeInstanceOf(Error);
  connect.mockResolvedValue(api);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(5000);
  });
  expect(view.result.current.api).toBe(api);
  expect(view.result.current.error).toBeNull();
});

it("cancels in-flight startup and clears retries when the screen unmounts", async () => {
  const connect = vi
    .spyOn(transport, "connect")
    .mockRejectedValue(new Error("Starting"));
  const view = renderHook(() => useServiceConnection());
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
  const signal = connect.mock.calls[0][0];
  view.unmount();
  expect(signal?.aborted).toBe(true);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(30000);
  });
  expect(connect).toHaveBeenCalledTimes(1);
});

it("manual retry cancels the previous attempt and retries immediately", async () => {
  const api = transport.createApi({
    base_url: "http://localhost/api",
    token: "test",
  });
  const connect = vi
    .spyOn(transport, "connect")
    .mockRejectedValueOnce(new Error("Starting"))
    .mockResolvedValue(api);
  const view = renderHook(() => useServiceConnection());
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
  const previousSignal = connect.mock.calls[0][0];
  await act(async () => view.result.current.retry());
  expect(previousSignal?.aborted).toBe(true);
  expect(connect).toHaveBeenCalledTimes(2);
  expect(view.result.current.api).toBe(api);
});
