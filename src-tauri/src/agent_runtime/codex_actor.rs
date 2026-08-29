use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, Notify};
use tokio_util::sync::CancellationToken;

use crate::process_supervisor::ProcessError;

use super::{
    codex_protocol::{
        dynamic_tool_specs, handle_control_request, handle_notification, outcome_unknown,
        parse_wire_message, process_failure, protocol_failure, provider_instruction_bundle,
        response_result, validate_candidate, validate_schema, write_rpc, ControlRequestContext,
        ControlRequestLedger, NotificationOutcome,
    },
    failed_execution, interrupted_execution, permission_failure, provider_connection_readiness,
    stream_provider_events, turn_acceptance_unknown, AgentRunState, CandidateDisposition,
    CodexProviderProcess, PreparedAgentRun, ProviderExecution, ProviderFailure,
    ProviderFailureCategory, ProviderTransportDisposition, ProviderUsage, RunCallbacks,
    PROVIDER_OUTPUT_LIMIT,
};
use crate::application_settings::{
    AiProviderKind, ProviderConnectionStatus, ProviderConnectionView, ProviderReasoningSelection,
};

const COMMAND_CAPACITY: usize = 8;
const ACTOR_OUTPUT_LIMIT: usize = PROVIDER_OUTPUT_LIMIT * 2;
const ARCHIVE_CONFIRMATION_PAGE_LIMIT: u32 = 100;
const ACTOR_DEADLINE_GRACE: Duration = Duration::from_secs(2);
const THREAD_CLEANUP_OPERATION_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub(crate) struct CodexProvider {
    sender: mpsc::Sender<ProviderCommand>,
    alive: Arc<AtomicBool>,
    terminated: Arc<AtomicBool>,
    termination: Arc<Notify>,
    connection: Arc<Mutex<ProviderConnectionView>>,
}

impl CodexProvider {
    pub(crate) async fn readiness(
        supervisor: &crate::process_supervisor::ProcessSupervisor,
        executable: PathBuf,
        process_directory: &Path,
        cancellation: CancellationToken,
    ) -> Result<Self, ProviderFailure> {
        let process = tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Err(super::readiness_interruption_failure()),
            process = CodexProviderProcess::readiness(
                supervisor,
                executable,
                process_directory,
                CancellationToken::new(),
            ) => process?,
        };
        let connection = Arc::new(Mutex::new(process.connection_snapshot()));
        let (sender, receiver) = mpsc::channel(COMMAND_CAPACITY);
        let alive = Arc::new(AtomicBool::new(true));
        let terminated = Arc::new(AtomicBool::new(false));
        let termination = Arc::new(Notify::new());
        tokio::spawn(run_actor(
            process,
            receiver,
            Arc::clone(&alive),
            Arc::clone(&terminated),
            Arc::clone(&termination),
            Arc::clone(&connection),
        ));
        Ok(Self {
            sender,
            alive,
            terminated,
            termination,
            connection,
        })
    }

    pub(crate) async fn refresh_readiness(&self) -> Result<bool, ProviderFailure> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::Refresh { response })
            .await
            .map_err(|_| process_failure(false))?;
        receiver.await.map_err(|_| process_failure(false))?
    }

    pub(crate) fn connection_snapshot(&self) -> ProviderConnectionView {
        self.connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) async fn delete_thread(&self, thread_ref: String) -> Result<(), ProviderFailure> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::DeleteThread {
                thread_ref,
                response,
            })
            .await
            .map_err(|_| process_failure(false))?;
        receiver.await.map_err(|_| process_failure(false))?
    }

    pub(super) async fn run_turn(
        &self,
        prepared: PreparedAgentRun,
        operation_limit: Duration,
        cancellation: CancellationToken,
        callbacks: RunCallbacks,
    ) -> ProviderExecution {
        let started = Instant::now();
        let deadline = started.checked_add(operation_limit).unwrap_or(started);
        let (response, receiver) = oneshot::channel();
        if self
            .sender
            .send(ProviderCommand::Run(Box::new(ProviderRunCommand {
                prepared,
                deadline,
                cancellation,
                callbacks,
                response,
            })))
            .await
            .is_err()
        {
            return failed_execution(process_failure(false), started);
        }
        let execution = receiver.await.unwrap_or_else(|_| {
            super::indeterminate_execution(
                "unknown_provider_thread",
                None,
                outcome_unknown(),
                started,
            )
        });
        if !self.alive.load(Ordering::Acquire) {
            loop {
                let notified = self.termination.notified();
                if self.terminated.load(Ordering::Acquire) {
                    break;
                }
                notified.await;
            }
        }
        execution
    }

    pub(crate) async fn shutdown(&self) -> Result<(), ProcessError> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(ProviderCommand::Shutdown { response })
            .await
            .map_err(|_| ProcessError::ObservationFailed)?;
        receiver
            .await
            .map_err(|_| ProcessError::ObservationFailed)?
    }

    pub(crate) fn is_closed(&self) -> bool {
        !self.alive.load(Ordering::Acquire) || self.sender.is_closed()
    }

    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.alive, &other.alive)
    }
}

enum ProviderCommand {
    Refresh {
        response: oneshot::Sender<Result<bool, ProviderFailure>>,
    },
    Run(Box<ProviderRunCommand>),
    DeleteThread {
        thread_ref: String,
        response: oneshot::Sender<Result<(), ProviderFailure>>,
    },
    Shutdown {
        response: oneshot::Sender<Result<(), ProcessError>>,
    },
}

