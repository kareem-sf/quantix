"""The evaluator must not pass broken rankings or swapped corpora."""

from quantix.retrieval_eval import evaluate_case, evaluate_run, ranking_config_hash

from .dataset import QUESTIONS, corpus_hash

CONFIG = ranking_config_hash(bilingual_boost=True)
CORPUS = corpus_hash()


def _question(**overrides):
    base = {
        "id": "case",
        "split": "train",
        "category": "numeric_units",
        "query": "cover",
        "query_language": "en",
        "passage_language": "en",
        "relevant": [{"document": "units-en", "locator": "page:1", "grade": 2}],
        "absent": False,
        "permitted_documents": None,
        "collection": "tender_evidence",
        "exhaustive": False,
    }
    base.update(overrides)
    return base


def _hit(document, locator, **overrides):
    row = {
        "document": document,
        "locator": locator,
        "id": f"{document}:{locator}",
        "tender_id": "t1",
    }
    row.update(overrides)
    return row


def _score(question, hits, **kwargs):
    return evaluate_case(
        question,
        hits,
        corpus_hash=kwargs.get("corpus_hash", CORPUS),
        config_hash=kwargs.get("config_hash", CONFIG),
        expected_corpus_hash=kwargs.get("expected_corpus_hash", CORPUS),
        expected_config_hash=kwargs.get("expected_config_hash", CONFIG),
        tender_id=kwargs.get("tender_id", "t1"),
    )


def test_missing_rank_is_not_treated_as_a_hit():
    result = _score(_question(), [])
    assert result["metrics"]["rank"] is None
    assert result["metrics"]["mrr"] == 0.0
    assert result["metrics"]["recall_at_10"] == 0.0
    assert result["passed"] is False
    assert result["critical"] == []


def test_two_relevant_passages_do_not_pass_as_complete_when_one_is_missing():
    question = _question(
        category="multiple_answers",
        relevant=[
            {"document": "curing-en", "locator": "page:1", "grade": 2},
            {"document": "curing-second", "locator": "page:1", "grade": 2},
        ],
    )
    result = _score(question, [_hit("curing-en", "page:1")])
    assert result["metrics"]["recall_at_10"] == 0.5
    assert result["metrics"]["all_relevant_found"] is False
    assert result["metrics"]["recall_at_10"] != 1.0


def test_absent_question_cannot_pass_when_a_supported_hit_is_returned():
    question = _question(category="unrelated", relevant=[], absent=True)
    result = _score(question, [_hit("pump-en", "page:1", weak_match=False)])
    assert result["metrics"]["false_supported"] is True
    assert "supported_answer_on_absent" in result["critical"]
    assert result["passed"] is False


def test_absent_question_passes_when_hits_are_empty_or_all_weak():
    question = _question(category="unrelated", relevant=[], absent=True)
    empty = _score(question, [])
    assert empty["passed"] is True
    assert empty["metrics"]["false_supported"] is False
    weak = _score(question, [_hit("pump-en", "page:1", weak_match=True)])
    assert weak["passed"] is True
    assert weak["metrics"]["false_supported"] is False


def test_forbidden_hit_cannot_pass():
    question = _question(
        permitted_documents=["bond-permitted"],
        relevant=[{"document": "bond-permitted", "locator": "page:1", "grade": 2}],
    )
    result = _score(question, [_hit("noise-bond-00", "page:1"), _hit("bond-permitted", "page:1")])
    assert "forbidden_passage" in result["critical"]
    assert result["passed"] is False


def test_stale_passage_marked_current_cannot_pass():
    result = _score(
        _question(),
        [_hit("units-en", "page:1", is_current=True, superseded=True)],
    )
    assert "stale_as_current" in result["critical"]
    assert result["passed"] is False


def test_foreign_tender_hit_cannot_pass():
    result = _score(_question(), [_hit("units-en", "page:1", tender_id="other")])
    assert "foreign_tender" in result["critical"]
    assert result["passed"] is False


def test_changed_corpus_hash_cannot_pass():
    result = _score(_question(), [_hit("units-en", "page:1")], expected_corpus_hash="0" * 64)
    assert "corpus_hash_mismatch" in result["critical"]
    assert result["passed"] is False


def test_changed_config_hash_cannot_pass():
    result = _score(_question(), [_hit("units-en", "page:1")], expected_config_hash="0" * 64)
    assert "config_hash_mismatch" in result["critical"]
    assert result["passed"] is False


def test_run_hash_mismatch_fails_the_whole_report():
    report = evaluate_run(
        QUESTIONS[:3],
        {question["id"]: [] for question in QUESTIONS[:3]},
        corpus_hash=CORPUS,
        config_hash=CONFIG,
        expected_corpus_hash="deadbeef",
        expected_config_hash=CONFIG,
        tender_id="t1",
    )
    assert report["summary"]["passed"] is False
    assert all("corpus_hash_mismatch" in case["critical"] for case in report["cases"])


def test_unmeasured_collections_are_not_scored_as_zero_recall():
    question = _question(
        category="long_work_product",
        collection="work_product",
        relevant=[{"document": "wp-long-report", "locator": "char:16000", "grade": 2}],
    )
    result = _score(question, [])
    assert result["available"] is False
    assert result["metrics"]["recall_at_10"] is None
    assert result["metrics"]["mrr"] is None
    assert result["passed"] is True
