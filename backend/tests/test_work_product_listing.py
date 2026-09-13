"""Work-product reads stay bounded and preserve immutable version history."""

from quantix.execution_context import engineer_identity
from quantix.repository import Repository
from quantix.work_product_models import WorkProductDraft
from quantix.work_products import WorkProductService


def test_lists_latest_products_and_pages_exact_version_history(tmp_path):
    repo = Repository(tmp_path)
    tender = repo.create_tender("Synthetic work-product library")
    identity = engineer_identity(tender["id"])
    service = WorkProductService(repo)
    first = service.save_draft(
        identity,
        WorkProductDraft(
            kind="table",
            title="Metric table",
            rows=[{"metric": f"M{index}", "value": index} for index in range(75)],
            content="First version",
            idempotency_key="metrics-v1",
        ),
    )
    revised = service.save_draft(
        identity,
        WorkProductDraft(
            product_id=first.product_id,
            expected_version=1,
            kind="chart",
            title="Metric chart",
            view_schema={"category_field": "metric", "value_field": "value"},
            rows=[{"metric": "M1", "value": 12.5}],
            content="Current version",
            idempotency_key="metrics-v2",
        ),
    )
    note = service.save_draft(
        identity,
        WorkProductDraft(
            kind="note",
            title="Review note",
            content="<script>bad()</script>Separate product",
            idempotency_key="review-note",
        ),
    )
    assert note.executed_scripts == 0
    assert note.sanitized_content == "Separate product"

    products = service.list(tender["id"], offset=0, limit=10)
    metric = next(item for item in products.items if item.product_id == first.product_id)
    assert products.total == 2
    assert metric.version == revised.version == 2
    assert metric.kind == "chart"
    assert metric.row_count == 1
    assert metric.dependency_state == "current"

    versions = service.versions(tender["id"], first.product_id, offset=0, limit=1)
    assert versions.total == 2
    assert versions.next_offset == 1
    assert [(item.version, item.title) for item in versions.items] == [(2, "Metric chart")]

    rows = service.page(tender["id"], first.product_id, 1, offset=10, limit=20)
    assert rows.total == 75
    assert len(rows.items) == 20
    assert rows.items[0] == {"metric": "M10", "value": 10}
