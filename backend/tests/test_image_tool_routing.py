"""Image-only Tender tools are offered only to models with established image input."""

from quantix.ai_catalog import documented_capabilities
from quantix.ai_runtime_mcp import RuntimeToolBridge
from quantix.office_tools import source_tools, usable_definitions


def test_image_tools_are_offered_only_with_image_support():
    names = {definition.name for definition in source_tools()}
    assert "view_document_page" in names

    without = {
        definition.name for definition in usable_definitions(source_tools(), image_support=False)
    }
    with_images = {
        definition.name for definition in usable_definitions(source_tools(), image_support=True)
    }

    assert without == names - {"view_document_page"}
    assert with_images == names


def test_subscription_bridge_hides_image_tools_without_image_support():
    assert "view_document_page" not in RuntimeToolBridge(None, None).tools
    assert "view_document_page" in RuntimeToolBridge(None, None, image_support=True).tools


def test_gemini_38_flash_documents_image_input():
    capabilities = documented_capabilities("google", "google", "gemini-3.8-flash")

    assert capabilities["images"] is True
    assert capabilities["tools"] is True
    assert capabilities["context_window"] == 1048576
