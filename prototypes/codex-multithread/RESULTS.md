# Prototype result: subscription-authenticated multi-thread Tender Office

**Measured verdict:** PASS

**Date:** 2026-08-06

**Environment:** Windows, `codex-cli 0.146.1`, app-server account type
`chatgpt`, plan type `pro`

The probe used the engineer's existing local ChatGPT authentication. It did
not request, copy, store, or print an email, credential, or token.

## Evidence

| Criterion | Result | Observation |
|---|---|---|
| Subscription authentication | Pass | `account/read` reported a ChatGPT Pro account; rate limits were readable. |
| Role topology | Pass | One app-server connection created Tender Analyst, Estimator, and Independent Reviewer threads with distinct instructions and working directories. |
| Concurrent work | Pass | Analyst and Estimator turns started 19 ms apart and their active intervals overlapped for about 49 seconds. |
| Producer outputs | Pass | Both turns completed and the host validated and registered their final JSON artifacts with SHA-256 hashes. |
| Own-workspace access | Pass | All three roles wrote a marker inside their assigned workspace. |
| Cross-workspace isolation | Pass | Both producer attempts to write into the other role's workspace were blocked. |
| Controlled coordination | Pass | The host started the Independent Reviewer only after both producer turns completed and supplied their registered artifacts as exact inputs. |
| Pause | Pass | The host interrupted an active Analyst turn and received terminal status `interrupted`. |
| Resume | Pass | After stopping and replacing app-server, `thread/resume` restored the Analyst's role, prior-deliverable context, and access to its earlier working file. |
| Cleanup | Pass | All temporary prototype threads were archived. |

The registered artifacts contained a Tender Analyst finding, an independent
Estimator view, and Reviewer findings over the same synthetic Tender Package.
The Reviewer correctly kept its findings open and did not approve the work.

## Decision supported by this prototype

Quantix v0 can use one local, ChatGPT-authenticated Codex app-server process
over stdio, with one persistent Codex thread per Agent Profile. Quantix—not a
Codex thread—must own task coordination, output validation and registration,
artifact handoff, audit records, and every EITL approval.

Each role receives a network-disabled workspace-write sandbox scoped to its
working directory. Sandbox files remain disposable Working Artifacts. A role's
final structured message crosses a host-controlled output contract before
Quantix registers an Artifact Version and makes it available to another role.

The probe also exposed an important implementation constraint: generated shell
programs are not a reliable artifact protocol. One exploratory run produced a
quoting error and another hit a transient Windows sandbox process-start error.
The passing run used a fixed host-supplied workspace command and structured
agent output. Quantix should therefore expose narrow typed tools and apply a
conservative scheduler instead of depending on arbitrary agent-authored shell
scripts or assuming an account-wide concurrency guarantee.

## Boundary

This is evidence for the tested private, engineer-operated v0 environment. It
does not establish an OpenAI concurrency SLA, production support for the
experimental app-server protocol, or permission to resell ChatGPT subscription
usage. Public commercial distribution still requires explicit confirmation
from OpenAI.

## Reproduce

```powershell
node prototypes/codex-multithread/probe.mjs
```

The detailed sanitized result and registered artifacts are generated under the
ignored `run-output/` directory.
