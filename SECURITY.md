# Security Policy

## Supported version

Quantix is pre-release software. Security fixes are applied to the current development line and `main` once integrated; older development branches are not supported releases.

## Reporting a vulnerability

Do not disclose exploitable details, credentials, Tender content, or client data in a public issue.

Use GitHub's private **Report a vulnerability** / Security Advisory flow for this repository when it is available. If private reporting is unavailable, open only a minimal public issue titled `Security contact request` with no exploit details or sensitive data so a private channel can be established.

Include, privately, the affected commit/version, impact, reproduction steps, and the smallest safe proof of concept.

## Sensitive-data rules

Quantix handles potentially confidential Tender data. Public repository content, issues, pull requests, logs, screenshots, CI artifacts, fixtures, and diagnostics must not contain real confidential Tender Packages, provider credentials, signing material, or other secrets.

Security changes must preserve the Rust Host trust boundary, default-deny permissions, renderer command allow-listing, evidence/provenance controls, Engineer-in-the-Loop approval authority, and fail-closed behavior.
