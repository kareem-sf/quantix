# Retrieval baseline — 13 September 2026

Status: Task 1 of the [retrieval improvements plan](../superpowers/plans/2026-09-13-retrieval-improvements.md). The production ranker was not changed. This records the measured starting point for later work.

## What was measured

A versioned synthetic set of **88 questions** (66 train, 22 held-out) over **47 labeled documents**, covering every acceptance category in the [embedding analysis](2026-09-13-embedding-retrieval-analysis.md). The original six bilingual smoke queries are retained as a subset. Held-out questions were not used to choose boosts, glossaries or cutoffs.

- Dataset version `2026-09-13.1`, corpus hash `da9dd93ed7793ab2277a18a26d9dccff2e805dcb689a070cbc9a0f934047eb0e`.
- Rankers: current keyword (`Repository.search`), meaning (`SemanticService.search`), combined (`hybrid_search`), and meaning with the bilingual boost helper forced to zero.
- Isolated home: `~/.quantix/cache/development/2026-09-13-retrieval-baseline`.
- Full run JSON: `benchmarks/retrieval-baseline-686806ab2d124e84aa21b9f5e3bd05bc.json`.
- Local model: `intfloat/multilingual-e5-small` revision `614241f622f53c4eeff9890bdc4f31cfecc418b3`, files present. Hugging Face registry verification was not performed (`registry_verified` left unset, not recorded as true or false-zero).
- Machine: Python 3.12.10, Windows 11, AMD64, 8 CPUs. Process RSS peak is unavailable on this Windows runtime and is recorded as null.

The evaluator refuses a pass when the corpus or ranker-config hash does not match, when a foreign Tender or out-of-scope passage appears, when a superseded passage is presented as current, or when an absent-answer question returns a non-weak hit. Average recall cannot waive those failures. Work-product, knowledge, research and exhaustive-list questions are scored as unavailable for the current source index, not as zero recall.

## Quality (measured cases only)

| Method | recall@5 | recall@10 | nDCG@10 | MRR | identifier accuracy | scoped recall@10 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| words | 0.873 | 0.887 | 0.793 | 0.769 | 0.656 | 0.667 |
| meaning | 0.958 | 0.958 | 0.818 | 0.771 | 0.594 | 1.000 |
| combined | 0.930 | 0.944 | 0.842 | 0.810 | 0.688 | 1.000 |
| meaning, no boost | 0.944 | 0.972 | 0.813 | 0.764 | 0.594 | 1.000 |

Held-out combined: recall@10 **0.938**, MRR **0.799**, identifier accuracy **1.0**. Train combined: recall@10 **0.945**, MRR **0.813**.

Each method measured 71 labeled source questions, left 9 unavailable, and recorded **8 critical failures**. `passed` is therefore false for every method.

## Critical failures (blocking)

All eight are `supported_answer_on_absent` (finding R13):

- Unrelated: Neptune orbital period, World Cup winner, nitrogen boiling point.
- No readable text: roof-plan DWG, scanned addendum (train and held-out).
- Missing facts: clause 14.8, concrete class C25/30.

False-supported rate on the unrelated slice is **1.0** for every method. The current rankers always return nearest neighbors. That gate is blocking for later adoption; it is not waived by the recall figures above.

No foreign-Tender, forbidden-scope or stale-as-current hits were observed in this run when scope was applied to combined search. Keyword scoped recall@10 was 0.667 (one of three scoped questions starved after post-filtering). Combined and meaning scoped recall@10 were 1.0 on this set.

## Smoke subset on the larger corpus

The original six-query / six-document smoke overstated quality. On this 47-document store, combined ranks for those same wordings were 3, 4, 3, 3, 3 and **missing** (VAT/Cairo). Keyword missed curing and VAT. Meaning still placed all six in the top 5. Later work must not treat the six-query script as the relevance gate.

## Bilingual boost ablation (R11)

Disabling the three-group boost raised meaning recall@10 from 0.958 to 0.972 and slightly lowered MRR (0.771 → 0.764). On the specific Arabic pump query `قدرة مضخة الخرسانة`, boost moved the hit from rank 7 to rank 3. That is not enough to keep the heuristic; it remains an unvalidated overlap with the old pump cases. No glossary is adopted from this run.

## Operations

| Measurement | Value |
| --- | --- |
| Initial index (includes first model load) | 6.142 s |
| Changed-only reindex after the pump revision | 0.161 s |
| Keyword query p50 / p95 | 10.1 / 14.3 ms (79 queries) |
| Meaning query p50 / p95 | 84.0 / 197.7 ms |
| Combined query p50 / p95 | 122.8 / 210.4 ms |
| Memory peak | unavailable on Windows |

Method order was words → meaning → combined → no-boost, so later meaning timings are warmer than the first meaning pass. Do not compare meaning vs no-boost latency from this run.

## Review thresholds (set before any ranker change)

These are review references, not tuning targets, and must not be fitted to held-out answers.

- Do not regress held-out combined recall@10 below **0.937** or MRR below **0.799** without a documented slice-level trade-off.
- Identifier accuracy on the combined held-out slice is currently 1.0; overall combined identifier accuracy is **0.688**. Watch clause/grade/unit misses separately from average recall.
- Critical permission/provenance failures remain blocking. The absent-answer / no-text gate must reach **zero** false supported answers before a relevance improvement can be called complete.
- Keep the no-boost ablation in later measurements. Do not add glossary terms from this dataset.

## Checks run

- `backend/.venv/Scripts/python.exe -m pytest backend/tests/retrieval -q` — 16 passed, then 17 after the frozen-manifest test.
- `backend/.venv/Scripts/python.exe scripts/benchmark_multilingual_retrieval.py --suite baseline` — real local E5, synthetic corpus only.
- Ruff check on the new evaluator and retrieval tests.

No production ranking, API schema, account, real Tender or commercial-send change was made. Live Manager/staff answer quality is not claimed from this retrieval-only baseline.