struct ProviderRunCommand {
    prepared: PreparedAgentRun,
    deadline: Instant,
    cancellation: CancellationToken,
    callbacks: RunCallbacks,
    response: oneshot::Sender<ProviderExecution>,
}

struct ActorTerminationSignal {
    terminated: Arc<AtomicBool>,
    termination: Arc<Notify>,
}

impl Drop for ActorTerminationSignal {
    fn drop(&mut self) {
        self.terminated.store(true, Ordering::Release);
        self.termination.notify_waiters();
    }
}

enum RunStage {
    ArchivePending,
    ThreadPending { resumed: bool },
    TurnPending,
    Active,
}

struct ActorRun {
    prepared: PreparedAgentRun,
    cancellation: CancellationToken,
    callbacks: RunCallbacks,
    response: oneshot::Sender<ProviderExecution>,
    stage: RunStage,
    started: Instant,
    deadline: Instant,
    thread_ref: Option<String>,
    turn_ref: Option<String>,
    turn_requested: bool,
    interrupt_sent: bool,
    interrupt_deadline: Option<Instant>,
    deadline_expired: bool,
    execution: ProviderExecution,
    output_schema: Value,
    final_candidate: Option<String>,
    control_requests: ControlRequestLedger,
    observed_bytes: usize,
}

enum PendingRpc {
    Archive(String),
    ArchiveConfirm {
        run_id: String,
        page: u32,
    },
    Thread(String),
    Turn(String),
    Interrupt(String),
    DeleteThread {
        thread_ref: String,
        response: oneshot::Sender<Result<(), ProviderFailure>>,
    },
    DeleteConfirm {
        thread_ref: String,
        archived: bool,
        page: u32,
        delete_failure: ProviderFailure,
        response: oneshot::Sender<Result<(), ProviderFailure>>,
    },
}

