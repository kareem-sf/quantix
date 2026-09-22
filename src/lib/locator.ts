/**
 * Where a passage is in its document, in words: "page 12", "BOQ, row 14".
 * Stored locators look like "page:12" or "sheet:BOQ/row:14".
 */
export function locatorLabel(locator: string | null | undefined): string {
  if (!locator) return "";
  const parts = locator.split("/").map((part) => {
    const at = part.indexOf(":");
    if (at < 0) return part;
    const key = part.slice(0, at);
    const value = part.slice(at + 1);
    switch (key) {
      case "page":
        return `page ${value}`;
      case "sheet":
        return value;
      case "row":
        return `row ${value}`;
      case "cell":
        return `cell ${value}`;
      case "range":
        return value;
      case "paragraph":
        return `paragraph ${value}`;
      case "slide":
        return `slide ${value}`;
      case "reply":
        return "supplier reply";
      case "characters":
        return "";
      default:
        return `${key} ${value}`;
    }
  });
  return parts.filter(Boolean).join(", ");
}
