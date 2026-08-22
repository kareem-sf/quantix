# ADR 0015: Record secure operational diagnostics

- Status: Accepted
- Date: 2026-08-21

## Context

Quantix already stores authoritative Tender audit events, Agent Runs, provider events, and parse attempts inside each Tender Store. Those records are deliberately domain-focused and do not explain enough of the host lifecycle to diagnose startup, package intake, parsing, embedding, provider protocol, and cleanup failures without live inspection.

Operational diagnostics must remain local, non-authoritative, non-blocking, and safe to share. They must not become a second audit trail or a path for Tender content, credentials, prompts, tool payloads, provider responses, filesystem paths, or hidden reasoning to escape their governed stores.

## Decision

Quantix records versioned newline-delimited JSON under the Application Home:

```text
logs/application/YYYY-MM-DD/000001.jsonl
logs/tenders/<tender-id>/YYYY-MM-DD/000001.jsonl
```

Application events and Tender events are physically separated. A Tender timeline merges the application stream with that Tender's stream only at inspection time. Persisted events use a typed allowlist of stable names, summaries, scalar facts, normalized outcomes, error codes, and correlation identifiers.

A per-`QuantixHost` diagnostics store owns bounded non-blocking queues and a single writer/router. Files rotate at a UTC day boundary or 32 MiB. Retention is the earlier of 90 days or 5 GiB aggregate, with size enforcement purging closed segments to 4.5 GiB. Diagnostics failure degrades diagnostics health but never blocks ordinary Tender work.

Deep diagnostics are an explicit, redacted Tender-scoped session limited to 60 minutes and 100 MiB. Deep mode adds bounded protocol and process metadata, never raw content. Permanent Tender deletion closes the Tender writer and erases exactly that Tender's log subtree before reporting local deletion complete.

Quantix exposes a paginated in-app timeline, health status, an exact host-owned open-folder action, and a locally generated redacted `.qdiag.zip` support bundle. Support bundles are never uploaded by Quantix.

## Consequences

- Operational logs supplement rather than replace canonical Tender records.
- A crash can lose the bounded queued tail, but not committed Tender data.
- All new instrumentation must use the typed diagnostics API; unrestricted logging and renderer console capture are prohibited.
- Exported copies moved outside the Application Home are outside Quantix retention and deletion control.
- No database migration or compatibility logger is introduced.
