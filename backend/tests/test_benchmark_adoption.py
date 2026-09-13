"""No inference: planted trusted records exercise the adoption boundary."""

import pytest

from quantix.benchmark_adoption import BenchmarkAdoptionService, _hash, configuration_basis
from quantix.benchmark_adoption_models import BenchmarkAdoptionDecision, BenchmarkAdoptionReview
from quantix.db import dump, now


def _block(repo, route, version, *, state="critical_block"):
    service = BenchmarkAdoptionService(repo)
    basis = configuration_basis(repo, route, version)
    record = BenchmarkAdoptionDecision(
        configuration_hash=_hash(basis),
        report_id="test-candidate",
        report_hash="a" * 64,
        baseline_report_id="test-baseline",
        baseline_report_hash="b" * 64,
        state=state,
        connection_id=route["connection_id"],
        model_id=route["model_id"],
        manager_profile_version=version,
        reasons=["Planted test-only critical error"],
        updated_at=now(),
    )
    with repo.atomic() as conn:
        conn.execute(
            "INSERT INTO benchmark_adoption VALUES (?,?)",
            (record.configuration_hash, dump(record.model_dump(mode="json"))),
        )
    return service, record


@pytest.mark.asyncio
async def test_blocked_staff_configuration_makes_zero_provider_calls(tmp_path, monkeypatch):
    from test_staff_runtime import _workspace

    from quantix.manager_runtime import ManagerRunProfiles
    from quantix.staff_runtime import run_staff_assignment

    repo, tender, connection, model, route, staff, binding, assignment, source = _workspace(
        tmp_path, monkeypatch
    )
    version = ManagerRunProfiles(repo).get(tender["id"], assignment.root_run_id).version
    _block(repo, binding.route.model_dump(mode="json"), version)
    calls = []

    async def forbidden(*args, **kwargs):
        calls.append(True)
        raise AssertionError("Provider must not be reached")

    monkeypatch.setattr("quantix.staff_runtime.execute_api", forbidden)
    with pytest.raises(ValueError, match="recorded benchmark critical"):
        await run_staff_assignment(repo, tender["id"], assignment.id)
    assert calls == []


def test_engineer_review_cannot_clear_critical_and_exact_hashes_are_required(tmp_path, monkeypatch):
    from test_staff_runtime import _workspace

    from quantix.manager_runtime import ManagerRunProfiles

    repo, tender, *rest = _workspace(tmp_path, monkeypatch)
    binding, assignment = rest[4], rest[5]
    version = ManagerRunProfiles(repo).get(tender["id"], assignment.root_run_id).version
    service, record = _block(repo, binding.route.model_dump(mode="json"), version)
    review = BenchmarkAdoptionReview(
        **{
            key: getattr(record, key)
            for key in (
                "configuration_hash",
                "report_id",
                "report_hash",
                "baseline_report_id",
                "baseline_report_hash",
            )
        },
        engineer_confirmed=True,
        rationale="Test engineer reviewed this comparison",
    )
    with pytest.raises(ValueError, match="Critical errors cannot"):
        service.review(review)
    with repo.atomic() as conn:
        record.state = "review_required"
        conn.execute(
            "UPDATE benchmark_adoption SET data_json=?", (dump(record.model_dump(mode="json")),)
        )
    with pytest.raises(ValueError, match="comparison changed"):
        service.review(review.model_copy(update={"report_hash": "c" * 64}))
    with pytest.raises(ValueError, match="missing"):
        service.review(review)


def test_fake_execution_cannot_register_live_acceptance(tmp_path):
    from quantix.benchmarks.runner import new_report
    from quantix.repository import Repository

    service = BenchmarkAdoptionService(Repository(tmp_path))
    report = new_report("live", repetitions=3)
    report.overall_completed = True
    with pytest.raises(ValueError, match="Only real admitted"):
        service.record_live_report(object(), report)
    assert service.list() == []


@pytest.mark.asyncio
async def test_every_request_rechecks_gate_and_unseen_configuration_is_allowed(
    tmp_path, monkeypatch
):
    from test_staff_runtime import _workspace

    from quantix.manager_runtime import ManagerRunProfiles

    repo, tender, connection, model, route, staff, binding, assignment, source = _workspace(
        tmp_path, monkeypatch
    )
    service = BenchmarkAdoptionService(repo)
    calls = []

    async def budget(*args, **kwargs):
        calls.append(True)
        return "reservation"

    selected = binding.route.model_dump(mode="json")
    checked = service.guard(tender["id"], assignment.root_run_id, selected, budget)
    assert await checked(1, 1) == "reservation"
    version = ManagerRunProfiles(repo).get(tender["id"], assignment.root_run_id).version
    _block(repo, selected, version)
    with pytest.raises(ValueError, match="recorded benchmark critical"):
        await checked(1, 1)
    assert len(calls) == 1


