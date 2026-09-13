"""Factory reset uses disposable homes and fake metadata-only OS credentials."""

import asyncio
import importlib
import json
import threading
from pathlib import Path

import pytest
from fastapi.testclient import TestClient


def reset_module():
    try:
        return importlib.import_module("quantix.factory_reset")
    except ModuleNotFoundError:
        pytest.fail("The factory-reset backend is not implemented")


class Credentials:
    supported = True

    def __init__(self, home):
        self.home = home
        self.targets = [{"type": 1, "target": "owned-credential"}]
        self.remaining = {"owned-credential"}
        self.fail = False
        self.inventories = 0

    def inventory(self, home, account_ids):
        self.inventories += 1
        return self.targets

    def delete(self, target):
        journal = json.loads((self.home / "pending-reset.json").read_text())
        assert journal["credential_targets"] == self.targets
        assert journal["phase"] == "cleaning_credentials"
        assert not journal["credentials_cleared"]
        if self.fail:
            raise OSError(5, "synthetic OS access failure")
        self.remaining.discard(target["target"])

    def validate_targets(self, home, account_ids, targets):
        if targets != self.targets:
            raise ValueError("Synthetic target does not belong to this home")


@pytest.fixture
def service(tmp_path, monkeypatch):
    module = reset_module()
    from quantix.ai_connections import AIConnectionService
    from quantix.repository import Repository

    monkeypatch.setattr("keyring.get_password", lambda *_: pytest.fail("No keyring reads"))
    repo = Repository(tmp_path / "home")
    AIConnectionService(repo)
    credentials = Credentials(repo.home)
    reset = module.FactoryResetService(repo.home, repo=repo, credentials=credentials)
    return reset, repo, credentials


def confirm(reset, fingerprint=None):
    from quantix.reset_models import ResetRequest
    return asyncio.run(reset.confirm(ResetRequest(
        fingerprint=fingerprint or reset.preview().fingerprint, confirmation="RESET",
    )))


def test_preview_counts_all_versions_and_owned_backups_without_reading_secrets(service):
    reset, repo, _ = service
    tender = repo.create_tender("Synthetic Tender")
    repo.register_artifact(tender["id"], "Drawing.pdf", "a" * 64, 4,
                           {"kind": "pdf", "status": "extracted", "segments": []})
    repo.register_artifact(tender["id"], "Drawing.pdf", "b" * 64, 8,
                           {"kind": "pdf", "status": "extracted", "segments": []})
    (repo.home / "backups").mkdir()
    (repo.home / "backups" / "one.zip").write_bytes(b"synthetic backup")
    preview = reset.preview()
    assert preview.supported and preview.home == str(repo.home)
    assert (preview.tender_count, preview.artifact_count, preview.account_count, preview.backup_count) == (1, 2, 0, 1)
    assert not preview.blockers


def test_confirmation_requires_current_preview_and_keeps_files_for_native_cleanup(service):
    reset, repo, credentials = service
    preview = reset.preview()
    repo.create_tender("Changed after preview")
    with pytest.raises(ValueError, match="changed"):
        confirm(reset, preview.fingerprint)
    assert not (repo.home / "pending-reset.json").exists()
    assert credentials.remaining
    receipt = confirm(reset)
    assert receipt.state == "ready"
    assert set(receipt.model_dump()) == {"reset_id", "state", "detail"}
    assert (repo.home / "quantix.sqlite").exists()
    assert repo.list_tenders()
    assert not credentials.remaining
    assert reset.pending


def test_repeated_confirmation_returns_original_receipt_without_new_inventory(service):
    reset, _, credentials = service
    fingerprint = reset.preview().fingerprint
    first = confirm(reset, fingerprint)
    assert confirm(reset, fingerprint) == first
    assert credentials.inventories == 1
    with pytest.raises(ValueError, match="preview"):
        confirm(reset, "f" * 64)


