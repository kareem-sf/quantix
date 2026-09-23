import { useState } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { money, quantity } from "../estimate/queries";
import { Face } from "../office/Face";
import { firstName, useOffice, type Staff } from "../office/queries";
import { useChoose, useMarkSent, usePackages, type Package } from "./queries";

/** Where a package stands, in the engineer's terms. */
function packageState(p: Package): [text: string, needsYou: boolean] {
  const chosen = p.quotes.find((q) => q.id === p.selected_quote_id);
  if (chosen) return [`${chosen.company} chosen`, false];
  if (p.recommended_quote_id) return ["Levelled · needs you", true];
  const drafts = p.enquiries.filter((e) => e.status === "draft").length;
  if (drafts) return [`${drafts} enquiry ${drafts === 1 ? "draft" : "drafts"} to send`, true];
  if (p.quotes.length) return [`${p.quotes.length} of ${p.enquiries.length || p.quotes.length} quotes in`, false];
  return ["Waiting for quotes", false];
}

export function Subcontract() {
  const { tenderId = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const packages = usePackages(tenderId);
  const office = useOffice(tenderId);
  const list = packages.data ?? [];
  const selected = list.find((p) => p.id === params.get("package")) ?? list[0];
  const people = new Map((office.data?.staff ?? []).map((m) => [m.id, m]));

  return (
    <div className="flex h-full w-full max-xl:flex-col max-xl:overflow-y-auto">
      <div className="flex min-w-0 grow flex-col px-8 pt-7">
        <h1 className="text-[22px] font-semibold tracking-tight">Subcontract</h1>
        <span className="text-ink-2">Trade and material packages</span>
        {list.length === 0 && packages.data && (
          <p className="pt-6 text-ink-2">
            No packages yet. Ask the office to set them up, for example: “Get quotes for the waterproofing.”
          </p>
        )}
        {list.length > 0 && (
          <nav aria-label="Packages" className="mt-[18px] flex gap-6 overflow-x-auto border-b border-line">
            {list.map((p) => {
              const [text, needsYou] = packageState(p);
              const on = p.id === selected?.id;
              return (
                <button
                  key={p.id}
                  onClick={() => setParams({ package: p.id })}
                  className={`flex shrink-0 flex-col gap-0.5 pb-2 text-left ${on ? "shadow-[inset_0_-2px_0_var(--color-ink)]" : ""}`}
                >
                  <span className={on ? "font-semibold" : "text-ink-2"}>{p.name}</span>
                  <span className="flex items-center gap-1.5 text-xs text-ink-3">
                    {needsYou && <span className="size-[7px] rounded-full bg-attention" />}
                    {p.kind === "supply" && "Supplier · "}
                    {text}
                  </span>
                </button>
              );
            })}
          </nav>
        )}
        {selected && <Levelling tenderId={tenderId} pkg={selected} people={people} />}
      </div>
      {selected && <Choice tenderId={tenderId} pkg={selected} people={people} />}
    </div>
  );
}

function Levelling({ tenderId, pkg, people }: { tenderId: string; pkg: Package; people: Map<string, Staff> }) {
  const columns = `48px minmax(160px,1fr) 104px 80px ${pkg.quotes.map(() => "112px").join(" ")}`;
  const grid = { display: "grid", gridTemplateColumns: columns, columnGap: "12px" };
  const leveller = people.get(pkg.quotes[0]?.proposed_by ?? pkg.created_by);
  const best = pkg.quotes.find((q) => q.rank === 1);
  const plugged = pkg.quotes.some((q) => Object.values(q.cells).some((c) => c.plugged));
  const excluding = pkg.quotes.filter((q) => q.exclusions.length > 0);

  return (
    <section aria-label={pkg.name} className="flex min-w-0 flex-col overflow-x-auto pt-5 pb-6">
      <h2 className="text-[17px] font-semibold">{pkg.name}</h2>
      <span className="text-ink-2">
        {pkg.quotes.length} {pkg.quotes.length === 1 ? "quote" : "quotes"}
        {pkg.enquiries.length > 0 && ` · ${pkg.enquiries.length} ${pkg.enquiries.length === 1 ? "enquiry" : "enquiries"}`}
        {leveller && pkg.quotes.length > 0 && ` · levelled by ${firstName(leveller)}`}
      </span>

      <div className="mt-5 min-w-fit">
        <div style={grid} className="items-end border-b border-line-strong px-2 pb-2 text-xs text-ink-3">
          <span>Item</span>
          <span>Description</span>
          <span className="text-right">Qty</span>
          <span className="text-right">Our rate</span>
          {pkg.quotes.map((q) => (
            <Link
              key={q.id}
              to={`/tenders/${tenderId}/documents?doc=${q.document_id}&page=1`}
              className={`text-right hover:text-ink ${q.id === best?.id ? "font-semibold text-ink" : ""}`}
            >
              {q.company}
            </Link>
          ))}
        </div>
        {pkg.items.map((item) => (
          <div key={item.id} style={grid} className="items-center border-b border-subtle px-2 py-2.5">
            <span className="text-ink-3">{item.item}</span>
            <span className="min-w-0 truncate" dir="auto">
              {item.description}
            </span>
            <span className="text-right">
              {quantity(item.quantity)} <bdi className="text-ink-3">{item.unit}</bdi>
            </span>
            <span className="text-right text-ink-2">{money(item.our_rate)}</span>
            {pkg.quotes.map((q) => {
              const cell = q.cells[item.id];
              if (cell.plugged)
                return (
                  <span key={q.id} className="text-right text-attention italic" title="Not quoted: our rate is used">
                    {cell.rate ? money(cell.rate) : "no rate"}
                  </span>
                );
              return (
                <Link
                  key={q.id}
                  to={`/tenders/${tenderId}/documents?doc=${q.document_id}&page=${cell.page}`}
                  title={cell.quote ?? undefined}
                  className={`text-right hover:underline ${q.id === best?.id ? "font-semibold" : ""}`}
                >
                  {money(cell.rate)}
                </Link>
              );
            })}
          </div>
        ))}
        <div style={grid} className="border-b border-subtle px-2 py-2.5 text-ink-2">
          <span />
          <span>Exclusions priced back in</span>
          <span />
          <span />
          {pkg.quotes.map((q) => (
            <span key={q.id} className="text-right" title={q.exclusions.map((e) => e.description).join(", ")}>
              {q.exclusions.length ? `+${money(q.exclusions_total)}` : "none"}
            </span>
          ))}
        </div>
        <div style={grid} className="px-2 py-3 font-semibold">
          <span />
          <span>Levelled total</span>
          <span />
          <span />
          {pkg.quotes.map((q) => (
            <span key={q.id} className={`text-right ${q.id === best?.id ? "" : "font-normal text-ink-2"}`}>
              {q.levelled_total === null ? "incomplete" : money(q.levelled_total)}
            </span>
          ))}
        </div>
      </div>

      <div className="mt-2 flex flex-col gap-1 text-xs text-ink-3">
        {plugged && (
          <span>
            <span className="text-attention italic">Italic</span> is our rate, used where the quote left a gap.
          </span>
        )}
        {excluding.length > 0 && (
          <span>
            {excluding
              .map((q) => `${q.company} excludes ${q.exclusions.map((e) => e.description.toLowerCase()).join(" and ")}`)
              .join("; ")}
            .
          </span>
        )}
        <span>Quantix levels every quote from the quoted rates and the approved quantities.</span>
      </div>
    </section>
  );
}

function Choice({ tenderId, pkg, people }: { tenderId: string; pkg: Package; people: Map<string, Staff> }) {
  const choose = useChoose(tenderId);
  const sent = useMarkSent(tenderId);
  const [open, setOpen] = useState<string | null>(null);
  const recommended = pkg.quotes.find((q) => q.id === pkg.recommended_quote_id);
  const chosen = pkg.quotes.find((q) => q.id === pkg.selected_quote_id);
  const adviser = pkg.recommended_by ? people.get(pkg.recommended_by) : undefined;

  return (
    <aside
      aria-label="Choice"
      className="flex w-[340px] shrink-0 flex-col gap-5 overflow-y-auto border-l border-line bg-white px-6 pt-7 pb-5 max-xl:w-full max-xl:overflow-visible max-xl:border-t max-xl:border-l-0 max-xl:px-8"
    >
      {recommended && pkg.recommendation && (
        <div className="flex gap-2.5 rounded-[10px] bg-rail p-3 leading-normal">
          {adviser && <Face id={adviser.id} size={24} />}
          <span className="flex flex-col gap-1">
            <span className="font-medium">
              {adviser ? `${firstName(adviser)} recommends` : "Recommended:"} {recommended.company}
            </span>
            <span className="text-[#27272A]">{pkg.recommendation}</span>
          </span>
        </div>
      )}

      {pkg.enquiries.length > 0 && (
        <div className="flex flex-col">
          <span className="pb-1.5 text-xs font-semibold text-ink-2">Enquiries</span>
          {pkg.enquiries.map((e) => {
            const quoted = pkg.quotes.some((q) => q.company === e.company);
            return (
              <div key={e.id} className="flex flex-col border-b border-subtle py-2">
                <button onClick={() => setOpen(open === e.id ? null : e.id)} className="flex justify-between text-left">
                  <span>{e.company}</span>
                  <span className={e.status === "draft" && !quoted ? "text-attention" : "text-ink-3"}>
                    {quoted ? "quoted" : e.status === "draft" ? "draft to send" : "no reply yet"}
                  </span>
                </button>
                {open === e.id && (
                  <div className="flex flex-col gap-2 pt-2">
                    <span className="font-medium">{e.subject}</span>
                    <p className="rounded-lg bg-rail p-3 leading-relaxed whitespace-pre-wrap text-[#27272A]" dir="auto">
                      {e.body}
                    </p>
                    <span className="flex gap-3">
                      {e.email && (
                        <a
                          href={`mailto:${e.email}?subject=${encodeURIComponent(e.subject)}&body=${encodeURIComponent(e.body)}`}
                          className="font-medium"
                        >
                          Open in mail
                        </a>
                      )}
                      <button onClick={() => void navigator.clipboard?.writeText(e.body)} className="text-ink-2">
                        Copy
                      </button>
                      {e.status === "draft" && (
                        <button onClick={() => sent.mutate(e.id)} className="text-ink-2">
                          Mark as sent
                        </button>
                      )}
                    </span>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      <div className="grow" />
      {chosen ? (
        <p className="text-ink-2">
          <span className="font-medium text-ink">{chosen.company}</span> is chosen. Their rates are in the estimate as
          {pkg.kind === "supply" ? " supplier" : " subcontract"} prices.
        </p>
      ) : (
        pkg.quotes.length > 0 && (
          <div className="flex flex-col gap-2">
            {recommended && (
              <>
                <p className="text-ink-2">
                  Choosing carries {recommended.company}’s rates into the estimate as
                  {pkg.kind === "supply" ? " supplier" : " subcontract"} prices.
                </p>
                <div className="flex gap-2">
                  <button
                    onClick={() => choose.mutate({ packageId: pkg.id, quoteId: recommended.id })}
                    disabled={choose.isPending}
                    className="h-[38px] grow rounded-lg bg-ink text-sm text-white disabled:bg-line-strong"
                  >
                    Choose {recommended.company}
                  </button>
                  {adviser && (
                    <Link
                      to={`/tenders/${tenderId}/office?with=${adviser.id}`}
                      className="flex h-[38px] items-center rounded-lg border border-line-strong px-3.5 text-sm"
                    >
                      Ask {firstName(adviser)}
                    </Link>
                  )}
                </div>
              </>
            )}
            {pkg.quotes
              .filter((q) => q.id !== recommended?.id)
              .map((q) => (
                <button
                  key={q.id}
                  onClick={() => choose.mutate({ packageId: pkg.id, quoteId: q.id })}
                  disabled={choose.isPending}
                  className="self-start text-ink-2 hover:text-ink"
                >
                  {recommended ? "Or choose" : "Choose"} {q.company}
                </button>
              ))}
          </div>
        )
      )}
      {(choose.isError || sent.isError) && <p className="text-attention">{(choose.error ?? sent.error)?.message}</p>}
    </aside>
  );
}
