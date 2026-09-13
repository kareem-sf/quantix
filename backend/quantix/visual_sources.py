"""Bounded visual evidence for the office, using preserved PDF originals."""

import asyncio
import base64
import io
import json
import math
from contextlib import closing

import pypdfium2

from .db import dump, new_id
from .documents import PDFIUM_LOCK


def render_region(path, page, region):
    if isinstance(page, bool) or not isinstance(page, int) or page < 1:
        raise ValueError("Choose a valid page number.")
    if len(region) != 4 or any(not math.isfinite(v) for v in region):
        raise ValueError("A drawing region needs four finite coordinates.")
    x, y, width, height = region
    if min(x, y) < 0 or min(width, height) <= 0 or x + width > 1 or y + height > 1:
        raise ValueError("The requested region must be inside the page.")
    with PDFIUM_LOCK, closing(pypdfium2.PdfDocument(path)) as document:
        if page > len(document):
            raise ValueError("This page is outside the document.")
        with closing(document[page - 1]) as original:
            full_width, full_height = original.get_size()
            cropped_width, cropped_height = full_width * width, full_height * height
            scale = min(
                1800 / cropped_width,
                4096 / cropped_height,
                math.sqrt(12000000 / (cropped_width * cropped_height)),
            )
            crop = (
                x * full_width,
                (1 - y - height) * full_height,
                (1 - x - width) * full_width,
                y * full_height,
            )
            with closing(original.render(scale=scale, crop=crop)) as bitmap:
                image = bitmap.to_pil().copy()
    output = io.BytesIO()
    with image:
        image.save(output, format="PNG")
    return output.getvalue()


async def visual_source(context, artifact_id, page, region):
    if context.is_staff and context._draft() is None:
        with context.read_scope():
            result = await _visual_source(context, artifact_id, page, region)
            json.dumps(result, ensure_ascii=False)
            return result
    result = await _visual_source(context, artifact_id, page, region)
    json.dumps(result, ensure_ascii=False)
    return result


async def _visual_source(context, artifact_id, page, region):
    repo, tender_id = context.repo, context.tender_id
    context.require_tool("view_document_page")
    context.ensure_scope_current()
    artifact = context.ensure_artifact_allowed(artifact_id)
    if artifact["kind"] != "pdf":
        raise ValueError(
            "Choose a PDF source for visual inspection. CAD originals need a supported PDF or CAD reader."
        )
    path = repo.object_path(tender_id, artifact_id)
    png = await asyncio.to_thread(render_region, path, page, region)
    # A source revision or binding revocation during rendering must not become
    # an attributable read for the old bytes.
    artifact = context.ensure_artifact_allowed(artifact_id)
    with repo.db.connect() as conn:
        existing = conn.execute(
            "SELECT id FROM evidence WHERE artifact_id=? AND page=? ORDER BY rowid LIMIT 1",
            (artifact_id, page),
        ).fetchone()
        if existing:
            source_id = existing[0]
        elif context.is_staff:
            source_id = new_id()
            context.stage_evidence(
                (
                    source_id,
                    artifact_id,
                    f"Page {page}",
                    "",
                    page,
                    None,
                    None,
                    "visual",
                    dump({"visual_reference": True, "text_extracted": False}),
                )
            )
        else:
            source_id = new_id()
            with repo.atomic() as write_conn:
                write_conn.execute(
                    "INSERT INTO evidence(id,artifact_id,locator,text,page,kind,metadata_json) VALUES(?,?,?,?,?,?,?)",
                    (
                        source_id,
                        artifact_id,
                        f"Page {page}",
                        "",
                        page,
                        "visual",
                        dump({"visual_reference": True, "text_extracted": False}),
                    ),
                )
    if context.is_staff and context._draft() is not None and not existing:
        evidence = {
            "id": source_id,
            "artifact_id": artifact_id,
            "artifact_name": artifact["name"],
            "locator": f"Page {page}",
            "text": "",
            "page": page,
            "kind": "visual",
            "metadata": {"visual_reference": True, "text_extracted": False},
        }
    else:
        evidence = repo.get_evidence(tender_id, source_id)
    reference = {
        "source_id": source_id,
        "artifact_name": artifact["name"],
        "page": page,
        "region": region,
        "coordinate_system": "x,y,width,height as fractions from the page's top-left",
        "instruction": "Inspect the image. Cite this source ID; distinguish printed dimensions from your interpretation. This is not a verified quantity takeoff.",
    }
    context.record_visual_receipt(evidence, artifact, page, region)
    context.add_seen_source(source_id)
    event_data = {
        "source_id": source_id,
        "artifact_id": artifact_id,
        "page": page,
        "region": region,
    }
    if context.actor_id is not None:
        event_data["actor_id"] = context.actor_id
    if context.is_staff:
        event_data.update(
            {
                "assignment_id": context.assignment_id,
                "profile_version": context.staff_version,
                "route_binding_id": context.route_binding_id,
            }
        )
    context.emit_event(
        "visual_source_viewed",
        "A source drawing region was inspected.",
        event_data,
    )
    return [
        {"type": "text", "text": json.dumps(reference)},
        {"type": "image", "mime_type": "image/png", "data": base64.b64encode(png).decode()},
    ]
