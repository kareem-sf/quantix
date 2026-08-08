use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process,
};

fn main() {
    if let Err(error) = run() {
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
    if thread_method == "thread/start"
        && (thread_request.pointer("/params/sandbox")
            != Some(&serde_json::Value::String("workspaceWrite".into()))
            || thread_request.pointer("/params/approvalPolicy")
                != Some(&serde_json::Value::String("never".into()))
            || thread_request.pointer("/params/dynamicTools/0/name")
                != Some(&serde_json::Value::String(
                    "quantix_read_tender_metadata".into(),
                )))
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
        new_thread_id = format!("thr_fixture_{sequence}");
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
    let input = workspace.join("inputs").join("tender-metadata-v1.json");
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
    let tender_name = provider_data_view
        .pointer("/tender/name")
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
    if scenario == "malformed-after-turn" {
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
    let candidate = if scenario == "output-invalid" {
        serde_json::json!({ "summary": "Missing the required next action." })
    } else {
        serde_json::json!({
            "summary": format!("{tender_name} is ready for controlled intake analysis."),
            "recommended_next_action": "Verify the imported package before detailed analysis."
        })
    };
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
    let document = match input_format {
        "pdf" => bilingual_pdf_document(name),
        "docx" => bilingual_docx_document(name),
        "xlsx" => bilingual_xlsx_document(name),
        _ => return Err(format!("unsupported fixture input format {input_format}").into()),
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
