# Subscription-authenticated Codex Tender Office probe

This throwaway prototype answers one decision question: can one locally
authenticated Codex app-server run several persistent, role-scoped Agent
Profile threads over the same Tender Package while the host controls
coordination, interruption, and resumption?

It is not Quantix product code. It deliberately uses only Node.js built-ins
and the installed `codex` CLI.

## What it proves

The probe:

1. verifies that app-server reports a ChatGPT-authenticated account without
   recording its email or credentials;
2. starts Tender Analyst and Estimator threads with different instructions
   and writable roots, then checks whether their active intervals overlap;
3. asks both roles to attempt a write into the other's workspace and records
   whether the sandbox blocks it;
4. starts an Independent Reviewer only after both producer turns finish;
5. interrupts a subsequent turn, restarts app-server, resumes the persisted
   Tender Analyst thread, and checks role/context continuity; and
6. archives all three temporary threads.

The synthetic Tender Package is intentionally tiny. Agent outputs and the
sanitized JSON result are written to ignored `run-output/`.

## Run

Prerequisites:

- Node.js 20 or newer;
- Codex CLI 0.146.1 or a deliberately revalidated later version; and
- `codex login status` reports `Logged in using ChatGPT`.

From the repository root:

```powershell
node prototypes/codex-multithread/probe.mjs
```

Exit code `0` means every measured criterion passed. Exit code `2` means the
probe completed but one or more decision criteria failed. Other non-zero exit
codes indicate a harness error.

## Interpretation boundary

A pass is evidence for a private, engineer-operated Quantix v0 on the tested
machine, account, CLI version, and date. It is not a concurrency SLA, a claim
that app-server is a stable production API, or permission to resell ChatGPT
subscription usage. OpenAI currently describes app-server as the integration
surface behind Codex clients and marks it experimental; public commercial
distribution still requires explicit confirmation from OpenAI.

Official references:

- [Codex app-server](https://developers.openai.com/codex/app-server/)
- [Codex authentication](https://developers.openai.com/codex/auth/)
