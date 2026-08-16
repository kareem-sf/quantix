# Quantix

[![CI](https://github.com/kareem-sf/quantix/actions/workflows/ci.yml/badge.svg)](https://github.com/kareem-sf/quantix/actions/workflows/ci.yml)

Quantix is an engineer-controlled tender office for preparing evidence-driven construction tender submissions with bounded AI assistance.

> **Status:** active pre-release development. Quantix is not yet qualified for public production use.

## Architecture

Quantix is a Tauri 2 desktop application with a React/TypeScript renderer and one trusted Rust Host. The renderer receives only named, domain-shaped commands; it has no generic filesystem, SQL, shell, process, credential, or updater interface.

Core design language and decisions live in [`CONTEXT.md`](CONTEXT.md), [`AGENTS.md`](AGENTS.md), and [`docs/adr/`](docs/adr/).

## Development

Requirements: the Node.js and Rust versions pinned by [`.node-version`](.node-version) and [`rust-toolchain.toml`](rust-toolchain.toml), plus the native prerequisites documented by Tauri for your operating system.

```text
npm ci
npm run tauri dev
```

Deterministic repository entry points:

```text
npm run format:check  # TypeScript, CSS, JSON, and Rust formatting
npm run check         # TypeScript typecheck and Rust clippy
npm test              # Renderer, Rust Host, and binding tests
npm run build         # Production renderer build
npm run build:desktop # Native Tauri package build
npm run verify        # Formatting, static checks, and deterministic tests
```

Live-provider and release acceptance commands are explicit and are not part of routine CI. See `package.json` and the accepted product specifications before running them.

## Private Windows release candidate

Quantix releases are intentionally layered. A draft Windows candidate is qualified first for engineer-operated, non-public use. Public distribution remains blocked until signed native acceptance passes on Windows 11 x64, macOS 14+ Apple Silicon, and Ubuntu 24.04 x64; signer-verified provenance binds the measured source to every native binary; and the legal and Codex production-assurance gates are satisfied.

On the trusted Windows 11 release workstation, open Git Bash and run the repeatable setup wizard:

```text
bash scripts/setup-windows-private-release.sh
```

The wizard creates the updater signing secrets, guides the one-time private GitHub runner setup, builds a draft NSIS/MSI candidate, and records deterministic plus five clean live qualification runs. It can be stopped and restarted; non-secret progress is kept in the ignored `.env.release.local` file. Never commit that file, updater private keys, passwords, certificates, downloaded candidate packages, or acceptance logs.

The candidate workflow is manual by design. It creates only draft prereleases and cannot authorize a public release. See [`ADR 0010`](docs/adr/0010-qualify-v0-through-layered-product-acceptance.md) for the complete release evidence model.

The generated TypeScript DTOs are committed outputs of the Rust Host types. Run `npm test` after changing an exported DTO and include regenerated declarations in the same commit.

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before opening a pull request. Changes must follow the domain glossary, relevant ADRs, linked specifications/issues, and the repository verification gates.

For vulnerabilities, follow [`SECURITY.md`](SECURITY.md). Never put credentials, proprietary Tender content, or other sensitive client data in public issues, pull requests, logs, or CI artifacts.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