@pytest.mark.asyncio
@pytest.mark.parametrize("operation", ["manager", "conversation"])
async def test_manager_and_classifier_block_before_provider_entry(tmp_path, monkeypatch, operation):
    from test_catalog_authority import configured_office

    from quantix.conversation import run_conversation
    from quantix.manager_runtime import ManagerRunProfiles
    from quantix.office import run_manager

    repo, tender, connections, account, model, policy, route = configured_office(
        tmp_path, monkeypatch
    )
    run = repo.create_run(tender["id"], operation)
    version = ManagerRunProfiles(repo).capture(tender["id"], run["id"]).version
    repo.update_run(run["id"], status="running")
    _block(repo, route, version)
    calls = []

    async def forbidden(*args, **kwargs):
        calls.append(True)
        raise AssertionError("No provider entry for a known blocked full route")

    monkeypatch.setattr("quantix.ai_execution.execute_api", forbidden)
    with pytest.raises(ValueError, match="recorded benchmark critical"):
        await (run_manager if operation == "manager" else run_conversation)(
            repo, tender["id"], run["id"], "Synthetic review"
        )
    assert calls == []


def _planted_verified_report(identifier):
    """Private persistence unit fixture, not provider acceptance evidence."""
    from test_agent_benchmarks import literal_first_case_observation

    from quantix.benchmarks.cases import CASES
    from quantix.benchmarks.models import CaseResult
    from quantix.benchmarks.runner import new_report
    from quantix.benchmarks.scoring import score

    report = new_report("live", repetitions=3)
    report.id = identifier
    value = literal_first_case_observation()
    report.cases = [
        CaseResult(
            case_id=CASES[0].id,
            category=CASES[0].category,
            repetition=1,
            critical=True,
            case_hash=_hash(CASES[0].model_dump(mode="json")),
            observed=value,
            scores=score(CASES[0], value),
        )
    ]
    report.overall_completed = (
        True  # Private storage logic only; public recorder re-collects all 72.
    )
    return report


def test_unknown_metrics_are_not_zero_and_real_comparison_values_are_visible(tmp_path):
    from quantix.repository import Repository

    service = BenchmarkAdoptionService(Repository(tmp_path))
    basis = {
        "route": {"connection_id": "test-account", "model_id": "test-model"},
        "manager_profile_version": 1,
    }
    baseline = _planted_verified_report("d" * 32)
    service._save_verified(baseline, basis)
    candidate = baseline.model_copy(deep=True)
    candidate.id = "e" * 32
    candidate.cases[0].observed.input_tokens = None
    candidate.cases[0].observed.requests = 2
    decision = service._save_verified(candidate, basis)
    assert any("unavailable" in reason and "input tokens" in reason for reason in decision.reasons)
    assert any(
        "1" in reason and "2" in reason and "requests" in reason for reason in decision.reasons
    )


def test_verified_report_id_is_immutable_and_exact_hash_read_is_required(tmp_path):
    from quantix.repository import Repository

    service = BenchmarkAdoptionService(Repository(tmp_path))
    basis = {
        "route": {"connection_id": "test-account", "model_id": "test-model"},
        "manager_profile_version": 1,
    }
    report = _planted_verified_report("d" * 32)
    first = service._save_verified(report, basis)
    assert service._save_verified(report, basis) == first
    changed = report.model_copy(deep=True)
    changed.detail = "A changed report must never replace immutable evidence."
    with pytest.raises(ValueError, match="immutable"):
        service._save_verified(changed, basis)
    assert service.read_report(report.id, first.report_hash).id == report.id
    with pytest.raises(ValueError, match="hash"):
        service.read_report(report.id, "f" * 64)


def test_verified_report_sql_updates_and_deletes_are_blocked(tmp_path):
    import sqlite3

    from quantix.repository import Repository

    service = BenchmarkAdoptionService(Repository(tmp_path))
    basis = {
        "route": {"connection_id": "test-account", "model_id": "test-model"},
        "manager_profile_version": 1,
    }
    report = _planted_verified_report("d" * 32)
    service._save_verified(report, basis)
    for statement in (
        "UPDATE benchmark_verified_reports SET data_json='{}'",
        "DELETE FROM benchmark_verified_reports",
    ):
        with pytest.raises(sqlite3.IntegrityError, match="immutable"):
            with service.repo.atomic() as conn:
                conn.execute(statement)


