import { useState } from "react";
import { ChevronDown, Send, Users } from "lucide-react";
import {
  tenderPath,
  useApi,
  useRefresh,
  useResource,
  type Schema,
} from "../api";
import { ErrorNotice, Loading } from "../components/common";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { CurrentWork } from "./office/CurrentWork";
import { Citations, type SourceSelection } from "./Sources";
import { NOTIONISTS_RECIPE, StaffPortrait } from "./StaffPortrait";

type Staff = Schema<"StaffMember">;
type Assignment = Schema<"Assignment">;

const statusText: Record<Assignment["status"], string> = {
  queued: "Queued",
  running: "Working",
  waiting: "Asked the Manager",
  completed: "Done",
  failed: "Failed",
  cancelled: "Cancelled",
};

const statusTone: Record<Assignment["status"], string> = {
  queued: "bg-sky-500",
  running: "bg-sky-500 animate-pulse",
  waiting: "bg-amber-500",
  completed: "bg-emerald-500",
  failed: "bg-destructive",
  cancelled: "bg-muted-foreground/60",
};

/** The staff the Tender Manager hired for this tender and the work it gave them. */
export function Team({
  tenderId,
  managerRunId,
  onSource,
}: {
  tenderId: string;
  /** The running Manager's work, when the engineer can steer it. */
  managerRunId?: string;
  onSource: (source: SourceSelection) => void;
}) {
  const team = useResource<Schema<"TeamView">>(
    `${tenderPath(tenderId)}/team`,
    true,
  );
  const [selected, setSelected] = useState<string | null>(null);
  const staff = team.data?.staff ?? [];
  const assignments = team.data?.assignments ?? [];
  const byId = new Map(staff.map((member) => [member.id, member]));
  const shown = selected
    ? assignments.filter((assignment) => assignment.staff_id === selected)
    : assignments;

  return (
    <section aria-label="Tender team" className="flex flex-col gap-5 @container">
      <CurrentWork tenderId={tenderId} onSource={onSource} />
      {managerRunId ? (
        <SteerManager tenderId={tenderId} runId={managerRunId} />
      ) : null}
      <div className="flex flex-col gap-0.5">
        <h2 className="text-sm font-medium">Team</h2>
        <p className="text-xs text-muted-foreground">
          The Tender Manager hires staff for this tender and assigns their work.
        </p>
      </div>
      <ErrorNotice error={team.error} />
      {team.isPending ? <Loading>Loading the team…</Loading> : null}
      {team.data && !staff.length ? (
        <div className="flex flex-col items-center gap-2 rounded-xl border border-dashed p-6 text-center">
          <Users className="size-5 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">
            No staff yet. The Manager hires them when the work needs more hands.
          </p>
        </div>
      ) : null}
      {staff.length ? (
        <ul
          className="grid gap-2 @md:grid-cols-2"
          aria-label="Staff"
        >
          {staff.map((member) => (
            <li key={member.id}>
              <StaffCard
                tenderId={tenderId}
                member={member}
                assignments={assignments.filter(
                  (assignment) => assignment.staff_id === member.id,
                )}
                selected={selected === member.id}
                onSelect={() =>
                  setSelected((current) =>
                    current === member.id ? null : member.id,
                  )
                }
              />
            </li>
          ))}
        </ul>
      ) : null}
      {assignments.length ? (
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <h3 className="text-xs font-medium text-muted-foreground">
              {selected
                ? `Work for ${byId.get(selected)?.name ?? "this staff member"}`
                : "All work"}
            </h3>
            {selected ? (
              <Button
                variant="ghost"
                size="xs"
                onClick={() => setSelected(null)}
              >
                Show all
              </Button>
            ) : null}
          </div>
          <ol className="flex flex-col gap-2" aria-label="Assignments">
            {shown.map((assignment) => (
              <li key={assignment.id}>
                <AssignmentCard
                  tenderId={tenderId}
                  assignment={assignment}
                  staff={byId.get(assignment.staff_id)}
                  onSource={onSource}
                />
              </li>
            ))}
          </ol>
        </div>
      ) : null}
    </section>
  );
}

