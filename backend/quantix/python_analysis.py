"""Selected byte snapshots in disposable rootless Python containers.

Inputs are supplied by a trusted source resolver, never by a model path. The
container has no host mounts. Receipt/output paths remain private beneath home.
"""

from __future__ import annotations

import asyncio
import base64
import json
import time
import uuid
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path

from .local_code_records import write_local_record
from .podman_runtime import PodmanRuntime
from .python_analysis_models import PythonAnalysisReceipt, PythonAnalysisRequest, PythonOutput
from .sandbox_protocol import SandboxFailure, safe_name, sha256, write_record


@dataclass(frozen=True)
class SelectedPythonInput:
    """Server-created exact input snapshot after the normal source-scope fence."""

    id: str
    name: str
    content: bytes
    sha256: str
    source_version: str


# These are fixed trusted transport programs, not model-generated shell text.
# They execute only inside the dedicated VM's disposable containers.
_POPULATE = """
import base64,json,pathlib,sys
for item in json.load(sys.stdin):
 p=pathlib.Path('/inputs')/item['name']
 p.write_bytes(base64.b64decode(item['data'],validate=True))
 p.chmod(0o444)
"""

_EXECUTE = """
import base64,json,os,pathlib,subprocess,sys,threading
request=json.load(sys.stdin)
limit=request['limit']
used=[0]
streams=[bytearray(),bytearray()]
failure=[]
child=subprocess.Popen([sys.executable,'-I','-c',request['code']],stdin=subprocess.DEVNULL,
 stdout=subprocess.PIPE,stderr=subprocess.PIPE,cwd='/outputs',start_new_session=True,
 env={'HOME':'/tmp','PATH':'/usr/local/bin:/usr/bin:/bin','MPLCONFIGDIR':'/tmp/matplotlib',
      'OPENBLAS_NUM_THREADS':'1','OMP_NUM_THREADS':'1','MPLBACKEND':'Agg'})
lock=threading.Lock()
def read(pipe,index):
 while True:
  data=pipe.read(65536)
  if not data: break
  with lock:
   used[0]+=len(data)
   if used[0]>limit:
    failure.append('The code exceeded its output limit.')
    try: os.killpg(child.pid,9)
    except ProcessLookupError: pass
   else: streams[index].extend(data)
threads=[threading.Thread(target=read,args=(child.stdout,0)),threading.Thread(target=read,args=(child.stderr,1))]
for t in threads:t.start()
try: child.wait(timeout=request['seconds'])
except subprocess.TimeoutExpired:
 failure.append('The code exceeded its time limit.')
finally:
 try: os.killpg(child.pid,9)
 except ProcessLookupError: pass
 child.wait()
for t in threads:t.join(timeout=2)
outputs=[]
for p in sorted(pathlib.Path('/outputs').iterdir()):
 if p.is_symlink() or not p.is_file():
  failure.append('Output folders and links are not supported. Save flat files in /outputs.')
  break
 size=p.stat().st_size
 used[0]+=size
 if used[0]>limit or len(outputs)>=100:
  failure.append('The code exceeded its output limit.')
  break
 outputs.append({'name':p.name,'data':base64.b64encode(p.read_bytes()).decode('ascii')})
json.dump({'returncode':child.returncode,'stdout':bytes(streams[0]).decode('utf-8','replace'),
 'stderr':bytes(streams[1]).decode('utf-8','replace'),'outputs':outputs if not failure else [],
 'detail':failure[0] if failure else ''},sys.stdout)
"""


