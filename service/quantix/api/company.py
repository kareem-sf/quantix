from datetime import datetime

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from quantix import company
from quantix.api.tenders import DB

router = APIRouter(tags=["company"])


class RuleIn(BaseModel):
    topic: str = Field(min_length=1, max_length=100)
    text: str = Field(min_length=1)


class RuleOut(RuleIn):
    id: str
    created_at: datetime


@router.get("/rules")
def get_rules(session: DB) -> list[RuleOut]:
    return [RuleOut.model_validate(r, from_attributes=True) for r in company.rules(session)]


@router.post("/rules", status_code=201)
def add_rule(body: RuleIn, session: DB) -> RuleOut:
    rule = company.CompanyRule(topic=body.topic.strip(), text=body.text.strip())
    session.add(rule)
    session.commit()
    return RuleOut.model_validate(rule, from_attributes=True)


@router.delete("/rules/{rule_id}", status_code=204)
def remove_rule(rule_id: str, session: DB) -> None:
    rule = session.get(company.CompanyRule, rule_id)
    if rule is None:
        raise HTTPException(status_code=404, detail="Not found.")
    session.delete(rule)
    session.commit()
