"""When Quantix finished reading each document, so a newer copy that leaves work on the older one wakes the Manager

Revision ID: 0015
Revises: 0014
"""

import sqlalchemy as sa
from alembic import op

revision = "0015"
down_revision = "0014"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.add_column("documents", sa.Column("read_at", sa.DateTime(), nullable=True))


def downgrade() -> None:
    op.drop_column("documents", "read_at")
