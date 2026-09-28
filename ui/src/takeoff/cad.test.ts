import { describe, expect, it } from "vitest";
import { screenCopy } from "../test/drawing";
import { alike, findWords, inWindow, parseScreen, pick, snapTo } from "./cad";

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

  it("reads each object's colour", () => {
    expect([...parseScreen(screenCopy()).colours]).toEqual([0x404046, 0xff0000, 0x404046]);
  });

  it("snaps to a line's ends and middle, and else onto the line", () => {
    const copy = parseScreen(screenCopy());
    expect(snapTo(copy, new Set(), -9.8, 0.1, 0.5)).toEqual({ point: [-10, 0], kind: "End" });
    expect(snapTo(copy, new Set(), 0.2, 0.1, 0.5)).toEqual({ point: [0, 0], kind: "Middle" });
    expect(snapTo(copy, new Set(), 5, 0.2, 0.5)).toEqual({ point: [5, 0], kind: "On line" });
    expect(snapTo(copy, new Set(), 5, 3, 0.5)).toBeNull();
    expect(snapTo(copy, new Set([0]), -9.8, 0.1, 0.5)).toBeNull(); // not on a hidden layer
  });

  it("snaps where two lines cross", () => {
    const copy = parseScreen(screenCopy());
    copy.segments.set([5, -5, 5, 5], 4); // the door leaf, redrawn across the wall
    expect(snapTo(copy, new Set(), 5.2, 0.2, 0.5)).toEqual({ point: [5, 0], kind: "Crossing" });
  });

  it("chooses by a box: what lies inside it, or what it touches", () => {
    const copy = parseScreen(screenCopy());
    expect(inWindow(copy, new Set(), [-11, -1, 11, 7], false)).toEqual([0, 1]);
    expect(inWindow(copy, new Set(), [-1, -1, 1, 1], false)).toEqual([]);
    expect(inWindow(copy, new Set(), [-1, -1, 1, 1], true)).toEqual([0]); // the wall runs through it
  });

  it("chooses everything like the chosen, and finds words", () => {
    const copy = parseScreen(screenCopy());
    copy.meta[3] = 0; // the door leaf on the wall's layer
    expect(alike(copy, new Set(), [0])).toEqual([0, 1]);
    expect(findWords(copy, "kitch")).toEqual([2]);
    expect(findWords(copy, "  ")).toEqual([]);
  });
});
