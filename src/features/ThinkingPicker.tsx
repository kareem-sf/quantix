import { useState } from "react";
import { Brain, ChevronDown } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice } from "../components/common";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { InputGroupButton } from "@/components/ui/input-group";
import { cn } from "@/lib/utils";
import { composerChip } from "./ModelPicker";

type Thinking = Schema<"TenderThinking">;

/** Radio values must be strings; the provider's own default has no level. */
const DEFAULT = "default";

/**
 * How much the Tender Manager's AI thinks before answering. Every provider's
 * own levels are offered, lightest first; less thinking uses less allowance.
 */
export function ThinkingPicker({
  tenderId,
  busy,
}: {
  tenderId: string;
  /** Tender work is running; the setting applies to the next instruction. */
  busy: boolean;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const thinking = useResource<Thinking>(`${tenderPath(tenderId)}/ai-thinking`);
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [announcement, setAnnouncement] = useState("");

  const data = thinking.data;
  // Nothing to choose until an AI runs this Tender and reports its levels.
  if (!data?.current || data.options.length < 2) return null;
  const current = data.current;
  const locked = busy || data.busy || saving;

  async function choose(value: string) {
    const option = data?.options.find(
      (item) => (item.value ?? DEFAULT) === value,
    );
    if (!option || locked || option.value === current.value) return;
    setSaving(true);
    setError(null);
    try {
      await api.post<Thinking>(`${tenderPath(tenderId)}/ai-thinking`, {
        reasoning: option.value,
        engineer_confirmed: true,
      } satisfies Schema<"TenderThinkingInput">);
      await refresh();
      setAnnouncement(`Thinking set to ${option.label}.`);
      setOpen(false);
    } catch (failure) {
      setError(failure);
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <DropdownMenu
        open={open}
        onOpenChange={(next) => {
          setOpen(next);
          if (next) setError(null);
        }}
      >
        <DropdownMenuTrigger
          render={
            <InputGroupButton
              size="xs"
              variant="ghost"
              className={cn(composerChip, "h-7 gap-1 px-2 text-xs")}
              aria-label={`Thinking: ${current.label}. Change how much the AI thinks`}
              title="How much the AI thinks"
            />
          }
        >
          <Brain className="size-3.5" />
          <span>{current.label}</span>
          <ChevronDown className="size-3.5 opacity-60" />
        </DropdownMenuTrigger>
        <DropdownMenuContent
          side="top"
          align="start"
          sideOffset={10}
          className="w-[min(18rem,calc(100vw-2rem))] p-1"
        >
          <DropdownMenuGroup>
            <DropdownMenuLabel className="flex flex-col gap-0.5 px-1.5 py-1">
              <span className="text-sm font-medium text-foreground">
                Thinking
              </span>
              <span className="text-xs font-normal">
                {locked && !saving
                  ? "Finish or stop the current work to change this."
                  : "Less thinking answers faster and uses less of your AI allowance."}
              </span>
            </DropdownMenuLabel>
            <DropdownMenuRadioGroup
              value={current.value ?? DEFAULT}
              onValueChange={(value) => void choose(String(value))}
            >
              {data.options.map((option) => (
                <DropdownMenuRadioItem
                  key={option.value ?? DEFAULT}
                  value={option.value ?? DEFAULT}
                  disabled={locked}
                  className="items-start py-1.5"
                >
                  <span className="flex min-w-0 flex-col">
                    <span>{option.label}</span>
                    <span className="text-xs text-muted-foreground">
                      {option.detail}
                    </span>
                  </span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuGroup>
          {error ? (
            <div className="px-1.5 pb-1">
              <ErrorNotice error={error} />
            </div>
          ) : null}
        </DropdownMenuContent>
      </DropdownMenu>
      <span className="sr-only" aria-live="polite">
        {announcement}
      </span>
    </>
  );
}
