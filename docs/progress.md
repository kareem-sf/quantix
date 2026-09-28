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

## 27 September 2026: the office answers from its own records

The engineer asked Salem, the Tender Manager on the real tender, how the general backfill (C.1.2) was priced, five
times between 16:51 and 17:02. Each turn ended by his own choice after 4 to 8 of his 12 steps with "Next I will
verify…", and nothing ever did the next step. The approved build-up was in the database the whole time, but no tool
could open it: `review_details` found only records still in his queue, `estimate_summary` gave totals and `list_boq`
had no rates. Across the tender, 40 of his 108 messages to the engineer promised work, and in 24 of them the
engineer spoke next.

- **Opening settled work.** `open_record` opens any record by its reference or BOQ line, settled or not: a rate's
  build-up with Quantix's line costs and amount, a BOQ line's source, rate, measurements and package, a draft's
  text, a quote's lines, who made it, what the Manager and the engineer decided and why, and every other version of
  the same work with why it was sent back. It replaces `review_details`. `find_records` finds BOQ lines (with their
  rates), facts, checklist items, measurements, packages and quotes by words. `priced_boq` lists the priced BOQ 60
  lines at a time, with the total of the lines asked for.
- **An answer rests on what was opened.** A message to the engineer that replies to them needs `sources` (records,
  BOQ lines, pages or summaries the sender opened, checked against the new `opened` log) or `next_steps`. A reply
  with neither is sent back: answer now, or list the work. The chat shows the sources as links to the page or the
  BOQ line.
- **A promise is a task.** Each next step becomes the sender's own "Follow up" task, at most 3 open at once. An open
  task given, sent back or set since a person's last turn began now wakes them once. The Manager can complete his
  follow-ups.
- **The rules** say the office's own records, not the tender documents, hold what was entered, priced or decided,
  and that a question is answered in the same turn.
- **Smaller fixes.** `propose_boq_items` names the lines past 40 it didn't enter. `view_page` opens image documents
  (.png, .jpg, .tif), which no agent could read before.
- Migration 0018 adds `opened` and `messages.sources`. Rehearsed on a copy of the real database: every row count
  unchanged. On that copy `open_record("Earthwork / C.1.2")` gives the approved 36.95 per m³ build-up, its assumptions
  and the four earlier versions with the reasons two were sent back.
- **Checks:** 6 new service tests and 1 new interface test. 138 service tests pass, the interface suite and
  typecheck pass, Ruff and the format check pass.

## 27 September 2026: search by meaning

The spec asks for search "by exact words and by meaning"; only exact words were built. The engineer asked for
meaning search (after Agent Zero's memory) and live web research, in that order.

- **A small model on the engineer's computer.** multilingual-e5-small (int8 ONNX, 118 MB, and a 17 MB tokenizer),
  English and Arabic, run with onnxruntime on half the processor threads, which measured faster than all of them.
  The first start fetches it into `~/.quantix/models`, pinned to one revision and checked against SHA-256
  digests. Offline, search goes by words alone and Quantix tries again later. No text leaves the computer.
- **Pages are indexed in the background once read,** in passages of whole lines up to 800 characters. A passage
  with fewer than 10 words, or one that isn't mostly letters and digits (a PDF font Quantix can't map), is left out.
  Vectors are kept by the text's digest, so the same text is computed once: the two SEC substations share most of
  their specification, so 14,446 passages came to 9,498 to compute: about 20 minutes at the 8 passages a second
  measured on this laptop while other work ran.
