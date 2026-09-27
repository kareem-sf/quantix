# Progress

## 23 September 2026: rebuild started

- The previous Quantix codebase was removed on branch `rebuild`; it remains in git history on `main`.
- Uncommitted work from `workspace-overhaul` is archived on `archive/workspace-overhaul-2026-09-23`.
- The old data home was moved to `~/.quantix-old-2026-09-23`; the new app starts with an empty `~/.quantix`.
- The product is specified in [spec.md](spec.md) and [architecture.md](architecture.md).
- Interface direction: minimal and engineer-first. White, hairlines, one typeface (Geist), one warm accent that
  means "needs you", green for approved. A sidebar holds the tenders, the stages and the office. Screens: Overview,
  Decision, Office, Documents, Takeoff, Estimate, Subcontract, Submission, Settings and New tender. The mockups
  are on the private design canvas "Quantix Office Directions".

The design was approved.

## 23 September 2026: skeleton

- **Service:** FastAPI, SQLAlchemy and Alembic under `~/.quantix`, with a per-launch access token. `/health`, and
  creating, listing and opening tenders. Datetimes are stored as UTC.
- **Interface:** Vite, React and Tailwind with the approved tokens, Geist bundled locally, and API types generated
  from the service. The sidebar lists tenders; New tender and Overview work.
- **Desktop:** a thin Tauri 2 window. The Vite dev server starts the service and forwards `/api` with the token.
- **Checks:** 8 service tests, 7 UI tests, typecheck, Ruff and Clippy pass. CI also fails when the API types drift
  from the service. A browser run created a synthetic tender through the real service; that test data was then
  removed.
- **Toolchain:** TypeScript stays on 5.9, because `openapi-typescript` needs the compiler API that TypeScript 7 does
  not have yet.

## 23 September 2026: AI connections

