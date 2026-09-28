"""CAD drawings: measurements of drawing objects, the layer map and tender queries

Revision ID: 0023
Revises: 0022
"""

import sqlalchemy as sa
from alembic import op

revision = "0023"
down_revision = "0022"
branch_labels = None
depends_on = None


def _review() -> list[sa.Column]:
    return [
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("proposed_by", sa.String(32), nullable=False),
        sa.Column("reason", sa.Text(), nullable=True),
        sa.Column("reviewed_by", sa.String(32), nullable=True),
        sa.Column("review_note", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("reviewed_at", sa.DateTime(), nullable=True),
        sa.Column("decided_at", sa.DateTime(), nullable=True),
    ]


def upgrade() -> None:
    # Not batch_alter_table: rebuilding measurements would drop it, and other tables reference what it references
    op.add_column("measurements", sa.Column("entities", sa.JSON(), nullable=True))
    op.add_column("measurements", sa.Column("rule", sa.JSON(), nullable=True))
    op.create_table(
        "layer_maps",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id"), nullable=False),
        sa.Column("layers", sa.JSON(), nullable=False),
        sa.Column("blocks", sa.JSON(), nullable=False),
        sa.Column("note", sa.Text(), nullable=False),
        *_review(),
    )
    op.create_index("layer_maps_tender", "layer_maps", ["tender_id", "status"])
    op.create_table(
        "tender_queries",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("kind", sa.String(20), nullable=False),
        sa.Column("title", sa.String(300), nullable=False),
        sa.Column("detail", sa.Text(), nullable=False),
        sa.Column("wording", sa.Text(), nullable=False),
        sa.Column("governs", sa.Text(), nullable=True),
        sa.Column("sources", sa.JSON(), nullable=False),
        sa.Column("boq_item_id", sa.String(32), sa.ForeignKey("boq_items.id"), nullable=True),
        sa.Column("measurement_ids", sa.JSON(), nullable=False),
        *_review(),
    )
    op.create_index("tender_queries_tender", "tender_queries", ["tender_id", "status"])


def downgrade() -> None:
    op.drop_index("tender_queries_tender", "tender_queries")
    op.drop_table("tender_queries")
    op.drop_index("layer_maps_tender", "layer_maps")
    op.drop_table("layer_maps")
    op.drop_column("measurements", "rule")
    op.drop_column("measurements", "entities")