- **Hybrid search.** `search_documents` and the Documents search fuse the word matches and the meaning matches
  (reciprocal rank fusion). A page counts as close in meaning when its best passage scores at least 0.05 above the
  tender's median passage for that query. The model scores everything fairly alike, so a fixed cut-off doesn't
  work: on 1,015 real passages, the right clause stood 0.05 to 0.11 above the median ("liquidated damages for
  delay", "retention money", "who provides site offices and storage"), and searches for things the tender doesn't
  have peaked at 0.036 to 0.048 ("termite treatment", "piling rig", "elevator maintenance").
- **The rate library, the directory and past tenders** list what has the words first, then entries close in meaning
  (a score of 0.84 or more, as a short name has no crowd to stand out from): "anti-termite" finds "Termite treatment
  below slab", "concrete supply" finds the ready-mix supplier. The tools tell staff to check each is the same work.
  One-word pairs such as "digging" and "Excavation" stay below the cut-off, and the words still find those.
- Migration 0019 adds `vectors` and `page_chunks`. Rehearsed on a copy of the real database: every other row count
  unchanged. A search over 18,000 indexed passages takes about 0.4 seconds.
- **Not done:** the benchmark check's "alike" items and the lessons' duplicate check still compare words.
- **Checks:** 3 new service tests (passages, hybrid search in English and Arabic, the library, directory and past
  tenders). 141 service tests and 62 interface tests pass; typecheck, Ruff and the format check pass. CI keeps the
  model in its cache.

## 27 September 2026: submission documents laid out properly

The built package's files were plain text in Word and Excel. The engineer asked for Anthropic's document skills.
Their license forbids copying them or deriving from them, so Quantix has its own engine on open libraries instead.

- **Company details** in Settings (name, address, CR and VAT numbers, logo) make up every document's letterhead.
- **Word and PDF** are drawn from one content model. Drafts are read as Markdown, so their headings, numbered and
  bulleted lists and tables come out as the real thing. Every document has:
  - a title block;
  - page numbers;
  - a header row that repeats on each page of a table;
  - Arabic set right to left, shaped in the PDFs.
- **The work programme** is a durations table built from Quantix's own figures, then the sequence and the overall
  duration.
- **The priced BOQ:**
  - it goes bill by bill in the client's row order, with subtotals and a summary that works VAT out on the total;
  - the workbook's amounts are `ROUND(quantity × rate, 2)` formulas, and Excel's total agrees with Quantix's to the
    cent (SAR 3,386,201.11 on the real tender);
  - both workbooks are laid out for printing.
- **One submission PDF** carries a cover, a contents page and every client document. The client queries sit under
  Correspondence, and the checklist and the new tender summary deck under Internal.
- **Drafts are checked** for notes to the office: sources, file names, "Tender Manager", "sent back". The drafting
  tools now ask for client-ready Markdown.
- **On the real tender**, the preview showed two approved drafts carrying notes to the office: the insurance
  statement's "Source:" line and the programme's sequence. The engineer can reopen them to have them redone.

## 27 September 2026: OCR, and reading the package beyond single pages

The real tender has 2,456 pages with no text of their own: the two scopes of work have 541 each (mostly English
data schedules), the specification 120, and a few drawings and site-visit pages. The office could read them only by
looking at the image, and not at all when its AI can't see images.

- **OCR on this computer.** RapidOCR 3.9 on ONNX Runtime reads each scanned page in the background once its
  document is read. On the real scans it read a data schedule at 97.8% confidence ("Suitable up to 2500", "XLPE",
  "(kVrms)") and an Arabic specification page at 93%, with some words better than the PDF's own text layer.
  Drawing sheets with small notes read at 75 to 85%: the scans themselves are poor, so pages are rendered at 200 dpi
  up to an A3 sheet. A document is read in Arabic when at least 30% of its own text is Arabic; the Arabic model reads
  the English on the page as well. The same file in two tenders is read once. Migration 0020 adds `pages.ocr` and
  `pages.ocr_score`.
- **`read_page`** says when a page was read by OCR and how sure it was, and to check figures that matter on the
  image. It can read up to 5 pages at once, and says when a scan hasn't been read yet.
- **`read_sheet`** gives a sheet's rows 80 at a time, or the rows with given words, as `read_page` shows them.
- **`compare_copies`** lists what changed between copies of a document, page by page and line by line.
- **The package map.** `describe_documents` (staff) sets what a document is and covers. The Documents screen already
  groups and describes documents by those fields, so the engineer sees the map as the office builds it.
  `list_documents` shows them, with scans read by OCR and older copies.
- **Coverage, kept apart.** `coverage` and the Documents screen show each document's scans still being read, the
  pages the office opened and the pages its work cites.
- Rehearsed on a copy of the real database (0019 to 0020, every row count unchanged). The OCR worker ran on the
  copy's real scans and the new words were found by search.
- **Checks:** 9 new service tests (5 OCR, 4 reading) and 1 interface test. OCR runs only in its own tests; elsewhere
  a blank page would keep it busy. 149 service tests and 63 interface tests pass, and so do the typecheck, Ruff and
  the format check.

## 27 September 2026: web research

The engineer asked for live market data: rates, suppliers, subcontractors, technical datasheets and outputs. They
chose two services with a fallback, and web research on by default in general words only.

- **Two tools for every person in the office.** `search_web` takes a few general words (at most 120 characters;
  the rules say never the client's or the project's name) and returns up to 8 results. `read_web_page` reads a
  page in parts of 8,000 characters. Only the search words and page addresses leave the computer.
- **Two services, one after the other.** Firecrawl answers first and needs no key: without one it gives a free
  daily allowance per computer. TinyFish takes over when Firecrawl can't answer (a limit, an error, no reply), once
  the engineer has added its free key. Settings has a "Web research" section for both keys; a key is kept only once
  its service accepts it. Keys live in `auth.json` under `web`.
- **Every page read is saved** in `web_pages`, as it was then. A page read in the past week is reused rather than
  read again. Its text comes to the office marked as information to check, not instructions.
- **A web price is evidence, checked like a quote.** `propose_rate` takes the basis "web" with the saved page's id
  and the quoted line: the quote must be on the saved page and a unit rate must be in the quote. The note must say
  how the market price becomes the rate. It goes through the Manager's review and the engineer's gate like any
  rate, and can't replace a rate the engineer approved. The Estimate shows "A market price from the web" with the
  quote and a link to the page, and the date it was read.
- **Also:** a message to the engineer can give a web page it read as a source, which opens the page; the directory
  keeps a firm's website, which `add_company` takes and the Directory links to. The rules and the evidence rule in
  AGENTS.md and the spec now include web pages.
- Migration 0021 adds `web_pages`, `rates.web_page_id` (without a database constraint: SQLite can only add one by
  rebuilding the table) and `companies.website`.
- **Live check:** through Quantix's own code and Firecrawl without a key, "ready mix concrete C35 price per cubic
  metre Riyadh" found a Riyadh supplier's 149 to 250 SAR range and its product page read as clean text; "bitumen
  waterproofing membrane supplier Riyadh" found Sika Saudi Arabia, Madar and Bitumat. TinyFish is checked only
  against its documented responses: it needs the engineer's free key.
- **Checks:** 3 new service tests (the fallback, a web price resting on the saved page, the keys) and 4 new interface
  tests (a web price, a web source in the chat, the keys, a firm's website). 156 service tests and 68 interface
  tests pass; typecheck, Ruff and the format check pass. Migrations 0020 and 0021 rehearsed on a copy of the real
  database: only `web_pages` is new, every other row count unchanged.
- **On the real tender, meaning search is live:** the running app fetched the model and indexed all 9,544 pages
  with text in both SEC tenders, 28,342 passages from 9,209 computed vectors. "who pays for water and electricity
  on site" finds the responsibility matrix and the purchase order's site storage clause; "retention money" finds
  the three retention clauses. "penalty for late completion" misses the liquidated damages clause, which
  "liquidated damages" finds.

## 27 September 2026: tools per person, the evidence rule, and figures on request

The last two parts of the office tools plan. With the new tools, staff would have seen about 55 tools at once and
the Manager about 45. Anthropic sees tool choice degrade past 30 to 50 tools, OpenAI advises under 20 and Google 10
to 20, and every tool's description costs tokens on every step.

- **Tools per person.** A small core for everyone, then one pack per kind of work: documents, boq, takeoff,
  pricing, subcontract, submission. Each pack carries its tools and a short method. `hire` now takes the kinds of
  work, and those packs are loaded every turn; the rest show as a name and a line until loaded with
  `load_capability`. Calling a hidden tool is sent back naming the pack to load. The Manager's packs only check
  work, and his hiring tools stay loaded while the team is small. An AI that can't see gets no image tools. A pricing
  estimator now sees 26 tools (40 before), the Manager 22. Built on pydantic-ai's on-demand capabilities, which work
  on every provider; tool search was left out because it is native only on Anthropic and OpenAI.
- **The evidence rule is enforced.** Filing BOQ lines, facts, scales, measurements, quoted rates, quotes, checklist
  items or pricing columns is sent back when a cited page wasn't opened by the person filing it.
- **Figures on request.** `calculate` (arithmetic with named values), `earthwork_volumes` (cut and fill from a grid
  of levels), `price_breakdown` (by bill, the lines that make most of the price, by kind of cost), `what_if` (the
  price with a change, nothing saved), `check_rate` (library with age, earlier tenders, similar lines, quotes).
- **More to look up.** `takeoff_summary` lists lines nobody has measured and one line's measurements in detail;
  `list_boq` pages and filters by bill; `estimate_summary` shows the markups; `search_past_tenders` shows the
  build-up and its note; `list_packages`; `search_conversation`; `what_changed`.
- **More to do.** `precheck` runs Quantix's checks on your own work before the Manager sees it. `withdraw` takes back
  any undecided work of your own (it replaces `withdraw_boq_items`). `apply_buildup` prices the same work on other
  lines from an approved build-up. The Manager's `suggest_library` asks the engineer; "Keep it in the library" saves
  the rate.
- **Instructions:** never work out a figure in your head; hire people with the kinds of work they will do.
- **Not built:** a model per person (`hire` choosing another AI). The engineer chose one office AI for everyone, and
  which models to offer is theirs to decide. Pages cited on work already filed are not checked after the fact.
- **Checks:** 12 new service tests (8 figures and look-ups, 4 packs). Scripted staff in five tests now read a page
  before citing it. 168 service tests and 68 interface tests pass, and so do the typecheck, Ruff and the format check.

## 27 September 2026: the office's answers checked with its real AI

`service/evals/office_answers.py` (pydantic-evals, a dev dependency) asks the office's own AI, as chosen in Settings,
three questions on a synthetic tender in a scratch data home: how each item was priced, the source of the slab
reinforcement rate, and its pricing in detail. The rate was sent back once and then approved. Each answer is scored:
the Manager wrote back, it rests on the rate's record, it promised nothing bare, and it came in the turn that read the
question. Runs used openai-gpt-5-4-mini, the office AI on the real tender.

- **First run:** every question was answered in the first turn from the records, with sources. But one answer was
  replaced by a later update, and next steps added to complete answers woke the Manager again and again: 33 to 36
  model requests and about 190,000 tokens a question.
- **Fixed:** an answer with sources is never replaced by a later update; next steps are only for a question the
  engineer asked that can't be answered yet, one set per question, after which the reply must be the answer with
  sources; the Manager's duties and briefing put the engineer's question before his review queue (they said the
  opposite of the rules). The turn allowance line no longer invites next steps.
- **After the fixes**, over the last two runs: 4 of 6 answers rested on the rate record, in the first turn, e.g.
  "BOQ item 4.3 is 28.1 t at 3,479.00 per t… The only earlier issue was the missing fixing labour and tie wire",
  sourced to the rate and the BOQ line. In the other 2 the Manager answered with real sources but about his review
  queue or the audit instead of the question: Quantix checks that a reply rests on something opened, not that it is
  on topic. Before these changes, on the real tender, 0 of 5.
- A question sometimes still leads the office on to other open work on the tender (up to 500,000 tokens over 10
  turns on an unfinished synthetic tender), which is the office working, not the answer.
- **Test fixes:** two `test_office.py` tests waited for "idle", which the office shows for a moment between a turn
  cut short and the one carrying it on; they now wait for the message they check.
- **Checks:** 2 new service tests. 170 service tests pass, and so do Ruff and the format check.

## 28 September 2026: redrafting what the client reads, and no silent stops

Nine drafts approved earlier on the real tender still carried notes to the office: file names, "Source:" and "Basis:"
lines, instructions to the team. The check for such notes came after their approval, and the audit lists only
blockers for approved work, so they never showed. The engineer reopened all nine. Redoing them showed where the office
still lost work or stopped:

- **Redrafts started from nothing.** A checklist item whose draft was sent back read "nothing drafted or attached
  yet", so staff wrote from scratch and dropped the substance: the insurance limits, the QA/QC obligations, and a
  guarantee statement that promised the guarantee the price leaves out. The checklist item now names the sent-back
  draft and why it went back, and `find_records` finds drafts by their own titles, which can differ from their item's.
- **Silent stops.** Someone who ended a turn with open tasks, having filed nothing and told no one, was never woken
  again, and no one was told. Quantix now notes it in the team room, which wakes the Manager to find out why. Its
  notices never wake the staff they name.
- **A refused update.** Five summaries the office can call (`what_changed`, `price_breakdown`, `what_if`, `coverage`,
  `list_packages`) couldn't be given as sources, so the Manager's update to the engineer was refused and he stopped.
  A test now checks every summary a tool records.
- **Released staff's tasks** stayed in the Manager's list for good. Release now names what is left undone, the list
  shows only the active team's tasks, and the person panel shows them as "not done".
- **Duplicate work.** The Manager assigned tasks his own send-backs had already made, so drafts were filed twice and
  versions he had accepted were replaced. The review tool now says a send-back is the maker's task to redo.
- **On the real tender**, all nine were redrafted and approved: the programme, five statements, the guarantee
  position and the two client queries. Each was checked against the annexures' own words: the HSE annexure is (C),
  not F, which is Delegation; audit findings are addressed in 48 hours and closed within a week; the manpower and
  material lists are due immediately after signing. The rebuilt package has no notes to the office in any document,
  every Word, Excel and PowerPoint file opens without repair and with no formula errors, and the audit is clear but
  for the unread KMZ the Manager accepted.
- **Seen, not changed:** an answer from the engineer that arrives during the Manager's turn is read on his next one.
  Once he then accepted a stripped query that the answer had told him to keep whole; the engineer sent it back.
- **Checks:** 3 new service tests and 1 interface test. 175 service tests and 69 interface tests pass, and so do the
  typecheck, Ruff and the format check.

## 28 September 2026: the due date

The engineer asked Salem to set the tender's due date. He replied that it was set, but nothing in Quantix could
change a due date after the tender was created: no control on the Overview, no API, no office tool. His reply got
past the answer check by citing an unrelated draft, since Quantix checks that a source was opened, not that it backs
the claim.

- **The engineer** sets or changes the due date where it is shown, on the Overview.
- **The Tender Manager** can set it with `set_due_date`, as the engineer tells him or as a page he read states it.
  Quantix notes the change in the team room.
- **The rules** now say that when no tool can do what is asked, the office says so plainly, never that it is done.
- **Checks:** 2 new service tests and 1 interface test.
- **Where the date comes from** is kept with it (migration 0022, columns added in place): the engineer's own date,
  entered by them or by the Manager on their word, or a tender document's, with the page and the words that state
  it, checked to be on that page. The Overview shows it on hover or focus of the mark beside the date: "Set by you,
  not from the tender documents", or the quote with a link to its page; the sidebar date has the same title. The
  Manager's duties are to find the deadline in the documents and set it quoting the page, or to tell the engineer
  none states it. A date the engineer gave stands: a document date can't replace it, and the Manager tells the
  engineer instead. His briefing says which kind of date it is, so the office never passes the engineer's date off
  as the tender's. Dates entered before this are recorded as the engineer's, with no time.

## 28 September 2026: what the demo of every upgrade found

Going through each screen on the real tender and asking the office live found five faults, now fixed:

- **Arabic lam-alef read backwards.** The specification stores each lam-alef ligature as its two letters in one box,
  alef first, so "خلال" read "خالل" and "الأعمال" read "األعمال"; the rebuild had lost the old fix. A box holding
  exactly a lam and an alef now reads lam first; a word whose letters all share one box keeps its stored order.
  502 of the tender's 4,284 Arabic text pages were read again (a same-letter reorder in every case) and their
  meaning passages rebuilt.
- **Characters beyond U+FFFF** came from PDFium as two halves on Arabic pages, which couldn't be saved: a new file
  with them would have shown as unreadable. The halves are joined.
- **Markups priced for longer than the programme stayed hidden.** They were approved before the programme was
  settled at 72 working days, and the audit showed only blockers for approved work. A warning comparing work with
  something settled after its approval now reaches the audit, and the time-related items are named in one finding.
- **Premium rates showed as 0.00.** The price summary rounded 0.003 to "0.00"; rates show every decimal they have.
- **A refused answer didn't say what to cite.** Salem opened the rate, answered without sources three times and
  stopped; the refusal now lists what he opened since the question, written as sources are.

Also dropped, as the engineer: two lessons that had led to wrong redrafts.
- **Checks:** 5 new service tests and 1 interface test. 182 service tests and 72 interface tests pass.

## 28 September 2026: problems in approved work reach the engineer through the Manager

The demo found the markups priced for 4 months against a 72-day programme approved after them. Salem never raised
it: the audit hid it, nothing woke him for it, and approved work was not his to change or to escalate.

- **He is told.** Quantix lists the problems it finds in work the engineer approved that nobody has put to them yet
  (`audit.approved_problems`), and wakes the Manager once for each he hasn't seen; his briefing names each with the
  reference to escalate it by.
- **He brings it with options.** `escalate` now takes approved work: the problem, where it shows, and 1 to 4
  corrections with his recommendation first. Quantix adds "Keep it as approved".
- **The engineer's answer acts.** A correction they choose reopens the work, with the answer as the instruction for
  whoever made it; keeping it changes nothing. Once put to them, the problem isn't raised again.
- **Checks:** 3 new service tests. 185 service tests and 72 interface tests pass.
- **Live, on the real tender:** the first run, Salem accepted the warning himself; then he named the markups as a
  BOQ line and was refused, twice; then the briefing ended "Nothing new" and he wrote that there was nothing to
  clear. Each was fixed (the Manager can't accept a warning on approved work; approved work needs no page or BOQ
  line, and naming the work itself is let through; what woke him is under "New for you"). On the fourth run he
  escalated the markups unprompted, in one turn: reduce the site staff to the 72-day programme (recommended), extend
  the programme to 104 days, or keep them. A "keep" of his own would have reopened the work to be kept, so Quantix
  refuses one and adds its own; the decision page marks an escalation's first option "Recommended".

## 28 September 2026: CAD drawings, takeoff from their objects, checks and tender queries

The engineer asked for DWG reading to come into the plan, using OpenCADStudio's libraries and ideas without the
application itself (GPL-3.0), so staff can read drawings, take off from them, verify the BOQ, answer questions, and
find missing items and conflicts.

- **The reader** (`cad/`, `qx-dwg`) on opencadcodec and opencadkernel (MPL-2.0), run as its own process. On the 19
  as-built drawings of the GAC showroom (AutoCAD 2010, 31,000 to 187,000 objects each) every file read, in 0.2 to
  1.2 s, into folders of 0.1 to 36 MB. The first probe's room names ("CLINIC ROOM 1") came from block definitions
  no plan uses: placing only what is drawn is what makes counts right. A file the failsafe reader returns empty is
  unreadable ("This drawing can't be opened").
- **Pages, units and takeoff.** Model space and each layout are pages of their words. Units are a scale on page 1
  the engineer approves; the office may only set the units the header states. Measurements are rules resolved to
  object keys when filed, computed from exact lengths and areas each time they are read. A rule that finds nothing
  for a BOQ line gives "Not on the drawings", the result the spec promised.
- **Layer map, rooms, checks, queries.** A layer map from a closed list of meanings; rooms from outlines, or from
  walls with each door's closing radius; drawing, BOQ and grid checks; tender queries with checked sources;
  contract type and order of precedence as facts. On the real ground floor plan, with a map of its plaster,
  column, glazing and door layers, Quantix found the washroom (3.83 m²) and the car elevator (18.31 m²), and no
  room where the showroom front is open. The checks there found 578 plaster lines drawn twice over others, 200 3D
  solids and 424 undecoded records not read, 5 clipped blocks, and drawn floor finishes, columns, sanitary fittings
  and glazing that nothing measured yet. On the 140,000-object ceiling plan they ran in about 2 s.
- **Office tools** in a `drawings` pack: `drawing_overview`, `query_drawing`, `view_drawing`, `find_problems`,
  `set_drawing_units`, `measure_drawing`, `propose_layer_map`, `raise_query`. Only `view_drawing` needs an AI that
  reads images, so an office AI that can't still takes off from CAD drawings.
- **Screens.** The Takeoff screen draws a CAD drawing with WebGL: the engineer sets its units, clicks objects to
  choose them, sees Quantix's count, length and area, and saves the measurement; layers can be hidden. A new
  Queries screen lists tender queries and layer maps for approval, and what the checks find. Checked in the browser
  on the real flooring plan in a scratch data folder: the units approved, a column picked and measured. That run
  found the units not refreshing on the screen after they were set, now fixed.
- **Migration 0023** adds `measurements.entities` and `rule` in place, and the `layer_maps` and `tender_queries`
  tables.
- **Left for later:** volumes of 3D solids, drawings a drawing refers to (xrefs), clipped blocks (placed whole, and
  flagged), and symbols drawn as loose lines instead of blocks.
- **Checks:** 13 new service tests with synthetic drawings written by `qx-dwg sample` (a two-room flat with a door,
  windows, a hatch with an island, a hand-written dimension and a pipe through a fire-rated wall), 4 Rust tests and
  6 new interface tests.

## 28 September 2026: clipped blocks, xrefs, loose symbols and 3D solids

The engineer asked for the whole of the DWG work in one go, so what the first round left for later is done.

- **Clipped blocks.** A block reference clipped with XCLIP shows only what lies inside its clip boundary: lines are
  cut where they leave it, a closed shape keeps the area and the part of its outline inside it, and what lies wholly
  outside is left out. The real flooring plans and washroom elevations have 33 clipped references between them. On
  the ground floor plan the five clipped expansion-joint arrays went from 1,104 objects to 1,016 and now stop at the
  building edge. On the washroom elevations the 120 × 60 cm tile grid stops at the wall face instead of running
  past it and above the ceiling line.
- **Xrefs.** A drawing that refers to another is given it from the package, matched by file name, and qx-dwg places
  it where it is referred to, with its layers named `XREF|LAYER`. The warning that it is missing then goes, and a
  newer copy of either file reads it again. None of the real drawings uses one, so this is tested with synthetic
  drawings only.
- **Loose symbols.** A new check finds copies of counted blocks (doors, windows, columns, sanitary fittings,
  fixtures) drawn as loose lines, by the types and lengths of their parts and their size. On the real elevations
  drawing it found two WCs and two washbasins drawn as lines in the washroom plan.
- **3D solids and regions.** They are lifted into opencadkernel's B-rep and meshed. Solids take off by volume, in
  m3, or by weight with a density. On the real sewage drawing the two steel ladders read as 162 solids (R20 rungs,
  UNP100 channels, plates), and the flooring plan's 200 plaster regions now have areas.
- **Checks:** 4 new service tests, 3 new Rust tests and 1 new interface test; 202 service tests, 79 interface
  tests, typecheck, ruff and clippy pass.

## 28 September 2026: an unanswered question after an AI outage, and each turn's thinking in the chat

The engineer asked Salem "is the markup a % or static number or both or what?" and got no answer, while the rail
kept saying he was "Checking the estimate". His turn ran for six and a half minutes, then the AI service stopped
answering ("Couldn't reach the service"). The office tried twice more within 15 seconds and paused. The pause notice
went to the team room, not to the chat the engineer was reading. His "now" line was never cleared, and his question
was already marked as read, so it would not have come back as new.

- **Retries.** A passing AI failure is now retried four times, waiting 5, 15, 45 and 135 seconds (over three minutes
  in all). The wait ends at once if the engineer presses Stop or Quantix closes.
- **The question comes back.** If the office gives up, what was new for the people whose turns failed is unread
  again. The engineer's question is then answered when the office carries on.
- **Why the office stopped.** `GET /office` gives the pause notice, and the chat shows it above the message box. When
  the office isn't working, nobody's "now" line is shown.
- **Each turn in the chat.** Every turn keeps its steps (migration 0024, `turns.steps`), saved while it runs. That is
  what the person was told, their thinking when the AI service returns it, their working notes, and each tool call
  with what it sent, what Quantix answered or why it sent it back. The chat folds each turn to one line, such as
  "Salem worked for 32 s", "Salem is working · Reading Conditions.pdf, page 3", or in orange "Salem stopped after
  6 min 40 s: Couldn't reach the service…". Opened, the line shows the thinking, notes and steps in words. The
  "Technical details" switch adds the tool names, the arguments, Quantix's answers and the start-of-turn briefing.
  No model, provider or token figures are shown. A direct chat shows that person's turns; the team room shows
  everyone's.
- **Working notes.** Some services keep a model's reasoning private (Runware's gpt-5-4-mini returns none). So the
  rules now ask each person to write a line or two before their tool calls: what they are about to do and why.
- **Checks:** 3 new service tests and 2 new interface tests; 205 service tests, 81 interface tests, typecheck and ruff
  pass. Migration 0024 was rehearsed on a copy of the real database: every table kept its rows.
- **Markups can be found.** Checked live in the browser: Salem answered the markup question again, and his turn log
  showed four searches for "markup" and "site support allowance" that found nothing. The record search covered every
  record except the markups, and he never opened them directly. `find_records` now finds the markups by their
  preliminaries' names, their note and the words markup, preliminaries, overheads, profit and adjustment. The line it
  returns says how each part is priced: preliminaries item by item, overheads and profit as percentages of cost, and
  the adjustment as a lump sum.
- **Sent-back markups can be opened.** Asked again after that fix, Salem still found nothing: every set of markups on
  the real tender had been sent back, and the office saw only live records, so its briefing said "none proposed yet".
  Sent-back drafts were already shown for correcting. Now the newest sent-back set of markups is too:
  `find_records` and `open_record("markups")` return it marked "sent back", and "where the tender stands" says the
  last set was sent back and none has been proposed since.
- **A pasted search line counts as its reference.** Asked a third time, Salem found the sent-back markups and wrote
  the right answer ("It is both…"), but twice gave the whole `find_records` line as his source. Quantix refused it
  both times, so the answer never reached the engineer. A source, or anything else opened by reference, is now read
  by the reference its line starts with ("markups · …" is "markups"). It must still have been opened.
- **Sources written in the text.** Asked a fourth time, Salem opened the markups and wrote the answer, but ended it
  with a "Sources: markups, estimate_summary" line instead of giving them. He was refused and stopped. A message with
  no sources now takes a last "Sources:" line as its sources (a page's own comma, as in "Bill.xlsx, page 2", is
  kept), and the checks on them are unchanged.
- **The markups sent back last.** The sent-back set shown to the office was the newest made: a duplicate turned down
  on 27 September. The set the engineer reopened today (approved on 27 September; "Reduce the site-support allowance
  to match the 72-working-day programme") is older. The office now sees the set sent back most recently.
- **Reopened work always has someone to redo it.** After Salem answered, the office went idle with no markups at
  all. The engineer had reopened the approved set at 12:11. A send-back becomes a "Redo …" task for whoever made the
  record, but Salem made those markups before the Manager stopped producing work, and the Manager gets no redo
  tasks. So the redo was no one's. Now, when the maker is the Manager or has been released, the Manager gets
  "Give out: Redo …" with the reason, to hand to someone with assign_task. He can complete it only once someone
  else has a task from after it.
- **Salem's work under Salem, and a reply to the engineer (evening).** The engineer saw Salem's turns under their
  own message, and no reply to "give the redo to someone". His turn logs showed four causes, all fixed:
  - **Sent back last.** The Manager's send-back is dated `reviewed_at`, not `decided_at`. The "sent back last"
    ordering read only `decided_at`, so Rashid was sent to correct the engineer's older 4-month set instead of his
    own 72-day redo, and refiled it at 4 months. It now orders by either date, and the markups search line carries
    the set's reference (`markups 2795b746 · …`).
  - **The engineer's decision stands.** Salem accepted Quantix's warning (4 months against 72 working days) as "a
    conservative allowance", against the engineer's answer. A warning on work whose earlier version the engineer
    corrected can no longer be accepted: the Manager sends it back with their decision, or escalates.
  - **A reply is owed.** Salem gave out and reviewed the work, all in the team room, and never wrote back. Someone
    the engineer wrote to last whose one finished turn since gave no message, no question and no work filed is
    woken once more, to tell the engineer what they did. It is read from the records, so a restart keeps it.
  - **Questions show in the chat.** His question to the engineer went to the Overview only, and a second one was
    refused because the first was still waiting, while the chat said "Asked you to decide". A direct chat now shows
    the person's questions as cards (a waiting one at the end): pick an option and send it, or answer in your own
    words. A refused question reads "Question not sent".
- **Chat layout.** Each run of one person's turns, messages and questions sits under their face and name, as a
  chatbot shows its thinking above its reply. A turn's folded line says what they last did ("Worked for 12 s ·
  Briefing Rashid"). Tools that don't describe themselves get plain words ("Wrote to you", "Gave the review"). A
  written "Sources:" line is hidden when the sources show as links.

## 28 September 2026: the Manager tells the engineer what waits for their approval

On the real tender Salem accepted the redone markups the engineer had asked to see ("Bring the new markups to me
when you've reviewed them") and never said so. The engineer found them by chance under Markups and summary. Nothing
told the engineer when the last approval was done, either. The same gap showed in several places:

- **He tells them, and says when nothing waits.** `reviews.with_engineer` lists what the Manager accepted that waits
  for the engineer (the rates and markups the Estimate shows, quote recommendations, enquiries to send), and
  `for_engineer` says it in a line with the screen for each. His duties say to tell the engineer once his review is
  done, and to say when nothing waits. His review's answer reminds him, and every briefing ends with the line.
  Quantix wakes him once (not after a finished turn of his that started since) when:
  - work he accepted waits and his last message to the engineer is older than it; the brief lists each record and
    its screen;
  - the engineer approved the last of it (or sent the last enquiry) after his last message.
- **Stuck in his queue.** The next redo priced monthly site staff by the day (preliminaries 155% of net). Salem
  couldn't send it back a third time, and his escalation was turned away twice: he named the markups themselves
  as a BOQ line, and markups have no page of their own. His turn ended and nothing woke him again. Now:
  - a source naming the escalated work, or giving no page or BOQ line, is left out (once this was live, Salem's third
    attempt gave the markups as a source with no page, and was refused again);
  - work Quantix's checks find a problem in needs no page, since its finding shows beside the decision (as for
    approved work);
  - work still in his queue after one finished turn since it came, neither decided nor escalated, wakes him once
    more to settle it.
- **Where to approve.** The Overview's "1 price to approve" opened the Estimate's "Needs you · 0". Markups waiting
  now count there, with a dot on Markups and summary and a line that opens them. The Takeoff sheet list marks
  sheets with marks waiting ("· needs you"). The Overview counts enquiries to send (`gates.enquiries`). The rail
  shows the count beside the open tender's Overview. The Manager's chat ends with "Waiting for your approval", one
  click from each screen, until nothing waits (`tenders/needsYou.ts`, shared with the Overview).

## 28 September 2026: every document opens as its own app shows it

The Documents viewer showed PDFs as fixed pictures and everything else as the text Quantix read: a DWG was a list of
its words, a workbook "A1=… | B1=…". Each kind now opens as its own app shows it, at the page a source names:

- **PDF** with PDF.js (`pdfjs-dist`): pages drawn from the file, sharp at any zoom, text that can be selected, links
  inside the document. A 100 MB, 2,250-page specification opens by the megabyte at the page asked for, and holds it
  while pages of other sizes load. Opened from a search, the words searched for are marked. PDF.js's character maps,
  fonts and image decoders are served under `/pdfjs` (a plugin in `vite.config.ts`); the window's CSP allows
  WebAssembly for the decoders.
- **Word** with `docx-preview`: pages, fonts, tables, pictures, headers and footers. A source names one of Quantix's
  parts of about 3,000 characters; the document opens where that part starts, marked for a moment.
- **Excel** from the service (`documents/sheets.py`, `GET /documents/{id}/sheets/{n}`): each cell's text as Excel
  shows it with its number format, merged cells, widths, heights, fonts, fills (theme colours and tints), borders,
  frozen rows and columns, right-to-left sheets, and sheets as tabs. Hidden rows and columns stay hidden, with a
  switch to show them; the first 5,000 rows show.
- **DWG and DXF** drawn by the Takeoff screen's drawing view, read-only, with model space and each layout as tabs.
- **Images** zoom and pan; a TIFF is sent as PNG, which screens can show.
- **Open original** opens the file in the app the computer uses for its type (AutoCAD, Excel, Word…), from a
  read-only copy under `tenders/<id>/opened`, so the stored file stays as supplied. It used to take the whole window
  to the raw file. Deleting the tender removes the copies.
- **The list** is narrower, with an icon per kind and plainer coverage ("12 pages opened by the office · 3 cited").
  A search can be cleared with × or Escape, keeps the open document, and says how many pages it found. Zoom works
  with the controls and Ctrl + wheel; moving through a document replaces the address instead of adding steps to Back.

## 28 September 2026: the drawing tools on PDFs printed from CAD, and a CAD viewer to measure with

The engineer asked why the Takeoff screen offered none of what came with OpenCADStudio's libraries on the SEC 8485
and 8486 tender. Its drawings are all PDFs, and only DWG and DXF files were read as drawings; its PDFs were printed
from CAD, and keep their lines.

- **PDFs read as drawings** (`documents/vectors.py`). The 8485 addendum's 130,498 strokes read as 96,044 objects
  in its CAD colours, in about 8 s, once: a closed outline is found from strokes printed one side at a time, so
  clicking the control room's outline gives 86.509 m round and 401.663 m² at the sheet's 1:214 scale. Hatch lines
  are cut to their clipping outlines as the PDF shows them, AutoCAD's text strokes become their words (the words
  written invisibly over them), and white masking is left out. The civil base design reads as 158,156 objects.
- **The Takeoff screen** draws a CAD drawing, or a PDF page drawn in lines, in its own colours, and gains:
  points placed on the drawing (Length, Area, Count and, on PDFs, Scale), snapped to a line's end or middle, where
  two lines cross, or onto a line, with the snap named beside the pointer and shift keeping a line square; the
  length or area placed so far; **Enclosed**, a click inside an area the lines close off, found with opencadkernel
  in a window around the click that widens until the region lies inside it (0.8 s inside the real control room);
  choosing by a box (shift-drag), everything like the chosen, or everything on a layer; each layer's colour; what
  one chosen object is ("closed polyline on orange FF7F00 · 0.51 mm"); words found on the drawing; zooming to a
  measurement's objects or points with Show; and Esc, Enter and Backspace as in CAD. A scanned page keeps the page
  picture and its tools.
- **CAD drawings** now keep their colours (qx-dwg format 4: by layer, by block and true colours), and the engineer
  can measure by points in model space, in its units.
- **Staff tools** take a PDF page drawn in lines as they take a CAD drawing: `drawing_overview`, `query_drawing`,
  `view_drawing` and `measure_drawing` (with its page) work on its pens and objects with the sheet's scale.
- **Faster:** the screen copy is built without a loop per object (3.7 s to 0.15 s on the addendum).
- **Checks:** 6 new service tests with a synthetic vector PDF (strokes joined, a tee left apart, a circle, a fill,
  letters and their word, a clipped hatch line, white, the PDF's own layers, a scan page) and a colour DWG; 5 new
  interface tests (snapping, crossings, choosing by a box, likeness, words) and 3 screen tests (points on a CAD
  drawing, Enclosed, a PDF drawn in lines).


## 29 September 2026: one window, one tender, the team beside it

The engineer called the interface unstructured: every tender and every staff member in the sidebar, two sidebars
in the Office, lessons and a delete button on the Overview, and a window that behaved like a web page. The audit
(44 findings) and the agreed structure are in the UI audit artifact; this is its first part, the shell.

- **Title bar.** The window has no Windows frame; Quantix draws its own bar: sidebar, back and forward, where the
  engineer is, Search (Ctrl+K), what the team is doing, what needs them, the Team button and its own minimise,
  maximise and close. Drag it to move the window, double-click it to maximise (`desktop/capabilities`).
- **Sidebar.** One tender at a time: a switcher lists the others, soonest due first, with what waits in each, and
  switching goes back to the screen last open in that tender. Each screen counts what waits there. The firm's
  library, directory, rules and details sit under Company; Settings is only settings. Ctrl+B folds it to icons,
  and Takeoff folds it, and closes the team, to give the drawing room.
- **Team panel.** The Office screen became a panel beside any screen (Team or Ctrl+J): faces across the top for
  the Tender Manager, each person and the team room, the conversation, and a profile that opens over it and closes
  with its X, Close or Esc. The Overview's own message box is gone; before the Manager joins, it offers "Write to
  the office". Old links to `/office` open the panel.
- **Keyboard.** Ctrl+K searches tenders, screens, people and actions; Ctrl+1 to Ctrl+7 open the tender's screens;
  Alt+Left and Alt+Right go back and forward.
- **Where the engineer was.** Quantix reopens on the last tender and screen.
- **Checks:** 5 new shell tests and the Office tests moved to the panel; walked through in the desktop window
  (switching tenders, the panel and profile, Ctrl+K, maximise, double-click and drag, Takeoff folding, Alt+Left).
