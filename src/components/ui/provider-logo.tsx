/**
 * Identifying marks for the AI providers Quantix can connect to, so an account
 * is recognised by its own logo instead of a coloured letter tile. Drawn
 * locally: the app never loads remote brand assets.
 */
import { useId } from "react";
import { cn } from "@/lib/utils";

export type ProviderLogoProps = {
  /** Connection provider id, for example "google" or "anthropic". */
  providerId: string;
  /** Account or service name, used for the fallback letter. */
  name?: string;
  className?: string;
};

/** Providers that draw their own mark, keyed by connection provider id. */
const marks: Record<string, (props: { id: string }) => React.ReactElement> = {
  google: GeminiMark,
  openai: OpenAIMark,
  codex: OpenAIMark,
  anthropic: AnthropicMark,
  xai: XaiMark,
  grok_build: XaiMark,
};

/** Tile colour behind a provider that has no mark of its own. */
export const providerTones: Record<string, string> = {
  custom: "bg-violet-600",
};

export function hasProviderMark(providerId: string) {
  return providerId in marks;
}

export function ProviderLogo({
  providerId,
  name,
  className,
}: ProviderLogoProps) {
  // React's generated id contains characters that are not valid in an SVG
  // url(#…) reference, which left gradient-filled marks invisible.
  const id = `quantix-mark-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  const Mark = marks[providerId];
  if (!Mark)
    return (
      <span
        aria-hidden="true"
        className={cn(
          "inline-flex items-center justify-center font-semibold",
          className,
        )}
      >
        {initial(name)}
      </span>
    );
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
      // A definite size: an SVG with no intrinsic size stretches to fill its
      // flex parent, which made the marks render as oversized empty tiles.
      className={cn("block size-4 shrink-0", className)}
    >
      <Mark id={id} />
    </svg>
  );
}

function initial(name?: string) {
  return (name || "?").trim().charAt(0).toUpperCase() || "?";
}

/** Gemini's four-point spark in the Google gradient. */
function GeminiMark({ id }: { id: string }) {
  return (
    <>
      <defs>
        <linearGradient
          id={id}
          x1="2"
          y1="21"
          x2="21"
          y2="3"
          gradientUnits="userSpaceOnUse"
        >
          <stop offset="0%" stopColor="#4285F4" />
          <stop offset="45%" stopColor="#9B72CB" />
          <stop offset="100%" stopColor="#D96570" />
        </linearGradient>
      </defs>
      <path
        fill={`url(#${id})`}
        d="M12 0c0 6.627 5.373 12 12 12-6.627 0-12 5.373-12 12 0-6.627-5.373-12-12-12C6.627 12 12 6.627 12 0Z"
      />
    </>
  );
}

/** Claude's radiating burst. */
function AnthropicMark() {
  const rays = Array.from({ length: 12 }, (_, index) => {
    const angle = (index * Math.PI) / 6;
    const sin = Math.sin(angle);
    const cos = Math.cos(angle);
    return (
      <line
        key={index}
        x1={12 + sin * 2.6}
        y1={12 - cos * 2.6}
        x2={12 + sin * 11}
        y2={12 - cos * 11}
      />
    );
  });
  return (
    <g
      stroke="#D97757"
      strokeWidth="2.1"
      strokeLinecap="round"
      vectorEffect="non-scaling-stroke"
    >
      {rays}
    </g>
  );
}

/** The interlocking OpenAI knot, drawn as three woven loops. */
function OpenAIMark() {
  return (
    <g
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinejoin="round"
    >
      <ellipse cx="12" cy="12" rx="4.1" ry="10" />
      <ellipse cx="12" cy="12" rx="4.1" ry="10" transform="rotate(60 12 12)" />
      <ellipse cx="12" cy="12" rx="4.1" ry="10" transform="rotate(120 12 12)" />
    </g>
  );
}

/** The xAI slash mark. */
function XaiMark() {
  return (
    <g stroke="currentColor" strokeLinecap="square">
      <path d="M3.4 2.8 20.6 21.2" strokeWidth="3.1" />
      <path d="M20.6 2.8 14.2 9.6" strokeWidth="2" />
      <path d="M9.4 14.6 3.4 21.2" strokeWidth="2" />
    </g>
  );
}
