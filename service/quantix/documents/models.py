import uuid
from datetime import UTC, datetime

from sqlalchemy import Boolean, Float, ForeignKey, Integer, LargeBinary, String, Text
from sqlalchemy.orm import Mapped, mapped_column

from quantix.core.db import Base, UTCDateTime

# waiting → reading → read | unreadable | failed; a document later replaced by a newer copy becomes "replaced".
STATUSES = ("waiting", "reading", "read", "unreadable", "failed", "replaced")


class Document(Base):
    __tablename__ = "documents"

    id: Mapped[str] = mapped_column(String(32), primary_key=True, default=lambda: uuid.uuid4().hex)
    tender_id: Mapped[str] = mapped_column(ForeignKey("tenders.id", ondelete="CASCADE"))
    path: Mapped[str] = mapped_column(String(1000))  # path inside the package, with forward slashes
    kind: Mapped[str] = mapped_column(String(20))
    size: Mapped[int] = mapped_column(Integer)
    sha256: Mapped[str] = mapped_column(String(64))
    status: Mapped[str] = mapped_column(String(20), default="waiting")
    note: Mapped[str | None] = mapped_column(Text)
    page_count: Mapped[int | None] = mapped_column(Integer)
    group_name: Mapped[str | None] = mapped_column(String(200))
    description: Mapped[str | None] = mapped_column(Text)
    created_at: Mapped[datetime] = mapped_column(UTCDateTime, default=lambda: datetime.now(UTC))
    read_at: Mapped[datetime | None] = mapped_column(UTCDateTime)  # when Quantix finished with it, however it went

    @property
    def name(self) -> str:
        return self.path.rsplit("/", 1)[-1]


class Page(Base):
    __tablename__ = "pages"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    document_id: Mapped[str] = mapped_column(ForeignKey("documents.id", ondelete="CASCADE"))
    number: Mapped[int] = mapped_column(Integer)
    text: Mapped[str] = mapped_column(Text)
    search_text: Mapped[str] = mapped_column(Text)
    has_text: Mapped[bool] = mapped_column(Boolean)
    width: Mapped[float | None] = mapped_column(Float)  # PDF page size in points, for takeoff geometry
    height: Mapped[float | None] = mapped_column(Float)


class PageChunk(Base):
    """One passage of a page in the meaning index. A page with nothing readable gets one row without a digest, so
    it counts as indexed."""

    __tablename__ = "page_chunks"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    page_id: Mapped[int] = mapped_column(ForeignKey("pages.id", ondelete="CASCADE"))
    start: Mapped[int] = mapped_column(Integer)  # where the passage starts and stops in the page's text
    stop: Mapped[int] = mapped_column(Integer)
    digest: Mapped[str | None] = mapped_column(String(64))


class Vector(Base):
    """A text's meaning as the model computes it, kept by the text's digest so the same text is computed once."""

    __tablename__ = "vectors"

    digest: Mapped[str] = mapped_column(String(64), primary_key=True)
    vector: Mapped[bytes] = mapped_column(LargeBinary)
