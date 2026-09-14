"""Local connection metadata, OS credentials and explicitly requested model discovery."""

from __future__ import annotations

import hashlib
import ipaddress
import json
import os
import threading
from contextlib import contextmanager
from urllib.parse import urlsplit, urlunsplit

import keyring
from pydantic import SecretStr

from .ai_catalog import (
    catalog_price,
    default_base_url,
    direct_provider_presets,
    documented_capabilities,
    provider_preset,
)
from .ai_models import (
    ConnectionInput,
    ConnectionRecord,
    ModelCapabilities,
    ModelInput,
    ModelRecord,
    PriceCard,
)
from .db import dump, new_id, now

_SCHEMA = """
CREATE TABLE IF NOT EXISTS ai_connections (
 id TEXT PRIMARY KEY, data_json TEXT NOT NULL, credential_ref TEXT,
 credential_mode TEXT NOT NULL DEFAULT 'missing'
);
CREATE TABLE IF NOT EXISTS ai_connection_models (
 connection_id TEXT NOT NULL REFERENCES ai_connections(id) ON DELETE CASCADE,
 model_id TEXT NOT NULL, data_json TEXT NOT NULL,
 PRIMARY KEY(connection_id,model_id)
);
CREATE TABLE IF NOT EXISTS ai_connection_metadata (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
"""
_RUNTIME_PROTOCOLS = {"codex", "grok_build"}
_DIRECT_PROTOCOLS = {
    "openai": {"openai_responses", "openai_chat"},
    "anthropic": {"anthropic"},
    "google": {"google"},
    "xai": {"openai_responses", "openai_chat"},
    "custom": {"openai_chat", "openai_responses"},
}
_SECRET_NAMES = {"api_key"}


def validate_base_url(value: str | None, *, allow_insecure_http=False) -> str | None:
    if value is None:
        return None
    parts = urlsplit(value)
    try:
        port = parts.port
    except ValueError as error:
        raise ValueError("Enter a valid provider endpoint port.") from error
    if (parts.scheme not in {"http", "https"} or not parts.hostname or parts.username
            or parts.password or parts.query or parts.fragment or "\\" in value
            or any(ord(character) < 33 for character in value)):
        raise ValueError("Use an HTTP(S) endpoint without credentials, query parameters or fragments.")
    hostname = parts.hostname.lower()
    loopback = hostname == "localhost"
    try:
        loopback = loopback or ipaddress.ip_address(hostname).is_loopback
    except ValueError:
        pass
    if parts.scheme == "http" and not loopback and not allow_insecure_http:
        raise ValueError("Remote providers require HTTPS. Explicitly allow insecure HTTP for a trusted server.")
    if port is not None and not 1 <= port <= 65535:
        raise ValueError("Enter a valid provider endpoint port.")
    return urlunsplit((parts.scheme, parts.netloc.lower(), parts.path.rstrip("/"), "", ""))


def _validate_settings(values: dict, preset: dict) -> dict:
    allowed = set(preset["settings_fields"])
    unknown = set(values) - allowed
    if unknown:
        raise ValueError("This provider has unsupported connection settings. Credentials belong in the credential fields.")
    result = {}
    for name, value in values.items():
        if name == "allow_provider_managed_extras":
            if type(value) is not bool:
                raise ValueError(f"{name.replace('_', ' ')} must be true or false.")
        elif name in {"runtime_timeout_seconds", "max_turns"}:
            maximum = 1800 if name == "runtime_timeout_seconds" else 32
            if type(value) is not int or not 1 <= value <= maximum:
                raise ValueError(f"{name.replace('_', ' ')} must be between 1 and {maximum}.")
        result[name] = value
    if preset["id"] == "grok_build":
        result.setdefault("allow_provider_managed_extras", False)
    return result


def _plain_credentials(values: dict | None) -> dict[str, str]:
    if not values:
        return {}
    if set(values) - _SECRET_NAMES:
        raise ValueError("An unsupported credential field was supplied.")
    result = {}
    for name, value in values.items():
        if isinstance(value, SecretStr):
            value = value.get_secret_value()
        if not isinstance(value, str) or len(value) > 4000:
            raise ValueError("A credential is invalid or too long.")
        if value.strip():
            result[name] = value.strip()
    return result


