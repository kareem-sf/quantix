# Quantix

Quantix is your local tendering team. The Tender Manager leads AI staff who read the package, take off and check quantities, price, plan and draft. You review, approve and steer. Scope, quantities, prices and release stay your decisions.

## Open Quantix

Double-click **Start-Quantix.cmd** in this folder. Quantix keeps its data under `~/.quantix`, outside this repository: the database, imported copies, outputs, private AI software and accounts, caches, backups, logs and runtime files. Supplied originals stay where you chose them. See [the storage layout](docs/unified-storage.md).

Open **Settings → AI accounts → Add AI account** and choose OpenAI, Anthropic, Google, xAI or a custom OpenAI-compatible service. OpenAI and xAI also offer ChatGPT and Grok subscription sign-in through their official clients. Connect, choose a model and check it; API billing is separate from subscriptions. Then choose that account in the Tender's AI setup and review its spending terms. See the [setup guide](docs/ai-setup-guide.md) and [provider support](docs/ai-provider-support.md).

## What you can do

- **Manager:** import a package and talk to the Tender Manager. It analyses the package, proposes a work plan, hires staff, assigns their work and brings results and decisions back to you. Ask for anything; there is no fixed sequence.
- **Team:** see the Manager's working brief, every staff member and each assignment's brief, question, answer, result and usage. Steer the Manager while it works.
- **Documents:** inspect originals, revisions, extracted text, search results and drawing pages.
- **Estimate:** review BOQ rows, rates and quantity proposals. **Takeoff** shows the quantities the team took from the drawings, checked against the BOQ: quantities that differ, work the BOQ is missing and BOQ items the drawings do not show. Ask the team for a takeoff with one button, then accept or reject each line. Supplier quotation requests are drafts you send from your own mail program.
- **Submission:** submission requirements with their source clauses and conditions, draft documents, client-format BOQ copies and reviewed local export packages. Export never sends anything to a client.
- **Settings:** AI accounts, the Manager's personality, approved reusable notes, the company library and backups.

For a BOQ supplied as PDF or Word, choose **Estimate → Add BOQ row from source**, select the exact passage and copy the row's quantity and unit. Proposed rows need your confirmation before pricing. When a file is revised, affected rows stay visible until you confirm their replacements.

Native DWG interpretation and signed installers are not available. Takeoff lines are proposals from AI reading the drawings; check them before relying on them.

## Troubleshooting

To start again with an empty workspace, open **Settings → Reset Quantix**, review what will be deleted, type **RESET** and choose **Reset and close**. Original files and exports outside `~/.quantix` are kept. See [reset and recovery](docs/factory-reset.md).

Quantix records sanitized technical events in `~/.quantix/logs`; **Settings → Technical details** shows the exact directory. Keep the reference shown in an error so its history can be found. Logs exclude Tender content and credentials. See [finding an error](docs/logging.md).

## Development

Source setup needs Python 3.12, Node 24 and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (including Rust). On Windows:

```powershell
python -m venv backend/.venv
backend/.venv/Scripts/python.exe -m pip install -c backend/requirements-dev.lock.txt -e './backend[dev]'
backend/.venv/Scripts/python.exe -m pip install openai-codex==0.147.0 openai-codex-cli-bin==0.147.0
npm ci
npm run dev
```

The second `pip install` adds the official ChatGPT/Codex client that the subscription route and its tests use; CI installs the same versions. On macOS/Linux create the environment with `python3.12 -m venv backend/.venv`, install with `backend/.venv/bin/python -m pip install -e './backend[dev]'`, then run `npm ci` and `sh Start-Quantix.sh`.

`npm run dev` starts the private service and the interface at `http://127.0.0.1:1420`; `npm run tauri dev` or Start-Quantix.cmd opens the desktop window. Both use the workspace in `~/.quantix`. There is no configuration file or environment variable to set: the service picks a free port and a fresh access token on each start and publishes them in `~/.quantix/runtime/connection.json`. AI accounts are added in the app, and their keys are kept in the operating system's credential store.

| Command | What it does |
| --- | --- |
| `npm run test:backend` | Backend tests (add `-n auto` through pytest-xdist for speed) |
| `npm run test:ui` | Interface tests |
| `npm run check` | Typecheck, Ruff and Clippy |
| `npm run format:check` | Prettier and Ruff formatting |
| `npm run verify` | `check` followed by both test suites |
| `npm run build` | Typecheck and production build of the interface into `dist/` |
| `npm run bindings` | Regenerate `src/bindings/api.ts` from the backend schemas |

CI runs the backend and interface checks, including formatting, on every pull request. Run the backend and interface suites one after the other on a busy machine: a few timing-sensitive tests can fail when both run in parallel.

## Build and deployment

Quantix is a desktop application: a Tauri window over a local Python service that holds the database, the imported files and the AI connections. It cannot be served as a static website, because every screen depends on that local service. `npm run build` checks that the interface compiles for production; the desktop app loads it from `dist/`.

Installers are built per operating system with `npm run package:service` and `npm run build:desktop`, or with the manual **Desktop packages** GitHub workflow. They are unsigned and have not yet been tested on a clean machine. The recipes and their limits are in [desktop packaging](docs/desktop-packaging.md).

See [the specification](docs/spec.md), [architecture](docs/architecture.md), [API contracts](docs/contracts.md) and [progress](docs/progress.md).
