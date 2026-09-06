# Agent drawing measurement proposals

Implemented in the development-only continuation. No tests, lint, typechecks, browser checks or live provider calls were run for this increment, following the explicit user instruction.

## Root integration

- Add `AgentMeasurementProposal[]` as `OfficeOutput.drawing_measurements`.
- Include `office_measurement.measurement_tools()` for the Manager and specialists that can inspect drawing pages.
- Call synchronous `validate_agent_measurement(context, proposal)` when validating proposed output and again at atomic publication. It returns a validated `AgentMeasurementProposal`.
- Publish each accepted proposal through `MeasurementService(repo).propose_agent(tender_id, proposal, run_id)` inside the owning job transaction.
- Preserve the existing approval-time `validate_quantity_proposal` hook and regenerate API/frontend types for the added origin and supporting-source fields.

The service and helper do not send messages or approve quantities. Root retains run cancellation/finalization authority and source-read tracking.

## Proposal and calculation

`AgentMeasurementProposal` extends the existing calibrated geometry input with `scope_label` and distinct supporting `source_ids`. Up to 28 supporting sources are accepted so the later derived measurement reference and BOQ row still fit the existing quantity-proposal contract's 30-source limit. The caller cannot submit an authoritative page size, calculated quantity, unit, author origin or engineer confirmation.

`calculate_drawing_measurement` is the read-only office tool. It accepts drawing artifact/page, length/area/count mode, normalized top-left points, optional calibration points/metres, scope label and supporting source IDs. The helper validates that all sources were already read and remain current in the same Tender; at least one reference must identify the measured page. Every measurement and calibration point must lie inside a region actually inspected through `view_document_page` during that run.

The quantity is calculated against the preserved PDF's actual dimensions through the existing calculator and shared PDFium lock. The result includes the supporting references and explicit agent-proposal/review limitations. The tool records only its run activity; it creates no measurement, engineer decision or BOQ change. Count uses no length calibration.

## Saved records and review

Manual creation and agent proposal publication use one `_store` implementation. Agent records retain `origin=agent`, `status=proposed`, their owning `run_id`, `reviewed_at=null` and `review_rationale=null`. Manual records retain the existing engineer-reviewed creation behavior. No engineer confirmation is invented for agent publication.

Supporting snapshots retain source IDs, original artifact/path/version/hash, exact locator/page and evidence fingerprints. Those references are rechecked before saving, on later inspection/linking and through the existing quantity-approval validation path. A changed supporting dimension source makes the measurement need recheck even when the measured PDF itself is unchanged.

Derived evidence retains agent origin and states that it is not engineer-reviewed, not extracted drawing text and not a checked BOQ quantity. The frontend shows the proposal origin, source references and absent initial engineer review explicitly.

Linking remains an explicit engineer action. It requires the existing engineer checkbox and rationale, now describing review of the drawing, calibration/count marks, supporting dimensions, scope and BOQ unit. The validated request's confirmation is forwarded to the quantity-proposal service; a separate `review_for_boq_proposal` decision is appended. This records the review without overwriting immutable agent authorship. A later quantity approval is still required before the BOQ's effective quantity changes.

No verification outcome is claimed for these development-only changes.
