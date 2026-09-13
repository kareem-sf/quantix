"""Saved mail passwords belong to the server account they were entered for."""

import importlib

import pytest

from quantix.repository import Repository


@pytest.fixture
def service(tmp_path, monkeypatch):
    module = importlib.import_module("quantix.correspondence")
    saved = {}
    monkeypatch.setattr(module.keyring, "get_password", lambda service, user: saved.get((service, user)))
    monkeypatch.setattr(
        module.keyring, "set_password", lambda service, user, password: saved.__setitem__((service, user), password)
    )

    def delete(service, user):
        if (service, user) not in saved:
            raise module.keyring.errors.PasswordDeleteError("missing")
        saved.pop((service, user))

    monkeypatch.setattr(module.keyring, "delete_password", delete)

    def no_live_mail(*args, **kwargs):
        raise AssertionError("Live SMTP and IMAP access is prohibited in development tests.")

    for name in ("SMTP", "SMTP_SSL"):
        monkeypatch.setattr(module.smtplib, name, no_live_mail)
    monkeypatch.setattr(module.imaplib, "IMAP4_SSL", no_live_mail)
    return module.QuoteService(Repository(tmp_path)), saved


ACCOUNT = {
    "smtp_host": "smtp.example.com",
    "smtp_port": 465,
    "smtp_security": "ssl",
    "smtp_username": "engineer@example.com",
    "from_address": "engineer@example.com",
    "imap_host": "imap.example.com",
    "imap_port": 993,
    "imap_username": "engineer@example.com",
}


def test_changing_the_server_does_not_reuse_a_saved_password(service):
    quotes, saved = service
    ready = quotes.update_settings({**ACCOUNT, "smtp_password": "smtp-secret", "imap_password": "imap-secret"})
    assert ready["smtp_ready"] is True and ready["imap_ready"] is True

    moved = quotes.update_settings({"smtp_host": "smtp.other.example", "imap_username": "someone@example.com"})

    assert moved["smtp_ready"] is False
    assert moved["imap_ready"] is False
    assert quotes._password("smtp") is None
    assert quotes._password("imap") is None
    with pytest.raises(ValueError, match="Configure the IMAP SSL account"):
        quotes.sync_replies()


def test_switching_back_finds_the_password_saved_for_that_account(service):
    quotes, _saved = service
    quotes.update_settings({**ACCOUNT, "smtp_password": "smtp-secret"})
    quotes.update_settings({"smtp_username": "other@example.com"})

    restored = quotes.update_settings({"smtp_username": "engineer@example.com"})

    assert restored["smtp_ready"] is True
    assert quotes._password("smtp") == "smtp-secret"


def test_clearing_a_password_removes_only_that_account_and_legacy_entries(service):
    quotes, saved = service
    saved[(quotes.keyring_service, "smtp")] = "unbound-legacy-secret"
    quotes.update_settings({**ACCOUNT, "smtp_password": "smtp-secret"})

    assert (quotes.keyring_service, "smtp") not in saved
    assert quotes.update_settings({"smtp_password": ""})["smtp_ready"] is False
    assert "smtp-secret" not in saved.values()
