# AGENTS.md

- Do not preserve backward compatibility. Remove obsolete paths instead of adding compatibility layers, fallbacks, or migrations.
- Choose the simplest implementation that fully meets the current requirements. Avoid speculative abstractions, configuration, and indirection.
- Grow the system in layers. Start from the smallest version that works end to end, and add each new capability on top of a product that already works. Never trade a working product for unfinished complexity.
- Keep components modular and concerns clearly separated.
- Prefer established, well-maintained libraries when they reduce overall complexity or improve reliability. Do not reimplement common functionality without a clear reason.
- Lean on the dependencies already in the project before writing your own implementation or adding packages. Do not assume a library lacks a capability without checking its documentation and types.
- Make architectural decisions for the long term. Do not accept a stopgap that only works for now and is meant to be replaced later.
For every non-trivial task, use a manager-worker approach.
- The primary agent owns planning, architecture, task decomposition, integration, review, and final verification.
- Delegate concrete, bounded, independent subtasks to subagents whenever useful.
- Prefer the fastest suitable lower-cost model for straightforward work, and use stronger models only when complexity requires them.
- Keep subagent file ownership non-overlapping to prevent conflicting edits.
- Continue discussing and planning with the user while subagents work.
- Review every subagent result and inspect its changes before accepting them.
- Run the relevant repository verification after integrating delegated work.
- Do not delegate trivial tasks where coordination would cost more than doing the work directly.

## Agent skills

### Issue tracker

Issues and specs are tracked in GitHub Issues for `kareem-sf/quantix`. See `docs/agents/issue-tracker.md`.

### Triage labels

The default mattpocock/skills triage labels are used. See `docs/agents/triage-labels.md`.

### Domain docs

Domain documentation uses the single-context layout. See `docs/agents/domain.md`.

## Repository commands

- Install JavaScript dependencies: `npm install`
- Run the desktop application in development: `npm run tauri dev`
- Check formatting without changing files: `npm run format:check`
- Run TypeScript typechecking and Rust clippy: `npm run check`
- Run deterministic tests and regenerate Rust-owned TypeScript DTOs: `npm test`
- Record deterministic product acceptance from a command file: `npm run acceptance:deterministic -- <application-home> <command.json>`
- Aggregate deterministic Product Acceptance Runs: `npm run acceptance:aggregate -- <application-home> <source-revision>`
- Record one opted-in local live-provider run: `npm run acceptance:live -- <application-home> <command.json>`
- Aggregate five clean private Windows runs: `npm run acceptance:private -- <application-home> <release-candidate-sha256>`
- Validate native package evidence: `npm run acceptance:native -- <application-home> <native-command.json>`
- Evaluate aggregate public release acceptance: `npm run acceptance:release -- <application-home> <gate-command.json>`
- Build the production renderer: `npm run build`
- Build the native desktop package: `npm run build:desktop`
- Run the development verification gate without building: `npm run verify`

Keep generated declarations under `src/bindings` committed. Do not edit them manually.
Production builds are explicit release-stage operations; do not run them during normal development.
