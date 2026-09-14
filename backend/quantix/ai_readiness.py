"""Read setup evidence without starting a worker or altering an account."""

import json


def _ready_models(repo, connection):
    if not connection["enabled"] or connection["credential_state"] == "missing":
        return {}, None
    from .ai_connections import is_supported_profile

    if not is_supported_profile(connection):
        return {}, None
    with repo.db.connect() as conn:
        if not conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_setup_accounts'"
        ).fetchone():
            return {}, None
        row = conn.execute(
            "SELECT data_json FROM ai_setup_accounts WHERE connection_id=?", (connection["id"],)
        ).fetchone()
        if not row:
            return {}, None
        saved = json.loads(row[0])
        selected = saved.get("selected_model_id") or saved.get("check", {}).get("model_id")
        records = {}
        if saved.get("check", {}).get("model_id"):
            records[saved["check"]["model_id"]] = saved
        if conn.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_setup_model_checks'"
        ).fetchone():
            for model_row in conn.execute(
                "SELECT model_id,data_json FROM ai_setup_model_checks WHERE connection_id=?",
                (connection["id"],),
            ):
                records[model_row["model_id"]] = json.loads(model_row["data_json"])
    records = {
        model_id: value
        for model_id, value in records.items()
        if value.get("check", {}).get("model_id") == model_id
        and value["check"].get("status") == "passed"
        and value.get("checked_revision") == connection["revision"]
    }
    if not records:
        return {}, selected
    if connection["protocol"] in {"codex", "grok_build"}:
        from .ai_components import AIComponentService

        installed = AIComponentService(repo).status(connection)
    else:
        from .ai_direct import direct_runtime_status

        installed = direct_runtime_status(connection)
    if installed.get("state") != "ready":
        return {}, selected
    version = installed["version"]
    return {
        model_id: value
        for model_id, value in records.items()
        if value.get("checked_component_version") == version
    }, selected


def ready_evidence(repo, connection, model_id=None):
    records, selected = _ready_models(repo, connection)
    return records.get(model_id or selected)


def ready_model_ids(repo, connection):
    return set(_ready_models(repo, connection)[0])


def require_ready(repo, connection, model_id):
    from .ai_connections import is_supported_profile

    if not is_supported_profile(connection):
        raise ValueError(
            "This saved AI account is retired. Choose one of the five supported provider routes."
        )
    evidence = ready_evidence(repo, connection, model_id)
    if evidence is None:
        raise ValueError(
            "Finish this AI account's connection check in Settings before starting Tender work."
        )
    return evidence["checked_component_version"]
