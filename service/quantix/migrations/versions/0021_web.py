"""Web research: saved web pages, rates priced from them, and a firm's website

Revision ID: 0021
Revises: 0020
"""

import sqlalchemy as sa
from alembic import op

revision = "0021"
down_revision = "0020"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "web_pages",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("owner_id", sa.String(64), nullable=False),
        sa.Column("url", sa.String(2000), nullable=False),
        sa.Column("title", sa.String(300), nullable=False),
        sa.Column("text", sa.Text(), nullable=False),
        sa.Column("read_at", sa.DateTime(), nullable=False),
    )
    op.create_index("web_pages_url", "web_pages", ["url"])
    # no constraint: SQLite can only add one by rebuilding the table, which cascades deletes from the tables that
    # refer to rates (see migration 0014)
    op.add_column("rates", sa.Column("web_page_id", sa.String(32), nullable=True))
    op.add_column("companies", sa.Column("website", sa.String(500), nullable=True))


def downgrade() -> None:
    op.drop_column("companies", "website")
    op.drop_column("rates", "web_page_id")
    op.drop_index("web_pages_url", "web_pages")
    op.drop_table("web_pages")
