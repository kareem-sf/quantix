use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process,
};

mod submission_generation_fixture;

fn main() {
    if let Err(error) = run() {
        if let Ok(executable) = env::current_exe() {
            let _ = fs::write(
                executable.with_extension("fixture-error"),
                error.to_string(),
            );
        }
        eprintln!("runtime fixture failed: {error}");
        process::exit(29);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let executable = env::current_exe()?;
    let tool = executable
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();

    if arguments.first().and_then(|value| value.to_str()) == Some("--version") {
        let version_delay = executable.with_extension("version-delay");
        if version_delay.is_file() {
            fs::write(executable.with_extension("version-ready"), b"ready")?;
            let milliseconds = fs::read_to_string(version_delay)?.trim().parse()?;
            std::thread::sleep(std::time::Duration::from_millis(milliseconds));
        }
        let version = fs::read_to_string(executable.with_extension("version"))?;
        println!("{tool} {}", version.trim());
        return Ok(());
    }

    if tool.contains("codex") {
        return run_codex(&executable, &arguments);
    }
    if tool == "uv" {
        return run_uv(&executable, &arguments);
    }
    if tool == "python" {
        return run_prepare_models(&arguments);
    }
    if tool.contains("docling") {
        return run_docling(&arguments);
    }
    Err(format!("unrecognized fixture tool {tool}").into())
}

fn run_codex(
    executable: &Path,
    arguments: &[std::ffi::OsString],
) -> Result<(), Box<dyn std::error::Error>> {
    let agent_scenario = executable.with_extension("agent-scenario");
    if agent_scenario.is_file() {
        return run_agent_codex(
            executable,
            arguments,
            fs::read_to_string(agent_scenario)?.trim(),
        );
    }
    let start_count_path = executable.with_extension("probe-start-count");
    let start_count = fs::read_to_string(&start_count_path)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Codex probe start count overflow")?;
    fs::write(&start_count_path, start_count.to_string())?;
    let mut environment = env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .collect::<Vec<_>>();
    environment.sort();
    fs::write(
        executable.with_extension("probe-environment"),
        environment.join("\n"),
    )?;
    let mut requests = io::BufReader::new(io::stdin()).lines();
    let initialize = read_json_request(&mut requests, "initialize")?;
    write_json(&serde_json::json!({
        "id": initialize.get("id").cloned().ok_or("initialize id")?,
        "result": {
            "codexHome": executable.parent().ok_or("missing fixture parent")?,
            "userAgent": "quantix/0.147.0 (fixture; runtime-readiness) (quantix; 0.1.0)",
            "platformFamily": if cfg!(windows) { "windows" } else { "unix" },
            "platformOs": env::consts::OS,
        }
    }))?;
    let initialized = read_json_line(&mut requests)?;
    if initialized
        .get("method")
        .and_then(serde_json::Value::as_str)
        != Some("initialized")
    {
        return Err("missing initialized notification".into());
    }
    loop {
        let Some(account_read) = requests.next() else {
            return Ok(());
        };
        let account_read: serde_json::Value = serde_json::from_str(&account_read?)?;
        if account_read
            .get("method")
            .and_then(serde_json::Value::as_str)
            != Some("account/read")
        {
            return Err("expected account/read request".into());
        }
        if account_read.pointer("/params/refreshToken") != Some(&serde_json::Value::Bool(true)) {
            return Err("Codex readiness did not request a managed-auth refresh".into());
        }
        let account_id = account_read
            .get("id")
            .cloned()
            .ok_or("account request id")?;
        let probe_delay = executable.with_extension("probe-delay");
        if probe_delay.is_file() {
            fs::write(executable.with_extension("probe-ready"), b"ready")?;
            let milliseconds = fs::read_to_string(probe_delay)?.trim().parse()?;
            std::thread::sleep(std::time::Duration::from_millis(milliseconds));
        }
        let auth = fs::read_to_string(executable.with_extension("auth"))?;
        let auth = auth.trim();
        let plan = fs::read_to_string(executable.with_extension("plan"))?;
        let plan = plan.trim();
        let account = match auth {
            "chatgpt" => serde_json::json!({
                "type": "chatgpt",
                "email": null,
                "planType": plan,
            }),
            "none" => serde_json::Value::Null,
            "apikey" => serde_json::json!({ "type": "apiKey" }),
            "malformed" => {
                println!("not-json");
                io::stdout().flush()?;
                return Ok(());
            }
            "mixed" => {
                println!("not-json");
                write_json(&serde_json::json!({
                    "id": account_id,
                    "result": {
                        "account": {
                            "type": "chatgpt",
                            "email": null,
                            "planType": "plus"
                        },
                        "requiresOpenaiAuth": true
                    }
                }))?;
                return Ok(());
            }
            state => return Err(format!("unknown auth fixture state {state}").into()),
        };
        write_json(&serde_json::json!({
            "id": account_id,
            "result": { "account": account, "requiresOpenaiAuth": true }
        }))?;
        if auth != "chatgpt" || !matches!(plan, "go" | "plus") {
            continue;
        }
        let model_list = read_json_request(&mut requests, "model/list")?;
        write_json(&serde_json::json!({
            "id": model_list.get("id").cloned().ok_or("model list request id")?,
            "result": {
                "data": [{
                    "id": "gpt-5.6-terra",
                    "model": "gpt-5.6-terra",
                    "displayName": "GPT-5.6 Terra",
                    "description": "Fixture Codex model",
                    "hidden": false,
                    "defaultReasoningEffort": "medium",
                    "supportedReasoningEfforts": [{
                        "reasoningEffort": "medium",
                        "description": "Fixture reasoning effort"
                    }],
                    "inputModalities": ["text"],
                    "supportsPersonality": true,
                    "isDefault": true
                }],
                "nextCursor": null
            }
        }))?;
    }
}

fn run_agent_codex(
    executable: &Path,
    arguments: &[std::ffi::OsString],
    scenario: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let arguments = arguments
        .iter()
        .filter_map(|value| value.to_str())
        .collect::<Vec<_>>();
    for feature in [
        "apps",
        "browser_use",
        "computer_use",
        "hooks",
        "image_generation",
        "in_app_browser",
        "multi_agent",
        "plugins",
        "shell_tool",
        "unified_exec",
    ] {
        if !arguments
            .windows(2)
            .any(|pair| pair == ["--disable", feature])
        {
            return Err(format!("agent fixture missing disabled feature {feature}").into());
        }
    }
    if !arguments.contains(&"--strict-config")
        || !arguments.contains(&"mcp_servers={}")
        || !arguments.contains(&"web_search=\"disabled\"")
    {
        return Err("agent fixture lacks default-deny provider configuration".into());
    }
    let mut environment = env::vars_os()
        .filter_map(|(name, _)| name.into_string().ok())
        .collect::<Vec<_>>();
    environment.sort();
    fs::write(
        executable.with_extension("agent-environment"),
        environment.join("\n"),
    )?;
    let start_count_path = executable.with_extension("agent-start-count");
    let start_count = fs::read_to_string(&start_count_path)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("agent fixture start count overflow")?;
    fs::write(&start_count_path, start_count.to_string())?;
    let mut requests = io::BufReader::new(io::stdin()).lines();
    let initialize = read_json_request(&mut requests, "initialize")?;
    let initialize_id = initialize.get("id").cloned().ok_or("initialize id")?;
    if scenario == "delayed-readiness" {
        std::thread::sleep(std::time::Duration::from_millis(1_400));
    }
    write_json(&serde_json::json!({
        "id": initialize_id,
        "result": {
            "codexHome": executable.parent().ok_or("missing fixture parent")?,
            "userAgent": if scenario == "unsupported-provider-version" {
                "Codex Desktop/0.146.0 (fixture; agent-runtime) (quantix; 0.1.0)"
            } else {
                "Codex Desktop/0.147.0 (fixture; agent-runtime) (quantix; 0.1.0)"
            },
            "platformFamily": if cfg!(windows) { "windows" } else { "unix" },
            "platformOs": env::consts::OS,
        }
    }))?;
    let initialized = read_json_line(&mut requests)?;
    if initialized
        .get("method")
        .and_then(serde_json::Value::as_str)
        != Some("initialized")
    {
        return Err("missing initialized notification".into());
    }
    write_json(&serde_json::json!({
        "method": "account/updated",
        "params": { "authMode": null, "planType": null }
    }))?;
    let account_read = read_json_request(&mut requests, "account/read")?;
    if account_read.pointer("/params/refreshToken") != Some(&serde_json::Value::Bool(true)) {
        return Err("Codex readiness did not request a managed-auth refresh".into());
    }
    if scenario == "account-auth-error" {
        write_json(&serde_json::json!({
            "id": account_read.get("id").cloned().ok_or("account request id")?,
            "error": {
                "code": -32000,
                "message": "fixture expired access token",
                "data": { "codexErrorInfo": "unauthorized" }
            }
        }))?;
        for request in requests {
            request?;
        }
        return Ok(());
    }
    let account = match scenario {
        "signed-out" => serde_json::Value::Null,
        "api-key" => serde_json::json!({ "type": "apiKey" }),
        _ => serde_json::json!({
            "type": "chatgpt",
            "email": null,
            "planType": "plus"
        }),
    };
    write_json(&serde_json::json!({
        "id": account_read.get("id").cloned().ok_or("account request id")?,
        "result": {
            "account": account,
            "requiresOpenaiAuth": true
        }
    }))?;
    if matches!(scenario, "signed-out" | "api-key") {
        for request in requests {
            request?;
        }
        return Ok(());
    }
    let usable_model = serde_json::json!({
        "id": "gpt-5.6-terra",
        "model": "gpt-5.6-terra",
        "displayName": "GPT-5.6 Terra",
        "description": "Fixture Codex model",
        "hidden": false,
        "defaultReasoningEffort": "medium",
        "supportedReasoningEfforts": [{
            "reasoningEffort": "medium",
            "description": "Fixture reasoning effort"
        }],
        "inputModalities": ["text"],
        "supportsPersonality": true,
        "isDefault": true
    });
    let model_list = read_json_request(&mut requests, "model/list")?;
    let first_page = if matches!(scenario, "missing-capability" | "model-second-page") {
        Vec::new()
    } else {
        vec![usable_model.clone()]
    };
    write_json(&serde_json::json!({
        "id": model_list.get("id").cloned().ok_or("model list request id")?,
        "result": {
            "data": first_page,
            "nextCursor": if scenario == "model-second-page" { Some("models-2") } else { None }
        }
    }))?;
    if scenario == "missing-capability" {
        for request in requests {
            request?;
        }
        return Ok(());
    }
    if scenario == "model-second-page" {
        let second_page = read_json_request(&mut requests, "model/list")?;
        if second_page
            .pointer("/params/cursor")
            .and_then(serde_json::Value::as_str)
            != Some("models-2")
        {
            return Err("Codex model pagination did not use the returned cursor".into());
        }
        write_json(&serde_json::json!({
            "id": second_page.get("id").cloned().ok_or("second model list request id")?,
            "result": { "data": [usable_model], "nextCursor": null }
        }))?;
    }

    let mut thread_count = 0_u32;
    let mut turn_count = 0_u32;
    loop {
        if !run_agent_turn(
            executable,
            &mut requests,
            &mut thread_count,
            &mut turn_count,
        )? {
            return Ok(());
        }
    }
}

fn run_agent_turn(
    executable: &Path,
    requests: &mut impl Iterator<Item = io::Result<String>>,
    thread_count: &mut u32,
    turn_count: &mut u32,
) -> Result<bool, Box<dyn std::error::Error>> {
    let Some(thread_request) = requests.next() else {
        return Ok(false);
    };
    let mut thread_request: serde_json::Value = serde_json::from_str(&thread_request?)?;
    let scenario = fs::read_to_string(executable.with_extension("agent-scenario"))?;
    let scenario = scenario.trim();
    if matches!(
        scenario,
        "production-task-multiplex" | "production-task-multiplex-auto"
    ) {
        return run_multiplexed_production_turns(
            executable,
            requests,
            thread_request,
            thread_count,
            turn_count,
            scenario == "production-task-multiplex-auto",
        );
    }
    if thread_request
        .get("method")
        .and_then(serde_json::Value::as_str)
        == Some("thread/archive")
    {
        let archived_thread = thread_request
            .pointer("/params/threadId")
            .and_then(serde_json::Value::as_str)
            .ok_or("archived thread id")?;
        if matches!(scenario, "archive-failure" | "archive-already-complete") {
            write_json(&serde_json::json!({
                "id": thread_request.get("id").cloned().ok_or("archive request id")?,
                "error": { "code": -32603, "message": "fixture archive failure" }
            }))?;
            let confirmation = read_json_request(requests, "thread/list")?;
            let archived = if scenario == "archive-already-complete" {
                vec![fixture_thread(archived_thread, ".")]
            } else {
                Vec::new()
            };
            write_json(&serde_json::json!({
                "id": confirmation.get("id").cloned().ok_or("archive confirmation id")?,
                "result": {
                    "data": archived,
                    "nextCursor": null,
                    "backwardsCursor": null
                }
            }))?;
            if scenario == "archive-failure" {
                return Ok(false);
            }
            thread_request = read_json_line(requests)?;
        } else {
            write_json(&serde_json::json!({
                "id": thread_request.get("id").cloned().ok_or("archive request id")?,
                "result": {}
            }))?;
            write_json(&serde_json::json!({
                "method": "thread/archived",
                "params": { "threadId": archived_thread }
            }))?;
            thread_request = read_json_line(requests)?;
        }
    }
    let thread_method = thread_request
        .get("method")
        .and_then(serde_json::Value::as_str)
        .ok_or("thread method")?;
    if !matches!(thread_method, "thread/start" | "thread/resume") {
        return Err("expected thread start or resume".into());
    }
    let is_record_scenario = scenario.starts_with("record-extraction")
        || scenario.starts_with("record-review")
        || scenario.starts_with("bid-package-review")
        || scenario.starts_with("external-rfi-review")
        || scenario.starts_with("calculation-rule-review")
        || scenario.starts_with("cost-estimator-calculation")
        || scenario.starts_with("cost-estimator-basis")
        || scenario.starts_with("basis-of-estimate-review")
        || scenario.starts_with("priced-cost-baseline-review")
        || scenario.starts_with("pricing-adjustment-review")
        || scenario.starts_with("production-task");
    let dynamic_tools_are_exact = if is_record_scenario {
        thread_request
            .pointer("/params/dynamicTools")
            .and_then(serde_json::Value::as_array)
            .is_some_and(Vec::is_empty)
    } else {
        thread_request.pointer("/params/dynamicTools/0/name")
            == Some(&serde_json::Value::String(
                "quantix_read_tender_metadata".into(),
            ))
    };
    if thread_method == "thread/start"
        && (thread_request.pointer("/params/sandbox")
            != Some(&serde_json::Value::String("workspaceWrite".into()))
            || thread_request.pointer("/params/approvalPolicy")
                != Some(&serde_json::Value::String("never".into()))
            || !dynamic_tools_are_exact)
    {
        return Err("thread lacks its default-deny sandbox contract".into());
    }
    if scenario == "hang-before-thread" {
        fs::write(executable.with_extension("thread-waiting"), b"waiting")?;
        for request in requests {
            request?;
        }
        return Ok(false);
    }
    if scenario == "malformed-before-turn" {
        println!("not-json");
        io::stdout().flush()?;
        return Ok(false);
    }
    if scenario == "process-failure-before-turn" {
        return Err("fixture provider process failure".into());
    }
    let new_thread_id;
    let thread_id = if let Some(existing) = thread_request
        .pointer("/params/threadId")
        .and_then(serde_json::Value::as_str)
    {
        existing
    } else {
        *thread_count = thread_count
            .checked_add(1)
            .ok_or("fixture thread count overflow")?;
        let sequence = if matches!(scenario, "success-new-thread" | "archive-already-complete") {
            2
        } else {
            *thread_count
        };
        new_thread_id = if scenario == "record-extraction-change-assessment"
            || scenario == "bid-package-review-change-assessment"
        {
            format!("thr_fixture_{}", scenario.replace('-', "_"))
        } else if scenario.starts_with("production-task") {
            format!(
                "thr_fixture_{}_{}",
                scenario.replace('-', "_"),
                thread_count
            )
        } else {
            format!("thr_fixture_{sequence}")
        };
        &new_thread_id
    };
    let cwd = thread_request
        .pointer("/params/cwd")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            env::current_dir()
                .expect("fixture cwd")
                .to_string_lossy()
                .into_owned()
        });
    let thread = fixture_thread(thread_id, &cwd);
    let thread_id_response = thread_request
        .get("id")
        .cloned()
        .ok_or("thread request id")?;
    write_json(&serde_json::json!({
        "id": thread_id_response,
        "result": {
            "approvalPolicy": "never",
            "approvalsReviewer": "user",
            "cwd": cwd.clone(),
            "model": "gpt-5.6-terra",
            "modelProvider": "openai",
            "reasoningEffort": "medium",
            "sandbox": {
                "type": "readOnly",
                "networkAccess": false,
            },
            "thread": thread.clone(),
        }
    }))?;
    write_json(&serde_json::json!({
        "method": "thread/started",
        "params": { "thread": thread }
    }))?;

    let turn_request = read_json_request(requests, "turn/start")?;
    if turn_request.pointer("/params/outputSchema").is_none()
        || turn_request.pointer("/params/sandboxPolicy/type")
            != Some(&serde_json::Value::String("workspaceWrite".into()))
        || turn_request.pointer("/params/sandboxPolicy/networkAccess")
            != Some(&serde_json::Value::Bool(false))
    {
        return Err("turn lacks its exact output or sandbox contract".into());
    }
    let working = Path::new(
        turn_request
            .pointer("/params/cwd")
            .and_then(serde_json::Value::as_str)
            .ok_or("turn cwd")?,
    );
    let workspace = working.parent().ok_or("Agent Run Workspace")?;
    let input = workspace
        .join("inputs")
        .join(if scenario.starts_with("record-extraction") {
            "tender-evidence-v1.json"
        } else if scenario.starts_with("record-review") {
            "tender-record-review-v1.json"
        } else if scenario.starts_with("bid-package-review") {
            "bid-decision-package-review-v1.json"
        } else {
            "tender-metadata-v1.json"
        });
    let output = workspace.join("outputs");
    if working.file_name().and_then(|name| name.to_str()) != Some("working")
        || turn_request.pointer("/params/sandboxPolicy/writableRoots/0")
            != Some(&serde_json::Value::String(
                output.to_string_lossy().into_owned(),
            ))
    {
        return Err("turn escaped its exact Agent Run Workspace roots".into());
    }
    let data_view: serde_json::Value = serde_json::from_slice(&fs::read(&input)?)?;
    let provider_input: serde_json::Value = serde_json::from_str(
        turn_request
            .pointer("/params/input/0/text")
            .and_then(serde_json::Value::as_str)
            .ok_or("provider instruction bundle")?,
    )?;
    let provider_data_view = provider_input
        .pointer("/provider_data_views/0/payload")
        .ok_or("provider-visible Data View payload")?;
    if provider_input
        .pointer("/quantix_invariants")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|invariants| {
            invariants.iter().any(|invariant| {
                invariant
                    .as_str()
                    .is_some_and(|text| text.contains("invoke any tools"))
            })
        })
    {
        return Err("provider instructions prohibit Host-authorized Typed Tools".into());
    }
    if provider_data_view != &data_view {
        return Err("provider-visible Data View differs from its materialized input".into());
    }
    if scenario.starts_with("bid-package-review") {
        let exposes_exact_evidence = provider_data_view
            .get("records")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|records| {
                records.iter().any(|record| {
                    record
                        .get("fields")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|fields| {
                            fields.iter().any(|field| {
                                field
                                    .get("evidence")
                                    .and_then(serde_json::Value::as_array)
                                    .is_some_and(|evidence| {
                                        evidence.iter().any(|item| {
                                            item.pointer("/location/original_text")
                                                .and_then(serde_json::Value::as_str)
                                                .is_some()
                                        })
                                    })
                            })
                        })
                })
            });
        if !exposes_exact_evidence {
            return Err("package review Data View omits exact Evidence".into());
        }
        if provider_data_view
            .pointer("/manifest/resource_implications")
            .and_then(serde_json::Value::as_array)
            .is_none()
        {
            return Err("package review Data View omits resource implications".into());
        }
    }
    let tender_name = provider_data_view
        .pointer("/tender/name")
        .or_else(|| provider_data_view.pointer("/record/title"))
        .or_else(|| provider_data_view.pointer("/manifest/package_id"))
        .or_else(|| provider_data_view.pointer("/external_rfi/rfi_id"))
        .and_then(serde_json::Value::as_str)
        .ok_or("provider-visible Tender name")?
        .to_owned();
    fs::write(
        executable.with_extension("agent-workspace"),
        serde_json::to_vec(&serde_json::json!({
            "workspace": workspace,
            "input_read_only": fs::metadata(&input)?.permissions().readonly(),
            "working_directory": working.is_dir(),
            "output_directory": output.is_dir(),
            "data_view": data_view,
            "provider_data_view": provider_data_view,
        }))?,
    )?;
    *turn_count = turn_count
        .checked_add(1)
        .ok_or("fixture turn count overflow")?;
    let turn_id = format!("turn_fixture_{turn_count}");
    let turn_request_id = turn_request.get("id").cloned().ok_or("turn request id")?;
    let running_turn = serde_json::json!({
        "id": turn_id,
        "status": "inProgress",
        "items": [],
        "error": null,
        "startedAt": 1_780_000_001_i64,
        "completedAt": null,
        "durationMs": null,
    });
    if matches!(
        scenario,
        "turn-start-response-lost" | "hang-after-turn-request"
    ) {
        fs::write(
            executable.with_extension("turn-accepted-without-response"),
            turn_id.as_bytes(),
        )?;
        if scenario == "hang-after-turn-request" {
            for request in requests {
                request?;
            }
            return Ok(false);
        }
        return Err("fixture lost turn/start response after acceptance".into());
    }
    write_json(&serde_json::json!({
        "id": turn_request_id,
        "result": { "turn": running_turn }
    }))?;
    write_json(&serde_json::json!({
        "method": "turn/started",
        "params": { "threadId": thread_id, "turn": running_turn }
    }))?;
    if matches!(
        scenario,
        "auth-notification-loss" | "subscription-notification-loss"
    ) {
        write_json(&serde_json::json!({
            "method": "account/updated",
            "params": if scenario == "auth-notification-loss" {
                serde_json::json!({ "authMode": null, "planType": null })
            } else {
                serde_json::json!({ "authMode": "chatgpt", "planType": null })
            }
        }))?;
        for request in requests {
            request?;
        }
        return Ok(false);
    }
    if matches!(
        scenario,
        "rate-limit-update-success" | "rate-limited" | "usage-stream"
    ) {
        write_json(&serde_json::json!({
            "method": "account/rateLimits/updated",
            "params": {
                "rateLimits": {
                    "limitId": "codex",
                    "limitName": null,
                    "planType": "plus",
                    "primary": {
                        "usedPercent": 100,
                        "windowDurationMins": 15,
                        "resetsAt": 1_780_000_900_i64
                    },
                    "secondary": null,
                    "credits": null,
                    "individualLimit": null,
                    "rateLimitReachedType": "rate_limit_reached",
                    "spendControlReached": false
                }
            }
        }))?;
        write_json(&serde_json::json!({
            "method": "account/rateLimits/updated",
            "params": {
                "rateLimits": {
                    "primary": { "usedPercent": 100 },
                    "secondary": null,
                    "rateLimitReachedType": null,
                    "spendControlReached": null
                }
            }
        }))?;
    }

    if scenario == "hang" {
        fs::write(executable.with_extension("turn-waiting"), b"waiting")?;
        for request in requests {
            request?;
        }
        return Ok(false);
    }
    if scenario.ends_with("malformed-after-turn") {
        println!("not-json");
        io::stdout().flush()?;
        return Ok(false);
    }
    if scenario == "rate-limited" {
        let error = serde_json::json!({
            "message": "fixture usage limit",
            "codexErrorInfo": "usageLimitExceeded"
        });
        write_json(&serde_json::json!({
            "method": "error",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "willRetry": false,
                "error": error.clone()
            }
        }))?;
        write_json(&serde_json::json!({
            "method": "turn/completed",
            "params": {
                "threadId": thread_id,
                "turn": {
                    "id": turn_id,
                    "status": "failed",
                    "items": [],
                    "error": error,
                    "startedAt": 1_780_000_001_i64,
                    "completedAt": 1_780_000_002_i64,
                    "durationMs": 1000
                }
            }
        }))?;
        for request in requests {
            request?;
        }
        return Ok(false);
    }
    if scenario == "auth-loss" {
        let error = serde_json::json!({
            "message": "fixture authentication expired",
            "codexErrorInfo": "unauthorized"
        });
        write_json(&serde_json::json!({
            "method": "error",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "willRetry": false,
                "error": error.clone()
            }
        }))?;
        write_json(&serde_json::json!({
            "method": "turn/completed",
            "params": {
                "threadId": thread_id,
                "turn": {
                    "id": turn_id,
                    "status": "failed",
                    "items": [],
                    "error": error,
                    "startedAt": 1_780_000_001_i64,
                    "completedAt": 1_780_000_002_i64,
                    "durationMs": 1000
                }
            }
        }))?;
        for request in requests {
            request?;
        }
        return Ok(false);
    }
    if scenario == "interrupt" {
        let interrupt = read_json_request(requests, "turn/interrupt")?;
        if interrupt
            .pointer("/params/threadId")
            .and_then(serde_json::Value::as_str)
            != Some(thread_id)
            || interrupt
                .pointer("/params/turnId")
                .and_then(serde_json::Value::as_str)
                != Some(turn_id.as_str())
        {
            return Err("interrupt targeted the wrong Provider Turn".into());
        }
        write_json(&serde_json::json!({
            "id": interrupt.get("id").cloned().ok_or("interrupt id")?,
            "result": {}
        }))?;
        write_json(&serde_json::json!({
            "method": "turn/completed",
            "params": {
                "threadId": thread_id,
                "turn": {
                    "id": turn_id,
                    "status": "interrupted",
                    "items": [],
                    "error": null,
                    "startedAt": 1_780_000_001_i64,
                    "completedAt": 1_780_000_002_i64,
                    "durationMs": 1000
                }
            }
        }))?;
        for request in requests {
            request?;
        }
        return Ok(false);
    }
    if !matches!(
        scenario,
        "success"
            | "success-retry"
            | "success-new-thread"
            | "archive-already-complete"
            | "access-tool"
            | "access-tool-revoked"
            | "hostile-control-requests"
            | "deny-then-hang"
            | "complete-after-interrupt"
            | "delayed-readiness"
            | "phase-null-final"
            | "retry-then-success"
            | "rate-limit-update-success"
            | "model-second-page"
            | "usage-stream"
            | "output-invalid"
            | "record-extraction"
            | "record-extraction-submission-generation"
            | "record-extraction-submission-generation-empty-material"
            | "record-extraction-submission-generation-unsupported"
            | "record-extraction-submission-generation-path-collision"
            | "record-extraction-coordinated"
            | "record-extraction-coordinated-split"
            | "record-extraction-coordinated-transitive-split"
            | "record-extraction-coordination-many"
            | "record-extraction-coordination-overflow"
            | "record-extraction-decline-risk"
            | "record-extraction-expanded"
            | "record-extraction-extra-characteristic"
            | "record-extraction-inventory-fill"
            | "record-extraction-inventory-overflow"
            | "record-extraction-delayed"
            | "record-extraction-change-assessment"
            | "record-extraction-duplicate-citation"
            | "record-extraction-invalid"
            | "record-review"
            | "record-review-delayed"
            | "bid-package-review"
            | "bid-package-review-change-assessment"
            | "bid-package-review-failed"
            | "bid-package-review-delayed"
            | "external-rfi-review"
            | "external-rfi-review-failed"
            | "external-rfi-review-delayed"
            | "calculation-rule-review"
            | "calculation-rule-review-failed"
            | "cost-estimator-calculation"
            | "cost-estimator-calculation-zero"
            | "cost-estimator-calculation-missing"
            | "cost-estimator-calculation-ambiguous"
            | "cost-estimator-calculation-unavailable"
            | "cost-estimator-calculation-invalid"
            | "cost-estimator-calculation-delayed"
            | "cost-estimator-basis"
            | "cost-estimator-basis-delayed"
            | "cost-estimator-basis-unresolved-query-delayed"
            | "cost-estimator-basis-incomplete"
            | "cost-estimator-basis-quote-scope-mismatch"
            | "cost-estimator-basis-unresolved-query"
            | "cost-estimator-basis-reconciliation-mismatch"
            | "cost-estimator-basis-missing-rate"
            | "cost-estimator-basis-allowance"
            | "cost-estimator-basis-uncontrolled-allowance"
            | "basis-of-estimate-review"
            | "basis-of-estimate-review-failed"
            | "priced-cost-baseline-review"
            | "priced-cost-baseline-review-failed"
            | "pricing-adjustment-review"
            | "pricing-adjustment-review-failed"
            | "production-task"
            | "production-task-coordination-cost-conflict"
            | "production-task-coordination-commitment-conflict"
            | "production-task-coordination-date-conflict"
            | "production-task-coordination-date-source-responsibility-conflict"
            | "production-task-coordination-missing"
            | "production-task-coordination-responsibility-conflict"
            | "production-task-coordination-wrong-key"
            | "production-task-delayed-a"
            | "production-task-delayed-b"
            | "production-task-delayed-review"
            | "production-task-evidence-invalid"
            | "production-task-output-over-budget"
            | "production-task-query-proposal"
            | "production-task-review-rework"
            | "production-task-review-critical"
            | "production-task-review-critical-repeat"
            | "production-task-review-major"
            | "production-task-review-minor"
    ) {
        return Err(format!("unknown agent fixture scenario {scenario}").into());
    }
    let started_item = serde_json::json!({
        "id": "message_fixture_1",
        "type": "agentMessage",
        "text": "",
        "phase": null
    });
    write_json(&serde_json::json!({
        "method": "item/started",
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "startedAtMs": 1_780_000_001_000_i64,
            "item": started_item
        }
    }))?;

    if scenario == "complete-after-interrupt" {
        fs::write(
            executable.with_extension("completion-race-waiting"),
            b"waiting",
        )?;
        let interrupt = read_json_request(requests, "turn/interrupt")?;
        write_json(&serde_json::json!({
            "id": interrupt.get("id").cloned().ok_or("interrupt id")?,
            "result": {}
        }))?;
    }
    if scenario == "delayed-readiness" {
        std::thread::sleep(std::time::Duration::from_millis(7_000));
    }
    write_json(&serde_json::json!({
        "method": "item/agentMessage/delta",
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "itemId": "message_fixture_1",
            "delta": "streamed-delta-must-not-be-canonical"
        }
    }))?;
    write_json(&serde_json::json!({
        "method": "item/reasoning/textDelta",
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "itemId": "reasoning_fixture_1",
            "contentIndex": 0,
            "delta": "hidden-reasoning-must-not-be-canonical"
        }
    }))?;
    if scenario == "retry-then-success" {
        write_json(&serde_json::json!({
            "method": "error",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "willRetry": true,
                "error": {
                    "message": "fixture transient provider error",
                    "codexErrorInfo": null
                }
            }
        }))?;
    }
    write_json(&serde_json::json!({
        "method": "thread/tokenUsage/updated",
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "tokenUsage": {
                "last": {
                    "inputTokens": 120,
                    "cachedInputTokens": 20,
                    "outputTokens": 35,
                    "reasoningOutputTokens": 10,
                    "totalTokens": 155
                },
                "total": {
                    "inputTokens": 120,
                    "cachedInputTokens": 20,
                    "outputTokens": 35,
                    "reasoningOutputTokens": 10,
                    "totalTokens": 155
                },
                "modelContextWindow": 200000
            }
        }
    }))?;
    if scenario == "usage-stream" {
        let interrupt = read_json_request(requests, "turn/interrupt")?;
        if interrupt
            .pointer("/params/threadId")
            .and_then(serde_json::Value::as_str)
            != Some(thread_id)
            || interrupt
                .pointer("/params/turnId")
                .and_then(serde_json::Value::as_str)
                != Some(turn_id.as_str())
        {
            return Err("streaming fixture interruption targeted the wrong Provider Turn".into());
        }
        write_json(&serde_json::json!({
            "id": interrupt.get("id").cloned().ok_or("interrupt id")?,
            "result": {}
        }))?;
        write_json(&serde_json::json!({
            "method": "turn/completed",
            "params": {
                "threadId": thread_id,
                "turn": {
                    "id": turn_id,
                    "status": "interrupted",
                    "items": [],
                    "error": null,
                    "startedAt": 1_780_000_001_i64,
                    "completedAt": 1_780_000_002_i64,
                    "durationMs": 1000
                }
            }
        }))?;
        for request in requests {
            request?;
        }
        return Ok(false);
    }
    if matches!(scenario, "access-tool" | "access-tool-revoked") {
        let tool_request = |id: &str, arguments: serde_json::Value| {
            serde_json::json!({
                "id": id,
                "method": "item/tool/call",
                "params": {
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "callId": id,
                    "namespace": "quantix",
                    "tool": "quantix_read_tender_metadata",
                    "arguments": arguments
                }
            })
        };
        write_json(&tool_request(
            "access_tool_before_approval",
            serde_json::json!({}),
        ))?;
        let denied = read_json_line(requests)?;
        if denied.pointer("/result/success") != Some(&serde_json::Value::Bool(false)) {
            return Err("Typed Tool was usable before EITL approval".into());
        }
        fs::write(executable.with_extension("access-tool-waiting"), b"waiting")?;
        for _ in 0..500 {
            if executable.with_extension("access-approved").is_file() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !executable.with_extension("access-approved").is_file() {
            return Err("timed out waiting for EITL Access Approval".into());
        }
        if scenario == "access-tool" {
            write_json(&tool_request(
                "access_tool_invalid_arguments",
                serde_json::json!({ "unexpected": true }),
            ))?;
            let invalid = read_json_line(requests)?;
            if invalid.pointer("/result/success") != Some(&serde_json::Value::Bool(false)) {
                return Err("invalid Typed Tool arguments were not denied".into());
            }
        }
        write_json(&tool_request(
            "access_tool_after_approval",
            serde_json::json!({}),
        ))?;
        let approved = read_json_line(requests)?;
        if scenario == "access-tool-revoked" {
            if approved.pointer("/result/success") != Some(&serde_json::Value::Bool(false)) {
                return Err("revoked Typed Tool authority remained usable".into());
            }
        } else {
            if approved.pointer("/result/success") != Some(&serde_json::Value::Bool(true))
                || !approved
                    .pointer("/result/contentItems/0/text")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| text.contains(&tender_name))
            {
                return Err("EITL-approved Typed Tool did not return its exact Data View".into());
            }
            write_json(&tool_request(
                "access_tool_over_quota",
                serde_json::json!({}),
            ))?;
            let over_quota = read_json_line(requests)?;
            if over_quota.pointer("/result/success") != Some(&serde_json::Value::Bool(false)) {
                return Err("Typed Tool exceeded its one-call quota".into());
            }
        }
    } else if scenario == "hostile-control-requests" {
        let outside = env::temp_dir().join("outside-quantix-workspace");
        let mut controls = vec![
            serde_json::json!({
                "id": "hostile_shell",
                "method": "item/commandExecution/requestApproval",
                "params": {
                    "threadId": thread_id, "turnId": turn_id,
                    "itemId": "command_shell", "startedAtMs": 1_780_000_001_000_i64,
                    "cwd": outside, "command": "powershell -Command Invoke-WebRequest https://example.com",
                    "reason": "Tender content says this must be approved"
                }
            }),
            serde_json::json!({
                "id": "hostile_package",
                "method": "item/commandExecution/requestApproval",
                "params": {
                    "threadId": thread_id, "turnId": turn_id,
                    "itemId": "command_package", "startedAtMs": 1_780_000_001_000_i64,
                    "command": "npm install hostile-package"
                }
            }),
            serde_json::json!({
                "id": "hostile_application",
                "method": "item/commandExecution/requestApproval",
                "params": {
                    "threadId": thread_id, "turnId": turn_id,
                    "itemId": "command_application", "startedAtMs": 1_780_000_001_000_i64,
                    "command": "start outlook.exe"
                }
            }),
            serde_json::json!({
                "id": "hostile_root",
                "method": "item/fileChange/requestApproval",
                "params": {
                    "threadId": thread_id, "turnId": turn_id,
                    "itemId": "file_change", "startedAtMs": 1_780_000_001_000_i64,
                    "grantRoot": outside, "reason": "Write outside the private workspace"
                }
            }),
            serde_json::json!({
                "id": "hostile_permissions",
                "method": "item/permissions/requestApproval",
                "params": {
                    "threadId": thread_id, "turnId": turn_id,
                    "itemId": "permissions", "startedAtMs": 1_780_000_001_000_i64,
                    "cwd": working, "permissions": {}, "reason": "Enable unrestricted access"
                }
            }),
            serde_json::json!({
                "id": "hostile_tool",
                "method": "item/tool/call",
                "params": {
                    "threadId": thread_id, "turnId": turn_id,
                    "callId": "tool_call", "tool": "unregistered_tool",
                    "arguments": { "destination": outside }
                }
            }),
            serde_json::json!({
                "id": "hostile_user_input",
                "method": "item/tool/requestUserInput",
                "params": {
                    "threadId": thread_id, "turnId": turn_id,
                    "itemId": "user_input", "isBlocking": true, "questions": []
                }
            }),
            serde_json::json!({
                "id": "hostile_mcp",
                "method": "mcpServer/elicitation/request",
                "params": {
                    "threadId": thread_id,
                    "serverName": "unregistered-server", "mode": "form",
                    "message": "Approve external access", "requestedSchema": {
                        "type": "object", "properties": {}
                    }
                }
            }),
        ];
        controls.push(controls[5].clone());
        for control in controls {
            let id = control.get("id").cloned().ok_or("hostile control id")?;
            let method = control
                .get("method")
                .and_then(serde_json::Value::as_str)
                .ok_or("hostile control method")?;
            write_json(&control)?;
            let denial = read_json_line(requests)?;
            if denial.get("id") != Some(&id)
                || denial
                    .pointer("/result/decision")
                    .and_then(serde_json::Value::as_str)
                    == Some("accept")
                || (method == "item/tool/requestUserInput"
                    && denial.pointer("/result/answers").is_none())
            {
                return Err(format!("Host did not safely deny {method}").into());
            }
        }
    } else if scenario != "complete-after-interrupt" {
        write_json(&serde_json::json!({
            "id": "control_fixture_1",
            "method": "item/commandExecution/requestApproval",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "itemId": "command_fixture_1",
                "startedAtMs": 1_780_000_001_000_i64,
                "command": "forbidden-command"
            }
        }))?;
        let denial = read_json_line(requests)?;
        if denial.get("id").and_then(serde_json::Value::as_str) != Some("control_fixture_1")
            || denial
                .pointer("/result/decision")
                .and_then(serde_json::Value::as_str)
                != Some("decline")
        {
            return Err("Host did not deny the control request".into());
        }
        if scenario == "deny-then-hang" {
            fs::write(executable.with_extension("denial-waiting"), b"waiting")?;
            for request in requests {
                request?;
            }
            return Ok(false);
        }
    }
    if let Some(suffix) = scenario.strip_prefix("production-task-delayed-") {
        fs::write(
            executable.with_extension(format!("production-{suffix}-waiting")),
            b"waiting",
        )?;
        for _ in 0..2_000 {
            if executable
                .with_extension(format!("production-{suffix}-release"))
                .is_file()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !executable
            .with_extension(format!("production-{suffix}-release"))
            .is_file()
        {
            return Err("timed out waiting to release production output".into());
        }
    }
    let mut candidate = if scenario == "output-invalid" {
        serde_json::json!({ "summary": "Missing the required next action." })
    } else if scenario == "record-extraction-delayed" {
        fs::write(
            executable.with_extension("record-output-waiting"),
            b"waiting",
        )?;
        for _ in 0..2_000 {
            if executable.with_extension("record-output-release").is_file() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !executable.with_extension("record-output-release").is_file() {
            return Err("timed out waiting to release Tender Record output".into());
        }
        record_extraction_candidate(provider_data_view)?
    } else if scenario == "record-extraction-decline-risk" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        let risk = candidate["records"]
            .as_array_mut()
            .and_then(|records| {
                records.iter_mut().find(|record| {
                    record.get("stable_key").and_then(serde_json::Value::as_str)
                        == Some("programme_pressure")
                })
            })
            .ok_or("fixture risk record")?;
        let evidence = risk["fields"][0]["evidence"].clone();
        risk["fields"]
            .as_array_mut()
            .ok_or("fixture risk fields")?
            .push(serde_json::json!({
                "name": "bid_recommendation",
                "value": "decline",
                "basis_kind": "evidence",
                "basis_reference": null,
                "basis_description": null,
                "original_expression": null,
                "normalized_value": null,
                "timezone": null,
                "uncertainty": null,
                "evidence": evidence
            }));
        candidate
    } else if scenario == "record-extraction-extra-characteristic" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        let mut added = candidate["records"]
            .as_array()
            .and_then(|records| {
                records.iter().find(|record| {
                    record.get("stable_key").and_then(serde_json::Value::as_str)
                        == Some("project_delivery_context")
                })
            })
            .cloned()
            .ok_or("Project Characteristic candidate")?;
        added["stable_key"] = serde_json::json!("late_project_characteristic");
        added["title"] = serde_json::json!("Late verified Project Characteristic");
        candidate["records"] = serde_json::json!([added]);
        candidate
    } else if scenario == "record-extraction-coordination-many" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        let template = candidate["records"]
            .as_array()
            .and_then(|records| {
                records.iter().find(|record| {
                    record.get("stable_key").and_then(serde_json::Value::as_str)
                        == Some("project_delivery_context")
                })
            })
            .cloned()
            .ok_or("Project Characteristic coordination template")?;
        let additions = (0..40).map(|index| {
            let mut record = template.clone();
            record["stable_key"] =
                serde_json::json!(format!("coordination_characteristic_{index:02}"));
            record["title"] = serde_json::json!(format!("Coordination characteristic {index:02}"));
            record
        });
        candidate["records"]
            .as_array_mut()
            .ok_or("Tender Record candidates")?
            .extend(additions);
        candidate
    } else if scenario == "record-extraction-coordination-overflow" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        let template = candidate["records"]
            .as_array()
            .and_then(|records| {
                records.iter().find(|record| {
                    record.get("stable_key").and_then(serde_json::Value::as_str)
                        == Some("project_delivery_context")
                })
            })
            .cloned()
            .ok_or("Project Characteristic coordination template")?;
        let additions = (0..56).map(|index| {
            let mut record = template.clone();
            record["stable_key"] =
                serde_json::json!(format!("large_coordination_characteristic_{index:02}"));
            record["title"] =
                serde_json::json!(format!("Large coordination characteristic {index:02}"));
            record["fields"][0]["value"] = serde_json::json!("v".repeat(4_000));
            record
        });
        candidate["records"]
            .as_array_mut()
            .ok_or("Tender Record candidates")?
            .extend(additions);
        candidate
    } else if scenario == "record-extraction-inventory-fill" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        let template = candidate["records"][0].clone();
        candidate["records"] = serde_json::Value::Array(
            (0..255)
                .map(|index| {
                    let mut record = template.clone();
                    record["stable_key"] =
                        serde_json::json!(format!("decision_inventory_{index:03}"));
                    record["title"] =
                        serde_json::json!(format!("Decision inventory record {index:03}"));
                    record
                })
                .collect(),
        );
        candidate
    } else if scenario == "record-extraction-inventory-overflow" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        let template = candidate["records"][0].clone();
        candidate["records"] = serde_json::Value::Array(
            [
                "decision_inventory_overflow_a",
                "decision_inventory_overflow_b",
            ]
            .into_iter()
            .map(|stable_key| {
                let mut record = template.clone();
                record["stable_key"] = serde_json::json!(stable_key);
                record["title"] = serde_json::json!(format!("Rejected {stable_key}"));
                record
            })
            .collect(),
        );
        candidate
    } else if scenario == "record-extraction-expanded" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        let mut added = candidate["records"][0].clone();
        added["stable_key"] = serde_json::json!("late_submission_obligation");
        added["title"] = serde_json::json!("Late discovered submission obligation");
        candidate["records"] = serde_json::json!([added]);
        candidate
    } else if scenario == "record-extraction-change-assessment" {
        let authoritative = provider_data_view
            .pointer("/evidence/0/reference")
            .cloned()
            .ok_or("change assessment replacement Evidence")?;
        let allowed = provider_data_view
            .pointer("/change_assessment/allowed_stable_keys")
            .and_then(serde_json::Value::as_array)
            .ok_or("change assessment stable-key contract")?
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<std::collections::HashSet<_>>();
        let mut records = provider_data_view
            .pointer("/change_assessment/prior_records")
            .and_then(serde_json::Value::as_array)
            .ok_or("change assessment prior records")?
            .clone();
        records
            .iter_mut()
            .flat_map(|record| {
                record
                    .get_mut("fields")
                    .and_then(serde_json::Value::as_array_mut)
                    .into_iter()
                    .flatten()
            })
            .for_each(|field| field["evidence"] = serde_json::json!([authoritative.clone()]));
        for record in &mut records {
            record["contradictions"] = serde_json::json!([]);
        }
        records.retain(|record| {
            record
                .get("stable_key")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|stable_key| allowed.contains(stable_key))
        });
        serde_json::json!({"records": records})
    } else if matches!(
        scenario,
        "record-extraction-coordinated"
            | "record-extraction-coordinated-split"
            | "record-extraction-coordinated-transitive-split"
    ) {
        let changed_branch = match scenario {
            "record-extraction-coordinated-split" => Some("programme_pressure"),
            "record-extraction-coordinated-transitive-split" => Some("project_delivery_context"),
            _ => None,
        };
        coordinated_record_extraction_candidate(provider_data_view, changed_branch)?
    } else if scenario.starts_with("record-extraction-submission-generation") {
        submission_generation_fixture::candidate(provider_data_view, scenario)?
    } else if scenario == "record-extraction" {
        record_extraction_candidate(provider_data_view)?
    } else if scenario == "record-extraction-invalid" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        candidate["records"][0]["fields"][0]["evidence"][0]["ordinal"] = serde_json::json!(999_999);
        candidate
    } else if scenario == "record-extraction-duplicate-citation" {
        let mut candidate = record_extraction_candidate(provider_data_view)?;
        let first = candidate["records"][2]["contradictions"][0]["evidence"][0].clone();
        candidate["records"][2]["contradictions"][0]["evidence"][1] = first;
        candidate
    } else if scenario == "record-review-delayed" {
        fs::write(
            executable.with_extension("record-review-waiting"),
            b"waiting",
        )?;
        for _ in 0..2_000 {
            if executable.with_extension("record-review-release").is_file() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !executable.with_extension("record-review-release").is_file() {
            return Err("timed out waiting to release Tender Record review".into());
        }
        serde_json::json!({
            "outcome": "verified",
            "rationale": "Every material field resolves to the exact supplied authoritative Evidence."
        })
    } else if scenario == "record-review" {
        serde_json::json!({
            "outcome": "verified",
            "rationale": "Every material field resolves to the exact supplied authoritative Evidence."
        })
    } else if scenario == "bid-package-review-delayed" {
        fs::write(
            executable.with_extension("bid-package-review-waiting"),
            b"waiting",
        )?;
        for _ in 0..2_000 {
            if executable
                .with_extension("bid-package-review-release")
                .is_file()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !executable
            .with_extension("bid-package-review-release")
            .is_file()
        {
            return Err("timed out waiting to release Bid Decision Package review".into());
        }
        serde_json::json!({ "outcome": "passed", "findings": [] })
    } else if scenario == "bid-package-review-failed" {
        let affected = provider_data_view
            .get("record_summaries")
            .and_then(serde_json::Value::as_array)
            .and_then(|records| records.first())
            .map(|record| {
                serde_json::json!({
                    "record_id": record["record_id"],
                    "version": record["version"]
                })
            })
            .into_iter()
            .collect::<Vec<_>>();
        serde_json::json!({
            "outcome": "failed",
            "findings": [{
                "severity": "critical",
                "code": "critical_compliance_gap",
                "summary": "The exact package does not establish a safe decision basis.",
                "affected_records": affected
            }]
        })
    } else if matches!(
        scenario,
        "bid-package-review" | "bid-package-review-change-assessment"
    ) {
        serde_json::json!({ "outcome": "passed", "findings": [] })
    } else if scenario == "external-rfi-review-delayed" {
        fs::write(
            executable.with_extension("external-rfi-review-waiting"),
            b"waiting",
        )?;
        for _ in 0..2_000 {
            if executable
                .with_extension("external-rfi-review-release")
                .is_file()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !executable
            .with_extension("external-rfi-review-release")
            .is_file()
        {
            return Err("timed out waiting to release External RFI review".into());
        }
        serde_json::json!({ "outcome": "passed", "findings": [] })
    } else if scenario == "external-rfi-review-failed" {
        let evidence_reference = provider_data_view
            .pointer("/external_rfi/source_evidence/0")
            .or_else(|| provider_data_view.pointer("/external_rfi/attachments/0"))
            .cloned()
            .ok_or("External RFI review Evidence")?;
        serde_json::json!({
            "outcome": "failed",
            "findings": [{
                "severity": "major",
                "code": "recipient_context_incomplete",
                "summary": "The exact draft does not establish enough contractual context for external issue.",
                "evidence_references": [evidence_reference]
            }]
        })
    } else if scenario == "external-rfi-review" {
        serde_json::json!({ "outcome": "passed", "findings": [] })
    } else if scenario == "calculation-rule-review-failed" {
        serde_json::json!({
            "outcome": "failed",
            "findings": [{
                "code": "rounding_policy_ambiguous",
                "summary": "The exact rule does not establish one reproducible rounding boundary."
            }]
        })
    } else if scenario == "calculation-rule-review" {
        serde_json::json!({ "outcome": "passed", "findings": [] })
    } else if scenario.starts_with("cost-estimator-calculation") {
        if scenario == "cost-estimator-calculation-delayed" {
            fs::write(
                executable.with_extension("cost-estimator-output-waiting"),
                b"waiting",
            )?;
            for _ in 0..2_000 {
                if executable
                    .with_extension("cost-estimator-output-release")
                    .is_file()
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if !executable
                .with_extension("cost-estimator-output-release")
                .is_file()
            {
                return Err("timed out waiting to release Cost Estimator output".into());
            }
        }
        let quantity_evidence = provider_data_view
            .get("quantity_evidence")
            .and_then(serde_json::Value::as_array)
            .ok_or("Cost Estimator quantity Evidence")?
            .iter()
            .map(|value| {
                value
                    .get("reference")
                    .cloned()
                    .ok_or("quantity Evidence reference")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let unit_rate_evidence = provider_data_view
            .get("unit_rate_evidence")
            .and_then(serde_json::Value::as_array)
            .ok_or("Cost Estimator unit-rate Evidence")?
            .iter()
            .map(|value| {
                value
                    .get("reference")
                    .cloned()
                    .ok_or("unit-rate Evidence reference")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let quantity = match scenario {
            "cost-estimator-calculation-missing" => {
                serde_json::json!({ "state": "missing", "value": null, "evidence": [] })
            }
            "cost-estimator-calculation-ambiguous" => {
                serde_json::json!({ "state": "ambiguous", "value": null, "evidence": quantity_evidence })
            }
            "cost-estimator-calculation-unavailable" => {
                serde_json::json!({ "state": "unavailable", "value": null, "evidence": quantity_evidence })
            }
            "cost-estimator-calculation-invalid" => {
                serde_json::json!({ "state": "provided", "value": "not-a-decimal", "evidence": quantity_evidence })
            }
            _ => serde_json::json!({
                "state": "provided",
                "value": if scenario == "cost-estimator-calculation-zero" { "0" } else { "1250" },
                "evidence": quantity_evidence
            }),
        };
        serde_json::json!({
            "quantity": quantity,
            "unit_rate": {
                "state": "provided",
                "value": "2.40",
                "evidence": unit_rate_evidence
            }
        })
    } else if scenario.starts_with("cost-estimator-basis") {
        if matches!(
            scenario,
            "cost-estimator-basis-delayed" | "cost-estimator-basis-unresolved-query-delayed"
        ) {
            fs::write(
                executable.with_extension("cost-estimator-basis-waiting"),
                b"waiting",
            )?;
            for _ in 0..2_000 {
                if executable
                    .with_extension("cost-estimator-basis-release")
                    .is_file()
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if !executable
                .with_extension("cost-estimator-basis-release")
                .is_file()
            {
                return Err("timed out waiting to release Cost Estimator Basis output".into());
            }
        }
        let boq_row = provider_data_view
            .pointer("/boq_rows/0")
            .ok_or("Basis BOQ row")?;
        let boq_row_key = boq_row.get("row_key").cloned().ok_or("Basis BOQ row key")?;
        let boq_references = boq_row
            .get("evidence")
            .and_then(serde_json::Value::as_array)
            .ok_or("Basis BOQ row Evidence")?
            .iter()
            .map(|value| {
                value
                    .get("reference")
                    .cloned()
                    .ok_or("Basis BOQ Evidence reference")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let quote_reference = provider_data_view
            .pointer("/quotation_evidence/0/reference")
            .cloned()
            .ok_or("Basis quotation Evidence reference")?;
        let evidence_value = |reference: &serde_json::Value| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let kind = reference.get("kind").and_then(serde_json::Value::as_str).ok_or("Evidence kind")?;
            if kind != "source_evidence" {
                return Err("Basis input is not source Evidence".into());
            }
            let exact = reference.get("reference").and_then(serde_json::Value::as_str).ok_or("Evidence reference")?;
            let (artifact_id, ordinal) = exact.rsplit_once('#').ok_or("Evidence ordinal")?;
            Ok(serde_json::json!({
                "artifact_id": artifact_id,
                "version": reference.get("version").cloned().ok_or("Evidence version")?,
                "ordinal": ordinal.parse::<u32>()?
            }))
        };
        let calculations = provider_data_view
            .get("approved_calculation_runs")
            .and_then(serde_json::Value::as_array)
            .ok_or("approved Calculation Runs")?;
        let calculation = calculations
            .iter()
            .find(|run| {
                run.get("description")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|description| {
                        description.to_ascii_lowercase().contains("boq-001")
                            && !description.to_ascii_lowercase().contains("quotation")
                            && !description.to_ascii_lowercase().contains("total")
                    })
            })
            .or_else(|| calculations.first())
            .ok_or("approved Calculation Run")?;
        let calculation_run_id = calculation
            .get("calculation_run_id")
            .cloned()
            .ok_or("Calculation Run id")?;
        let quote_calculation_run_id = calculations
            .iter()
            .find(|run| {
                run.get("description")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|description| {
                        description.to_ascii_lowercase().contains("quotation")
                    })
            })
            .or(if scenario == "cost-estimator-basis-missing-rate" {
                Some(calculation)
            } else {
                None
            })
            .unwrap_or(calculation)
            .get("calculation_run_id")
            .cloned()
            .ok_or("quotation normalization Calculation Run id")?;
        let comparison_total_calculation_run_id = calculations
            .iter()
            .find(|run| {
                run.get("description")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|description| description.to_ascii_lowercase().contains("total"))
            })
            .or_else(|| calculations.last())
            .and_then(|run| run.get("calculation_run_id"))
            .cloned()
            .ok_or("comparison total Calculation Run id")?;
        let tender_queries = provider_data_view
            .get("tender_queries")
            .and_then(serde_json::Value::as_array)
            .ok_or("material assumption Queries")?;
        let query_reference =
            |entry: &serde_json::Value| -> Result<serde_json::Value, Box<dyn std::error::Error>> {
                let query = entry.get("query").ok_or("Query")?;
                Ok(serde_json::json!({
                    "query_id": query.get("query_id").cloned().ok_or("Query id")?,
                    "version": query.get("version").cloned().ok_or("Query version")?
                }))
            };
        let approved_assumptions = tender_queries
            .iter()
            .filter(|entry| {
                entry.pointer("/approved_treatment/treatment")
                    == Some(&serde_json::Value::String("approved_assumption".into()))
            })
            .map(&query_reference)
            .collect::<Result<Vec<_>, _>>()?;
        let unresolved_queries = tender_queries
            .iter()
            .filter(|entry| {
                entry
                    .get("approved_treatment")
                    .is_none_or(serde_json::Value::is_null)
            })
            .map(&query_reference)
            .collect::<Result<Vec<_>, _>>()?;
        let unresolved_query = unresolved_queries
            .first()
            .cloned()
            .or_else(|| approved_assumptions.first().cloned())
            .ok_or("estimate Query")?;
        let approved_assumption_entry = tender_queries.iter().find(|entry| {
            entry.pointer("/approved_treatment/treatment")
                == Some(&serde_json::Value::String("approved_assumption".into()))
        });
        let approved_allowance_entry = tender_queries.iter().find(|entry| {
            entry.pointer("/approved_treatment/treatment")
                == Some(&serde_json::Value::String("allowance".into()))
        });
        let uses_allowance = matches!(
            scenario,
            "cost-estimator-basis-allowance" | "cost-estimator-basis-uncontrolled-allowance"
        );
        let boq_rows = if scenario == "cost-estimator-basis-missing-rate" {
            serde_json::json!([{
                "row_key": boq_row_key,
                "description": "Cable containment without an approved rate",
                "disposition": "missing",
                "evidence": boq_references.iter().map(&evidence_value).collect::<Result<Vec<_>, _>>()?,
                "calculation_run_id": null,
                "affected_queries": [unresolved_query.clone()]
            }])
        } else {
            serde_json::json!([{
                "row_key": boq_row_key,
                "description": "Cable containment",
                "disposition": "priced",
                "evidence": boq_references.iter().map(&evidence_value).collect::<Result<Vec<_>, _>>()?,
                "calculation_run_id": calculation_run_id,
                "affected_queries": if matches!(scenario, "cost-estimator-basis-unresolved-query" | "cost-estimator-basis-unresolved-query-delayed") {
                    serde_json::json!(unresolved_queries)
                } else {
                    serde_json::json!([])
                }
            }])
        };
        serde_json::json!({
            "scope": "Complete BOQ account for the controlled tender scope.",
            "pricing_date": calculation.get("pricing_date").cloned().ok_or("pricing date")?,
            "currencies": [calculation.get("output_currency").cloned().ok_or("output currency")?],
            "taxes": ["Taxes are excluded unless expressly included in an approved build-up."],
            "rate_sources": ["Exact supplier quotation and approved Calculation Run inputs."],
            "productivity": ["Use only the exact EITL-approved productivity assumption."],
            "design_maturity": "Tender design is sufficiently defined for this controlled BOQ row.",
            "gaps": if scenario == "cost-estimator-basis-incomplete" {
                serde_json::json!(["One evidence-backed rate-source gap remains unresolved."])
            } else {
                serde_json::json!([])
            },
            "exclusions": ["No unpriced scope is included in the canonical total."],
            "boq_rows": boq_rows,
            "cbs_components": if scenario == "cost-estimator-basis-missing-rate" {
                serde_json::json!([])
            } else {
                serde_json::json!([{
                    "component_id": "cccccccccccccccccccccccccccccccc",
                    "cost_code": "LAB-001",
                    "work_package": "Electrical containment",
                    "category": if uses_allowance { "risk" } else { "labor" },
                    "description": "Installation labor build-up",
                    "boq_row_keys": [boq_row_key.clone()],
                    "resource_build_up_ids": ["eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"]
                }])
            },
            "resource_build_ups": if scenario == "cost-estimator-basis-missing-rate" {
                serde_json::json!([])
            } else {
                serde_json::json!([{
                    "build_up_id": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                    "cbs_component_id": "cccccccccccccccccccccccccccccccc",
                    "category": if uses_allowance { "risk" } else { "labor" },
                    "description": "One exact approved labor build-up for BOQ-001.",
                    "calculation_run_id": calculation_run_id
                }])
            },
            "quotations": [{
                "quotation_id": "dddddddddddddddddddddddddddddddd",
                "kind": "supplier",
                "counterparty": "Controlled supplier",
                "exact_scope": "BOQ-001 cable containment",
                "quotation_date": "2026-08-01",
                "currency": calculation.get("output_currency").cloned().ok_or("quote currency")?,
                "exclusions": ["Installation by others"],
                "valid_until": "2026-12-31",
                "evidence": evidence_value(&quote_reference)?,
                "normalization_calculation_run_id": quote_calculation_run_id,
                "covered_boq_row_keys": [if scenario == "cost-estimator-basis-quote-scope-mismatch" {
                    serde_json::json!("BOQ-404")
                } else {
                    boq_row_key.clone()
                }],
                "comparison_assumptions": ["Compare at the exact approved pricing date and currency basis."]
            }],
            "allowances": if uses_allowance {
                let entry = if scenario == "cost-estimator-basis-allowance" {
                    approved_allowance_entry.ok_or("approved allowance basis")?
                } else {
                    approved_assumption_entry.ok_or("approved assumption allowance basis")?
                };
                let reference = query_reference(entry)?;
                serde_json::json!([{
                    "allowance_id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "description": if scenario == "cost-estimator-basis-allowance" {
                        "Controlled risk allowance bound to the exact approved treatment"
                    } else {
                        "Attempted allowance without an Allowance treatment"
                    },
                    "cbs_component_id": "cccccccccccccccccccccccccccccccc",
                    "resource_build_up_id": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                    "query_id": reference.get("query_id").cloned().ok_or("allowance Query id")?,
                    "query_version": reference.get("version").cloned().ok_or("allowance Query version")?,
                    "decision_id": entry.pointer("/approved_treatment/decision_id").cloned().ok_or("allowance decision id")?,
                    "evidence": boq_references.iter().map(&evidence_value).collect::<Result<Vec<_>, _>>()?,
                    "rationale": entry.pointer("/approved_treatment/rationale").cloned().ok_or("allowance rationale")?
                }])
            } else {
                serde_json::json!([])
            },
            "material_assumptions": serde_json::json!(approved_assumptions),
            "comparison_total_calculation_run_id": comparison_total_calculation_run_id
        })
    } else if scenario.starts_with("basis-of-estimate-review") {
        if scenario == "basis-of-estimate-review-failed" {
            let row_key = provider_data_view
                .pointer("/basis_of_estimate/boq_rows/0/row_key")
                .cloned()
                .ok_or("Basis review BOQ row key")?;
            serde_json::json!({
                "outcome": "failed",
                "findings": [{
                    "code": "unresolved_basis_gap",
                    "summary": "The exact Basis retains a material unresolved rate-source gap.",
                    "affected_boq_row_keys": [row_key]
                }]
            })
        } else {
            serde_json::json!({ "outcome": "passed", "findings": [] })
        }
    } else if scenario.starts_with("priced-cost-baseline-review") {
        if scenario == "priced-cost-baseline-review-failed" {
            serde_json::json!({
                "outcome": "failed",
                "findings": [{
                    "code": "baseline_reconciliation_failed",
                    "summary": "The Priced Cost Baseline does not reproduce the exact approved Basis aggregate."
                }]
            })
        } else {
            serde_json::json!({ "outcome": "passed", "findings": [] })
        }
    } else if scenario.starts_with("pricing-adjustment-review") {
        if scenario == "pricing-adjustment-review-failed" {
            serde_json::json!({
                "outcome": "failed",
                "findings": [{
                    "code": "adjustment_provenance_failed",
                    "summary": "The adjustment does not reconcile to its exact controlled Calculation Run."
                }]
            })
        } else {
            serde_json::json!({ "outcome": "passed", "findings": [] })
        }
    } else if scenario == "production-task-output-over-budget" {
        serde_json::json!({
            "evidence_references": (0..256)
                .map(|_| "x".repeat(1_100))
                .collect::<Vec<_>>(),
            "gaps": [],
            "summary": "Output deliberately exceeds the exact task byte budget."
        })
    } else if scenario.starts_with("production-task")
        && provider_data_view
            .get("query_control")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        let query = provider_data_view
            .get("tender_queries")
            .and_then(serde_json::Value::as_array)
            .and_then(|queries| queries.first())
            .ok_or("query control context")?;
        let query_evidence = query
            .get("evidence")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let added_evidence = provider_data_view
            .pointer("/production_task/exact_inputs")
            .and_then(serde_json::Value::as_array)
            .and_then(|inputs| {
                inputs
                    .iter()
                    .find(|input| !query_evidence.contains(input))
                    .cloned()
            })
            .into_iter()
            .collect::<Vec<_>>();
        serde_json::json!({
            "query_updates": [{
                "query_id": query.get("query_id").cloned().unwrap_or_default(),
                "base_version": query.get("query_version").cloned().unwrap_or_default(),
                "added_evidence": added_evidence,
                "proposed_treatments": [{
                    "treatment": "qualification",
                    "rationale": "The owning specialist proposes an attributable controlled qualification for Manager decision."
                }],
                "response": null,
                "response_evidence": []
            }]
        })
    } else if scenario.starts_with("production-task")
        && provider_data_view
            .get("review_candidate")
            .is_some_and(|candidate| !candidate.is_null())
    {
        let review_candidate = provider_data_view
            .get("review_candidate")
            .ok_or("review candidate")?;
        let target_version = review_candidate
            .get("artifact_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or("review target version")?;
        let evidence_reference = review_candidate
            .pointer("/payload/evidence_references/0")
            .and_then(serde_json::Value::as_str)
            .ok_or("review target evidence")?;
        let resolved_finding_ids = review_candidate
            .pointer("/payload/remediations")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|remediation| remediation.get("finding_id").cloned())
            .collect::<Vec<_>>();
        let severity = if scenario == "production-task-review-major" {
            Some("major")
        } else if scenario == "production-task-review-minor" {
            Some("minor")
        } else if (matches!(
            scenario,
            "production-task-review-rework" | "production-task-review-critical"
        ) && target_version == 1)
            || scenario == "production-task-review-critical-repeat"
        {
            Some("critical")
        } else {
            None
        };
        if severity == Some("minor") {
            serde_json::json!({
                "result": "satisfied",
                "resolved_finding_ids": [],
                "findings": [{
                    "severity": "minor",
                    "summary": "A minor presentation limitation remains disclosed.",
                    "evidence_references": [evidence_reference]
                }]
            })
        } else if let Some(severity) = severity {
            serde_json::json!({
                "result": "requires_remediation",
                "resolved_finding_ids": [],
                "findings": [{
                    "severity": severity,
                    "summary": "The exact candidate does not satisfy the approved review criteria.",
                    "evidence_references": [evidence_reference]
                }]
            })
        } else {
            serde_json::json!({
                "result": "satisfied",
                "resolved_finding_ids": resolved_finding_ids,
                "findings": []
            })
        }
    } else if scenario.starts_with("production-task") {
        let mut evidence_references = provider_data_view
            .pointer("/production_task/exact_inputs")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|input| {
                Some(format!(
                    "{}:{}:{}",
                    input.get("kind")?.as_str()?,
                    input.get("reference")?.as_str()?,
                    input.get("version")?.as_u64()?,
                ))
            })
            .collect::<Vec<_>>();
        evidence_references.extend(
            provider_data_view
                .get("dependency_outputs")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|output| {
                    Some(format!(
                        "production_artifact_version:{}:{}",
                        output.get("artifact_id")?.as_str()?,
                        output.get("artifact_version")?.as_u64()?,
                    ))
                }),
        );
        if let Some(target) = provider_data_view
            .get("remediation_target")
            .filter(|target| !target.is_null())
        {
            evidence_references.push(format!(
                "production_artifact_version:{}:{}",
                target
                    .get("artifact_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("remediation artifact id")?,
                target
                    .get("artifact_version")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or("remediation artifact version")?,
            ));
        }
        let remediation_findings = provider_data_view
            .get("remediation_findings")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let remediation_version = provider_data_view
            .pointer("/remediation_target/artifact_version")
            .and_then(serde_json::Value::as_u64);
        if let Some(version) = remediation_version {
            evidence_references.extend(remediation_findings.iter().filter_map(|finding| {
                Some(format!(
                    "production_review_finding:{}:{}",
                    finding.get("finding_id")?.as_str()?,
                    version,
                ))
            }));
        }
        evidence_references.sort();
        evidence_references.dedup();
        if scenario == "production-task-evidence-invalid" {
            evidence_references = vec!["tender_revision:unavailable:1".into()];
        }
        let coordination_evidence = evidence_references
            .first()
            .cloned()
            .ok_or("production coordination evidence")?;
        let coordination_contract = provider_data_view
            .get("coordination_contract")
            .ok_or("production coordination contract")?;
        let required_subjects = coordination_contract
            .get("required_subjects")
            .and_then(serde_json::Value::as_array)
            .ok_or("required coordination subjects")?;
        let assignment_contracts = coordination_contract
            .get("assignment_contracts")
            .and_then(serde_json::Value::as_array)
            .ok_or("coordination assignment contracts")?;
        let source_observations = coordination_contract
            .get("source_observations")
            .and_then(serde_json::Value::as_array)
            .ok_or("coordination source observations")?;
        let mut coordination_observations = Vec::new();
        let mut wrong_key_injected = false;
        for subject in required_subjects {
            let subject = subject.as_str().ok_or("required coordination subject")?;
            if let Some(assignment_contract) = assignment_contracts.iter().find(|contract| {
                contract.get("subject").and_then(serde_json::Value::as_str) == Some(subject)
            }) {
                let mut assignments = assignment_contract
                    .get("required_keys")
                    .and_then(serde_json::Value::as_array)
                    .ok_or("required coordination assignment keys")?
                    .iter()
                    .map(|key| {
                        let key = key.as_str().ok_or("coordination assignment key")?;
                        let source = source_observations
                            .iter()
                            .filter(|observation| {
                                observation
                                    .get("subject")
                                    .and_then(serde_json::Value::as_str)
                                    == Some(subject)
                            })
                            .filter_map(|observation| {
                                observation
                                    .pointer("/value/values")
                                    .and_then(serde_json::Value::as_array)
                            })
                            .flatten()
                            .filter_map(serde_json::Value::as_str)
                            .find(|assignment| {
                                assignment
                                    .split_once('=')
                                    .is_some_and(|(candidate, _)| candidate == key)
                            });
                        Ok(source
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("{key}=validated")))
                    })
                    .collect::<Result<Vec<String>, &'static str>>()?;
                if scenario == "production-task-coordination-commitment-conflict"
                    && subject == "technical_commitment"
                {
                    let assignment = assignments
                        .iter_mut()
                        .find(|assignment| {
                            assignment.contains("project_delivery_context")
                                && assignment.contains("required_capability")
                        })
                        .ok_or("host-owned source coordination key")?;
                    let (key, _) = assignment
                        .split_once('=')
                        .ok_or("source coordination assignment")?;
                    *assignment = format!("{key}=structural_engineering");
                }
                if scenario == "production-task-coordination-responsibility-conflict"
                    && subject == "responsible_party"
                {
                    let assignment = assignments.first_mut().ok_or("responsibility assignment")?;
                    let (key, _) = assignment.split_once('=').ok_or("responsibility key")?;
                    *assignment = format!("{key}=unapproved_profile");
                }
                if matches!(
                    scenario,
                    "production-task-coordination-date-source-responsibility-conflict"
                ) && subject == "responsible_party"
                {
                    let assignment = assignments
                        .iter_mut()
                        .find(|assignment| !assignment.starts_with("task:"))
                        .ok_or("source responsibility assignment")?;
                    let (key, _) = assignment
                        .split_once('=')
                        .ok_or("source responsibility key")?;
                    *assignment = format!("{key}=contractor");
                }
                if matches!(
                    scenario,
                    "production-task-coordination-date-conflict"
                        | "production-task-coordination-date-source-responsibility-conflict"
                ) && subject == "submission_deadline"
                {
                    let assignment = assignments.first_mut().ok_or("deadline assignment")?;
                    let (key, _) = assignment
                        .split_once('=')
                        .ok_or("deadline coordination key")?;
                    *assignment = format!("{key}=2099-12-31T23:59:59Z");
                }
                if scenario == "production-task-coordination-wrong-key" && !wrong_key_injected {
                    let assignment = assignments
                        .first_mut()
                        .ok_or("required coordination assignment")?;
                    *assignment = "agent_invented_key=validated".into();
                    wrong_key_injected = true;
                }
                coordination_observations.extend(assignments.chunks(32).map(|assignments| {
                    serde_json::json!({
                        "evidence_references": [coordination_evidence.clone()],
                        "subject": subject,
                        "value": { "kind": "text_set", "values": assignments },
                    })
                }));
            } else if subject == "expected_delivery_cost" {
                coordination_observations.push(serde_json::json!({
                    "evidence_references": [coordination_evidence.clone()],
                    "subject": subject,
                    "value": {
                        "currency": "EGP",
                        "kind": "amount",
                        "value": if scenario == "production-task-coordination-cost-conflict" {
                            "999"
                        } else {
                            "150"
                        },
                    },
                }));
            } else {
                let value = source_observations
                    .iter()
                    .find(|observation| {
                        observation
                            .get("subject")
                            .and_then(serde_json::Value::as_str)
                            == Some(subject)
                    })
                    .and_then(|observation| observation.get("value"))
                    .cloned()
                    .ok_or("required scalar coordination source")?;
                coordination_observations.push(serde_json::json!({
                    "evidence_references": [coordination_evidence.clone()],
                    "subject": subject,
                    "value": value,
                }));
            }
        }
        let mut output = serde_json::json!({
            "coordination_observations": coordination_observations,
            "evidence_references": evidence_references,
            "gaps": [],
            "summary": format!("Completed the exact bounded production task for {tender_name}.")
        });
        if scenario == "production-task-coordination-missing" {
            output
                .as_object_mut()
                .ok_or("production output object")?
                .remove("coordination_observations");
        }
        let approved_query_treatments = provider_data_view
            .get("approved_query_treatments")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !approved_query_treatments.is_empty() {
            output.as_object_mut().ok_or("production output object")?.insert(
                "query_treatment_applications".into(),
                serde_json::Value::Array(
                    approved_query_treatments
                        .into_iter()
                        .map(|decision| {
                            let decision_id = decision.get("decision_id").cloned().unwrap_or_default();
                            let query_version = decision.get("query_version").cloned().unwrap_or_default();
                            serde_json::json!({
                                "application": "Applied the exact Manager-approved Query Treatment to this new Artifact Version.",
                                "decision_id": decision_id,
                                "evidence_references": [
                                    format!(
                                        "approved_query_treatment:{}:{}",
                                        decision.get("decision_id").and_then(serde_json::Value::as_str).unwrap_or_default(),
                                        decision.get("query_version").and_then(serde_json::Value::as_u64).unwrap_or_default(),
                                    ),
                                    format!(
                                        "tender_query_version:{}:{}",
                                        decision.get("query_id").and_then(serde_json::Value::as_str).unwrap_or_default(),
                                        decision.get("query_version").and_then(serde_json::Value::as_u64).unwrap_or_default(),
                                    )
                                ],
                                "query_id": decision.get("query_id").cloned().unwrap_or_default(),
                                "query_version": query_version,
                                "treatment": decision.get("treatment").cloned().unwrap_or_default(),
                            })
                        })
                        .collect(),
                ),
            );
        }
        if scenario == "production-task-query-proposal"
            && provider_data_view
                .get("remediation_target")
                .is_none_or(serde_json::Value::is_null)
        {
            let evidence = provider_data_view
                .pointer("/production_task/exact_inputs/0")
                .cloned()
                .ok_or("query proposal evidence")?;
            let task_key = provider_data_view
                .pointer("/production_task/task_key")
                .cloned()
                .ok_or("query proposal task")?;
            output.as_object_mut().ok_or("production output object")?.insert(
                "query_proposals".into(),
                serde_json::json!([{
                    "affected_records": [],
                    "affected_task_keys": [task_key],
                    "ambiguity_or_gap": "The exact input leaves a responsibility-sensitive production gap.",
                    "due_at": "2099-01-01T00:00:00.000Z",
                    "evidence": [evidence],
                    "material": true,
                    "proposed_treatments": [{
                        "rationale": "The Manager should approve an exact treatment before dependent work closes.",
                        "treatment": "approved_assumption"
                    }],
                    "query_type": "responsibility_sensitive",
                    "question": "Which party owns the unresolved production responsibility?",
                    "release_blocking": true
                }]),
            );
        }
        if !remediation_findings.is_empty() {
            let remediation_evidence = output
                .get("evidence_references")
                .and_then(serde_json::Value::as_array)
                .and_then(|references| references.first())
                .cloned()
                .ok_or("remediation evidence")?;
            output.as_object_mut().ok_or("production output object")?.insert(
                "remediations".into(),
                serde_json::Value::Array(
                    remediation_findings
                        .into_iter()
                        .map(|finding| {
                            serde_json::json!({
                                "finding_id": finding.get("finding_id").cloned().unwrap_or_default(),
                                "treatment": "Produced a new immutable Artifact Version that addresses the finding.",
                                "evidence_references": [remediation_evidence.clone()]
                            })
                        })
                        .collect(),
                ),
            );
        }
        output
    } else {
        serde_json::json!({
            "summary": format!("{tender_name} is ready for controlled intake analysis."),
            "recommended_next_action": "Verify the imported package before detailed analysis."
        })
    };
    if let Some(records) = candidate
        .get_mut("records")
        .and_then(serde_json::Value::as_array_mut)
    {
        for record in records {
            if record.get("generation_instruction").is_none() {
                record["generation_instruction"] = serde_json::Value::Null;
            }
        }
    }
    let final_item = serde_json::json!({
        "id": "message_fixture_1",
        "type": "agentMessage",
        "text": serde_json::to_string(&candidate)?,
        "phase": if scenario == "phase-null-final" {
            serde_json::Value::Null
        } else {
            serde_json::Value::String("final_answer".into())
        }
    });
    write_json(&serde_json::json!({
        "method": "item/completed",
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "completedAtMs": 1_780_000_002_000_i64,
            "item": final_item.clone()
        }
    }))?;
    write_json(&serde_json::json!({
        "method": "turn/completed",
        "params": {
            "threadId": thread_id,
            "turn": {
                "id": turn_id,
                "status": "completed",
                "items": [final_item.clone()],
                "error": null,
                "startedAt": 1_780_000_001_i64,
                "completedAt": 1_780_000_002_i64,
                "durationMs": 1000
            }
        }
    }))?;
    Ok(true)
}

fn multiplexed_production_candidate(
    provider_data_view: &serde_json::Value,
    evidence_reference: &str,
    suffix: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let coordination_contract = provider_data_view
        .get("coordination_contract")
        .ok_or("multiplexed production coordination contract")?;
    let required_subjects = coordination_contract
        .get("required_subjects")
        .and_then(serde_json::Value::as_array)
        .ok_or("multiplexed required coordination subjects")?;
    let assignment_contracts = coordination_contract
        .get("assignment_contracts")
        .and_then(serde_json::Value::as_array)
        .ok_or("multiplexed coordination assignment contracts")?;
    let source_observations = coordination_contract
        .get("source_observations")
        .and_then(serde_json::Value::as_array)
        .ok_or("multiplexed coordination source observations")?;
    let mut coordination_observations = Vec::new();
    for subject in required_subjects {
        let subject = subject
            .as_str()
            .ok_or("multiplexed required coordination subject")?;
        if let Some(assignment_contract) = assignment_contracts.iter().find(|contract| {
            contract.get("subject").and_then(serde_json::Value::as_str) == Some(subject)
        }) {
            let assignments = assignment_contract
                .get("required_keys")
                .and_then(serde_json::Value::as_array)
                .ok_or("multiplexed required coordination assignment keys")?
                .iter()
                .map(|key| {
                    let key = key
                        .as_str()
                        .ok_or("multiplexed coordination assignment key")?;
                    let source = source_observations
                        .iter()
                        .filter(|observation| {
                            observation
                                .get("subject")
                                .and_then(serde_json::Value::as_str)
                                == Some(subject)
                        })
                        .filter_map(|observation| {
                            observation
                                .pointer("/value/values")
                                .and_then(serde_json::Value::as_array)
                        })
                        .flatten()
                        .filter_map(serde_json::Value::as_str)
                        .find(|assignment| {
                            assignment
                                .split_once('=')
                                .is_some_and(|(candidate, _)| candidate == key)
                        });
                    Ok(source
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{key}=validated")))
                })
                .collect::<Result<Vec<String>, Box<dyn std::error::Error>>>()?;
            coordination_observations.extend(assignments.chunks(32).map(|assignments| {
                serde_json::json!({
                    "evidence_references": [evidence_reference],
                    "subject": subject,
                    "value": { "kind": "text_set", "values": assignments },
                })
            }));
        } else if subject == "expected_delivery_cost" {
            coordination_observations.push(serde_json::json!({
                "evidence_references": [evidence_reference],
                "subject": subject,
                "value": { "currency": "EGP", "kind": "amount", "value": "150" },
            }));
        } else {
            let value = source_observations
                .iter()
                .find(|observation| {
                    observation
                        .get("subject")
                        .and_then(serde_json::Value::as_str)
                        == Some(subject)
                })
                .and_then(|observation| observation.get("value"))
                .cloned()
                .ok_or("multiplexed required scalar coordination source")?;
            coordination_observations.push(serde_json::json!({
                "evidence_references": [evidence_reference],
                "subject": subject,
                "value": value,
            }));
        }
    }
    Ok(serde_json::json!({
        "coordination_observations": coordination_observations,
        "evidence_references": [evidence_reference],
        "gaps": [],
        "summary": format!("Completed multiplexed production task {suffix}.")
    }))
}

fn run_multiplexed_production_turns(
    executable: &Path,
    requests: &mut impl Iterator<Item = io::Result<String>>,
    first_thread_request: serde_json::Value,
    thread_count: &mut u32,
    turn_count: &mut u32,
    automatic: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let automatic_batch = if automatic {
        let path = executable.with_extension("production-auto-batch");
        let batch = fs::read_to_string(&path)
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        fs::write(path, batch.saturating_add(1).to_string())?;
        batch
    } else {
        0
    };
    let target_turns = if automatic && !matches!(automatic_batch, 0 | 3) {
        1
    } else {
        2
    };
    let mut turns = Vec::new();
    let mut threads = Vec::new();
    let mut next_request = Some(first_thread_request);
    while turns.len() < target_turns {
        let request = match next_request.take() {
            Some(request) => request,
            None => read_json_line(requests)?,
        };
        match request.get("method").and_then(serde_json::Value::as_str) {
            Some("thread/archive") => {
                let archived_thread = request
                    .pointer("/params/threadId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("multiplexed archived thread id")?;
                write_json(&serde_json::json!({
                    "id": request.get("id").cloned().ok_or("archive request id")?,
                    "result": {}
                }))?;
                write_json(&serde_json::json!({
                    "method": "thread/archived",
                    "params": { "threadId": archived_thread }
                }))?;
            }
            Some(method @ ("thread/start" | "thread/resume")) => {
                if threads.len() >= 2
                    || (method == "thread/start"
                        && (request.pointer("/params/sandbox")
                            != Some(&serde_json::Value::String("workspaceWrite".into()))
                            || request.pointer("/params/approvalPolicy")
                                != Some(&serde_json::Value::String("never".into()))
                            || !request
                                .pointer("/params/dynamicTools")
                                .and_then(serde_json::Value::as_array)
                                .is_some_and(Vec::is_empty)))
                {
                    return Err(
                        "multiplexed production thread lacks its exact sandbox contract".into(),
                    );
                }
                let suffix = ["a", "b"][threads.len()];
                let thread_id = if method == "thread/resume" {
                    request
                        .pointer("/params/threadId")
                        .and_then(serde_json::Value::as_str)
                        .ok_or("multiplexed resumed thread id")?
                        .to_owned()
                } else {
                    *thread_count = thread_count
                        .checked_add(1)
                        .ok_or("fixture thread count overflow")?;
                    format!("thr_fixture_production_multiplex_{thread_count}")
                };
                let cwd = request
                    .pointer("/params/cwd")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| ".".into());
                let thread = fixture_thread(&thread_id, &cwd);
                write_json(&serde_json::json!({
                    "id": request.get("id").cloned().ok_or("thread request id")?,
                    "result": {
                        "approvalPolicy": "never",
                        "approvalsReviewer": "user",
                        "cwd": cwd,
                        "model": "gpt-5.6-terra",
                        "modelProvider": "openai",
                        "reasoningEffort": "medium",
                        "sandbox": { "type": "readOnly", "networkAccess": false },
                        "thread": thread.clone()
                    }
                }))?;
                write_json(&serde_json::json!({
                    "method": "thread/started",
                    "params": { "thread": thread }
                }))?;
                threads.push((suffix, thread_id));
            }
            Some("turn/start") => {
                if request.pointer("/params/outputSchema").is_none()
                    || request.pointer("/params/sandboxPolicy/type")
                        != Some(&serde_json::Value::String("workspaceWrite".into()))
                    || request.pointer("/params/sandboxPolicy/networkAccess")
                        != Some(&serde_json::Value::Bool(false))
                {
                    return Err(
                        "multiplexed production turn lacks its exact output contract".into(),
                    );
                }
                let thread_id = request
                    .pointer("/params/threadId")
                    .and_then(serde_json::Value::as_str)
                    .ok_or("multiplexed turn thread")?;
                let suffix = threads
                    .iter()
                    .find(|(_, candidate)| candidate == thread_id)
                    .map(|(suffix, _)| *suffix)
                    .ok_or("multiplexed turn used an unknown thread")?;
                if turns
                    .iter()
                    .any(|(_, candidate, _, _)| candidate == thread_id)
                {
                    return Err("multiplexed thread received more than one turn".into());
                }
                *turn_count = turn_count
                    .checked_add(1)
                    .ok_or("fixture turn count overflow")?;
                let turn_id = format!("turn_fixture_production_multiplex_{turn_count}");
                let running_turn = serde_json::json!({
                    "id": turn_id,
                    "status": "inProgress",
                    "items": [],
                    "error": null,
                    "startedAt": 1_780_000_001_i64,
                    "completedAt": null,
                    "durationMs": null
                });
                write_json(&serde_json::json!({
                    "id": request.get("id").cloned().ok_or("turn request id")?,
                    "result": { "turn": running_turn }
                }))?;
                write_json(&serde_json::json!({
                    "method": "turn/started",
                    "params": { "threadId": thread_id, "turn": running_turn }
                }))?;
                fs::write(
                    executable.with_extension(format!("production-{suffix}-waiting")),
                    b"waiting",
                )?;
                let provider_input: serde_json::Value = serde_json::from_str(
                    request
                        .pointer("/params/input/0/text")
                        .and_then(serde_json::Value::as_str)
                        .ok_or("multiplexed provider instruction bundle")?,
                )?;
                let provider_data_view = provider_input
                    .pointer("/provider_data_views/0/payload")
                    .ok_or("multiplexed provider Data View")?;
                let review = provider_data_view
                    .get("review_candidate")
                    .is_some_and(|candidate| !candidate.is_null());
                let evidence_reference = if review {
                    provider_data_view
                        .pointer("/review_candidate/payload/evidence_references/0")
                        .and_then(serde_json::Value::as_str)
                        .ok_or("multiplexed review evidence")?
                        .to_owned()
                } else {
                    let input = provider_data_view
                        .pointer("/production_task/exact_inputs/0")
                        .ok_or("multiplexed production exact input")?;
                    format!(
                        "{}:{}:{}",
                        input
                            .get("kind")
                            .and_then(serde_json::Value::as_str)
                            .ok_or("input kind")?,
                        input
                            .get("reference")
                            .and_then(serde_json::Value::as_str)
                            .ok_or("input reference")?,
                        input
                            .get("version")
                            .and_then(serde_json::Value::as_u64)
                            .ok_or("input version")?,
                    )
                };
                let candidate = if review {
                    serde_json::json!({
                        "result": "satisfied",
                        "resolved_finding_ids": [],
                        "findings": []
                    })
                } else {
                    multiplexed_production_candidate(
                        provider_data_view,
                        &evidence_reference,
                        suffix,
                    )?
                };
                turns.push((suffix, thread_id.to_owned(), turn_id, candidate));
            }
            _ => return Err("unexpected multiplexed production request".into()),
        }
    }
    if automatic && target_turns == 2 {
        fs::write(
            executable.with_extension("production-multiplex-observed"),
            b"two independent turns",
        )?;
    }

    for (suffix, _, _, _) in &turns {
        if automatic {
            continue;
        }
        for _ in 0..2_000 {
            if executable
                .with_extension(format!("production-{suffix}-release"))
                .is_file()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if !executable
            .with_extension(format!("production-{suffix}-release"))
            .is_file()
        {
            return Err("timed out waiting to release multiplexed production output".into());
        }
    }

    for (suffix, thread_id, turn_id, candidate) in turns {
        let final_item = serde_json::json!({
            "id": format!("message_fixture_{suffix}"),
            "type": "agentMessage",
            "text": serde_json::to_string(&candidate)?,
            "phase": "final_answer"
        });
        write_json(&serde_json::json!({
            "method": "item/completed",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "completedAtMs": 1_780_000_002_000_i64,
                "item": final_item.clone()
            }
        }))?;
        write_json(&serde_json::json!({
            "method": "turn/completed",
            "params": {
                "threadId": thread_id,
                "turn": {
                    "id": turn_id,
                    "status": "completed",
                    "items": [final_item],
                    "error": null,
                    "startedAt": 1_780_000_001_i64,
                    "completedAt": 1_780_000_002_i64,
                    "durationMs": 1000
                }
            }
        }))?;
    }
    Ok(true)
}

fn coordinated_record_extraction_candidate(
    data_view: &serde_json::Value,
    changed_branch: Option<&str>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut candidate = record_extraction_candidate(data_view)?;
    let (general_evidence, changed_branch_evidence) = if changed_branch.is_some() {
        let evidence = data_view
            .get("evidence")
            .and_then(serde_json::Value::as_array)
            .ok_or("Tender Evidence Data View")?;
        let mut artifact_ids = Vec::new();
        for item in evidence {
            let artifact_id = item
                .pointer("/reference/artifact_id")
                .and_then(serde_json::Value::as_str)
                .ok_or("split-source Evidence artifact")?;
            if !artifact_ids
                .iter()
                .any(|candidate| candidate == artifact_id)
            {
                artifact_ids.push(artifact_id.to_owned());
            }
        }
        if artifact_ids.len() != 2 {
            return Err("split-source fixture requires exactly two Source Artifacts".into());
        }
        let reference_for = |artifact_id: &str| {
            evidence.iter().find(|item| {
                item.pointer("/reference/artifact_id")
                    .and_then(serde_json::Value::as_str)
                    == Some(artifact_id)
            })
        };
        (
            reference_for(&artifact_ids[0])
                .and_then(|item| item.get("reference"))
                .cloned()
                .ok_or("general split-source Evidence")?,
            reference_for(&artifact_ids[1])
                .and_then(|item| item.get("reference"))
                .cloned()
                .ok_or("changed split-source Evidence")?,
        )
    } else {
        let exact = candidate
            .pointer("/records/0/fields/0/evidence/0")
            .cloned()
            .ok_or("coordinated record evidence")?;
        (exact.clone(), exact)
    };
    let records = candidate["records"]
        .as_array_mut()
        .ok_or("coordinated record candidates")?;
    if let Some(changed_branch) = changed_branch {
        for record in records.iter_mut() {
            let is_changed_branch = record.get("stable_key").and_then(serde_json::Value::as_str)
                == Some(changed_branch);
            for field in record
                .get_mut("fields")
                .and_then(serde_json::Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if field
                    .get("evidence")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|evidence| !evidence.is_empty())
                {
                    field["evidence"] = serde_json::json!([if is_changed_branch {
                        changed_branch_evidence.clone()
                    } else {
                        general_evidence.clone()
                    }]);
                }
            }
        }
    }
    if let Some(deadline) = records.iter_mut().find(|record| {
        record.get("stable_key").and_then(serde_json::Value::as_str) == Some("submission_deadline")
    }) {
        deadline["contradictions"] = serde_json::json!([]);
        deadline["fields"][0]["uncertainty"] = serde_json::Value::Null;
    }
    records.push(serde_json::json!({
        "stable_key": "clarification_cutoff",
        "kind": "deadline",
        "title": "Clarification cutoff",
        "fields": [{
            "name": "deadline",
            "value": "8 May 2026 at 14:00 Cairo time",
            "basis_kind": "evidence",
            "basis_reference": null,
            "basis_description": null,
            "original_expression": "8 May 2026 at 14:00 Cairo time",
            "normalized_value": "2026-05-08T14:00:00+03:00",
            "timezone": "Africa/Cairo",
            "uncertainty": null,
            "evidence": [general_evidence.clone()]
        }],
        "contradictions": []
    }));
    if let Some(project) = records.iter_mut().find(|record| {
        record.get("stable_key").and_then(serde_json::Value::as_str)
            == Some("project_delivery_context")
    }) {
        project["fields"]
            .as_array_mut()
            .ok_or("project delivery fields")?
            .push(serde_json::json!({
                "name": "responsible_party",
                "value": "tender_coordinator",
                "basis_kind": "evidence",
                "basis_reference": null,
                "basis_description": null,
                "original_expression": null,
                "normalized_value": null,
                "timezone": null,
                "uncertainty": null,
                "evidence": [general_evidence]
            }));
    }
    Ok(candidate)
}

fn record_extraction_candidate(
    data_view: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let evidence = data_view
        .get("evidence")
        .and_then(serde_json::Value::as_array)
        .ok_or("Tender Evidence Data View")?;
    let authoritative_item = evidence
        .iter()
        .find(|item| {
            item.pointer("/location/language")
                .and_then(serde_json::Value::as_str)
                == Some("arabic")
                && item
                    .pointer("/location/translated_text")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
        })
        .or_else(|| evidence.first())
        .ok_or("authoritative record evidence")?;
    let authoritative = authoritative_item
        .get("reference")
        .cloned()
        .ok_or("authoritative record reference")?;
    let authoritative_text = authoritative_item
        .pointer("/location/original_text")
        .and_then(serde_json::Value::as_str)
        .ok_or("authoritative original text")?;
    let deadline_evidence = evidence
        .iter()
        .filter(|item| {
            item.pointer("/location/original_text")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|text| text.contains("Submission deadline"))
        })
        .filter_map(|item| item.get("reference").cloned())
        .collect::<Vec<_>>();
    let deadline = deadline_evidence
        .first()
        .cloned()
        .unwrap_or_else(|| authoritative.clone());
    let conflicting_deadline = deadline_evidence
        .last()
        .cloned()
        .unwrap_or_else(|| authoritative.clone());
    let mut candidate = serde_json::json!({
        "records": [
            {
                "stable_key": "authoritative_notice",
                "kind": "clause",
                "title": "Exact authoritative notice",
                "fields": [{
                    "name": "text",
                    "value": authoritative_text,
                    "basis_kind": "evidence",
                    "basis_reference": null,
                    "basis_description": null,
                    "original_expression": null,
                    "normalized_value": null,
                    "timezone": null,
                    "uncertainty": null,
                    "evidence": [authoritative.clone()]
                }],
                "contradictions": []
            },
            {
                "stable_key": "bid_security_required",
                "kind": "requirement",
                "title": "Bid security requirement",
                "fields": [{
                    "name": "requirement",
                    "value": "Bid security is required.",
                    "basis_kind": "evidence",
                    "basis_reference": null,
                    "basis_description": null,
                    "original_expression": null,
                    "normalized_value": null,
                    "timezone": null,
                    "uncertainty": null,
                    "evidence": [authoritative.clone()]
                }],
                "contradictions": []
            },
            {
                "stable_key": "submission_deadline",
                "kind": "deadline",
                "title": "Submission deadline",
                "fields": [{
                    "name": "deadline",
                    "value": "15 May 2026 at 14:00 Cairo time",
                    "basis_kind": "evidence",
                    "basis_reference": null,
                    "basis_description": null,
                    "original_expression": "15 May 2026 at 14:00 Cairo time",
                    "normalized_value": "2026-05-15T14:00:00+03:00",
                    "timezone": "Africa/Cairo",
                    "uncertainty": "A conflicting supplied expression requires resolution.",
                    "evidence": [deadline]
                }],
                "contradictions": [{
                    "field_name": "deadline",
                    "summary": "The supplied Evidence contains conflicting submission dates.",
                    "evidence": [deadline, conflicting_deadline]
                }]
            },
            {
                "stable_key": "project_delivery_context",
                "kind": "project_characteristic",
                "title": "Verified project delivery context",
                "fields": [{
                    "name": "required_capability",
                    "value": "document_control",
                    "basis_kind": "evidence",
                    "basis_reference": null,
                    "basis_description": null,
                    "original_expression": null,
                    "normalized_value": null,
                    "timezone": null,
                    "uncertainty": null,
                    "evidence": [authoritative.clone()]
                }],
                "contradictions": []
            },
            {
                "stable_key": "programme_pressure",
                "kind": "risk",
                "title": "Tender programme pressure",
                "fields": [{
                    "name": "recommended_capability",
                    "value": "tender_analysis",
                    "basis_kind": "evidence",
                    "basis_reference": null,
                    "basis_description": null,
                    "original_expression": null,
                    "normalized_value": null,
                    "timezone": null,
                    "uncertainty": null,
                    "evidence": [authoritative.clone()]
                }],
                "contradictions": []
            },
            {
                "stable_key": "crane_capacity",
                "kind": "assumption",
                "title": "Crane capacity remains unknown",
                "fields": [{
                    "name": "capacity",
                    "value": null,
                    "basis_kind": "assumption",
                    "basis_reference": "crane_capacity",
                    "basis_description": "No supplied Evidence states the required crane capacity.",
                    "original_expression": null,
                    "normalized_value": null,
                    "timezone": null,
                    "uncertainty": "Unresolved evidence gap",
                    "evidence": []
                }],
                "contradictions": []
            }
        ]
    });
    if let Some(authority) = data_view
        .get("authorities")
        .and_then(serde_json::Value::as_array)
        .and_then(|authorities| authorities.first())
    {
        candidate["records"]
            .as_array_mut()
            .ok_or("Tender Record candidate records")?
            .push(serde_json::json!({
                "stable_key": "engineer_entry_basis",
                "kind": "project_characteristic",
                "title": "Attributable Engineer entry",
                "fields": [{
                    "name": "engineer_value",
                    "value": authority.get("value").cloned().ok_or("authority value")?,
                    "basis_kind": authority.get("kind").cloned().ok_or("authority kind")?,
                    "basis_reference": authority.get("authority_id").cloned().ok_or("authority id")?,
                    "basis_description": authority.get("description").cloned().ok_or("authority description")?,
                    "original_expression": null,
                    "normalized_value": null,
                    "timezone": null,
                    "uncertainty": null,
                    "evidence": []
                }],
                "contradictions": []
            }));
    }
    Ok(candidate)
}

