import { describe, expect, it } from "vitest";
import type { TenderGlance } from "./queries";
import { daysTo, stages, standing } from "./stage";

// a tender with every stage done: all documents read, the BOQ priced, no packages, the checklist ready
const glance = (over: Partial<TenderGlance> = {}): TenderGlance =>
  ({ documents: 4, read: 4, items: 10, priced: 10, packages: 0, chosen: 0, requirements: 3, ready: 3, ...over }) as TenderGlance;

describe("where a tender stands", () => {
  it("names the first stage not done and how far it has got", () => {
    expect(standing(glance({ documents: 0, read: 0 }))).toBe("No documents yet");
    expect(standing(glance({ read: 1 }))).toBe("Reading · 1 of 4");
    expect(standing(glance({ items: 0, priced: 0 }))).toBe("BOQ not entered");
    expect(standing(glance({ priced: 7 }))).toBe("Pricing · 7 of 10");
    expect(standing(glance({ packages: 3, chosen: 1 }))).toBe("Quotes · 1 of 3 chosen");
    expect(standing(glance({ ready: 2 }))).toBe("Submission · 2 of 3 ready");
    expect(standing(glance({ requirements: 0, ready: 0 }))).toBe("Submission · no checklist");
    expect(standing(glance())).toBe("Ready to build");
  });

  it("counts a stage done only from Quantix's own counts", () => {
    expect(stages(glance())).toEqual([true, true, true, true, true]);
    expect(stages(glance({ documents: 0, read: 0, items: 0, priced: 0, requirements: 0, ready: 0 }))).toEqual([
      false, false, false, false, false,
    ]);
    // with nothing to subcontract, quotes are done once the BOQ is priced; with packages, once each has its quote
    expect(stages(glance({ priced: 9 }))[3]).toBe(false);
    expect(stages(glance({ packages: 2, chosen: 2, priced: 9 }))[3]).toBe(true);
    expect(stages(glance({ packages: 2, chosen: 1 }))[3]).toBe(false);
  });
});

describe("days to a date", () => {
  it("counts whole days from today, whatever the time of day", () => {
    expect(daysTo("2026-10-14", new Date(2026, 8, 23, 0, 5))).toBe(21);
    expect(daysTo("2026-10-14", new Date(2026, 8, 23, 23, 55))).toBe(21);
    expect(daysTo("2026-09-23", new Date(2026, 8, 23, 18, 0))).toBe(0);
    expect(daysTo("2026-09-20", new Date(2026, 8, 23, 9, 0))).toBe(-3);
  });
});
