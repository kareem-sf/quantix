"""Public settings and OS-owned credentials; credentials never enter the database."""

import hashlib
import os

import keyring

from .models import Settings, SettingsPatch


class SettingsService:
    def __init__(self, repo):
        self.repo = repo
        identity = hashlib.sha256(str(repo.home).encode()).hexdigest()[:16]
        self.service = f"Quantix-{identity}"

    def api_key(self) -> str | None:
        configured = os.environ.get("OPENAI_API_KEY", "").strip()
        if configured:
            return configured
        try:
            return keyring.get_password(self.service, "openai")
        except keyring.errors.KeyringError:
            return None

    def public(self) -> Settings:
        present = bool(self.api_key())
        return Settings(
            provider_ready=present,
            model=self.repo.setting("model", "gpt-6-astra"),
            default_currency=self.repo.setting("default_currency", "EGP"),
            home=str(self.repo.home),
            preferences=self.repo.setting("preferences", ""),
            provider_detail="API connection configured. Live work will verify access."
            if present
            else "Add your OpenAI API key to enable the Tender Manager and market research.",
        )

    def update(self, patch: SettingsPatch) -> Settings:
        if patch.api_key is not None:
            secret = patch.api_key.get_secret_value().strip()
            if len(secret) > 1000:
                raise ValueError("The API key is too long.")
            try:
                if secret:
                    keyring.set_password(self.service, "openai", secret)
                else:
                    try:
                        keyring.delete_password(self.service, "openai")
                    except keyring.errors.PasswordDeleteError:
                        pass
            except keyring.errors.KeyringError as error:
                raise ValueError(
                    "Windows could not save the connection securely. Check your credential store."
                ) from error
        with self.repo.atomic():
            for field in ("model", "default_currency", "preferences"):
                value = getattr(patch, field)
                if value is not None:
                    self.repo.set_setting(field, value.strip())
        return self.public()
