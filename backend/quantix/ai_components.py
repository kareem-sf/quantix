"""On-demand, per-user AI software with immutable versions and worker leases.

Preparation installs software only. It never signs in, contacts a model account,
or runs inference. Provider imports are confined to the selected private worker.
"""

import asyncio
import inspect
import json
import os
import shutil
import subprocess
import threading
from contextlib import asynccontextmanager, contextmanager, suppress
from pathlib import Path
from uuid import uuid4

from filelock import FileLock, Timeout

from .ai_component_manifest import (
    ComponentUnavailable,
    checked_download,
    diagnostics_writer,
    extract_archive,
    inside,
    inventory,
    manifest_file,
    native_host,
    platform_key,
    read_manifest,
    receipt_valid,
    worker_directory,
    worker_files,
    worker_fingerprint,
)
from .diagnostics import record, record_exception
from .storage import ai_components_dir


def _atomic_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}-{uuid4().hex}.tmp")
    try:
        with temporary.open("w", encoding="utf-8") as output:
            json.dump(value, output, sort_keys=True)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def _read_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
        return value if isinstance(value, dict) else {}
    except (OSError, ValueError):
        return {}


async def _blocking(function, *args, cancelled):
    """Do not delete an installation while its cancelled download thread writes."""
    task = asyncio.create_task(asyncio.to_thread(function, *args))
    try:
        return await asyncio.shield(task)
    except asyncio.CancelledError:
        cancelled.set()
        # Repeated Cancel clicks must not release ownership of a thread that
        # still writes into the candidate directory.
        while not task.done():
            try:
                await asyncio.shield(task)
            except asyncio.CancelledError:
                cancelled.set()
            except Exception:
                break
        with suppress(Exception, asyncio.CancelledError):
            task.result()
        raise


def _deep_receipt_valid(path):
    """Run the expensive immutable receipt hash check in a worker thread."""
    return receipt_valid(path, deep=True)


async def _finish_cleanup(operation):
    task = asyncio.create_task(operation)
    interrupted = False
    while not task.done():
        try:
            await asyncio.shield(task)
        except asyncio.CancelledError:
            interrupted = True
    task.result()
    if interrupted:
        raise asyncio.CancelledError


@asynccontextmanager
async def _acquire(lock: FileLock):
    while True:
        try:
            lock.acquire(timeout=0)
            break
        except Timeout:
            await asyncio.sleep(0.1)
    try:
        yield
    finally:
        lock.release()


