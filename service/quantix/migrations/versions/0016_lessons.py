"""What the Tender Manager learned from work that needed correcting

Revision ID: 0016
Revises: 0015
"""

import sqlalchemy as sa
from alembic import op

revision = "0016"
down_revision = "0015"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "lessons",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("text", sa.Text(), nullable=False),
        sa.Column("topic", sa.String(100), nullable=False),
        sa.Column("source", sa.String(300), nullable=False),
        sa.Column("learned_by", sa.String(32), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("lessons_tender", "lessons", ["tender_id", "status"])


def downgrade() -> None:
    op.drop_index("lessons_tender", "lessons")
    op.drop_table("lessons")
