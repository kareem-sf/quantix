import { describe, expect, it } from "vitest";
import type { TenderGlance } from "../tenders/queries";
import { news, notice } from "./notify";

const glance = (over: Partial<TenderGlance>): TenderGlance =>
  ({ id: "t1", name: "Al Noor School", archived: false, waiting: 0, team: "idle", ...over }) as TenderGlance;

describe("news", () => {
  it("says nothing on the first look", () => {
    expect(news(null, [glance({ waiting: 3 })])).toEqual([]);
  });

  it("tells of new decisions and of a team that finished", () => {
    expect(news([glance({ waiting: 1 })], [glance({ waiting: 3 })])).toEqual([
      { tender: "t1", name: "Al Noor School", text: "2 new decisions need you" },
    ]);
    expect(news([glance({ team: "working" })], [glance({ team: "idle" })])).toEqual([
      { tender: "t1", name: "Al Noor School", text: "The team finished its work" },
    ]);
  });

  it("stays quiet when decisions are taken or a tender is archived", () => {
    expect(news([glance({ waiting: 2 })], [glance({ waiting: 1 })])).toEqual([]);
    expect(news([glance({ waiting: 0 })], [glance({ waiting: 2, archived: true })])).toEqual([]);
  });
});

describe("notice", () => {
  it("opens the tender when the news is from one, and the Desk when it is from several", () => {
    const school = { tender: "t1", name: "Al Noor School", text: "1 new decision needs you" };
    expect(notice([school])).toEqual({
      title: "Al Noor School",
      body: "1 new decision needs you",
      open: "/tenders/t1",
    });
    expect(notice([school, { tender: "t2", name: "Ring Road", text: "The team finished its work" }])).toEqual({
      title: "Several tenders",
      body: "Al Noor School: 1 new decision needs you\nRing Road: The team finished its work",
      open: "/desk",
    });
  });
});
