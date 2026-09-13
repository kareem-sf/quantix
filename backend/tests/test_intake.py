import hashlib
import importlib
import threading
import zipfile

import pytest
from openpyxl import Workbook

from quantix.repository import Repository


def importer():
    try:
        return importlib.import_module("quantix.intake").import_package
    except ModuleNotFoundError:
        pytest.fail("Package intake has not been implemented")


def workbook(path):
    book = Workbook()
    sheet = book.active
    sheet.title = "Civil"
    sheet.append(["Item", "Description", "Unit", "Qty", "Rate", "Amount"])
    sheet.append(["1", "Reinforced concrete", "m3", 25, None, "=D2*E2"])
    book.save(path)
    book.close()


def test_import_keeps_original_bytes_and_reuses_identical_sources(tmp_path):
    root = tmp_path / "package"
    root.mkdir()
    workbook(root / "Civil.xlsx")
    before = hashlib.sha256((root / "Civil.xlsx").read_bytes()).hexdigest()
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Tender")
    for _ in range(2):
        run = repo.create_run(tender["id"], "import", str(root))
        result = importer()(repo, tender["id"], root, run["id"], threading.Event())
        assert result["registered"] == 1
    assert hashlib.sha256((root / "Civil.xlsx").read_bytes()).hexdigest() == before
    artifacts = repo.list_artifacts(tender["id"], current_only=False)
    assert len(artifacts) == 1
    assert (
        repo.object_path(tender["id"], artifacts[0]["id"]).read_bytes()
        == (root / "Civil.xlsx").read_bytes()
    )
    hit = repo.search(tender["id"], "reinforced")[0]
    assert hit["sheet"] == "Civil"
    assert "25" in hit["text"]


def test_zip_traversal_is_rejected_without_publishing(tmp_path):
    archive = tmp_path / "bad.zip"
    with zipfile.ZipFile(archive, "w") as z:
        z.writestr("../escape.txt", "unsafe")
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Tender")
    run = repo.create_run(tender["id"], "import", str(archive))
    with pytest.raises(ValueError, match="unsafe"):
        importer()(repo, tender["id"], archive, run["id"], threading.Event())
    assert repo.list_artifacts(tender["id"]) == []
    assert not (tmp_path / "escape.txt").exists()


def test_cancelled_import_does_not_publish_a_completed_message(tmp_path):
    root = tmp_path / "package"
    root.mkdir()
    workbook(root / "Civil.xlsx")
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Tender")
    run = repo.create_run(tender["id"], "import", str(root))
    cancelled = threading.Event()
    cancelled.set()
    with pytest.raises(InterruptedError):
        importer()(repo, tender["id"], root, run["id"], cancelled)
    assert repo.messages(tender["id"]) == []


def test_application_home_cannot_be_imported_recursively(tmp_path):
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Tender")
    run = repo.create_run(tender["id"], "import", str(tmp_path))
    with pytest.raises(ValueError, match="workspace"):
        importer()(repo, tender["id"], tmp_path, run["id"], threading.Event())