fn read_json_request(
    requests: &mut impl Iterator<Item = io::Result<String>>,
    expected_method: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let request = read_json_line(requests)?;
    if request.get("method").and_then(serde_json::Value::as_str) != Some(expected_method) {
        return Err(format!("expected {expected_method} request").into());
    }
    Ok(request)
}

fn fixture_thread(thread_id: &str, cwd: &str) -> serde_json::Value {
    serde_json::json!({
        "id": thread_id,
        "sessionId": thread_id,
        "preview": "",
        "ephemeral": false,
        "modelProvider": "openai",
        "createdAt": 1_780_000_000_i64,
        "updatedAt": 1_780_000_000_i64,
        "cwd": cwd,
        "source": "appServer",
        "cliVersion": "0.147.0",
        "status": { "type": "idle" },
        "turns": [],
    })
}

fn read_json_line(
    requests: &mut impl Iterator<Item = io::Result<String>>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let line = requests.next().ok_or("missing JSONL request")??;
    Ok(serde_json::from_str(&line)?)
}

fn write_json(value: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let mut value = value.clone();
    if value.get("method").is_some() && value.get("id").is_none() {
        value
            .as_object_mut()
            .ok_or("fixture JSONL message must be an object")?
            .insert(
                "emittedAtMs".into(),
                serde_json::json!(1_780_000_000_000_i64),
            );
    }
    println!("{}", serde_json::to_string(&value)?);
    io::stdout().flush()?;
    Ok(())
}

