"""Authenticated observation of existing runs using the AI SDK UI protocol."""

import json

from fastapi import APIRouter, HTTPException, Query, Request
from fastapi.responses import StreamingResponse

from .chat_stream import RunChatStream


def create_router(repo, *, should_stop=None):
    router = APIRouter(prefix="/api", tags=["Agent chat"])
    service = RunChatStream(repo, should_stop=should_stop)

    @router.get("/tenders/{tender_id}/runs/{run_id}/chat-stream")
    async def observe_run(
        tender_id: str,
        run_id: str,
        request: Request,
        after: int = Query(0, ge=0),
        cursor: str | None = Query(None, max_length=1024),
    ):
        try:
            service.require_run(tender_id, run_id)
            with repo.db.connect() as conn:
                if cursor:
                    service.activity._decode(conn, run_id, cursor)
                elif after:
                    anchor = conn.execute(
                        "SELECT run_id FROM run_events WHERE id=?", (after,)
                    ).fetchone()
                    if anchor is not None and anchor[0] != run_id:
                        raise ValueError("The activity cursor does not belong to this run.")
        except KeyError as error:
            raise HTTPException(
                status_code=404, detail="This work could not be found in the selected Tender."
            ) from error
        except InterruptedError as error:
            raise HTTPException(status_code=503, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

        async def body():
            async for chunk in service.chunks(tender_id, run_id, after=after, cursor=cursor):
                if await request.is_disconnected():
                    return
                yield (
                    "data: " + json.dumps(chunk, ensure_ascii=False, separators=(",", ":")) + "\n\n"
                )
            yield "data: [DONE]\n\n"

        return StreamingResponse(
            body(),
            media_type="text/event-stream",
            headers={
                "x-vercel-ai-ui-message-stream": "v1",
                "X-Accel-Buffering": "no",
                "Cache-Control": "no-store",
            },
        )

    return router
