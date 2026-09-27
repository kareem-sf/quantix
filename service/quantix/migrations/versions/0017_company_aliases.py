"""Other names a firm in the directory goes by

Revision ID: 0017
Revises: 0016
"""

import sqlalchemy as sa
from alembic import op

revision = "0017"
down_revision = "0016"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.add_column("companies", sa.Column("aliases", sa.JSON(), nullable=False, server_default="[]"))


def downgrade() -> None:
    op.drop_column("companies", "aliases")
