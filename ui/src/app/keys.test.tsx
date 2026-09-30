import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useEscape } from "./keys";

function Panel({ onClose }: { onClose: () => void }) {
  useEscape(onClose);
  return (
    <div>
      <button>Approve</button>
      <input aria-label="Reason" />
      <textarea aria-label="What to put right" />
      <select aria-label="Kind">
        <option>Labour</option>
      </select>
      <div role="textbox" aria-label="Note" contentEditable suppressContentEditableWarning />
    </div>
  );
}

describe("Esc", () => {
  it("closes what is open, and no other key does", () => {
    const close = vi.fn();
    render(<Panel onClose={close} />);

    fireEvent.keyDown(screen.getByRole("button", { name: "Approve" }), { key: "Enter" });
    expect(close).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getByRole("button", { name: "Approve" }), { key: "Escape" });
    expect(close).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(close).toHaveBeenCalledTimes(2);
  });

  it("leaves the engineer's typing alone", () => {
    const close = vi.fn();
    render(<Panel onClose={close} />);

    for (const name of ["Reason", "What to put right", "Kind", "Note"]) {
      fireEvent.keyDown(screen.getByLabelText(name), { key: "Escape" });
    }
    expect(close).not.toHaveBeenCalled();
  });

  it("closes what is open now, and nothing once it has gone", () => {
    const first = vi.fn();
    const second = vi.fn();
    const { rerender, unmount } = render(<Panel onClose={first} />);

    rerender(<Panel onClose={second} />);
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);

    unmount();
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(second).toHaveBeenCalledTimes(1);
  });
});
