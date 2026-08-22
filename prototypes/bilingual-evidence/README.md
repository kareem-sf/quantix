# Bilingual evidence-linked analysis prototype

This throwaway prototype answers one question:

> Can the proposed Quantix runtime reliably extract and cite Arabic and English requirements from the acceptance Tender, preserve original-language Evidence, distinguish assumptions, and produce a structured reviewed output?

## Review the measured result

Open `evidence-review.html` directly in a browser. It is a self-contained snapshot of the passing live run and needs no server or dependencies.

The walkthrough lets an Engineer User inspect:

- Arabic-over-English precedence and the FD-12 conflict;
- an addendum superseding the original deadline;
- a missing mandatory form becoming a draft external RFI;
- an absent crane capacity remaining a proposed, null assumption;
- an untrusted instruction being ignored;
- exact evidence validation, including a deliberate tamper test;
- the independent-review and EITL approval boundary.

## Reproduce the live probe

Prerequisites: Node.js and a locally authenticated Codex CLI.

```powershell
node .\prototypes\bilingual-evidence\probe.mjs
```

The probe uses the engineer's existing ChatGPT-authenticated Codex session. It starts one isolated Codex app-server, two independent analyst threads, and one reviewer thread. Unrelated configured MCP servers are disabled only for the child process. Generated workspaces and registered run artifacts are written below `run-output/`, then ignored by Git.

The host—not the model—computes source hashes, verifies locators and byte-exact excerpts, applies deterministic evidence precedence, and compares decision-bearing stable projections. The reviewer receives only the registered artifacts and validation results. Neither analyst nor reviewer can approve.

## Scope

The fixture is an original CC0 bilingual Markdown slice. This result does not test PDF parsing, OCR, spreadsheets, drawings, full-package scale, or the future complete acceptance Tender. See `RESULTS.md` for the measured conclusion and limits.
