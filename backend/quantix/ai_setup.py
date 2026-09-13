"""One guided journey from a chosen AI service to a genuinely checked account."""

import asyncio
import hashlib
import inspect
import json
from decimal import ROUND_UP, Decimal

from .ai_catalog import catalog_price, documented_capabilities
from .ai_connections import (
    AIConnectionService,
    is_direct_profile,
    is_subscription_profile,
    is_supported_profile,
)
from .ai_models import ConnectionInput, ModelInput
from .ai_recommendations import suggest_model
from .ai_runtime_common import RuntimeConnectionFailure, RuntimeUnavailable
from .ai_setup_catalog import (
    default_connection,
    service_for_connection,
    setup_method,
    setup_services,
)
from .ai_setup_models import (
    SetupAccount,
    SetupAction,
    SetupCheckPreview,
    SetupConfigure,
    SetupSoftware,
    SetupStart,
)
from .ai_setup_store import SetupCheckMeter, SetupStore, SubscriptionCheckMeter
from .ai_subscription import included_only_current, unknown_subscription
from .db import dump
from .diagnostics import record, record_exception

GROK_CHECK_MAX_ROUNDS = 5


_CHECK_EMPTY = {"status": "not_checked", "detail": "This account has not been checked yet."}


class AISetupService:
    def __init__(self, repo, *, components=None, worker=None, direct=None):
        from .ai_components import AIComponentService
        from .ai_direct import DirectAPIService
        from .ai_worker_client import AIWorkerClient
        self.repo = repo
        self.connections = AIConnectionService(repo)
        self.store = SetupStore(repo)
        self.components = components or AIComponentService(repo)
        self.worker = worker or AIWorkerClient(repo)
        self.direct = direct or DirectAPIService(repo)
        self.tasks = {}
        self.closing = False
        self.store.recover()

    @staticmethod
    def services():
        return setup_services()

    def _state(self, identifier, **values):
        result = self.store.update(identifier, **values)
        stage = values.get("stage")
        if isinstance(stage, str):
            record("ai_setup_transition", phase=stage, outcome="state", connection_id=identifier)
        return result

    def _busy(self, identifier):
        task = self.tasks.get(identifier)
        return bool(task and not task.done())

    def _software(self, connection):
        if is_direct_profile(connection):
            from .ai_direct import direct_runtime_status
            return SetupSoftware.model_validate(direct_runtime_status(connection)).model_dump(mode="json")
        if is_subscription_profile(connection):
            return SetupSoftware.model_validate(self.components.status(connection)).model_dump(mode="json")
        return {
            "component_id": "unsupported",
            "state": "attention",
            "detail": "This saved AI account is retired. Choose one of the five supported provider routes.",
            "progress": None,
            "version": None,
        }

    def _existing_model(self, identifier):
        choices = set()
        with self.repo.db.connect() as conn:
            if conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='tender_ai_policy'").fetchone():
                for row in conn.execute("SELECT data_json FROM tender_ai_policy"):
                    policy = json.loads(row[0])
                    for route in [policy.get("manager"), policy.get("specialist"), *policy.get("role_routes", {}).values()]:
                        if route and route.get("connection_id") == identifier:
                            choices.add(route["model_id"])
        if len(choices) == 1:
            return next(iter(choices))
        if not choices:
            models = self.connections.models(identifier)
            if len(models) == 1:
                return models[0]["model_id"]
        return None

    def get(self, identifier):
        connection = self.connections.get(identifier)
        service, method = service_for_connection(connection)
        supported = is_supported_profile(connection)
        saved = self.store.get(identifier)
        software = self._software(connection)
        models = self.connections.models(identifier)
        recommendation_id, recommendation = suggest_model(models)
        selected = saved.get("selected_model_id") or self._existing_model(identifier)
        check = dict(saved.get("check") or _CHECK_EMPTY)
        check["reserved_usd"] = self.store.unresolved_cost(identifier)
        stage = saved.get("stage", "needs_preparation")
        detail = saved.get("detail", "Connect this AI account to prepare its software and check access.")
        if is_subscription_profile(connection) and not saved.get("stage") and software["state"] == "ready":
            stage, detail = "needs_sign_in", "The original client is prepared. Refresh the account or sign in through its official flow."
        if not method["available"]:
            stage, detail = "attention", method["unavailable_reason"] or method["detail"]
        if check["status"] == "passed" and (saved.get("checked_revision") != connection["revision"] or saved.get("checked_component_version") != software["version"]):
            check = dict(_CHECK_EMPTY)
            stage, detail = "ready_to_check", "This account changed. Check it again before using the new settings."
        if selected and not any(model["model_id"] == selected for model in models):
            stage, detail = "choose_model", "The previously selected AI is unavailable. Choose another model; nothing has been switched automatically."
        if is_direct_profile(connection) and connection["credential_state"] == "missing" and not self._busy(identifier):
            check = dict(_CHECK_EMPTY) | {"reserved_usd": self.store.unresolved_cost(identifier)}
            stage, detail = "needs_credentials", "This account's access key is no longer available. Enter it again, or repair its access settings in More options."
        if software["state"] != "ready" and not self._busy(identifier):
            # Preparing or repairing the client is what fixes every non-ready
            # software state. Reporting "attention" for a subscription account
            # left the panel offering only sign-in, which cannot succeed while
            # its component is stale, so the same button returned each time.
            stage = (
                "needs_preparation"
                if is_subscription_profile(connection) or software["state"] != "attention"
                else "attention"
            )
            detail = software["detail"]
        if not connection["enabled"]:
            stage, detail = "attention", "This AI account is turned off. Enable it in More options before continuing."
        if not supported:
            stage, detail = "attention", method["unavailable_reason"] or "This saved AI account is retired. Choose one of the five supported provider routes."
            check = dict(_CHECK_EMPTY) | {"reserved_usd": self.store.unresolved_cost(identifier)}
        check["reserved_usd"] = self.store.unresolved_cost(identifier)
        return SetupAccount(id=identifier, service_id=service["id"], service_title=service["title"],
            method_id=method["id"], method_title=method["title"], connection=connection,
            supported=supported, access_kind=method.get("access_kind", "api_key"),
            stage=stage, detail=detail, progress=saved.get("progress"), software=software,
            active=self._busy(identifier),
            models=models, selected_model_id=selected, recommended_model_id=recommendation_id,
            recommendation=recommendation, login_url=saved.get("login_url"), user_code=saved.get("user_code"), check=check,
            subscription_usage=saved.get("subscription_usage") if connection["protocol"] == "grok_build" else None).model_dump(mode="json")

    def list(self):
        return [self.get(connection["id"]) for connection in self.connections.list()]

    async def start(self, values):
        request = SetupStart.model_validate(values)
        service, method = setup_method(request.service_id, request.method_id)
        existing = self.connections.list()
        count = sum(service_for_connection(c)[0]["id"] == service["id"] for c in existing)
        name = request.name or service["title"] + (f" {count + 1}" if count else "")
        connection_values = request.connection or default_connection(method, name)
        connection_values = ConnectionInput.model_validate(connection_values)
        if connection_values.provider_id != method["provider_id"]:
            raise ValueError("The supplied account details do not match the AI service you chose.")
        connection_values.name = name
        if method["requires_details"] and request.connection is None:
            raise ValueError("Enter the service details in More options. Your provider or company administrator supplies them.")
        connection = self.connections.create(connection_values)
        if is_direct_profile(connection):
            missing = connection["credential_state"] == "missing"
            self._state(
                connection["id"],
                stage="needs_credentials" if missing else "discovering",
                detail="Enter the access key from your AI account to continue." if missing else "Finding the AI models available to your account.",
                check=dict(_CHECK_EMPTY),
            )
            if not missing:
                self._launch(connection["id"], "refresh")
        elif is_subscription_profile(connection):
            software = self._software(connection)
            self._state(
                connection["id"],
                stage="needs_sign_in" if software["state"] == "ready" else "needs_preparation",
                detail="Sign in through the official client to continue." if software["state"] == "ready" else software["detail"],
                check=dict(_CHECK_EMPTY),
            )
        else:
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        return self.get(connection["id"])

    async def configure(self, identifier, values):
        request = SetupConfigure.model_validate(values)
        if self._busy(identifier):
            raise ValueError("Wait for setup to finish, or cancel it before changing this account.")
        connection_before = self.connections.get(identifier)
        if not is_supported_profile(connection_before):
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        connection = self.connections.update(identifier, request.connection)
        self._state(identifier, check=dict(_CHECK_EMPTY), checked_revision=None, login_url=None, user_code=None, subscription_usage=None)
        if is_direct_profile(connection):
            if connection["credential_state"] == "missing":
                self._state(identifier, stage="needs_credentials", active=False, detail="Enter the access key from your AI account to continue.")
            else:
                self._launch(identifier, "refresh")
        elif is_subscription_profile(connection):
            software = self._software(connection)
            self._state(identifier, stage="needs_sign_in" if software["state"] == "ready" else "needs_preparation", active=False,
                        detail="Sign in through the official client to continue." if software["state"] == "ready" else software["detail"])
        else:
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        return self.get(identifier)

    def _launch(self, identifier, action, **values):
        if self.closing:
            raise ValueError("Quantix is closing. Reopen it before continuing setup.")
        if self._busy(identifier):
            raise ValueError("This account is already being prepared. Wait or cancel its current action.")
        if action == "check":
            with self.connections.lease(identifier, exclusive=True), self.connections.authority_guard(), self.repo.atomic():
                self.store.invalidate_model_checks(identifier, values["preview"]["model_id"])
        stage = "checking" if action == "check" else "preparing" if action in {"prepare", "repair"} else "discovering"
        self._state(identifier, stage=stage, active=True, detail="Checking the connection…" if action == "check" else "Finding the AI models available to your account.", progress=None)
        task = asyncio.create_task(self._perform(identifier, action, **values), name=f"quantix-ai-setup-{identifier}")
        self.tasks[identifier] = task
        task.add_done_callback(lambda finished: self.tasks.pop(identifier, None) if self.tasks.get(identifier) is finished else None)

    async def action(self, identifier, values):
        request = SetupAction.model_validate(values)
        connection = self.connections.get(identifier)
        direct = is_direct_profile(connection)
        subscription = is_subscription_profile(connection)
        if not is_supported_profile(connection) and request.action not in {"cancel", "rename"}:
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        if request.sign_in_method is not None and not subscription:
            raise ValueError("Direct API connections use an API key instead of original-client sign-in.")
        if direct and request.action in {"sign_in", "sign_out", "remove_software"}:
            raise ValueError("This API connection uses bundled software. Enter its key or refresh its models in Settings.")
        if request.action == "set_subscription_extras":
            if not subscription or connection["protocol"] != "grok_build":
                raise ValueError("Provider-managed extras are available only for the Grok subscription connection.")
            if request.allow_provider_managed_extras is None:
                raise ValueError("Choose whether this Grok account may use provider-managed extras.")
            if self._busy(identifier):
                raise ValueError("Wait for setup to finish before changing this spending preference.")
            with self.connections.authority_guard(), self.repo.atomic():
                current = self.connections.get(identifier)
                old_revision = current["revision"]
                updated = current | {"settings": {**current.get("settings", {}), "allow_provider_managed_extras": request.allow_provider_managed_extras}}
                for field in ("credential_state", "id", "revision", "created_at", "updated_at", "status", "last_error", "credentials", "session_only", "_model", "_checked_component_version"):
                    updated.pop(field, None)
                updated = self.connections.update(identifier, updated)
                self.store.carry_spending_preference_checks(identifier, old_revision, updated["revision"])
            self._state(identifier, detail="Grok spending preference saved. Review the account's Tender approval before using it.")
            return self.get(identifier)
        if request.action == "refresh_usage" and (not subscription or connection["protocol"] != "grok_build"):
            raise ValueError("Usage refresh is available only for the Grok subscription connection.")
        if subscription and request.action == "refresh_usage":
            if self._busy(identifier):
                raise ValueError("Wait for the current account action to finish before refreshing usage.")
            snapshot = await self._refresh_subscription_usage(identifier, force=True)
            detail = (snapshot or {}).get("detail") or "Grok usage was refreshed. Review the current allowance, then check the selected model."
            if included_only_current(snapshot):
                detail = "Grok usage was refreshed. Review the current allowance, then check the selected model."
            self._state(identifier, active=False, detail=detail)
            return self.get(identifier)
        if request.action == "cancel":
            task = self.tasks.get(identifier)
            if task and not task.done():
                task.cancel()
                await asyncio.gather(task, return_exceptions=True)
            if subscription and self.worker is not None:
                await self.worker.cancel_account(identifier)
            self._state(identifier, stage="cancelled", active=False, detail="Setup stopped. Your account and Tender records are preserved.", login_url=None, user_code=None)
        elif request.action in {"prepare", "repair", "refresh", "sign_in", "sign_out", "remove_software"}:
            if direct and connection["credential_state"] == "missing":
                self._state(identifier, stage="needs_credentials", active=False, detail="Enter the access key from your AI account to continue.")
            else:
                self._launch(identifier, "refresh" if direct else request.action, sign_in_method=request.sign_in_method)
        elif request.action == "check":
            preview = await self.check_preview_async(identifier) if subscription else self.check_preview(identifier)
            if not preview["allowed"]:
                raise ValueError(preview["detail"])
            if request.check_fingerprint != preview["fingerprint"]:
                raise ValueError("The account or check cost changed. Review the displayed check again.")
            if connection["billing"] in {"metered", "unknown"} and request.maximum_cost_usd != preview["maximum_cost_usd"]:
                raise ValueError("Review and accept the displayed cost allowance before starting this check.")
            if preview["requires_unknown_cost_consent"] and not request.accept_unknown_cost:
                raise ValueError("Review the unknown-cost notice and explicitly accept this short connection check.")
            self._launch(identifier, "check", preview=preview, accept_unknown_cost=request.accept_unknown_cost)
        elif request.action == "select_model":
            if self._busy(identifier):
                raise ValueError("Wait or cancel the current setup action before choosing another AI.")
            if not any(m["model_id"] == request.model_id for m in self.connections.models(identifier)):
                raise ValueError("Choose a model available to this account.")
            evidence = self.store.model_check(identifier, request.model_id)
            current_software = self._software(connection)
            valid = evidence.get("check", {}).get("status") == "passed" and evidence.get("checked_revision") == connection["revision"] and evidence.get("checked_component_version") == current_software["version"] and current_software["state"] == "ready" and connection["credential_state"] != "missing"
            self._state(identifier, selected_model_id=request.model_id, stage="ready" if valid else "ready_to_check",
                        check=evidence["check"] if valid else dict(_CHECK_EMPTY),
                        checked_revision=evidence.get("checked_revision") if valid else None,
                        checked_component_version=evidence.get("checked_component_version") if valid else None,
                        detail="Your checked model is selected." if valid else "Your choice is saved. Check this model before using it.")
        elif request.action == "rename":
            if not request.name:
                raise ValueError("Enter a name for this account.")
            self.connections.rename(identifier, request.name)
        return self.get(identifier)

    async def _perform(self, identifier, action, **values):
        try:
            connection = self.connections.get(identifier)
            if not is_supported_profile(connection):
                _, method = service_for_connection(connection)
                if not method["available"]:
                    self._state(identifier, stage="attention", detail=method["unavailable_reason"] or method["detail"], active=False)
                    return
            if action == "check":
                await self._check(identifier, values["preview"], accept_unknown_cost=values.get("accept_unknown_cost", False))
                return
            if is_direct_profile(connection):
                if connection["credential_state"] == "missing":
                    self._state(identifier, stage="needs_credentials", detail="Enter the access key from your AI account to continue.", active=False)
                    return
                await self._discover(identifier)
                return
            await self._perform_subscription(identifier, action, **values)
        except asyncio.CancelledError:
            self._state(identifier, stage="cancelled", detail="Setup was stopped. Choose Retry when you are ready.", login_url=None, user_code=None)
            raise
        except Exception as error:
            # Provider exception details can include credentials. Only expected
            # application ValueErrors are displayed; worker errors are sanitized.
            reference = record_exception("ai_setup_failed", error, phase=action, connection_id=identifier)
            known = isinstance(error, (RuntimeUnavailable, RuntimeConnectionFailure, ValueError)) or type(error).__name__ in {"DirectProviderError", "DirectProviderUnavailable"}
            message = str(error) if known else "This AI could not finish setup. Check your account details and choose Retry."
            message = f"{message[:1080]} (Reference: {reference})"[:1200]
            if action == "check":
                check = self.store.get(identifier).get("check", {})
                if check.get("status") in {"checking", "failed", "interrupted"}:
                    self._state(identifier, check=check | {
                        "status": "failed" if check.get("status") == "checking" else check.get("status"),
                        "detail": message,
                        "reserved_usd": self.store.unresolved_cost(identifier),
                    })
            self._state(identifier, stage="attention", detail=message, login_url=None, user_code=None)
        finally:
            if is_subscription_profile(self.connections.get(identifier)) and self.worker is not None:
                saved = self.store.get(identifier)
                if action != "sign_in" or saved.get("stage") != "needs_sign_in":
                    await self.worker.cancel_account(identifier)
            self._state(identifier, active=False, progress=None)

    async def _perform_subscription(self, identifier, action, *, sign_in_method=None, **_values):
        connection = self.connections.get(identifier)
        if not is_subscription_profile(connection):
            raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
        if action in {"prepare", "repair"}:
            if self.worker is not None:
                await self.worker.cancel_account(identifier)
            with self.connections.lease(identifier, exclusive=True) as current:
                result = await self.components.prepare(current, repair=action == "repair",
                                                        progress=lambda value: self._state(identifier, progress=value.get("progress"), detail=value.get("detail", "Preparing selected AI software.")))
            with self.connections.lease(identifier, exclusive=True) as current:
                status = await self.worker.runtime_status(current)
            if status.get("state") == "signed_in":
                self._state(identifier, stage="discovering", detail="The original client confirmed its saved sign-in. Finding available models.", progress=result.get("progress"))
                await self._discover(identifier)
            elif status.get("state") == "sign_in_required":
                self.store.invalidate_model_checks(identifier)
                self._state(identifier, stage="needs_sign_in", detail=status.get("detail", "The original client is ready. Sign in through its official account flow."), progress=result.get("progress"), login_url=status.get("login_url"), user_code=status.get("user_code"))
            else:
                self._state(identifier, stage="attention", detail=status.get("detail", "The original client is prepared but could not confirm its saved sign-in."), progress=result.get("progress"))
            return
        if action == "remove_software":
            if self.worker is not None:
                await self.worker.cancel_account(identifier)
            with self.connections.lease(identifier, exclusive=True):
                result = self.components.remove(connection)
            self.store.invalidate_model_checks(identifier)
            self._state(identifier, stage="needs_preparation", detail=result.get("detail", "Prepare the original client before continuing."), selected_model_id=None, login_url=None, user_code=None)
            return
        if action == "sign_in":
            terminal = None
            with self.connections.lease(identifier, exclusive=True):
                # A deliberate account action revokes the previous route proof
                # before the original client opens its browser flow.
                self.connections.account_access_changed(identifier)
                current = self.connections.get(identifier)
                result = await self.worker.login(current, sign_in_method=sign_in_method)
                state = result.get("state") if isinstance(result, dict) else None
                if state == "login_pending":
                    self._state(identifier, stage="needs_sign_in", active=True, detail=result.get("detail", "Complete the official sign-in to continue."), login_url=result.get("login_url"), user_code=result.get("user_code"))
                    while True:
                        await asyncio.sleep(0.5)
                        terminal = await self.worker.runtime_status(current)
                        if terminal.get("state") == "login_pending":
                            self._state(identifier, stage="needs_sign_in", active=True,
                                        detail=terminal.get("detail", "Complete the official sign-in to continue."),
                                        login_url=terminal.get("login_url"), user_code=terminal.get("user_code"))
                            continue
                        if terminal.get("state") != "login_pending":
                            break
                    result = terminal
            self.store.invalidate_model_checks(identifier)
            state = result.get("state") if isinstance(result, dict) else None
            if state == "signed_in":
                self._state(identifier, stage="discovering", detail="The original client confirmed sign-in. Finding the models available to this account.", login_url=None, user_code=None)
                await self._discover(identifier)
            else:
                self._state(identifier, stage="attention", detail=result.get("detail", "The original client did not confirm sign-in."), login_url=result.get("login_url"), user_code=result.get("user_code"))
            return
        if action == "sign_out":
            with self.connections.lease(identifier, exclusive=True):
                self.connections.account_access_changed(identifier)
                current = self.connections.get(identifier)
                result = await self.worker.logout(current)
            await self.worker.cancel_account(identifier)
            self.store.invalidate_model_checks(identifier)
            state = result.get("state") if isinstance(result, dict) else None
            if state == "signed_out":
                self._state(identifier, stage="needs_sign_in", detail=result.get("detail", "The original client signed out. Sign in again before checking access."), login_url=None, user_code=None)
            else:
                self._state(identifier, stage="attention", detail=result.get("detail", "The original client did not confirm sign-out. Review the original account controls and retry."), login_url=None, user_code=None)
            return
        if action == "refresh":
            with self.connections.lease(identifier, exclusive=True) as current:
                status = await self.worker.runtime_status(current)
            state = status.get("state") if isinstance(status, dict) else None
            if state == "signed_in":
                self._state(identifier, stage="discovering", detail="The original client confirmed its saved sign-in. Finding available models.")
                await self._discover(identifier)
            elif state == "sign_in_required":
                self.store.invalidate_model_checks(identifier)
                self._state(identifier, stage="needs_sign_in", detail=status.get("detail", "Sign in through the official client to continue."), login_url=status.get("login_url"), user_code=status.get("user_code"))
            else:
                self._state(identifier, stage="attention", detail=status.get("detail", "The original client could not confirm its saved sign-in."), login_url=status.get("login_url"), user_code=status.get("user_code"))
            return
        raise ValueError("This subscription setup action is unavailable. Retry the account step.")

    async def _discover(self, identifier):
        previous = self.store.get(identifier).get("selected_model_id") or self._existing_model(identifier)
        self._state(identifier, stage="discovering", detail="Finding the AI models available to your account…", login_url=None, user_code=None)
        with self.connections.lease(identifier, exclusive=True) as connection:
            credentials = self.connections.credentials(identifier)
            if is_direct_profile(connection):
                values = await self.direct.catalog(connection, credentials)
            elif is_subscription_profile(connection):
                values = await self.worker.catalog(connection, credentials)
            else:
                raise ValueError("This saved AI account is retired. Choose one of the five supported provider routes.")
            revision = connection["revision"]
        enriched = []
        software = self._software(connection)
        for value in values:
            item = ModelInput.model_validate(value).model_dump(mode="json")
            documented = documented_capabilities(connection["provider_id"], connection["protocol"], item["model_id"])
            # Static documentation may fill missing catalog details, but must
            # not override a provider's explicit capability denial.
            item["capabilities"].update({key: value for key, value in documented.items()
                                         if item["capabilities"].get(key) is not False})
            proof = self.store.model_check(identifier, item["model_id"])
            if proof.get("check", {}).get("status") == "passed" and proof.get("checked_revision") == revision and proof.get("checked_component_version") == software["version"]:
                for field in ("tools", "structured_output"):
                    if item["capabilities"].get(field) is None:
                        item["capabilities"][field] = True
            if item.get("pricing") is None and is_direct_profile(connection):
                item["pricing"] = catalog_price(connection["provider_id"], item["model_id"])
            enriched.append(item)
        models = self.connections.store_discovered_models(identifier, enriched, revision)
        recommended, _ = suggest_model(models)
        selected = previous or recommended
        exists = any(m["model_id"] == selected for m in models)
        proof = self.store.model_check(identifier, selected) if selected else {}
        checked = exists and proof.get("check", {}).get("status") == "passed" and proof.get("checked_revision") == self.connections.get(identifier)["revision"] and proof.get("checked_component_version") == software["version"] and software["state"] == "ready"
        if checked:
            self._state(identifier, selected_model_id=selected, stage="ready", check=proof["check"], checked_revision=proof["checked_revision"],
                        checked_component_version=proof["checked_component_version"], detail="Account access and available models refreshed. Your checked model is still ready.")
            return
        self._state(identifier, selected_model_id=selected, stage="ready_to_check" if exists else "choose_model",
                    check=dict(_CHECK_EMPTY), checked_revision=None,
                    detail="A model is selected. Review it and check the connection." if exists else "Choose an available model. Quantix has not changed your previous choice.")

    def _subscription_snapshot_for(self, identifier):
        value = self.store.get(identifier).get("subscription_usage")
        return value if isinstance(value, dict) else None

    async def _refresh_subscription_usage(self, identifier, *, force=False, guarded=False):
        connection = self.connections.get(identifier)
        if not is_subscription_profile(connection) or connection["protocol"] != "grok_build":
            return self._subscription_snapshot_for(identifier)
        # A pending browser login owns the worker account task. Do not make a
        # GET or preview compete with that interactive operation.
        if self._busy(identifier) and not force:
            return self._subscription_snapshot_for(identifier)
        if guarded:
            snapshot = await self.worker.subscription_usage(connection) if self.worker is not None else unknown_subscription()
        else:
            with self.connections.lease(identifier, exclusive=True) as current:
                snapshot = await self.worker.subscription_usage(current) if self.worker is not None else unknown_subscription()
        return snapshot

    @staticmethod
    def _subscription_usage_authority(snapshot):
        if not isinstance(snapshot, dict):
            return None
        # fetched_at records freshness but does not grant a new route proof.
        return {key: value for key, value in snapshot.items() if key != "fetched_at"}

    def _preview(self, identifier, *, usage=None):
        account = self.get(identifier)
        connection = account["connection"]
        model = next((m for m in account["models"] if m["model_id"] == account["selected_model_id"]), None)
        subscription = is_subscription_profile(connection)
        allowed = account["supported"] and account["stage"] in {"ready_to_check", "ready", "attention", "cancelled"} and account["software"]["state"] == "ready" and connection["enabled"] and model is not None and connection["credential_state"] != "missing" and not self._busy(identifier)
        detail = "This short check sends a generic instruction, never Tender documents."
        cost = None
        source = None
        requires_unknown = False
        requests = 2
        max_output = 1024
        max_input = 16384
        limit_description = None
        if subscription:
            from .ai_worker_client import CHECK_DEADLINE_SECONDS
            requests = GROK_CHECK_MAX_ROUNDS if connection["protocol"] == "grok_build" else None
            max_output = max_input = None
            limit_description = (
                f"Grok Build allows up to five worker rounds within the {CHECK_DEADLINE_SECONDS}-second original-client check deadline; provider subscription usage remains account-managed."
                if connection["protocol"] == "grok_build"
                else f"Codex uses one original-client turn within the {CHECK_DEADLINE_SECONDS}-second check deadline. Codex does not expose its internal inference or token limits."
            )
            if connection["protocol"] == "grok_build" and not included_only_current(usage):
                allowed = False
                detail += " Grok's current included allowance could not be confirmed. Choose Refresh usage and review the official usage settings."
        elif not allowed:
            detail = "Finish preparing and connecting the account, then select a model."
        elif connection["billing"] in {"metered", "unknown"}:
            prices = model.get("pricing")
            if not prices:
                requires_unknown = True
                detail += " The model price is unknown. Review and accept this bounded test before it sends any request."
            else:
                amount = (Decimal(16384) * Decimal(str(prices["input_per_million"])) + Decimal(1024) * Decimal(str(prices["output_per_million"]))) * requests / Decimal(1000000)
                cost = float(amount.quantize(Decimal("0.000001"), rounding=ROUND_UP))
                source = prices["source"]
                detail += " The displayed allowance is a conservative estimate; the provider reports actual usage."
        if subscription and not allowed and account["stage"] not in {"ready_to_check", "ready", "attention", "cancelled"}:
            detail = "Finish preparing and signing in to the original client, then select a model."
        body = {"connection_revision": connection["revision"], "model": model, "maximum_cost_usd": cost,
                "component_version": account["software"]["version"], "max_requests": requests,
                "max_output_tokens": max_output, "max_input_tokens": max_input,
                "requires_unknown_cost_consent": requires_unknown,
                "subscription_check": subscription,
                "subscription_usage": self._subscription_usage_authority(usage) if subscription and connection["protocol"] == "grok_build" else None}
        return SetupCheckPreview(allowed=allowed, detail=detail, model_id=account["selected_model_id"], maximum_cost_usd=cost,
            pricing_source=source, max_requests=requests, max_output_tokens=max_output,
            requires_unknown_cost_consent=requires_unknown, max_input_tokens=max_input,
            limit_description=limit_description, subscription_check=subscription,
            fingerprint=hashlib.sha256(dump(body).encode()).hexdigest()).model_dump()

    def check_preview(self, identifier):
        connection = self.connections.get(identifier)
        usage = self._subscription_snapshot_for(identifier) if is_subscription_profile(connection) else None
        return self._preview(identifier, usage=usage)

    async def check_preview_async(self, identifier):
        connection = self.connections.get(identifier)
        usage = self._subscription_snapshot_for(identifier) if is_subscription_profile(connection) else None
        if is_subscription_profile(connection) and connection["protocol"] == "grok_build" and not self._busy(identifier):
            usage = await self._refresh_subscription_usage(identifier)
        return self._preview(identifier, usage=usage)

    async def _check(self, identifier, preview, *, accept_unknown_cost=False):
        meter = None
        with self.connections.lease(identifier, exclusive=True) as connection:
            subscription = is_subscription_profile(connection)
            usage = self._subscription_snapshot_for(identifier)
            if subscription and connection["protocol"] == "grok_build":
                usage = await self._refresh_subscription_usage(identifier, force=True, guarded=True)
                # The fresh preflight is part of dispatch authority. Its
                # fetched_at is intentionally excluded from the fingerprint.
                if not included_only_current(usage):
                    raise ValueError("Grok's current included allowance could not be confirmed. Refresh usage in the official settings, then try the check again.")
            models = self.connections.models(identifier)
            model = next(m for m in models if m["model_id"] == preview["model_id"])
            body = {"connection_revision": connection["revision"], "model": model, "maximum_cost_usd": preview["maximum_cost_usd"], "component_version": self._software(connection)["version"], "max_requests": preview["max_requests"], "max_output_tokens": preview["max_output_tokens"], "max_input_tokens": preview["max_input_tokens"], "requires_unknown_cost_consent": preview["requires_unknown_cost_consent"], "subscription_check": subscription, "subscription_usage": self._subscription_usage_authority(usage) if subscription and connection["protocol"] == "grok_build" else None}
            if hashlib.sha256(dump(body).encode()).hexdigest() != preview["fingerprint"]:
                raise ValueError("The account changed before its check started. Review the check again.")
            route = {"connection_id": identifier, "model_id": model["model_id"], "reasoning": None,
                     "max_output_tokens": 1024, "web_search": False, "max_search_calls": 0}
            worker_requests = GROK_CHECK_MAX_ROUNDS if connection["protocol"] == "grok_build" else max(1, int(connection.get("settings", {}).get("max_turns", 12)))
            selected = connection | {"_model": model, "_checked_component_version": body["component_version"], "_subscription_check": subscription,
                                      "_execution_limits": {"max_requests": worker_requests, "max_output_tokens": 1024, "context_window": model["capabilities"].get("context_window")}}
            if subscription:
                meter = SubscriptionCheckMeter(self.repo, connection, model,
                                               max_requests=GROK_CHECK_MAX_ROUNDS if connection["protocol"] == "grok_build" else None)
            else:
                meter = SetupCheckMeter(
                    self.repo,
                    connection,
                    model,
                    preview["maximum_cost_usd"],
                    requests=preview["max_requests"],
                    input_allowance=preview["max_input_tokens"],
                    unknown_cost_accepted=accept_unknown_cost and preview["requires_unknown_cost_consent"],
                )
            self._state(identifier, check={"status": "checking", "detail": "Checking the selected AI with a generic instruction."})
            try:
                credentials = self.connections.credentials(identifier)
                checker = self.worker.check if subscription else self.direct.check
                result = await checker(route, selected, credentials,
                                       before_request=meter.before_request,
                                       on_response=meter.on_response)
                if not result.get("tools_supported"):
                    raise ValueError("The AI did not call the connection-check tool exactly once. Retry the access check or choose another model.")
                if not result.get("output_supported"):
                    raise ValueError("The AI did not return the unchanged connection-check value in structured output. Retry the access check or choose another model.")
            except BaseException as error:
                self._state(identifier, check=meter.summary("interrupted" if isinstance(error, asyncio.CancelledError) else "failed", "This check did not finish successfully. Any uncertain charge remains recorded."))
                raise
            checked_revision = connection["revision"]
        # Persist observed tool/schema support without fabricating vision or web
        # capabilities. Model updates are allowed only after the execution lease.
        with self.connections.authority_guard(), self.repo.atomic():
            current = self.connections.get(identifier)
            if current["revision"] != checked_revision or self.store.get(identifier).get("selected_model_id") != model["model_id"]:
                raise ValueError("The account changed after its check. The previous result cannot approve these new settings; check it again.")
            values = ModelInput.model_validate({k: model[k] for k in ModelInput.model_fields})
            original_revision = checked_revision
            values.capabilities.tools = True
            values.capabilities.structured_output = True
            if model["capabilities"].get("tools") is not True or model["capabilities"].get("structured_output") is not True:
                # A successful check establishes observed capabilities for
                # this already checked account/model. It is not a credential,
                # endpoint, billing or catalog authority edit, so it must not
                # advance the connection revision and invalidate its proof.
                self.connections.observe_model_capabilities(
                    identifier,
                    model["model_id"],
                    values.capabilities,
                    expected_revision=checked_revision,
                )
            evidence = {"checked_revision": checked_revision, "checked_component_version": body["component_version"],
                        "check": meter.summary("passed", "The selected AI responded and completed its connection check.")}
            self.store.save_model_check(identifier, model["model_id"], evidence, previous_revision=original_revision)
            self._state(identifier, stage="ready", detail="This AI account is ready. Choose it when setting up a Tender.", **evidence)

    async def close(self):
        self.closing = True
        pending = list(self.tasks.values())
        for task in pending:
            task.cancel()
        if pending:
            await asyncio.gather(*pending, return_exceptions=True)
        if self.worker is not None:
            await self.worker.close()
        closed = self.direct.close()
        if inspect.isawaitable(closed):
            await closed