def is_direct_profile(connection: dict) -> bool:
    """Return whether a saved profile is eligible for the bundled API runtime."""
    provider = connection.get("provider_id")
    allowed_billing = {"unknown", "metered"} if provider == "custom" else {"metered"}
    return (
        provider in _DIRECT_PROTOCOLS
        and connection.get("protocol") in _DIRECT_PROTOCOLS[provider]
        and connection.get("auth_type") in {"api_key", "environment"}
        and connection.get("billing") in allowed_billing
    )


def is_subscription_profile(connection: dict) -> bool:
    """Return whether this is one of the two approved original-client routes."""
    return (
        connection.get("protocol") in {"codex", "grok_build"}
        and connection.get("provider_id") in {"codex", "grok_build"}
        and connection.get("protocol") == connection.get("provider_id")
        and connection.get("auth_type") == "client_login"
        and connection.get("billing") == "subscription"
    )


def is_supported_profile(connection: dict) -> bool:
    """Return whether the saved profile can participate in current setup and work."""
    return is_direct_profile(connection) or is_subscription_profile(connection)


class _WorkspaceState:
    def __init__(self):
        self.lock = threading.RLock()
        self.sessions: dict[str, dict[str, str]] = {}
        self.leases: dict[str, int] = {}
        self.exclusive: set[str] = set()


