import hashlib
import importlib
import json
import sqlite3
import stat
from pathlib import Path
from zipfile import ZipFile, ZipInfo

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.repository import Repository


def save_source(repo, tid, data=b"Original tender bytes", name="Sources/spec.pdf"):
    digest = hashlib.sha256(data).hexdigest()
    (repo.objects / digest).write_bytes(data)
    repo.register_artifact(
        tid,
        name,
        digest,
        len(data),
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [{"locator": "page:1", "text": "Concrete scope"}],
        },
    )
    return digest


@pytest.fixture
def setup(tmp_path):
    repo = Repository(tmp_path / "workspace")
    tid = repo.create_tender("Before backup")["id"]
    digest = save_source(repo, tid)
    module = importlib.import_module("quantix.backup")
    return repo, tid, digest, module.BackupService(repo), module


def archive_path(repo, result):
    return repo.home / "backups" / result["filename"]


def test_restore_consent_is_bound_to_the_inspected_archive(setup):
    repo, tid, digest, service, module = setup
    first = service.create()
    path = archive_path(repo, first)
    inspected_hash = service.inspect(path)["backup"]["sha256"]
    repo.create_tender("Added after first backup")
    second = service.create()
    path.write_bytes(archive_path(repo, second).read_bytes())
    with pytest.raises(ValueError, match="changed"):
        service.stage_restore(
            path, True, "Restore the inspected backup", expected_sha256=inspected_hash
        )
    assert not (repo.home / "pending-restore.json").exists()
    assert len(repo.list_tenders()) == 2


def rewrite(source, destination, transform=None, extra=None):
    with ZipFile(source) as old, ZipFile(destination, "w") as new:
        for entry in old.infolist():
            data = old.read(entry.filename)
            new.writestr(entry, transform(entry.filename, data) if transform else data)
        if extra:
            new.writestr(*extra)


def test_backup_contains_snapshot_references_and_excludes_secrets_and_caches(setup):
    repo, tid, digest, service, module = setup
    for name in ["credentials.json", ".env", "semantic.sqlite", "semantic.sqlite-wal"]:
        (repo.home / name).write_text("PRIVATE SECRET OR CACHE", encoding="utf-8")
    (repo.home / "models").mkdir()
    (repo.home / "models" / "weights.bin").write_bytes(b"MODEL")
    repo.set_setting("api_key", "SECRET_IN_DATABASE")
    repo.set_setting("preferences", "Use metric units")
    backup = service.create()
    with ZipFile(archive_path(repo, backup)) as archive:
        assert set(archive.namelist()) == {"manifest.json", "quantix.sqlite", f"objects/{digest}"}
        assert archive.read(f"objects/{digest}") == b"Original tender bytes"
        assert b"SECRET_IN_DATABASE" not in archive.read("quantix.sqlite")
    assert repo.setting("api_key") == "SECRET_IN_DATABASE"
    assert (repo.objects / digest).read_bytes() == b"Original tender bytes"
    assert service.inspect(archive_path(repo, backup))["valid"] is True
    assert service.list()[0]["id"] == backup["id"]


def test_backup_uses_references_from_snapshot_not_later_live_writes(setup, monkeypatch):
    repo, tid, digest, service, module = setup
    snapshot = module._snapshot_database

    def take_then_change(home, target):
        snapshot(home, target)
        save_source(repo, tid, b"New later source", "Sources/later.pdf")
        repo.create_tender("Created after snapshot")

    monkeypatch.setattr(module, "_snapshot_database", take_then_change)
    backup = service.create()
    with ZipFile(archive_path(repo, backup)) as archive:
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["tender_count"] == 1
        assert manifest["original_count"] == 1
        assert len([name for name in archive.namelist() if name.startswith("objects/")]) == 1


