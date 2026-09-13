# Interface motion sources and decisions

Reviewed 12 September 2026. These changes use the existing Motion 13 and Lucide dependencies, with shadcn/Base UI still owning menus, dialogs, focus and keyboard navigation. Product state, permissions and API handlers remain authoritative.

| Requested source | Inspected selection | Quantix use |
| --- | --- | --- |
| [Amicro buttons](https://amicro.vercel.app/buttons) | Copy, Delete, Preview, Theme, Search, Download, Upload, Reload | Shared `MicroButton`: 36px pill, 1.02 hover / 0.96 press scale, spring icon exchange, deletion shake. Copy confirms a completed write; preview/search/theme icons follow their actual state. Import and download retain their existing handlers. |
| [Cards](https://amicro.vercel.app/cards) | Arc, linear spread, corner fan, cascade, scatter, radial fan and stacks | Inspected and fetched. Overlapping presentation would hide parallel records, so document tables and office lists stay intact. Saved draft detail cards receive a short reveal. |
| [Carousels](https://amicro.vercel.app/carousels) | Interactive arc, CoverFlow, Time Machine | Inspected and fetched. Perspective layouts and automatic cycling do not fit source review; preserve exact-page navigation. |
| [Loaders](https://amicro.vercel.app/loaders) | Pulse Dots and the public registry of loading indicators | Small inherited-color pulse dots in shared loading notices; status text remains available to assistive technology. Existing spinner and skeleton now stop for reduced motion. |
| [Dither charts](https://amicro.vercel.app/dither-charts) | Eleven canvas visualizers | Inspected and fetched. No substitution for precise quantities, costs, coverage or timelines; canvas texture adds no engineering information. |
| [Anime](https://amicro.vercel.app/Anime) | Public animation gallery and entrance primitives, including Fade Up | Short fade-up on reading-map and draft-detail disclosures. No orbital decorations or looping text effects in work records. |
| [Transitions.dev](https://transitions.dev/detail.html) | Public library, panel reveal, skeleton guidance, motion-token scale | Scoped 250ms/4px reveal using smooth-out easing, with no blur over engineering text. Existing accessible modal/menu behavior retained. |

Full public sources fetched for inspection beneath `~/.quantix/tmp/ui-references-20260912`: Amicro revision `86b55340bfb939b8e93bb53aa46ba017c3449f1c`; Transitions.dev revision `0b236ec0754fb7408d6291dca52484ecd8e10812`. Demo/customer data is not bundled with Quantix.

## CLI, skill and Refine investigation

Amicro's documented `npx @subhanhq/amicro@latest add` route and npm package 1.0.1 were checked. Its registry examples add `framer-motion`; selected source was adapted directly to Quantix's installed `motion/react`, semantic theme tokens, controlled action state and reduced-motion behavior. No second motion dependency or wholesale registry overwrite is needed.

[Transitions.dev skills](https://transitions.dev/skill.html) were fetched and read from the official repository (`transitions-dev` and `transitions-polish`). Its [Refine tool](https://transitions.dev/refine.html), npm version 0.3.34, injects a development script, installs skills and starts an agent-connected relay. It was inspected, not started or installed into Quantix. This increment uses the public guidance directly; no premium account, purchase, extra AI execution or live refinement service was required.

## Behavior and verification boundaries

Delete/Remove opens the existing confirmation and stays disabled until it is explicitly checked. Hover and focus only animate the icon. Preview toggles the current PDF preview while retaining the selected page, zoom and source text. Search clears only the query and returns focus to the input; area/type/status filters remain selected. The Settings theme button changes light/dark using the existing reveal, and the existing theme menu retains System. All new animation has a reduced-motion path.

Validation results and native/scaling limitations are recorded in `docs/progress.md`.

## Amicro license

MIT License

Copyright (c) 2026 SYED  SUBHAN UDDIN

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
