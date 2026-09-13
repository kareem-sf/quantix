# Quantix home and clean reset

9 September 2026. The engineer explicitly requested deletion of `%LOCALAPPDATA%\Quantix` and `~/.quantix`, followed by a clean start using `~/.quantix` for Quantix-managed storage. This replaces the prior split-layout decision; no migration or backup of the deleted app data is requested.

## Storage contract

The normal desktop and development launch use the current OS user's `~/.quantix` on every supported platform. Old `QUANTIX_HOME` and `QUANTIX_CONNECTION_FILE` environment values must not silently select the old storage locations. The explicit Python `--home` argument and repository constructors may remain available for isolated developer probes; that root must own their complete service data/components/logs/temp tree.

Keep the existing database/object/output layout directly under the chosen root to avoid unnecessary database/schema changes:

- `quantix.sqlite`, other domain/usage/delivery databases, settings and restore journals.
- `objects/`, `extractions/`, `outputs/`, `models/`, `backups/` and existing domain directories.
- `ai-components/` for private downloaded software; `ai-runtimes/` for separate account profiles.
- `logs/` for diagnostics and launcher console captures.
- `runtime/connection.json` for the local service connection, `runtime/tauri-running.json` for development launch configuration, `runtime/openapi.json` for the generated schema.
- `runtime/tmp/` for application temporary work; `runtime/webview/` for native renderer state where the platform's documented WebView API supports an explicit directory.
- `cache/` for application/library caches not already located in a more specific owned directory.

Python, Node and Rust each use a small path helper. The independent language entry points must resolve the same normal root and runtime filenames. AI component storage derives from the repository root, not platformdirs. The copied dependency-free worker logger retains its explicit service-supplied directory and must not infer a new root from isolated worker HOME. Normal sign-in browser profiles remain the user's browser profiles; do not relocate or copy them.

Application startup sets process-local temp/cache paths before dependent libraries initialize. Explicit extraction scratch directories should use the selected root. Source code, project dependencies, compiler output and developer review artifacts remain in the repository. User-supplied originals, chosen exports, OS-protected API/mail secrets and OS-managed facilities are outside this application-data contract.

## Reset sequence

1. Record and stop only the running Quantix service, desktop, launchers and owned children. Preserve unrelated processes.
2. Resolve and validate the two exact user-named absolute paths. Reject unexpected root links and verify targets do not include the source repository or original supplied package.
3. Delete those two folders. Do not migrate their databases, AI account material, software or logs. The updated code ignores old repository runtime records; existing ignored developer review artifacts are not application input.
4. Apply the unified path code and documentation. No legacy auto-migration/fallback.
5. Start a fresh local service/app and check root paths, empty Tender/account lists, diagnostic location and absence of recreated legacy AppData storage. Do not reconnect AI or import a package automatically.

## Verification

Follow AGENTS.md: no broad suites, lint, typechecks, general browser QA or release packages. Use source review and focused path/startup probes. A normal development launch may compile the changed native shell. Record any platform WebView limitation honestly. Prior successful Grok acceptance is historical after this reset; the new workspace has no configured account.

## Current execution status

Source changes and independent review are complete. Isolated Python/API probes passed selected-root database/components/logs/temp routing, empty Tender/account lists despite an ambient synthetic provider key, legacy environment rejection and normal browser-profile preservation. Read-only Node path/environment checks passed. The Windows native shell compiled successfully in development mode, with unused-code warnings. No release package, broad suite, lint, typecheck or browser QA was run.

Quantix and its identified owned processes were stopped. Automatic approval policy rejected the deletion commands, so the engineer deleted the two folders manually. The primary then verified that both paths were absent before launching Quantix. No alternate deletion route, migration, provider login or source-package import was performed.

The clean native development launch passed. The running service reports `C:\Users\kareem\.quantix`, with zero Tenders, zero AI accounts and provider readiness false. Diagnostics are available in `logs/`. Six owned WebView processes were observed using `runtime/webview`; the legacy `%LOCALAPPDATA%\Quantix` folder was not recreated. The updated bindings command also succeeded, writing `runtime/openapi.json` under the new root and generating the repository's frontend declarations. Quantix remains running as a fresh workspace; accounts and Tender packages must be added explicitly.
