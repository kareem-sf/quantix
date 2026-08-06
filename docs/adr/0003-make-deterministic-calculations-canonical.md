# Make deterministic calculations canonical

Quantix makes its deterministic Calculation Engine the sole authority for calculated Tender values; Codex may extract Proposed inputs, select approved rules, request scenarios, and explain results, while spreadsheets and external engineering tools remain controlled inputs or renderers. We chose immutable Calculation Runs and versioned Calculation Manifests over LLM arithmetic or spreadsheet-led totals so every commercial, programme, evaluation, and supported engineering commitment is dimensionally valid, reviewable, and reproducible.

## Consequences

- Canonical arithmetic uses exact decimals or integers with explicit dimensions, units, currencies, Exchange Rates, precision, and Rounding Policies.
- Calculation Rules pass tests, independent review, and EITL approval before activation; safety-critical engineering also requires an approved discipline rule or verified tool adapter.
- Calculation Runs bind exact input revisions, rules, policies, scenario, engine version, intermediate values, results, provenance, and hashes. Missing, incompatible, or invalid inputs fail visibly instead of being guessed or coerced.
- Scenarios never overwrite one another, direct result overrides are forbidden, and every deliberate Calculation Adjustment remains a separate reviewed and EITL-approved input.
- Routine runs receive deterministic validation and policy-driven review. EITL approves grouped Calculation Manifests for the Basis of Estimate, material assumptions and adjustments, safety-critical engineering, Priced Cost Baseline, selected pricing scenario, and Approved Tender Price.
- All documents and exports render the same canonical Calculation Runs. Unexplained reconciliation differences block release, and spreadsheet edits re-enter through Intake and Change Assessment rather than changing the Tender Store directly.
- Approved manifests preserve enough information to reproduce results across supported machines, locales, and timezones. Rule or engine changes create new runs and invalidate only affected current work.