function StaffCard({
  tenderId,
  member,
  assignments,
  selected,
  onSelect,
}: {
  tenderId: string;
  member: Staff;
  assignments: Assignment[];
  selected: boolean;
  onSelect: () => void;
}) {
  const api = useApi();
  const refresh = useRefresh();
  const [error, setError] = useState<unknown>(null);
  const [retiring, setRetiring] = useState(false);
  const open = assignments.filter((assignment) =>
    ["queued", "running", "waiting"].includes(assignment.status),
  ).length;

  async function retire() {
    setRetiring(true);
    setError(null);
    try {
      await api.post<Staff>(
        `${tenderPath(tenderId)}/team/staff/${encodeURIComponent(member.id)}/retire`,
      );
      await refresh();
    } catch (failure) {
      setError(failure);
    } finally {
      setRetiring(false);
    }
  }

  return (
    <article
      className={cn(
        "flex h-full flex-col gap-2 rounded-xl border bg-card p-3 text-sm",
        selected && "border-ring ring-2 ring-ring/30",
        member.status === "retired" && "opacity-70",
      )}
      aria-label={member.name}
    >
      <button
        type="button"
        className="flex items-start gap-3 text-start"
        aria-pressed={selected}
        onClick={onSelect}
      >
        <StaffPortrait
          portrait={{ style: NOTIONISTS_RECIPE.styleId, seed: member.portrait_seed }}
          name={member.name}
          size={40}
          decorative
        />
        <span className="min-w-0 flex-1">
          <span className="block truncate font-medium" dir="auto">
            {member.name}
          </span>
          <span className="block truncate text-xs text-muted-foreground" dir="auto">
            {member.role}
          </span>
          <span className="mt-1 block text-xs text-muted-foreground">
            {member.status === "retired"
              ? "Retired"
              : open
                ? `${open} open · ${assignments.length} total`
                : `${assignments.length} ${assignments.length === 1 ? "assignment" : "assignments"}`}
          </span>
        </span>
      </button>
      <div className="flex flex-wrap gap-1">
        {member.specialisms.map((specialism) => (
          <Badge key={specialism} variant="secondary" className="font-normal">
            {specialism}
          </Badge>
        ))}
      </div>
      <details className="group text-xs">
        <summary className="flex cursor-pointer list-none items-center gap-1 text-muted-foreground">
          Profile
          <ChevronDown className="size-3 transition-transform group-open:rotate-180" />
        </summary>
        <div className="mt-2 flex flex-col gap-2">
          <p className="whitespace-pre-wrap" dir="auto">
            {member.background}
          </p>
          <p className="whitespace-pre-wrap text-muted-foreground" dir="auto">
            {member.working_style}
          </p>
          {member.status === "active" ? (
            <Button
              variant="outline"
              size="xs"
              className="self-start"
              disabled={retiring || open > 0}
              title={open ? "Wait until their open work is finished." : undefined}
              onClick={() => void retire()}
            >
              Retire
            </Button>
          ) : null}
        </div>
      </details>
      <ErrorNotice error={error} />
    </article>
  );
}