def test_credential_failure_is_durable_gated_and_retries_original_inventory(service):
    reset, repo, credentials = service
    credentials.fail = True
    first = confirm(reset)
    assert first.state == "credential_error"
    status = reset.status()
    assert set(status.model_dump()) == {"reset_id", "state", "detail", "credentials_cleared", "fingerprint"}
    assert not status.credentials_cleared
    assert "synthetic OS" not in status.detail
    with pytest.raises(ValueError, match="reset"):
        reset.enter_request("GET", "/api/tenders")
    restarted = reset_module().FactoryResetService(repo.home, credentials=credentials)
    credentials.fail = False
    receipt = confirm(restarted, status.fingerprint)
    assert receipt.reset_id == first.reset_id and receipt.state == "ready"
    assert credentials.inventories == 1


def test_reset_refuses_busy_runs_and_inflight_reads_or_writes(service):
    reset, repo, _ = service
    tender = repo.create_tender("Busy")
    run = repo.create_run(tender["id"], "manager", "Synthetic work")
    with pytest.raises(ValueError, match="work"):
        confirm(reset)
    repo.update_run(run["id"], status="completed", detail="Finished")
    for method in ("GET", "POST"):
        tracked = reset.enter_request(method, "/api/ai/connections/example/runtime")
        try:
            with pytest.raises(ValueError, match="request"):
                confirm(reset)
        finally:
            reset.leave_request(tracked)
    assert confirm(reset).state == "ready"


def test_confirmation_closes_clients_with_gate_already_active(service):
    reset, _, credentials = service

    async def close_clients():
        assert reset.pending
        with pytest.raises(ValueError, match="reset"):
            reset.enter_request("POST", "/api/settings")
        assert credentials.remaining
        await asyncio.sleep(0)

    reset.close_clients = close_clients
    assert confirm(reset).state == "ready"


def test_pending_journal_does_not_recover_jobs_or_construct_workspace(tmp_path, monkeypatch):
    reset_module()
    home = tmp_path / "home"
    home.mkdir()
    journal = {
        "format": 1, "reset_id": "a" * 32, "home": str(home.resolve()),
        "phase": "credential_error", "credentials_cleared": False,
        "fingerprint": "b" * 64, "confirmed_at": "2026-09-09T12:00:00+00:00",
        "detail": "Credential cleanup needs another attempt.", "credential_targets": [],
    }
    (home / "pending-reset.json").write_text(json.dumps(journal))
    import quantix.api as api

    monkeypatch.setattr(api, "Repository", lambda *_: pytest.fail("Recovery must not construct Repository"))
    monkeypatch.setattr("keyring.get_password", lambda *_: pytest.fail("Recovery must not read keyring"))
    app = api.create_app(home, "test-session-token")
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer test-session-token"
        assert client.get("/api/health").json()["reset_pending"] is True
        assert client.get("/api/reset/status").json()["state"] == "credential_error"
        assert client.get("/api/diagnostics").status_code == 200
        assert client.get("/api/tenders").status_code == 409
        assert client.patch("/api/settings", json={"preferences": "changed"}).status_code == 409
        assert client.post("/api/diagnostics/events", json={}).status_code == 409
        assert client.get("/api/reset/status", headers={"Authorization": ""}).status_code == 401
    assert not (home / "quantix.sqlite").exists()


def test_reset_route_requires_typed_confirmation_and_reports_unsupported(tmp_path, monkeypatch):
    reset_module()
    import quantix.api as api
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    with TestClient(api.create_app(tmp_path / "home", "test-session-token")) as client:
        client.headers["Authorization"] = "Bearer test-session-token"
        preview = client.get("/api/reset/preview")
        assert preview.status_code == 200
        assert not preview.json()["supported"]
        assert "factory_reset" not in client.get("/api/health").json()["capabilities"]
        assert client.post("/api/reset", json={"fingerprint": preview.json()["fingerprint"], "confirmation": "reset"}).status_code == 422
        assert client.post("/api/reset", json={"fingerprint": preview.json()["fingerprint"], "confirmation": "RESET"}).status_code == 409


def test_invalid_or_symlinked_journal_fails_closed(tmp_path):
    module = reset_module()
    home = tmp_path / "home"
    home.mkdir()
    (home / "pending-reset.json").write_text("not-json")
    with pytest.raises(ValueError, match="reset"):
        module.load_journal(home)


