"""Sheet/range filters retain original evidence and paginate after selection."""

import pytest
from fastapi.testclient import TestClient

from quantix.api import create_app


def test_http_sheet_and_range_filter_before_pagination_and_preserve_artifact_scope(tmp_path):
    app = create_app(tmp_path, "synthetic-session")
    repo = app.state.repo
    tender = repo.create_tender("Synthetic workbook")
    segments = [{"locator": f"{sheet}!A{row}:F{row}", "text": f"Row {row}", "sheet": sheet,
                 "cell_range": f"A{row}:F{row}", "kind": "spreadsheet_row", "metadata": {"row": row}}
                for sheet in ("BOQ", "Rates") for row in range(1, 141)]
    artifact, _ = repo.register_artifact(tender["id"], "workbook.xlsx", "a"*64, 100,
        {"kind": "spreadsheet", "status": "extracted", "segments": segments})
    original = repo.artifact_evidence(tender["id"], artifact["id"], 0, 200)
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer synthetic-session"
        path = f"/api/tenders/{tender['id']}/artifacts/{artifact['id']}/evidence"
        result = client.get(path, params={"sheet": "Rates", "cell_range": "B120:D125", "offset": 1, "limit": 2})
        assert result.status_code == 200
        assert [row["locator"] for row in result.json()] == ["Rates!A121:F121", "Rates!A122:F122"]
        assert all(row["sheet"] == "Rates" for row in result.json())
        # Selecting a region retains each full original source row, including
        # columns outside the selected region needed to interpret its context.
        assert result.json()[0]["cell_range"] == "A121:F121"
        assert client.get(path, params={"sheet": "rates"}).json() == []
        assert client.get(path, params={"sheet": "BOQ", "cell_range": "G120:H125"}).json() == []
        assert repo.artifact_evidence(tender["id"], artifact["id"], 0, 200) == original
        other = repo.create_tender("Other Tender")
        assert client.get(path.replace(tender["id"], other["id"]), params={"sheet": "Rates"}).status_code == 404


@pytest.mark.parametrize("region", ["not-a-range", "A0:D2", "XFE1:XFE2", "A1048577:B1048578", "D20:A1"])
def test_invalid_sheet_range_is_actionable_before_reading_sources(tmp_path, region):
    app = create_app(tmp_path, "synthetic-session")
    repo = app.state.repo
    tender = repo.create_tender("Synthetic workbook")
    artifact, _ = repo.register_artifact(tender["id"], "workbook.xlsx", "a"*64, 100,
        {"kind": "spreadsheet", "status": "extracted", "segments": []})
    with TestClient(app) as client:
        client.headers["Authorization"] = "Bearer synthetic-session"
        response = client.get(f"/api/tenders/{tender['id']}/artifacts/{artifact['id']}/evidence", params={"sheet": "BOQ", "cell_range": region})
        assert response.status_code == 409
        assert "cell range" in response.json()["detail"].lower()
