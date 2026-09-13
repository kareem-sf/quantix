# Reset Quantix

Use **Settings → Reset Quantix** to return the Windows desktop application to an empty workspace.

1. Finish or stop any current Tender work and AI account setup.
2. If you want to keep a backup, export it from **Settings → Backups** to a location outside the Quantix home first.
3. Open **Reset Quantix** and review the application folder and record counts.
4. Type **RESET**, then choose **Reset and close**.
5. Open Quantix again after cleanup. Add your AI accounts and import your Tender files as you would after a fresh installation.

Reset removes all data inside the displayed Quantix home, normally `~/.quantix`: Tender records, imported copies, extracted data, outputs, local backups, AI profiles and private sign-ins, downloaded AI software, preferences, drafts, caches, desktop browser state, temporary files and logs. It also removes the protected API, mail and private Codex credentials owned by that home. Quantix does not make an automatic copy elsewhere when resetting.

Original files and exports saved outside that folder stay in their chosen locations. Independent Codex, Grok and other applications keep their own accounts. The Quantix program and project source files remain installed. An empty workspace coordination lock may remain in the home; it contains no user data.

## If reset needs attention

If work is running, close the reset dialog and let that work finish or stop it through its normal controls. Then open a fresh reset review.

Once reset has been confirmed, the workspace stays in reset recovery until cleanup finishes. If saved credentials cannot be removed, the recovery screen offers a retry. If another program keeps a Quantix file open, close that program and retry cleanup. A failed reset is never reported as complete.

If the desktop component is too old, close and reopen the updated application before confirming reset. Browser previews and platforms without both credential and desktop cleanup support cannot perform a factory reset.

## Connection cleanup maintenance

Verified on 9 September 2026 against the bundled clients. Windows cleanup uses metadata-only credential enumeration and exact owned targets; it does not read credential blobs. Quantix API and mail services are scoped by the application home's SHA-256 identity. Codex 0.147.0 also owns `cli|{hash}.Codex Auth` and `secrets|{hash}.codex` targets for each private Codex home. Its hash uses the native canonical path, including the Windows extended-path prefix. The encrypted-store key must be removed as well as the ordinary login key. Grok Build 1.0.13 keeps its supported authentication in the private profile directory, removed during offline cleanup.

Recheck these names whenever bundled client versions change. Sources: [Codex authentication storage](https://github.com/openai/codex/blob/rust-v0.147.0/codex-rs/login/src/auth/storage.rs), [Codex encrypted secrets](https://github.com/openai/codex/blob/rust-v0.147.0/codex-rs/secrets/src/lib.rs), [Rust keyring Windows targets](https://github.com/hwchen/keyring-rs/blob/v3.6.3/src/windows.rs), and [Grok auth storage](https://github.com/xai-org/grok-build/blob/72a61251/crates/codegen/xai-grok-shell/src/auth/manager.rs). Journal, API and startup rules are in [the shared contract](contracts.md#factory-reset-2026-09-09).
