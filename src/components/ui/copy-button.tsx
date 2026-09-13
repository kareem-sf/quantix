import { useEffect, useRef, useState } from "react";
import { MicroButton } from "./micro-button";
import { cn } from "@/lib/utils";
import { Input } from "./input";

type CopyButtonProps = {
  value: string;
  label?: string;
  className?: string;
};

export function CopyButton(props: CopyButtonProps) {
  // A different source gets fresh feedback even if an older write is pending.
  return <CopyButtonValue key={props.value} {...props} />;
}

function CopyButtonValue({
  value,
  label = "Copy Hash",
  className,
}: CopyButtonProps) {
  const [state, setState] = useState<"idle" | "copying" | "copied" | "error">(
    "idle",
  );
  const writing = useRef(false);
  const copied = state === "copied";

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setState("idle"), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  async function copy() {
    if (writing.current) return;
    writing.current = true;
    setState("copying");
    try {
      await navigator.clipboard.writeText(value);
      setState("copied");
    } catch {
      setState("error");
    } finally {
      writing.current = false;
    }
  }

  return (
    <div
      className={cn(
        "inline-flex min-w-0 max-w-full flex-col items-start gap-2",
        className,
      )}
    >
      <MicroButton
        kind="copy"
        active={copied}
        onClick={() => void copy()}
        disabled={state === "copying" || !value}
      >
        {copied ? "Copied" : state === "copying" ? "Copying…" : label}
      </MicroButton>
      <span role="status" className="sr-only">
        {copied ? "Copied to clipboard." : ""}
      </span>
      {state === "error" ? (
        <div className="flex w-full min-w-0 flex-col gap-1">
          <p role="alert" className="text-xs text-destructive">
            Could not copy. Try again, or select and copy the value below.
          </p>
          <Input
            aria-label="Value to copy manually"
            readOnly
            value={value}
            onFocus={(event) => event.currentTarget.select()}
          />
        </div>
      ) : null}
    </div>
  );
}