def test_startup_filesystem_phase_stops_before_runtime_or_log_initialization(tmp_path, monkeypatch):
    reset_module()
    home = tmp_path / "home"
    home.mkdir()
    journal = {
        "format": 1, "reset_id": "a" * 32, "home": str(home.resolve()), "phase": "ready",
        "credentials_cleared": True, "fingerprint": "b" * 64,
        "confirmed_at": "2026-09-09T12:00:00+00:00", "detail": "Ready", "credential_targets": [],
    }
    (home / "pending-reset.json").write_text(json.dumps(journal))
    import quantix.__main__ as entry
    monkeypatch.setattr("sys.argv", ["quantix", "--home", str(home)])
    monkeypatch.setattr("quantix.storage.prepare_process_environment", lambda *_: pytest.fail("Must gate before runtime creation"))
    with pytest.raises(SystemExit):
        entry.main()


def test_checkpoint_and_log_writes_do_not_invalidate_preview_but_mutations_do(service):
    import os
    reset, repo, _ = service
    initial = reset.preview().fingerprint
    database = repo.home / "quantix.sqlite"
    info = database.stat()
    os.utime(database, ns=(info.st_atime_ns, info.st_mtime_ns + 10000000))
    (repo.home / "logs" / "synthetic.log").write_text("only a log")
    assert reset.preview().fingerprint == initial
    tracked = reset.enter_request("PATCH", "/api/settings")
    reset.leave_request(tracked)
    assert reset.preview().fingerprint != initial


def test_retry_rejects_tampered_credential_inventory_before_any_deletion(service):
    reset, repo, credentials = service
    credentials.fail = True
    confirm(reset)
    path = repo.home / "pending-reset.json"
    journal = json.loads(path.read_text())
    journal["credential_targets"] = [{"type": 1, "target": "independent-app"}]
    path.write_text(json.dumps(journal))
    credentials.fail = False
    attempted = []
    credentials.delete = lambda target: attempted.append(target)
    restarted = reset_module().FactoryResetService(repo.home, credentials=credentials)
    receipt = confirm(restarted, journal["fingerprint"])
    assert receipt.state == "credential_error"
    assert attempted == []
    assert credentials.remaining == {"owned-credential"}


def test_real_api_admission_blocks_reset_overlap_and_later_domain_requests(tmp_path, monkeypatch):
    from concurrent.futures import ThreadPoolExecutor

    import quantix.api as api
    module = reset_module()
    home = (tmp_path / "home").resolve()
    credentials = Credentials(home)
    monkeypatch.setattr(module, "WindowsCredentialAdapter", lambda: credentials)
    monkeypatch.setattr(module, "normal_home", lambda: home)
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    app = api.create_app(home, "test-session-token")
    entered, release = threading.Event(), threading.Event()

    @app.get("/api/synthetic-work")
    async def work():
        entered.set()
        assert await asyncio.to_thread(release.wait, 10)
        return {"finished": True}

    with TestClient(app) as client, ThreadPoolExecutor(max_workers=3) as pool:
        client.headers["Authorization"] = "Bearer test-session-token"
        preview = client.get("/api/reset/preview").json()
        command = {"fingerprint": preview["fingerprint"], "confirmation": "RESET"}
        running = pool.submit(client.get, "/api/synthetic-work")
        try:
            assert entered.wait(5)
            assert client.post("/api/reset", json=command).status_code == 409
            assert not (home / "pending-reset.json").exists()
        finally:
            release.set()
        assert running.result(10).status_code == 200
        inventory_entered, inventory_release = threading.Event(), threading.Event()
        inventory = credentials.inventory

        def slow_inventory(*args):
            inventory_entered.set()
            assert inventory_release.wait(10)
            return inventory(*args)

        credentials.inventory = slow_inventory
        reset_request = pool.submit(client.post, "/api/reset", json=command)
        try:
            assert inventory_entered.wait(5)
            assert client.get("/api/tenders").status_code == 409
            assert client.post("/api/tenders", json={"name": "Too late"}).status_code == 409
            assert client.get("/api/health").json()["reset_pending"] is True
            assert client.get("/api/reset/status").json()["state"] == "cleaning_credentials"
            duplicate = pool.submit(client.post, "/api/reset", json=command)
        finally:
            inventory_release.set()
        first, second = reset_request.result(10), duplicate.result(10)
        assert first.status_code == second.status_code == 200
        assert first.json() == second.json()
        assert first.json()["state"] == "ready"
        assert not credentials.remaining


