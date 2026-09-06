"""Exercise the real SDK loop with a test-only provider and a real temporary repository."""

import json

import pytest
from agents.items import ModelResponse
from agents.models.interface import Model, ModelProvider
from agents.usage import Usage
from openai.types.responses import ResponseFunctionToolCall, ResponseOutputMessage

from quantix import office
from quantix.repository import Repository


@pytest.mark.asyncio
async def test_real_sdk_specialist_tool_loop_preserves_usage_and_proposed_records(
    tmp_path, monkeypatch
):
    repo = Repository(tmp_path)
    tender_id = repo.create_tender("Concrete works")["id"]
    artifact, _ = repo.register_artifact(
        tender_id,
        "Civil/spec.pdf",
        "a" * 64,
        100,
        {
            "kind": "pdf",
            "status": "extracted",
            "segments": [
                {"locator": "Page 2", "text": "Concrete shall achieve 35 MPa.", "page": 2}
            ],
        },
    )
    source_id = repo.artifact_evidence(tender_id, artifact["id"])[0]["id"]
    run_id = repo.create_run(tender_id, "manager", "Review concrete")["id"]

    class ScriptedModel(Model):
        turn = 0

        async def get_response(
            self,
            system_instructions,
            input,
            model_settings,
            tools,
            output_schema,
            handoffs,
            tracing,
            **kwargs,
        ):
            assert model_settings.reasoning.effort == "xhigh"
            assert "sk-" not in str(input)
            self.turn += 1
            if self.turn == 1:
                output = [
                    ResponseFunctionToolCall(
                        type="function_call",
                        name="consult_specialist",
                        call_id="call-consult",
                        arguments=json.dumps(
                            {
                                "role": "Concrete reviewer",
                                "brief": "Review the clause",
                                "source_ids": [source_id],
                            }
                        ),
                    )
                ]
            else:
                assert "35 MPa" in str(input)
                if self.turn == 2:
                    assert not any(tool.name == "consult_specialist" for tool in tools)
                payload = office.OfficeOutput(
                    summary="The specified concrete strength is 35 MPa.", source_ids=[source_id]
                )
                output = [
                    ResponseOutputMessage(
                        id=f"msg-{self.turn}",
                        role="assistant",
                        status="completed",
                        type="message",
                        content=[
                            {
                                "type": "output_text",
                                "text": payload.model_dump_json(),
                                "annotations": [],
                            }
                        ],
                    )
                ]
            return ModelResponse(
                output=output,
                usage=Usage(requests=1, input_tokens=100, output_tokens=50, total_tokens=150),
                response_id=f"response-{self.turn}",
            )

        async def stream_response(self, *args, **kwargs):
            raise AssertionError("This worker uses the awaited SDK loop.")
            yield

    model = ScriptedModel()

    class ScriptedProvider(ModelProvider):
        def get_model(self, model_name):
            assert model_name == "gpt-6-astra"
            return model

    monkeypatch.setattr(office, "OpenAIProvider", lambda **kwargs: ScriptedProvider())
    result = await office.run_manager(repo, tender_id, run_id, "Review concrete", "test-key")
    assert result.output.source_ids == [source_id]
    assert result.usage["input_tokens"] == 300
    assert result.usage["total_tokens"] == 450
    assert len(result.usage["request_details"]) == 3
    assert repo.messages(tender_id) == []
    assert any(event["kind"] == "specialist_result" for event in repo.run_events(run_id))
    assert repo.get_run(run_id)["status"] == "queued"  # Root job runner owns lifecycle.
