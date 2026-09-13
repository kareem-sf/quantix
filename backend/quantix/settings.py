"""Office preferences; AI credentials belong to named connection profiles."""

from .models import Settings


class SettingsService:
    def __init__(self, repo):
        self.repo = repo

    def public(self):
        from .ai_connections import AIConnectionService
        from .ai_readiness import ready_model_ids
        connections = [c for c in AIConnectionService(self.repo).list() if c["enabled"]]
        present = any(ready_model_ids(self.repo, c) for c in connections)
        return Settings(provider_ready=present, model="Choose an AI for each Tender",
                        default_currency=self.repo.setting("default_currency", "EGP"),
                        home=str(self.repo.home), preferences=self.repo.setting("preferences", ""),
                        provider_detail="Connect an AI account in Settings, then choose it for your Tender.")

    def update(self, patch):
        with self.repo.atomic():
            for field in ("default_currency", "preferences"):
                value = getattr(patch, field)
                if value is not None:
                    self.repo.set_setting(field, value.strip())
        return self.public()
