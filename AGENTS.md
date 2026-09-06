# Quantix

Build an adaptive, engineer-controlled Tender Office. The current specification is docs/spec.md and execution record is docs/progress.md. This is a fresh project; obsolete code and old GitHub specifications are not implementation requirements.

- Use gpt-6-astra with xhigh reasoning for every delegated development/research/review agent. Never silently substitute models.
- The primary agent owns architecture, integration, review, and verification. Delegate bounded independent work with non-overlapping file ownership. Inspect all changes.
- Prefer maintained libraries, official SDKs, and documented APIs. Validate before implementing. Keep components small and responsibilities clear.
- Build complete working increments; no compatibility layers, fake production behaviour, or speculative infrastructure.
- All product copy uses plain construction-engineering language. The main contact is the Tender Manager.
- Preserve supplied Tender files. Do not commit customer documents, API keys, private extracted content, or runtime databases.
- Use source references for factual findings. Keep imported, extracted, analysed, and reviewed coverage distinct.
- Current user override: implement the complete MVP without writing or running tests, lint, typechecks, browser QA or live integration checks. Testing is deferred until the engineer tests with us. Do not describe untested changes as verified. Keep existing tests for that later session. Do not build release packages during normal development.
- Read docs/contracts.md before changing a shared interface. Update API schemas and generated frontend types together.
- User approval already covers architecture decisions, local implementation, installation of project dependencies, local verification, and reversible fixes. Continue work without routine permission questions.

## Commands

Commands are established by the bootstrap task and kept in README.md. Backend tests run with the project virtual environment. User data lives outside the repository unless a test explicitly uses a temporary directory.
