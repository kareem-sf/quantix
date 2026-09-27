# Quantix

Quantix is a tendering office on the engineer's desktop. A Tender Manager leads AI staff who read the tender,
take off quantities, price the work, level subcontract and supplier quotes and prepare the submission. The engineer
stays in the loop and approves at every gate. The specification is `docs/spec.md`; the architecture is
`docs/architecture.md`; the current record is `docs/progress.md`.

## Rules

- **Engineer in the loop.** Anything that changes scope, method of measurement, quantities, rates, subcontract or
  supplier choices, the final price or the release waits at a gate for the engineer. The Fully autonomous setting
  lets the Tender Manager's review approve those gates, and marks each as not reviewed by the engineer. Export and
  release are always the engineer's.
- **The Tender Manager reviews everything.** Staff produce every record; the Manager never produces one. Each
  proposal goes to his review first: he accepts it, saying what he checked, or sends it back to whoever made it,
  saying what to correct. Only what he accepted reaches the engineer. Nothing approves itself. Nothing is left
  unresolved silently: what the office can't settle, he escalates to the engineer with where it shows and his
  suggested corrections, and the engineer decides.
- **Quantix computes, the model proposes.** Agents submit records through `propose`. Quantix validates them, checks
  their evidence and calculates every quantity, extension, total and comparison. A model never states a computed
  number as fact.
- **Evidence.** Every factual finding cites a document location that exists and was read. Keep imported, read and
  reviewed coverage distinct.
- **Staff are generated.** The Manager hires staff per tender with generated profiles. There is no default roster,
  fixed role list or hard-coded persona.
- **Talk is real.** The team conversation shows only messages that agents actually sent through their tools.
  Never invent progress or planning text.
- **Engineer experience first.** Minimal, calm interface in plain construction English. Each screen has one clear
  next action. Every number is one click from its source. No provider, model or token details in the work areas.
- **Nothing hard-coded about standards.** The office reads the tender's method of measurement, currency and tax,
  and the engineer approves them.
- **Data.** Everything Quantix manages lives under `~/.quantix`: AI connections and keys in `auth.json`, office
  settings in `settings.json`, tender records in the database. Never commit customer documents, keys, extracted
  content or databases. Preserve supplied files unchanged.
- **AI connections.** API keys for Anthropic, OpenAI, Google, xAI and one OpenAI-compatible endpoint; ChatGPT/Codex
  and Grok subscriptions only through their official clients. Never copy subscription tokens into API credentials.
- **Simple.** A modular monolith. Prefer maintained libraries and documented APIs; validate them before relying on
  them. No speculative infrastructure, compatibility layers or code nothing uses.
- **Done means verified.** Each change ends with service tests, UI tests, typecheck and lint passing. Use synthetic
  data for approvals and other acceptance mutations. Approving the real tender and anything commercial remain the
  engineer's decisions.

## Commands

Commands are in `README.md`. Service tests run with the project virtual environment (`service/.venv`).