fn run_uv(
    executable: &Path,
    arguments: &[std::ffi::OsString],
) -> Result<(), Box<dyn std::error::Error>> {
    assert_isolated_python_environment()?;
    if arguments.first().and_then(|value| value.to_str()) != Some("sync") {
        return Err("unexpected uv fixture command".into());
    }
    for required in [
        "--locked",
        "--no-dev",
        "--managed-python",
        "--python",
        "--project",
        "--no-config",
    ] {
        if !arguments.iter().any(|argument| argument == required) {
            return Err(format!("missing required uv argument {required}").into());
        }
    }
    let delay = executable.with_extension("delay");
    if delay.is_file() {
        let milliseconds = fs::read_to_string(delay)?.trim().parse()?;
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
    let environment =
        PathBuf::from(env::var_os("UV_PROJECT_ENVIRONMENT").ok_or("missing project environment")?);
    let python =
        PathBuf::from(env::var_os("UV_PYTHON_INSTALL_DIR").ok_or("missing Python install root")?);
    let python_downloads = PathBuf::from(
        env::var_os("UV_PYTHON_DOWNLOADS_JSON_URL")
            .ok_or("missing approved Python downloads manifest")?,
    );
    if !python_downloads.is_file() {
        return Err("approved Python downloads manifest is missing".into());
    }
    if arguments.iter().any(|argument| argument == "--check") {
        if !arguments.iter().any(|argument| argument == "--offline")
            || !environment.join("fixture_dependency.py").is_file()
            || !python.join("managed-python.txt").is_file()
        {
            return Err("managed environment is not synchronized".into());
        }
        return Ok(());
    }
    let binary_directory = if cfg!(windows) {
        environment.join("Scripts")
    } else {
        environment.join("bin")
    };
    fs::create_dir_all(&binary_directory)?;
    fs::create_dir_all(&python)?;
    fs::write(environment.join("fixture_dependency.py"), b"fixture=true\n")?;
    fs::write(python.join("managed-python.txt"), b"3.12.13\n")?;
    let concrete_python = python.join(format!(
        "cpython-3.12.13-{}-{}-none",
        env::consts::OS,
        env::consts::ARCH
    ));
    fs::create_dir_all(&concrete_python)?;
    fs::write(concrete_python.join("install.txt"), b"3.12.13\n")?;
    let minor_python = python.join(format!(
        "cpython-3.12-{}-{}-none",
        env::consts::OS,
        env::consts::ARCH
    ));
    #[cfg(windows)]
    junction::create(&concrete_python, &minor_python)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(&concrete_python, &minor_python)?;
    #[cfg(unix)]
    {
        let library = environment.join("lib");
        fs::create_dir_all(&library)?;
        std::os::unix::fs::symlink(&library, environment.join("lib64"))?;
    }
    let extension = env::consts::EXE_EXTENSION;
    for name in ["docling", "python"] {
        let destination = binary_directory.join(if extension.is_empty() {
            name.to_owned()
        } else {
            format!("{name}.{extension}")
        });
        fs::copy(executable, &destination)?;
        fs::write(
            destination.with_extension("version"),
            fs::read_to_string(executable.with_file_name("docling.version"))?,
        )?;
        make_executable(&destination)?;
    }
    Ok(())
}

fn run_prepare_models(arguments: &[std::ffi::OsString]) -> Result<(), Box<dyn std::error::Error>> {
    assert_isolated_python_environment()?;
    let script = arguments
        .first()
        .ok_or("missing model preparation script")?;
    if Path::new(script).file_name().and_then(|name| name.to_str()) != Some("prepare_models.py") {
        return Err("unexpected model preparation script".into());
    }
    let output = argument_value(arguments, "--output-dir")?;
    for profile in [
        "layout",
        "tableformer",
        "code_formula",
        "picture_classifier",
        "rapidocr",
    ] {
        let model = output.join(profile).join("model.bin");
        fs::create_dir_all(model.parent().ok_or("model parent")?)?;
        fs::write(model, format!("{profile} fixture model"))?;
    }
    println!("{}", output.display());
    Ok(())
}

fn run_docling(arguments: &[std::ffi::OsString]) -> Result<(), Box<dyn std::error::Error>> {
    assert_isolated_python_environment()?;
    if arguments.first().and_then(|value| value.to_str()) != Some("convert") {
        return Err("unexpected Docling fixture command".into());
    }
    let source = arguments
        .get(1)
        .map(PathBuf::from)
        .ok_or("missing staged Docling source")?;
    let output = argument_value(arguments, "--output")?;
    let models = argument_value(arguments, "--artifacts-path")?;
    for profile in [
        "layout",
        "tableformer",
        "code_formula",
        "picture_classifier",
        "rapidocr",
    ] {
        if !models.join(profile).join("model.bin").is_file() {
            return Err(format!("fixture {profile} model is missing").into());
        }
    }
    if source.file_name().and_then(|name| name.to_str()) == Some("readiness.pdf") {
        return run_docling_readiness(arguments, &output);
    }
    for flag in [
        "--no-enable-remote-services",
        "--no-allow-external-plugins",
        "--abort-on-error",
        "--quiet",
    ] {
        if !arguments.iter().any(|argument| argument == flag) {
            return Err(format!("missing controlled Docling flag {flag}").into());
        }
    }
    let input_format = source
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or("staged source has no format")?;
    if argument_value(arguments, "--from")? != Path::new(input_format)
        || argument_value(arguments, "--to")? != Path::new("json")
        || argument_value(arguments, "--image-export-mode")? != Path::new("placeholder")
        || argument_value(arguments, "--document-timeout")? != Path::new("840")
        || argument_value(arguments, "--num-threads")? != Path::new("2")
        || argument_value(arguments, "--device")? != Path::new("cpu")
    {
        return Err("unexpected controlled Docling parse contract".into());
    }
    if source
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        != Some("input")
        || output.file_name().and_then(|name| name.to_str()) != Some("candidate")
    {
        return Err("Docling did not receive isolated staged paths".into());
    }
    fs::create_dir_all(&output)?;
    let name = source
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("invalid staged source name")?;
    let source_bytes = fs::read(&source)?;
    if source_bytes
        .windows(b"INTERRUPTED_PROCESS".len())
        .any(|window| window == b"INTERRUPTED_PROCESS")
    {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
    if source_bytes
        .windows(b"PROCESS_FAILURE".len())
        .any(|window| window == b"PROCESS_FAILURE")
        || source_bytes
            .windows(b"/Encrypt".len())
            .any(|window| window == b"/Encrypt")
    {
        return Err("fixture Docling conversion failed".into());
    }
    if source_bytes
        .windows(b"MALFORMED_OUTPUT".len())
        .any(|window| window == b"MALFORMED_OUTPUT")
    {
        fs::write(
            output.join(format!("{name}.json")),
            br#"{"schema_name":"DoclingDocument"}"#,
        )?;
        return Ok(());
    }
    if source_bytes
        .windows(b"EXCESSIVE_OUTPUT".len())
        .any(|window| window == b"EXCESSIVE_OUTPUT")
    {
        fs::File::create(output.join(format!("{name}.json")))?.set_len(64 * 1024 + 1)?;
        return Ok(());
    }
    if source_bytes
        .windows(b"LOSS_OUTPUT".len())
        .any(|window| window == b"LOSS_OUTPUT")
    {
        fs::write(
            output.join(format!("{name}.json")),
            serde_json::to_vec(&serde_json::json!({
                "schema_name": "DoclingDocument",
                "version": "1.10.0",
                "name": name,
                "origin": { "mimetype": "application/pdf", "filename": format!("{name}.pdf") },
                "body": { "self_ref": "#/body", "children": [] },
                "groups": [],
                "texts": [],
                "tables": [],
                "pages": {}
            }))?,
        )?;
        return Ok(());
    }
    if source_bytes
        .windows(b"INVALID_REFERENCE_OUTPUT".len())
        .any(|window| window == b"INVALID_REFERENCE_OUTPUT")
    {
        let mut document = bilingual_pdf_document(name);
        document["body"]["children"] = serde_json::json!([{ "$ref": "#/texts/999" }]);
        fs::write(
            output.join(format!("{name}.json")),
            serde_json::to_vec(&document)?,
        )?;
        return Ok(());
    }
    let document = if source_bytes
        .windows(b"TENDER_RECORD_GOLDEN".len())
        .any(|window| window == b"TENDER_RECORD_GOLDEN")
    {
        tender_record_pdf_document(name)
    } else {
        match input_format {
            "pdf" => bilingual_pdf_document(name),
            "docx" => bilingual_docx_document(name),
            "xlsx" => bilingual_xlsx_document(name),
            _ => return Err(format!("unsupported fixture input format {input_format}").into()),
        }
    };
    fs::write(
        output.join(format!("{name}.json")),
        serde_json::to_vec(&document)?,
    )?;
    Ok(())
}

fn run_docling_readiness(
    arguments: &[std::ffi::OsString],
    output: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if argument_value(arguments, "--ocr-engine")? != Path::new("rapidocr")
        || argument_value(arguments, "--ocr-mode")? != Path::new("full_page")
        || argument_value(arguments, "--ocr-lang")? != Path::new("ch")
    {
        return Err("RapidOCR full-page smoke was not requested".into());
    }
    fs::create_dir_all(output)?;
    fs::write(
        output.join("readiness.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema_name": "DoclingDocument",
            "name": "readiness",
            "origin": { "mimetype": "application/pdf" },
            "texts": [{
                "text": "Docling bundles PDF document conversion to JSON and Markdown in an easy self contained package"
            }],
            "pages": { "1": { "size": { "width": 1, "height": 1 } } },
        }))?,
    )?;
    Ok(())
}

