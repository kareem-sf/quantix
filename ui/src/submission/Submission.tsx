import { useState, type FormEvent } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { money } from "../estimate/queries";
import { Face } from "../office/Face";
import { firstName, useOffice, type Staff } from "../office/queries";
import { dueShort } from "../tenders/due";
import { useTender } from "../tenders/queries";
import {
  useAddRequirement,
  useAttach,
  useBuild,
  useDecideDraft,
  useMarkReady,
  useOpenFolder,
  useSubmission,
  type Built,
  type Requirement,
} from "./queries";

const MARK: Record<string, string> = {
  ready: "bg-ink",
  review: "bg-attention",
  missing: "border-2 border-line-strong",
};

function stateText(r: Requirement): string {
  if (r.state === "review") return "Draft · needs you";
  if (r.state === "missing") return "Missing";
  if (r.file_name) return "Ready · file added";
  if (r.draft?.status === "office_approved") return "Ready · approved by the office";
  return "Ready";
}

export function Submission() {
  const { tenderId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const tender = useTender(tenderId);
  const submission = useSubmission(tenderId);
  const office = useOffice(tenderId);
  const filter = params.get("show") ?? "all";
  const rows = submission.data?.requirements ?? [];
  const ready = rows.filter((r) => r.state === "ready").length;
  const review = rows.filter((r) => r.state === "review").length;
  const missing = rows.filter((r) => r.state === "missing").length;
  const shown = rows.filter((r) => filter === "all" || r.state === filter);
  const sections = [...new Set(shown.map((r) => r.section))];
  const selected = rows.find((r) => r.id === params.get("item"));
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));
  const keep = (extra: Record<string, string>) => setParams({ ...(filter === "all" ? {} : { show: filter }), ...extra });

  return (
    <div className="relative flex h-full w-full">
      <section aria-label="Submission checklist" className="flex min-w-0 grow flex-col px-8 pt-7">
        <h1 className="text-[22px] font-semibold tracking-tight">Submission</h1>
        <span className="text-ink-2">
          {ready} of {rows.length} ready{tender.data?.due_date && ` · submit ${dueShort(tender.data.due_date).replace("due ", "by ")}`}
        </span>

        <div className="mt-[18px] flex gap-[18px] border-b border-line">
          {[
            ["all", "All"],
            ["review", `Needs you · ${review}`],
            ["missing", `Missing · ${missing}`],
          ].map(([key, label]) => (
            <button
              key={key}
              onClick={() => setParams(key === "all" ? {} : { show: key })}
              className={`h-8 text-[13px] ${filter === key ? "font-semibold shadow-[inset_0_-2px_0_var(--color-ink)]" : "text-ink-2"}`}
            >
              {label}
            </button>
          ))}
        </div>

        <div className="min-h-0 grow overflow-y-auto pb-4">
          {rows.length === 0 && submission.data && (
            <p className="pt-6 text-ink-2">
              No checklist yet. Ask the office to list what the tender requires, for example: “Prepare the submission
              checklist.”
            </p>
          )}
          {sections.map((section) => (
            <div key={section} className="flex flex-col">
              <span className="px-2 pt-5 pb-1 text-xs font-semibold text-ink-2">{section}</span>
              {shown
                .filter((r) => r.section === section)
                .map((r) => (
                  <button
                    key={r.id}
                    onClick={() => keep({ item: r.id })}
                    className={`flex items-center gap-3 border-b border-subtle px-2 py-3 text-left ${r.id === selected?.id ? "rounded-md bg-subtle" : "hover:bg-rail"}`}
                  >
                    <span className={`size-3.5 shrink-0 rounded-full ${MARK[r.state]}`} />
                    <span className="flex min-w-0 grow flex-col gap-0.5">
                      <span className="truncate" dir="auto">
                        {r.title}
                      </span>
                      <span className="text-xs text-ink-3">
                        {r.document_name ? `${r.document_name}, page ${r.page}` : "Added by you"}
                      </span>
                    </span>
                    <span className={r.state === "review" ? "font-medium text-attention" : "text-ink-2"}>
                      {stateText(r)}
                    </span>
                  </button>
                ))}
            </div>
          ))}
          <AddRequirement tenderId={tenderId} />
        </div>
        <BuildBar tenderId={tenderId} notReady={rows.length - ready} columns={submission.data?.columns ?? []} />
      </section>
      {selected && <RequirementPanel tenderId={tenderId} requirement={selected} people={people} />}
    </div>
  );
}

