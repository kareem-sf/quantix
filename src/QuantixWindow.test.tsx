import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { QuantixWindow } from "./QuantixWindow";
import { quantixSmoothEase } from "./motion/motionPresets";
import type { WindowTitleBarMenu } from "./WindowTitleBar";

const menus: readonly WindowTitleBarMenu[] = [];
const defaultMatchMedia = window.matchMedia;

interface MutableMediaQueryList extends MediaQueryList {
  setMatches(matches: boolean): void;
}

function installMatchMedia({ fine = true, reduce = false } = {}) {
  const lists = new Map<string, MutableMediaQueryList>();
  const matchMedia = vi.fn((query: string) => {
    const eventTarget = new EventTarget();
    let matches = query === "(pointer: fine)" ? fine : reduce;
    const list = {
      get matches() {
        return matches;
      },
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: eventTarget.addEventListener.bind(eventTarget),
      removeEventListener: eventTarget.removeEventListener.bind(eventTarget),
      dispatchEvent: eventTarget.dispatchEvent.bind(eventTarget),
      setMatches(nextMatches: boolean) {
        matches = nextMatches;
        const event = new Event("change") as MediaQueryListEvent;
        Object.defineProperties(event, {
          matches: { value: matches },
          media: { value: query },
        });
        eventTarget.dispatchEvent(event);
      },
    } as unknown as MutableMediaQueryList;
    lists.set(query, list);
    return list;
  });

  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: matchMedia,
  });
  return lists;
}

afterEach(() => {
  cleanup();
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: defaultMatchMedia,
  });
  vi.restoreAllMocks();
});

describe("QuantixWindow motion orchestration", () => {
  it("uses the bounded Smooth Ease transition for workspace layout motion", () => {
    expect(quantixSmoothEase).toEqual({
      type: "tween",
      duration: 0.22,
      ease: [0.2, 0, 0, 1],
    });
  });

  it("combines the saved and operating-system reduced-motion preferences", () => {
    const media = installMatchMedia({ fine: true, reduce: true });
    const { container, rerender } = render(
      <QuantixWindow menus={menus}>
        <main>Workspace</main>
      </QuantixWindow>,
    );
    const root = container.querySelector(".quantix-window");
    expect(root?.getAttribute("data-reduced-motion")).toBe("true");

    media.get("(prefers-reduced-motion: reduce)")?.setMatches(false);
    rerender(
      <QuantixWindow menus={menus} reducedMotion>
        <main>Workspace</main>
      </QuantixWindow>,
    );
    expect(root?.getAttribute("data-reduced-motion")).toBe("true");
  });

  it("keeps the title bar outside the transformed depth scene", () => {
    installMatchMedia();
    const { container } = render(
      <QuantixWindow menus={menus}>
        <main>Workspace</main>
      </QuantixWindow>,
    );

    const root = container.querySelector(".quantix-window");
    const titleBar = container.querySelector(".window-title-bar");
    const scene = container.querySelector(".quantix-window__content");
    expect(root?.children[0]).toBe(titleBar);
    expect(root?.children[1]).toBe(scene);
    expect(scene?.contains(titleBar)).toBe(false);
  });

  it("enables depth tracking only for a ready fine-pointer workspace", () => {
    const media = installMatchMedia();
    const { container, rerender } = render(
      <QuantixWindow menus={menus}>
        <main>Workspace</main>
      </QuantixWindow>,
    );
    const scene = container.querySelector<HTMLElement>(
      ".quantix-window__content",
    );
    expect(scene?.dataset.quantixParallax).toBe("enabled");

    media.get("(pointer: fine)")?.setMatches(false);
    expect(scene?.dataset.quantixParallax).toBe("disabled");

    rerender(
      <QuantixWindow menus={menus} motionState="static">
        <main>Workspace</main>
      </QuantixWindow>,
    );
    expect(scene?.dataset.quantixParallax).toBe("disabled");
  });

  it("updates MotionValues without re-rendering workspace children", () => {
    installMatchMedia();
    const childRender = vi.fn();
    function Workspace() {
      childRender();
      return <main>Workspace</main>;
    }

    const { container } = render(
      <QuantixWindow menus={menus}>
        <Workspace />
      </QuantixWindow>,
    );
    const scene = container.querySelector<HTMLElement>(
      ".quantix-window__content",
    );
    expect(scene).not.toBeNull();
    vi.spyOn(scene!, "getBoundingClientRect").mockReturnValue({
      bottom: 500,
      height: 500,
      left: 0,
      right: 1000,
      top: 0,
      width: 1000,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    });

    fireEvent.pointerEnter(scene!, { clientX: 500, clientY: 250 });
    fireEvent.pointerMove(scene!, { clientX: 1000, clientY: 500 });
    fireEvent.pointerLeave(scene!);
    expect(childRender).toHaveBeenCalledTimes(1);
  });
});
