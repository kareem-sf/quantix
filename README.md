# Quantix

Quantix is your local Tender Office. Work through the Tender Manager, inspect the sources and retain control over scope, quantities, prices and release.

## Open Quantix

Double-click **Start-Quantix.cmd** in this folder. Your Tender data is saved under `%LOCALAPPDATA%\Quantix`, outside this repository. Imported originals remain unchanged.

Enter your OpenAI API key privately in **Settings** to enable Manager and specialist work. Credentials are saved in the Windows credential store. Supplier mail has separate settings and explicit sending approval.

## MVP workflows

- **Manager:** analyse a package, propose a work plan, coordinate approved specialists and bring decisions back to you.
- **Project map:** connect buildings, areas, disciplines and work to supporting documents; record exactly what you reviewed.
- **Files:** inspect originals, revisions, processing notes, search results and calibrated drawing measurements.
- **Work:** follow specialist results, stop work and prepare/review supplier requests.
- **Estimate:** review BOQ quantities, rates and separate measurement proposals; create draft documents, client-format BOQ copies, submission requirements and reviewed local export packages.
- **Settings:** manage provider/mail connections, reusable approved notes and workspace backups.

Routine document drafts can be produced within an approved work plan. Final export is a separate engineer decision and does not send the package to a client.

## Current status

The MVP implementation is ready for our joint testing session. At your request, the latest development changes have **not been tested**. Native DWG interpretation and automatic OCR remain unavailable; original files and supported PDF drawing pages remain accessible. Live AI and mail workflows require your configured connections.

## Development commands

Use the existing project environment on this computer. For a fresh development setup:

```powershell
python -m venv backend/.venv
backend/.venv/Scripts/python.exe -m pip install -c backend/requirements-dev.lock.txt -e './backend[dev]'
npm ci
npm run dev
```

`npm run dev` starts the private service and interface at `http://127.0.0.1:1420`. The Windows shell starts with `node scripts/start-desktop.mjs` or Start-Quantix.cmd.

`npm run bindings` generates frontend API declarations. Existing `npm run verify`, `npm test`, and formatting/check commands are retained for the later testing session; do not run them while the current no-tests instruction applies. No signed production installer has been produced.

See [the specification](docs/spec.md), [architecture](docs/architecture.md), [API contracts](docs/contracts.md), and [current progress and limitations](docs/progress.md).
