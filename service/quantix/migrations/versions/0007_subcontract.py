"""Subcontract and supplier quotes: the directory, packages, enquiries and quotes

Revision ID: 0007
Revises: 0006
"""

import sqlalchemy as sa
from alembic import op

revision = "0007"
down_revision = "0006"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "companies",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("owner_id", sa.String(64), nullable=False),
        sa.Column("name", sa.String(200), nullable=False),
        sa.Column("kind", sa.String(20), nullable=False),
        sa.Column("trades", sa.Text(), nullable=False),
        sa.Column("email", sa.String(200), nullable=True),
        sa.Column("phone", sa.String(60), nullable=True),
        sa.Column("notes", sa.Text(), nullable=True),
        sa.Column("added_by", sa.String(32), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_table(
        "packages",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("name", sa.String(200), nullable=False),
        sa.Column("kind", sa.String(20), nullable=False),
        sa.Column("items", sa.JSON(), nullable=False),
        sa.Column("recommended_quote_id", sa.String(32), nullable=True),
        sa.Column("recommendation", sa.Text(), nullable=True),
        sa.Column("recommended_by", sa.String(32), nullable=True),
        sa.Column("selected_quote_id", sa.String(32), nullable=True),
        sa.Column("created_by", sa.String(32), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("decided_at", sa.DateTime(), nullable=True),
    )
    op.create_table(
        "enquiries",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("package_id", sa.String(32), sa.ForeignKey("packages.id", ondelete="CASCADE"), nullable=False),
        sa.Column("company_id", sa.String(32), sa.ForeignKey("companies.id"), nullable=False),
        sa.Column("subject", sa.String(300), nullable=False),
        sa.Column("body", sa.Text(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("created_by", sa.String(32), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("sent_at", sa.DateTime(), nullable=True),
    )
    op.create_table(
        "quotes",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("package_id", sa.String(32), sa.ForeignKey("packages.id", ondelete="CASCADE"), nullable=False),
        sa.Column("company_id", sa.String(32), sa.ForeignKey("companies.id"), nullable=False),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id"), nullable=False),
        sa.Column("lines", sa.JSON(), nullable=False),
        sa.Column("exclusions", sa.JSON(), nullable=False),
        sa.Column("proposed_by", sa.String(32), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )


def downgrade() -> None:
    for table in ("quotes", "enquiries", "packages", "companies"):
        op.drop_table(table)
