# Contributing to Quantix

Quantix is a controlled, pre-1.0 engineering product. Contributions should preserve the repository's domain authority, evidence model, security boundaries, and deterministic test seams.

## Before changing code

1. Read `AGENTS.md`.
2. Read `CONTEXT.md` and use its domain terms exactly.
3. Read the ADRs relevant to the change.
4. Link the change to an existing issue/specification, or open one when the behavior is not already specified.
5. Prefer the smallest durable end-to-end change over parallel implementations, compatibility layers, or speculative abstractions.

## Development workflow

Use a short-lived branch such as `feature/...`, `fix/...`, `issue/...`, `research/...`, or `chore/...`.

Install dependencies from the committed lockfile:

```text
npm ci
```

Before opening or updating a pull request, run:

```text
npm run verify
npm run build
```

Live-provider, native-package, private-qualification, and public-release commands are separate explicit gates. Do not make live AI credentials a routine test or CI requirement.

## Pull requests

Keep each pull request focused and link the governing issue/specification. Describe observable behavior, domain/ADR impact, verification performed, security/data implications, and any generated bindings that changed.

Do not merge changes that knowingly weaken Engineer-in-the-Loop authority, evidence/provenance, exact-version binding, permission boundaries, review independence, deterministic calculations, or fail-closed recovery semantics.

## Data and secrets

Never commit or attach:

- API keys, tokens, passwords, signing material, or provider credentials.
- Real confidential Tender Packages or proprietary client/company documents.
- Private user data or secrets copied from local Quantix state.
- Hidden model reasoning or raw provider traffic that the product intentionally excludes.

Use synthetic or appropriately licensed fixtures for public tests.

## Generated files

Generated TypeScript declarations owned by Rust DTOs are committed artifacts. When an exported DTO changes, regenerate the declarations through the repository's existing test/export seam and commit the result with the source change.

## Commit quality

Use clear imperative commit messages. Reference the issue number when useful. Avoid drive-by formatting, unrelated refactors, generated build output, or dependency changes unrelated to the pull request.
