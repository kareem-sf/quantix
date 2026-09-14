"""A gateway may add a vendor namespace to the model it reports; nothing else may differ."""

from quantix.ai_api_provider import same_reported_model


def test_gateway_vendor_namespace_is_the_same_model():
    assert same_reported_model("custom", "openai_chat", "deepseek-v4.1-flash", "deepseek/deepseek-v4.1-flash")
    assert same_reported_model("custom", "openai_chat", "deepseek-v4.1-flash", "deepseek-v4.1-flash")


def test_a_different_model_is_still_rejected():
    assert not same_reported_model("custom", "openai_chat", "deepseek-v4.1-flash", "deepseek/deepseek-v4-flash")
    assert not same_reported_model("custom", "openai_chat", "deepseek-v4.1-flash", "a/b/deepseek-v4.1-flash")
    assert not same_reported_model("custom", "openai_chat", "deepseek-v4.1-flash", "deepseek-v4.1-flash-lite")


def test_first_party_providers_stay_exact():
    assert not same_reported_model("openai", "openai_chat", "gpt-5.4", "openai/gpt-5.4")
