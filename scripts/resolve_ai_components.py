"""Refresh reviewed AI installation metadata; never install a provider or run it.

This maintenance command downloads publisher metadata and resolves wheel/npm
locks. Changing a pin is an explicit source change. It is not an application
test, dependency build, login, model request, or release packaging command.
"""

import argparse
import base64
import hashlib
import json
import re
import subprocess
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSETS = ROOT / "backend" / "ai-components"
UV_VERSION = "0.12.10"
PYTHON_VERSION = "3.12.14"
PYTHON_BUILD = "20260901"
NODE_VERSION = "24.20.0"
COPILOT_VERSION = "1.0.83"
GROK_VERSION = "1.0.13"
PLATFORMS = {
    "windows-x86_64": ("x86_64-pc-windows-msvc", "win-x64", "win32-x64"),
    "windows-aarch64": ("aarch64-pc-windows-msvc", "win-arm64", "win32-arm64"),
    "macos-x86_64": ("x86_64-apple-darwin", "darwin-x64", "darwin-x64"),
    "macos-aarch64": ("aarch64-apple-darwin", "darwin-arm64", "darwin-arm64"),
    "linux-x86_64": ("x86_64-unknown-linux-gnu", "linux-x64", "linux-x64"),
    "linux-aarch64": ("aarch64-unknown-linux-gnu", "linux-arm64", "linux-arm64"),
}
COMMON = ["mcp==2.1.1", "pydantic==2.13.5", "jsonschema==4.26.0", "platformdirs==4.11.7"]
COMPONENTS = {
    **{f"api-{name}": [f"pydantic-ai-slim[{name}]==2.40.0"]
       for name in ("openai", "anthropic", "google", "bedrock", "mistral", "cohere")},
    "api-openai-azure": ["pydantic-ai-slim[openai]==2.40.0", "azure-identity==1.25.3", "aiohttp==3.14.3"],
    "api-anthropic-azure": ["pydantic-ai-slim[anthropic]==2.40.0", "azure-identity==1.25.3", "aiohttp==3.14.3"],
    "client-codex": ["openai-codex==0.147.0", "openai-codex-cli-bin==0.147.0"],
    "client-copilot": ["github-copilot-sdk==1.0.13"],
    "client-claude": ["claude-agent-sdk==0.2.152", "pydantic-ai-slim[anthropic]==2.40.0"],
    "client-gemini": [],
    "client-grok": [],
}


def download(url):
    request = urllib.request.Request(url, headers={"User-Agent": "Quantix-Manifest-Maintenance"})
    with urllib.request.urlopen(request, timeout=90) as response:
        return response.read()


def release(repository, tag):
    return json.loads(download(f"https://api.github.com/repos/{repository}/releases/tags/{tag}"))


def asset(record, name):
    item = next(value for value in record["assets"] if value["name"] == name)
    digest = item.get("digest", "").removeprefix("sha256:")
    if not re.fullmatch(r"[a-f0-9]{64}", digest):
        raise ValueError(f"Publisher SHA-256 unavailable: {name}")
    return {"url": item["browser_download_url"], "sha256": digest, "filename": name}


def sums(url):
    result = {}
    for line in download(url).decode().splitlines():
        fields = line.split()
        if len(fields) == 2 and re.fullmatch(r"[a-f0-9]{64}", fields[0]):
            result[fields[1].lstrip("*")] = fields[0]
    return result


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")


def grok_component():
    """Read only registry metadata; installation downloads one native package."""
    assets = {}
    for target, (_, _, native_target) in PLATFORMS.items():
        package = f"@xai-official/grok-{native_target}"
        record = json.loads(download(f"https://registry.npmjs.org/{package}/{GROK_VERSION}"))
        operating_system, cpu = native_target.split("-", 1)
        dist = record["dist"]
        integrity = dist["integrity"]
        algorithm, encoded = integrity.split("-", 1)
        digest = base64.b64decode(encoded, validate=True)
        filename = f"grok-{native_target}-{GROK_VERSION}.tgz"
        url = f"https://registry.npmjs.org/{package}/-/{filename}"
        if (record["name"] != package or record["version"] != GROK_VERSION
                or record["os"] != [operating_system] or record["cpu"] != [cpu]
                or algorithm != "sha512" or len(digest) != 64 or dist["tarball"] != url):
            raise ValueError(f"Unreviewed Grok native package metadata: {target}")
        executable = "bin/grok.exe" if operating_system == "win32" else "bin/grok"
        assets[target] = {"url": url, "integrity": integrity, "filename": filename,
                          "package": package, "root": "package", "os": operating_system, "cpu": cpu,
                          "executable": executable, "compressed_executable": f"{executable}.br"}
    return {"version": GROK_VERSION, "requirements": COMMON.copy(), "locks": {},
            "native": {"directory": "client", "compression": "brotli", "assets": assets}}


