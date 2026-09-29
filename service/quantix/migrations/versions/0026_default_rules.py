"""Company rules any firm can start from: examples in every market, marked so the engineer adjusts or removes them

Revision ID: 0026
Revises: 0025
"""

import uuid
from datetime import UTC, datetime

import sqlalchemy as sa
from alembic import op

revision = "0026"
down_revision = "0025"
branch_labels = None
depends_on = None

# No currency, tax, method of measurement or local standard here: the office reads those from each tender. Where a
# figure is given it is an example to change, and the rule says so.
DEFAULTS = [
    (
        "Quantities",
        "Check the BOQ quantities against the drawings. Where they differ by more than 5%, or work is drawn "
        "but not billed, raise a tender query; never change the client's quantities.",
    ),
    (
        "Quantities",
        "Measure as the tender's method of measurement says. Waste, laps and working space go in the rate, "
        "not in the quantity.",
    ),
    (
        "Rates",
        "Build up every significant rate from labour, plant, materials and subcontract, with the outputs and "
        "wastage stated. A rate taken whole needs a dated source.",
    ),
    (
        "Rates",
        "Use a quote only while it is valid: past its stated validity, or older than 30 days if it states none, "
        "it needs the supplier's confirmation.",
    ),
    (
        "Rates",
        "Treat library and past-tender rates as benchmarks. Adjust them for date, location and quantity before "
        "use, and say how.",
    ),
    (
        "Rates",
        "Price in the tender's currency. For anything bought in another currency, state the exchange rate and "
        "its date.",
    ),
    (
        "Markups",
        "Keep preliminaries, overheads and profit apart from the unit rates. Spread them into the rates only "
        "in the priced BOQ, and only when the tender asks for it.",
    ),
    (
        "Markups",
        "Price time-related preliminaries (staff, site facilities, utilities, equipment hire) on the programme "
        "duration and fixed ones as lump sums. State the duration and whether each rate is per day, week or month.",
    ),
    (
        "Markups",
        "Overheads and profit are the engineer's decision on each tender. Propose them with the reasons; "
        "never assume them.",
    ),
    (
        "Markups",
        "Price the contract's commercial terms where they cost money: retention, bonds and guarantees, "
        "insurances, payment terms and delay damages. Say where each is carried.",
    ),
    (
        "Subcontract",
        "Ask at least three firms for every major package. When fewer quote, say so in the recommendation.",
    ),
    (
        "Subcontract",
        "Level quotes line by line on the same quantities: price back exclusions and fill gaps with our "
        "own rate before comparing totals.",
    ),
    (
        "Subcontract",
        "Recommend on value, not price alone: scope covered, programme, capacity, past performance and qualifications.",
    ),
    (
        "Exclusions and qualifications",
        "List every exclusion and qualification in the submission; never leave one only in a rate note.",
    ),
    (
        "Exclusions and qualifications",
        "Don't qualify against the tender conditions without the engineer's approval. Raise a tender query first.",
    ),
    (
        "Tender queries",
        "Raise a query for work drawn or specified but not billed, for documents that disagree and for "
        "errors in the BOQ. Cite the documents and say which one governs.",
    ),
    (
        "Tender queries",
        "Write each query as a plain request for confirmation, with the BOQ item and the document reference.",
    ),
    (
        "Submission",
        "Follow the tender's required forms, order and numbering exactly, with the document names and "
        "annexure references as issued.",
    ),
    (
        "Submission",
        "Statements on health and safety, quality, insurance, guarantees and similar state the contract's "
        "specific obligations, not general promises, and commit to nothing the tender doesn't ask for.",
    ),
    (
        "Submission",
        "Never change the client's BOQ descriptions, units or quantities in the priced BOQ. Where an item "
        "appears in more than one bill, keep each bill's own reference.",
    ),
    (
        "Risk",
        "Flag provisional sums, prime cost sums and quantities that look short. Add a risk allowance only with "
        "the engineer's approval, and say what it covers.",
    ),
    (
        "House style",
        "Write in plain, short sentences. Give every figure with its unit, and every price with its currency.",
    ),
    ("House style", "Every figure in a submission document must match the priced BOQ."),
]


def upgrade() -> None:
    op.add_column("company_rules", sa.Column("example", sa.Boolean(), nullable=False, server_default=sa.false()))
    rules = sa.table(
        "company_rules",
        sa.column("id", sa.String),
        sa.column("owner_id", sa.String),
        sa.column("topic", sa.String),
        sa.column("text", sa.Text),
        sa.column("example", sa.Boolean),
        sa.column("created_at", sa.DateTime),
    )
    now = datetime.now(UTC).replace(tzinfo=None)
    op.bulk_insert(
        rules,
        [
            {"id": uuid.uuid4().hex, "owner_id": "local", "topic": t, "text": text, "example": True, "created_at": now}
            for t, text in DEFAULTS
        ],
    )


def downgrade() -> None:
    op.execute("DELETE FROM company_rules WHERE example")
    op.drop_column("company_rules", "example")
