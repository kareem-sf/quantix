from quantix.message_wording import plain_message

OLD_SUMMARY = (
    "I registered and indexed the tender package (20 documents).\n\n"
    "**Mapping the tender package:** The package map could not be completed: The AI response did not "
    "match the required structured output. Its result was not published.\n\n"
    "Ask me anything about this tender, or tell me what to prepare first."
)


def test_old_package_summary_reads_in_plain_words():
    text = plain_message("manager", OLD_SUMMARY)
    assert text == (
        "I've read the tender package (20 documents).\n\n"
        "**Package overview:** I couldn't finish it. Ask me to try again.\n\n"
        "Ask me anything about this tender, or tell me what to prepare first."
    )


def test_old_waiting_and_other_stage_lines():
    waiting = plain_message(
        "manager",
        "**Mapping the tender package:** Choose the AI for this Tender to map the package and identify the project",
    )
    assert waiting == (
        "**Package overview:** Choose the AI for this Tender to map the package and identify the project."
    )
    assert "worker" not in plain_message(
        "manager",
        "**Mapping the tender package:** The package map could not be completed: This worker operation is unsupported.",
    )
    assert plain_message(
        "manager", "**Indexing tender evidence:** Meaning search could not be prepared: boom"
    ) == ("**Search:** it couldn't be prepared. I can still read the documents one by one.")


def test_record_ids_in_code_quotes_are_dropped_but_engineer_text_is_untouched():
    assert (
        plain_message(
            "manager",
            "Saved for your review: evidence table `dac27499ac11476bb10d29c140cdb9b5` v3.",
        )
        == "Saved for your review: evidence table v3."
    )
    typed = "I registered and indexed the tender package (20 documents)."
    assert plain_message("engineer", typed) == typed


def test_manager_is_told_to_address_the_engineer_as_you():
    from quantix.office import INSTRUCTIONS

    assert 'call them "you" and never "the\n  engineer"' in INSTRUCTIONS
