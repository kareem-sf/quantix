"""Pinned Grok Build process, configuration and account ownership boundaries.

Source contract: xai-org/grok-build 72a61251, config reference and headless guide.
No OAuth credentials are read, copied, exported or supplied to another client.
"""

import asyncio
import json
import os
import re
import sys
from contextlib import asynccontextmanager
from pathlib import Path
from uuid import uuid4

from .common import RuntimeUnavailable, child_environment
from .runtime_setup import component_root

GROK_VERSION = "1.0.13"
# Mirrored by the core setup preview, host client and worker validation.
GROK_CHECK_MAX_ROUNDS = 5
GROK_DOCS = "https://docs.x.ai/build/overview"
MODEL_PATTERN = r"[A-Za-z0-9][A-Za-z0-9._:/-]{0,299}"


def grok_command(connection):
    if connection.get("settings", {}).get("executable_path"):
        raise RuntimeUnavailable("Use the Grok Build component installed by Quantix for this connection.")
    root = component_root().resolve()
    expected = root / "client" / "bin" / ("grok.exe" if os.name == "nt" else "grok")
    configured = os.environ.get("QUANTIX_GROK_BINARY")
    if (os.environ.get("QUANTIX_GROK_VERSION") != GROK_VERSION or not configured
            or not Path(configured).is_absolute() or Path(configured).resolve() != expected
            or not expected.is_file() or expected.is_symlink()):
        raise RuntimeUnavailable("Prepare the pinned Grok Build component before connecting this account.")
    manifest = root / "client" / "package.json"
    try:
        metadata = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        raise RuntimeUnavailable("The installed Grok Build publisher manifest is unavailable. Prepare its component again.") from None
    if metadata.get("version") != GROK_VERSION or not str(metadata.get("name", "")).startswith("@xai-official/grok-"):
        raise RuntimeUnavailable("The Grok Build package does not match the version supported by this adapter.")
    return [str(expected)]


def grok_home(home):
    return home / "grok"


def _require_owned_configuration(home):
    # System requirements can override local rules. Do not defeat administrator
    # authority or claim MCP-only operation underneath unknown policy.
    paths = [Path("/etc/grok/requirements.toml"), Path("/etc/grok/managed_config.toml")]
    if sys.platform == "darwin":
        paths.extend([Path("/Library/Managed Preferences/ai.x.grok.plist"),
                      Path("/Library/Preferences/ai.x.grok.plist")])
    if os.name != "nt" and any(path.exists() for path in paths):
        raise RuntimeUnavailable("Grok Build has centrally managed configuration. An administrator must provide an approved Tender-tool integration.")
    root = grok_home(home)
    for path in (home, root):
        if path.is_symlink():
            raise RuntimeUnavailable("The Grok account directory must be a private local directory.")
    # These locations are never populated by Quantix. Unexpected local hooks or
    # plugins must not acquire authority through this signed-in connection.
    for relative in ("requirements.toml", "managed_config.toml",
                     "AGENTS.md", "Agents.md", "AGENT.md", "CLAUDE.md", "Claude.md"):
        path = root / relative
        if path.exists() or path.is_symlink():
            raise RuntimeUnavailable("This Grok account has additional local configuration. Use a separate Quantix connection for Tender work.")
    for name in ("hooks", "plugins", "skills", "agents"):
        path = root / name
        if path.is_symlink() or (path.exists() and (not path.is_dir() or any(path.iterdir()))):
            raise RuntimeUnavailable("This Grok account has additional hooks or extensions. Use a separate Quantix connection.")
    hook_paths = root / "hooks-paths"
    if hook_paths.is_symlink() or (hook_paths.exists() and (
            not hook_paths.is_file() or hook_paths.stat().st_size > 1024
            or hook_paths.read_text(encoding="utf-8").strip())):
        raise RuntimeUnavailable("This Grok account has additional hook locations. Use a separate Quantix connection.")