def test_backup_restores_extended_outputs_and_frozen_submission(setup):
    from quantix.outputs import OutputService
    from quantix.submissions import SubmissionService

    repo, tid, digest, service, module = setup
    outputs = OutputService(repo)
    decision = {"engineer_confirmed": True, "rationale": "Review scoped registers"}
    draft = outputs.generate(tid, {**decision, "kind": "registers_xlsx"})
    submissions = SubmissionService(repo)
    preview = submissions.preview(tid, {"output_ids": [draft["id"]]})
    frozen = submissions.approve(
        tid,
        {
            **decision,
            "output_ids": [draft["id"]],
            "fingerprint": preview["fingerprint"],
            "final_review_confirmed": True,
            "acknowledged_scope": "Recorded registers only",
            "acknowledged_gaps": preview["warnings"],
        },
    )
    backup = service.create()
    assert backup["output_count"] == 2
    with ZipFile(archive_path(repo, backup)) as archive:
        for saved in (draft, frozen):
            assert (
                hashlib.sha256(archive.read("outputs/" + saved["filename"])).hexdigest()
                == saved["sha256"]
            )
    # Restore repairs both the selected draft and its frozen approved export.
    outputs.path(tid, draft["id"]).unlink()
    submissions.path(tid, frozen["id"]).unlink()
    service.stage_restore(
        archive_path(repo, backup),
        True,
        "Recover missing exports",
        expected_sha256=backup["sha256"],
    )
    module.apply_pending_restore(repo.home)
    restored = Repository(repo.home)
    assert OutputService(restored).path(tid, draft["id"]).is_file()
    assert SubmissionService(restored).path(tid, frozen["id"]).is_file()


def test_stage_is_consent_gated_and_does_not_restore_live_database(setup):
    repo, tid, digest, service, module = setup
    backup = service.create()
    repo.create_tender("After backup")
    with pytest.raises(ValueError):
        service.stage_restore(
            archive_path(repo, backup),
            False,
            "No consent",
            expected_sha256=hashlib.sha256(
                Path(archive_path(repo, backup)).read_bytes()
            ).hexdigest(),
        )
    with pytest.raises(ValueError):
        service.stage_restore(
            archive_path(repo, backup),
            True,
            " ",
            expected_sha256=hashlib.sha256(
                Path(archive_path(repo, backup)).read_bytes()
            ).hexdigest(),
        )
    staged = service.stage_restore(
        archive_path(repo, backup),
        True,
        "Restore the earlier workspace.",
        expected_sha256=hashlib.sha256(Path(archive_path(repo, backup)).read_bytes()).hexdigest(),
    )
    assert staged["restart_required"] is True
    assert staged["status"] == "restore_ready"
    assert len(repo.list_tenders()) == 2


@pytest.mark.parametrize(
    "malicious", ["../outside", "/absolute", "C:/escape", "objects\\escape", "objects/../escape"]
)
def test_unsafe_zip_paths_never_escape_staging(setup, tmp_path, malicious):
    repo, tid, digest, service, module = setup
    backup = service.create()
    bad = tmp_path / "unsafe.zip"
    rewrite(archive_path(repo, backup), bad, extra=(malicious, b"bad"))
    with pytest.raises(ValueError):
        service.stage_restore(
            bad,
            True,
            "Try restore",
            expected_sha256=hashlib.sha256(Path(bad).read_bytes()).hexdigest(),
        )
    assert not (tmp_path / "outside").exists()
    assert not (repo.home / "pending-restore.json").exists()


@pytest.mark.parametrize("kind", ["symlink", "duplicate", "hash", "newer"])
def test_corrupt_or_unsupported_archive_is_rejected(setup, tmp_path, kind):
    repo, tid, digest, service, module = setup
    backup = service.create()
    bad = tmp_path / "bad.zip"
    if kind == "symlink":
        entry = ZipInfo("objects/link")
        entry.create_system = 3
        entry.external_attr = (stat.S_IFLNK | 0o777) << 16
        rewrite(archive_path(repo, backup), bad, extra=(entry, b"/outside"))
    elif kind == "duplicate":
        with pytest.warns(UserWarning):
            rewrite(archive_path(repo, backup), bad, extra=("quantix.sqlite", b"duplicate"))
    else:

        def alter(name, data):
            if kind == "hash" and name == f"objects/{digest}":
                return b"Changed data"
            if kind == "newer" and name == "manifest.json":
                manifest = json.loads(data)
                manifest["format_version"] = 999
                return json.dumps(manifest).encode()
            return data

        rewrite(archive_path(repo, backup), bad, transform=alter)
    with pytest.raises(ValueError):
        service.inspect(bad)


