# Supplier correspondence interface

Implemented 2026-09-06 in the existing white, navy and teal interface. All production controls call the authenticated local API. No SMTP connection, inbox access, keyring mutation, live provider call or reference Tender mutation was performed during this task.

## Root integration

- `src/features/Quotes.tsx` exports `Quotes({ tenderId, onSource? })`. Place it in Work. `onSource` uses the existing `SourceSelection` contract from `Sources.tsx`.
- `src/features/MailSettings.tsx` exports `MailSettings()`. Place it as a separate section after the existing Settings form; do not nest forms.
- Both modules import `src/styles/correspondence.css`. No root stylesheet import is required.
- Both use the existing ApiContext and React Query cache. All request and response types come from generated `Schema<...>` bindings, including `QuotePreview.restore_reconciliation_required` added by root during implementation.

## Behaviours

The request list opens an exact server preview showing sender, To/Cc, subject, message text, SMTP host, stored status, attachment versions/sizes/full hashes, source citations and message fingerprint. New and edited drafts submit full DraftInput values, without approval fields. Attachments can only be selected from saved current Tender artifacts, with a 20-file selection limit. Existing older attachments remain explicit and removable. Backend size/integrity/version checks remain authoritative. Supporting source references are selected from real Tender source search results.

One confirmation and approval note supplies the same SendDecision to approval and, when expressly selected, sending. Approve only performs no send call. SMTP readiness gates sending, not drafting or EML download. The EML control downloads the authenticated binary response as a message file and records no submission.

Recorded delivery history remains visible separately from restored database status, and differing submission fingerprints are explicit. Sending and editing are unavailable for sent, partial, uncertain, sending or attempting history. A required restore reconciliation is shown before action and accepts only engineer-written content; it cannot override a delivery receipt. Network errors trigger status refresh and clear consent without automatic retry. A failed send request requires a status refresh and new review before another user-directed attempt. SMTP acceptance is never described as confirmed inbox delivery.

Supplier replies show origin, received/retrieved date basis, original Date header, preserved text, warnings and newly registered evidence citations. Manual registration requires an explicit timezone-aware ISO timestamp, validates it locally, and sends supporting source IDs separately from the evidence IDs returned by the backend. Replies do not accept prices, quantities or commercial conditions.

Mail Settings separates SMTP SSL/STARTTLS and optional IMAP SSL configuration from AI credentials. Passwords are write-only, cleared from form state after a successful save, omitted when blank, and explicitly removable using a checkbox. No readiness flag is presented as a successful connection test. Inbox reading is only invoked by the Check supplier replies button against a configured saved account, with a 30-message bounded request, visible result counts, warnings and remaining-batch indication.

## Verification and handoff

Behaviour tests are in `Quotes.test.tsx` and `MailSettings.test.tsx`; all network responses are synthetic test doubles. Initial tests demonstrated missing feature modules. Subsequent tests caught and fixed error loss during status refresh. Final review demonstrated unintended draft submission when opening a supporting source inside a form; root fixed the shared Citation button with `type="button"` in `Sources.tsx`, and the regression now passes.

Commands:

```powershell
npm run test:ui -- src/features/Quotes.test.tsx src/features/MailSettings.test.tsx
npm run check:ui
npx prettier --check src/features/Quotes.tsx src/features/MailSettings.tsx src/features/Quotes.test.tsx src/features/MailSettings.test.tsx src/styles/correspondence.css
```

Verification result: **16 targeted tests passed; TypeScript passed.** Tests cover draft creation/edit payloads, identical approval/send decisions, approval without configured SMTP, newer delivery history, send error preservation and no automatic retries, manual reply evidence and date validation, explicit restore reconciliation, authenticated EML download, current-artifact selection limits, source inspection without form submission, write-only passwords, mail settings errors and bounded explicit reply sync.

This Node runtime initially exposed experimental Web Storage globals to test workers. Root fixed test-runner environment inheritance so the plain npm test command works. No shared test setup was changed by this task.

The root agent owns Work/Settings wiring, integrated browser visual checks, shared interface review and the full milestone verification gate. This task did not start a server or perform independent browser QA against private reference data. No release package was built. Existing design tokens, open rows, plain form sections and modal conventions were retained; form text is explicitly 16px and layouts collapse to one column on narrow viewports.
