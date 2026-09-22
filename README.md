# Quantix

Quantix is a tendering office on your desktop. A Tender Manager leads AI staff who read the tender, take off
quantities from the drawings, price the work, level subcontract and supplier quotes and prepare the submission.
You review and decide at every gate. See [the specification](docs/spec.md) and [architecture](docs/architecture.md).

Quantix keeps its data in `~/.quantix`. Your original tender files are never changed.

## Open Quantix

Double-click **Start-Quantix.cmd**. It opens the desktop window, which starts the local service.

## Development

You need Python 3.12, Node 24 or later, and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
(including Rust). Set up once:

```powershell
py -3.12 -m venv service/.venv
service/.venv/Scripts/python -m pip install -e "./service[dev]"
npm ci
```

On macOS or Linux, create the environment with `python3.12 -m venv service/.venv` and install with
`service/.venv/bin/python -m pip install -e "./service[dev]"`.

`npm run dev` starts the interface at `http://localhost:1420`. The dev server also starts the service on a free
local port with a fresh access token, and forwards `/api` to it. `npm run tauri dev` opens the same interface in
the desktop window.

| Command | What it does |
| --- | --- |
| `npm run test:service` | Service tests |
| `npm run test:ui` | Interface tests |
| `npm run check` | Typecheck, Ruff and Clippy |
| `npm run verify` | `check`, then both test suites |
| `npm run build` | Typecheck and production build of the interface into `dist/` |
| `npm run bindings` | Regenerate the interface's API types from the service |

CI runs all of these on every pull request and fails if the API types are out of date.
