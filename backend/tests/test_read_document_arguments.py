"""Reading a whole document needs only the document: paging is optional."""

from quantix.office_tools import source_tools


def _definition(name: str):
    return next(item for item in source_tools() if item.name == name)


def test_read_whole_document_requires_only_the_document():
    parameters = _definition("read_whole_document").parameters
    required = parameters.get("required") or []

    assert required == ["artifact_id"]
    assert set(parameters["properties"]) >= {"artifact_id", "offset"}


def test_paging_starts_at_the_beginning():
    model = _definition("read_whole_document").argument_model
    parsed = model.model_validate({"artifact_id": "synthetic-artifact"})

    assert parsed.offset == 0
