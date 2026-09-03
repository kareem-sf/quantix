import io
import json
import sys

from pydantic_ai.models.test import TestModel

from quantix_ai_worker import worker as worker_module

CALL_SCHEMA = {"type": "object", "properties": {"query": {"type": "string"}}}
OUTPUT_SCHEMA = {
    "type": "object",
    "properties": {"summary": {"type": "string"}},
    "required": ["summary"],
}
LOOKUP_TOOL = {
    "name": "lookup_item",
    "description": "look up an item",
    "parameters": CALL_SCHEMA,
}


class InteractiveInput(io.TextIOBase):
    def __init__(self, init_frame, answer):
        self.lines = [json.dumps(init_frame)]
        self.index = 0
        self.pending_emissions = []
        self.answer = answer

    def readline(self):
        while self.index >= len(self.lines):
            requests = [
                frame
                for frame in self.pending_emissions
                if frame["kind"] == "approval_request"
            ]
            if not requests or self.answer is None:
                return ""
            for request in requests:
                approved, result, denial = self.answer(request)
                self.lines.append(
                    json.dumps(
                        {
                            "kind": "approval",
                            "tool_call_id": request["tool_call_id"],
                            "approved": approved,
                            "result": result,
                            "denial_message": denial,
                        }
                    )
                )
            self.pending_emissions = [
                frame
                for frame in self.pending_emissions
                if frame["kind"] != "approval_request"
            ]
        line = self.lines[self.index]
        self.index += 1
        return line + "\n"


class Recorder:
    def __init__(self, target, stdin):
        self.target = target
        self.stdin = stdin
        self.frames = []

    def write_line(self, frame):
        self.target(frame)
        if frame.get("kind") == "approval_request":
            self.stdin.pending_emissions.append(frame)
        self.frames.append(frame)


def run(init_frame, model, answer):
    stdin = InteractiveInput(init_frame, answer)
    captured = io.StringIO()
    original_stdin, original_stdout = sys.stdin, sys.stdout
    original_build = worker_module.build_model
    original_write = worker_module.Worker.write_line
    sys.stdin, sys.stdout = stdin, captured
    worker_module.build_model = lambda request: model
    recorder = Recorder(original_write, stdin)
    worker_module.Worker.write_line = staticmethod(
        lambda frame: recorder.write_line(frame)
    )
    try:
        worker_module.main()
    except SystemExit as exit_error:
        assert exit_error.code in (0, None), captured.getvalue()
    finally:
        sys.stdin, sys.stdout = original_stdin, original_stdout
        worker_module.build_model = original_build
        worker_module.Worker.write_line = staticmethod(original_write)
    out_frames = recorder.frames
    assert out_frames, "no output frames"
    assert out_frames[0]["kind"] == "ready", out_frames
    assert sum(1 for f in out_frames if f["kind"] in ("result", "failure")) == 1
    return out_frames


def initialize(op, tools=None, **overrides):
    frame = {
        "kind": "initialize",
        "op": op,
        "route": "openai",
        "base_url": None,
        "api_key": "test-key",
        "model_id": "test-model",
        "reasoning": None,
        "instructions": "helpful",
        "output_schema": None,
        "tools": tools if tools is not None else [],
        "budgets": {
            "max_tool_rounds": 4,
            "max_output_bytes": 1024 * 1024,
            "timeout_ms": 30000,
        },
        "mode": "gated",
        "input": "Do the work.",
    }
    frame.update(overrides)
    return frame


def conformance():
    frames = run(initialize("probe"), TestModel(), None)
    assert frames[-1]["kind"] == "result" and frames[-1]["usage"], frames

    frames = run(initialize("turn"), TestModel(), None)
    assert frames[-1]["kind"] == "result" and frames[-1]["text"], frames

    def approve(request):
        return True, {"query": request["tool_name"]}, None

    frames = run(
        initialize("turn", tools=[LOOKUP_TOOL]),
        TestModel(call_tools=["lookup_item"]),
        approve,
    )
    requests = [f for f in frames if f["kind"] == "approval_request"]
    assert requests and requests[0]["tool_name"] == "lookup_item", frames
    assert frames[-1]["kind"] == "result", frames

    def deny(request):
        return False, None, "Denied by Quantix: policy"

    frames = run(
        initialize("turn", tools=[LOOKUP_TOOL]),
        TestModel(call_tools=["lookup_item"]),
        deny,
    )
    assert frames[-1]["kind"] == "result", frames

    frames = run(
        initialize("turn", output_schema=OUTPUT_SCHEMA),
        TestModel(call_tools=[], custom_output_args={"summary": "done"}),
        None,
    )
    assert frames[-1]["kind"] == "result" and frames[-1]["output"] == {
        "summary": "done"
    }, frames

    try:
        run(
            initialize("turn", tools=[LOOKUP_TOOL]),
            TestModel(call_tools=["lookup_item"]),
            None,
        )
        raise AssertionError("input ended without terminal must fail")
    except worker_module.ProtocolError:
        pass

    try:
        lines = io.StringIO(
            json.dumps({"kind": "cancel"}) + "\n"
        )
        original_stdin, original_stdout = sys.stdin, sys.stdout
        sys.stdin, sys.stdout = lines, io.StringIO()
        try:
            worker_module.main()
        except worker_module.ProtocolError:
            pass
        else:
            raise AssertionError("first frame must be initialize")
    finally:
        sys.stdin, sys.stdout = original_stdin, original_stdout

    try:
        worker_module.build_model({**initialize("turn"), "route": "nonsense"})
        raise AssertionError("unknown route must fail")
    except worker_module.ProtocolError:
        pass

    try:
        worker_module.build_model_settings(
            {**initialize("turn"), "route": "anthropic", "reasoning": "high"}
        )
        raise AssertionError("unsupported reasoning must fail closed")
    except worker_module.ProtocolError:
        pass

    print("conformance: all scenarios passed")


if __name__ == "__main__":
    conformance()
