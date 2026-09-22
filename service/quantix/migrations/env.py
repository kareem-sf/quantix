from alembic import context

from quantix.core.db import Base
from quantix.tenders import Tender  # noqa: F401  (registers the table on Base.metadata)

connection = context.config.attributes["connection"]
context.configure(connection=connection, target_metadata=Base.metadata, render_as_batch=True)

with context.begin_transaction():
    context.run_migrations()
