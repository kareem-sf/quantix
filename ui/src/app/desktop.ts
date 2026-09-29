import { isTauri } from "@tauri-apps/api/core";

/** Quantix is a desktop app, not a web page: right-click offers Cut, Copy and Paste only in a text field or on text
 * the engineer selected, never the browser's Back, Reload, Save as and Print. In the desktop window a web link
 * opens in the engineer's own browser: the window hands every page outside Quantix to it (desktop/src/main.rs),
 * but a link that asks for a new tab never reaches that, so it goes the same way as any other link. */
export function behaveLikeAnApp(page: Document = document, desktop = isTauri()) {
  page.addEventListener("contextmenu", (event) => {
    const editing = (event.target as Element | null)?.closest?.("input, textarea, [contenteditable='true']");
    const selected = Boolean(page.getSelection()?.toString());
    if (!editing && !selected) event.preventDefault();
  });
  if (!desktop) return;
  page.addEventListener(
    "click",
    (event) => {
      const link = (event.target as Element | null)?.closest?.("a[target='_blank']") as HTMLAnchorElement | null;
      if (!link || !/^(https?|mailto):/.test(link.href) || new URL(link.href).origin === page.location.origin) return;
      event.preventDefault();
      page.location.assign(link.href);
    },
    true,
  );
}