fn bilingual_pdf_document(name: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_name": "DoclingDocument",
        "version": "1.10.0",
        "name": name,
        "origin": { "mimetype": "application/pdf", "filename": format!("{name}.pdf") },
        "body": {
            "self_ref": "#/body",
            "children": [
                { "$ref": "#/groups/0" },
                { "$ref": "#/tables/0" }
            ]
        },
        "groups": [{
            "self_ref": "#/groups/0",
            "parent": { "$ref": "#/body" },
            "children": [
                { "$ref": "#/texts/0" },
                { "$ref": "#/texts/1" },
                { "$ref": "#/texts/2" }
            ],
            "name": "Commercial Conditions",
            "label": "section"
        }],
        "texts": [
            {
                "self_ref": "#/texts/0",
                "parent": { "$ref": "#/groups/0" },
                "label": "section_header",
                "orig": "Commercial Conditions",
                "text": "Commercial Conditions",
                "prov": [{
                    "page_no": 1,
                    "charspan": [0, 21],
                    "bbox": { "l": 10, "t": 90, "r": 190, "b": 70, "coord_origin": "BOTTOMLEFT" }
                }]
            },
            {
                "self_ref": "#/texts/1",
                "parent": { "$ref": "#/groups/0" },
                "label": "text",
                "orig": "Bid security is required.",
                "text": "Bid security is required.",
                "prov": [
                    {
                        "page_no": 1,
                        "charspan": [0, 12],
                        "bbox": { "l": 10, "t": 65, "r": 190, "b": 45, "coord_origin": "BOTTOMLEFT" }
                    },
                    {
                        "page_no": 2,
                        "charspan": [12, 25],
                        "bbox": { "l": 10, "t": 95, "r": 190, "b": 75, "coord_origin": "BOTTOMLEFT" }
                    }
                ]
            },
            {
                "self_ref": "#/texts/2",
                "parent": { "$ref": "#/groups/0" },
                "label": "text",
                "orig": "ضمان العطاء مطلوب.",
                "text": "ضمان العطاء مطلوب.",
                "prov": [{
                    "page_no": 2,
                    "charspan": [0, 19],
                    "bbox": { "l": 10, "t": 90, "r": 190, "b": 70, "coord_origin": "BOTTOMLEFT" }
                }]
            }
        ],
        "tables": [{
            "self_ref": "#/tables/0",
            "parent": { "$ref": "#/body" },
            "label": "table",
            "prov": [{
                "page_no": 2,
                "charspan": [0, 0],
                "bbox": { "l": 10, "t": 60, "r": 190, "b": 10, "coord_origin": "BOTTOMLEFT" }
            }],
            "data": {
                "num_rows": 2,
                "num_cols": 2,
                "table_cells": [
                    { "start_row_offset_idx": 0, "end_row_offset_idx": 1, "start_col_offset_idx": 0, "end_col_offset_idx": 1, "text": "Item" },
                    { "start_row_offset_idx": 0, "end_row_offset_idx": 1, "start_col_offset_idx": 1, "end_col_offset_idx": 2, "text": "Price" },
                    { "start_row_offset_idx": 1, "end_row_offset_idx": 2, "start_col_offset_idx": 0, "end_col_offset_idx": 1, "text": "Concrete" },
                    { "start_row_offset_idx": 1, "end_row_offset_idx": 2, "start_col_offset_idx": 1, "end_col_offset_idx": 2, "text": "125000" }
                ]
            }
        }],
        "pages": {
            "1": { "page_no": 1, "size": { "width": 200, "height": 100 } },
            "2": { "page_no": 2, "size": { "width": 200, "height": 100 } }
        }
    })
}

