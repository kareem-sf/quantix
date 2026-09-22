from alembic import context

from quantix.core.db import Base
from quantix.documents.models import Document  # noqa: F401  (registers the tables on Base.metadata)
from quantix.office.models import Staff  # noqa: F401
from quantix.tenders import Tender  # noqa: F401

connection = context.config.attributes["connection"]
context.configure(connection=connection, target_metadata=Base.metadata, render_as_batch=True)

with context.begin_transaction():
    context.run_migrations()
