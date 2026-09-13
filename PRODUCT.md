# Quantix product context

<!-- impeccable:product-schema 1 -->

## Platform

web

The current interface uses React inside a Tauri desktop application. This records the interface technology, not an assertion of tablet deployment support. The user confirmed laptop and tablet as redesign targets on 2026-09-09. Quantix is a desktop application; connected tablet access was removed on 2026-09-13. Responsive layouts remain a design requirement and do not imply cross-device access or synchronization.

## Users

Construction field engineers, using both laptops and tablets. The interface must be friendly to engineers without requiring them to understand AI infrastructure. The Tender Manager is the main assistant and contact within the product.

## Product Purpose

Quantix is an engineer-controlled local Tender Office. It helps understand tender packages, organize evidence, propose and execute approved work, prepare quantities and estimates, review supplier information, and prepare submission documents. This purpose and the existing capabilities come from docs/spec.md; the current redesign request changes the audience emphasis and interface, not the product into a site-management system.

## Operating Context

Engineers work with drawings, specifications, BOQs, revisions, requirements, assumptions, prices, supplier information, and submission records. They need to inspect evidence and retain authority over engineering and commercial decisions. Use on laptops and tablets is user-confirmed; gloves, outdoor glare, connection quality, device operating systems, and working language preferences for future tablet use are not yet confirmed.

## Capabilities and Constraints

- Preserve original tender packages, revisions, source locations, and history.
- Keep imported, extracted, analysed, and engineer-reviewed coverage distinct.
- Keep proposed quantities and rates separate from accepted values.
- Routine work may proceed within an approved plan. Material assumptions, quantity changes, commercial decisions, sending, and final release require engineer control.
- Keep account identities, billing, permitted models, data permissions, and spending authority explicit. No automatic paid fallback.
- Current supported direct APIs and subscription routes are governed by docs/subscription-connections.md.
- Quantix-managed private files and runtime data live under ~/.quantix. Do not commit customer documents, extracted content, credentials, or databases.
- The current desktop product does not establish offline AI, remote tablet access, synchronization, native tablet packaging, automatic OCR, or native DWG interpretation. Do not imply those capabilities through design copy.

## Brand Commitments

The name is Quantix. Product copy uses plain construction-engineering language. The user requested English-only responses in this task. The existing product includes English interface text and multilingual Manager responses; the redesign must preserve supported language behavior unless explicitly changed.

## Evidence on Hand

- docs/spec.md: current product specification.
- AGENTS.md: current project constraints and engineering authority.
- docs/design/workspace-redesign.md: previous approved workspace design; its visual approach is evidence for this new redesign, not an automatic limit on it.
- src/App.tsx, src/features, and src/styles: incumbent implementation.
- docs/design/approved-manager.png and docs/design/approved-plan-review.png: previous concept references, not proof of current rendered behavior.

## Product Principles

1. Explain what is happening and offer one clear next action.
2. Use the engineer's tasks and vocabulary; place technical controls in More options.
3. Keep evidence close to decisions and distinguish proposals from checked work.
4. Preserve engineer control without repetitive or ambiguous approval steps.
5. Support the same core tasks with mouse, keyboard, and touch, while adapting density and layout to the available screen.

## Open Decisions

The opening layout is agreed: Manager conversation beside the current document/result, with an expandable live office. Only the Tender Manager exists by default; it generates complete staff profiles live. Staff use distinct illustrated AI portraits, with important exchanges, handoffs and questions shown by default and full retained conversations expandable. The Manager personality is engineer-customizable.

Desktop and text come first; push-to-talk follows afterward. Closing the main window keeps authorized work running while the computer remains awake, with a visible tray indicator; explicit Quit stops work safely. The detailed visual concept and technical implementation must still be verified. See docs/design/live-dynamic-office.md and docs/design/office-completeness-review.md.
