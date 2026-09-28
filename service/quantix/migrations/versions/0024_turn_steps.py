"""What each turn did, step by step: the person's thinking and notes, their tool calls and what Quantix sent back

Revision ID: 0024
Revises: 0023
"""

import sqlalchemy as sa
from alembic import op

revision = "0024"
down_revision = "0023"
branch_labels = None
depends_on = None


def upgrade() -> None:
    # added in place: turns belongs to tenders and staff, and rebuilding it is never needed for a new column
    op.add_column("turns", sa.Column("steps", sa.JSON(), nullable=True))


def downgrade() -> None:
    op.drop_column("turns", "steps")
