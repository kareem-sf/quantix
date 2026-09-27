"""Scanned pages read by OCR, with the language model used and the confidence

Revision ID: 0020
Revises: 0019
"""

import sqlalchemy as sa
from alembic import op

revision = "0020"
down_revision = "0019"
branch_labels = None
depends_on = None


def upgrade() -> None:
    # Not batch_alter_table: rebuilding pages would drop it, and with foreign keys on that deletes its passages
    op.add_column("pages", sa.Column("ocr", sa.String(10), nullable=True))
    op.add_column("pages", sa.Column("ocr_score", sa.Float(), nullable=True))


def downgrade() -> None:
    op.drop_column("pages", "ocr_score")
    op.drop_column("pages", "ocr")
