import { isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState, type ReactNode } from "react";
import lockup from "../../../brand/logo/quantix-lockup-endorsed-on-light.svg?url";
import signature from "../../../brand/founder/signature-on-light.svg?url";

/** The QS Mind products, in the order a quantity surveyor's work runs: estimate, tender, procure. */
const HOUSE: [stage: string, name: string, what: string][] = [
  ["Stage 1 · Pre-contract", "Quanta", "AI cost consultant for construction"],
  ["Stage 2 · Tender", "Quantix", "Your tendering office, on your desktop"],
  ["Stage 3 · Procurement", "Tawreed", "From BOQ to procurement packages"],
];

const DOES = [
  "A Tender Manager leads AI staff who read the tender, take off quantities, price the work, level subcontract and supplier quotes, and prepare the submission.",
  "The Tender Manager reviews everything the staff produce before it reaches you, and you approve at every gate: scope, quantities, rates, subcontracts, the final price and the release.",
  "The AI proposes; Quantix computes. Every quantity, extension, total and comparison is worked out by Quantix, never stated by the AI.",
  "Every finding cites the document page or web page it came from, one click away.",
];

/** About Quantix: what it is, the QS Mind house it belongs to, and who made it. */
export function About() {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    if (isTauri()) void getVersion().then(setVersion);
  }, []);

  return (
    <div className="flex w-full max-w-[704px] flex-col gap-10 px-8 pt-10 pb-12">
      <header className="flex flex-col gap-4">
        <img src={lockup} alt="Quantix by QS Mind" className="h-14 self-start" draggable={false} />
        <p className="text-[15px] leading-relaxed text-ink-2">
          Your tendering office, on your desktop.{version && <span className="text-ink-3"> · Version {version}</span>}
        </p>
      </header>

      <Part title="What Quantix does">
        <ul className="flex flex-col gap-2.5">
          {DOES.map((line) => (
            <li key={line} className="flex gap-2.5 leading-relaxed">
              <span className="mt-[9px] size-1 shrink-0 rounded-full bg-ink-3" />
              {line}
            </li>
          ))}
        </ul>
      </Part>

      <Part title="A QS Mind product">
        <p className="leading-relaxed">
          Quantix is a product of <span className="font-medium">QS Mind</span>, the commercial mind of construction. QS
          Mind&rsquo;s products follow a quantity surveyor&rsquo;s work from the first estimate to the last purchase
          order, and Quantix is its tender stage.
        </p>
        <div className="flex flex-col divide-y divide-line rounded-xl border border-line-strong">
          {HOUSE.map(([stage, name, what]) => (
            <div key={name} className={`flex items-center gap-4 px-4 py-3 ${name === "Quantix" ? "bg-rail" : ""}`}>
              <span className="w-40 shrink-0 text-xs text-ink-3">{stage}</span>
              <span className="flex min-w-0 flex-col">
                <span className="font-medium">
                  {name}
                  {name === "Quantix" && <span className="font-normal text-ink-3"> · this app</span>}
                </span>
                <span className="text-ink-2">{what}</span>
              </span>
            </div>
          ))}
        </div>
      </Part>

      <Part title="Your data">
        <p className="leading-relaxed">
          Tender documents, records and settings stay on this computer, in the <code>.quantix</code> folder in your user
          folder. The files the client sent are kept exactly as supplied. What the office sends to the AI service you
          connect is the only thing that leaves, under your own key.
        </p>
      </Part>

      <Part title="Founder">
        <a
          href="https://kareemsafwat.com"
          target="_blank"
          rel="noreferrer"
          className="flex items-center gap-4 self-start rounded-xl border border-line-strong px-4 py-3 hover:border-ink-4"
        >
          <FounderMark />
          <span className="flex flex-col gap-1">
            <img src={signature} alt="Kareem Safwat" className="h-7 self-start" draggable={false} />
            <span className="text-ink-2">
              Founded &amp; developed by <span className="font-medium text-ink">Kareem Safwat</span> · kareemsafwat.com
            </span>
          </span>
        </a>
      </Part>

      <footer className="border-t border-line pt-5 text-xs leading-relaxed text-ink-3">
        © {new Date().getFullYear()} QS Mind. All rights reserved. Quantix and the Weave mark belong to QS Mind. The
        brand typeface, Urbanist, is used under the SIL Open Font License.
      </footer>
    </div>
  );
}

function Part({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section aria-label={title} className="flex flex-col gap-3">
      <h2 className="text-[15px] font-semibold">{title}</h2>
      {children}
    </section>
  );
}

/** Kareem Safwat's K, as brand/founder/k-mark.svg draws it. */
function FounderMark() {
  return (
    <svg viewBox="0 0 300 300" fill="none" className="size-10 shrink-0" aria-hidden="true" focusable="false">
      <defs>
        <linearGradient id="quantix-founder-k" x1="80" y1="70" x2="220" y2="230" gradientUnits="userSpaceOnUse">
          <stop offset="0%" stopColor="#FEF08A" />
          <stop offset="50%" stopColor="#D97706" />
          <stop offset="100%" stopColor="#78350F" />
        </linearGradient>
      </defs>
      <line x1="80" y1="70" x2="80" y2="230" stroke="url(#quantix-founder-k)" strokeWidth="8" strokeLinecap="square" />
      <line x1="80" y1="150" x2="220" y2="70" stroke="url(#quantix-founder-k)" strokeWidth="8" strokeLinecap="square" />
      <line
        x1="80"
        y1="150"
        x2="220"
        y2="230"
        stroke="url(#quantix-founder-k)"
        strokeWidth="8"
        strokeLinecap="square"
      />
    </svg>
  );
}
