# Local semantic search

Implemented `SemanticService(repo)` in `backend/quantix/semantic.py` and the `SemanticStatus` / `SemanticUnavailable` types in `semantic_models.py`. No repository, API, UI, office-worker, or configuration interfaces were edited. Root integration can call:

```python
service.status(tender_id)  # SemanticStatus-compatible dictionary; no model download
service.index(tender_id, cancelled=None, progress=None)
service.search(tender_id, query, limit=20)  # existing evidence dictionary shape
```

`progress` is a synchronous callback `(percent: int, detail: str)`. Cancellation raises `InterruptedError`. Missing/stale/failed semantic retrieval raises `SemanticUnavailable`, a `ValueError` subclass with an explicit `.code`. Search never delegates to keyword search. No extra router is required; root retains authentication and job lifecycle wiring.

## Model and dependencies

Verified installed: FastEmbed **0.8.0**, numpy **2.5.2**, ONNX Runtime **1.29.0**, and huggingface-hub **1.30.0**. FastEmbed and numpy were added by root. The service also directly uses huggingface-hub's documented `snapshot_download` API; it is installed as FastEmbed's dependency.

The model is `intfloat/multilingual-e5-small`, pinned to revision `614241f622f53c4eeff9890bdc4f31cfecc418b3`, with 384-dimensional mean-pooled, normalized embeddings. Official FastEmbed documentation explicitly supports this model through `TextEmbedding.add_custom_model`. Inspected FastEmbed source confirms the generic query/passage methods do not add E5's required prefixes; this service explicitly supplies `query: ` to `query_embed` and `passage: ` to `passage_embed`.

ONNX inference is CPU-only and local. It uses a fixed local model path and `local_files_only=True`. Public model downloads are pinned, permit only the required tokenizer/config/ONNX files and README, explicitly disable implicit credentials and telemetry, and remain under `repo.home/models`. No source text or queries are passed to the download worker or an external inference endpoint. The model's ONNX file is 470,268,510 bytes.

The optional Hugging Face Xet transfer stalled on this computer. The documented `HF_HUB_DISABLE_XET=1` path completed the ordinary HTTP download and retains resumable partial files. Downloads run in an owned hidden subprocess tree with cancellation and a 15-minute ceiling. Progress reports phases, not invented byte percentages. ONNX model initialization and one embedding batch are synchronous library calls; cancellation is checked before and after those calls, rather than claiming immediate interruption inside native inference.

Sources: [FastEmbed registration and usage](https://pypi.org/project/fastembed/), [supported models](https://qdrant.github.io/fastembed/examples/Supported_Models/), [version-pinned Hub downloads](https://huggingface.co/docs/huggingface_hub/guides/download), [E5 model](https://huggingface.co/intfloat/multilingual-e5-small).

## Storage and source integrity

Derived data is durable in `repo.home/semantic.sqlite`. Vectors are normalized float32 values, deduplicated by exact chunk content and model fingerprint. Separate occurrence records retain every Tender/evidence reference and character offset. Search returns the best matching chunk per evidence occurrence, preserving original document/page/sheet/cell locators and complete source text. `metadata.semantic_match` adds the matched source substring and offsets; it does not replace the original evidence.

Index state fingerprints current artifact/content/evidence/metadata, extractor implementation, model revision, library version, pooling/prefix behavior, and chunk settings. Source revisions, model changes, or extractor changes make the index stale. Live joins require both the selected Tender and current artifacts when retrieving hits, so obsolete and foreign sources cannot be returned. Source changes during embedding prevent a false ready index. Published references and state change atomically; cancelled builds leave the previous published index intact and may reuse completed vector batches.

Status distinguishes `empty`, `model_missing`, `not_indexed`, `ready`, `stale`, and `limit_exceeded`. Missing stored vectors are recognized as an incomplete index rather than producing a misleading empty search result. Similarity scores rank relevance; they are not calibrated confidence or evidence validation.

Limits: 100,000 evidence rows / 64 MiB source text, 50,000 unique chunks, 300,000 occurrence references, 1,200 characters per initial chunk, 120-character maximum overlap, 480-token safety threshold within E5's 512-token context, 16 passages per embedding batch, and at most 100 returned sources. Whole original evidence remains in the repository. Search fetches full source text only for selected results.

## Verification and integration fix

Eleven generated behavior tests verify real repository persistence and Tender isolation with synthetic vectors only in tests: relevance without lexical overlap, occurrence-preserving deduplication, revision exclusion, model/extractor invalidation, cancellation, bounded offsets, invalid vectors, missing-index exceptions, source changes during indexing, and model-download process cleanup. Ordinary tests download no models and send no data.

A separate real FastEmbed/ONNX smoke test embedded three public synthetic construction passages. The English rain-leak query ranked waterproofing first at cosine **0.8613** (alternatives 0.7868 / 0.7789); an Arabic equivalent also ranked waterproofing first at **0.7947** (alternatives 0.7228 / 0.7026). A temporary real repository indexed and searched successfully with this model. These small checks establish working local multilingual inference, not overall retrieval accuracy on the customer's package.

During download verification, the Windows virtual-environment executable was observed launching an underlying Python child. Root authorized the additional shared `processes.stop_owned_process_tree(Popen, timeout=3)` helper and its use in semantic download and legacy Word conversion. It targets only the caller's retained process tree with Windows taskkill, not process names. A real subprocess/grandchild test proves owned descendants exit while an unrelated process remains alive. Word's separate identity-checked WINWORD cleanup remains intact. No new dependency was needed.

Final targeted command: `backend/.venv/Scripts/python.exe -m pytest backend/tests/test_semantic.py backend/tests/test_processes.py backend/tests/test_document_word.py backend/tests/test_documents.py -q`. Result: **35 passed**. Owned Python files pass Ruff lint and formatting. No commits were made.
