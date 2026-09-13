# Rate proposal review interface

Implemented 2026-09-06. This work owns only `src/features/RateProposals.tsx`, its test file, `src/styles/rate-proposals.css` and this report. No production Tender, market source, provider, rate or quantity was changed during development.

## Root integration

Import `RateProposals` from `src/features/RateProposals.tsx` and render it in Estimate with:

```tsx
<RateProposals tenderId={tenderId} items={view.items} onSource={onSource} />
```

The component imports its own stylesheet and uses the existing ApiContext, React Query cache, source citations, status and error controls. All DTOs use the generated `RateProposalRecord`, `RateApproval`, `EstimateItem`, `RateSource` and `RateComponent` schema types. No dependency, shared interface or root file was changed.

## Behaviour

- GET the Tender's stored rate proposals; show real empty/loading/error states and allow inspection of proposed, approved and stale records.
- Present the exact proposed direct rate or component build-up beside the current installed rate, with currency and the original proposal unit. A changed current BOQ unit never relabels the historical proposal's unit.
- Build-up products and totals use integer-scaled decimal arithmetic, retaining all twelve possible fractional product digits without floating-point rounding. No estimate values are calculated or written by this display.
- Show observed/estimated basis, source date, location, conditions, safe HTTP/HTTPS source links, source citations, tax basis and explicit VAT uncertainty.
- Keep supplied BOQ quantity, approved measured quantity, effective quantity, source-row confirmation, quantity/unit cells, measurement proposals and source issues distinct. Rate approval sends no quantity fields.
- Make the full immutable source/item basis available for inspection, including source hashes, approved measurements and linked source revisions. Display preparation and approved-basis fingerprints separately.
- Block approval for decided records, unavailable current BOQ items and server-reported stale proposals. Detect scalar item/rate changes against the captured basis locally while the service remains authoritative for complete basis/version checks.
- Warn explicitly when approval will replace an existing rate and its price conditions. Require a nonblank engineer note and review checkbox. Send only `{engineer_confirmed:true,rationale,confirm_source}` to the stored proposal approval endpoint.
- Offer source confirmation only for unconfirmed rows, unchecked by default. Disable it when the supplied quantity cell remains unresolved. Approval with false keeps that source review incomplete.
- Disable the in-flight decision to prevent duplicate requests. Update the proposal cache from the real response, refresh the estimate and preserve backend errors without automatic retries. A refusal clears confirmation while retaining the decision note.

## Verification

Tests were written before the component. The initial run demonstrated the missing module; the stale-unit regression then demonstrated incorrect use of a newer BOQ unit before its fix.

```powershell
npm run test:ui -- src/features/RateProposals.test.tsx
npm run check:ui
npx prettier --check src/features/RateProposals.tsx src/features/RateProposals.test.tsx src/styles/rate-proposals.css
```

Tests use only synthetic fetch responses and a held test response for the in-flight case. Coverage includes exact decimals and provenance, separate quantities, explicit replacement consent, strict approval payloads, optional source confirmation, stale/already-approved/missing item blocks, current-rate changes, API errors, duplicate prevention, original-unit preservation and unresolved quantity confirmation.

Result: **13 targeted tests passed; TypeScript passed; scoped Prettier check passed.**

Root owns Estimate wiring, integrated browser visual verification and the full milestone verification gate. The component follows the existing white/navy/teal design, open list/detail layout and 16px form controls. Narrow layouts stack the fact rows and keep the build-up table in a local horizontal scroll area. No new image concept, server, release build or live data mutation was used for this targeted addition.
