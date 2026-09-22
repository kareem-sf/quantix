import { useState } from "react";
import { ArrowUpRight, FileText, Link2 } from "lucide-react";
import type { Schema } from "../api";
import { BrandMark } from "../app/BrandMark";
import { Bubble, BubbleContent } from "@/components/ui/bubble";
import { Button } from "@/components/ui/button";
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item";
import {
  Message,
  MessageContent,
  MessageHeader,
} from "@/components/ui/message";
import { Citations, type SourceSelection } from "./Sources";
import { plainText, RichText } from "@/components/rich-text";
import { ApprovalList } from "./chat/ApprovalList";
import { ChoiceQuestion } from "./chat/ChoiceQuestion";

export function ManagerMessage({
  message,
  tenderId,
  onSource,
  onArtifact,
  answer,
  onAnswer,
  onAskChanges,
  onOpenApproval,
}: {
  message: Schema<"Message">;
  tenderId: string;
  onSource: (source: SourceSelection) => void;
  onArtifact?: (href: string, artifactId: string) => void;
  /** The engineer's reply after this message, which answers its question. */
  answer?: string;
  /** Sends a picked answer; absent when the question can't be answered now. */
  onAnswer?: (text: string) => Promise<unknown>;
  onAskChanges?: (text: string) => void;
  /** Opens one approval item in full in the side panel. */
  onOpenApproval?: (kind: string, id: string) => void;
}) {
  const long = isLong(message.content);
  const [expanded, setExpanded] = useState(!long);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [recordGroupsOpen, setRecordGroupsOpen] = useState<
    Record<string, boolean>
  >({});
  if (message.role !== "engineer" && message.role !== "manager") return null;
  const engineer = message.role === "engineer";
  const content = expanded ? message.content : compactPreview(message.content);
  // BOQ rows, findings and requirements are decided in the approval card.
  const groups = ["output", "work_product", "calculation"] as const;
  const links = (message.result_links ?? []).filter((link) =>
    (groups as readonly string[]).includes(link.kind),
  );
  const approvals = message.approvals ?? [];

  function recordLink(
    link: Schema<"ResultLink">,
    title = link.title,
    action = resultKindLabel(link.kind),
  ) {
    return (
      <Item
        key={`${link.kind}:${link.id}:${title}`}
        variant="outline"
        size="sm"
        className="bg-card"
        render={
          <a
            href={link.target}
            onClick={(event) => {
              event.preventDefault();
              if (onArtifact) onArtifact(link.target, link.id);
              else
                window.location.hash = link.target.startsWith("#")
                  ? link.target
                  : `#${link.target}`;
            }}
          />
        }
      >
        <ItemMedia variant="icon" className="size-8 rounded-md bg-muted">
          <FileText />
        </ItemMedia>
        <ItemContent>
          <ItemTitle>{title}</ItemTitle>
          <ItemDescription>{action}</ItemDescription>
        </ItemContent>
        <ItemActions>
          <ArrowUpRight className="size-4 text-muted-foreground rtl:-scale-x-100" />
        </ItemActions>
      </Item>
    );
  }

  return (
    <Message
      align={engineer ? "end" : "start"}
      className="manager-message"
      data-message-id={message.id}
    >
      {engineer ? null : <BrandMark size={32} className="self-start" />}
      <MessageContent>
        <MessageHeader className={engineer ? "justify-end px-1" : "px-0"}>
          <span className="text-foreground">
            {engineer ? "You" : "Tender Manager"}
          </span>
          <time dateTime={message.created_at} className="ms-2 font-normal">
            {formatTime(message.created_at)}
          </time>
        </MessageHeader>
        <Bubble
          variant={engineer ? "secondary" : "ghost"}
          align={engineer ? "end" : "start"}
          className={engineer ? "max-w-[85%]" : "max-w-full"}
        >
          <BubbleContent
            className={engineer ? "rounded-2xl px-4 py-2.5" : undefined}
          >
            <RichText
              text={content}
              className={engineer ? "bidi-text" : undefined}
              onSource={(sourceId) => onSource({ sourceId })}
            />
          </BubbleContent>
        </Bubble>
        {long ? (
          <Button
            type="button"
            variant="link"
            size="sm"
            className="h-auto self-start px-0 text-muted-foreground"
            aria-expanded={expanded}
            aria-label={expanded ? "Hide full reply" : "Show full reply"}
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? "Show less" : "Show full reply"}
          </Button>
        ) : null}
        {approvals.length ? (
          <ApprovalList
            tenderId={tenderId}
            approvals={approvals}
            onOpen={
              onOpenApproval
                ? (item) => onOpenApproval(item.kind, item.id)
                : undefined
            }
            onAskChanges={
              onAskChanges
                ? (items) =>
                    onAskChanges(
                      `About ${items.length === 1 ? `"${plainText(items[0].title, 80)}"` : `these ${items.length} items`}: `,
                    )
                : undefined
            }
          />
        ) : null}
        {message.question ? (
          <ChoiceQuestion
            question={message.question}
            answer={answer}
            onAnswer={onAnswer}
          />
        ) : null}
        {links.length ? (
          <div
            className="flex w-full max-w-xl flex-col gap-2"
            aria-label="Tender records"
          >
            {groups.map((kind) => {
              const records = links.filter((link) => link.kind === kind);
              if (records.length < 2)
                return records.map((link) => recordLink(link));
              const label =
                kind === "output"
                  ? "documents"
                  : kind === "work_product"
                    ? "saved drafts"
                    : "calculations";
              return (
                <div key={kind} className="flex flex-col gap-2">
                  {recordLink(
                    records[0],
                    `${records.length} ${label}`,
                    resultKindLabel(kind),
                  )}
                  <details
                    onToggle={(event) => {
                      const open = event.currentTarget.open;
                      setRecordGroupsOpen((current) => ({
                        ...current,
                        [kind]: open,
                      }));
                    }}
                  >
                    <summary className="cursor-pointer text-xs text-muted-foreground hover:text-foreground">
                      Show individual {label}
                    </summary>
                    {recordGroupsOpen[kind] ? (
                      <div className="mt-2 flex flex-col gap-2">
                        {records.map((link) => recordLink(link))}
                      </div>
                    ) : null}
                  </details>
                </div>
              );
            })}
          </div>
        ) : null}
        {message.source_ids.length ? (
          <details
            onToggle={(event) => setSourcesOpen(event.currentTarget.open)}
          >
            <summary className="flex w-fit cursor-pointer items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground">
              <Link2 className="size-3.5" aria-hidden="true" />
              {message.source_ids.length} source
              {message.source_ids.length === 1 ? "" : "s"}
            </summary>
            {sourcesOpen ? (
              <div className="mt-2">
                <Citations
                  ids={message.source_ids}
                  tenderId={tenderId}
                  onOpen={onSource}
                />
              </div>
            ) : null}
          </details>
        ) : null}
      </MessageContent>
    </Message>
  );
}

function isLong(content: string) {
  return content.length > 900 || content.split(/\n\s*\n/).length > 5;
}

function compactPreview(content: string) {
  const firstParagraph = content.split(/\n\s*\n/, 1)[0];
  // A whole first paragraph stays valid Markdown; cutting inside it would leave
  // stray ** or table pipes, so a long one becomes clean plain text instead.
  const preview =
    firstParagraph.length <= 280 && !/^\s*\|/.test(firstParagraph)
      ? firstParagraph.trimEnd()
      : plainText(firstParagraph, 280);
  return preview.length < content.length && !preview.endsWith("…")
    ? `${preview} …`
    : preview;
}

function formatTime(value: string) {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime())
    ? ""
    : parsed.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function resultKindLabel(kind: Schema<"ResultLink">["kind"]) {
  return {
    plan: "Review plan",
    task: "View task",
    finding: "Review finding",
    output: "Open document",
    requirement: "Review requirement",
    boq_item: "Review BOQ row",
    work_product: "Open draft",
    calculation: "Open calculation",
  }[kind];
}
