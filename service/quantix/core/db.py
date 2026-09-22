from collections.abc import Iterator
from datetime import UTC, datetime
from pathlib import Path

from alembic import command
from alembic.config import Config
from sqlalchemy import DateTime, Engine, TypeDecorator, create_engine, event
from sqlalchemy.orm import DeclarativeBase, Session, sessionmaker

MIGRATIONS = Path(__file__).resolve().parent.parent / "migrations"


class Base(DeclarativeBase):
    pass


class UTCDateTime(TypeDecorator[datetime]):
    """Stores aware datetimes as UTC and reads them back as UTC; SQLite itself keeps no time zone."""

    impl = DateTime
    cache_ok = True

    def process_bind_param(self, value: datetime | None, dialect) -> datetime | None:
        return value.astimezone(UTC).replace(tzinfo=None) if value else None

    def process_result_value(self, value: datetime | None, dialect) -> datetime | None:
        return value.replace(tzinfo=UTC) if value else None


def open_database(home: Path) -> sessionmaker[Session]:
    """Open the database under the data home, bring its schema up to date and return a session factory."""
    home.mkdir(parents=True, exist_ok=True)
    engine = create_engine(f"sqlite:///{home / 'quantix.sqlite'}")
    event.listen(engine, "connect", _sqlite_pragmas)
    _upgrade(engine)
    return sessionmaker(engine, expire_on_commit=False)


def sessions(factory: sessionmaker[Session]) -> Iterator[Session]:
    with factory() as session:
        yield session


def _upgrade(engine: Engine) -> None:
    config = Config()
    config.set_main_option("script_location", str(MIGRATIONS))
    with engine.begin() as connection:
        config.attributes["connection"] = connection
        command.upgrade(config, "head")


def _sqlite_pragmas(connection, _record) -> None:
    cursor = connection.cursor()
    cursor.execute("PRAGMA foreign_keys=ON")
    cursor.execute("PRAGMA journal_mode=WAL")
    cursor.close()
