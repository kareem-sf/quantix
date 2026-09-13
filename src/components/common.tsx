import type { ReactNode } from "react";
import { CircleAlert } from "lucide-react";
import { ApiError, errorText } from "../api";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Empty as EmptyRoot,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import { ActivityDots } from "@/components/ui/activity-dots";
import { cn } from "@/lib/utils";

/** Shared app primitives built on shadcn/ui components. */

export function ErrorNotice({ error }: { error: unknown }) {
  if (!error) return null;
  const message = error instanceof ApiError ? error.message : errorText(error);
  const reference = error instanceof ApiError ? error.requestId : undefined;
  return (
    <Alert variant="destructive" className="my-3">
      <CircleAlert />
      <AlertDescription className="text-destructive/90">
        <span className="wrap-anywhere">{message}</span>
        {reference ? (
          <details className="mt-1">
            <summary className="cursor-pointer text-xs text-muted-foreground">
              More options
            </summary>
            <p className="mt-1 text-xs text-muted-foreground">
              Request reference: <code className="font-mono">{reference}</code>
            </p>
          </details>
        ) : null}
      </AlertDescription>
    </Alert>
  );
}

export function Loading({ children = "Loading…" }: { children?: ReactNode }) {
  return (
    <div
      role="status"
      className="flex items-center gap-2 p-6 text-sm text-muted-foreground"
    >
      <ActivityDots />
      {children}
    </div>
  );
}

export function Empty({
  title,
  children,
  action,
}: {
  title: string;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <EmptyRoot className="py-12">
      <EmptyHeader>
        <EmptyTitle>
          <h2 className="text-base font-medium">{title}</h2>
        </EmptyTitle>
        {children ? <EmptyDescription>{children}</EmptyDescription> : null}
      </EmptyHeader>
      {action ? <EmptyContent>{action}</EmptyContent> : null}
    </EmptyRoot>
  );
}

const statusTones: Record<string, string> = {
  extracted: "bg-emerald-500",
  completed: "bg-emerald-500",
  accepted: "bg-emerald-500",
  approved: "bg-emerald-500",
  resolved: "bg-emerald-500",
  confirmed: "bg-emerald-500",
  failed: "bg-destructive",
  rejected: "bg-destructive",
  needs_attention: "bg-amber-500",
  needs_review: "bg-amber-500",
  unsupported: "bg-amber-500",
  interrupted: "bg-amber-500",
  proposed: "bg-amber-500",
  running: "bg-sky-500",
  queued: "bg-sky-500",
};

export function Status({ value }: { value: string }) {
  return (
    <Badge
      role="status"
      variant="outline"
      className="gap-1.5 font-normal text-muted-foreground"
    >
      <span
        aria-hidden="true"
        className={cn(
          "size-1.5 rounded-full",
          statusTones[value] ?? "bg-muted-foreground/60",
        )}
      />
      {statusLabel(value)}
    </Badge>
  );
}

export function statusLabel(value: string) {
  const labels: Record<string, string> = {
    extracted: "Read",
    unsupported: "Not readable",
    needs_attention: "Needs attention",
  };
  return (
    labels[value] ??
    value.replaceAll("_", " ").replace(/^./, (letter) => letter.toUpperCase())
  );
}

/**
 * A dialog that is open while mounted. `drawer` presents it as a side sheet.
 * Focus trapping, background inertness and focus return come from Base UI.
 */
export function Modal({
  title,
  onClose,
  children,
  drawer = false,
  legacy = true,
  drawerClassName,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  drawer?: boolean;
  /** Content not yet rebuilt on shadcn keeps its element-level styles. */
  legacy?: boolean;
  drawerClassName?: string;
}) {
  const onOpenChange = (open: boolean) => {
    if (!open) onClose();
  };
  if (drawer)
    return (
      <Sheet open onOpenChange={onOpenChange}>
        <SheetContent
          side="right"
          className={cn(
            "w-full gap-0 overflow-y-auto data-[side=right]:sm:max-w-2xl",
            drawerClassName,
          )}
        >
          <SheetHeader className="border-b pe-12">
            <SheetTitle dir="auto">{title}</SheetTitle>
          </SheetHeader>
          <div className={cn("flex-1 p-4", legacy && "legacy-screen")}>
            {children}
          </div>
        </SheetContent>
      </Sheet>
    );
  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent
        className={cn(
          "max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-lg",
          legacy && "legacy-screen",
        )}
      >
        <DialogHeader className="pe-8">
          <DialogTitle dir="auto">{title}</DialogTitle>
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  );
}
