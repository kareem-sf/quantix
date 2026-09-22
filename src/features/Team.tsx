import { useState } from "react";
import { ChevronDown, Send, Users } from "lucide-react";
import { AiOrb, orbStateFor } from "@/components/ui/thinking-orb";
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
import { cn } from "@/lib/utils";
import { RichText } from "@/components/rich-text";
import { TaskRows, type TaskRow } from "@/components/beautiful/task-rows";
import { emptyFilters } from "./activity/types";
import { useRunActivity } from "./activity/useRunActivity";
import { blockHeading, buildWorkLog } from "./work-log/model";
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
  running: "bg-sky-500",
  waiting: "bg-amber-500",
  completed: "bg-emerald-500",
  failed: "bg-destructive",
  cancelled: "bg-muted-foreground/60",
};

const recordText: Record<string, string> = {
  takeoff: "takeoff lines",
  boq_item_proposals: "BOQ rows",
  quantity_proposals: "quantities",
  submission_requirements: "submission requirements",
  project_map_nodes: "project map items",
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
  const [openMember, setOpenMember] = useState<string | null>(null);
  const [showPast, setShowPast] = useState(false);
  const staff = (team.data?.staff ?? []).filter(
    (member) => member.status === "active",
  );
  const assignments = team.data?.assignments ?? [];
  const byId = new Map(
    (team.data?.staff ?? []).map((member) => [member.id, member]),
  );
  const past = assignments.filter((assignment) =>
    ["completed", "failed", "cancelled"].includes(assignment.status),
  );
  const managerNow = useManagerNow(tenderId, managerRunId);

  const managerRow: TaskRow = managerRunId
    ? {
        id: "manager",
        label: "Tender Manager",
        note: managerNow || "Working",
        status: "running",
      }
    : {
        id: "manager",
        label: "Tender Manager",
        note: "Ready for your next request",
        status: "todo",
      };
  const rows: TaskRow[] = [
    ...staff.map((member): TaskRow => {
      const mine = assignments
        .filter((assignment) => assignment.staff_id === member.id)
        .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
      const current = mine.find((assignment) =>
        ["queued", "running", "waiting"].includes(assignment.status),
      );
      const last = mine[0];
      const summary = last?.result?.summary?.split("\n")[0]?.trim();
      return {
        id: member.id,
        label: `${member.name} · ${member.role}`,
        note: current
          ? current.status === "waiting"
            ? `Asked the Tender Manager: ${current.question ?? current.title}`
            : current.status === "queued"
              ? `Starting next: ${current.title}`
              : current.title
          : last?.status === "completed"
            ? `Finished: ${summary || last.title}`
            : last
              ? "Idle · last job stopped"
              : "Idle",
        meta:
          current?.status === "running" || !last
            ? undefined
            : formatTime(last.updated_at),
        status: current
          ? current.status === "running"
            ? "running"
            : current.status === "waiting"
              ? "blocked"
              : "todo"
          : last?.status === "completed"
            ? "done"
            : "todo",
      };
    }),
  ];
  const selected = openMember ? byId.get(openMember) : undefined;
  const toggleMember = (row: TaskRow) => {
    if (row.id === "manager") return;
    setOpenMember((current) => (current === row.id ? null : row.id));
  };

  return (
    <section
      aria-label="Tender team"
      className="flex flex-col gap-4 @container"
    >
      <h2 className="text-sm font-medium">Team</h2>
      <ErrorNotice error={team.error} />
      {team.isPending ? <Loading>Loading the team…</Loading> : null}
      {team.data ? (
        <TaskRows
          aria-label="Team"
          rows={[managerRow, ...rows]}
          showStatusPill={false}
          onSelect={toggleMember}
        />
      ) : null}
      {team.data && !staff.length ? (
        <p className="flex items-center gap-2 text-sm text-muted-foreground">
          <Users className="size-4" />
          No staff yet. The Tender Manager hires them when the work needs more
          hands.
        </p>
      ) : null}
      {selected ? (
        <StaffCard
          tenderId={tenderId}
          member={selected}
          assignments={assignments.filter(
            (assignment) => assignment.staff_id === selected.id,
          )}
          onSource={onSource}
          onClose={() => setOpenMember(null)}
        />
      ) : null}
      {past.length ? (
        <div className="flex flex-col gap-2">
          <Button
            variant="ghost"
            size="sm"
            className="-ms-2 self-start text-muted-foreground"
            aria-expanded={showPast}
            onClick={() => setShowPast((value) => !value)}
          >
            <ChevronDown
              data-icon="inline-start"
              className={cn("transition-transform", !showPast && "-rotate-90")}
            />
            Past work ({past.length})
          </Button>
          {showPast ? (
            <ol className="flex flex-col gap-2" aria-label="Past work">
              {past.map((assignment) => (
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
          ) : null}
        </div>
      ) : null}
    </section>
  );
}

/** The Tender Manager's current action, from its live work log. */
function useManagerNow(tenderId: string, runId?: string) {
  const activity = useRunActivity(
    tenderId,
    runId ?? "none",
    emptyFilters,
    Boolean(runId),
    true,
  );
  if (!runId) return "";
  const log = buildWorkLog(activity.data?.items ?? []);
  const last = log.blocks.at(-1);
  if (!last) return "";
  const running = last.facts.find((fact) => fact.state === "running");
  if (running) return [running.line, running.subject].filter(Boolean).join(" ");
  return blockHeading(last);
}

function StaffCard({
  tenderId,
  member,
  assignments,
  onSource,
  onClose,
}: {
  tenderId: string;
  member: Staff;
  assignments: Assignment[];
  onSource: (source: SourceSelection) => void;
  onClose: () => void;
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
      className="bui-fade-up flex flex-col gap-3 rounded-lg bg-card p-3 text-sm ring-1 ring-border"
      aria-label={member.name}
    >
      <div className="flex items-start gap-3">
        <StaffPortrait
          portrait={{
            style: NOTIONISTS_RECIPE.styleId,
            seed: member.portrait_seed,
          }}
          name={member.name}
          size={40}
          decorative
        />
        <div className="min-w-0 flex-1">
          <p className="font-medium">{member.name}</p>
          <p className="text-xs text-muted-foreground">{member.role}</p>
        </div>
        <Button variant="ghost" size="xs" onClick={onClose}>
          Close
        </Button>
      </div>
      <div className="flex flex-wrap gap-1">
        {member.specialisms.map((specialism) => (
          <Badge key={specialism} variant="secondary" className="font-normal">
            {specialism}
          </Badge>
        ))}
      </div>
      <p className="whitespace-pre-wrap">{member.background}</p>
      <p className="whitespace-pre-wrap text-muted-foreground">
        {member.working_style}
      </p>
      {assignments.length ? (
        <ol
          className="flex flex-col gap-2"
          aria-label={`Work for ${member.name}`}
        >
          {assignments.map((assignment) => (
            <li key={assignment.id}>
              <AssignmentCard
                tenderId={tenderId}
                assignment={assignment}
                staff={member}
                onSource={onSource}
              />
            </li>
          ))}
        </ol>
      ) : null}
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
  return (
    <details
      className="group rounded-xl border bg-card text-sm"
      open={assignment.status === "waiting" || undefined}
    >
      <summary className="flex cursor-pointer list-none items-start gap-3 p-3">
        {assignment.status === "running" ? (
          <AiOrb state={orbStateFor(assignment.title)} className="-mx-1.5" />
        ) : (
          <span
            aria-hidden="true"
            className={cn(
              "mt-1.5 size-2 shrink-0 rounded-full",
              statusTone[assignment.status],
            )}
          />
        )}
        <span className="min-w-0 flex-1">
          <span className="block font-medium">{assignment.title}</span>
          <span className="block text-xs text-muted-foreground">
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
            <RichText text={result.summary} className="text-sm" />
            {result.findings?.length ? (
              <ul className="flex flex-col gap-2" aria-label="Findings">
                {result.findings.map((finding, index) => (
                  <li
                    key={`${finding.title}:${index}`}
                    className="flex flex-col gap-1"
                  >
                    <span className="flex flex-wrap items-center gap-1.5">
                      <Badge variant="outline" className="font-normal">
                        {finding.kind}
                      </Badge>
                      <span className="font-medium">{finding.title}</span>
                    </span>
                    <span className="text-muted-foreground">
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
            {Object.keys(result.saved_records ?? {}).length ? (
              <p className="text-xs text-muted-foreground">
                Saved for your review:{" "}
                {Object.entries(result.saved_records ?? {})
                  .map(
                    ([kind, total]) => `${total} ${recordText[kind] ?? kind}`,
                  )
                  .join(", ")}
              </p>
            ) : null}
          </div>
        ) : null}
      </div>
    </details>
  );
}

function Field({ label, children }: { label: string; children: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted-foreground">{label}</span>
      <p className="whitespace-pre-wrap">{children}</p>
    </div>
  );
}

function formatTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? ""
    : date.toLocaleString([], { dateStyle: "short", timeStyle: "short" });
}
