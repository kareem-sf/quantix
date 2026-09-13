import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/**
 * The Quantix type scale (src/styles/brand.css) as small semantic components:
 * page title 18px, section 16px, body 14px, caption 12px; weights 400–600.
 */

export function TypographyH1({ className, ...props }: ComponentProps<"h1">) {
  return (
    <h1
      className={cn(
        "scroll-m-20 text-lg font-semibold tracking-tight text-balance",
        className,
      )}
      {...props}
    />
  );
}

export function TypographyH2({ className, ...props }: ComponentProps<"h2">) {
  return (
    <h2
      className={cn("scroll-m-20 text-base font-medium", className)}
      {...props}
    />
  );
}

export function TypographyH3({ className, ...props }: ComponentProps<"h3">) {
  return (
    <h3
      className={cn("scroll-m-20 text-sm font-medium", className)}
      {...props}
    />
  );
}

export function TypographyH4({ className, ...props }: ComponentProps<"h4">) {
  return (
    <h4
      className={cn("scroll-m-20 text-sm font-medium", className)}
      {...props}
    />
  );
}

export function TypographyP({ className, ...props }: ComponentProps<"p">) {
  return (
    <p
      className={cn("text-sm leading-6 [&:not(:first-child)]:mt-4", className)}
      {...props}
    />
  );
}

export function TypographyBlockquote({
  className,
  ...props
}: ComponentProps<"blockquote">) {
  return (
    <blockquote
      className={cn("mt-4 border-s-2 ps-4 text-sm italic", className)}
      {...props}
    />
  );
}

export function TypographyList({ className, ...props }: ComponentProps<"ul">) {
  return (
    <ul
      className={cn("my-4 ms-6 list-disc text-sm [&>li]:mt-1.5", className)}
      {...props}
    />
  );
}

export function TypographyInlineCode({
  className,
  ...props
}: ComponentProps<"code">) {
  return (
    <code
      className={cn(
        "relative rounded bg-muted px-[0.3rem] py-[0.2rem] font-mono text-xs",
        className,
      )}
      {...props}
    />
  );
}

export function TypographyLead({ className, ...props }: ComponentProps<"p">) {
  return (
    <p className={cn("text-sm text-muted-foreground", className)} {...props} />
  );
}

export function TypographyLarge({
  className,
  ...props
}: ComponentProps<"div">) {
  return <div className={cn("text-base font-medium", className)} {...props} />;
}

export function TypographySmall({
  className,
  ...props
}: ComponentProps<"small">) {
  return (
    <small
      className={cn("text-xs leading-none font-medium", className)}
      {...props}
    />
  );
}

export function TypographyMuted({ className, ...props }: ComponentProps<"p">) {
  return (
    <p className={cn("text-sm text-muted-foreground", className)} {...props} />
  );
}
