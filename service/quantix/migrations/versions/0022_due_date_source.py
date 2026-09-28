"""Where a tender's due date comes from: the engineer's own date, or a page of the tender documents

Revision ID: 0022
Revises: 0021
"""

import sqlalchemy as sa
from alembic import op

revision = "0022"
down_revision = "0021"
branch_labels = None
depends_on = None


def upgrade() -> None:
    # added in place: rebuilding tenders would cascade deletes to every tender's records (see migration 0014)
    op.add_column("tenders", sa.Column("due_date_basis", sa.String(20), nullable=True))
    op.add_column("tenders", sa.Column("due_date_by", sa.String(32), nullable=True))
    op.add_column("tenders", sa.Column("due_date_at", sa.DateTime(), nullable=True))
    op.add_column("tenders", sa.Column("due_date_document_id", sa.String(32), nullable=True))
    op.add_column("tenders", sa.Column("due_date_page", sa.Integer(), nullable=True))
    op.add_column("tenders", sa.Column("due_date_quote", sa.Text(), nullable=True))
    # until now only the engineer could give a due date, when creating the tender or on the Overview; when is unknown
    op.execute("UPDATE tenders SET due_date_basis = 'engineer', due_date_by = 'engineer' WHERE due_date IS NOT NULL")


def downgrade() -> None:
    for column in ("due_date_quote", "due_date_page", "due_date_document_id", "due_date_at", "due_date_by"):
        op.drop_column("tenders", column)
    op.drop_column("tenders", "due_date_basis")
