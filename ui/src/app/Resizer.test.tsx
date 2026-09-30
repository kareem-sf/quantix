import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Resizer } from "./Resizer";

function edgeOf(edge: "left" | "right", width: number) {
  const onWidth = vi.fn();
  const onReset = vi.fn();
  render(<Resizer edge={edge} width={width} label="Panel width" onWidth={onWidth} onReset={onReset} />);
  return { handle: screen.getByRole("separator", { name: "Panel width" }), onWidth, onReset };
}

describe("a panel's edge", () => {
  it("widens the sidebar as its right edge is dragged right, until it is let go", () => {
    const { handle, onWidth } = edgeOf("right", 224);

    fireEvent.pointerDown(handle, { button: 0, clientX: 224 });
    expect(document.documentElement).toHaveAttribute("data-resizing");
    fireEvent.pointerMove(window, { clientX: 284 });
    expect(onWidth).toHaveBeenLastCalledWith(284);
    fireEvent.pointerMove(window, { clientX: 150 });
    expect(onWidth).toHaveBeenLastCalledWith(150); // the shell keeps the width within bounds

    fireEvent.pointerUp(window);
    expect(document.documentElement).not.toHaveAttribute("data-resizing");
    fireEvent.pointerMove(window, { clientX: 500 });
    expect(onWidth).toHaveBeenCalledTimes(2);
  });

  it("widens the team as its left edge is dragged left", () => {
    const { handle, onWidth } = edgeOf("left", 400);

    fireEvent.pointerDown(handle, { button: 0, clientX: 1000 });
    fireEvent.pointerMove(window, { clientX: 900 });
    expect(onWidth).toHaveBeenLastCalledWith(500);
    fireEvent.pointerMove(window, { clientX: 1100 });
    expect(onWidth).toHaveBeenLastCalledWith(300);
    fireEvent.pointerUp(window);
  });

  it("ignores any button but the main one", () => {
    const { handle, onWidth } = edgeOf("right", 224);

    fireEvent.pointerDown(handle, { button: 2, clientX: 224 });
    fireEvent.pointerMove(window, { clientX: 300 });
    expect(onWidth).not.toHaveBeenCalled();
    expect(document.documentElement).not.toHaveAttribute("data-resizing");
  });

  it("goes back to the usual width on a double-click", () => {
    const { handle, onReset } = edgeOf("right", 310);

    fireEvent.doubleClick(handle);
    expect(onReset).toHaveBeenCalledTimes(1);
  });
});
