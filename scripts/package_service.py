"""Release-only sidecar packaging. Run natively on the target operating system."""
import argparse
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--target", required=True, help="Tauri target triple matching this build host")
args = parser.parse_args()
architecture = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine().lower())
suffix = {"win32": "pc-windows-msvc", "darwin": "apple-darwin", "linux": "unknown-linux-gnu"}.get(sys.platform)
if not architecture or not suffix or args.target != f"{architecture}-{suffix}":
    parser.error("Build on the target operating system and processor, using its matching Rust target triple.")
work = root / ".packaging"
work.mkdir(parents=True, exist_ok=True)
# This script is release-only. The small native owner is bundled as data so a
# frozen service can launch AI children without inheriting its private DLL path.
subprocess.run(["cargo", "build", "--manifest-path", str(root / "src-tauri" / "Cargo.toml"),
                "--release", "--target", args.target, "--bin", "quantix-ai-host"], cwd=root, check=True)
extension = ".exe" if sys.platform == "win32" else ""
host = root / "src-tauri" / "target" / args.target / "release" / f"quantix-ai-host{extension}"
command = [sys.executable, "-m", "PyInstaller", "--noconfirm", "--clean", "--onefile", "--name", "quantix-service", "--paths", str(root / "backend"), "--distpath", str(work / "dist"), "--workpath", str(work / "work"), "--specpath", str(work)]
for package in ("quantix", "mcp", "mcp_types", "fastembed", "onnxruntime", "pypdfium2", "genai_prices",
                "keyring", "pydantic_ai", "pydantic_graph", "openai", "anthropic", "google.genai", "httpx2"):
    command += ["--collect-all", package]
for distribution in ("quantix-service", "mcp", "fastembed", "genai-prices", "pydantic-ai-slim",
                     "pydantic-graph", "openai", "anthropic", "google-genai", "httpx2"):
    command += ["--recursive-copy-metadata", distribution]
for package in ("openai_codex", "codex_cli_bin", "copilot", "claude_agent_sdk", "azure.identity", "boto3", "botocore", "mistralai", "cohere"):
    command += ["--exclude-module", package]
for source, destination in ((root / "backend" / "quantix" / "diagnostics.py", "quantix"),
                            (root / "backend" / "ai-components", "ai-components"),
                            (root / "backend" / "ai_worker", "ai_worker"),
                            (host, "native")):
    command += ["--add-data", f"{source}{os.pathsep}{destination}"]
command += [str(root / "scripts" / "backend-entry.py")]
subprocess.run(command, cwd=root, check=True)
destination = root / "src-tauri" / "binaries" / f"quantix-service-{args.target}{extension}"
destination.parent.mkdir(parents=True, exist_ok=True)
shutil.copy2(work / "dist" / ("quantix-service" + extension), destination)
print("Packaged sidecar:", destination.name)
