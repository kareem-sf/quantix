# Agent business integration

Completed the scoped manager/specialist integration on 2026-09-06. No live model, SMTP or IMAP calls occurred.

## Publication boundary

The current architecture remains intact: run_manager/run_specialist return PreparedOfficeResult. Model tools only read business records and evidence. prepare_result validates proposals and captures the local read context; it creates no quotation draft or rate proposal records. publish_prepared revalidates the context and publishes only inside the job owner's existing repo.atomic finalization transaction. Cancellation and job lifecycle remain root-owned.

PreparedOfficeResult now also carries immutable tuples for item basis fingerprints, trusted engineer/scoped-quote recipients, and recipient addresses actually exposed by read source fragments. These are local runtime data, not model-controlled output fields. A source email found only in an unread tail cannot justify a draft. Contacts named in the current engineer request or approved engineer-scope rationale remain available through publication.

Quote drafts use QuoteService.create_draft. Rate proposals use EstimateService.propose_rate. A subsequent publication failure rolls back all main-database draft/proposal/message changes together. QuoteService's external append-only draft-creation marker can remain after a rolled-back draft, but it contains no submitted message or domain quote record and cannot send anything.

## Model output types

OfficeOutput adds:

- `quote_drafts: list[DraftInput]`, maximum 8. Existing fields are to,cc,subject,body,attachment_ids,source_ids. No send or approval fields exist. Recipients must come from actually read current Tender evidence, the engineer's request/approved scope, inspected quotation history within the Tender, or this run's web_findings details whose URLs were actually consulted. Researched contacts remain proposed and retain their web finding/source attribution in the published run result. Attachments must belong to the Tender, be current originals, and have inspected evidence. QuoteService rechecks the actual file bytes before creating the draft.
- `unit_rate_proposals: list[UnitRateProposalInput]`, maximum 20. UnitRateProposalInput adds item_id to UnitRateInput. The model cannot set a quantity, confirm_source, engineer_confirmed or rationale.

UnitRateInput fields: unit_rate OR nonempty components, currency, tax_basis (`excluding_vat|including_vat|unknown`), nullable vat_percent, provenance. Components retain `{name,quantity,unit_rate,unit}`. Provenance reuses RateSource: `{basis:observed|estimated,observed_on,source_ids,urls,geography,conditions}`. Decimal values remain strings. A proposal must contain exactly one direct rate or component build-up. VAT remains unknown when null.

Local proposal source IDs must have been read and must still be current in the selected Tender. Web URLs must have been returned by the current run's native research. Existing market `price_proposals` remain distinct from BOQ-linked `unit_rate_proposals` and installed estimate values.

## Read-only tools

The existing keyword, document, source and visual-page tools remain available. Added tools are:

- `inspect_tender_records(record_type, offset, limit)` lists paginated findings, decisions, tasks, runs or messages, retaining real proposed/accepted/status/stale labels and marking truncated previews. It reaches records beyond the initial prompt's history limits.
- `read_tender_record(record_type, record_id, offset=0, limit=8000)` returns full stored record JSON as bounded character pages with json_text,text_offset,total_characters,next_offset. record_type is a closed enum and all queries are Tender-scoped. It can retrieve remaining long text/structured work results without marking their source references as read evidence.
- `inspect_estimate(offset, limit)` returns current BOQ rows, installed commercial values, pricing completeness/unknowns, basis fingerprints and actual source evidence. It captures item basis hashes locally for later validation. Response items are `{item:EstimateItem,basis_fingerprint,source}`.
- `inspect_quote_requests(offset, limit)` returns bounded quotation text, recipients, stored status and newer local delivery-history facts. It never creates, approves or sends mail.
- `read_quote_replies(quote_id, offset, limit)` returns reply provenance, warnings and actual source evidence. The first two source segments per reply are read; remaining_source_ids are listed for explicit follow-up reading. Receipt does not approve a commercial rate.
- `search_semantic_sources(query, limit)` calls SemanticService.search on the local index. Missing/stale/unavailable indexes return `{available:false,status,detail,sources:[]}`. There is no keyword fallback and no automatic model download/index creation. Valid hits read the matching character offsets, not always the beginning of the source.

Page limits are 1–20. read_source now accepts optional `offset=0,limit=12000`, returning text_offset,text_length,next_offset and text_is_partial. Source metadata preserves supplier sender/date/message provenance alongside spreadsheet cell information. Existing invocations without offset retain their behavior. All new function tools use strict schemas and do not accept a caller-selected Tender ID.

## Full engineer context

