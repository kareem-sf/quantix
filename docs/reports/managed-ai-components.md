# Managed AI software implementation

Implemented source only on 7 September 2026. No tests, lint, typechecks, browser
checks, provider/client installations, account calls, live integrations, native
builds or release packages were run. Dependency resolution and publisher hash
metadata generation are the only executable preparation work performed.

The document service no longer requires Pydantic AI, provider SDKs or original
client binaries. It retains MCP and the lightweight bundled model-price catalog.
`AIComponentService` prepares only the selected protocol component in the OS
user's shared Quantix application-data directory. Connection account homes
remain under their workspace's `ai-runtimes/<connection-id>` directory.

The installer fetches hash-pinned private uv and standalone CPython archives,
creates a unique virtual environment at its permanent path, synchronizes a full
hash lock using binary wheels only, and copies Quantix worker source as data.
Copilot and Gemini also receive pinned private Node/npm and integrity-locked
official clients. Node/npm lifecycle scripts and Gemini optional native modules
are disabled. Copilot's separately hash-pinned native analysis runtime is
materialized with its retained package assets. No system Python, registry,
global package directory or shell configuration is changed.

File inventories bind installed versions to exact locks and worker source.
Activation switches one small JSON record atomically after preparation.
Component locks serialize prepare/remove operations across workspaces; held
version leases prevent removal while a worker is active. Repair creates a new
version and leaves active workers' bytes intact. Cancelling preparation stops
its native process owner and joins cooperative download/extraction work before
removing the incomplete candidate. Shared uv/Python downloads remain cached
when a provider component is removed; private sign-in files are retained.

`quantix-ai-host` is a separate Rust executable. It clears frozen Python loader
state in its own process. Windows assigns a suspended worker to a non-breakaway
Job Object before resuming it; closing the last job handle terminates descendants.
POSIX workers receive a dedicated process group and a bounded TERM/KILL shutdown
on parent pipe EOF, stop signal or worker exit. Its stdin reader runs separately
from the child writer so a client that stops consuming input cannot block parent
EOF detection. This is process lifecycle ownership, not an OS security sandbox.

The release-only service packager embeds this small native host, installation
metadata and worker source as data; provider packages are explicitly excluded.
Development startup incrementally builds the host when the engineer next starts
Quantix. Those build commands were added but not run in this session.

Public source references used for the implementation:

- [uv CLI and binary/hash controls](https://docs.astral.sh/uv/reference/cli/).
- [uv private environment controls](https://docs.astral.sh/uv/reference/environment/).
- [uv 0.12.10 release metadata](https://api.github.com/repos/astral-sh/uv/releases/tags/0.12.10).
- [Standalone CPython 20260901 release metadata](https://api.github.com/repos/astral-sh/python-build-standalone/releases/tags/20260901).
- [Node 24.20.0 checksums](https://nodejs.org/dist/v24.20.0/SHASUMS256.txt).
- [Copilot 1.0.83 checksums](https://github.com/github/copilot-cli/releases/download/v1.0.83/SHA256SUMS.txt).
- [PyInstaller subprocess loader guidance](https://pyinstaller.org/en/stable/common-issues-and-pitfalls.html#launching-external-programs-from-the-frozen-application).
- [Windows Job Object lifecycle](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).
- [Windows suspended process creation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw).
- [Rust POSIX process groups](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html).

Current publisher wheel coverage is documented in
[`backend/ai-components/README.md`](../../backend/ai-components/README.md).
All installation, cancellation, authentication and packaging behavior still
requires the engineer's deferred joint testing session.
