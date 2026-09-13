/**
 * Underline that draws in on hover and focus. Adapted from Skiper UI
 * (skiper40) by @gurvinder-singh02 — https://skiper-ui.com. Add these classes
 * to a link-style Button or anchor.
 */
export const linkUnderline = [
  "relative hover:no-underline focus-visible:no-underline",
  "before:pointer-events-none before:absolute before:inset-x-0 before:bottom-0 before:h-px before:bg-current before:content-['']",
  "before:origin-right before:scale-x-0 before:transition-transform before:duration-300 before:ease-[cubic-bezier(0.4,0,0.2,1)] rtl:before:origin-left",
  "hover:before:origin-left hover:before:scale-x-100 rtl:hover:before:origin-right",
  "focus-visible:before:scale-x-100 motion-reduce:before:transition-none",
].join(" ");