fn tender_record_pdf_document(name: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_name": "DoclingDocument",
        "version": "1.10.0",
        "name": name,
        "origin": { "mimetype": "application/pdf", "filename": format!("{name}.pdf") },
        "body": {
            "self_ref": "#/body",
            "children": [{ "$ref": "#/groups/0" }]
        },
        "groups": [{
            "self_ref": "#/groups/0",
            "parent": { "$ref": "#/body" },
            "children": [
                { "$ref": "#/texts/0" },
                { "$ref": "#/texts/1" },
                { "$ref": "#/texts/2" },
                { "$ref": "#/texts/3" },
                { "$ref": "#/texts/4" }
            ],
            "name": "Tender Conditions",
            "label": "section"
        }],
        "texts": [
            {
                "self_ref": "#/texts/0",
                "parent": { "$ref": "#/groups/0" },
                "label": "section_header",
                "orig": "Tender Conditions",
                "text": "Tender Conditions",
                "prov": [{
                    "page_no": 1,
                    "charspan": [0, 17],
                    "bbox": { "l": 10, "t": 90, "r": 190, "b": 70, "coord_origin": "BOTTOMLEFT" }
                }]
            },
            {
                "self_ref": "#/texts/1",
                "parent": { "$ref": "#/groups/0" },
                "label": "text",
                "orig": "Bid security is required.",
                "text": "Bid security is required.",
                "prov": [{
                    "page_no": 1,
                    "charspan": [0, 25],
                    "bbox": { "l": 10, "t": 65, "r": 190, "b": 45, "coord_origin": "BOTTOMLEFT" }
                }]
            },
            {
                "self_ref": "#/texts/2",
                "parent": { "$ref": "#/groups/0" },
                "label": "text",
                "orig": "\u{0636}\u{0645}\u{0627}\u{0646} \u{0627}\u{0644}\u{0639}\u{0637}\u{0627}\u{0621} \u{0645}\u{0637}\u{0644}\u{0648}\u{0628}.",
                "text": "Bid security is required.",
                "prov": [{
                    "page_no": 2,
                    "charspan": [0, 19],
                    "bbox": { "l": 10, "t": 90, "r": 190, "b": 70, "coord_origin": "BOTTOMLEFT" }
                }]
            },
            {
                "self_ref": "#/texts/3",
                "parent": { "$ref": "#/groups/0" },
                "label": "text",
                "orig": "Submission deadline: 15 May 2026 at 14:00 Cairo time.",
                "text": "Submission deadline: 15 May 2026 at 14:00 Cairo time.",
                "prov": [{
                    "page_no": 1,
                    "charspan": [0, 57],
                    "bbox": { "l": 10, "t": 40, "r": 190, "b": 25, "coord_origin": "BOTTOMLEFT" }
                }]
            },
            {
                "self_ref": "#/texts/4",
                "parent": { "$ref": "#/groups/0" },
                "label": "text",
                "orig": "Submission deadline: 16 May 2026 at 14:00 Cairo time.",
                "text": "Submission deadline: 16 May 2026 at 14:00 Cairo time.",
                "prov": [{
                    "page_no": 2,
                    "charspan": [0, 57],
                    "bbox": { "l": 10, "t": 65, "r": 190, "b": 45, "coord_origin": "BOTTOMLEFT" }
                }]
            }
        ],
        "tables": [],
        "pages": {
            "1": { "page_no": 1, "size": { "width": 200, "height": 100 } },
            "2": { "page_no": 2, "size": { "width": 200, "height": 100 } }
        }
    })
}

