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

Next: the office runtime (Manager, hired staff, team room, direct messages, gates).
