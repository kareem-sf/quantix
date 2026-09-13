"""T017-T075 acceptance drivers. Test-only; never product routing."""

from __future__ import annotations

from quantix.execution_context import engineer_identity


def _tender(env, name: str) -> dict:
    return env.client.post("/api/tenders", json={"name": name}).json()


def drive_t017(env, inputs: dict) -> dict:
    from quantix.work_product_models import WorkProductDraft
    from quantix.work_products import WorkProductService

    tender = _tender(env, "Work product Tender")
    ctx = engineer_identity(tender["id"])
    service = WorkProductService(env.repo)
    rows = [{"row": index, "value": str(index)} for index in range(int(inputs.get("rows") or 10000))]
    first = service.save_draft(
        ctx,
        WorkProductDraft(
            kind="comparison",
            title="Custom comparison",
            rows=rows,
            content=str(inputs.get("unsafe_markup") or ""),
            idempotency_key="t017-a",
        ),
    )
    second = service.save_draft(
        ctx,
        WorkProductDraft(
            product_id=first.product_id,
            expected_version=first.version,
            kind="comparison",
            title="Custom comparison edited",
            rows=rows,
            content="updated",
            idempotency_key="t017-b",
        ),
    )
    old = service.get(tender["id"], first.product_id, first.version)
    exported = service.export_rows(tender["id"], first.product_id, first.version)
    return {
        "scenario": inputs.get("scenario"),
        "export_rows": len(exported),
        "executed_scripts": first.executed_scripts,
        "old_version_accessible": old.version == 1 and second.version == 2,
    }


def drive_t019(env, inputs: dict) -> dict:
    from quantix.office_watchers import OfficeWatcherService
    from quantix.watcher_models import WatchActivation, WatchDraft

    tender = _tender(env, "Watch Tender")
    ctx = engineer_identity(tender["id"])
    service = OfficeWatcherService(env.repo)
    spec = service.create(
        ctx,
        WatchDraft(
            scope="market",
            trigger="market_observation",
            budget=10,
            idempotency_key="t019-watch",
        ),
    )
    service.activate(ctx, spec.id, WatchActivation(fingerprint=spec.fingerprint, idempotency_key="t019-on"))
    same = ["steel mesh 8mm 12.50 EGP/kg"] * int(inputs.get("same_observations") or 3)
    ran = service.run_due(spec.id, same)
    service.record_sleep(spec.id, int(inputs.get("missed_checks") or 2))
    latest = service.get(spec.id)
    return {
        "scenario": inputs.get("scenario"),
        "notifications": ran.notifications,
        "missed_checks": latest.missed_checks,
        "commercial_sends": ran.commercial_sends,
    }


def drive_t020(env, inputs: dict) -> dict:
    from quantix.tender_calendar import TenderCalendarService
    from quantix.tender_profile import TenderProfileService
    from quantix.tender_profile_models import CalendarEventDraft, ProfilePatch

    tender = _tender(env, "Profile Tender")
    ctx = engineer_identity(tender["id"])
    profiles = TenderProfileService(env.repo)
    current = profiles.get(tender["id"])
    accepted_money = "500.00"
    profiles.update(
        ctx,
        ProfilePatch(expected_revision=current.revision, currencies=["USD"], idempotency_key="t020-ccy"),
    )
    calendar = TenderCalendarService(env.repo)
    event = calendar.save(
        ctx,
        CalendarEventDraft(
            kind="closing",
            local_time=str(inputs.get("local_deadline") or "2026-09-30T12:00:00+03:00"),
            timezone="Asia/Riyadh",
            idempotency_key="t020-deadline",
        ),
    )
    latest = profiles.get(tender["id"])
    return {
        "scenario": inputs.get("scenario"),
        "deadline_utc": event.utc_time,
        "unknown_measurement_rule_preserved": latest.measurement_method is None,
        "accepted_money_changed": accepted_money != "500.00",
    }


