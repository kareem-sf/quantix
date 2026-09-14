# Tender team runtime

Status: implemented 2026-09-14. Replaces the dynamic-office delegation stack. Interfaces: [contracts](../contracts.md#tender-manager-and-team--14-september-2026).

## Why

Quantix is a tendering team, not a lone Tender Manager. The Manager hires staff whose profiles are generated
live, delegates work to them and to an AI team, and the engineer sees all of it.

The previous implementation never ran end to end. Before one colleague could start, the engineer had to
approve a plan, a delegation grant, an AI team and per-staff route options. Each step then crossed about
19,000 lines of routing, bindings, checkpoints, ownership leases, handoffs, coordination receipts and
notebooks. The local database had zero assignments.

## Concepts

- **Tender Manager** — leads the tender. One conversation with the engineer. Editable profile.
- **Staff member** — created by the Manager for this tender: name, role, discipline, experience, working style
  and a portrait seed. Nothing is hard-coded. The engineer can retire a member.
- **Assignment** — work the Manager gives to one staff member: title, brief, the documents to start from,
  the expected result, and the AI model to use.
  - Status: queued, running, waiting for the Manager, completed, failed or cancelled.
  - Holds the staff result (summary, findings, cited evidence, counts of records saved for review), any
    question and usage.
- **Team tab** — the engineer sees the working brief, every staff member, and each assignment's brief,
  question, answer, result and usage, and can steer the running Manager. There is no separate messaging
  system.

## Flow

1. The engineer sends a message and a Manager run starts.
2. The Manager works with evidence tools and team tools: `hire_staff`, `assign_work`, `list_team`,
   `read_assignment` and `answer_staff`. `assign_work` only queues work.
3. When the Manager's turn ends, the controller runs the queued assignments under the same run and the same
   tender allowance.
   - Direct API routes run up to three staff at once.
   - Subscription clients (Codex, Grok) run one at a time.
   - Each staff turn is a full agent loop over the tender's evidence tools.
   - Each turn ends with a completed result or a question for the Manager.
4. If any assignment finished or asked something, the Manager gets another turn with those outcomes.
   A run has at most six Manager turns.
5. The Manager's final answer is published to the conversation. Stop cancels the run and every queued or
   running assignment.

## AI team

- Staff use the tender's approved AI route by default.
- The Manager may choose another model for an assignment, but only from checked models on accounts the
  tender already allows.
- The engineer sees which model did each piece of work.
- There is no per-assignment approval: the tender allowance and the request cap bound all spending.

## Authority

- Staff can:
  - read all current tender evidence;
  - calculate;
  - search the public web when the tender's route allows it;
  - stage takeoff lines, BOQ rows, quantities, submission requirements and project map items, saved for
    review when the assignment completes.
- Staff cannot send, approve, delete or change engineer decisions.
- Findings and proposals stay proposals until the engineer approves them.

## Removed

Plan-review AI-team approval, delegation grants and envelopes, route options and bindings, office checkpoints,
ownership leases, the assignment scheduler and graph, handoffs, office messages and delivery receipts, staff
notebooks, the staff lifecycle and capability catalog, the agent definition library, resource leases and code
runtime scopes.

## Delivered

1. Backend team runtime with its tests; the replaced modules are deleted.
2. Team tab: working brief, staff roster, assignments, results, usage and steering.
3. Manager harness: a 4,000-character core prompt (was 10,600), a three-field answer schema (1,100 characters
   of schema, was 15,000), `propose` and `proposal_format` for every record kind, no separate routing call.
   The Manager has 35 tools; the duplicate search and document readers were removed, but the target of
   about 20 was not reached.
4. Agentic takeoff: staff take quantities from the drawings with `propose(kind="takeoff")`; Quantix compares
   each line with the BOQ and the engineer reviews them in Estimate → Takeoff.
