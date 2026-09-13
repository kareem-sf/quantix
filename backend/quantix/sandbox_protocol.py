"""Bounded subprocess transport. Model text never becomes a shell command."""

from __future__ import annotations

import asyncio
import hashlib
import json
import os
import re
import signal
import subprocess
from dataclasses import dataclass
from pathlib import Path


class SandboxFailure(RuntimeError):
    pass


def safe_name(value: str) -> str:
    if (
        not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,119}", value)
        or value.endswith((".", " "))
        or value.split(".")[0].upper()
        in {
            "CON",
            "PRN",
            "AUX",
            "NUL",
            *[f"COM{i}" for i in range(10)],
            *[f"LPT{i}" for i in range(10)],
        }
    ):
        raise ValueError("Use a simple file name without folders or reserved device names.")
    return value


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write_record(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(value, ensure_ascii=False, sort_keys=True), encoding="utf-8")
    os.replace(temporary, path)


@dataclass(frozen=True)
class CommandResult:
    returncode: int
    stdout: bytes
    stderr: bytes


class CommandRunner:
    def __init__(self):
        self.active: set[int] = set()
        self.tasks: set[asyncio.Task] = set()

    async def close(self):
        pending = [task for task in self.tasks if task is not asyncio.current_task()]
        for task in pending:
            task.cancel()
        if pending:
            await asyncio.gather(*pending, return_exceptions=True)

    async def run(
        self,
        args: list[str],
        *,
        cwd: Path,
        env: dict[str, str],
        timeout: float,
        max_output: int = 1024 * 1024,
        stdin: bytes | None = None,
    ) -> CommandResult:
        kwargs = (
            {"creationflags": subprocess.CREATE_NO_WINDOW}
            if os.name == "nt"
            else {"start_new_session": True}
        )
        process = await asyncio.create_subprocess_exec(
            *args,
            cwd=cwd,
            env=env,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            **kwargs,
        )
        job = None
        if os.name == "nt":
            # The executable is a fixed trusted runtime client, not model code.
            # Its descendants are grouped before any input is delivered.
            import win32api
            import win32job

            try:
                job = win32job.CreateJobObject(None, "")
                handle = win32api.OpenProcess(0x0100 | 0x0001, False, process.pid)
                try:
                    win32job.AssignProcessToJobObject(job, handle)
                finally:
                    handle.Close()
            except Exception:
                if job:
                    job.Close()
                process.kill()
                await process.communicate()
                raise SandboxFailure("Windows could not contain the runtime process tree.")
        self.active.add(process.pid)
        owner = asyncio.current_task()
        self.tasks.add(owner)
        remaining = max_output

        async def read(stream):
            nonlocal remaining
            chunks = []
            while chunk := await stream.read(65536):
                remaining -= len(chunk)
                if remaining < 0:
                    raise SandboxFailure("The code exceeded its output limit.")
                chunks.append(chunk)
            return b"".join(chunks)

        async def send():
            try:
                if stdin:
                    process.stdin.write(stdin)
                    await process.stdin.drain()
            except (BrokenPipeError, ConnectionResetError):
                pass
            finally:
                process.stdin.close()

        tasks = [
            asyncio.create_task(read(process.stdout)),
            asyncio.create_task(read(process.stderr)),
            asyncio.create_task(send()),
        ]
        completed = False
        try:
            async with asyncio.timeout(timeout):
                result = await asyncio.gather(*tasks)
                await process.wait()
            completed = process.returncode == 0
            return CommandResult(process.returncode, result[0], result[1])
        except TimeoutError as exc:
            raise SandboxFailure("The code exceeded its time limit.") from exc
        finally:
            if not completed:
                if os.name == "nt":
                    win32job.TerminateJobObject(job, 1)
                else:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            for task in tasks:
                if not task.done():
                    task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)

            # A full pipe can keep Process.wait pending even after kill. Drain
            # discarded bytes without retaining them before reaping the child.
            async def discard(stream):
                while await stream.read(65536):
                    pass

            try:
                async with asyncio.timeout(2):
                    await asyncio.gather(discard(process.stdout), discard(process.stderr))
                    await process.wait()
            except TimeoutError as error:
                raise SandboxFailure(
                    "The runtime process tree could not be reaped within its cleanup limit."
                ) from error
            finally:
                if job:
                    job.Close()
                self.active.discard(process.pid)
                self.tasks.discard(owner)
