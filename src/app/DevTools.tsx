import { useEffect, useState } from "react";
import { useLocation } from "react-router-dom";
import { X } from "lucide-react";
import { useResource, type Schema } from "../api";
import { useTheme } from "../theme";
import { Button } from "@/components/ui/button";
import { DebugValues } from "@/components/ui/debug-panel";
import { Kbd } from "@/components/ui/kbd";

/**
 * Development-only panel with route, theme, service revisions and recent
 * runtime errors. Hidden until Ctrl+Shift+D opens it.
 */
export function DevTools() {
  const [open, setOpen] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);
  const [viewport, setViewport] = useState(() => ({
    width: window.innerWidth,
    height: window.innerHeight,
  }));
  const location = useLocation();
  const { theme, resolvedTheme } = useTheme();
  const health = useResource<Schema<"Health">>("/health");

  useEffect(() => {
    const push = (message: string) =>
      setErrors((current) => [
        ...current.slice(-4),
        `${new Date().toLocaleTimeString()} ${message}`,
      ]);
    const onKey = (event: KeyboardEvent) => {
      if (
        (event.ctrlKey || event.metaKey) &&
        event.shiftKey &&
        event.key.toLowerCase() === "d"
      ) {
        event.preventDefault();
        setOpen((value) => !value);
      }
    };
    const onError = (event: ErrorEvent) => push(event.message);
    const onRejection = (event: PromiseRejectionEvent) =>
      push(
        event.reason instanceof Error
          ? event.reason.message
          : String(event.reason),
      );
    const onResize = () =>
      setViewport({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("keydown", onKey);
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
      window.removeEventListener("resize", onResize);
    };
  }, []);

  return (
    <>
      {open ? (
        <div
          role="dialog"
          aria-label="Developer panel"
          className="fixed end-3 bottom-3 z-[100] flex w-80 max-w-[calc(100vw-1.5rem)] flex-col gap-2 rounded-xl border bg-popover/95 p-3 text-popover-foreground shadow-xl backdrop-blur"
        >
          <div className="flex items-center justify-between gap-2">
            <p className="flex items-center gap-2 text-xs font-medium">
              Developer panel <Kbd>Ctrl+Shift+D</Kbd>
            </p>
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              aria-label="Close developer panel"
              onClick={() => setOpen(false)}
            >
              <X />
            </Button>
          </div>
          <DebugValues
            values={{
              route: `${location.pathname}${location.search}`,
              theme: `${theme} (${resolvedTheme})`,
              viewport: `${viewport.width}×${viewport.height}`,
              version: health.data?.version,
              workspace: health.data?.workspace_revision,
              office: health.data?.office_revision,
              capabilities: health.data?.capabilities?.length ?? 0,
              online: navigator.onLine,
            }}
          />
          {errors.length ? (
            <div className="flex flex-col gap-1 border-t pt-2 font-mono text-xs text-red-600 dark:text-red-400">
              {errors.map((error, index) => (
                <p key={index} className="break-all">
                  {error}
                </p>
              ))}
            </div>
          ) : (
            <p className="border-t pt-2 text-xs text-muted-foreground">
              No runtime errors captured.
            </p>
          )}
        </div>
      ) : null}
    </>
  );
}
