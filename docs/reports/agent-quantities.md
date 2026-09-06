# Specialist quantity proposals

Implemented in development-only mode. No tests, lint, typechecks, browser checks or live checks were run for this increment, following the explicit user instruction.

## Root integration contract

`quantity_models.AgentQuantityProposal` contains `item_id`, `quantity` using the existing `DecimalText` contract, nonblank `calculation`, and distinct `source_ids`. Up to 29 supporting references are accepted so adding the BOQ source row remains within the existing 30-reference quantity-proposal contract.

Add `quantity_proposals: list[AgentQuantityProposal]` to `OfficeOutput`. Root calls:

```python
validate_quantity_proposals(output, context)
publish_quantities(output, context)  # inside the owning job's final transaction
```

Both helpers live in `office_quantities.py`. Publication returns `{"quantity_proposals": [...]}`. The helpers require the BOQ item in `context.item_bases`, validate the actual read full `rate_basis` fingerprint, and require all supporting references and the BOQ source row to have been read and remain current in the same Tender.

`EstimateService` now exposes:

```python
validate_agent_quantity(tender_id, values, expected_basis) -> (request, current_rate_basis)
propose_agent_quantity(tender_id, values, run_id, expected_basis) -> QuantityProposal
quantity_item_fingerprint(tender_id, item_id) -> str
```

The agent publication requires a run belonging to the Tender, matches the full item basis captured during reading, and joins the caller's transaction. It creates no engineer decision, approval or pricing change. Existing engineer proposals and agent proposals share `_store_quantity_proposal`; only the engineer route records the engineer's provided proposal rationale as a decision.

## Record and approval basis

`QuantityProposal` adds `origin` (`engineer` or `agent`), nullable `run_id` and nullable `basis_fingerprint`. The quantity basis fingerprint covers `source_id`, `artifact_id`, `content_hash`, `description`, `unit`, `quantity_cell` and `supplied_quantity`. It excludes unrelated rates, currency, VAT and other approved proposals.

Root's pending agent quantity approval compares the stored fingerprint with `quantity_item_fingerprint` before changing state. Later source or quantity interpretation changes therefore require a fresh proposal, while unrelated rate edits do not invalidate the pending quantity's engineering basis. Existing source-currentness and calibrated measurement hooks remain in their root-owned approval/read-time paths.

The quantity is in the selected BOQ unit. The calculation is an explicitly proposed engineering explanation, suitable for volumes, grouping, deductions and other scope calculations. No new expression language, arithmetic evaluator or automatic accuracy claim was introduced. The engineer reviews the calculation and evidence before approval.

The estimate editor displays agent authorship and run lineage and explains the review required. Manual form labels now cover general quantity proposals as well as direct drawing measurements. Supplied quantities remain preserved and remain the default until the separate engineer approval.

No verification outcome is claimed for these changes.
