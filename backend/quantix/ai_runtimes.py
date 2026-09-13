"""SDK-free original-client facade over the guided component coordinator."""

from .ai_runtime_common import RuntimeUnavailable


class RuntimeService:
    def __init__(self, repo):
        self.repo = repo
        from .ai_components import AIComponentService
        from .ai_worker_client import AIWorkerClient
        self.components = AIComponentService(repo)
        self.worker = AIWorkerClient(repo)

    def _connection(self, connection_id):
        from .ai_connections import AIConnectionService, is_supported_profile
        connection = AIConnectionService(self.repo).get(connection_id)
        if not is_supported_profile(connection):
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        return connection

    async def status(self, connection_id):
        connection = self._connection(connection_id)
        if connection["protocol"] not in {"codex", "grok_build"}:
            raise ValueError("Direct API connections do not use original-client runtime status.")
        state = self.components.status(connection)
        if state["state"] == "ready":
            account = await self.worker.runtime_status(connection)
            state.update({key: account.get(key) for key in ("state", "detail", "login_url", "user_code", "docs_url") if key in account})
        return state

    async def discover_models(self, connection_id):
        connection = self._connection(connection_id)
        if connection["protocol"] not in {"codex", "grok_build"}:
            raise ValueError("Direct API connections use bundled model discovery.")
        return await self.worker.catalog(connection, {})

    async def install(self, connection_id):
        connection = self._connection(connection_id)
        if connection["protocol"] not in {"codex", "grok_build"}:
            raise ValueError("Direct API connections do not need original-client installation.")
        return await self.components.prepare(connection)

    async def login(self, connection_id):
        connection = self._connection(connection_id)
        if connection["protocol"] not in {"codex", "grok_build"}:
            raise ValueError("Direct API connections use an API key instead of original-client sign-in.")
        return await self.worker.login(connection)

    async def logout(self, connection_id):
        connection = self._connection(connection_id)
        if connection["protocol"] not in {"codex", "grok_build"}:
            raise ValueError("Direct API connections use an API key instead of original-client sign-out.")
        return await self.worker.logout(connection)

    async def close(self):
        await self.worker.close()


async def execute_runtime(route, connection, credentials, context, instruction, output_type,
                          *, consult=None, before_request=None, on_response=None):
    from .ai_connections import is_subscription_profile
    if not is_subscription_profile(connection):
        raise RuntimeUnavailable("This AI connection does not use an approved original-client subscription route.")
    from .ai_worker_client import AIWorkerClient
    return await AIWorkerClient(context.repo).execute(
        route, connection, credentials, context, instruction, output_type,
        consult=consult, before_request=before_request, on_response=on_response,
    )