def test_restore_preserves_before_state_and_originals_in_versioned_backup(setup):
    repo, tid, digest, service, module = setup
    first = service.create()
    repo.create_tender("After backup")
    newer = save_source(repo, tid, b"New revision", "Sources/spec.pdf")
    (repo.home / "semantic.sqlite").write_bytes(b"derived old index")
    service.stage_restore(
        archive_path(repo, first),
        True,
        "Return to saved revision",
        expected_sha256=hashlib.sha256(Path(archive_path(repo, first)).read_bytes()).hexdigest(),
    )
    result = module.apply_pending_restore(repo.home)
    restored = Repository(repo.home)
    assert [tender["name"] for tender in restored.list_tenders()] == ["Before backup"]
    assert (repo.objects / digest).read_bytes() == b"Original tender bytes"
    assert (repo.objects / newer).read_bytes() == b"New revision"
    assert not (repo.home / "semantic.sqlite").exists()
    before = next(row for row in service.list() if row["id"] == result["before_restore_backup_id"])
    assert service.inspect(archive_path(repo, before))["backup"]["tender_count"] == 2
    assert not (repo.home / "pending-restore.json").exists()
    assert module.apply_pending_restore(repo.home) is None


@pytest.mark.parametrize("point", ["before_swap", "after_swap"])
def test_interrupted_restore_replays_before_database_open_without_mixed_wal(
    setup, monkeypatch, point
):
    repo, tid, digest, service, module = setup
    first = service.create()
    repo.create_tender("After backup")
    service.stage_restore(
        archive_path(repo, first),
        True,
        "Restore saved state",
        expected_sha256=hashlib.sha256(Path(archive_path(repo, first)).read_bytes()).hexdigest(),
    )
    replace = module._replace_file
    fired = False

    def interrupt(source, target):
        nonlocal fired
        if Path(target).name == "quantix.sqlite" and not fired:
            fired = True
            if point == "before_swap":
                raise OSError("Simulated interruption")
            replace(source, target)
            raise OSError("Simulated interruption")
        return replace(source, target)

    monkeypatch.setattr(module, "_replace_file", interrupt)
    with pytest.raises(OSError, match="Simulated"):
        module.apply_pending_restore(repo.home)
    assert json.loads((repo.home / "pending-restore.json").read_text())["phase"] == "installing"
    # Recovery must discard these before it opens/replaces the target database.
    (repo.home / "quantix.sqlite-wal").write_bytes(b"stale unrelated WAL")
    (repo.home / "quantix.sqlite-shm").write_bytes(b"stale unrelated SHM")
    monkeypatch.setattr(module, "_replace_file", replace)
    module.apply_pending_restore(repo.home)
    assert [t["name"] for t in Repository(repo.home).list_tenders()] == ["Before backup"]
    assert not (repo.home / "quantix.sqlite-wal").exists()


def test_snapshot_includes_committed_wal_data(setup):
    repo, tid, digest, service, module = setup
    connection = sqlite3.connect(repo.db.path)
    connection.execute("PRAGMA journal_mode=WAL")
    connection.execute("PRAGMA wal_autocheckpoint=0")
    connection.execute("UPDATE tenders SET name='Committed in WAL' WHERE id=?", (tid,))
    connection.commit()
    try:
        backup = service.create()
        service.stage_restore(
            archive_path(repo, backup),
            True,
            "Use saved state",
            expected_sha256=hashlib.sha256(
                Path(archive_path(repo, backup)).read_bytes()
            ).hexdigest(),
        )
    finally:
        connection.close()
    module.apply_pending_restore(repo.home)
    assert Repository(repo.home).get_tender(tid)["name"] == "Committed in WAL"


