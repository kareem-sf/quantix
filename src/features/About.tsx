import { useResource, type Schema } from "../api";
import { BrandMark } from "../app/BrandMark";

const credits = [
  {
    name: "Amicro",
    detail: "Action buttons, pulse dots and disclosure motion by Syed Subhan",
    href: "https://amicro.vercel.app/buttons",
  },
  {
    name: "Transitions.dev",
    detail: "Panel transition and accessible motion guidance by Jakub Antalik",
    href: "https://transitions.dev/",
  },
  {
    name: "shadcn/ui",
    detail: "Component system",
    href: "https://ui.shadcn.com",
  },
  {
    name: "Skiper UI",
    detail:
      "Theme reveal, smooth caret, counter, animated link, squircle, gooey, scroll fade, animated icons, progressive blur, loop, breakpoint and debug panels",
    href: "https://skiper-ui.com",
  },
  {
    name: "BorderBeam",
    detail: "Prompt box border beam by Jakub Antalik",
    href: "https://beam.jakubantalik.com",
  },
  {
    name: "Ali Imam UI",
    detail: "Gauge, typewriter and animated border",
    href: "https://aliimam.in",
  },
  {
    name: "Watermelon UI",
    detail: "Filter disclosure",
    href: "https://ui.watermelon.sh",
  },
];

export function About() {
  const health = useResource<Schema<"Health">>("/health");
  return (
    <section aria-labelledby="about-heading">
      <div className="section-heading">
        <div className="flex items-center gap-3">
          <BrandMark size={48} />
          <h2 id="about-heading">About Quantix</h2>
        </div>
      </div>
      <p className="muted">
        Your local Tender Office. Work through the Tender Manager, inspect the
        sources and retain control over scope, quantities, prices and release.
      </p>
      <dl className="about-facts">
        <dt>Version</dt>
        <dd>{health.data?.version ?? "Checking…"}</dd>
        <dt>Design and engineering</dt>
        <dd>Kareem Safwat</dd>
        <dt>Brand artwork</dt>
        <dd>Quantix transparent identity · v4</dd>
      </dl>
      <h3>Interface credits</h3>
      <ul className="about-credits">
        {credits.map((credit) => (
          <li key={credit.name}>
            <strong>{credit.name}</strong> · {credit.detail}
          </li>
        ))}
      </ul>
    </section>
  );
}