class PythonAnalysisService:
    def __init__(self, home: Path, *, runtime=None):
        self.home = Path(home).resolve()
        self.runtime = runtime or PodmanRuntime(self.home)

    async def execute(
        self,
        request: PythonAnalysisRequest,
        *,
        inputs: list[SelectedPythonInput],
        context,
        authority_check,
        runtime_fingerprint=None,
    ) -> PythonAnalysisReceipt:
        async with self.runtime.operation():
            return await self._execute(
                request,
                inputs=inputs,
                context=context,
                authority_check=authority_check,
                runtime_fingerprint=runtime_fingerprint,
            )

    async def _execute(
        self, request, *, inputs, context, authority_check, runtime_fingerprint=None
    ) -> PythonAnalysisReceipt:
        """Caller owns reviewed tool permission and source resolution.

        authority_check is required and invoked before dispatch/publication. It
        must reject stopped/superseded work, changed inputs or expanded limits.
        Calling this function never creates or enlarges an execution grant.
        """
        authority_check()
        if set(request.input_ids) != {item.id for item in inputs} or len(inputs) != len(
            request.input_ids
        ):
            raise ValueError("Select the exact reviewed inputs for this Python method.")
        if len({item.name.casefold() for item in inputs}) != len(inputs):
            raise ValueError("Selected inputs must have distinct file names.")
        if sum(len(item.content) for item in inputs) > 50 * 1024 * 1024:
            raise ValueError("Selected Python inputs exceed 50 MiB.")
        for item in inputs:
            safe_name(item.name)
            if sha256(item.content) != item.sha256 or not item.source_version:
                raise ValueError("A selected source changed. Select its current version again.")
        status = await self.runtime.status()
        if not status.python_available:
            raise SandboxFailure(status.detail + " " + status.next_action)
        # No await between setup admission and increment. Runtime changes are
        # blocked until cleanup and receipt persistence both finish.
        if self.runtime.lock.locked():
            raise SandboxFailure("Wait for Python setup to finish.")
        identifier = uuid.uuid4().hex
        name, volume = f"quantix-run-{identifier}", f"quantix-input-{identifier}"
        work = self.home / "code" / "runs" / identifier
        work.mkdir(parents=True)
        manifest = self.runtime.manifest()
        started = time.monotonic()
        receipt = PythonAnalysisReceipt(
            id=identifier,
            status="failed",
            code_sha256=sha256(request.code.encode()),
            image_id=manifest["image_id"],
            library_versions=manifest["libraries"],
            input_hashes={item.id: item.sha256 for item in inputs},
            limits=request.limits,
            created_at=datetime.now(UTC).isoformat(),
        )
        (work / "code.py").write_text(request.code, encoding="utf-8")
        snapshots = work / "inputs"
        snapshots.mkdir()
        for item in inputs:
            (snapshots / item.name).write_bytes(item.content)
        cleanup = []

        def persist(phase, *, running=True):
            write_local_record(
                work / "receipt.json",
                {
                    **receipt.model_dump(mode="json"),
                    "status": "running" if running else receipt.status,
                    "inputs": [
                        {
                            "id": item.id,
                            "name": item.name,
                            "sha256": item.sha256,
                            "source_version": item.source_version,
                            "size_bytes": len(item.content),
                        }
                        for item in inputs
                    ],
                    "cleanup_unconfirmed": cleanup,
                },
                context=context,
                engine="python",
                phase=phase,
                runtime_fingerprint=runtime_fingerprint,
            )

        persist("preparing_selected_inputs")
        self.runtime.active_runs += 1
        try:
            await self.runtime.command(
                "volume", "create", "--label=io.quantix.owner=python", volume, connection=True
            )
            # A trusted one-shot writer receives only selected bytes. It has no
            # host path mounts; this volume becomes read-only for model code.
            bundle = json.dumps(
                [
                    {"name": item.name, "data": base64.b64encode(item.content).decode()}
                    for item in inputs
                ]
            ).encode()
            await self.runtime.command(
                "run",
                "--rm",
                "--label=io.quantix.owner=python",
                "--timeout=35",
                "--name",
                f"quantix-load-{identifier}",
                "--interactive",
                "--pull=never",
                "--network=none",
                "--read-only",
                "--cap-drop=ALL",
                "--security-opt=no-new-privileges",
                "--memory=256m",
                "--pids-limit=16",
                "--mount",
                f"type=volume,source={volume},target=/inputs",
                "--user=0:0",
                manifest["image_id"],
                "python",
                "-I",
                "-c",
                _POPULATE,
                connection=True,
                stdin=bundle,
                timeout=30,
            )
            authority_check()
            persist("executing")
            result = await self.runtime.command(
                *self.runtime.container_args(
                    name, manifest["image_id"], limits=request.limits, input_volume=volume
                ),
                "python",
                "-I",
                "-c",
                _EXECUTE,
                connection=True,
                stdin=json.dumps(
                    {
                        "code": request.code,
                        "seconds": request.limits.seconds,
                        "limit": request.limits.output_mib * 1024 * 1024,
                    }
                ).encode(),
                timeout=request.limits.seconds + 10,
                max_output=request.limits.output_mib * 1024 * 1024 * 2 + 65536,
            )
            parsed = json.loads(result.stdout)
            receipt.stdout, receipt.stderr = parsed["stdout"], parsed["stderr"]
            if not isinstance(receipt.stdout, str) or not isinstance(receipt.stderr, str):
                raise SandboxFailure("The Python response was invalid.")
            total = len(receipt.stdout.encode()) + len(receipt.stderr.encode())
            outputs = parsed["outputs"]
            if not isinstance(outputs, list) or len(outputs) > 100:
                raise SandboxFailure("The Python output list exceeded its limit.")
            destination = work / "outputs"
            destination.mkdir()
            seen = set()
            for output in outputs:
                filename = safe_name(output["name"])
                if filename.casefold() in seen:
                    raise SandboxFailure("The Python output contains duplicate names.")
                seen.add(filename.casefold())
                data = base64.b64decode(output["data"], validate=True)
                total += len(data)
                if total > request.limits.output_mib * 1024 * 1024:
                    raise SandboxFailure("The code exceeded its output limit.")
                (destination / filename).write_bytes(data)
                receipt.outputs.append(
                    PythonOutput(name=filename, size_bytes=len(data), sha256=sha256(data))
                )
            authority_check()
            receipt.detail = str(parsed.get("detail", ""))[:2000]
            if parsed["returncode"] == 0 and not receipt.detail:
                receipt.status = "completed"
            elif not receipt.detail:
                receipt.detail = (
                    "The Python method failed. Inspect its error output and revise the method."
                )
        except asyncio.CancelledError:
            receipt.status = "cancelled"
            receipt.detail = "Python work was stopped. Its container has been removed."
            raise
        except Exception as error:
            receipt.detail = str(error)[:2000]
        finally:
            # Client cancellation alone does not stop a remote container. Reap
            # both the code and input writer through the exact owned connection.
            for container in (name, f"quantix-load-{identifier}"):
                try:
                    await self.runtime.command(
                        "rm", "--force", "--ignore", container, connection=True, timeout=15
                    )
                except Exception:
                    cleanup.append(container)
            try:
                await self.runtime.command(
                    "volume", "rm", "--force", volume, connection=True, timeout=15
                )
            except Exception:
                cleanup.append(volume)
            if cleanup:
                receipt.status = "failed"
                receipt.detail = (
                    "Python cleanup could not be confirmed. Repair local Python before another run."
                )
                manifest["qualified"] = False
                write_record(self.runtime.manifest_path, manifest)
            receipt.duration_seconds = round(time.monotonic() - started, 4)
            try:
                persist(receipt.status, running=False)
            finally:
                self.runtime.active_runs -= 1
        return receipt
