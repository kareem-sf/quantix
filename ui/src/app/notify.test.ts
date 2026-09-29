import { describe, expect, it } from "vitest";
import type { TenderGlance } from "../tenders/queries";
import { news } from "./notify";

const glance = (over: Partial<TenderGlance>): TenderGlance =>
  ({ id: "t1", name: "Al Noor School", archived: false, waiting: 0, team: "idle", ...over }) as TenderGlance;

describe("news", () => {
  it("says nothing on the first look", () => {
    expect(news(null, [glance({ waiting: 3 })])).toEqual([]);
  });

  it("tells of new decisions and of a team that finished", () => {
    expect(news([glance({ waiting: 1 })], [glance({ waiting: 3 })])).toEqual(["Al Noor School: 2 new decisions need you"]);
    expect(news([glance({ team: "working" })], [glance({ team: "idle" })])).toEqual([
      "Al Noor School: the team finished its work",
    ]);
  });

  it("stays quiet when decisions are taken or a tender is archived", () => {
    expect(news([glance({ waiting: 2 })], [glance({ waiting: 1 })])).toEqual([]);
    expect(news([glance({ waiting: 0 })], [glance({ waiting: 2, archived: true })])).toEqual([]);
  });
});
