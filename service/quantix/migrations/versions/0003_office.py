"""The office: staff, messages, tasks and decisions

Revision ID: 0003
Revises: 0002
"""

import sqlalchemy as sa
from alembic import op

revision = "0003"
down_revision = "0002"
branch_labels = None
depends_on = None


def _tender() -> sa.Column:
    return sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False)


def upgrade() -> None:
    op.create_table(
        "staff",
        sa.Column("id", sa.String(32), primary_key=True),
        _tender(),
        sa.Column("is_manager", sa.Boolean(), nullable=False),
        sa.Column("name", sa.String(100), nullable=False),
        sa.Column("role", sa.String(100), nullable=False),
        sa.Column("profile", sa.JSON(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("now", sa.String(300), nullable=True),
        sa.Column("last_read", sa.Integer(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_table(
        "messages",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        _tender(),
        sa.Column("sender", sa.String(32), nullable=False),
        sa.Column("channel", sa.String(32), nullable=False),
        sa.Column("kind", sa.String(20), nullable=False),
        sa.Column("text", sa.Text(), nullable=False),
        sa.Column("created_at", sa.DateTime(), nullable=False),
    )
    op.create_index("messages_channel", "messages", ["tender_id", "channel", "id"])
    op.create_table(
        "tasks",
        sa.Column("id", sa.String(32), primary_key=True),
        _tender(),
        sa.Column("staff_id", sa.String(32), sa.ForeignKey("staff.id", ondelete="CASCADE"), nullable=False),
        sa.Column("title", sa.String(300), nullable=False),
        sa.Column("brief", sa.Text(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("result", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("done_at", sa.DateTime(), nullable=True),
    )
    op.create_table(
        "decisions",
        sa.Column("id", sa.String(32), primary_key=True),
        _tender(),
        sa.Column("raised_by", sa.String(32), nullable=False),
        sa.Column("gate", sa.String(40), nullable=True),
        sa.Column("title", sa.String(300), nullable=False),
        sa.Column("text", sa.Text(), nullable=False),
        sa.Column("options", sa.JSON(), nullable=False),
        sa.Column("status", sa.String(20), nullable=False),
        sa.Column("answer", sa.Text(), nullable=True),
        sa.Column("created_at", sa.DateTime(), nullable=False),
        sa.Column("decided_at", sa.DateTime(), nullable=True),
    )


def downgrade() -> None:
    for table in ("decisions", "tasks", "messages", "staff"):
        op.drop_table(table)
