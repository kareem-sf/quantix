"""The labeled corpus stays complete, frozen and distinct from held-out answers."""

import json
from pathlib import Path

from quantix.retrieval_eval import fingerprint, ranking_config_hash

from .dataset import (
    LANGUAGE_PAIRS,
    QUESTIONS,
    REQUIRED_CATEGORIES,
    SMOKE_QUERIES,
    corpus_hash,
    corpus_payload,
    validate_dataset,
)


def test_dataset_meets_the_acceptance_inventory():
    report = validate_dataset()
    assert report["questions"] >= 80
    assert report["held_out"] >= 19
    assert report["dataset_version"] == "2026-09-13.1"
    splits = {question["id"]: question["split"] for question in QUESTIONS}
    held = {qid for qid, split in splits.items() if split == "held_out"}
    train = {qid for qid, split in splits.items() if split == "train"}
    assert held.isdisjoint(train)
    assert {
        question["category"] for question in QUESTIONS if question["split"] == "held_out"
    } == REQUIRED_CATEGORIES
    pairs = {
        f"{question['query_language']}->{question['passage_language']}" for question in QUESTIONS
    }
    assert LANGUAGE_PAIRS <= pairs


def test_smoke_queries_keep_the_original_six_wordings():
    smoke = [question for question in QUESTIONS if question["smoke"]]
    assert [(question["id"], question["query"]) for question in smoke] == [
        ("smoke-pump-en", SMOKE_QUERIES[0][1]),
        ("smoke-pump-ar", SMOKE_QUERIES[1][1]),
        ("smoke-waterproof", SMOKE_QUERIES[2][1]),
        ("smoke-rebar", SMOKE_QUERIES[3][1]),
        ("smoke-curing", SMOKE_QUERIES[4][1]),
        ("smoke-vat", SMOKE_QUERIES[5][1]),
    ]
    assert SMOKE_QUERIES[0][1].startswith("أي بند يشترط إنتاجية 70")
    assert "B500D" in SMOKE_QUERIES[3][1]


def test_frozen_manifest_matches_the_labeled_corpus():
    manifest = json.loads(
        (Path(__file__).parent / "fixtures" / "manifest.json").read_text(encoding="utf-8")
    )
    report = validate_dataset()
    assert manifest["corpus_hash"] == report["corpus_hash"] == corpus_hash()
    assert manifest["dataset_version"] == report["dataset_version"]
    assert manifest["questions"] == report["questions"]
    assert manifest["held_out"] == report["held_out"]
    assert manifest["documents"] == report["documents"]


def test_corpus_and_config_hashes_are_stable_and_sensitive():
    first = corpus_hash()
    assert first == fingerprint(corpus_payload())
    assert first == corpus_hash()
    payload = corpus_payload()
    payload["questions"] = list(payload["questions"]) + [{"id": "tampered"}]
    assert fingerprint(payload) != first
    boosted = ranking_config_hash(bilingual_boost=True)
    assert boosted == ranking_config_hash(bilingual_boost=True)
    assert boosted != ranking_config_hash(bilingual_boost=False)