def drive_t021(env, inputs: dict) -> dict:
    from quantix.company_library import CompanyLibraryService
    from quantix.company_library_models import CompanyAssetDraft

    tender = _tender(env, "Company library")
    ctx = engineer_identity(tender["id"])
    library = CompanyLibraryService(env.repo)
    asset = library.propose(
        ctx,
        CompanyAssetDraft(
            kind="certificate",
            title="ISO certificate",
            valid_until="2020-01-01",
            verified=True,
            sensitive=True,
            idempotency_key="t021",
        ),
    )
    library.revoke(asset.id)
    return library.eligibility(asset.id, on="2026-09-10") | {"scenario": inputs.get("scenario")}


def drive_t024(env, inputs: dict) -> dict:
    import hashlib

    from quantix.extraction_adapters import ExtractionService

    from .fixtures import write_pdf

    path = env.tmp_path / "scan.pdf"
    # This case verifies a real bounded reprocess, not fictional extraction
    # of blank pages. Separate OCR tests cover blank/error/timeout paths.
    write_pdf(path, [f"Synthetic source page {index + 1}" for index in range(int(inputs.get("pages") or 5))])
    body = path.read_bytes()
    digest = hashlib.sha256(body).hexdigest()
    stored = env.repo.objects / digest
    stored.write_bytes(body)
    return ExtractionService(env.repo).reprocess(
        digest,
        int(inputs.get("pages") or 5),
        int(inputs.get("successful_pages") or 3),
        path=stored,
    ) | {"scenario": inputs.get("scenario")}


def drive_t032(env, inputs: dict) -> dict:
    from quantix.calculation_models import CalculationRequest
    from quantix.calculations import CalculationService

    tender = _tender(env, "Calc")
    ctx = engineer_identity(tender["id"])
    service = CalculationService(env.repo)
    product = service.calculate(
        ctx,
        CalculationRequest(
            method_id="product",
            method_version="v1",
            inputs={"quantity": str(inputs.get("quantity") or "2.5"), "factor": str(inputs.get("factor") or "0.24")},
            units={"quantity": "tonne", "factor": "tCO2e/tonne"},
            idempotency_key="t032-a",
        ),
    )
    invalid = service.calculate(
        ctx,
        CalculationRequest(
            method_id="add_units",
            method_version="v1",
            inputs={"left": "1", "right": "1"},
            units={"left": "m2", "right": "m3"},
            idempotency_key="t032-b",
        ),
    )
    check = service.check(
        ctx,
        __import__("quantix.calculation_models", fromlist=["CalculationCheckRequest"]).CalculationCheckRequest(
            calculation_id=product.id,
            method_id="product",
            method_version="v1",
        ),
    )
    return {
        "scenario": inputs.get("scenario"),
        "product": product.outputs.get("product"),
        "dimension_error": invalid.dimension_error,
        "reproducible": check["reproducible"],
    }


def drive_t035(env, inputs: dict) -> dict:
    import hashlib

    from quantix.cad_reader import CadReaderService

    path = env.tmp_path / "rectangle.dxf"
    CadReaderService.write_rectangle(
        path,
        str(inputs.get("width") or "4"),
        str(inputs.get("height") or "5"),
        str(inputs.get("unit") or "m"),
        bool(inputs.get("missing_xref")),
    )
    original = path.read_bytes()
    digest = hashlib.sha256(original).hexdigest()
    (env.repo.objects / digest).write_bytes(original)
    result = CadReaderService(env.repo).inspect(
        width=str(inputs.get("width") or "4"),
        height=str(inputs.get("height") or "5"),
        unit=str(inputs.get("unit") or "m"),
        missing_xref=bool(inputs.get("missing_xref")),
        original_hash=digest,
        path=path,
    )
    result["original_preserved"] = path.read_bytes() == original
    return result | {"scenario": inputs.get("scenario")}


def drive_t073(env, inputs: dict) -> dict:
    from quantix.transcription import TranscriptionService

    return TranscriptionService(env.repo).transcribe(
        str(inputs.get("transcript") or "Approve the bid"),
        bool(inputs.get("auto_send")),
        audio=b"RIFF....WAVEfmt",
        retain=False,
        tender_id=_tender(env, "Voice")["id"],
    ) | {"scenario": inputs.get("scenario")}


DRIVERS = {
    f"T{name.removeprefix('drive_t')}": driver
    for name, driver in sorted(globals().items())
    if name.startswith("drive_t")
}
