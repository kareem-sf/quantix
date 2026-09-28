import { describe, expect, it } from "vitest";
import { dueSentence, dueShort, dueSource } from "./due";

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

  it("says whose date it is and where it comes from", () => {
    const engineer = { basis: "engineer" as const, set_by: null, set_at: null, document_id: null, document_name: null, page: null, quote: null };
    expect(dueSource(engineer)?.text).toBe("You set this date. It wasn't taken from the tender documents.");
    const onTheirWord = { ...engineer, set_by: "Salem", set_at: "2026-09-28T07:40:00Z" };
    expect(dueSource(onTheirWord)?.text).toBe(
      "Salem entered this date on 28 September 2026, as you asked. It wasn't taken from the tender documents.",
    );
    const page = { ...onTheirWord, basis: "document" as const, document_id: "d1", document_name: "ITT.pdf", page: 4, quote: "due by 14 October 2026" };
    expect(dueSource(page)).toEqual({
      title: "From the tender documents",
      text: "“due by 14 October 2026”, ITT.pdf, page 4. Salem entered it on 28 September 2026.",
    });
    expect(dueSource(null)).toBeNull();
  });

  it("uses a short form in the sidebar", () => {
    expect(dueShort("2026-10-14")).toBe("due 14 Oct");
  });
});
