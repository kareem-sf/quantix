/** Quantix is a desktop app, not a web page: right-click offers Cut, Copy and Paste only in a text field or on text
 * the engineer selected, never the browser's Back, Reload, Save as and Print. */
export function behaveLikeAnApp(page: Document = document) {
  page.addEventListener("contextmenu", (event) => {
    const editing = (event.target as Element | null)?.closest?.("input, textarea, [contenteditable='true']");
    const selected = Boolean(page.getSelection()?.toString());
    if (!editing && !selected) event.preventDefault();
  });
}
