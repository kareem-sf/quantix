# Selected AI software

These files describe the original subscription clients Quantix installs only
when an engineer prepares a ChatGPT (Codex) or Grok subscription connection.
They contain public publisher metadata and dependency locks, not account
credentials, downloaded clients or Tender information. Direct API connections
(OpenAI, Anthropic, Google, xAI and custom OpenAI-compatible endpoints) run in
the bundled service and need nothing from this folder.

`manifest.json` pins uv 0.12.10, CPython 3.12.14 from Astral's 20260901 build,
the Codex Python client 0.147.0 and the official Grok native client 1.0.13.
Archive SHA-256 values were read from the publishers' release metadata. Python
locks include the complete transitive dependency set and SHA-256 hashes.
Runtime installation requires binary wheels and does not run a build.

Grok downloads only the selected `@xai-official/grok-<platform>` native npm
archive and checks its published SHA-512 integrity before extraction. Its native
binary is Brotli-compressed inside that archive. Quantix's native process helper
decodes it with pinned `brotli-decompressor` 5.0.3, with checked absolute sibling
paths, a 600 MiB output limit and failed-output cleanup. No Node runtime, npm
wrapper, publisher installation script or PATH change is needed.

The locks support Windows x64/ARM64, macOS x64/ARM64 and Linux glibc x64/ARM64.
musl Linux and other processor targets have no reviewed manifests.

The two `constraints-<target>.txt` files allow the binary resolver to choose
cryptography within the pinned SDK's own dependency ranges where the Windows
development snapshot lacks wheels. Each resulting target lock still pins exact
versions and hashes; the installer never changes versions to find a fallback.

Maintainers refresh metadata with `scripts/resolve_ai_components.py`, passing an
absolute uv executable. This is dependency metadata resolution only. Review
changed pins, hashes and platform coverage before committing.
