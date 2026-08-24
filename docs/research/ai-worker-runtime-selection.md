# AI worker runtime selection

**Evidence snapshot:** 2026-08-24

**Decision:** Direct Codex app-server 0.149.1 for account-backed execution and a
disposable Python 3.12.13/Pydantic AI 2.33.0 worker for general providers.

## Question

What is the smallest durable runtime boundary for the newly approved account,
direct-provider, and compatible-endpoint connections while the Rust Host retains
all Quantix workflow and security authority?

This record updates the conclusion, not the historical analysis, in
[Agent-framework selection for Quantix v0](./agent-framework-selection.md). That
research correctly rejected a second orchestration authority. Its second-provider
revisit condition is now satisfied, so a model-facing framework may be used only
inside a disposable worker.

## Account-backed Codex: direct app-server versus SDK

| Candidate | Exact version | Runtime cost | Tool and security boundary | Retry behavior | License | Decision |
| --- | --- | --- | --- | --- | --- | --- |
| Official `@openai/codex` app-server | `0.149.1-win32-x64` | One pinned native Codex child process over supervised stdio JSON-RPC; no Node runtime in production | The Rust Host consumes generated protocol directly, treats `dynamicTools` as additive, executes only Host-declared tools, and fails on any built-in action. Host-managed `chatgptAuthTokens` remains experimental and non-shippable without written approval. | Reserved account-provider recovery is fixed at four request retries and five interrupted-stream retries. Quantix adds no outer retry. | Apache-2.0 | Adopt for account-backed execution. |
| Codex Python SDK | `openai-codex==0.147.0`, depending on `openai-codex-cli-bin==0.147.0` and requiring Python 3.10+ | Adds a Python wrapper and its pinned CLI binary while still relying on the Codex runtime; its bundled 0.147.0 runtime also does not match the selected 0.149.1 contract | The higher-level thread/turn/login API does not replace direct validation of app-server account, approval, lifecycle, event, and tool surfaces. The Python SDK is not the production security boundary. | Adds no acceptable Quantix retry authority; the underlying reserved provider retains its runtime-owned recovery. | Apache-2.0 | Reject as the production account boundary. |

The app-server is the account-backed product integration. Quantix privately
prototypes only the pinned `chatgptAuthTokens` seam needed to pass Host-owned,
vault-backed access tokens to it. Quantix never constructs or calls the private
ChatGPT execution backend; the official runtime's upstream route is opaque.

## General providers: worker framework comparison

| Candidate | Exact version | Runtime and packaging cost | Tool boundary | Retry behavior | License | Decision |
| --- | --- | --- | --- | --- | --- | --- |
| Pydantic AI slim | `pydantic-ai-slim[anthropic,google,openai,xai]==2.33.0` on Python `3.12.13`, installed by uv `0.12.2` | Reuses Quantix's verified Python/uv provisioning; one isolated, non-editable, locked environment and one disposable `python.exe -I -m quantix_ai_worker` process per operation | Provides provider normalization, streaming, structured output, and external/deferred tools. The Host owns schemas, permission, execution, idempotency, budgets, approval, persistence, and canonical state. It is not a workflow or durability engine. | Provider SDK automatic retries and Pydantic tool/output retries are disabled. `FallbackModel` is forbidden. The Host may authorize one safe bounded same-request retry. | MIT | Adopt only inside the disposable general-provider worker. |
| Vercel AI SDK | `ai@7.0.77` with provider packages such as `@ai-sdk/openai@4.0.46`, on Node `22` | Adds Node 22 plus a separate dependency, packaging, update, process-hardening, and license-inventory path | Its tool loop could be constrained behind Host RPC, but it does not improve the authority boundary over the existing Python worker and would still require provider-specific hardening. | SDK/provider retry defaults would have to be disabled and replaced by the same Host policy; fallbacks remain forbidden. | Apache-2.0 | Reject for Layer 1. |
| Raw Rust adapters | Existing Rust Host; no added framework version | No second language runtime, but Quantix must implement and continuously maintain six request, auth, streaming, tool-delta, structured-output, usage, and error protocols | Direct control is strong, but all provider protocol correctness and drift handling become Quantix-owned trusted code inside the Host. | Quantix would have to implement, audit, and test every transport and semantic retry rule separately. | No framework license; each Rust dependency retains its own license | Reject because maintenance and trusted-code cost exceed the runtime saving. |

