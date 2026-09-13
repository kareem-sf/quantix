"""Worker-side validation of the already approved connection endpoint."""

import ipaddress
from urllib.parse import urlsplit, urlunsplit


def validate_base_url(value, *, allow_insecure_http=False):
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
    loopback = parts.hostname.lower() == "localhost"
    try:
        loopback = loopback or ipaddress.ip_address(parts.hostname).is_loopback
    except ValueError:
        pass
    if parts.scheme == "http" and not loopback and not allow_insecure_http:
        raise ValueError("Remote providers require HTTPS unless insecure HTTP was explicitly approved.")
    if port is not None and not 1 <= port <= 65535:
        raise ValueError("Enter a valid provider endpoint port.")
    return urlunsplit((parts.scheme, parts.netloc.lower(), parts.path.rstrip("/"), "", ""))
