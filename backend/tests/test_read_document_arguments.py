"""Reading a document needs only the document: paging arguments are optional."""

from quantix.office_tools import source_tools


def _definition(name: str):
    return next(item for item in source_tools() if item.name == name)


def test_read_document_requires_only_the_document():
    parameters = _definition("read_document").parameters
    required = parameters.get("required") or []

    assert required == ["artifact_id"]
    assert set(parameters["properties"]) >= {"artifact_id", "offset", "limit"}


def test_paging_defaults_stay_inside_the_tool_limits():
    model = _definition("read_document").argument_model
    parsed = model.model_validate({"artifact_id": "synthetic-artifact"})

    assert parsed.offset == 0
    assert 1 <= parsed.limit <= 20