@asynccontextmanager
async def account_lock(home, *, on_wait=None, before_release=None, timeout=1800):
    """Cancellable cross-worker serialization without stale PID lock files."""
    home.mkdir(parents=True, exist_ok=True)
    path = home / "grok-operation.lock"
    if path.is_symlink():
        raise RuntimeUnavailable("The private Grok account lock is invalid.")
    stream = path.open("a+b")
    acquired = False
    announced = False
    deadline = asyncio.get_running_loop().time() + timeout
    try:
        if os.name == "nt":
            import msvcrt
            stream.seek(0, 2)
            if stream.tell() == 0:
                stream.write(b"\0")
                stream.flush()
        while not acquired:
            try:
                stream.seek(0)
                if os.name == "nt":
                    msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
                else:
                    import fcntl
                    fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                acquired = True
            except (OSError, BlockingIOError):
                if not announced and on_wait:
                    await on_wait()
                announced = True
                if asyncio.get_running_loop().time() >= deadline:
                    raise RuntimeUnavailable("This Grok account is still busy. Retry after its current work or sign-in finishes.") from None
                await asyncio.sleep(0.2)
        _require_owned_configuration(home)
        yield
    finally:
        try:
            if acquired and before_release:
                await before_release()
        finally:
            if acquired:
                stream.seek(0)
                if os.name == "nt":
                    msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
                else:
                    fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
            stream.close()


def write_private(path, text):
    if path.is_symlink():
        raise RuntimeUnavailable("The private Grok configuration path is invalid.")
    temporary = path.with_name(path.name + "." + uuid4().hex + ".tmp")
    try:
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as stream:
            stream.write(text)
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def configure(home, *, model=None, output_limit=1024, bridge=None):
    """Caller holds account_lock until every client using this config closes."""
    _require_owned_configuration(home)
    root = grok_home(home)
    root.mkdir(parents=True, exist_ok=True)
    if model is not None and (not isinstance(model, str) or not re.fullmatch(MODEL_PATTERN, model)):
        raise RuntimeUnavailable("Choose an exact model from this Grok account's model catalog.")
    features = ["active_agent_messages", "ask_user_question", "auto_wake", "backend_tools", "campaigns",
                "codebase_indexing", "feedback", "feedback_trace_card", "image_gen", "lsp_tools",
                "managed_config", "mcp_auto_restart", "mcp_liveness_watchers", "mcp_push_server_status",
                "mcp_recursive_config_watch", "non_git_warning", "remember_mode", "repo_status_in_system_prompt",
                "session_recap", "session_search", "subagent_worktree_snapshot", "telemetry", "title_refresh",
                "turn_summary", "two_pass_compaction", "video_gen", "voice_mode", "web_fetch", "write_file"]
    lines = ["# Owned by Quantix. OAuth credentials remain in the official client's separate auth store.",
             "[cli]", "auto_update = false", "use_leader = false", "[auth]", 'preferred_method = "oidc"',
             "[grok_com_config]", "disable_api_key_auth = true", "[features]", *[f"{name} = false" for name in features],
             "[relay]", "enabled = false", "[memory]", "enabled = false", "[subagents]", "enabled = false",
             "[workflows]", "enabled = false", "[managed_mcps]", "enabled = false", "gateway_tools_enabled = false",
             "[telemetry]", "trace_upload = false", "otel_enabled = false", "[ui]", "prompt_suggestions = false",
             "[session]", "load_envrc = false", "auto_compact_threshold_percent = 100",
             "[toolset.bash]", "login_shell_capture = false", "[plugins]", "paths = []", "[skills]", "paths = []",
             "[permission]", 'deny = ["Bash", "Read(**)", "Write", "Edit", "Grep", "WebFetch"]' if bridge else 'deny = ["Bash", "Read", "Write", "Edit", "Grep", "WebFetch"]',
             'allow = ["MCPTool(quantix__*)"]' if bridge else 'deny = ["MCPTool"]']
    # Avoid duplicate TOML keys in account-only configuration.
    if not bridge:
        lines[-2:] = ['deny = ["Bash", "Read", "Write", "Edit", "Grep", "WebFetch", "MCPTool"]']
    for vendor in ("claude", "cursor", "codex"):
        lines.extend([f"[compat.{vendor}]", *[f"{cell} = false" for cell in ("skills", "rules", "agents", "mcps", "hooks", "sessions")]])
    if model:
        # Per-model overrides beat catalog defaults. Do not override routing,
        # credentials, endpoint, context size or model identity.
        lines.extend([f"[model.{json.dumps(model)}]", f"max_completion_tokens = {output_limit}", "max_retries = 0"])
    if bridge:
        lines.extend(["[mcp_servers.quantix]", f"url = {json.dumps(bridge.url)}",
                      'bearer_token_env_var = "QUANTIX_MCP_TOKEN"', "enabled = true", "startup_timeout_sec = 30",
                      "tool_timeout_sec = 1800"])
    write_private(root / "config.toml", "\n".join(lines) + "\n")


