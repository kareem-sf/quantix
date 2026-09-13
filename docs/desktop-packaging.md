# Desktop builds

The Windows development launcher remains `Start-Quantix.cmd`. macOS/Linux source users can run `sh Start-Quantix.sh` after dependency setup. Source development requires [Tauri's platform prerequisites](https://v2.tauri.app/start/prerequisites/), Python 3.12, Node 24 and Rust. A future packaged installation bundles the Python service so engineers do not need development tools.

## Local workspace paths

- Windows: `%USERPROFILE%\.quantix`
- macOS/Linux: `~/.quantix`

Normal launch always uses this root and ignores old `QUANTIX_HOME` / `QUANTIX_CONNECTION_FILE` environment values. Python's explicit `--home` and repository constructor roots remain available for isolated developer verification, with their components/logs/temp kept inside that root. Accounts and Tender facts belong to the selected root. Client sign-in material and provider keys are not portable backup contents. No project or engineer name is embedded in a build. See the [unified storage contract](unified-storage.md).

The Windows/Linux desktop sets the supported WebView data directory to `runtime/webview` before constructing the main window. Tauri's directory override is unsupported on macOS, so its WebKit storage remains OS-managed; no private API or browser-profile relocation is used. This platform limitation is distinct from the service data root.

## Release preparation only

These are build recipes, not instructions to run during the current development session. Install the project environment and frontend dependencies first:

```text
node scripts/python.mjs -m pip install -e "./backend[packaging]"
npm run package:service -- --target <native-target-triple>
npm run build:desktop -- --target <native-target-triple>
```

Recipe examples: `x86_64-pc-windows-msvc`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`. Build on the actual target OS/processor; SDK binary availability can further limit a target. The packager rejects cross-compilation. It bundles Python modules, package metadata and native SDK assets into `src-tauri/binaries/quantix-service-<target>` (plus `.exe` on Windows).

The release-specific Tauri config embeds the service. The desktop owns its child, supplies a fresh local token/port and shuts down the service on closing. Native and development discovery use `~/.quantix/runtime/connection.json`; development launch configuration and generated OpenAPI are in the same runtime directory. Generated application binaries and packaging build directories remain ignored project artifacts.

The manual-only workflow `.github/workflows/desktop-packages.yml` prepares Windows x64, macOS ARM64 and Linux x64 artifacts. It contains build steps only, grants read access to repository contents, does not publish a GitHub release and excludes workspace data/accounts. macOS uses ad-hoc signing, not a trusted publisher identity. Design follows [Tauri's build guidance](https://v2.tauri.app/distribute/pipelines/github/) and [GitHub's runner architecture list](https://github.com/actions/runner-images).

No package or workflow has been built/executed for this increment. Clean-machine startup, SDK assets, shutdown, Linux secret stores, signing/notarization and installer behavior remain acceptance/release work before distribution to other engineers.
