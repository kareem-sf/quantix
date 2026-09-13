"""Quantix-owned Podman machine; no access to a user's other connections.

The only container mounts are per-run named volumes. In particular WSL's
automatic host-drive mounts are never mounted in the container namespace.
"""

from __future__ import annotations

import asyncio
import importlib.metadata
import io
import json
import os
import platform
import re
import shutil
import urllib.request
import zipfile
from contextlib import asynccontextmanager
from pathlib import Path

from .code_composition import monty_binary
from .python_analysis_models import SandboxStatus
from .sandbox_protocol import CommandRunner, SandboxFailure, sha256, write_record

PODMAN_VERSION = "6.1.1"
PODMAN_ZIP_SHA256 = "68766f21aebec379ec34cfee46d0550b025ec6d79c02fbdcb61a80bc7191ef01"
PODMAN_URL = (
    "https://github.com/podman-container-tools/podman/releases/download/"
    "v6.1.1/podman-remote-release-windows_amd64.zip"
)
BASE_IMAGE = (
    "docker.io/library/python@sha256:"
    "593bd06efe90efa80dc4eee3948be7c0fde4134606dd40d8dd8dbcade98e669c"
)
LIBRARIES = {
    "numpy": "2.2.6",
    "pandas": "2.2.3",
    "openpyxl": "3.1.5",
    "python-docx": "1.2.0",
    "matplotlib": "3.10.3",
}
MACHINE_NAME = "quantix-python"
IMAGE_TAG = "localhost/quantix-python:1"
IMAGE_ID = re.compile(r"(?:sha256:)?[0-9a-f]{64}")


