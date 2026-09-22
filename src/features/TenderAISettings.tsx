import { useState } from "react";
import { useResource, tenderPath, type Schema } from "../api";
import { ErrorNotice, Loading } from "../components/common";
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select";
import { ModelPicker } from "./ModelPicker";
import { ThinkingPicker } from "./ThinkingPicker";
import { TenderAI } from "./TenderAI";
import { tenderDisplayName } from "../app/tender-name";

/**
 * The AI each tender uses, how much it thinks, and what it may spend. The AI
 * and its thinking can also be switched from the Manager's message box.
 */
export function TenderAISettings({
  tenderId: initialTenderId,
  onManageAccounts,
}: {
  tenderId?: string;
  onManageAccounts: () => void;
}) {
  const tenders = useResource<Schema<"Tender">[]>("/tenders");
  const [chosen, setChosen] = useState(initialTenderId);
  const list = tenders.data ?? [];
  const tenderId =
    chosen && list.some((tender) => tender.id === chosen)
      ? chosen
      : list[0]?.id;
  const policy = useResource<Schema<"TenderAIRecord">>(
    `${tenderPath(tenderId ?? "none")}/ai-policy`,
    Boolean(tenderId),
  );
  const overview = useResource<Schema<"Overview">>(
    tenderPath(tenderId ?? "none"),
    Boolean(tenderId),
  );
  const [pickerOpen, setPickerOpen] = useState(false);

  if (tenders.isPending) return <Loading>Loading tenders…</Loading>;
  if (!list.length)
    return (
      <p className="text-sm text-muted-foreground">
        Create a tender first, then choose its AI here.
      </p>
    );
  const busy = (overview.data?.active_runs.length ?? 0) > 0;
  const record =
    policy.data && !Array.isArray(policy.data) ? policy.data : undefined;

  return (
    <section
      aria-label="Tender AI and spending"
      className="flex flex-col gap-6"
    >
      <div className="flex flex-col gap-1">
        <h2 className="text-base font-medium">Tender AI and spending</h2>
        <p className="text-sm text-muted-foreground">
          The AI each tender uses, how much it thinks, and what it may spend.
        </p>
      </div>
      <ErrorNotice error={tenders.error ?? policy.error} />
      {list.length > 1 ? (
        <label className="flex flex-col gap-1.5 text-sm">
          Tender
          <NativeSelect
            value={tenderId}
            onChange={(event) => setChosen(event.target.value)}
            className="max-w-md"
          >
            {list.map((tender) => (
              <NativeSelectOption key={tender.id} value={tender.id}>
                {tenderDisplayName(tender)}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </label>
      ) : null}
      {tenderId ? (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-sm text-muted-foreground">
              AI for this tender
            </span>
            <ModelPicker
              tenderId={tenderId}
              policy={record}
              policyPending={policy.isPending}
              busy={busy}
              open={pickerOpen}
              onOpenChange={setPickerOpen}
              onManageAccounts={onManageAccounts}
              onAdvanced={() =>
                document
                  .getElementById("tender-ai-limits")
                  ?.scrollIntoView?.({ block: "start" })
              }
            />
            <ThinkingPicker tenderId={tenderId} busy={busy} />
          </div>
          <div id="tender-ai-limits">
            <TenderAI key={tenderId} tenderId={tenderId} />
          </div>
        </>
      ) : null}
    </section>
  );
}
