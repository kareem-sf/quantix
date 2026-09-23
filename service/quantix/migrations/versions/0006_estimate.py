"""Estimating: the company rate library, rates per BOQ item, and markups

Revision ID: 0006
Revises: 0005
"""

import sqlalchemy as sa
from alembic import op

revision = "0006"
down_revision = "0005"
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
    op.create_table(
        "library",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("owner_id", sa.String(64), nullable=False),
        sa.Column("kind", sa.String(20), nullable=False),
        sa.Column("name", sa.String(300), nullable=False),
        sa.Column("unit", sa.String(40), nullable=False),
        sa.Column("rate", sa.Numeric(18, 4), nullable=False),
        sa.Column("currency", sa.String(10), nullable=False),
        sa.Column("source", sa.Text(), nullable=False),
        sa.Column("dated", sa.Date(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_table(
        "rates",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("boq_item_id", sa.String(32), sa.ForeignKey("boq_items.id"), nullable=False),
        sa.Column("basis", sa.String(20), nullable=False),
        sa.Column("unit_rate", sa.Numeric(18, 4), nullable=True),
        sa.Column("lines", sa.JSON(), nullable=True),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id"), nullable=True),
        sa.Column("page", sa.Integer(), nullable=True),
        sa.Column("quote", sa.Text(), nullable=True),
        sa.Column("library_id", sa.String(32), sa.ForeignKey("library.id"), nullable=True),
        sa.Column("note", sa.Text(), nullable=False),
        *_review(),
    )
    op.create_index("rates_item", "rates", ["boq_item_id"])
    op.create_table(
        "markups",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("preliminaries", sa.Numeric(8, 4), nullable=False),
        sa.Column("overheads", sa.Numeric(8, 4), nullable=False),
        sa.Column("profit", sa.Numeric(8, 4), nullable=False),
        sa.Column("adjustment", sa.Numeric(18, 2), nullable=False),
        sa.Column("note", sa.Text(), nullable=False),
        *_review(),
    )


def downgrade() -> None:
    op.drop_table("markups")
    op.drop_table("rates")
    op.drop_table("library")
