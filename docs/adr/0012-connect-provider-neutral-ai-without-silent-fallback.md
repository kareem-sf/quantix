---
status: accepted
---

# Connect provider-neutral AI without silent fallback

Quantix supports three peer AI Provider Connections behind its existing Quantix-owned AI Provider Contract: an OpenAI account through Codex-managed ChatGPT login, Anthropic through BYOK, and Gemini through BYOK. Quantix remains the Tender workflow, permission, evidence, audit, and approval authority; each provider supplies bounded intelligence through a provider-native adapter and receives no authority over canonical Tender state.

This decision supersedes ADR 0008's Codex-only scope and the Codex-only authentication, credential, and BYOK consequences in ADR 0009. Their Host-owned Provider Contract, permission, thread-exposure, run-integrity, process-supervision, and fail-closed controls continue to apply.

## Consequences

- Quantix initially permits one Provider Connection for each supported provider. Several providers may be connected simultaneously, but exactly one application-wide AI Execution Selection supplies the default Provider Connection, provider-qualified model, and provider-native reasoning setting for future Agent Runs.
- Quantix does not add per-Agent provider selection, multiple accounts for one provider, a model router, a fallback chain, an OpenAI API-key connection, local-model support, or a generic OpenAI-compatible endpoint in this layer.
- The OpenAI connection uses a supervised, version-pinned Codex app-server over its local protocol. Codex owns the documented ChatGPT browser login, device-code fallback, credential persistence, refresh, logout, account state, live model catalogue, threads, turns, and streamed events. Quantix does not read another Codex installation's credential file, manage raw ChatGPT tokens, or call undocumented ChatGPT backend routes.
- Anthropic and Gemini connections use their first-party APIs with Tendering Engineer-supplied API keys. Provider Credentials are Secret data held in the operating-system credential vault and never enter Application Settings, `installation.sqlite`, a Tender Store, Provider Turn context, logs, diagnostics, backups, archives, or exports.
- Removing the OpenAI connection invokes Codex-managed logout. Removing an Anthropic or Gemini connection deletes its local vault secret and connection facts; because Quantix did not create the external key, it explains that revocation must be completed in the provider console.
- Each adapter uses its provider's native request, streaming, tool, usage, error, and recovery semantics. Quantix normalizes only its own AI Provider Contract and does not pretend provider protocols are interchangeable wire formats.
- Model lists are loaded from the ready connection's live provider catalogue. Codex reasoning choices come from live `model/list` metadata, and Anthropic effort choices come from live Models API capability flags. Gemini exposes a live model catalogue but not exact per-model thinking levels or budgets, so it initially offers only Provider default or Automatic reasoning; Quantix adds explicit choices only when a machine-readable live provider capability proves them.
- A cached Provider Capability Catalogue may explain an earlier selection while offline but cannot authorize new AI work. Missing, stale, removed, incompatible, or unprovable capabilities fail closed rather than being reconstructed from a Quantix enum, model-name pattern, compatibility table, undocumented endpoint, or paid speculative probe.
- Every new Agent Run immutably captures its Provider Connection identity, provider-qualified model, provider-native reasoning selection, catalogue provenance, and adapter/runtime version. Changing Application Settings affects only future runs; active, queued, interrupted, and indeterminate runs retain their captured configuration.
- Quantix never silently substitutes a different provider, model, or reasoning setting. If credentials, quota, connectivity, a selected model, or a required capability becomes unavailable before turn acceptance, affected work enters Waiting for AI Provider while non-AI Tender work remains available. The same work may resume automatically when the same selection is ready; changing selection requires an explicit Tendering Engineer decision and a separately attributable run where necessary.
- A failure in one Provider Connection does not disable another ready connection. Cross-provider movement remains an explicit decision because it changes data destination, account, cost basis, model behavior, and potentially the resulting work.
- Connecting a provider presents a concise data-destination and cost-account disclosure. The active provider and exact captured selection remain visible in Application Settings and Agent Run details without repeated approval prompts for every ordinary turn.
- The Codex app-server integration remains version-gated and fail-closed on protocol or mandatory-capability drift. Public or commercial claims that an Engineer may use a consumer ChatGPT subscription remain blocked until applicable OpenAI terms and product authorization support Quantix's intended distribution; open-source implementations that manage ChatGPT tokens directly establish technical feasibility, not that authorization.

## Evidence

- [Multi-provider authentication and model-discovery research](../research/multi-provider-auth-model-discovery.md)
- [Official Codex app-server documentation](https://learn.chatgpt.com/docs/app-server)
- [Official Anthropic authentication documentation](https://platform.claude.com/docs/en/manage-claude/authentication)
- [Official Anthropic Models API](https://platform.claude.com/docs/en/api/models/list)
- [Official Gemini API-key documentation](https://ai.google.dev/gemini-api/docs/api-key)
- [Official Gemini Models API](https://ai.google.dev/api/models)
- [Official Gemini thinking documentation](https://ai.google.dev/gemini-api/docs/generate-content/thinking)
