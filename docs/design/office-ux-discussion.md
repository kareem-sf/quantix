# Office experience: decisions and additional ideas

This is a discussion brief, not an expansion of the accepted implementation scope. The settled architecture remains in [adaptive office scope](<D:/AI Work/quantix/docs/design/adaptive-office-scope.md>): only the Tender Manager exists by default; all other complete staff profiles are generated dynamically; the Manager personality is engineer-customizable; staff contexts are separate; actual messages and handoffs drive the live office. Ollama and the specified external integrations remain excluded, with BIM deferred.

The core architecture is sufficiently defined for planning. Before building the main live-office surface, resolve its information hierarchy, interaction depth and representation of staff. These choices influence layout and acceptance, not whether arbitrary supported requests can be handled.

## 1. The opening workspace

| Direction | Main focus | Advantage | Tradeoff |
|---|---|---|---|
| Balanced workspace | Manager conversation alongside the current document/result, with expandable live office | Keeps instructions, evidence and work close | Requires careful responsive layout and selection behavior |
| Manager first | Spacious conversation with linked work cards; office opens separately | Simple entry point and low initial cognitive load | More transitions when inspecting many records |
| Live office first | Dynamic staff/team activity dominates; conversation and documents open as needed | Makes delegation and collaboration immediately visible | Can compete with detailed document/calculation work |

Selected by the engineer: balanced workspace, with the Manager conversation beside the current document or result and the live office expandable. Keep the work area absent until there is useful content. The Manager remains the primary contact. This layout choice is accepted; the other new ideas remain proposals.

These are presentation choices over the same Tender records. They are not workflow modes and must not limit the requests the Manager can accept.

## 2. Other high-value experience decisions

| Decision | Suggested starting position | What must remain possible |
|---|---|---|
| Staff representation — selected | Distinct illustrated AI portraits with names and roles; portrait loading does not block work | Inspect the complete generated profile without cluttering the work surface |
| Amount of internal conversation — selected | Important exchanges, handoffs and questions visible by default; details expandable | Read every actual retained message and follow artifact references |
| Staff addressing | The engineer can address a generated colleague through the Manager | One coordinated authority and instruction history |
| Working information | Show task, result, evidence and blocker before lengthy personality details | Full profiles, tools, notes and history remain accessible |
| Document interaction | Clicking a source opens it beside the result when space permits; optional expanded inspection | Preserve drawing zoom, worksheet selection and return context |
| Interruptions | Group related staff questions; interrupt for time-sensitive material decisions | Urgent errors, blocked work and necessary authority remain visible |
| Staff lifecycle | Keep active staff prominent and completed/idle staff in history | The Manager can reuse or adapt an existing generated colleague in the same Tender |
| Manager identity | Same configurable Manager across projects, separate project context | Explicitly scoped company preferences and Tender-specific instructions |
| Motion | Task-driven arrivals, handoffs and state transitions; quiet settled states | Reduced motion, readable transcript and keyboard operation |
| Effort preference | Engineer can ask for a quick answer, fuller work or independent checking | Existing spending/permission bounds and honest quality limitations |
| Voice — selected timing | Text first; push-to-talk afterward, through separately verified capabilities | All work remains usable through text; no assumed always-listening microphone |
| Devices — selected timing | Desktop first; connected tablet access afterward | Tablet connectivity/hosting must be designed explicitly before promising remote access |

The opening layout, illustrated staff representation, default conversation density, desktop-first delivery and text-first input are settled. Closing the main window will keep active work running with a visible tray indicator; Quit remains a separate safe shutdown action. Remaining entries are suggested defaults or candidate enhancements, not additional blocking questions.

## 3. Additional product ideas

### Task-specific work surfaces

For an unfamiliar request, the Manager may propose a task-specific table, comparison board, issue matrix, timeline or calculation form. For example, a purchase-timing comparison could show supplier, payment date, delivery date, storage demand and unresolved terms, while a scope review could show item, source conflict and required clarification.

The renderer uses approved components and typed actions, with data connected to actual Tender records. The model does not inject unrestricted JavaScript, invent live values or bypass permissions. Persistent artifact IDs and source links let an unusual request produce a useful inspectable workspace without a bespoke screen for every possible workflow.

### A living request brief

