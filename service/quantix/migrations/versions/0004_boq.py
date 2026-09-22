"""BOQ items and tender facts, each with its source and approval

Revision ID: 0004
Revises: 0003
"""

import sqlalchemy as sa
from alembic import op

revision = "0004"
down_revision = "0003"
branch_labels = None
depends_on = None


def _common() -> list[sa.Column]:
    return [
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id"), nullable=False),
        sa.Column("page", sa.Integer(), nullable=False),
        sa.Column("quote", sa.Text(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("proposed_by", sa.String(32), nullable=False),
        sa.Column("reason", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("decided_at", sa.DateTime(), nullable=True),
    ]


def upgrade() -> None:
    op.create_table(
        "boq_items",
        *_common(),
        sa.Column("section", sa.String(300), nullable=True),
        sa.Column("item", sa.String(40), nullable=False),
        sa.Column("description", sa.Text(), nullable=False),
        sa.Column("unit", sa.String(40), nullable=False),
        sa.Column("quantity", sa.Numeric(18, 4), nullable=True),
        sa.Column("position", sa.Integer(), nullable=False),
    )
    op.create_index("boq_items_tender", "boq_items", ["tender_id", "position"])
    op.create_table(
        "facts",
        *_common(),
        sa.Column("kind", sa.String(40), nullable=False),
        sa.Column("value", sa.Text(), nullable=False),
    )


def downgrade() -> None:
    op.drop_table("facts")
    op.drop_table("boq_items")