def test_routes_publish_restore_ready_without_mutating_live_state(setup):
    repo, tid, digest, service, module = setup
    routes = importlib.import_module("quantix.backup_routes")
    app = FastAPI()
    app.include_router(routes.create_router(repo))
    client = TestClient(app)
    made = client.post("/api/backups")
    assert made.status_code == 200
    path = str(archive_path(repo, made.json()))
    assert client.post("/api/backups/restore", json={"path": path}).status_code == 422
    ready = client.post(
        "/api/backups/restore",
        json={
            "path": path,
            "engineer_confirmed": True,
            "rationale": "Restore saved state",
            "expected_sha256": made.json()["sha256"],
        },
    )
    assert ready.status_code == 200
    assert ready.json()["restart_required"] is True
    assert "BackupInspection" in app.openapi()["components"]["schemas"]


def test_registered_outputs_and_all_original_revisions_restore_into_new_home(setup):
    from quantix.outputs import OutputService

    repo, tid, digest, service, module = setup
    newer = save_source(repo, tid, b"Second revision", "Sources/spec.pdf")
    output = OutputService(repo).generate(
        tid,
        {
            "kind": "analysis_docx",
            "engineer_confirmed": True,
            "rationale": "Prepare a draft report",
        },
    )
    backup = service.create()
    with ZipFile(archive_path(repo, backup)) as archive:
        assert f"objects/{digest}" in archive.namelist()
        assert f"objects/{newer}" in archive.namelist()
        assert f"outputs/{output['filename']}" in archive.namelist()
        assert "outputs/" + str(Path(output["filename"]).with_suffix(".json")) in archive.namelist()
    destination = Repository(repo.home.parent / "restored-workspace")
    module.BackupService(destination).stage_restore(
        archive_path(repo, backup),
        True,
        "Restore this saved workspace",
        expected_sha256=hashlib.sha256(Path(archive_path(repo, backup)).read_bytes()).hexdigest(),
    )
    module.apply_pending_restore(destination.home)
    restored_output = OutputService(Repository(destination.home)).path(tid, output["id"])
    assert hashlib.sha256(restored_output.read_bytes()).hexdigest() == output["sha256"]


def test_locked_database_stops_restore_before_publication_and_can_retry(setup):
    repo, tid, digest, service, module = setup
    backup = service.create()
    repo.create_tender("Still current")
    service.stage_restore(
        archive_path(repo, backup),
        True,
        "Restore the saved point",
        expected_sha256=hashlib.sha256(Path(archive_path(repo, backup)).read_bytes()).hexdigest(),
    )
    connection = sqlite3.connect(repo.db.path)
    connection.execute("BEGIN IMMEDIATE")
    try:
        with pytest.raises((ValueError, sqlite3.OperationalError)):
            module.apply_pending_restore(repo.home)
        assert len(repo.list_tenders()) == 2
        assert json.loads((repo.home / "pending-restore.json").read_text())["phase"] == "prepared"
    finally:
        connection.close()
    module.apply_pending_restore(repo.home)
    assert len(Repository(repo.home).list_tenders()) == 1


def test_changed_staged_archive_aborts_startup_without_changing_database(setup):
    repo, tid, digest, service, module = setup
    backup = service.create()
    staged = service.stage_restore(
        archive_path(repo, backup),
        True,
        "Restore saved point",
        expected_sha256=hashlib.sha256(Path(archive_path(repo, backup)).read_bytes()).hexdigest(),
    )
    repo.create_tender("Must remain")
    (repo.home / "restore-staging" / staged["restore_id"] / "source.zip").write_bytes(b"changed")
    with pytest.raises(ValueError, match="changed"):
        module.apply_pending_restore(repo.home)
    assert len(repo.list_tenders()) == 2


