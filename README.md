# Quantix

Quantix is an engineer-controlled tender office for preparing evidence-driven construction tender submissions with bounded AI assistance.

The v0 desktop architecture is Tauri 2 with a React/TypeScript renderer and one trusted Rust Host. The renderer receives only named, domain-shaped commands; it has no generic filesystem, SQL, shell, process, credential, or updater interface.

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
npm test              # Rust Host and binding tests
npm run build         # Production renderer build
npm run build:desktop # Native Tauri package build
npm run verify        # Development verification; does not build or package
```

Production builds are explicit release-stage commands. Normal development uses
`npm run tauri dev` and `npm run verify` without creating release artifacts.

The generated TypeScript DTOs are committed outputs of the Rust Host types. Run `npm test` after changing an exported DTO and include the regenerated declarations in the same commit.

## License

Apache-2.0.
