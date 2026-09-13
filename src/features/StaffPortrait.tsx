import { Avatar, Style } from "@dicebear/core";
import notionistsDefinition from "@dicebear/styles/notionists.json" with { type: "json" };
import { memo, useMemo, useState, type CSSProperties } from "react";

export const PORTRAIT_RENDER_SIZE = 160;
const DEFAULT_DISPLAY_SIZE = 64;
const MIN_DISPLAY_SIZE = 24;
const MAX_DISPLAY_SIZE = 160;
const MAX_SEED_LENGTH = 500;

export const NOTIONISTS_RECIPE = {
  styleId: "notionists-v1",
  coreVersion: "10.7.0",
  stylesVersion: "10.6.0",
  renderSize: PORTRAIT_RENDER_SIZE,
  idRandomization: false,
} as const;

export interface StaffPortraitDescriptor {
  readonly style: string;
  readonly seed: string;
}

export interface StaffPortraitProps {
  portrait: StaffPortraitDescriptor;
  name: string;
  size?: number;
  decorative?: boolean;
}

type PortraitResult = {
  readonly key: string;
  readonly source?: string;
  readonly svg?: string;
  readonly error?: string;
};

type CachedPortrait = {
  readonly source: string;
  readonly svg: string;
};

type PortraitStyle = CSSProperties & {
  "--staff-portrait-size": string;
};

const notionistsStyle = new Style(notionistsDefinition);
const portraitCache = new Map<string, CachedPortrait>();
const controlCharacterPattern = /[\u0000-\u001f\u007f]/u;

/**
 * Render the supported style from the server-owned descriptor.
 *
 * This function intentionally accepts only the versioned local style ID. The
 * display name is never part of the render key, so changing a profile label
 * cannot change a persisted colleague's appearance.
 */
export function renderPortraitSvg(
  portrait: StaffPortraitDescriptor,
): string | null {
  return resolvePortrait(portrait).svg ?? null;
}

function StaffPortraitView({
  portrait,
  name,
  size,
  decorative = false,
}: StaffPortraitProps) {
  const result = useMemo(
    () => resolvePortrait(portrait),
    [portrait.style, portrait.seed],
  );
  const [failedKey, setFailedKey] = useState<string | null>(null);
  const [retryCount, setRetryCount] = useState(0);
  const displayName = readableName(name);
  const displaySize = boundedDisplaySize(size);
  const failed = Boolean(result.source && failedKey === result.key);
  const showImage = Boolean(result.source && !failed);
  const frameStyle: PortraitStyle = {
    "--staff-portrait-size": `${displaySize}px`,
  };
  const failureReason = result.error ?? "Portrait failed to load locally.";

  return (
    <span
      className={`staff-portrait${showImage ? "" : " staff-portrait-fallback"}`}
      data-portrait-style={portrait.style}
    >
      <span className="staff-portrait__frame" style={frameStyle}>
        {showImage ? (
          <img
            key={`${result.key}:${retryCount}`}
            className="staff-portrait__image"
            src={result.source}
            width={PORTRAIT_RENDER_SIZE}
            height={PORTRAIT_RENDER_SIZE}
            alt={decorative ? "" : `Illustrated portrait of ${displayName}`}
            aria-hidden={decorative || undefined}
            data-portrait-style={NOTIONISTS_RECIPE.styleId}
            onError={() => setFailedKey(result.key)}
          />
        ) : (
          <span
            className="staff-portrait__fallback"
            role={decorative ? undefined : "img"}
            aria-hidden={decorative || undefined}
            aria-label={
              decorative ? undefined : `Portrait unavailable for ${displayName}`
            }
          >
            <span aria-hidden="true">{getStaffPortraitInitials(name)}</span>
          </span>
        )}
      </span>
      {!showImage ? (
        <span className="staff-portrait__reason" role="status">
          {failureReason} This does not affect staff work.
        </span>
      ) : null}
      {failed ? (
        <button
          className="staff-portrait__retry"
          type="button"
          onClick={() => {
            setFailedKey(null);
            setRetryCount((count) => count + 1);
          }}
        >
          Try portrait again
        </button>
      ) : null}
    </span>
  );
}

export const StaffPortrait = memo(
  StaffPortraitView,
  areStaffPortraitPropsEqual,
);

export function getStaffPortraitInitials(name: string): string {
  const normalized = typeof name === "string" ? name.trim() : "";
  if (!normalized) return "?";

  const words = normalized.split(/\s+/u).filter(Boolean);
  const firstWord = graphemes(words[0] ?? "");
  if (words.length === 1) {
    return firstWord.slice(0, 2).join(" ").toLocaleUpperCase();
  }

  const lastWord = graphemes(words.at(-1) ?? "");
  return `${firstWord[0] ?? ""}${lastWord[0] ?? ""}`.toLocaleUpperCase();
}

function resolvePortrait(portrait: StaffPortraitDescriptor): PortraitResult {
  const style = readString(portrait?.style);
  const rawSeed = readString(portrait?.seed);
  const seed = rawSeed.trim();
  const key = `${style}\u0000${seed}`;

  if (style !== NOTIONISTS_RECIPE.styleId) {
    return {
      key,
      error: "Portrait style is unavailable locally.",
    };
  }
  if (!isBoundedSeed(rawSeed)) {
    return {
      key,
      error: "Portrait seed is invalid.",
    };
  }

  const cached = portraitCache.get(key);
  if (cached) return { key, ...cached };

  try {
    const avatar = new Avatar(notionistsStyle, {
      seed,
      size: PORTRAIT_RENDER_SIZE,
      idRandomization: NOTIONISTS_RECIPE.idRandomization,
    });
    const rendered = {
      key,
      source: avatar.toDataUri(),
      svg: avatar.toString(),
    };
    portraitCache.set(key, rendered);
    return rendered;
  } catch {
    return {
      key,
      error: "Portrait could not be rendered locally.",
    };
  }
}

function isBoundedSeed(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.trim().length > 0 &&
    value.length <= MAX_SEED_LENGTH &&
    !controlCharacterPattern.test(value)
  );
}

function readString(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function readableName(value: string): string {
  const normalized = typeof value === "string" ? value.trim() : "";
  return normalized || "staff member";
}

function boundedDisplaySize(value: number | undefined): number {
  if (value === undefined || !Number.isFinite(value)) {
    return DEFAULT_DISPLAY_SIZE;
  }
  return Math.min(
    MAX_DISPLAY_SIZE,
    Math.max(MIN_DISPLAY_SIZE, Math.round(value)),
  );
}

function areStaffPortraitPropsEqual(
  previous: Readonly<StaffPortraitProps>,
  next: Readonly<StaffPortraitProps>,
): boolean {
  return (
    previous.name === next.name &&
    previous.size === next.size &&
    previous.decorative === next.decorative &&
    previous.portrait.style === next.portrait.style &&
    previous.portrait.seed === next.portrait.seed
  );
}

function graphemes(value: string): string[] {
  const segmenter = (
    Intl as typeof Intl & {
      Segmenter?: new (
        locales?: string | string[],
        options?: { granularity: "grapheme" },
      ) => { segment(value: string): Iterable<{ segment: string }> };
    }
  ).Segmenter;
  if (segmenter) {
    return Array.from(
      new segmenter(undefined, { granularity: "grapheme" }).segment(value),
      (part) => part.segment,
    );
  }
  return Array.from(value);
}
