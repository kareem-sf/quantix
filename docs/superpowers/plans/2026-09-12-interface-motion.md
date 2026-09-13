# Quantix interface motion implementation plan

**Goal:** Apply the useful Amicro and Transitions.dev interactions to existing Quantix workflows.

**Architecture:** Keep shadcn/Base UI behavior and existing API handlers. Use one Motion action-button primitive, a lightweight loading indicator, and scoped CSS reveal tokens. Existing confirmation forms retain authority. Preserve all current uncommitted work.

**References:** User-supplied button code; Amicro buttons/cards/carousels/loaders/dither-charts/Anime catalogues and public MIT source; Transitions.dev public skill, panel reveal, skeleton and motion-token guidance. Existing product contract: `docs/design/workspace-redesign.md`.

- [x] Build and test a reusable pill action button for delete, preview, search, theme, copy, download, upload and refresh. Icons reflect actual toggled/success state; delete shakes on hover/focus. Disable animations for reduced motion. Keep native button type, disabled state and keyboard behavior.
- [x] Integrate delete into saved-account removal entry points and confirmed submit actions; search/clear into Documents; preview show/hide and download into the source viewer; theme toggle into Settings while preserving the Light/Dark/System menu; download into saved drafts. Reuse the shared button for Copy Hash.
- [x] Adapt Amicro Pulse Dots for shared loading notices, and short fade-up/panel reveal for reading-map and draft-details expansions. Keep persistent data tables, source identities and actionable errors visible. Add reduced-motion guards to existing spinner/skeleton primitives.
- [x] Run affected UI suites, typecheck, formatting and diff review. Check real-app search, source preview, theme, and confirmation cancellation, using synthetic data for mutations; check light/dark and narrow layouts. Record remaining native/scaling limits. No release package.

## Selection rationale

Amicro's overlapping card spreads and perspective carousels obscure parallel document records, so they are not suitable replacements for the register or office lists. Dither canvas visualizers are decorative; retain legible engineering quantities and existing quantitative charts. Anime's simple fade entrances fit disclosures; orbital, cursor-following, particle and looping display effects do not help daily tender work. Transitions.dev panel/skeleton timing informs the selected reveals; existing shadcn dialogs and menus already provide accessible transitions.

The complete public source trees were fetched under `~/.quantix/tmp/ui-references-20260912` for inspection. Direct adaptation avoids the Amicro registry adding a second `framer-motion` dependency. Refine was inspected: its live command injects scripts and starts an agent-connected relay; the current task can be completed using the published CSS/React guidance without installing that development service or invoking another AI account.
