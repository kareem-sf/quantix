import asyncio
import hashlib
import importlib
import io

import pypdfium2
import pytest
from PIL import Image

from quantix.office_tools import OfficeContext
from quantix.repository import Repository


def module():
    try:
        return importlib.import_module("quantix.visual_sources")
    except ModuleNotFoundError:
        pytest.fail("Visual source access has not been implemented")


@pytest.fixture
def drawing(tmp_path):
    path = tmp_path / "drawing.pdf"
    document = pypdfium2.PdfDocument.new()
    page = document.new_page(600, 800)
    page.close()
    document.save(path)
    document.close()
    repo = Repository(tmp_path / "home")
    tender = repo.create_tender("Drawing test")
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    (repo.objects / digest).write_bytes(path.read_bytes())
    artifact, _ = repo.register_artifact(
        tender["id"],
        "Drawing.pdf",
        digest,
        path.stat().st_size,
        {"kind": "pdf", "status": "needs_attention", "segments": [], "metadata": {"page_count": 1}},
    )
    run = repo.create_run(tender["id"], "manager")
    return repo, tender, artifact, run


def test_pdf_crop_is_bounded_and_leaves_the_source_unchanged(drawing):
    repo, tender, artifact, _ = drawing
    path = repo.object_path(tender["id"], artifact["id"])
    before = path.read_bytes()
    image = Image.open(io.BytesIO(module().render_region(path, 1, [0, 0, 0.5, 0.5])))
    assert 0 < image.width <= 1800
    assert 0 < image.height <= 4096
    assert path.read_bytes() == before
    with pytest.raises(ValueError):
        module().render_region(path, 2, [0, 0, 1, 1])
    with pytest.raises(ValueError):
        module().render_region(path, 1, [-1, 0, 1, 1])


def test_image_only_page_has_a_durable_scoped_source_reference(drawing):
    repo, tender, artifact, run = drawing
    context = OfficeContext(repo, tender["id"], run["id"])
    output = asyncio.run(module().visual_source(context, artifact["id"], 1, [0, 0, 1, 1]))
    assert any(
        item.get("type") == "image" and item.get("mime_type") == "image/png" and item.get("data")
        for item in output
    )
    assert len(context.seen_sources) == 1
    source_id = next(iter(context.seen_sources))
    source = repo.get_evidence(tender["id"], source_id)
    assert source["page"] == 1
    assert source["kind"] == "visual"
    other = repo.create_tender("Other")
    with pytest.raises(KeyError):
        repo.get_evidence(other["id"], source_id)


def test_visual_tool_refuses_another_tenders_file(drawing):
    repo, _, artifact, _ = drawing
    other = repo.create_tender("Other")
    run = repo.create_run(other["id"], "manager")
    context = OfficeContext(repo, other["id"], run["id"])
    # Another Tender's file is unknown here: refused, with nothing about it revealed.
    with pytest.raises((KeyError, ValueError), match="No document with this ID"):
        asyncio.run(module().visual_source(context, artifact["id"], 1, [0, 0, 1, 1]))


def test_large_page_images_are_reencoded_to_fit_a_tool_result():
    import os

    from PIL import Image

    from quantix.visual_sources import MAX_IMAGE_BYTES, fit_image

    noisy = Image.frombytes("RGB", (1800, 2400), os.urandom(1800 * 2400 * 3))
    buffer = io.BytesIO()
    noisy.save(buffer, format="PNG")
    assert len(buffer.getvalue()) > MAX_IMAGE_BYTES
    data, mime = fit_image(buffer.getvalue())
    assert mime == "image/jpeg" and len(data) <= MAX_IMAGE_BYTES * 1.2
