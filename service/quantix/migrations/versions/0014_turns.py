"""The office's turns, and whether a tender's office is stopped, so both survive a restart

Revision ID: 0014
Revises: 0013
"""

import sqlalchemy as sa
from alembic import op

revision = "0014"
down_revision = "0013"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "turns",
        sa.Column("id", sa.Integer(), primary_key=True, autoincrement=True),
        sa.Column("tender_id", sa.String(32), sa.ForeignKey("tenders.id", ondelete="CASCADE"), nullable=False),
        sa.Column("staff_id", sa.String(32), sa.ForeignKey("staff.id", ondelete="CASCADE"), nullable=False),
        sa.Column("model", sa.String(200), nullable=False),
        sa.Column("started_at", sa.DateTime(), nullable=False),
        sa.Column("ended_at", sa.DateTime(), nullable=True),
        sa.Column("ended", sa.String(20), nullable=True),
        sa.Column("note", sa.Text(), nullable=True),
        sa.Column("calls", sa.JSON(), nullable=False),
        sa.Column("requests", sa.Integer(), nullable=False),
        sa.Column("input_tokens", sa.Integer(), nullable=False),
        sa.Column("cached_tokens", sa.Integer(), nullable=False),
        sa.Column("output_tokens", sa.Integer(), nullable=False),
    )
    op.create_index("turns_staff", "turns", ["tender_id", "staff_id", "id"])
    with op.batch_alter_table("tenders") as table:
        table.add_column(sa.Column("office_paused", sa.Boolean(), nullable=False, server_default=sa.false()))


def downgrade() -> None:
    with op.batch_alter_table("tenders") as table:
        table.drop_column("office_paused")
    op.drop_index("turns_staff", "turns")
    op.drop_table("turns")
