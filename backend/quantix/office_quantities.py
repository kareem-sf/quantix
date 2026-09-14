"""Validate and publish attributed agent quantity proposals with no approval power."""

from .estimates import EstimateService


def validate_quantity_proposals(output, context):
    """Require the actual read BOQ basis and supporting evidence from this run."""
    from .source_boq import validate_source_row

    for proposal in output.boq_item_proposals:
        context.validate_sources([proposal.source_id])
        validate_source_row(context.repo, context.tender_id, proposal.model_dump())
    if not output.quantity_proposals:
        return
    service = EstimateService(context.repo)
    for proposal in output.quantity_proposals:
        if proposal.item_id not in context.item_bases:
            raise ValueError("The quantity proposal's BOQ item was not read in this run.")
        context.validate_sources(proposal.source_ids)
        _, basis = service.validate_agent_quantity(
            context.tender_id,
            proposal.model_dump(),
            context.item_bases[proposal.item_id],
        )
        context.validate_sources([basis["source_id"], *proposal.source_ids])


def publish_quantities(output, context):
    """Root calls this inside the owning job's final publication transaction."""
    validate_quantity_proposals(output, context)
    service = EstimateService(context.repo)
    return {
        "boq_item_proposals": [
            service.propose_source_row(
                context.tender_id, proposal.model_dump(), run_id=context.run_id
            )
            for proposal in output.boq_item_proposals
        ],
        "quantity_proposals": [
            service.propose_agent_quantity(
                context.tender_id,
                proposal.model_dump(),
                context.run_id,
                context.item_bases[proposal.item_id],
            )
            for proposal in output.quantity_proposals
        ],
    }