function AddRequirement({ tenderId }: { tenderId: string }) {
  const add = useAddRequirement(tenderId);
  const [open, setOpen] = useState(false);
  const [section, setSection] = useState("Commercial");
  const [title, setTitle] = useState("");
  const field = "h-9 rounded-lg border border-line-strong px-3 outline-none focus:border-ink";

  function submit(event: FormEvent) {
    event.preventDefault();
    add.mutate({ section, title }, { onSuccess: () => setTitle("") });
  }

  if (!open)
    return (
      <button onClick={() => setOpen(true)} className="px-2 pt-4 text-ink-2 hover:text-ink">
        Add a requirement
      </button>
    );
  return (
    <form onSubmit={submit} className="flex flex-wrap gap-2 px-2 pt-4">
      <input aria-label="Section" required value={section} onChange={(e) => setSection(e.target.value)} className={`${field} w-36`} />
      <input
        aria-label="Requirement"
        required
        placeholder="What must be submitted"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        className={`${field} grow`}
      />
      <button className="h-9 rounded-lg bg-ink px-4 text-white">Add</button>
      {add.isError && <p className="w-full text-attention">{add.error.message}</p>}
    </form>
  );
}

function BuildBar(props: { tenderId: string; notReady: number; columns: { document_name: string; sheet: number; rate_column: string; amount_column: string }[] }) {
  const build = useBuild(props.tenderId);
  const [spread, setSpread] = useState(true);
  return (
    <div className="flex flex-col gap-3 border-t border-line py-4">
      {build.data ? (
        <BuiltCard built={build.data} />
      ) : (
        <span className="text-xs text-ink-3">
          {props.columns.length
            ? props.columns
                .map((c) => `Priced BOQ in the client’s ${c.document_name}, sheet ${c.sheet}: rates in ${c.rate_column}, amounts in ${c.amount_column}.`)
                .join(" ")
            : "The office hasn’t matched the client’s BOQ columns yet, so the priced BOQ will be in Quantix’s layout."}
        </span>
      )}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <span className="text-ink-2">The package is built on your computer. Quantix never sends anything to the client.</span>
        <span className="flex items-center gap-4">
          <label className="flex items-center gap-2 text-ink-2">
            <input type="checkbox" checked={spread} onChange={(e) => setSpread(e.target.checked)} className="accent-ink" />
            Markups in the rates
          </label>
          <button
            onClick={() => build.mutate(spread)}
            disabled={build.isPending}
            className="h-[38px] rounded-lg bg-ink px-4 text-sm whitespace-nowrap text-white disabled:bg-line-strong"
          >
            Build package{props.notReady > 0 && ` · ${props.notReady} not ready`}
          </button>
        </span>
      </div>
      {build.isError && <p className="text-attention">{build.error.message}</p>}
    </div>
  );
}

function BuiltCard({ built }: { built: Built }) {
  const open = useOpenFolder();
  const difference = Number(built.priced_total) - Number(built.summary_total);
  return (
    <div className="flex flex-col gap-2 rounded-[10px] bg-rail p-3">
      <span className="flex items-center justify-between gap-3">
        <span className="font-medium">Built: {built.folder}</span>
        <button onClick={() => open.mutate(built.folder)} className="font-medium underline underline-offset-4">
          Open folder
        </button>
      </span>
      <span className="text-ink-2">{built.files.join(" · ")}</span>
      <span className="text-ink-2">
        The priced BOQ adds up to {money(built.priced_total)}
        {Math.abs(difference) >= 0.005
          ? `, ${money(String(Math.abs(difference)))} ${difference > 0 ? "over" : "under"} the tender total of ${money(built.summary_total)} from rounding the rates.`
          : ", the tender total."}
      </span>
      {built.not_ready.length > 0 && (
        <span className="text-attention">Not ready: {built.not_ready.join("; ")}.</span>
      )}
      {open.isError && <span className="text-attention">{open.error.message}</span>}
    </div>
  );
}

