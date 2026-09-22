# Quantix progress

Current record. Entries before 14 September 2026 (the adaptive office programme, audit repairs, GitHub integration and retrieval plan Tasks 1–5) are in git history; see `docs/progress.md` at commit `19ee3de`.

## Simplification and tendering team — 14 September 2026

An agent-architecture audit ([report](reports/2026-09-14-agent-architect-audit.md)) found the stack was too big for what it did: most code served approval layers and optional systems, and the delegated staff path had never run end to end. The engineer chose what to keep. Work was done on branch `cleanup/2026-09-14` in steps, each with its tests.

**Removed:** the dynamic-office delegation stack (grants, envelopes, route bindings, ownership leases, checkpoints, handoffs, office messages, notebooks, scheduler and graph), AI team approval, the no-tools routing classifier, the code sandbox and local code runtime, public research and working memory, reusable agent definitions, the benchmark suite and adoption gate, supplier mail sending/sync and watchers, push-to-talk voice, the manual measurement canvas and saved measurement records, the Copilot, Gemini CLI and Claude Code routes, 34 unused UI components and about 3,900 lines of unused CSS. About 144,000 lines were deleted and 9,000 added, including lock files and the tests of removed code.

**Kept AI routes:** OpenAI, Anthropic, Google, xAI and one OpenAI-compatible endpoint by API key; ChatGPT/Codex and Grok subscriptions through their official clients. Codex and Grok accounts need **Prepare** and **Check** again because the worker changed.

**Tendering team** ([design](design/tender-team-runtime.md)): the Manager hires staff with generated profiles, assigns work and gets results and questions back on its next turn, all inside one run and one allowance. Staff run in parallel on API accounts and one at a time on a subscription client. The engineer sees and steers everything in the Team tab.

**Manager harness:** every message goes straight to the Manager. The core prompt is 4,000 characters (was 10,600) and the answer schema 1,100 characters (was 15,000). Plans, takeoff lines, BOQ rows, quantities, rates, prices, quote drafts, requirements, map items, programmes and drafts are staged with `propose` and checked when called and again before saving. Leftover staff-scope filters were removed; staff read the whole tender. The Manager has 35 tools; the target of about 20 was not reached.

**Agentic takeoff:** staff record takeoff lines from the drawings. Quantix compares each with the BOQ and labels it matches (within 2%), differs, unit differs, BOQ has no quantity, missing from BOQ or not on drawings. The engineer accepts or rejects lines in Estimate → Takeoff, and can ask the team for a takeoff with one button.

**Verification on the branch:** full backend suite 755 passed, 1 skipped; `test_retrieval_api.py::test_strict_meaning_and_combined_still_refuse_unavailable_indexes` failed once under parallel load and passed alone (it races the background meaning indexer on this machine). Full UI suite 246 tests in 56 files passed; frontend typecheck and Ruff clean. An in-process smoke test called 49 GET routes on a seeded tender with no server errors.

**Not verified:** no live AI run of the team or the takeoff, no desktop app walk-through, and no check that a given vision model measures drawings accurately. Takeoff lines are proposals for the engineer to check.

## Audit of the workspace overhaul — 22 September 2026

An audit of branch `workspace-overhaul` ran the app against an isolated, synthetic workspace and fixed nine defects: the missing Activity technical log, "Group documents now" hanging without an AI, spreadsheet BOQ rows losing their item numbers, a stale confirmation warning, "excluding vat" wording, the side panel hiding the Manager on narrow windows, tabs overflowing at tablet width, and missing tenders retrying and polling forever. CI formatting was also repaired. Backend 845 passed, 1 skipped; UI 272 passed; typecheck, Ruff, Prettier, Clippy and the build pass. Follow-up the same day: Submission became one checklist with requirements, drafts and the package in sections below it; Activity rows gained a summary line (`GET /tenders/{id}/job-summaries`); a live run on a synthetic tender (USD 0.039, Runware deepseek-v4-flash) exercised naming, grouping, requirements, approval, planning, a hired QS and a choice card, and found that switching a tender's AI left the old account usable for staff, which is fixed. Backend 847 passed, 1 skipped; UI 275 passed. A second pass fixed the Team list and the Manager's voice, then completed the synthetic tender: rates approved, estimate complete at SAR 536,940.00 excluding VAT, priced client BOQ, requirements resolved and a local export approved (nothing sent), for USD 0.075 in total. It fixed six more defects: rates named by item number, a calculation argument ending the job, renames staling the estimate, openpyxl workbooks refused as protected, stale Manager status, and exceptions shown as plain ready. Backend 854 passed, 1 skipped; UI 276 passed. Details are in [the audit report](../audit-report.md).

## Open

- Live acceptance of a Manager run that hires staff and of a drawing takeoff on the reference package.
- Codex and Grok accounts need Prepare and Check after the worker change.
- Databases keep the tables of removed features; nothing reads or writes them.
- Native DWG interpretation, signed installers and supplier email integration remain unavailable.
