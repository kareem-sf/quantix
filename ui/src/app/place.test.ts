import { afterEach, describe, expect, it, vi } from "vitest";
import { lastTender, placeIn, remember, store, stored } from "./place";

afterEach(() => vi.restoreAllMocks());

describe("where the engineer was", () => {
  it("reopens the last tender, each on the screen last used in it", () => {
    remember("/tenders/t1/estimate?item=i1");
    remember("/tenders/t2/documents");

    expect(lastTender()).toBe("t2");
    expect(placeIn("t1")).toBe("/tenders/t1/estimate?item=i1");
    expect(placeIn("t2")).toBe("/tenders/t2/documents");
    expect(placeIn("t3")).toBe("/tenders/t3"); // never opened: its Overview
  });

  it("doesn't return to a decision page, and a firm screen leaves the tender as it was", () => {
    remember("/tenders/t1/takeoff");
    remember("/tenders/t1/decisions/q1");
    remember("/library");

    expect(lastTender()).toBe("t1");
    expect(placeIn("t1")).toBe("/tenders/t1/takeoff");
  });

  it("starts from the Overview when what was kept can't be read", () => {
    localStorage.setItem("quantix.place", "{not json");
    expect(lastTender()).toBeUndefined();
    expect(placeIn("t1")).toBe("/tenders/t1");

    localStorage.setItem("quantix.place", JSON.stringify({ last: "t1" }));
    expect(lastTender()).toBeUndefined();
    remember("/tenders/t1/submission");
    expect(placeIn("t1")).toBe("/tenders/t1/submission");
  });

  it("keeps working when the window can't store anything", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("The quota has been exceeded.", "QuotaExceededError");
    });

    expect(() => remember("/tenders/t1/estimate")).not.toThrow();
    expect(() => store("folded", true)).not.toThrow();
    expect(lastTender()).toBeUndefined();
    expect(stored("folded", false)).toBe(false);
  });
});

describe("the window's own settings", () => {
  it("keeps each under its own key, with the default until one is set", () => {
    expect(stored("sidebarWidth", 224)).toBe(224);
    store("sidebarWidth", 300);
    store("folded", true);

    expect(stored("sidebarWidth", 224)).toBe(300);
    expect(stored("folded", false)).toBe(true);
    expect(stored("teamWidth", 400)).toBe(400);
  });

  it("uses the default when a setting can't be read", () => {
    localStorage.setItem("quantix.teamWidth", "wide");
    expect(stored("teamWidth", 400)).toBe(400);
  });
});
