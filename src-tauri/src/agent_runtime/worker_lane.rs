use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::process_supervisor::{
    ProcessSpec, ProcessSupervisor, ProcessTermination, SupervisedConversation,
};

pub(crate) const WORKER_MAX_TOOL_ROUNDS: u32 = 32;
const WORKER_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerFailureCategory {
    Auth,
    RateLimited,
    Network,
    InvalidOutput,
    Budget,
    Protocol,
    Provider,
    Cancelled,
    Process,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerDriverError {
    pub category: WorkerFailureCategory,
    pub message: String,
}

impl WorkerDriverError {
    fn protocol(message: impl Into<String>) -> Self {
        Self {
            category: WorkerFailureCategory::Protocol,
            message: message.into(),
        }
    }

    fn process(message: impl Into<String>) -> Self {
        Self {
            category: WorkerFailureCategory::Process,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerOperation {
    Probe,
    Turn,
}

#[derive(Debug, Clone)]
pub struct WorkerToolDescriptor {
    pub name: String,
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct WorkerRunRequest {
    pub route: String,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model_id: String,
    pub reasoning: Option<String>,
    pub instructions: String,
    pub output_schema: Option<Value>,
    pub tools: Vec<WorkerToolDescriptor>,
    pub input: String,
    pub operation: WorkerOperation,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkerOutcome {
    Probe {
        text: String,
        usage: WorkerUsage,
    },
    Turn {
        output: Option<Value>,
        text: String,
        usage: WorkerUsage,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerApproval {
    Approved(Value),
    Denied(String),
}

pub async fn run_worker_operation(
    supervisor: &ProcessSupervisor,
    worker_python: &Path,
    application_home: &Path,
    request: &WorkerRunRequest,
    cancellation: CancellationToken,
    mut on_approval: impl FnMut(&str, &Value) -> WorkerApproval,
) -> Result<WorkerOutcome, WorkerDriverError> {
    let staging_directory = application_home.join("staging");
    std::fs::create_dir_all(&staging_directory)
        .map_err(|_| WorkerDriverError::process("the AI worker staging directory is missing"))?;
    let mut conversation = start_worker(
        supervisor,
        worker_python,
        application_home,
        &staging_directory,
        request,
        cancellation,
    )
    .await?;
    let outcome = exchange(&mut conversation, request, &mut on_approval).await;
    match outcome {
        Ok(outcome) => {
            finish_worker(conversation).await?;
            Ok(outcome)
        }
        Err(error) => {
            let _ = conversation
                .finish(Some(ProcessTermination::Cancelled))
                .await;
            Err(error)
        }
    }
}

async fn start_worker(
    supervisor: &ProcessSupervisor,
    worker_python: &Path,
    application_home: &Path,
    staging_directory: &Path,
    request: &WorkerRunRequest,
    cancellation: CancellationToken,
) -> Result<SupervisedConversation, WorkerDriverError> {
    let environment = crate::managed_runtime::controlled_worker_environment(application_home)
        .into_iter()
        .chain([(
            std::ffi::OsString::from("PYTHONNOUSERSITE"),
            std::ffi::OsString::from("1"),
        )])
        .collect();
    let spec = ProcessSpec {
        executable: worker_python.to_path_buf(),
        arguments: [
            std::ffi::OsString::from("-I"),
            std::ffi::OsString::from("-m"),
            std::ffi::OsString::from("quantix_ai_worker"),
        ]
        .into_iter()
        .collect(),
        current_directory: Some(staging_directory.to_path_buf()),
        environment,
        inherit_environment: false,
        stdin: Vec::new(),
        timeout: request.timeout,
        stdout_limit: WORKER_OUTPUT_LIMIT,
        stderr_limit: WORKER_OUTPUT_LIMIT,
    };
    supervisor
        .start_conversation(spec, cancellation)
        .await
        .map_err(|_| WorkerDriverError::process("the AI worker could not be started"))
}

async fn exchange(
    conversation: &mut SupervisedConversation,
    request: &WorkerRunRequest,
    on_approval: &mut impl FnMut(&str, &Value) -> WorkerApproval,
) -> Result<WorkerOutcome, WorkerDriverError> {
    conversation
        .begin_operation(request.timeout, WORKER_OUTPUT_LIMIT, WORKER_OUTPUT_LIMIT)
        .map_err(|_| WorkerDriverError::protocol("the AI worker operation could not start"))?;
    let initialize = json!({
        "kind": "initialize",
        "op": match request.operation {
            WorkerOperation::Probe => "probe",
            WorkerOperation::Turn => "turn",
        },
        "route": request.route,
        "base_url": request.base_url,
        "api_key": request.api_key,
        "model_id": request.model_id,
        "reasoning": request.reasoning,
        "instructions": request.instructions,
        "output_schema": request.output_schema,
        "tools": request
            .tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                })
            })
            .collect::<Vec<_>>(),
        "budgets": {
            "max_tool_rounds": WORKER_MAX_TOOL_ROUNDS,
            "max_output_bytes": WORKER_OUTPUT_LIMIT,
            "timeout_ms": request.timeout.as_millis() as u64,
        },
        "mode": "gated",
        "input": request.input,
    });
    write_frame(conversation, &initialize).await?;
    let mut saw_ready = false;
    let mut rounds: u32 = 0;
    let mut usage: Option<WorkerUsage> = None;
    loop {
        let frame = read_frame(conversation).await?;
        match frame.get("kind").and_then(Value::as_str) {
            Some("ready") => {
                if saw_ready {
                    return Err(WorkerDriverError::protocol("duplicate ready frame"));
                }
                saw_ready = true;
            }
            Some("approval_request") => {
                rounds += 1;
                if rounds > WORKER_MAX_TOOL_ROUNDS {
                    return Err(WorkerDriverError {
                        category: WorkerFailureCategory::Budget,
                        message: "tool round limit reached".to_owned(),
                    });
                }
                let tool_call_id = string_field(&frame, "tool_call_id")?;
                let tool_name = string_field(&frame, "tool_name")?;
                let arguments = frame.get("arguments").cloned().unwrap_or(json!({}));
                let answer = on_approval(&tool_name, &arguments);
                let reply = match answer {
                    WorkerApproval::Approved(result) => json!({
                        "kind": "approval",
                        "tool_call_id": tool_call_id,
                        "approved": true,
                        "result": result,
                    }),
                    WorkerApproval::Denied(message) => json!({
                        "kind": "approval",
                        "tool_call_id": tool_call_id,
                        "approved": false,
                        "denial_message": message,
                    }),
                };
                write_frame(conversation, &reply).await?;
            }
            Some("usage") => {
                usage = Some(WorkerUsage {
                    input_tokens: frame
                        .get("input_tokens")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| WorkerDriverError::protocol("malformed usage frame"))?,
                    output_tokens: frame
                        .get("output_tokens")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| WorkerDriverError::protocol("malformed usage frame"))?,
                });
            }
            Some("result") => {
                let usage =
                    usage.ok_or_else(|| WorkerDriverError::protocol("result without usage"))?;
                let output = frame.get("output").cloned().filter(Value::is_object);
                let text = frame
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| WorkerDriverError::protocol("malformed result frame"))?
                    .to_owned();
                return match request.operation {
                    WorkerOperation::Probe => Ok(WorkerOutcome::Probe { text, usage }),
                    WorkerOperation::Turn => Ok(WorkerOutcome::Turn {
                        output,
                        text,
                        usage,
                    }),
                };
            }
            Some("failure") => {
                let category = string_field(&frame, "category")?;
                let message = string_field(&frame, "message")?;
                return Err(WorkerDriverError {
                    category: match category.as_str() {
                        "auth" => WorkerFailureCategory::Auth,
                        "rate_limited" => WorkerFailureCategory::RateLimited,
                        "network" => WorkerFailureCategory::Network,
                        "invalid_output" => WorkerFailureCategory::InvalidOutput,
                        "budget" => WorkerFailureCategory::Budget,
                        "provider" => WorkerFailureCategory::Provider,
                        _ => WorkerFailureCategory::Protocol,
                    },
                    message,
                });
            }
            Some("event") => {}
            _ => return Err(WorkerDriverError::protocol("unexpected worker frame")),
        }
    }
}

async fn finish_worker(conversation: SupervisedConversation) -> Result<(), WorkerDriverError> {
    let output = conversation
        .finish(None)
        .await
        .map_err(|_| WorkerDriverError::process("the AI worker did not exit cleanly"))?;
    match (output.termination, output.exit_code) {
        (ProcessTermination::Exited, Some(0)) => Ok(()),
        _ => Err(WorkerDriverError::process(
            "the AI worker exited with an unexpected status",
        )),
    }
}

fn string_field(frame: &Value, name: &str) -> Result<String, WorkerDriverError> {
    frame
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| WorkerDriverError::protocol(format!("malformed frame: missing {name}")))
}

