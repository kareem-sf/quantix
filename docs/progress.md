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

Next: AI connections.
