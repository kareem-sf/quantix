# Quantix

Build an adaptive, engineer-controlled Tender Office. The current specification is docs/spec.md and execution record is docs/progress.md. This is a fresh project; obsolete code and old GitHub specifications are not implementation requirements.

- The primary agent owns architecture, integration, review, and verification. Inspect all changes.
- Prefer maintained libraries, official SDKs, and documented APIs. Validate before implementing. Keep components small and responsibilities clear.
- Build complete working increments; no compatibility layers, fake production behaviour, or speculative infrastructure.
- All product copy uses plain construction-engineering language. The main contact is the Tender Manager.
- Quantix's core UX principle is simple plain-language guidance with one clear next action. Keep details and advanced controls in More options while preserving every capability and actionable error.
- Current AI scope keeps direct API keys for OpenAI, Anthropic, Google, xAI and OpenAI-compatible BYOK/custom, and restores supported ChatGPT/Codex and Grok subscription methods through the original official clients. Follow docs/subscription-connections.md and dated official-source research; other subscription routes remain unavailable unless their integration is documented and permitted. Direct APIs retain bundled SDK execution. Never copy OAuth tokens into API credentials or silently change billing/approved models. Keep Tender data permission, source validation and spending authority intact.
- Preserve supplied Tender files. Do not commit customer documents, API keys, private extracted content, or runtime databases.
- The normal Quantix application home is `~/.quantix`. Keep databases, imported copies, outputs, private AI software/account profiles, caches, logs, temporary work, connection records and supported desktop WebView state beneath it. Do not reintroduce AppData or repository `.quantix-dev` runtime storage. Source/dependencies/build artifacts stay in the project; OS-protected credentials and user-chosen original/export locations remain separate. Explicit isolated backend roots are for development verification only.
- Use source references for factual findings. Keep imported, extracted, analysed, and reviewed coverage distinct.
- Current user override for the approved 2026-09-09 redesign: implement docs/design/workspace-redesign.md with affected backend and UI tests, frontend typecheck, and visual user-journey checks in the real app, including light/dark and scaling. This supersedes the earlier verification restriction for this work. Do not build release packages. Use synthetic data for approvals and other acceptance mutations; approval of the real Tender and commercial sending remain the engineer's decisions.
- Read docs/contracts.md before changing a shared interface. Update API schemas and generated frontend types together.
- User approval already covers architecture decisions, local implementation, installation of project dependencies, local verification, and reversible fixes. Continue work without routine permission questions.

## Commands

Commands are established by the bootstrap task and kept in README.md. Backend tests run with the project virtual environment. User data lives outside the repository unless a test explicitly uses a temporary directory.