The Pydantic AI lock, not transitive version ranges, is authoritative. The
researched resolution includes OpenAI 3.3.1, Anthropic 1.0.0, Google Gen AI
2.19.0, xAI SDK 1.19.0, Pydantic 2.13.4, Pydantic Core 2.46.4, Pydantic Graph
2.33.0, HTTPX2 2.12.0, and Tiktoken 0.14.0. The checked-in final lock must also
pin the isolated build backend.

## Selected boundary

```text
Rust Quantix Host (only authority and canonical writer)
    |-- supervised stdio JSON-RPC -> Codex app-server 0.149.1
    |       `-- account-backed upstream route remains opaque
    `-- private JSONL IPC -> disposable Python 3.12.13 worker
            `-- Pydantic AI 2.33.0 -> direct/compatible providers
```

All persistent secrets at rest exist only in the current-user-DPAPI-encrypted
vault. During an authorized operation, only the exact Host and assigned worker
may hold the selected credential in memory/private IPC. Neither worker may own
Tender workflow, durable state, approval, fallback, or canonical publication.

## Release and compatibility implications

- The 0.149.1 generated schema labels `chatgptAuthTokens` unstable,
  OpenAI-internal-only, and “do not use.” Account activation stays out of public
  builds until OpenAI approves the Quantix client/integration in writing.
- `dynamicTools` and `chatgptAuthTokens` are pinned experimental seams. Their
  removal or drift makes only the account connection incompatible.
- The Codex read-only sandbox cannot prove restricted readable roots. Private
  prototyping uses an empty staged workspace and rejects built-in actions, but
  production account activation also requires an approved filesystem boundary.
- General-provider worker failure or incompatibility blocks that exact operation;
  it never invokes Codex, another provider, model, or connection.

## Primary sources and package evidence

- [OpenAI: Codex as a platform](https://developers.openai.com/blog/codex-as-a-platform)
- [OpenAI Codex app-server](https://learn.chatgpt.com/docs/app-server)
- [OpenAI Codex SDK](https://learn.chatgpt.com/docs/codex-sdk)
- [OpenAI Codex authentication](https://learn.chatgpt.com/docs/auth)
- [Codex 0.149.1 npm metadata](https://registry.npmjs.org/@openai%2Fcodex/0.149.1)
- [Codex Python SDK documentation](https://github.com/openai/codex/tree/main/sdk/python/docs)
- [Codex Python SDK 0.147.0 package metadata](https://pypi.org/project/openai-codex/0.147.0/)
- [OpenAI Codex Apache-2.0 license](https://github.com/openai/codex/blob/main/LICENSE)
- [Pydantic AI providers](https://pydantic.dev/docs/ai/models/overview/)
- [Pydantic AI deferred tools](https://pydantic.dev/docs/ai/tools-toolsets/deferred-tools/)
- [Pydantic AI retries](https://pydantic.dev/docs/ai/retries/)
- [Pydantic AI 2.33.0 package metadata](https://pypi.org/project/pydantic-ai/2.33.0/)
- [Vercel AI SDK providers](https://ai-sdk.dev/docs/foundations/providers-and-models)
- [Vercel AI SDK 7.0.77 npm metadata](https://registry.npmjs.org/ai/7.0.77)
- [Layer 1 AI connection foundation design](../superpowers/specs/2026-08-24-layer-1-ai-connection-foundation-design.md)
