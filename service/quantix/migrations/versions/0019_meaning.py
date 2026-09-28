"""The meaning index: page passages and the vectors of texts

Revision ID: 0019
Revises: 0018
"""

import sqlalchemy as sa
from alembic import op

revision = "0019"
down_revision = "0018"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "vectors",
        sa.Column("digest", sa.String(64), primary_key=True),
        sa.Column("vector", sa.LargeBinary(), nullable=False),
    )
    op.create_table(
        "page_chunks",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column("page_id", sa.Integer(), sa.ForeignKey("pages.id", ondelete="CASCADE"), nullable=False),
        sa.Column("start", sa.Integer(), nullable=False),
        sa.Column("stop", sa.Integer(), nullable=False),
        sa.Column("digest", sa.String(64), nullable=True),
    )
    op.create_index("page_chunks_page", "page_chunks", ["page_id"])


def downgrade() -> None:
    op.drop_index("page_chunks_page", "page_chunks")
    op.drop_table("page_chunks")
    op.drop_table("vectors")
