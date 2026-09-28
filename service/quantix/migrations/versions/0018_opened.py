"""What each person in the office opened, and the sources an answer to the engineer rests on

Revision ID: 0018
Revises: 0017
"""

import sqlalchemy as sa
from alembic import op

revision = "0018"
down_revision = "0017"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "opened",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("staff_id", sa.String(32), sa.ForeignKey("staff.id", ondelete="CASCADE"), nullable=False),
        sa.Column("kind", sa.String(20), nullable=False),
        sa.Column("ref", sa.String(80), nullable=False),
        sa.Column("at", sa.DateTime(), nullable=False),
    )
    op.create_index("opened_by", "opened", ["staff_id", "kind", "ref"])
    op.create_index("opened_tender", "opened", ["tender_id", "kind"])
    op.add_column("messages", sa.Column("sources", sa.JSON(), nullable=True))


def downgrade() -> None:
    op.drop_column("messages", "sources")
    op.drop_index("opened_tender", "opened")
    op.drop_index("opened_by", "opened")
    op.drop_table("opened")
