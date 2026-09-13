"""Manager profile snapshots captured at run admission."""

from concurrent.futures import ThreadPoolExecutor

import pytest

from quantix import diagnostics
from quantix.manager_profile import ManagerProfileService
from quantix.manager_runtime import ManagerRunProfiles, prompt_profile
from quantix.repository import Repository
from quantix.staff_models import ManagerProfileEdit


@pytest.fixture(autouse=True)
def close_temporary_diagnostics():
    before = diagnostics._writer
    yield
    after = diagnostics._writer
    if after is not None and after is not before:
        after.close()


def test_capture_pins_version_and_get_remains_historical_after_edit(tmp_path):
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Synthetic Tender")
    run = repo.create_run(tender["id"], "conversation", "Review")
    profiles = ManagerRunProfiles(repo)
    first = profiles.capture(tender["id"], run["id"])

    updated = ManagerProfileService(repo).update(
        ManagerProfileEdit(
            expected_version=first.version,
            display_name="Updated Manager",
            title=first.title,
            persona=first.persona,
            personality=first.personality,
            working_preferences=first.working_preferences,
        )
    )

    assert updated.version == first.version + 1
    assert profiles.get(tender["id"], run["id"]).display_name == first.display_name
    with repo.db.connect() as conn:
        row = conn.execute(
            "SELECT tender_id,manager_id,manager_version FROM office_manager_run_profiles WHERE run_id=?",
            (run["id"],),
        ).fetchone()
    assert tuple(row) == (tender["id"], first.id, first.version)


def test_new_run_captures_new_version_and_reopen_preserves_pin(tmp_path):
    home = tmp_path / "home"
    repo = Repository(home)
    tender = repo.create_tender("Synthetic Tender")
    first_run = repo.create_run(tender["id"], "manager", "First")
    profiles = ManagerRunProfiles(repo)
    first = profiles.capture(tender["id"], first_run["id"])
    service = ManagerProfileService(repo)
    second_version = service.update(
        ManagerProfileEdit(
            expected_version=first.version,
            display_name="Second Manager",
            title=first.title,
            persona=first.persona,
            personality=first.personality,
            working_preferences=first.working_preferences,
        )
    )
    second_run = repo.create_run(tender["id"], "manager", "Second")
    second = profiles.capture(tender["id"], second_run["id"])
    reopened = ManagerRunProfiles(Repository(home))

    assert second.version == second_version.version
    assert reopened.get(tender["id"], first_run["id"]).version == first.version
    assert reopened.get(tender["id"], second_run["id"]).version == second.version


def test_capture_is_idempotent_after_terminal_and_rejects_uncaptured_terminal(tmp_path):
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Synthetic Tender")
    captured_run = repo.create_run(tender["id"], "manager", "Captured")
    profiles = ManagerRunProfiles(repo)
    first = profiles.capture(tender["id"], captured_run["id"])
    repo.update_run(captured_run["id"], status="completed")
    assert profiles.capture(tender["id"], captured_run["id"]).version == first.version

    uncaptured = repo.create_run(tender["id"], "manager", "Never captured")
    repo.update_run(uncaptured["id"], status="completed")
    with pytest.raises(ValueError, match="queued or running"):
        profiles.capture(tender["id"], uncaptured["id"])


def test_cross_tender_run_is_rejected_and_missing_pin_cannot_be_read(tmp_path):
    repo = Repository(tmp_path / "home")
    first = repo.create_tender("First Tender")
    second = repo.create_tender("Second Tender")
    run = repo.create_run(first["id"], "manager", "First")
    profiles = ManagerRunProfiles(repo)

    with pytest.raises(KeyError):
        profiles.capture(second["id"], run["id"])
    with pytest.raises(KeyError):
        profiles.get(first["id"], "missing-run")


def test_outer_admission_rollback_removes_run_and_profile_pin(tmp_path):
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Synthetic Tender")
    profiles = ManagerRunProfiles(repo)

    with pytest.raises(RuntimeError):
        with repo.atomic():
            run = repo.create_run(tender["id"], "conversation", "Rollback")
            profiles.capture(tender["id"], run["id"])
            raise RuntimeError("force rollback")

    with repo.db.connect() as conn:
        assert conn.execute("SELECT COUNT(*) FROM runs").fetchone()[0] == 0
        assert conn.execute("SELECT COUNT(*) FROM office_manager_run_profiles").fetchone()[0] == 0


def test_concurrent_duplicate_capture_keeps_one_pinned_version(tmp_path):
    home = tmp_path / "home"
    repo = Repository(home)
    tender = repo.create_tender("Synthetic Tender")
    run = repo.create_run(tender["id"], "conversation", "Concurrent")

    def capture_once(_):
        return ManagerRunProfiles(Repository(home)).capture(tender["id"], run["id"])

    with ThreadPoolExecutor(max_workers=6) as workers:
        captured = list(workers.map(capture_once, range(12)))

    assert {(item.id, item.version) for item in captured} == {(captured[0].id, captured[0].version)}
    with repo.db.connect() as conn:
        assert conn.execute(
            "SELECT COUNT(*) FROM office_manager_run_profiles WHERE run_id=?", (run["id"],)
        ).fetchone()[0] == 1


def test_prompt_profile_contains_complete_user_fields_without_authority(tmp_path):
    repo = Repository(tmp_path / "home")
    profile = ManagerProfileService(repo).get()
    value = prompt_profile(profile)

    assert set(value) == {"display_name", "title", "persona", "personality", "working_preferences"}
    assert set(value["personality"]) == {
        "description",
        "traits",
        "communication_style",
        "problem_solving_style",
        "collaboration_style",
        "uncertainty_handling",
        "initiative",
        "explanation_style",
        "language_preferences",
        "working_habits",
    }
    assert "account" not in value
    assert "grant" not in value