fn bilingual_docx_document(name: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_name": "DoclingDocument",
        "version": "1.10.0",
        "name": name,
        "origin": {
            "mimetype": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "filename": format!("{name}.docx")
        },
        "body": {
            "self_ref": "#/body",
            "children": [{ "$ref": "#/groups/0" }]
        },
        "groups": [{
            "self_ref": "#/groups/0",
            "parent": { "$ref": "#/body" },
            "children": [
                { "$ref": "#/texts/0" },
                { "$ref": "#/texts/1" },
                { "$ref": "#/texts/2" }
            ],
            "name": "Scope of Works",
            "label": "section"
        }],
        "texts": [
            {
                "self_ref": "#/texts/0",
                "parent": { "$ref": "#/groups/0" },
                "label": "section_header",
                "orig": "Scope of Works",
                "text": "Scope of Works"
            },
            {
                "self_ref": "#/texts/1",
                "parent": { "$ref": "#/groups/0" },
                "label": "text",
                "orig": "Works include concrete and finishes.",
                "text": "Works include concrete and finishes."
            },
            {
                "self_ref": "#/texts/2",
                "parent": { "$ref": "#/groups/0" },
                "label": "text",
                "orig": "تشمل الأعمال الخرسانة والتشطيبات.",
                "text": "تشمل الأعمال الخرسانة والتشطيبات."
            }
        ],
        "tables": [],
        "pages": {}
    })
}