Show what the Manager currently understands the engineer wants: outcome, scope, relevant revision, constraints and expected output. The engineer can correct it directly. Changes create instruction revisions and update remaining work at safe boundaries. This is useful when a long conversation gradually changes the assignment.

### A concise return-to-work briefing

On reopening a Tender, show what changed since the last visit: new results, unresolved decisions, changed sources and interrupted work. Build it from actual events and provide links. No change should mean no fabricated briefing or unnecessary model call.

### Questions gathered by the Manager

Several staff may need the same missing fact. The Manager should consolidate their questions, avoid asking again when the answer exists in evidence, and prioritize the question that unlocks the most relevant work. Every specialist should receive the same recorded answer and its applicability.

### Decision impact before acceptance

Before accepting a proposed assumption, quantity or rate, show the affected calculations, estimate lines and outputs. A comparison can explain what would change and what remains unknown. The impact preview is a draft calculation; it does not apply the change.

### Explain this delay

One action should explain why work is waiting: source missing, prior calculation unfinished, account unavailable, authority required or provider error. Link to the exact remedy. The engineer should not have to inspect an agent graph to discover the blocker.

### Ask about the selection

An engineer can select BOQ rows, a drawing region, a quotation paragraph or a result and ask the Manager about it. The request carries precise selection and revision context. This reduces repeated filenames and ambiguous instructions such as “check this.”

### Internal working copy and client copy

Make it easy to inspect the difference between internal calculations and the intended submission. Show which margins, notes, supplier comparisons and assumptions are included in the client-facing output. The preview uses actual export selections and preserves the separate release decision.

### Project vocabulary

Keep approved project abbreviations, package names, local terminology and bilingual equivalents. Retrieval and interpretation can use these mappings without rewriting original source text. Ambiguous abbreviations remain questions until their meaning is established for the relevant context.

### Explain why this person was created

Every generated colleague can show the Manager's creation reason, the task it supports and the capabilities it requested. This makes dynamic staffing understandable. The explanation cannot manufacture qualifications or claim that a generated persona guarantees competence.

### A record of promised work

Track the outcomes the Manager has committed to deliver, their actual status and unresolved prerequisites. This prevents a long conversation from losing an earlier request. Promises must correspond to work records rather than optimistic prose.

### Learn from a correction with explicit scope

When the engineer corrects a method or term, offer an appropriate scope: this result, this Tender, or a proposed company method. Do not silently generalize a project-specific correction to every future job. Approved reusable changes should have a version and evaluation case.

### Show the case for extra review

When the Manager proposes another specialist or an independent check, explain the unresolved issue it is expected to address. Present the relevant effort/spending information when available. Do not create extra staff simply to make the office look busy.

## 4. UI details that matter in daily engineering work

- Keep the active Tender, work basis/revision and current request visible at the appropriate level.
- Put a clear next action beside a result or blocker rather than requiring the engineer to discover it in another view.
- Use names, text states and record links with color as a supporting signal.
- Keep long profiles out of the default card; show the complete profile in the staff desk.
- Preserve selected rows, drawing zoom and scroll position when moving between conversation and evidence.
- Let tables support efficient keyboard use, comparison and copy/paste where appropriate.
- Distinguish a draft result, a staff check and an engineer decision in every relevant view.
- Keep new events from stealing focus or scrolling the engineer away from what they are reading.
- Pause decorative movement while inspecting documents or calculations; real state changes stay visible.
- Handle zero staff, many staff, long generated names/titles, mixed Arabic/English, unavailable data and disconnected service explicitly.
- Make source, calculation and result references addressable and recoverable after reopening the application.
- Treat notification density and workspace density as user preferences, independently of personality and permissions.

## 5. Suggested discussion sequence

The opening workspace, illustrated staff representation and important-exchanges default are agreed. The next design step is a concrete visual concept using a synthetic Tender with three states: Manager alone, dynamically staffed work, and a result awaiting an engineer decision.

The synthetic concept must label its data as illustrative. Use generated example staff only inside the concept/test data, never as production defaults. Evaluate the layout by completing an actual proposed journey—give an instruction, inspect a colleague's work, open evidence and return a decision—rather than selecting a screenshot on appearance alone.

New ideas above remain candidates. They do not automatically expand the first implementation increment or supersede the accepted scope and live-office brief.

See the [completeness review](<D:/AI Work/quantix/docs/design/office-completeness-review.md>) for the coverage audit and final recorded delivery choices.