- The rebuild became `main` (PR #157). Legacy folders, worktrees, branches and obsolete Dependabot PRs were removed.
- **Where keys live:** AI connections, their keys and model checks are in `~/.quantix/auth.json`. Office settings
  (the office mode, and which connection and model the office uses) are in `~/.quantix/settings.json`. The
  engineer chose plain files over the OS credential store.
- **Connections:** Anthropic, OpenAI, Google, xAI and an OpenAI-compatible address. A key is kept only after the
  service lists its models. A model check proves tool use: the model must hand back a one-off code through a tool
  call. The office may only use a model that passed its check.
- **Screen:** Settings has the office mode, the connections (add, choose a model, check, remove) and the office's AI.
- **Checks:** 18 service tests and 11 UI tests. In the browser, a fake key was sent to Anthropic's real API and
  came back as "The key was refused", with nothing stored.

## 23 September 2026: documents

- **Adding a package:** choose a folder or individual files. The service keeps copies under
  `~/.quantix/tenders/<id>/files`, named by their content hash, so a copy is never moved or overwritten. The same
  file again is ignored; a changed file replaces the older copy, which is kept and marked "replaced".
- **Reading, in the background:** PDF text page by page; Excel one page per sheet, with cell references; Word in
  pages of about 3,000 characters. DWG, DOC and XLS files are listed with a plain reason why they can't be read.
  Scanned pages are marked as having no text, and the office reads them from the page image; OCR was left out to
  keep things simple.
- **Arabic:** lines are rebuilt from where each character is drawn (PDFium's loose boxes), so words read right to
  left whatever order the PDF stores them in. English and number runs stay left to right, marks stay with their
  letter, and "%1" reads "1%". A synthetic PDF made with real text shaping is a test fixture: PDFium's raw text is
  backwards, Quantix's is correct.
- **Search:** SQLite full-text search over every page, ignoring Arabic vowel marks and letter variants. Snippets
  show the document's own words.
- **Screens:** Documents (folder groups, search, a page viewer with rendered PDF pages, Open original) and the
  Overview's package summary.
- **`QUANTIX_HOME`:** points the service at a scratch data folder, for testing without touching real data.
- **Checks:** 30 service tests and 15 UI tests. A browser run on a scratch data folder: 4 files read, an Arabic
  search found the right page, a PDF page rendered, and a sheet showed as text.

## 23 September 2026: the office

- **Records:** staff with generated personas, messages (the team room, plus each person's chat with the engineer),
  tasks and decisions. Everything shown in the office comes from these rows.
- **Runtime:** a background loop wakes people with something new and runs one turn each: a tool loop of at most 12
  model requests on the office's checked model.
  - The Manager follows the whole team room. Staff wake for their tasks, for messages that name them, and for the
    engineer's direct messages.
  - Each turn's context is rebuilt from the records, not kept as chat history, so tokens stay low.
  - Agents talk only through tools, so the team room shows only what they actually sent.
- **Tools:** list, search, read and view pages (page images for scans and drawings), post to the team, message
  the engineer, raise a concern, ask the engineer (2–4 options), complete a task. The Manager also has hire,
  assign_task and release. A mistaken argument goes back to the model as a correction instead of failing the turn.
- **The Tender Manager:** created for each tender with an AI-generated persona when the engineer first writes.
- **Safeguards:** Stop takes effect between steps. The office pauses after 40 turns without hearing from the
  engineer. An AI failure pauses it with a plain reason. A person who runs out of steps mid-task gets another turn.
- **Fully autonomous mode:** ask_engineer tells the agent to decide and explain its reasons in the team room.
- **Screens:**
  - Overview: "N decisions need you", the Manager's latest message, and a box to ask the Manager.
  - Office: the team room, direct chats, and profiles with background, working style, opinions and tasks.
  - Decision: the question, its options or your own words.
  - Sidebar: each person's face and what they are doing now.
- **Portraits** are drawn from each person's id, never from their name.
- **Checks:** 35 service tests, including a whole office conversation through the real runtime with a scripted
  model; the service suite passed 8 runs in a row. 20 UI tests pass. A browser run on a seeded scratch folder
  caught a crash the tests missed: an effect returned `scrollIntoView()`'s value, which this Chrome returns. It is
  fixed, and a plain error screen now replaces React Router's developer page.
- **Not yet done:** a live run with a real model. The engineer adds an AI key in Settings, and the office is then
  tried on a synthetic tender with a small budget.

## 23 September 2026: BOQ and tender facts

- **Proposing items:** staff enter the client's BOQ with `propose_boq_items`, up to 40 lines a call. Each line
  carries the page it is on and a quote. Quantix keeps a line only if the quote is on that page (ignoring Arabic
  marks and spacing; "..." may skip words), and the item number and quantity are in the quote. Duplicates are
  refused. Lines that fail come back to the model with the reason; the rest are saved.
- **Tender facts:** method of measurement, currency and VAT, each proposed with its source. A newer approved fact
  replaces the older one.
- **Gates:** "Needs you" counts what is proposed, live. The engineer approves everything waiting in one go, or item
  by item; a rejection with a reason goes back to whoever proposed it. Approvals are reported to the Manager. In
  fully autonomous mode records are saved as "office-approved".
- **Estimate screen:** the BOQ by section with statuses, Approve all, the tender facts, and an item panel with the
  source page link, the quote and who entered it. Arabic units are isolated so they display correctly.
- **Checks:** 41 service tests (3 runs in a row), 24 UI tests. A browser run on a scratch folder: proposals from the
  sample Excel BOQ showed on the Overview and the Estimate screen, and Approve all approved them.

## 23 September 2026: quantity takeoff

- **Scale:** set per sheet by calibrating on a dimension printed on the drawing. The dimension text must really be
  on the page, and it may be in metres or millimetres. Approving a new scale replaces the sheet's older ones, which
  also fixed a tie between two scales set within Windows' 15 ms clock tick.
- **Measurements:** stored as geometry in page points. A length, area or count, optionally times a height (length
  to m²) or a thickness (area to m³), and optionally linked to a BOQ item. Quantities are never stored: Quantix
  computes them from the points and the sheet's current scale whenever they are read, so a corrected scale updates
  the whole sheet. Counts are whole numbers.
- **Snapping:** Quantix reads the corners and line ends of the lines drawn in each PDF page, and points snap onto
  them: staff's points within 6 page points, the engineer's clicks within 10 screen pixels. The same clicks on a
  synthetic plan gave 806.7 m² before snapping and 800.020 m² after, which is exactly what the drawing holds.
- **Staff tools:**
  - `find_on_page` gives exact positions of printed text from the PDF.
  - `set_scale` and `measure` take view_page pixels, which Quantix snaps and converts.
  - `takeoff_summary` gives the comparison.
- **Comparison with the BOQ:** each measured item matches (within 2%), differs (with the percentage), has a
  different unit (Arabic units understood), or the BOQ has no quantity; measurements with no BOQ item show as
  missing from the BOQ.
- **Takeoff screen:** a sheet picker, the drawing with every mark (proposed in orange, approved in dark), Length,
  Area, Count and Scale tools, zoom, and a panel with each quantity, its BOQ result, approve or reject, and remove
  to redo.
- **Checks:** 47 service tests (the takeoff tests passed 12 runs in a row), 27 UI tests. A browser run on a
  synthetic plan set the scale and measured the slab through the real screen, with exact results. The layout was
  fixed for narrow windows.

## 23 September 2026: estimating

- **Rates:** staff price each BOQ item with `propose_rate`, either as a unit rate or as a first-principles build-up
  of labour, plant, material and subcontract lines per unit, with wastage. Every rate states its basis:
  - a quote: the document, page and quoted line are checked, and the rate must be in the quote;
  - the company library: the entry must exist;
  - the office's own estimate: its reasoning is required in the note.
  Estimates stay marked as estimates.
- **Arithmetic:** Quantix computes line costs, the rate, the amount (BOQ quantity × rate, to the cent), the net, then
  preliminaries, overheads and profit (each on the running subtotal), the adjustment, and VAT from the approved VAT
  fact.
- **Gates:** rates and markups wait for the engineer. Approving replaces an item's older rate. Approve all, and
  rejections go back to the estimator.
- **Company library:** rates kept across tenders. Approving a rate can save its resources, or the unit rate, to the
  library. The engineer can add or remove entries, but an entry a rate is based on is kept. Staff search it with
  `search_library`.
- **Screens:**
  - Estimate: rate and amount columns, the net, a status per row, and an item panel with the build-up arithmetic,
    the basis with its page link, and "Save to the company library".
  - Markups and summary: the price breakdown and markup approval.
  - Company library screen.
  - The panels become a drawer on windows narrower than 1280 px.
- **Checks:** 52 service tests (3 runs in a row) and 30 UI tests. A browser run on a scratch folder showed a seeded
  build-up of 293.55 + 114.00 + 63.00 = 470.55, an amount of 146,999.82, and a summary down to 237,131.82 including
  VAT, all checked by hand.

## 23 September 2026: subcontract and supplier quotes

- **Directory:** the firm's subcontractors and suppliers, kept across tenders. Staff search it and add companies
  (for example from the tender's approved vendor list); the engineer adds and removes them on the Directory screen.
  A company with an enquiry or quote on a tender is kept.
- **Packages:** staff group BOQ items into subcontract or supply packages with `create_package`.
- **Enquiries:** staff draft them with `draft_enquiry`. The engineer opens a draft in their own mail program (or
  copies it) and marks it sent. Quantix never sends mail.
- **Quotes:** `record_quote` keeps each quoted rate with its page and line, checked against the quote document. The
  rate must appear in the line. Exclusions are kept with their line and the office's estimate of what they add. A
  revised quote from the same company replaces the earlier one.
- **Levelling (computed, never stored):** each quote's amounts are the BOQ quantity × the quoted rate, to the cent.
  Gaps are filled with our own rate and flagged. Our rate is never one taken from the package's own quotes, even
  after one is chosen. Exclusions are added back, and quotes are ranked on the levelled total. A gap with no rate of
  ours leaves the total incomplete and unranked.
- **Choice:**
  - Staff recommend a quote with their reasons; the choice is the engineer's (a new Subcontract gate).
  - Choosing puts the quoted rates into the estimate as approved quote rates, each with its line as evidence.
  - The Manager is told, including any exclusions still to cover.
  - In Fully autonomous mode the office chooses itself, marked as office-approved.
- **Screens:**
  - Subcontract: package tabs, the levelling table with every rate one click from its page, the recommendation,
    enquiry drafts, and "Choose". The side panel stacks below the table on windows narrower than 1280 px.
  - A Directory screen.
  - A Subcontract line on the Overview.
- **Checks:** 57 service tests, including hand-checked levelling: Gulf 21,080.00 + 34,300.00 + 5,000.00 =
  60,380.00; Najd 20,460.00 + 37,240.00 plug = 57,700.00. Also 34 UI tests. A browser run on a scratch folder
  showed the table, opened the mail draft, marked it sent, and chose Najd; item 3.1 then priced at 20,460.00 in the
  estimate. That run found "Our rate" switching to the chosen quote's rate; this is fixed and covered by a test.


## 23 September 2026: submission

- **Checklist:** staff add what the tender requires the bidder to submit, with `add_requirements`. Each
  requirement cites the clause that requires it, and only the ones whose clause checks out are kept. The engineer
  can add their own.
- **States:** each requirement is ready, needs you (a draft waits for review) or missing.
- **Drafts:** staff draft submission documents with `draft_document`, leaving signatures and anything only the
  engineer can provide as blanks. Drafts wait for the engineer, a new Submission gate. Sending one back tells the
  author why. In Fully autonomous mode drafts are approved by the office and marked as not reviewed.
- **What the engineer provides:** they attach a file (a bond or a certificate) or mark the requirement ready.
- **Priced BOQ in the client's format:** staff read the client workbook's header and set the rate and amount
  columns with `set_pricing_columns`; the header must contain those columns. Quantix writes each item's rate and
  amount into a copy of the client's workbook, on the row its BOQ line was read from, keeping the client's own
  formulas. The supplied file is never changed. Items from other BOQ sources go in a "Priced BOQ.xlsx" in Quantix's
  layout.
- **Markups in the rates (the engineer's option, on by default):** every rate is lifted by total ÷ net, so the BOQ
  adds up to the tender total. The build reports any difference left by rounding each rate to the cent.
- **Build package:** always the engineer's. It writes the priced BOQ, the approved drafts as Word files, the
  attached files and a checklist workbook to `~/.quantix/exports/<tender> <date time>`. It lists anything not
  ready, and "Open folder" opens it. Nothing is sent to the client.
- **Screens:** Submission, following the approved mockup: the checklist by section with a filter, a requirement
  panel with its source, the draft, approve or send back, add a file or mark ready, and the build bar. Also a
  Submission line on the Overview.
- **Checks:** 62 service tests and 38 UI tests. The spread is checked by hand: net 120,952.80 plus 10%
  preliminaries = 133,048.08, factor 1.1, rates 20.35 and 3,836.80, and the BOQ adds up to 133,048.08. A browser
  run on a scratch folder approved a draft, marked the priced BOQ ready and built the package. The files on disk
  were checked: rates in E, amounts in F, the client's formula kept, a 3-row checklist and the Word draft.


## 23 September 2026: company rules and past tenders

- **Company rules:** the engineer writes the firm's rules by topic on the Company rules screen: standard markups,
  exclusions, qualifications, house style. Every staff member reads them in their briefing each turn, so every new
  team follows them.
- **Tender outcome:** open, submitted, won or lost, set from the Overview.
- **Past tenders:** staff look up the firm's approved rates for similar items on its other tenders with
  `search_past_tenders`. Each result carries the tender's outcome, the date and the basis. Only approved rates
  count, and staff are told to check that a rate is still current before relying on it.
- **Checks:** 65 service tests and 40 UI tests. In the browser, two rules showed by topic, and the outcome was set
  to Won on the Overview.


## 23 September 2026: acceptance, synthetic tender end to end

- **End-to-end test** (`service/tests/test_acceptance.py`): a synthetic tender goes through every gate on the real
  office runtime, with only the model scripted.
  - The engineer asks the team room to price the tender. The office appoints the Tender Manager, who hires an
    estimator with a generated profile and briefs her.
  - The estimator enters the BOQ and the tender's currency and VAT. The engineer approves them.
  - She prices two items and proposes markups. The engineer approves them.
  - She quotes and levels a waterproofing package: Gulf 34,300.00 plus a 2,000.00 exclusion = 36,300.00; Najd
    35,770.00, ranked first. The engineer chooses Najd.
  - She builds the submission checklist, drafts the method statement and reads the client BOQ's columns. The
    engineer approves the draft and marks the priced BOQ ready.
  - Every gate is then clear. The price is net 156,722.80 and total 172,395.08, with VAT of 25,859.26, all checked
    by hand. The built package's priced BOQ adds up to the same total, with rates 20.35, 3,836.80 and 40.15 in the
    client's column E, and nothing is left not ready.
- **Checks:** 66 service tests, 3 runs in a row.
- **Still to do for the MVP:**
  - The same run with a live AI connection.
  - The Arch-Civil Rev03 reference package in the desktop app.
  Both need an AI key added in Settings (stored in `~/.quantix/auth.json`); none is connected yet.


## 27 September 2026: the Tender Manager reviews everything

Running the real SEC 8485/8486 tender showed the Manager was never in the review path. Staff proposals went
straight to the engineer, and in Fully autonomous mode the person who made a record approved it themselves.

- **Review flow:**
  - Staff produce every record; the Manager has no production tools.
  - Each proposal starts `proposed`, in the Manager's review queue, and new work wakes him. He accepts it, saying
    what he checked, or sends it back, saying what to correct.
  - Accepted work waits at the engineer's gate as `reviewed`. In Fully autonomous mode it becomes
    `office_approved`.
  - Send-backs from the Manager and from the engineer go to the team room, naming whoever made the record. The
    engineer can reopen approved work with a reason.
  - Statuses and review fields are shared in `quantix/core/review.py`; the queue is in `quantix/review/records.py`.
- **On the screens:** "With the Manager for review", "Reviewed by <name>: <note>", Reopen on approved work, and
  the Overview says how much is with the Manager.
- **Fixed on the way:**
  - Choosing a subcontract quote failed when two bills share an item number.
  - The pricing count differed between the Overview and the Estimate.
- **Checks:** 104 service tests and 52 UI tests. The acceptance test now has the Manager review all 12 records
  before the engineer decides.

## 27 September 2026: Quantix checks the office's work

Each check comes from a mistake in the real run. Findings are computed from the records whenever they are asked
for, never stored, and each one names the pages or records it rests on.

- **Rates:**
  - The same item is priced differently elsewhere in the tender.
  - The rate is more than 30% from the firm's own rates for items like it: the library and past tenders, each
    listed with its source and date.
  - Wastage is outside 0–50%.
  - A library rate is in a different unit from the item (blocker).
- **Measurements:**
  - The same area is measured again for the same line on the same sheet (blocker).
  - Another measurement of the line is within 5% of this one.
  - The points aren't on the drawing's lines.
  - The takeoff differs from the BOQ.
- **Facts:** the value has numbers the quoted clause doesn't give.
- **Markups:** preliminaries are outside 5–15% of the net cost, or a time-related item doesn't match the work
  schedule's overall duration.
- **Drafts:**
  - Text is still to be filled in (blocker).
  - BOQ lines are left out of the work schedule.
  - The overall duration is shorter than the longest line (blocker), or longer than all the lines run one after
    another.
  - The work schedule now keeps its lines and its overall duration.
- **Quote recommendations:** the quote isn't the lowest levelled one, our own rates fill its gaps, or an exclusion
  has nothing allowed for it.
- **The Manager's review:**
  - His queue shows what the checks found, and `review_details` lists each finding with its source.
  - He can't accept a record with a blocker.
  - He accepts a warning only with his reason. The reasons are kept in `acceptances` (migration 0012).
- **On the screens:** "Quantix’s checks" appears on the Estimate item, the facts, the markups, the selected
  measurement, the submission draft and the subcontract recommendation. Each finding has its source link and the
  Manager's reason.
- **On the real tender:**
  - Migration 0012 is applied, and today's records come up clean.
  - Replaying the records sent back during the run, the checks would have stopped the site measured three times.
    They flag the points placed off the drawing, and takeoffs of +218.9% and +2,393% against the BOQ.
  - They would also have stopped three programme drafts that still said "to be inserted", "to be confirmed" or
    "[from engine output]".
- **Known cost:** the first check of a measurement on a dense sheet reads its vector geometry, about 4 seconds on
  the GLO layouts. The last four sheets stay cached.
- **Checks:** 110 service tests and 53 UI tests.

## 27 September 2026: escalations

What the office can't settle now reaches the engineer with its evidence, instead of going round in send-backs.

- **The Manager's `escalate` tool** turns a record in his queue into a decision for the engineer. The decision
  holds:
  - the problem;
  - where it shows: the record's own BOQ line and page, plus the document pages and BOQ lines he cites, each
    checked to exist;
  - his suggested corrections, 1 to 4, as the choices.
- **Rules:**
  - A record has one open escalation at most.
  - While the escalation is open, the Manager can't accept the record or send it back, and his queue count leaves
    it out.
  - Once the same work has been sent back twice, he can't send it back again until the engineer has answered an
    escalation about it.
  - The engineer's answer reaches his chat, and he applies it with his review.
- **On the decision page:** "Where it shows" links each page and BOQ line, Quantix's findings on the record
  appear, and the suggestions sit beside "Or answer in your own words".
- Migration 0013 adds `subject_kind`, `subject_id` and `sources` to decisions.

## 27 September 2026: the tender audit before release

- **What the audit checks** (`quantix/review/audit.py`):
  - work still with the Tender Manager or at the engineer's gates, and decisions waiting for an answer;
  - currency or VAT not recorded (blockers), and the method of measurement not recorded (a warning);
  - no markups, or BOQ lines with a quantity but no rate;
  - rows of the client's workbook that have a unit and a quantity but aren't in the BOQ. The quantity and unit
    columns are the ones the entered rows use.
  - documents still being read (a blocker), or that couldn't be read (a warning);
  - an empty or unready checklist;
  - every finding on work the Manager has already reviewed. The engineer's own approval settles a warning, never a
    blocker.
- **The Manager** runs `audit_tender` before he tells the engineer the tender is ready. He accepts warnings by their
  short name, with his reason.
- **The engineer** sees "Before release" on the Overview, including the warnings the Manager accepted and his reasons.
  The build button counts the audit's blockers, and the built package lists them as not ready.
- **Fixed on the way:** a draft without a work schedule stores JSON null, which SQL doesn't treat as NULL, so the
  markups check crashed on it. The schedule is now looked for in Python.
- **Not added:** a separate priced-total check. The build card already compares the priced BOQ with the summary
  total, and the missing-rows check covers the real risk of rows going back unpriced.
- **On the real tender:** the audit takes 0.6 seconds. It finds:
  - one unanswered decision, "Programme line reference correction";
  - the site's .kmz file, which Quantix can't read.

  All 26 workbook rows with quantities are in the BOQ: columns G (Rev.01 quantity) and E (unit) were found from
  the entered rows.

## 27 September 2026: the review running on the real tender

I answered the stale programme question as the engineer, and reopened the approved programme because it left
out Earthwork / C.2.7.2. The office then ran the new flow live:
- the Manager reviewed with `review_queue`, `review_details` and `review`, audited with `audit_tender`, and escalated
  twice with sources;
- the checks caught a redraft that dropped every 8486 line.

What it showed, and what changed:
- **Reopen** was offered only on office-approved work, so the engineer couldn't take back their own approval. It is
  now on every approved draft, rate, fact, scale and markups.
- **A redraft listed the 8485 lines twice** where the 8486 lines belonged. `draft_work_schedule` now refuses a line
  listed twice, and names the same number in the other bill.
- **The two-send-backs rule** counted rejections from before the engineer answered, so every redraft went back to
  the engineer. The count now starts from the engineer's latest answer on the same work.
- **A send-back gave the maker no task**, so he waited to be told. It now opens a "Redo …" task for them.
- **A staff member closed tasks claiming work he never filed.** `complete_task` now checks what they filed since the
  task began:
  - a "Redo …" task needs the corrected work filed;
  - any other task needs filed work, or a statement that it only asked for a report.

## 27 September 2026: the office keeps its place, and a record of every turn

From the article "Harness engineering" (the system around the model decides how reliable an agent is): store the
work's state outside the model and the process, and keep a trace of every run.

- **Every decision the engineer made is in each briefing.** It showed only the last 10 answers. The real tender has
  14, so the office had stopped seeing the BOQ section convention and that zero-quantity lines are priced
  rate-only.
- **Each turn is a row in `turns`**, written as it starts and filled in as it ends:
  - who took it, on which AI model;
  - each tool call, with Quantix's reason when it sent the call back;
  - the requests and tokens used;
  - how it ended.

  Migration 0014 adds the table.
- **The office resumes from those rows after a restart.** Before, the office's state lived only in memory:
  - A stopped office started working again after a restart, if anything was unread. The pause is now kept on the
    tender (`office_paused`), so it stays stopped until the engineer writes.
  - Someone cut off mid-work only woke if a new message named them. Now a turn that was cut short, or interrupted
    by Quantix closing, wakes the person again.
  - The 40-turn budget started over at every restart. It now counts the turns since the engineer last wrote.
- **Migration 0014 first failed on the real database.** It added `office_paused` with `batch_alter_table`. That
  rebuilds tenders: it copies the table, drops the old one and renames the copy. With foreign keys on, dropping
  tenders deletes every tender's records through the cascades.
  - SQLite refused on the real database ("database table is locked"), so nothing was lost. The service couldn't
    start until the two empty tables the failed run left behind were dropped.
  - On a test database, the same migration deleted the tender's messages.
  - 0014 now adds the column in place (`op.add_column`). A new test runs the newest migration over a database that
    already holds a tender and its records.
  - Rehearsed on a copy of the real database: the upgrade to 0014 kept every tender, message, BOQ line, decision,
    document and task.
  - 0009 rebuilt tenders the same way when it added `outcome`. It is applied already, and new migrations are
    covered by the test.
- **Checks:** 122 service tests. Each new test fails when the behaviour it covers is taken out.

## 27 September 2026: addenda carry the work over

From the article "300 agents, one graph, and a loop that edits the loop" (route work by its state, and re-check
only what changed). A newer copy of a file replaced the older one. The work citing the older copy stayed approved
and nothing flagged it, so a price could still rest on a superseded bill or drawing.

- **Unchanged work moves onto the newer copy.** As the newer copy is read, Quantix moves each piece of work whose
  source is unchanged:
  - the quoted words on the same page, or on the one page that has them;
  - a workbook row with the same cells, when an addendum added rows above it;
  - the drawing within 20 points of a measurement or a scale line.

  The document's note says how much moved.
- **Changed work holds the release.** Work left on an older copy is a blocker: the Manager can't accept it, and
  the audit lists it even when it is approved. A newer copy that leaves work behind wakes the Manager, and his
  briefing lists each piece and who did it.
- **Redoing it replaces the older work.**
  - A BOQ line is revised in place, so its rate and measurements stay with it. The Manager sees what the older
    copy said.
  - A checklist item takes the new clause and goes back to his review.
  - A scale or measurement on the newer copy replaces the older copy's.
  - A fact, or a rate priced from a quote, may replace an approved one that rests on an older copy.
- Migration 0015 adds `documents.read_at`.
- **Checks:** 3 new service tests: a revised bill with a row added above and a changed quantity, a drawing revised
  around one measurement, and revised conditions changing a fact and a checklist item. The service suite, Ruff and
  the format check pass for these files.

## 27 September 2026: lessons, the AI scorecard and an allowance per tender

This adds the three harness layers from "Harness engineering" that Quantix still lacked: feedback, observability
and an operational cap. Tool control, a separate checker and memory kept in the records were already in place. Model routing
stays out: the engineer chose one office AI for everyone.

On the real tender, more work was sent back than approved, and nothing of it outlived the tender:

| Record | Approved | Sent back |
| --- | --- | --- |
| Rates | 26 | 29 |
| Drafts | 9 | 19 |
| Markups | 1 | 7 |
| Scales | 2 | 6 |
| Measurements | 1 | 6 |

The firm's rules held 0 lessons from any of it.

- **Lessons.**
  - With a verdict on work that needed correcting (a send-back, or the accepted version of work sent back
    before), the Manager can give the lesson: one general rule.
  - Quantix refuses a lesson that is too short, one the office already follows, one already among the company
    rules, or one like a lesson the engineer dropped.
  - Every briefing on the tender lists the lessons, so someone hired later doesn't repeat the mistake.
  - The Overview shows them under "What the office learned". "Keep for later tenders" copies a lesson into the
    company rules, under a topic from its kind of work (Rates, Takeoff, BOQ…). "Drop" takes it out of the
    briefings.
  - Migration 0016 adds `lessons`.
- **How each AI has done.** Settings reads the turn records. For each model it shows turns finished, tool calls
  Quantix sent back, work accepted (records filed on its turns that the Manager or the engineer accepted, out of
  those accepted or sent back) and tokens per accepted record. A record belongs to its filer's latest turn that
  started before it. Records filed before turns were recorded aren't counted. `GET /ai/usage`.
- **An AI allowance per tender.**
  - `tender_allowance` in Settings, in millions of tokens (read and written), with no limit by default.
  - Before each turn, the office pauses on a tender that has used it and says so in the team room. Raising it and
    sending a message carries on.
  - Settings lists each tender's use against the allowance. The spec's "Later" now says money rather than tokens.
- **On the real tender:** the running app applied migration 0016. The one turn recorded so far, 27,955 tokens on
  openai-gpt-5-4-mini, shows in Settings.
- **Not done:** week-on-week drift for each model waits for enough turn records to compare.
- **Checks:**
  - 4 new service tests: a lesson followed by a later hire, the engineer keeping and dropping lessons, the scorecard,
    and the allowance pausing and resuming the office. 130 service tests pass.
  - 3 new interface tests. 60 interface tests pass, and so does the typecheck.
  - Three `test_office.py` tests sometimes time out in a full run, at HEAD too, on a `wait_for` between turns.

## 27 September 2026: one name per firm

From the same article (an alias table, checked before anything merges, so one entity never becomes two). The
directory matched a firm only by its exact name. A quote headed "ABC Contracting Co." for the directory's "ABC
Contracting" was refused with "Add it with add_company first", which is how one firm became two.

- **A firm's name is compared, not its spelling.** Quantix ignores case, punctuation, Arabic letter forms and legal
  forms (Co., LLC, W.L.L., Est., شركة, مؤسسة, ذ.م.م). Recording a quote, drafting an enquiry or recommending under
  another spelling finds the firm; a recorded quote's reply names it as the directory does. Adding it again is
  refused.
- **A name that may be a firm already there asks first.** When every word of the shorter name is in the other, as
  with "ABC Contracting" and "ABC General Contracting", staff add it only with `different_from` naming the firm.
  The engineer's Directory offers "Add as a different firm". A name the directory doesn't have suggests the firms
  it may mean.
- **The engineer merges a firm entered twice.** "Same firm as…" on a Directory row moves its enquiries and quotes
  to the chosen firm. That firm keeps the other name, shown as "Also …", so the name finds it from then on.
  Contacts it lacked are kept. Two quotes for one package can't be merged. Migration 0017 adds
  `companies.aliases`.
- **Checks:** 2 new service tests and 1 new interface test. The service suite, interface suite, typecheck, Ruff and
  the format check pass. On the running app, every read route answers both real tenders without a server error.
