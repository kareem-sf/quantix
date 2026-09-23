from alembic import context

from quantix.boq.models import BoqItem  # noqa: F401  (registers the tables on Base.metadata)
from quantix.company import CompanyRule  # noqa: F401
from quantix.core.db import Base
from quantix.documents.models import Document  # noqa: F401
from quantix.estimate.models import Rate  # noqa: F401
from quantix.office.models import Staff  # noqa: F401
from quantix.subcontract.models import Package  # noqa: F401
from quantix.submission.models import Draft  # noqa: F401
from quantix.takeoff.models import Scale  # noqa: F401
from quantix.tenders import Tender  # noqa: F401

connection = context.config.attributes["connection"]
context.configure(connection=connection, target_metadata=Base.metadata, render_as_batch=True)

with context.begin_transaction():
    context.run_migrations()
