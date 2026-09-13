# Live Office UI L19–L21

Implemented the bounded Live Office increment for the approved dynamic-office design.

## Delivered

- `LiveOffice` renders the Manager anchor, real generated staff profiles, actual assignment states, Manager-only empty state, connection/retry state, full staff pagination, event-derived arrival styling, responsive work area and motion controls.
- `OfficeMessages` renders actual retained instructions, questions, replies, findings, handoffs and expandable notes with sender/version/time/reply references, source/result/output links, pagination, and append-safe snapshot reconciliation.
- `StaffDesk` reads the exact saved desk, preserves historical profile selection and loaded pages across office sequence refreshes, paginates versions/work orders/results/source receipts from offset zero, guards all reads against stale Tender/staff responses, and clearly labels saved output as draft content.
- `StaffDraftContent` renders every populated canonical `OfficeOutput` section as inspectable engineering content with structured fields and source callbacks. The workspace result pane can reuse it for exact staff-result display.
- `office.css` imports the existing local portrait styling and adds light/dark, responsive, keyboard-friendly surfaces, hidden-window motion pausing and true reduced-motion behavior.

## Validation

- `npm run check:ui` passed.
- Focused office tests passed: 4 files, 10 tests.
- Full UI suite passed: 40 files, 190 tests.
- Prettier passed for all owned UI files.
- Impeccable detector returned no findings for the initial owned surface pass.

No release package, real Tender approval, commercial send, provider call, or customer data was used.
