"""Normal browser profile paths for explicit original-client sign-in only.

Do not use this for model execution, metadata refresh or the worker itself.
The official client's own account-directory override must stay private.
"""

import json
import os
from pathlib import Path

from .common import RuntimeUnavailable

_PROFILE_KEYS = {"HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA",
                 "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"}
_ACCOUNT_KEYS = {"GROK_HOME", "GEMINI_CLI_HOME", "COPILOT_HOME"}


def sign_in_browser_environment(environment, *, account_key, account_home):
    """Keep OS/browser selection normal without importing browser credentials."""
    if account_key not in _ACCOUNT_KEYS or not Path(account_home).is_absolute():
        raise RuntimeUnavailable("This sign-in needs an explicit private AI account directory.")
    try:
        context = json.loads(os.environ.get("QUANTIX_DESKTOP_BROWSER_CONTEXT", ""))
        if not isinstance(context, dict) or set(context) != _PROFILE_KEYS:
            raise ValueError()
        for value in context.values():
            if value is not None and (not isinstance(value, str) or not value
                                      or len(value) > 32767 or "\0" in value or not Path(value).is_absolute()):
                raise ValueError()
        required = {"HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"} if os.name == "nt" else {"HOME"}
        if any(not context.get(key) for key in required):
            raise ValueError()
    except (ValueError, TypeError):
        raise RuntimeUnavailable("Reopen Quantix to apply its browser update before starting sign-in. Your saved AI account is kept.") from None
    result = dict(environment)
    for key, value in context.items():
        if value is None:
            result.pop(key, None)  # Let browsers use their normal OS defaults.
        else:
            result[key] = value
    # These documented provider overrides keep tokens and client configuration
    # outside the browser profile even though the child may open a browser.
    result[account_key] = str(account_home)
    result.pop("QUANTIX_DESKTOP_BROWSER_CONTEXT", None)
    return result