def test_missing_original_prevents_a_false_successful_backup(setup):
    repo, tid, digest, service, module = setup
    (repo.objects / digest).unlink()
    with pytest.raises(ValueError, match="missing"):
        service.create()
    assert service.list() == []


def test_workspace_link_cannot_redirect_backup_reads(setup, tmp_path):
    repo, tid, digest, service, module = setup
    original = repo.home / "objects-preserved"
    repo.objects.rename(original)
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / digest).write_bytes(b"private outside data")
    try:
        repo.objects.symlink_to(outside, target_is_directory=True)
    except OSError:
        original.rename(repo.objects)
        pytest.skip("Windows does not permit test symlink creation.")
    with pytest.raises(ValueError, match="workspace|links|junctions"):
        service.create()
    assert (outside / digest).read_bytes() == b"private outside data"


def test_restore_extraction_destination_must_stay_inside_target_home(setup, tmp_path):
    repo, tid, digest, service, module = setup
    backup = service.create()
    outside = tmp_path / "not-the-workspace"
    with pytest.raises(ValueError, match="workspace"):
        module._validate_archive(repo.home, archive_path(repo, backup), outside)
    assert not outside.exists()


@pytest.mark.parametrize("damage", ["missing_original", "changed_original", "missing_output"])
def test_healthy_backup_recovers_damaged_workspace_with_explicit_recovery_evidence(
    setup, tmp_path, damage
):
    from quantix.outputs import OutputService

    repo, tid, digest, service, module = setup
    output = OutputService(repo).generate(
        tid,
        {
            "kind": "analysis_docx",
            "engineer_confirmed": True,
            "rationale": "Prepare synthetic draft",
        },
    )
    backup = service.create()
    repo.create_tender("Pre-restore evidence must survive")
    repo.set_setting("api_key", "SYNTHETIC_SECRET_EXCLUDED_FROM_RECOVERY")
    (repo.home / "credentials.json").write_text("SYNTHETIC_SECRET", encoding="utf-8")
    damaged_path = (
        "outputs/" + output["filename"] if damage == "missing_output" else "objects/" + digest
    )
    target = repo.home / damaged_path
    if damage == "changed_original":
        target.write_bytes(b"Changed original bytes retained as recovery evidence")
    else:
        target.unlink()
    service.stage_restore(
        archive_path(repo, backup),
        True,
        "Recover damaged workspace",
        expected_sha256=backup["sha256"],
    )
    restored = module.apply_pending_restore(repo.home)
    assert restored["status"] == "restored"
    assert target.exists()
    assert not (repo.home / "pending-restore.json").exists()
    recovery = repo.home / restored["recovery_evidence_path"]
    with ZipFile(recovery) as archive:
        assert archive.testzip() is None
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["archive_type"] == "quantix-recovery-evidence"
        assert manifest["complete_backup"] is False
        if damage == "changed_original":
            saved = next(item for item in manifest["files"] if item["path"] == damaged_path)
            assert saved["state"] == "changed"
            assert saved["expected_sha256"] == digest
            assert (
                archive.read(damaged_path)
                == b"Changed original bytes retained as recovery evidence"
            )
        else:
            assert damaged_path in {item["path"] for item in manifest["missing_files"]}
        snapshot_path = tmp_path / "before-restore-evidence.sqlite"
        snapshot_path.write_bytes(archive.read("quantix.sqlite"))
        assert "credentials.json" not in archive.namelist()
        assert b"SYNTHETIC_SECRET_EXCLUDED_FROM_RECOVERY" not in archive.read("quantix.sqlite")
        for saved_file in manifest["files"]:
            assert (
                hashlib.sha256(archive.read(saved_file["path"])).hexdigest() == saved_file["sha256"]
            )
    connection = sqlite3.connect(snapshot_path)
    try:
        assert connection.execute("SELECT COUNT(*) FROM tenders").fetchone()[0] == 2
    finally:
        connection.close()
    assert len(Repository(repo.home).list_tenders()) == 1
    with pytest.raises(ValueError, match="recovery|restorable"):
        service.inspect(recovery)
    latest = service.latest_restore()
    assert latest["restore_id"] == restored["restore_id"]
    assert latest["completed_at"]
    assert latest["recovery_evidence_path"] == restored["recovery_evidence_path"]


