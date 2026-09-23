"""Submission: requirements, drafts and the client BOQ's pricing columns

Revision ID: 0008
Revises: 0007
"""

import sqlalchemy as sa
from alembic import op

revision = "0008"
down_revision = "0007"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "requirements",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("section", sa.String(100), nullable=False),
        sa.Column("title", sa.String(300), nullable=False),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id"), nullable=True),
        sa.Column("page", sa.Integer(), nullable=True),
        sa.Column("quote", sa.Text(), nullable=True),
        sa.Column("added_by", sa.String(32), nullable=False),
        sa.Column("ready_note", sa.Text(), nullable=True),
        sa.Column("file_name", sa.String(300), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_table(
        "drafts",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column(
            "requirement_id", sa.String(32), sa.ForeignKey("requirements.id", ondelete="CASCADE"), nullable=False
        ),
        sa.Column("title", sa.String(300), nullable=False),
        sa.Column("body", sa.Text(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("proposed_by", sa.String(32), nullable=False),
        sa.Column("reason", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("decided_at", sa.DateTime(), nullable=True),
    )
    op.create_table(
        "pricing_columns",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id"), nullable=False),
        sa.Column("sheet", sa.Integer(), nullable=False),
        sa.Column("rate_column", sa.String(3), nullable=False),
        sa.Column("amount_column", sa.String(3), nullable=False),
        sa.Column("quote", sa.Text(), nullable=False),
        sa.Column("proposed_by", sa.String(32), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )


def downgrade() -> None:
    for table in ("pricing_columns", "drafts", "requirements"):
        op.drop_table(table)
