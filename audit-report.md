# Quantix audit — 22 September 2026

Branch `workspace-overhaul`, with its uncommitted work. The source of truth was `AGENTS.md`, `docs/spec.md`, `docs/progress.md` and the approved design `docs/design/workspace-overhaul.md`. No `design-research.md` exists; the overhaul design and its interview decisions play that role.

## What the project is

- **Stack:** a Tauri 2 desktop shell (Rust) over a local FastAPI service (Python 3.12, SQLite) with a React 19 + Vite + Tailwind 4 + shadcn/Base UI interface.
- **Platforms:** Windows first; macOS and Linux from source.
- **Deployment model:** a local desktop application. The service picks a free port and a fresh token on each start and publishes them in `~/.quantix/runtime/connection.json`. Nothing is hosted, so static hosting does not apply: every screen reads the local service.

## How it was tested

- **Executed:** the full backend, UI, type, lint, format, Clippy and build commands (results below). The real app ran through `npm run dev` in the Claude browser pane, pointed at a separate, empty workspace (`USERPROFILE` set to a scratch folder), so the engineer's real `~/.quantix` and tender were never touched. A synthetic package was generated for it: a two-page specification PDF, a four-row XLSX BOQ with formulas and a DOCX of tender conditions.
- **Inspection only:** focus rings (the browser tool's key presses do not reach the page, so keyboard focus could not be driven), the AI-driven flows (no AI account exists in the isolated workspace, and a live run costs money) and the Tauri-only features (native file pickers, drag and drop, Reset).

## Checklist and results

| Area | How | Result |
| --- | --- | --- |
| First run, empty state | Live | "Create your first tender" with one action. Pass. |
| New tender: empty, invalid and valid path | Live | Empty path keeps Create disabled. A missing path gives a plain error with a retry. The valid path imported 3 files, extracted 4 BOQ rows and flagged the formulas with no stored results. Pass. |
| Documents: groups, problems, viewer, search | Live | The PDF page renders; search finds the bid-bond clause first. **"Group documents now" without an AI hung on "Sorting…" forever. Fixed.** |
| Estimate: table, search, filters, row review | Live | **Item numbers were missing for every spreadsheet BOQ row. Fixed.** Search by text and by item number works; "No rows match." for no hits. |
| Confirm a row, price it, totals, reload | Live (synthetic data) | 1,240 m3 × 25.50 = 31,620.00, shown in the row and the header, and kept after a reload. **The "Confirm this inferred BOQ row" warning stayed after confirming, and the tax basis read "excluding vat". Both fixed.** Native validation blocks a rate with no conditions. |
| Estimate header actions without an AI | Live | Take off from drawings gives a plain "choose a Tender Manager connection" message. Pass. |
| Submission | Live | Empty state with one action. Pass (see the design differences below). |
| Right panel: Plan, Team, Documents, Activity | Live | Opens on Plan; every tab has a one-line empty state. **The Activity tab had no Technical log. Added.** |
| Settings: every section, preferences, backup | Live | Every section loads. The default currency and instructions persist after a reload. Create backup works and its ZIP downloads (HTTP 200, 64 KB). An unknown `?section=` falls back to AI accounts. Reset is refused in the browser, as designed. |
| Themes | Live | Light, Dark and System are radio items; Dark applies and is stored. |
| Responsive: 375, 768, 1024, desktop | Live, DOM measured | No page scrolls sideways at any width; the BOQ table scrolls inside its own frame on a phone. **A narrowed window opened on the side panel instead of the conversation, and at 768 px the Documents and Activity tabs were hidden. Both fixed.** The mobile navigation sheet is labelled, takes focus and closes on navigation. |
| Accessibility | Live DOM plus inspection | Form fields have labels, the sidebar marks the current page with `aria-current`, the dialogs are labelled and the icon-only buttons have names. Focus-visible styles come from the shadcn components (inspection only). |
| Routes: deep links, reload, back, unknown ids | Live | Record deep links (`?record=`, `?section=`) survive a reload; Back works; an unknown tender section falls back to the Manager. **An unknown tender id waited ~8 s through retries, then showed an item error with only "Try again", and polled every 2 s forever. Fixed.** |
| API | Live | All 53 GET routes that could be filled with real ids returned 200 with no server errors. Two returned 409 by design (page preview of XLSX and DOCX); 21 need records an AI run creates. Requests without a token or with a wrong one get 401. CORS allows only the app's own origins. |
| Secrets and private data | Scan | No API keys or private keys in tracked or untracked files; no databases or `connection.json` in the tree. |
| Console | Live | The only errors were from deliberate 404/409 tests and backend restarts during edits. |

## Defects fixed

Each fix has a regression test except 7, which was verified live because jsdom does not lay out container queries, and 10, which is formatting only.

1. **The Activity tab had no technical log** (design §2, Activity). The raw-event view (`ActivityRows`, `ActivityDetail`) was only reachable through `LiveRunStream`, which nothing could open any more. `LiveRunStream` and its test are deleted; the new `src/features/activity/TechnicalLog.tsx` puts a collapsed "Technical log" in each job, loaded only when opened. Test: `JobHistory.test.tsx`.
2. **"Group documents now" could hang forever.** Without an AI the analysis finished at once without grouping, but the page kept saying it was sorting. The page now follows the real run and says why it stopped (`DocumentGroups.tsx`), and `POST /analysis` refuses with the plain AI reason instead of starting a run that cannot group (`api.py`; `JobManager._ai_ready` became `ai_ready`). Tests: `DocumentGroups.test.tsx`, `test_project_identity.py`.
3. **Spreadsheet BOQ rows lost their item numbers.** The extractor found the Item column but never stored it, so the table showed "—" and the Manager's `inspect_estimate` lookup by item number could not match spreadsheet rows. Rows now keep `row_reference`, and "Refresh from documents" fills it in on rows saved before this fix (`estimates.py`). Test: `test_estimates.py`.
4. **A confirmed row kept its "Confirm this inferred BOQ row" warning** (`estimates.py`). Test: `test_estimates.py`.
5. **The tax basis read "excluding vat"** in the row review; it now reads "Excluding VAT" (`EstimateEditor.tsx`). Test: `EstimateEditor.test.tsx`.
6. **A narrowed window opened on the side panel, hiding the Tender Manager.** The open state saved at desktop width now reopens only when both fit, and starting closed on a narrow window no longer overwrites that choice (`workspace-state.ts`). Test: `RightWorkspace.test.tsx`.
7. **The panel tabs overflowed at tablet width.** The panel is now a size container; below 24 rem the tabs show their icons, and the names stay available to screen readers and as tooltips (`RightWorkspace.tsx`). Verified live at 768 px: all four tabs fit in the 205 px panel.
8. **A missing tender retried, polled forever and offered no way out.** Reads no longer retry 4xx responses except 408 and 429 (`shouldRetry` in `api.ts`, used by the query client in `main.tsx`). Polling stops after such an error, and a missing tender shows "This tender is not in Quantix any more" with "Back to tenders" (`TenderWorkspace.tsx`). Test: `api.test.ts`.
9. **The technical log labelled Quantix's own steps "Unknown actor"**; they now read "Quantix" (`run_activity_reader.py`). Test: `test_run_activity.py`.
10. **CI formatting failed:** 60 interface files failed Prettier and one backend file failed Ruff format. Both formatters were applied with no other changes.

## Follow-up: design decisions and a live AI run

The engineer asked for the two design gaps to be decided and for a live AI run.

### Submission is one checklist

The Checklist, Documents and Package tabs are gone. The checklist is the page and "Build submission package" is its one action; it stays disabled until every item is ready or has an exception. Below it, three sections open on demand: **Add or review requirements**, **Draft documents** and **Submission package**. Nothing was removed. Older `?view=documents|package` links open the matching section, and "Build submission package" opens the package section. Tests: `Outputs.test.tsx`.

### Activity rows carry a summary line

`GET /api/tenders/{id}/job-summaries` (`backend/quantix/job_summaries.py`) counts, per run, the facts each tool recorded when it finished: documents read (named, so they are not counted twice), searches, page views, staff hired, colleagues asked, drafts saved and proposals. The Activity panel fetches it once and merges the attempts of each job into one line, such as "Read 3 documents · searched 3 times · prepared 1 proposal". The contract is in `docs/contracts.md`, and the bindings were regenerated; they were already behind several routes on this branch. Tests: `test_job_summaries.py`, `JobHistory.test.tsx`.

### Live run

Run in the engineer's own workspace on a **new synthetic tender** built from the made-up package, capped at USD 1. The real Fire Station tender was not touched.

| Step | What happened | Cost |
| --- | --- | --- |
| Import | The AI named the project honestly ("project not named in any document"), gave the short name "Concrete Foundations Tender", found the due date "21 October 2026 at 12:00" and grouped the three files with English descriptions. | USD 0.0024 |
| Find the submission requirements | On **Cheaper Inference** the provider refused: the account has run out of credit. Quantix said so plainly and charged nothing. The tender was switched to the engineer's other checked account, **Runware** (deepseek-v4-flash), and **Continue** resumed the job. The Manager read all three files, quoted three requirements from Specification page 2, listed ten gaps and staged the requirements for approval. | USD 0.008 |
| Approve the requirements | One approved in the review drawer in the app, two through the same endpoint. The checklist moved each from "waiting for your approval" to "missing" (no document linked yet). | — |
| Plan and hire a QS | The Manager wrote a plan, hired "Dina Alotaibi, Quantity Surveyor" and did the check itself when her assignment failed (see defect 11). | USD 0.013 |
| Dina's check, after the fix | Dina ran on Runware and reported two matches and two items the specification does not cover; the Manager verified her result against the sources. | USD 0.012 |
| Choice card in the chat | Choosing "Approve baseline; I'll give the VAT rate before pricing" posted the answer; the Manager recorded it and held pricing for the VAT rate and the measurement method. | USD 0.004 |

**Total: USD 0.039.** In the app: the Plan tab showed the live plan with owners and states; Team showed "Tender Manager · Working" during the run; Activity showed the new summary lines, with the two attempts of the requirements job combined; the sidebar showed the short name, the due date and a waiting count of 7.

### Defect found by the live run and fixed

11. **Staff could be billed to an account the engineer had switched away from.** Choosing a new AI for a tender added it to the allowed accounts but never removed the old one, and `list_team` offers every allowed account's models. The Manager passed Cheaper Inference's model for Dina, so her work went to the out-of-credit account although the tender was on Runware. Choosing an AI now keeps only the chosen account plus any account a separate specialist, role or fallback route still uses, and drops extra-spending approvals of removed accounts (`ai_setup_routes.py`). Tests: `test_tender_ai_setup.py`; the earlier test that expected the old account to remain was updated. After the fix, Dina's assignment ran on Runware.

### Not a defect

- The "â€“" and "�" seen in terminal output came from the Windows console decoding. The API sends correct UTF-8 (checked byte by byte), and the app shows "Bid Bond – 1% of Tender Value" correctly.
- One of three approval calls made from the shell returned an empty body; a retry succeeded, and the service log shows no application error. It did not recur.

## Second follow-up: bugs fixed and the tender completed

### Design gaps closed

- **Team** now lists everyone in one list, as designed: the Tender Manager ("Ready for your next request", or its live action while working), then each colleague with their role and current action or last result. Past work stays collapsed. Test: `Team.test.tsx`.
- **The Manager speaks to you as "you".** The prompt had no rule and called you "the engineer" throughout, which the model copied. A rule now covers summaries, notes, questions and the brief. Live check: "Pricing inputs are now settled from you", "What is left is yours". Test: `test_message_wording.py`.

### Defects found while completing the tender, all fixed with tests

12. **Rates proposed by item number were refused in a loop.** Once spreadsheet rows kept their item numbers (fix 3), the model sent `item_id: "1.1"`. The check said "row 1.1 was not read", and repeating the same read hit the duplicate guard. `propose` now resolves an item number to the row id when exactly one row read in the job carries it (`proposal_tools.py`). Test: `test_office_business.py`.
13. **A wrong calculation argument ended the whole paid job.** `calculate_engineering` rejected `precision: "2"` with a non-recoverable error. It now reads "2" as two decimal places, and any other bad value goes back to the model to correct (`engineering_tools.py`). Test: `test_engineering_workers.py`.
14. **Renaming a tender made the estimate stale.** A rename bumped the tender's source revision, so the AI naming the project straight after import left "Refresh BOQ candidates after source changes" on a finished estimate. A name no longer bumps the revision (`repository.py`). Test: `test_estimates.py`.
15. **Workbooks written by openpyxl were refused as "protected".** openpyxl always writes an empty `<workbookProtection/>`, which Excel does not enforce. Only a set lock flag or password now counts as protection (`client_boq.py`). Test: `test_client_boq_protection.py`.
16. **The Manager reported stale progress.** Asked for a status, it said the rates were unsaved, the BOQ unpriced and the requirements awaiting review, although all were done. Its context held only BOQ row counts. It now gets `tender_records_now` (rows confirmed and priced, rate proposals waiting, totals, blocking reasons, requirement decisions, drafts and approved exports), with a rule to trust it over its brief and old messages (`office.py`). Live re-check: an accurate status. Test: `test_estimates.py`.
17. **Exceptions looked like finished items.** A bid bond handled outside Quantix showed plain "ready"; it now reads "ready · exception" and still counts as ready. Test: `SubmissionChecklist.test.tsx`.

Checked and not a defect: the empty replies to some shell API calls lined up with backend hot reloads after my edits, which happen only in development.

### The Concrete Foundations tender, completed

All decisions below are synthetic; this is a made-up tender.

| Step | Result |
| --- | --- |
| VAT and measurement basis | You set 15% VAT and all-in Riyadh rates for September 2026 (no measurement method is stated). |
| Rates | The Manager proposed 1.1 SAR 21.00/m³, 1.2 SAR 500.00/m³, 1.3 SAR 565.00/m³ and 1.4 SAR 4,500.00/t with build-ups; the arithmetic checks (for example 19.00 + 10% = 20.90, rounded to 21.00). All four were approved with source confirmation. |
| Estimate | **SAR 536,940.00 excluding VAT, SAR 617,481.00 including VAT, complete, no blockers**, matching an independent calculation line by line. |
| Priced client BOQ | Created from the client's own `BOQ.xlsx`: rates in column E, the client's `=Qty*Rate` formulas kept in F. Opened and checked. |
| Requirements | The priced BOQ is linked and satisfied. Bid bond and site visit certificate are exceptions ("issued by our bank / by the client at the visit, added to the envelope"). |
| Submission | The checklist read "3 of 3 ready". The package review listed every exception and scope limit; the local export was approved as `submission-855028e4….zip` (priced BOQ, its manifest and the submission manifest). **Nothing was sent to anyone.** |
| Status check | The Manager's summary matched the records exactly. |

**AI spend on this tender: USD 0.075** (Runware, deepseek-v4-flash).

## Command results

All of these were run after the last change, one after another.

| Command | Result |
| --- | --- |
| `npm run test:backend -- -n auto` | 854 passed, 1 skipped |
| `npm run test:ui` | 62 files, 276 tests passed |
| `npm run check:ui` (tsc) | Pass |
| `ruff check backend` | All checks passed |
| `ruff format --check backend` | 307 files already formatted |
| `prettier --check src scripts *.json *.ts` | Pass |
| `cargo clippy --all-targets -D warnings` | Pass |
| `npm run build` | Built; Vite's chunk-size warning remains |
| `vite preview` of `dist/` (first pass) | Loads with no console errors and waits on "Connecting to your Tender Office…", as expected outside the desktop window |

One regression from fix 8 was found and corrected: `Backups.test.tsx` expected a refused (409) read to be retried automatically. It now expects the error to show at once and the engineer's "Check restoration history again" to recover.

## Remaining limitations

- **Live AI coverage:** takeoff from drawings was not run (the synthetic package has no drawings), and the live run used the browser pane over the desktop app's own service, not clicks in the Tauri window.
- **Cheaper Inference is out of credit.** The Fire Station tender still uses it, so its next AI job will fail until credit is added or it is switched to Runware.
- **Desktop-only features** (native pickers, drag and drop, Reset and the window itself) were not driven; the browser pane served the same interface. Keyboard-only navigation could not be driven because the tool's key presses do not reach the page.
- **Bundle size:** the interface is one 2.0 MB script (580 KB gzipped). Vite warns about it. It loads from disk in the desktop app, so it was left as is.
- **Load-sensitive tests:** under full parallel load (both suites at once), two backend connection tests and the start of two UI test files timed out; all passed when run alone and in the sequential final run.
- **Release packaging** was not run, as `AGENTS.md` asks. Installers are unsigned and untested on a clean machine.
- `film/` and `skills-lock.json` are untracked and unrelated to this audit; they were left alone.
