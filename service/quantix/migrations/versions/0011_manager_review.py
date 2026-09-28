"""The Tender Manager reviews every record the staff propose before it reaches the engineer

Revision ID: 0011
Revises: 0010
"""

import sqlalchemy as sa
from alembic import op

revision = "0011"
down_revision = "0010"
branch_labels = None
depends_on = None

REVIEWED = ("boq_items", "facts", "scales", "measurements", "rates", "markups", "drafts")
# no review status of their own: reviewed once the Manager has looked at them
MARKED = ("requirements", "enquiries", "packages")


def upgrade() -> None:
    for name in (*REVIEWED, *MARKED):
        with op.batch_alter_table(name) as table:
            table.add_column(sa.Column("reviewed_by", sa.String(32), nullable=True))
            table.add_column(sa.Column("reviewed_at", sa.DateTime(), nullable=True))
            table.add_column(sa.Column("review_note", sa.Text(), nullable=True))
    with op.batch_alter_table("staff") as table:
        table.add_column(sa.Column("reviewed_up_to", sa.DateTime(), nullable=True))
    with op.batch_alter_table("packages") as table:
        table.add_column(sa.Column("recommended_at", sa.DateTime(), nullable=True))
    # What the office added before review existed was already in the engineer's hands
    op.execute("UPDATE requirements SET reviewed_by = 'engineer', reviewed_at = created_at")
    op.execute("UPDATE enquiries SET reviewed_by = 'engineer', reviewed_at = created_at")
    op.execute(
        "UPDATE packages SET reviewed_by = 'engineer', reviewed_at = created_at WHERE recommended_quote_id IS NOT NULL"
    )


def downgrade() -> None:
    with op.batch_alter_table("packages") as table:
        table.drop_column("recommended_at")
    with op.batch_alter_table("staff") as table:
        table.drop_column("reviewed_up_to")
    for name in (*REVIEWED, *MARKED):
        with op.batch_alter_table(name) as table:
            table.drop_column("review_note")
            table.drop_column("reviewed_at")
            table.drop_column("reviewed_by")