def test_latest_restore_is_none_until_completion_and_survives_reopening(setup):
    repo, tid, digest, service, module = setup
    assert service.latest_restore() is None
    backup = service.create()
    service.stage_restore(
        archive_path(repo, backup),
        True,
        "Restore saved workspace",
        expected_sha256=backup["sha256"],
    )
    assert service.latest_restore() is None
    restored = module.apply_pending_restore(repo.home)
    latest = module.BackupService(Repository(repo.home)).latest_restore()
    assert latest["restore_id"] == restored["restore_id"]
    assert latest["recovery_evidence_path"] is None
    assert latest["before_restore_backup_id"] == restored["before_restore_backup_id"]


def test_interrupted_damaged_restore_reuses_preserved_evidence_and_removes_old_wal(
    setup, monkeypatch
):
    repo, tid, digest, service, module = setup
    backup = service.create()
    repo.create_tender("Pre-restore state")
    (repo.objects / digest).write_bytes(b"Changed bytes must remain recoverable")
    service.stage_restore(
        archive_path(repo, backup), True, "Recover saved point", expected_sha256=backup["sha256"]
    )
    install = module._install

    def interrupt(*args):
        raise OSError("Simulated interruption before publication")

    monkeypatch.setattr(module, "_install", interrupt)
    with pytest.raises(OSError, match="Simulated"):
        module.apply_pending_restore(repo.home)
    pending = json.loads((repo.home / "pending-restore.json").read_text())
    assert pending["phase"] == "installing"
    recovery = repo.home / pending["recovery_evidence_path"]
    evidence_hash = hashlib.sha256(recovery.read_bytes()).hexdigest()
    assert service.latest_restore() is None
    (repo.home / "quantix.sqlite-wal").write_bytes(b"Unrelated stale WAL")
    (repo.home / "quantix.sqlite-shm").write_bytes(b"Unrelated stale SHM")
    monkeypatch.setattr(module, "_install", install)
    restored = module.apply_pending_restore(repo.home)
    assert restored["recovery_evidence_path"] == pending["recovery_evidence_path"]
    assert hashlib.sha256(recovery.read_bytes()).hexdigest() == evidence_hash
    assert len(list((repo.home / "backups").glob("*.recovery.zip"))) == 1
    assert (repo.objects / digest).read_bytes() == b"Original tender bytes"
    assert len(Repository(repo.home).list_tenders()) == 1
    assert not (repo.home / "quantix.sqlite-wal").exists()


def test_latest_restore_requires_a_recorded_completion_time(setup):
    repo, tid, digest, service, module = setup
    backup = service.create()
    service.stage_restore(
        archive_path(repo, backup), True, "Restore saved point", expected_sha256=backup["sha256"]
    )
    restored = module.apply_pending_restore(repo.home)
    history = repo.home / "restore-history"
    saved = history / (restored["restore_id"] + ".json")
    complete = json.loads(saved.read_text())
    old = dict(complete)
    old.pop("completed_at")
    old.pop("recovery_evidence_path")
    saved.write_text(json.dumps(old), encoding="utf-8")
    (history / ("1" * 32 + ".json")).write_text("invalid JSON", encoding="utf-8")
    wrong = old | {"restore_id": "2" * 32}
    (history / ("3" * 32 + ".json")).write_text(json.dumps(wrong), encoding="utf-8")
    assert service.latest_restore() is None
    saved.write_text(json.dumps(complete), encoding="utf-8")
    latest = service.latest_restore()
    assert latest["restore_id"] == restored["restore_id"]
    assert latest["completed_at"]
    assert latest["recovery_evidence_path"] is None
