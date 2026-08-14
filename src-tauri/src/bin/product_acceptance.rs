use std::{env, fs, path::PathBuf, process::Command};

use quantix_lib::{
    EvaluatePublicReleaseGateCommand, QuantixHost, RecordLiveQualificationRunCommand,
    RecordNativePlatformQualificationCommand, RunDeterministicAcceptanceCommand,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.as_slice() == ["release-check"] {
        let application_home = env::var("QUANTIX_APPLICATION_HOME")
            .map(PathBuf::from)
            .map_err(|_| "QUANTIX_APPLICATION_HOME is required for release packaging")?;
        let release_candidate = env::var("QUANTIX_RELEASE_CANDIDATE_SHA256")
            .map_err(|_| "QUANTIX_RELEASE_CANDIDATE_SHA256 is required for release packaging")?;
        let resource_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let host = QuantixHost::new(application_home, resource_directory);
        let gate = host.inspect_current_public_release_gate(&release_candidate)?;
        return match gate {
            Some(record) if record.public_production_ready => {
                println!("{}", serde_json_canonicalizer::to_string(&record)?);
                Ok(())
            }
            _ => Err("Public Release Gate is absent, expired, or blocked".into()),
        };
    }
    let [mode, application_home, input] = arguments.as_slice() else {
        return Err("usage: quantix-product-acceptance <deterministic|aggregate|live|private|native|release> <application-home> <command.json|source-revision|release-candidate-sha256>, or release-check with QUANTIX_APPLICATION_HOME and QUANTIX_RELEASE_CANDIDATE_SHA256".into());
    };
    let application_home = PathBuf::from(application_home);
    let resource_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let host = QuantixHost::new(application_home, resource_directory.clone());
    match mode.as_str() {
        "deterministic" => {
            let command: RunDeterministicAcceptanceCommand =
                serde_json::from_slice(&fs::read(input)?)?;
            let run = host.run_deterministic_product_acceptance(command)?;
            println!("{}", serde_json_canonicalizer::to_string(&run)?);
            if run.hard_gate_failures.is_empty() {
                Ok(())
            } else {
                Err("deterministic Product Acceptance hard gate failed".into())
            }
        }
        "aggregate" => {
            let record = host.aggregate_product_acceptance(input)?;
            println!("{}", serde_json_canonicalizer::to_string(&record)?);
            if record.hard_gate_failures.is_empty() {
                Ok(())
            } else {
                Err("aggregate Product Acceptance hard gate failed".into())
            }
        }
        "live" => {
            let mut command: RecordLiveQualificationRunCommand =
                serde_json::from_slice(&fs::read(input)?)?;
            let codex_executable = resource_directory
                .join("runtime")
                .join("bin")
                .join(if cfg!(windows) { "codex.exe" } else { "codex" });
            let login_status = Command::new(&codex_executable)
                .args(["login", "status"])
                .output()?;
            if !login_status.status.success() {
                return Err("Codex-managed authentication is not ready".into());
            }
            let codex_version = Command::new(&codex_executable).arg("--version").output()?;
            if !codex_version.status.success() {
                return Err("Bundled Codex version cannot be measured".into());
            }
            command.codex_version = String::from_utf8(codex_version.stdout)?.trim().into();
            command.platform = exact_windows_platform()?;
            command.fixture_sha256 = quantix_lib::acceptance_fixture_sha256();
            command.oracle_sha256 = quantix_lib::acceptance_oracle_sha256();
            let candidate = command
                .artifacts
                .iter()
                .find(|artifact| artifact.name == "release_candidate")
                .ok_or("live evidence must include the exact release_candidate artifact")?;
            if candidate.sha256 != command.release_candidate_sha256 {
                return Err(
                    "release candidate artifact does not match the qualification sequence".into(),
                );
            }
            let run = host.record_live_qualification_run(command)?;
            println!("{}", serde_json_canonicalizer::to_string(&run)?);
            if run.hard_gate_failures.is_empty() {
                Ok(())
            } else {
                Err("live Qualification hard gate failed".into())
            }
        }
        "private" => {
            let record = host.qualify_private_v0(input)?;
            println!("{}", serde_json_canonicalizer::to_string(&record)?);
            Ok(())
        }
        "native" => {
            let command: RecordNativePlatformQualificationCommand =
                serde_json::from_slice(&fs::read(input)?)?;
            let record = host.record_native_platform_qualification(command)?;
            println!("{}", serde_json_canonicalizer::to_string(&record)?);
            if record.passed {
                Ok(())
            } else {
                Err("Native package Qualification is blocked".into())
            }
        }
        "release" => {
            let command: EvaluatePublicReleaseGateCommand =
                serde_json::from_slice(&fs::read(input)?)?;
            let record = host.evaluate_public_release_gate(command)?;
            println!("{}", serde_json_canonicalizer::to_string(&record)?);
            if record.public_production_ready {
                Ok(())
            } else {
                Err("Public Release Gate is blocked".into())
            }
        }
        _ => Err("unknown acceptance entry point".into()),
    }
}

fn exact_windows_platform() -> Result<String, Box<dyn std::error::Error>> {
    if std::env::consts::OS != "windows" || std::env::consts::ARCH != "x86_64" {
        return Err("private v0 qualification requires Windows x64".into());
    }
    let product_name = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[System.Environment]::OSVersion.Version.ToString()",
        ])
        .output()?;
    let version = String::from_utf8(product_name.stdout)?;
    if !product_name.status.success() || !version.trim().starts_with("10.0.") {
        return Err("Windows 11 version could not be established".into());
    }
    Ok("windows_11_x64".into())
}
