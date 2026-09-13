import { describe, expect, it } from "vitest";
import {
  parseRouteContext,
  safeLocalOrigin,
  settingsRoute,
  sourceRoute,
  stripSourceQuery,
  tenderRoute,
} from "./routes";

describe("workspace routes", () => {
  it("builds an addressable tender section route", () => {
    expect(tenderRoute("tender/one", "work")).toBe(
      "/tenders/tender%2Fone/work",
    );
  });

  it("keeps source identity and locator context in the URL", () => {
    const path = sourceRoute("one", "documents", {
      artifactId: "artifact-1",
      version: 3,
      page: 12,
      origin: "/tenders/one/work",
    });
    expect(path).toBe(
      "/tenders/one/documents?artifact_id=artifact-1&version=3&page=12&origin=%2Ftenders%2Fone%2Fwork",
    );
    expect(parseRouteContext(path)).toEqual({
      kind: "tender",
      tenderId: "one",
      section: "documents",
      artifactId: "artifact-1",
      version: 3,
      page: 12,
      origin: "/tenders/one/work",
    });
  });

  it("round-trips spreadsheet locator and content identity through a source route", () => {
    const path = sourceRoute("one", "documents", {
      sourceId: "evidence-1",
      artifactId: "artifact-1",
      version: 4,
      contentHash: "sha-4",
      page: 2,
      sheet: "BOQ",
      cellRange: "A4:F9",
      origin: "/tenders/one/documents",
      view: "register",
    });
    expect(parseRouteContext(path)).toMatchObject({
      sourceId: "evidence-1",
      artifactId: "artifact-1",
      version: 4,
      contentHash: "sha-4",
      page: 2,
      sheet: "BOQ",
      cellRange: "A4:F9",
      origin: "/tenders/one/documents",
    });
    expect(stripSourceQuery(path)).toBe("/tenders/one/documents?view=register");
  });

  it("preserves the route that opened Settings", () => {
    const path = settingsRoute("/tenders/one/estimate");
    expect(path).toBe("/settings?return=%2Ftenders%2Fone%2Festimate");
    expect(parseRouteContext(path)).toEqual({
      kind: "settings",
      origin: "/tenders/one/estimate",
    });
  });

  it("ignores invalid locators and rejects malformed or external return routes", () => {
    expect(
      parseRouteContext("/tenders/one/documents?page=0&version=1.5"),
    ).toEqual({
      kind: "tender",
      tenderId: "one",
      section: "documents",
    });
    expect(parseRouteContext("/tenders/%E0%A4%A/documents").kind).toBe(
      "settings",
    );
    expect(safeLocalOrigin("https://example.com/steal")).toBeUndefined();
    expect(safeLocalOrigin("//example.com/steal")).toBeUndefined();
    expect(safeLocalOrigin("/tenders/one/work?view=plan&record=plan-1")).toBe(
      "/tenders/one/work?view=plan&record=plan-1",
    );
    expect(
      stripSourceQuery(
        "/tenders/one/work?view=plan&record=plan-1&source_id=evidence-1&page=2",
      ),
    ).toBe("/tenders/one/work?view=plan&record=plan-1");
  });
});
