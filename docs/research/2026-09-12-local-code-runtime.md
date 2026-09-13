# Local code runtime evidence — 12 September 2026

This records implementation evidence, not completion of full-container acceptance.

## Inspected official interfaces

- [Monty Python quickstart](https://pydantic.dev/docs/monty/quickstart/python/) and [resource limits](https://pydantic.dev/docs/monty/limitations/resource_limits/): current `pydantic-monty==0.0.23` pins client/runtime 0.0.23. `AsyncMonty` owns subprocess workers; explicit `binary_path`, one process, one checkout, per-request timeout and per-session memory/time/suspension limits are used. Nothing is mounted and no host objects or OS callbacks are supplied. Tools remain ordinary Quantix `ToolDefinition.invoke` callbacks with trusted invocation identities.
- [Podman 6.1.1 release](https://github.com/podman-container-tools/podman/releases/tag/v6.1.1): Windows x64 portable archive SHA256 `68766f21aebec379ec34cfee46d0550b025ec6d79c02fbdcb61a80bc7191ef01` was read from official release metadata, downloaded and verified. The executable reports `podman version 6.1.1`.
- [Podman machine init](https://docs.podman.io/en/latest/markdown/podman-machine-init.1.html): private XDG configuration/data/cache, connection configuration, HOME and temporary directories are beneath Quantix home. `[machine] volumes=[]` avoids the default home mount. Windows WSL host drives are not mounted into containers. Containers receive only a named volume of copied, selected inputs mounted read-only; no host bind mount or Podman socket is exposed.
- [Podman run](https://docs.podman.io/en/latest/markdown/podman-run.1.html): disposable rootless containers use network none, read-only root, uid 1000, dropped capabilities, no-new-privileges, CPU/memory/process/output/time bounds and server-side container timeout. Cancellation also explicitly removes both execution/input containers and their named volume.
- [Official Python image](https://hub.docker.com/_/python): the Docker Registry manifest for `3.12.12-slim-bookworm` was inspected. Multiarchitecture OCI index SHA256 `593bd06efe90efa80dc4eee3948be7c0fde4134606dd40d8dd8dbcade98e669c` is pinned. Its Linux amd64 manifest is `2986c55feb36e6cae00fa1fefb454283e4b33f35e75ff8bdd123b134130be301`; arm64 is `228390eced221ad2986a3dd77e10b1accca6ba8ceb36f1572de1b346a337cc1a`.

The initial analysis image installs numpy 2.2.6, pandas 2.2.3, openpyxl 3.1.5, python-docx 1.2.0 and matplotlib 3.10.3. Setup inspects the resulting immutable image ID and all installed distribution versions, validates rootless/capability/network/cgroup/read-only observations, and records them. Each execution receipt retains the image, library versions, exact code/hash, input IDs/versions/hashes, limits and output hashes. Rebuilding requires current reviewed tool/runtime authority; the service does not mint that authority.

## Observed locally

Real Monty finite async tool composition, filesystem/module/unknown-tool rejection, memory limits and current-authority callbacks pass on Windows. Bounded subprocess output, timeout and cancellation pass; Windows process trees use Job Objects for abnormal termination. Synthetic fixtures verify Python input hash validation, preservation, unsafe output rejection and forced cleanup. API tests verify Tender-scoped receipts and always-download output handling, including HTML. Settings tests exercise prerequisite recovery and explicit private-machine removal.

The private Podman executable lives under `~/.quantix/code/podman/software/6.1.1`. Actual `wsl.exe --status` failed its prerequisite check. The application therefore reports `prerequisite_required`; no machine/image build or arbitrary full Python code has run on this host. No Windows feature was enabled, restart requested, or unrelated Podman installation changed.

## Integration and remaining acceptance

Use one shared `PodmanRuntime` for router actions and `PythonAnalysisService` so setup cannot race admitted runs. The trusted controller resolves source bytes through its current grant, constructs `SelectedPythonInput`, and supplies an authority callback that checks exact method/tool/runtime/limits and active run before dispatch and publication. The code service itself has no public execute endpoint, billing authority, provider credentials or source-path capability. Composition similarly receives only the controller's already-fenced tool set. Saving output does not approve engineering conclusions or mark a source inspected.

Windows WSL2 full-container execution, filesystem/network/process/resource probes and crash recovery remain unverified. AppleHV/QEMU paths are implemented through Podman machine but have not been exercised here. Private machine-image provenance and operating-system qualification must be captured during those platform checks. Real desktop light/dark/scaling checks and final integrated tool/method journeys belong to the primary acceptance pass.
