import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";
import { useSettings } from "../settings/queries";
import { useDesk, type TenderGlance } from "../tenders/queries";

export type News = { tender: string; name: string; text: string; kind: "decisions" | "finished" };

/** What changed since the last look, worth a Windows notification: decisions that newly wait for the engineer, and a
 * team that finished working. Nothing on the first look, when nothing is new yet. */
export function news(before: TenderGlance[] | null, now: TenderGlance[]): News[] {
  if (!before) return [];
  const earlier = new Map(before.map((t) => [t.id, t]));
  return now.flatMap((t) => {
    const was = earlier.get(t.id);
    if (!was || t.archived) return [];
    const more = t.waiting - was.waiting;
    const told = (text: string, kind: News["kind"]) => [{ tender: t.id, name: t.name, text, kind }];
    if (more > 0) return told(`${more} new ${more === 1 ? "decision needs" : "decisions need"} you`, "decisions");
    if (was.team === "working" && t.team === "idle") return told("The team finished its work", "finished");
    return [];
  });
}

/** One notification: a single tender's news opens that tender; news from several opens the Desk. */
export function notice(found: News[]): { title: string; body: string; open: string } {
  if (new Set(found.map((n) => n.tender)).size === 1)
    return { title: found[0].name, body: found.map((n) => n.text).join("\n"), open: `/tenders/${found[0].tender}` };
  return { title: "Several tenders", body: found.map((n) => `${n.name}: ${n.text}`).join("\n"), open: "/desk" };
}

/** In the desktop app, tell the engineer in Windows when something needs them while Quantix isn't in front, as
 * much as Settings asks for, and open the place it's about when they click it. */
export function useNotifications() {
  const desk = useDesk();
  const wanted = useSettings().data?.notifications ?? "all";
  const navigate = useNavigate();
  const last = useRef<TenderGlance[] | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    const stop = listen<string>("notification-clicked", (e) => void navigate(e.payload));
    return () => void stop.then((unlisten) => unlisten());
  }, [navigate]);
  useEffect(() => {
    if (!desk.data) return;
    const found = news(last.current, desk.data).filter(
      (n) => wanted === "all" || (wanted === "decisions" && n.kind === "decisions"),
    );
    last.current = desk.data;
    if (!isTauri() || found.length === 0 || document.hasFocus()) return;
    void invoke("notify", notice(found));
  }, [desk.data, wanted]);
}
