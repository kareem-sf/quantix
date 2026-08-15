# Quantix

[![CI](https://github.com/kareem-sf/quantix/actions/workflows/ci.yml/badge.svg)](https://github.com/kareem-sf/quantix/actions/workflows/ci.yml)

Quantix is an engineer-controlled tender office for preparing evidence-driven construction tender submissions with bounded AI assistance.

> **Status:** active pre-release development. Quantix is not yet qualified for public production use.

## Architecture

Quantix is a Tauri 2 desktop application with a React/TypeScript renderer and one trusted Rust Host. The renderer receives only named, domain-shaped commands; it has no generic filesystem, SQL, shell, process, credential, or updater interface.

Core design language and decisions live in [`CONTEXT.md`](CONTEXT.md), [`AGENTS.md`](AGENTS.md), and [`docs/adr/`](docs/adr/).

## Development

Requirements: current Node.js/npm, the Rust toolchain, and the native prerequisites documented by Tauri for your operating system.

```text
npm install
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

The generated TypeScript DTOs are committed outputs of the Rust Host types. Run `npm test` after changing an exported DTO and include regenerated declarations in the same commit.

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) before opening a pull request. Changes must follow the domain glossary, relevant ADRs, linked specifications/issues, and the repository verification gates.

For vulnerabilities, follow [`SECURITY.md`](SECURITY.md). Never put credentials, proprietary Tender content, or other sensitive client data in public issues, pull requests, logs, or CI artifacts.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
