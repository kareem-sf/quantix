# Tender workflow audit repairs — 13 September 2026

The engineer requested implementation of all issues found in the code and running-app audit. This increment repairs the reviewed workflows and their regression coverage; it does not claim universal live-model or native-platform qualification.

## Required behavior

1. Tender analysis exports select a completed engineering analysis by recorded run provenance. A greeting, status response, failed run or unrelated recent conversation must not replace the analysis or invalidate its export. Missing attributable analysis is explicit.
2. A readable PDF/Word BOQ has a complete source-linked path into the estimate. Manager and engineer proposals retain an exact source excerpt, row reference, quantity and unit. Multiple rows may cite one page. They remain unconfirmed until the engineer checks them; rates, quantity changes, tax and release retain existing approval boundaries. Refresh and revisions preserve history and flag changed sources.
3. Submission requirements preserve applicability, conditions and exceptions alongside an exact cited clause. Source-backed qualification checks reject obvious dropped conditions as correctable draft errors. Unestablished applicability and historical proposals require visible review; no stored real Tender requirement or decision is silently rewritten.
4. Completed Manager work links to its actual saved requirements, drafts and calculations. A saved progress brief must not present an outdated next step as current after later engineering work. Continuation receives explicit guidance to update the brief; missing updates are visible rather than invented.
5. Documents label text extraction separately from analysis and engineer review. Narrow layouts retain a visible navigation control. Checkbox controls do not consume a whole row or clip their labels. Light/dark, keyboard, narrow widths and scaled layouts remain usable.
6. Relevant regression suites, final shared API bindings and frontend typecheck pass. Exercise the repaired journey with synthetic source files and local records, including source revisions and approval gates. Resolve actual regressions found by verification.

## Implementation boundaries

Use existing Python/FastAPI/SQLite authority, React routing and UI libraries. Separate source-based BOQ ingestion and report-analysis selection into focused helpers. Any existing-database schema change must have transactional verification and backup/recovery; preserve original files and record identities. No provider/account change, paid model request, real Tender approval, commercial send or release package is authorized as a test.

Work in the current checkout because the operative application includes substantial uncommitted work. Capture changed-file baselines beneath `~/.quantix/tmp` for review, and preserve unrelated concurrent work. Do not commit the combined existing checkout. User authorization already includes local architecture, implementation, dependencies and reversible fixes; no further routine design-approval stop is required.