class AIConnectionService:
    _states: dict[str, _WorkspaceState] = {}
    _states_lock = threading.Lock()

    def __init__(self, repo):
        self.repo = repo
        identity = hashlib.sha256(str(repo.home).encode()).hexdigest()[:16]
        self.service = f"Quantix-{identity}"
        with self._states_lock:
            self._state = self._states.setdefault(str(repo.home), _WorkspaceState())
        with self._state.lock, repo.db.connect(write=True) as conn:
            for statement in _SCHEMA.split(";"):
                if statement.strip():
                    conn.execute(statement)

    @staticmethod
    def presets() -> list[dict]:
        return direct_provider_presets()

    def _keyring(self):
        backend = keyring.get_keyring()
        if not type(backend).__module__.startswith(("keyring.backends.Windows", "keyring.backends.macOS",
                "keyring.backends.SecretService", "keyring.backends.libsecret", "keyring.backends.kwallet")):
            raise ValueError("The operating system credential store is unavailable. Choose session-only credentials.")
        return backend

    def _secret_read(self, reference: str | None) -> dict[str, str]:
        if not reference:
            return {}
        try:
            raw = self._keyring().get_password(self.service, reference)
            if not raw:
                return {}
            return {"api_key": raw} if reference == "openai" else _plain_credentials(json.loads(raw))
        except (keyring.errors.KeyringError, ValueError, TypeError) as error:
            raise ValueError("The saved credential could not be read from the operating system store.") from error

    def _secret_write(self, credentials: dict) -> str:
        reference = f"ai-{new_id()}"
        try:
            self._keyring().set_password(self.service, reference, dump(credentials))
        except (keyring.errors.KeyringError, ValueError) as error:
            raise ValueError("The operating system could not save credentials. Choose session-only storage or repair the credential store.") from error
        return reference

    def _secret_delete(self, reference: str | None):
        if not reference or reference == "openai":
            return  # A reserved legacy reference is never deleted by Quantix.
        try:
            self._keyring().delete_password(self.service, reference)
        except keyring.errors.PasswordDeleteError:
            pass
        except (keyring.errors.KeyringError, ValueError) as error:
            raise ValueError("The operating system could not remove the saved credential.") from error

    def _row(self, identifier):
        with self.repo.db.connect() as conn:
            row = conn.execute("SELECT * FROM ai_connections WHERE id=?", (identifier,)).fetchone()
        if row is None:
            raise KeyError(identifier)
        return dict(row)

    def _public(self, row):
        data = json.loads(row["data_json"])
        auth, mode = data["auth_type"], row["credential_mode"]
        if auth == "client_login":
            state = "runtime"
        elif auth == "environment":
            state = "environment" if os.environ.get(data["environment_key"] or "", "").strip() else "missing"
        elif mode == "session":
            state = "session" if self._state.sessions.get(row["id"]) else "missing"
        elif row["credential_ref"]:
            state = "stored"
        else:
            state = "missing"
        data["credential_state"] = state
        return ConnectionRecord.model_validate(data).model_dump(mode="json")

    def list(self) -> list[dict]:
        with self._state.lock, self.repo.db.connect() as conn:
            rows = conn.execute("SELECT * FROM ai_connections ORDER BY id").fetchall()
            # Accounts saved for providers Quantix no longer offers cannot be loaded.
            supported = [dict(row) for row in rows
                         if is_supported_profile(json.loads(row["data_json"]))]
            return sorted((self._public(row) for row in supported), key=lambda row: row["name"].casefold())

    def get(self, identifier) -> dict:
        with self._state.lock:
            return self._public(self._row(identifier))

    def _validated(self, values) -> tuple[ConnectionInput, dict]:
        values = ConnectionInput.model_validate(values)
        preset = provider_preset(values.provider_id)
        if values.protocol not in preset["protocols"] or values.auth_type not in preset["auth_methods"]:
            raise ValueError("The protocol or authentication method is unavailable for this provider.")
        if values.provider_id in _DIRECT_PROTOCOLS and values.protocol in _DIRECT_PROTOCOLS[values.provider_id]:
            if values.auth_type not in {"api_key", "environment"}:
                raise ValueError("Direct API connections use an explicit API key or environment variable.")
            values.billing = "unknown" if values.provider_id == "custom" else "metered"
        if values.protocol == "grok_build" and values.billing != "subscription":
            raise ValueError("Grok subscription sign-in uses its subscription spending settings. Choose a separate Grok API-key account for API billing.")
        values.settings = _validate_settings(values.settings, preset)
        if values.auth_type == "environment" and not values.environment_key:
            raise ValueError("Enter the environment variable containing this connection's API key.")
        if values.auth_type != "environment" and values.environment_key:
            raise ValueError("Environment key applies only to environment authentication.")
        if values.auth_type in {"client_login", "environment"} and _plain_credentials(values.credentials):
            raise ValueError("This authentication method does not accept stored API credentials.")
        url = values.base_url or default_base_url(values.provider_id, values.protocol)
        values.base_url = validate_base_url(url, allow_insecure_http=values.allow_insecure_http)
        if preset["kind"] == "runtime":
            if values.base_url:
                raise ValueError("Client runtimes do not accept a provider endpoint here.")
        elif values.protocol in {"openai_chat", "openai_responses", "anthropic"} and not values.base_url:
            raise ValueError("Enter the provider's endpoint for the selected protocol.")
        if values.provider_id == "custom" and not values.base_url:
            raise ValueError("Enter the custom provider endpoint.")
        return values, preset

    def _assert_idle(self, identifier):
        if self._state.leases.get(identifier, 0):
            raise ValueError("This connection is in use. Stop its work before changing or removing it.")

    @contextmanager
    def authority_guard(self):
        """Take before a synchronous DB transaction that reads AI authority.

        Profile mutations use the same lock then SQLite order. Never hold this
        guard across an await, provider request or other long-running work.
        """
        with self._state.lock:
            yield

    @contextmanager
    def lease(self, identifier, *, exclusive=False):
        with self._state.lock:
            if identifier in self._state.exclusive:
                raise ValueError("This connection is updating its model catalog. Wait for that action to finish.")
            if exclusive:
                self._assert_idle(identifier)
            connection = self.get(identifier)
            if not connection["enabled"]:
                raise ValueError("This AI connection is disabled.")
            self._state.leases[identifier] = self._state.leases.get(identifier, 0) + 1
            if exclusive:
                self._state.exclusive.add(identifier)
        try:
            yield connection
        finally:
            with self._state.lock:
                remaining = self._state.leases[identifier] - 1
                if remaining:
                    self._state.leases[identifier] = remaining
                else:
                    self._state.leases.pop(identifier, None)
                if exclusive:
                    self._state.exclusive.discard(identifier)

    @staticmethod
    def _destination(data):
        identity_settings = {key: value for key, value in data.get("settings", {}).items()
                             if key in {"project", "location", "region", "aws_profile", "tenant_id", "client_id", "billing_product"}}
        return (data["provider_id"], data["protocol"], data["base_url"], data["auth_type"],
                data.get("environment_key"), dump(identity_settings))

    def account_access_changed(self, identifier):
        """Invalidate old route authority while the caller owns account access.

        Deliberate sign-in can select a different subscription identity. No
        private identity is needed in public records to revoke the old grant.
        """
        with self._state.lock, self.repo.db.connect(write=True) as conn:
            if identifier not in self._state.exclusive or self._state.leases.get(identifier) != 1:
                raise ValueError("Changing client-account authority requires its exclusive account operation.")
            current = self.get(identifier)
            if current["auth_type"] != "client_login":
                raise ValueError("This account does not use original-client sign-in.")
            data = json.loads(self._row(identifier)["data_json"])
            data.update(revision=current["revision"] + 1, updated_at=now(), status="configured", last_error=None)
            conn.execute("UPDATE ai_connections SET data_json=? WHERE id=?", (dump(data), identifier))
        return self.get(identifier)

    def create(self, values) -> dict:
        values, _ = self._validated(values)
        identifier, stamp = new_id(), now()
        data = values.model_dump(mode="json", exclude={"credentials", "session_only"})
        data.update(id=identifier, revision=1, created_at=stamp, updated_at=stamp,
                    credential_state="missing", status="configured", last_error=None)
        secrets = _plain_credentials(values.credentials)
        reference = None
        mode = "session" if values.session_only and secrets else "missing"
        with self._state.lock:
            if secrets and not values.session_only:
                reference, mode = self._secret_write(secrets), "stored"
            try:
                with self.repo.db.connect(write=True) as conn:
                    conn.execute("INSERT INTO ai_connections VALUES(?,?,?,?)", (identifier, dump(data), reference, mode))
            except Exception:
                self._secret_delete(reference)
                raise
            if secrets and values.session_only:
                self._state.sessions[identifier] = secrets
            return self.get(identifier)

    def update(self, identifier, values) -> dict:
        with self._state.lock:
            current_row = self._row(identifier)
            if not is_supported_profile(json.loads(current_row["data_json"])):
                raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        values, _ = self._validated(values)
        if not is_supported_profile(values.model_dump(mode="json", exclude={"credentials", "session_only"})):
            raise ValueError("Choose one of the five supported provider routes for this connection.")
        with self._state.lock:
            self._assert_idle(identifier)
            row = self._row(identifier)
            old = self._public(row)
            data = values.model_dump(mode="json", exclude={"credentials", "session_only"})
            changed_destination = self._destination(old) != self._destination(data)
            if (changed_destination and old["auth_type"] == "environment"
                    and values.auth_type == "environment" and values.environment_key == old["environment_key"]):
                raise ValueError("Create a separate connection when reusing an environment credential with another destination.")
            if changed_destination and values.credentials is None and old["credential_state"] in {"stored", "session", "environment"}:
                raise ValueError("Changing the destination requires newly supplied credentials, or an explicit empty credential object to clear them.")
            reference, mode = row["credential_ref"], row["credential_mode"]
            secrets = _plain_credentials(values.credentials)
            replace_credentials = values.credentials is not None
            if replace_credentials:
                reference, mode = None, "missing"
                if secrets:
                    if values.session_only:
                        mode = "session"
                    else:
                        reference, mode = self._secret_write(secrets), "stored"
            data.update(id=identifier, revision=old["revision"] + 1, created_at=old["created_at"],
                        updated_at=now(), credential_state="missing", status="configured", last_error=None)
            try:
                with self.repo.db.connect(write=True) as conn:
                    conn.execute("UPDATE ai_connections SET data_json=?,credential_ref=?,credential_mode=? WHERE id=?",
                                 (dump(data), reference, mode, identifier))
                    if changed_destination:
                        conn.execute("DELETE FROM ai_connection_models WHERE connection_id=?", (identifier,))
            except Exception:
                if reference != row["credential_ref"]:
                    self._secret_delete(reference)
                raise
            if replace_credentials:
                self._state.sessions.pop(identifier, None)
                if mode == "session":
                    self._state.sessions[identifier] = secrets
                if row["credential_ref"] != reference:
                    self._secret_delete(row["credential_ref"])
            return self.get(identifier)

    def delete(self, identifier):
        with self._state.lock:
            self._assert_idle(identifier)
            row = self._row(identifier)
            self._secret_delete(row["credential_ref"])
            from .ai_policy_repair import prune_deleted_accounts

            with self.repo.db.connect(write=True) as conn:
                conn.execute("DELETE FROM ai_connection_models WHERE connection_id=?", (identifier,))
                conn.execute("DELETE FROM ai_connections WHERE id=?", (identifier,))
                # Forget the account everywhere in the same transaction. A
                # Tender that still named it could not have its AI changed
                # again: saving re-validates the Tender's own allow list.
                prune_deleted_accounts(conn, {identifier})
            self._state.sessions.pop(identifier, None)
        return {"deleted": True}

    def rename(self, identifier, name):
        name = name.strip()
        if not name or len(name) > 150:
            raise ValueError("Enter an account name between 1 and 150 characters.")
        with self._state.lock, self.repo.db.connect(write=True) as conn:
            row = self._row(identifier)
            data = json.loads(row["data_json"])
            data.update(name=name, updated_at=now())
            conn.execute("UPDATE ai_connections SET data_json=? WHERE id=?", (dump(data), identifier))
        return self.get(identifier)

    def credentials(self, identifier) -> dict[str, str]:
        """Private execution boundary. Never return this from an HTTP route or event."""
        with self._state.lock:
            row = self._row(identifier)
            data = self._public(row)
            if data["auth_type"] == "environment":
                value = os.environ.get(data["environment_key"], "").strip()
                return {"api_key": value} if value else {}
            if row["credential_mode"] == "session":
                return dict(self._state.sessions.get(identifier, {}))
            return self._secret_read(row["credential_ref"])

    def models(self, identifier) -> list[dict]:
        self.get(identifier)
        with self.repo.db.connect() as conn:
            rows = conn.execute("SELECT data_json FROM ai_connection_models WHERE connection_id=? ORDER BY model_id", (identifier,)).fetchall()
        return [ModelRecord.model_validate_json(row[0]).model_dump(mode="json") for row in rows]

    def save_model(self, identifier, values, *, source="manual") -> dict:
        model = ModelInput.model_validate(values)
        with self._state.lock:
            self._assert_idle(identifier)
            connection = self.get(identifier)
            if model.pricing is None:
                price = catalog_price(connection["provider_id"], model.model_id)
                model.pricing = PriceCard.model_validate(price) if price else None
            result = ModelRecord(**model.model_dump(), connection_id=identifier, source=source, updated_at=now()).model_dump(mode="json")
            with self.repo.db.connect(write=True) as conn:
                old = conn.execute("SELECT data_json FROM ai_connection_models WHERE connection_id=? AND model_id=?",
                                   (identifier, model.model_id)).fetchone()
                old_value = json.loads(old[0]) if old else None
                if (source == "manual" and old_value and old_value.get("source") in {"provider", "catalog"}
                    and old_value.get("model_id") == result["model_id"] and old_value.get("display_name") == result["display_name"]
                    and ModelCapabilities.model_validate(old_value.get("capabilities") or {}).model_dump(mode="json") == result["capabilities"]):
                    # Pricing has its own dated provenance. Editing only the
                    # price must not turn observed model capabilities into an
                    # unverified manual assertion.
                    result["source"] = old_value["source"]
                if old_value is None or not self._capabilities_unchanged(conn, identifier, old_value, result, connection["revision"]):
                    self._invalidate_model_readiness(conn, identifier, model.model_id)
                conn.execute("INSERT INTO ai_connection_models VALUES(?,?,?) ON CONFLICT(connection_id,model_id) DO UPDATE SET data_json=excluded.data_json",
                             (identifier, model.model_id, dump(result)))
            return result

    def observe_model_capabilities(self, identifier, model_id, capabilities, expected_revision):
        """Store capabilities proved by a completed check without revoking authority.

        The account/model identity and its checked connection revision stay
        unchanged. Catalog changes invalidate only affected model readiness;
        credentials, destination and billing retain their account revision.
        """
        values = ModelCapabilities.model_validate(capabilities)
        with self._state.lock, self.repo.db.connect(write=True) as conn:
            self._assert_idle(identifier)
            row = conn.execute(
                "SELECT data_json FROM ai_connection_models WHERE connection_id=? AND model_id=?",
                (identifier, model_id),
            ).fetchone()
            if row is None:
                raise KeyError(model_id)
            connection = self.get(identifier)
            if connection["revision"] != expected_revision:
                raise ValueError("The connection changed during its model check. Check the model again.")
            data = json.loads(row[0])
            current = ModelCapabilities.model_validate(data.get("capabilities") or {})
            observed = values.model_dump(mode="json")
            for field in ("tools", "structured_output"):
                if observed.get(field) is True:
                    setattr(current, field, True)
            data["capabilities"] = current.model_dump(mode="json")
            # Do not touch updated_at: this is observed evidence, not a new
            # provider catalog identity or account destination.
            conn.execute(
                "UPDATE ai_connection_models SET data_json=? WHERE connection_id=? AND model_id=?",
                (dump(data), identifier, model_id),
            )
        return next(item for item in self.models(identifier) if item["model_id"] == model_id)

    @staticmethod
    def _model_evidence(conn, identifier, model_id):
        if not conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_setup_model_checks'").fetchone():
            return {}
        row = conn.execute("SELECT data_json FROM ai_setup_model_checks WHERE connection_id=? AND model_id=?",
                           (identifier, model_id)).fetchone()
        return json.loads(row[0]) if row else {}

    def _capabilities_unchanged(self, conn, identifier, old, new, revision):
        previous = ModelCapabilities.model_validate(old.get("capabilities") or {}).model_dump()
        incoming = ModelCapabilities.model_validate(new.get("capabilities") or {}).model_dump()
        evidence = self._model_evidence(conn, identifier, old["model_id"])
        checked = evidence.get("check", {}).get("status") == "passed" and evidence.get("checked_revision") == revision
        # A completed check can confirm these previously omitted capabilities.
        for field in ("tools", "structured_output"):
            if checked and previous[field] is None and incoming[field] is True:
                previous[field] = True
        return previous == incoming

    @staticmethod
    def _invalidate_model_readiness(conn, identifier, model_id):
        # Clear both stores: the selected-account copy can otherwise resurrect
        # removed proof during SetupStore's legacy evidence backfill.
        if conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_setup_model_checks'").fetchone():
            conn.execute("DELETE FROM ai_setup_model_checks WHERE connection_id=? AND model_id=?", (identifier, model_id))
        if conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_setup_accounts'").fetchone():
            row = conn.execute("SELECT data_json FROM ai_setup_accounts WHERE connection_id=?", (identifier,)).fetchone()
            if row:
                saved = json.loads(row[0])
                if saved.get("check", {}).get("model_id") == model_id:
                    saved.update(check={"status": "not_checked", "detail": "Check this model again before using it."},
                                 checked_revision=None, checked_component_version=None)
                    conn.execute("UPDATE ai_setup_accounts SET data_json=? WHERE connection_id=?", (dump(saved), identifier))

    def store_discovered_models(self, identifier, values, expected_revision):
        validated = [ModelInput.model_validate(value) for value in values]
        with self._state.lock, self.repo.db.connect(write=True) as conn:
            self._assert_idle(identifier)
            if self.get(identifier)["revision"] != expected_revision:
                raise ValueError("The connection changed during model discovery. Discover its models again.")
            available = {model.model_id for model in validated}
            for old in conn.execute("SELECT model_id,data_json FROM ai_connection_models WHERE connection_id=?", (identifier,)).fetchall():
                if old["model_id"] not in available and json.loads(old["data_json"])["source"] != "manual":
                    self._invalidate_model_readiness(conn, identifier, old["model_id"])
                    conn.execute("DELETE FROM ai_connection_models WHERE connection_id=? AND model_id=?", (identifier, old["model_id"]))
            for model in validated:
                old = conn.execute("SELECT data_json FROM ai_connection_models WHERE connection_id=? AND model_id=?", (identifier, model.model_id)).fetchone()
                old_value = json.loads(old[0]) if old else None
                if old_value and old_value["source"] == "manual":
                    continue
                if old_value:
                    if model.pricing is None and old_value.get("pricing"):
                        model.pricing = PriceCard.model_validate(old_value["pricing"])
                    evidence = self._model_evidence(conn, identifier, model.model_id)
                    if evidence.get("check", {}).get("status") == "passed" and evidence.get("checked_revision") == expected_revision:
                        for field in ("tools", "structured_output"):
                            if getattr(model.capabilities, field) is None and old_value.get("capabilities", {}).get(field) is True:
                                setattr(model.capabilities, field, True)
                entry = ModelRecord(**model.model_dump(), connection_id=identifier, source="provider", updated_at=now()).model_dump(mode="json")
                if old_value is None or not self._capabilities_unchanged(conn, identifier, old_value, entry, expected_revision):
                    self._invalidate_model_readiness(conn, identifier, model.model_id)
                conn.execute("INSERT INTO ai_connection_models VALUES(?,?,?) ON CONFLICT(connection_id,model_id) DO UPDATE SET data_json=excluded.data_json",
                             (identifier, model.model_id, dump(entry)))
            self._status(identifier, "models_discovered")
        return self.models(identifier)

    def _status(self, identifier, status, message=None):
        with self._state.lock, self.repo.db.connect(write=True) as conn:
            row = conn.execute("SELECT data_json FROM ai_connections WHERE id=?", (identifier,)).fetchone()
            if row is None:
                raise KeyError(identifier)
            data = json.loads(row[0])
            data.update(status=status, last_error=message)
            conn.execute("UPDATE ai_connections SET data_json=? WHERE id=?", (dump(data), identifier))

    def mark_used(self, identifier):
        self._status(identifier, "used")
        return self.get(identifier)

    def mark_error(self, identifier, message):
        # Provider exceptions can contain keys, request payloads and private Tender content.
        self._status(identifier, "needs_attention", "The AI request failed. Check the model, credentials, permissions and provider limits.")
        return self.get(identifier)

    async def discover(self, identifier) -> list[dict]:
        """Only the explicit Discover models action invokes remote metadata requests."""
        connection = self.get(identifier)
        if not is_supported_profile(connection):
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        with self.lease(identifier, exclusive=True) as connection:
            credentials = self.credentials(identifier)
            try:
                if is_subscription_profile(connection):
                    from .ai_worker_client import AIWorkerClient
                    discovered = await AIWorkerClient(self.repo).catalog(connection, credentials)
                else:
                    from .ai_direct import DirectAPIService
                    discovered = await DirectAPIService(self.repo).catalog(connection, credentials)
                for item in discovered:
                    documented = documented_capabilities(connection["provider_id"], connection["protocol"], item["model_id"])
                    item["capabilities"].update({key: value for key, value in documented.items()
                                                 if item["capabilities"].get(key) is not False})
                    if item.get("pricing") is None and is_direct_profile(connection):
                        item["pricing"] = catalog_price(connection["provider_id"], item["model_id"])
                revision = connection["revision"]
            except Exception as error:
                self.mark_error(identifier, "discovery")
                raise ValueError("Model discovery failed. Check the endpoint and credentials, or add a model manually.") from error
        return self.store_discovered_models(identifier, discovered, revision)

