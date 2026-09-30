import { describe, expect, it } from "vitest";
import { must } from "./client";

const answer = (status: number) => new Response(null, { status });

describe("the service's answers", () => {
  it("gives the data, including an empty list", () => {
    expect(must({ data: [], response: answer(200) })).toEqual([]);
    expect(must({ data: { id: "t1" }, response: answer(201) })).toEqual({ id: "t1" });
  });

  it("carries the service's own message when it refuses", () => {
    expect(() => must({ error: { detail: "Tender not found." }, response: answer(404) })).toThrow("Tender not found.");
  });

  it("says what the service answered when it gives no message to show", () => {
    const invalid = { detail: [{ loc: ["body", "name"], msg: "Field required" }] };
    expect(() => must({ error: invalid, response: answer(422) })).toThrow("The Quantix service answered 422.");
    expect(() => must({ response: answer(500) })).toThrow("The Quantix service answered 500.");
  });
});
