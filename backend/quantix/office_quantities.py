"""Validate and publish attributed agent quantity proposals with no approval power."""

from .estimates import EstimateService


def validate_quantity_proposals(output, context):
    """Require the actual read BOQ basis and supporting evidence from this run."""
    from .source_boq import save_conflict, validate_source_row

    staged: dict[tuple[str, str], dict] = {}
    if output.boq_item_proposals:
        EstimateService(context.repo)  # makes sure the estimate tables exist
    with context.repo.db.connect() as conn:
        for proposal in output.boq_item_proposals:
            try:
                context.validate_sources([proposal.source_id])
                checked, _, artifact = validate_source_row(
                    context.repo, context.tender_id, proposal.model_dump()
                )
                # Everything the final save checks is checked now, while it can be fixed.
                key = (checked.source_id, checked.row_reference.casefold())
                written = checked.model_dump(mode="json")
                if staged.setdefault(key, written) != written:
                    raise ValueError(
                        "Another staged row from the same passage uses this item number with different "
                        "text. Give each row its own item number, or stage it once."
                    )
                problem = save_conflict(context.repo, conn, context.tender_id, checked, artifact)
                if problem:
                    raise ValueError(problem)
            except (KeyError, ValueError) as error:
                # Name the row, so one bad row in a batch is fixed without guessing.
                message = error.args[0] if error.args else str(error)
                raise type(error)(f"BOQ row {proposal.row_reference}: {message}") from None
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
