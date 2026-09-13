# Quantix diagnostics and repair design

9 September 2026. Authorized by the current request and AGENTS.md standing local implementation approval.

Storage update: the later [unified storage contract](unified-storage.md) supersedes this document's original independent-log-directory decision. Logs now belong to `<application root>/logs`, normally `~/.quantix/logs`, together with the rest of the managed data.

## Outcome

An engineer can understand what failed, take one useful next action, and give a support person an error reference. Quantix keeps the technical history automatically under `~/.quantix/logs`, independent of the Tender data directory. This does not move existing Tender records.

## Evidence motivating the change

The running Windows app completed Grok authentication and model discovery. A native Check access attempt failed with the broad message “Grok did not complete the request within its account, model or work limits.” The current worker disables logging; the host discards worker stderr; installers drain errors; the release shell discards sidecar events. Previous repairs exist in the working tree but do not establish successful current access.

## Chosen approach

Use standard library structured local logging with explicit safe fields. Keep the existing local service, isolated workers, SQLite business records, and React/Tauri UI. A cloud telemetry service would add an account and data transmission without helping this local failure. Capturing every raw SDK message would risk recording credentials and Tender content.

Each process writes its own JSONL file to avoid unsafe multi-process rotation. Rotate at 5 MiB with two backups; retain at most 14 days and approximately 100 MiB across completed log files, pruning only files owned by the diagnostic writer. Preserve live writers. A logging failure must not stop Tender work; expose unavailable status and issue one generic stderr warning.

Records contain UTC time, level, component, stable event, process/session identifier, request/operation/run identifier where available, duration, HTTP status, protocol/model/component version, safe phase and outcome. Exceptions retain bounded class chains and code frame names/line numbers, excluding messages, source lines, locals and arbitrary paths. Raw credentials, headers, query strings, request/response bodies, source names/content, instructions, auth URLs/codes, provider output, environment dumps and command arguments are excluded. Controlled application error messages remain visible to the engineer, but are not blindly copied into logs.

The service owns the canonical Python diagnostic writer. Component preparation copies that dependency-free writer into the isolated worker package and includes its bytes in the component fingerprint. The service passes an explicit log directory and correlation context so isolated worker HOME never changes the destination. Worker and provider boundaries log classifications and safe numeric metadata; raw third-party logging remains disabled.

## Service and UI contract

Authenticated `GET /api/diagnostics` returns the log directory, availability/detail, retention settings and session identifier. Authenticated `POST /api/diagnostics/events` accepts a strict bounded renderer event schema with enums and numeric/code-location metadata; never arbitrary error text or payloads. Successful response is a small acknowledgement. Invalid fields cannot echo submitted input. An authenticated diagnostic request is rate bounded; failures are best effort and cannot recursively report themselves.

API responses expose a generated `X-Quantix-Request-Id`; unexpected failures return a plain recovery message plus its reference, and are recorded with sanitized stack structure. Renderer API errors preserve the reference. Global renderer errors and unhandled promise rejections are captured as safe type/location metadata. A React error boundary provides a recovery action. Settings exposes Technical details on demand with the actual diagnostic folder and recording state. Simple setup failures keep specific safe messages and a useful retry action.

## Grok investigation and acceptance

Record terminal event type/subtype, process exit status, observed rounds, submission received flag and exact approved/reported model identity. Inspect the current pinned official client documentation and source before changing CLI behavior. Do not increase paid exposure, approve Tender policy, enable extra spending, loosen tool isolation or accept unknown model aliases to force a green check. Use only the built-in generic check, with fresh included-only entitlement checks and cancellation. Make each repair follow a reproduced or source-proven cause.

## Verification limits

Current user instruction defers broad suites, lint, typechecks and release builds. The current request permits targeted native UI investigation and AI setup/live checks. Source inspection, schema regeneration, focused synthetic diagnostic probes and generic live connection checks will be reported separately. No complete MVP verification or supported-provider matrix will be claimed from a single Grok check.

## Scope beyond this increment

Whole-project and UX findings are documented with priority and evidence. Large workflow redesigns, provider upgrades, OCR/CAD support and release work remain proposals unless needed to solve the current failure. Preserve all existing dirty work and customer records; no commits or release packages during this investigation.
