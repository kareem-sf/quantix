"""The tender register: when a tender was archived, and when its outcome (submitted, won, lost) was last set

Revision ID: 0025
Revises: 0024
"""

import sqlalchemy as sa
from alembic import op

revision = "0025"
down_revision = "0024"
branch_labels = None
depends_on = None


def upgrade() -> None:
    # added in place: rebuilding tenders would cascade deletes to every tender's records (see migration 0014)
    op.add_column("tenders", sa.Column("archived_at", sa.DateTime(), nullable=True))
    op.add_column("tenders", sa.Column("outcome_at", sa.DateTime(), nullable=True))


def downgrade() -> None:
    op.drop_column("tenders", "outcome_at")
    op.drop_column("tenders", "archived_at")
