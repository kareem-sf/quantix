from quantix.repository import Repository
from quantix.tender_summaries import TenderSummaryService, short_name


def test_short_name_prefers_the_ai_name_then_the_english_part():
    arabic = "إنشاء وبناء محطة خدمات مكافحة الحريق (Fire-Fighting Services Station) - New Fire Station, KSAU-HS"
    assert short_name(arabic, None) == "New Fire Station, KSAU-HS"
    assert (
        short_name(arabic, {"short_name": "New Fire Station · KSAU-HS"})
        == "New Fire Station · KSAU-HS"
    )
    assert short_name("مشروع (Warehouse Riyadh)", None) == "Warehouse Riyadh"
    assert short_name("Office tower", None) == "Office tower"


def test_summaries_report_live_work_and_waiting_items(tmp_path):
    repo = Repository(tmp_path / "quantix")
    tender = repo.create_tender("Warehouse Riyadh")
    repo.add_finding(tender["id"], "Visit date differs", "x", "risk", [])
    repo.create_run(tender["id"], "manager")
    [summary] = TenderSummaryService(repo).list().tenders
    assert summary.short_name == "Warehouse Riyadh"
    assert summary.waiting == 1
    assert summary.working is True
