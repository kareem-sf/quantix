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
            let command: RecordLiveQualificationRunCommand =
                serde_json::from_slice(&fs::read(input)?)?;
            let codex_executable = resource_directory
                .join("runtime")
                .join("bin")
                .join(if cfg!(windows) { "codex.exe" } else { "codex" });
            let login_status = Command::new(codex_executable)
                .args(["login", "status"])
                .output()?;
            if !login_status.status.success() {
                return Err("Codex-managed authentication is not ready".into());
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
