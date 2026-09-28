import { describe, expect, it } from "vitest";
import { behaveLikeAnApp } from "./desktop";

function rightClick(target: Element) {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

describe("behaveLikeAnApp", () => {
  document.body.innerHTML = `<nav><button>Estimate</button></nav><p>Rate 453.13</p><input aria-label="Message" />`;
  behaveLikeAnApp(document);

  it("keeps the browser menu off the interface", () => {
    expect(rightClick(document.querySelector("button")!)).toBe(true);
    expect(rightClick(document.querySelector("p")!)).toBe(true);
  });

  it("leaves Cut, Copy and Paste in text fields", () => {
    expect(rightClick(document.querySelector("input")!)).toBe(false);
  });

  it("leaves Copy on text the engineer selected", () => {
    document.getSelection()!.selectAllChildren(document.querySelector("p")!);
    expect(rightClick(document.querySelector("p")!)).toBe(false);
    document.getSelection()!.removeAllRanges();
  });
});