class AIComponentService:
    def __init__(self, repo):
        self.repo = repo
        # Private software belongs to the selected workspace. Account homes
        # remain separate under that workspace's ai-runtimes directory.
        self.root = ai_components_dir(repo.home).resolve()

    def component_id(self, connection: dict) -> str:
        protocol = connection.get("protocol")
        if protocol in {"openai_chat", "openai_responses"}:
            return "api-openai-azure" if connection.get("auth_type") == "azure_identity" else "api-openai"
        if protocol == "anthropic" and connection.get("auth_type") == "azure_identity":
            return "api-anthropic-azure"
        component = {"anthropic": "api-anthropic", "google": "api-google",
                     "bedrock": "api-bedrock", "mistral": "api-mistral", "cohere": "api-cohere",
                     "codex": "client-codex", "copilot": "client-copilot",
                     "claude_agent": "client-claude", "claude_code": "client-claude",
                     "gemini_cli": "client-gemini", "grok_build": "client-grok"}.get(protocol)
        if component is None:
            raise ComponentUnavailable("This AI connection has no supported software component.")
        return component

    def _component(self, connection):
        identifier = self.component_id(connection)
        manifest, target = read_manifest(), platform_key()
        definition = manifest["components"].get(identifier)
        if not definition or target not in definition.get("locks", {}):
            raise ComponentUnavailable("The selected AI software has no supported binary installation for this operating system and processor.")
        lock = manifest_file(definition["locks"][target])
        # Source files are copied as data; no worker wheel build is required.
        worker_hash = worker_fingerprint()
        import hashlib
        fingerprint = hashlib.sha256(json.dumps({"component": definition, "platform": manifest["platforms"][target],
            "worker": worker_hash, "python": manifest["python_version"], "build": manifest["python_build"],
            "revision": manifest["revision"]}, sort_keys=True).encode()).hexdigest()
        return identifier, manifest, definition, target, lock, fingerprint

    def _directory(self, identifier):
        return self.root / "components" / identifier

    def _guard(self, identifier):
        directory = self.root / "locks"
        directory.mkdir(parents=True, exist_ok=True)
        return FileLock(directory / f"{identifier}.lock")

    def _active(self, identifier, fingerprint, *, deep=False):
        directory = self._directory(identifier)
        record = _read_json(directory / "active.json")
        version = record.get("version")
        if not isinstance(version, str) or record.get("fingerprint") != fingerprint:
            return None
        path = inside(directory / "versions", version)
        receipt = _read_json(path / "receipt.json")
        python_root = inside(self.root, receipt.get("python_root", ""))
        if receipt.get("fingerprint") != fingerprint or not receipt_valid(path, deep=deep, allowed_roots=(python_root,)):
            return None
        if not receipt_valid(python_root, deep=deep):
            return None
        return path

    def _status(self, identifier, state, detail, progress=None, version=None):
        return {"component_id": identifier, "state": state, "detail": detail,
                "progress": progress, "version": version}

    def status(self, connection) -> dict:
        identifier = self.component_id(connection)
        try:
            _, _, _, _, _, fingerprint = self._component(connection)
            native_host()
            directory = self._directory(identifier)
            previous = _read_json(directory / "state.json")
            if previous.get("state") == "preparing":
                try:
                    with self._guard(identifier).acquire(timeout=0):
                        pass
                except Timeout:
                    return self._status(identifier, "preparing", previous.get("detail", "Preparing selected AI software."), previous.get("progress"))
            active = self._active(identifier, fingerprint)
            if active:
                return self._status(identifier, "ready", "The selected AI software is installed. Account access is checked separately.", 100, active.name)
            if previous.get("state") in {"attention", "preparing"}:
                return self._status(identifier, "attention", previous.get("detail") if previous.get("state") == "attention" else "AI software preparation was interrupted. Select Prepare to continue.")
            if (directory / "active.json").exists():
                return self._status(identifier, "attention", "The selected AI software needs preparation or repair before it can be used.")
            return self._status(identifier, "missing", "Prepare the software for this AI connection when you need it.")
        except (ComponentUnavailable, OSError, KeyError, TypeError) as exc:
            detail = str(exc) if isinstance(exc, ComponentUnavailable) else "Quantix could not read the selected AI software installation. Repair its software."
            return self._status(identifier, "attention", detail)

    def _environment(self, home: Path) -> dict[str, str]:
        allowed = {"SYSTEMROOT", "WINDIR", "SYSTEMDRIVE", "COMSPEC", "NUMBER_OF_PROCESSORS",
                   "PROCESSOR_ARCHITECTURE", "LANG", "LC_ALL", "SSL_CERT_FILE", "SSL_CERT_DIR",
                   "HTTPS_PROXY", "HTTP_PROXY", "NO_PROXY"}
        env = {key: value for key, value in os.environ.items() if key.upper() in allowed}
        for name in ("tmp", "config", "data", "cache"):
            (home / name).mkdir(parents=True, exist_ok=True)
        system_path = str(Path(os.environ.get("SYSTEMROOT", "C:/Windows")) / "System32") if os.name == "nt" else "/usr/bin:/bin"
        env.update({"HOME": str(home), "USERPROFILE": str(home), "APPDATA": str(home / "config"),
                    "LOCALAPPDATA": str(home / "data"), "XDG_CONFIG_HOME": str(home / "config"),
                    "XDG_DATA_HOME": str(home / "data"), "XDG_CACHE_HOME": str(home / "cache"),
                    "TEMP": str(home / "tmp"), "TMP": str(home / "tmp"), "PATH": system_path,
                    "UV_NO_CONFIG": "1", "UV_NO_PROGRESS": "1", "UV_PYTHON_DOWNLOADS": "never",
                    "UV_PYTHON_INSTALL_REGISTRY": "0", "UV_PYTHON_NO_REGISTRY": "1",
                    "UV_PYTHON_INSTALL_DIR": str(self.root / "python"), "UV_PYTHON_BIN_DIR": str(self.root / "bin"),
                    "UV_CACHE_DIR": str(self.root / "cache" / "uv"), "UV_LINK_MODE": "copy",
                    "UV_INDEX_URL": "https://pypi.org/simple", "PIP_CONFIG_FILE": os.devnull,
                    "PYTHONDONTWRITEBYTECODE": "1", "PYTHONNOUSERSITE": "1", "PYTHONUTF8": "1",
                    "COPILOT_SKIP_CLI_DOWNLOAD": "1", "COPILOT_AUTO_UPDATE": "false",
                    "DISABLE_TELEMETRY": "1", "DO_NOT_TRACK": "1"})
        return env

    async def _run(self, command, *, cwd, env, seconds=1200):
        started = asyncio.get_running_loop().time()
        record("ai_installer_process", phase="installer", outcome="started")
        host = native_host()
        options = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {}
        process = await asyncio.create_subprocess_exec(str(host), "--", *map(str, command), cwd=cwd, env=env,
            stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.PIPE, **options)
        # Drain diagnostics with a fixed bound. Never persist paths/account text.
        async def discard():
            while await process.stderr.read(64 * 1024):
                pass
        drain = asyncio.create_task(discard())
        try:
            async with asyncio.timeout(seconds):
                code = await process.wait()
            record("ai_installer_process", phase="installer", outcome="completed",
                   exit_code=code, duration_ms=int((asyncio.get_running_loop().time() - started) * 1000))
            if code:
                raise ComponentUnavailable("AI software preparation could not finish. Check the network and available storage, then retry.")
        except BaseException as error:
            if not isinstance(error, asyncio.CancelledError):
                record_exception("ai_installer_process_failed", error, phase="installer",
                                 duration_ms=int((asyncio.get_running_loop().time() - started) * 1000))
            raise
        finally:
            async def shutdown():
                if process.stdin:
                    process.stdin.close()  # Host observes EOF and owns every child.
                if process.returncode is None:
                    with suppress(asyncio.TimeoutError):
                        await asyncio.wait_for(asyncio.shield(process.wait()), 5)
                    if process.returncode is None:
                        process.terminate()
                        with suppress(asyncio.TimeoutError):
                            await asyncio.wait_for(asyncio.shield(process.wait()), 5)
                    if process.returncode is None:
                        process.kill()
                        await process.wait()
                drain.cancel()
                await asyncio.gather(drain, return_exceptions=True)
            await _finish_cleanup(shutdown())

    def _remove_directory(self, path):
        resolved = path.resolve()
        if resolved == self.root or not resolved.is_relative_to(self.root):
            raise ComponentUnavailable("The AI software removal path is outside its private directory.")
        if resolved.exists():
            shutil.rmtree(resolved)

    async def _archive(self, asset, destination, cancelled, *, allow_links=True):
        destination.mkdir(parents=True, exist_ok=True)
        archive = destination.parent / f"download-{uuid4().hex}.archive"
        try:
            await _blocking(checked_download, asset, archive, cancelled, cancelled=cancelled)
            await _blocking(extract_archive, archive, destination, cancelled, allow_links, cancelled=cancelled)
        finally:
            archive.unlink(missing_ok=True)

    async def _base(self, manifest, target, cancelled):
        async with _acquire(self._guard("shared-runtimes")):
            result = {}
            for kind in ("uv", "python"):
                asset = manifest["platforms"][target][kind]
                shared = self.root / "shared"
                pointer = shared / f"{kind}-{asset['sha256'][:20]}.json"
                saved = _read_json(pointer).get("directory")
                directory = inside(shared, saved) if isinstance(saved, str) else shared / f"{kind}-{asset['sha256'][:20]}"
                if not await _blocking(_deep_receipt_valid, directory, cancelled=cancelled):
                    # A damaged Python base may be used by existing workers. Make
                    # a new immutable base instead of repairing bytes in place.
                    if directory.exists():
                        directory = directory.with_name(f"{directory.name}-{uuid4().hex}")
                    directory.mkdir(parents=True, exist_ok=False)
                    try:
                        await self._archive(asset, directory, cancelled)
                        executable = inside(directory, asset["executable"])
                        if not executable.is_file():
                            raise ComponentUnavailable("The downloaded AI runtime is missing its executable.")
                        files = await _blocking(inventory, directory, cancelled, cancelled=cancelled)
                        required = [asset["executable"]]
                        if kind == "python":
                            library = "python/Lib" if os.name == "nt" else "python/lib/python3.12"
                            required += [f"{library}/encodings/__init__.py", f"{library}/os.py"]
                            required += [path.relative_to(directory).as_posix() for path in (directory / "python").glob("*.dll")]
                        if any(name not in files for name in required):
                            raise ComponentUnavailable("The private AI runtime is missing a required library.")
                        _atomic_json(directory / "receipt.json", {"format": 1, "archive_sha256": asset["sha256"], "files": files, "required": required})
                        _atomic_json(pointer, {"directory": directory.name})
                    except BaseException:
                        self._remove_directory(directory)
                        raise
                result[kind] = (directory, inside(directory, asset["executable"]))
            return result

    async def prepare(self, connection, *, repair=False, progress=None) -> dict:
        identifier, manifest, definition, target, lock, fingerprint = self._component(connection)
        record("ai_component_prepare", phase="preparation", outcome="started", component_id=identifier,
               component_version=definition.get("version"))
        native_host()
        cancelled = threading.Event()
        async with _acquire(self._guard(identifier)):
            active = (await _blocking(lambda: self._active(identifier, fingerprint, deep=True), cancelled=cancelled)
                      if not repair else None)
            if active:
                return self._status(identifier, "ready", "The selected AI software is installed. Account access is checked separately.", 100, active.name)
            directory = self._directory(identifier)
            candidate = directory / "versions" / f"{definition['version']}-{fingerprint[:12]}-{uuid4().hex}"
            candidate.mkdir(parents=True, exist_ok=False)

            async def report(percent, detail):
                record("ai_component_phase", phase="preparation", outcome="progress",
                       component_id=identifier, progress=percent)
                value = self._status(identifier, "preparing", detail, percent, candidate.name)
                _atomic_json(directory / "state.json", value)
                if progress:
                    result = progress(value)
                    if inspect.isawaitable(result):
                        await result

            try:
                await report(5, "Preparing private AI runtime files.")
                base = await self._base(manifest, target, cancelled)
                env = self._environment(self.root / "installer-home")
                await report(25, "Creating the selected AI software environment.")
                # Candidate is already at its permanent path: venv scripts are
                # never relocated. No global Python/registry/shell changes occur.
                await self._run([base["uv"][1], "--no-config", "venv", "--python", base["python"][1],
                    "--no-python-downloads", candidate / "venv"], cwd=candidate, env=env)
                python = candidate / "venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
                shutil.copy2(lock, candidate / "requirements.txt")
                await report(40, "Downloading only the selected AI libraries.")
                # uv rejects --no-build combined with --only-binary. The latter
                # already requires wheels for every dependency in this hash lock.
                await self._run([base["uv"][1], "--no-config", "pip", "sync", "requirements.txt", "--python", python,
                    "--require-hashes", "--only-binary", ":all:",
                    "--index-url", "https://pypi.org/simple"], cwd=candidate, env=env)
                if definition.get("npm"):
                    await report(65, "Preparing the official client and its private Node runtime.")
                    node_asset = manifest["platforms"][target]["node"]
                    expanded = candidate / "node-download"
                    await self._archive(node_asset, expanded, cancelled)
                    inside(expanded, node_asset["root"]).rename(candidate / "node")
                    self._remove_directory(expanded)
                    node = inside(candidate / "node", node_asset["executable"])
                    npm = candidate / "node" / ("node_modules/npm/bin/npm-cli.js" if os.name == "nt" else "lib/node_modules/npm/bin/npm-cli.js")
                    if not node.is_file() or not npm.is_file():
                        raise ComponentUnavailable("The official Node archive is missing its client installer.")
                    package = definition["npm"]
                    client = candidate / package["directory"]
                    client.mkdir()
                    shutil.copy2(manifest_file(package, "lock", "lock_sha256"), client / "package-lock.json")
                    shutil.copy2(manifest_file(package, "package", "package_sha256"), client / "package.json")
                    env["PATH"] = str(node.parent) + os.pathsep + env["PATH"]
                    env["npm_config_cache"] = str(self.root / "cache" / "npm")
                    env["npm_config_update_notifier"] = "false"
                    user_config = self.root / "installer-home" / "user.npmrc"
                    global_config = self.root / "installer-home" / "global.npmrc"
                    user_config.write_text("", encoding="utf-8")
                    global_config.write_text("", encoding="utf-8")
                    npm_command = [node, npm, "ci", "--ignore-scripts", "--no-audit", "--no-fund", "--no-bin-links",
                                   "--userconfig", user_config, "--globalconfig", global_config,
                                   "--registry", "https://registry.npmjs.org/"]
                    if package.get("omit_optional"):
                        npm_command.append("--omit=optional")
                    await self._run(npm_command, cwd=client, env=env)
                if identifier == "client-copilot":
                    await report(80, "Downloading the pinned Copilot analysis runtime.")
                    asset = manifest["platforms"][target]["copilot"]
                    expanded = candidate / "copilot-download"
                    await self._archive(asset, expanded, cancelled)
                    package_root = inside(expanded, asset["root"])
                    destination = candidate / "runtime" / manifest["copilot_version"] / "prebuilds" / asset["platform"]
                    # The SDK's hostless layout puts retained package assets
                    # beside runtime.node and the native wrapper. Materialize
                    # the complete reviewed platform package there, with no
                    # provider downloader and no mutable account cache involved.
                    shutil.copytree(package_root, destination, ignore=shutil.ignore_patterns("prebuilds"))
                    shutil.copytree(package_root / "prebuilds" / asset["platform"], destination, dirs_exist_ok=True)
                    (destination / ".hostless-runtime-assets-v2").write_text("1\n", encoding="ascii")
                    self._remove_directory(expanded)
                native_client = None
                if identifier == "client-grok":
                    await report(75, "Downloading the official Grok client for this computer.")
                    package = definition["native"]
                    asset = package["assets"][target]
                    expanded = candidate / "grok-download"
                    await self._archive(asset, expanded, cancelled, allow_links=False)
                    package_root = inside(expanded, asset["root"])
                    metadata = _read_json(package_root / "package.json")
                    if (metadata.get("name") != asset["package"] or metadata.get("version") != definition["version"]
                            or metadata.get("os") != [asset["os"]] or metadata.get("cpu") != [asset["cpu"]]):
                        raise ComponentUnavailable("The downloaded Grok package does not match the selected version and computer.")
                    client = inside(candidate, package["directory"])
                    package_root.rename(client)
                    self._remove_directory(expanded)
                    compressed = inside(client, asset["compressed_executable"])
                    executable = inside(client, asset["executable"])
                    if not compressed.is_file() or compressed.stat().st_size == 0 or executable.exists():
                        raise ComponentUnavailable("The official Grok package is missing its expected compressed client.")
                    await report(85, "Unpacking the official Grok client in its private software folder.")
                    # Quantix's own bounded native decoder handles the publisher's
                    # Brotli payload. No Node, npm script, PATH entry or Grok
                    # command is needed to prepare this immutable installation.
                    await self._run([native_host(), "--decompress-brotli", compressed, executable,
                                     str(600 * 1024 * 1024)], cwd=candidate, env=env, seconds=180)
                    if not executable.is_file() or executable.stat().st_size == 0:
                        raise ComponentUnavailable("The official Grok client could not be unpacked. Repair its software.")
                    if os.name != "nt":
                        executable.chmod(0o755)
                    compressed.unlink()
                    native_client = {"version": definition["version"], "package": asset["package"],
                                     "integrity": asset["integrity"], "executable": executable.relative_to(candidate).as_posix()}
                await report(90, "Recording installed files and preparing the AI worker.")
                worker = candidate / "worker"
                for source in worker_files():
                    destination = worker / source.relative_to(worker_directory())
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source, destination)
                # The service owns the canonical dependency-free writer. It is
                # copied as data into the isolated worker and included in the
                # worker fingerprint so every prepared receipt binds it.
                writer_destination = worker / "quantix_ai_worker" / "diagnostics.py"
                writer_destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(diagnostics_writer(), writer_destination)
                required = self._required_files(identifier, candidate, manifest, target)
                required += [path.relative_to(candidate).as_posix() for path in worker.rglob("*.py")]
                files = await _blocking(inventory, candidate, cancelled, cancelled=cancelled)
                if self._component(connection)[-1] != fingerprint:
                    raise ComponentUnavailable("Quantix's AI software files changed during preparation. Select Prepare again.")
                _atomic_json(candidate / "receipt.json", {"format": 1, "fingerprint": fingerprint,
                    "python_root": base["python"][0].relative_to(self.root).as_posix(), "files": files, "required": required,
                    **({"native_client": native_client} if native_client else {})})
                # Final synchronous activation has no cancellation gap after it.
                _atomic_json(directory / "active.json", {"version": candidate.name, "fingerprint": fingerprint})
                result = self._status(identifier, "ready", "The selected AI software is installed. Account access is checked separately.", 100, candidate.name)
                _atomic_json(directory / "state.json", result)
                record("ai_component_prepare", phase="preparation", outcome="completed",
                       component_id=identifier, component_version=definition.get("version"), progress=100)
                return result
            except BaseException as exc:
                cancelled.set()
                if _read_json(directory / "active.json").get("version") != candidate.name:
                    self._remove_directory(candidate)
                detail = "AI software preparation was interrupted. Select Prepare to continue." if isinstance(exc, (asyncio.CancelledError, InterruptedError)) else (str(exc) if isinstance(exc, ComponentUnavailable) else "AI software preparation could not finish. Check the network and available storage, then retry.")
                _atomic_json(directory / "state.json", self._status(identifier, "attention", detail))
                if not isinstance(exc, asyncio.CancelledError):
                    record_exception("ai_component_prepare_failed", exc, phase="preparation",
                                     component_id=identifier, component_version=definition.get("version"))
                if isinstance(exc, (asyncio.CancelledError, InterruptedError)):
                    raise
                raise ComponentUnavailable(detail) from None

    def _required_files(self, identifier, directory, manifest, target):
        python = directory / "venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        required = [python, directory / "worker" / "worker_entry.py"]
        site = directory / "venv" / ("Lib/site-packages" if os.name == "nt" else "lib/python3.12/site-packages")
        required += [site / package / "__init__.py" for package in ("mcp", "pydantic", "pydantic_core", "httpx2", "jsonschema")]
        if identifier.startswith("api-"):
            required += [site / "pydantic_ai" / "__init__.py", site / "pydantic_graph" / "__init__.py"]
            package = {"api-openai": "openai", "api-openai-azure": "openai", "api-anthropic": "anthropic",
                       "api-anthropic-azure": "anthropic", "api-google": "google/genai", "api-bedrock": "boto3", "api-mistral": "mistralai/client", "api-cohere": "cohere"}[identifier]
            required.append(site / package / "__init__.py")
            if identifier in {"api-openai-azure", "api-anthropic-azure"}:
                required.append(site / "azure/identity/__init__.py")
        if identifier == "client-codex":
            required += [site / "openai_codex" / "__init__.py"]
            # The publisher wheel owns the platform-specific binary location.
            binaries = list((site / "codex_cli_bin").rglob("codex.exe" if os.name == "nt" else "codex"))
            if not any(path.is_file() for path in binaries):
                raise ComponentUnavailable("The selected Codex wheel is missing its original client binary.")
            required += [path for path in binaries if path.is_file()]
        elif identifier == "client-claude":
            required += [site / "claude_agent_sdk" / "__init__.py", site / "claude_agent_sdk" / "_bundled" / ("claude.exe" if os.name == "nt" else "claude")]
        elif identifier == "client-copilot":
            pair = directory / "runtime" / manifest["copilot_version"] / "prebuilds" / manifest["platforms"][target]["copilot"]["platform"]
            native_package = directory / "account-client" / "node_modules" / "@github" / f"copilot-{manifest['platforms'][target]['copilot']['platform']}"
            required += [site / "copilot" / "__init__.py", pair / ("copilot-runtime.exe" if os.name == "nt" else "copilot-runtime"), pair / "runtime.node", pair / ".hostless-runtime-assets-v2",
                         native_package / ("copilot.exe" if os.name == "nt" else "copilot"),
                         directory / "account-client" / "node_modules" / "@github" / "copilot" / "npm-loader.js"]
        elif identifier == "client-gemini":
            required += [directory / "client" / "node_modules" / "@google" / "gemini-cli" / "bundle" / "gemini.js"]
        elif identifier == "client-grok":
            native = manifest["components"][identifier]["native"]
            client = inside(directory, native["directory"])
            required += [inside(client, native["assets"][target]["executable"]),
                         client / "package.json", client / "THIRD_PARTY_NOTICES.md"]
        if not all(path.is_file() and path.stat().st_size for path in required):
            raise ComponentUnavailable("The selected AI software is missing a required installed file. Repair its software.")
        if identifier in {"client-copilot", "client-gemini"}:
            required.append(directory / "node" / ("node.exe" if os.name == "nt" else "bin/node"))
        return [path.relative_to(directory).as_posix() for path in required]

    def worker_command(self, connection) -> list[str]:
        identifier, _, _, _, _, fingerprint = self._component(connection)
        directory = self._active(identifier, fingerprint, deep=True)
        if directory is None:
            # Name which check refused the installed version: a stale build and
            # a damaged one both reach the same message, and without this the
            # cause can only be found by recomputing fingerprints by hand.
            installed = _read_json(self._directory(identifier) / "active.json").get("fingerprint")
            record(
                "ai_component_unavailable",
                level="warning",
                phase="worker_command",
                outcome="stale_version" if installed and installed != fingerprint else "unverified",
                component_id=identifier,
                connection_id=connection.get("id"),
                protocol=connection.get("protocol"),
            )
            raise ComponentUnavailable("Prepare or repair the selected AI software before using this connection.")
        python = directory / "venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        return [str(native_host()), "--", str(python.absolute()), "-I", "-B", str(directory / "worker" / "worker_entry.py")]

    def environment(self, connection, command=None) -> dict[str, str]:
        # A leased command pins the exact version even if a repair activates a new
        # version while this worker runs. Do not reread active.json for that case.
        command = command or self.worker_command(connection)
        directory = Path(command[-1]).parent.parent
        if not directory.is_relative_to(self._directory(self.component_id(connection)) / "versions"):
            raise ComponentUnavailable("The AI worker command does not belong to this connection's selected software.")
        values = {"QUANTIX_AI_COMPONENT_ROOT": str(directory), "QUANTIX_COMPONENT_MANAGED": "1",
                  "COPILOT_SKIP_CLI_DOWNLOAD": "1", "COPILOT_AUTO_UPDATE": "false"}
        node = directory / "node" / ("node.exe" if os.name == "nt" else "bin/node")
        if node.is_file():
            values["QUANTIX_NODE_BINARY"] = str(node)
        if self.component_id(connection) == "client-grok":
            native = _read_json(directory / "receipt.json").get("native_client", {})
            executable = native.get("executable")
            version = native.get("version")
            if not isinstance(executable, str) or not isinstance(version, str):
                raise ComponentUnavailable("The Grok software receipt is missing its client version. Repair its software.")
            values.update({"QUANTIX_GROK_BINARY": str(inside(directory, executable)), "QUANTIX_GROK_VERSION": version})
        return values

    @contextmanager
    def lease(self, connection):
        identifier = self.component_id(connection)
        try:
            with self._guard(identifier).acquire(timeout=0):
                command = self.worker_command(connection)
                version = Path(command[-1]).parent.parent.name
                folder = self.root / "leases" / identifier / version
                folder.mkdir(parents=True, exist_ok=True)
                path = folder / f"{uuid4().hex}.lock"
                lock = FileLock(path)
                lock.acquire(timeout=0)
        except Timeout:
            raise ComponentUnavailable("The selected AI software is being prepared or removed. Wait for it to finish.") from None
        try:
            yield command
        finally:
            lock.release()
            path.unlink(missing_ok=True)

    @asynccontextmanager
    async def async_lease(self, connection):
        """Lease a verified worker version without hashing it on the event loop."""
        identifier = self.component_id(connection)
        cancelled = threading.Event()
        lock = None
        path = None
        try:
            # Keep the component guard around command selection and the deep
            # receipt check so preparation/removal cannot race verification.
            async with _acquire(self._guard(identifier)):
                command = await _blocking(
                    lambda: self.worker_command(connection),
                    cancelled=cancelled,
                )
                version = Path(command[-1]).parent.parent.name
                folder = self.root / "leases" / identifier / version
                folder.mkdir(parents=True, exist_ok=True)
                path = folder / f"{uuid4().hex}.lock"
                lock = FileLock(path)
                lock.acquire(timeout=0)
        except Timeout:
            if lock is not None:
                lock.release()
            if path is not None:
                path.unlink(missing_ok=True)
            raise ComponentUnavailable("The selected AI software is being prepared or removed. Wait for it to finish.") from None
        try:
            yield command
        finally:
            if lock is not None:
                lock.release()
            if path is not None:
                path.unlink(missing_ok=True)

    def _leased(self, identifier, version):
        for path in (self.root / "leases" / identifier / version).glob("*.lock"):
            try:
                with FileLock(path).acquire(timeout=0):
                    pass
                path.unlink(missing_ok=True)
            except Timeout:
                return True
        return False

    def remove(self, connection) -> dict:
        identifier = self.component_id(connection)
        try:
            with self._guard(identifier).acquire(timeout=0):
                directory = self._directory(identifier)
                versions = list((directory / "versions").glob("*"))
                if any(self._leased(identifier, path.name) for path in versions):
                    raise ComponentUnavailable("This AI software is still in use. Stop its active work before removing it.")
                (directory / "active.json").unlink(missing_ok=True)
                for path in versions:
                    self._remove_directory(path)
                value = self._status(identifier, "missing", "The shared AI software was removed. Private connection sign-in files were kept.")
                _atomic_json(directory / "state.json", value)
                return value
        except Timeout:
            raise ComponentUnavailable("This AI software is being prepared. Cancel preparation before removing it.") from None