def test_real_api_rejects_busy_account_lease_before_staging_reset(tmp_path, monkeypatch):
    import quantix.api as api
    from quantix.ai_connections import AIConnectionService
    module = reset_module()
    home = (tmp_path / "home").resolve()
    credentials = Credentials(home)
    monkeypatch.setattr(module, "WindowsCredentialAdapter", lambda: credentials)
    monkeypatch.setattr(module, "normal_home", lambda: home)
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    app = api.create_app(home, "test-session-token")
    state = AIConnectionService(app.state.repo)._state
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer test-session-token"
        state.leases["synthetic"] = 1
        try:
            preview = client.get("/api/reset/preview").json()
            assert any("account" in detail for detail in preview["blockers"])
            result = client.post("/api/reset", json={"fingerprint": preview["fingerprint"], "confirmation": "RESET"})
            assert result.status_code == 409
            assert credentials.inventories == 0
            assert not (home / "pending-reset.json").exists()
        finally:
            state.leases.clear()


def test_metadata_inventory_failure_and_interrupted_cleaning_retry_without_losing_gate(service):
    reset, repo, credentials = service
    original = credentials.inventory
    credentials.inventory = lambda *_: (_ for _ in ()).throw(OSError("synthetic enumeration failure"))
    receipt = confirm(reset)
    assert receipt.state == "credential_error"
    journal = json.loads((repo.home / "pending-reset.json").read_text())
    assert journal["credential_targets"] is None
    credentials.inventory = original
    journal["phase"] = "cleaning_credentials"
    (repo.home / "pending-reset.json").write_text(json.dumps(journal))
    restarted = reset_module().FactoryResetService(repo.home, credentials=credentials)
    assert confirm(restarted, journal["fingerprint"]).state == "ready"


def test_health_read_that_started_before_confirmation_must_finish_first(service):
    reset, _, _ = service
    tracked = reset.enter_request("GET", "/api/health")
    try:
        with pytest.raises(ValueError, match="request"):
            confirm(reset)
    finally:
        reset.leave_request(tracked)
    assert confirm(reset).state == "ready"
    assert reset.enter_request("GET", "/api/health") is None


def test_hardlinked_journal_is_rejected_without_changing_neighbor(service, tmp_path):
    import os
    reset, repo, _ = service
    confirm(reset)
    journal = repo.home / "pending-reset.json"
    neighbor = tmp_path / "neighbor.json"
    os.link(journal, neighbor)
    original = neighbor.read_bytes()
    with pytest.raises(ValueError, match="reset"):
        reset_module().load_journal(repo.home)
    assert neighbor.read_bytes() == original


def test_inventory_publication_failure_prevents_first_deletion(service, monkeypatch):
    reset, _, credentials = service
    module = reset_module()
    write = module.write_journal

    def refuse_inventory(home, journal):
        if journal.get("credential_targets"):
            raise OSError("synthetic disk failure")
        write(home, journal)

    monkeypatch.setattr(module, "write_journal", refuse_inventory)
    result = confirm(reset)
    assert result.state == "credential_error"
    assert credentials.remaining == {"owned-credential"}


def test_startup_rejects_junction_before_resolving_home(tmp_path, monkeypatch):
    import quantix.__main__ as entry
    reset_module()
    home = tmp_path / "home"
    home.mkdir()
    original = Path.is_junction
    monkeypatch.setattr(Path, "is_junction", lambda value: value == home or original(value))
    monkeypatch.setattr("sys.argv", ["quantix", "--home", str(home)])
    monkeypatch.setattr("quantix.storage.prepare_process_environment", lambda *_: pytest.fail("No runtime initialization for a redirected home"))
    with pytest.raises(ValueError, match="junction"):
        entry.main()
