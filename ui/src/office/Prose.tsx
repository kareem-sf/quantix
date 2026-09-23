import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

const COMPONENTS: Components = {
  p: ({ children }) => <p className="mb-2 last:mb-0">{children}</p>,
  ul: ({ children }) => <ul className="mb-2 list-disc space-y-1 pl-5 last:mb-0">{children}</ul>,
  ol: ({ children }) => <ol className="mb-2 list-decimal space-y-1 pl-5 last:mb-0">{children}</ol>,
  li: ({ children }) => <li className="pl-0.5">{children}</li>,
  strong: ({ children }) => <strong className="font-semibold text-ink">{children}</strong>,
  h1: ({ children }) => <p className="mb-1.5 font-semibold text-ink">{children}</p>,
  h2: ({ children }) => <p className="mb-1.5 font-semibold text-ink">{children}</p>,
  h3: ({ children }) => <p className="mb-1.5 font-semibold text-ink">{children}</p>,
  code: ({ children }) => <code className="rounded bg-subtle px-1 text-[0.95em]">{children}</code>,
  a: ({ children, href }) => (
    <a href={href} target="_blank" rel="noreferrer" className="underline underline-offset-2">
      {children}
    </a>
  ),
  table: ({ children }) => (
    <div className="mb-2 overflow-x-auto last:mb-0">
      <table className="border-collapse text-[13px]">{children}</table>
    </div>
  ),
  th: ({ children }) => <th className="border-b border-line-strong px-2 py-1 text-left font-semibold">{children}</th>,
  td: ({ children }) => <td className="border-b border-subtle px-2 py-1 align-top">{children}</td>,
};

/** Older messages ran points together as "(1) … (2) …" and signed off "— Salem": lay them out as a list. */
export function tidy(text: string, signer?: string): string {
  let out = text.trim();
  if (signer) out = out.replace(new RegExp(`\\s*[—–-]\\s*${signer}\\.?$`), "");
  if (!out.includes("\n") && /\(1\)\s/.test(out) && /\(2\)\s/.test(out)) {
    const [lead, ...points] = out.split(/\s*\((?=\d+\)\s)/);
    const items = points.map((p) => p.replace(/^(\d+)\)\s*/, "$1. "));
    out = [lead.trim(), items.join("\n")].filter(Boolean).join("\n\n");
  }
  return out;
}

/** What someone in the office wrote, laid out for reading: paragraphs, lists, bold and tables. No raw HTML. */
export function Prose({ text, signer, className = "" }: { text: string; signer?: string; className?: string }) {
  return (
    <div dir="auto" className={`leading-relaxed [overflow-wrap:anywhere] ${className}`}>
      <Markdown remarkPlugins={[remarkGfm]} components={COMPONENTS}>
        {tidy(text, signer)}
      </Markdown>
    </div>
  );
}
