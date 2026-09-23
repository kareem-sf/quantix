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

Next: estimating (unit rates with sources, build-ups, markups, the company rate library).
