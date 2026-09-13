# Unified storage implementation plan

> Use the existing Superpowers delegated implementation/review workflow. User authorization already covers this concrete design and the explicitly requested reset.

**Goal:** Reset both old app folders and route all Quantix-owned runtime storage into `~/.quantix`.

**Architecture:** Small Python/Node/Rust path helpers; a single normal home and shared runtime filenames. Preserve domain layout and explicit isolated backend roots without environment-driven legacy fallback.

**Spec:** [Unified storage contract](../../unified-storage.md).

## Tasks and ownership

- [x] Primary: inventory/stop owned processes, validate the two named targets, maintain docs and review baselines. No source implementation by primary.
- [x] The engineer deleted the two named folders manually; primary verified both absent before startup.
- [x] Backend implementer (Luna/xhigh): Python storage helper; service initialization, repository-owned AI components, diagnostics, temp/cache destinations and schema exporter. Own backend/quantix and scripts/export_openapi.py only, plus packaging recipe only if required for a copied module.
- [x] Desktop/tooling implementer (Luna/xhigh): Node/Rust path helpers, dev/start scripts, bindings command/package.json, native connection/service/log paths and supported WebView user-data directory. No backend changes.
- [x] Primary plus Astra/xhigh reviewer: inspect complete per-turn diffs and shared paths; address concrete defects through original implementers.
- [x] Primary: isolated path/API probes, Node path checks, native development compilation, and README/progress/contracts updates.
- [x] Actual clean native launch and empty normal-home verification after manual deletion.

## Preflight

Backend and desktop tasks share only this path contract: normal home `~/.quantix`, connection `runtime/connection.json`, generated schema `runtime/openapi.json`, temp `runtime/tmp`, logs `logs`. Backend owns schema export, Node owns consuming it in bindings generation. No public JSON schema shape change is intended. Keep current dirty source work and tests. Do not start app/import modules that initialize real storage until primary reports the deletion complete. No commits, broad tests/lint/typechecks/browser QA/release builds or unrequested account setup.

## Execution record

Reset is explicitly authorized by the latest user message. No retained backup or migration of the two app folders. Source/developer artifacts remain separate from app state.

Primary stopped the matching service and verified project-owned desktop/launcher/process descendants. The combined deletion and subsequent separately validated literal-path AppData deletion were both rejected by automatic approval policy with the reason `blocked by policy`; neither folder deletion ran. The user was asked to remove the two folders manually. Continue source implementation/review, but do not perform a real-root clean startup until deletion is confirmed. No alternate deletion method is authorized as a way around this tool rejection.

Implementation source review is complete. Corrected native pre-PyInstaller temp routing, explicit root propagation in semantic children, cached Python tempfile routing and the diagnostic writer's unresolved-directory fallback. Isolated path/API checks and native development compilation passed; real data-folder deletion and clean startup remain pending. The two named directories were rechecked and still existed at handoff; no Quantix desktop process was running.

Final continuation: the engineer confirmed manual deletion, and primary verified both paths absent with no running desktop. Normal launch recreated only `~/.quantix`. Authenticated checks reported the correct home, empty Tender/account arrays and active diagnostics under `logs/`; six owned WebView processes used `runtime/webview`. The legacy AppData folder remained absent. `npm run bindings` successfully used the new `runtime/openapi.json` location. Clean-start result is saved in the ignored review workspace as `clean-start-result.json`. No provider reconnection, package import, broad tests or release build occurred.