class PodmanRuntime:
    def __init__(self, home: Path, *, runner=None, executable: Path | None = None):
        self.home = Path(home).resolve()
        self.root = self.home / "code" / "podman"
        self.runner = runner or CommandRunner()
        self._executable = executable
        self.lock = asyncio.Lock()
        self.active_runs = 0
        self.operations: set[asyncio.Task] = set()
        self._operation_depth: dict[asyncio.Task, int] = {}
        self.closing = False

    @asynccontextmanager
    async def operation(self):
        if self.closing:
            raise SandboxFailure(
                "Local Python is shutting down. Reopen Quantix before starting work."
            )
        if self.lock.locked():
            raise SandboxFailure(
                "Wait for runtime setup or maintenance to finish before starting work."
            )
        task = asyncio.current_task()
        self.operations.add(task)
        self._operation_depth[task] = self._operation_depth.get(task, 0) + 1
        try:
            yield
        finally:
            self._operation_depth[task] -= 1
            if self._operation_depth[task] == 0:
                self._operation_depth.pop(task)
                self.operations.discard(task)

    @asynccontextmanager
    async def maintenance(self):
        if self.closing or self.lock.locked() or self.active_runs or self.operations:
            raise SandboxFailure("Wait for all active runtime work to finish before maintenance.")
        async with self.lock:
            task = asyncio.current_task()
            self.operations.add(task)
            try:
                yield
            finally:
                self.operations.discard(task)

    async def close(self):
        """Drain owners before private-machine cleanup; never touch other instances."""
        self.closing = True
        pending = [task for task in self.operations if task is not asyncio.current_task()]
        for task in pending:
            task.cancel()
        if pending:
            await asyncio.gather(*pending, return_exceptions=True)
        await self.runner.close()
        if self.executable() and (self.root / "owner.json").is_file():
            await self.cleanup_owned_runs()

    async def remove_for_reset(self):
        """Unregister our VM before its disk directory is removed by reset."""
        if self.active_runs or self.lock.locked() or self.operations:
            raise SandboxFailure("Stop active code work before resetting Quantix.")
        if (self.root / "owner.json").is_file():
            await self.action("remove")
        await self.close()

    async def cleanup_owned_runs(self):
        """Reconcile only names carrying our label in our private connection."""
        probe = await self.command("machine", "inspect", MACHINE_NAME, check=False)
        if probe.returncode:
            return
        machine = json.loads(probe.stdout)[0]
        if machine.get("State") != "running":
            return
        try:
            containers = json.loads(
                (
                    await self.command(
                        "ps",
                        "--all",
                        "--filter",
                        "label=io.quantix.owner=python",
                        "--format=json",
                        connection=True,
                    )
                ).stdout
            )
            for container in containers:
                for name in container.get("Names", []):
                    if re.fullmatch(
                        r"quantix-(?:run-[0-9a-f]{32}|load-[0-9a-f]{32}|mcp-[0-9a-f]{32}|check)",
                        name,
                    ):
                        await self.command("rm", "--force", "--ignore", name, connection=True)
            volumes = json.loads(
                (
                    await self.command(
                        "volume",
                        "ls",
                        "--filter",
                        "label=io.quantix.owner=python",
                        "--format=json",
                        connection=True,
                    )
                ).stdout
            )
            for volume in volumes:
                name = volume.get("Name", "")
                if re.fullmatch(r"quantix-input-[0-9a-f]{32}", name):
                    await self.command("volume", "rm", "--force", name, connection=True)
        except Exception as error:
            manifest = self.manifest()
            if manifest:
                manifest["qualified"] = False
                write_record(self.manifest_path, manifest)
            raise SandboxFailure(
                "Python cleanup could not be confirmed. Repair the private runtime."
            ) from error

    @property
    def manifest_path(self):
        return self.root / "image.json"

    def manifest(self):
        if not self.manifest_path.is_file():
            return None
        record = json.loads(self.manifest_path.read_text(encoding="utf-8"))
        if not IMAGE_ID.fullmatch(record.get("image_id", "")):
            raise SandboxFailure("The Python image record is invalid. Repair local Python.")
        return record

    def executable(self) -> Path | None:
        if self._executable:
            return self._executable
        candidates = sorted((self.root / "software" / PODMAN_VERSION).glob("**/podman.exe"))
        if candidates:
            return candidates[0]
        if platform.system() != "Windows":
            found = shutil.which("podman")
            return Path(found).resolve() if found else None
        return None

    def environment(self):
        # Inherit only OS process-start prerequisites, never AI credentials,
        # container connections, home directories, proxies or SSH agent state.
        env = {
            key: os.environ[key] for key in ("SystemRoot", "WINDIR", "COMSPEC") if key in os.environ
        }
        for leaf in ("home", "config", "data", "cache", "tmp", "runtime"):
            (self.root / leaf).mkdir(parents=True, exist_ok=True)
        binary = self.executable()
        env.update(
            {
                "HOME": str(self.root / "home"),
                "USERPROFILE": str(self.root / "home"),
                "XDG_CONFIG_HOME": str(self.root / "config"),
                "XDG_DATA_HOME": str(self.root / "data"),
                "XDG_CACHE_HOME": str(self.root / "cache"),
                "XDG_RUNTIME_DIR": str(self.root / "runtime"),
                "TMP": str(self.root / "tmp"),
                "TEMP": str(self.root / "tmp"),
                "TMPDIR": str(self.root / "tmp"),
                "CONTAINERS_CONF": str(self.root / "containers.conf"),
                "PODMAN_CONNECTIONS_CONF": str(self.root / "connections.json"),
                "PATH": os.pathsep.join(
                    filter(
                        None,
                        [
                            str(binary.parent) if binary else "",
                            str(Path(os.environ.get("SystemRoot", "C:/Windows")) / "System32")
                            if os.name == "nt"
                            else "/usr/local/bin:/usr/bin:/bin",
                        ],
                    )
                ),
            }
        )
        config = self.root / "containers.conf"
        if not config.exists():
            config.write_text("[machine]\nvolumes = []\n", encoding="utf-8")
        if config.read_text(encoding="utf-8") != "[machine]\nvolumes = []\n":
            raise SandboxFailure("The managed machine configuration changed. Repair local Python.")
        return env

    async def command(
        self, *args, timeout=30, stdin=None, max_output=1024 * 1024, connection=False, check=True
    ):
        binary = self.executable()
        if not binary:
            raise SandboxFailure("Set up local Python to install its private Podman software.")
        prefix = [str(binary)] + (["--connection", MACHINE_NAME] if connection else [])
        result = await self.runner.run(
            prefix + list(args),
            cwd=self.root,
            env=self.environment(),
            timeout=timeout,
            stdin=stdin,
            max_output=max_output,
        )
        if check and result.returncode:
            # Do not propagate arbitrary runtime paths or environment details.
            raise SandboxFailure(
                "The isolated Python runtime command failed. Repair local Python and try again."
            )
        return result

    async def status(self) -> SandboxStatus:
        try:
            monty_binary()
            composition = True
        except (ImportError, importlib.metadata.PackageNotFoundError, SandboxFailure):
            composition = False
        base = dict(
            machine_name=MACHINE_NAME,
            composition_available=composition,
            active_runs=self.active_runs,
        )
        if self.lock.locked() or self.closing:
            return SandboxStatus(
                **base,
                state="busy",
                detail="Local Python setup is running.",
                next_action="Wait for setup to finish.",
            )
        if not self.executable():
            return SandboxStatus(
                **base,
                state="not_installed",
                detail="Local Python needs its isolated runtime.",
                next_action="Set up local Python.",
            )
        if platform.system() == "Windows":
            wsl = Path(os.environ.get("SystemRoot", "C:/Windows")) / "System32" / "wsl.exe"
            if not wsl.is_file():
                return SandboxStatus(
                    **base,
                    state="prerequisite_required",
                    detail="Windows Subsystem for Linux 2 is required. Enable it in Windows, restart if requested, then return here.",
                    next_action="Enable WSL 2 in Windows.",
                )
            probe = await self.runner.run(
                [str(wsl), "--status"], cwd=self.root, env=self.environment(), timeout=15
            )
            if probe.returncode:
                return SandboxStatus(
                    **base,
                    state="prerequisite_required",
                    detail="Windows Subsystem for Linux 2 is not ready. Complete its Windows setup, then try again.",
                    next_action="Complete WSL 2 setup in Windows.",
                )
        try:
            probe = await self.command("machine", "inspect", MACHINE_NAME, check=False)
            if probe.returncode:
                return SandboxStatus(
                    **base,
                    state="setup_required",
                    detail="The private Python machine has not been created.",
                    next_action="Set up local Python.",
                )
            machine = json.loads(probe.stdout)[0]
            if not (self.root / "owner.json").is_file():
                raise SandboxFailure("This machine has no Quantix ownership record.")
            if machine.get("Mounts"):
                raise SandboxFailure("The Python machine has unexpected host-folder mounts.")
            if machine.get("State") != "running":
                return SandboxStatus(
                    **base,
                    state="stopped",
                    detail="Local Python is stopped.",
                    next_action="Start local Python.",
                )
            manifest = self.manifest()
            if not manifest or manifest.get("qualified") is not True:
                return SandboxStatus(
                    **base,
                    state="setup_required",
                    detail="The Python libraries and isolation checks need setup.",
                    next_action="Set up local Python.",
                )
            info = json.loads(
                (await self.command("info", "--format", "json", connection=True)).stdout
            )
            if info.get("host", {}).get("security", {}).get("rootless") is not True:
                raise SandboxFailure("Rootless containers are required for local Python.")
            await self.command("image", "exists", manifest["image_id"], connection=True)
            return SandboxStatus(
                **base,
                state="ready",
                python_available=True,
                image_id=manifest["image_id"],
                library_versions=manifest["libraries"],
                detail="Local Python is ready with no network or host-folder access.",
                next_action="Use a reviewed method with selected Tender inputs.",
            )
        except (SandboxFailure, OSError, ValueError, KeyError):
            return SandboxStatus(
                **base,
                state="needs_repair",
                detail="The private Python runtime could not pass its checks.",
                next_action="Repair local Python.",
            )

    def _install_windows(self):
        if platform.machine().lower() not in {"amd64", "x86_64"}:
            raise SandboxFailure(
                "Automatic setup currently supports Windows x64. This processor needs a qualified runtime package."
            )
        with urllib.request.urlopen(PODMAN_URL, timeout=60) as response:
            data = response.read(80 * 1024 * 1024 + 1)
        if len(data) > 80 * 1024 * 1024 or sha256(data) != PODMAN_ZIP_SHA256:
            raise SandboxFailure("The Podman software download did not match its reviewed hash.")
        destination = self.root / "software" / PODMAN_VERSION
        destination.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            if sum(item.file_size for item in archive.infolist()) > 250 * 1024 * 1024:
                raise SandboxFailure("The Podman software archive is too large.")
            for item in archive.infolist():
                path = destination / item.filename
                if (
                    not path.resolve().is_relative_to(destination.resolve())
                    or "\\" in item.filename
                ):
                    raise SandboxFailure("The Podman software archive has an invalid path.")
            archive.extractall(destination)
        write_record(destination / "receipt.json", {"url": PODMAN_URL, "sha256": PODMAN_ZIP_SHA256})

    async def action(self, action: str):
        if self.lock.locked() or self.active_runs or self.operations:
            raise SandboxFailure(
                "Wait for active Python work to finish before changing its runtime."
            )
        async with self.maintenance():
            self.root.mkdir(parents=True, exist_ok=True)
            if not self.executable():
                if action not in {"setup", "repair"}:
                    raise SandboxFailure("Local Python is not installed.")
                if platform.system() != "Windows":
                    raise SandboxFailure(
                        "Install Podman using its official operating-system installer, then return to setup."
                    )
                download = asyncio.create_task(asyncio.to_thread(self._install_windows))
                try:
                    await asyncio.shield(download)
                except asyncio.CancelledError:
                    # urllib has a bounded socket timeout. Join its work before
                    # reset/shutdown can remove the private software directory.
                    await download
                    raise
            if platform.system() == "Windows" and action in {"setup", "repair", "start"}:
                wsl = Path(os.environ.get("SystemRoot", "C:/Windows")) / "System32" / "wsl.exe"
                if not wsl.is_file():
                    raise SandboxFailure(
                        "Enable Windows Subsystem for Linux 2 in Windows, restart if requested, then return to local Python setup."
                    )
                prerequisite = await self.runner.run(
                    [str(wsl), "--status"], cwd=self.root, env=self.environment(), timeout=15
                )
                if prerequisite.returncode:
                    raise SandboxFailure(
                        "Complete Windows Subsystem for Linux 2 setup in Windows, then return here. Quantix has not enabled Windows features."
                    )
            if action in {"remove", "stop"}:
                if not (self.root / "owner.json").is_file():
                    raise SandboxFailure("Only the Quantix-owned machine can be changed.")
                await self.command("machine", "stop", MACHINE_NAME, check=False, timeout=60)
                if action == "remove":
                    await self.command("machine", "rm", "--force", MACHINE_NAME, timeout=60)
                    self.manifest_path.unlink(missing_ok=True)
                    (self.root / "owner.json").unlink()
            else:
                probe = await self.command("machine", "inspect", MACHINE_NAME, check=False)
                if probe.returncode:
                    # --now is intentionally absent: this never enables Windows
                    # features or requests a reboot on the engineer's behalf.
                    write_record(
                        self.root / "owner.json", {"name": MACHINE_NAME, "home": str(self.home)}
                    )
                    await self.command(
                        "machine",
                        "init",
                        "--cpus",
                        "2",
                        "--memory",
                        "3072",
                        "--disk-size",
                        "12",
                        "--rootful=false",
                        MACHINE_NAME,
                        timeout=600,
                        max_output=4 * 1024 * 1024,
                    )
                elif not (self.root / "owner.json").is_file():
                    raise SandboxFailure("The matching machine is not owned by Quantix.")
                machine = json.loads(
                    (await self.command("machine", "inspect", MACHINE_NAME)).stdout
                )[0]
                if machine.get("Mounts"):
                    raise SandboxFailure(
                        "Remove the private runtime and set it up again without host-folder mounts."
                    )
                if machine.get("State") != "running":
                    await self.command("machine", "start", MACHINE_NAME, timeout=120)
                if action != "start" or not self.manifest():
                    await self.cleanup_owned_runs()
                    await self._build_and_qualify()
        return await self.status()

    async def _build_and_qualify(self):
        info = json.loads((await self.command("info", "--format", "json", connection=True)).stdout)
        if info.get("host", {}).get("security", {}).get("rootless") is not True:
            raise SandboxFailure("The dedicated machine must use rootless containers.")
        build = self.root / "build"
        build.mkdir(exist_ok=True)
        containerfile = (
            f"FROM {BASE_IMAGE}\nRUN python -m pip install --no-cache-dir "
            + " ".join(f"{key}=={value}" for key, value in LIBRARIES.items())
            + "\nRUN mkdir /inputs /outputs && chmod 1777 /outputs\nUSER 1000:1000\n"
        )
        (build / "Containerfile").write_text(containerfile, encoding="utf-8")
        await self.command(
            "build",
            "--pull=always",
            "--tag",
            IMAGE_TAG,
            str(build),
            connection=True,
            timeout=600,
            max_output=8 * 1024 * 1024,
        )
        image = json.loads(
            (await self.command("image", "inspect", IMAGE_TAG, connection=True)).stdout
        )[0]
        image_id = image["Id"]
        if not IMAGE_ID.fullmatch(image_id):
            raise SandboxFailure("The built Python image could not be pinned.")
        probe = """
import os,json,pathlib,importlib.metadata as m
assert os.getuid()==1000
assert not os.path.exists('/mnt/c')
assert not os.path.exists('/run/podman/podman.sock')
assert os.statvfs('/').f_flag & os.ST_RDONLY
status=pathlib.Path('/proc/self/status').read_text()
assert 'CapEff:\\t0000000000000000' in status
assert 'NoNewPrivs:\\t1' in status
assert set(os.listdir('/sys/class/net')) == {'lo'}
assert pathlib.Path('/sys/fs/cgroup/memory.max').read_text().strip() == '2147483648'
assert pathlib.Path('/sys/fs/cgroup/pids.max').read_text().strip() == '64'
quota,period=pathlib.Path('/sys/fs/cgroup/cpu.max').read_text().split()
assert int(quota)/int(period) == 2
print(json.dumps(dict((d.metadata['Name'],d.version) for d in m.distributions())))
"""
        result = await self.command(
            *self.container_args("quantix-check", image_id),
            "python",
            "-I",
            "-c",
            probe,
            connection=True,
        )
        libraries = json.loads(result.stdout)
        for name, expected in LIBRARIES.items():
            if libraries.get(name) != expected:
                raise SandboxFailure("The installed Python libraries differ from the reviewed set.")
        write_record(
            self.manifest_path,
            {
                "image_id": image_id,
                "base_image": BASE_IMAGE,
                "libraries": libraries,
                "containerfile_sha256": sha256(containerfile.encode()),
                "qualified": True,
                "platform": platform.system(),
                "podman_version": PODMAN_VERSION,
            },
        )

    def container_args(self, name, image_id, *, limits=None, input_volume=None):
        from .python_analysis_models import PythonLimits

        limits = limits or PythonLimits()
        if not re.fullmatch(r"quantix-[a-z0-9-]+", name) or not IMAGE_ID.fullmatch(image_id):
            raise ValueError("Invalid server-owned container identity.")
        args = [
            "run",
            "--rm",
            "--label=io.quantix.owner=python",
            "--name",
            name,
            "--pull=never",
            "--network=none",
            "--timeout",
            str(limits.seconds + 5),
            "--read-only",
            "--read-only-tmpfs=false",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--user=1000:1000",
            "--pids-limit",
            str(limits.processes),
            "--cpus",
            str(limits.cpus),
            "--memory",
            f"{limits.memory_mib}m",
            "--memory-swap",
            f"{limits.memory_mib}m",
            "--ulimit",
            "nofile=128:128",
            "--ulimit",
            "core=0:0",
            "--tmpfs",
            "/tmp:rw,noexec,nosuid,nodev,size=64m,mode=1777",
            "--tmpfs",
            f"/outputs:rw,noexec,nosuid,nodev,size={limits.output_mib}m,mode=1777",
            "--workdir=/outputs",
            "--env=HOME=/tmp",
            "--env=MPLCONFIGDIR=/tmp/matplotlib",
            "--env=OPENBLAS_NUM_THREADS=1",
            "--env=OMP_NUM_THREADS=1",
        ]
        if input_volume:
            if not re.fullmatch(r"quantix-input-[0-9a-f]{32}", input_volume):
                raise ValueError("Invalid input volume.")
            args += ["--mount", f"type=volume,source={input_volume},target=/inputs,ro=true"]
        return [*args, "--interactive", image_id]


def get_code_runtime(repo) -> PodmanRuntime:
    """One lifecycle owner per Repository, shared by API and reviewed tools."""
    existing = getattr(repo, "_code_runtime", None)
    if existing is None:
        existing = PodmanRuntime(repo.home)
        repo._code_runtime = existing
    return existing
