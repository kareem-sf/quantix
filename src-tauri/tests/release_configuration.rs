use std::{fs, path::PathBuf};

use serde_json::Value;

use quantix_lib::EvaluatePublicReleaseGateCommand;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf()
}

#[test]
fn native_candidate_build_does_not_require_public_release_authorization() {
    let config: Value = serde_json::from_slice(
        &fs::read(repository_root().join("src-tauri/tauri.conf.json"))
            .expect("read Tauri configuration"),
    )
    .expect("parse Tauri configuration");

    assert_eq!(
        config["build"]["beforeBuildCommand"],
        "npm run prepare:runtime && npm run build",
        "candidate packaging must precede product qualification; public authorization belongs to the later publication boundary",
    );
}

#[test]
fn custom_titlebar_is_windows_only_and_has_explicit_window_permissions() {
    let root = repository_root();
    let capability: Value = serde_json::from_slice(
        &fs::read(root.join("src-tauri/capabilities/windows-titlebar.json"))
            .expect("read Windows titlebar capability"),
    )
    .expect("parse Windows titlebar capability");
    let permissions = capability["permissions"]
        .as_array()
        .expect("titlebar permissions");

    assert_eq!(capability["platforms"], serde_json::json!(["windows"]));
    assert_eq!(capability["windows"], serde_json::json!(["main"]));
    for required in [
        "core:window:default",
        "core:window:allow-close",
        "core:window:allow-minimize",
        "core:window:allow-toggle-maximize",
        "core:window:allow-start-dragging",
        "core:event:allow-listen",
        "core:event:allow-unlisten",
    ] {
        assert!(
            permissions.iter().any(|permission| permission == required),
            "Windows titlebar capability is missing `{required}`",
        );
    }

    let host =
        fs::read_to_string(root.join("src-tauri/src/lib.rs")).expect("read Tauri host entrypoint");
    assert!(
        host.contains("#[cfg(target_os = \"windows\")]")
            && host.contains("window.set_decorations(false)?;"),
        "the main window must become frameless only on Windows",
    );
}

#[test]
fn windows_candidate_workflow_creates_only_private_windows_installers() {
    let workflow =
        fs::read_to_string(repository_root().join(".github/workflows/candidate-windows.yml"))
            .expect("read Windows candidate workflow");

    for required in [
        "name: Windows candidate",
        "run-name: Windows candidate ${{ inputs.candidate_id }}",
        "runs-on: windows-2025",
        "releaseDraft: true",
        "prerelease: true",
        "args: --bundles nsis,msi",
    ] {
        assert!(
            workflow.contains(required),
            "Windows candidate workflow is missing `{required}`",
        );
    }
    assert!(
        !workflow.contains("release:preflight"),
        "an unbuilt candidate cannot already possess Public Release Gate authorization",
    );
    assert!(
        !workflow.contains("macos") && !workflow.contains("ubuntu"),
        "Private v0 candidate packaging is Windows-only",
    );
    assert!(
        !workflow.contains("self-hosted") && !workflow.contains("quantix-release"),
        "candidate packaging must use an ephemeral GitHub-hosted runner",
    );
    let build_step = workflow
        .split("- name: Build the private Windows candidate")
        .nth(1)
        .expect("candidate build step");
    let pre_build_steps = workflow
        .split("- name: Build the private Windows candidate")
        .next()
        .expect("steps before candidate build");
    assert!(
        build_step.contains("TAURI_SIGNING_PRIVATE_KEY:"),
        "the candidate build step needs the updater signing key",
    );
    assert!(
        build_step
            .contains("QUANTIX_UPDATE_PUBLIC_KEY: ${{ steps.updater-public-key.outputs.value }}",),
        "release-mode Rust must receive the updater public key committed in Tauri configuration",
    );
    assert!(
        !pre_build_steps.contains("TAURI_SIGNING_PRIVATE_KEY:"),
        "signing secrets must not be exposed to checkout, dependency installation, or verification",
    );
}

#[test]
fn public_release_command_distinguishes_source_manifest_from_windows_binary() {
    let hash = |value: char| value.to_string().repeat(64);
    let command = serde_json::from_value::<EvaluatePublicReleaseGateCommand>(serde_json::json!({
        "release_candidate_manifest_sha256": hash('a'),
        "private_windows_candidate_binary_sha256": hash('b'),
        "private_qualification_sha256": hash('c'),
        "native_platforms": [],
        "license_review": {
            "inventory_sha256": hash('d'),
            "reviewed_categories": [
                "redistributed_binaries",
                "rust_dependencies",
                "typescript_dependencies",
                "python_dependencies",
                "model_assets",
                "templates"
            ],
            "passed": false,
            "findings": [],
            "reviewed_by": "engineer_user",
            "reviewed_at": "2026-08-16T00:00:00Z",
            "expires_at": "2027-08-16T00:00:00Z"
        },
        "codex_production_assurance": {
            "production_supported": false,
            "evidence_reference": "No production assurance is currently available.",
            "evidence_sha256": hash('e'),
            "verified_by": "engineer_user",
            "verified_at": "2026-08-16T00:00:00Z",
            "expires_at": "2027-08-16T00:00:00Z"
        },
        "integration_terms": {
            "third_party_subscription_integration_authorized": false,
            "terms_reference": "No distribution authorization is currently available.",
            "terms_sha256": hash('f'),
            "decided_by": "engineer_user",
            "decided_at": "2026-08-16T00:00:00Z",
            "expires_at": "2027-08-16T00:00:00Z"
        },
        "technical_risks": [],
        "release_artifacts": [{ "name": "candidate", "sha256": hash('1') }],
        "approver": "engineer_user"
    }));

    assert!(
        command.is_ok(),
        "the public release boundary must receive distinct source-manifest and private Windows binary identities: {command:?}",
    );
}
