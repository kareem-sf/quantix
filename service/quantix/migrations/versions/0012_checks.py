"""Quantix's checks: the Manager's reasons for accepting warnings, and each work schedule's lines and duration

Revision ID: 0012
Revises: 0011
"""

import sqlalchemy as sa
from alembic import op

revision = "0012"
down_revision = "0011"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "acceptances",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("key", sa.String(300), nullable=False),
        sa.Column("reason", sa.Text(), nullable=False),
        sa.Column("accepted_by", sa.String(32), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("acceptances_key", "acceptances", ["tender_id", "key"])
    with op.batch_alter_table("drafts") as table:
        table.add_column(sa.Column("schedule", sa.JSON(), nullable=True))


def downgrade() -> None:
    with op.batch_alter_table("drafts") as table:
        table.drop_column("schedule")
    op.drop_index("acceptances_key", "acceptances")
    op.drop_table("acceptances")
