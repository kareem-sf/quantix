import { describe, expect, it } from "vitest";
import { behaveLikeAnApp } from "./desktop";

function rightClick(target: Element) {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

describe("behaveLikeAnApp", () => {
  document.body.innerHTML = `<nav><button>Estimate</button></nav><p>Rate 453.13</p><input aria-label="Message" />
    <a href="https://www.iana.org/prices" target="_blank">Web price</a><a href="/tenders/t1" target="_blank">Quantix</a>`;
  behaveLikeAnApp(document, true);

  it("keeps the browser menu off the interface", () => {
    expect(rightClick(document.querySelector("button")!)).toBe(true);
    expect(rightClick(document.querySelector("p")!)).toBe(true);
  });

  it("leaves Cut, Copy and Paste in text fields", () => {
    expect(rightClick(document.querySelector("input")!)).toBe(false);
  });

  it("sends a new-tab web link the way every link outside Quantix goes, to the engineer's browser", () => {
    const click = (name: string) => {
      const event = new MouseEvent("click", { bubbles: true, cancelable: true });
      [...document.querySelectorAll("a")].find((a) => a.textContent === name)!.dispatchEvent(event);
      return event.defaultPrevented;
    };
    expect(click("Web price")).toBe(true); // not a new window: the page asks to go there, and the window hands it on
    expect(click("Quantix")).toBe(false); // Quantix's own pages are left alone
  });

  it("leaves a click on anything but a new-tab link alone", () => {
    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    document.querySelector("button")!.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });

  it("leaves Copy on text the engineer selected", () => {
    document.getSelection()!.selectAllChildren(document.querySelector("p")!);
    expect(rightClick(document.querySelector("p")!)).toBe(false);
    document.getSelection()!.removeAllRanges();
  });
});

describe("behaveLikeAnApp in the browser preview", () => {
  it("lets a new-tab link open a tab, as a browser does", () => {
    const page = document.implementation.createHTMLDocument();
    page.body.innerHTML = `<a href="https://www.iana.org/prices" target="_blank">Web price</a>`;
    behaveLikeAnApp(page, false);

    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    page.querySelector("a")!.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });
});
