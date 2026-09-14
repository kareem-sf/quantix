# Tender team runtime

Status: approved direction (engineer, 2026-09-14). Replaces the dynamic-office delegation stack.

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
  and a portrait seed. Nothing is hard-coded. The Manager can revise or retire a member.
- **Assignment** — work the Manager gives to one staff member: title, brief, the documents to start from,
  the expected result, and the AI model to use.
  - Status: queued, running, waiting for the Manager, completed, failed or cancelled.
  - Holds the staff result (summary, findings, cited evidence), any question, usage and cost.
- **Office timeline** — briefs, questions, answers and results come straight from assignments. There is no
  separate messaging system.

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
  - search the public web when the tender allows it;
  - save attributed, unapproved proposals.
- Staff cannot send, approve, delete or change engineer decisions.
- Findings and proposals stay proposals until the engineer approves them.

## Removed

Plan-review AI-team approval, delegation grants and envelopes, route options and bindings, office checkpoints,
ownership leases, the assignment scheduler and graph, handoffs, office messages and delivery receipts, staff
notebooks, the staff lifecycle and capability catalog, the agent definition library, resource leases and code
runtime scopes.

## Delivery

1. Backend team runtime (this document) with its tests; delete the replaced modules.
2. Office view: staff roster, assignment timeline, results and cost in one panel.
3. Manager harness:
   - short core prompt;
   - small answer schema;
   - proposal save tools checked at call time;
   - about 20 tools;
   - no separate routing call.
4. Agentic drawing takeoff: a vision staff member measures from drawings, cross-checks BOQ quantities and lists
   items missing from the BOQ.
