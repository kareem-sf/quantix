"""Quantity takeoff: page sizes, sheet scales and measurements

Revision ID: 0005
Revises: 0004
"""

import sqlalchemy as sa
from alembic import op

revision = "0005"
down_revision = "0004"
branch_labels = None
depends_on = None


def _review() -> list[sa.Column]:
    return [
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("proposed_by", sa.String(32), nullable=False),
        sa.Column("reason", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("decided_at", sa.DateTime(), nullable=True),
    ]


def upgrade() -> None:
    with op.batch_alter_table("pages") as pages:
        pages.add_column(sa.Column("width", sa.Float(), nullable=True))
        pages.add_column(sa.Column("height", sa.Float(), nullable=True))
    op.create_table(
        "scales",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id"), nullable=False),
        sa.Column("page", sa.Integer(), nullable=False),
        sa.Column("metres_per_point", sa.Float(), nullable=False),
        sa.Column("line", sa.JSON(), nullable=False),
        sa.Column("length_m", sa.Float(), nullable=False),
        sa.Column("dimension", sa.String(100), nullable=False),
        *_review(),
    )
    op.create_table(
        "measurements",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id"), nullable=False),
        sa.Column("page", sa.Integer(), nullable=False),
        sa.Column("kind", sa.String(10), nullable=False),
        sa.Column("label", sa.String(300), nullable=False),
        sa.Column("points", sa.JSON(), nullable=False),
        sa.Column("multiplier", sa.Numeric(12, 4), nullable=True),
        sa.Column("unit", sa.String(10), nullable=False),
        sa.Column("boq_item_id", sa.String(32), sa.ForeignKey("boq_items.id"), nullable=True),
        *_review(),
    )


def downgrade() -> None:
    op.drop_table("measurements")
    op.drop_table("scales")
    with op.batch_alter_table("pages") as pages:
        pages.drop_column("height")
        pages.drop_column("width")
