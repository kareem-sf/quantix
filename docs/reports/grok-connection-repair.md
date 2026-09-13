# Grok connection repair — 8 September 2026

## Finding

The reproduced connection symptom was a current allowance with zero prepaid
balance and zero on-demand cap while Quantix displayed
`auto_topup_enabled: null` and paused included-only readiness. The worker was
already reading the billing counters correctly. Its auto-top-up normalizer
treated a successful response with no configured rule as unknown, so the
readiness guard failed closed even when the provider had positively returned a
disabled/no-rule state. No account identifiers or private billing history are
included here.

## Source-backed response contract

The pinned official Grok Build source defines `GetAutoTopupRuleResponse` as a
wrapper containing `rule: Option<AutoTopupRule>`. Its `AutoTopupRule.enabled`
field uses a false default because proto3 JSON omits false scalar fields. The
official pager source separately defines `AutoTopupInfo::disabled()` as a
known disabled state and documents `AutoTopupFetch::Resolved` as including that
state when the backend reports no rule. Its unresolved `None` means the rule
has not been fetched, so a successful wrapper with no rule must not be mapped
to unknown. Therefore these source-backed successful response shapes establish
that automatic top-up is disabled:

```json
{}
{"rule": null}
{"rule": {}}
{"rule": {"enabled": false}}
```

The worker now accepts only that wrapper and its documented rule fields. The
focused tests exercise the four disabled shapes above, plus an enabled rule
and malformed envelopes using synthetic fixtures; they do not claim that any
one of those raw shapes was captured from the signed-in account. A
non-object rule, an unknown envelope field, an unknown rule field, or a
non-boolean `enabled` value remains unknown and keeps included-only access
paused. HTTP/protocol failures still raise before normalization.

The following is a synthetic test shape (25% used), derived from the public
`SubscriptionUsage` contract; it is not a raw account observation:

```json
{
  "used_percent": 25.0,
  "prepaid_balance_usd": "0.00",
  "on_demand_cap_usd": "0.00",
  "on_demand_used_usd": "0.00",
  "auto_topup_enabled": false,
  "included_only_allowed": true
}
```

No account identifiers, credentials, provider URLs or raw private response
body are persisted or reported by this repair.

## Changes and focused validation

`grok_auth.py` now maps the official successful no-rule response to `False`,
while preserving `None` for malformed shapes. The focused test file covers
omitted/null rules, proto-default and explicit-field disabled rules, enabled
rules, non-boolean values, and successful-looking error/malformed envelopes.
The payloads are synthetic fixtures; no live account metadata retrieval was
performed for this report. The targeted command, run from the repository root,
passed:

```text
backend/.venv/Scripts/python.exe -m pytest backend/tests/test_grok_connection.py backend/tests/test_grok_worker_client.py -q
11 passed in 1.13s
```

Sources: [official Grok Build billing extension](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-shell/src/extensions/billing.rs),
including the `GetAutoTopupRuleResponse`/`AutoTopupRule` definitions, and the
[official pager credit-bar contract](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/views/credit_bar.rs),
including its known disabled state and resolved no-rule handling.

## Controlled check diagnostic

A single controlled check was run against the existing local Grok connection
with the selected model, the generic Quantix check instruction, fresh
included-only preflight and the real `SetupCheckMeter`. It contained no Tender
content and did not retry. The sanitized exception chain was:

```text
ModelWorkerFailure -> ExceptionGroup -> ExceptionGroup -> RuntimeUnavailable
```

The setup meter recorded no reservation and no provider usage fields, so the
failure occurred before a model round was reserved. The original capture did
not retain the leaf exception message, only its sanitized class chain, so no
more specific provider or CLI cause is claimed here. Before this repair, the
core MCP session boundary collapsed the nested, already-sanitized
`RuntimeUnavailable` into a generic connection message. `ai_worker_client.py`
now promotes only known sanitized runtime failures from nested MCP task groups;
unknown transport exceptions still use the generic boundary. No live check was
run after this change.

The native CLI probe also showed that `--agent-profile` is an `agent`-command
option and is rejected by the top-level headless invocation; the top-level
option is `--agent <NAME>`, where the official CLI accepts a profile path. The
headless builder now uses `--agent`, retaining the pinned CLI's explicit
`--no-leader` and `--no-auto-update` guards. The focused regression test
verifies that the headless command contains the supported profile option and
the two isolation/update guards, and does not include the rejected
`--agent-profile` spelling. The
[official headless guide](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/14-headless-mode.md)
documents the top-level headless flags; the installed 1.0.13 CLI `--help`
probe confirmed the profile option spelling.

An earlier inspection used a historical immutable component version and was
incorrectly treated as the active installation. The current active receipt
contains the corrected `--agent` spelling; no stale-component conclusion is
drawn from that earlier attempt.

The worker boundary also now promotes only already-sanitized
`RuntimeUnavailable`, `RuntimeConnectionFailure` or `InterruptedError` leaves
from nested execution `ExceptionGroup`s. Unknown worker exceptions remain the
generic component message, so provider text and private prompts do not cross
the process boundary.

## Current model identity finding

After the current worker was prepared, the single engineer-led check reached
the provider and completed three model rounds. Its sanitized meter record
reported requested model `grok-4.5` and provider usage identity
`grok-4.5-build`; the exact-model guard correctly rejected that unrecognized
identity. This is the concrete cause of the remaining setup failure. No raw
prompt, account identifier or credential was captured.

xAI's public model page identifies `grok-4.5` as the public model and lists
`grok-build-latest` among its aliases. The pinned Build client's provider
usage identity above is a separate, observed Build SKU. Quantix now accepts
only this explicit mapping for this exact public model and rejects other
reported identities; it does not infer a general `*-build` suffix rule.
Usage completeness uses the same one-identity mapping, so this accepted alias
cannot be marked incomplete merely because its ledger key differs from the
public request id; multiple or unlisted identities remain rejected.
