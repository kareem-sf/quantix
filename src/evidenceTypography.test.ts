import { describe, expect, it } from "vitest";

import {
  evidenceLanguageTag,
  evidenceTextAttributes,
  evidenceTextDirection,
} from "./evidenceTypography";

describe("evidence typography", () => {
  it("maps source languages to valid BCP 47 tags", () => {
    expect(evidenceLanguageTag("arabic")).toBe("ar");
    expect(evidenceLanguageTag("English")).toBe("en");
    expect(evidenceLanguageTag("mixed")).toBeUndefined();
    expect(evidenceLanguageTag("undetermined")).toBeUndefined();
  });

  it("maps source directions to HTML direction values", () => {
    expect(evidenceTextDirection("right_to_left")).toBe("rtl");
    expect(evidenceTextDirection("left_to_right")).toBe("ltr");
    expect(evidenceTextDirection("mixed")).toBe("auto");
    expect(evidenceTextDirection("neutral")).toBe("auto");
  });

  it("builds attributes for an evidence location", () => {
    expect(
      evidenceTextAttributes({
        language: "arabic",
        direction: "right_to_left",
      }),
    ).toEqual({ lang: "ar", dir: "rtl" });
  });
});
