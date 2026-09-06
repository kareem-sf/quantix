"""Validate and publish attributed agent quantity proposals with no approval power."""

from .estimates import EstimateService


def validate_quantity_proposals(output, context):
    """Require the actual read BOQ basis and supporting evidence from this run."""
    if not output.quantity_proposals:
        return
    service = EstimateService(context.repo)
    for proposal in output.quantity_proposals:
        if proposal.item_id not in context.item_bases:
            raise ValueError("The quantity proposal's BOQ item was not read in this run.")
        context.validate_sources(proposal.source_ids)
        _, basis = service.validate_agent_quantity(
            context.tender_id, proposal.model_dump(), context.item_bases[proposal.item_id],
        )
        context.validate_sources([basis["source_id"], *proposal.source_ids])


def publish_quantities(output, context):
    """Root calls this inside the owning job's final publication transaction."""
    validate_quantity_proposals(output, context)
    if not output.quantity_proposals:
        return {"quantity_proposals": []}
    service = EstimateService(context.repo)
    return {"quantity_proposals": [
        service.propose_agent_quantity(
            context.tender_id, proposal.model_dump(), context.run_id,
            context.item_bases[proposal.item_id],
        )
        for proposal in output.quantity_proposals
    ]}
