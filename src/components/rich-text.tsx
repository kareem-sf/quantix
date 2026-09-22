import type { ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { FileText } from "lucide-react";
import { ExternalLink } from "./ExternalLink";
import { cn } from "@/lib/utils";
import { nodeText, textDirection } from "@/lib/text-direction";

const SOURCE_PREFIX = "#source:";
// The AI cites evidence as [id] or [id, id]; 32 hex characters each.
const CITATION = /\[\s*([0-9a-f]{32}(?:\s*,\s*[0-9a-f]{32})*)\s*\]/g;
const BARE_ID = /\s?\(?`?(?<![#:,/\w])[0-9a-f]{32}(?![\w])`?\)?/g;

/** Replace raw evidence IDs with small source links an engineer can read. */
function linkCitations(text: string) {
  return (
    text
      .replace(CITATION, (_match, ids: string) => {
        const list = ids.split(",").map((id) => id.trim());
        const label = list.length === 1 ? "source" : `${list.length} sources`;
        return `[${label}](${SOURCE_PREFIX}${list.join(",")})`;
      })
      // A bare record ID in a sentence ("saved as work product 315e…") means
      // nothing to an engineer; the record itself is linked below the reply.
      .replace(BARE_ID, "")
  );
}

/** Long notes are stored shortened, which can cut a bold span in half. */
function closeDanglingEmphasis(text: string) {
  const marks = text.match(/\*\*/g)?.length ?? 0;
  if (marks % 2 === 0) return text;
  const last = text.lastIndexOf("**");
  return `${text.slice(0, last)}${text.slice(last + 2)}`;
}

// Arabic paragraphs read right to left and English ones left to right, each
// decided by the script most of its letters use.
function directed<
  T extends
    | "p"
    | "li"
    | "h1"
    | "h2"
    | "h3"
    | "h4"
    | "h5"
    | "h6"
    | "blockquote"
    | "td"
    | "th",
>(Tag: T) {
  return function Directed({
    children,
    node: _node,
    ...props
  }: { children?: ReactNode; node?: unknown } & Record<string, unknown>) {
    const Element = Tag as unknown as "p";
    return (
      <Element {...(props as object)} dir={textDirection(nodeText(children))}>
        {children}
      </Element>
    );
  };
}

const DIRECTED = Object.fromEntries(
  (
    [
      "p",
      "li",
      "h1",
      "h2",
      "h3",
      "h4",
      "h5",
      "h6",
      "blockquote",
      "td",
      "th",
    ] as const
  ).map((tag) => [tag, directed(tag)]),
) as Components;

function components(
  onSource?: (sourceId: string) => void,
  inline = false,
): Components {
  return {
    ...DIRECTED,
    a: ({ href, children }) => {
      if (href?.startsWith(SOURCE_PREFIX)) {
        const ids = href.slice(SOURCE_PREFIX.length).split(",").filter(Boolean);
        const chip =
          "mx-0.5 inline-flex h-4.5 translate-y-[-1px] items-center gap-1 rounded-[5px] bg-muted px-1 align-middle text-[11px] font-medium text-muted-foreground ring-1 ring-border/60 no-underline";
        return onSource && ids.length ? (
          <button
            type="button"
            className={cn(chip, "hover:bg-accent hover:text-foreground")}
            title="Open the cited passage"
            onClick={() => onSource(ids[0])}
          >
            <FileText aria-hidden className="size-2.5" />
            {children}
          </button>
        ) : (
          <span className={chip}>
            <FileText aria-hidden className="size-2.5" />
            {children}
          </span>
        );
      }
      return <ExternalLink href={href}>{children}</ExternalLink>;
    },
    // Wide tables scroll inside their own box instead of stretching the page.
    table: ({ children }) => (
      <div className="typeset-scroll">
        <table>{children}</table>
      </div>
    ),
    ...(inline ? { p: ({ children }) => <>{children}</> } : {}),
  };
}

// Keep ordinary links safe while letting the source links through.
function urlTransform(url: string) {
  if (url.startsWith(SOURCE_PREFIX)) return url;
  return /^(https?:|mailto:|#|\/)/i.test(url) ? url : "";
}

/**
 * Text the AI wrote, rendered as formatted text: bold, lists, tables and links
 * instead of raw Markdown symbols, with cited evidence IDs shown as small
 * source links. Use `inline` inside a heading or a single line.
 */
export function RichText({
  text,
  inline = false,
  onSource,
  className,
}: {
  text: string;
  inline?: boolean;
  onSource?: (sourceId: string) => void;
  className?: string;
}): ReactNode {
  const markdown = (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={components(onSource, inline)}
      urlTransform={urlTransform}
    >
      {linkCitations(closeDanglingEmphasis(text))}
    </ReactMarkdown>
  );
  const dir = textDirection(text);
  if (inline)
    return (
      <span className={className} dir={dir}>
        {markdown}
      </span>
    );
  return (
    <div className={cn("markdown typeset typeset-chat", className)} dir={dir}>
      {markdown}
    </div>
  );
}

/** Whether AI text needs block formatting (tables, lists, headings, paragraphs). */
export function isStructured(text: string) {
  return (
    /\n\s*\n/.test(text.trim()) ||
    /^\s*(#{1,6}\s|[-*+]\s|\d+[.)]\s|\|.*\|)/m.test(text) ||
    text.length > 320
  );
}

/**
 * One line of readable text from Markdown, for places that show a single line:
 * plan steps, rows, summaries. Symbols and evidence IDs are removed, never shown.
 */
export function plainText(text: string, max = 0) {
  const flat = text
    .replace(CITATION, "")
    .replace(BARE_ID, "")
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/^\s*\|?[\s:|-]+\|[\s:|-]*$/gm, " ")
    .replace(/\|/g, " · ")
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/gm, "")
    .replace(/^\s*(?:[-*+]|\d+[.)])\s+/gm, "")
    .replace(/(\*\*|__)(.*?)\1/g, "$2")
    .replace(/(^|[^\w*])([*_])([^*_\n]+)\2(?=[^\w*]|$)/g, "$1$3")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\*\*/g, "")
    .replace(/\s+/g, " ")
    .replace(/(\s*·\s*)+/g, " · ")
    .replace(/\s+([.,;:])/g, "$1")
    .replace(/^\s*·\s*|\s*·\s*$/g, "")
    .trim();
  if (!max || flat.length <= max) return flat;
  const cut = flat.slice(0, max);
  const sentence = cut.lastIndexOf(". ");
  return `${(sentence > max * 0.5 ? cut.slice(0, sentence + 1) : cut).trimEnd()}…`;
}
