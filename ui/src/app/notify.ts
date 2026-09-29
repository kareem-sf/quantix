import { isTauri } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { useEffect, useRef } from "react";
import { useDesk, type TenderGlance } from "../tenders/queries";

/** What changed since the last look, worth a Windows notification: decisions that newly wait for the engineer, and a
 * team that finished working. Nothing on the first look, when nothing is new yet. */
export function news(before: TenderGlance[] | null, now: TenderGlance[]): string[] {
  if (!before) return [];
  const earlier = new Map(before.map((t) => [t.id, t]));
  return now.flatMap((t) => {
    const was = earlier.get(t.id);
    if (!was || t.archived) return [];
    const more = t.waiting - was.waiting;
    if (more > 0) return [`${t.name}: ${more} new ${more === 1 ? "decision needs" : "decisions need"} you`];
    if (was.team === "working" && t.team === "idle") return [`${t.name}: the team finished its work`];
    return [];
  });
}

/** In the desktop app, tell the engineer in Windows when something needs them while Quantix isn't in front. */
export function useNotifications() {
  const desk = useDesk();
  const last = useRef<TenderGlance[] | null>(null);
  useEffect(() => {
    if (!desk.data) return;
    const lines = news(last.current, desk.data);
    last.current = desk.data;
    if (!isTauri() || lines.length === 0 || document.hasFocus()) return;
    void (async () => {
      if (!(await isPermissionGranted()) && (await requestPermission()) !== "granted") return;
      sendNotification({ title: "Quantix", body: lines.join("\n") });
    })();
  }, [desk.data]);
}
