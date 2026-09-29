from datetime import datetime

from fastapi import APIRouter, HTTPException, UploadFile
from fastapi.responses import FileResponse
from pydantic import BaseModel, Field

from quantix import company
from quantix.api.ai import Home
from quantix.api.tenders import DB

router = APIRouter(tags=["company"])


class RuleIn(BaseModel):
    topic: str = Field(min_length=1, max_length=100)
    text: str = Field(min_length=1)


class RuleOut(RuleIn):
    id: str
    example: bool = Field(description="One Quantix starts every firm with, for the engineer to adjust or remove")
    created_at: datetime


class RuleChange(BaseModel):
    topic: str | None = Field(default=None, min_length=1, max_length=100)
    text: str | None = Field(default=None, min_length=1)


@router.get("/rules")
def get_rules(session: DB) -> list[RuleOut]:
    return [RuleOut.model_validate(r, from_attributes=True) for r in company.rules(session)]


@router.post("/rules", status_code=201)
def add_rule(body: RuleIn, session: DB) -> RuleOut:
    rule = company.CompanyRule(topic=body.topic.strip(), text=body.text.strip())
    session.add(rule)
    session.commit()
    return RuleOut.model_validate(rule, from_attributes=True)


@router.patch("/rules/{rule_id}")
def change_rule(rule_id: str, body: RuleChange, session: DB) -> RuleOut:
    """The engineer adjusts a rule; an example adjusted is the firm's own rule from then on."""
    rule = session.get(company.CompanyRule, rule_id)
    if rule is None:
        raise HTTPException(status_code=404, detail="Not found.")
    if body.topic is not None:
        rule.topic = body.topic.strip()
    if body.text is not None:
        rule.text = body.text.strip()
    rule.example = False
    session.commit()
    return RuleOut.model_validate(rule, from_attributes=True)


@router.delete("/rules/{rule_id}", status_code=204)
def remove_rule(rule_id: str, session: DB) -> None:
    rule = session.get(company.CompanyRule, rule_id)
    if rule is None:
        raise HTTPException(status_code=404, detail="Not found.")
    session.delete(rule)
    session.commit()


class ProfileIn(BaseModel):
    name: str = Field(max_length=200)
    address: str = Field(max_length=1000)
    cr_number: str = Field(max_length=50)
    vat_number: str = Field(max_length=50)


class ProfileOut(ProfileIn):
    has_logo: bool


def _profile_out(profile: company.Profile) -> ProfileOut:
    fields = {k: getattr(profile, k) for k in ("name", "address", "cr_number", "vat_number")}
    return ProfileOut(**fields, has_logo=profile.logo is not None)


@router.get("/company")
def get_profile(home: Home) -> ProfileOut:
    """The firm's details for the title block and letterhead of every document Quantix writes."""
    return _profile_out(company.profile(home))


@router.put("/company")
def save_profile(body: ProfileIn, home: Home) -> ProfileOut:
    return _profile_out(company.save_profile(home, body.name, body.address, body.cr_number, body.vat_number))


@router.put("/company/logo", status_code=204)
async def save_logo(file: UploadFile, home: Home) -> None:
    try:
        company.save_logo(home, await file.read())
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error


@router.delete("/company/logo", status_code=204)
def remove_logo(home: Home) -> None:
    company.remove_logo(home)


@router.get("/company/logo", response_class=FileResponse)
def get_logo(home: Home) -> FileResponse:
    logo = company.profile(home).logo
    if logo is None:
        raise HTTPException(status_code=404, detail="No logo yet.")
    return FileResponse(logo, media_type="image/png", headers={"Cache-Control": "no-store"})
