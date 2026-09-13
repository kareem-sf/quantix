# Quantix

Quantix is your local Tender Office. Work through the Tender Manager, inspect the sources and retain control over scope, quantities, prices and release.

## Open Quantix

Double-click **Start-Quantix.cmd** in this folder. Quantix-managed data is saved under `~/.quantix` (`C:\Users\kareem\.quantix` on this computer), outside this repository. This includes the database, imported copies, outputs, private AI components/accounts, caches, local backups, logs and runtime files. Supplied originals remain in their chosen location. See [the storage layout](docs/unified-storage.md).

Open **Settings → AI accounts → Add AI account**. Choose OpenAI, Anthropic, Google, xAI or a custom OpenAI-compatible service. OpenAI and xAI offer eligible subscription sign-in alongside API keys. Choose your access method, connect, select a model and check it. API billing is separate from subscriptions. Then open the Tender's **AI setup**, choose the checked account and review its spending terms. See the [setup guide](docs/ai-setup-guide.md) and [provider support](docs/ai-provider-support.md). Supplier mail has separate settings and explicit sending approval.

## MVP workflows

- **Manager:** analyse a package, propose a work plan, coordinate approved specialists and bring decisions back to you.
- **Project map:** connect buildings, areas, disciplines and work to supporting documents; record exactly what you reviewed.
- **Files:** inspect originals, revisions, processing notes, search results and calibrated drawing measurements.
- **Work:** follow specialist results, stop work and prepare/review supplier requests.
- **Estimate:** review BOQ quantities, rates and separate measurement proposals; create draft documents, client-format BOQ copies, submission requirements and reviewed local export packages.

For a BOQ supplied as PDF or Word, choose **Estimate → Add BOQ row from source**, select the exact passage and copy the row's quantity and unit. Proposed rows need your explicit confirmation before pricing. When a file is revised, Quantix keeps affected rows visible until you confirm their specific replacements or record a decision tied to the reviewed file revision. Submission requirements retain their source clause, conditions and exceptions for review.
- **Settings:** manage provider/mail connections, reusable approved notes and workspace backups.

Routine document drafts can be produced within an approved work plan. Final export is a separate engineer decision and does not send the package to a client.

## Current status

The desktop is being extended under the approved [full agentic office programme](docs/superpowers/plans/2026-09-12-full-agentic-office.md), including reusable professionals, shared tools, research and recorded calculations. Implementation and acceptance are tracked in [progress](docs/progress.md); historical connection results do not establish readiness for a changed account or runtime. The current programme authorizes backend/UI suites, typechecks and rendered journey checks; release packaging remains excluded. Local Python needs the qualified isolated runtime shown in Settings. Native DWG interpretation and automatic OCR remain unavailable; original files and PDF drawing pages remain accessible.

## Diagnostics and troubleshooting

To start again with an empty workspace, open **Settings → Reset Quantix**, review what will be deleted, type **RESET**, and choose **Reset and close**. This removes all Quantix-owned data and saved connections, including local backups. Original files and exports outside the Quantix home are preserved. Reopen Quantix after cleanup to set it up again. See [reset and recovery](docs/factory-reset.md).

Quantix records sanitized technical events in `~/.quantix/logs` alongside its other managed data. Open **Settings → Technical details** for the recording status and exact directory. Keep the reference shown in an error so its service/worker history can be found. Logs rotate and have bounded retention; they exclude raw Tender content and credentials. See [finding an error](docs/logging.md).

Normal launch ignores the old `QUANTIX_HOME` and `QUANTIX_CONNECTION_FILE` environment variables. A clean reset starts with no Tenders or AI profiles; reconnect accounts and import packages explicitly. API/mail secrets remain protected by the operating system credential store. Windows/Linux WebView state uses `runtime/webview`; macOS WebKit storage remains OS-managed because the documented Tauri API cannot choose its directory.

## Development commands

Use the existing environment on this computer. Fresh Windows source setup requires Python 3.12, Node 24 and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/):

```powershell
python -m venv backend/.venv
backend/.venv/Scripts/python.exe -m pip install -c backend/requirements-dev.lock.txt -e './backend[dev]'
npm ci
npm run dev
```

`npm run dev` starts the private service and interface at `http://127.0.0.1:1420`. The Windows shell starts with `node scripts/start-desktop.mjs` or Start-Quantix.cmd.

For macOS/Linux source setup, create the environment with `python3.12 -m venv backend/.venv`, install with `backend/.venv/bin/python -m pip install -e './backend[dev]'`, then run `npm ci` and `sh Start-Quantix.sh`. The Windows dependency snapshot is not a cross-platform lock. Workspace paths and release-only recipes are in [desktop packaging](docs/desktop-packaging.md). No fresh-machine package has been built for this increment.

`npm run bindings` generates frontend API declarations. `npm run test:backend`, `npm run test:ui`, `npm run check:ui` and the formatting/check commands are authorized for the current programme. Backend tests use the project virtual environment. No signed production installer has been produced.

`node scripts/python.mjs scripts/run-agent-benchmarks.py` validates the 24 synthetic cases without an AI request; add `--deterministic` to run recorded calculation probes. Live benchmark options and explicit account/model/budget requirements are documented in [the benchmark guide](backend/quantix/benchmarks/README.md). Dataset validation and synthetic model tests do not establish live engine acceptance.

See [the specification](docs/spec.md), [architecture](docs/architecture.md), [API contracts](docs/contracts.md), and [current progress and limitations](docs/progress.md).
