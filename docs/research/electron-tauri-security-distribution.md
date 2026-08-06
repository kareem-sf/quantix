# Electron and Tauri 2 security and distribution for Quantix v0

Status: research for GitHub issue [#17](https://github.com/kareem-sf/quantix/issues/17)

Evidence snapshot: 2026-08-06

Scope: desktop shell, renderer boundary, platform WebView, packaging, signing,
updates, accessibility, crash visibility, maintenance, and operational evidence.
This report does not change ADR 0009 and does not decide the Rust Host library
stack, Codex/Docling embedding, or prototype results covered by sibling tickets.

## Finding

Both Electron with a TypeScript Host and Tauri 2 with a Rust Host are credible,
maintained foundations for a privileged local desktop application. Tauri is not
a categorical cross-platform or security upgrade, and Electron is not a
categorically simpler or safer choice.

For the criteria in this ticket, **keep Electron as the provisional baseline**.
Tauri earns a real advantage for its framework-level capability, permission, and
scope system and for a first-party updater that creates cryptographically signed
artifacts on Windows, macOS, and Linux. Electron earns the stronger advantages
for Quantix today: one pinned Chromium behavior across the three desktop targets,
a documented Chromium accessibility path, explicit renderer and child-process
failure events, a much longer production record, and no second application
language beyond the already-required React/TypeScript frontend. Electron Forge
also absorbs Electron-native ABI rebuilding rather than requiring Quantix to
invent packaging code.

That is a provisional architectural conclusion, not confirmation of ADR 0009.
Tauri should replace it only if the remaining research and equivalent packaged
prototypes prove all of the following:

1. a Rust Host reuses equally mature deep modules without recreating process,
   workflow, SQLite, content, archive, validation, and recovery machinery;
2. packaged Windows, macOS, and Linux artifacts pass the same renderer, IPC,
   child-process, updater, crash-recovery, and accessibility contract;
3. system-WebView variation creates no platform adapter or support burden that
   outweighs Tauri's capability and resource advantages; and
4. measured startup, memory, package, and update costs are material after the
   Codex binary, uv, Docling runtime, and models are included.

Until those facts exist, changing the ADR would prefer a promising shell feature
set over the simplest durable total system required by `AGENTS.md`.

## Method and current health

Current library documentation was fetched through Context7 first, then checked
against the owners' documentation, repositories, releases, policies, and public
advisories. Community reports below are attributable GitHub reports and are used
only to expose operational failure modes, not as popularity votes.

As of the evidence snapshot:

| Project | Current release | Live repository evidence | Licence |
| --- | --- | --- | --- |
| Electron | [v43.3.0, 2026-08-04](https://github.com/electron/electron/releases/tag/v43.3.0) | [122,375 stars; same-day activity](https://api.github.com/repos/electron/electron) | [MIT](https://github.com/electron/electron/blob/main/LICENSE) |
| Electron Forge | [v7.11.2, 2026-05-20](https://github.com/electron/forge/releases/tag/v7.11.2) | [7,122 stars; same-day activity](https://api.github.com/repos/electron/forge) | [MIT](https://github.com/electron/forge/blob/main/LICENSE) |
| Tauri | [v2.11.5, 2026-07-01](https://github.com/tauri-apps/tauri/releases/tag/tauri-v2.11.5) | [109,971 stars; same-day activity](https://api.github.com/repos/tauri-apps/tauri) | [MIT or Apache-2.0](https://github.com/tauri-apps/tauri/blob/dev/LICENSE.spdx) |
| Wry | [v0.56.0, 2026-07-30](https://github.com/tauri-apps/wry/releases/tag/wry-v0.56.0) | [4,899 stars; same-day activity](https://api.github.com/repos/tauri-apps/wry) | [MIT or Apache-2.0](https://github.com/tauri-apps/wry/blob/dev/LICENSE.spdx) |

Stars do not separate these options; both have active owners, recent releases,
permissive licences, private vulnerability reporting, and public advisories.
Electron supports only its latest three stable majors on an eight-week major
cadence, so an Electron application must upgrade promptly
([release policy](https://www.electronjs.org/docs/latest/tutorial/electron-timelines)).
Tauri declares releases newer than 1.0 supported and targets coordinated public
disclosure within 90 days
([security policy](https://github.com/tauri-apps/tauri/blob/dev/SECURITY.md)).

Public advisory counts are not comparable measures of safety: Electron embeds a
browser and Node and has a much larger, older attack surface. They do establish
that neither option removes security maintenance. Electron published patched
2026 advisories affecting sandbox, context isolation, protocols, and Node
([example](https://github.com/electron/electron/security/advisories/GHSA-h7rp-cf8h-j98x));
Tauri 2.11.1 patched an origin-confusion flaw that let remote pages reach
local-only IPC commands
([GHSA-7gmj-67g7-phm9](https://github.com/tauri-apps/tauri/security/advisories/GHSA-7gmj-67g7-phm9)).
Quantix needs an explicit dependency-update and packaged regression cadence in
either architecture.

## The total architecture, not the shell slogan

With a genuine Rust Host, the two candidates have the same high-level process
seams:

```text
Electron                                  Tauri 2
React/Chromium renderer                   React/system-WebView renderer
        | validated preload IPC                   | invoke IPC + capabilities
TypeScript Host in main process           Rust Host in core process
        |              |                          |              |
Codex app-server     Docling CLI           Codex app-server     Docling CLI
```

Tauri does **not** add a process seam when Rust owns the complete Host; Electron
does **not** merge the sandboxed renderer and trusted Host into one security
principal. Both have a privileged core, a less-trusted web renderer, and two
supervised child-process classes. Per-Tender SQLite/content storage and recovery
rules remain Host-owned domain behavior in either design.

The Interface costs differ:

- Electron adds a preload module and a Quantix-owned allowlisted IPC contract,
  but keeps frontend, IPC types, domain Host, Codex schema, process supervision,
  and existing approved libraries in TypeScript.
- Tauri adds Rust/Cargo and cross-language command types, but its capability
  manifests make renderer authority declarative and reviewable. A Rust Host can
  also eliminate Electron's native Node ABI. Whether that is a net reduction in
  custom code depends on the Rust Host dependency research, not on Tauri itself.
- Both still need Quantix-owned domain commands, EITL gates, input validation,
  persistence invariants, process adapters, and recovery decisions. Tauri's
  permissions and Electron's sandbox do not implement those product rules.

## Renderer isolation and privileged IPC

### Electron

Electron's supported secure configuration is materially better than its old
reputation suggests. `nodeIntegration` defaults off, `contextIsolation` defaults
on, and renderer sandboxing defaults on since Electron 20. The official security
guide nevertheless makes application code responsible for the complete policy:
use a sandboxed local renderer, expose one narrow method at a time through
`contextBridge`, validate the sender of every privileged IPC message, constrain
navigation/new windows/external opening, handle permission requests, use a strict
CSP, and avoid `file://`
([security checklist](https://www.electronjs.org/docs/latest/tutorial/security)).

This is a small but critical custom Interface. Passing `ipcRenderer` or generic
send/on primitives across the bridge defeats it. Sender validation is not
framework-enforced; every handler must use the shared Quantix dispatcher. Domain
payload validation is required on top of sender validation.

Electron fuses can disable `ELECTRON_RUN_AS_NODE`, `NODE_OPTIONS`, and CLI
inspection and can enable Windows/macOS ASAR integrity plus loading only from the
ASAR. Fuses are flipped before OS code signing
([fuse reference](https://www.electronjs.org/docs/latest/tutorial/fuses)). Two
limits matter: important fuses are not secure defaults, and embedded ASAR
integrity is documented only for Windows and macOS. Signed Linux packaging still
needs its platform/package trust model.

### Tauri 2

Tauri's core process has full operating-system access; its system WebView reaches
that authority through invoke IPC. Permissions turn commands on or off, scopes
constrain parameters, and capabilities attach those grants to named windows,
WebViews, remote origins, and target platforms
([security model](https://v2.tauri.app/security/),
[capabilities](https://v2.tauri.app/security/capabilities/),
[permissions](https://v2.tauri.app/security/permissions/)). Tauri's runtime
authority checks origin and ACL before dispatch. This is stronger reusable policy
machinery than Electron provides.

It is not automatic least privilege. Capability grants merge when a window is in
multiple capabilities; all capability files in the conventional directory are
enabled unless the application explicitly selects them; plugin `default`
permissions are bundles chosen by plugin authors; and a command implementation
must interpret its scope correctly. Quantix should explicitly enable one local
main-window capability, grant named domain commands rather than filesystem/shell/
SQL primitives, deny remote origins, and validate every payload in Rust. This is
still a Quantix-owned Interface, but the authority policy is machine-readable.

### Security result

Tauri wins the **IPC authority** criterion. Electron can meet the same narrow
domain surface, but enforcement is custom dispatcher/preload code plus tests.
This advantage is meaningful for a renderer that displays tender documents and
AI-produced content. It does not by itself outweigh Tauri's additional platform
runtime matrix or prove the Rust Host is simpler.

## Real cross-platform rendering

Electron ships the same version of Chromium and Node with every application
artifact. Each Electron release provides Windows, macOS, and Linux binaries;
the current supported baseline is Windows 10+, macOS Ventura+, and Linux binaries
built on Ubuntu 22.04
([platform support](https://github.com/electron/electron#platform-support)). This
costs download, installed disk, runtime processes, and full-app security updates,
but gives Quantix one browser feature and rendering baseline.

Tauri dynamically uses WebView2 on Windows, WKWebView on macOS, and WebKitGTK on
Linux
([process model](https://v2.tauri.app/concept/process-model/)). The trade is
explicit in Tauri's documentation:

- WebView2 is an independently updated Chromium runtime. Windows 10/11 normally
  already provide it; an installer can bootstrap it. Quantix may require a
  minimum version, use the system Evergreen runtime, add roughly 127 MB for an
  offline installer, or add roughly 180 MB to pin a fixed runtime
  ([installation modes](https://v2.tauri.app/distribute/windows-installer/#webview2-installation-options)).
- WKWebView is part of macOS and is updated with the OS. An unsupported macOS
  therefore stops receiving WebKit fixes. Quantix cannot ship one pinned WKWebView
  to normalize old systems
  ([WebView versions](https://v2.tauri.app/reference/webview-versions/)).
- Linux WebKitGTK versions vary by distribution. Tauri v2 development and
  ordinary distribution require WebKitGTK 4.1 and other native packages. An
  AppImage bundles dependencies but must be built on the oldest supported base;
  its official guide says the artifact commonly grows from 2–6 MB to 70+ MB
  ([prerequisites](https://v2.tauri.app/start/prerequisites/),
  [AppImage limitations](https://v2.tauri.app/distribute/appimage/)).

The material consequence is not that Tauri is unreliable. It is that Quantix
would support three renderer engines and, on Linux, multiple package/distro/
graphics combinations. HTML semantics and most React code remain shared, but
Web APIs, font metrics, focus, drag/drop, print/PDF behavior, GPU issues, and
assistive-technology bridges require a real OS matrix. Electron reduces that
matrix to Chromium plus OS integration differences. It cannot remove OS-specific
window, installer, keychain, file permission, and accessibility testing.

For a document-heavy tender office, deterministic rendering is more material
than it is for a simple CRUD shell. Tauri must demonstrate equivalence using
representative long tables, bidirectional text, PDF/document previews, keyboard
focus, zoom, printing, and large artifact lists in packaged prototypes.

## Packaging, signing, and updates

Both projects provide maintained bundlers; neither makes platform distribution
one build on one machine.

| Concern | Electron + Forge | Tauri 2 |
| --- | --- | --- |
| Build matrix | Forge makes platform-specific artifacts and recommends native CI runners for each target ([build lifecycle](https://www.electronforge.io/core-concepts/build-lifecycle)) | Official GitHub Actions matrix builds Windows, both macOS architectures, and Linux; MSI requires Windows and cross-compiled NSIS is explicitly a last resort ([pipeline](https://v2.tauri.app/distribute/pipelines/github/), [Windows installer](https://v2.tauri.app/distribute/windows-installer/)) |
| Windows | Forge makers include Squirrel and MSIX plus signing support | WiX MSI and NSIS with Windows signing support |
| macOS | ZIP/DMG/pkg makers; Developer ID signing and notarization | app/DMG/App Store; Developer ID signing and notarization; Apple hardware/account constraints remain |
| Linux | deb, RPM, Flatpak, Snap and other makers; no built-in Electron updater | deb, RPM, AppImage, Flatpak, Snap and AUR; first-party updater artifact is AppImage |
| Application updates | Electron `autoUpdater` supports Windows and macOS only; Linux should use its package manager. Public GitHub projects may use `update.electronjs.org` ([autoUpdater](https://www.electronjs.org/docs/latest/api/auto-updater/)) | Official updater plugin covers Windows, macOS, and Linux, enforces HTTPS in production, and requires signed metadata/artifacts; it produces MSI/NSIS, macOS archive, and Linux AppImage updater artifacts ([updater](https://v2.tauri.app/plugin/updater/)) |

Tauri wins the **first-party cross-platform updater** criterion, with an important
qualification: its Linux self-update path selects AppImage. If Quantix selects
deb/RPM/Flatpak/Snap for policy or integration, the distribution channel's update
mechanism remains authoritative. Tauri's updater signature is also additional to,
not a substitute for, Windows Authenticode, Apple signing/notarization, or Linux
package provenance.

Electron's Linux updater gap is a real seam, but not necessarily custom product
code: choosing distro-managed packages deliberately can be simpler and safer
than self-update. The prototype/decision must pick a concrete v0 Linux format,
not score an abstract “Linux supported” box.

Signed-update safety above the transport is identical. Quantix must request EITL
approval, reject updates during active Agent Runs, require the defined verified
backup when durable data may be affected, install only a newer compatible build,
and recover from interruption. Neither framework understands Tender Store schema
or EITL policy.

### Native dependencies

Electron supports native Node modules, but Electron's ABI differs from stock
Node. Modules generally need rebuilding after Electron upgrades. Forge runs
`@electron/rebuild` automatically during development and packaging
([native module guide](https://www.electronjs.org/docs/latest/tutorial/using-native-node-modules)).
That mature automation materially contains—but does not eliminate—the risk for
`better-sqlite3`; packaged smoke tests are mandatory for every target/architecture.

A Rust Host compiles SQLite and other native crates into target artifacts and
removes the Electron ABI. It substitutes Rust targets, C toolchains, macOS SDKs,
Windows MSVC/WiX/NSIS, and Linux WebKitGTK development packages. Whether this is
less operational work is an empirical build-matrix question. It is not proven by
the absence of `.node` files.

## Accessibility

Electron has an explicit accessibility contract: web accessibility practices
apply, Chromium automatically enables accessibility support when assistive
technology such as JAWS or VoiceOver is present, and the application can inspect
or enable the Chromium accessibility tree
([Electron accessibility](https://www.electronjs.org/docs/latest/tutorial/accessibility)).
This is mature reusable infrastructure, not proof that a React UI is accessible.

Tauri correctly delegates accessibility to the chosen system WebView. WebView2,
WKWebView, and WebKitGTK all expose web content to their native accessibility
stacks, and WebKitGTK treats accessibility as a core feature
([WebKitGTK](https://webkitgtk.org/)). But Tauri has no equivalent documented
cross-platform accessibility guarantee or application API. WebKitGTK 2.44, for
example, added the missing GTK4 connection between web content's accessibility
tree and the surrounding widget hierarchy while noting remaining improvements
([WebKitGTK 2.44](https://webkitgtk.org/2024/03/27/webkigit-2.44.html)). Version
variation therefore affects accessibility as well as CSS/Web APIs.

Electron wins **documented uniformity**; neither option wins actual Quantix
accessibility without packaged tests. The acceptance contract must use native
controls where possible and verify semantic names/roles/values, focus order,
keyboard-only operation, dialogs, live status, zoom, high contrast, reduced
motion, and text scaling with at least NVDA on Windows, VoiceOver on macOS, and
Orca on the chosen Linux distribution. A successful DOM/aXe run is necessary but
not sufficient.

## Crash visibility and recovery

Both frameworks isolate web content in processes supplied by Chromium or the
system WebView. Host-owned SQLite transactions and persisted operation facts are
the real Tender recovery mechanism; renderer state must always be disposable.

Electron exposes `render-process-gone`, `unresponsive`, `responsive`, and
`child-process-gone`, including reasons such as crash, OOM, launch failure, and
Windows integrity failure, and integrates Crashpad through `crashReporter`
([WebContents events](https://www.electronjs.org/docs/latest/api/web-contents),
[details](https://www.electronjs.org/docs/latest/api/structures/render-process-gone-details),
[crash reporter](https://www.electronjs.org/docs/latest/api/crash-reporter)).
Quantix can exercise deterministic renderer-crash tests and replace the window
without restarting or replaying Host work.

Tauri 2.11.5's public `WebviewEvent` currently exposes drag/drop but no equivalent
portable “webview process gone” event
([API](https://docs.rs/tauri/2.11.5/tauri/enum.WebviewEvent.html)). OS-specific
hooks may be possible through Wry platform handles, but adopting them would be
custom platform code. This does not make the Rust Host less recoverable; it makes
renderer failure detection/replacement a prototype question and gives Electron
the current **portable crash-observability** advantage.

## Production and community evidence

Electron's official showcase lists hundreds of production applications and
high-scale products including 1Password, Discord, GitHub Desktop, Slack, Signal,
and Visual Studio Code
([showcase](https://www.electronjs.org/apps)). That does not prove a particular
Electron application is secure or efficient, but it is strong evidence that its
signing, accessibility, enterprise deployment, crash, native-module, and update
failure modes have been exercised for years.

Tauri is also production software rather than an experiment. Its organization
maintains Tauri, Wry, official plugins and two external security-audit reports;
the official Awesome Tauri list includes substantial file, database, AI, media,
and developer applications such as Spacedrive, Jan, Yaak, Cap, and Aptakube
([applications](https://github.com/tauri-apps/awesome-tauri#applications),
[audits](https://github.com/tauri-apps/tauri/tree/dev/audits)). Tauri 2 reached
stable in October 2024, so its v2-specific three-platform operational history is
shorter than Electron's.

Attributable reports illustrate the different support burdens:

- a Tauri/Wry update caused blank, unresponsive windows on a Windows Insider
  build until WebView2 flags were changed; maintainers classified the behavior as
  upstream/platform-specific
  ([tauri#12975](https://github.com/tauri-apps/tauri/issues/12975));
- Tauri v2 could not build on Ubuntu 20.04 because required WebKitGTK 4.1 packages
  were unavailable, confirmed by a maintainer
  ([tauri#8897](https://github.com/tauri-apps/tauri/issues/8897));
- an Electron Forge report of an invalid macOS signature was traced by the
  maintainer to missing real Developer ID signing after fuse mutation, showing
  that Forge automation still requires a correct signing identity and packaged
  verification
  ([forge#3757](https://github.com/electron/forge/issues/3757)); and
- a `better-sqlite3` Electron report was traced by its maintainer to native-module
  project/rebuild configuration, demonstrating that Forge's supported path must
  be tested rather than assumed
  ([better-sqlite3#1111](https://github.com/WiseLibs/better-sqlite3/issues/1111)).

These are not defect counts. They are concrete reasons to build, install, launch,
update, and recover the same fixture on native runners instead of accepting
development-mode success.

## Resource and operational cost

Tauri's official minimum can be below 600 KB because it does not bundle a browser
engine; Electron embeds Chromium and Node. Tauri therefore has a credible shell
download, installed-size, startup, and memory advantage
([Tauri size](https://v2.tauri.app/start/#smaller-app-size)). Electron itself
advises profiling memory/CPU/disk and avoiding main-process blocking rather than
claiming a universal cost
([performance guide](https://www.electronjs.org/docs/latest/tutorial/performance)).

The shell-only number is not a Quantix decision metric. Both packages must carry
or install a Codex binary, uv/Python/Docling environment, native libraries, and
versioned model assets. A fixed/offline WebView2 can erase Tauri's Windows package
advantage, while Evergreen shifts patching and version control to Windows.
Docling conversions and AI work are likely to dominate working-set peaks. The
equivalent packaged prototype must report:

- installer and installed bytes, separating app, Codex, Python, Docling, models,
  WebView/runtime, symbols, and update artifacts;
- cold/warm launch to usable UI and to recovered Tender;
- idle private/working-set memory and process count;
- memory/CPU while opening a large document and while Codex/Docling run;
- full versus differential update bytes and rollback behavior; and
- CI minutes/cache size plus the number of target-specific build steps.

No winner should be declared on an empty-window benchmark.

## Criterion result for the final decision

| Criterion from `AGENTS.md` / issue #17 | Current evidence |
| --- | --- |
| Simplest durable total architecture | Electron provisional win because it keeps the approved frontend/Host/protocol ecosystem in TypeScript; Tauri can overturn this only if the Rust Host stack is demonstrably deeper and smaller overall |
| Minimal custom security code | Tauri win for capabilities/permissions/scopes; both still require a narrow domain Interface and validation |
| Few process seams | Tie with a complete Rust Host; reject any Tauri design that keeps a separate Node Host |
| Uniform real cross-platform renderer | Electron win through one bundled Chromium; Tauri requires three-engine and Linux-distro validation |
| Signed installers | Tie; platform credentials, native runners, and OS trust rules dominate |
| Signed automatic updates | Tauri win for a first-party Windows/macOS/AppImage path; Electron Linux deliberately delegates to package managers |
| Native dependency operations | Different risks: Electron ABI/rebuild versus Rust targets/toolchains/WebKitGTK; prototype required |
| Accessibility evidence | Electron provisional win for one Chromium contract and explicit documentation; both require native assistive-technology tests |
| Renderer crash observability | Electron win through portable public crash/unresponsive events |
| Production maturity | Electron win; Tauri is active and credible but v2 has a shorter operational record |
| Shell resource cost | Tauri likely win; materiality to the complete Quantix package is unproven |
| Long-term maintainability | Unresolved: Electron demands rapid whole-runtime upgrades; Tauri delegates WebView patches but expands the platform behavior matrix |

## Required handoff to the prototype contract

The equivalent prototypes should hold the React fixture, Tender data fixture,
Codex/Docling stand-ins or real approved binaries, update feed semantics, crash
injection points, and acceptance assertions constant. They must be packaged and
run on native Windows, macOS Intel/Apple Silicon as selected, and the exact Linux
distribution/package format proposed for v0. They should specifically prove:

1. renderer compromise tests cannot invoke unlisted commands, use raw filesystem/
   process/SQL primitives, navigate to remote content, or spoof privileged IPC;
2. the Host remains authoritative when the renderer crashes, hangs, or reloads;
3. a forced Host/child-process death leaves no partial canonical Tender state and
   recovery performs no silent replay;
4. signed test updates are accepted, tampered/wrong-key/downgrade artifacts are
   rejected, and EITL/application gates work;
5. WebView/runtime absence or unsupported versions fail with an actionable path;
6. native screen-reader, keyboard, focus, scale, contrast, and reduced-motion
   checks pass; and
7. platform adapters, build files, permissions, artifacts, processes, privileges,
   and recovery branches are counted alongside lines of custom Host code.

On this evidence alone, do not supersede ADR 0009. Advance Tauri as a fully equal
prototype, not as the presumed destination; confirmation should follow the
complete Rust-stack, Codex/Docling, and packaged-prototype evidence.
