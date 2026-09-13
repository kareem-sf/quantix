"""Adapter/lifecycle fixtures, explicitly not real VM qualification evidence."""

import asyncio
import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from quantix.browser_research import BASE_IMAGE, BrowserResearchRuntime
from quantix.podman_runtime import PodmanRuntime
from quantix.repository import Repository
from quantix.sandbox_protocol import CommandResult, SandboxFailure, sha256, write_record


class FakeRuntime(PodmanRuntime):
    def __init__(self, home, *, ready=True, block=False):
        super().__init__(home, executable=Path("private-podman"))
        self.ready, self.block = ready, block
        self.calls = []
        self.entered = asyncio.Event()

    async def status(self):
        return SimpleNamespace(
            python_available=self.ready,
            state="ready" if self.ready else "prerequisite_required",
            detail="WSL2 needs setup." if not self.ready else "Ready",
            next_action="Complete Windows setup.",
        )

    async def command(self, *args, **kwargs):
        self.calls.append((args, kwargs))
        if args[0] == "run" and "--detach" not in args:
            self.entered.set()
            if self.block:
                await asyncio.sleep(30)
            return CommandResult(
                0,
                json.dumps(
                    {
                        "final_url": "https://example.test/page",
                        "title": "Synthetic page",
                        "text": "Read only",
                        "html": "<p>Read only</p>",
                        "status_code": 200,
                    }
                ).encode(),
                b"",
            )
        return CommandResult(0, b"", b"")


def configured(tmp_path, monkeypatch, **options):
    repo = Repository(tmp_path)
    runtime = FakeRuntime(repo.home, **options)
    monkeypatch.setattr("quantix.browser_research.get_code_runtime", lambda _repo: runtime)
    monkeypatch.setattr(
        "quantix.research_service.ResearchService._validate_url", lambda _self, _url: ["1.1.1.1"]
    )
    browser = BrowserResearchRuntime(repo)
    # Unit fixture only: these fields are planted, not an executed OS probe.
    proof = {
        "qualified": True,
        "playwright": "1.63.0",
        "networkless_renderer": True,
        "direct_egress_denied": ["10.0.2.2", "192.168.1.1", "169.254.169.254", "1.1.1.1"],
        "gateway_rejections": [
            "http://127.0.0.1/",
            "http://169.254.169.254/",
            "https://other.example/",
            "http://example.com:443/",
        ],
        "remote_post_denied": True,
        "local_chromium_rendered": True,
        "uid": 1000,
        "gateway_get_url": "https://example.com/",
        "gateway_get_status": 200,
        "gateway_get_body_bytes": 100,
        "gateway_tls_verified": True,
    }
    identity = {
        "image_id": "sha256:" + "a" * 64,
        "source_hash": browser._source_hash(),
        "proof": proof,
    }
    write_record(
        browser.manifest_path,
        {
            **identity,
            "base_image": BASE_IMAGE,
            "qualified": True,
            "fingerprint": sha256(json.dumps(identity, sort_keys=True).encode()),
        },
    )
    return browser, runtime


@pytest.mark.asyncio
async def test_private_podman_present_but_wsl_missing_is_truthful(tmp_path, monkeypatch):
    browser, runtime = configured(tmp_path, monkeypatch, ready=False)
    status = await browser.status()
    assert status.podman_available and not status.ready and status.state == "prerequisite_required"
    assert "WSL2" in status.detail
    with pytest.raises(SandboxFailure, match="WSL2"):
        await browser.render("https://example.test/page")
    assert runtime.calls == []


@pytest.mark.asyncio
async def test_renderer_has_no_network_and_only_readonly_proxy_volume(tmp_path, monkeypatch):
    browser, runtime = configured(tmp_path, monkeypatch)
    page = await browser.render("https://example.test/page")
    assert page.runtime_image_id == "sha256:" + "a" * 64
    commands = [args for args, _ in runtime.calls if args[0] == "run"]
    proxy, renderer = commands
    assert "--detach" in proxy and "--entrypoint=node" in proxy
    assert "--network=none" in renderer
    assert any("target=/proxy,ro=true" in arg for arg in renderer)
    assert all("type=bind" not in arg for args in commands for arg in args)
    assert all(options.get("connection") is True for _, options in runtime.calls)
    assert len([args for args, _ in runtime.calls if args[:2] == ("rm", "--force")]) == 2
    assert not runtime.operations


@pytest.mark.asyncio
async def test_browser_cancel_reaps_both_containers_and_releases_lease(tmp_path, monkeypatch):
    browser, runtime = configured(tmp_path, monkeypatch, block=True)
    task = asyncio.create_task(browser.render("https://example.test/page"))
    await runtime.entered.wait()
    with pytest.raises(SandboxFailure, match="active"):
        async with runtime.maintenance():
            pass
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert len([args for args, _ in runtime.calls if args[:2] == ("rm", "--force")]) == 2
    assert runtime.calls[-1][0][:2] == ("volume", "rm")
    assert not runtime.operations


@pytest.mark.asyncio
async def test_mutated_source_or_missing_proof_never_reports_ready(tmp_path, monkeypatch):
    browser, _runtime = configured(tmp_path, monkeypatch)
    monkeypatch.setattr(browser, "_source_hash", lambda: "different-source")
    assert (await browser.status()).state == "needs_repair"


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "mutation", ["missing_positive_probe", "base", "fingerprint", "probe_targets"]
)
async def test_manifest_requires_exact_complete_proof_and_recomputed_hash(
    tmp_path, monkeypatch, mutation
):
    browser, _ = configured(tmp_path, monkeypatch)
    value = json.loads(browser.manifest_path.read_text())
    if mutation == "missing_positive_probe":
        value["proof"].pop("gateway_get_status")
    elif mutation == "base":
        value["base_image"] = "mutable-tag"
    elif mutation == "fingerprint":
        value["fingerprint"] = "b" * 64
    else:
        value["proof"]["direct_egress_denied"] = []
        identity = {key: value[key] for key in ("image_id", "source_hash", "proof")}
        value["fingerprint"] = sha256(json.dumps(identity, sort_keys=True).encode())
    write_record(browser.manifest_path, value)
    assert (await browser.status()).state == "needs_repair"