def environment(home, *, allow_browser=False, bridge=None):
    env = {key: value for key, value in child_environment(home).items() if value}
    env.update({"GROK_HOME": str(grok_home(home)), "GROK_DISABLE_AUTOUPDATER": "1", "GROK_DISABLE_API_KEY_AUTH": "1",
                "GROK_SUBAGENTS": "0", "GROK_MEMORY": "0", "GROK_WORKFLOWS": "0", "GROK_CAMPAIGNS": "0",
                "GROK_MANAGED_MCPS_ENABLED": "0", "GROK_MANAGED_MCP_GATEWAY_TOOLS_ENABLED": "0",
                "GROK_TELEMETRY_ENABLED": "0", "GROK_TELEMETRY_TRACE_UPLOAD": "0", "GROK_EXTERNAL_OTEL": "0",
                "GROK_PROMPT_SUGGESTIONS": "0", "GROK_TITLE_REFRESH": "0", "GROK_TURN_SUMMARY": "0",
                "GROK_SESSION_RECAP": "0", "GROK_BACKEND_SEARCH": "0", "GROK_MAX_RETRIES": "0", "RUST_LOG": "off"})
    for vendor in ("CLAUDE", "CURSOR"):
        for cell in ("SKILLS", "RULES", "AGENTS", "MCPS", "HOOKS", "SESSIONS"):
            env[f"GROK_{vendor}_{cell}_ENABLED"] = "0"
    if allow_browser:
        from .browser_environment import sign_in_browser_environment
        env = sign_in_browser_environment(env, account_key="GROK_HOME", account_home=grok_home(home))
        for name in ("DISPLAY", "WAYLAND_DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR", "XAUTHORITY", "USER", "LOGNAME"):
            if os.environ.get(name):
                env[name] = os.environ[name]
    else:
        env["NO_BROWSER"] = "true"
    if bridge:
        env["QUANTIX_MCP_TOKEN"] = bridge.token
    return env


async def spawn(command, work, env, *, limit=1024 * 1024):
    host = os.environ.get("QUANTIX_AI_HOST_BINARY")
    if not host or not Path(host).is_absolute() or not Path(host).is_file():
        raise RuntimeUnavailable("The native Quantix process owner is unavailable. Prepare the AI component again.")
    # The nested native host owns Grok's complete job/process group and observes
    # stdin EOF even when this Python worker is forcibly terminated.
    return await asyncio.create_subprocess_exec(
        host, "--", *command, cwd=work, env=env, stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, limit=limit,
        **({"creationflags": 0x08000000} if os.name == "nt" else {}),
    )


async def discard(stream):
    while await stream.read(8192):
        pass


async def close_process(process):
    if process is None:
        return
    if process.stdin:
        process.stdin.close()
    if process.returncode is None:
        try:
            # EOF asks the native owner to stop the whole owned client tree.
            await asyncio.wait_for(process.wait(), 2)
            return
        except TimeoutError:
            try:
                process.terminate()
            except ProcessLookupError:
                pass
        try:
            await asyncio.wait_for(process.wait(), 4)
        except TimeoutError:
            try:
                process.kill()
            except ProcessLookupError:
                pass
            try:
                await asyncio.wait_for(process.wait(), 2)
            except TimeoutError:
                raise RuntimeUnavailable("Grok Build did not close within its shutdown allowance. Its result was withheld.") from None