async fn run_actor(
    mut process: CodexProviderProcess,
    mut commands: mpsc::Receiver<ProviderCommand>,
    alive: Arc<AtomicBool>,
    terminated: Arc<AtomicBool>,
    termination: Arc<Notify>,
    connection: Arc<Mutex<ProviderConnectionView>>,
) {
    let _termination_signal = ActorTerminationSignal {
        terminated,
        termination,
    };
    let mut runs = HashMap::<String, ActorRun>::new();
    let mut pending = HashMap::<u64, PendingRpc>::new();
    let mut next_rpc_id = 10_000_u64;
    let mut tick = tokio::time::interval(Duration::from_millis(10));

    loop {
        tokio::select! {
            biased;
            command = commands.recv() => {
                let Some(command) = command else {
                    terminate_actor(
                        &mut process,
                        &mut commands,
                        &mut runs,
                        &alive,
                        process_failure(true),
                    ).await;
                    return;
                };
                match command {
                    ProviderCommand::Refresh { response } => {
                        let result = if runs.is_empty() && pending.is_empty()
                        {
                            process.refresh_readiness().await.map(|updated| {
                                *connection
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = updated;
                                true
                            })
                        } else {
                            Ok(false)
                        };
                        let fatal = result.is_err();
                        let _ = response.send(result);
                        if fatal {
                            terminate_actor(
                                &mut process,
                                &mut commands,
                                &mut runs,
                                &alive,
                                process_failure(true),
                            ).await;
                            return;
                        }
                    }
                    ProviderCommand::DeleteThread {
                        thread_ref,
                        response,
                    } => {
                        if thread_ref.is_empty()
                            || thread_ref.len() > 1000
                            || !runs.is_empty()
                            || !pending.is_empty()
                            || connection
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .status
                                != ProviderConnectionStatus::Ready
                        {
                            let _ = response.send(Err(permission_failure()));
                            continue;
                        }
                        if let Err(failure) = process.conversation_mut().and_then(|conversation| {
                            conversation
                                .begin_operation(
                                    THREAD_CLEANUP_OPERATION_TIMEOUT,
                                    ACTOR_OUTPUT_LIMIT,
                                    ACTOR_OUTPUT_LIMIT,
                                )
                                .map_err(|_| process_failure(false))
                        }) {
                            let _ = response.send(Err(failure));
                            continue;
                        }
                        let id = match allocate_id(&mut next_rpc_id) {
                            Ok(id) => id,
                            Err(failure) => {
                                let _ = response.send(Err(failure));
                                continue;
                            }
                        };
                        if write_rpc(
                            process.conversation_mut().expect("provider conversation is active"),
                            &json!({
                                "method": "thread/delete",
                                "id": id,
                                "params": { "threadId": thread_ref.clone() }
                            }),
                        )
                        .await
                        .is_err()
                        {
                            let failure = process_failure(false);
                            let _ = response.send(Err(failure.clone()));
                            terminate_actor(
                                &mut process,
                                &mut commands,
                                &mut runs,
                                &alive,
                                failure,
                            )
                            .await;
                            return;
                        }
                        pending.insert(
                            id,
                            PendingRpc::DeleteThread {
                                thread_ref,
                                response,
                            },
                        );
                    }
                    ProviderCommand::Run(command) => {
                        let ProviderRunCommand {
                            prepared,
                            deadline,
                            cancellation,
                            callbacks,
                            response,
                        } = *command;
                        let account_management_pending = pending.values().any(|operation| {
                            matches!(
                                operation,
                                PendingRpc::DeleteThread { .. }
                                    | PendingRpc::DeleteConfirm { .. }
                            )
                        });
                        let connection_snapshot = connection
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .clone();
                        if account_management_pending
                        {
                            let _ = response.send(failed_execution(
                                permission_failure(),
                                Instant::now(),
                            ));
                            continue;
                        }
                        if let Err(failure) = provider_connection_readiness(&connection_snapshot) {
                            let _ = response.send(failed_execution(failure, Instant::now()));
                            continue;
                        }
                        let transport_deadline = runs
                            .values()
                            .map(|run| run.deadline)
                            .chain(std::iter::once(deadline))
                            .max()
                            .unwrap_or(deadline);
                        let remaining = transport_deadline
                            .saturating_duration_since(Instant::now());
                        let operation = process.begin_multiplexed_run(remaining);
                        if operation.is_err() {
                            alive.store(false, Ordering::Release);
                            commands.close();
                            let pending_failures = drain_failed_runs(&mut runs, process_failure(true));
                            send_failed_runs(pending_failures);
                            let _ = response.send(failed_execution(process_failure(false), Instant::now()));
                            let _ = process.shutdown().await;
                            return;
                        }
                        if let Err(failure) = start_run(
                            &mut process,
                            &mut runs,
                            &mut pending,
                            &mut next_rpc_id,
                            prepared,
                            deadline,
                            cancellation,
                            callbacks,
                            response,
                        ).await {
                            terminate_actor(
                                &mut process,
                                &mut commands,
                                &mut runs,
                                &alive,
                                failure,
                            ).await;
                            return;
                        }
                    }
                    ProviderCommand::Shutdown { response } => {
                        alive.store(false, Ordering::Release);
                        commands.close();
                        let pending_failures = drain_failed_runs(&mut runs, process_failure(true));
                        send_failed_runs(pending_failures);
                        let result = process.shutdown().await;
                        let _ = response.send(result);
                        return;
                    }
                }
            }
            _ = tick.tick(), if !runs.is_empty() => {
                if let Err(failure) = enforce_run_limits(
                    &mut process,
                    &mut runs,
                    &mut pending,
                    &mut next_rpc_id,
                ).await {
                    terminate_actor(
                        &mut process,
                        &mut commands,
                        &mut runs,
                        &alive,
                        failure,
                    ).await;
                    return;
                }
            }
            line = process.read_line(), if !runs.is_empty() || !pending.is_empty() => {
                let line = match line {
                    Ok(line) => line,
                    Err(_) => {
                        let failure = if runs.values().any(|run| {
                            run.deadline_expired && run.turn_requested
                        }) {
                            permission_failure()
                        } else {
                            process_failure(true)
                        };
                        terminate_actor(
                            &mut process,
                            &mut commands,
                            &mut runs,
                            &alive,
                            failure,
                        ).await;
                        return;
                    }
                };
                let message = match parse_wire_message(&line) {
                    Ok(message) => message,
                    Err(_) => {
                        terminate_actor(
                            &mut process,
                            &mut commands,
                            &mut runs,
                            &alive,
                            protocol_failure(true),
                        ).await;
                        return;
                    }
                };
                let handled = handle_message(
                    CodexActorContext {
                        process: &mut process,
                        runs: &mut runs,
                        pending: &mut pending,
                        next_rpc_id: &mut next_rpc_id,
                    },
                    &line,
                    message,
                ).await;
                if let Err(failure) = handled {
                    terminate_actor(
                        &mut process,
                        &mut commands,
                        &mut runs,
                        &alive,
                        failure,
                    ).await;
                    return;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn start_run(
    process: &mut CodexProviderProcess,
    runs: &mut HashMap<String, ActorRun>,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    prepared: PreparedAgentRun,
    deadline: Instant,
    cancellation: CancellationToken,
    callbacks: RunCallbacks,
    response: oneshot::Sender<ProviderExecution>,
) -> Result<(), ProviderFailure> {
    let started = Instant::now();
    if cancellation.is_cancelled() {
        let _ = response.send(interrupted_execution(started));
        return Ok(());
    }
    if deadline <= started {
        let _ = response.send(failed_execution(permission_failure(), started));
        return Ok(());
    }
    let output_schema = match serde_json::from_str(&prepared.task.output_contract_json) {
        Ok(schema) => schema,
        Err(_) => {
            let _ = response.send(failed_execution(protocol_failure(false), started));
            return Ok(());
        }
    };
    if provider_instruction_bundle(&prepared).is_err() {
        let _ = response.send(failed_execution(protocol_failure(false), started));
        return Ok(());
    }
    let run_id = prepared.run_id.clone();
    if runs.contains_key(&run_id) {
        let _ = response.send(failed_execution(protocol_failure(false), started));
        return Ok(());
    }
    let archive = prepared.provider_thread_to_archive.is_some();
    runs.insert(
        run_id.clone(),
        ActorRun {
            prepared,
            cancellation,
            callbacks,
            response,
            stage: if archive {
                RunStage::ArchivePending
            } else {
                RunStage::ThreadPending { resumed: false }
            },
            started,
            deadline,
            thread_ref: None,
            turn_ref: None,
            turn_requested: false,
            interrupt_sent: false,
            interrupt_deadline: None,
            deadline_expired: false,
            execution: ProviderExecution {
                state: AgentRunState::Running,
                transport_disposition: ProviderTransportDisposition::Indeterminate,
                candidate_disposition: CandidateDisposition::NotEvaluated,
                provider_thread_ref: None,
                provider_turn_ref: None,
                events: Vec::new(),
                usage: ProviderUsage::default(),
                failure: None,
                candidate_payload_json: None,
            },
            output_schema,
            final_candidate: None,
            control_requests: ControlRequestLedger::default(),
            observed_bytes: 0,
        },
    );
    if archive {
        send_archive(process, runs, pending, next_rpc_id, &run_id).await
    } else {
        send_thread(process, runs, pending, next_rpc_id, &run_id).await
    }
}

async fn send_archive(
    process: &mut CodexProviderProcess,
    runs: &mut HashMap<String, ActorRun>,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    run_id: &str,
) -> Result<(), ProviderFailure> {
    let thread_ref = runs
        .get(run_id)
        .and_then(|run| run.prepared.provider_thread_to_archive.clone())
        .ok_or_else(|| protocol_failure(false))?;
    let id = allocate_id(next_rpc_id)?;
    write_rpc(
        process.conversation_mut()?,
        &json!({
            "method": "thread/archive",
            "id": id,
            "params": { "threadId": thread_ref }
        }),
    )
    .await
    .map_err(|_| process_failure(false))?;
    pending.insert(id, PendingRpc::Archive(run_id.to_owned()));
    Ok(())
}

async fn send_archive_confirmation(
    process: &mut CodexProviderProcess,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    run_id: &str,
    cursor: Option<&str>,
    page: u32,
) -> Result<(), ProviderFailure> {
    if page > ARCHIVE_CONFIRMATION_PAGE_LIMIT {
        return Err(protocol_failure(false));
    }
    let id = allocate_id(next_rpc_id)?;
    write_rpc(
        process.conversation_mut()?,
        &json!({
            "method": "thread/list",
            "id": id,
            "params": {
                "archived": true,
                "cursor": cursor,
                "limit": 100
            }
        }),
    )
    .await
    .map_err(|_| process_failure(false))?;
    pending.insert(
        id,
        PendingRpc::ArchiveConfirm {
            run_id: run_id.to_owned(),
            page,
        },
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn send_delete_confirmation(
    process: &mut CodexProviderProcess,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    thread_ref: String,
    archived: bool,
    cursor: Option<&str>,
    page: u32,
    delete_failure: ProviderFailure,
    response: oneshot::Sender<Result<(), ProviderFailure>>,
) -> Result<(), ProviderFailure> {
    if page > ARCHIVE_CONFIRMATION_PAGE_LIMIT {
        let _ = response.send(Err(delete_failure));
        return Ok(());
    }
    let id = allocate_id(next_rpc_id)?;
    write_rpc(
        process.conversation_mut()?,
        &json!({
            "method": "thread/list",
            "id": id,
            "params": {
                "archived": archived,
                "cursor": cursor,
                "limit": 100
            }
        }),
    )
    .await
    .map_err(|_| process_failure(false))?;
    pending.insert(
        id,
        PendingRpc::DeleteConfirm {
            thread_ref,
            archived,
            page,
            delete_failure,
            response,
        },
    );
    Ok(())
}

async fn checkpoint_archived_and_send_thread(
    process: &mut CodexProviderProcess,
    runs: &mut HashMap<String, ActorRun>,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    run_id: &str,
) -> Result<(), ProviderFailure> {
    let archived_ref = runs
        .get(run_id)
        .and_then(|run| run.prepared.provider_thread_to_archive.clone())
        .ok_or_else(|| protocol_failure(false))?;
    let run = runs
        .get_mut(run_id)
        .ok_or_else(|| protocol_failure(false))?;
    let on_archived = std::mem::replace(
        &mut run.callbacks.on_thread_archived,
        Box::new(|_| Err(process_failure(false))),
    );
    if let Err(failure) = on_archived(&archived_ref) {
        fail_one(runs, run_id, failure.clone())?;
        return Err(failure);
    }
    send_thread(process, runs, pending, next_rpc_id, run_id).await
}

async fn send_thread(
    process: &mut CodexProviderProcess,
    runs: &mut HashMap<String, ActorRun>,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    run_id: &str,
) -> Result<(), ProviderFailure> {
    let run = runs
        .get_mut(run_id)
        .ok_or_else(|| protocol_failure(false))?;
    let existing = run.prepared.provider_thread_ref.as_deref();
    if run.prepared.provider_selection.provider != AiProviderKind::Codex {
        return Err(protocol_failure(false));
    }
    let resumed = existing.is_some();
    let (method, params) = if let Some(thread_ref) = existing {
        (
            "thread/resume",
            json!({ "threadId": thread_ref, "excludeTurns": true }),
        )
    } else {
        let tools = dynamic_tool_specs(&run.prepared.permission_grant)?;
        (
            "thread/start",
            json!({
                "cwd": run.prepared.workspace
                    .join(&run.prepared.permission_grant.workspace.working_area),
                "approvalPolicy": "never",
                "sandbox": "workspace-write",
                "serviceName": "quantix",
                "model": run.prepared.provider_selection.model_id,
                "dynamicTools": tools,
            }),
        )
    };
    let id = allocate_id(next_rpc_id)?;
    write_rpc(
        process.conversation_mut()?,
        &json!({ "method": method, "id": id, "params": params }),
    )
    .await
    .map_err(|_| process_failure(false))?;
    run.stage = RunStage::ThreadPending { resumed };
    pending.insert(id, PendingRpc::Thread(run_id.to_owned()));
    Ok(())
}

async fn send_turn(
    process: &mut CodexProviderProcess,
    runs: &mut HashMap<String, ActorRun>,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    run_id: &str,
) -> Result<(), ProviderFailure> {
    let run = runs
        .get_mut(run_id)
        .ok_or_else(|| protocol_failure(false))?;
    let thread_ref = run
        .thread_ref
        .as_deref()
        .ok_or_else(|| protocol_failure(false))?;
    let instruction_bundle = provider_instruction_bundle(&run.prepared)?;
    if run.prepared.provider_selection.provider != AiProviderKind::Codex {
        return Err(protocol_failure(false));
    }
    let effort = match &run.prepared.provider_selection.reasoning {
        ProviderReasoningSelection::Effort(effort) => Some(effort.as_str()),
        ProviderReasoningSelection::ProviderDefault => None,
    };
    let on_requested = std::mem::replace(
        &mut run.callbacks.on_requested,
        Box::new(|| Err(protocol_failure(false))),
    );
    on_requested()?;
    run.turn_requested = true;
    let id = allocate_id(next_rpc_id)?;
    write_rpc(
        process.conversation_mut()?,
        &json!({
            "method": "turn/start",
            "id": id,
            "params": {
                "threadId": thread_ref,
                "input": [{ "type": "text", "text": instruction_bundle }],
                "model": run.prepared.provider_selection.model_id,
                "effort": effort,
                "cwd": run.prepared.workspace
                    .join(&run.prepared.permission_grant.workspace.working_area),
                "approvalPolicy": "never",
                "sandboxPolicy": {
                    "type": "workspaceWrite",
                    "networkAccess": false,
                    "writableRoots": [
                        run.prepared.workspace
                            .join(&run.prepared.permission_grant.workspace.staged_outputs)
                    ]
                },
                "outputSchema": run.output_schema,
            }
        }),
    )
    .await
    .map_err(|_| turn_acceptance_unknown())?;
    run.stage = RunStage::TurnPending;
    pending.insert(id, PendingRpc::Turn(run_id.to_owned()));
    Ok(())
}

async fn send_interrupt(
    process: &mut CodexProviderProcess,
    runs: &mut HashMap<String, ActorRun>,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
    run_id: &str,
) -> Result<(), ProviderFailure> {
    let run = runs.get_mut(run_id).ok_or_else(|| protocol_failure(true))?;
    let thread_ref = run.thread_ref.as_deref().ok_or_else(outcome_unknown)?;
    let turn_ref = run.turn_ref.as_deref().ok_or_else(outcome_unknown)?;
    let id = allocate_id(next_rpc_id)?;
    write_rpc(
        process.conversation_mut()?,
        &json!({
            "method": "turn/interrupt",
            "id": id,
            "params": { "threadId": thread_ref, "turnId": turn_ref }
        }),
    )
    .await
    .map_err(|_| outcome_unknown())?;
    run.interrupt_sent = true;
    run.interrupt_deadline = Some(Instant::now() + ACTOR_DEADLINE_GRACE);
    pending.insert(id, PendingRpc::Interrupt(run_id.to_owned()));
    Ok(())
}

struct CodexActorContext<'a> {
    process: &'a mut CodexProviderProcess,
    runs: &'a mut HashMap<String, ActorRun>,
    pending: &'a mut HashMap<u64, PendingRpc>,
    next_rpc_id: &'a mut u64,
}

async fn handle_message(
    context: CodexActorContext<'_>,
    line: &[u8],
    message: Value,
) -> Result<(), ProviderFailure> {
    let CodexActorContext {
        process,
        runs,
        pending,
        next_rpc_id,
    } = context;
    if message.get("id").is_some() && message.get("method").is_none() {
        let id = message
            .get("id")
            .and_then(Value::as_u64)
            .ok_or_else(|| protocol_failure(true))?;
        let Some(operation) = pending.remove(&id) else {
            return Err(protocol_failure(true));
        };
        return handle_response(
            CodexActorContext {
                process,
                runs,
                pending,
                next_rpc_id,
            },
            line.len(),
            message,
            operation,
        )
        .await;
    }
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| protocol_failure(true))?;
    if message.get("id").is_some() {
        let run_id = route_message_to_run(runs, message.get("params").unwrap_or(&Value::Null))?;
        account_bytes(runs, &run_id, line.len())?;
        let run = runs
            .get_mut(&run_id)
            .ok_or_else(|| protocol_failure(true))?;
        let thread_ref = run.thread_ref.as_deref().ok_or_else(outcome_unknown)?;
        let turn_ref = run.turn_ref.as_deref().ok_or_else(outcome_unknown)?;
        handle_control_request(
            process.conversation_mut()?,
            &message,
            ControlRequestContext {
                grant: &run.prepared.permission_grant,
                expected_thread_ref: thread_ref,
                expected_turn_ref: turn_ref,
                expired: Instant::now() >= run.deadline,
                ledger: &mut run.control_requests,
                on_denied: &mut run.callbacks.on_denied,
                on_tool_call: &mut run.callbacks.on_tool_call,
            },
        )
        .await?;
        return Ok(());
    }
    validate_schema("ServerNotification", &message)?;
    let params = message.get("params").unwrap_or(&Value::Null);
    let routed = notification_turn_ref(params)
        .map(|turn_ref| {
            runs.iter()
                .find_map(|(run_id, run)| {
                    (run.turn_ref.as_deref() == Some(turn_ref)).then(|| run_id.clone())
                })
                .ok_or_else(|| protocol_failure(true))
        })
        .transpose()?;
    let targets = if let Some(run_id) = routed {
        vec![run_id]
    } else if matches!(
        method,
        "account/updated" | "account/rateLimits/updated" | "warning"
    ) {
        runs.keys().cloned().collect()
    } else {
        return Ok(());
    };
    let mut terminal = Vec::new();
    let mut global_account_loss = false;
    let mut provider_must_restart = false;
    for run_id in targets {
        account_bytes(runs, &run_id, line.len())?;
        let run = runs
            .get_mut(&run_id)
            .ok_or_else(|| protocol_failure(true))?;
        let turn_ref = run.turn_ref.as_deref().ok_or_else(outcome_unknown)?;
        let outcome = handle_notification(
            method,
            params,
            turn_ref,
            &mut run.execution,
            &mut run.final_candidate,
        )?;
        stream_provider_events(&mut run.execution, &mut run.callbacks.on_event)?;
        if matches!(outcome, NotificationOutcome::Terminal) {
            global_account_loss |= method == "account/updated";
            provider_must_restart |= run.execution.failure.as_ref().is_some_and(|failure| {
                matches!(
                    failure.category,
                    ProviderFailureCategory::AuthenticationRequired
                        | ProviderFailureCategory::SubscriptionRequired
                        | ProviderFailureCategory::ProtocolInvalid
                        | ProviderFailureCategory::ProcessFailed
                )
            });
            terminal.push(run_id);
        }
    }
    for run_id in terminal {
        complete_run(runs, &run_id)?;
    }
    if global_account_loss || provider_must_restart {
        return Err(outcome_unknown());
    }
    Ok(())
}

async fn handle_response(
    context: CodexActorContext<'_>,
    line_bytes: usize,
    message: Value,
    operation: PendingRpc,
) -> Result<(), ProviderFailure> {
    let CodexActorContext {
        process,
        runs,
        pending,
        next_rpc_id,
    } = context;
    let operation = match operation {
        PendingRpc::DeleteThread {
            thread_ref,
            response,
        } => match response_result(&message, "v2/ThreadDeleteResponse", false) {
            Ok(_) => {
                let _ = response.send(Ok(()));
                return Ok(());
            }
            Err(delete_failure) => {
                return send_delete_confirmation(
                    process,
                    pending,
                    next_rpc_id,
                    thread_ref,
                    false,
                    None,
                    1,
                    delete_failure,
                    response,
                )
                .await;
            }
        },
        PendingRpc::DeleteConfirm {
            thread_ref,
            archived,
            page,
            delete_failure,
            response,
        } => {
            let result = response_result(&message, "v2/ThreadListResponse", false)?;
            let present = result
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| protocol_failure(false))?
                .iter()
                .any(|thread| {
                    thread.get("id").and_then(Value::as_str) == Some(thread_ref.as_str())
                });
            if present {
                let _ = response.send(Err(delete_failure));
                return Ok(());
            }
            if let Some(cursor) = result.get("nextCursor").and_then(Value::as_str) {
                return send_delete_confirmation(
                    process,
                    pending,
                    next_rpc_id,
                    thread_ref,
                    archived,
                    Some(cursor),
                    page.saturating_add(1),
                    delete_failure,
                    response,
                )
                .await;
            }
            if !archived {
                return send_delete_confirmation(
                    process,
                    pending,
                    next_rpc_id,
                    thread_ref,
                    true,
                    None,
                    1,
                    delete_failure,
                    response,
                )
                .await;
            }
            let _ = response.send(Ok(()));
            return Ok(());
        }
        other => other,
    };
    let run_id = match &operation {
        PendingRpc::Archive(run_id)
        | PendingRpc::ArchiveConfirm { run_id, .. }
        | PendingRpc::Thread(run_id)
        | PendingRpc::Turn(run_id)
        | PendingRpc::Interrupt(run_id) => run_id.clone(),
        PendingRpc::DeleteThread { .. } | PendingRpc::DeleteConfirm { .. } => {
            unreachable!("account and cleanup responses return above")
        }
    };
    if !runs.contains_key(&run_id) {
        return Ok(());
    }
    account_bytes(runs, &run_id, line_bytes)?;
    match operation {
        PendingRpc::Archive(run_id) => {
            if response_result(&message, "v2/ThreadArchiveResponse", false).is_err() {
                return send_archive_confirmation(process, pending, next_rpc_id, &run_id, None, 1)
                    .await;
            }
            checkpoint_archived_and_send_thread(process, runs, pending, next_rpc_id, &run_id).await
        }
        PendingRpc::ArchiveConfirm { run_id, page } => {
            let result = response_result(&message, "v2/ThreadListResponse", false)?;
            let archived_ref = runs
                .get(&run_id)
                .and_then(|run| run.prepared.provider_thread_to_archive.as_deref())
                .ok_or_else(|| protocol_failure(false))?;
            let confirmed = result
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(|| protocol_failure(false))?
                .iter()
                .any(|thread| thread.get("id").and_then(Value::as_str) == Some(archived_ref));
            if confirmed {
                return checkpoint_archived_and_send_thread(
                    process,
                    runs,
                    pending,
                    next_rpc_id,
                    &run_id,
                )
                .await;
            }
            if let Some(cursor) = result.get("nextCursor").and_then(Value::as_str) {
                return send_archive_confirmation(
                    process,
                    pending,
                    next_rpc_id,
                    &run_id,
                    Some(cursor),
                    page.saturating_add(1),
                )
                .await;
            }
            fail_one(runs, &run_id, process_failure(false))
        }
        PendingRpc::Thread(run_id) => {
            let resumed = matches!(
                runs.get(&run_id).map(|run| &run.stage),
                Some(RunStage::ThreadPending { resumed: true })
            );
            let definition = if resumed {
                "v2/ThreadResumeResponse"
            } else {
                "v2/ThreadStartResponse"
            };
            let result = response_result(&message, definition, false)?;
            let thread_ref = result
                .pointer("/thread/id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| protocol_failure(false))?
                .to_owned();
            let run = runs
                .get_mut(&run_id)
                .ok_or_else(|| protocol_failure(false))?;
            run.thread_ref = Some(thread_ref.clone());
            run.execution.provider_thread_ref = Some(thread_ref.clone());
            let on_thread = std::mem::replace(
                &mut run.callbacks.on_thread_established,
                Box::new(|_, _| Err(process_failure(false))),
            );
            if let Err(failure) = on_thread(&thread_ref, resumed) {
                fail_one(runs, &run_id, failure.clone())?;
                return Err(failure);
            }
            send_turn(process, runs, pending, next_rpc_id, &run_id).await
        }
        PendingRpc::Turn(run_id) => {
            let result = response_result(&message, "v2/TurnStartResponse", true)?;
            let turn_ref = result
                .pointer("/turn/id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(turn_acceptance_unknown)?
                .to_owned();
            let run = runs.get_mut(&run_id).ok_or_else(outcome_unknown)?;
            let on_accepted = std::mem::replace(
                &mut run.callbacks.on_accepted,
                Box::new(|_| Err(outcome_unknown())),
            );
            on_accepted(&turn_ref)?;
            run.turn_ref = Some(turn_ref.clone());
            run.execution.provider_turn_ref = Some(turn_ref);
            run.stage = RunStage::Active;
            Ok(())
        }
        PendingRpc::Interrupt(_) => {
            response_result(&message, "v2/TurnInterruptResponse", true)?;
            Ok(())
        }
        PendingRpc::DeleteThread { .. } | PendingRpc::DeleteConfirm { .. } => {
            unreachable!("account and cleanup responses return above")
        }
    }
}

async fn enforce_run_limits(
    process: &mut CodexProviderProcess,
    runs: &mut HashMap<String, ActorRun>,
    pending: &mut HashMap<u64, PendingRpc>,
    next_rpc_id: &mut u64,
) -> Result<(), ProviderFailure> {
    let now = Instant::now();
    if runs.values().any(|run| {
        run.interrupt_sent
            && run
                .interrupt_deadline
                .is_some_and(|deadline| now >= deadline)
    }) {
        return Err(outcome_unknown());
    }
    let mut interrupt = Vec::new();
    let mut cancel_before_request = Vec::new();
    for (run_id, run) in runs.iter() {
        if run.response.is_closed() {
            return Err(outcome_unknown());
        }
        if !(run.cancellation.is_cancelled() || now >= run.deadline) {
            continue;
        }
        match run.stage {
            RunStage::Active if !run.interrupt_sent => interrupt.push(run_id.clone()),
            RunStage::TurnPending => return Err(outcome_unknown()),
            RunStage::ArchivePending | RunStage::ThreadPending { .. } => {
                cancel_before_request.push(run_id.clone())
            }
            RunStage::Active => {}
        }
    }
    for run_id in cancel_before_request {
        let run = runs
            .remove(&run_id)
            .ok_or_else(|| protocol_failure(false))?;
        let execution = if run.cancellation.is_cancelled() {
            interrupted_execution(run.started)
        } else {
            failed_execution(permission_failure(), run.started)
        };
        let _ = run.response.send(execution);
    }
    for run_id in interrupt {
        if let Some(run) = runs.get_mut(&run_id) {
            run.deadline_expired = !run.cancellation.is_cancelled() && now >= run.deadline;
        }
        send_interrupt(process, runs, pending, next_rpc_id, &run_id).await?;
    }
    Ok(())
}

fn complete_run(runs: &mut HashMap<String, ActorRun>, run_id: &str) -> Result<(), ProviderFailure> {
    let mut run = runs.remove(run_id).ok_or_else(|| protocol_failure(true))?;
    if run.cancellation.is_cancelled() {
        run.execution.state = AgentRunState::Interrupted;
        run.execution.failure = Some(super::interruption_failure());
        run.execution.candidate_payload_json = None;
        run.final_candidate = None;
    } else if effective_deadline_expired(run.deadline_expired, run.deadline) {
        run.execution.state = AgentRunState::Failed;
        run.execution.failure = Some(permission_failure());
        run.execution.candidate_payload_json = None;
        run.final_candidate = None;
    }
    let provider_terminal_state = run.execution.state;
    run.execution.transport_disposition = match provider_terminal_state {
        AgentRunState::Completed => ProviderTransportDisposition::Completed,
        AgentRunState::Failed => ProviderTransportDisposition::Failed,
        AgentRunState::Interrupted => ProviderTransportDisposition::Interrupted,
        AgentRunState::Indeterminate | AgentRunState::Running => {
            ProviderTransportDisposition::Indeterminate
        }
    };
    if run.execution.state == AgentRunState::Completed {
        match validate_candidate(
            run.final_candidate.as_deref(),
            &run.output_schema,
            run.prepared.task.resource_budget.output_bytes,
        ) {
            Ok(payload) => run.execution.candidate_payload_json = Some(payload),
            Err(failure) => {
                run.execution.state = AgentRunState::Failed;
                run.execution.failure = Some(failure);
                run.execution.candidate_disposition =
                    CandidateDisposition::rejected(vec!["schema_rejection".into()]);
            }
        }
    }
    if run.execution.state == AgentRunState::Running {
        run.execution.state = AgentRunState::Indeterminate;
        run.execution.failure = Some(outcome_unknown());
    }
    run.execution.usage.elapsed_milliseconds = Some(
        run.started
            .elapsed()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
    );
    let _ = run.response.send(run.execution);
    Ok(())
}

fn effective_deadline_expired(observed_by_tick: bool, deadline: Instant) -> bool {
    observed_by_tick || Instant::now() >= deadline
}

fn fail_one(
    runs: &mut HashMap<String, ActorRun>,
    run_id: &str,
    failure: ProviderFailure,
) -> Result<(), ProviderFailure> {
    let run = runs.remove(run_id).ok_or_else(|| protocol_failure(false))?;
    let execution = if run.turn_requested {
        super::indeterminate_execution(
            run.thread_ref
                .as_deref()
                .unwrap_or("unknown_provider_thread"),
            run.turn_ref,
            failure,
            run.started,
        )
    } else {
        failed_execution(failure, run.started)
    };
    let _ = run.response.send(execution);
    Ok(())
}

fn drain_failed_runs(
    runs: &mut HashMap<String, ActorRun>,
    failure: ProviderFailure,
) -> Vec<(oneshot::Sender<ProviderExecution>, ProviderExecution)> {
    let mut failed = Vec::with_capacity(runs.len());
    for (_, run) in runs.drain() {
        let run_failure =
            if !run.turn_requested && failure.category == ProviderFailureCategory::OutcomeUnknown {
                if failure.redacted_detail.as_deref()
                    == Some("The supervised Codex process did not complete the operation.")
                {
                    process_failure(false)
                } else {
                    protocol_failure(false)
                }
            } else {
                failure.clone()
            };
        let execution = if run.deadline_expired && !run.cancellation.is_cancelled() {
            failed_execution(permission_failure(), run.started)
        } else if run.cancellation.is_cancelled() {
            interrupted_execution(run.started)
        } else if run.turn_requested {
            super::indeterminate_execution(
                run.thread_ref
                    .as_deref()
                    .unwrap_or("unknown_provider_thread"),
                run.turn_ref,
                run_failure,
                run.started,
            )
        } else {
            failed_execution(run_failure, run.started)
        };
        failed.push((run.response, execution));
    }
    failed
}

fn send_failed_runs(failed: Vec<(oneshot::Sender<ProviderExecution>, ProviderExecution)>) {
    for (response, execution) in failed {
        let _ = response.send(execution);
    }
}

async fn terminate_actor(
    process: &mut CodexProviderProcess,
    commands: &mut mpsc::Receiver<ProviderCommand>,
    runs: &mut HashMap<String, ActorRun>,
    alive: &AtomicBool,
    failure: ProviderFailure,
) {
    alive.store(false, Ordering::Release);
    commands.close();
    let failed = drain_failed_runs(runs, failure);
    send_failed_runs(failed);
    let _ = process.shutdown().await;
}

fn route_message_to_run(
    runs: &HashMap<String, ActorRun>,
    params: &Value,
) -> Result<String, ProviderFailure> {
    let turn_ref = params.get("turnId").and_then(Value::as_str);
    let thread_ref = params.get("threadId").and_then(Value::as_str);
    runs.iter()
        .find_map(|(run_id, run)| {
            let turn_matches =
                turn_ref.is_none_or(|turn_ref| run.turn_ref.as_deref() == Some(turn_ref));
            let thread_matches =
                thread_ref.is_some_and(|thread_ref| run.thread_ref.as_deref() == Some(thread_ref));
            (turn_matches && thread_matches).then(|| run_id.clone())
        })
        .ok_or_else(|| protocol_failure(true))
}

fn notification_turn_ref(params: &Value) -> Option<&str> {
    params
        .get("turnId")
        .and_then(Value::as_str)
        .or_else(|| params.pointer("/turn/id").and_then(Value::as_str))
}

fn account_bytes(
    runs: &mut HashMap<String, ActorRun>,
    run_id: &str,
    bytes: usize,
) -> Result<(), ProviderFailure> {
    let run = runs.get_mut(run_id).ok_or_else(|| protocol_failure(true))?;
    run.observed_bytes = run
        .observed_bytes
        .checked_add(bytes)
        .filter(|total| *total <= PROVIDER_OUTPUT_LIMIT)
        .ok_or_else(|| protocol_failure(true))?;
    Ok(())
}

fn allocate_id(next_rpc_id: &mut u64) -> Result<u64, ProviderFailure> {
    let id = *next_rpc_id;
    *next_rpc_id = next_rpc_id
        .checked_add(1)
        .ok_or_else(|| protocol_failure(false))?;
    Ok(id)
}

impl CodexProviderProcess {
    fn begin_multiplexed_run(&mut self, operation_limit: Duration) -> Result<(), ProviderFailure> {
        self.conversation_mut()?
            .begin_operation(
                operation_limit.saturating_add(ACTOR_DEADLINE_GRACE),
                ACTOR_OUTPUT_LIMIT,
                ACTOR_OUTPUT_LIMIT,
            )
            .map_err(|_| process_failure(false))
    }

    async fn read_line(&mut self) -> Result<Vec<u8>, ProcessError> {
        self.conversation_mut()
            .map_err(|_| ProcessError::ObservationFailed)?
            .read_line()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_deadline_expires_before_the_next_enforcement_tick() {
        let expired = Instant::now()
            .checked_sub(Duration::from_millis(1))
            .expect("past instant");

        assert!(effective_deadline_expired(false, expired));
    }
}
