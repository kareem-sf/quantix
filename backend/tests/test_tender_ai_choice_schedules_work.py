"""Choosing a Tender's AI schedules follow-up work on the service event loop."""

import asyncio

from fastapi import FastAPI
from fastapi.testclient import TestClient

from quantix import ai_setup_routes


def test_follow_up_work_runs_on_the_event_loop(monkeypatch):
    seen = {}
    monkeypatch.setattr(ai_setup_routes, "select_tender_ai", lambda repo, setup, tender_id, request: {"ok": tender_id})

    def schedule(tender_id):
        # asyncio.create_task needs a running loop; a worker thread has none.
        seen["loop"] = asyncio.get_running_loop() is not None
        seen["tender"] = tender_id

    app = FastAPI()
    router = ai_setup_routes.create_router(repo=None, setup=None, on_tender_ai=schedule)
    for route in router.routes:
        if route.path == "/api/tenders/{tender_id}/ai-setup":
            route.response_model = None
    app.include_router(router)
    with TestClient(app) as client:
        response = client.post("/api/tenders/t1/ai-setup",
                               json={"account_id": "a", "model_id": "m", "engineer_confirmed": True})
    assert response.status_code == 200, response.text
    assert seen == {"loop": True, "tender": "t1"}
