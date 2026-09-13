"""T001: working-baseline protection and executable acceptance fixtures."""

from __future__ import annotations

import importlib.util
import json
import sqlite3
from pathlib import Path

import pytest

from quantix.db import Database, SchemaMigrationError, apply_forward_migrations
from quantix.repository import Repository

from .baseline import capture_tree, copy_tree, integrate_tree
from .cases import CaseRegistry, drive_t001
from .fixtures import GENERATED_TEST_COLLEAGUE, write_package


def _load_export_module():
    path = Path(__file__).resolve().parents[3] / "scripts" / "export_openapi.py"
    spec = importlib.util.spec_from_file_location("quantix_export_openapi", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def test_t001(run_case):
    result = run_case("T001", {"scenario": "isolated_baseline", "tenders": 2, "automatic_ai": False})
    assert result["specialists_per_tender"] == [0, 0]
    assert result["original_hashes_preserved"] is True
    assert result["provider_requests"] == 0


def test_t001_missing_case_id_raises(run_case):
    with pytest.raises(LookupError, match="Acceptance driver is not implemented"):
        run_case("T999-missing", {"scenario": "isolated_baseline"})


def test_t001_duplicate_register_rejected(case_registry):
    with pytest.raises(ValueError, match="Duplicate acceptance case"):
        case_registry.register("T001", drive_t001)


def test_t001_idle_driver_is_not_the_registered_driver():
    """A driver that returns expected values without using the service is not registered."""

    def idle(_env, _inputs):
        return {
            "specialists_per_tender": [0, 0],
            "original_hashes_preserved": True,
            "provider_requests": 0,
        }

    registry = CaseRegistry()
    registry.register("T001", drive_t001)
    assert registry.drivers["T001"] is drive_t001
    assert registry.drivers["T001"] is not idle


def test_t001_altered_working_file_blocks_integration(tmp_path):
    original = tmp_path / "working"
    (original / "src").mkdir(parents=True)
    (original / "src" / "app.py").write_text("print('baseline')\n", encoding="utf-8")
    (original / "docs").mkdir()
    (original / "docs" / "contracts.md").write_text("# contracts\n", encoding="utf-8")
    captured = capture_tree(original)
    isolated = tmp_path / "isolated"
    copy_tree(original, isolated)
    (isolated / "src" / "app.py").write_text("print('implemented')\n", encoding="utf-8")
    (original / "src" / "app.py").write_text("print('engineer edit')\n", encoding="utf-8")
    with pytest.raises(ValueError, match="not overwritten"):
        integrate_tree(isolated, original, captured)
    assert (original / "src" / "app.py").read_text(encoding="utf-8") == "print('engineer edit')\n"


def test_t001_failed_schema_migration_keeps_prior_database(tmp_path):
    home = tmp_path / "home"
    repo = Repository(home)
    tender = repo.create_tender("Keep this Tender")
    with repo.db.connect() as conn:
        before = conn.execute("PRAGMA user_version").fetchone()[0]
        assert before >= 1

    def boom(conn):
        conn.execute("ALTER TABLE tenders ADD COLUMN extra TEXT")
        raise RuntimeError("synthetic migration failure")

    with pytest.raises(SchemaMigrationError, match="still usable"):
        apply_forward_migrations(repo.db.path, migrations={before + 1: boom})

    recovered = Database(home)
    with recovered.connect() as conn:
        assert conn.execute("PRAGMA user_version").fetchone()[0] == before
        columns = {row[1] for row in conn.execute("PRAGMA table_info(tenders)")}
        assert "extra" not in columns
        row = conn.execute("SELECT id, name FROM tenders WHERE id=?", (tender["id"],)).fetchone()
        assert row["name"] == "Keep this Tender"


def test_t001_pre_migration_backup_restore(tmp_path):
    from quantix.backup import BackupService, apply_pending_restore

    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Backup before migration")
    service = BackupService(repo)
    backup = service.create()
    repo.create_tender("Added after backup")
    assert len(repo.list_tenders()) == 2
    service.stage_restore(
        repo.home / "backups" / backup["filename"],
        True,
        "Restore the pre-migration workspace",
        expected_sha256=backup["sha256"],
    )
    apply_pending_restore(repo.home)
    restored = Repository(repo.home)
    names = [item["name"] for item in restored.list_tenders()]
    assert names == ["Backup before migration"]
    assert tender["id"] == restored.list_tenders()[0]["id"]


def test_t001_schema_export_refuses_pending_reset(tmp_path, monkeypatch):
    home = tmp_path / "quantix-home"
    home.mkdir()
    journal = {
        "format": 1,
        "reset_id": "a" * 32,
        "home": str(home.absolute()),
        "phase": "ready",
        "credentials_cleared": True,
        "fingerprint": "b" * 64,
        "confirmed_at": "2026-09-10T12:00:00+00:00",
        "detail": "Ready",
        "credential_targets": [],
    }
    (home / "pending-reset.json").write_text(json.dumps(journal), encoding="utf-8")
    before = {path.relative_to(home).as_posix() for path in home.rglob("*")}
    module = _load_export_module()
    monkeypatch.setattr("quantix.storage.normal_home", lambda: home)

    def fail_prepare(*_args, **_kwargs):
        pytest.fail("Schema export must refuse before creating ordinary runtime directories")

    monkeypatch.setattr("quantix.storage.prepare_process_environment", fail_prepare)
    with pytest.raises(SystemExit, match="reset is pending"):
        module.export_schema()
    after = {path.relative_to(home).as_posix() for path in home.rglob("*")}
    assert after == before
    assert not (home / "runtime").exists()
    assert not (home / "quantix.sqlite").exists()
    assert not (home / "cache").exists()
    assert not (home / "logs").exists()


def test_t001_schema_export_uses_isolated_home(tmp_path, monkeypatch):
    home = tmp_path / "quantix-home"
    module = _load_export_module()
    monkeypatch.setattr("quantix.storage.normal_home", lambda: home)
    monkeypatch.setattr("keyring.get_password", lambda *_: None)
    destination = module.export_schema()
    assert destination == home / "runtime" / "openapi.json"
    schema = json.loads(destination.read_text(encoding="utf-8"))
    assert schema["info"]["title"] == "Quantix local workspace"
    assert not (home / "quantix.sqlite").exists()
    leftovers = list((home / "runtime" / "tmp").glob("quantix-schema-*"))
    assert leftovers == []


def test_t001_synthetic_package_is_labelled_test_data(tmp_path):
    from docx import Document

    files = write_package(tmp_path / "package")
    quotation = "\n".join(paragraph.text for paragraph in Document(files["quotation"]).paragraphs)
    assert GENERATED_TEST_COLLEAGUE in quotation
    assert files["pdf"].read_bytes().startswith(b"%PDF")
    assert files["xlsx"].suffix == ".xlsx"
    assert files["xlsm"].suffix == ".xlsm"


def test_t001_common_contracts(run_case):
    result = run_case("T001", {"scenario": "isolated_baseline", "tenders": 2, "automatic_ai": False})
    assert result["secret_leaked"] is False
    assert result["get_created_staff"] is False
    assert result["provider_requests"] == 0
    assert result["evidence_kind"] == "synthetic_execution"
    assert result["artifact_count"] >= 2


def test_t001_unsupported_schema_version_is_rejected(tmp_path):
    home = tmp_path / "home"
    Database(home)
    with sqlite3.connect(home / "quantix.sqlite") as conn:
        conn.execute("PRAGMA user_version=99")
        conn.commit()
    with pytest.raises(ValueError, match="unsupported version"):
        Database(home)
