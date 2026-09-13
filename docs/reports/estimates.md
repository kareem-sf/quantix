# Estimating and draft outputs

Implemented 2026-09-06. This is a minimal usable estimating/output increment; complete Tender coverage and client submission compliance are not claimed.

## Files and integration

- `backend/quantix/estimates.py`: EstimateService, current-revision BOQ candidates, Decimal build-ups, per-currency totals and separate measured quantity proposals.
- `backend/quantix/estimate_models.py`: strict request/response schemas.
- `backend/quantix/estimate_routes.py`: create_router(repo), with /api prefix.
- `backend/quantix/outputs.py`: consolidated XLSX and analysis DOCX generation, local manifests and integrity-checked downloads.
- `backend/tests/test_estimates.py`, `backend/tests/test_outputs.py`: behavior and artifact checks.
- Appended Estimates and draft outputs section to `docs/contracts.md`; no pre-existing contract sections changed.

Root includes the router under existing /api bearer/origin enforcement, regenerates frontend types from OpenAPI, and calls EstimateService(repo).refresh(tender_id) after imports. Repository overview should count boq_items with active=1. API requests require explicit engineer_confirmed:true and a nonblank rationale. Office agents have no rate/quantity/output approval tools.

## Behavior

BOQ candidates are inferred from actual spreadsheet cell metadata, recognizable unit values, numeric values and dynamically discovered headers. No source filename, project area or fixed column index determines quantity. Reversed header labels are reported; multiple possible numeric quantities remain unresolved until the engineer chooses a real source cell. Formula cache availability, formula errors and hidden-source context remain visible. Supplied quantities are retained; source workbooks are never edited.

All inferred rows require source confirmation before entering totals. A measured quantity stores its own calculation text and evidence references. It does not replace the effective quantity until separately approved. Replaced measurement evidence invalidates that approval in the current view. Replaced rate evidence requires review. New BOQ source revisions do not inherit previous rates or approvals.

Rate inputs support a direct rate or explicit component consumption × component unit rate. Calculations use Decimal with local 80-digit precision; line currency values round half-up to two decimal places. Currencies are grouped, never added together. Observed rates require a dated source ID or HTTP(S) URL. Estimated rates retain their separate basis and rationale. Human-entered URLs are recorded without claiming independent web verification.

VAT percentage must be supplied explicitly; missing VAT never becomes zero. Rates including VAT are converted only when the percentage is known. Partial priced subtotals are labelled separately from nullable complete totals. Missing rates, unknown tax treatment, unconfirmed rows, unresolved quantities and pending source refresh block pricing completeness. This completeness applies to identified candidate rows, not the full Tender package.

Generated outputs remain draft pending engineer review and final release. XLSX sheets are Summary, BOQ, Rate build-ups and Sources. Summary contains server-calculated snapshot totals; BOQ and component arithmetic use auditable Excel formulas. Imported text is forced to inert strings so source descriptions cannot become executable spreadsheet formulas. Word includes the latest Tender Manager analysis, extraction coverage, findings and their actual decision states, assumptions/questions, pricing exceptions and exact source references. Each file has a SHA-256 manifest and a Tender-scoped stored output record.

## Verification

The test-first cycles covered reversed headers, ambiguous quantity cells, broken source formulas, exact rate arithmetic, VAT included/excluded/unknown, separate quantity approval, cross-Tender/invalid source rejection, revised measurement evidence, original quantity preservation, missing-input completeness, record schemas, actual Office ZIP integrity, generated formulas, source manifests, output tampering and download scoping.

```powershell
backend/.venv/Scripts/python.exe -m pytest backend/tests/test_estimates.py backend/tests/test_outputs.py -q
backend/.venv/Scripts/ruff.exe check backend/quantix/estimates.py backend/quantix/estimate_models.py backend/quantix/estimate_routes.py backend/quantix/outputs.py backend/tests/test_estimates.py backend/tests/test_outputs.py
```

Result: 17 tests passed; lint passed. One upstream Starlette/AnyIO TestClient deprecation warning remains.

Manual fixture QA used separate hidden Word/Excel instances with macros disabled, link updates disabled, read-only opens, no saving back, and process cleanup. Word rendered one page; all four workbook sheets rendered. PDFium rendered every page to PNG and all were inspected. Removed the inherited Word Title border and re-rendered. Excel CalculateFullRebuild returned 126.25 excluding VAT and 143.93 including VAT, matching the independently checked Decimal totals. QA artifacts remain in temporary directories, not source control.

Maintained APIs verified against installed openpyxl/python-docx and official documentation: [openpyxl formula behavior](https://openpyxl.readthedocs.io/en/stable/simple_formulae.html), [python-docx authoring](https://python-docx.readthedocs.io/en/latest/user/quickstart.html). openpyxl does not calculate formulas; normal export does not require local Office. Word/Excel automation was used only for this manual QA.

## Limits

- Candidate recognition is conservative, not a universal BOQ parser. Unrecognized units, unusual layouts, continuation rows and nonspreadsheet BOQs need source review and future mapping support. No checked visual takeoff is claimed.
- No client-format priced copy is produced. Rewriting arbitrary source workbook cells could change meanings, formulas, macros or layout; this increment produces a clearly identified consolidated workbook instead.
- Source formula caches are not recalculated during extraction. Excel output formulas recalculate when opened; applications that only read cached values may show blank formula results until recalculation. Summary snapshots remain available.
- Excel has finite numeric precision. Server arithmetic preserves accepted Decimal digits, while extreme numeric inputs and unusually long source descriptions require export review. Normal construction-sized fixture values and layouts were verified, not every possible workbook size.
- Observed source references are not binding supplier quotations. No macros were executed, providers contacted, supplier messages sent or outputs released.
- Generation captures a database snapshot, then writes files and registers the completed manifest. A crash before registration may leave an unlisted output file; it does not create a falsely completed output record.