def _model_entry(item: dict, protocol: str) -> dict | None:
    identifier = item.get("id") or item.get("name") or item.get("modelId")
    if not isinstance(identifier, str) or not identifier.strip():
        return None
    if protocol == "google" and identifier.startswith("models/"):
        identifier = identifier[7:]
    caps = {}
    raw = item.get("capabilities") or {}
    if isinstance(raw, dict):
        for target, source in [("tools", "function_calling"), ("images", "vision"),
                               ("structured_output", "structured_outputs"), ("images", "image_input"), ("pdf", "pdf_input")]:
            value = raw.get(source)
            if isinstance(value, dict):
                value = value.get("supported")
            if type(value) is bool:
                caps[target] = value
        effort = raw.get("effort", {})
        if isinstance(effort, dict):
            caps["reasoning"] = [name for name, entry in effort.items() if isinstance(entry, dict) and entry.get("supported") is True]
    for target, sources in [("context_window", ("context_length", "max_context_length", "max_input_tokens", "inputTokenLimit")),
                            ("max_output_tokens", ("max_tokens", "outputTokenLimit"))]:
        for source in sources:
            value = item.get(source)
            if type(value) is int and value > 0:
                caps[target] = value
                break
    parameters = item.get("supported_parameters")
    if isinstance(parameters, list):
        if "tools" in parameters:
            caps["tools"] = True
        if "structured_outputs" in parameters:
            caps["structured_output"] = True
    architecture = item.get("architecture") or {}
    if isinstance(architecture, dict) and isinstance(architecture.get("input_modalities"), list):
        caps["images"] = "image" in architecture["input_modalities"]
    return ModelInput(model_id=identifier, display_name=item.get("display_name") or item.get("displayName") or item.get("modelName") or identifier,
                      capabilities=caps).model_dump(mode="json")
