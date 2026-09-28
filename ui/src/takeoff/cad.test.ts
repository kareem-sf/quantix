import { describe, expect, it } from "vitest";
import { screenCopy } from "../test/drawing";
import { parseScreen, pick } from "./cad";

describe("A drawing's screen copy", () => {
  it("reads the packed segments, objects and texts", () => {
    const copy = parseScreen(screenCopy());
    expect(copy.header.objects).toBe(3);
    expect([...copy.segments]).toEqual([-10, 0, 10, 0, -2, 6, 2, 6]);
    expect([...copy.owner]).toEqual([0, 1]);
    expect(copy.meta[3]).toBe(1); // the door's layer
    expect(copy.header.texts[0][1]).toBe("KITCHEN");
  });

  it("picks the object nearest a click, and skips hidden layers", () => {
    const copy = parseScreen(screenCopy());
    expect(pick(copy, new Set(), 3, 0.2, 0.5)).toBe(0);
    expect(pick(copy, new Set(), 0, 5.8, 0.5)).toBe(1);
    expect(pick(copy, new Set([1]), 0, 5.8, 0.5)).toBeNull();
    expect(pick(copy, new Set(), 0, 3, 0.5)).toBeNull();
  });
});
