import json
from types import SimpleNamespace

from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix.sandbox_routes import create_router


def test_python_receipt_is_scoped_and_download_is_never_active_content(tmp_path):
    from quantix.python_analysis_models import PythonAnalysisReceipt, PythonLimits, PythonOutput
    from quantix.sandbox_protocol import sha256

    identifier = "a" * 32
    output = b"<script>alert(1)</script>"
    folder = tmp_path / "code" / "runs" / identifier
    (folder / "outputs").mkdir(parents=True)
    (folder / "outputs" / "result.html").write_bytes(output)
    record = PythonAnalysisReceipt(
        id=identifier,
        status="completed",
        code_sha256="x",
        image_id="sha256:" + "b" * 64,
        library_versions={},
        input_hashes={},
        limits=PythonLimits(),
        outputs=[PythonOutput(name="result.html", size_bytes=len(output), sha256=sha256(output))],
        created_at="2026-09-12T00:00:00Z",
    ).model_dump()
    (folder / "receipt.json").write_text(json.dumps({**record, "tender_id": "selected"}))
    app = FastAPI()
    app.include_router(create_router(SimpleNamespace(home=tmp_path)))
    client = TestClient(app)
    assert client.get(f"/api/tenders/other/python-runs/{identifier}").status_code == 404
    assert client.get(f"/api/tenders/selected/python-runs/{identifier}").status_code == 200
    response = client.get(
        f"/api/tenders/selected/python-runs/{identifier}/outputs/result.html?download=false"
    )
    assert response.headers["content-type"] == "application/octet-stream"
    assert response.headers["content-disposition"].startswith("attachment")
    assert response.content == output
    (folder / "outputs" / "result.html").write_bytes(b"changed")
    assert (
        client.get(
            f"/api/tenders/selected/python-runs/{identifier}/outputs/result.html"
        ).status_code
        == 409
    )