def refresh_grok(manifest):
    """Add the native pin without re-resolving unrelated installed software."""
    definition = grok_component()
    # Grok has the identical Python base as Gemini and no Python client SDK.
    # Reuse each exact complete target lock, including its transitive hashes.
    base = manifest["components"]["client-gemini"]
    if base["requirements"] != definition["requirements"]:
        raise ValueError("Grok's Python base must be resolved again: Gemini's requirements changed.")
    directory = ASSETS / "locks"
    (directory / "client-grok.in").write_text("\n".join(COMMON) + "\n", encoding="utf-8", newline="\n")
    for target in PLATFORMS:
        source = ASSETS / base["locks"][target]["path"]
        data = source.read_bytes()
        if hashlib.sha256(data).hexdigest() != base["locks"][target]["sha256"]:
            raise ValueError(f"Grok's shared Python lock is damaged: {target}")
        lock = directory / f"client-grok-{target}.txt"
        lock.write_bytes(data)
        definition["locks"][target] = {"path": f"locks/{lock.name}", "sha256": hashlib.sha256(data).hexdigest()}
    manifest["components"]["client-grok"] = definition
    write_json(ASSETS / "manifest.json", manifest)


def metadata():
    uv, python = release("astral-sh/uv", UV_VERSION), release("astral-sh/python-build-standalone", PYTHON_BUILD)
    node_sums = sums(f"https://nodejs.org/dist/v{NODE_VERSION}/SHASUMS256.txt")
    copilot_sums = sums(f"https://github.com/github/copilot-cli/releases/download/v{COPILOT_VERSION}/SHA256SUMS.txt")
    platforms = {}
    for key, (triple, node_target, copilot_target) in PLATFORMS.items():
        windows = key.startswith("windows-")
        uv_file = f"uv-{triple}" + (".zip" if windows else ".tar.gz")
        python_file = f"cpython-{PYTHON_VERSION}+{PYTHON_BUILD}-{triple}-install_only.tar.gz"
        node_file = f"node-v{NODE_VERSION}-{node_target}" + (".zip" if windows else ".tar.xz")
        copilot_file = f"github-copilot-{COPILOT_VERSION}-{copilot_target}.tgz"
        platforms[key] = {
            "rust_target": triple,
            "uv": {**asset(uv, uv_file), "executable": "uv.exe" if windows else f"uv-{triple}/uv"},
            "python": {**asset(python, python_file), "executable": "python/python.exe" if windows else "python/bin/python3.12"},
            "node": {"url": f"https://nodejs.org/dist/v{NODE_VERSION}/{node_file}", "sha256": node_sums[node_file], "filename": node_file,
                     "root": f"node-v{NODE_VERSION}-{node_target}", "executable": "node.exe" if windows else "bin/node"},
            "copilot": {"url": f"https://github.com/github/copilot-cli/releases/download/v{COPILOT_VERSION}/{copilot_file}", "sha256": copilot_sums[copilot_file], "filename": copilot_file,
                        "platform": copilot_target, "root": "package"},
        }
    manifest = {"format": 1, "revision": "2026-09-07.1", "uv_version": UV_VERSION,
                "python_version": PYTHON_VERSION, "python_build": PYTHON_BUILD,
                "node_version": NODE_VERSION, "copilot_version": COPILOT_VERSION,
                "platforms": platforms, "components": {}}
    for name, requirements in COMPONENTS.items():
        value = grok_component() if name == "client-grok" else {"version": "1", "requirements": COMMON + requirements, "locks": {}}
        if name in {"client-copilot", "client-gemini"}:
            value["npm"] = {"directory": "account-client" if name == "client-copilot" else "client",
                            "lock": f"npm/{name}/package-lock.json", "package": f"npm/{name}/package.json",
                            "omit_optional": name == "client-gemini"}
            for field in ("lock", "package"):
                path = ASSETS / value["npm"][field]
                if path.is_file():
                    value["npm"][f"{field}_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
        for platform in PLATFORMS:
            lock = ASSETS / "locks" / f"{name}-{platform}.txt"
            if lock.exists():
                value["locks"][platform] = {"path": f"locks/{lock.name}", "sha256": hashlib.sha256(lock.read_bytes()).hexdigest()}
        manifest["components"][name] = value
    write_json(ASSETS / "manifest.json", manifest)
    return manifest


def resolve_python(manifest, uv_binary, selected, components=None):
    directory = ASSETS / "locks"
    directory.mkdir(parents=True, exist_ok=True)
    # Keep the recorded dependency choices, including transitive SDK versions.
    constraints = ASSETS / "constraints.txt"
    for name, value in manifest["components"].items():
        if components and name not in components:
            continue
        input_path = directory / f"{name}.in"
        input_path.write_text("\n".join(value["requirements"]) + "\n", encoding="utf-8", newline="\n")
        for platform, (triple, _, _) in PLATFORMS.items():
            if selected and platform not in selected:
                continue
            lock = directory / f"{name}-{platform}.txt"
            platform_constraints = constraints
            if platform in {"macos-x86_64", "windows-aarch64"}:
                # These two Windows snapshot transitives have no usable wheels
                # on this target. Resolve within the pinned SDK's declared
                # ranges, then commit the resulting target-specific exact lock.
                platform_constraints = ASSETS / f"constraints-{platform}.txt"
                lines = [line for line in constraints.read_text(encoding="utf-8").splitlines()
                         if not line.lower().startswith(("cryptography==", "tiktoken=="))]
                platform_constraints.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
            command = [str(uv_binary), "--no-config", "pip", "compile", str(input_path.relative_to(ROOT)),
                       "--python-version", PYTHON_VERSION, "--python-platform", triple,
                       "--no-build", "--generate-hashes", "--no-header", "--no-annotate",
                       "--constraint", str(platform_constraints.relative_to(ROOT)), "--output-file", str(lock.relative_to(ROOT)),
                       "--index-url", "https://pypi.org/simple"]
            completed = subprocess.run(command, cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            if completed.returncode:
                # Missing publisher wheels must remain an unsupported target.
                lock.unlink(missing_ok=True)
                value["locks"].pop(platform, None)
                print(f"Unavailable binary-only lock: {name} / {platform}: {completed.stderr[-1500:]}", flush=True)
                continue
            value["locks"][platform] = {"path": f"locks/{lock.name}", "sha256": hashlib.sha256(lock.read_bytes()).hexdigest()}
            print(f"Resolved {name} / {platform}", flush=True)
    write_json(ASSETS / "manifest.json", manifest)


def resolve_npm(manifest, node, npm):
    for name, dependency in (("client-copilot", {"@github/copilot": COPILOT_VERSION}),
                             ("client-gemini", {"@google/gemini-cli": "0.58.0"})):
        directory = ASSETS / "npm" / name
        directory.mkdir(parents=True, exist_ok=True)
        write_json(directory / "package.json", {"name": f"quantix-{name}", "version": "1.0.0", "private": True, "dependencies": dependency})
        subprocess.run([str(node), str(npm), "install", "--package-lock-only", "--ignore-scripts", "--no-audit", "--no-fund"], cwd=directory, check=True)
        lock = directory / "package-lock.json"
        for package_path, package in json.loads(lock.read_text(encoding="utf-8"))["packages"].items():
            if package_path and (not package.get("integrity") or not package.get("resolved", "").startswith("https://registry.npmjs.org/")):
                raise ValueError("Every npm dependency must have publisher registry integrity.")
        definition = manifest["components"][name]["npm"]
        definition["lock_sha256"] = hashlib.sha256(lock.read_bytes()).hexdigest()
        definition["package_sha256"] = hashlib.sha256((directory / "package.json").read_bytes()).hexdigest()
    write_json(ASSETS / "manifest.json", manifest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--uv", type=Path)
    parser.add_argument("--node", type=Path)
    parser.add_argument("--npm", type=Path)
    parser.add_argument("--platform", action="append", choices=PLATFORMS)
    parser.add_argument("--component", action="append", choices=COMPONENTS)
    parser.add_argument("--metadata-only", action="store_true")
    parser.add_argument("--grok-only", action="store_true",
                        help="Refresh only native Grok metadata and reuse the identical pinned Python base; no software is run.")
    args = parser.parse_args()
    if args.grok_only:
        refresh_grok(json.loads((ASSETS / "manifest.json").read_text(encoding="utf-8")))
        return
    manifest = metadata()
    if not args.metadata_only:
        if not args.uv:
            parser.error("Pass the absolute uv executable used only for dependency resolution.")
        resolve_python(manifest, args.uv.resolve(), args.platform, args.component)
        if args.node and args.npm:
            resolve_npm(manifest, args.node.resolve(), args.npm.resolve())


if __name__ == "__main__":
    main()