function RequirementPanel(props: { tenderId: string; requirement: Requirement; people: Map<string, Staff> }) {
  const { tenderId, requirement: r } = props;
  const [, setParams] = useSearchParams();
  const decide = useDecideDraft(tenderId);
  const ready = useMarkReady(tenderId);
  const attach = useAttach(tenderId);
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  const author = r.draft ? props.people.get(r.draft.proposed_by) : undefined;
  const error = decide.error ?? ready.error ?? attach.error;

  return (
    <aside
      aria-label={r.title}
      className="flex w-[400px] shrink-0 flex-col gap-[18px] overflow-y-auto border-l border-line bg-white px-6 pt-7 pb-5 max-xl:absolute max-xl:inset-y-0 max-xl:right-0 max-xl:z-10 max-xl:shadow-[-8px_0_24px_rgba(0,0,0,0.08)]"
    >
      <button aria-label="Close" onClick={() => setParams({})} className="-mt-3 -mr-2 self-end text-lg leading-none text-ink-3 hover:text-ink">
        ×
      </button>
      <div className="flex flex-col gap-1">
        <span className="text-ink-3">{r.section}</span>
        <h2 className="text-[17px] leading-snug font-semibold" dir="auto">
          {r.title}
        </h2>
      </div>

      {r.document_id && (
        <div className="flex flex-col gap-1.5">
          <span className="text-xs font-semibold text-ink-2">Required by</span>
          <Link
            to={`/tenders/${tenderId}/documents?doc=${r.document_id}&page=${r.page}`}
            className="underline underline-offset-4"
          >
            {r.document_name}, page {r.page}
          </Link>
          <p className="rounded-lg bg-rail p-3 leading-relaxed text-[#27272A]" dir="auto">
            {r.quote}
          </p>
        </div>
      )}

      {r.draft && (
        <div className="flex flex-col gap-1.5">
          <span className="flex items-center gap-2 text-xs font-semibold text-ink-2">
            {author && <Face id={author.id} size={18} />}
            Drafted{author && ` by ${firstName(author)}`}
            {r.draft.status === "office_approved" && " · approved by the office, not reviewed"}
          </span>
          <div className="flex flex-col gap-2 rounded-lg border border-line p-3">
            <span className="font-medium">{r.draft.title}</span>
            <p className="leading-relaxed whitespace-pre-wrap text-[#27272A]" dir="auto">
              {r.draft.body}
            </p>
          </div>
        </div>
      )}

      {r.file_name && <p className="text-ink-2">File added: {r.file_name}</p>}
      {r.ready_note !== null && <p className="text-ink-2">Marked ready{r.ready_note && `: ${r.ready_note}`}</p>}

      <div className="grow" />
      {r.draft?.status === "proposed" &&
        (rejecting ? (
          <div className="flex flex-col gap-2">
            <textarea
              aria-label="What to change"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              placeholder={`Tell ${author ? firstName(author) : "the office"} what to change`}
              rows={3}
              className="rounded-lg border border-line-strong px-3 py-2 outline-none focus:border-ink"
            />
            <button
              onClick={() => decide.mutate({ id: r.draft!.id, approve: false, reason }, { onSuccess: () => setRejecting(false) })}
              className="h-[38px] rounded-lg bg-ink text-sm text-white"
            >
              Send back
            </button>
          </div>
        ) : (
          <div className="flex gap-2">
            <button onClick={() => decide.mutate({ id: r.draft!.id, approve: true })} className="h-[38px] grow rounded-lg bg-ink text-sm text-white">
              Approve draft
            </button>
            <button onClick={() => setRejecting(true)} className="h-[38px] rounded-lg border border-line-strong px-3.5 text-sm">
              Send back
            </button>
          </div>
        ))}
      <div className="flex items-center gap-4">
        <label className="cursor-pointer text-ink-2 hover:text-ink">
          {r.file_name ? "Replace the file" : "Add the file"}
          <input
            type="file"
            aria-label="Add the file"
            className="hidden"
            onChange={(e) => e.target.files?.[0] && attach.mutate({ id: r.id, file: e.target.files[0] })}
          />
        </label>
        {r.ready_note === null ? (
          r.state !== "ready" && (
            <button onClick={() => ready.mutate({ id: r.id, ready: true })} className="text-ink-2 hover:text-ink">
              I have this ready
            </button>
          )
        ) : (
          <button onClick={() => ready.mutate({ id: r.id, ready: false })} className="text-ink-2 hover:text-ink">
            Not ready after all
          </button>
        )}
      </div>
      {error && <p className="text-attention">{error.message}</p>}
    </aside>
  );
}