The worker preserves the complete validated engineer instruction rather than truncating at 12,000 characters. An explicit 40,000-character worker limit covers the existing API's 20,000-character request plus 10,000-character prepended preferences; larger inputs are rejected before model execution. Full stored standing preferences are included separately as standing_engineer_preferences for managers and specialists. Approved scope rationale remains explicit. Assigned task descriptions retain up to the repository's full 10,000-character limit.

Credential/path redaction is separate from clipping, so replacement text cannot accidentally remove the final binding instruction. Full record pagination also redacts before pagination rather than silently shortening stored text. Existing jobs currently prepend manager preferences too; root may remove that duplicate prefix because the worker now supplies the separate field to both manager and specialists. The permitted test-double adjustment adds Repository.setting to MemoryRepository; no production fallback was introduced.

## Rate proposal service and routes

New EstimateService methods:

```python
rate_basis(tender_id, item_id) -> dict  # basis, fingerprint, source_id
validate_rate(tender_id, item_id, values, expected_basis=None) -> tuple
propose_rate(tender_id, item_id, values, *, expected_basis=None, run_id=None) -> dict
get_rate_proposal(tender_id, proposal_id) -> dict
list_rate_proposals(tender_id) -> list[dict]
approve_rate(tender_id, proposal_id, values) -> dict
```

The new rate_proposals table preserves immutable proposal payload, source IDs and original item basis. SQLite triggers reject updates/deletion of that content. Approval changes only decision state and records the resulting installed basis; the existing decisions ledger records the formal engineer rationale.

The basis includes the source hash, source/item fields, installed rate/components, approved measurement selection, and linked source versions/currentness. A changed item, rate, quantity selection or supporting source invalidates a pending proposal. Approval never silently overwrites those changes.

Approve_rate requires RateApproval `{engineer_confirmed:true,rationale,confirm_source:false}`. confirm_source is an explicit optional checkbox; it is never supplied by the model. Approval invokes existing update_item monetary validation, preserving the Decimal/VAT guards. Supplied/approved quantities remain unchanged. Approval with confirm_source false leaves an unconfirmed source row unconfirmed and its totals incomplete. The same proposal cannot be approved twice.

New endpoints, already included by the existing estimate router factory:

- GET `/api/tenders/{tender_id}/estimate/rate-proposals` -> list[RateProposalRecord]
- POST `/api/tenders/{tender_id}/estimate/rate-proposals/{proposal_id}/approve`, RateApproval -> RateProposalRecord

RateProposalRecord fields: id,tender_id,item_id,payload,basis,basis_fingerprint,approved_basis_fingerprint,source_ids,run_id,status (`proposed|approved`),is_current,created_at. An approved record remains current only while its installed basis and sources still match. Root should regenerate frontend types from OpenAPI and expose the exact proposal basis/conditions before the engineer approves.

publish_prepared's returned job result additionally contains quote_drafts and unit_rate_proposals with their actual persisted records. No model-facing send, mail-account, sync, source-confirmation or rate-approval tool was added.

## Verification

```powershell
backend/.venv/Scripts/python.exe -m pytest backend/tests -q
backend/.venv/Scripts/ruff.exe check backend/quantix/office_business.py backend/quantix/office_types.py backend/quantix/office.py backend/quantix/office_tools.py backend/quantix/estimates.py backend/quantix/estimate_models.py backend/quantix/estimate_routes.py backend/tests/test_office_business.py backend/tests/test_rate_proposals.py
```

Result: **176 passed, 1 skipped in the full backend run; lint passed.** After the final redaction-expansion fix, all **52 office/business/SDK/integration-guard tests** were rerun and passed. The existing skipped test requires Windows symlink-creation privilege. One upstream Starlette/AnyIO TestClient deprecation warning remains. The full run includes existing prepared-result cancellation/transaction, source version, semantic, visual, correspondence, backup and monetary integration guards.

The 20 new tests cover the agent-to-draft/rate-proposal chain, absence of pre-publication domain writes, atomic rollback, unread items/recipient fragments, current-source and item-basis changes, immutable proposal content, explicit engineer approval, preserved quantities, reply evidence, semantic unavailable behavior and hit offsets, typed routes, approved engineer contacts, attributed public research contacts, unsourced recipient rejection, full instructions/preferences, oversize rejection, redaction expansion and full/paginated record access.

## Limits

This increment proposes commercial work; it does not independently prove that a supplier quotation, researched contact or numerical interpretation is correct. Public research can supply draft recipients when attributed to this run's consulted web sources, while sending still requires exact-content engineer approval. Contacts identified only visually need readable/attributed evidence or an explicit engineer address. Rate proposals require an existing BOQ candidate and preserve existing quantity/VAT decisions. No live provider/mail validation or client UI changes were performed by this task.
