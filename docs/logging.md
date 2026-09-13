# Finding a Quantix error

Quantix's technical diagnostics belong in `~/.quantix/logs`. On this Windows account that is `C:\Users\kareem\.quantix\logs`. The database, preserved imported copies, AI software/profiles and other managed state share the parent `~/.quantix` home. Supplied original packages remain in their user-chosen locations. See the [unified storage contract](unified-storage.md).

Open **Settings → Technical details** to see the actual log directory and whether the local service is recording. When an action shows a reference, keep that reference and the approximate time. A connection setup error may have an error reference; HTTP errors have a request reference. Both are searchable in the JSONL files.

## Find an operation

PowerShell example, using a reference copied from the error:

```powershell
$quantixLogPath = Join-Path $env:USERPROFILE '.quantix/logs'
$quantixReference = 'paste-the-reference-here'
Get-ChildItem -LiteralPath $quantixLogPath -File -Filter '*.jsonl*' |
  Where-Object { $_.Name -notlike '*.lock' } |
  Select-String -SimpleMatch $quantixReference
```

Start with the matching failure, then follow its request, operation and session identifiers. Compare timestamps and process identifiers for desktop/launcher events. The service and isolated AI workers write separate process files so they can rotate independently.

Useful evidence includes the setup phase, provider protocol, software version, selected/reported model, number of model rounds, terminal reason, process exit, HTTP status, elapsed time, and bounded exception classes/code locations. Not every event has every field. An absent value is unknown, not zero or success.

## Grok troubleshooting

1. **Prepare** concerns installed software. A valid software receipt does not prove sign-in or model access.
2. **Connect** concerns account authentication and available model metadata. Do not repeat sign-in for a tool-exchange failure unless the error establishes an account problem.
3. **Check** sends a generic sample through the real model/tool exchange. Inspect the terminal result and check evidence. A model response alone does not establish a passed check.
4. **Ready** applies to the checked account revision, model and software version. Tender selection/data authority and ordinary engineering work remain separate.

For the reproduced 9 September failure, discovery was blocked by a permission rule before the actual check tool ran. See [the specific repair](grok-repair-2026-09-09.md). Do not solve future failures by globally allowing native tools, increasing spending authority or accepting an unknown model identity.

## What the files contain

These are technical event records. They intentionally omit raw prompts, Tender text, customer file names, email content, tokens, credentials, request bodies, provider response bodies, sign-in codes/URLs and unrestricted stdout/stderr. Exception records retain code structure rather than raw exception messages, source lines or local variables. The normal Tender history retains its existing business records separately.

Original AI clients can maintain their own private session files inside the account runtime. Those are not the sanitized Quantix diagnostic files and should not be included blindly in support material. No automatic upload or remote telemetry is part of this system.

## Retention and failure behavior

Process files rotate at 5 MiB with two rotated files retained per process. Completed owned logs are pruned using a 14-day retention period and approximately 100 MiB retention target; active writers are preserved. This is bounded diagnostic history, not an unlimited audit archive. Keep a relevant diagnostic excerpt before routine retention removes it.

If a log directory cannot be written, diagnostic recording must fail without stopping Tender work. The service reports recording unavailable. A process killed by the operating system may not write its final event. A complete project backup and a diagnostic log serve different purposes.

## Acceptance status

The precise source-review, targeted diagnostic probes and live connection result for this increment are recorded in [progress](progress.md) and the [implementation record](superpowers/plans/2026-09-09-diagnostics-and-ai-repair.md). Do not infer full MVP verification from the presence of a log file.
