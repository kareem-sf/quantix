import { describe, expect, it } from "vitest";
import { dueSentence, dueShort } from "./due";

const today = new Date(2026, 8, 23, 15, 30);

describe("due dates", () => {
  it("counts whole days left from today", () => {
    expect(dueSentence("2026-10-14", today)).toBe("Due 14 October, 21 days left.");
    expect(dueSentence("2026-09-24", today)).toBe("Due tomorrow, 24 September.");
    expect(dueSentence("2026-09-23", today)).toBe("Due today, 23 September.");
    expect(dueSentence("2026-09-01", today)).toBe("Was due 1 September.");
  });

  it("says plainly when there is no date", () => {
    expect(dueSentence(null, today)).toBe("No due date yet.");
    expect(dueShort(null)).toBe("no due date");
  });

  it("uses a short form in the sidebar", () => {
    expect(dueShort("2026-10-14")).toBe("due 14 Oct");
  });
});
