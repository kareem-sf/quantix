"""Escalations: a decision about a record the office couldn't settle, with where the problem shows

Revision ID: 0013
Revises: 0012
"""

import sqlalchemy as sa
from alembic import op

revision = "0013"
down_revision = "0012"
branch_labels = None
depends_on = None


def upgrade() -> None:
    with op.batch_alter_table("decisions") as table:
        table.add_column(sa.Column("subject_kind", sa.String(20), nullable=True))
        table.add_column(sa.Column("subject_id", sa.String(32), nullable=True))
        table.add_column(sa.Column("sources", sa.JSON(), nullable=True))


def downgrade() -> None:
    with op.batch_alter_table("decisions") as table:
        table.drop_column("sources")
        table.drop_column("subject_id")
        table.drop_column("subject_kind")
