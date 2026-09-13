# Focused diagnostic and AI acceptance

9 September 2026. This records narrow checks authorized by the engineer. It is not full MVP verification.

This evidence predates the subsequent [requested storage reset](../unified-storage.md). It does not establish a connected account in a newly reset home; the source repair remains, but a fresh workspace requires explicit account setup.

## Reproduction

Computer Use inspected the already running native app. Grok was signed in, Grok 4.5 was selected, and included usage was available with paid extras disabled. The access check failed after three rounds. Field-only inspection of that generic session's local native event records showed three `search_tool` attempts, each denied. No Tender data was sent by the check.

The pinned client's permission and turn-loop source established two integration defects: bare `Read` also blocks pathless tool discovery; three rounds cannot complete the usual discovery/check/submit/final exchange. The [repair design](../grok-repair-2026-09-09.md) records the exact correction and source references.

## Focused synthetic checks

The primary agent ran:

```text
backend/.venv/Scripts/python.exe -m pytest backend/tests/test_grok_connection.py backend/tests/test_grok_worker_client.py -q
18 passed in 1.25s
```

These are the retained focused Grok tests plus bounded regression checks. They cover billing normalization, sanitized nested worker failures, known model identity, headless arguments, read-rule scope, consistent check allowance, protocol-specific check instructions and safe diagnostic terminal reasons. They do not test the entire backend or every provider.

Separate temporary-data probes exercised the real diagnostic writer and FastAPI application. They passed authentication/request-reference handling, allowed-origin CORS on unexpected errors, strict renderer payload rejection, a streaming payload size cap, generated OpenAPI references, safe route templates, exclusion of a synthetic private value and session token, live foreign-writer retention, the 60/minute event limit, signed exit codes and rotation. A separate Node writer probe passed safe error classification, private-field exclusion, signed exit code recording and nonfatal behavior for an unwritable destination.

The implementation agent also performed focused syntax/import and diagnostic probes. Those are not additional broad verification or additional live provider acceptance. Primary source review and independent Astra/xhigh reviews found and corrected bounded native, UI and diagnostic defects. Implementation used Luna/xhigh as required.

## Native activation and observation

`npm run bindings` successfully regenerated the frontend declarations from the current API. The normal source launcher compiled and opened the desktop in the development profile. It emitted unused-code warnings, including the existing AI-host Duration import and native logging items; this was not a lint run or a release build.

The first launch collided with an existing native instance. The new diagnostic events showed the duplicate launcher exiting and its newly owned backend stopping; the existing window consequently lost its service. The primary closed that window normally and reopened cleanly. This was an activation sequence problem, not a claimed data loss or model failure.

The real service subsequently reported diagnostic recording available at `C:\Users\kareem\.quantix\logs`. Computer Use observed the shorter Settings screen with AI accounts visible and named secondary disclosures. Existing Tender records and the unsent Manager draft remained visible. The Grok component was prepared through the native interface without repeating sign-in or changing the selected model/spending preference.

One five-second account-status request timed out during preparation. The operation subsequently progressed and completed. Source inspection found synchronous deep receipt hashing inside async preparation; this is a follow-up responsiveness issue, not evidence that the preparation failed. It is included in the project review as a remaining improvement.

## Final live result

The repaired generic Grok check passed at **2026-09-08 22:16:17 UTC** (9 September, 01:16 Cairo time). The native interface displayed **Ready** and **Access check passed**. The saved check remains bound to `grok-4.5` and the prepared software version; it is not a manually assigned ready flag.

The real ledger reported four model rounds, 21,785 input tokens, 357 output tokens and the recognized `grok-4.5-build` identity. Worker diagnostics recorded `end_turn`, exit code 0, validated submission received, and completion. The worker check operation lasted about 12.6 seconds; this excludes the preceding account/setup preparation. The core accepted the exactly-once tool invocation and unchanged structured check value.

The active component is `1.0.13-67f8c04cc138-d1bd610f38954835b0104e641e3bf5aa`. Its installed worker writes to the same explicit `~/.quantix/logs` destination as the service, with the shared service session and a distinct operation identifier. Subscription allowance only remained selected. No Tender analysis or new Tender data permission followed the check.

## Limits

No broad test suites, lint, typechecks, general browser QA, release builds or release packages were run. No supplier message, Tender analysis, paid-extras change or new Tender AI permission was authorized through this check. Native UI observations cover the repaired settings/setup interaction, not every workflow or accessibility case. Provider eligibility outside the tested account and support on macOS/Linux remain unverified.
