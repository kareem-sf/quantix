"""Public browser in the private VM: networkless renderer and a fixed-target proxy."""

from __future__ import annotations

import asyncio
import json
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Literal
from urllib.parse import urlsplit

from pydantic import BaseModel, ConfigDict, Field, model_validator

from .podman_runtime import IMAGE_ID, get_code_runtime
from .python_analysis_models import PythonLimits
from .sandbox_protocol import SandboxFailure, sha256, write_record

IMAGE = "localhost/quantix-research-browser:2"
BASE_IMAGE = "mcr.microsoft.com/playwright@sha256:eff16c30e6f3f4af0a03fa4b706120d5e9b0891c344a27d64559aff5900a4a27"
MAX_RENDERED_CHARS = 400_000
MAX_OUTPUT_BYTES = 4_000_000
_SOURCES = (
    "Containerfile",
    "package.json",
    "package-lock.json",
    "read-page.mjs",
    "public-proxy.mjs",
    "qualify.mjs",
)
_STATE = Literal[
    "not_installed",
    "prerequisite_required",
    "setup_required",
    "stopped",
    "ready",
    "needs_repair",
    "busy",
]


class BrowserQualificationProof(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    qualified: Literal[True]
    playwright: Literal["1.63.0"]
    networkless_renderer: Literal[True]
    direct_egress_denied: list[str]
    gateway_rejections: list[str]
    remote_post_denied: Literal[True]
    local_chromium_rendered: Literal[True]
    uid: Literal[1000]
    gateway_get_url: Literal["https://example.com/"]
    gateway_get_status: Literal[200]
    gateway_get_body_bytes: int = Field(gt=0, le=2 * 1024 * 1024)
    gateway_tls_verified: Literal[True]

    @model_validator(mode="after")
    def exact_probes(self):
        if self.direct_egress_denied != [
            "10.0.2.2",
            "192.168.1.1",
            "169.254.169.254",
            "1.1.1.1",
        ] or self.gateway_rejections != [
            "http://127.0.0.1/",
            "http://169.254.169.254/",
            "https://other.example/",
            "http://example.com:443/",
        ]:
            raise ValueError("The browser isolation probes are incomplete.")
        return self


class BrowserImageManifest(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    image_id: str = Field(pattern=r"^sha256:[a-f0-9]{64}$")
    source_hash: str = Field(pattern=r"^[a-f0-9]{64}$")
    base_image: Literal[BASE_IMAGE]
    qualified: bool
    proof: BrowserQualificationProof
    fingerprint: str = Field(pattern=r"^[a-f0-9]{64}$")

    @model_validator(mode="after")
    def exact_fingerprint(self):
        expected = sha256(
            json.dumps(
                {
                    "image_id": self.image_id,
                    "source_hash": self.source_hash,
                    "proof": self.proof.model_dump(),
                },
                sort_keys=True,
            ).encode()
        )
        if self.fingerprint != expected:
            raise ValueError("The saved browser fingerprint does not match its proof.")
        return self


@dataclass(frozen=True)
class BrowserResearchStatus:
    ready: bool
    podman_available: bool
    image: str
    detail: str
    state: _STATE = "setup_required"
    image_id: str | None = None


@dataclass(frozen=True)
class RenderedPublicPage:
    requested_url: str
    final_url: str
    title: str
    text: str
    html: str
    runtime_image_id: str | None = None
    runtime_fingerprint: str | None = None
    status_code: int = 200


def _source_directory():
    return Path(__file__).resolve().parents[1] / "research-browser"


class BrowserResearchRuntime:
    def __init__(self, repo):
        self.repo = repo
        self.runtime = get_code_runtime(repo)
        self.root = repo.home / "code" / "browser"
        self.manifest_path = self.root / "image.json"

    def _sources(self):
        source = _source_directory()
        files = {}
        for name in _SOURCES:
            path = source / name
            if not path.is_file() or path.is_symlink() or path.stat().st_size > 1024 * 1024:
                raise SandboxFailure(
                    "The reviewed public-browser build files are missing or changed."
                )
            files[name] = path.read_bytes()
        if BASE_IMAGE.encode() not in files["Containerfile"]:
            raise SandboxFailure(
                "The public-browser base image is not pinned to its reviewed digest."
            )
        return files

    def _source_hash(self):
        hashes = {name: sha256(value) for name, value in self._sources().items()}
        for name in (
            "browser_research.py",
            "podman_runtime.py",
            "sandbox_protocol.py",
            "python_analysis_models.py",
        ):
            hashes["host:" + name] = sha256(Path(__file__).with_name(name).read_bytes())
        return sha256(json.dumps(hashes, sort_keys=True).encode())

    def _manifest(self):
        if not self.manifest_path.exists():
            return None
        return BrowserImageManifest.model_validate_json(
            self.manifest_path.read_text(encoding="utf-8")
        ).model_dump(mode="json")

    async def status(self):
        shared = await self.runtime.status()
        available = self.runtime.executable() is not None
        if not shared.python_available:
            return BrowserResearchStatus(
                False, available, IMAGE, shared.detail + " " + shared.next_action, shared.state
            )
        try:
            manifest = self._manifest()
            if not manifest:
                return BrowserResearchStatus(
                    False,
                    available,
                    IMAGE,
                    "Set up and qualify the isolated public-page reader.",
                    "setup_required",
                )
            if not manifest.get("qualified") or manifest.get("source_hash") != self._source_hash():
                raise SandboxFailure(
                    "The browser's isolation proof is missing or its source changed. Set up the reader again."
                )
            await self.runtime.command("image", "exists", manifest["image_id"], connection=True)
            return BrowserResearchStatus(
                True,
                available,
                IMAGE,
                "The qualified public reader uses a networkless browser and a checked public-target proxy.",
                "ready",
                manifest["image_id"],
            )
        except (OSError, ValueError, SandboxFailure):
            return BrowserResearchStatus(
                False,
                available,
                IMAGE,
                "The private browser image or network-isolation proof needs repair.",
                "needs_repair",
            )

    def _args(self, name, image_id, *, proxy=False, volume=None):
        limits = PythonLimits(
            cpus=1,
            memory_mib=256 if proxy else 768,
            seconds=80,
            processes=16 if proxy else 128,
            output_mib=1,
        )
        args = self.runtime.container_args(name, image_id, limits=limits)
        args = [
            value.replace("nofile=128:128", "nofile=512:512").replace(
                "/tmp:rw,noexec,nosuid,nodev,size=64m", "/tmp:rw,noexec,nosuid,nodev,size=256m"
            )
            for value in args
        ]
        if proxy:
            args = [
                "--network=slirp4netns:allow_host_loopback=false"
                if value == "--network=none"
                else "--user=0:0"
                if value == "--user=1000:1000"
                else value
                for value in args
            ]
        if volume:
            args[-1:-1] = [
                "--mount",
                f"type=volume,source={volume},target=/proxy" + ("" if proxy else ",ro=true"),
            ]
        return args

    async def _start_proxy(self, identifier, image_id, host, address, port, scheme):
        volume, proxy_name = "quantix-input-" + identifier, "quantix-run-" + identifier
        await self.runtime.command(
            "volume", "create", "--label=io.quantix.owner=python", volume, connection=True
        )
        args = self._args(proxy_name, image_id, proxy=True, volume=volume)
        args[-1:-1] = ["--detach", "--entrypoint=node"]
        await self.runtime.command(
            *args,
            "/app/public-proxy.mjs",
            "/proxy/reader.sock",
            host,
            address,
            str(port),
            scheme + ":",
            connection=True,
            timeout=30,
        )
        return volume, proxy_name

    async def _cleanup(self, names, volume):
        failures = []
        for name in names:
            try:
                await self.runtime.command(
                    "rm", "--force", "--ignore", name, connection=True, timeout=15
                )
            except Exception:
                failures.append(name)
        if volume:
            try:
                await self.runtime.command(
                    "volume", "rm", "--force", volume, connection=True, timeout=15
                )
            except Exception:
                failures.append(volume)
        if failures:
            if self.manifest_path.exists():
                manifest = self._manifest()
                manifest["qualified"] = False
                write_record(self.manifest_path, manifest)
            raise SandboxFailure(
                "Reader cleanup could not be confirmed. Repair the private runtime before another request."
            )

    async def setup(self):
        shared = await self.runtime.status()
        if not shared.python_available:
            raise SandboxFailure(shared.detail + " " + shared.next_action)
        async with self.runtime.maintenance():
            from .research_service import ResearchService

            addresses = await asyncio.to_thread(
                ResearchService(self.repo)._validate_url, "https://example.com/"
            )
            address = next((value for value in addresses if ":" not in value), None)
            if address is None:
                raise SandboxFailure(
                    "The qualification site needs a reachable public IPv4 address."
                )
            files, source_hash = self._sources(), self._source_hash()
            build = self.root / "build" / source_hash
            build.mkdir(parents=True, exist_ok=True)
            for name, value in files.items():
                (build / name).write_bytes(value)
            await self.runtime.command(
                "build",
                "--pull=always",
                "--force-rm",
                "--tag",
                IMAGE,
                str(build),
                connection=True,
                timeout=900,
                max_output=8 * 1024 * 1024,
            )
            image_id = json.loads(
                (await self.runtime.command("image", "inspect", IMAGE, connection=True)).stdout
            )[0]["Id"]
            if not IMAGE_ID.fullmatch(image_id):
                raise SandboxFailure("The browser image could not be pinned.")
            identifier, browser_id = uuid.uuid4().hex, uuid.uuid4().hex
            volume, proxy_name = "quantix-input-" + identifier, "quantix-run-" + identifier
            browser_name = "quantix-run-" + browser_id
            try:
                await self._start_proxy(identifier, image_id, "example.com", address, 443, "https")
                args = self._args(browser_name, image_id, volume=volume)
                args[-1:-1] = ["--entrypoint=node"]
                proof = json.loads(
                    (
                        await self.runtime.command(
                            *args,
                            "/app/qualify.mjs",
                            "/proxy/reader.sock",
                            connection=True,
                            timeout=75,
                            max_output=262144,
                        )
                    ).stdout
                )
                proof = BrowserQualificationProof.model_validate(proof).model_dump(mode="json")
            finally:
                await self._cleanup([browser_name, proxy_name], volume)
            write_record(
                self.manifest_path,
                {
                    "image_id": image_id,
                    "source_hash": source_hash,
                    "base_image": BASE_IMAGE,
                    "qualified": True,
                    "proof": proof,
                    "fingerprint": sha256(
                        json.dumps(
                            {"image_id": image_id, "source_hash": source_hash, "proof": proof},
                            sort_keys=True,
                        ).encode()
                    ),
                },
            )
        return await self.status()

    async def render(self, url):
        from .research_service import ResearchService

        async with self.runtime.operation():
            status = await self.status()
            if not status.ready:
                raise SandboxFailure(status.detail)
            addresses = await asyncio.to_thread(ResearchService(self.repo)._validate_url, url)
            address = next((value for value in addresses if ":" not in value), None)
            if address is None:
                raise ValueError(
                    "The isolated public reader currently requires a public IPv4 destination."
                )
            target = urlsplit(url)
            host = target.hostname.encode("idna").decode("ascii").lower() if target.hostname else ""
            port = target.port or (443 if target.scheme == "https" else 80)
            manifest = self._manifest()
            identifier, browser_id = uuid.uuid4().hex, uuid.uuid4().hex
            volume, proxy_name = "quantix-input-" + identifier, "quantix-run-" + identifier
            browser_name = "quantix-run-" + browser_id
            try:
                await self._start_proxy(
                    identifier, manifest["image_id"], host, address, port, target.scheme
                )
                result = await self.runtime.command(
                    *self._args(browser_name, manifest["image_id"], volume=volume),
                    url,
                    host,
                    address,
                    str(MAX_RENDERED_CHARS),
                    "/proxy/reader.sock",
                    connection=True,
                    timeout=80,
                    max_output=MAX_OUTPUT_BYTES,
                )
                value = json.loads(result.stdout)
                if not all(
                    isinstance(value.get(key), str) for key in ("final_url", "text", "html")
                ):
                    raise ValueError("The isolated browser returned an invalid page record.")
                final = urlsplit(value["final_url"])
                if (
                    final.scheme != target.scheme
                    or (final.port or (443 if final.scheme == "https" else 80)) != port
                    or not final.hostname
                    or final.hostname.encode("idna").decode().lower() != host
                ):
                    raise ValueError("The rendered page left its checked public host.")
                return RenderedPublicPage(
                    url,
                    value["final_url"],
                    str(value.get("title", ""))[:300],
                    value["text"][:MAX_RENDERED_CHARS],
                    value["html"][:MAX_RENDERED_CHARS],
                    manifest["image_id"],
                    manifest["fingerprint"],
                    int(value.get("status_code", 200)),
                )
            finally:
                await self._cleanup([browser_name, proxy_name], volume)


__all__ = ["BrowserResearchRuntime", "BrowserResearchStatus", "RenderedPublicPage"]