fn bilingual_xlsx_document(name: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_name": "DoclingDocument",
        "version": "1.10.0",
        "name": name,
        "origin": {
            "mimetype": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "filename": format!("{name}.xlsx")
        },
        "body": {
            "self_ref": "#/body",
            "children": [{ "$ref": "#/groups/0" }]
        },
        "groups": [{
            "self_ref": "#/groups/0",
            "parent": { "$ref": "#/body" },
            "children": [{ "$ref": "#/tables/0" }],
            "name": "Pricing",
            "label": "sheet"
        }],
        "texts": [],
        "tables": [{
            "self_ref": "#/tables/0",
            "parent": { "$ref": "#/groups/0" },
            "label": "table",
            "prov": [{
                "page_no": 1,
                "charspan": [0, 0],
                "bbox": { "l": 0, "t": 4, "r": 3, "b": 0, "coord_origin": "TOPLEFT" }
            }],
            "data": {
                "num_rows": 4,
                "num_cols": 3,
                "table_cells": [
                    { "start_row_offset_idx": 0, "end_row_offset_idx": 1, "start_col_offset_idx": 0, "end_col_offset_idx": 1, "text": "Item" },
                    { "start_row_offset_idx": 3, "end_row_offset_idx": 4, "start_col_offset_idx": 1, "end_col_offset_idx": 2, "text": "خرسانة" },
                    { "start_row_offset_idx": 3, "end_row_offset_idx": 4, "start_col_offset_idx": 2, "end_col_offset_idx": 3, "text": "450.75" }
                ]
            }
        }],
        "pages": {
            "1": { "page_no": 1, "size": { "width": 3, "height": 4 } }
        }
    })
}

fn assert_isolated_python_environment() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["PYTHONPATH", "PYTHONHOME", "VIRTUAL_ENV", "CONDA_PREFIX"] {
        if env::var_os(name).is_some() {
            return Err(format!("uncontrolled environment variable was inherited: {name}").into());
        }
    }
    Ok(())
}

fn argument_value(
    arguments: &[std::ffi::OsString],
    name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let index = arguments
        .iter()
        .position(|value| value == name)
        .ok_or_else(|| format!("missing {name}"))?;
    arguments
        .get(index + 1)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {name} value").into())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

#[cfg(windows)]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}
