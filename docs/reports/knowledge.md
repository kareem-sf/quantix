# Approved reusable knowledge

Implemented 2026-09-06. All tests use temporary synthetic workspaces. No customer document, live provider, credential store or production Tender was accessed or changed.

## Files and root integration

- `backend/quantix/knowledge.py`: `KnowledgeService(repo)`.
- `backend/quantix/knowledge_models.py`: strict create/decision inputs and public note, provenance and audit schemas.
- `backend/quantix/knowledge_routes.py`: `create_router(repo)`, already prefixed `/api`.
- `backend/tests/test_knowledge.py`: behaviour and router regressions.
- This report. No shared API, repository, office, model tool, frontend or backup file was changed.

Root includes the router under existing authentication/error handling and regenerates frontend bindings. Office integration should expose only `list` and `get`, clearly labeled as reusable notes. Source IDs in a reusable note belong to its **original `source_tender_id`**. They must not be registered as read evidence in another active Tender or promoted into that Tender's factual source references.

## Service and HTTP contracts

```python
KnowledgeService(repo)
service.create(values) -> dict
service.get(knowledge_id) -> dict
service.list(*, include_withdrawn=False, category=None, offset=0, limit=100) -> list[dict]
service.withdraw(knowledge_id, values) -> dict
```

| Route                                         | Contract                                                                         |
| --------------------------------------------- | -------------------------------------------------------------------------------- |
| GET `/api/knowledge`                          | `KnowledgeRecord[]`; optional include_withdrawn, category, offset, limit (1–100) |
| POST `/api/knowledge`                         | `KnowledgeCreate` → `KnowledgeRecord`                                            |
| GET `/api/knowledge/{knowledge_id}`           | `KnowledgeRecord`, including withdrawal/audit history                            |
| POST `/api/knowledge/{knowledge_id}/withdraw` | `KnowledgeDecision` → `KnowledgeRecord`                                          |

`KnowledgeCreate` fields:

```text
title: nonblank string, maximum 200
content: nonblank string, maximum 20,000
category: preference | method | reference | price | tax
engineer_confirmed: true (strict actual boolean)
rationale: nonblank string, maximum 4,000
source_tender_id: optional original Tender ID
source_ids: optional list of up to 100 source IDs, deduplicated in order
verified_on: optional YYYY-MM-DD
recheck_after: optional YYYY-MM-DD
```

Linked source IDs require the original Tender ID and must resolve to its current source revisions at creation. A supplied original Tender ID is checked even without linked source IDs. A verification date cannot be in the future; a recheck date cannot precede a supplied verification date. Past or current recheck dates are allowed and immediately mark the note for recheck. Null characters and unrecognized input fields are rejected.

`KnowledgeDecision` contains only `engineer_confirmed:true` and `rationale`.

`KnowledgeRecord` retains all note/provenance input fields except decision inputs, plus:

```text
id, source_tender_name, sources[], status (approved | withdrawn)
approved_at, approval_rationale, withdrawn_at, withdrawal_rationale
sources_current: boolean | null (null means no linked source references)
needs_recheck: boolean
commercial_revalidation_required: boolean
revalidation_reasons[]
use_limitations: explicit applicability/verification statement
audit[]
```

Each `KnowledgeSource` exposes `source_id,tender_id,artifact_id,artifact_name,relative_path,locator,content_hash,version,evidence_hash,available,is_current`. Saved filenames, paths and locators preserve exact whitespace. `available` means the saved evidence record and its artifact metadata still resolve; it does not claim original file bytes were inspected. `is_current` checks the original artifact identity/hash/version/current flag and the evidence hash. The evidence hash binds text, locator, page/sheet/cell range, kind and metadata; extracted source text is never copied into the provenance payload.

Each `KnowledgeAudit` exposes `id,knowledge_id,action (approve | withdraw),engineer_confirmed,rationale,created_at`.

Revalidation reason codes are `source_revision_changed`, `source_evidence_changed`, `source_unavailable`, `recheck_date_reached`, `commercial_use_requires_fresh_validation` and `withdrawn`. They are calculated at read time without changing the approved payload.

## Authority and storage

Two local workspace tables are created with `CREATE TABLE IF NOT EXISTS`: `reusable_knowledge` holds immutable approved text/category/dates/provenance; `knowledge_audit` holds append-only engineer approvals and withdrawals. SQLite triggers reject UPDATE and DELETE on both tables. A unique `(knowledge_id, action)` constraint prevents duplicate decisions. Status is derived from the immutable audit, so withdrawal retains the note, original rationale and supporting provenance. Withdrawn notes are excluded from the default list but remain directly inspectable or listable with `include_withdrawn=true`.

Create and withdraw participate in `Repository.atomic()`, including a surrounding caller transaction. They create no Tender finding, message, rate, quantity or Tender-scoped decision. Global approval needs its own audit because the existing decisions table requires a Tender.

Approval authorizes reuse of the exact engineer-entered note. It does not prove that supporting sources entail every statement, independently verify the note, establish applicability to another Tender, or confirm an original source row. The public `use_limitations` statement carries these distinctions. Verification dates are engineer-entered metadata, not service certifications.

Price and tax categories **always** return `commercial_revalidation_required=true` and `needs_recheck=true`, regardless of verification/recheck dates. No note automatically installs prices or tax treatments. Other approved notes can remain available with `needs_recheck=true` when their sources change or their recheck date is reached; approved text and the original decision stay unchanged.

The tables and audit reside in the main workspace database, which the existing backup service already preserves. No new infrastructure, dependency, external storage, credential or network access was added. There is no edit or automatic promotion endpoint; revisions require a new deliberate approved note, with withdrawal available for the earlier one.

## Verification

Tests were authored before implementation. The first run demonstrated the missing service module. A later regression demonstrated Pydantic stripping source filename/locator whitespace; the public source schema was corrected to preserve exact provenance.

```powershell
backend/.venv/Scripts/python.exe -m pytest backend/tests/test_knowledge.py -q
backend/.venv/Scripts/ruff.exe check backend/quantix/knowledge.py backend/quantix/knowledge_models.py backend/quantix/knowledge_routes.py backend/tests/test_knowledge.py
backend/.venv/Scripts/ruff.exe format --check backend/quantix/knowledge.py backend/quantix/knowledge_models.py backend/quantix/knowledge_routes.py backend/tests/test_knowledge.py
```

Result: **23 tests passed; lint passed; all four Python files formatted.** One existing upstream Starlette/AnyIO TestClient deprecation warning remains.

Coverage includes strict explicit approval, validation/extra-field rejection, no Tender fact promotion, persistence across service restart, original-Tender scope checks, deduplicated source snapshots without copied extraction text, source revision/extraction changes and missing source reads, price/tax revalidation, date semantics, auditable withdrawal and duplicate refusal, SQL immutability, surrounding transaction rollback, category pagination, typed routes and exact source names/locations. Root owns router/model/UI wiring and the complete milestone verification gate.
