"""Documents and their pages, with full-text search

Revision ID: 0002
Revises: 0001
"""

import sqlalchemy as sa
from alembic import op

revision = "0002"
down_revision = "0001"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "documents",
        sa.Column("id", sa.String(32), primary_key=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("path", sa.String(1000), nullable=False),
        sa.Column("kind", sa.String(20), nullable=False),
        sa.Column("size", sa.Integer(), nullable=False),
        sa.Column("sha256", sa.String(64), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("note", sa.Text(), nullable=True),
        sa.Column("page_count", sa.Integer(), nullable=True),
        sa.Column("group_name", sa.String(200), nullable=True),
        sa.Column("description", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("documents_tender", "documents", ["tender_id"])
    op.create_table(
        "pages",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column("document_id", sa.String(32), sa.ForeignKey("documents.id", ondelete="CASCADE"), nullable=False),
        sa.Column("number", sa.Integer(), nullable=False),
        sa.Column("text", sa.Text(), nullable=False),
        sa.Column("search_text", sa.Text(), nullable=False),
        sa.Column("has_text", sa.Boolean(), nullable=False),
        sa.UniqueConstraint("document_id", "number"),
    )
    op.execute(
        "CREATE VIRTUAL TABLE pages_fts USING fts5("
        "search_text, content='pages', content_rowid='id', tokenize='unicode61 remove_diacritics 2')"
    )
    op.execute(
        "CREATE TRIGGER pages_ai AFTER INSERT ON pages BEGIN "
        "INSERT INTO pages_fts(rowid, search_text) VALUES (new.id, new.search_text); END"
    )
    op.execute(
        "CREATE TRIGGER pages_ad AFTER DELETE ON pages BEGIN "
        "INSERT INTO pages_fts(pages_fts, rowid, search_text) VALUES ('delete', old.id, old.search_text); END"
    )


def downgrade() -> None:
    op.execute("DROP TRIGGER pages_ad")
    op.execute("DROP TRIGGER pages_ai")
    op.execute("DROP TABLE pages_fts")
    op.drop_table("pages")
    op.drop_table("documents")
