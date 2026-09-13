# Selected AI software

These files describe software installed only when an engineer prepares an AI
connection. They contain public publisher metadata and dependency locks, not
account credentials, downloaded clients or Tender information.

`manifest.json` pins uv 0.12.10, CPython 3.12.14 from Astral's 20260901 build,
Node 24.20.0, the existing provider SDK versions, Copilot runtime 1.0.83,
and the official Grok native client 1.0.13.
Archive SHA-256 values were read from the publishers' release metadata or checksum
files. Python locks include the complete transitive dependency set and SHA-256
hashes. Runtime installation requires binary wheels and does not run a build.
The npm locks record registry integrity for every resolved dependency; client
installation disables lifecycle scripts. Gemini's optional terminal/keychain
native packages are omitted; its published standalone JavaScript bundle is used.

Grok downloads only the selected `@xai-official/grok-<platform>` native npm
archive and checks its published SHA-512 integrity before extraction. Its native
binary is Brotli-compressed inside that archive. Quantix's existing native
process helper decodes it with pinned `brotli-decompressor` 5.0.3, with checked
absolute sibling paths, a 600 MiB output limit and failed-output cleanup.
No Node runtime, npm wrapper, publisher installation script or PATH change is
needed. Grok's six Python locks copy the identical complete minimal Gemini worker
base, without adding a provider Python SDK or decompression dependency.

The locks support Windows x64, macOS x64/ARM64 and Linux glibc x64/ARM64.
Windows ARM64 supports native Anthropic, Google, Bedrock, Mistral, Cohere, Codex,
Copilot, Gemini and Grok. OpenAI API components require tiktoken wheels unavailable for
Windows ARM64; the pinned Claude client SDK also lacks that target's wheel.
Those combinations fail with an explicit setup message. musl Linux and other
processor targets have no reviewed manifests.

The two `constraints-<target>.txt` files allow the binary resolver to choose
cryptography and tiktoken within the pinned SDK's own dependency ranges where
the previous Windows development snapshot lacks wheels. Each resulting target
lock still pins exact versions and hashes; the engineer's installer never
changes versions to find a fallback. The snapshot constraints are maintenance
inputs and are never installed as a dependency bundle.

Maintainers refresh metadata with `scripts/resolve_ai_components.py`, passing an
absolute uv executable and, for npm lock updates, absolute Node and npm entry
paths. This is dependency metadata resolution only. Review changed pins, hashes
and platform coverage before committing; software/account execution and joint
testing are separate work.

Use `scripts/resolve_ai_components.py --grok-only` to refresh just Grok's pinned
registry metadata and copy the unchanged base locks. It leaves every other
component definition and pin alone. See `docs/reports/grok-components.md` for
the installation layout, reviewed sources and deferred acceptance scenarios.
