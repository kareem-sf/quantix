import { Children, isValidElement, type ReactNode } from "react";

// Arabic (and Hebrew) letters, including presentation forms.
const RTL_LETTER = /[֐-׿؀-ۿݐ-ݿࢠ-ࣿיִ-﷿ﹰ-﻿]/g;
const LTR_LETTER = /[A-Za-zÀ-ɏ]/g;

/**
 * The reading direction of a piece of text, decided by the script most of its
 * letters are written in. An Arabic sentence that mentions "Systems.pdf" is
 * still right to left; an English sentence quoting one Arabic title is not.
 */
export function textDirection(text: string): "rtl" | "ltr" {
  const rtl = text.match(RTL_LETTER)?.length ?? 0;
  const ltr = text.match(LTR_LETTER)?.length ?? 0;
  return rtl > ltr ? "rtl" : "ltr";
}

/** `dir="rtl"` for mostly-Arabic text, nothing otherwise. */
export function rtlDir(text: string | null | undefined): "rtl" | undefined {
  return text && textDirection(text) === "rtl" ? "rtl" : undefined;
}

/** The plain text inside rendered children, for deciding their direction. */
export function nodeText(node: ReactNode): string {
  let text = "";
  Children.forEach(node, (child) => {
    if (typeof child === "string" || typeof child === "number")
      text += String(child);
    else if (isValidElement<{ children?: ReactNode }>(child))
      text += nodeText(child.props.children);
  });
  return text;
}
