import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import splashSource from "../splash.html?raw";

type EventHandler = (event: { payload: unknown }) => void;

const tauri = vi.hoisted(() => ({
  handlers: new Map<string, EventHandler>(),
  invoke: vi.fn(),
  listen: vi.fn(
    async (event: string, handler: EventHandler): Promise<() => void> => {
      tauri.handlers.set(event, handler);
      return () => tauri.handlers.delete(event);
    },
  ),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

const splashBody = splashSource.match(/<body>([\s\S]+)<\/body>/i)?.[1] ?? "";

function renderRealSplashMarkup() {
  document.body.innerHTML = splashBody;
}

describe("transparent Evidence Calibration splash", () => {
  let animate: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.resetModules();
    vi.useFakeTimers();
    tauri.handlers.clear();
    tauri.invoke.mockImplementation(async (command: string) =>
      command === "inspect_startup_display_ready" ? false : undefined,
    );
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn(() => ({ matches: false })),
    });
    renderRealSplashMarkup();
    animate = vi.fn(() => ({ cancel: vi.fn() }) as unknown as Animation);
    Object.defineProperty(Element.prototype, "animate", {
      configurable: true,
      value: animate,
    });
  });

  afterEach(() => {
    document.body.innerHTML = "";
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("uses the real splash markup: ten paired evidence cells and one datum", () => {
    const cells = document.querySelectorAll<SVGGElement>(
      "[id^='quantix-cell-']",
    );
    expect(cells).toHaveLength(10);
    for (const cell of cells) {
      expect(
        cell.querySelector(".quantix-evidence-cell__counter"),
      ).not.toBeNull();
      expect(cell.querySelector(".quantix-evidence-cell__face")).not.toBeNull();
    }
    expect(document.querySelectorAll("#quantix-authority-datum")).toHaveLength(
      1,
    );
    expect(document.querySelector("[role='status']")).toBeNull();
  });

  it("plays a brief assembly before handing off when startup is ready early", async () => {
    await import("./splash");
    await vi.waitFor(() => {
      expect(tauri.handlers.has("quantix-startup-ready")).toBe(true);
    });

    expect(animate).toHaveBeenCalledTimes(11);
    expect(animate.mock.calls[0]?.[1]).toMatchObject({ duration: 900 });

    tauri.handlers.get("quantix-startup-ready")?.({ payload: undefined });
    await vi.advanceTimersByTimeAsync(959);
    expect(tauri.invoke).not.toHaveBeenCalledWith("finish_startup_splash");

    await vi.advanceTimersByTimeAsync(1);
    expect(animate).toHaveBeenCalledTimes(12);
    expect(animate.mock.calls[11]?.[1]).toMatchObject({ duration: 120 });
    await vi.advanceTimersByTimeAsync(120);
    expect(tauri.invoke).toHaveBeenCalledWith("finish_startup_splash");
  });

  it("uses the locked arrival, calibration, and datum motion cues", async () => {
    await import("./splash");

    const firstCellFrames = animate.mock.calls[0]?.[0] as Keyframe[];
    const datumFrames = animate.mock.calls[10]?.[0] as Keyframe[];
    expect(animate).toHaveBeenCalledTimes(11);
    expect(firstCellFrames[1]).toMatchObject({
      offset: 200 / 6250,
      opacity: 0,
      transform: "translate(-12px, -4px)",
      easing: "cubic-bezier(0.22, 1, 0.36, 1)",
    });
    expect(firstCellFrames[3]).toMatchObject({
      offset: 3925 / 6250,
      transform: "translate(-1.2px, -0.6px)",
      easing: "cubic-bezier(0.33, 1, 0.68, 1)",
    });
    expect(firstCellFrames[firstCellFrames.length - 1]).toMatchObject({
      offset: 1,
      opacity: 1,
      transform: "translate(0px, 0px)",
    });
    expect(datumFrames[0]).toMatchObject({
      offset: 0,
      opacity: 0,
      transform: "translate(-12px, -6.37px)",
    });
    expect(datumFrames[1]).toMatchObject({
      offset: 4550 / 6250,
      opacity: 0,
      easing: "cubic-bezier(0.22, 1, 0.36, 1)",
    });
    expect(datumFrames[2]).toMatchObject({
      offset: 5670 / 6250,
      transform: "translate(1.225px, 0.65px)",
      easing: "cubic-bezier(0.33, 1, 0.68, 1)",
    });
    expect(datumFrames[3]).toMatchObject({
      offset: 6020 / 6250,
      transform: "translate(-0.35px, -0.186px)",
    });
  });

  it("holds the finished logo until late readiness, then fades only once", async () => {
    await import("./splash");
    await vi.waitFor(() => {
      expect(tauri.handlers.has("quantix-startup-ready")).toBe(true);
    });

    await vi.advanceTimersByTimeAsync(1080);
    expect(tauri.invoke).not.toHaveBeenCalledWith("finish_startup_splash");

    tauri.handlers.get("quantix-startup-ready")?.({ payload: undefined });
    expect(animate).toHaveBeenCalledTimes(12);
    await vi.advanceTimersByTimeAsync(120);
    expect(
      tauri.invoke.mock.calls.filter(
        ([command]) => command === "finish_startup_splash",
      ),
    ).toHaveLength(1);

    tauri.handlers.get("quantix-startup-ready")?.({ payload: undefined });
    await vi.advanceTimersByTimeAsync(120);
    expect(
      tauri.invoke.mock.calls.filter(
        ([command]) => command === "finish_startup_splash",
      ),
    ).toHaveLength(1);
  });

  it("switches to the final static mark for reduced motion", async () => {
    await import("./splash");
    await vi.waitFor(() => {
      expect(tauri.handlers.has("quantix-startup-preferences")).toBe(true);
    });

    tauri.handlers.get("quantix-startup-preferences")?.({
      payload: { reducedMotion: true },
    });

    tauri.handlers.get("quantix-startup-ready")?.({ payload: undefined });
    await vi.advanceTimersByTimeAsync(0);
    expect(tauri.invoke).toHaveBeenCalledWith("finish_startup_splash");
    expect(animate).toHaveBeenCalledTimes(11);
  });

  it("renders no animation when system reduced motion is enabled at launch", async () => {
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn(() => ({ matches: true })),
    });
    await import("./splash");
    await vi.waitFor(() => {
      expect(tauri.handlers.has("quantix-startup-ready")).toBe(true);
    });

    expect(animate).not.toHaveBeenCalled();
    tauri.handlers.get("quantix-startup-ready")?.({ payload: undefined });
    await vi.advanceTimersByTimeAsync(0);
    expect(tauri.invoke).toHaveBeenCalledWith("finish_startup_splash");
  });

  it("keeps native recovery available for an incomplete renderer", async () => {
    document.body.innerHTML = "<main id='splash-root'></main>";
    await expect(import("./splash")).resolves.toBeDefined();
    await vi.advanceTimersByTimeAsync(1000);
    expect(tauri.invoke).not.toHaveBeenCalledWith("finish_startup_splash");
  });

  it("keeps native recovery available when Web Animations is unavailable", async () => {
    Object.defineProperty(Element.prototype, "animate", {
      configurable: true,
      value: undefined,
    });
    await expect(import("./splash")).resolves.toBeDefined();
    expect(tauri.invoke).not.toHaveBeenCalledWith("finish_startup_splash");
  });
});
