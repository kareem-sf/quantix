"""Specialist quantity arithmetic remains an attributed, unapproved proposal."""

from pydantic import Field, model_validator

from .estimate_models import DecimalText, EstimateModel


class AgentQuantityProposal(EstimateModel):
    item_id: str = Field(min_length=1, max_length=100)
    quantity: DecimalText
    calculation: str = Field(min_length=1, max_length=6000)
    source_ids: list[str] = Field(min_length=1, max_length=29)

    @model_validator(mode="after")
    def distinct_sources(self):
        if any(not value.strip() or len(value) > 100 for value in self.source_ids):
            raise ValueError("Use valid supporting quantity source references.")
        if len(set(self.source_ids)) != len(self.source_ids):
            raise ValueError("Cite each supporting source once.")
        return self
