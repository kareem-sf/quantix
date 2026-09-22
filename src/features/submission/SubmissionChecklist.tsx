import { Check, Circle, CircleDot, FileText, Pause } from "lucide-react";
import { tenderPath, useResource, type Schema } from "../../api";
import { ErrorNotice, Loading } from "../../components/common";
import { Button } from "@/components/ui/button";
import { GlideMenu } from "@/components/beautiful/glide-menu";
import { cn } from "@/lib/utils";
import { rtlDir } from "@/lib/text-direction";
import { locatorLabel } from "@/lib/locator";
import { AskManagerButton } from "../chat/AskManagerButton";

type Requirement = Schema<"RequirementRecord">;
export type ChecklistState =
  "ready" | "exception" | "drafted" | "approval" | "missing";

/** Where one requirement stands, in the words an engineer would use. */
export function checklistState(requirement: Requirement): ChecklistState {
  if (requirement.status === "proposed") return "approval";
  const linked = requirement.linked_outputs.filter(
    (output) => output.is_current && output.available,
  );
  if (
    requirement.review_status === "satisfied" &&
    requirement.review_is_current
  )
    return "ready";
  if (
    requirement.review_status === "exception" &&
    requirement.review_is_current
  )
    return "exception";
  if (linked.length) return "drafted";
  return "missing";
}

const STATES: Record<
  ChecklistState,
  { word: string; tone: string; icon: typeof Check }
> = {
  ready: { word: "ready", tone: "text-(--success)", icon: Check },
  // Handled outside Quantix with a recorded reason, such as a bank's bid bond.
  exception: {
    word: "ready · exception",
    tone: "text-(--success)",
    icon: Check,
  },
  drafted: {
    word: "drafted · review",
    tone: "text-foreground",
    icon: CircleDot,
  },
  approval: {
    word: "waiting for your approval",
    tone: "text-(--warning-ink)",
    icon: Pause,
  },
  missing: { word: "missing", tone: "text-muted-foreground", icon: Circle },
};

/**
 * One checklist of what the tender asks the bidder to submit: where each item
 * comes from, the file that covers it, and whether it is ready.
 */
export function SubmissionChecklist({
  tenderId,
  onOpen,
  onBuild,
  onOpenManager,
}: {
  tenderId: string;
  onOpen: (requirementId: string) => void;
  /** Takes the engineer to the chat after handing the Manager a job. */
  onOpenManager?: () => void;
  onBuild: () => void;
}) {
  const requirements = useResource<Requirement[]>(
    `${tenderPath(tenderId)}/requirements?include_withdrawn=false&offset=0&limit=100`,
  );
  if (requirements.isPending) return <Loading>Loading the checklist…</Loading>;
  const items = requirements.data ?? [];
  const ready = items.filter((item) =>
    ["ready", "exception"].includes(checklistState(item)),
  ).length;
  const outstanding = items.length - ready;

  return (
    <section aria-label="Submission checklist" className="flex flex-col gap-3">
      <ErrorNotice error={requirements.error} />
      <h3 className="text-sm font-medium">
        {items.length
          ? `${ready} of ${items.length} ready`
          : "Nothing on the checklist yet"}
      </h3>
      {items.length ? (
        <div className="overflow-hidden rounded-lg bg-card ring-1 ring-border">
          <GlideMenu
            className="flex flex-col"
            rowSelector="[data-checklist-row]"
          >
            {items.map((item) => {
              const state = checklistState(item);
              const meta = STATES[state];
              const source = item.sources[0];
              const file =
                item.linked_outputs.find((output) => output.is_current) ??
                item.linked_outputs[0];
              return (
                <button
                  key={item.id}
                  type="button"
                  data-checklist-row
                  onClick={() => onOpen(item.id)}
                  className="relative z-10 flex w-full items-start gap-3 border-b px-3 py-2.5 text-start last:border-0"
                >
                  <meta.icon
                    aria-hidden
                    className={cn("mt-0.5 size-4 shrink-0", meta.tone)}
                  />
                  <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span
                      dir={rtlDir(item.title)}
                      className="text-sm font-medium wrap-anywhere"
                    >
                      {item.title}
                    </span>
                    {source ? (
                      <span className="text-xs text-muted-foreground wrap-anywhere">
                        from {source.artifact_name}
                        {source.locator
                          ? ` · ${locatorLabel(source.locator)}`
                          : ""}
                      </span>
                    ) : null}
                  </span>
                  <span className="flex shrink-0 flex-col items-end gap-0.5 text-xs">
                    <span className={meta.tone}>{meta.word}</span>
                    {file ? (
                      <span className="flex items-center gap-1 text-muted-foreground">
                        <FileText aria-hidden className="size-3" />
                        <span className="max-w-40 truncate">
                          {file.filename}
                        </span>
                      </span>
                    ) : null}
                  </span>
                </button>
              );
            })}
          </GlideMenu>
        </div>
      ) : (
        <div className="flex flex-col items-start gap-3">
          <p className="text-sm text-muted-foreground">
            The Tender Manager can read the package and list everything the
            tender asks you to submit.
          </p>
          <AskManagerButton
            tenderId={tenderId}
            onSent={onOpenManager}
            request="Find everything the tender asks us to submit with our bid: documents, forms, certificates, guarantees and samples, with deadlines and where each is stated. List them as submission requirements for my approval."
          >
            Ask the Tender Manager to find them
          </AskManagerButton>
        </div>
      )}
      {items.length ? (
        <div className="flex flex-wrap items-center gap-2">
          <Button
            type="button"
            size="sm"
            disabled={outstanding > 0}
            onClick={onBuild}
          >
            Build submission package
          </Button>
          {outstanding ? (
            <span className="text-xs text-muted-foreground">
              {outstanding} not ready yet
            </span>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