function AssignmentCard({
  tenderId,
  assignment,
  staff,
  onSource,
}: {
  tenderId: string;
  assignment: Assignment;
  staff?: Staff;
  onSource: (source: SourceSelection) => void;
}) {
  const result = assignment.result;
  const usage = assignment.usage ?? {};
  const tokens =
    Number(usage.input_tokens ?? 0) + Number(usage.output_tokens ?? 0);
  return (
    <details
      className="group rounded-xl border bg-card text-sm"
      open={assignment.status === "waiting" || undefined}
    >
      <summary className="flex cursor-pointer list-none items-start gap-3 p-3">
        <span
          aria-hidden="true"
          className={cn(
            "mt-1.5 size-2 shrink-0 rounded-full",
            statusTone[assignment.status],
          )}
        />
        <span className="min-w-0 flex-1">
          <span className="block font-medium" dir="auto">
            {assignment.title}
          </span>
          <span className="block text-xs text-muted-foreground" dir="auto">
            {staff?.name ?? "Staff member"} · {statusText[assignment.status]} ·{" "}
            {formatTime(assignment.updated_at)}
          </span>
        </span>
        <ChevronDown className="mt-0.5 size-4 shrink-0 text-muted-foreground transition-transform group-open:rotate-180" />
      </summary>
      <div className="flex flex-col gap-3 px-3 pb-3 ps-8">
        <Field label="Brief">{assignment.brief}</Field>
        <Field label="Expected result">{assignment.expected_result}</Field>
        {assignment.question ? (
          <Field label="Question for the Manager">{assignment.question}</Field>
        ) : null}
        {assignment.answer ? (
          <Field label="Manager's answer">{assignment.answer}</Field>
        ) : null}
        {assignment.status === "failed" && assignment.detail ? (
          <p className="text-destructive" role="alert">
            {assignment.detail}
          </p>
        ) : null}
        {result ? (
          <div className="flex flex-col gap-2 rounded-lg bg-muted/50 p-3">
            <p className="whitespace-pre-wrap" dir="auto">
              {result.summary}
            </p>
            {result.findings?.length ? (
              <ul className="flex flex-col gap-2" aria-label="Findings">
                {result.findings.map((finding, index) => (
                  <li key={`${finding.title}:${index}`} className="flex flex-col gap-1">
                    <span className="flex flex-wrap items-center gap-1.5">
                      <Badge variant="outline" className="font-normal">
                        {finding.kind}
                      </Badge>
                      <span className="font-medium" dir="auto">
                        {finding.title}
                      </span>
                    </span>
                    <span className="text-muted-foreground" dir="auto">
                      {finding.detail}
                    </span>
                    {finding.source_ids?.length ? (
                      <Citations
                        ids={finding.source_ids}
                        tenderId={tenderId}
                        onOpen={onSource}
                      />
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : null}
            {result.source_ids?.length ? (
              <Citations
                ids={result.source_ids}
                tenderId={tenderId}
                onOpen={onSource}
              />
            ) : null}
          </div>
        ) : null}
        <p className="text-xs text-muted-foreground">
          {assignment.model_id}
          {usage.requests
            ? ` · ${Number(usage.requests)} ${Number(usage.requests) === 1 ? "request" : "requests"}`
            : ""}
          {tokens ? ` · ${tokens.toLocaleString()} tokens` : ""}
        </p>
      </div>
    </details>
  );
}

function Field({ label, children }: { label: string; children: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted-foreground">{label}</span>
      <p className="whitespace-pre-wrap" dir="auto">
        {children}
      </p>
    </div>
  );
}

function SteerManager({ tenderId, runId }: { tenderId: string; runId: string }) {
  const api = useApi();
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<unknown>(null);

  async function send() {
    const content = text.trim();
    if (!content) return;
    setSending(true);
    setError(null);
    try {
      await api.post<Schema<"InstructionAdmission">>(
        `${tenderPath(tenderId)}/runs/${encodeURIComponent(runId)}/steering`,
        {
          kind: "constraint",
          content,
          selection: "",
          idempotency_key: `steer-${runId}-${Date.now()}`,
        } satisfies Schema<"InstructionRevisionRequest">,
      );
      setText("");
      setSent(true);
    } catch (failure) {
      setError(failure);
    } finally {
      setSending(false);
    }
  }

  return (
    <form
      className="flex flex-col gap-2 rounded-xl border bg-card p-3"
      onSubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      <label className="flex flex-col gap-1.5 text-sm">
        Steer the Manager while it works
        <Textarea
          dir="auto"
          rows={2}
          maxLength={20000}
          value={text}
          onChange={(event) => {
            setText(event.target.value);
            setSent(false);
          }}
          placeholder="For example: use the revision C drawings only"
        />
      </label>
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs text-muted-foreground" role="status">
          {sent ? "Sent. The Manager applies it at its next step." : ""}
        </p>
        <Button type="submit" size="sm" disabled={sending || !text.trim()}>
          <Send data-icon="inline-start" />
          Send
        </Button>
      </div>
      <ErrorNotice error={error} />
    </form>
  );
}

function formatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? ""
    : date.toLocaleString([], { dateStyle: "short", timeStyle: "short" });
}
