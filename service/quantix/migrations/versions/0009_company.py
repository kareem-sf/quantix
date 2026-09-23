"""Company knowledge: house rules, and each tender's outcome for past-tender benchmarks

Revision ID: 0009
Revises: 0008
"""

import sqlalchemy as sa
from alembic import op

revision = "0009"
down_revision = "0008"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "company_rules",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("owner_id", sa.String(64), nullable=False),
        sa.Column("topic", sa.String(100), nullable=False),
        sa.Column("text", sa.Text(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    with op.batch_alter_table("tenders") as table:
        table.add_column(sa.Column("outcome", sa.String(20), nullable=False, server_default="open"))


def downgrade() -> None:
    with op.batch_alter_table("tenders") as table:
        table.drop_column("outcome")
    op.drop_table("company_rules")
