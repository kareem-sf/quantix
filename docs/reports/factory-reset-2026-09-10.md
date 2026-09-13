# Factory reset implementation and acceptance

Implemented the requested Windows Settings reset. The separate request to delete `quantix-final-review-_lktvvvd` from the Windows temporary directory was rejected by automatic approval review with `blocked by policy`; it was not deleted or included in reset scope.

## Result

**Settings → Reset Quantix** provides one review dialog with the exact owned home, record counts and typed **RESET** confirmation. **Reset and close** removes the home's protected credentials first, closes the desktop/service, then removes all owned files and browser state offline. Reopening creates an empty workspace. Originals, exports and independent accounts outside the owned home are preserved. No hidden backup is made.

The feature checks both native and backend support before confirmation. Busy work, stale previews and uncertain responses have explicit repair/retry paths. Once accepted, normal Tender work is gated until cleanup completes. Credential inventory is durably recorded before deletion, never exposes secret values, and is checked again against the home's ownership on retry. Native cleanup retains coordination through failure publication and does not traverse links.

## Verification

- Full UI suite: **147 passed**, 33 files. A subsequently added App acceptance/cache-clear regression and final reset UI changes passed in the focused **21-test** suite. TypeScript checking passed against regenerated Pydantic bindings.
- Affected backend suite: **114 passed, 1 skipped** across reset, credentials, API, backup, plan review and pending conversation. The skipped backup test requires symlink creation unavailable to its Windows fixture; reset-native link tests run separately. One installed Starlette/AnyIO deprecation warning remains.
- Updated an obsolete API secrecy test to create a synthetic named AI connection through the current API, verify no secret in account/settings/health/reset-preview responses, and verify rejected legacy credential fields are not echoed. No compatibility API was added.
- Windows native cleanup and Node preflight tests cover real Python FileLock contention, all data categories, neighboring originals, read-only hardlinks, root/journal/child links, concurrent helpers, interruption and explicit retry. A real synthetic Repository is populated, purged by native code, then reopened with zero Tenders/accounts and default preferences/currency.
- Actual Quantix desktop: opened Settings and the review dialog in light and dark mode; verified the real home and counts, disabled final action before typed confirmation, visible focus, hover contrast, and Escape cancellation. Initial window was approximately 1440×1000. Global Windows display scaling was not changed for this reset-only acceptance.
- Live preview showed **1 Tender, 20 imported copies, 3 AI accounts and 2 backups**. The actual workspace was not reset; no real credential deletion, Tender approval, commercial sending, or provider request was performed by this verification.

Native development compilation is permitted and was used. No release package, commit or push was made. See [the user guide](../factory-reset.md), [design](../design/factory-reset.md) and [contracts](../contracts.md#factory-reset-2026-09-09).

Final native hardening passed independent root verification: **11 native tests and 4 Node preflight tests**. Pinned directories reject competing write handles while permitting ordinary child files; journal replacement stays atomic through a same-parent native rename anchored to the already-open file. The independent reviewer found no new issues in this delta. The development executable was rebuilt and reopened successfully.

Final live read-only checks confirmed support is available, the preview remains stable after ordinary Settings reads, no blockers are reported, reset status is null and no pending reset journal exists. Counts remain 1 Tender / 20 copies / 3 accounts / 2 backups. Frontend typecheck was rerun successfully. The separate blocked temporary directory still exists.
