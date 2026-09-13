# Grok software installation — 8 September 2026

The `grok_build` connection protocol selects the shared `client-grok` software
component. The installer pins official Grok **1.0.13** for Windows x64/ARM64,
macOS x64/ARM64 and glibc Linux x64/ARM64. musl Linux and other architectures
retain the existing unsupported-platform explanation.

Only the chosen platform's native npm archive is downloaded during preparation.
Quantix checks the registry's published SHA-512 SRI before extracting anything,
rejects links, bounds archive expansion and checks the native package's name,
version, operating system and processor. Existing SHA-256 downloads remain on
their original validation path.

The publisher ships `package/bin/grok.exe.br` on Windows and
`package/bin/grok.br` on POSIX. Quantix's native helper adds one data-only command:
`--decompress-brotli SOURCE TARGET MAX_BYTES`. Both paths must be absolute; their
resolved parents must match; the source must be a regular `.br` file and the
target its missing sibling with that extension removed. It creates a new file,
streams at most the approved output size (hard ceiling 600 MiB), rejects empty
or invalid output and removes its own partial file on errors. The installer's
owned process boundary controls timeout/cancellation; cancellation cleanup also
removes the inactive candidate, including a forcibly interrupted partial output.
The helper uses the already-resolved Rust `brotli-decompressor` **5.0.3**, now an
explicit exact dependency. It does not run the downloaded client.

No Node runtime, npm wrapper, publisher postinstall script, shell profile,
machine PATH or system Python installation is added for Grok. The Python worker uses
the existing private runtime and the same four direct dependencies as Gemini:
MCP 2.1.1, Pydantic 2.13.5, jsonschema 4.26.0 and platformdirs 4.11.7. Each Grok
target lock is a byte-for-byte copy of that target's existing complete Gemini
Python lock, including every transitive pin and hash.

The permanent layout within the leased component version is:

```text
client/bin/grok.exe              # Windows
client/bin/grok                  # macOS/Linux
client/package.json
client/THIRD_PARTY_NOTICES.md
venv/...
worker/...
receipt.json
```

`receipt.json` requires the binary, package metadata, license notices and worker
files, with the usual complete installed-file SHA-256 inventory. It also records
`native_client` with `version`, `package`, publisher `integrity`, and relative
`executable`. The worker environment exposes `QUANTIX_GROK_BINARY` and
`QUANTIX_GROK_VERSION` from that exact leased receipt, alongside
`QUANTIX_AI_COMPONENT_ROOT`. It never resolves Grok from PATH. The account worker
owns a separate private `GROK_HOME`; no account data is written to shared software.

Existing installation guards, active-version pointers, final synchronous atomic
activation, immutable repair candidates, process ownership and software leases
remain in use. Removal waits for workers and keeps private sign-in homes. A
failed or cancelled preparation cannot activate its candidate. Reopening
Quantix loads the changed backend and newly compiled native helper; the current
running application is left open during implementation.

## Source inspection and generation performed

The installation/lease/receipt source, manifest, target locks and native helper
were inspected. Registry metadata for all six exact Grok package versions was
read. Only the Windows x64 native package was downloaded into memory for package
content inspection; its published SHA-512 matched. It contains four regular
files: the compressed executable, package metadata, README and third-party
notices. No publisher code was run or installed. The other platform layouts
follow xAI's original launcher source and are checked again by the engineer's
installer before activation.

`scripts/resolve_ai_components.py --grok-only` generated the Grok manifest entry
and six locks from publisher metadata and the unchanged worker base.
`cargo update --offline --package quantix-desktop` updated the root dependency
edge only and reported zero package-version changes. No tests, lint, typechecks,
native builds, browser QA, live account/model checks or release packages were
run. Runtime behavior remains untested until the joint engineer session.

Sources:

- [xAI application integration interface](https://docs.x.ai/build/cli/headless-scripting).
- [xAI original native-package launcher and Brotli layout](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/npm/grok/bin/grok).
- Exact 1.0.13 native package metadata: [Windows x64](https://registry.npmjs.org/@xai-official/grok-win32-x64/1.0.13), [Windows ARM64](https://registry.npmjs.org/@xai-official/grok-win32-arm64/1.0.13), [macOS x64](https://registry.npmjs.org/@xai-official/grok-darwin-x64/1.0.13), [macOS ARM64](https://registry.npmjs.org/@xai-official/grok-darwin-arm64/1.0.13), [Linux x64](https://registry.npmjs.org/@xai-official/grok-linux-x64/1.0.13), [Linux ARM64](https://registry.npmjs.org/@xai-official/grok-linux-arm64/1.0.13).
- [Rust Brotli decompressor 5.0.3 API](https://docs.rs/brotli-decompressor/5.0.3/brotli_decompressor/reader/struct.Decompressor.html).

## Deferred installation acceptance

- Fresh preparation on each supported target downloads only its native Grok
  archive and required Python worker software, checks integrity, and activates
  the complete receipt without signing in.
- A bad download hash, missing compressed binary, mismatched package identity,
  invalid Brotli stream or expansion limit failure leaves the prior version
  available and a clear repair message.
- Cancel during download, archive extraction, decompression or dependency
  preparation. The owned activity ends before candidate cleanup; restart shows
  preparation needs attention rather than a falsely ready account.
- Repair creates a new version while existing leased workers retain their exact
  old binary and receipt. No active files are replaced in place.
- Two Grok accounts share the software installation and retain distinct private
  account homes. Removing software while used is refused; removing idle software
  preserves both sign-ins and other providers' installations.
- Reopening after the source update loads the new helper/backend. Existing
  API-key connections, models, approvals and usage remain unchanged.
