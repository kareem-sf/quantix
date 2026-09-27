"""Preliminaries priced item by item instead of a percentage the office chose

Revision ID: 0010
Revises: 0009
"""

import sqlalchemy as sa
from alembic import op

revision = "0010"
down_revision = "0009"
branch_labels = None
depends_on = None


def upgrade() -> None:
    with op.batch_alter_table("markups") as table:
        table.add_column(sa.Column("preliminary_items", sa.JSON(), nullable=False, server_default="[]"))
        table.drop_column("preliminaries")


def downgrade() -> None:
    with op.batch_alter_table("markups") as table:
        table.add_column(sa.Column("preliminaries", sa.Numeric(8, 4), nullable=False, server_default="0"))
        table.drop_column("preliminary_items")
