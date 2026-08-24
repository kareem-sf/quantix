# Task 6 baseline test and format repair report

Base: `289a25bca24399f7e2d7ae91f02a2d10818266bd`

## Baseline RED

Command:

`cargo test --manifest-path src-tauri/Cargo.toml --test bid_decisions affected_unapproved_package_review_can_be_interrupted_before_material_rework --features runtime-fixture -- --exact --nocapture --test-threads=1`

Output (exit 1):

`tests\bid_decisions.rs:3866:14: replacement-bound impacted record successor`

`test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 109 filtered out; finished in 13.58s`

## Repair

- Rebound the interruption-only test from shared-source `programme_pressure` to isolated `project_delivery_context`.
- Applied the exact rustfmt indentation in `manager_workspace.rs`.

## GREEN and verification

Focused command:

`cargo test --manifest-path src-tauri/Cargo.toml --test bid_decisions affected_unapproved_package_review_can_be_interrupted_before_material_rework --features runtime-fixture -- --exact --nocapture --test-threads=1`

Output (exit 0):

`test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 109 filtered out; finished in 15.18s`

Complete integration-binary command:

`cargo test --manifest-path src-tauri/Cargo.toml --test bid_decisions --features runtime-fixture -- --nocapture --test-threads=1`

Output (exit 0):

`test result: ok. 110 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1319.13s`

Formatting and whitespace commands:

`rustfmt --edition 2021 --check src-tauri/tests/bid_decisions.rs src-tauri/tests/manager_workspace.rs`

`npm run format:check`

`git diff --check`

Output (all exit 0): rustfmt and `git diff --check` had no output; `npm run format:check` reported `All matched files use Prettier code style!`.