async fn write_frame(
    conversation: &mut SupervisedConversation,
    frame: &Value,
) -> Result<(), WorkerDriverError> {
    let mut line = frame.to_string();
    line.push('\n');
    conversation
        .write(line.as_bytes())
        .await
        .map_err(|error| map_process_error(error, "writing to the AI worker"))
}

async fn read_frame(conversation: &mut SupervisedConversation) -> Result<Value, WorkerDriverError> {
    let line = conversation
        .read_line()
        .await
        .map_err(|error| map_process_error(error, "reading from the AI worker"))?;
    serde_json::from_slice(&line)
        .map_err(|_| WorkerDriverError::protocol("the AI worker produced a malformed frame"))
}

fn map_process_error(
    error: crate::process_supervisor::ProcessError,
    context: &str,
) -> WorkerDriverError {
    use crate::process_supervisor::ProcessError as Error;
    let category = match error {
        Error::Cancelled => WorkerFailureCategory::Cancelled,
        Error::OutputLimitExceeded => WorkerFailureCategory::Budget,
        Error::TimedOut => WorkerFailureCategory::Budget,
        Error::Exited => WorkerFailureCategory::Process,
        _ => WorkerFailureCategory::Process,
    };
    WorkerDriverError {
        category,
        message: format!("{context}: {error:?}"),
    }
}