def test_tampered_baseline_cannot_suppress_regression_or_accept_candidate(tmp_path):
    import json

    from quantix.repository import Repository

    service = BenchmarkAdoptionService(Repository(tmp_path))
    basis = {
        "route": {"connection_id": "test-account", "model_id": "test-model"},
        "manager_profile_version": 1,
    }
    baseline = _planted_verified_report("d" * 32)
    service._save_verified(baseline, basis)
    with service.repo.atomic() as conn:
        conn.execute(
            "DROP TRIGGER IF EXISTS benchmark_report_no_update"
        )  # Simulate damaged legacy/restored data.
        value = json.loads(
            conn.execute("SELECT data_json FROM benchmark_verified_reports").fetchone()[0]
        )
        value["overall_completed"] = False
        conn.execute("UPDATE benchmark_verified_reports SET data_json=?", (dump(value),))
    candidate = baseline.model_copy(deep=True)
    candidate.id = "e" * 32
    with pytest.raises(ValueError, match="repair|hash"):
        service._save_verified(
            candidate, basis | {"route": basis["route"] | {"model_id": "another-config"}}
        )


@pytest.mark.parametrize("state", ["accepted", "review_required"])
@pytest.mark.parametrize("missing", ["candidate", "baseline"])
def test_missing_candidate_cannot_retain_acceptance_or_be_waived(
    tmp_path, monkeypatch, state, missing
):
    from test_catalog_authority import configured_office

    from quantix.manager_runtime import ManagerRunProfiles

    repo, tender, connections, account, model, policy, route = configured_office(
        tmp_path, monkeypatch
    )
    run = repo.create_run(tender["id"], "manager")
    version = ManagerRunProfiles(repo).capture(tender["id"], run["id"]).version
    service = BenchmarkAdoptionService(repo)
    basis = configuration_basis(repo, route, version)
    baseline = _planted_verified_report("d" * 32)
    service._save_verified(baseline, basis)
    candidate = baseline.model_copy(deep=True)
    candidate.id = "e" * 32
    if state == "review_required":
        candidate.cases[0].observed.requests = 2
    decision = service._save_verified(candidate, basis)
    assert decision.state == state
    with repo.atomic() as conn:
        conn.execute(
            "DROP TRIGGER IF EXISTS benchmark_report_no_delete"
        )  # Simulate a damaged restore.
        conn.execute(
            "DELETE FROM benchmark_verified_reports WHERE id=?",
            ((candidate if missing == "candidate" else baseline).id,),
        )
    with pytest.raises(ValueError, match="repair|missing"):
        if state == "accepted":
            service.assert_allowed(tender["id"], run["id"], route)
        else:
            service.review(
                BenchmarkAdoptionReview(
                    **{
                        key: getattr(decision, key)
                        for key in (
                            "configuration_hash",
                            "report_id",
                            "report_hash",
                            "baseline_report_id",
                            "baseline_report_hash",
                        )
                    },
                    engineer_confirmed=True,
                    rationale="A review cannot waive missing evidence.",
                )
            )


def test_complete_immutable_comparison_can_be_reviewed(tmp_path):
    from quantix.repository import Repository

    service = BenchmarkAdoptionService(Repository(tmp_path))
    basis = {
        "route": {"connection_id": "test-account", "model_id": "test-model"},
        "manager_profile_version": 1,
    }
    baseline = _planted_verified_report("d" * 32)
    service._save_verified(baseline, basis)
    candidate = baseline.model_copy(deep=True)
    candidate.id = "e" * 32
    candidate.cases[0].observed.requests = 2
    decision = service._save_verified(candidate, basis)
    review = BenchmarkAdoptionReview(
        **{
            key: getattr(decision, key)
            for key in (
                "configuration_hash",
                "report_id",
                "report_hash",
                "baseline_report_id",
                "baseline_report_hash",
            )
        },
        engineer_confirmed=True,
        rationale="These exact saved extra requests are justified.",
    )
    assert service.review(review).state == "accepted"


def test_checked_runtime_version_changes_configuration_without_account_revision_change(
    tmp_path, monkeypatch
):
    from test_catalog_authority import configured_office

    from quantix.ai_setup_store import SetupStore

    repo, tender, connections, account, model, policy, route = configured_office(
        tmp_path, monkeypatch
    )
    old = configuration_basis(repo, route, 1)
    monkeypatch.setattr(
        "quantix.ai_direct.direct_runtime_status",
        lambda _: {
            "state": "ready",
            "version": "synthetic-new-client",
            "component_id": "direct-api",
            "detail": "Ready",
            "progress": 100,
        },
    )
    SetupStore(repo).save_model_check(
        account["id"],
        model["model_id"],
        {
            "check": {"status": "passed", "model_id": model["model_id"]},
            "checked_revision": account["revision"],
            "checked_component_version": "synthetic-new-client",
        },
    )
    new = configuration_basis(repo, route, 1)
    assert old["connection_revision"] == new["connection_revision"]
    assert old["model_basis"] == new["model_basis"]
    assert old["checked_component_version"] != new["checked_component_version"]
    assert _hash(old) != _hash(new)
